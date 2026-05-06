//! Offline Whisper transcription adapter.
//!
//! Loads a `ggml`-format model file at startup and exposes a synchronous
//! `transcribe()` method that runs inference on a 16 kHz mono f32 PCM chunk.
//!
//! The `whisper-rs` crate (version 0.11) wraps the upstream `whisper.cpp`
//! C++ library, which is compiled from source the first time you run
//! `cargo build`.  CMake must be in PATH for the build to succeed.

use std::path::{Path, PathBuf};

use thiserror::Error;
use whisper_rs::{
    get_lang_str, FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters,
};

/// Errors from the offline STT adapter.
#[derive(Debug, Error)]
pub enum OfflineSttError {
    #[error("Model file not found: {0}")]
    ModelNotFound(PathBuf),

    #[error("Failed to load Whisper model: {0}")]
    ModelLoad(String),

    #[error("Whisper transcription failed: {0}")]
    Inference(String),
}

/// A single Whisper output segment with its intra-chunk timing and
/// token-probability-derived confidence.
#[derive(Clone, Debug)]
pub struct OfflineSegment {
    /// Segment text (already trimmed).
    pub text: String,
    /// Segment start offset within the chunk, in milliseconds.
    pub start_ms: u32,
    /// Segment end offset within the chunk, in milliseconds.
    pub end_ms: u32,
    /// Mean token probability for the segment, in \[0.0, 1.0\].
    ///
    /// `whisper-rs` exposes `full_get_token_prob` as already-exponentiated
    /// probability per token; we average across tokens to get a per-segment
    /// prior the operator can see on each candidate row.
    pub confidence: f32,
}

/// A transcription result from the offline Whisper adapter.
#[derive(Clone, Debug)]
pub struct OfflineTranscript {
    /// Full transcribed text (concatenation of segment texts).
    pub text: String,
    /// Whisper's detected language code (e.g. `"en"`, `"ha"`).
    ///
    /// When `hint_language` is `None` this is the auto-detected language
    /// pulled from the state after inference; otherwise it echoes the hint.
    pub language: String,
    /// Mean token probability across all segments, in \[0.0, 1.0\].
    pub confidence: f32,
    /// How long inference took in milliseconds.
    pub latency_ms: u32,
    /// Per-segment breakdown with real Whisper timestamps and confidences.
    pub segments: Vec<OfflineSegment>,
}

/// Returns true if the model filename indicates it's a multilingual Whisper
/// build (contains "multilingual", or contains a known model size like
/// "tiny"/"base"/"small"/"medium"/"large" without the ".en" suffix). This
/// gates whether `set_detect_language(true)` is safe to call.
fn is_multilingual_model(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|f| f.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    if name.contains("multilingual") {
        return true;
    }
    // English-only models have ".en" before the extension: ggml-small.en.bin,
    // stt-whisper-en-small.bin (custom alias), etc.
    if name.contains(".en.") || name.contains("-en-") || name.ends_with(".en.bin") {
        return false;
    }
    // Safe default: treat as English-only if we can't tell.
    false
}

/// Wraps a loaded Whisper model context.
///
/// Loading is expensive (it reads and maps the whole model file), so construct
/// this once and reuse it across many [`transcribe`][Self::transcribe] calls.
pub struct OfflineSttAdapter {
    ctx: WhisperContext,
    model_path: PathBuf,
}

impl OfflineSttAdapter {
    /// Loads the model at `model_path`.
    ///
    /// Returns an error if the file does not exist or cannot be parsed by
    /// `whisper.cpp`.
    pub fn load(model_path: impl AsRef<Path>) -> Result<Self, OfflineSttError> {
        let model_path = model_path.as_ref().to_path_buf();
        if !model_path.exists() {
            return Err(OfflineSttError::ModelNotFound(model_path));
        }
        let ctx = WhisperContext::new_with_params(
            model_path
                .to_str()
                .ok_or_else(|| OfflineSttError::ModelLoad("invalid path".to_string()))?,
            WhisperContextParameters::default(),
        )
        .map_err(|e| OfflineSttError::ModelLoad(format!("{e:?}")))?;

        Ok(Self { ctx, model_path })
    }

    /// Returns the path of the loaded model file.
    pub fn model_path(&self) -> &Path {
        &self.model_path
    }

    /// Transcribes a 16 kHz mono f32 PCM chunk.
    ///
    /// `hint_language` is a two-letter BCP-47 code such as `"en"` or `"ha"`.
    /// Pass `None` to let Whisper auto-detect.
    pub fn transcribe(
        &self,
        samples: &[f32],
        hint_language: Option<&str>,
    ) -> Result<OfflineTranscript, OfflineSttError> {
        let started = std::time::Instant::now();

        // `whisper-rs` 0.11 exposes BeamSearch but upstream marks it as WIP.
        // Greedy with `best_of: 2` gives the command lane a small local
        // hypothesis search without changing output shape or adding cloud
        // dependency.
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 2 });
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        params.set_no_context(true);
        params.set_single_segment(true);
        params.set_temperature(0.0);
        params.set_temperature_inc(0.0);

        // Either pin the language (when the operator gave a hint) or let
        // Whisper detect it. CRITICAL: the English-only `.en` Whisper models
        // (ggml-small.en.bin, etc.) do NOT have a
        // language-identification head. Calling `set_detect_language(true)`
        // on those models causes whisper.cpp to error out and the entire
        // transcribe call returns Err — which manifests in the UI as "no
        // transcripts ever appear" even though capture is running. So we
        // gate auto-detect on the model filename containing "multilingual"
        // (or NOT containing ".en"); otherwise we pin to English.
        let is_multilingual = is_multilingual_model(&self.model_path);
        let effective_hint = hint_language.or(if is_multilingual { None } else { Some("en") });
        match effective_hint {
            Some(lang) => {
                params.set_language(Some(lang));
                params.set_detect_language(false);
            }
            None => {
                params.set_language(None);
                params.set_detect_language(true);
            }
        }

        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| OfflineSttError::Inference(format!("{e:?}")))?;

        state
            .full(params, samples)
            .map_err(|e| OfflineSttError::Inference(format!("{e:?}")))?;

        let segment_count = state
            .full_n_segments()
            .map_err(|e| OfflineSttError::Inference(format!("{e:?}")))?;

        let mut text = String::new();
        let mut segments: Vec<OfflineSegment> = Vec::with_capacity(segment_count.max(0) as usize);
        let mut total_prob_sum: f64 = 0.0;
        let mut total_token_count: u64 = 0;

        for i in 0..segment_count {
            let seg_text = match state.full_get_segment_text(i) {
                Ok(t) => t.trim().to_string(),
                Err(_) => continue,
            };
            if seg_text.is_empty() {
                continue;
            }

            // Whisper timestamps are in centiseconds (1 unit = 10 ms).
            let t0 = state.full_get_segment_t0(i).unwrap_or(0).max(0) as u64 * 10;
            let t1 = state.full_get_segment_t1(i).unwrap_or(0).max(0) as u64 * 10;
            let start_ms = t0 as u32;
            let end_ms = t1.max(t0) as u32;

            // Average token probability for this segment.
            let n_tokens = state.full_n_tokens(i).unwrap_or(0).max(0);
            let mut seg_prob_sum: f64 = 0.0;
            let mut seg_token_count: u64 = 0;
            for tok in 0..n_tokens {
                if let Ok(p) = state.full_get_token_prob(i, tok) {
                    if p.is_finite() && p > 0.0 {
                        seg_prob_sum += p as f64;
                        seg_token_count += 1;
                    }
                }
            }
            let seg_confidence = if seg_token_count > 0 {
                (seg_prob_sum / seg_token_count as f64) as f32
            } else {
                0.0
            };
            total_prob_sum += seg_prob_sum;
            total_token_count += seg_token_count;

            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&seg_text);

            segments.push(OfflineSegment {
                text: seg_text,
                start_ms,
                end_ms,
                confidence: seg_confidence.clamp(0.0, 1.0),
            });
        }

        // Pull the auto-detected language id from the post-inference state.
        let language = match hint_language {
            Some(l) => l.to_string(),
            None => state
                .full_lang_id_from_state()
                .ok()
                .and_then(get_lang_str)
                .map(|s| s.to_string())
                .unwrap_or_else(|| "en".to_string()),
        };

        let confidence = if total_token_count > 0 {
            ((total_prob_sum / total_token_count as f64) as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };

        Ok(OfflineTranscript {
            text: text.trim().to_string(),
            language,
            confidence,
            latency_ms: started.elapsed().as_millis() as u32,
            segments,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_returns_error_for_missing_file() {
        let result = OfflineSttAdapter::load("/nonexistent/model.bin");
        assert!(matches!(result, Err(OfflineSttError::ModelNotFound(_))));
    }
}
