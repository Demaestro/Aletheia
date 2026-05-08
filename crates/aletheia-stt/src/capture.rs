//! Microphone audio capture loop.
//!
//! Opens the default system input device using `cpal`, resamples to 16 kHz
//! mono f32 PCM (the format Whisper expects), and accumulates samples into
//! short overlapping chunks before sending each chunk through a `tokio` channel.
//!
//! ## Latency vs. accuracy tradeoff
//!
//! Whisper accuracy improves with longer audio context but every extra second
//! of audio is a second of transcription lag the operator sees. The default
//! configuration uses a sliding-window approach to balance both:
//!
//! - **`chunk_duration_secs = 3`** — each chunk is 3 s of audio. Whisper still
//!   sees enough context to produce high-quality output, and end-to-end
//!   transcription latency is ~3 s + inference (down from 5 s + inference).
//! - **`overlap_secs = 1`** — every chunk includes the last 1 s of the
//!   previous chunk so words spoken near a chunk boundary aren't cut in half.
//!   The capture stride is therefore `chunk_duration_secs - overlap_secs = 2 s`
//!   — a fresh chunk is emitted every 2 s. The consumer is responsible for
//!   suppressing segments that fall entirely within the overlap window
//!   (using `AudioChunk::overlap_ms`) so the same words don't appear twice
//!   in the transcript view.
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
    /// Total length of each emitted chunk, including overlap. Whisper sees
    /// this many seconds of audio per inference call. Default: 3 seconds.
    pub chunk_duration_secs: u32,
    /// How many seconds of the previous chunk are repeated at the start of
    /// the next chunk. Prevents Whisper from cutting words mid-syllable at
    /// chunk boundaries. The effective emit cadence is
    /// `chunk_duration_secs - overlap_secs`. Default: 1 second.
    /// Set to 0 to disable overlap entirely.
    pub overlap_secs: u32,
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
            chunk_duration_secs: 3,
            overlap_secs: 1,
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
    /// Milliseconds at the start of this chunk that overlap with the previous
    /// chunk. Whisper segments whose `end_ms <= overlap_ms` are duplicates
    /// of words already transcribed and should be dropped. `0` for the very
    /// first chunk in a session (no prior chunk to overlap with) or when
    /// overlap is disabled.
    pub overlap_ms: u32,
}

/// Handle to a running audio capture session.
///
/// Dropping this value stops the underlying cpal stream and closes the sender
/// side of the channel, which causes the receiver to return `None` once the
/// buffer is drained.
pub struct AudioCapture {
    // Keeping the stream alive — drop this to stop capture.
    _stream: cpal::Stream,
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

        let target_rate = config.target_sample_rate;

        // Buffer holds RESAMPLED MONO samples at target_rate (16 kHz), not
        // raw device samples — so the chunk-fill threshold must be measured
        // in target_rate samples regardless of what the device hardware does.
        // Earlier code used `device_rate * duration * channels`, which silently
        // multiplied the latency by `device_rate / target_rate` when those
        // differed (e.g. a 48 kHz / 2-ch laptop mic would only emit a chunk
        // every 30 seconds at the 5-second nominal setting). That bug masked
        // the latency win we get from a smaller chunk duration.
        let samples_per_chunk =
            (target_rate as usize).saturating_mul(config.chunk_duration_secs as usize);
        let overlap_samples =
            (target_rate as usize).saturating_mul(config.overlap_secs as usize);
        // Sanity: overlap must strictly precede chunk size, otherwise nothing
        // ever drains and the buffer grows without bound. Clamp here so a
        // misconfiguration becomes a smaller overlap rather than a deadlock.
        let overlap_samples = overlap_samples.min(samples_per_chunk.saturating_sub(1));
        let stride_samples = samples_per_chunk - overlap_samples;
        let overlap_ms = (config.overlap_secs.saturating_mul(1_000)).min(
            (config.chunk_duration_secs.saturating_sub(1)).saturating_mul(1_000),
        );

        // Shared accumulator — lives inside the stream callback closure.
        let buffer = std::sync::Arc::new(std::sync::Mutex::new(Vec::<f32>::with_capacity(
            samples_per_chunk + 1024,
        )));
        // Track whether the next emitted chunk is the very first in this
        // session. The first chunk has no prior chunk to overlap with, so its
        // overlap_ms is reported as 0 even when overlap is configured.
        let first_chunk = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));

        let buffer_clone = buffer.clone();
        let first_clone = first_chunk.clone();
        let tx_clone = tx.clone();

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
                            stride_samples,
                            overlap_ms,
                            &buffer_clone,
                            &first_clone,
                            &tx_clone,
                        );
                    },
                    |e| eprintln!("[aletheia-stt] capture error: {e}"),
                    None,
                )
                .map_err(|e| CaptureError::StreamBuild(e.to_string()))?,

            cpal::SampleFormat::I16 => {
                let buffer_clone2 = buffer.clone();
                let first_clone2 = first_chunk.clone();
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
                                stride_samples,
                                overlap_ms,
                                &buffer_clone2,
                                &first_clone2,
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
                let first_clone3 = first_chunk.clone();
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
                                stride_samples,
                                overlap_ms,
                                &buffer_clone3,
                                &first_clone3,
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

        Ok((Self { _stream: stream }, rx))
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Converts `data` (any sample rate, any channel count) to 16 kHz mono,
/// accumulates into `buffer`, and emits sealed chunks with the configured
/// overlap.
///
/// Each emitted chunk contains exactly `samples_per_chunk` samples. After
/// emitting we drain `stride_samples` from the front of the buffer (where
/// `stride = chunk - overlap`), so the *last* `overlap_samples` of the chunk
/// remain in the buffer and become the *first* `overlap_samples` of the next
/// chunk. This gives Whisper word-boundary context across chunk seams.
#[allow(clippy::too_many_arguments)]
fn process_samples(
    data: &[f32],
    device_rate: u32,
    channels: usize,
    target_rate: u32,
    samples_per_chunk: usize,
    stride_samples: usize,
    overlap_ms: u32,
    buffer: &std::sync::Mutex<Vec<f32>>,
    first_chunk: &std::sync::atomic::AtomicBool,
    tx: &mpsc::Sender<AudioChunk>,
) {
    // Defensive — should be enforced at construction.
    if samples_per_chunk == 0 || stride_samples == 0 || channels == 0 {
        return;
    }

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
        // Copy a full chunk-sized window — do NOT drain it yet, we need to
        // retain the overlap tail in the buffer for the next chunk.
        let chunk_samples: Vec<f32> = buf[..samples_per_chunk].to_vec();
        // Drain only the non-overlapping prefix. The remaining
        // `samples_per_chunk - stride_samples == overlap_samples` samples
        // become the head of the next chunk.
        buf.drain(..stride_samples);

        let is_first = first_chunk.swap(false, std::sync::atomic::Ordering::Relaxed);
        let chunk = AudioChunk {
            samples: chunk_samples,
            sealed_at_ms: now_ms(),
            // First chunk has no prior to overlap with — report 0 so the
            // consumer doesn't wrongly suppress its leading segments.
            overlap_ms: if is_first { 0 } else { overlap_ms },
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

    /// First chunk reports `overlap_ms == 0` because nothing came before;
    /// subsequent chunks carry the configured overlap so the consumer can
    /// dedup their leading segments.
    #[test]
    fn first_chunk_reports_no_overlap_then_subsequent_chunks_do() {
        use std::sync::Arc;
        use std::sync::atomic::AtomicBool;

        // 16 kHz, mono, samples_per_chunk = 100, stride = 60 → overlap = 40,
        // overlap_ms = 40 * 1000 / 16_000 == 2.5 → round to 2 ms (we'll just
        // pass the configured value through).
        let samples_per_chunk = 100usize;
        let stride_samples = 60usize;
        let overlap_ms = 200u32;
        let buffer = Arc::new(std::sync::Mutex::new(Vec::<f32>::new()));
        let first = Arc::new(AtomicBool::new(true));
        let (tx, mut rx) = mpsc::channel::<AudioChunk>(8);

        // Pump 220 mono samples in one shot — enough for two chunks given
        // stride=60 (need samples_per_chunk first, then samples_per_chunk
        // again after draining stride).
        let data: Vec<f32> = (0..220).map(|i| (i as f32) / 220.0).collect();
        process_samples(
            &data,
            16_000, // device_rate
            1,      // channels
            16_000, // target_rate
            samples_per_chunk,
            stride_samples,
            overlap_ms,
            &buffer,
            &first,
            &tx,
        );

        let chunk1 = rx.try_recv().expect("first chunk should emit");
        assert_eq!(chunk1.samples.len(), samples_per_chunk);
        assert_eq!(chunk1.overlap_ms, 0, "first chunk has no prior to overlap with");

        let chunk2 = rx.try_recv().expect("second chunk should emit");
        assert_eq!(chunk2.samples.len(), samples_per_chunk);
        assert_eq!(
            chunk2.overlap_ms, overlap_ms,
            "subsequent chunks carry the configured overlap"
        );

        // Assert the overlap REGION matches: chunk2's first
        // `samples_per_chunk - stride_samples` samples should equal chunk1's
        // tail. This is the actual word-boundary continuity guarantee.
        let overlap_samples = samples_per_chunk - stride_samples;
        let chunk1_tail = &chunk1.samples[chunk1.samples.len() - overlap_samples..];
        let chunk2_head = &chunk2.samples[..overlap_samples];
        for (a, b) in chunk1_tail.iter().zip(chunk2_head.iter()) {
            assert!((a - b).abs() < 1e-6, "overlap region must be byte-identical");
        }
    }

    /// With `overlap_secs == 0` the buffer drains a full chunk each time and
    /// no samples are repeated — degenerate sliding window.
    #[test]
    fn zero_overlap_falls_back_to_non_overlapping_chunks() {
        use std::sync::Arc;
        use std::sync::atomic::AtomicBool;

        let samples_per_chunk = 50usize;
        let stride_samples = 50usize; // == samples_per_chunk
        let buffer = Arc::new(std::sync::Mutex::new(Vec::<f32>::new()));
        let first = Arc::new(AtomicBool::new(true));
        let (tx, mut rx) = mpsc::channel::<AudioChunk>(8);

        let data: Vec<f32> = (0..100).map(|i| i as f32).collect();
        process_samples(
            &data, 16_000, 1, 16_000, samples_per_chunk, stride_samples, 0, &buffer, &first, &tx,
        );

        let c1 = rx.try_recv().unwrap();
        let c2 = rx.try_recv().unwrap();
        // Disjoint — chunk2's head is a fresh sample, not chunk1's tail.
        assert!((c1.samples.last().unwrap() - c2.samples[0]).abs() > 0.5);
        assert_eq!(c1.overlap_ms, 0);
        assert_eq!(c2.overlap_ms, 0);
    }
}
