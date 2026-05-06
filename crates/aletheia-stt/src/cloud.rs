//! Cloud STT fallback adapter.
//!
//! Sends a WAV audio buffer to the OpenAI Whisper API (or a compatible
//! endpoint) and returns a single transcript string.  The API key is passed in
//! by the caller — this crate never touches the OS vault directly so it stays
//! testable without live credentials.
//!
//! # Selecting a provider
//! Set `ALETHEIA_CLOUD_STT_PROVIDER` to `"openai"` (default) or
//! `"assemblyai"`.  Both accept the same `api_key` argument.

use std::io::Cursor;

use reqwest::multipart;
use thiserror::Error;

/// Errors that can occur during a cloud STT call.
#[derive(Debug, Error)]
pub enum CloudSttError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("WAV encoding failed: {0}")]
    WavEncoding(String),

    #[error("API returned an error ({status}): {body}")]
    ApiError { status: u16, body: String },

    #[error("API response could not be parsed: {0}")]
    ParseError(String),
}

/// A transcribed segment returned by the cloud API.
#[derive(Clone, Debug)]
pub struct CloudTranscript {
    /// Transcribed text (may span multiple sentences).
    pub text: String,
    /// Reported language BCP-47 code (e.g. `"en"`, `"ha"`).
    /// `None` when the API does not include language detection.
    pub language: Option<String>,
    /// Round-trip latency in milliseconds measured by this call.
    pub latency_ms: u32,
}

/// Thin async wrapper around the OpenAI Whisper transcription endpoint.
///
/// A new client is constructed per call, which is cheap — the underlying
/// `reqwest::Client` pool is not shared here intentionally: each cloud STT
/// call is a one-shot operation that should succeed or fail independently.
pub struct CloudSttAdapter {
    api_key: String,
    provider: CloudSttProvider,
    timeout_ms: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum CloudSttProvider {
    #[default]
    OpenAi,
    AssemblyAi,
}

impl CloudSttAdapter {
    /// Creates a new adapter.
    ///
    /// `api_key` is the provider secret read from the OS vault by the caller.
    /// `timeout_ms` is the per-request timeout; 10 000 ms is a safe default
    /// for a 30-second audio chunk.
    pub fn new(api_key: impl Into<String>, provider: CloudSttProvider, timeout_ms: u32) -> Self {
        Self {
            api_key: api_key.into(),
            provider,
            timeout_ms,
        }
    }

    /// Transcribes `samples` (16 kHz mono f32 PCM) by uploading them as a WAV
    /// file to the configured cloud endpoint.
    ///
    /// Returns a [`CloudTranscript`] on success, or a [`CloudSttError`] if the
    /// network call fails or the API returns a non-2xx status.
    pub async fn transcribe(
        &self,
        samples: &[f32],
        hint_language: Option<&str>,
    ) -> Result<CloudTranscript, CloudSttError> {
        let wav_bytes = encode_wav(samples, 16_000)?;
        let started_at = std::time::Instant::now();

        match self.provider {
            CloudSttProvider::OpenAi => {
                self.transcribe_openai(wav_bytes, hint_language, started_at)
                    .await
            }
            CloudSttProvider::AssemblyAi => self.transcribe_assemblyai(wav_bytes, started_at).await,
        }
    }

    // -----------------------------------------------------------------------
    // OpenAI Whisper API  (POST /v1/audio/transcriptions)
    // -----------------------------------------------------------------------

    async fn transcribe_openai(
        &self,
        wav_bytes: Vec<u8>,
        hint_language: Option<&str>,
        started_at: std::time::Instant,
    ) -> Result<CloudTranscript, CloudSttError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(u64::from(self.timeout_ms)))
            .build()?;

        let mut form = multipart::Form::new()
            .text("model", "whisper-1")
            .text("response_format", "json")
            .part(
                "file",
                multipart::Part::bytes(wav_bytes)
                    .file_name("audio.wav")
                    .mime_str("audio/wav")
                    .map_err(|e| CloudSttError::ParseError(e.to_string()))?,
            );

        if let Some(lang) = hint_language {
            form = form.text("language", lang.to_string());
        }

        let response = client
            .post("https://api.openai.com/v1/audio/transcriptions")
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await?;

        let status = response.status().as_u16();
        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(CloudSttError::ApiError { status, body });
        }

        #[derive(serde::Deserialize)]
        struct OpenAiResponse {
            text: String,
        }

        let parsed: OpenAiResponse = response
            .json()
            .await
            .map_err(|e| CloudSttError::ParseError(e.to_string()))?;

        Ok(CloudTranscript {
            text: parsed.text.trim().to_string(),
            language: hint_language.map(str::to_string),
            latency_ms: started_at.elapsed().as_millis() as u32,
        })
    }

    // -----------------------------------------------------------------------
    // AssemblyAI  (POST /v2/transcript with audio_url or direct upload)
    // -----------------------------------------------------------------------

    async fn transcribe_assemblyai(
        &self,
        wav_bytes: Vec<u8>,
        started_at: std::time::Instant,
    ) -> Result<CloudTranscript, CloudSttError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(u64::from(self.timeout_ms)))
            .build()?;

        // Step 1: upload the audio file and receive a temporary URL.
        let upload_response = client
            .post("https://api.assemblyai.com/v2/upload")
            .header("authorization", &self.api_key)
            .header("content-type", "audio/wav")
            .body(wav_bytes)
            .send()
            .await?;

        if !upload_response.status().is_success() {
            let status = upload_response.status().as_u16();
            let body = upload_response.text().await.unwrap_or_default();
            return Err(CloudSttError::ApiError { status, body });
        }

        #[derive(serde::Deserialize)]
        struct UploadResponse {
            upload_url: String,
        }

        let upload: UploadResponse = upload_response
            .json()
            .await
            .map_err(|e| CloudSttError::ParseError(e.to_string()))?;

        // Step 2: submit a transcription job.
        let job_response = client
            .post("https://api.assemblyai.com/v2/transcript")
            .header("authorization", &self.api_key)
            .json(&serde_json::json!({
                "audio_url": upload.upload_url,
                "language_detection": true,
            }))
            .send()
            .await?;

        if !job_response.status().is_success() {
            let status = job_response.status().as_u16();
            let body = job_response.text().await.unwrap_or_default();
            return Err(CloudSttError::ApiError { status, body });
        }

        #[derive(serde::Deserialize)]
        struct JobResponse {
            id: String,
        }
        let job: JobResponse = job_response
            .json()
            .await
            .map_err(|e| CloudSttError::ParseError(e.to_string()))?;

        // Step 3: poll until status is "completed" or "error".
        let poll_url = format!("https://api.assemblyai.com/v2/transcript/{}", job.id);
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(1_500)).await;

            let poll = client
                .get(&poll_url)
                .header("authorization", &self.api_key)
                .send()
                .await?;

            #[derive(serde::Deserialize)]
            struct PollResponse {
                status: String,
                text: Option<String>,
                language_code: Option<String>,
                error: Option<String>,
            }

            let result: PollResponse = poll
                .json()
                .await
                .map_err(|e| CloudSttError::ParseError(e.to_string()))?;

            match result.status.as_str() {
                "completed" => {
                    return Ok(CloudTranscript {
                        text: result.text.unwrap_or_default().trim().to_string(),
                        language: result.language_code,
                        latency_ms: started_at.elapsed().as_millis() as u32,
                    });
                }
                "error" => {
                    return Err(CloudSttError::ApiError {
                        status: 200,
                        body: result.error.unwrap_or_else(|| "unknown error".to_string()),
                    });
                }
                _ => {
                    // Still processing — keep polling.
                    if started_at.elapsed().as_millis() > u128::from(self.timeout_ms) {
                        return Err(CloudSttError::ApiError {
                            status: 408,
                            body: "AssemblyAI transcription timed out".to_string(),
                        });
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// WAV encoding helper
// ---------------------------------------------------------------------------

/// Encodes raw f32 PCM samples (mono, `sample_rate` Hz) into an in-memory WAV
/// byte buffer suitable for uploading to cloud transcription APIs.
fn encode_wav(samples: &[f32], sample_rate: u32) -> Result<Vec<u8>, CloudSttError> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let mut buf: Vec<u8> = Vec::with_capacity(samples.len() * 4 + 44);
    {
        let cursor = Cursor::new(&mut buf);
        let mut writer = hound::WavWriter::new(cursor, spec)
            .map_err(|e| CloudSttError::WavEncoding(e.to_string()))?;
        for &sample in samples {
            writer
                .write_sample(sample)
                .map_err(|e| CloudSttError::WavEncoding(e.to_string()))?;
        }
        writer
            .finalize()
            .map_err(|e| CloudSttError::WavEncoding(e.to_string()))?;
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_wav_produces_valid_header() {
        // 1 second of silence at 16 kHz
        let samples = vec![0.0_f32; 16_000];
        let bytes = encode_wav(&samples, 16_000).expect("wav encoding succeeds");

        // WAV header starts with RIFF
        assert_eq!(&bytes[0..4], b"RIFF");
        // Byte 8..12 must be WAVE
        assert_eq!(&bytes[8..12], b"WAVE");
        // File should be larger than just the 44-byte header
        assert!(bytes.len() > 44);
    }
}
