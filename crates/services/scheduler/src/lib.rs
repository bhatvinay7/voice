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

#[derive(Clone, Debug)]
#[repr(align(64))]
pub struct CellState {
    pub cell_id: String,
    pub capacity: u32,
    pub active_sessions: u32,
    pub free_slots: u32,
    pub status: String,
}

#[derive(Clone)]
pub struct SchedulerState {
    pub cells: Arc<CachePadded<DashMap<String, CellState>>>,
    pub session_to_cell: Arc<CachePadded<DashMap<String, String>>>,
    // Bucket index: free_slots -> HashSet of cell_ids
    pub free_slots_index: Arc<CachePadded<DashMap<u32, std::collections::HashSet<String>>>>,
}

impl SchedulerState {
    pub fn new() -> Self {
        Self {
            cells: Arc::new(CachePadded::new(DashMap::new())),
            session_to_cell: Arc::new(CachePadded::new(DashMap::new())),
            free_slots_index: Arc::new(CachePadded::new(DashMap::new())),
        }
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

        tokio::spawn(async move {
            let _tx_keep_alive = tx;
            let mut current_cell_id = String::new();
            while let Ok(Some(msg)) = stream.message().await {
                if let Some(payload) = msg.msg {
                    match payload {
                        grpc::cell_message::Msg::Register(reg) => {
                            current_cell_id = reg.cell_id.clone();
                            state.register_cell(CellState {
                                cell_id: reg.cell_id.clone(),
                                capacity: reg.capacity,
                                active_sessions: reg.active_sessions,
                                free_slots: reg.free_slots,
                                status: reg.status,
                            });
                            state.update_metrics();
                            println!("[Scheduler] Registered cell: {}", reg.cell_id);
                        }
                        grpc::cell_message::Msg::SlotUpdate(update) => {
                            state.update_cell_slots(&current_cell_id, update.free_slots);
                            state.update_metrics();
                            println!("[Scheduler] Slot update for {}: free_slots={}", current_cell_id, update.free_slots);
                        }
                        grpc::cell_message::Msg::SessionStarted(evt) => {
                            state.session_to_cell.insert(evt.session_id.clone(), evt.cell_id.clone());
                            println!("[Scheduler] Session {} started on cell {}", evt.session_id, evt.cell_id);
                        }
                        grpc::cell_message::Msg::SessionEnded(evt) => {
                            state.session_to_cell.remove(&evt.session_id);
                            println!("[Scheduler] Session {} ended on cell {}", evt.session_id, evt.cell_id);
                        }
                        grpc::cell_message::Msg::Heartbeat(hb) => {
                            state.register_cell(CellState {
                                cell_id: hb.cell_id.clone(),
                                capacity: hb.capacity,
                                active_sessions: hb.active_sessions,
                                free_slots: hb.free_slots,
                                status: hb.status,
                            });
                            state.update_metrics();
                        }
                    }
                }
            }
            // Cleanup on disconnect
            if !current_cell_id.is_empty() {
                state.cells.remove(&current_cell_id);
                for i in 0..=50 {
                    if let Some(mut bucket) = state.free_slots_index.get_mut(&i) {
                        bucket.remove(&current_cell_id);
                    }
                }
                state.update_metrics();
            }
            println!("[Scheduler] Cell stream disconnected");
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
