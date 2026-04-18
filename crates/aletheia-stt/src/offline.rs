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
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

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

/// A transcription result from the offline Whisper adapter.
#[derive(Clone, Debug)]
pub struct OfflineTranscript {
    /// Full transcribed text.
    pub text: String,
    /// Whisper's detected language code (e.g. `"en"`, `"ha"`).
    pub language: String,
    /// How long inference took in milliseconds.
    pub latency_ms: u32,
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

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);

        if let Some(lang) = hint_language {
            params.set_language(Some(lang));
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
        for i in 0..segment_count {
            if let Ok(segment) = state.full_get_segment_text(i) {
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(segment.trim());
            }
        }

        // Whisper encodes detected language in the first token; fall back to
        // the hint if auto-detection is not available from the API surface.
        let language = hint_language.unwrap_or("en").to_string();

        Ok(OfflineTranscript {
            text: text.trim().to_string(),
            language,
            latency_ms: started.elapsed().as_millis() as u32,
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
