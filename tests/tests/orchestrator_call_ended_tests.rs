use db::DatabaseStore;
use integration_tests::load_test_env;
use orchestrator::{Orchestrator, QueueEvent, RunId};
use pubsub::create_shared_bus;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_orchestrator_multiturn_cached_context_qa() {
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
        let agent =
            agent::LlmAgentActor::new(run_id, db_clone.clone(), pubsub_clone.clone(), orch_tx_in);
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
    let session_id = "session-qa-cache-999".to_string();
    orch_tx
        .send(QueueEvent::SessionStarted {
            session_id: session_id.clone(),
            user_id: "user-buyer-verified".to_string(),
            language: "en".to_string(),
        })
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    // 4. Turn 1: User sends initial search query
    orch_tx
        .send(QueueEvent::SttTranscript {
            session_id: session_id.clone(),
            text: "I want property in dubai marina".to_string(),
            language: "en".to_string(),
            is_final: true,
        })
        .await
        .unwrap();

    // Wait for initial search and RAG caching to resolve
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Ok(events) = db.load_recent_stream_events().await {
            if events
                .iter()
                .any(|e| e.session_id.contains(&session_id) && e.event_type == "LLM_RESPONSE")
            {
                break;
            }
        }
    }

    // 5. Turn 2: User asks follow-up question: "What is the rent?"
    // Handled directly from agent's cached context without repeating the initial generic response!
    orch_tx
        .send(QueueEvent::SttTranscript {
            session_id: session_id.clone(),
            text: "What is the rent?".to_string(),
            language: "en".to_string(),
            is_final: true,
        })
        .await
        .unwrap();

    // 6. Turn 3: User asks follow-up question: "How many bedrooms does it have?"
    orch_tx
        .send(QueueEvent::SttTranscript {
            session_id: session_id.clone(),
            text: "How many bedrooms does it have?".to_string(),
            language: "en".to_string(),
            is_final: true,
        })
        .await
        .unwrap();

    // 7. Verify session history recorded in redis conversation stream
    let mut found_rent_response = false;
    let mut found_bedroom_response = false;

    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Ok(events) = db.load_recent_stream_events().await {
            for event in events.iter() {
                if event.session_id.contains(&session_id) && event.event_type == "LLM_RESPONSE" {
                    if event.payload_json.contains("rent") || event.payload_json.contains("115000")
                    {
                        found_rent_response = true;
                    }
                    if event.payload_json.contains("bedroom") || event.payload_json.contains("2") {
                        found_bedroom_response = true;
                    }
                }
            }
        }
        if found_rent_response && found_bedroom_response {
            break;
        }
    }

    assert!(
        found_rent_response,
        "Agent must answer rent inquiry from cached property context"
    );
    assert!(
        found_bedroom_response,
        "Agent must answer bedroom inquiry from cached property context"
    );
}

#[tokio::test]
async fn test_orchestrator_call_ended_saves_text_and_audio_presigned_urls_to_db() {
    load_test_env();

    // 1. Setup tool client dynamically from environment variables
    let tool_client = integration_tests::create_test_tool_client();

    // 2. Setup Orchestrator (The Brain) with real DbActor
    let pubsub = create_shared_bus(100);
    let (orch_tx, orch_rx) = mpsc::channel(100);
    let protected_orch_rx = Arc::new(tokio::sync::Mutex::new(orch_rx));
    let (db_tx, db_rx) = mpsc::channel(1024);
    let protected_db_rx = Arc::new(tokio::sync::Mutex::new(db_rx));
    let local_db = sled::Config::new().temporary(true).open().unwrap();

    let db = DatabaseStore::new();
    let db_clone = db.clone();
    let pubsub_clone = pubsub.clone();
    let agent_factory = Arc::new(move |run_id: RunId, orch_tx_in: mpsc::Sender<QueueEvent>| {
        let (agent_tx, agent_rx) = mpsc::channel(100);
        let agent =
            agent::LlmAgentActor::new(run_id, db_clone.clone(), pubsub_clone.clone(), orch_tx_in);
        tokio::spawn(async move {
            agent::LlmAgentActor::run_agent_loop(agent_rx, agent).await;
        });
        agent_tx
    });

    // Spawn DbActor loop to handle db requests asynchronously
    let db_actor = db::actor::DbActor::new(db.clone(), None, local_db.clone());
    tokio::spawn(db::actor::DbActor::run_loop(protected_db_rx, db_actor));

    let orchestrator = Orchestrator::new(
        pubsub.clone(),
        db.clone(),
        local_db.clone(),
        tool_client,
        orch_tx.clone(),
        db_tx,
        agent_factory,
    );

    // Run Orchestrator Dispatcher
    let orch_clone = orchestrator.clone();
    tokio::spawn(Orchestrator::run_dispatcher(protected_orch_rx, orch_clone));

    // 3. Start a call session
    let session_id = "session-presigned-test-888".to_string();
    orch_tx
        .send(QueueEvent::SessionStarted {
            session_id: session_id.clone(),
            user_id: "user-buyer-presigned".to_string(),
            language: "en".to_string(),
        })
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Simulate active user transcript during the call
    orch_tx
        .send(QueueEvent::SttTranscript {
            session_id: session_id.clone(),
            text: "Hello, I am looking for a 2 bedroom apartment in Dubai.".to_string(),
            language: "en".to_string(),
            is_final: true,
        })
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(150)).await;

    // 4. End the call by sending SessionCancelled event
    orch_tx
        .send(QueueEvent::SessionCancelled {
            session_id: session_id.clone(),
        })
        .await
        .unwrap();

    // 5. Wait for the DB actor to execute SaveS3Endpoints and persist records
    tokio::time::sleep(Duration::from_millis(300)).await;

    // 6. Verify text and audio presigned URLs are saved in db.call_endpoints
    let endpoint_record = db
        .get_call_endpoints(&session_id).await.unwrap()
        .expect("CallEndpointsRecord must be saved to DB when call is ended");

    assert_eq!(endpoint_record.session_id, session_id);
    assert!(
        endpoint_record
            .text_url
            .contains("transcript.txt?expires=3600"),
        "Text URL must be a valid presigned transcript URL, got: {}",
        endpoint_record.text_url
    );
    assert!(
        endpoint_record.audio_url.contains("audio.wav?expires=3600"),
        "Audio URL must be a valid presigned audio URL, got: {}",
        endpoint_record.audio_url
    );

    // 7. Verify text and audio presigned URLs are also saved in db.call_summaries
    let summary_record = db
        .get_call_summary(&session_id).await.unwrap()
        .expect("CallSummaryRecord must be saved to DB when call is ended");

    assert_eq!(summary_record.call_id, session_id);
    assert_eq!(summary_record.text_presigned_url, endpoint_record.text_url);
    assert_eq!(
        summary_record.audio_presigned_url,
        endpoint_record.audio_url
    );
    assert!(
        summary_record.duration_seconds > 0,
        "Duration must be recorded"
    );

    // 8. Verify Redis stream event SESSION_ENDED is also recorded
    if let Ok(events) = db.load_recent_stream_events().await {
        let has_ended_event = events
            .iter()
            .any(|e| e.session_id == session_id && e.event_type == "SESSION_ENDED");
        assert!(
            has_ended_event,
            "SESSION_ENDED event must be pushed to stream:audit"
        );
    };
}

#[tokio::test]
async fn test_db_store_call_ended_presigned_urls_persistence() {
    load_test_env();

    let db = DatabaseStore::new();
    let cfg = config::AppConfig::load().unwrap_or_else(|_| config::AppConfig::load_default());
    let session_id = "session-direct-presigned-777".to_string();
    let user_id = "user-vip-client".to_string();

    let s3_endpoint = &cfg.r2_endpoint_url;
    let s3_bucket = &cfg.r2_bucket_name;
    let audio_presigned = format!(
        "{}/{}/{}/audio.wav?expires=3600&sig=abc",
        s3_endpoint, s3_bucket, session_id
    );
    let text_presigned = format!(
        "{}/{}/{}/transcript.txt?expires=3600&sig=xyz",
        s3_endpoint, s3_bucket, session_id
    );

    // Persist call endpoints on call end
    db.save_call_endpoints(&session_id, &audio_presigned, &text_presigned).await.unwrap();
    db.save_call_summary(
        &session_id,
        &user_id,
        240,
        &text_presigned,
        &audio_presigned,
        "AI Summary: Client verified and ready for contract signing.",
        250000,
        "Downtown Dubai",
    ).await.unwrap();

    // 1. Verify get_call_endpoints retrieves exact text and audio presigned URLs
    let endpoints = db
        .get_call_endpoints(&session_id).await.unwrap()
        .expect("Must retrieve call endpoints for ended session");
    assert_eq!(endpoints.session_id, session_id);
    assert_eq!(endpoints.audio_url, audio_presigned);
    assert_eq!(endpoints.text_url, text_presigned);
    assert!(endpoints.audio_url.contains("audio.wav"));
    assert!(endpoints.text_url.contains("transcript.txt"));

    // 2. Verify get_call_summary retrieves exact text and audio presigned URLs
    let summary = db
        .get_call_summary(&session_id).await.unwrap()
        .expect("Must retrieve call summary for ended session");
    assert_eq!(summary.call_id, session_id);
    assert_eq!(summary.user_id, user_id);
    assert_eq!(summary.audio_presigned_url, audio_presigned);
    assert_eq!(summary.text_presigned_url, text_presigned);
    assert_eq!(summary.duration_seconds, 240);
    assert_eq!(summary.qualified_budget_aed, 250000);
    assert_eq!(summary.preferred_location, "Downtown Dubai");

    // 3. Verify get_all_call_endpoints contains the ended session
    let all_endpoints = db.get_all_call_endpoints().await.unwrap();
    assert!(all_endpoints.iter().any(|ep| ep.session_id == session_id));
}
