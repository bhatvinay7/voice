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
    pub slots: crate::slots::Slots,
    pub phone_media: crate::phone_media::PhoneMediaRegistry,
    pub timer_wheel: Arc<SyncMutex<TimerWheel>>,
    // Routing handles only. The actors, graphs, task maps, and cancellation
    // tokens never leave their owning session.
    session_inboxes: Arc<DashMap<String, mpsc::Sender<SessionEvent>>>,
    pub task_to_session: Arc<DashMap<TaskId, String>>,
    pub pubsub: Arc<PubSubBus>,
    pub db: DatabaseStore,
    pub local_db: sled::Db,
    pub tool_client: ToolExecutionServiceClient<Channel>,
    pub queue_tx: mpsc::Sender<QueueEvent>,
    pub db_tx: mpsc::Sender<db::actor::DbRequest>,
    pub agent_factory:
        Arc<dyn Fn(RunId, mpsc::Sender<QueueEvent>) -> mpsc::Sender<AgentCmd> + Send + Sync>,
    pub inference_semaphore: Arc<tokio::sync::Semaphore>,
    pub durable_scheduler: crate::durable_scheduler::DurableTaskScheduler,
    pub durable_task_lease_ms: u64,
}

impl Clone for Orchestrator {
    fn clone(&self) -> Self {
        Self {
            slots: self.slots.clone(),
            phone_media: self.phone_media.clone(),
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
            inference_semaphore: self.inference_semaphore.clone(),
            durable_scheduler: self.durable_scheduler.clone(),
            durable_task_lease_ms: self.durable_task_lease_ms,
        }
    }
}

impl Orchestrator {
    pub(crate) fn remove_session_inbox(&self, session_id: &str) {
        self.session_inboxes.remove(session_id);
    }

    pub fn persist_recovery_marker(
        &self,
        session_id: &str,
        owner_epoch: u64,
    ) -> Result<(), String> {
        if session_id.trim().is_empty() {
            return Err("session_id cannot be empty".into());
        }
        let tree = self
            .local_db
            .open_tree("session_recovery")
            .map_err(|error| error.to_string())?;
        let key = format!("session:{session_id}");
        tree.insert(key.as_bytes(), owner_epoch.to_be_bytes().as_slice())
            .map_err(|error| error.to_string())?;
        tree.flush().map_err(|error| error.to_string()).map(|_| ())
    }

    /// Request copy-only diagnostics from the owning session actor. This is the
    /// only cross-session read API: callers never access a session graph or
    /// task map through the global registry.
    pub async fn request_session_state(
        &self,
        session_id: &str,
        timeout: Duration,
    ) -> Result<SessionStateSnapshot, String> {
        let inbox = self
            .session_inboxes
            .get(session_id)
            .map(|entry| entry.value().clone())
            .ok_or_else(|| format!("session '{session_id}' is not active"))?;
        let (reply_to, mut response_rx) = mpsc::channel(1);
        tokio::time::timeout(
            timeout,
            inbox.send(QueueEvent::SessionStateRequested {
                session_id: session_id.to_string(),
                reply_to,
            }),
        )
        .await
        .map_err(|_| format!("session '{session_id}' mailbox timed out"))?
        .map_err(|_| format!("session '{session_id}' mailbox is closed"))?;
        tokio::time::timeout(timeout, response_rx.recv())
            .await
            .map_err(|_| format!("session '{session_id}' state request timed out"))?
            .ok_or_else(|| format!("session '{session_id}' state reply channel closed"))
    }

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

        let durable_scheduler = crate::durable_scheduler::DurableTaskScheduler::open(&local_db)
            .expect("open durable task scheduler");
        let durable_task_lease_ms = config::AppConfig::load()
            .unwrap_or_else(|_| config::AppConfig::load_default())
            .orchestrator_task_lease_ms;
        let orchestrator = Self {
            slots: crate::slots::Slots::new(
                &local_db,
                std::env::var("MAX_SLOTS_PER_CELL")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .filter(|v| *v > 0)
                    .unwrap_or(20),
            )
            .expect("open cell slot store"),
            phone_media: crate::phone_media::PhoneMediaRegistry::default(),
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
            inference_semaphore: Arc::new(tokio::sync::Semaphore::new(8)),
            durable_scheduler,
            durable_task_lease_ms,
        };

        // Sled is used only as a recovery/status signal. The live actor still
        // wakes through MPSC; this watcher reconciles external recovery writes
        // and never carries transcript/token traffic.
        if let Ok(tree) = orchestrator.local_db.open_tree("session_recovery") {
            let recovery_queue = orchestrator.queue_tx.clone();
            tokio::task::spawn_blocking(move || {
                let changes = tree.watch_prefix(b"session:");
                for change in changes {
                    let key = match change {
                        sled::Event::Insert { key, .. } | sled::Event::Remove { key, .. } => key,
                    };
                    let Ok(key) = std::str::from_utf8(&key) else {
                        continue;
                    };
                    let Some(session_id) = key.strip_prefix("session:") else {
                        continue;
                    };
                    if session_id.is_empty() {
                        continue;
                    }
                    if recovery_queue
                        .blocking_send(QueueEvent::SessionResumed {
                            session_id: session_id.to_string(),
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            });
        }
        orchestrator
    }

    pub async fn run_dispatcher(
        protected_rx: ProtectedReceiver<QueueEvent>,
        mut orch: Orchestrator,
    ) {
        let reaper_slots = orch.slots.clone();
        let reaper_queue = orch.queue_tx.clone();
        tokio::spawn(async move {
            reaper_slots.run_reaper(reaper_queue).await;
        });
        loop {
            let fut = AssertUnwindSafe(async {
                // The receiver lock protects dequeueing only. Never retain it
                // while delivering work to a session: a full inbox in one
                // session must not prevent events for every other session from
                // leaving the global queue.
                let event = {
                    let mut rx = protected_rx.lock().await;
                    tokio::select! {
                        event = rx.recv() => event,
                        _ = tokio::time::sleep(Duration::from_millis(50)) => None,
                    }
                };

                if let Some(event) = event {
                    orch.dispatch_event(event).await;
                }

                // Run durable and in-memory deadline reconciliation on every
                // dispatcher cycle, not only when the global queue is idle.
                // Continuous audio traffic must not starve a scheduled retry.
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
                        query_id: None,
                        playback_generation: 0,
                        error: "Task execution timed out".to_string(),
                        retryable: true,
                    }))
                    .await;
                }

                let now_ms = chrono::Utc::now().timestamp_millis();
                if let Err(error) = orch.durable_scheduler.recover_expired_claims(now_ms) {
                    tracing::error!(%error, "durable scheduler claim recovery failed");
                }
                match orch
                    .durable_scheduler
                    .claim_due(now_ms, orch.durable_task_lease_ms, 32)
                {
                    Ok(tasks) => {
                        for task in tasks {
                            orch.dispatch_event(QueueEvent::ScheduledTaskDue {
                                session_id: task.session_id,
                                task_id: task.task_id,
                            })
                            .await;
                        }
                    }
                    Err(error) => {
                        tracing::error!(%error, "durable scheduler due-task claim failed")
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
            if let Some(run_start) = s.find("run-") {
                let run_id = &s[run_start + "run-".len()..];
                if !run_id.is_empty() {
                    return Some(run_id.to_string());
                }
            }
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
            QueueEvent::TelephonyAudio { session_id, .. } => Some(session_id.clone()),
            QueueEvent::SttTranscript { session_id, .. } => Some(session_id.clone()),
            QueueEvent::TranscriptFragment { session_id, .. } => Some(session_id.clone()),
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
                    AgentEvent::WaitForRelatedTool { run_id, .. } => &run_id.0,
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
            QueueEvent::GraphContextRequested { session_id, .. } => Some(session_id.clone()),
            QueueEvent::GraphEnrichmentReady { session_id, .. } => Some(session_id.clone()),
            QueueEvent::SessionStateRequested { session_id, .. } => Some(session_id.clone()),
            QueueEvent::CommitTranscript { session_id, .. } => Some(session_id.clone()),
            QueueEvent::ScheduledTaskDue { session_id, .. } => Some(session_id.clone()),
        };

        if let Some(sid) = session_id {
            let deleted_key = format!("session:deleted:{}", sid);
            if matches!(&event, QueueEvent::SessionStarted { .. }) {
                // An explicit new call may reuse an identifier after the old
                // tombstone has been dealt with.
                let _ = self.local_db.remove(deleted_key.as_bytes());
            } else if self
                .local_db
                .get(deleted_key.as_bytes())
                .ok()
                .flatten()
                .is_some()
            {
                // Deleted sessions are terminal. In particular, do not use the
                // legacy call-* fallback below to recreate an agent or resubscribe
                // it to Sled/Redis from a late event.
                tracing::debug!(%sid, "dropping event for deleted session");
                return;
            }
            if matches!(
                &event,
                QueueEvent::UserAudioChunk { .. }
                    | QueueEvent::TelephonyAudio { .. }
                    | QueueEvent::SttTranscript { .. }
                    | QueueEvent::TranscriptFragment { .. }
                    | QueueEvent::StsEvent { .. }
            ) {
                let _ = self.slots.touch(&sid, chrono::Utc::now().timestamp());
            }
            let mut is_new = false;
            if let QueueEvent::SessionStarted { .. } = &event {
                if self.session_inboxes.contains_key(&sid) {
                    return;
                }
                if let Err(error) = self.slots.acquire(&sid, chrono::Utc::now().timestamp()) {
                    tracing::warn!(%sid, %error, "cell admission rejected");
                    self.phone_media.close(&sid);
                    return;
                }
                let (tx, rx) = mpsc::channel(100);
                self.session_inboxes.insert(sid.clone(), tx);

                let session_actor = SessionActor::new(sid.clone(), self.clone());
                let cleanup = self.clone();
                let cleanup_id = sid.clone();
                tokio::spawn(async move {
                    if AssertUnwindSafe(session_actor.run(rx))
                        .catch_unwind()
                        .await
                        .is_err()
                    {
                        cleanup.phone_media.close(&cleanup_id);
                        cleanup.session_inboxes.remove(&cleanup_id);
                        let _ = cleanup.local_db.remove(format!("session:{}", cleanup_id));
                    }
                    let _ = cleanup.slots.release(&cleanup_id);
                });
                is_new = true;
                let _ = self
                    .local_db
                    .insert(format!("session:{}", sid).as_bytes(), b"active");
                let _ = self.local_db.flush();
            }

            let inbox = self
                .session_inboxes
                .get(&sid)
                .map(|entry| entry.value().clone());
            if let Some(tx) = inbox {
                // Keep the global dispatcher non-blocking, but never discard
                // terminal agent/tool work: losing Agent::Finished is exactly
                // equivalent to the caller hearing no answer.
                let must_deliver = matches!(
                    &event,
                    QueueEvent::SessionCancelled { .. }
                        | QueueEvent::CommitTranscript { .. }
                        | QueueEvent::ScheduledTaskDue { .. }
                        | QueueEvent::Agent(
                            AgentEvent::Finished { .. } | AgentEvent::Failed { .. }
                        )
                        | QueueEvent::Tool(ToolEvent::Done { .. } | ToolEvent::Failed { .. })
                );
                match tx.try_send(event.clone()) {
                    Ok(()) => {}
                    Err(tokio::sync::mpsc::error::TrySendError::Full(_)) if must_deliver => {
                        let retry_tx = tx.clone();
                        tokio::spawn(async move {
                            match tokio::time::timeout(Duration::from_secs(2), retry_tx.send(event))
                                .await
                            {
                                Ok(Ok(())) => {}
                                Ok(Err(error)) => {
                                    tracing::error!(%error, "critical session event could not be delivered; inbox closed")
                                }
                                Err(_) => {
                                    tracing::error!("critical session event delivery timed out")
                                }
                            }
                        });
                    }
                    Err(error) => {
                        tracing::warn!(%sid, %error, "session inbox backpressure; noncritical event dropped");
                    }
                }
            } else if !is_new {
                if matches!(&event, QueueEvent::SessionCancelled { .. }) {
                    self.phone_media.close(&sid);
                    let _ = self.slots.release(&sid);
                    let _ = self.local_db.remove(format!("session:{}", sid));
                    let _ = self.local_db.insert(deleted_key.as_bytes(), b"deleted");
                    let _ = self.local_db.flush();
                    return;
                }
                // Check Sled persistent store or active call- prefix for graceful session recovery
                let is_persisted = match self.local_db.get(format!("session:{}", sid).as_bytes()) {
                    Ok(Some(_)) => true,
                    _ => {
                        sid.starts_with("call-")
                            && !matches!(&event, QueueEvent::SessionCancelled { .. })
                    }
                };

                if is_persisted && !matches!(&event, QueueEvent::SessionCancelled { .. }) {
                    if self
                        .slots
                        .acquire(&sid, chrono::Utc::now().timestamp())
                        .is_err()
                    {
                        return;
                    }
                    let (tx, rx) = mpsc::channel(100);
                    self.session_inboxes.insert(sid.clone(), tx.clone());

                    let session_actor = SessionActor::new(sid.clone(), self.clone());
                    let cleanup = self.clone();
                    let cleanup_id = sid.clone();
                    tokio::spawn(async move {
                        let _ = AssertUnwindSafe(session_actor.run(rx)).catch_unwind().await;
                        cleanup.phone_media.close(&cleanup_id);
                        cleanup.session_inboxes.remove(&cleanup_id);
                        let _ = cleanup.slots.release(&cleanup_id);
                    });

                    if let Err(error) = tx.try_send(event.clone()) {
                        tracing::warn!(%sid, %error, "recovered session inbox unavailable; event dropped");
                    }
                } else {
                    eprintln!("[Dispatcher] Event dropped for unknown session: {}", sid);
                }
            }
        }
    }
}
