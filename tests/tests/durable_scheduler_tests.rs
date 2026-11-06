use orchestrator::{
    durable_scheduler::{DurableTask, DurableTaskScheduler, DurableTaskStatus},
    RunId, TaskId,
};

fn task(status: DurableTaskStatus) -> DurableTask {
    DurableTask {
        task_id: TaskId("task-durable-1".to_string()),
        session_id: "call-durable-1".to_string(),
        run_id: RunId("run-call-durable-1".to_string()),
        tool_name: "search_properties".to_string(),
        args: serde_json::json!({"query": "Dubai villa"}),
        query_node_id: Some("user-1".to_string()),
        query_graph_version: 7,
        attempt: 1,
        max_attempts: 3,
        retry_count: 0,
        due_at_ms: 0,
        lease_expires_at_ms: None,
        status,
        last_error: None,
    }
}

fn task_for(task_id: &str, session_id: &str) -> DurableTask {
    let mut value = task(DurableTaskStatus::Pending);
    value.task_id = TaskId(task_id.to_string());
    value.session_id = session_id.to_string();
    value.run_id = RunId(format!("run-{session_id}"));
    value
}

#[test]
fn durable_scheduler_claims_a_due_task_once_and_recovers_an_expired_lease() {
    let db = sled::Config::new().temporary(true).open().unwrap();
    let scheduler = DurableTaskScheduler::open(&db).unwrap();
    scheduler
        .schedule(&task(DurableTaskStatus::Pending))
        .unwrap();

    let first = scheduler.claim_due(1, 10, 10).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(scheduler.claim_due(1, 10, 10).unwrap().len(), 0);

    assert!(scheduler
        .renew_lease(&TaskId("task-durable-1".to_string()), 5, 10)
        .unwrap());
    scheduler.recover_expired_claims(11).unwrap();
    assert_eq!(scheduler.claim_due(11, 10, 10).unwrap().len(), 0);

    scheduler.recover_expired_claims(15).unwrap();
    let recovered = scheduler.claim_due(15, 10, 10).unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].task_id.0, "task-durable-1");
}

#[test]
fn durable_scheduler_claims_multiple_sessions_without_cross_call_state() {
    let db = sled::Config::new().temporary(true).open().unwrap();
    let scheduler = DurableTaskScheduler::open(&db).unwrap();
    scheduler
        .schedule(&task_for("task-call-a", "call-a"))
        .unwrap();
    scheduler
        .schedule(&task_for("task-call-b", "call-b"))
        .unwrap();

    let claimed = scheduler.claim_due(1, 10, 10).unwrap();
    let sessions = claimed
        .iter()
        .map(|task| task.session_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        sessions,
        std::collections::BTreeSet::from(["call-a", "call-b"])
    );
    assert!(claimed
        .iter()
        .all(|task| task.status == DurableTaskStatus::Claimed));
}
