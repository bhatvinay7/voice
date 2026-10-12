use super::session::SessionActor;
use super::timer::TimerWheel;
use crate::grpc::tools::tool_execution_service_client::ToolExecutionServiceClient;
use crate::models::*;
use dashmap::DashMap;
use db::DatabaseStore;
use futures_util::{FutureExt, StreamExt};
use parking_lot::Mutex as SyncMutex;
use pubsub::PubSubBus;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tonic::transport::Channel;

pub type ProtectedReceiver<T> = Arc<Mutex<mpsc::Receiver<T>>>;

pub struct CellChannels {
    pub intake_tx: mpsc::Sender<SessionEvent>,
    pub control_tx: mpsc::UnboundedSender<SessionEvent>,
    pub bulk_tx: tokio::sync::broadcast::Sender<SessionEvent>,
}

pub struct Orchestrator {
    pub timer_wheel: Arc<SyncMutex<TimerWheel>>,
    pub session_inboxes: Arc<DashMap<String, mpsc::Sender<SessionEvent>>>,
    pub task_to_session: Arc<DashMap<TaskId, String>>,
    pub pubsub: Arc<PubSubBus>,
    pub db: DatabaseStore,
    pub local_db: sled::Db,
    pub tool_client: ToolExecutionServiceClient<Channel>,
    pub queue_tx: mpsc::Sender<QueueEvent>,
    pub db_tx: mpsc::Sender<db::actor::DbRequest>,
    pub agent_factory:
        Arc<dyn Fn(RunId, mpsc::Sender<QueueEvent>) -> mpsc::Sender<AgentCmd> + Send + Sync>,
}

impl Clone for Orchestrator {
    fn clone(&self) -> Self {
        Self {
            timer_wheel: self.timer_wheel.clone(),
            session_inboxes: self.session_inboxes.clone(),
            task_to_session: self.task_to_session.clone(),
            pubsub: self.pubsub.clone(),
            db: self.db.clone(),
            local_db: self.local_db.clone(),
            tool_client: self.tool_client.clone(),
            queue_tx: self.queue_tx.clone(),
            db_tx: self.db_tx.clone(),
            agent_factory: self.agent_factory.clone(),
        }
    }
}

impl Orchestrator {
    pub fn new(
        pubsub: Arc<PubSubBus>,
        db: DatabaseStore,
        local_db: sled::Db,
        tool_client: ToolExecutionServiceClient<Channel>,
        queue_tx: mpsc::Sender<QueueEvent>,
        db_tx: mpsc::Sender<db::actor::DbRequest>,
        agent_factory: Arc<
            dyn Fn(RunId, mpsc::Sender<QueueEvent>) -> mpsc::Sender<AgentCmd> + Send + Sync,
        >,
    ) -> Self {
        let mut timers = TimerWheel::new();

        let prefix = b"timeout:";
        for item in local_db.scan_prefix(prefix) {
            if let Ok((k, v)) = item {
                if let Ok(key_str) = std::str::from_utf8(&k) {
                    let task_id_str = key_str.trim_start_matches("timeout:");
                    if v.len() == 8 {
                        let mut bytes = [0u8; 8];
                        bytes.copy_from_slice(&v);
                        let expire_time = i64::from_be_bytes(bytes);
                        let now = chrono::Utc::now().timestamp_millis();

                        let remaining_ms = (expire_time - now).max(1);
                        let key = timers.queue.insert(
                            TaskId(task_id_str.to_string()),
                            Duration::from_millis(remaining_ms as u64),
                        );
                        timers.keys.insert(TaskId(task_id_str.to_string()), key);
                    }
                }
            }
        }

        Self {
            timer_wheel: Arc::new(SyncMutex::new(timers)),
            session_inboxes: Arc::new(DashMap::new()),
            task_to_session: Arc::new(DashMap::new()),
            pubsub,
            db,
            local_db,
            tool_client,
            queue_tx,
            db_tx,
            agent_factory,
        }
    }

    pub async fn run_dispatcher(
        protected_rx: ProtectedReceiver<QueueEvent>,
        mut orch: Orchestrator,
    ) {
        loop {
            let fut = AssertUnwindSafe(async {
                let mut rx = protected_rx.lock().await;
                tokio::select! {
                    Some(event) = rx.recv() => {
                        orch.dispatch_event(event).await;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(50)) => {
                        let expired_tasks = {
                            let mut wheel = orch.timer_wheel.lock();
                            let mut expired = Vec::new();
                            while let Some(Some(item)) = wheel.queue.next().now_or_never() {
                                let task_id = item.into_inner();
                                wheel.keys.remove(&task_id);
                                expired.push(task_id);
                            }
                            expired
                        };

                        for task_id in expired_tasks {
                            orch.dispatch_event(QueueEvent::Tool(ToolEvent::Failed {
                                task_id,
                                attempt: 0,
                                error: "Task execution timed out".to_string(),
                                retryable: true,
                            })).await;
                        }
                    }
                }
            });

            if let Err(_) = fut.catch_unwind().await {
                eprintln!("[Orchestrator] Panic intercepted safely in dispatcher!");
            }
        }
    }

    pub fn resolve_task_session(&self, task_id: &TaskId) -> Option<String> {
        if let Some(entry) = self.task_to_session.get(task_id) {
            return Some(entry.value().clone());
        }
        let task_id_str = &task_id.0;
        if task_id_str.starts_with("task-run-") {
            let s = task_id_str.replace("task-run-", "");
            if let Some(idx) = s.rfind('-') {
                return Some(s[..idx].to_string());
            } else {
                return Some(s);
            }
        }
        if task_id_str.starts_with("task-") {
            let s = task_id_str.trim_start_matches("task-");
            if let Some(idx) = s.rfind('-') {
                return Some(s[..idx].to_string());
            } else {
                return Some(s.to_string());
            }
        }
        None
    }

    pub async fn dispatch_event(&mut self, event: QueueEvent) {
        let session_id = match &event {
            QueueEvent::SessionStarted { session_id, .. } => Some(session_id.clone()),
            QueueEvent::SessionResumed { session_id } => Some(session_id.clone()),
            QueueEvent::SessionCancelled { session_id } => Some(session_id.clone()),
            QueueEvent::UserAudioChunk { session_id, .. } => Some(session_id.clone()),
            QueueEvent::SttTranscript { session_id, .. } => Some(session_id.clone()),
            QueueEvent::InterviewerSpeech { session_id, .. } => Some(session_id.clone()),
            QueueEvent::BargeInTrigger { session_id, .. } => Some(session_id.clone()),
            QueueEvent::StsEvent { session_id, .. } => Some(session_id.clone()),
            QueueEvent::GraphNodesFetched { session_id, .. } => Some(session_id.clone()),
            QueueEvent::Agent(agent_event) => {
                let run_id = match agent_event {
                    AgentEvent::ToolRequested { run_id, .. } => &run_id.0,
                    AgentEvent::Finished { run_id, .. } => &run_id.0,
                    AgentEvent::Failed { run_id, .. } => &run_id.0,
                    AgentEvent::NeedsUserInput { run_id, .. } => &run_id.0,
                };
                let sid = run_id.strip_prefix("run-").unwrap_or(run_id);
                Some(sid.to_string())
            }
            QueueEvent::Tool(tool_event) => {
                let task_id = match tool_event {
                    ToolEvent::Done { task_id, .. } => task_id,
                    ToolEvent::Failed { task_id, .. } => task_id,
                    ToolEvent::Heartbeat { task_id, .. } => task_id,
                };
                self.resolve_task_session(task_id)
            }
            QueueEvent::DbActionCompleted { task_id, .. } => self.resolve_task_session(task_id),
        };

        if let Some(sid) = session_id {
            let mut is_new = false;
            if let QueueEvent::SessionStarted { .. } = &event {
                let (tx, rx) = mpsc::channel(100);
                self.session_inboxes.insert(sid.clone(), tx);

                let session_actor = SessionActor::new(sid.clone(), self.clone());
                tokio::spawn(async move {
                    session_actor.run(rx).await;
                });
                is_new = true;
                let _ = self
                    .local_db
                    .insert(format!("session:{}", sid).as_bytes(), b"active");
                let _ = self.local_db.flush();
            }

            if let Some(tx) = self.session_inboxes.get(&sid) {
                let _ = tx.send(event.clone()).await;
            } else if !is_new {
                // Check Sled persistent store or active call- prefix for graceful session recovery
                let is_persisted = match self.local_db.get(format!("session:{}", sid).as_bytes()) {
                    Ok(Some(_)) => true,
                    _ => {
                        sid.starts_with("call-")
                            && !matches!(&event, QueueEvent::SessionCancelled { .. })
                    }
                };

                if is_persisted {
                    let (tx, rx) = mpsc::channel(100);
                    self.session_inboxes.insert(sid.clone(), tx.clone());

                    let session_actor = SessionActor::new(sid.clone(), self.clone());
                    tokio::spawn(async move {
                        session_actor.run(rx).await;
                    });

                    let _ = tx.send(event.clone()).await;
                } else {
                    eprintln!("[Dispatcher] Event dropped for unknown session: {}", sid);
                }
            }
        }
    }
}
