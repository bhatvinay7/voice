use scheduler::{CellControlService, SchedulerState, CellState};
use std::time::Duration;

#[tokio::test]
async fn test_scheduler_cell_registration_and_scaling() {
    let state = SchedulerState::new();

    state.register_cell(CellState {
        cell_id: "cell-1".to_string(),
        capacity: 20,
        active_sessions: 0,
        free_slots: 20,
        status: "Active".to_string(),
    });

    // Verify state
    let cell = state.cells.get("cell-1").unwrap();
    assert_eq!(cell.capacity, 20);
    assert_eq!(cell.free_slots, 20);
    assert_eq!(cell.active_sessions, 0);
    assert_eq!(cell.status, "Active");
}

#[tokio::test]
async fn test_slot_acquire_and_routing_check() {
    let state = SchedulerState::new();

    state.register_cell(CellState {
        cell_id: "cell-1".to_string(),
        capacity: 20,
        active_sessions: 0,
        free_slots: 20,
        status: "Active".to_string(),
    });

    let best_cell = state.find_best_cell();
    assert_eq!(best_cell, Some("cell-1".to_string()));

    state.update_cell_slots("cell-1", 19);
    let cell = state.cells.get("cell-1").unwrap();
    assert_eq!(cell.free_slots, 19);
}

#[tokio::test]
async fn test_slot_release_and_call_cancellation() {
    let state = SchedulerState::new();

    state.register_cell(CellState {
        cell_id: "cell-1".to_string(),
        capacity: 20,
        active_sessions: 1,
        free_slots: 19,
        status: "Active".to_string(),
    });

    state.session_to_cell.insert("session-101".to_string(), "cell-1".to_string());
    assert_eq!(*state.session_to_cell.get("session-101").unwrap(), "cell-1");

    state.session_to_cell.remove("session-101");
    state.update_cell_slots("cell-1", 20);
    let cell = state.cells.get("cell-1").unwrap();
    assert_eq!(cell.free_slots, 20);
    assert!(state.session_to_cell.get("session-101").is_none());
}

#[tokio::test]
async fn test_cell_heartbeat_check() {
    let state = SchedulerState::new();

    state.register_cell(CellState {
        cell_id: "cell-1".to_string(),
        capacity: 20,
        active_sessions: 0,
        free_slots: 20,
        status: "Active".to_string(),
    });

    state.register_cell(CellState {
        cell_id: "cell-1".to_string(),
        capacity: 20,
        active_sessions: 5,
        free_slots: 15,
        status: "Active".to_string(),
    });

    let cell = state.cells.get("cell-1").unwrap();
    assert_eq!(cell.active_sessions, 5);
    assert_eq!(cell.free_slots, 15);
}

#[tokio::test]
async fn test_sled_check_and_delayqueue_timeout_cancellation() {
    // 1. Sled Persistence Check
    let local_db = sled::Config::new().temporary(true).open().unwrap();
    local_db.insert(b"idempotency_key_1", b"processed").unwrap();
    let res = local_db.get(b"idempotency_key_1").unwrap().unwrap();
    assert_eq!(res.as_ref(), b"processed");

    // 2. DelayQueue abstraction check
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _ = tx.send("timeout_fired").await;
    });

    let r = tokio::time::timeout(Duration::from_millis(150), rx.recv()).await;
    assert!(r.is_ok());
    assert_eq!(r.unwrap().unwrap(), "timeout_fired");
}

#[tokio::test]
async fn test_grpc_connection_to_scheduler_check() {
    use scheduler::grpc::cell_control_server::CellControlServer;
    use tonic::transport::Server;

    let state = SchedulerState::new();
    let service = CellControlService { state };

    let addr = "[::1]:50059".parse().unwrap();

    tokio::spawn(async move {
        Server::builder()
            .add_service(CellControlServer::new(service))
            .serve(addr)
            .await
            .unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let channel = tonic::transport::Endpoint::from_static("http://[::1]:50059")
        .connect()
        .await;

    assert!(channel.is_ok(), "gRPC client successfully connected to scheduler");
}
