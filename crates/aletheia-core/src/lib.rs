//! Shared domain contracts for Aletheia services.

pub mod events;
pub mod health;
pub mod ids;
pub mod security;
pub mod time;

pub use events::{AuditAction, DomainEvent, EventEnvelope, EventLog, InMemoryEventLog};
pub use health::{HealthImpact, HealthReport, HealthState};
pub use ids::{DeviceId, IntegrationId, ServiceSessionId};
pub use security::{CapabilityScope, RedactedSecret};
pub use time::{Millis, TimestampMs, now_ms};
