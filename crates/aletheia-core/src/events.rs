//! Append-only domain events for crash recovery, audit, and sync.

use crate::health::HealthReport;
use crate::ids::{DeviceId, IntegrationId, ServiceSessionId};
use crate::time::{Millis, TimestampMs, now_ms};

/// Security-relevant operator or system action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuditAction {
    ServiceStarted,
    ServiceStopped,
    CandidateApproved,
    PreviewRendered,
    LiveOutputSent,
    LiveOutputCleared,
    PluginInstalled,
    SecretUpdated,
    DataMiserEnabled,
    IntegrationConfigUpdated,
    SupportBundleExported,
    BoothPackExported,
    ProductionReadinessChecked,
    LocalRehearsalRun,
    AiDetectionRun,
    OfflineAssetInstalled,
    OfflinePackExported,
    /// An operator read a secret from the OS vault (e.g. an API key or password).
    VaultAccessed,
}

/// Domain events shared across Rust services and later persisted to SQLite.
#[derive(Clone, Debug, PartialEq)]
pub enum DomainEvent {
    AudioCaptureStarting {
        session_id: ServiceSessionId,
        device_id: DeviceId,
    },
    AudioCaptureStarted {
        session_id: ServiceSessionId,
        device_id: DeviceId,
        sample_rate_hz: u32,
        channels: u16,
    },
    AudioCaptureStopped {
        session_id: ServiceSessionId,
        reason: String,
    },
    AudioFrameMeasured {
        session_id: ServiceSessionId,
        rms: f32,
        peak: f32,
        zero_crossing_rate: f32,
        frame_duration_ms: Millis,
    },
    VadDecisionMade {
        session_id: ServiceSessionId,
        speech_detected: bool,
        confidence: f32,
        mode: &'static str,
    },
    ScriptureCandidateCreated {
        session_id: ServiceSessionId,
        reference: String,
        confidence: f32,
        reason: String,
    },
    PreviewSceneRendered {
        session_id: ServiceSessionId,
        reference: String,
        theme_id: String,
    },
    LiveOutputSent {
        session_id: ServiceSessionId,
        integration_id: IntegrationId,
        reference: String,
    },
    HealthChanged(HealthReport),
    AuditRecorded {
        session_id: Option<ServiceSessionId>,
        action: AuditAction,
        actor: String,
        detail: String,
    },
}

/// Event envelope with sequence and timestamp.
#[derive(Clone, Debug, PartialEq)]
pub struct EventEnvelope {
    pub sequence: u64,
    pub timestamp_ms: TimestampMs,
    pub event: DomainEvent,
}

/// Append-only event log contract.
pub trait EventLog {
    /// Appends an event and returns the assigned envelope.
    fn append(&mut self, event: DomainEvent) -> EventEnvelope;

    /// Returns all events in append order.
    fn all(&self) -> &[EventEnvelope];
}

/// In-memory implementation used by unit tests and early services.
#[derive(Default)]
pub struct InMemoryEventLog {
    next_sequence: u64,
    events: Vec<EventEnvelope>,
}

impl EventLog for InMemoryEventLog {
    fn append(&mut self, event: DomainEvent) -> EventEnvelope {
        self.next_sequence += 1;
        let envelope = EventEnvelope {
            sequence: self.next_sequence,
            timestamp_ms: now_ms(),
            event,
        };
        self.events.push(envelope.clone());
        envelope
    }

    fn all(&self) -> &[EventEnvelope] {
        &self.events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_log_assigns_sequences_in_order() {
        let session_id = ServiceSessionId::new("sunday-am").expect("valid session id");
        let device_id = DeviceId::new("focusrite-usb").expect("valid device id");
        let mut log = InMemoryEventLog::default();

        log.append(DomainEvent::AudioCaptureStarting {
            session_id: session_id.clone(),
            device_id: device_id.clone(),
        });
        log.append(DomainEvent::AudioCaptureStarted {
            session_id,
            device_id,
            sample_rate_hz: 48_000,
            channels: 1,
        });

        assert_eq!(log.all()[0].sequence, 1);
        assert_eq!(log.all()[1].sequence, 2);
    }
}
