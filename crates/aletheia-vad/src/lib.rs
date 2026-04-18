//! Hybrid voice activity detection.
//!
//! The production implementation will combine Silero VAD with this adaptive RMS
//! fallback. The fallback exists so noisy, offline, low-spec booths still keep a
//! deterministic detection path.

use aletheia_core::Millis;

/// Runtime VAD mode used for operator health and audit events.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VadMode {
    Silero,
    AdaptiveRmsFallback,
    Hybrid,
}

impl VadMode {
    /// Stable label for logs and domain events.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Silero => "silero",
            Self::AdaptiveRmsFallback => "adaptive-rms",
            Self::Hybrid => "hybrid",
        }
    }
}

/// Audio frame metrics produced before VAD scoring.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioFrameMetrics {
    pub rms: f32,
    pub peak: f32,
    pub zero_crossing_rate: f32,
    pub sample_count: usize,
    pub duration_ms: Millis,
}

impl AudioFrameMetrics {
    /// Measures normalized mono PCM samples.
    pub fn from_pcm(samples: &[f32], sample_rate_hz: u32) -> Self {
        if samples.is_empty() || sample_rate_hz == 0 {
            return Self {
                rms: 0.0,
                peak: 0.0,
                zero_crossing_rate: 0.0,
                sample_count: samples.len(),
                duration_ms: 0,
            };
        }

        let mut squared_sum = 0.0;
        let mut peak = 0.0_f32;
        let mut crossings = 0_u32;
        let mut previous = samples[0];

        for &sample in samples {
            let clamped = sample.clamp(-1.0, 1.0);
            squared_sum += clamped * clamped;
            peak = peak.max(clamped.abs());
            if (previous < 0.0 && clamped >= 0.0) || (previous >= 0.0 && clamped < 0.0) {
                crossings += 1;
            }
            previous = clamped;
        }

        let rms = (squared_sum / samples.len() as f32).sqrt();
        let zero_crossing_rate = crossings as f32 / samples.len() as f32;
        let duration_ms = (samples.len() as u64 * 1_000) / sample_rate_hz as u64;

        Self {
            rms,
            peak,
            zero_crossing_rate,
            sample_count: samples.len(),
            duration_ms,
        }
    }
}

/// VAD configuration tuned for conservative worship production use.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HybridVadConfig {
    pub mode: VadMode,
    pub speech_rms_multiplier: f32,
    pub min_speech_rms: f32,
    pub max_music_zcr: f32,
    pub noise_floor_alpha: f32,
    pub hangover_frames: u8,
}

impl Default for HybridVadConfig {
    fn default() -> Self {
        Self {
            mode: VadMode::Hybrid,
            speech_rms_multiplier: 2.8,
            min_speech_rms: 0.018,
            max_music_zcr: 0.22,
            noise_floor_alpha: 0.08,
            hangover_frames: 4,
        }
    }
}

/// VAD decision emitted for every analyzed frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VadDecision {
    pub speech_detected: bool,
    pub confidence: f32,
    pub noise_floor: f32,
    pub mode: VadMode,
}

/// Adaptive hybrid VAD.
#[derive(Debug)]
pub struct HybridVad {
    config: HybridVadConfig,
    noise_floor: f32,
    hangover_remaining: u8,
}

impl HybridVad {
    /// Creates a new detector with an initial conservative noise floor.
    pub fn new(config: HybridVadConfig) -> Self {
        Self {
            config,
            noise_floor: config.min_speech_rms / config.speech_rms_multiplier,
            hangover_remaining: 0,
        }
    }

    /// Scores a frame. Silero integration will feed its model confidence here
    /// later; today the deterministic fallback owns the decision.
    pub fn analyze(
        &mut self,
        metrics: AudioFrameMetrics,
        silero_confidence: Option<f32>,
    ) -> VadDecision {
        let adaptive_threshold =
            (self.noise_floor * self.config.speech_rms_multiplier).max(self.config.min_speech_rms);
        let fallback_confidence = self.fallback_confidence(metrics, adaptive_threshold);
        let confidence = match (self.config.mode, silero_confidence) {
            (VadMode::Silero, Some(model_confidence)) => model_confidence,
            (VadMode::Hybrid, Some(model_confidence)) => {
                (model_confidence * 0.72) + (fallback_confidence * 0.28)
            }
            _ => fallback_confidence,
        }
        .clamp(0.0, 1.0);

        let direct_speech = confidence >= 0.55
            && metrics.rms >= adaptive_threshold
            && metrics.zero_crossing_rate <= self.config.max_music_zcr;

        let speech_detected = if direct_speech {
            self.hangover_remaining = self.config.hangover_frames;
            true
        } else if self.hangover_remaining > 0 {
            self.hangover_remaining -= 1;
            true
        } else {
            false
        };

        if !speech_detected {
            self.update_noise_floor(metrics.rms);
        }

        VadDecision {
            speech_detected,
            confidence,
            noise_floor: self.noise_floor,
            mode: self.config.mode,
        }
    }

    /// Current adaptive noise floor.
    pub fn noise_floor(&self) -> f32 {
        self.noise_floor
    }

    fn fallback_confidence(&self, metrics: AudioFrameMetrics, adaptive_threshold: f32) -> f32 {
        if metrics.rms <= 0.0 || metrics.zero_crossing_rate > self.config.max_music_zcr {
            return 0.0;
        }

        let energy_ratio = metrics.rms / adaptive_threshold;
        ((energy_ratio - 0.65) / 1.6).clamp(0.0, 1.0)
    }

    fn update_noise_floor(&mut self, rms: f32) {
        let alpha = self.config.noise_floor_alpha.clamp(0.001, 0.5);
        self.noise_floor = (self.noise_floor * (1.0 - alpha)) + (rms.max(0.000_1) * alpha);
    }
}

impl Default for HybridVad {
    fn default() -> Self {
        Self::new(HybridVadConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_pcm_metrics() {
        let metrics = AudioFrameMetrics::from_pcm(&[0.0, 0.25, -0.25, 0.5, -0.5], 1_000);

        assert_eq!(metrics.sample_count, 5);
        assert_eq!(metrics.duration_ms, 5);
        assert!(metrics.rms > 0.3);
        assert_eq!(metrics.peak, 0.5);
        assert!(metrics.zero_crossing_rate > 0.4);
    }

    #[test]
    fn detects_speech_above_adaptive_threshold() {
        let mut vad = HybridVad::default();
        let metrics = AudioFrameMetrics {
            rms: 0.09,
            peak: 0.24,
            zero_crossing_rate: 0.08,
            sample_count: 480,
            duration_ms: 10,
        };

        let decision = vad.analyze(metrics, None);

        assert!(decision.speech_detected);
        assert!(decision.confidence >= 0.55);
    }

    #[test]
    fn rejects_music_like_high_crossing_frame() {
        let mut vad = HybridVad::default();
        let metrics = AudioFrameMetrics {
            rms: 0.12,
            peak: 0.35,
            zero_crossing_rate: 0.42,
            sample_count: 480,
            duration_ms: 10,
        };

        let decision = vad.analyze(metrics, None);

        assert!(!decision.speech_detected);
        assert_eq!(decision.confidence, 0.0);
    }
}
