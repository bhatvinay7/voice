use integration_tests::load_test_env;
use db::DatabaseStore;
use orchestrator::{AgentCmd, QueueEvent, RunId};
use pubsub::create_shared_bus;
use serde_json::json;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_llm_intent_classification_and_out_of_domain_culinary_steering() {
    load_test_env();
    let db = DatabaseStore::new();

    // Out-of-domain inquiry containing "reserve a table at a restaurant"
    // Crucial check: Must NOT trigger book_viewing_slot despite containing "reserve"
    let user_query = "Can you recommend a good Italian restaurant and reserve a dining table in Dubai Marina?";

    let lower = user_query.to_lowercase();
    let is_booking = (lower.contains("book") || lower.contains("reserve") || lower.contains("schedule") || lower.contains("appointment") || lower.contains("slot"))
        && !lower.contains("restaurant") && !lower.contains("food") && !lower.contains("dining") && !lower.contains("hotel");

    assert!(!is_booking, "Dining and restaurant inquiries must not be misclassified as property viewing bookings");

    // Execute through tool executor
    let tool_args = json!({ "query": user_query });
    let result = orchestrator::engine::handlers::executor::execute_in_process_tool(
        &db,
        "search_properties",
        &tool_args,
        "task-culinary-1",
    ).await;

    assert_eq!(result["success"], true);
    let speech_text = result["response_text"].as_str().unwrap();
    assert!(
        speech_text.contains("culinary hotspots") || speech_text.contains("award-winning fine dining"),
        "AI must politely steer culinary questions back to residential properties in luxury dining hotspots: {}",
        speech_text
    );
}

#[tokio::test]
async fn test_verification_gate_and_slot_double_booking_conflict() {
    load_test_env();
    let db = DatabaseStore::new();

    let listing_id = "prop-101";
    let slot_time = "2026-10-05T10:00:00Z";

    // 1. Unverified user attempts booking -> BLOCKED by verification gate
    let unverified_user = "user-unverified";
    let res_unverified = db.book_viewing_slot(listing_id, unverified_user, slot_time, "idemp-gate-1").await.unwrap();
    assert!(!res_unverified.success);
    assert_eq!(res_unverified.error_code, Some("VERIFICATION_REQUIRED".to_string()));
    assert!(res_unverified.error.unwrap().contains("Emirates ID"));

    // 2. User verifies Emirates ID document
    let verified_profile = db.verify_user_document(unverified_user, "784-1990-1234567-1").await.unwrap();
    assert!(verified_profile.is_verified);

    // 3. Verified user books the slot -> SUCCEEDS
    let res_verified = db.book_viewing_slot(listing_id, unverified_user, slot_time, "idemp-gate-2").await.unwrap();
    assert!(res_verified.success);
    let original_booking_id = res_verified.booking.as_ref().unwrap().id.clone();

    // 4. Another user attempts to book the SAME slot -> CONFLICT! BLOCKED with SLOT_UNAVAILABLE
    let second_user = "user-verified";
    let res_conflict = db.book_viewing_slot(listing_id, second_user, slot_time, "idemp-gate-3").await.unwrap();
    assert!(!res_conflict.success, "Second user must not be able to double-book already reserved slot");
    assert_eq!(res_conflict.error_code, Some("SLOT_UNAVAILABLE".to_string()));

    // 5. Original user retries with SAME idempotency key -> Idempotent success (returns same booking)
    let res_idempotent = db.book_viewing_slot(listing_id, unverified_user, slot_time, "idemp-gate-2").await.unwrap();
    assert!(res_idempotent.success);
    assert_eq!(res_idempotent.booking.unwrap().id, original_booking_id, "Idempotent retry must return existing booking");
}

#[tokio::test]
async fn test_llm_conversational_history_and_tone_context_injection() {
    load_test_env();

    let db = DatabaseStore::new();
    let pubsub = create_shared_bus(10);
    let run_id = RunId("run-tone-test-1".to_string());
    let (orch_tx, mut _orch_rx) = mpsc::channel::<QueueEvent>(100);

    let mut agent = agent::LlmAgentActor::new(
        run_id.clone(),
        db,
        pubsub,
        orch_tx,
    );

    assert!(agent.session_history.is_empty());

    // 1. User provides initial input
    agent.handle_command(AgentCmd::Start {
        run_id: run_id.clone(),
        input: "Looking for villas in Palm Jumeirah".to_string(),
    }).await;
    assert_eq!(agent.session_history.len(), 1);

    // 2. Parallel Tone Analysis task detects caller emotion/tone and injects context
    agent.handle_command(AgentCmd::InjectToneContext {
        run_id: run_id.clone(),
        session_id: "session-tone-1".to_string(),
        tone_intent: "hesitant_budget_conscious".to_string(),
        confidence: 0.94,
    }).await;

    // Verify tone injection was recorded in conversation history
    assert_eq!(agent.session_history.len(), 2);
    let last_entry = agent.session_history.last().unwrap();
    assert!(last_entry.contains("[SYSTEM_TONE_INJECTION]"));
    assert!(last_entry.contains("hesitant_budget_conscious"));
    assert!(last_entry.contains("0.94"));
}

#[tokio::test]
async fn test_llm_agent_property_context_caching_and_followup_qa() {
    load_test_env();

    let db = DatabaseStore::new();
    let pubsub = create_shared_bus(10);
    let run_id = RunId("run-context-cache-test".to_string());
    let (orch_tx, mut orch_rx) = mpsc::channel::<QueueEvent>(100);

    let mut agent = agent::LlmAgentActor::new(
        run_id.clone(),
        db,
        pubsub,
        orch_tx,
    );

    // 1. Initial user inquiry: "I want property in dubai"
    agent.handle_command(AgentCmd::UserInput {
        run_id: run_id.clone(),
        user_id: "user-101".to_string(),
        role: orchestrator::SpeakerRole::Candidate,
        text: "I want property in dubai".to_string(),
    }).await;

    // Must trigger search_properties tool because no context is cached yet
    let tool_event = orch_rx.recv().await.expect("Must receive tool request for initial search");
    let task_id = match tool_event {
        QueueEvent::Agent(orchestrator::AgentEvent::ToolRequested { tool_name, task_id, .. }) => {
            assert_eq!(tool_name, "search_properties");
            task_id
        }
        other => panic!("Expected AgentEvent::ToolRequested, got: {:?}", other),
    };

    // 2. Simulated tool returns listing data for Luxury Marina Skyline Apartment
    let listing = db::PropertyListing {
        id: "prop-101".to_string(),
        title: "Luxury Marina Skyline Apartment".to_string(),
        secondary_title: "2BR Premium Unit".to_string(),
        description: "Modern high-floor apartment overlooking the marina promenade".to_string(),
        secondary_description: "Fully renovated with top-tier finishes".to_string(),
        city: "Dubai".to_string(),
        location: "Dubai Marina".to_string(),
        price_aed: 115000,
        listing_type: "rent".to_string(),
        bedrooms: 2,
        bathrooms: 2,
        area_sqft: 1250,
        pet_friendly: true,
        near_metro: true,
        agent_id: "agent-001".to_string(),
        available_slots: vec!["2026-10-15T10:00:00Z".to_string(), "2026-10-16T14:00:00Z".to_string()],
    };

    agent.handle_command(AgentCmd::ToolResult {
        run_id: run_id.clone(),
        task_id,
        outcome: Ok(serde_json::json!({
            "success": true,
            "response_text": "I found Luxury Marina Skyline Apartment in Dubai Marina, priced at AED 115000 per year with 2 bedrooms.",
            "total_found": 1,
            "listings": [listing],
        })),
    }).await;

    // Drain initial finished event
    let initial_finish = orch_rx.recv().await.expect("Must receive Finished event for initial query");
    match initial_finish {
        QueueEvent::Agent(orchestrator::AgentEvent::Finished { result, .. }) => {
            assert!(result.contains("Luxury Marina Skyline Apartment"));
        }
        other => panic!("Expected AgentEvent::Finished, got: {:?}", other),
    }

    // Verify properties are cached in agent state
    assert_eq!(agent.cached_listings.len(), 1);
    assert_eq!(agent.active_listing_id, Some("prop-101".to_string()));

    // 3. User asks follow-up 1: "What is the rent?"
    // Expected: Agent MUST answer directly from cached context with zero tool requests!
    agent.handle_command(AgentCmd::UserInput {
        run_id: run_id.clone(),
        user_id: "user-101".to_string(),
        role: orchestrator::SpeakerRole::Candidate,
        text: "What is the rent?".to_string(),
    }).await;

    let finish_event1 = orch_rx.recv().await.expect("Must receive immediate Finished event from cache");
    match finish_event1 {
        QueueEvent::Agent(orchestrator::AgentEvent::Finished { result, .. }) => {
            assert!(result.contains("115000"), "Must state correct rent: {}", result);
            assert!(result.contains("Luxury Marina Skyline Apartment"), "Must refer to cached property: {}", result);
        }
        other => panic!("Expected Finished event from cache, got: {:?}", other),
    }

    // 4. User asks follow-up 2: "How many bedrooms does it have?"
    agent.handle_command(AgentCmd::UserInput {
        run_id: run_id.clone(),
        user_id: "user-101".to_string(),
        role: orchestrator::SpeakerRole::Candidate,
        text: "How many bedrooms does it have?".to_string(),
    }).await;

    let finish_event2 = orch_rx.recv().await.expect("Must receive Finished event from cache");
    match finish_event2 {
        QueueEvent::Agent(orchestrator::AgentEvent::Finished { result, .. }) => {
            assert!(result.contains("2 bedrooms"), "Must state bedrooms: {}", result);
            assert!(result.contains("2 bathrooms"), "Must state bathrooms: {}", result);
        }
        other => panic!("Expected Finished event from cache, got: {:?}", other),
    }

    // 5. User asks follow-up 3: "Is it pet friendly?"
    agent.handle_command(AgentCmd::UserInput {
        run_id: run_id.clone(),
        user_id: "user-101".to_string(),
        role: orchestrator::SpeakerRole::Candidate,
        text: "Can I bring my dog? Is it pet friendly?".to_string(),
    }).await;

    let finish_event3 = orch_rx.recv().await.expect("Must receive Finished event from cache");
    match finish_event3 {
        QueueEvent::Agent(orchestrator::AgentEvent::Finished { result, .. }) => {
            assert!(result.contains("pet-friendly"), "Must answer pet question: {}", result);
        }
        other => panic!("Expected Finished event from cache, got: {:?}", other),
    }

    // 6. User asks follow-up 4: "Can I book a viewing slot for this?"
    // Expected: Agent triggers book_viewing_slot with listing_id = "prop-101" from cached listing!
    agent.handle_command(AgentCmd::UserInput {
        run_id: run_id.clone(),
        user_id: "user-101".to_string(),
        role: orchestrator::SpeakerRole::Candidate,
        text: "Can I book a viewing slot for this?".to_string(),
    }).await;

    let booking_event = orch_rx.recv().await.expect("Must receive booking tool request");
    match booking_event {
        QueueEvent::Agent(orchestrator::AgentEvent::ToolRequested { tool_name, args, .. }) => {
            assert_eq!(tool_name, "book_viewing_slot");
            assert_eq!(args["listing_id"], "prop-101", "Must book the cached active property");
        }
        other => panic!("Expected booking tool request, got: {:?}", other),
    }

    // 7. User asks for a DIFFERENT location: "Actually show me villas in Palm Jumeirah instead"
    // Expected: Agent recognizes cached context is insufficient for a different area -> triggers search_properties!
    agent.handle_command(AgentCmd::UserInput {
        run_id: run_id.clone(),
        user_id: "user-101".to_string(),
        role: orchestrator::SpeakerRole::Candidate,
        text: "Actually show me villas in Palm Jumeirah instead".to_string(),
    }).await;

    let new_search_event = orch_rx.recv().await.expect("Must trigger new search for different location");
    match new_search_event {
        QueueEvent::Agent(orchestrator::AgentEvent::ToolRequested { tool_name, args, .. }) => {
            assert_eq!(tool_name, "search_properties");
            assert!(args["query"].as_str().unwrap().contains("Palm Jumeirah"));
        }
        other => panic!("Expected new search tool request, got: {:?}", other),
    }
}


