//! Phone-only speech generation using the same configured TTS provider as web calls.
use crate::{
    models::*,
    phone_media::PhoneOutput,
    tts_pipeline::{TtsPipeline, VoiceEnhancer},
};
use futures_util::{FutureExt, StreamExt};
use std::{collections::VecDeque, panic::AssertUnwindSafe, time::Instant};
use tokio::sync::mpsc;

pub fn spawn(
    session_id: String,
    events: mpsc::Sender<StsPipelineEvent>,
    recording: mpsc::Sender<S3DumpData>,
    output: mpsc::Sender<PhoneOutput>,
) -> mpsc::Sender<StsPipelineCmd> {
    let (tx, mut rx) = mpsc::channel(50);
    tokio::spawn(async move {
        let cfg = config::AppConfig::load().unwrap_or_else(|_| config::AppConfig::load_default());
        let pipeline = TtsPipeline::new(
            cfg.openai_api_key,
            cfg.openai_tts_url,
            cfg.tts_model,
            cfg.tts_fallback_model,
            cfg.tts_voice,
        );
        let started_at = Instant::now();
        // Phone calls need the same physical source isolation as WebRTC: a
        // fast clarification/wait prompt must never sit behind a large-agent
        // answer in a shared FIFO queue.
        let mut agent_pending: VecDeque<(u64, String)> = VecDeque::new();
        let mut fast_pending: VecDeque<(u64, String)> = VecDeque::new();
        let mut playback_generation = 0_u64;
        if let Ok(greeting) = std::env::var("AI_GREETING") {
            if !greeting.trim().is_empty() {
                agent_pending.push_back((playback_generation, greeting));
            }
        }
        'session: loop {
            let (generation, text) = if let Some(request) = fast_pending.pop_front() {
                request
            } else if let Some(request) = agent_pending.pop_front() {
                request
            } else {
                match rx.recv().await {
                    Some(StsPipelineCmd::InjectContext { text })
                    | Some(StsPipelineCmd::InjectAgentContext { text }) => {
                        agent_pending.push_back((playback_generation, text));
                        continue;
                    }
                    Some(StsPipelineCmd::InjectFastContext { text }) => {
                        playback_generation = playback_generation.saturating_add(1);
                        agent_pending.clear();
                        fast_pending.clear();
                        if output.send(PhoneOutput::Clear).await.is_err() {
                            break;
                        }
                        fast_pending.push_back((playback_generation, text));
                        continue;
                    }
                    Some(StsPipelineCmd::Stop) => {
                        playback_generation = playback_generation.saturating_add(1);
                        agent_pending.clear();
                        fast_pending.clear();
                        if output.send(PhoneOutput::Clear).await.is_err() {
                            break;
                        }
                        continue;
                    }
                    _ => break,
                }
            };
            let speech = AssertUnwindSafe(speak(
                &pipeline,
                &text,
                &session_id,
                started_at,
                &recording,
                &output,
            ))
            .catch_unwind();
            tokio::pin!(speech);
            loop {
                tokio::select! {
                    biased;
                    command = rx.recv() => match command {
                        Some(StsPipelineCmd::InjectContext { text })
                        | Some(StsPipelineCmd::InjectAgentContext { text }) => {
                            if agent_pending.len() < 50 { agent_pending.push_back((playback_generation, text)); }
                            else { let _ = events.send(StsPipelineEvent::Error("Phone speech queue is full".into())).await; }
                        }
                        Some(StsPipelineCmd::InjectFastContext { text }) => {
                            playback_generation = playback_generation.saturating_add(1);
                            agent_pending.clear();
                            fast_pending.clear();
                            if output.send(PhoneOutput::Clear).await.is_err() { break 'session; }
                            fast_pending.push_back((playback_generation, text));
                            break;
                        }
                        Some(StsPipelineCmd::Stop) => {
                            playback_generation = playback_generation.saturating_add(1);
                            agent_pending.clear();
                            fast_pending.clear();
                            if output.send(PhoneOutput::Clear).await.is_err() { break 'session; }
                            break; // Dropping the speech future cancels its HTTP stream.
                        }
                        _ => break 'session,
                    },
                    result = &mut speech => {
                        let event = match result {
                            Ok(Ok(())) => StsPipelineEvent::AudioComplete,
                            Ok(Err(error)) => StsPipelineEvent::Error(error),
                            Err(_) => StsPipelineEvent::Error("Phone TTS panicked; interrupted utterance discarded".into()),
                        };
                        if events.send(event).await.is_err() { break 'session; }
                        break;
                    }
                }
            }
            // A preemption observed while this request was awaiting TTS
            // completion invalidates it before the next queue selection.
            if generation != playback_generation {
                continue;
            }
        }
    });
    tx
}

async fn speak(
    pipeline: &TtsPipeline,
    text: &str,
    session_id: &str,
    started_at: Instant,
    recording: &mpsc::Sender<S3DumpData>,
    output: &mpsc::Sender<PhoneOutput>,
) -> Result<(), String> {
    let response = pipeline.open_audio_stream(text).await?;
    let mut stream = response.bytes_stream();
    let mut bytes = VecDeque::new();
    let mut enhancer = VoiceEnhancer::default();
    let mut sample_position = None;
    // 20 ms at the TTS provider's 24 kHz PCM format.
    const FRAME_BYTES: usize = 480 * 2;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "Phone TTS audio stream failed")?;
        bytes.extend(chunk);
        while bytes.len() >= FRAME_BYTES {
            let frame: Vec<u8> = bytes.drain(..FRAME_BYTES).collect();
            send_frame(
                frame,
                &mut enhancer,
                session_id,
                started_at,
                &mut sample_position,
                recording,
                output,
            )
            .await?;
        }
    }
    if bytes.len() % 2 != 0 {
        return Err("Incomplete TTS PCM sample".into());
    }
    if !bytes.is_empty() {
        bytes.resize(FRAME_BYTES, 0);
        send_frame(
            bytes.into_iter().collect(),
            &mut enhancer,
            session_id,
            started_at,
            &mut sample_position,
            recording,
            output,
        )
        .await?;
    }
    Ok(())
}

async fn send_frame(
    bytes: Vec<u8>,
    enhancer: &mut VoiceEnhancer,
    session_id: &str,
    started_at: Instant,
    sample_position: &mut Option<u64>,
    recording: &mpsc::Sender<S3DumpData>,
    output: &mpsc::Sender<PhoneOutput>,
) -> Result<(), String> {
    let mut pcm: Vec<i16> = bytes
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    enhancer.process(&mut pcm);
    output
        .send(PhoneOutput::Audio {
            pcm: pcm.clone(),
            sample_rate: 24_000,
        })
        .await
        .map_err(|_| "Phone media socket closed")?;
    let position = *sample_position.get_or_insert_with(|| elapsed_audio_samples(started_at));
    *sample_position = Some(position + pcm.len() as u64 * 16_000 / 24_000);
    let _ = recording
        .send(S3DumpData::Audio {
            session_id: session_id.to_owned(),
            samples: pcm,
            sample_rate: 24_000,
            channels: 1,
            elapsed_samples: position,
            speaker: AudioSpeaker::Assistant,
        })
        .await;
    Ok(())
}
