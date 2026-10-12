use crate::models::{elapsed_audio_samples, AudioSpeaker, S3DumpData};
use crate::tts_pipeline::{TtsPipeline, VoiceEnhancer};
use futures_util::StreamExt;
use livekit::webrtc::audio_frame::AudioFrame;
use livekit::webrtc::audio_source::native::NativeAudioSource;
use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{info, trace, warn};

const PCM_SAMPLES_PER_FRAME: usize = 240;
const PCM_BYTES_PER_FRAME: usize = PCM_SAMPLES_PER_FRAME * 2;

pub enum TtsPlaybackResult {
    Completed,
    Interrupted,
}

struct SpeakingGuard(Arc<AtomicBool>);

impl Drop for SpeakingGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

pub async fn play_tts_stream(
    pipeline: &TtsPipeline,
    text: &str,
    audio_source: &NativeAudioSource,
    clear_rx: &mut mpsc::Receiver<()>,
    tts_rx: &mut mpsc::Receiver<String>,
    ai_is_speaking: &Arc<AtomicBool>,
    recording_tx: &mpsc::Sender<S3DumpData>,
    session_id: &str,
    recording_started_at: Instant,
) -> Result<TtsPlaybackResult, String> {
    let request_started_at = Instant::now();
    let response = tokio::select! {
        clear = clear_rx.recv() => {
            if clear.is_some() {
                clear_playback(audio_source, tts_rx);
                return Ok(TtsPlaybackResult::Interrupted);
            }
            return Err("TTS interrupt channel closed".to_string());
        }
        response = pipeline.open_audio_stream(text) => response?,
    };
    info!(
        "TTS response headers received in {} ms ({} text chars)",
        request_started_at.elapsed().as_millis(),
        text.chars().count()
    );

    ai_is_speaking.store(true, Ordering::SeqCst);
    let _speaking_guard = SpeakingGuard(ai_is_speaking.clone());
    let result = stream_response(
        response,
        audio_source,
        clear_rx,
        tts_rx,
        recording_tx,
        session_id,
        recording_started_at,
        request_started_at,
    )
    .await;
    info!(
        "TTS stream finished after {} ms",
        request_started_at.elapsed().as_millis()
    );
    result
}

async fn stream_response(
    response: reqwest::Response,
    audio_source: &NativeAudioSource,
    clear_rx: &mut mpsc::Receiver<()>,
    tts_rx: &mut mpsc::Receiver<String>,
    recording_tx: &mpsc::Sender<S3DumpData>,
    session_id: &str,
    recording_started_at: Instant,
    request_started_at: Instant,
) -> Result<TtsPlaybackResult, String> {
    let mut audio_stream = response.bytes_stream();
    let mut pending_bytes = std::collections::VecDeque::new();
    let mut enhancer = VoiceEnhancer::default();
    let mut first_frame_sent = false;

    loop {
        let next_chunk = tokio::select! {
            clear = clear_rx.recv() => {
                if clear.is_some() {
                    clear_playback(audio_source, tts_rx);
                    return Ok(TtsPlaybackResult::Interrupted);
                }
                return Err("TTS interrupt channel closed".to_string());
            }
            chunk = audio_stream.next() => chunk,
        };

        let Some(chunk) = next_chunk else {
            break;
        };
        let chunk = chunk.map_err(|error| format!("TTS audio stream failed: {}", error))?;
        pending_bytes.extend(chunk);

        while pending_bytes.len() >= PCM_BYTES_PER_FRAME {
            if clear_rx.try_recv().is_ok() {
                clear_playback(audio_source, tts_rx);
                return Ok(TtsPlaybackResult::Interrupted);
            }
            let frame_bytes: Vec<u8> = pending_bytes.drain(..PCM_BYTES_PER_FRAME).collect();
            play_pcm_frame(
                frame_bytes,
                &mut enhancer,
                audio_source,
                recording_tx,
                session_id,
                recording_started_at,
            )
            .await?;
            log_first_frame_latency(&mut first_frame_sent, request_started_at);
        }
    }

    if !pending_bytes.is_empty() {
        if pending_bytes.len() % 2 != 0 {
            pending_bytes.pop_back();
        }
        if !pending_bytes.is_empty() {
            pending_bytes.resize(PCM_BYTES_PER_FRAME, 0);
            play_pcm_frame(
                pending_bytes.into_iter().collect(),
                &mut enhancer,
                audio_source,
                recording_tx,
                session_id,
                recording_started_at,
            )
            .await?;
            log_first_frame_latency(&mut first_frame_sent, request_started_at);
        }
    }

    Ok(TtsPlaybackResult::Completed)
}

fn log_first_frame_latency(first_frame_sent: &mut bool, request_started_at: Instant) {
    if *first_frame_sent {
        return;
    }
    *first_frame_sent = true;
    info!(
        "TTS first PCM frame submitted in {} ms",
        request_started_at.elapsed().as_millis()
    );
}

async fn play_pcm_frame(
    frame_bytes: Vec<u8>,
    enhancer: &mut VoiceEnhancer,
    audio_source: &NativeAudioSource,
    recording_tx: &mpsc::Sender<S3DumpData>,
    session_id: &str,
    recording_started_at: Instant,
) -> Result<(), String> {
    let mut samples: Vec<i16> = frame_bytes
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    enhancer.process(&mut samples);

    match recording_tx.try_send(S3DumpData::Audio {
        session_id: session_id.to_string(),
        samples: samples.clone(),
        sample_rate: 24_000,
        channels: 1,
        elapsed_samples: elapsed_audio_samples(recording_started_at),
        speaker: AudioSpeaker::Assistant,
    }) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(_)) => {
            trace!("Recording queue full; dropping assistant audio frame.");
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            warn!("Recording worker closed; continuing assistant playback without recording.");
        }
    }

    let frame = AudioFrame {
        data: Cow::Owned(samples),
        sample_rate: 24_000,
        num_channels: 1,
        samples_per_channel: PCM_SAMPLES_PER_FRAME as u32,
    };
    audio_source
        .capture_frame(&frame)
        .await
        .map_err(|error| format!("LiveKit rejected a TTS audio frame: {}", error))
}

fn clear_playback(audio_source: &NativeAudioSource, tts_rx: &mut mpsc::Receiver<String>) {
    while tts_rx.try_recv().is_ok() {}
    audio_source.clear_buffer();
}
