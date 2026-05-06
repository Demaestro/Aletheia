//! Microphone audio capture loop.
//!
//! Opens the default system input device using `cpal`, resamples to 16 kHz
//! mono f32 PCM (the format Whisper expects), and accumulates samples into
//! 30-second chunks before sending each chunk through a `tokio` channel.
//!
//! # Usage
//! ```rust,no_run
//! use aletheia_stt::capture::{AudioCapture, CaptureConfig};
//!
//! let config = CaptureConfig::default();
//! let (capture, mut receiver) = AudioCapture::start(config).unwrap();
//!
//! tokio::spawn(async move {
//!     while let Some(chunk) = receiver.recv().await {
//!         // chunk.samples is a Vec<f32> at 16 kHz mono
//!         println!("Received {} samples", chunk.samples.len());
//!     }
//! });
//!
//! // Later, to stop capturing:
//! drop(capture);
//! ```

use cpal::traits::StreamTrait;

use thiserror::Error;
use tokio::sync::mpsc;

/// Errors from the capture subsystem.
#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("No default input device is available")]
    NoInputDevice,

    #[error("Could not build audio stream: {0}")]
    StreamBuild(String),

    #[error("Audio stream error: {0}")]
    StreamError(String),

    #[error("Unsupported sample format from device")]
    UnsupportedFormat,
}

/// Configuration for the audio capture loop.
#[derive(Clone, Debug)]
pub struct CaptureConfig {
    /// Target sample rate for Whisper (16 000 Hz).
    pub target_sample_rate: u32,
    /// How many milliseconds of audio to accumulate before emitting a chunk.
    /// Shorter chunks reduce latency; longer chunks improve accuracy.
    /// Default: 5000 ms (sub-10-second first-word latency).
    pub chunk_duration_ms: u32,
    /// Internal channel buffer: how many chunks can queue up before the
    /// capture loop blocks.  Default: 4.
    pub channel_capacity: usize,
    /// Optional: name of the input device to open.  `None` uses the OS default.
    pub device_name: Option<String>,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            target_sample_rate: 16_000,
            chunk_duration_ms: 5_000,
            channel_capacity: 4,
            device_name: None,
        }
    }
}

/// Returns the names of all available audio input devices on this host.
///
/// Falls back to `default_input_device()` if full enumeration fails or returns
/// an empty list — some Windows audio drivers (notably Intel SST) silently
/// fail enumeration even when a usable default mic exists. This guarantees the
/// operator's mic picker is never empty when a default device is reachable.
pub fn list_input_devices() -> Vec<String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    let mut out: Vec<String> = match host.input_devices() {
        Ok(devices) => devices.filter_map(|d| d.name().ok()).collect(),
        Err(e) => {
            eprintln!("[aletheia-stt] input_devices enumeration failed: {e}");
            Vec::new()
        }
    };
    if let Some(default_name) = host.default_input_device().and_then(|d| d.name().ok()) {
        if !out.iter().any(|n| n == &default_name) {
            out.insert(0, default_name);
        }
    }
    out
}

/// A chunk of 16 kHz mono f32 PCM samples ready for transcription.
#[derive(Clone, Debug)]
pub struct AudioChunk {
    /// Raw samples: 16 kHz, mono, f32 [-1.0, 1.0].
    pub samples: Vec<f32>,
    /// Wall-clock milliseconds when the chunk was sealed.
    pub sealed_at_ms: u64,
}

/// Handle to a running audio capture session.
///
/// Dropping this value stops the underlying cpal stream and closes the sender
/// side of the channel, which causes the receiver to return `None` once the
/// buffer is drained.
pub struct AudioCapture {
    // Keeping the stream alive — drop this to stop capture.
    _stream: cpal::Stream,
    // Keep one sender alive with the stream. Some Windows host backends can
    // delay callback registration; without this guard the receiver can observe
    // a disconnected channel and stop capture even though the stream opened.
    _tx: mpsc::Sender<AudioChunk>,
}

impl AudioCapture {
    /// Starts capturing from the system input device specified in `config.device_name`,
    /// falling back to the OS default if the name is `None` or not found.
    ///
    /// Returns the capture handle and a receiver channel that yields
    /// [`AudioChunk`] values.  Drop the handle to stop the stream.
    pub fn start(
        config: CaptureConfig,
    ) -> Result<(Self, mpsc::Receiver<AudioChunk>), CaptureError> {
        use cpal::traits::{DeviceTrait, HostTrait};
        let host = cpal::default_host();

        let device = if let Some(ref name) = config.device_name {
            // Try to find the named device; fall back to default if not found.
            host.input_devices()
                .ok()
                .and_then(|mut it| it.find(|d| d.name().ok().as_deref() == Some(name.as_str())))
                .or_else(|| host.default_input_device())
                .ok_or(CaptureError::NoInputDevice)?
        } else {
            host.default_input_device()
                .ok_or(CaptureError::NoInputDevice)?
        };

        let device_config = device
            .default_input_config()
            .map_err(|e| CaptureError::StreamBuild(e.to_string()))?;

        let device_sample_rate = device_config.sample_rate().0;
        let device_channels = device_config.channels() as usize;

        let (tx, rx) = mpsc::channel::<AudioChunk>(config.channel_capacity);

        // The accumulator stores 16 kHz mono samples after mixdown/resampling,
        // so the seal threshold must use the target sample rate. Using the
        // device rate * channel count makes a 2s chunk become ~12s on common
        // 48 kHz stereo laptop inputs.
        let samples_per_chunk =
            ((config.target_sample_rate as usize) * (config.chunk_duration_ms as usize) / 1000)
                .max(1);

        // Shared accumulator — lives inside the stream callback closure.
        let buffer = std::sync::Arc::new(std::sync::Mutex::new(Vec::<f32>::with_capacity(
            samples_per_chunk + 1024,
        )));

        let buffer_clone = buffer.clone();
        let tx_clone = tx.clone();
        let target_rate = config.target_sample_rate;

        let stream = match device_config.sample_format() {
            cpal::SampleFormat::F32 => device
                .build_input_stream(
                    &device_config.into(),
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        process_samples(
                            data,
                            device_sample_rate,
                            device_channels,
                            target_rate,
                            samples_per_chunk,
                            &buffer_clone,
                            &tx_clone,
                        );
                    },
                    |e| eprintln!("[aletheia-stt] capture error: {e}"),
                    None,
                )
                .map_err(|e| CaptureError::StreamBuild(e.to_string()))?,

            cpal::SampleFormat::I16 => {
                let buffer_clone2 = buffer.clone();
                device
                    .build_input_stream(
                        &device_config.into(),
                        move |data: &[i16], _: &cpal::InputCallbackInfo| {
                            let floats: Vec<f32> =
                                data.iter().map(|s| f32::from(*s) / 32_768.0).collect();
                            process_samples(
                                &floats,
                                device_sample_rate,
                                device_channels,
                                target_rate,
                                samples_per_chunk,
                                &buffer_clone2,
                                &tx_clone,
                            );
                        },
                        |e| eprintln!("[aletheia-stt] capture error: {e}"),
                        None,
                    )
                    .map_err(|e| CaptureError::StreamBuild(e.to_string()))?
            }

            cpal::SampleFormat::U16 => {
                let buffer_clone3 = buffer.clone();
                device
                    .build_input_stream(
                        &device_config.into(),
                        move |data: &[u16], _: &cpal::InputCallbackInfo| {
                            let floats: Vec<f32> = data
                                .iter()
                                .map(|s| (f32::from(*s) / 32_768.0) - 1.0)
                                .collect();
                            process_samples(
                                &floats,
                                device_sample_rate,
                                device_channels,
                                target_rate,
                                samples_per_chunk,
                                &buffer_clone3,
                                &tx_clone,
                            );
                        },
                        |e| eprintln!("[aletheia-stt] capture error: {e}"),
                        None,
                    )
                    .map_err(|e| CaptureError::StreamBuild(e.to_string()))?
            }

            _ => return Err(CaptureError::UnsupportedFormat),
        };

        stream
            .play()
            .map_err(|e| CaptureError::StreamBuild(e.to_string()))?;

        Ok((
            Self {
                _stream: stream,
                _tx: tx,
            },
            rx,
        ))
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Converts `data` (any sample rate, any channel count) to 16 kHz mono,
/// accumulates into `buffer`, and sends a sealed chunk when it fills up.
fn process_samples(
    data: &[f32],
    device_rate: u32,
    channels: usize,
    target_rate: u32,
    samples_per_chunk: usize,
    buffer: &std::sync::Mutex<Vec<f32>>,
    tx: &mpsc::Sender<AudioChunk>,
) {
    // Step 1: mix down to mono.
    let mono: Vec<f32> = data
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();

    // Step 2: naive linear resample to target_rate if necessary.
    let resampled: Vec<f32> = if device_rate == target_rate {
        mono
    } else {
        resample_linear(&mono, device_rate, target_rate)
    };

    // Step 3: accumulate and drain complete chunks.
    let mut buf = match buffer.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    buf.extend_from_slice(&resampled);

    while buf.len() >= samples_per_chunk {
        let chunk_samples: Vec<f32> = buf.drain(..samples_per_chunk).collect();
        let chunk = AudioChunk {
            samples: chunk_samples,
            sealed_at_ms: now_ms(),
        };
        // Non-blocking send: if the channel is full we discard the chunk
        // rather than blocking the audio callback thread.
        let _ = tx.try_send(chunk);
    }
}

/// Linear interpolation resample — fast, good enough for speech at 16 kHz.
fn resample_linear(input: &[f32], in_rate: u32, out_rate: u32) -> Vec<f32> {
    if input.is_empty() {
        return Vec::new();
    }
    let ratio = in_rate as f64 / out_rate as f64;
    let out_len = ((input.len() as f64) / ratio).ceil() as usize;
    let mut output = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_pos = (i as f64) * ratio;
        let src_idx = src_pos as usize;
        let frac = src_pos - src_idx as f64;
        let a = input[src_idx.min(input.len() - 1)];
        let b = input[(src_idx + 1).min(input.len() - 1)];
        output.push(a + (b - a) * frac as f32);
    }
    output
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_linear_doubles_length_at_half_rate() {
        let input: Vec<f32> = (0..16).map(|i| i as f32).collect();
        // Resample from 8 kHz to 16 kHz should roughly double the output length.
        let output = resample_linear(&input, 8_000, 16_000);
        // Allow ± 2 samples for rounding.
        assert!((output.len() as isize - (input.len() as isize * 2)).abs() <= 2);
    }

    #[test]
    fn resample_linear_halves_length_at_double_rate() {
        let input: Vec<f32> = (0..32).map(|i| i as f32).collect();
        // Resample from 32 kHz to 16 kHz should roughly halve the output.
        let output = resample_linear(&input, 32_000, 16_000);
        assert!((output.len() as isize - (input.len() as isize / 2)).abs() <= 2);
    }

    #[test]
    fn resample_linear_passthrough_when_rates_equal() {
        let input = vec![0.1_f32, 0.2, 0.3, 0.4];
        let output = resample_linear(&input, 16_000, 16_000);
        assert_eq!(output.len(), input.len());
        for (a, b) in input.iter().zip(output.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }
}
