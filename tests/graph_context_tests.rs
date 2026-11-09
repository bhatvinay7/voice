use workflow::ConversationGraph;

#[test]
fn unheard_tool_context_remains_eligible_for_a_follow_up_turn() {
    let mut graph = ConversationGraph::new("call-test");
    let first_user = graph.append_user_text("Show me a Dubai villa").unwrap();
    graph.upsert_tool_context_for_turn(
        "Villa price is AED 6,200,000",
        vec![first_user],
        Some("tool-search-1".to_string()),
        1,
    );
    assert_eq!(graph.pending_llm_node_ids().len(), 1);

    graph.append_user_text("What is the price?").unwrap();
    assert_eq!(graph.pending_llm_node_ids().len(), 1);
}

#[test]
fn short_contextual_turn_is_not_dropped_before_agent_dispatch() {
    let mut graph = ConversationGraph::new("call-test");
    assert!(graph.append_user_text("no").is_some());
}

#[test]
fn graph_payload_is_built_on_demand_for_only_the_requested_query_branch() {
    let mut graph = ConversationGraph::new("call-test");
    let dubai_user = graph.append_user_text("Show Dubai villas").unwrap();
    let dubai_query_id = graph.nodes[&dubai_user].event_id.clone();
    graph.upsert_tool_context_for_turn(
        "Dubai villa: AED 6,200,000",
        vec![dubai_user],
        Some("search-dubai".to_string()),
        1,
    );

    let abu_dhabi_user = graph.append_user_text("Show Abu Dhabi villas").unwrap();
    graph.upsert_tool_context_for_turn(
        "Abu Dhabi villa: AED 4,000,000",
        vec![abu_dhabi_user],
        Some("search-abu-dhabi".to_string()),
        2,
    );

    let context = graph
        .relevant_snapshot_json_for_query(&dubai_query_id)
        .expect("known query should construct context only on request");
    assert!(context.contains("Dubai villa"));
    assert!(!context.contains("Abu Dhabi villa"));
    assert!(graph
        .relevant_snapshot_json_for_query("unknown-query")
        .is_none());
}

#[test]
fn completed_older_tool_result_is_retained_for_audit_without_becoming_latest_context() {
    let mut graph = ConversationGraph::new("call-test");
    let older_user = graph.append_user_text("Show villas in Dubai").unwrap();
    let older_query = graph.nodes[&older_user].event_id.clone();

    // The caller asks again before the first search has completed.
    let latest_user = graph
        .append_user_text("What rentals are available in Abu Dhabi?")
        .unwrap();
    let latest_query = graph.nodes[&latest_user].event_id.clone();

    // The first search completes late. Its result stays connected to its own
    // question so it remains auditable and can be used if relevant later.
    graph.upsert_tool_context_for_turn(
        "Dubai villa result: AED 6,200,000",
        vec![older_user],
        Some("tool-dubai".to_string()),
        1,
    );

    let older_context = graph
        .relevant_snapshot_json_for_query(&older_query)
        .expect("older query retains its completed result");
    assert!(older_context.contains("Dubai villa result"));

    let latest_context = graph
        .relevant_snapshot_json_for_query(&latest_query)
        .expect("latest query remains independently addressable");
    assert!(!latest_context.contains("Dubai villa result"));
}
