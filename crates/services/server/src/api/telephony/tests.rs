use super::*;
use axum::{
    body::to_bytes,
    extract::{Path, Query, State},
    http::Uri,
};
use futures_util::{SinkExt, StreamExt};
use orchestrator::phone_media::{PhoneMediaRegistry, PhoneOutput};
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc;

pub(super) fn state(provider: TelephonyProvider) -> (AppState, mpsc::Receiver<QueueEvent>) {
    let mut config = config::AppConfig::load_default();
    config.telephony.provider = provider;
    config.telephony.public_base_url = Some("https://voice.example.com".into());
    config.telephony.webhook_secret = Some("media-test-secret".into());
    config.telephony.twilio_account_sid = Some("ACtest".into());
    config.telephony.twilio_auth_token = Some("twilio-test-secret".into());
    config.telephony.telnyx_api_key = Some("telnyx-test-key".into());
    config.telephony.telnyx_connection_id = Some("connection-test".into());
    config.telephony.telnyx_public_key = Some("unused-test-key".into());
    config.telephony.vonage_api_key = Some("vonage-test-key".into());
    config.telephony.vonage_signature_secret = Some("vonage-test-secret".into());
    config.telephony.plivo_auth_id = Some("plivo-test-id".into());
    config.telephony.plivo_auth_token = Some("plivo-test-secret".into());
    config.telephony.sip_gateway_url = Some("https://gateway.example.com".into());
    config.telephony.custom_ingress_url = Some("https://gateway.example.com".into());
    let db = db::DatabaseStore::new();
    let (orch_tx, rx) = mpsc::channel(100);
    (
        AppState {
            slots: orchestrator::slots::Slots::new(&sled::Config::new().temporary(true).open().unwrap(), 20).unwrap(),
            db: db.clone(),
            orch_tx,
            config,
            phone_media: PhoneMediaRegistry::default(),
            pubsub: pubsub::create_shared_bus(100),
            rag: Arc::new(rag::RAGPipeline::new(db)),
        },
        rx,
    )
}

#[tokio::test]
async fn inactive_and_unsigned_provider_callbacks_never_create_sessions() {
    for selected in [
        TelephonyProvider::Disabled,
        TelephonyProvider::Twilio,
        TelephonyProvider::Telnyx,
        TelephonyProvider::Vonage,
        TelephonyProvider::Plivo,
        TelephonyProvider::Sip,
        TelephonyProvider::Custom,
    ] {
        let (state, mut queue) = state(selected);
        for requested in ["twilio", "telnyx", "vonage", "plivo", "sip", "custom"] {
            let response = inbound_handler(
                Path(requested.into()),
                State(state.clone()),
                OriginalUri(Uri::from_static("/api/telephony/test/incoming")),
                HeaderMap::new(),
                Bytes::new(),
            )
            .await;
            assert_eq!(
                response.status(),
                if requested == selected.as_str() {
                    StatusCode::UNAUTHORIZED
                } else {
                    StatusCode::NOT_FOUND
                }
            );
            assert!(queue.try_recv().is_err());
        }
    }
}

#[tokio::test]
async fn signed_gateway_retries_do_not_create_duplicate_sessions_and_hangup_cancels() {
    let (state, mut queue) = state(TelephonyProvider::Custom);
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    headers.insert("authorization", "Bearer media-test-secret".parse().unwrap());
    let uri = Uri::from_static("/api/telephony/custom/incoming");
    let body = Bytes::from_static(br#"{"call_id":"call-a","event":"start","from":"caller"}"#);
    for _ in 0..2 {
        let response = inbound_handler(
            Path("custom".into()),
            State(state.clone()),
            OriginalUri(uri.clone()),
            headers.clone(),
            body.clone(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }
    assert!(
        matches!(queue.recv().await, Some(QueueEvent::SessionStarted { session_id, .. }) if session_id == "phone:custom:call-a")
    );
    assert!(queue.try_recv().is_err());
    let response = inbound_handler(
        Path("custom".into()),
        State(state.clone()),
        OriginalUri(uri),
        headers,
        Bytes::from_static(br#"{"call_id":"call-a","event":"hangup"}"#),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(matches!(
        queue.recv().await,
        Some(QueueEvent::SessionCancelled { .. })
    ));
    assert!(state.phone_media.sender("phone:custom:call-a").is_none());
}

#[test]
fn media_tokens_bind_provider_session_and_expiry_and_keep_full_telnyx_ids() {
    let (state, _) = state(TelephonyProvider::Telnyx);
    let config = &state.config.telephony;
    let id = "phone:telnyx:v3:control-id";
    let token =
        mint_media_capability(config, TelephonyProvider::Telnyx, id, unix_now() + 120).unwrap();
    assert!(verify_media_capability(
        config,
        TelephonyProvider::Telnyx,
        id,
        &token
    ));
    assert!(!verify_media_capability(
        config,
        TelephonyProvider::Twilio,
        id,
        &token
    ));
    assert!(!verify_media_capability(
        config,
        TelephonyProvider::Telnyx,
        "phone:telnyx:other",
        &token
    ));
    let expired =
        mint_media_capability(config, TelephonyProvider::Telnyx, id, unix_now() - 1).unwrap();
    assert!(!verify_media_capability(
        config,
        TelephonyProvider::Telnyx,
        id,
        &expired
    ));
    let url = media_url(config, TelephonyProvider::Telnyx, id).unwrap();
    assert!(url.contains("/v3%3Acontrol-id/"));
    assert!(!url.contains('?'));
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    let callback = normalize_callback(
        TelephonyProvider::Telnyx,
        &headers,
        br#"{"data":{"event_type":"call.hangup","payload":{"call_control_id":"v3:control-id"}}}"#,
    )
    .unwrap();
    assert_eq!(callback.call_id, "v3:control-id");
    assert_eq!(callback.kind, CallbackKind::End);
}

#[tokio::test]
async fn browser_routes_reject_phone_ids_without_dispatching_or_creating_tokens() {
    let (state, mut queue) = state(TelephonyProvider::Twilio);
    let id = "phone:twilio:CA-test".to_string();
    let request = || super::super::calls::CallReq {
        call_id: id.clone(),
        user_id: "browser-user".into(),
    };
    let responses = [
        super::super::calls::init_call_session(State(state.clone()), Json(request()))
            .await
            .into_response(),
        super::super::calls::end_call_session(State(state.clone()), Json(request()))
            .await
            .into_response(),
        super::super::calls::pause_call_handler(
            State(state.clone()),
            Json(super::super::calls::PauseCallReq {
                session_id: id.clone(),
                paused: true,
            }),
        )
        .await
        .into_response(),
        super::super::calls::get_cached_suggestions(Path(id.clone()), State(state.clone()))
            .await
            .into_response(),
        super::super::calls::sse_suggestions_handler(
            Query(super::super::calls::SseQuery { call_id: Some(id) }),
            State(state.clone()),
        )
        .await
        .into_response(),
    ];
    for response in responses {
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
    assert!(queue.try_recv().is_err());
}

#[tokio::test]
async fn twilio_and_plivo_responses_select_supported_stream_urls_and_codecs() {
    for provider in [TelephonyProvider::Twilio, TelephonyProvider::Plivo] {
        let (state, _) = state(provider);
        let response = provider_accept_response(
            &state.config.telephony,
            provider,
            &format!("phone:{}:call-test", provider.as_str()),
        )
        .await
        .unwrap();
        let body = to_bytes(response.into_body(), 8192).await.unwrap();
        let text = std::str::from_utf8(&body).unwrap();
        assert!(text.contains("wss://voice.example.com/api/telephony/"));
        assert!(!text.contains('?'));
        if provider == TelephonyProvider::Plivo {
            assert!(text.contains("audio/x-mulaw;rate=8000"));
            assert!(text.contains("keepCallAlive=\"true\""));
        }
    }
}

#[tokio::test]
async fn every_provider_socket_carries_audio_both_ways_without_web_events() {
    use tokio_tungstenite::tungstenite::Message;
    for provider in [
        TelephonyProvider::Twilio,
        TelephonyProvider::Telnyx,
        TelephonyProvider::Vonage,
        TelephonyProvider::Plivo,
        TelephonyProvider::Sip,
        TelephonyProvider::Custom,
    ] {
        let (state, mut queue) = state(provider);
        let mut browser = state.pubsub.subscribe();
        let call_id = if provider == TelephonyProvider::Telnyx {
            "v3:call-test"
        } else {
            "call-test"
        };
        let session = format!("phone:{}:{call_id}", provider.as_str());
        state.phone_media.reserve(&session);
        let output = state.phone_media.sender(&session).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = crate::api::api_routes().with_state(state.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let token = mint_media_capability(
            &state.config.telephony,
            provider,
            &session,
            unix_now() + 120,
        )
        .unwrap();
        let url = format!(
            "ws://{addr}/api/telephony/{}/media/{call_id}/{token}",
            provider.as_str()
        );
        let (mut socket, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
        // The capability cannot attach a second provider socket to the call.
        assert!(tokio_tungstenite::connect_async(&url).await.is_err());
        let start = match provider {
            TelephonyProvider::Twilio => {
                json!({"event":"start", "start":{"callSid":call_id,"streamSid":"stream-test","mediaFormat":{"encoding":"audio/x-mulaw","sampleRate":8000,"channels":1}}})
            }
            TelephonyProvider::Telnyx => {
                json!({"event":"start", "start":{"call_control_id":call_id,"stream_id":"stream-test","media_format":{"encoding":"PCMU","sample_rate":8000,"channels":1}}})
            }
            TelephonyProvider::Plivo => {
                json!({"event":"start", "start":{"callId":call_id,"streamId":"stream-test","mediaFormat":{"encoding":"audio/x-mulaw","sampleRate":8000}}})
            }
            TelephonyProvider::Vonage => {
                json!({"event":"websocket:connected","content-type":"audio/l16;rate=16000"})
            }
            _ => {
                json!({"event":"start","call_id":call_id,"encoding":"pcm_s16le","sample_rate":16000,"channels":1})
            }
        };
        socket
            .send(Message::Text(start.to_string().into()))
            .await
            .unwrap();
        let input = if matches!(
            provider,
            TelephonyProvider::Twilio | TelephonyProvider::Telnyx | TelephonyProvider::Plivo
        ) {
            Message::Text(json!({"event":"media","streamSid":"stream-test","streamId":"stream-test","media":{"payload":STANDARD.encode([255;160]),"track":"inbound"}}).to_string().into())
        } else {
            Message::Binary(vec![0; 640].into())
        };
        socket.send(input).await.unwrap();
        match tokio::time::timeout(Duration::from_secs(3), queue.recv())
            .await
            .unwrap()
            .unwrap()
        {
            QueueEvent::TelephonyAudio {
                session_id,
                samples,
                sample_rate,
                elapsed_samples,
                ..
            } => {
                assert_eq!(session_id, session);
                assert_eq!(sample_rate, 16_000);
                assert_eq!(samples.len(), 320);
                assert_eq!(elapsed_samples, 0);
            }
            _ => panic!("expected phone audio"),
        }
        output
            .send(PhoneOutput::Audio {
                pcm: vec![1000; 480],
                sample_rate: 24_000,
            })
            .await
            .unwrap();
        let response = tokio::time::timeout(Duration::from_secs(3), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        if matches!(
            provider,
            TelephonyProvider::Twilio | TelephonyProvider::Telnyx | TelephonyProvider::Plivo
        ) {
            let value: Value = serde_json::from_str(response.to_text().unwrap()).unwrap();
            assert_eq!(
                value["event"],
                if provider == TelephonyProvider::Plivo {
                    "playAudio"
                } else {
                    "media"
                }
            );
            assert_eq!(
                STANDARD
                    .decode(value["media"]["payload"].as_str().unwrap())
                    .unwrap()
                    .len(),
                160
            );
        } else {
            assert_eq!(response.into_data().len(), 640);
        }
        assert!(browser.try_recv().is_err());
        socket.close(None).await.unwrap();
        assert!(
            matches!(tokio::time::timeout(Duration::from_secs(3), queue.recv()).await.unwrap(), Some(QueueEvent::SessionCancelled { session_id }) if session_id == session)
        );
        assert!(state.phone_media.sender(&session).is_none());
        server.abort();
    }
}

#[tokio::test]
async fn phone_admission_rejects_full_cell_without_leaking_media_reservation() {
    let (state, mut queue) = state(TelephonyProvider::Custom);
    for i in 0..state.slots.capacity {
        state.slots.acquire(&format!("occupied-{i}"), 0).unwrap();
    }
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    headers.insert("authorization", "Bearer media-test-secret".parse().unwrap());
    let response = inbound_handler(
        Path("custom".into()), State(state.clone()),
        OriginalUri(Uri::from_static("/api/telephony/custom/incoming")), headers,
        Bytes::from_static(br#"{"call_id":"overflow","event":"start"}"#),
    ).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(queue.try_recv().is_err());
    assert!(state.phone_media.sender("phone:custom:overflow").is_none());
    assert_eq!(state.slots.used(), state.slots.capacity);
}
