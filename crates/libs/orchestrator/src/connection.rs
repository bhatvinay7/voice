use crate::models::{QueueEvent, SessionEvent};
use scheduler::grpc::cell_control_client::CellControlClient;
use scheduler::grpc::{cell_message, CellMessage, Heartbeat, RegisterCell};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

pub struct ConnectionManager {
    cell_id: String,
    capacity: u32,
    scheduler_endpoint: String,
    orch_tx: mpsc::Sender<SessionEvent>,
}

impl ConnectionManager {
    pub fn new(
        cell_id: String,
        capacity: u32,
        scheduler_endpoint: String,
        orch_tx: mpsc::Sender<SessionEvent>,
    ) -> Self {
        Self {
            cell_id,
            capacity,
            scheduler_endpoint,
            orch_tx,
        }
    }

    pub async fn run(&self) {
        loop {
            match self.connect_and_process().await {
                Ok(_) => {
                    println!(
                        "[ConnectionManager] Disconnected from scheduler. Reconnecting in 5s..."
                    );
                }
                Err(e) => {
                    eprintln!(
                        "[ConnectionManager] Connection error: {}. Reconnecting in 5s...",
                        e
                    );
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }

    async fn connect_and_process(&self) -> Result<(), Box<dyn std::error::Error>> {
        let endpoint = tonic::transport::Endpoint::from_shared(self.scheduler_endpoint.clone())?;
        let channel = endpoint.connect().await?;
        let mut client = CellControlClient::new(channel);

        let (tx, rx) = mpsc::channel(100);

        // Initial Registration
        let register_msg = CellMessage {
            msg: Some(cell_message::Msg::Register(RegisterCell {
                cell_id: self.cell_id.clone(),
                capacity: self.capacity,
                active_sessions: 0, // Should be fetched from sled/state
                free_slots: self.capacity,
                status: "READY".to_string(),
            })),
        };
        tx.send(register_msg).await?;

        // Start heartbeat task
        let cell_id_clone = self.cell_id.clone();
        let capacity_clone = self.capacity;
        let tx_clone = tx.clone();
        let heartbeat_task = tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                let hb = CellMessage {
                    msg: Some(cell_message::Msg::Heartbeat(Heartbeat {
                        cell_id: cell_id_clone.clone(),
                        capacity: capacity_clone,
                        active_sessions: 0, // TODO: Fetch real values
                        free_slots: capacity_clone,
                        status: "READY".to_string(),
                    })),
                };
                if tx_clone.send(hb).await.is_err() {
                    break;
                }
            }
        });

        let mut response_stream = client
            .connect_stream(tonic::Request::new(ReceiverStream::new(rx)))
            .await?
            .into_inner();

        while let Some(msg) = response_stream.message().await? {
            if let Some(payload) = msg.msg {
                match payload {
                    scheduler::grpc::scheduler_message::Msg::StartSession(start) => {
                        println!(
                            "[ConnectionManager] Received StartSession for {}",
                            start.session_id
                        );
                        // Forward to orchestrator
                        let _ = self
                            .orch_tx
                            .send(QueueEvent::SessionStarted {
                                session_id: start.session_id,
                                user_id: "unknown".to_string(),
                                language: "en".to_string(),
                            })
                            .await;
                    }
                    scheduler::grpc::scheduler_message::Msg::CancelSession(cancel) => {
                        println!(
                            "[ConnectionManager] Received CancelSession for {}",
                            cancel.session_id
                        );
                        let _ = self
                            .orch_tx
                            .send(QueueEvent::SessionCancelled {
                                session_id: cancel.session_id,
                            })
                            .await;
                    }
                    scheduler::grpc::scheduler_message::Msg::StopSession(stop) => {
                        println!(
                            "[ConnectionManager] Received StopSession for {}",
                            stop.session_id
                        );
                    }
                    scheduler::grpc::scheduler_message::Msg::Drain(_) => {
                        println!("[ConnectionManager] Received Drain command");
                    }
                    scheduler::grpc::scheduler_message::Msg::Ping(_) => {
                        // Respond to ping or just log
                    }
                }
            }
        }

        heartbeat_task.abort();
        Ok(())
    }
}
