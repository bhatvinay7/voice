//! `gpt-4o-realtime` Speech-to-Speech pipeline worker.
//!
//! Activated when `STS_MODE=STS_STREAM` in the environment.
//! Implements the same [`StsPipelineCmd`] / [`StsPipelineEvent`] surface as
//! the legacy `sts_pipe` worker — [`SessionActor`] needs no changes.
//!
//! ## Audio flow
//! ```
//!  LiveKit remote track (24 kHz PCM16, from client mic)
//!        │  NativeAudioStream
//!        ├──► S3 dump pipeline  (user audio recording — unchanged)
//!        │
//!        └──► base64 PCM16 chunks (~40 ms)
//!               │  input_audio_buffer.append
//!               ▼
//!         OpenAI Realtime WebSocket (gpt-4o-realtime)
//!               │  server-side VAD detects speech automatically
//!               │
//!               ├── response.audio.delta  (base64 PCM16 24 kHz)
//!               │     → NativeAudioSource → LiveKit local track → client hears AI
//!               │     → S3 dump pipeline  (AI audio recording)
//!               │
//!               ├── response.audio_transcript.delta
//!               │     → StsPipelineEvent::AiTranscriptChunk → SessionActor → Redis/S3
//!               │
//!               ├── conversation.item.input_audio_transcription.completed
//!               │     → StsPipelineEvent::UserTranscriptChunk (is_final=true)
//!               │     → SessionActor → AgentCmd::UserInput  (same as legacy path)
//!               │
//!               ├── input_audio_buffer.speech_started  (server-side barge-in)
//!               │     → tts_source.clear_buffer()
//!               │     → StsPipelineEvent::BargeIn
//!               │
//!               └── response.audio.done
//!                     → StsPipelineEvent::AudioComplete
//! ```

use crate::models::{AudioSpeaker, S3DumpData, StsPipelineCmd, StsPipelineEvent};
use base64::Engine as _;
use futures_util::{FutureExt, SinkExt, StreamExt};
use livekit::options::TrackPublishOptions;
use livekit::track::{LocalAudioTrack, LocalTrack};
use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::audio_source::AudioSourceOptions;
use livekit::webrtc::audio_stream::native::NativeAudioStream;
use livekit::{prelude::RtcAudioSource, RoomOptions};
use std::collections::HashSet;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{self, http::Request};
use tracing::{error, info, warn};

// All pipeline tuning values are env-driven via AppConfig — no hardcoded constants.
// OPENAI_REALTIME_WS_BASE | OPENAI_REALTIME_MODEL | OPENAI_REALTIME_VOICE
// OPENAI_REALTIME_SAMPLE_RATE | OPENAI_REALTIME_FLUSH_FRAMES

// ── public worker handle ───────────────────────────────────────────────────────

/// Drop-in replacement for `StsPipelineWorker` using the OpenAI Realtime API.
/// Call [`RealtimePipelineWorker::get_cmd_tx`] to obtain the [`StsPipelineCmd`]
/// sender — identical interface to the legacy worker.
pub struct RealtimePipelineWorker {
    cmd_tx: mpsc::Sender<StsPipelineCmd>,
}

impl RealtimePipelineWorker {
    /// Spawn the supervisor + session task and return a handle immediately.
    pub fn spawn(
        session_id: String,
        event_tx: mpsc::Sender<StsPipelineEvent>,
        recording_tx: mpsc::Sender<S3DumpData>,
    ) -> Self {
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<StsPipelineCmd>(100);

        tokio::spawn(async move {
            let mut keep_running = true;
            while keep_running {
                info!(session = %session_id, "Realtime STS: starting session worker");

                let ev = event_tx.clone();
                let rec = recording_tx.clone();
                let sid = session_id.clone();

                let result = AssertUnwindSafe(async {
                    run_session(sid, &mut cmd_rx, ev, rec).await;
                })
                .catch_unwind()
                .await;

                match result {
                    Ok(_) => {
                        info!(session = %session_id, "Realtime STS: exited gracefully");
                        keep_running = false;
                    }
                    Err(e) => {
                        error!(
                            session = %session_id, panic = ?e,
                            "Realtime STS: PANIC — restarting in 500 ms"
                        );
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    }
                }
            }
        });

        Self { cmd_tx }
    }

    pub async fn send_cmd(
        &self,
        cmd: StsPipelineCmd,
    ) -> Result<(), mpsc::error::SendError<StsPipelineCmd>> {
        self.cmd_tx.send(cmd).await
    }

    /// Returns a cloned sender — same type the session actor stores.
    pub fn get_cmd_tx(&self) -> mpsc::Sender<StsPipelineCmd> {
        self.cmd_tx.clone()
    }
}

// ── session runner ─────────────────────────────────────────────────────────────

async fn run_session(
    session_id: String,
    cmd_rx: &mut mpsc::Receiver<StsPipelineCmd>,
    event_tx: mpsc::Sender<StsPipelineEvent>,
    recording_tx: mpsc::Sender<S3DumpData>,
) {
    let cfg = config::AppConfig::load_default();

    if cfg.openai_api_key.is_empty() {
        error!("Realtime STS: OPENAI_API_KEY not set");
        return;
    }

    // ── resolve model / voice / URL ───────────────────────────────────────────
    let model = non_empty(&cfg.openai_realtime_model, "gpt-4o-realtime-preview");
    let voice = non_empty(&cfg.openai_realtime_voice, "shimmer");
    let ws_url = if cfg.openai_realtime_url.is_empty() {
        let base = non_empty(
            &cfg.openai_realtime_ws_base,
            "wss://api.openai.com/v1/realtime",
        );
        format!("{}?model={}", base, model)
    } else {
        cfg.openai_realtime_url.clone()
    };
    // Audio tuning — both pulled from cfg (env vars OPENAI_REALTIME_SAMPLE_RATE / FLUSH_FRAMES)
    let sample_rate = cfg.openai_realtime_sample_rate; // Hz, typically 24000
    let flush_frames = cfg.openai_realtime_flush_frames; // LiveKit frames per WS flush
    let stt_language = cfg.stt_language.clone(); // BCP-47, e.g. "en", "ar"
    let llm_response_language = cfg.llm_response_language.clone(); // optional response-language constraint

    info!(session = %session_id, %model, %voice, sample_rate, flush_frames, "Realtime STS: connecting");

    // ── LiveKit connect ───────────────────────────────────────────────────────
    let recording_started = std::time::Instant::now();

    let lk_token = match livekit_api::access_token::AccessToken::with_api_key(
        &cfg.livekit_api_key,
        &cfg.livekit_api_secret,
    )
    .with_identity("AI-Realtime-Agent")
    .with_name("Real Estate AI")
    .with_grants(livekit_api::access_token::VideoGrants {
        room_join: true,
        room: session_id.clone(),
        can_publish: Some(true),
        can_subscribe: Some(true),
        can_publish_data: Some(true),
        ..Default::default()
    })
    .to_jwt()
    {
        Ok(t) => t,
        Err(e) => {
            error!("Realtime STS: LiveKit JWT error: {e}");
            return;
        }
    };

    let (room, mut room_events) =
        match livekit::Room::connect(&cfg.livekit_url, &lk_token, RoomOptions::default()).await {
            Ok(r) => r,
            Err(e) => {
                error!("Realtime STS: LiveKit connect failed: {e}");
                return;
            }
        };

    // Publish the AI audio output track (24 kHz mono)
    let ai_source = NativeAudioSource::new(AudioSourceOptions::default(), sample_rate, 1, 40);
    let ai_track = LocalAudioTrack::create_audio_track(
        "assistant-realtime",
        RtcAudioSource::Native(ai_source.clone()),
    );
    if let Err(e) = room
        .local_participant()
        .publish_track(LocalTrack::Audio(ai_track), TrackPublishOptions::default())
        .await
    {
        error!("Realtime STS: publish track failed: {e}");
    }

    // ── OpenAI Realtime WebSocket ─────────────────────────────────────────────
    let request = match build_ws_request(&ws_url, &cfg.openai_api_key) {
        Ok(r) => r,
        Err(e) => {
            error!("Realtime STS: WS request error: {e}");
            return;
        }
    };

    let (ws_stream, _) =
        match tokio_tungstenite::connect_async_tls_with_config(request, None, false, None).await {
            Ok(r) => r,
            Err(e) => {
                error!("Realtime STS: OpenAI connect failed: {e}");
                return;
            }
        };

    let (mut ws_sink, mut ws_stream) = ws_stream.split();

    // Send session.update — system prompt + VAD config from the same env vars
    // the legacy pipeline uses (no new config needed).
    let instructions = non_empty(
        &cfg.agent_system_prompt,
        "You are a helpful voice assistant. Be concise and natural.",
    );

    // Append a language-constraint sentence when LLM_RESPONSE_LANGUAGE is set.
    let instructions = if llm_response_language.is_empty() {
        instructions.to_string()
    } else {
        format!(
            "{} Always respond in {} regardless of the language the user speaks in.",
            instructions, llm_response_language
        )
    };

    // Build the input_audio_transcription object — include language only when set.
    let transcription_cfg = if stt_language.is_empty() {
        serde_json::json!({ "model": "whisper-1" })
    } else {
        serde_json::json!({ "model": "whisper-1", "language": stt_language })
    };

    let session_cfg = serde_json::json!({
        "type": "session.update",
        "session": {
            "modalities": ["audio", "text"],
            "instructions": instructions,
            "voice": voice,
            "input_audio_format": "pcm16",
            "output_audio_format": "pcm16",
            "input_audio_transcription": transcription_cfg,
            "turn_detection": {
                "type": "server_vad",
                "threshold": cfg.vad_speech_probability_threshold,
                "silence_duration_ms": cfg.vad_end_of_speech_timeout_ms,
                "create_response": true,
                "interrupt_response": true
            },
            "temperature": 0.7,
            "max_response_output_tokens": 1024
        }
    });

    if let Err(e) = ws_sink
        .send(tungstenite::Message::Text(session_cfg.to_string().into()))
        .await
    {
        error!("Realtime STS: session.update failed: {e}");
        return;
    }
    info!(session = %session_id, "Realtime STS: session.update sent");

    // ── internal channels ─────────────────────────────────────────────────────
    // PCM bytes (user mic) → base64 encoder task
    let (pcm_tx, mut pcm_rx) = mpsc::channel::<Vec<u8>>(500);
    // Outgoing WS messages (audio + control events)
    let (ws_out_tx, mut ws_out_rx) = mpsc::channel::<tungstenite::Message>(500);
    let mut greeting_pending = std::env::var("AI_GREETING")
        .ok()
        .filter(|greeting| !greeting.trim().is_empty());

    // Pass sample_rate + flush_frames into the forwarder and event handler below.
    // Both are captured by value from cfg above.
    let ai_speaking = Arc::new(AtomicBool::new(false));
    let attached: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

    // Attach pre-existing participant tracks
    for (_sid, participant) in room.remote_participants() {
        for (_tsid, pub_) in participant.track_publications() {
            if let Some(livekit::track::RemoteTrack::Audio(track)) = pub_.track() {
                let id = track.sid().to_string();
                if attached.lock().unwrap().insert(id.clone()) {
                    spawn_audio_forwarder(
                        session_id.clone(),
                        track,
                        pcm_tx.clone(),
                        recording_tx.clone(),
                        recording_started,
                        sample_rate,
                        flush_frames,
                        cfg.vad_min_speech_amplitude,
                    );
                    if let Some(greeting) = greeting_pending.take() {
                        queue_realtime_greeting(&ws_out_tx, &greeting, &session_id).await;
                    }
                }
            }
        }
    }

    // Task: PCM bytes → base64 → WS `input_audio_buffer.append`
    let ws_out_audio = ws_out_tx.clone();
    tokio::spawn(async move {
        while let Some(pcm_bytes) = pcm_rx.recv().await {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&pcm_bytes);
            let msg = serde_json::json!({
                "type": "input_audio_buffer.append",
                "audio": b64
            });
            if ws_out_audio
                .send(tungstenite::Message::Text(msg.to_string().into()))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    // Task: drain ws_out_rx → actual WebSocket sink
    tokio::spawn(async move {
        while let Some(msg) = ws_out_rx.recv().await {
            if ws_sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    // ── main event loop ───────────────────────────────────────────────────────
    loop {
        tokio::select! {

            // ── commands from SessionActor ────────────────────────────────────
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(StsPipelineCmd::InjectContext { text }) | Some(StsPipelineCmd::InjectAgentContext { text }) => {
                        info!(
                            session = %session_id, chars = text.len(),
                            "Realtime STS: InjectContext → assistant turn + response.create"
                        );
                        // Inject agent text as an assistant message; the model
                        // will immediately speak it back in the configured voice.
                        let item = serde_json::json!({
                            "type": "conversation.item.create",
                            "item": {
                                "type": "message",
                                "role": "assistant",
                                "content": [{ "type": "input_text", "text": text }]
                            }
                        });
                        let create = serde_json::json!({"type": "response.create"});
                        let _ = ws_out_tx.send(tungstenite::Message::Text(item.to_string().into())).await;
                        let _ = ws_out_tx.send(tungstenite::Message::Text(create.to_string().into())).await;
                    }

                    Some(StsPipelineCmd::InjectFastContext { text }) => {
                        info!(session = %session_id, "Realtime STS: fast planner preempting agent audio");
                        ai_speaking.store(false, Ordering::SeqCst);
                        ai_source.clear_buffer();
                        let cancel = serde_json::json!({"type": "response.cancel"});
                        let _ = ws_out_tx.send(tungstenite::Message::Text(cancel.to_string().into())).await;
                        let item = serde_json::json!({"type":"conversation.item.create","item":{"type":"message","role":"assistant","content":[{"type":"input_text","text":text}]}});
                        let create = serde_json::json!({"type":"response.create"});
                        let _ = ws_out_tx.send(tungstenite::Message::Text(item.to_string().into())).await;
                        let _ = ws_out_tx.send(tungstenite::Message::Text(create.to_string().into())).await;
                    }

                    Some(StsPipelineCmd::Stop) => {
                        info!(session = %session_id, "Realtime STS: Stop / barge-in");
                        ai_speaking.store(false, Ordering::SeqCst);
                        ai_source.clear_buffer();
                        let cancel = serde_json::json!({"type": "response.cancel"});
                        let _ = ws_out_tx.send(tungstenite::Message::Text(cancel.to_string().into())).await;
                    }

                    Some(StsPipelineCmd::Shutdown) | None => {
                        info!(session = %session_id, "Realtime STS: shutdown");
                        ai_speaking.store(false, Ordering::SeqCst);
                        ai_source.clear_buffer();
                        let _ = room.close().await;
                        break;
                    }
                }
            }

            // ── messages from OpenAI Realtime ─────────────────────────────────
            ws_msg = ws_stream.next() => {
                match ws_msg {
                    Some(Ok(tungstenite::Message::Text(json_str))) => {
                        handle_event(
                            &json_str,
                            &ai_source,
                            &event_tx,
                            &recording_tx,
                            &session_id,
                            &ai_speaking,
                            sample_rate,
                        )
                        .await;
                    }
                    Some(Ok(_)) => {} // binary / ping / pong
                    Some(Err(e)) => {
                        error!(session = %session_id, "Realtime STS: WS error: {e}");
                        break;
                    }
                    None => {
                        warn!(session = %session_id, "Realtime STS: WS stream closed");
                        break;
                    }
                }
            }

            // ── LiveKit room events ───────────────────────────────────────────
            lk = room_events.recv() => {
                match lk {
                    Some(livekit::RoomEvent::TrackSubscribed { track, .. }) => {
                        if let livekit::track::RemoteTrack::Audio(audio) = track {
                            let id = audio.sid().to_string();
                            if attached.lock().unwrap().insert(id.clone()) {
                                info!(session = %session_id, track = %id, "Realtime STS: attaching track");
                                spawn_audio_forwarder(
                                    session_id.clone(),
                                    audio,
                                    pcm_tx.clone(),
                                    recording_tx.clone(),
                                    recording_started,
                                    sample_rate,
                                    flush_frames,
                                    cfg.vad_min_speech_amplitude,
                                );
                                if let Some(greeting) = greeting_pending.take() {
                                    queue_realtime_greeting(&ws_out_tx, &greeting, &session_id).await;
                                }
                            }
                        }
                    }
                    Some(livekit::RoomEvent::TrackUnsubscribed { track, .. }) => {
                        attached.lock().unwrap().remove(&track.sid().to_string());
                    }
                    Some(livekit::RoomEvent::Disconnected { reason }) => {
                        warn!(session = %session_id, "Realtime STS: LiveKit disconnected: {reason:?}");
                        break;
                    }
                    Some(_) => {}
                    None => break,
                }
            }
        }
    }
}

// ── OpenAI Realtime event handler ─────────────────────────────────────────────

async fn queue_realtime_greeting(
    output: &mpsc::Sender<tungstenite::Message>,
    greeting: &str,
    session_id: &str,
) {
    let item = serde_json::json!({
        "type": "conversation.item.create",
        "item": {
            "type": "message",
            "role": "assistant",
            "content": [{ "type": "input_text", "text": greeting }]
        }
    });
    let response = serde_json::json!({"type": "response.create"});
    if output
        .send(tungstenite::Message::Text(item.to_string().into()))
        .await
        .is_ok()
        && output
            .send(tungstenite::Message::Text(response.to_string().into()))
            .await
            .is_ok()
    {
        info!(session = %session_id, "Realtime STS: initial greeting queued after caller track connected");
    }
}

async fn handle_event(
    json_str: &str,
    ai_source: &NativeAudioSource,
    event_tx: &mpsc::Sender<StsPipelineEvent>,
    recording_tx: &mpsc::Sender<S3DumpData>,
    session_id: &str,
    ai_speaking: &Arc<AtomicBool>,
    sample_rate: u32,
) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) else {
        return;
    };

    match v["type"].as_str().unwrap_or("") {
        // ── AI audio output chunk ──────────────────────────────────────────────
        "response.audio.delta" => {
            let Some(b64) = v["delta"].as_str() else {
                return;
            };
            let Ok(pcm_bytes) = base64::engine::general_purpose::STANDARD.decode(b64) else {
                return;
            };
            let samples: Vec<i16> = pcm_bytes
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]))
                .collect();

            ai_speaking.store(true, Ordering::SeqCst);

            // Play through LiveKit NativeAudioSource → client hears the AI
            if let Err(e) = ai_source
                .capture_frame(&livekit::webrtc::audio_frame::AudioFrame {
                    data: std::borrow::Cow::Owned(samples.clone()),
                    sample_rate,
                    num_channels: 1,
                    samples_per_channel: samples.len() as u32,
                })
                .await
            {
                tracing::warn!(session_id, "capture_frame failed: {e:?}");
            }

            // Record AI audio into S3 dump pipeline (same as legacy path)
            let _ = recording_tx.try_send(S3DumpData::Audio {
                session_id: session_id.to_string(),
                samples,
                sample_rate: sample_rate,
                channels: 1,
                elapsed_samples: crate::models::elapsed_audio_samples(std::time::Instant::now()),
                speaker: AudioSpeaker::Assistant,
            });
        }

        // ── AI transcript chunk (streaming, word-by-word) ──────────────────────
        "response.audio_transcript.delta" => {
            if let Some(text) = v["delta"].as_str().filter(|s| !s.is_empty()) {
                let _ = event_tx
                    .send(StsPipelineEvent::AiTranscriptChunk {
                        text: text.to_string(),
                    })
                    .await;
            }
        }

        // ── AI audio response done ─────────────────────────────────────────────
        "response.audio.done" => {
            ai_speaking.store(false, Ordering::SeqCst);
            let _ = event_tx.send(StsPipelineEvent::AudioComplete).await;
            info!(session = %session_id, "Realtime STS: AI turn complete");
        }

        // ── User transcript (final, after server VAD silence) ──────────────────
        "conversation.item.input_audio_transcription.completed" => {
            let transcript = v["transcript"].as_str().unwrap_or("").trim().to_string();
            if !transcript.is_empty() {
                info!(session = %session_id, %transcript, "Realtime STS: user transcript");
                let _ = event_tx
                    .send(StsPipelineEvent::UserTranscriptChunk {
                        text: transcript.clone(),
                        is_final: true,
                        noise: None, // Realtime API handles its own VAD; no acoustic noise profile.
                    })
                    .await;
                let _ = recording_tx.try_send(S3DumpData::Text {
                    session_id: session_id.to_string(),
                    payload: format!("USER: {}", transcript),
                });
            }
        }

        // ── Server VAD: user started speaking (barge-in) ──────────────────────
        "input_audio_buffer.speech_started" => {
            info!(session = %session_id, "Realtime STS: user speaking (barge-in)");
            if ai_speaking.swap(false, Ordering::SeqCst) {
                ai_source.clear_buffer();
                let _ = event_tx.send(StsPipelineEvent::BargeIn).await;
            } else {
                // The provider can detect caller speech while local audio is
                // still buffered even though no delta is arriving right now.
                let _ = event_tx.send(StsPipelineEvent::UserSpeechStarted).await;
            }
        }

        // ── Error from OpenAI ─────────────────────────────────────────────────
        "error" => {
            let code = v["error"]["code"].as_str().unwrap_or("");
            let msg = v["error"]["message"].as_str().unwrap_or("unknown");
            error!(session = %session_id, %code, "Realtime STS: OpenAI error: {msg}");
            let _ = event_tx
                .send(StsPipelineEvent::Error(format!(
                    "OpenAI Realtime [{code}]: {msg}"
                )))
                .await;
        }

        // ── Informational ─────────────────────────────────────────────────────
        "session.created" | "session.updated" => {
            info!(session = %session_id, event = %v["type"].as_str().unwrap_or(""), "Realtime STS: session ready");
        }

        other => {
            tracing::trace!(session = %session_id, event = %other, "Realtime STS: event");
        }
    }
}

// ── LiveKit audio forwarder ────────────────────────────────────────────────────

/// Reads PCM16 at 24 kHz from a LiveKit RemoteAudioTrack, records it to the
/// S3 pipeline, and forwards batched chunks to the WebSocket audio encoder.
fn spawn_audio_forwarder(
    session_id: String,
    audio_track: livekit::track::RemoteAudioTrack,
    pcm_tx: mpsc::Sender<Vec<u8>>,
    recording_tx: mpsc::Sender<S3DumpData>,
    recording_started: std::time::Instant,
    sample_rate: u32,
    flush_frames: usize,
    noise_floor_amplitude: u16,
) {
    // Request 24 kHz mono — matches what OpenAI Realtime expects.
    let mut stream = NativeAudioStream::new(audio_track.rtc_track(), sample_rate as i32, 1);

    tokio::spawn(async move {
        let mut batch: Vec<i16> = Vec::new();
        let mut frames = 0usize;
        let mut rec_open = true;

        while let Some(frame) = stream.next().await {
            let data: &[i16] = frame.data.as_ref();
            if data.is_empty() {
                continue;
            }

            // Clean the microphone data before either recording or forwarding
            // it. This mirrors the normal STS noise floor and lets the
            // realtime service receive a usable interruption immediately.
            let clean_data: Vec<i16> = data
                .iter()
                .map(|sample| {
                    if sample.unsigned_abs() <= noise_floor_amplitude {
                        0
                    } else {
                        *sample
                    }
                })
                .collect();

            // Always record user audio to S3 (same as legacy sts_vad.rs)
            if rec_open {
                let pkt = S3DumpData::Audio {
                    session_id: session_id.clone(),
                    samples: clean_data.clone(),
                    sample_rate: frame.sample_rate,
                    channels: frame.num_channels as u16,
                    elapsed_samples: crate::models::elapsed_audio_samples(recording_started),
                    speaker: AudioSpeaker::User,
                };
                match recording_tx.try_send(pkt) {
                    Ok(()) => {}
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        tracing::trace!("Realtime STS: recording queue full, dropping frame");
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        warn!(session = %session_id, "Realtime STS: recording worker closed");
                        rec_open = false;
                    }
                }
            }

            // The remote track is caller input. It must continue to reach the
            // realtime WebSocket while assistant audio plays so server VAD can
            // detect barge-in and cancel the response. Dropping these frames
            // makes interruption impossible in STS_STREAM mode.
            batch.extend_from_slice(&clean_data);
            frames += 1;

            if frames >= flush_frames {
                frames = 0;
                let bytes: Vec<u8> = batch.iter().flat_map(|s| s.to_le_bytes()).collect();
                batch.clear();
                if pcm_tx.send(bytes).await.is_err() {
                    break; // WS sender dropped — session is ending
                }
            }
        }

        info!(session = %session_id, "Realtime STS: audio forwarder exited");
    });
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn non_empty<'a>(s: &'a str, default: &'a str) -> &'a str {
    if s.trim().is_empty() {
        default
    } else {
        s
    }
}

fn build_ws_request(url: &str, api_key: &str) -> Result<Request<()>, String> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let mut req = url
        .into_client_request()
        .map_err(|e| format!("invalid URL: {e}"))?;
    let h = req.headers_mut();
    h.insert(
        "Authorization",
        format!("Bearer {api_key}")
            .parse()
            .map_err(|e| format!("auth header: {e}"))?,
    );
    h.insert(
        "OpenAI-Beta",
        "realtime=v1"
            .parse()
            .map_err(|e| format!("beta header: {e}"))?,
    );
    Ok(req)
}
