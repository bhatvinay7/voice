use crate::{
    models::{QueueEvent, SessionEvent},
    slots::Slots,
};
use scheduler::grpc::{
    cell_control_client::CellControlClient, cell_message, scheduler_message, AdmissionResult,
    CellMessage, Heartbeat, RegisterCell,
};
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

#[derive(Clone)]
pub struct ConnectionManager {
    cell_id: String,
    scheduler_endpoint: String,
    orch_tx: mpsc::Sender<SessionEvent>,
    slots: Slots,
}
impl ConnectionManager {
    pub fn new(
        cell_id: String,
        scheduler_endpoint: String,
        orch_tx: mpsc::Sender<SessionEvent>,
        slots: Slots,
    ) -> Self {
        Self {
            cell_id,
            scheduler_endpoint,
            orch_tx,
            slots,
        }
    }
    pub async fn run(&self) {
        let discovery = std::env::var("SCHEDULER_DISCOVERY_ADDR")
            .ok()
            .filter(|s| !s.is_empty());
        let mut connections: HashMap<String, tokio::task::JoinHandle<()>> = HashMap::new();
        let mut tick = tokio::time::interval(Duration::from_secs(10));
        loop {
            tick.tick().await;
            let endpoints: HashSet<String> = if let Some(address) = &discovery {
                match tokio::net::lookup_host(address).await {
                    Ok(addresses) => addresses.map(|ip| format!("http://{ip}")).collect(),
                    Err(error) => {
                        tracing::warn!(%error, "scheduler DNS lookup failed; retaining current streams");
                        continue;
                    }
                }
            } else {
                [self.scheduler_endpoint.clone()].into_iter().collect()
            };
            connections.retain(|endpoint, task| {
                if endpoints.contains(endpoint) && !task.is_finished() {
                    true
                } else {
                    task.abort();
                    false
                }
            });
            for endpoint in endpoints {
                connections.entry(endpoint.clone()).or_insert_with(|| {
                    let manager = self.clone();
                    tokio::spawn(async move {
                        loop {
                            if let Err(error) = manager.connect_and_process(endpoint.clone()).await
                            {
                                tracing::warn!(%error, %endpoint, "scheduler stream disconnected");
                            }
                            tokio::time::sleep(Duration::from_secs(3)).await;
                        }
                    })
                });
            }
        }
    }
    fn heartbeat(&self) -> CellMessage {
        CellMessage {
            msg: Some(cell_message::Msg::Heartbeat(Heartbeat {
                cell_id: self.cell_id.clone(),
                capacity: self.slots.capacity,
                active_sessions: self.slots.used(),
                free_slots: self.slots.free(),
                status: if self.slots.draining() {
                    "DRAINING"
                } else {
                    "READY"
                }
                .into(),
            })),
        }
    }
    async fn connect_and_process(
        &self,
        endpoint: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let channel = tonic::transport::Endpoint::from_shared(endpoint)?
            .connect_timeout(Duration::from_secs(5))
            .http2_keep_alive_interval(Duration::from_secs(15))
            .keep_alive_timeout(Duration::from_secs(5))
            .keep_alive_while_idle(true)
            .connect()
            .await?;
        let mut client = CellControlClient::new(channel);
        let (tx, rx) = mpsc::channel(100);
        let mut changes = self.slots.subscribe();
        tx.send(CellMessage {
            msg: Some(cell_message::Msg::Register(RegisterCell {
                cell_id: self.cell_id.clone(),
                capacity: self.slots.capacity,
                active_sessions: self.slots.used(),
                free_slots: self.slots.free(),
                status: if self.slots.draining() {
                    "DRAINING"
                } else {
                    "READY"
                }
                .into(),
            })),
        })
        .await?;
        let mut stream = client
            .connect_stream(tonic::Request::new(ReceiverStream::new(rx)))
            .await?
            .into_inner();
        let mut heartbeat = tokio::time::interval(Duration::from_secs(10));
        // No detached heartbeat task: cancellation drops the entire connection.
        loop {
            tokio::select! {
                _ = heartbeat.tick() => { tx.send(self.heartbeat()).await?; }
                changed = changes.changed() => { changed?; tx.send(self.heartbeat()).await?; }
                message = stream.message() => {
                    let Some(message) = message? else { return Ok(()); };
                    match message.msg {
                        Some(scheduler_message::Msg::StartSession(start)) => {
                            let result = if scheduler::session_cell(&start.session_id).ok() != Some(self.cell_id.as_str()) {
                                Err("session belongs to another cell".into())
                            } else if start.session_id.starts_with("phone:") {
                                Err("phone media must be attached locally".into())
                            } else { self.slots.acquire(&start.session_id, chrono::Utc::now().timestamp()) };
                            let mut accepted = result.is_ok();
                            let mut reason = result.as_ref().err().cloned().unwrap_or_default();
                            if matches!(result, Ok(true)) {
                                if self.orch_tx.try_send(QueueEvent::SessionStarted { session_id: start.session_id.clone(), user_id: start.user_id, language: start.language }).is_err() {
                                    self.slots.release(&start.session_id)?;
                                    accepted = false; reason = "orchestrator queue full".into();
                                }
                            }
                            tx.send(CellMessage { msg: Some(cell_message::Msg::AdmissionResult(AdmissionResult { session_id: start.session_id, accepted, reason, request_id: start.request_id })) }).await?;
                            tx.send(self.heartbeat()).await?;
                        }
                        Some(scheduler_message::Msg::CancelSession(cancel)) => {
                            if scheduler::session_cell(&cancel.session_id).ok() == Some(self.cell_id.as_str()) { self.orch_tx.send(QueueEvent::SessionCancelled { session_id: cancel.session_id }).await?; }
                        }
                        Some(scheduler_message::Msg::StopSession(stop)) => {
                            if scheduler::session_cell(&stop.session_id).ok() == Some(self.cell_id.as_str()) { self.orch_tx.send(QueueEvent::SessionCancelled { session_id: stop.session_id }).await?; }
                        }
                        Some(scheduler_message::Msg::Drain(_)) => self.slots.drain(),
                        Some(scheduler_message::Msg::Ping(_)) => { tx.send(self.heartbeat()).await?; }
                        None => {},
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scheduler::{
        grpc::{cell_control_server::CellControlServer, StartSession},
        CellControlService, SchedulerState,
    };
    #[tokio::test]
    async fn two_schedulers_share_one_cell_admission_authority() {
        let db = sled::Config::new().temporary(true).open().unwrap();
        let slots = Slots::new(&db, 1).unwrap();
        let (queue, mut events) = mpsc::channel(10);
        let manager = ConnectionManager::new("cell-1".into(), String::new(), queue, slots.clone());
        let mut clients = Vec::new();
        let mut tasks = Vec::new();
        for _ in 0..2 {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}", listener.local_addr().unwrap());
            let state = SchedulerState::new().unwrap();
            let service = CellControlService {
                state: state.clone(),
            };
            let incoming = futures_util::stream::unfold(listener, |listener| async {
                Some((listener.accept().await.map(|(socket, _)| socket), listener))
            });
            tasks.push(tokio::spawn(async move {
                tonic::transport::Server::builder()
                    .add_service(CellControlServer::new(service))
                    .serve_with_incoming(incoming)
                    .await
                    .unwrap();
            }));
            let client = CellControlClient::connect(endpoint.clone()).await.unwrap();
            let manager = manager.clone();
            tasks.push(tokio::spawn(async move {
                let _ = manager.connect_and_process(endpoint).await;
            }));
            tokio::time::timeout(Duration::from_secs(2), async {
                while !state.cells.contains_key("cell-1") {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            clients.push(client);
        }
        let mut second = clients.pop().unwrap();
        let mut first = clients.pop().unwrap();
        let id = "session_550e8400-e29b-41d4-a716-446655440000_cell-1";
        let request = StartSession {
            session_id: id.into(),
            user_id: "caller".into(),
            language: "en".into(),
            request_id: 0,
        };
        let (a, b) = tokio::join!(first.start(request.clone()), second.start(request));
        assert!(a.unwrap().into_inner().accepted);
        assert!(b.unwrap().into_inner().accepted);
        assert!(matches!(
            events.recv().await,
            Some(QueueEvent::SessionStarted { .. })
        ));
        assert!(events.try_recv().is_err());
        let result = first
            .start(StartSession {
                session_id: "session_550e8400-e29b-41d4-a716-446655440001_cell-1".into(),
                user_id: "other".into(),
                language: "en".into(),
                request_id: 0,
            })
            .await
            .unwrap()
            .into_inner();
        assert!(!result.accepted);
        assert_eq!(slots.used(), 1);
        for task in tasks {
            task.abort();
        }
    }
}
