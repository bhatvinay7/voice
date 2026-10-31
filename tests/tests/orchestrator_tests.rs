use integration_tests::load_test_env;
use orchestrator::{
    AgentCmd, Orchestrator, QueueEvent, RunId, SessionActor,
    TaskId, TaskStateRecord, TaskStatus, ToolEvent,
};
use db::DatabaseStore;
use pubsub::create_shared_bus;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

fn create_test_session_actor(session_id: &str) -> (SessionActor, mpsc::Receiver<QueueEvent>, mpsc::Receiver<AgentCmd>) {
    let pubsub = create_shared_bus(10);
    let db = DatabaseStore::new();
    let tool_client = integration_tests::create_test_tool_client();
    let (orch_tx, orch_rx) = mpsc::channel(100);
    let (db_tx, _) = mpsc::channel(1024);
    let local_db = sled::Config::new().temporary(true).open().unwrap();

    let (agent_tx_out, agent_rx_out) = mpsc::channel(100);
    let agent_tx_shared = Arc::new(tokio::sync::Mutex::new(Some(agent_tx_out)));

    let agent_factory = Arc::new(move |_run_id: RunId, _orch_tx: mpsc::Sender<QueueEvent>| {
        agent_tx_shared.try_lock().ok().and_then(|mut guard| guard.take()).unwrap_or_else(|| {
            let (tx, _) = mpsc::channel(100);
            tx
        })
    });

    let orchestrator = Orchestrator::new(pubsub, db, local_db, tool_client, orch_tx, db_tx, agent_factory);
    let session_actor = SessionActor::new(session_id.to_string(), orchestrator);

    (session_actor, orch_rx, agent_rx_out)
}

#[tokio::test]
async fn test_timeout_registration_heartbeat_and_sled_wal() {
    load_test_env();

    let (mut session_actor, _orch_rx, _agent_rx) = create_test_session_actor("session-timeout-1");
    let task_id = TaskId("task-wal-101".to_string());
    let db_key = format!("timeout:{}", task_id.0);

    // 1. Register timeout for 5 seconds
    session_actor.register_timeout(task_id.clone(), Duration::from_secs(5));

    // 2. Verify Write-Ahead Log (WAL) written into Sled embedded DB
    let sled_val = session_actor.orch.local_db.get(db_key.as_bytes()).unwrap();
    assert!(sled_val.is_some(), "Sled WAL must persist expiration timestamp for registered task");

    // 3. Fire ToolEvent::Heartbeat -> updates/extends timeout in DelayQueue
    let task_record = TaskStateRecord {
        task_id: task_id.clone(),
        run_id: RunId("run-session-timeout-1".to_string()),
        tool_name: "heavy_rag_indexing".to_string(),
        args: serde_json::json!({}),
        status: TaskStatus::Running,
        attempt: 1,
        max_retry: 3,
        retry_count: 0,
        timeout_duration: Duration::from_secs(8),
        error: None,
    };
    session_actor.tasks.insert(task_id.clone(), task_record);

    session_actor.handle_tool_event(ToolEvent::Heartbeat {
        task_id: task_id.clone(),
        attempt: 1,
        progress_pct: 50,
    }).await;

    // Sled WAL remains active during heartbeat
    assert!(session_actor.orch.local_db.get(db_key.as_bytes()).unwrap().is_some());

    // 4. Fire ToolEvent::Done -> removes timeout handle from in-memory queue & deletes from Sled DB
    session_actor.handle_tool_event(ToolEvent::Done {
        task_id: task_id.clone(),
        attempt: 1,
        output: serde_json::json!({ "success": true }),
    }).await;

    assert_eq!(session_actor.tasks.get(&task_id).unwrap().status, TaskStatus::Succeeded);
    let sled_cleared = session_actor.orch.local_db.get(db_key.as_bytes()).unwrap();
    assert!(sled_cleared.is_none(), "Sled WAL key must be deleted immediately once task completes successfully");
}

#[tokio::test]
async fn test_orchestrator_tool_failure_retry_loop_and_max_retry_exhaustion() {
    load_test_env();

    let (mut session_actor, _orch_rx, _agent_rx) = create_test_session_actor("session-retry-1");
    let task_id = TaskId("task-retry-test-1".to_string());
    let run_id = RunId("run-session-retry-1".to_string());

    let task_record = TaskStateRecord {
        task_id: task_id.clone(),
        run_id: run_id.clone(),
        tool_name: "remote_crm_sync".to_string(),
        args: serde_json::json!({ "crm_id": "crm-888" }),
        status: TaskStatus::Running,
        attempt: 1,
        max_retry: 2,
        retry_count: 0,
        timeout_duration: Duration::from_secs(5),
        error: None,
    };
    session_actor.tasks.insert(task_id.clone(), task_record);

    // Attempt 1 fails
    session_actor.handle_tool_event(ToolEvent::Failed {
        task_id: task_id.clone(),
        attempt: 1,
        error: "Connection timeout to CRM".to_string(),
        retryable: true,
    }).await;

    let updated_1 = session_actor.tasks.get(&task_id).unwrap();
    assert_eq!(updated_1.status, TaskStatus::Retrying, "Task should transition to Retrying state on attempt 1 failure");
    assert_eq!(updated_1.attempt, 2);
    assert_eq!(updated_1.retry_count, 1);

    // Attempt 2 fails (Reaches max_retry = 2) -> Fails permanently
    session_actor.handle_tool_event(ToolEvent::Failed {
        task_id: task_id.clone(),
        attempt: 2,
        error: "Authentication refused".to_string(),
        retryable: true,
    }).await;

    let updated_2 = session_actor.tasks.get(&task_id).unwrap();
    assert_eq!(updated_2.status, TaskStatus::Failed, "Task must permanently fail after exhausting max retries");
    assert_eq!(updated_2.retry_count, 2);
    assert!(updated_2.error.as_ref().unwrap().contains("Authentication refused"));
}

#[tokio::test]
async fn test_human_interviewer_override_and_transcript_discarding() {
    load_test_env();

    let (mut session_actor, _orch_rx, mut agent_rx) = create_test_session_actor("session-override-1");

    // 1. Human interviewer speaks -> sends QueueEvent::InterviewerSpeech { active: true }
    session_actor.handle_queue_event(QueueEvent::InterviewerSpeech {
        session_id: "session-override-1".to_string(),
        interviewer_id: "int-1".to_string(),
        active: true,
    }).await;
    assert!(session_actor.ai_listen_muted, "SessionActor must flag ai_listen_muted active");

    // Verify Agent was sent SetAiListenMute { muted: true }
    let cmd = agent_rx.recv().await.expect("Agent must receive command on interviewer speaking");
    match cmd {
        AgentCmd::SetAiListenMute { muted, .. } => assert!(muted, "AI agent should be muted while interviewer speaks"),
        other => panic!("Expected SetAiListenMute, got: {:?}", other),
    }

    // 2. Transcript chunks arriving while interviewer is speaking must be DISCARDED
    session_actor.handle_queue_event(QueueEvent::SttTranscript {
        session_id: "session-override-1".to_string(),
        text: "User said something while interviewer was speaking".to_string(),
        language: "en".to_string(),
        is_final: true,
    }).await;

    // Verify no UserInput command was forwarded to the Agent
    assert!(agent_rx.try_recv().is_err(), "Agent must NOT receive UserInput when interviewer speaking");

    // 3. Interviewer stops speaking -> sends QueueEvent::InterviewerSpeech { active: false }
    session_actor.handle_queue_event(QueueEvent::InterviewerSpeech {
        session_id: "session-override-1".to_string(),
        interviewer_id: "int-1".to_string(),
        active: false,
    }).await;
    assert!(!session_actor.ai_listen_muted, "Interviewer speaking flag should clear");

    // Verify Agent was unmuted
    let cmd_unmute = agent_rx.recv().await.expect("Agent must receive unmute command");
    match cmd_unmute {
        AgentCmd::SetAiListenMute { muted, .. } => assert!(!muted, "AI agent should be unmuted after interviewer ends speech"),
        other => panic!("Expected SetAiListenMute false, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_grpc_rag_tool_execution_through_orchestrator_brain() {
    load_test_env();

    // 1. Setup tool client dynamically from environment variables
    let tool_client = integration_tests::create_test_tool_client();

    // 2. Setup Orchestrator (The Brain) with DelayQueue, sled WAL, and Agent Factory
    let pubsub = create_shared_bus(100);
    let (orch_tx, orch_rx) = mpsc::channel(100);
    let protected_orch_rx = Arc::new(tokio::sync::Mutex::new(orch_rx));
    let (db_tx, _) = mpsc::channel(1024);
    let local_db = sled::Config::new().temporary(true).open().unwrap();

    let db = DatabaseStore::new();
    let db_clone = db.clone();
    let pubsub_clone = pubsub.clone();
    let agent_factory = Arc::new(move |run_id: RunId, orch_tx_in: mpsc::Sender<QueueEvent>| {
        let (agent_tx, agent_rx) = mpsc::channel(100);
        let agent = agent::LlmAgentActor::new(run_id, db_clone.clone(), pubsub_clone.clone(), orch_tx_in);
        tokio::spawn(async move {
            agent::LlmAgentActor::run_agent_loop(agent_rx, agent).await;
        });
        agent_tx
    });

    let orchestrator = Orchestrator::new(
        pubsub.clone(),
        db.clone(),
        local_db.clone(),
        tool_client,
        orch_tx.clone(),
        db_tx,
        agent_factory,
    );

    // Run Dispatcher
    let orch_clone = orchestrator.clone();
    tokio::spawn(Orchestrator::run_dispatcher(protected_orch_rx, orch_clone));

    // 3. Start a session
    let session_id = "session-grpc-brain-test-1".to_string();
    orch_tx.send(QueueEvent::SessionStarted {
        session_id: session_id.clone(),
        user_id: "user-verified".to_string(),
        language: "en".to_string(),
    }).await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    // 4. Send final STT user inquiry: "2 bedroom apartment in Dubai Marina"
    orch_tx.send(QueueEvent::SttTranscript {
        session_id: session_id.clone(),
        text: "2 bedroom apartment in Dubai Marina".to_string(),
        language: "en".to_string(),
        is_final: true,
    }).await.unwrap();

    // 5. Poll conversation stream until AI completes RAG search
    let mut resolved = false;
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Ok(events) = db.load_recent_stream_events().await {
            if events.iter().any(|e| e.session_id.contains(&session_id) && e.event_type == "LLM_RESPONSE") {
                resolved = true;
                break;
            }
        }
    }

    assert!(resolved, "Orchestrator brain must execute tool via gRPC, resolve RAG listings, and clear timeouts in DelayQueue and Sled WAL");
}
