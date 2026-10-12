use crate::models::{S3DumpData, StsPipelineCmd, StsPipelineEvent, WebRtcAudioChunk};
use futures_util::FutureExt;
use std::panic::AssertUnwindSafe;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

const WAV_OUTPUT_BYTES_PER_SECOND: u64 = 16_000 * 2 * 2;

pub async fn run_sts_pipeline(
    session_id: String,
    mut cmd_rx: mpsc::Receiver<StsPipelineCmd>,
    event_tx: mpsc::Sender<StsPipelineEvent>,
    webrtc_tx: mpsc::Sender<WebRtcAudioChunk>,
) {
    loop {
        let fut = AssertUnwindSafe(async {
            while let Some(cmd) = cmd_rx.recv().await {
                match cmd {
                    StsPipelineCmd::InjectContext { text } => {
                        println!(
                            "[STS Supervisor] Sending RAG Context to STS model: {}",
                            text
                        );

                        // Simulate STS text -> audio generation
                        // 1. Send live audio directly to WebRTC
                        let _ = webrtc_tx
                            .send(WebRtcAudioChunk {
                                session_id: session_id.clone(),
                                pcm_bytes: vec![0, 1, 2, 3], // Dummy PCM bytes
                            })
                            .await;

                        // 2. Send transcript events to Decision Engine
                        let _ = event_tx
                            .send(StsPipelineEvent::AiTranscriptChunk {
                                text: "Simulated speech transcript".to_string(),
                            })
                            .await;

                        let _ = event_tx.send(StsPipelineEvent::AudioComplete).await;
                    }
                    StsPipelineCmd::Stop => {
                        println!("[STS Supervisor] STOP COMMAND received. Halting WebRTC streaming instantly.");
                    }
                    StsPipelineCmd::Shutdown => {
                        info!("[STS Supervisor] Session pipeline shutting down.");
                        break;
                    }
                }
            }
        });

        if let Err(_) = fut.catch_unwind().await {
            eprintln!(
                "[STS Supervisor] Panic intercepted safely! Restarting STS pipeline supervisor."
            );
        } else {
            break;
        }
    }
}

pub async fn run_s3_dump_pipeline(session_id: String, mut rx: mpsc::Receiver<S3DumpData>) {
    let cfg = match config::AppConfig::load() {
        Ok(cfg) => Some(cfg),
        Err(config_error) => {
            error!(
                "Could not load R2 configuration for recording session {}: {}",
                session_id, config_error
            );
            None
        }
    };
    let mut audio_chunks = Vec::new();
    let mut audio_bytes = 0usize;
    let mut recording_limit_reached = false;
    let max_audio_bytes = cfg
        .as_ref()
        .map(|cfg| cfg.recording_max_buffer_bytes)
        .unwrap_or(0);
    let max_recording_samples = (max_audio_bytes as u64)
        .saturating_mul(16_000)
        .checked_div(WAV_OUTPUT_BYTES_PER_SECOND)
        .unwrap_or(0);
    loop {
        let fut = AssertUnwindSafe(async {
            while let Some(data) = rx.recv().await {
                match data {
                    S3DumpData::Text {
                        session_id: event_session,
                        payload,
                    } => {
                        info!(
                            "[S3 Dump Supervisor {}] Received conversation text ({} bytes)",
                            event_session,
                            payload.len()
                        );
                    }
                    audio @ S3DumpData::Audio { .. } => {
                        let (chunk_bytes, elapsed_samples) = match &audio {
                            S3DumpData::Audio {
                                samples,
                                elapsed_samples,
                                ..
                            } => (
                                samples.len().saturating_mul(std::mem::size_of::<i16>()),
                                *elapsed_samples,
                            ),
                            _ => (0, 0),
                        };
                        let next_size = audio_bytes.saturating_add(chunk_bytes);
                        if next_size <= max_audio_bytes && elapsed_samples <= max_recording_samples
                        {
                            audio_bytes = next_size;
                            audio_chunks.push(audio);
                        } else if !recording_limit_reached {
                            recording_limit_reached = true;
                            warn!("Recording buffer limit reached for session {} ({} bytes); dropping subsequent recording frames while keeping the live session unaffected.", session_id, max_audio_bytes);
                        }
                    }
                    S3DumpData::Finalize {
                        session_id: requested_session,
                        reply_to,
                    } => {
                        let result = if requested_session == session_id {
                            match (
                                cfg.as_ref(),
                                crate::r2_storage::build_conversation_wav(&mut audio_chunks),
                            ) {
                                (Some(cfg), Ok(wav)) => {
                                    crate::r2_storage::upload_conversation(cfg, &session_id, &wav)
                                        .await
                                }
                                (None, _) => Err(
                                    "R2 recording configuration is unavailable; check the server logs for the missing setting"
                                        .to_string(),
                                ),
                                (_, Err(error)) => Err(error),
                            }
                        } else {
                            Err("Recording session ID did not match the active session".to_string())
                        };
                        let _ = reply_to.send(result);
                        break;
                    }
                }
            }
        });

        if let Err(panic_error) = fut.catch_unwind().await {
            error!(
                "S3 dump supervisor for session {} panicked; recovering channel worker: {:?}",
                session_id, panic_error
            );
        } else {
            break;
        }
    }
}
