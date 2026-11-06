//! Optional backend-side DeepFilterNet bridge.
//!
//! DeepFilterNet's supported realtime binary accepts 48 kHz WAV input. The
//! voice transport currently delivers 16 kHz PCM, so this adapter performs a
//! bounded resample around one utterance, invokes the configured binary, and
//! converts the result back to the STT sample rate. If the binary is absent or
//! fails, the original PCM is returned so a deployment never loses speech.

use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{fs, process::Command, time::timeout};

#[derive(Clone, Debug)]
pub struct DeepFilterNetProcessor {
    enabled: bool,
    binary: String,
    model: Option<String>,
    timeout: Duration,
}

impl DeepFilterNetProcessor {
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var("DEEPFILTERNET_ENABLED")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            binary: std::env::var("DEEPFILTERNET_BIN")
                .unwrap_or_else(|_| "deep-filter".to_string()),
            model: std::env::var("DEEPFILTERNET_MODEL")
                .ok()
                .filter(|v| !v.trim().is_empty()),
            timeout: Duration::from_millis(
                std::env::var("DEEPFILTERNET_TIMEOUT_MS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(2500),
            ),
        }
    }

    pub async fn process(&self, pcm: &[i16], sample_rate: u32, channels: u16) -> Vec<i16> {
        if !self.enabled || pcm.is_empty() || channels != 1 || sample_rate == 0 {
            return pcm.to_vec();
        }
        let input = resample(pcm, sample_rate, 48_000);
        let token = format!("{}-{}", std::process::id(), uuid::Uuid::new_v4());
        let dir = std::env::temp_dir().join(format!("deepfilter-{token}"));
        let input_path = dir.join("input.wav");
        let output_dir = dir.join("out");
        if fs::create_dir_all(&output_dir).await.is_err()
            || fs::write(&input_path, encode_wav(&input, 48_000))
                .await
                .is_err()
        {
            return pcm.to_vec();
        }
        let mut command = Command::new(&self.binary);
        command
            .arg("-o")
            .arg(&output_dir)
            .arg(&input_path)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        if let Some(model) = &self.model {
            command.arg("-m").arg(model);
        }
        let result = timeout(self.timeout, command.output()).await;
        let output_path = output_dir.join("input.wav");
        let enhanced = match result {
            Ok(Ok(output)) if output.status.success() => read_wav_pcm(&output_path)
                .await
                .map(|audio| resample(&audio, 48_000, sample_rate)),
            Ok(Ok(output)) => {
                tracing::warn!(status = ?output.status, "DeepFilterNet failed; using original speech PCM");
                None
            }
            Ok(Err(error)) => {
                tracing::warn!(%error, binary = %self.binary, "DeepFilterNet unavailable; using original speech PCM");
                None
            }
            Err(_) => {
                tracing::warn!(
                    timeout_ms = self.timeout.as_millis(),
                    "DeepFilterNet timed out; using original speech PCM"
                );
                None
            }
        };
        let _ = fs::remove_dir_all(&dir).await;
        enhanced.unwrap_or_else(|| pcm.to_vec())
    }
}

fn encode_wav(samples: &[i16], sample_rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend(samples.iter().flat_map(|sample| sample.to_le_bytes()));
    out
}

async fn read_wav_pcm(path: &PathBuf) -> Option<Vec<i16>> {
    let bytes = fs::read(path).await.ok()?;
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" {
        return None;
    }
    let data_pos = bytes.windows(4).position(|window| window == b"data")? + 4;
    let len = u32::from_le_bytes(bytes.get(data_pos..data_pos + 4)?.try_into().ok()?) as usize;
    let start = data_pos + 4;
    let end = (start + len).min(bytes.len());
    Some(
        bytes[start..end]
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect(),
    )
}

fn resample(input: &[i16], from: u32, to: u32) -> Vec<i16> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let output_len = ((input.len() as u64 * to as u64) / from as u64).max(1) as usize;
    (0..output_len)
        .map(|index| {
            let position = index as f64 * from as f64 / to as f64;
            let left = position.floor() as usize;
            let right = (left + 1).min(input.len() - 1);
            let fraction = position.fract() as f32;
            (input[left] as f32 + (input[right] - input[left]) as f32 * fraction)
                .round()
                .clamp(i16::MIN as f32, i16::MAX as f32) as i16
        })
        .collect()
}
