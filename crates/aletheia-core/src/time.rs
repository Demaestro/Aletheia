//! Time primitives used across service events.

/// Milliseconds measured as a duration.
pub type Millis = u64;

/// Unix timestamp in milliseconds.
pub type TimestampMs = u64;

/// Returns a monotonic-ish wall-clock timestamp for event envelopes.
///
/// The app will later replace this with a Tauri-managed clock service so crash
/// recovery can reconcile monotonic and wall-clock times.
pub fn now_ms() -> TimestampMs {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as TimestampMs)
        .unwrap_or_default()
}
