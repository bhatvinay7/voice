use agent::LlmAgentActor;
use db::DatabaseStore;
use orchestrator::tts_playback::PlaybackGenerationGate;
use orchestrator::{
    durable_scheduler::{DurableTask, DurableTaskScheduler, DurableTaskStatus},
    AgentCmd, AgentEvent, QueueEvent, RunId, SessionStateSnapshot, TaskId,
};
use pubsub::create_shared_bus;
use tokio::sync::{broadcast, mpsc};
use workflow::ConversationGraph;

fn durable_task(task_id: &str, session_id: &str) -> DurableTask {
    DurableTask {
        task_id: TaskId(task_id.to_string()),
        session_id: session_id.to_string(),
        run_id: RunId(format!("run-{session_id}")),
        tool_name: "search_properties".to_string(),
        args: serde_json::json!({"query": "Dubai villa"}),
        query_node_id: Some(format!("user-{session_id}")),
        query_graph_version: 2,
        attempt: 1,
        max_attempts: 3,
        retry_count: 0,
        due_at_ms: 0,
        lease_expires_at_ms: None,
        status: DurableTaskStatus::Pending,
        last_error: None,
    }
}

#[test]
fn playback_generation_rejects_audio_queued_before_fast_preemption() {
    let mut gate = PlaybackGenerationGate::default();
    let agent_generation = gate.current();
    assert!(gate.accepts(agent_generation));

    let fast_generation = gate.preempt();
    assert!(fast_generation > agent_generation);
    assert!(!gate.accepts(agent_generation));
    assert!(gate.accepts(fast_generation));
}

#[tokio::test]
async fn session_state_requests_are_scoped_to_the_owning_actor_mailbox() {
    let (call_a_tx, mut call_a_rx) = mpsc::channel::<QueueEvent>(1);
    let (_call_b_tx, mut call_b_rx) = mpsc::channel::<QueueEvent>(1);
    let (reply_tx, mut reply_rx) = mpsc::channel(1);

    call_a_tx
        .send(QueueEvent::SessionStateRequested {
            session_id: "call-a".to_string(),
            reply_to: reply_tx,
        })
        .await
        .unwrap();

    assert!(
        call_b_rx.try_recv().is_err(),
        "another call must not receive the request"
    );
    let QueueEvent::SessionStateRequested {
        session_id,
        reply_to,
    } = call_a_rx.recv().await.unwrap()
    else {
        panic!("expected session-state mailbox request");
    };
    assert_eq!(session_id, "call-a");
    reply_to
        .send(SessionStateSnapshot {
            session_id,
            graph_version: 7,
            active_task_count: 2,
            running_task_count: 1,
            retrying_task_count: 1,
            pending_transcript: true,
            sts_idle: false,
            speech_generation_id: 9,
        })
        .await
        .unwrap();

    let state = reply_rx.recv().await.unwrap();
    assert_eq!(state.session_id, "call-a");
    assert_eq!(state.graph_version, 7);
    assert_eq!(state.active_task_count, 2);
    assert!(state.pending_transcript);
}

#[test]
fn graph_context_is_query_scoped_and_does_not_leak_between_calls_or_branches() {
    let mut graph = ConversationGraph::new("call-a");
    let dubai = graph.append_user_text("Show me villas in Dubai").unwrap();
    let dubai_event = graph.nodes[&dubai].event_id.clone();
    graph.upsert_tool_context_for_turn(
        "Dubai result: AED 6,200,000",
        vec![dubai],
        Some("tool-dubai".into()),
        1,
    );
    let abu_dhabi = graph
        .append_user_text("Show me villas in Abu Dhabi")
        .unwrap();
    graph.upsert_tool_context_for_turn(
        "Abu Dhabi result: AED 4,000,000",
        vec![abu_dhabi],
        Some("tool-abu-dhabi".into()),
        2,
    );

    let context = graph
        .relevant_snapshot_json_for_query(&dubai_event)
        .unwrap();
    assert!(context.contains("Dubai result"));
    assert!(!context.contains("Abu Dhabi result"));
}

#[test]
fn durable_retry_contract_claims_once_renews_leases_and_stops_after_terminal_state() {
    let db = sled::Config::new().temporary(true).open().unwrap();
    let scheduler = DurableTaskScheduler::open(&db).unwrap();
    let task = durable_task("task-a", "call-a");
    scheduler.schedule(&task).unwrap();

    let claimed = scheduler.claim_due(1, 10, 16).unwrap();
    assert_eq!(claimed.len(), 1);
    assert_eq!(scheduler.claim_due(1, 10, 16).unwrap().len(), 0);
    assert!(scheduler.renew_lease(&task.task_id, 5, 10).unwrap());
    scheduler.recover_expired_claims(11).unwrap();
    assert!(scheduler.claim_due(11, 10, 16).unwrap().is_empty());

    scheduler.recover_expired_claims(15).unwrap();
    assert_eq!(scheduler.claim_due(15, 10, 16).unwrap().len(), 1);
    scheduler
        .terminal(&task.task_id, DurableTaskStatus::Succeeded, None)
        .unwrap();
    scheduler.recover_expired_claims(1_000).unwrap();
    assert!(scheduler.claim_due(1_000, 10, 16).unwrap().is_empty());
}

#[tokio::test]
async fn grounded_tool_result_reaches_the_agent_response_event() {
    let (queue_tx, mut queue_rx) = mpsc::channel::<QueueEvent>(4);
    let run_id = RunId("run-call-agent-contract".to_string());
    let mut agent = LlmAgentActor::new(
        run_id.clone(),
        DatabaseStore::new(),
        create_shared_bus(4),
        queue_tx,
    );
    // Avoid a real Redis append; this test checks orchestration routing only.
    let (fanout, _) = broadcast::channel(4);
    agent.conversation_tx = Some(fanout);

    agent
        .handle_command(AgentCmd::ToolResult {
            run_id: run_id.clone(),
            task_id: TaskId("task-agent-contract".to_string()),
            query_id: Some("query-agent-contract".to_string()),
            outcome: Ok(serde_json::json!({
                "response_text": "The price is AED 6,200,000.",
                "domain_relevant": true,
                "listings": []
            })),
            user_question: None,
            graph_context: None,
        })
        .await;

    match queue_rx.recv().await {
        Some(QueueEvent::Agent(AgentEvent::Finished {
            run_id: received,
            playback_generation,
            result,
            relevance,
            ..
        })) => {
            assert_eq!(received, run_id);
            assert_eq!(playback_generation, 0);
            assert_eq!(result, "The price is AED 6,200,000.");
            assert!(relevance.is_relevant());
        }
        other => panic!("expected grounded agent completion, got {other:?}"),
    }
}

#[test]
fn graph_enrichment_is_query_scoped_revisioned_and_idempotent() {
    let mut graph = ConversationGraph::new("call-enrichment");
    let dubai_node = graph
        .append_user_text("What is the Dubai villa price?")
        .unwrap();
    let dubai_query = graph.nodes[&dubai_node].event_id.clone();
    let base_revision = graph.graph_version;

    // A later caller question advances the graph, but does not let a worker
    // rewrite the later branch. The enrichment stays attached to its source.
    let abu_node = graph.append_user_text("What about Abu Dhabi?").unwrap();
    let enrichment = graph.upsert_graph_enrichment_for_query(
        &dubai_query,
        base_revision,
        "resolved entity: Dubai villa; relation: price",
    );
    assert!(enrichment.is_some());
    let enrichment_id = enrichment.unwrap();
    assert_eq!(graph.nodes[&enrichment_id].parent_ids, vec![dubai_node]);
    assert_ne!(graph.nodes[&enrichment_id].parent_ids, vec![abu_node]);

    // Replaying the same worker result is an upsert, not a duplicate node.
    let before = graph.nodes.len();
    graph.upsert_graph_enrichment_for_query(
        &dubai_query,
        base_revision,
        "resolved entity: Dubai villa; relation: purchase price",
    );
    assert_eq!(graph.nodes.len(), before);
    assert!(graph.nodes[&enrichment_id]
        .content
        .contains("purchase price"));

    // A non-existent branch and a future revision are both rejected.
    assert!(graph
        .upsert_graph_enrichment_for_query("missing", base_revision, "bad")
        .is_none());
    assert!(graph
        .upsert_graph_enrichment_for_query(&dubai_query, graph.graph_version + 1, "bad")
        .is_none());
}

#[test]
fn pending_turns_remain_graph_connected_for_related_tool_permits() {
    let mut graph = ConversationGraph::new("call-pending-permit");
    let first = graph.append_user_text("Show villas in Dubai").unwrap();
    let second = graph
        .append_user_text("What is the price of that one?")
        .unwrap();
    assert_eq!(graph.nodes[&second].parent_ids, vec![first.clone()]);

    graph.upsert_tool_context_for_turn(
        "Dubai villa price: AED 6,200,000",
        vec![first],
        Some("tool-first".to_string()),
        1,
    );
    let second_query = graph.nodes[&second].event_id.clone();
    let related = graph
        .relevant_snapshot_json_for_query(&second_query)
        .expect("second query must retain its pending predecessor branch");
    assert!(related.contains("Dubai villa price"));
    assert!(related.contains("follows_or_refines"));
    assert!(related.contains("verified_result_for"));
}
