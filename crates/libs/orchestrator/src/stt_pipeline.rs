use futures_util::FutureExt;
use reqwest::{multipart, Client};
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

pub struct SttAudioChunk {
    pub pcm_data: Vec<i16>,
    pub sample_rate: u32,
    pub channels: u16,
    pub model_override: Option<String>,
}

pub struct SttPipeline {
    client: Client,
    api_key: String,
    endpoint_url: String,
    model: String,
    prompt: String,
}

impl SttPipeline {
    pub fn new(api_key: String, endpoint_url: String) -> Self {
        Self::with_model(
            api_key,
            endpoint_url,
            "whisper-large-v3".to_string(),
            std::env::var("STT_PROMPT").unwrap_or_default(),
        )
    }

    pub fn with_model(
        api_key: String,
        endpoint_url: String,
        model: String,
        prompt: String,
    ) -> Self {
        Self {
            client: Client::new(),
            api_key,
            endpoint_url,
            model,
            prompt,
        }
    }

    /// Process a chunk of audio (e.g. triggered by VAD silence)
    pub async fn transcribe_chunk(&self, pcm_data: Vec<i16>) -> Option<String> {
        self.transcribe_chunk_custom(&pcm_data, 16000, 1, None)
            .await
    }

    /// Transcribe audio chunk supporting custom sample rate, channel count, and dynamic model overrides
    pub async fn transcribe_chunk_custom(
        &self,
        pcm_data: &[i16],
        sample_rate: u32,
        channels: u16,
        model_override: Option<&str>,
    ) -> Option<String> {
        if pcm_data.is_empty() {
            info!("STT: Skipping empty audio chunk");
            return None;
        }

        let active_model = model_override.unwrap_or(&self.model);
        info!(
            "STT: Transcribing audio chunk of {} samples ({}Hz, {}ch) via STT model '{}'",
            pcm_data.len(),
            sample_rate,
            channels,
            active_model
        );

        // Encode raw 16-bit PCM data into a valid RIFF/WAV format for Groq / OpenAI STT APIs
        let byte_data = encode_pcm_to_wav(pcm_data, sample_rate, channels);

        let part = multipart::Part::bytes(byte_data.clone())
            .file_name("chunk.wav")
            .mime_str("audio/wav")
            .unwrap();

        let form = multipart::Form::new()
            .part("file", part)
            .text("model", active_model.to_string())
            .text("language", "en")
            .text("prompt", self.prompt.clone());

        let response = self
            .client
            .post(&self.endpoint_url)
            .bearer_auth(&self.api_key)
            .multipart(form)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await;

        match response {
            Ok(res) if res.status().is_success() => {
                let raw_text = res.text().await.unwrap_or_default();
                info!("STT Raw Success Response: {}", raw_text);

                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw_text) {
                    if let Some(text) = json.get("text").and_then(|t| t.as_str()) {
                        let trimmed = text.trim();
                        if is_whisper_hallucination(trimmed) {
                            info!("STT: Discarded silence hallucination: '{}'", trimmed);
                        } else if !trimmed.is_empty() {
                            info!("STT Successfully parsed text: {}", trimmed);
                            return Some(trimmed.to_string());
                        } else {
                            info!("STT parsed text was empty");
                        }
                    } else {
                        error!("STT Success but missing 'text' field in JSON: {}", raw_text);
                    }
                } else {
                    error!("STT Success but failed to parse JSON: {}", raw_text);
                }
            }
            Ok(res) => {
                let status = res.status();
                let err_body = res.text().await.unwrap_or_default();
                error!("STT Failed with status {}: {}", status, err_body);

                // Multi-model fallback: try alternative Whisper model
                let fallback_model = if active_model == "whisper-large-v3-turbo" {
                    "whisper-large-v3"
                } else {
                    "whisper-large-v3-turbo"
                };
                info!(
                    "STT: Attempting fallback with alternative model '{}'...",
                    fallback_model
                );

                let fb_part = multipart::Part::bytes(byte_data)
                    .file_name("chunk.wav")
                    .mime_str("audio/wav")
                    .unwrap();
                let fb_form = multipart::Form::new()
                    .part("file", fb_part)
                    .text("model", fallback_model.to_string())
                    .text("prompt", self.prompt.clone());

                if let Ok(fb_res) = self
                    .client
                    .post(&self.endpoint_url)
                    .bearer_auth(&self.api_key)
                    .multipart(fb_form)
                    .timeout(std::time::Duration::from_secs(15))
                    .send()
                    .await
                {
                    if fb_res.status().is_success() {
                        let fb_raw = fb_res.text().await.unwrap_or_default();
                        if let Ok(fb_json) = serde_json::from_str::<serde_json::Value>(&fb_raw) {
                            if let Some(text) = fb_json.get("text").and_then(|t| t.as_str()) {
                                let trimmed = text.trim();
                                if !is_whisper_hallucination(trimmed) && !trimmed.is_empty() {
                                    info!(
                                        "STT Fallback model '{}' successfully parsed text: {}",
                                        fallback_model, trimmed
                                    );
                                    return Some(trimmed.to_string());
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("STT Network/Timeout error: {}", e);
            }
        }

        None
    }

    /// Spawns a dedicated, isolated Tokio mpsc channel worker for a specific call session.
    /// Spawns an isolated panic-guarded task for each audio chunk request sent to Groq.
    pub fn spawn_call_audio_worker(
        self: Arc<Self>,
        call_id: String,
        buffer_capacity: usize,
        result_tx: mpsc::Sender<String>,
    ) -> mpsc::Sender<SttAudioChunk> {
        let (audio_tx, mut audio_rx) = mpsc::channel::<SttAudioChunk>(buffer_capacity);
        let self_clone = self.clone();
        let call_id_clone = call_id.clone();

        tokio::spawn(async move {
            info!("STT Pipeline: Spawning dedicated per-call audio channel worker for call '{}' (capacity={})", call_id_clone, buffer_capacity);

            let mut worker_running = true;
            while worker_running {
                let call_id_inner = call_id_clone.clone();
                let self_inner = self_clone.clone();
                let audio_rx_ref = &mut audio_rx;
                let result_tx_ref = &result_tx;

                let call_id_for_loop = call_id_inner.clone();
                let result = AssertUnwindSafe(async move {
                    while let Some(chunk) = audio_rx_ref.recv().await {
                        let transcription_started_at = std::time::Instant::now();
                        let self_request = self_inner.clone();
                        let result_tx_request = result_tx_ref.clone();
                        let call_id_request = call_id_for_loop.clone();

                        // Process chunks sequentially to guarantee strict chronological order of speech
                        let req_res = AssertUnwindSafe(async move {
                            self_request
                                .transcribe_chunk_custom(
                                    &chunk.pcm_data,
                                    chunk.sample_rate,
                                    chunk.channels,
                                    chunk.model_override.as_deref(),
                                )
                                .await
                        })
                        .catch_unwind()
                        .await;

                        match req_res {
                            Ok(Some(text)) => {
                                info!(
                                    "STT completed for session {} in {} ms ({} transcript chars)",
                                    call_id_request,
                                    transcription_started_at.elapsed().as_millis(),
                                    text.chars().count()
                                );
                                let _ = result_tx_request.send(text).await;
                            }
                            Ok(None) => {
                                warn!(
                                    "STT produced no transcript for session {} after {} ms",
                                    call_id_request,
                                    transcription_started_at.elapsed().as_millis()
                                );
                            }
                            Err(panic_err) => {
                                error!(
                                    "CRITICAL: Groq STT request task PANICKED for call '{}'! {:?}",
                                    call_id_request, panic_err
                                );
                            }
                        }
                    }
                })
                .catch_unwind()
                .await;

                match result {
                    Ok(_) => {
                        info!(
                            "STT Pipeline: Audio worker channel for call '{}' closed cleanly.",
                            call_id_inner
                        );
                        worker_running = false;
                    }
                    Err(panic_err) => {
                        error!(
                            "CRITICAL: Audio worker outer loop for call '{}' PANICKED! Recovering channel worker... {:?}",
                            call_id_inner, panic_err
                        );
                        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                    }
                }
            }
        });

        audio_tx
    }

    /// Helper for spawning default audio stream worker without call ID
    pub fn spawn_audio_stream_worker(
        self: Arc<Self>,
        buffer_capacity: usize,
        result_tx: mpsc::Sender<String>,
    ) -> mpsc::Sender<SttAudioChunk> {
        self.spawn_call_audio_worker("default-call".to_string(), buffer_capacity, result_tx)
    }
}

/// Helper function to discard common Whisper hallucinations on low audio / background noise
pub fn is_whisper_hallucination(text: &str) -> bool {
    let cleaned: String = text
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect();
    let trimmed = cleaned.trim();

    if trimmed.is_empty() {
        return true;
    }

    const HALLUCINATIONS: &[&str] = &[
        "thank you",
        "thank you very much",
        "thanks for watching",
        "thank you for watching",
        "thanks for listening",
        "thank you for listening",
        "terima kasih telah menonton",
        "terima kasih banyak",
        "terima kasih",
        "bye",
        "bye bye",
        "goodbye",
        "subtitles by",
        "subtitles",
        "watching",
        "you",
        "so",
        "silence",
        "um",
        "uh",
    ];

    HALLUCINATIONS.contains(&trimmed) || trimmed.split_whitespace().all(|w| w == "um" || w == "uh")
}

/// Helper function to construct a standard 44-byte RIFF WAV header around PCM i16 samples
fn encode_pcm_to_wav(pcm_data: &[i16], sample_rate: u32, channels: u16) -> Vec<u8> {
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * u32::from(channels) * u32::from(bits_per_sample) / 8;
    let block_align = channels * bits_per_sample / 8;
    let subchunk2_size = (pcm_data.len() * 2) as u32;
    let chunk_size = 36 + subchunk2_size;

    let mut wav = Vec::with_capacity(44 + pcm_data.len() * 2);

    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&chunk_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt subchunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // Subchunk1Size (16 for PCM)
    wav.extend_from_slice(&1u16.to_le_bytes()); // AudioFormat (1 for PCM)
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data subchunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&subchunk2_size.to_le_bytes());

    // PCM samples
    for &sample in pcm_data {
        wav.extend_from_slice(&sample.to_le_bytes());
    }

    wav
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_pcm_to_wav_header() {
        let pcm = vec![0i16; 16000]; // 1 second of audio at 16kHz
        let wav_bytes = encode_pcm_to_wav(&pcm, 16000, 1);

        assert_eq!(wav_bytes.len(), 44 + 32000);
        assert_eq!(&wav_bytes[0..4], b"RIFF");
        assert_eq!(&wav_bytes[8..12], b"WAVE");
        assert_eq!(&wav_bytes[12..16], b"fmt ");
        assert_eq!(&wav_bytes[36..40], b"data");

        let subchunk2_size = u32::from_le_bytes(wav_bytes[40..44].try_into().unwrap());
        assert_eq!(subchunk2_size, 32000);

        let chunk_size = u32::from_le_bytes(wav_bytes[4..8].try_into().unwrap());
        assert_eq!(chunk_size, 36 + 32000);
    }

    #[tokio::test]
    async fn test_spawn_call_audio_worker_isolation() {
        let stt = Arc::new(SttPipeline::new(
            "mock_key".to_string(),
            "http://localhost:9999/audio/transcriptions".to_string(),
        ));
        let (res_tx1, _res_rx1) = mpsc::channel::<String>(10);
        let (res_tx2, _res_rx2) = mpsc::channel::<String>(10);

        let audio_tx_call1 =
            stt.clone()
                .spawn_call_audio_worker("call-101".to_string(), 10, res_tx1);
        let audio_tx_call2 =
            stt.clone()
                .spawn_call_audio_worker("call-102".to_string(), 10, res_tx2);

        assert!(!audio_tx_call1.is_closed());
        assert!(!audio_tx_call2.is_closed());

        let chunk1 = SttAudioChunk {
            pcm_data: vec![0i16; 1600],
            sample_rate: 16000,
            channels: 1,
            model_override: None,
        };
        let chunk2 = SttAudioChunk {
            pcm_data: vec![0i16; 1600],
            sample_rate: 16000,
            channels: 1,
            model_override: None,
        };

        assert!(audio_tx_call1.send(chunk1).await.is_ok());
        assert!(audio_tx_call2.send(chunk2).await.is_ok());

        drop(audio_tx_call1);
        drop(audio_tx_call2);
    }
}
