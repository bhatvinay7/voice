use integration_tests::load_test_env;
use agent::ModelChecker;
use config::AppConfig;
use db::DatabaseStore;
use orchestrator::{AgentCmd, QueueEvent, RunId};
use pubsub::create_shared_bus;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_model_connection_and_existence_verification() {
    load_test_env();

    let cfg = match AppConfig::load() {
        Ok(c) => c,
        Err(_) => AppConfig::load_default(),
    };

    // 1. Verify primary LLM model connection and existence
    let llm_check = ModelChecker::check_model_exists(
        &cfg.llm_endpoint_url,
        &cfg.llm_api_key,
        &cfg.llm_model,
    ).await;

    assert!(
        llm_check.exists,
        "Configured primary LLM model '{}' at endpoint '{}' must exist. Message: {}",
        cfg.llm_model, cfg.llm_endpoint_url, llm_check.message
    );

    // 2. Verify pluggable decision model connection and existence
    let decision_check = ModelChecker::check_model_exists(
        &cfg.decision_llm_endpoint_url,
        &cfg.decision_llm_api_key,
        &cfg.decision_llm_model,
    ).await;

    assert!(
        decision_check.exists,
        "Configured decision model '{}' at endpoint '{}' must exist. Message: {}",
        cfg.decision_llm_model, cfg.decision_llm_endpoint_url, decision_check.message
    );

    // 3. Verify STT model connection and existence
    let stt_check = ModelChecker::check_model_exists(
        &cfg.groq_stt_url,
        &cfg.groq_api_key,
        &cfg.stt_model,
    ).await;

    assert!(
        stt_check.exists,
        "Configured STT model '{}' at endpoint '{}' must exist. Message: {}",
        cfg.stt_model, cfg.groq_stt_url, stt_check.message
    );

    // 4. Verify TTS model connection and existence
    let tts_check = ModelChecker::check_model_exists(
        &cfg.openai_tts_url,
        &cfg.openai_api_key,
        &cfg.tts_model,
    ).await;

    assert!(
        tts_check.exists,
        "Configured TTS model '{}' at endpoint '{}' must exist. Message: {}",
        cfg.tts_model, cfg.openai_tts_url, tts_check.message
    );
}

#[tokio::test]
async fn test_non_existent_model_detected_correctly() {
    load_test_env();

    // Verify checker handles invalid/empty endpoints safely without panicking
    let invalid_check = ModelChecker::check_model_exists(
        "",
        "mock-key",
        "invalid-model-name-xyz",
    ).await;

    assert!(!invalid_check.exists, "Empty endpoint must be marked as not existing");
    assert!(invalid_check.message.contains("empty"));
}

#[tokio::test]
async fn test_system_models_verification_report() {
    load_test_env();

    let cfg = match AppConfig::load() {
        Ok(c) => c,
        Err(_) => AppConfig::load_default(),
    };

    let report = ModelChecker::verify_all_system_models(&cfg).await;

    assert!(
        report.all_models_valid,
        "System model verification report must validate all models. LLM: {}, Decision: {}, STT: {}, TTS: {}",
        report.llm_model.message, report.decision_model.message, report.stt_model.message, report.tts_model.message
    );
}

#[tokio::test]
async fn test_edge_questions_availability_and_cache_thinking() {
    load_test_env();

    let db = DatabaseStore::new();
    let pubsub = create_shared_bus(10);
    let run_id = RunId("run-edge-avail-qa".to_string());
    let (orch_tx, mut orch_rx) = mpsc::channel::<QueueEvent>(100);

    let mut agent = agent::LlmAgentActor::new(
        run_id.clone(),
        db,
        pubsub,
        orch_tx,
    );

    // Seed agent with an active cached property
    let listing = db::PropertyListing {
        id: "prop-202".to_string(),
        title: "Downtown Boulevard Residence".to_string(),
        secondary_title: "1BR Luxury Suite".to_string(),
        description: "High-floor suite with Burj Khalifa views and covered parking".to_string(),
        secondary_description: "Fully furnished with designer appliances".to_string(),
        city: "Dubai".to_string(),
        location: "Downtown Dubai".to_string(),
        price_aed: 95000,
        listing_type: "rent".to_string(),
        bedrooms: 1,
        bathrooms: 2,
        area_sqft: 850,
        pet_friendly: false,
        near_metro: true,
        agent_id: "agent-007".to_string(),
        available_slots: vec!["2026-10-20T11:00:00Z".to_string(), "2026-10-21T15:00:00Z".to_string()],
    };
    agent.active_listing_id = Some("prop-202".to_string());
    agent.cached_listings = vec![listing];

    // 1. Edge Question: "Is it available or not?"
    // Requirement 1: Agent MUST answer from already fetched data instead of tool calling again!
    agent.handle_command(AgentCmd::UserInput {
        run_id: run_id.clone(),
        user_id: "user-202".to_string(),
        role: orchestrator::SpeakerRole::Candidate,
        text: "Is it available or not?".to_string(),
    }).await;

    let avail_event = match orch_rx.recv().await {
        Some(ev) => ev,
        None => panic!("Must receive event from agent"),
    };

    match avail_event {
        QueueEvent::Agent(orchestrator::AgentEvent::Finished { result, .. }) => {
            assert!(result.to_lowercase().contains("available"), "Must confirm availability: {}", result);
            assert!(result.contains("Downtown Boulevard Residence"), "Must mention listing title: {}", result);
            assert!(result.contains("2026-10-20T11:00:00Z"), "Must mention available slots: {}", result);
        }
        other => panic!("Expected AgentEvent::Finished from cache, but got: {:?}", other),
    }

    // 2. Edge Question: "Is this apartment furnished?"
    agent.handle_command(AgentCmd::UserInput {
        run_id: run_id.clone(),
        user_id: "user-202".to_string(),
        role: orchestrator::SpeakerRole::Candidate,
        text: "Is this apartment furnished or unfurnished?".to_string(),
    }).await;

    let furn_event = match orch_rx.recv().await {
        Some(ev) => ev,
        None => panic!("Must receive event from agent"),
    };

    match furn_event {
        QueueEvent::Agent(orchestrator::AgentEvent::Finished { result, .. }) => {
            assert!(result.to_lowercase().contains("furnished"), "Must answer furnishing: {}", result);
        }
        other => panic!("Expected Finished from cache, got: {:?}", other),
    }

    // 3. Edge Question: "What view does it have?"
    agent.handle_command(AgentCmd::UserInput {
        run_id: run_id.clone(),
        user_id: "user-202".to_string(),
        role: orchestrator::SpeakerRole::Candidate,
        text: "What view does it have?".to_string(),
    }).await;

    let view_event = match orch_rx.recv().await {
        Some(ev) => ev,
        None => panic!("Must receive event from agent"),
    };

    match view_event {
        QueueEvent::Agent(orchestrator::AgentEvent::Finished { result, .. }) => {
            assert!(result.to_lowercase().contains("view"), "Must describe view: {}", result);
        }
        other => panic!("Expected Finished from cache, got: {:?}", other),
    }

    // 4. Edge Question: "How many cheques are required?"
    agent.handle_command(AgentCmd::UserInput {
        run_id: run_id.clone(),
        user_id: "user-202".to_string(),
        role: orchestrator::SpeakerRole::Candidate,
        text: "How many cheques are required for payment?".to_string(),
    }).await;

    let cheque_event = match orch_rx.recv().await {
        Some(ev) => ev,
        None => panic!("Must receive event from agent"),
    };

    match cheque_event {
        QueueEvent::Agent(orchestrator::AgentEvent::Finished { result, .. }) => {
            assert!(result.contains("95000") || result.contains("cheque"), "Must answer payment terms: {}", result);
        }
        other => panic!("Expected Finished from cache, got: {:?}", other),
    }

    // 5. Unrelated Question: "Can you write a python script for sorting an array?"
    // Requirement 2: Agent must inform user in English that information is not available!
    agent.handle_command(AgentCmd::UserInput {
        run_id: run_id.clone(),
        user_id: "user-202".to_string(),
        role: orchestrator::SpeakerRole::Candidate,
        text: "Can you write a python script for sorting an array?".to_string(),
    }).await;

    let unrel_event = match orch_rx.recv().await {
        Some(ev) => ev,
        None => panic!("Must receive event from agent"),
    };

    match unrel_event {
        QueueEvent::Agent(orchestrator::AgentEvent::Finished { result, .. }) => {
            assert!(result.contains("information is not available"), "Must state information not available: {}", result);
            assert!(result.contains("real estate"), "Must steer back to real estate in English: {}", result);
        }
        other => panic!("Expected Finished for unrelated inquiry, got: {:?}", other),
    }
}
