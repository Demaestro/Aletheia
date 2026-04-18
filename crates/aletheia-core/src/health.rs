//! Health contracts for degraded-mode UX.

use crate::time::TimestampMs;

/// Operator-facing health state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HealthState {
    Healthy,
    Degraded,
    Offline,
    Failed,
}

/// How a health issue affects live production.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HealthImpact {
    Informational,
    DegradesDetection,
    DegradesOutput,
    BlocksLiveOutput,
}

/// Plain-language health report emitted by a service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HealthReport {
    pub service: &'static str,
    pub state: HealthState,
    pub impact: HealthImpact,
    pub message: String,
    pub operator_action: String,
    pub checked_at_ms: TimestampMs,
}

impl HealthReport {
    /// Creates a new operator-facing health report.
    pub fn new(
        service: &'static str,
        state: HealthState,
        impact: HealthImpact,
        message: impl Into<String>,
        operator_action: impl Into<String>,
        checked_at_ms: TimestampMs,
    ) -> Self {
        Self {
            service,
            state,
            impact,
            message: message.into(),
            operator_action: operator_action.into(),
            checked_at_ms,
        }
    }
}
