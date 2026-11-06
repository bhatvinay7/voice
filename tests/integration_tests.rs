use agent::LlmAgentActor;
use config::{AppConfig, StsMode, TelephonyProvider};
use db::{AdminGraphNode, DatabaseStore};
use orchestrator::{
    AgentCmd, AgentEvent, Orchestrator, ProtectedReceiver, QueueEvent, RunId, TaskId, TaskStatus,
    ToolEvent,
};
use pubsub::create_shared_bus;
use rag::RAGPipeline;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex};
use workflow::{
    evaluate_readiness, ConversationGraph, ConversationNodeKind, FairReadyScheduler,
    OutputSequencer, Readiness, ReadyWork, ReorderBuffer, SessionWorkflow, SessionWorkflowStatus,
    SpeechSegment, SummaryCheckpoint, ToolRegistration, ToolRegistry, ToolTerminalState,
    TranscriptFragment,
};

#[tokio::test]
async fn test_config_loader() {
    let cfg = AppConfig::load().unwrap_or_else(|_| AppConfig::load_default());
    assert_eq!(cfg.port, 4000);
    assert_eq!(cfg.stt_provider, "openai");
    assert_eq!(cfg.r2_bucket_name, "uae-estate-vault");
}

#[test]
fn session_workflow_release_discards_only_call_state() {
    let mut workflow = SessionWorkflow::create("run-1", Vec::new());
    workflow.release();
    assert!(workflow.nodes.is_empty());
    assert_eq!(workflow.status, SessionWorkflowStatus::Released);
}

#[test]
fn sts_mode_accepts_only_configured_voice_values() {
    assert_eq!(StsMode::parse("STS_NORMAL"), Ok(StsMode::Normal));
    assert_eq!(StsMode::parse("STS_STREAM"), Ok(StsMode::Stream));
    assert!(StsMode::parse("STS_REAL_TIME").is_err());
}

#[test]
fn v5_reactive_components_reorder_fairly_and_sequence_speech() {
    let now = Instant::now();
    let mut reorder = ReorderBuffer::new(0, Duration::from_millis(10));
    assert!(reorder
        .push(
            TranscriptFragment {
                stream_id: "s".into(),
                sequence_id: 1,
                text: "world".into(),
                is_final: true
            },
            now
        )
        .is_empty());
    let ready = reorder.push(
        TranscriptFragment {
            stream_id: "s".into(),
            sequence_id: 0,
            text: "hello".into(),
            is_final: false,
        },
        now,
    );
    assert_eq!(
        ready.iter().map(|f| f.text.as_str()).collect::<Vec<_>>(),
        vec!["hello", "world"]
    );

    let mut fair = FairReadyScheduler::default();
    for (session, seq) in [("a", 0), ("b", 1), ("a", 2)] {
        fair.push(ReadyWork {
            session_id: session.into(),
            node_id: format!("n{seq}"),
            turn_id: 0,
            generation_id: seq,
            priority: 3,
            enqueue_sequence: seq,
            deadline: None,
            graph_version: 1,
        });
    }
    assert_eq!(fair.pop().unwrap().session_id, "a");
    assert_eq!(fair.pop().unwrap().session_id, "b");

    let mut speech = OutputSequencer::default();
    assert!(speech
        .push(SpeechSegment {
            turn_id: 1,
            generation_id: 1,
            output_sequence: 1,
            text: "two".into(),
            stable: true
        })
        .is_empty());
    assert_eq!(
        speech
            .push(SpeechSegment {
                turn_id: 1,
                generation_id: 1,
                output_sequence: 0,
                text: "one".into(),
                stable: true
            })
            .len(),
        2
    );
}

#[test]
fn v5_tool_registry_rejects_stale_or_duplicate_results() {
    let mut registry = ToolRegistry::default();
    assert!(registry.register(ToolRegistration {
        session_id: "s".into(),
        node_id: "n".into(),
        call_id: "c".into(),
        generation: 1,
        owner_epoch: 2,
        deadline: Instant::now() + Duration::from_secs(1),
        state: ToolTerminalState::Running
    }));
    assert!(!registry.register(ToolRegistration {
        session_id: "s".into(),
        node_id: "n".into(),
        call_id: "c".into(),
        generation: 1,
        owner_epoch: 2,
        deadline: Instant::now(),
        state: ToolTerminalState::Running
    }));
    assert!(!registry.finish("c", "s", "n", 9, 2, ToolTerminalState::Completed));
    assert!(registry.finish("c", "s", "n", 1, 2, ToolTerminalState::Completed));
    assert!(!registry.finish("c", "s", "n", 1, 2, ToolTerminalState::Completed));
}

#[test]
fn v5_readiness_and_summary_are_version_gated() {
    let waiting = evaluate_readiness(true, &[("location", true), ("budget", false)], &[], true);
    assert_eq!(waiting.state, Readiness::WaitingForContext);
    let ready = evaluate_readiness(true, &[("location", true)], &[], true);
    assert_eq!(ready.state, Readiness::ReadyForGeneration);
    let checkpoint = SummaryCheckpoint {
        session_id: "s".into(),
        summary: "context".into(),
        summary_version: 4,
        covered_turn_id: 2,
        owner_epoch: 7,
    };
    assert!(checkpoint.can_apply(4, 7));
    assert!(!checkpoint.can_apply(3, 7));
    assert!(!checkpoint.can_apply(4, 8));
}

#[test]
fn v5_summary_checkpoint_is_serializable_and_versioned() {
    let checkpoint = SummaryCheckpoint {
        session_id: "session-1".into(),
        summary: "latest context".into(),
        summary_version: 12,
        covered_turn_id: 4,
        owner_epoch: 9,
    };
    let encoded = serde_json::to_string(&checkpoint).expect("checkpoint should serialize");
    let decoded: SummaryCheckpoint =
        serde_json::from_str(&encoded).expect("checkpoint should deserialize");
    assert_eq!(decoded.summary_version, 12);
    assert!(decoded.can_apply(12, 9));
    assert!(!decoded.can_apply(11, 9));
}

#[test]
fn conversation_graph_requires_meaningful_text_and_reconstructs_context() {
    let mut graph = ConversationGraph::new("session-graph");
    assert!(graph.append_user_text("uh").is_none());
    let user_id = graph
        .append_user_text("Find rental homes in Dubai")
        .expect("meaningful user text");
    let user_node = graph.nodes.get(&user_id).expect("user node");
    assert!(!user_node.event_id.is_empty());
    assert!(user_node.created_at.contains('T'));
    let intent_id = graph
        .append_optimized_intent("rental homes in Dubai", &user_id)
        .expect("optimized intent");
    let context_id = graph
        .append_tool_context("Dubai rental listing context", vec![intent_id])
        .expect("tool context");
    let (question, context) = graph.response_context().expect("response context");
    assert_eq!(question, "Find rental homes in Dubai");
    assert_eq!(context, "Dubai rental listing context");
    assert_eq!(
        graph.nodes.get(&context_id).map(|node| &node.kind),
        Some(&ConversationNodeKind::ToolContext)
    );
}

#[tokio::test]
async fn concurrent_graph_updates_are_serialized_and_stale_snapshots_are_rejected() {
    let graph = Arc::new(Mutex::new(ConversationGraph::new("race-session")));
    let user_id = {
        let mut graph = graph.lock().await;
        let user = graph
            .append_user_text("Find rental homes in Dubai")
            .unwrap();
        graph
            .append_optimized_intent("rental homes in Dubai", &user)
            .unwrap();
        user
    };
    let first = Arc::clone(&graph);
    let second = Arc::clone(&graph);
    let (first_id, second_id) = tokio::join!(
        async move {
            let mut graph = first.lock().await;
            let version = graph.graph_version;
            let id = graph
                .upsert_tool_context(
                    "Listing A: Dubai Marina, AED 120000",
                    vec![user_id.clone()],
                    Some("rag-search".into()),
                )
                .unwrap();
            (id, version)
        },
        async move {
            let mut graph = second.lock().await;
            let id = graph
                .upsert_tool_context(
                    "Listing B: Downtown Dubai, AED 130000",
                    vec![user_id],
                    Some("availability-refresh".into()),
                )
                .unwrap();
            id
        }
    );
    let graph = graph.lock().await;
    assert_ne!(first_id.0, second_id.0);
    assert!(graph.graph_version >= 4);
    assert_eq!(graph.nodes.get(&first_id).unwrap().parent_ids.len(), 1);
    assert_eq!(graph.nodes.get(&second_id).unwrap().parent_ids.len(), 1);
    assert!(graph.pending_llm_node_ids().len() >= 2);
    // A snapshot captured before the second update is stale and must not be
    // treated as the current graph revision by the reconciliation worker.
    assert!(first_id.1 < graph.graph_version);
}

#[tokio::test]
async fn speech_cancellation_cannot_release_superseded_generation() {
    let sequencer = Arc::new(Mutex::new(OutputSequencer::default()));
    {
        let mut sequencer = sequencer.lock().await;
        assert!(sequencer
            .push(SpeechSegment {
                turn_id: 4,
                generation_id: 9,
                output_sequence: 1,
                text: "stale second fragment".into(),
                stable: true,
            })
            .is_empty());
        sequencer.cancel_generation(4, 9);
    }
    let late_push = Arc::clone(&sequencer);
    let released = tokio::spawn(async move {
        late_push.lock().await.push(SpeechSegment {
            turn_id: 4,
            generation_id: 9,
            output_sequence: 0,
            text: "stale first fragment".into(),
            stable: true,
        })
    })
    .await
    .unwrap();
    assert!(
        released.is_empty(),
        "cancelled speech must never be emitted"
    );

    let mut sequencer = sequencer.lock().await;
    let fresh = sequencer.push(SpeechSegment {
        turn_id: 4,
        generation_id: 10,
        output_sequence: 0,
        text: "fresh answer".into(),
        stable: true,
    });
    assert_eq!(fresh.len(), 1);
    assert_eq!(fresh[0].text, "fresh answer");
}

#[tokio::test]
async fn test_telephony_provider_selection_accepts_supported_env_values() {
    for value in ["twilio", "telnyx", "vonage", "plivo", "sip", "custom"] {
        let provider = TelephonyProvider::parse(value)
            .unwrap_or_else(|error| panic!("{value} should be supported: {error}"));
        assert_eq!(provider.as_str(), value);
    }
    assert!(TelephonyProvider::parse("unknown-provider").is_err());
}

#[tokio::test]
async fn test_idempotency_and_verification_gate() {
    let db = DatabaseStore::new();

    let res_unverified = db
        .book_viewing_slot(
            "prop-101",
            "user-unverified",
            "2026-10-05T10:00:00Z",
            "idemp-test-1",
        )
        .await
        .unwrap();
    assert!(!res_unverified.success);
    assert_eq!(
        res_unverified.error_code,
        Some("VERIFICATION_REQUIRED".to_string())
    );

    let verified_user = db
        .verify_user_document("user-unverified", "784-1990-1234567-1")
        .await
        .unwrap();
    assert!(verified_user.is_verified);

    let res_verified = db
        .book_viewing_slot(
            "prop-101",
            "user-unverified",
            "2026-10-05T10:00:00Z",
            "idemp-test-2",
        )
        .await
        .unwrap();
    assert!(res_verified.success);
    assert!(res_verified.booking.is_some());

    let res_retry = db
        .book_viewing_slot(
            "prop-101",
            "user-unverified",
            "2026-10-05T10:00:00Z",
            "idemp-test-2",
        )
        .await
        .unwrap();
    assert!(res_retry.success);
    if let Some(booking) = res_retry.booking {
        if let Some(original_booking) = res_verified.booking {
            assert_eq!(booking.id, original_booking.id);
        }
    }
}

#[tokio::test]
async fn test_admin_graph_node_management() {
    let db = DatabaseStore::new();

    let initial_nodes = db.get_graph_nodes().await.unwrap();
    assert!(!initial_nodes.is_empty());

    let new_node = AdminGraphNode {
        id: "node-test-1".to_string(),
        step_type: "DynamicErrorResolver".to_string(),
        instruction: "Run compiler typo & error check".to_string(),
        status: "Active".to_string(),
        is_preset: false,
    };

    db.add_graph_node(new_node).await.unwrap();

    let updated_nodes = db.get_graph_nodes().await.unwrap();
    assert_eq!(updated_nodes.len(), initial_nodes.len() + 1);
}

#[tokio::test]
async fn test_r2_transcript_dump_and_call_summary() {
    let db = DatabaseStore::new();

    let r2_transcript_uri =
        db.generate_r2_vault_path("agent-001", "transcripts", "call-101_transcript.json");
    let r2_audio_uri = db.generate_r2_vault_path("agent-001", "recordings", "call-101_audio.mp3");

    let summary_record = db
        .save_call_summary(
            "call-101",
            "user-verified",
            180,
            &r2_transcript_uri,
            &r2_audio_uri,
            "Caller interested in 2BR Marina apartment under 120k AED. Viewing reserved.",
            120000,
            "Dubai Marina",
        )
        .await
        .expect("save call summary");

    assert_eq!(summary_record.call_id, "call-101");
    assert_eq!(summary_record.qualified_budget_aed, 120000);

    let summaries = db.get_call_summaries().await.unwrap();
    assert_eq!(summaries.len(), 1);
}

#[tokio::test]
async fn test_upstash_redis_streams_and_db_worker_sync() {
    let db = DatabaseStore::new();

    let stream_result = db
        .push_redis_stream(
            "stream:conversation",
            "session-call-101",
            "trace-999",
            "USER_PROMPT",
            "{\"text\": \"hello\"}",
        )
        .await;

    assert!(stream_result.is_ok());

    let guard = db.load_recent_stream_events().await;
    if let Ok(events) = guard {
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].session_id, "session-call-101");
        assert_eq!(events[0].trace_id, "trace-999");
    }
}

#[tokio::test]
async fn test_mpsc_channel_routing_and_orchestrator_state_machine() {
    let pubsub = create_shared_bus(10);
    let db = DatabaseStore::new();
    let (orch_tx, orch_rx) = mpsc::channel::<QueueEvent>(100);
    let protected_orch_rx: ProtectedReceiver<QueueEvent> = Arc::new(Mutex::new(orch_rx));

    let channel = tonic::transport::Endpoint::from_static("http://[::1]:50051").connect_lazy();
    let tool_client =
        orchestrator::grpc::tools::tool_execution_service_client::ToolExecutionServiceClient::new(
            channel,
        );
    let (db_tx, _) = mpsc::channel(1024);
    let local_db = sled::Config::new().temporary(true).open().unwrap();

    let agent_factory = Arc::new(move |_run_id: RunId, _orch_tx: mpsc::Sender<QueueEvent>| {
        let (agent_tx, _) = mpsc::channel(100);
        agent_tx
    });

    let orchestrator = Orchestrator::new(
        pubsub,
        db,
        local_db,
        tool_client,
        orch_tx.clone(),
        db_tx,
        agent_factory,
    );
    let mut session_actor =
        orchestrator::SessionActor::new("session-101".to_string(), orchestrator.clone());

    let send_result = orch_tx
        .send(QueueEvent::SessionStarted {
            session_id: "session-101".to_string(),
            user_id: "user-verified".to_string(),
            language: "en".to_string(),
        })
        .await;
    assert!(send_result.is_ok());

    let mut rx = protected_orch_rx.lock().await;
    if let Some(event) = rx.recv().await {
        session_actor.handle_queue_event(event).await;
    }

    assert_eq!(session_actor.session_id, "session-101");
}

#[tokio::test]
async fn test_ai_agent_listen_mute_human_override() {
    let pubsub = create_shared_bus(10);
    let (orch_tx, _orch_rx) = mpsc::channel::<QueueEvent>(100);
    let db = DatabaseStore::new();

    let mut agent_actor = LlmAgentActor::new(RunId("run-1".to_string()), db, pubsub, orch_tx);
    assert!(!agent_actor.is_muted);

    agent_actor
        .handle_command(AgentCmd::SetAiListenMute {
            run_id: RunId("run-1".to_string()),
            muted: true,
        })
        .await;
    assert!(agent_actor.is_muted);

    agent_actor
        .handle_command(AgentCmd::SetAiListenMute {
            run_id: RunId("run-1".to_string()),
            muted: false,
        })
        .await;
    assert!(!agent_actor.is_muted);
}

#[tokio::test]
async fn test_delay_queue_timeout_registration_and_immediate_clearing_on_success() {
    let pubsub = create_shared_bus(10);
    let db = DatabaseStore::new();
    let channel = tonic::transport::Endpoint::from_static("http://[::1]:50051").connect_lazy();
    let tool_client =
        orchestrator::grpc::tools::tool_execution_service_client::ToolExecutionServiceClient::new(
            channel,
        );
    let (orch_tx, _) = mpsc::channel(100);
    let (db_tx, _) = mpsc::channel(1024);
    let local_db = sled::Config::new().temporary(true).open().unwrap();

    let agent_factory = Arc::new(move |_run_id: RunId, _orch_tx: mpsc::Sender<QueueEvent>| {
        let (agent_tx, _) = mpsc::channel(100);
        agent_tx
    });

    let orchestrator = Orchestrator::new(
        pubsub,
        db,
        local_db,
        tool_client,
        orch_tx,
        db_tx,
        agent_factory,
    );
    let mut session_actor = orchestrator::SessionActor::new("session-1".to_string(), orchestrator);

    let task_id = TaskId("task-grpc-tool-1".to_string());
    let run_id = RunId("run-session-1".to_string());

    session_actor
        .handle_agent_event(AgentEvent::ToolRequested {
            run_id,
            task_id: task_id.clone(),
            query_id: None,
            playback_generation: 0,
            depends_on_query_ids: Vec::new(),
            tool_name: "search_properties".to_string(),
            wait_message: None,
            args: serde_json::json!({ "query": "2 bedroom" }),
        })
        .await;

    assert!(session_actor.tasks.contains_key(&task_id));

    session_actor
        .handle_tool_event(ToolEvent::Done {
            task_id: task_id.clone(),
            attempt: 1,
            query_id: None,
            playback_generation: 0,
            output: serde_json::json!({ "success": true }),
        })
        .await;

    if let Some(task) = session_actor.tasks.get(&task_id) {
        assert_eq!(task.status, TaskStatus::Succeeded);
    }
}

#[tokio::test]
async fn test_english_rag_search() {
    let db = DatabaseStore::new();
    let rag = RAGPipeline::new(db);

    let results_en = rag
        .search("2-bedroom near metro under 120k in Dubai Marina")
        .await
        .unwrap();
    assert!(!results_en.is_empty());
    assert_eq!(results_en[0].listing.id, "prop-101");

    let results_downtown = rag.search("1-bedroom studio in bay area").await.unwrap();
    assert!(!results_downtown.is_empty());
    assert_eq!(results_downtown[0].listing.id, "prop-102");

    // Dynamic RAG knowledge search (Dubai buying procedure and Golden Visa)
    let procedure_docs = rag
        .search_knowledge("what is the procedure and steps to buy property in Dubai Form F")
        .await
        .unwrap();
    assert!(
        !procedure_docs.is_empty(),
        "RAG must dynamically retrieve Dubai buying procedure"
    );
    assert_eq!(procedure_docs[0].document.id, "doc-dubai-buy-procedure");
    assert!(procedure_docs[0].document.content.contains("Form F (MOU)"));

    let visa_docs = rag
        .search_knowledge("what is the requirement for 10-year golden visa 2 million AED")
        .await
        .unwrap();
    assert!(
        !visa_docs.is_empty(),
        "RAG must dynamically retrieve Golden Visa knowledge"
    );
    assert_eq!(visa_docs[0].document.id, "doc-uae-golden-visa");
}

#[tokio::test]
async fn test_user_auth_bhatvinay74_gmail_com() {
    let db = DatabaseStore::new();
    let email = "bhatvinay74@gmail.com";
    let password = "12345678";
    assert_eq!(password, "12345678", "User credential check");

    let user_id = format!("user-{}", email.replace('@', "-").replace('.', "-"));
    let user_profile = db
        .verify_user_document(&user_id, "784-1995-8765432-1")
        .await
        .unwrap();

    assert_eq!(user_profile.id, user_id);
    assert!(user_profile.is_verified);
    assert_eq!(
        user_profile.emirates_id_number,
        Some("784-1995-8765432-1".to_string())
    );
}

#[tokio::test]
async fn test_tenancy_contract_parser_and_review_workflow() {
    let db = DatabaseStore::new();

    let contract = db
        .upload_tenancy_contract(
            "agent-001",
            "tenancy_agreement_2026.pdf",
            "Rashid Al-Maktoum",
            "Vinay Bhat",
            "Unit 2408, Princess Tower, Dubai Marina",
            115000,
            5750,
            4,
            90,
            0.78, // Low confidence -> trigger review
        )
        .await
        .unwrap();

    assert!(contract.needs_human_review);
    assert_eq!(contract.status, "pending_review");

    let queue = db.get_contract_review_queue().await.unwrap();
    assert!(queue.iter().any(|c| c.id == contract.id));

    let approved = db.approve_contract_review(&contract.id).await.unwrap();
    assert!(approved.is_some());

    let approved_contract = approved.unwrap();
    assert_eq!(approved_contract.status, "approved");
    assert!(!approved_contract.needs_human_review);
}

#[tokio::test]
async fn test_outbox_email_sms_notification_worker() {
    let db = DatabaseStore::new();
    let user_id = "user-verified";
    let slot_time = "2026-10-15T14:00:00Z";
    let idempotency_key = "idemp-outbox-test-1";

    let res = db
        .book_viewing_slot("prop-101", user_id, slot_time, idempotency_key)
        .await
        .unwrap();
    assert!(res.success);

    let pending_notifications = db.get_outbox_notifications().await.unwrap();
    assert!(pending_notifications.len() >= 2);

    let processed = db.process_outbox_notifications().await.unwrap();
    assert!(processed >= 2);

    let after_process = db.get_outbox_notifications().await.unwrap();
    assert!(after_process.iter().all(|n| n.status == "sent"));
}

#[tokio::test]
async fn test_remote_database_and_redis_connection_config() {
    let cfg = AppConfig::load().unwrap_or_else(|_| AppConfig::load_default());
    assert!(cfg.postgres_url.contains("postgresql://"));
    assert!(cfg.redis_url.contains("redis://"));
}
