use crate::models::S3DumpData;
use crate::sts_vad::spawn_audio_track_processor;
use crate::tts_playback::{
    play_tts_stream, PlaybackClear, PlaybackGenerationGate, TtsPlaybackResult,
};
use futures_util::FutureExt;
use std::panic::AssertUnwindSafe;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

#[derive(Debug)]
struct PlaybackRequest {
    generation: u64,
    text: String,
}

pub use crate::models::{StsPipelineCmd, StsPipelineEvent};

const STT_AUDIO_QUEUE_CAPACITY: usize = 3;

pub struct StsPipelineWorker {
    cmd_tx: mpsc::Sender<StsPipelineCmd>,
}

impl StsPipelineWorker {
    pub fn spawn(
        session_id: String,
        event_tx: mpsc::Sender<StsPipelineEvent>,
        recording_tx: mpsc::Sender<S3DumpData>,
    ) -> Self {
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<StsPipelineCmd>(100);

        tokio::spawn(async move {
            let mut worker_active = true;

            while worker_active {
                let event_tx_clone = event_tx.clone();
                // Borrow the receiver so it's not moved into the AssertUnwindSafe block,
                // allowing us to reuse it across panics/restarts.
                let cmd_rx_ref = &mut cmd_rx;

                info!("Starting STS Pipeline Worker (LiveKit + Silero VAD)...");

                let result = AssertUnwindSafe(async {
                    let cfg = config::AppConfig::load().unwrap_or_else(|_| config::AppConfig::load_default());
                    let recording_started_at = std::time::Instant::now();
                    let token = livekit_api::access_token::AccessToken::with_api_key(&cfg.livekit_api_key, &cfg.livekit_api_secret)
                        .with_identity("AI-Voice-Agent")
                        .with_name("Real Estate Agent")
                        .with_grants(livekit_api::access_token::VideoGrants {
                            room_join: true,
                            room: session_id.clone(),
                            can_publish: Some(true),
                            can_subscribe: Some(true),
                            can_publish_data: Some(true),
                            ..Default::default()
                        })
                        .to_jwt().unwrap_or_default();

                    info!("STS Pipeline: Connecting AI Agent to LiveKit Room {}...", session_id);

                    let (room, mut room_rx) = match livekit::Room::connect(&cfg.livekit_url, &token, livekit::RoomOptions::default()).await {
                        Ok(res) => res,
                        Err(e) => {
                            error!("STS Pipeline: Failed to connect to LiveKit: {}", e);
                            return;
                        }
                    };

                    let stt = std::sync::Arc::new(crate::stt_pipeline::SttPipeline::with_model(
                        cfg.groq_api_key.clone(),
                        cfg.groq_stt_url.clone(),
                        cfg.stt_model.clone(),
                        cfg.stt_prompt.clone(),
                        cfg.stt_language.clone(),
                    ));
                    let tts = std::sync::Arc::new(crate::tts_pipeline::TtsPipeline::new(
                        cfg.openai_api_key.clone(),
                        cfg.openai_tts_url.clone(),
                        cfg.tts_model.clone(),
                        cfg.tts_fallback_model.clone(),
                        cfg.tts_voice.clone(),
                    ));

                    // Channel to receive transcription results from the backpressure audio worker
                    let (res_tx, mut res_rx) = mpsc::channel::<(String, crate::audio_gate::NoiseProfile)>(100);
                    let audio_tx = stt.clone().spawn_call_audio_worker(
                        session_id.clone(),
                        STT_AUDIO_QUEUE_CAPACITY,
                        res_tx,
                    );

                    let event_tx_transcript = event_tx_clone.clone();
                    tokio::spawn(async move {
                        while let Some((text, noise)) = res_rx.recv().await {
                            let _ = event_tx_transcript.send(StsPipelineEvent::UserTranscriptChunk {
                                text,
                                is_final: true,
                                noise: Some(noise),
                            }).await;
                        }
                    });

                    info!("STS Pipeline: Successfully joined Room '{}'", room.name());

                    // Create an audio track to send TTS back to the user
                    use livekit::webrtc::audio_source::native::NativeAudioSource;
                    use livekit::webrtc::audio_source::AudioSourceOptions;
                    use livekit::track::LocalAudioTrack;
                    use livekit::track::LocalTrack;
                    use livekit::options::TrackPublishOptions;
                    let tts_audio_source = NativeAudioSource::new(AudioSourceOptions::default(), 24000, 1, 40);
                    let tts_audio_track = LocalAudioTrack::create_audio_track("assistant-voice", livekit::prelude::RtcAudioSource::Native(tts_audio_source.clone()));
                    if let Err(e) = room.local_participant().publish_track(
                        LocalTrack::Audio(tts_audio_track),
                        TrackPublishOptions::default(),
                    ).await {
                        error!("STS Pipeline: Failed to publish assistant audio track: {}", e);
                    }

                    // Physically isolated ingress queues. Fast planner output
                    // has priority and can revoke only queued agent audio.
                    let (agent_tts_tx, mut agent_tts_rx) = mpsc::channel::<PlaybackRequest>(50);
                    let (fast_tts_tx, mut fast_tts_rx) = mpsc::channel::<PlaybackRequest>(16);
                    let (clear_tx, mut clear_rx) = mpsc::channel::<PlaybackClear>(10);
                    let tts_worker_source = tts_audio_source.clone();
                    let tts_worker_clone = tts.clone();
                    let recording_tx_tts = recording_tx.clone();
                    let session_id_tts = session_id.clone();
                    let recording_started_at_tts = recording_started_at;
                    let event_tx_complete = event_tx_clone.clone();

                    // Shared flag to duck VAD while AI is speaking
                    let ai_is_speaking = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let ai_is_speaking_tts = ai_is_speaking.clone();

                    tokio::spawn(async move {
                        info!("STS Pipeline: Spawning dedicated per-call TTS channel worker for call '{}'", session_id_tts);
                        let mut worker_running = true;
                        let mut active_generation = PlaybackGenerationGate::default();
                        while worker_running {
                            let worker_source_ref = tts_worker_source.clone();
                            let worker_tts_ref = tts_worker_clone.clone();
                                let agent_rx_ref = &mut agent_tts_rx;
                                let fast_rx_ref = &mut fast_tts_rx;
                            let clear_rx_ref = &mut clear_rx;
                            let ai_is_speaking_tts_ref = ai_is_speaking_tts.clone();
                            let recording_tx_ref = recording_tx_tts.clone();
                            let session_id_ref = session_id_tts.clone();
                            let event_tx_complete_ref = event_tx_complete.clone();

                            let result = AssertUnwindSafe(async move {
                                loop {
                                    let next = tokio::select! {
                                        biased;
                                        clear = clear_rx_ref.recv() => {
                                            match clear {
                                                Some(PlaybackClear::AgentOnly) => while agent_rx_ref.try_recv().is_ok() {},
                                                Some(PlaybackClear::All) => {
                                                    while agent_rx_ref.try_recv().is_ok() {}
                                                    while fast_rx_ref.try_recv().is_ok() {}
                                                }
                                                None => break,
                                            }
                                            worker_source_ref.clear_buffer();
                                            continue;
                                        }
                                        fast = fast_rx_ref.recv() => fast,
                                        agent = agent_rx_ref.recv() => agent,
                                    };
                                    let Some(request) = next else { break; };
                                    if !active_generation.accepts(request.generation) {
                                        tracing::debug!(request_generation = request.generation, active_generation = active_generation.current(), "dropping stale STS playback request");
                                        continue;
                                    }
                                    while active_generation.current() < request.generation {
                                        active_generation.preempt();
                                    }
                                    info!(generation = request.generation, chars = request.text.chars().count(), "STS Pipeline: TTS worker received source-isolated request");
                                    match play_tts_stream(
                                        &worker_tts_ref,
                                        &request.text,
                                        &worker_source_ref,
                                        clear_rx_ref,
                                        &ai_is_speaking_tts_ref,
                                        &recording_tx_ref,
                                        &session_id_ref,
                                        recording_started_at_tts,
                                    ).await {
                                        Ok(TtsPlaybackResult::Completed) => {
                                            let _ = event_tx_complete_ref.send(StsPipelineEvent::AudioComplete).await;
                                        }
                                        Ok(TtsPlaybackResult::Interrupted(PlaybackClear::AgentOnly)) => {
                                            while agent_rx_ref.try_recv().is_ok() {}
                                        }
                                        Ok(TtsPlaybackResult::Interrupted(PlaybackClear::All)) => {
                                            while agent_rx_ref.try_recv().is_ok() {}
                                            while fast_rx_ref.try_recv().is_ok() {}
                                        }
                                        Err(error) => {
                                            error!("STS Pipeline: TTS stream failed: {}", error);
                                            let _ = event_tx_complete_ref.send(StsPipelineEvent::Error(error)).await;
                                        }
                                    }
                                }
                            }).catch_unwind().await;

                            match result {
                                Ok(_) => {
                                    info!("STS Pipeline: TTS worker channel closed cleanly.");
                                    worker_running = false;
                                }
                                Err(panic_err) => {
                                    error!("CRITICAL: TTS worker loop PANICKED! Recovering... {:?}", panic_err);
                                    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                                }
                            }
                        }
                    });
                    // Wait for the caller's audio track before greeting. This
                    // prevents web/mobile clients that join just after the
                    // backend from missing audio played before they connect.
                    let mut greeting_pending = std::env::var("AI_GREETING")
                        .ok()
                        .filter(|greeting| !greeting.trim().is_empty());

                    let attached_tracks: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>> = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));

                    // Immediately inspect and attach to any pre-existing participant audio tracks
                    for (_sid, participant) in room.remote_participants() {
                        for (_tsid, publication) in participant.track_publications() {
                            if let Some(livekit::track::RemoteTrack::Audio(audio_track)) = publication.track() {
                                let track_sid = audio_track.sid().to_string();
                                if attached_tracks.lock().unwrap().insert(track_sid.clone()) {
                                    info!("STS Pipeline: Attaching to pre-existing remote audio track {} from participant {}", track_sid, participant.identity());
                                    let vad_params = crate::audio_gate::VadParams::from_config(&cfg);
                                    spawn_audio_track_processor(
                                        session_id.clone(),
                                        audio_track,
                                        audio_tx.clone(),
                                        event_tx_clone.clone(),
                                        recording_tx.clone(),
                                        ai_is_speaking.clone(),
                                        recording_started_at,
                                        vad_params,
                                    );
                                        if let Some(greeting) = greeting_pending.take() {
                                            if let Err(error) = agent_tts_tx.send(PlaybackRequest { generation: 0, text: greeting }).await {
                                                error!("STS Pipeline: Failed to queue configured greeting: {}", error);
                                            }
                                        }
                                }
                            }
                        }
                    }

                    let mut playback_generation = PlaybackGenerationGate::default();
                    loop {
                        tokio::select! {
                            cmd = cmd_rx_ref.recv() => {
                                match cmd {
                                    Some(StsPipelineCmd::InjectContext { text }) | Some(StsPipelineCmd::InjectAgentContext { text }) => {
                                        info!("STS Pipeline: queuing main-agent TTS response");
                                        if let Err(e) = agent_tts_tx.send(PlaybackRequest { generation: playback_generation.current(), text }).await {
                                            error!("STS Pipeline: Failed to queue TTS request: {}", e);
                                        }
                                    },
                                    Some(StsPipelineCmd::InjectFastContext { text }) => {
                                        // Fast planner output supersedes any unplayed main-agent
                                        // answer. Clear transport playback and queued TTS before
                                        // placing the clarification/acknowledgement on STS.
                                        info!("STS Pipeline: fast-planner speech preempting main-agent TTS");
                                        playback_generation.preempt();
                                        ai_is_speaking.store(false, std::sync::atomic::Ordering::SeqCst);
                                        tts_audio_source.clear_buffer();
                                        let _ = clear_tx.send(PlaybackClear::AgentOnly).await;
                                        if let Err(e) = fast_tts_tx.send(PlaybackRequest { generation: playback_generation.current(), text }).await {
                                            error!("STS Pipeline: Failed to queue fast-planner TTS request: {}", e);
                                        }
                                    },
                                     Some(StsPipelineCmd::Stop) => {
                                        info!("STS Pipeline: Barge-in detected! Stopping TTS instantly and clearing queues.");
                                        playback_generation.preempt();
                                        ai_is_speaking.store(false, std::sync::atomic::Ordering::SeqCst);
                                        tts_audio_source.clear_buffer();
                                        // The clear signal is ordered and must not be
                                        // dropped: it drains queued TTS text before the
                                        // next user turn is allowed to reach STS.
                                        let _ = clear_tx.send(PlaybackClear::All).await;
                                    },
                                    Some(StsPipelineCmd::Shutdown) => {
                                        info!("STS Pipeline: Shutting down session {}", session_id);
                                        ai_is_speaking.store(false, std::sync::atomic::Ordering::SeqCst);
                                        tts_audio_source.clear_buffer();
                                        let _ = clear_tx.send(PlaybackClear::All).await;
                                        let _ = room.close().await;
                                        worker_active = false;
                                        break;
                                    },
                                    None => {
                                        info!("STS Pipeline cmd channel closed");
                                        let _ = room.close().await;
                                        break;
                                    },
                                }
                            }
                            event = room_rx.recv() => {
                                match event {
                                    Some(livekit::RoomEvent::TrackSubscribed { track, publication: _, participant: _ }) => {
                                        info!("STS Pipeline: Subscribed to remote track {}", track.sid());
                                        if let livekit::track::RemoteTrack::Audio(audio_track) = track {
                                            let track_sid = audio_track.sid().to_string();
                                            if attached_tracks.lock().unwrap().insert(track_sid.clone()) {
                                                info!("STS Pipeline: Audio track {} received, attaching NativeAudioStream for live STT transcription...", track_sid);
                                                let vad_params = crate::audio_gate::VadParams::from_config(&cfg);
                                                spawn_audio_track_processor(
                                                    session_id.clone(),
                                                    audio_track,
                                                    audio_tx.clone(),
                                                    event_tx_clone.clone(),
                                                    recording_tx.clone(),
                                                    ai_is_speaking.clone(),
                                                    recording_started_at,
                                                    vad_params,
                                                );
                                                if let Some(greeting) = greeting_pending.take() {
                                                    if let Err(error) = agent_tts_tx.send(PlaybackRequest { generation: playback_generation.current(), text: greeting }).await {
                                                        error!("STS Pipeline: Failed to queue configured greeting: {}", error);
                                                    }
                                                }
                                            } else {
                                                info!("STS Pipeline: Audio track {} already attached, ignoring duplicate subscription event.", track_sid);
                                            }
                                        }
                                    }
                                    Some(livekit::RoomEvent::TrackUnsubscribed { track, publication: _, participant: _ }) => {
                                        let track_sid = track.sid().to_string();
                                        attached_tracks.lock().unwrap().remove(&track_sid);
                                        info!("STS Pipeline: Audio track {} unsubscribed, removed from tracker.", track_sid);
                                    }
                                    Some(livekit::RoomEvent::TrackPublished { publication, participant: _ }) => {
                                        info!("STS Pipeline: Remote track published: {} (waiting for subscription event)", publication.sid());
                                    }
                                    Some(livekit::RoomEvent::Disconnected { reason }) => {
                                        warn!("STS Pipeline: Disconnected from LiveKit room: {:?}", reason);
                                        break;
                                    }
                                    Some(_) => {
                                        // Ignore other events for now
                                    }
                                    None => {
                                        info!("STS Pipeline: Room event channel closed");
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }).catch_unwind().await;

                match result {
                    Ok(_) => {
                        info!("STS Pipeline Worker exited gracefully.");
                        worker_active = false;
                    }
                    Err(err) => {
                        error!("CRITICAL: STS Pipeline Worker PANICKED via AssertUnwindSafe! Restarting in 500ms... {:?}", err);
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

    pub fn get_cmd_tx(&self) -> mpsc::Sender<StsPipelineCmd> {
        self.cmd_tx.clone()
    }
}
