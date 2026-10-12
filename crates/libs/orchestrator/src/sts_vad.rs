use futures_util::StreamExt;
use livekit::webrtc::audio_stream::native::NativeAudioStream;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

const MODEL_AUDIO_NOISE_FLOOR_AMPLITUDE: u16 = 100;

pub fn spawn_audio_track_processor(
    session_id: String,
    audio_track: livekit::track::RemoteAudioTrack,
    audio_tx: mpsc::Sender<crate::stt_pipeline::SttAudioChunk>,
    event_tx: mpsc::Sender<crate::models::StsPipelineEvent>,
    recording_tx: mpsc::Sender<crate::models::S3DumpData>,
    ai_is_speaking: std::sync::Arc<std::sync::atomic::AtomicBool>,
    recording_started_at: std::time::Instant,
    speech_probability_threshold: f32,
    barge_in_probability_threshold: f32,
    min_speech_amplitude: u16,
    energy_bypass_amplitude: u16,
    barge_in_amplitude: u16,
    end_of_speech_timeout_ms: u64,
) {
    let mut audio_stream = NativeAudioStream::new(audio_track.rtc_track(), 16000, 1);

    tokio::spawn(async move {
        use silero_vad_pure::{SampleRate, SileroVad};

        let mut vad = match SileroVad::new(SampleRate::Hz16000) {
            Ok(v) => v,
            Err(e) => {
                error!("STS Pipeline: Failed to initialize Silero VAD: {}", e);
                return;
            }
        };

        let mut audio_buffer: Vec<i16> = Vec::new();
        let mut pre_roll_buffer: std::collections::VecDeque<i16> =
            std::collections::VecDeque::new();
        let mut vad_chunk_buffer: Vec<f32> = Vec::with_capacity(512);

        let mut silence_started_at: Option<std::time::Instant> = None;
        let mut is_speaking = false;
        let mut consecutive_barge_in_chunks = 0;

        // VAD parameters
        let max_pre_roll_samples = 3200; // 200ms at 16000Hz
        let max_buffer_samples = 16000 * 15; // 15 seconds max

        let mut last_sample_rate = 16000;
        let mut last_channels = 1;
        let mut frame_count = 0;
        let mut recording_channel_open = true;

        let mut next_frame_fut = Box::pin(audio_stream.next());

        loop {
            tokio::select! {
                frame_opt = &mut next_frame_fut => {
                    next_frame_fut = Box::pin(audio_stream.next());
                    match frame_opt {
                        Some(frame) => {
                            last_sample_rate = frame.sample_rate;
                            last_channels = frame.num_channels as u16;
                            let frame_data: &[i16] = frame.data.as_ref();
                            if frame_data.is_empty() { continue; }
                            // Zero the low-level noise floor before VAD, STT, or recording sees this frame.
                            let filtered_frame_data: Vec<i16> = frame_data
                                .iter()
                                .map(|sample| {
                                    if sample.unsigned_abs() <= MODEL_AUDIO_NOISE_FLOOR_AMPLITUDE {
                                        0
                                    } else {
                                        *sample
                                    }
                                })
                                .collect();

                            if recording_channel_open {
                                let packet = crate::models::S3DumpData::Audio {
                                    session_id: session_id.clone(),
                                    samples: filtered_frame_data.clone(),
                                    sample_rate: frame.sample_rate,
                                    channels: frame.num_channels as u16,
                                    elapsed_samples: crate::models::elapsed_audio_samples(
                                        recording_started_at,
                                    ),
                                    speaker: crate::models::AudioSpeaker::User,
                                };
                                match recording_tx.try_send(packet) {
                                    Ok(()) => {}
                                    Err(mpsc::error::TrySendError::Full(_)) => {
                                        tracing::trace!("Recording queue full; dropping user audio frame.");
                                    }
                                    Err(mpsc::error::TrySendError::Closed(_)) => {
                                        warn!("Recording worker closed; continuing speech recognition without recording.");
                                        recording_channel_open = false;
                                    }
                                }
                            }

                            frame_count += 1;
                            if frame_count % 100 == 0 {
                                let max_amp = frame_data
                                    .iter()
                                    .map(|sample| sample.unsigned_abs())
                                    .max()
                                    .unwrap_or(0);
                                info!("STS Pipeline: Received audio frame #{} (Active WebRTC stream, max_amp: {})", frame_count, max_amp);
                            }

                            // Convert to f32 and feed into vad chunk buffer
                            for &sample in &filtered_frame_data {
                                vad_chunk_buffer.push(sample as f32 / 32768.0);
                            }

                            // Keep filtered microphone audio in pre-roll/audio_buffer for STT.
                            let ai_speaking_current = ai_is_speaking.load(std::sync::atomic::Ordering::SeqCst);
                            if ai_speaking_current {
                                // While AI is playing TTS, keep pre-roll clean to avoid capturing speaker echo
                                pre_roll_buffer.clear();
                                if is_speaking {
                                    audio_buffer.extend_from_slice(&filtered_frame_data);
                                }
                            } else if is_speaking {
                                audio_buffer.extend_from_slice(&filtered_frame_data);
                            } else {
                                pre_roll_buffer.extend(filtered_frame_data.iter().copied());
                                while pre_roll_buffer.len() > max_pre_roll_samples {
                                    pre_roll_buffer.pop_front();
                                }
                            }

                            while vad_chunk_buffer.len() >= 512 {
                                let chunk: Vec<f32> = vad_chunk_buffer.drain(0..512).collect();

                                let max_chunk_amp = chunk.iter().map(|x| x.abs()).max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap_or(0.0) * 32768.0;

                                match vad.process(&chunk) {
                                    Ok(speech_prob) => {
                                        let ai_speaking = ai_is_speaking.load(std::sync::atomic::Ordering::SeqCst);

                                        // Require configured minimum energy before speech can open an STT request.
                                        let is_speech_chunk = if ai_speaking {
                                            max_chunk_amp > min_speech_amplitude as f32
                                                && ((speech_prob > barge_in_probability_threshold
                                                    && max_chunk_amp > barge_in_amplitude as f32)
                                                    || max_chunk_amp > energy_bypass_amplitude as f32)
                                        } else {
                                            max_chunk_amp > min_speech_amplitude as f32
                                                && (speech_prob > speech_probability_threshold
                                                    || max_chunk_amp > energy_bypass_amplitude as f32)
                                        };

                                        if is_speech_chunk {
                                            if ai_speaking {
                                                consecutive_barge_in_chunks += 1;
                                                // Require 3 consecutive frames (~96ms) to confirm genuine user interruption, not echo click
                                                if consecutive_barge_in_chunks >= 3 {
                                                    if !is_speaking {
                                                        info!("STS Pipeline: Genuine barge-in confirmed (Prob: {:.3}, Max Amp: {:.1}) -> stopping AI playback", speech_prob, max_chunk_amp);
                                                        let _ = event_tx.try_send(crate::models::StsPipelineEvent::BargeIn);
                                                        audio_buffer.clear();
                                                        is_speaking = true;
                                                    }
                                                }
                                            } else {
                                                consecutive_barge_in_chunks = 0;
                                                if !is_speaking {
                                                    info!("STS Pipeline: VAD Speech started (Probability: {:.3}, Max Amp: {:.1})", speech_prob, max_chunk_amp);
                                                    audio_buffer.extend(pre_roll_buffer.iter().copied());
                                                    pre_roll_buffer.clear();
                                                    is_speaking = true;
                                                }
                                            }
                                            silence_started_at = None;
                                        } else {
                                            consecutive_barge_in_chunks = 0;
                                            if is_speaking && silence_started_at.is_none() {
                                                silence_started_at = Some(std::time::Instant::now());
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        error!("STS Pipeline: Silero VAD processing error: {}", e);
                                    }
                                }
                            }
                        }
                        None => break,
                    }
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(25)) => {}
            }

            let is_over_timeout = silence_started_at
                .map(|started_at| {
                    started_at.elapsed().as_millis() >= end_of_speech_timeout_ms as u128
                })
                .unwrap_or(false);
            let is_too_long = audio_buffer.len() >= max_buffer_samples;

            if is_speaking && (is_over_timeout || is_too_long) {
                // Send to STT only if utterance is long enough (e.g. > 200ms to allow "Stop" / "No")
                if audio_buffer.len() >= 3200 {
                    // at least 200ms of audio
                    for sample in &mut audio_buffer {
                        if sample.unsigned_abs() <= min_speech_amplitude {
                            *sample = 0;
                        }
                    }
                    let chunk = crate::stt_pipeline::SttAudioChunk {
                        pcm_data: std::mem::take(&mut audio_buffer),
                        sample_rate: last_sample_rate,
                        channels: last_channels,
                        model_override: None,
                    };
                    info!(
                        "STS Pipeline: Silero VAD triggered, sending chunk of {} samples for STT",
                        chunk.pcm_data.len()
                    );
                    match audio_tx.try_send(chunk) {
                        Ok(()) => {}
                        Err(mpsc::error::TrySendError::Full(_)) => {
                            warn!("STS Pipeline: STT queue full for session {}; dropping completed utterance to keep LiveKit audio ingestion responsive.", session_id);
                        }
                        Err(mpsc::error::TrySendError::Closed(_)) => {
                            warn!("STS Pipeline: STT worker closed for session {}; stopping audio stream listener.", session_id);
                            break;
                        }
                    }
                } else {
                    info!(
                        "STS Pipeline: VAD Speech ended but ignored short utterance ({} samples)",
                        audio_buffer.len()
                    );
                    audio_buffer.clear();
                }
                is_speaking = false;
                silence_started_at = None;
                consecutive_barge_in_chunks = 0;
                vad.reset(); // CRITICAL: Reset neural net hidden state after utterance
            }
        }
    });
}
