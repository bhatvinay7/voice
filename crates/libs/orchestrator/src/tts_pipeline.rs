use reqwest::{Client, Response};
use tracing::{error, info};

/// Applies a restrained low-mid lift consistently across streamed PCM frames.
#[derive(Default)]
pub struct VoiceEnhancer {
    low_frequency: f32,
}

impl VoiceEnhancer {
    pub fn process(&mut self, samples: &mut [i16]) {
        for sample in samples {
            let signal = *sample as f32;
            self.low_frequency += 0.075 * (signal - self.low_frequency);
            let enhanced = (signal + self.low_frequency * 0.28) * 1.12;
            *sample = enhanced.clamp(-30_000.0, 30_000.0) as i16;
        }
    }
}

pub struct TtsPipeline {
    client: Client,
    api_key: String,
    endpoint_url: String,
    model: String,
    fallback_model: String,
    voice: String,
}

impl TtsPipeline {
    pub fn new(
        api_key: String,
        endpoint_url: String,
        model: String,
        fallback_model: String,
        voice: String,
    ) -> Self {
        Self {
            client: Client::new(),
            api_key,
            endpoint_url,
            model,
            fallback_model,
            voice,
        }
    }

    /// Opens a PCM audio response that the caller can play while TTS is generating it.
    pub async fn open_audio_stream(&self, text: &str) -> Result<Response, String> {
        info!(
            "TTS: Streaming via model '{}' (voice '{}'): {}",
            self.model, self.voice, text
        );

        match self.request_audio_stream(&self.model, text).await {
            Ok(response) => Ok(response),
            Err(primary_error)
                if !self.fallback_model.is_empty() && self.fallback_model != self.model =>
            {
                error!(
                    "TTS streaming request failed with model '{}': {}; retrying with '{}'",
                    self.model, primary_error, self.fallback_model
                );
                self.request_audio_stream(&self.fallback_model, text)
                    .await
                    .map_err(|fallback_error| {
                        format!(
                            "TTS primary request failed ({}); fallback failed ({})",
                            primary_error, fallback_error
                        )
                    })
            }
            Err(error) => Err(error),
        }
    }

    async fn request_audio_stream(&self, model: &str, text: &str) -> Result<Response, String> {
        let payload = serde_json::json!({
            "model": model,
            "input": text,
            "voice": self.voice,
            "response_format": "pcm",
            "stream_format": "audio"
        });

        let response = self
            .client
            .post(&self.endpoint_url)
            .bearer_auth(&self.api_key)
            .json(&payload)
            .timeout(std::time::Duration::from_secs(90))
            .send()
            .await
            .map_err(|error| format!("TTS request failed: {}", error))?;

        if response.status().is_success() {
            return Ok(response);
        }

        let status = response.status();
        let error_body = response
            .text()
            .await
            .unwrap_or_else(|_| "unable to read error response".to_string());
        Err(format!("TTS request returned {}: {}", status, error_body))
    }
}
