use futures_util::StreamExt;
use livekit::webrtc::audio_stream::native::NativeAudioStream;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::audio_gate::{
    classify_barge_in_chunk, classify_chunk, count_speech_in_window, is_loud_noise_window,
    ChunkClass, NoiseProfile, VadParams,
};

/// Convert the raw VAD chunk to floor-gated PCM for the STT channel. This
/// preserves the onset of a user turn that begins while assistant audio is
/// playing without sending sub-floor samples downstream.
fn clean_vad_chunk_to_pcm(chunk: &[f32], amplitude_floor: u16) -> Vec<i16> {
    chunk
        .iter()
        .map(|sample| {
            let pcm = (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
            if pcm.unsigned_abs() < amplitude_floor {
                0
            } else {
                pcm
            }
        })
        .collect()
}

pub fn spawn_audio_track_processor(
    session_id: String,
    audio_track: livekit::track::RemoteAudioTrack,
    audio_tx: mpsc::Sender<crate::stt_pipeline::SttAudioChunk>,
    event_tx: mpsc::Sender<crate::models::StsPipelineEvent>,
    recording_tx: mpsc::Sender<crate::models::S3DumpData>,
    ai_is_speaking: std::sync::Arc<std::sync::atomic::AtomicBool>,
    recording_started_at: std::time::Instant,
    params: VadParams,
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

        // ── Buffers ──────────────────────────────────────────────────────────
        let mut audio_buffer: Vec<i16> = Vec::new();
        let mut pre_roll_buffer: std::collections::VecDeque<i16> =
            std::collections::VecDeque::new();
        let mut vad_chunk_buffer: Vec<f32> = Vec::with_capacity(512);

        // ── State ─────────────────────────────────────────────────────────────
        let mut silence_started_at: Option<std::time::Instant> = None;
        let mut is_speaking = false;
        let mut consecutive_barge_in_chunks: u32 = 0;

        // ── Analysis window (1.0–1.3 s rolling window of 32 ms chunks) ──────
        // 1200 ms / 32 ms ≈ 37 chunk slots.
        let window_capacity = (params.analysis_window_ms / 32).max(8) as usize;
        let mut window: std::collections::VecDeque<ChunkClass> =
            std::collections::VecDeque::with_capacity(window_capacity);

        // Per-utterance noise profiling accumulators.
        let mut utt_chunk_classes: Vec<ChunkClass> = Vec::new();
        let mut utt_speech_probs: Vec<f32> = Vec::new();
        let mut utt_chunk_rms: Vec<f32> = Vec::new();
        let mut utt_total_samples: usize = 0;
        let mut utt_clipped_samples: usize = 0;

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
                            let frame_peak = frame_data
                                .iter()
                                .map(|sample| sample.unsigned_abs())
                                .max()
                                .unwrap_or(0);
                            // A sub-floor frame is silence/noise for this
                            // session. It is deliberately kept out of every
                            // downstream audio path, including neural VAD,
                            // recording, and STT. The end-of-speech timer still
                            // advances so an already-open caller turn completes.
                            let frame_is_above_floor = frame_peak > params.min_speech_amplitude;

                            // Keep the original waveform for Silero.  An amplitude floor is
                            // a policy decision, not a denoiser: zeroing samples before the
                            // neural VAD removes speech detail and can make a real caller
                            // indistinguishable from silence.  The floor is still applied to
                            // recorded/STT audio and by `classify_*` below, so sub-floor sound
                            // cannot open a turn or cause a barge-in.
                            let filtered_frame_data: Vec<i16> = frame_data
                                .iter()
                                .map(|sample| {
                                    if sample.unsigned_abs() < params.min_speech_amplitude {
                                        0
                                    } else {
                                        *sample
                                    }
                                })
                                .collect();

                            if recording_channel_open && frame_is_above_floor {
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
                                info!(
                                    frame = frame_count,
                                    raw_peak = frame_peak,
                                    configured_floor = params.min_speech_amplitude,
                                    "STS Pipeline: received active WebRTC audio"
                                );
                            }

                            // Feed the unmodified waveform to Silero; use the separately
                            // gated buffer for STT after a human-speech turn is confirmed.
                            if frame_is_above_floor {
                                for &sample in frame_data {
                                    vad_chunk_buffer.push(sample as f32 / 32768.0);
                                }
                            } else if is_speaking && silence_started_at.is_none() {
                                silence_started_at = Some(std::time::Instant::now());
                            }

                            // Maintain audio buffers for STT.
                            let ai_speaking_current = ai_is_speaking.load(std::sync::atomic::Ordering::SeqCst);
                            if frame_is_above_floor && ai_speaking_current {
                                // While AI is playing TTS, keep pre-roll clean to avoid speaker echo.
                                pre_roll_buffer.clear();
                                if is_speaking {
                                    audio_buffer.extend_from_slice(&filtered_frame_data);
                                }
                            } else if frame_is_above_floor && is_speaking {
                                audio_buffer.extend_from_slice(&filtered_frame_data);
                            } else if frame_is_above_floor {
                                pre_roll_buffer.extend(filtered_frame_data.iter().copied());
                                while pre_roll_buffer.len() > max_pre_roll_samples {
                                    pre_roll_buffer.pop_front();
                                }
                            }

                            // ── Process 32ms (512-sample) Silero chunks ───────────────────
                            while vad_chunk_buffer.len() >= 512 {
                                let chunk: Vec<f32> = vad_chunk_buffer.drain(0..512).collect();
                                let clean_pcm_chunk =
                                    clean_vad_chunk_to_pcm(&chunk, params.min_speech_amplitude);

                                // Peak and RMS for this chunk (on the f32 scale 0.0–1.0).
                                let max_chunk_amp = chunk
                                    .iter()
                                    .map(|x| x.abs())
                                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                                    .unwrap_or(0.0);

                                let rms = {
                                    let sum_sq: f32 = chunk.iter().map(|x| x * x).sum();
                                    (sum_sq / chunk.len() as f32).sqrt()
                                };

                                // Clipped samples in this chunk (using u16 scale).
                                let chunk_clipped = chunk
                                    .iter()
                                    .filter(|x| ((*x * 32768.0) as u16) >= params.clip_level)
                                    .count();

                                match vad.process(&chunk) {
                                    Ok(speech_prob) => {
                                        let ai_speaking = ai_is_speaking.load(std::sync::atomic::Ordering::SeqCst);

                                        // Classify this chunk using the noise gate.
                                        let chunk_class = if ai_speaking {
                                            classify_barge_in_chunk(max_chunk_amp, speech_prob, &params)
                                        } else {
                                            classify_chunk(max_chunk_amp, speech_prob, &params)
                                        };

                                        // Maintain rolling analysis window.
                                        if window.len() >= window_capacity {
                                            window.pop_front();
                                        }
                                        window.push_back(chunk_class);

                                        // Accumulate utterance profiling data.
                                        if is_speaking {
                                            utt_chunk_classes.push(chunk_class);
                                            utt_speech_probs.push(speech_prob);
                                            utt_chunk_rms.push(rms);
                                            utt_total_samples += chunk.len();
                                            utt_clipped_samples += chunk_clipped;
                                        }

                                        // ── Turn-open logic (user is not currently speaking) ────
                                        if !is_speaking && ai_speaking {
                                            // Barge-in must not wait for the normal
                                            // 1.2s turn-open window. That window is
                                            // designed for clean user turns, while
                                            // interruption should react after a
                                            // short consecutive speech run.
                                            if chunk_class == ChunkClass::Speech {
                                                consecutive_barge_in_chunks = consecutive_barge_in_chunks.saturating_add(1);
                                            } else {
                                                consecutive_barge_in_chunks = 0;
                                            }
                                            if consecutive_barge_in_chunks >= params.barge_in_min_chunks.max(1) {
                                                info!(
                                                    "STS Pipeline: Genuine barge-in confirmed (Prob: {:.3}, Max Amp: {:.1}) -> stopping AI playback",
                                                    speech_prob,
                                                    max_chunk_amp * 32768.0
                                                );
                                                // Do not drop the interrupt signal under mailbox
                                                // pressure: the session must cancel stale TTS before
                                                // this clean user turn is committed.
                                                if event_tx.send(crate::models::StsPipelineEvent::BargeIn).await.is_err() {
                                                    return;
                                                }
                                                audio_buffer.clear();
                                                // While assistant speech was active, pre-roll is
                                                // deliberately empty to avoid echo. Preserve the
                                                // VAD-confirmed user onset instead, otherwise the
                                                // first words of an interruption are lost before the
                                                // audio reaches the STT channel.
                                                audio_buffer.extend_from_slice(&clean_pcm_chunk);
                                                is_speaking = true;
                                                consecutive_barge_in_chunks = 0;
                                                utt_chunk_classes.clear();
                                                utt_speech_probs.clear();
                                                utt_chunk_rms.clear();
                                                utt_chunk_classes.push(chunk_class);
                                                utt_speech_probs.push(speech_prob);
                                                utt_chunk_rms.push(rms);
                                                utt_total_samples = chunk.len();
                                                utt_clipped_samples = chunk_clipped;
                                            }
                                            silence_started_at = None;
                                        } else if !is_speaking {
                                            // A turn opens only if the window has enough genuine speech
                                            // AND is not dominated by very-loud noise.
                                            let window_slice: Vec<ChunkClass> =
                                                window.iter().copied().collect();
                                            let speech_in_window =
                                                count_speech_in_window(&window_slice);
                                            let dominated_by_loud =
                                                is_loud_noise_window(&window_slice, params.max_loud_ratio);

                                            if speech_in_window >= params.min_speech_chunks_in_window
                                                && !dominated_by_loud
                                            {
                                                // Normal turn start.
                                                consecutive_barge_in_chunks = 0;
                                                info!(
                                                    "STS Pipeline: VAD Speech started (Probability: {:.3}, Max Amp: {:.1})",
                                                    speech_prob,
                                                    max_chunk_amp * 32768.0
                                                );
                                                audio_buffer.extend(pre_roll_buffer.iter().copied());
                                                pre_roll_buffer.clear();
                                                is_speaking = true;
                                                // Playback can remain buffered in LiveKit after the
                                                // TTS HTTP stream has completed. Notify the session
                                                // at the first confirmed caller turn so it clears
                                                // that stale audio immediately.
                                                if event_tx
                                                    .send(crate::models::StsPipelineEvent::UserSpeechStarted)
                                                    .await
                                                    .is_err()
                                                {
                                                    return;
                                                }
                                                // Reset utterance profiling.
                                                utt_chunk_classes.clear();
                                                utt_speech_probs.clear();
                                                utt_chunk_rms.clear();
                                                utt_total_samples = 0;
                                                utt_clipped_samples = 0;
                                            } else if chunk_class == ChunkClass::LoudNoise {
                                                // Very loud chunk resets the barge-in counter so the AI keeps speaking.
                                                if consecutive_barge_in_chunks > 0 {
                                                    tracing::debug!(
                                                        "STS Pipeline: LoudNoise chunk reset barge-in counter (amp: {:.1})",
                                                        max_chunk_amp * 32768.0
                                                    );
                                                }
                                                consecutive_barge_in_chunks = 0;
                                            } else {
                                                consecutive_barge_in_chunks = 0;
                                            }
                                            silence_started_at = None;
                                        } else {
                                            // Already speaking — update end-of-speech timer.
                                            if chunk_class == ChunkClass::Speech || chunk_class == ChunkClass::QuietNoise {
                                                // Any detectable sound resets the silence timer.
                                                if chunk_class == ChunkClass::Speech {
                                                    silence_started_at = None;
                                                } else if silence_started_at.is_none() {
                                                    silence_started_at = Some(std::time::Instant::now());
                                                }
                                            } else if silence_started_at.is_none() {
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
                    started_at.elapsed().as_millis() >= params.end_of_speech_timeout_ms as u128
                })
                .unwrap_or(false);
            let is_too_long = audio_buffer.len() >= max_buffer_samples;

            if is_speaking && (is_over_timeout || is_too_long) {
                // Send to STT only if utterance is long enough (> 200ms).
                if audio_buffer.len() >= 3200 {
                    // Compute noise profile for this utterance.
                    let noise_profile = NoiseProfile::compute(
                        &utt_chunk_classes,
                        &utt_speech_probs,
                        &utt_chunk_rms,
                        utt_total_samples,
                        utt_clipped_samples,
                        &params,
                    );
                    info!(
                        "STS Pipeline: Utterance complete — noise level={:?} speech_ratio={:.2} loud_ratio={:.2} sustained={} impulsive={}",
                        noise_profile.level,
                        noise_profile.speech_ratio,
                        noise_profile.loud_noise_ratio,
                        noise_profile.sustained_tone,
                        noise_profile.impulsive,
                    );

                    // Zero samples below the min amplitude floor before STT.
                    for sample in &mut audio_buffer {
                        if sample.unsigned_abs() <= params.min_speech_amplitude {
                            *sample = 0;
                        }
                    }

                    let chunk = crate::stt_pipeline::SttAudioChunk {
                        pcm_data: std::mem::take(&mut audio_buffer),
                        sample_rate: last_sample_rate,
                        channels: last_channels,
                        model_override: None,
                        noise: noise_profile,
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

                // Reset state.
                is_speaking = false;
                silence_started_at = None;
                consecutive_barge_in_chunks = 0;
                utt_chunk_classes.clear();
                utt_speech_probs.clear();
                utt_chunk_rms.clear();
                utt_total_samples = 0;
                utt_clipped_samples = 0;
                vad.reset(); // CRITICAL: Reset neural net hidden state after utterance.
            }
        }
    });
}
