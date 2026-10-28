//! Bidirectional provider media. Phone audio and transcripts never enter LiveKit.
use crate::AppState;
use axum::extract::ws::{Message, WebSocket};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use config::TelephonyProvider;
use futures_util::{SinkExt, StreamExt};
use orchestrator::{phone_media::PhoneOutput, telephony_audio, QueueEvent};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const MAX_MEDIA_MESSAGE_BYTES: usize = 64 * 1024;

pub(super) async fn run(
    socket: WebSocket,
    state: AppState,
    provider: TelephonyProvider,
    session_id: String,
    mut outbound: mpsc::Receiver<PhoneOutput>,
    cancelled: CancellationToken,
) {
    let (mut writer, mut reader) = socket.split();
    let mut protocol = MediaProtocol::new(
        provider,
        session_id.splitn(3, ':').nth(2).unwrap_or_default(),
    );
    let mut vad = PhoneVad::default();
    let mut elapsed_samples = 0_u64;
    let (utterance_tx, mut utterance_rx) = mpsc::channel::<Vec<i16>>(8);
    let stt_state = state.clone();
    let stt_id = session_id.clone();
    let stt = tokio::spawn(async move {
        let pipeline = orchestrator::SttPipeline::with_model(
            stt_state.config.groq_api_key.clone(),
            stt_state.config.groq_stt_url.clone(),
            stt_state.config.stt_model.clone(),
            stt_state.config.stt_prompt.clone(),
        );
        while let Some(samples) = utterance_rx.recv().await {
            if let Some(text) = pipeline.transcribe_chunk(samples).await {
                if stt_state
                    .orch_tx
                    .send(QueueEvent::SttTranscript {
                        session_id: stt_id.clone(),
                        text,
                        language: "en".into(),
                        is_final: true,
                    })
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
    });
    let _stt_guard = AbortTask(stt);
    let handshake = tokio::time::sleep(Duration::from_secs(10));
    tokio::pin!(handshake);
    let mut next_output = tokio::time::Instant::now();
    loop {
        tokio::select! {
            biased;
            _ = cancelled.cancelled() => break,
            _ = &mut handshake, if !protocol.started => break,
            incoming = reader.next() => {
                let Some(Ok(message)) = incoming else { break; };
                if let Message::Ping(bytes) = message {
                    if send(&mut writer, Message::Pong(bytes)).await.is_err() { break; }
                    continue;
                }
                let audio = match protocol.receive(message) {
                    Ok(Some(audio)) => audio,
                    Ok(None) => continue,
                    Err(error) => { tracing::debug!(%session_id, %error, "Phone media ended"); break; }
                };
                let samples = telephony_audio::resample_mono_pcm(&audio.pcm, audio.sample_rate, 16_000);
                let speaking = samples.iter().any(|sample| sample.unsigned_abs() >= state.config.vad_barge_in_amplitude);
                if speaking && !vad.is_speaking() {
                    while outbound.try_recv().is_ok() {}
                    if send(&mut writer, protocol.clear_message()).await.is_err() { break; }
                    if enqueue(&state, QueueEvent::BargeInTrigger { session_id: session_id.clone(), timestamp_ms: chrono::Utc::now().timestamp_millis() as u64 }).await.is_err() { break; }
                }
                let utterance = vad.push(&samples, state.config.vad_min_speech_amplitude, state.config.vad_end_of_speech_timeout_ms);
                let count = samples.len() as u64;
                if enqueue(&state, QueueEvent::TelephonyAudio {
                    session_id: session_id.clone(), samples, sample_rate: 16_000, channels: 1, elapsed_samples,
                }).await.is_err() { break; }
                elapsed_samples = elapsed_samples.saturating_add(count);
                if let Some(utterance) = utterance {
                    if !matches!(tokio::time::timeout(Duration::from_secs(2), utterance_tx.send(utterance)).await, Ok(Ok(()))) { break; }
                }
            }
            packet = async {
                tokio::time::sleep_until(next_output).await;
                outbound.recv().await
            }, if protocol.started => {
                let Some(packet) = packet else { break; };
                match packet {
                    PhoneOutput::Clear => {
                        if send(&mut writer, protocol.clear_message()).await.is_err() { break; }
                    }
                    PhoneOutput::Audio { pcm, sample_rate } => {
                        if vad.is_speaking() { continue; }
                        let message = protocol.audio_message(&pcm, sample_rate);
                        if send(&mut writer, message).await.is_err() { break; }
                        next_output = tokio::time::Instant::now() + Duration::from_millis(20);
                    }
                }
            }
        }
    }
    state.phone_media.close(&session_id);
    let _ = state
        .orch_tx
        .send(QueueEvent::SessionCancelled { session_id })
        .await;
}

struct AbortTask(tokio::task::JoinHandle<()>);
impl Drop for AbortTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn enqueue(state: &AppState, event: QueueEvent) -> Result<(), ()> {
    match tokio::time::timeout(Duration::from_secs(2), state.orch_tx.send(event)).await {
        Ok(Ok(())) => Ok(()),
        _ => Err(()),
    }
}

async fn send(
    writer: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    message: Message,
) -> Result<(), ()> {
    match tokio::time::timeout(Duration::from_secs(2), writer.send(message)).await {
        Ok(Ok(())) => Ok(()),
        _ => Err(()),
    }
}

struct Audio {
    pcm: Vec<i16>,
    sample_rate: u32,
}
struct MediaProtocol {
    provider: TelephonyProvider,
    call_id: String,
    stream_id: String,
    started: bool,
}

impl MediaProtocol {
    fn new(provider: TelephonyProvider, call_id: &str) -> Self {
        Self {
            provider,
            call_id: call_id.into(),
            stream_id: String::new(),
            started: false,
        }
    }

    fn receive(&mut self, message: Message) -> Result<Option<Audio>, &'static str> {
        match message {
            Message::Binary(bytes) => {
                if !self.started
                    || !matches!(
                        self.provider,
                        TelephonyProvider::Vonage
                            | TelephonyProvider::Sip
                            | TelephonyProvider::Custom
                    )
                {
                    return Err("Unexpected binary media");
                }
                decode_pcm(&bytes).map(|pcm| {
                    Some(Audio {
                        pcm,
                        sample_rate: 16_000,
                    })
                })
            }
            Message::Text(text) => {
                if text.len() > MAX_MEDIA_MESSAGE_BYTES {
                    return Err("Media message is too large");
                }
                let value: Value = serde_json::from_str(&text).map_err(|_| "Invalid media JSON")?;
                let event = value
                    .get("event")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                match event {
                    "stop" | "closed" | "hangup" | "websocket:closed" => Err("Call ended"),
                    "start" | "websocket:connected" => {
                        self.start(&value)?;
                        Ok(None)
                    }
                    "media" => {
                        if !self.started {
                            return Err("Media arrived before start");
                        }
                        if let Some(id) = value
                            .get("streamSid")
                            .or_else(|| value.get("streamId"))
                            .and_then(Value::as_str)
                        {
                            if !self.stream_id.is_empty() && id != self.stream_id {
                                return Err("Stream ID mismatch");
                            }
                        }
                        let media = value.get("media").ok_or("Missing media")?;
                        if media
                            .get("track")
                            .and_then(Value::as_str)
                            .is_some_and(|track| track != "inbound" && track != "inbound_track")
                        {
                            return Ok(None);
                        }
                        let encoded = media
                            .get("payload")
                            .and_then(Value::as_str)
                            .ok_or("Missing audio payload")?;
                        let bytes = STANDARD
                            .decode(encoded)
                            .map_err(|_| "Invalid audio encoding")?;
                        match self.provider {
                            TelephonyProvider::Twilio
                            | TelephonyProvider::Telnyx
                            | TelephonyProvider::Plivo => {
                                if bytes.is_empty() {
                                    return Err("Empty audio");
                                }
                                Ok(Some(Audio {
                                    pcm: telephony_audio::decode_mulaw_8khz(&bytes),
                                    sample_rate: 8_000,
                                }))
                            }
                            _ => decode_pcm(&bytes).map(|pcm| {
                                Some(Audio {
                                    pcm,
                                    sample_rate: 16_000,
                                })
                            }),
                        }
                    }
                    _ => Ok(None),
                }
            }
            Message::Close(_) => Err("Socket closed"),
            _ => Ok(None),
        }
    }

    fn start(&mut self, value: &Value) -> Result<(), &'static str> {
        if self.started {
            return Err("Duplicate media start");
        }
        if self.provider == TelephonyProvider::Vonage {
            if value.get("event").and_then(Value::as_str) != Some("websocket:connected")
                || value.get("content-type").and_then(Value::as_str) != Some("audio/l16;rate=16000")
            {
                return Err("Unsupported Vonage media format");
            }
            self.started = true;
            return Ok(());
        }
        let start = value.get("start").unwrap_or(value);
        let (call_key, stream_key) = match self.provider {
            TelephonyProvider::Twilio => ("callSid", "streamSid"),
            TelephonyProvider::Telnyx => ("call_control_id", "stream_id"),
            TelephonyProvider::Plivo => ("callId", "streamId"),
            _ => ("call_id", "stream_id"),
        };
        if start.get(call_key).and_then(Value::as_str) != Some(self.call_id.as_str()) {
            return Err("Media call ID mismatch");
        }
        self.stream_id = start
            .get(stream_key)
            .or_else(|| value.get(stream_key))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if matches!(
            self.provider,
            TelephonyProvider::Twilio | TelephonyProvider::Plivo
        ) && self.stream_id.is_empty()
        {
            return Err("Missing stream ID");
        }
        let format = start
            .get("mediaFormat")
            .or_else(|| start.get("media_format"))
            .unwrap_or(start);
        let rate = format
            .get("sampleRate")
            .or_else(|| format.get("sample_rate"))
            .and_then(Value::as_u64);
        let codec = format
            .get("encoding")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let supported = match self.provider {
            TelephonyProvider::Twilio | TelephonyProvider::Plivo => {
                rate == Some(8_000) && codec.eq_ignore_ascii_case("audio/x-mulaw")
            }
            TelephonyProvider::Telnyx => rate == Some(8_000) && codec.eq_ignore_ascii_case("PCMU"),
            TelephonyProvider::Sip | TelephonyProvider::Custom => {
                rate == Some(16_000) && codec == "pcm_s16le"
            }
            _ => false,
        };
        if !supported
            || format
                .get("channels")
                .and_then(Value::as_u64)
                .is_some_and(|channels| channels != 1)
        {
            return Err("Unsupported phone media format");
        }
        self.started = true;
        Ok(())
    }

    fn audio_message(&self, pcm: &[i16], rate: u32) -> Message {
        match self.provider {
            TelephonyProvider::Twilio | TelephonyProvider::Telnyx | TelephonyProvider::Plivo => {
                let pcm = telephony_audio::resample_mono_pcm(pcm, rate, 8_000);
                let payload = STANDARD.encode(telephony_audio::encode_mulaw_8khz(&pcm));
                let value = match self.provider {
                    TelephonyProvider::Twilio => {
                        json!({"event":"media", "streamSid":self.stream_id, "media":{"payload":payload}})
                    }
                    TelephonyProvider::Plivo => {
                        json!({"event":"playAudio", "media":{"contentType":"audio/x-mulaw", "sampleRate":8000, "payload":payload}})
                    }
                    _ => json!({"event":"media", "media":{"payload":payload}}),
                };
                Message::Text(value.to_string().into())
            }
            _ => Message::Binary(
                telephony_audio::resample_mono_pcm(pcm, rate, 16_000)
                    .iter()
                    .flat_map(|sample| sample.to_le_bytes())
                    .collect::<Vec<_>>()
                    .into(),
            ),
        }
    }

    fn clear_message(&self) -> Message {
        let value = match self.provider {
            TelephonyProvider::Twilio => json!({"event":"clear", "streamSid":self.stream_id}),
            TelephonyProvider::Plivo => json!({"event":"clearAudio", "streamId":self.stream_id}),
            TelephonyProvider::Vonage => json!({"action":"clear"}),
            _ => json!({"event":"clear"}),
        };
        Message::Text(value.to_string().into())
    }
}

fn decode_pcm(bytes: &[u8]) -> Result<Vec<i16>, &'static str> {
    if bytes.is_empty() || bytes.len() > MAX_MEDIA_MESSAGE_BYTES || bytes.len() % 2 != 0 {
        return Err("Invalid PCM frame");
    }
    Ok(bytes
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect())
}

#[derive(Default)]
struct PhoneVad {
    silence_samples: usize,
    samples: Vec<i16>,
}
impl PhoneVad {
    fn is_speaking(&self) -> bool {
        !self.samples.is_empty()
    }
    fn push(&mut self, samples: &[i16], min_amplitude: u16, silence_ms: u64) -> Option<Vec<i16>> {
        if samples
            .iter()
            .any(|sample| sample.unsigned_abs() >= min_amplitude)
        {
            self.silence_samples = 0;
        } else if self.samples.is_empty() {
            return None;
        } else {
            self.silence_samples += samples.len();
        }
        self.samples.extend_from_slice(samples);
        // Bound continuous speech to ten seconds and keep STT sequential.
        if self.samples.len() >= 160_000
            || (self.silence_samples >= (silence_ms as usize).saturating_mul(16)
                && self.samples.len() >= 3200)
        {
            self.silence_samples = 0;
            Some(std::mem::take(&mut self.samples))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_requires_matching_call_and_supported_format() {
        let mut protocol = MediaProtocol::new(TelephonyProvider::Twilio, "CA-a");
        assert!(protocol
            .receive(Message::Text(
                json!({"event":"media","media":{"payload":"/w=="}})
                    .to_string()
                    .into()
            ))
            .is_err());
        let mut start = json!({"event":"start","start":{"callSid":"CA-b","streamSid":"MZ-a","mediaFormat":{"encoding":"audio/x-mulaw","sampleRate":8000,"channels":1}}});
        assert!(protocol.start(&start).is_err());
        start["start"]["callSid"] = json!("CA-a");
        start["start"]["mediaFormat"]["sampleRate"] = json!(16000);
        assert!(protocol.start(&start).is_err());
        start["start"]["mediaFormat"]["sampleRate"] = json!(8000);
        assert!(protocol.start(&start).is_ok());
        assert!(protocol.start(&start).is_err());
        assert!(protocol
            .receive(Message::Text(
                json!({"event":"media","streamSid":"wrong","media":{"payload":"/w=="}})
                    .to_string()
                    .into()
            ))
            .is_err());
    }

    #[test]
    fn phone_pcm_validation_and_mulaw_conversion_preserve_sign_and_duration() {
        assert!(decode_pcm(&[1]).is_err());
        assert!(decode_pcm(&vec![0; MAX_MEDIA_MESSAGE_BYTES + 2]).is_err());
        let pcm = [-2000, 0, 2000];
        let decoded = telephony_audio::decode_mulaw_8khz(&telephony_audio::encode_mulaw_8khz(&pcm));
        assert!(decoded[0] < 0 && decoded[2] > 0);
        assert_eq!(decoded[1], 0);
        assert_eq!(
            telephony_audio::resample_mono_pcm(&[0; 160], 8000, 16000).len(),
            320
        );
    }

    #[test]
    fn vad_bounds_continuous_speech_and_emits_on_silence() {
        let mut vad = PhoneVad::default();
        let mut emitted = 0;
        for _ in 0..600 {
            if let Some(samples) = vad.push(&[1000; 320], 500, 300) {
                assert!(samples.len() <= 160_000);
                emitted += samples.len();
            }
        }
        assert_eq!(emitted, 160_000);
        for _ in 0..15 {
            if let Some(samples) = vad.push(&[0; 320], 500, 300) {
                emitted += samples.len();
            }
        }
        assert!(!vad.is_speaking());
        assert_eq!(emitted, 600 * 320 + 15 * 320);
    }
}
