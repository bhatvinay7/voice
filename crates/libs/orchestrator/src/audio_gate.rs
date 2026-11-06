/// Audio noise-gate heuristics shared by the LiveKit STS pipeline and the phone pipeline.
///
/// # Design
/// Loud audio is **never discarded at the VAD level**.  Instead each 32 ms chunk
/// is *classified* so that downstream components (STT, LLM) can decide whether
/// to clean up noise, carry on, or ask the user to repeat.
///
/// The gate decides whether to *open a user turn* based on a 1.0–1.3 s rolling
/// analysis window.  Very-high-amplitude chunks (horns, whistles, mixers, animal
/// sounds) do **not** count toward the turn-open or barge-in counters, so the AI
/// keeps speaking uninterrupted.

/// Loudness classification of a single 32 ms audio chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkClass {
    /// Peak is below the minimum speech floor — background silence.
    BelowFloor,
    /// Peak is inside the normal human speaking range **and** Silero agrees.
    Speech,
    /// Inside the normal range but Silero did not detect speech — quiet noise
    /// (filler, hum).
    QuietNoise,
    /// Peak is above the max speech amplitude or the chunk is clipped — loud
    /// non-speech event.  Does **not** count toward turn-open or barge-in.
    LoudNoise,
}

/// Per-utterance noise summary carried alongside the STT transcript.
#[derive(Debug, Clone)]
pub struct NoiseProfile {
    /// Fraction of chunks that Silero classified as speech (0.0–1.0).
    pub speech_ratio: f32,
    /// Fraction of chunks in the `LoudNoise` class (0.0–1.0).
    pub loud_noise_ratio: f32,
    /// Fraction of samples within the utterance that are clipped (≥ clip_level).
    pub clipped_ratio: f32,
    /// True when the utterance energy was suspiciously flat (ringtone / mixer /
    /// sustained "moooo").
    pub sustained_tone: bool,
    /// True when the crest factor indicates transient impulses (hammering, clicks).
    pub impulsive: bool,
    /// Coarse noise level for downstream decision logic.
    pub level: NoiseLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoiseLevel {
    /// Transcript is likely clean.
    Clean,
    /// Some noise artefacts present; LLM should attempt to clean.
    Moderate,
    /// Heavy noise throughout; LLM should try to reconstruct or ask to repeat.
    Heavy,
}

impl NoiseProfile {
    pub fn compute(
        chunk_classes: &[ChunkClass],
        speech_probs: &[f32],
        chunk_rms_values: &[f32],
        total_samples: usize,
        clipped_samples: usize,
        params: &VadParams,
    ) -> Self {
        let n = chunk_classes.len().max(1) as f32;

        let speech_count = chunk_classes
            .iter()
            .filter(|c| **c == ChunkClass::Speech)
            .count();
        let loud_count = chunk_classes
            .iter()
            .filter(|c| **c == ChunkClass::LoudNoise)
            .count();

        let speech_ratio = speech_count as f32 / n;
        let loud_noise_ratio = loud_count as f32 / n;
        let clipped_ratio = if total_samples == 0 {
            0.0
        } else {
            clipped_samples as f32 / total_samples as f32
        };

        // Sustained-tone detection: low energy variation = flat signal.
        let sustained_tone = if chunk_rms_values.len() >= 8 {
            let mean = chunk_rms_values.iter().sum::<f32>() / chunk_rms_values.len() as f32;
            let variance = chunk_rms_values
                .iter()
                .map(|r| (r - mean).powi(2))
                .sum::<f32>()
                / chunk_rms_values.len() as f32;
            let cv = if mean > 0.0 {
                variance.sqrt() / mean
            } else {
                0.0
            };
            cv < params.min_energy_variation
        } else {
            false
        };

        // Impulsive detection: average speech-prob among non-loud chunks is low but
        // max RMS is very high (sharp spikes with silence around them).
        let impulsive = if !chunk_rms_values.is_empty() {
            let max_rms = chunk_rms_values.iter().cloned().fold(0.0f32, f32::max);
            let mean_rms = chunk_rms_values.iter().sum::<f32>() / chunk_rms_values.len() as f32;
            let crest = if mean_rms > 0.0 {
                max_rms / mean_rms
            } else {
                0.0
            };
            let avg_prob = if speech_probs.is_empty() {
                0.0
            } else {
                speech_probs.iter().sum::<f32>() / speech_probs.len() as f32
            };
            crest > params.max_crest_factor && avg_prob < params.speech_probability_threshold
        } else {
            false
        };

        let level = if speech_ratio >= 0.4 && loud_noise_ratio < params.max_loud_ratio {
            if loud_noise_ratio > 0.1 || clipped_ratio > 0.02 {
                NoiseLevel::Moderate
            } else {
                NoiseLevel::Clean
            }
        } else if speech_ratio >= 0.15 {
            NoiseLevel::Moderate
        } else {
            NoiseLevel::Heavy
        };

        NoiseProfile {
            speech_ratio,
            loud_noise_ratio,
            clipped_ratio,
            sustained_tone,
            impulsive,
            level,
        }
    }
}

/// Tuning parameters for the VAD gate, collected into a single struct to avoid
/// long argument lists at call sites.
#[derive(Debug, Clone)]
pub struct VadParams {
    /// Silero speech probability needed to count a chunk as speech.
    pub speech_probability_threshold: f32,
    /// Silero speech probability needed for barge-in.
    pub barge_in_probability_threshold: f32,
    /// Minimum peak amplitude (u16 scale) to be above the silence floor.
    pub min_speech_amplitude: u16,
    /// Upper peak amplitude for a chunk to be *in the normal speaking range*.
    /// Chunks louder than this are `LoudNoise` and never count as a turn.
    pub max_speech_amplitude: u16,
    /// Peak amplitude at which genuine audio may bypass an overly conservative
    /// Silero probability result. The normal floor still applies and loud
    /// chunks are rejected before this check.
    pub energy_bypass_amplitude: u16,
    /// Barge-in requires peak ≥ this **and** ≤ `max_speech_amplitude`.
    pub barge_in_amplitude: u16,
    /// Consecutive speech chunks required to interrupt current AI playback.
    pub barge_in_min_chunks: u32,
    /// Silence after end of speech that closes the utterance (milliseconds).
    pub end_of_speech_timeout_ms: u64,
    /// Rolling analysis window in milliseconds (1000–1300).
    pub analysis_window_ms: u64,
    /// Minimum number of `Speech` chunks inside the window needed to open a turn.
    pub min_speech_chunks_in_window: u32,
    /// Peak at or above this level is counted as clipped.
    pub clip_level: u16,
    /// Maximum crest factor (peak/rms) before a chunk is labelled impulsive.
    pub max_crest_factor: f32,
    /// Minimum coefficient of variation of chunk RMS; below this = sustained tone.
    pub min_energy_variation: f32,
    /// If the fraction of `LoudNoise` chunks in a window meets or exceeds this,
    /// the window is marked `LoudNoiseOnly` and never starts a user turn.
    pub max_loud_ratio: f32,
}

impl VadParams {
    /// Load from config, applying defaults for all new fields.
    pub fn from_config(cfg: &config::AppConfig) -> Self {
        Self {
            speech_probability_threshold: cfg.vad_speech_probability_threshold,
            barge_in_probability_threshold: cfg.vad_barge_in_probability_threshold,
            min_speech_amplitude: cfg.vad_min_speech_amplitude,
            max_speech_amplitude: cfg.vad_max_speech_amplitude,
            energy_bypass_amplitude: cfg.vad_energy_bypass_amplitude,
            barge_in_amplitude: cfg.vad_barge_in_amplitude,
            barge_in_min_chunks: cfg.vad_barge_in_min_chunks,
            end_of_speech_timeout_ms: cfg.vad_end_of_speech_timeout_ms,
            analysis_window_ms: cfg.vad_analysis_window_ms,
            min_speech_chunks_in_window: cfg.vad_min_speech_chunks_in_window,
            clip_level: cfg.vad_clip_level,
            max_crest_factor: cfg.vad_max_crest_factor,
            min_energy_variation: cfg.vad_min_energy_variation,
            max_loud_ratio: cfg.vad_max_loud_ratio,
        }
    }
}

/// Classify a single 32ms chunk given its peak amplitude and Silero speech
/// probability.
pub fn classify_chunk(peak: f32, speech_prob: f32, params: &VadParams) -> ChunkClass {
    let peak_u16 = (peak * 32768.0) as u16;
    if peak_u16 < params.min_speech_amplitude {
        return ChunkClass::BelowFloor;
    }
    // Clipped or very loud — never counts toward a turn.
    if peak_u16 >= params.max_speech_amplitude || peak_u16 >= params.clip_level {
        return ChunkClass::LoudNoise;
    }
    if speech_prob >= params.speech_probability_threshold
        || (params.energy_bypass_amplitude > 0 && peak_u16 >= params.energy_bypass_amplitude)
    {
        ChunkClass::Speech
    } else {
        ChunkClass::QuietNoise
    }
}

/// Count how many chunks in `window` are `Speech`.
pub fn count_speech_in_window(window: &[ChunkClass]) -> u32 {
    window.iter().filter(|c| **c == ChunkClass::Speech).count() as u32
}

/// True when a window is dominated by very-loud chunks and should be discarded
/// as a `LoudNoiseOnly` event (no user turn opened).
pub fn is_loud_noise_window(window: &[ChunkClass], max_loud_ratio: f32) -> bool {
    if window.is_empty() {
        return false;
    }
    let loud = window
        .iter()
        .filter(|c| **c == ChunkClass::LoudNoise)
        .count();
    loud as f32 / window.len() as f32 >= max_loud_ratio
}

/// Classify a barge-in chunk: same rules as `classify_chunk` but uses the
/// barge-in amplitude floor rather than the turn-open floor.
pub fn classify_barge_in_chunk(peak: f32, speech_prob: f32, params: &VadParams) -> ChunkClass {
    let peak_u16 = (peak * 32768.0) as u16;
    if peak_u16 < params.barge_in_amplitude {
        return ChunkClass::BelowFloor;
    }
    // Still cap at the same max-speech amplitude to block horns/whistles.
    if peak_u16 >= params.max_speech_amplitude || peak_u16 >= params.clip_level {
        return ChunkClass::LoudNoise;
    }
    // Only use the energy bypass in the low-amplitude human band. Applying it
    // to every sub-cap peak would let motors/whistles interrupt AI playback.
    let low_amplitude_speech = params.energy_bypass_amplitude > 0
        && peak_u16 >= params.energy_bypass_amplitude
        && peak_u16 <= params.barge_in_amplitude.saturating_mul(2);
    if speech_prob >= params.barge_in_probability_threshold || low_amplitude_speech {
        ChunkClass::Speech
    } else {
        ChunkClass::QuietNoise
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_params() -> VadParams {
        VadParams {
            speech_probability_threshold: 0.15,
            barge_in_probability_threshold: 0.40,
            min_speech_amplitude: 100,
            max_speech_amplitude: 20000,
            energy_bypass_amplitude: 100,
            barge_in_amplitude: 200,
            barge_in_min_chunks: 2,
            end_of_speech_timeout_ms: 650,
            analysis_window_ms: 1200,
            min_speech_chunks_in_window: 4,
            clip_level: 30000,
            max_crest_factor: 8.0,
            min_energy_variation: 0.25,
            max_loud_ratio: 0.6,
        }
    }

    #[test]
    fn silence_is_below_floor() {
        let p = test_params();
        assert_eq!(classify_chunk(0.001, 0.9, &p), ChunkClass::BelowFloor);
    }

    #[test]
    fn normal_speech_classified() {
        let p = test_params();
        // peak ~5000 → 0.152 in f32, prob 0.8
        let peak = 5000.0 / 32768.0;
        assert_eq!(classify_chunk(peak, 0.8, &p), ChunkClass::Speech);
    }

    #[test]
    fn energy_bypass_opens_audio_above_configured_amplitude() {
        let p = test_params();
        let peak = 1373.0 / 32768.0;
        assert_eq!(classify_chunk(peak, 0.01, &p), ChunkClass::Speech);
    }

    #[test]
    fn very_loud_is_loud_noise() {
        let p = test_params();
        // peak 25000 → above max_speech_amplitude 20000
        let peak = 25000.0 / 32768.0;
        assert_eq!(classify_chunk(peak, 0.95, &p), ChunkClass::LoudNoise);
    }

    #[test]
    fn clipped_is_loud_noise() {
        let p = test_params();
        let peak = 31000.0 / 32768.0;
        assert_eq!(classify_chunk(peak, 0.99, &p), ChunkClass::LoudNoise);
    }

    #[test]
    fn loud_noise_window_detected() {
        let p = test_params();
        let window = vec![
            ChunkClass::LoudNoise,
            ChunkClass::LoudNoise,
            ChunkClass::LoudNoise,
            ChunkClass::Speech,
            ChunkClass::LoudNoise,
        ];
        assert!(is_loud_noise_window(&window, p.max_loud_ratio));
    }

    #[test]
    fn mixed_window_not_loud_noise() {
        let p = test_params();
        let window = vec![
            ChunkClass::Speech,
            ChunkClass::Speech,
            ChunkClass::LoudNoise,
            ChunkClass::Speech,
        ];
        assert!(!is_loud_noise_window(&window, p.max_loud_ratio));
    }

    #[test]
    fn barge_in_loud_resets_counter() {
        let p = test_params();
        let peak = 28000.0 / 32768.0; // above max_speech_amplitude
        assert_eq!(
            classify_barge_in_chunk(peak, 0.99, &p),
            ChunkClass::LoudNoise
        );
    }

    #[test]
    fn low_amplitude_human_speech_can_barge_in_at_configured_floor() {
        let p = test_params();
        let peak = 100.0 / 32768.0;
        assert_eq!(classify_barge_in_chunk(peak, 0.05, &p), ChunkClass::Speech);
        assert_eq!(
            classify_barge_in_chunk(99.0 / 32768.0, 0.99, &p),
            ChunkClass::BelowFloor
        );
    }
}
