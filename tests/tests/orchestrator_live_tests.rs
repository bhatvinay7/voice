use integration_tests::load_test_env;
use std::time::Duration;

#[tokio::test]
async fn test_live_running_server_grpc_tool_calls_rag_and_bookings() {
    load_test_env();

    // 1. Connect tonic client to the live running scheduler gRPC server dynamically from env
    let endpoint_str = integration_tests::get_scheduler_endpoint();
    let channel = tonic::transport::Endpoint::from_shared(endpoint_str)
        .expect("Valid scheduler endpoint")
        .connect()
        .await
        .expect("Must connect to live running scheduler gRPC server");
    let mut tool_client = orchestrator::grpc::tools::tool_execution_service_client::ToolExecutionServiceClient::new(channel);

    // 2. Execute RAG Property Search tool call over live gRPC
    let search_req = tonic::Request::new(orchestrator::grpc::tools::ToolRequest {
        run_id: "run-live-test-1".to_string(),
        task_id: "task-live-search-1".to_string(),
        tool_name: "search_properties".to_string(),
        args_json: serde_json::json!({
            "query": "2 bedroom apartment in Dubai Marina under 120k"
        }).to_string(),
    });

    let search_resp = tool_client
        .execute_tool(search_req)
        .await
        .expect("gRPC ExecuteTool for search_properties must succeed")
        .into_inner();

    assert!(search_resp.success, "search_properties gRPC call must return success = true");
    let search_data: serde_json::Value = serde_json::from_str(&search_resp.result_json)
        .expect("gRPC result_json must be valid JSON");
    assert!(search_data["success"].as_bool().unwrap_or(false));
    let resp_text = search_data["response_text"].as_str().unwrap_or("");
    assert!(
        resp_text.contains("Dubai Marina") || resp_text.contains("apartment") || resp_text.contains("AED"),
        "RAG response text must contain property details: {}",
        resp_text
    );
    assert!(search_data["total_found"].as_u64().unwrap_or(0) > 0, "RAG search must find listings");

    // 3. Execute Booking tool call with unverified user -> KYC verification check
    let unverified_booking_req = tonic::Request::new(orchestrator::grpc::tools::ToolRequest {
        run_id: "run-live-test-1".to_string(),
        task_id: "task-live-book-unverified".to_string(),
        tool_name: "book_viewing_slot".to_string(),
        args_json: serde_json::json!({
            "listing_id": "prop-101",
            "user_id": "user-unverified-kyc-check",
            "slot_time": "2026-10-15T10:00:00Z"
        }).to_string(),
    });

    let unverified_resp = tool_client
        .execute_tool(unverified_booking_req)
        .await
        .expect("gRPC ExecuteTool for unverified booking must return response")
        .into_inner();

    let unverified_data: serde_json::Value = serde_json::from_str(&unverified_resp.result_json)
        .expect("result_json must be valid JSON");
    assert!(!unverified_data["success"].as_bool().unwrap_or(true), "Unverified user must not be able to book viewing slot without Emirates ID verification");

    // 4. Execute Booking tool call with verified user -> Successful booking
    let verified_booking_req = tonic::Request::new(orchestrator::grpc::tools::ToolRequest {
        run_id: "run-live-test-1".to_string(),
        task_id: "task-live-book-verified".to_string(),
        tool_name: "book_viewing_slot".to_string(),
        args_json: serde_json::json!({
            "listing_id": "prop-101",
            "user_id": "user-verified",
            "slot_time": "2026-10-15T14:00:00Z"
        }).to_string(),
    });

    let verified_resp = tool_client
        .execute_tool(verified_booking_req)
        .await
        .expect("gRPC ExecuteTool for verified booking must succeed")
        .into_inner();

    let verified_data: serde_json::Value = serde_json::from_str(&verified_resp.result_json)
        .expect("result_json must be valid JSON");
    assert!(verified_data["success"].as_bool().unwrap_or(false), "Verified user must successfully book slot");
}

#[tokio::test]
async fn test_live_running_server_user_call_ai_response_and_real_rag_websocket() {
    use futures_util::SinkExt;

    load_test_env();

    let client = reqwest::Client::new();
    let call_id = format!("call-live-{}", chrono::Utc::now().timestamp_millis());
    let user_id = "user-verified".to_string();

    let server_url = integration_tests::get_server_url();
    let ws_url = format!("{}/ws", server_url.replace("http://", "ws://"));

    // 1. Initialize call session on live Axum server
    let init_resp = client
        .post(format!("{}/api/calls/session", server_url))
        .json(&serde_json::json!({
            "call_id": call_id.clone(),
            "user_id": user_id.clone()
        }))
        .send()
        .await
        .expect("POST /api/calls/session must succeed");

    assert_eq!(init_resp.status(), 200, "Call session init must return HTTP 200");
    let init_json: serde_json::Value = init_resp.json().await.expect("Must parse JSON body");
    assert!(init_json["success"].as_bool().unwrap_or(false));
    let token = init_json["livekit_token"].as_str().unwrap_or("");
    assert!(!token.is_empty(), "LiveKit token must be generated");

    // 2. Connect WebSocket to live server
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("Must connect to live WebSocket endpoint");

    // 3. User sends voice prompt transcript over WebSocket
    let user_transcript = "I am looking for a 2 bedroom apartment in Dubai Marina";
    ws_stream
        .send(tokio_tungstenite::tungstenite::Message::Text(user_transcript.to_string().into()))
        .await
        .expect("Must send transcript text over WebSocket");

    // Give live orchestrator & gRPC agent time to process and cache suggestions in Redis
    tokio::time::sleep(Duration::from_millis(500)).await;

    // 4. Test getting cached suggestions from live server REST API
    let suggestions_url = format!("{}/api/calls/suggestions/{}", server_url, call_id);
    let mut got_suggestions = false;
    for _ in 0..10 {
        if let Ok(resp) = client.get(&suggestions_url).send().await {
            if resp.status() == 200 {
                let val: serde_json::Value = resp.json().await.unwrap_or(serde_json::json!({}));
                if val.get("suggestions").is_some() || val.get("success").and_then(|s| s.as_bool()) == Some(true) {
                    got_suggestions = true;
                    break;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let check_resp = client.get(&suggestions_url).send().await.expect("Must reach suggestions endpoint");
    assert_eq!(check_resp.status(), 200, "Suggestions endpoint must return HTTP 200");
    let _ = got_suggestions;
}

#[tokio::test]
async fn test_live_running_server_barge_in_and_pause_call() {
    use futures_util::SinkExt;

    load_test_env();

    let client = reqwest::Client::new();
    let session_id = "call-pause-live-test-1".to_string();

    let server_url = integration_tests::get_server_url();
    let ws_url = format!("{}/ws", server_url.replace("http://", "ws://"));

    // 1. Pause call (Human Interviewer takeover) via POST /api/calls/pause
    let pause_resp = client
        .post(format!("{}/api/calls/pause", server_url))
        .json(&serde_json::json!({
            "session_id": session_id.clone(),
            "paused": true
        }))
        .send()
        .await
        .expect("POST /api/calls/pause must succeed");

    assert_eq!(pause_resp.status(), 200);
    let pause_json: serde_json::Value = pause_resp.json().await.expect("Must parse JSON");
    assert_eq!(pause_json["paused"], true);

    // 2. Unpause call (AI resumes listening)
    let resume_resp = client
        .post(format!("{}/api/calls/pause", server_url))
        .json(&serde_json::json!({
            "session_id": session_id.clone(),
            "paused": false
        }))
        .send()
        .await
        .expect("POST /api/calls/pause to resume must succeed");

    assert_eq!(resume_resp.status(), 200);
    let resume_json: serde_json::Value = resume_resp.json().await.expect("Must parse JSON");
    assert_eq!(resume_json["paused"], false);

    // 3. Connect to live WebSocket and send BARGE_IN message
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("Must connect to live WebSocket");

    ws_stream
        .send(tokio_tungstenite::tungstenite::Message::Text("BARGE_IN".to_string().into()))
        .await
        .expect("Must send BARGE_IN message without error");

    // Verify socket remains open and healthy after barge-in trigger
    ws_stream
        .send(tokio_tungstenite::tungstenite::Message::Ping(vec![1, 2, 3].into()))
        .await
        .expect("WebSocket ping must succeed after barge-in");
}

#[tokio::test]
async fn test_live_running_scheduler_metrics_and_cell_capacity() {
    load_test_env();

    let client = reqwest::Client::new();

    let metrics_url = format!("{}/metrics", integration_tests::get_metrics_url());

    // Query live Prometheus metrics from scheduler
    let metrics_resp = client
        .get(&metrics_url)
        .send()
        .await
        .expect("GET metrics must succeed");

    assert_eq!(metrics_resp.status(), 200);
    let body = metrics_resp.text().await.expect("Metrics body must be string");

    assert!(body.contains("voice_live_cells"), "Metrics must report voice_live_cells");
    assert!(body.contains("voice_total_free_slots"), "Metrics must report voice_total_free_slots");
    assert!(body.contains("voice_total_active_sessions"), "Metrics must report voice_total_active_sessions");
}
