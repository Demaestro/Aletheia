//! Audio ingest service boundary.
//!
//! This crate intentionally starts as a pure Rust state machine. The production
//! adapter will add `cpal` behind this boundary once the host has the required C
//! toolchain and audio libraries.

use aletheia_core::{
    DeviceId, DomainEvent, EventLog, HealthImpact, HealthReport, HealthState, Millis,
    ServiceSessionId, now_ms,
};
use aletheia_vad::{AudioFrameMetrics, HybridVad, VadDecision};

/// Audio ingest lifecycle state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AudioIngestState {
    Stopped,
    Starting,
    Listening,
    Degraded(String),
    Recovering(String),
    Failed(String),
}

/// Static device capture configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioDeviceConfig {
    pub device_id: DeviceId,
    pub display_name: String,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub frame_duration_ms: Millis,
}

impl AudioDeviceConfig {
    /// Creates a mono pulpit-mic profile that works as a safe default.
    pub fn pulpit_mic(device_id: DeviceId, display_name: impl Into<String>) -> Self {
        Self {
            device_id,
            display_name: display_name.into(),
            sample_rate_hz: 48_000,
            channels: 1,
            frame_duration_ms: 20,
        }
    }
}

/// Audio ingest service with explicit state transitions.
pub struct AudioIngestService<L: EventLog> {
    session_id: ServiceSessionId,
    config: AudioDeviceConfig,
    state: AudioIngestState,
    vad: HybridVad,
    event_log: L,
}

impl<L: EventLog> AudioIngestService<L> {
    /// Creates a stopped audio service.
    pub fn new(
        session_id: ServiceSessionId,
        config: AudioDeviceConfig,
        vad: HybridVad,
        event_log: L,
    ) -> Self {
        Self {
            session_id,
            config,
            state: AudioIngestState::Stopped,
            vad,
            event_log,
        }
    }

    /// Starts capture and emits auditable lifecycle events.
    pub fn start(&mut self) -> Result<(), AudioIngestError> {
        match self.state {
            AudioIngestState::Stopped | AudioIngestState::Recovering(_) => {
                self.state = AudioIngestState::Starting;
                self.event_log.append(DomainEvent::AudioCaptureStarting {
                    session_id: self.session_id.clone(),
                    device_id: self.config.device_id.clone(),
                });
                self.state = AudioIngestState::Listening;
                self.event_log.append(DomainEvent::AudioCaptureStarted {
                    session_id: self.session_id.clone(),
                    device_id: self.config.device_id.clone(),
                    sample_rate_hz: self.config.sample_rate_hz,
                    channels: self.config.channels,
                });
                Ok(())
            }
            AudioIngestState::Listening | AudioIngestState::Starting => {
                Err(AudioIngestError::AlreadyListening)
            }
            AudioIngestState::Degraded(_) | AudioIngestState::Failed(_) => {
                Err(AudioIngestError::RequiresRecovery)
            }
        }
    }

    /// Stops capture without losing the event log.
    pub fn stop(&mut self, reason: impl Into<String>) {
        let reason = reason.into();
        self.state = AudioIngestState::Stopped;
        self.event_log.append(DomainEvent::AudioCaptureStopped {
            session_id: self.session_id.clone(),
            reason,
        });
    }

    /// Processes one normalized mono PCM frame.
    pub fn process_pcm_frame(&mut self, samples: &[f32]) -> Result<VadDecision, AudioIngestError> {
        if self.state != AudioIngestState::Listening {
            return Err(AudioIngestError::NotListening);
        }

        let metrics = AudioFrameMetrics::from_pcm(samples, self.config.sample_rate_hz);
        let decision = self.vad.analyze(metrics, None);

        self.event_log.append(DomainEvent::AudioFrameMeasured {
            session_id: self.session_id.clone(),
            rms: metrics.rms,
            peak: metrics.peak,
            zero_crossing_rate: metrics.zero_crossing_rate,
            frame_duration_ms: metrics.duration_ms,
        });
        self.event_log.append(DomainEvent::VadDecisionMade {
            session_id: self.session_id.clone(),
            speech_detected: decision.speech_detected,
            confidence: decision.confidence,
            mode: decision.mode.as_str(),
        });

        Ok(decision)
    }

    /// Marks the service degraded but not stopped.
    pub fn degrade(&mut self, message: impl Into<String>, operator_action: impl Into<String>) {
        let message = message.into();
        self.state = AudioIngestState::Degraded(message.clone());
        self.event_log
            .append(DomainEvent::HealthChanged(HealthReport::new(
                "audio-ingest",
                HealthState::Degraded,
                HealthImpact::DegradesDetection,
                message,
                operator_action,
                now_ms(),
            )));
    }

    /// Moves the service into a recoverable state after device drop or power loss.
    pub fn recover(&mut self, reason: impl Into<String>) {
        self.state = AudioIngestState::Recovering(reason.into());
    }

    /// Current lifecycle state.
    pub fn state(&self) -> &AudioIngestState {
        &self.state
    }

    /// Immutable access to the event log.
    pub fn event_log(&self) -> &L {
        &self.event_log
    }
}

/// Audio ingest transition errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioIngestError {
    AlreadyListening,
    NotListening,
    RequiresRecovery,
}

impl std::fmt::Display for AudioIngestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyListening => formatter.write_str("audio ingest is already listening"),
            Self::NotListening => formatter.write_str("audio ingest is not listening"),
            Self::RequiresRecovery => {
                formatter.write_str("audio ingest requires recovery before start")
            }
        }
    }
}

impl std::error::Error for AudioIngestError {}

#[cfg(test)]
mod tests {
    use super::*;
    use aletheia_core::{InMemoryEventLog, events::EventLog};

    fn service() -> AudioIngestService<InMemoryEventLog> {
        let session_id = ServiceSessionId::new("sunday-am").expect("valid session id");
        let device_id = DeviceId::new("focusrite-usb").expect("valid device id");
        AudioIngestService::new(
            session_id,
            AudioDeviceConfig::pulpit_mic(device_id, "Focusrite USB"),
            HybridVad::default(),
            InMemoryEventLog::default(),
        )
    }

    #[test]
    fn starts_from_stopped_state() {
        let mut service = service();

        service.start().expect("service starts");

        assert_eq!(service.state(), &AudioIngestState::Listening);
        assert_eq!(service.event_log().all().len(), 2);
    }

    #[test]
    fn rejects_frames_before_start() {
        let mut service = service();

        let result = service.process_pcm_frame(&[0.0; 128]);

        assert_eq!(result, Err(AudioIngestError::NotListening));
    }

    #[test]
    fn emits_measurement_and_vad_events_for_frame() {
        let mut service = service();
        service.start().expect("service starts");

        let samples = vec![0.08; 960];
        let decision = service
            .process_pcm_frame(&samples)
            .expect("frame processed");

        assert!(decision.speech_detected);
        assert_eq!(service.event_log().all().len(), 4);
    }

    #[test]
    fn can_recover_after_device_drop() {
        let mut service = service();
        service.start().expect("service starts");
        service.degrade("USB device disappeared", "Reconnect Focusrite USB");
        assert!(matches!(service.state(), AudioIngestState::Degraded(_)));

        service.recover("device visible again");
        service.start().expect("service restarts from recovery");

        assert_eq!(service.state(), &AudioIngestState::Listening);
    }
}
