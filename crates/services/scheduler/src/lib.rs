pub mod grpc {
    tonic::include_proto!("scheduler");
}

pub mod tools_grpc {
    tonic::include_proto!("tools");
}

pub mod tool_service;

use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use grpc::cell_control_server::CellControl;
use grpc::{CellMessage, SchedulerMessage};

use prometheus::{IntGauge, register_int_gauge};
use lazy_static::lazy_static;

lazy_static! {
    pub static ref VOICE_LIVE_CELLS: IntGauge = register_int_gauge!("voice_live_cells", "Number of live cells").unwrap();
    pub static ref VOICE_TOTAL_ACTIVE_SESSIONS: IntGauge = register_int_gauge!("voice_total_active_sessions", "Total active sessions").unwrap();
    pub static ref VOICE_TOTAL_FREE_SLOTS: IntGauge = register_int_gauge!("voice_total_free_slots", "Total free slots across all cells").unwrap();
}

use crossbeam_utils::CachePadded;

use serde::{Serialize, Deserialize};
use bincode;
use chrono::Utc;
use sled::Db;
use std::time::Duration;
use std::collections::{HashMap};

use tokio_util::time::DelayQueue;
use tokio_util::time::delay_queue;
use delay_queue::Key as DelayKey;
use tokio::task::JoinHandle;
use std::panic::AssertUnwindSafe;
use futures_util::{FutureExt, StreamExt};
use tokio::time::sleep as tokio_sleep;
use std::time::Duration as StdDuration;

#[derive(Clone, Debug)]
#[repr(align(64))]
pub struct CellState {
    pub cell_id: String,
    pub capacity: u32,
    pub active_sessions: u32,
    pub free_slots: u32,
    pub status: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SessionRecord {
    start_ts: i64,
    cell_id: String,
    max_secs: u64,
    delayed_job_id: Option<String>,
}

#[derive(Debug)]
enum CellTimeoutCommand {
    Add { session_id: String, duration: Duration, delayed_job_id: Option<String> },
    Cancel { session_id: String },
    Shutdown,
}

#[derive(Clone, Debug)]
pub enum DecisionEventKind {
    TimeoutExpired,
    SessionCancelled,
    SessionEnded,
}

#[derive(Clone, Debug)]
pub struct DecisionMessage {
    pub session_id: String,
    pub cell_id: Option<String>,
    pub kind: DecisionEventKind,
    pub ts: i64,
}

#[derive(Clone)]
pub struct SchedulerState {
    pub db: Arc<Db>,
    pub cells: Arc<CachePadded<DashMap<String, CellState>>>,
    pub session_to_cell: Arc<CachePadded<DashMap<String, String>>>,
    pub free_slots_index: Arc<CachePadded<DashMap<u32, std::collections::HashSet<String>>>>,
    pub cell_timeout_senders: Arc<CachePadded<DashMap<String, mpsc::Sender<CellTimeoutCommand>>>>,
    pub cell_grpc_senders: Arc<CachePadded<DashMap<String, mpsc::Sender<Result<SchedulerMessage, Status>>>>>,
    pub decision_tx: Arc<CachePadded<DashMap<String, mpsc::Sender<DecisionMessage>>>>,
}

impl SchedulerState {
    pub fn new() -> Result<Self, sled::Error> {
        let db = sled::Config::new().temporary(true).open()?;
        Ok(Self::new_with_db(db))
    }

    pub fn new_with_db(db: Db) -> Self {
        Self {
            db: Arc::new(db),
            cells: Arc::new(CachePadded::new(DashMap::new())),
            session_to_cell: Arc::new(CachePadded::new(DashMap::new())),
            free_slots_index: Arc::new(CachePadded::new(DashMap::new())),
            cell_timeout_senders: Arc::new(CachePadded::new(DashMap::new())),
            cell_grpc_senders: Arc::new(CachePadded::new(DashMap::new())),
            decision_tx: Arc::new(CachePadded::new(DashMap::new())),
        }
    }

    pub fn persist_session_start(&self, session_id: &str, cell_id: &str, max_secs: u64, delayed_job_id: Option<String>) -> sled::Result<()> {
        let rec = SessionRecord {
            start_ts: Utc::now().timestamp(),
            cell_id: cell_id.to_string(),
            max_secs,
            delayed_job_id,
        };
        let bytes = bincode::serialize(&rec).unwrap();
        self.db.insert(session_id.as_bytes(), bytes)?;
        let _ = self.db.flush();
        Ok(())
    }

    pub fn remove_session_record(&self, session_id: &str) -> sled::Result<Option<SessionRecord>> {
        if let Some(old) = self.db.remove(session_id.as_bytes())? {
            let rec: SessionRecord = bincode::deserialize(&old).unwrap();
            let _ = self.db.flush();
            Ok(Some(rec))
        } else {
            Ok(None)
        }
    }

    pub fn ensure_cell_manager(&self, cell_id: &str) -> mpsc::Sender<CellTimeoutCommand> {
        if let Some(tx) = self.cell_timeout_senders.get(cell_id) {
            return tx.value().clone();
        }

        let (tx, rx) = mpsc::channel(256);
        self.cell_timeout_senders.insert(cell_id.to_string(), tx.clone());
        let state = Arc::new(self.clone());
        let cell = cell_id.to_string();
        state.clone().spawn_supervised_cell_manager(cell, rx);
        tx
    }

    pub fn start_reaper(self: Arc<Self>, expiry_seconds: i64, scan_interval: Duration) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(scan_interval);
            loop {
                interval.tick().await;
                let now_ts = Utc::now().timestamp();
                let mut batch = Vec::new();
                for item in self.db.iter().take(1000) {
                    match item {
                        Ok((k, v)) => {
                            if let Ok(rec) = bincode::deserialize::<SessionRecord>(&v) {
                                if now_ts - rec.start_ts >= expiry_seconds {
                                    if let Ok(key_str) = String::from_utf8(k.to_vec()) {
                                        batch.push(key_str);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("sled iteration error: {:?}", e);
                        }
                    }
                }
                for session_id in batch {
                    match self.remove_session_record(&session_id) {
                        Ok(Some(rec)) => {
                            tracing::info!("reaper: expired session {} (cell {}) - cleaning", session_id, rec.cell_id);
                            if let Some(job_id) = rec.delayed_job_id {
                                tracing::debug!("reaper: would cancel external job id {}", job_id);
                            }
                            if let Some((_, cell_id)) = self.session_to_cell.remove(&session_id) {
                                if let Some(mut c) = self.cells.get_mut(&cell_id) {
                                    let new_free = c.free_slots.saturating_add(1);
                                    self.update_cell_slots(&cell_id, new_free);
                                }
                            }
                            let dm = DecisionMessage { session_id: session_id.clone(), cell_id: Some(rec.cell_id.clone()), kind: DecisionEventKind::TimeoutExpired, ts: Utc::now().timestamp() };
                            for entry in self.decision_tx.iter() {
                                let tx = entry.value();
                                let _ = tx.try_send(dm.clone());
                            }
                            self.update_metrics();
                        }
                        Ok(None) => {}
                        Err(e) => {
                            tracing::error!("reaper remove_session_record error for {}: {:?}", session_id, e);
                        }
                    }
                }
            }
        })
    }

    pub fn start_aggregated_broadcaster(self: Arc<Self>, interval: Duration) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(interval);
            loop {
                tick.tick().await;
                let sched_msg = SchedulerMessage::default();
                for sender_entry in self.cell_grpc_senders.iter() {
                    let tx = sender_entry.value();
                    let _ = tx.try_send(Ok(sched_msg.clone()));
                }
            }
        })
    }

    pub fn update_metrics(&self) {
        VOICE_LIVE_CELLS.set(self.cells.len() as i64);
        let mut active = 0;
        let mut free = 0;
        for cell in self.cells.iter() {
            active += cell.active_sessions;
            free += cell.free_slots;
        }
        VOICE_TOTAL_ACTIVE_SESSIONS.set(active as i64);
        VOICE_TOTAL_FREE_SLOTS.set(free as i64);
    }

    pub fn register_cell(&self, cell: CellState) {
        let cell_id = cell.cell_id.clone();
        let free_slots = cell.free_slots;
        self.cells.insert(cell_id.clone(), cell);

        self.free_slots_index
            .entry(free_slots)
            .or_default()
            .insert(cell_id);
    }

    pub fn update_cell_slots(&self, cell_id: &str, new_free_slots: u32) {
        if let Some(mut cell) = self.cells.get_mut(cell_id) {
            let old_free = cell.free_slots;
            cell.free_slots = new_free_slots;

            if old_free != new_free_slots {
                if let Some(mut old_bucket) = self.free_slots_index.get_mut(&old_free) {
                    old_bucket.remove(cell_id);
                }
                self.free_slots_index
                    .entry(new_free_slots)
                    .or_default()
                    .insert(cell_id.to_string());
            }
        }
    }

    pub fn find_best_cell(&self) -> Option<String> {
        for i in (1..=50).rev() {
            if let Some(bucket) = self.free_slots_index.get(&i) {
                if let Some(cell_id) = bucket.iter().next() {
                    return Some(cell_id.clone());
                }
            }
        }
        None
    }

    pub fn spawn_supervised_cell_manager(self: Arc<Self>, cell_id: String, mut rx: mpsc::Receiver<CellTimeoutCommand>) {
        let state = self.clone();
        tokio::spawn(async move {
            let mut restart_count: u32 = 0;
            loop {
                let manager_future = start_cell_timeout_manager(state.clone(), cell_id.clone(), rx);
                let res = AssertUnwindSafe(manager_future).catch_unwind().await;
                match res {
                    Ok(_) => {
                        tracing::info!("Supervisor: cell manager for {} exited gracefully", cell_id);
                        let _ = state.cell_timeout_senders.remove(&cell_id);
                        break;
                    }
                    Err(panic_info) => {
                        restart_count = restart_count.saturating_add(1);
                        tracing::error!("Supervisor: cell manager for {} panicked (restart #{}) - {:?}", cell_id, restart_count, panic_info);
                        let backoff = std::cmp::min(5000, 100 * (1u32 << std::cmp::min(restart_count, 6)) as u32);
                        tokio_sleep(StdDuration::from_millis(backoff as u64)).await;
                        let (tx, new_rx) = mpsc::channel::<CellTimeoutCommand>(256);
                        state.cell_timeout_senders.insert(cell_id.clone(), tx.clone());
                        rx = new_rx;
                    }
                }
            }
        });
    }
}

async fn start_cell_timeout_manager(state: Arc<SchedulerState>, cell_id: String, mut rx: mpsc::Receiver<CellTimeoutCommand>) {
    let mut dq: DelayQueue<String> = DelayQueue::new();
    let mut key_map: HashMap<String, DelayKey> = HashMap::new();

    loop {
        tokio::select! {
            maybe_cmd = rx.recv() => {
                match maybe_cmd {
                    Some(CellTimeoutCommand::Add { session_id, duration, delayed_job_id }) => {
                        let _ = state.persist_session_start(&session_id, &cell_id, duration.as_secs(), delayed_job_id.clone());
                        if let Some(old_k) = key_map.remove(&session_id) {
                            let _ = dq.remove(&old_k);
                        }
                        let k = dq.insert(session_id.clone(), duration);
                        key_map.insert(session_id.clone(), k);
                        state.session_to_cell.insert(session_id.clone(), cell_id.clone());
                        state.update_metrics();
                    }
                    Some(CellTimeoutCommand::Cancel { session_id }) => {
                        let removed = state.remove_session_record(&session_id).ok().flatten();
                        if let Some(k) = key_map.remove(&session_id) {
                            let _ = dq.remove(&k);
                        }
                        if let Some((_, mapped_cell)) = state.session_to_cell.remove(&session_id) {
                            if let Some(mut c) = state.cells.get_mut(&mapped_cell) {
                                let new_free = c.free_slots.saturating_add(1);
                                state.update_cell_slots(&mapped_cell, new_free);
                            }
                        }
                        state.update_metrics();
                        let dm = DecisionMessage { session_id: session_id.clone(), cell_id: removed.as_ref().map(|r| r.cell_id.clone()), kind: DecisionEventKind::SessionCancelled, ts: Utc::now().timestamp() };
                        for entry in state.decision_tx.iter() {
                            let tx = entry.value();
                            if let Err(e) = tx.try_send(dm.clone()) {
                                tracing::warn!("decision_tx try_send failed for {}: {:?}", &session_id, e);
                            }
                        }
                    }
                    Some(CellTimeoutCommand::Shutdown) => {
                        tracing::info!("Cell manager {} received Shutdown", cell_id);
                        break;
                    }
                    None => {
                        tracing::info!("Cell manager {} command channel closed", cell_id);
                        break;
                    }
                }
            }

            expired = dq.next(), if !dq.is_empty() => {
                if let Some(expired) = expired {
                    let session_id = expired.into_inner();
                    key_map.remove(&session_id);

                    match state.remove_session_record(&session_id) {
                        Ok(Some(rec)) => {
                            let _ = state.session_to_cell.remove(&session_id);
                            if let Some(mut c) = state.cells.get_mut(&rec.cell_id) {
                                let new_free = c.free_slots.saturating_add(1);
                                state.update_cell_slots(&rec.cell_id, new_free);
                            }
                            state.update_metrics();
                            let dm = DecisionMessage { session_id: session_id.clone(), cell_id: Some(rec.cell_id.clone()), kind: DecisionEventKind::TimeoutExpired, ts: Utc::now().timestamp() };
                            for entry in state.decision_tx.iter() {
                                let tx = entry.value();
                                if let Err(e) = tx.try_send(dm.clone()) {
                                    tracing::warn!("decision_tx try_send failed for expired {}: {:?}", session_id, e);
                                }
                            }
                            tracing::info!("Session {} expired on cell {}", session_id, rec.cell_id);
                        }
                        Ok(None) => {
                            tracing::debug!("Expired session {} already removed in sled", session_id);
                        }
                        Err(e) => {
                            tracing::error!("sled remove error on expiry {}: {:?}", session_id, e);
                        }
                    }
                }
            }
        }
    }

    for (session_id, _) in key_map.into_iter() {
        let _ = state.remove_session_record(&session_id);
        let _ = state.session_to_cell.remove(&session_id);
    }

    tracing::info!("Cell timeout manager for {} exiting", cell_id);
}

pub struct CellControlService {
    pub state: SchedulerState,
}

#[tonic::async_trait]
impl CellControl for CellControlService {
    type ConnectStreamStream = ReceiverStream<Result<SchedulerMessage, Status>>;

    async fn connect_stream(
        &self,
        request: Request<tonic::Streaming<CellMessage>>,
    ) -> Result<Response<Self::ConnectStreamStream>, Status> {
        let mut stream = request.into_inner();
        let (tx, rx) = mpsc::channel(100);
        let state = self.state.clone();

        let state_arc = Arc::new(state.clone());
        let tx_clone_for_registry = tx.clone();

        tokio::spawn(async move {
            let mut current_cell_id = String::new();

            while let Ok(Some(msg)) = stream.message().await {
                if let Some(payload) = msg.msg {
                    match payload {
                        grpc::cell_message::Msg::Register(reg) => {
                            current_cell_id = reg.cell_id.clone();
                            state_arc.cell_grpc_senders.insert(current_cell_id.clone(), tx_clone_for_registry.clone());
                            let _ = state_arc.ensure_cell_manager(&current_cell_id);

                            state_arc.register_cell(CellState {
                                cell_id: reg.cell_id.clone(),
                                capacity: reg.capacity,
                                active_sessions: reg.active_sessions,
                                free_slots: reg.free_slots,
                                status: reg.status,
                            });
                            state_arc.update_metrics();
                            println!("[Scheduler] Registered cell: {}", reg.cell_id);
                        }
                        grpc::cell_message::Msg::SlotUpdate(update) => {
                            if !current_cell_id.is_empty() {
                                state_arc.update_cell_slots(&current_cell_id, update.free_slots);
                                state_arc.update_metrics();
                                println!("[Scheduler] Slot update for {}: free_slots={}", current_cell_id, update.free_slots);
                            } else {
                                tracing::warn!("received SlotUpdate but no current_cell_id set");
                            }
                        }
                        grpc::cell_message::Msg::SessionStarted(evt) => {
                            let session_id = evt.session_id.clone();
                            let cell_id = evt.cell_id.clone();
                            let cell_mgr = state_arc.ensure_cell_manager(&cell_id);
                            let duration = Duration::from_secs(20 * 60);
                            let _ = cell_mgr.send(CellTimeoutCommand::Add {
                                session_id: session_id.clone(),
                                duration,
                                delayed_job_id: None,
                            }).await;
                            println!("[Scheduler] Session {} started on cell {}", session_id, cell_id);
                        }
                        grpc::cell_message::Msg::SessionEnded(evt) => {
                            let session_id = evt.session_id.clone();
                            let cell_id = evt.cell_id.clone();
                            if let Some(tx_mgr) = state_arc.cell_timeout_senders.get(&cell_id) {
                                let _ = tx_mgr.value().send(CellTimeoutCommand::Cancel { session_id: session_id.clone() }).await;
                            } else {
                                let _ = state_arc.remove_session_record(&session_id);
                                if let Some((_, mapped_cell)) = state_arc.session_to_cell.remove(&session_id) {
                                    if let Some(mut c) = state_arc.cells.get_mut(&mapped_cell) {
                                        let new_free = c.free_slots.saturating_add(1);
                                        state_arc.update_cell_slots(&mapped_cell, new_free);
                                    }
                                }
                                state_arc.update_metrics();
                            }
                            println!("[Scheduler] Session {} ended on cell {}", session_id, evt.cell_id);
                        }
                        grpc::cell_message::Msg::Heartbeat(hb) => {
                            let cid = hb.cell_id.clone();
                            let _ = state_arc.ensure_cell_manager(&cid);
                            state_arc.register_cell(CellState {
                                cell_id: hb.cell_id.clone(),
                                capacity: hb.capacity,
                                active_sessions: hb.active_sessions,
                                free_slots: hb.free_slots,
                                status: hb.status,
                            });
                            state_arc.update_metrics();
                        }
                    }
                }
            }

            if !current_cell_id.is_empty() {
                tracing::info!("[Scheduler] Cell {} disconnected; cleaning up", current_cell_id);
                state_arc.cell_grpc_senders.remove(&current_cell_id);
                if let Some((_, tx_mgr)) = state_arc.cell_timeout_senders.remove(&current_cell_id) {
                    let _ = tx_mgr.send(CellTimeoutCommand::Shutdown).await;
                }
                state_arc.cells.remove(&current_cell_id);
                for i in 0..=50 {
                    if let Some(mut bucket) = state_arc.free_slots_index.get_mut(&i) {
                        bucket.remove(&current_cell_id);
                    }
                }
                let mut sessions_to_remove = Vec::new();
                for entry in state_arc.session_to_cell.iter() {
                    if entry.value() == &current_cell_id {
                        sessions_to_remove.push(entry.key().clone());
                    }
                }
                for sess in sessions_to_remove {
                    let _ = state_arc.remove_session_record(&sess);
                    let _ = state_arc.session_to_cell.remove(&sess);
                }
                state_arc.update_metrics();
            }

            println!("[Scheduler] Cell stream disconnected");
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
