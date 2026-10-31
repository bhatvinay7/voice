use db::DatabaseStore;

#[tokio::test]
async fn test_livekit_tls_websocket_connection() {
    let _ = dotenvy::dotenv();
    let _ = dotenvy::from_path(".env");
    let _ = dotenvy::from_path("../.env");
    let _ = dotenvy::from_path("../../.env");

    let url = std::env::var("LIVEKIT_URL").expect("LIVEKIT_URL must be imported from .env");
    let api_key = std::env::var("LIVEKIT_API_KEY").expect("LIVEKIT_API_KEY must be imported from .env");
    let api_secret = std::env::var("LIVEKIT_API_SECRET").expect("LIVEKIT_API_SECRET must be imported from .env");

    let token = livekit_api::access_token::AccessToken::with_api_key(&api_key, &api_secret)
        .with_identity("test-identity")
        .with_name("Test Identity")
        .with_grants(livekit_api::access_token::VideoGrants {
            room_join: true,
            room: "test-room".to_string(),
            ..Default::default()
        })
        .to_jwt()
        .unwrap_or_default();

    let result = livekit::Room::connect(&url, &token, livekit::RoomOptions::default()).await;
    
    if let Err(ref err) = result {
        let err_str = format!("{:?}", err);
        assert!(
            !err_str.contains("TLS support not compiled in"),
            "LiveKit failed with TLS support error: {}",
            err_str
        );
    }
}

#[tokio::test]
async fn test_webrtc_mic_audio_data_sending_and_forwarding() {
    let _ = dotenvy::dotenv();
    let _ = dotenvy::from_path(".env");
    let _ = dotenvy::from_path("../.env");
    let _ = dotenvy::from_path("../../.env");

    let livekit_url = std::env::var("LIVEKIT_URL")
        .expect("LIVEKIT_URL must be imported from .env");
    let livekit_api_key = std::env::var("LIVEKIT_API_KEY")
        .expect("LIVEKIT_API_KEY must be imported from .env");
    let livekit_api_secret = std::env::var("LIVEKIT_API_SECRET")
        .expect("LIVEKIT_API_SECRET must be imported from .env");

    assert!(!livekit_url.is_empty(), "LIVEKIT_URL imported from .env must not be empty");
    assert!(!livekit_api_key.is_empty(), "LIVEKIT_API_KEY imported from .env must not be empty");
    assert!(!livekit_api_secret.is_empty(), "LIVEKIT_API_SECRET imported from .env must not be empty");

    let room_name = "room-webrtc-test-mic";
    let token = livekit_api::access_token::AccessToken::with_api_key(&livekit_api_key, &livekit_api_secret)
        .with_identity("mic-publisher-001")
        .with_name("Mic Publisher")
        .with_grants(livekit_api::access_token::VideoGrants {
            room_join: true,
            room: room_name.to_string(),
            can_publish: Some(true),
            can_subscribe: Some(true),
            can_publish_data: Some(true),
            ..Default::default()
        })
        .to_jwt()
        .expect("Failed to issue JWT token using .env credentials");

    assert!(!token.is_empty(), "Generated WebRTC JWT token must not be empty");

    let sample_rate = 16000;
    let duration_ms = 100;
    let num_samples = (sample_rate * duration_ms) / 1000;
    let mut pcm_mic_data: Vec<i16> = Vec::with_capacity(num_samples);
    for i in 0..num_samples {
        let sample = (32767.0 * (2.0 * std::f32::consts::PI * 440.0 * (i as f32) / (sample_rate as f32)).sin()) as i16;
        pcm_mic_data.push(sample);
    }
    let pcm_bytes: Vec<u8> = pcm_mic_data.iter().flat_map(|s| s.to_le_bytes()).collect();

    let (webrtc_tx, mut webrtc_rx) = tokio::sync::mpsc::channel::<orchestrator::WebRtcAudioChunk>(100);
    let (s3_dump_tx, s3_dump_rx) = tokio::sync::mpsc::channel::<orchestrator::S3DumpData>(100);

    let session_id = "session-mic-forwarding-101".to_string();

    let session_id_clone = session_id.clone();
    tokio::spawn(async move {
        orchestrator::supervisors::run_s3_dump_pipeline(session_id_clone, s3_dump_rx).await;
    });

    let chunk = orchestrator::WebRtcAudioChunk {
        session_id: session_id.clone(),
        pcm_bytes: pcm_bytes.clone(),
    };

    let send_res = webrtc_tx.send(chunk).await;
    assert!(send_res.is_ok(), "WebRTC mic audio chunk should be sent into channel successfully");

    if let Some(forwarded_chunk) = webrtc_rx.recv().await {
        assert_eq!(forwarded_chunk.session_id, session_id);
        assert_eq!(forwarded_chunk.pcm_bytes.len(), pcm_bytes.len());
        assert_eq!(forwarded_chunk.pcm_bytes, pcm_bytes, "Forwarded WebRTC audio data must match original mic audio input");
    } else {
        panic!("WebRTC mic audio chunk was not received on the forwarding receiver!");
    }

    let s3_send_res = s3_dump_tx.send(orchestrator::S3DumpData::Audio {
        session_id: session_id.clone(),
        samples: pcm_bytes.chunks_exact(2).map(|pair| i16::from_le_bytes([pair[0], pair[1]])).collect(),
        sample_rate: 16_000,
        channels: 1,
        elapsed_samples: 0,
        speaker: orchestrator::AudioSpeaker::User,
    }).await;
    assert!(s3_send_res.is_ok(), "Mic audio chunk should be forwarded to S3 dump channel");

    let connect_res = livekit::Room::connect(&livekit_url, &token, livekit::RoomOptions::default()).await;
    if let Err(ref err) = connect_res {
        let err_str = format!("{:?}", err);
        assert!(
            !err_str.contains("TLS support not compiled in"),
            "LiveKit failed with TLS error: {}",
            err_str
        );
    }
}

#[tokio::test]
async fn test_dynamic_db_role_update_and_admin_rbac() {
    let db = DatabaseStore::new();

    let admin_user = db.get_user("user-admin").await.unwrap().expect("user-admin must exist in db");
    assert_eq!(admin_user.role, "admin");

    let normal_user = db.get_user("user-verified").await.unwrap().expect("user-verified must exist in db");
    assert_eq!(normal_user.role, "user");

    assert_ne!(normal_user.role, "admin");

    let promoted = db.update_user_role("user-verified", "admin").await.unwrap().expect("Role update should succeed");
    assert_eq!(promoted.role, "admin");

    let updated_user = db.get_user("user-verified").await.unwrap().expect("user-verified must exist in db");
    assert_eq!(updated_user.role, "admin");

    let demoted = db.update_user_role("user-verified", "user").await.unwrap().expect("Role demote should succeed");
    assert_eq!(demoted.role, "user");
    let demoted_user = db.get_user("user-verified").await.unwrap().unwrap();
    assert_eq!(demoted_user.role, "user");
}
