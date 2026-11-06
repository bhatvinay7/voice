//! Scheduler state is a disposable view of connected cells; cells own admission.
pub mod grpc {
    tonic::include_proto!("scheduler");
}
pub mod tools_grpc {
    tonic::include_proto!("tools");
}
pub mod tool_service;
use dashmap::DashMap;
use grpc::{cell_control_server::CellControl, AdmissionResult, CellMessage, SchedulerMessage};
use lazy_static::lazy_static;
use prometheus::{register_int_gauge, IntGauge};
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
lazy_static! {
    pub static ref VOICE_LIVE_CELLS: IntGauge =
        register_int_gauge!("voice_live_cells", "Connected cells").unwrap();
    pub static ref VOICE_TOTAL_ACTIVE_SESSIONS: IntGauge = register_int_gauge!(
        "voice_total_active_sessions",
        "Active sessions observed by this scheduler; do not sum across schedulers"
    )
    .unwrap();
    pub static ref VOICE_TOTAL_FREE_SLOTS: IntGauge = register_int_gauge!(
        "voice_total_free_slots",
        "Free slots observed by this scheduler"
    )
    .unwrap();
}
#[derive(Clone, Debug)]
pub struct CellState {
    pub cell_id: String,
    pub capacity: u32,
    pub active_sessions: u32,
    pub free_slots: u32,
    pub status: String,
}
#[derive(Clone)]
pub struct SchedulerState {
    pub cells: Arc<DashMap<String, CellState>>,
    // Compatibility/diagnostics only; routing never relies on this map.
    pub session_to_cell: Arc<DashMap<String, String>>,
    pub cell_grpc_senders: Arc<DashMap<String, mpsc::Sender<Result<SchedulerMessage, Status>>>>,
    pending: Arc<DashMap<u64, (String, oneshot::Sender<AdmissionResult>)>>,
    next_request: Arc<AtomicU64>,
}
impl SchedulerState {
    pub fn new() -> sled::Result<Self> {
        Ok(Self {
            cells: Arc::new(DashMap::new()),
            session_to_cell: Arc::new(DashMap::new()),
            cell_grpc_senders: Arc::new(DashMap::new()),
            pending: Arc::new(DashMap::new()),
            next_request: Arc::new(AtomicU64::new(1)),
        })
    }
    pub fn new_with_db(_db: sled::Db) -> Self {
        Self::new().unwrap()
    }
    pub fn register_cell(&self, mut cell: CellState) {
        cell.active_sessions = cell.active_sessions.min(cell.capacity);
        cell.free_slots = cell.free_slots.min(cell.capacity - cell.active_sessions);
        self.cells.insert(cell.cell_id.clone(), cell);
        self.update_metrics();
    }
    pub fn update_cell_slots(&self, id: &str, free: u32) {
        if let Some(mut cell) = self.cells.get_mut(id) {
            cell.free_slots = free.min(cell.capacity);
            cell.active_sessions = cell.capacity - cell.free_slots;
        }
        self.update_metrics();
    }
    pub fn find_best_cell(&self) -> Option<String> {
        self.cells
            .iter()
            .filter(|c| c.free_slots > 0 && (c.status == "READY" || c.status == "Active"))
            .max_by_key(|c| c.free_slots)
            .map(|c| c.cell_id.clone())
    }
    pub fn update_metrics(&self) {
        VOICE_LIVE_CELLS.set(self.cells.len() as i64);
        VOICE_TOTAL_ACTIVE_SESSIONS.set(self.cells.iter().map(|c| c.active_sessions as i64).sum());
        VOICE_TOTAL_FREE_SLOTS.set(self.cells.iter().map(|c| c.free_slots as i64).sum());
    }
}
/// Routing information is NOT authentication. Expose this API only to trusted
/// internal callers; provider signatures and browser authorization stay at ingress.
pub fn session_cell(id: &str) -> Result<&str, Status> {
    let rest = id
        .strip_prefix("session_")
        .ok_or_else(|| Status::invalid_argument("expected session_UUID_CELL"))?;
    let (uuid, cell) = rest
        .split_once('_')
        .ok_or_else(|| Status::invalid_argument("missing cell"))?;
    if uuid.len() != 36
        || !uuid.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
        || cell.is_empty()
        || cell.len() > 253
        || !cell
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.')
    {
        return Err(Status::invalid_argument("invalid routed session ID"));
    }
    Ok(cell)
}
pub struct CellControlService {
    pub state: SchedulerState,
}
#[tonic::async_trait]
impl CellControl for CellControlService {
    type ConnectStreamStream = ReceiverStream<Result<SchedulerMessage, Status>>;
    async fn start(
        &self,
        request: Request<grpc::StartSession>,
    ) -> Result<Response<AdmissionResult>, Status> {
        let mut start = request.into_inner();
        let cell = session_cell(&start.session_id)?;
        let sender = self
            .state
            .cell_grpc_senders
            .get(cell)
            .map(|s| s.clone())
            .ok_or_else(|| Status::unavailable("cell not connected"))?;
        let id = start.session_id.clone();
        let (tx, rx) = oneshot::channel();
        let request_id = self.state.next_request.fetch_add(1, Ordering::Relaxed);
        start.request_id = request_id;
        self.state.pending.insert(request_id, (id.clone(), tx));
        // Correlation IDs prevent a late acknowledgement or cleanup from affecting a retry.
        let pending = self.state.pending.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(10)).await;
            pending.remove(&request_id);
        });
        if sender
            .try_send(Ok(SchedulerMessage {
                msg: Some(grpc::scheduler_message::Msg::StartSession(start)),
            }))
            .is_err()
        {
            self.state.pending.remove(&request_id);
            return Err(Status::resource_exhausted("cell control queue full"));
        }
        let result = tokio::time::timeout(Duration::from_secs(5), rx).await;
        // Keep a timed-out entry until cleanup to avoid an old acknowledgement
        // resolving a new request on this scheduler.
        match result {
            Ok(Ok(result)) => Ok(Response::new(result)),
            _ => Err(Status::deadline_exceeded(
                "admission outcome unknown; retry SAME session ID",
            )),
        }
    }
    async fn stop(
        &self,
        request: Request<grpc::StopSession>,
    ) -> Result<Response<AdmissionResult>, Status> {
        let stop = request.into_inner();
        let cell = session_cell(&stop.session_id)?;
        let sender = self
            .state
            .cell_grpc_senders
            .get(cell)
            .map(|s| s.clone())
            .ok_or_else(|| Status::unavailable("cell not connected"))?;
        let id = stop.session_id.clone();
        sender
            .try_send(Ok(SchedulerMessage {
                msg: Some(grpc::scheduler_message::Msg::StopSession(stop)),
            }))
            .map_err(|_| Status::resource_exhausted("cell queue full"))?;
        Ok(Response::new(AdmissionResult {
            session_id: id,
            accepted: true,
            reason: "stop queued; capacity released after shutdown".into(),
            request_id: 0,
        }))
    }
    async fn connect_stream(
        &self,
        request: Request<tonic::Streaming<CellMessage>>,
    ) -> Result<Response<Self::ConnectStreamStream>, Status> {
        let mut stream = request.into_inner();
        let (tx, rx) = mpsc::channel(100);
        let state = self.state.clone();
        tokio::spawn(async move {
            let mut id = String::new();
            loop {
                let message =
                    match tokio::time::timeout(Duration::from_secs(35), stream.message()).await {
                        Ok(Ok(Some(message))) => message,
                        _ => break,
                    };
                // A replaced stream must never overwrite or remove the new connection.
                let connection = if id.is_empty() {
                    None
                } else {
                    state.cell_grpc_senders.get(&id)
                };
                if !id.is_empty() && !connection.as_ref().is_some_and(|s| s.same_channel(&tx)) {
                    break;
                }
                match message.msg {
                    Some(grpc::cell_message::Msg::Register(reg))
                        if id.is_empty() && !reg.cell_id.is_empty() =>
                    {
                        id = reg.cell_id.clone();
                        let mut registration = state
                            .cell_grpc_senders
                            .entry(id.clone())
                            .or_insert_with(|| tx.clone());
                        *registration = tx.clone();
                        state.register_cell(CellState {
                            cell_id: id.clone(),
                            capacity: reg.capacity,
                            active_sessions: reg.active_sessions,
                            free_slots: reg.free_slots,
                            status: reg.status,
                        });
                    }
                    Some(grpc::cell_message::Msg::Heartbeat(hb))
                        if hb.cell_id == id && !id.is_empty() =>
                    {
                        state.register_cell(CellState {
                            cell_id: id.clone(),
                            capacity: hb.capacity,
                            active_sessions: hb.active_sessions,
                            free_slots: hb.free_slots,
                            status: hb.status,
                        });
                    }
                    Some(grpc::cell_message::Msg::AdmissionResult(result))
                        if session_cell(&result.session_id).ok() == Some(id.as_str()) =>
                    {
                        if let dashmap::mapref::entry::Entry::Occupied(entry) =
                            state.pending.entry(result.request_id)
                        {
                            if entry.get().0 == result.session_id {
                                let (_, sender) = entry.remove();
                                let _ = sender.send(result);
                            }
                        }
                    }
                    Some(grpc::cell_message::Msg::SlotUpdate(update)) if !id.is_empty() => {
                        state.update_cell_slots(&id, update.free_slots)
                    }
                    Some(grpc::cell_message::Msg::SessionStarted(evt)) if evt.cell_id == id => {
                        state.session_to_cell.insert(evt.session_id, id.clone());
                    }
                    Some(grpc::cell_message::Msg::SessionEnded(evt)) if evt.cell_id == id => {
                        state.session_to_cell.remove(&evt.session_id);
                    }
                    _ => break,
                }
            }
            // Entry lock makes removal and replacement mutually exclusive.
            if let dashmap::mapref::entry::Entry::Occupied(entry) =
                state.cell_grpc_senders.entry(id.clone())
            {
                if entry.get().same_channel(&tx) {
                    state.cells.remove(&id);
                    entry.remove();
                }
            }
            state.update_metrics();
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_routed_ids() {
        assert_eq!(
            session_cell("session_550e8400-e29b-41d4-a716-446655440000_cell-1").unwrap(),
            "cell-1"
        );
        for id in [
            "phone:twilio:a",
            "session_fake_cell-1",
            "session_550e8400-e29b-41d4-a716-446655440000_../../other",
        ] {
            assert!(session_cell(id).is_err());
        }
    }
    #[test]
    fn capacity_is_not_limited_to_fifty_and_updates_replace_old_view() {
        let s = SchedulerState::new().unwrap();
        s.register_cell(CellState {
            cell_id: "a".into(),
            capacity: 100,
            active_sessions: 0,
            free_slots: 100,
            status: "READY".into(),
        });
        assert_eq!(s.find_best_cell().as_deref(), Some("a"));
        s.register_cell(CellState {
            cell_id: "a".into(),
            capacity: 100,
            active_sessions: 100,
            free_slots: 100,
            status: "READY".into(),
        });
        assert_eq!(s.find_best_cell(), None);
        s.update_cell_slots("a", 200);
        assert_eq!(s.cells.get("a").unwrap().free_slots, 100);
    }
}

#[cfg(test)]
mod stream_tests {
    use super::*;
    use grpc::{cell_control_client::CellControlClient, cell_control_server::CellControlServer};
    async fn connect(
        client: &mut CellControlClient<tonic::transport::Channel>,
        free: u32,
    ) -> (
        mpsc::Sender<CellMessage>,
        tonic::Streaming<SchedulerMessage>,
    ) {
        let (tx, rx) = mpsc::channel(10);
        tx.send(CellMessage {
            msg: Some(grpc::cell_message::Msg::Register(grpc::RegisterCell {
                cell_id: "cell-1".into(),
                capacity: 2,
                active_sessions: 2 - free,
                free_slots: free,
                status: "READY".into(),
            })),
        })
        .await
        .unwrap();
        let stream = client
            .connect_stream(ReceiverStream::new(rx))
            .await
            .unwrap()
            .into_inner();
        (tx, stream)
    }
    #[tokio::test]
    async fn routes_admission_ack_and_fences_replaced_stream() {
        let state = SchedulerState::new().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let incoming = futures_util::stream::unfold(listener, |listener| async {
            Some((listener.accept().await.map(|(socket, _)| socket), listener))
        });
        let service = CellControlService {
            state: state.clone(),
        };
        let server = tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(CellControlServer::new(service))
                .serve_with_incoming(incoming)
                .await
                .unwrap();
        });
        let mut client = CellControlClient::connect(format!("http://{address}"))
            .await
            .unwrap();
        let (old_tx, old_stream) = connect(&mut client, 2).await;
        let (tx, mut stream) = connect(&mut client, 1).await;
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if state.cells.get("cell-1").is_some_and(|c| c.free_slots == 1) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        drop(old_tx);
        drop(old_stream);
        let id = "session_550e8400-e29b-41d4-a716-446655440000_cell-1";
        let mut caller = client.clone();
        let start = tokio::spawn(async move {
            caller
                .start(grpc::StartSession {
                    session_id: id.into(),
                    user_id: "u".into(),
                    language: "en".into(),
                    request_id: 0,
                })
                .await
        });
        let message = tokio::time::timeout(Duration::from_secs(2), stream.message())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let Some(grpc::scheduler_message::Msg::StartSession(request)) = message.msg else {
            panic!("expected admission")
        };
        tx.send(CellMessage {
            msg: Some(grpc::cell_message::Msg::AdmissionResult(AdmissionResult {
                session_id: id.into(),
                accepted: false,
                reason: "cell full".into(),
                request_id: request.request_id,
            })),
        })
        .await
        .unwrap();
        let result = start.await.unwrap().unwrap().into_inner();
        assert!(!result.accepted);
        assert_eq!(result.reason, "cell full");
        assert!(state.cell_grpc_senders.contains_key("cell-1"));
        assert!(state.pending.is_empty());
        server.abort();
    }
}
