//! STT routing policy, adapter contracts, audio capture, and cloud fallback.

pub mod capture;
pub mod cloud;
pub mod offline;

use serde::{Deserialize, Serialize};

/// STT adapter execution mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SttMode {
    Offline,
    Cloud,
    Hybrid,
}

impl SttMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::Cloud => "cloud",
            Self::Hybrid => "hybrid",
        }
    }
}

/// STT adapter readiness snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SttReadiness {
    pub offline_models_ready: bool,
    pub cloud_ready: bool,
    pub last_cloud_latency_ms: Option<u32>,
}

impl SttReadiness {
    pub fn offline_ready_only() -> Self {
        Self {
            offline_models_ready: true,
            cloud_ready: false,
            last_cloud_latency_ms: None,
        }
    }
}

/// STT routing policy per session.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SttRoutingPolicy {
    pub data_miser_enabled: bool,
    /// Hard offline mode. When true, no audio leaves the machine no matter
    /// what other flags say — this is the operator-facing "Offline" toggle
    /// in the runtime UI and the architectural-vision "zero cloud calls"
    /// guarantee. Overrides `cloud_allowed` and `hybrid_allowed`.
    pub offline_mode_enabled: bool,
    pub offline_preferred: bool,
    pub cloud_allowed: bool,
    pub hybrid_allowed: bool,
    pub max_cloud_latency_ms: u32,
    pub confidence_floor: f32,
}

impl Default for SttRoutingPolicy {
    fn default() -> Self {
        Self {
            data_miser_enabled: true,
            offline_mode_enabled: false,
            offline_preferred: true,
            cloud_allowed: true,
            hybrid_allowed: true,
            max_cloud_latency_ms: 200,
            confidence_floor: 0.85,
        }
    }
}

/// Routing decision result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SttRoutingPlan {
    pub mode: SttMode,
    pub adapter_id: String,
    pub reason: String,
}

/// Router for STT adapter selection.
#[derive(Default)]
pub struct SttRouter;

impl SttRouter {
    pub fn route(&self, policy: &SttRoutingPolicy, readiness: &SttReadiness) -> SttRoutingPlan {
        // Hard offline mode: if the operator (or the architectural-vision
        // safety default) has set offline_mode_enabled, every cloud or
        // hybrid path is closed regardless of cloud_allowed/hybrid_allowed
        // or readiness.
        let cloud_permitted =
            !policy.offline_mode_enabled && policy.cloud_allowed && !policy.data_miser_enabled;
        let hybrid_permitted =
            !policy.offline_mode_enabled && policy.hybrid_allowed && !policy.data_miser_enabled;

        if readiness.offline_models_ready {
            if hybrid_permitted && readiness.cloud_ready && cloud_latency_ok(policy, readiness) {
                return SttRoutingPlan {
                    mode: SttMode::Hybrid,
                    adapter_id: "stt-hybrid".to_string(),
                    reason: "Offline is available and cloud latency is within the hybrid budget."
                        .to_string(),
                };
            }

            return SttRoutingPlan {
                mode: SttMode::Offline,
                adapter_id: "stt-local".to_string(),
                reason: "Offline models are installed; routing stays local for resiliency."
                    .to_string(),
            };
        }

        if cloud_permitted && readiness.cloud_ready && cloud_latency_ok(policy, readiness) {
            return SttRoutingPlan {
                mode: SttMode::Cloud,
                adapter_id: "stt-cloud".to_string(),
                reason: "Offline models are missing; cloud fallback is permitted.".to_string(),
            };
        }

        let reason = if policy.offline_mode_enabled {
            "Offline mode is enabled; no cloud or hybrid route is permitted.".to_string()
        } else {
            "STT routing blocked: install offline models or allow cloud fallback.".to_string()
        };
        SttRoutingPlan {
            mode: SttMode::Offline,
            adapter_id: "stt-blocked".to_string(),
            reason,
        }
    }
}

fn cloud_latency_ok(policy: &SttRoutingPolicy, readiness: &SttReadiness) -> bool {
    readiness
        .last_cloud_latency_ms
        .map(|latency| latency <= policy.max_cloud_latency_ms)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_offline_when_ready() {
        let policy = SttRoutingPolicy::default();
        let readiness = SttReadiness {
            offline_models_ready: true,
            cloud_ready: true,
            last_cloud_latency_ms: Some(120),
        };
        let plan = SttRouter.route(&policy, &readiness);
        assert_eq!(plan.mode, SttMode::Offline);
    }

    #[test]
    fn selects_hybrid_when_allowed_and_low_latency() {
        let policy = SttRoutingPolicy {
            data_miser_enabled: false,
            ..SttRoutingPolicy::default()
        };
        let readiness = SttReadiness {
            offline_models_ready: true,
            cloud_ready: true,
            last_cloud_latency_ms: Some(110),
        };
        let plan = SttRouter.route(&policy, &readiness);
        assert_eq!(plan.mode, SttMode::Hybrid);
    }

    #[test]
    fn selects_cloud_when_offline_missing() {
        let policy = SttRoutingPolicy {
            data_miser_enabled: false,
            ..SttRoutingPolicy::default()
        };
        let readiness = SttReadiness {
            offline_models_ready: false,
            cloud_ready: true,
            last_cloud_latency_ms: Some(140),
        };
        let plan = SttRouter.route(&policy, &readiness);
        assert_eq!(plan.mode, SttMode::Cloud);
    }

    #[test]
    fn offline_mode_blocks_cloud_and_hybrid() {
        let policy = SttRoutingPolicy {
            data_miser_enabled: false,
            offline_mode_enabled: true,
            ..SttRoutingPolicy::default()
        };
        // Even with offline ready and cloud also ready, hybrid is forbidden.
        let with_offline = SttRouter.route(
            &policy,
            &SttReadiness {
                offline_models_ready: true,
                cloud_ready: true,
                last_cloud_latency_ms: Some(80),
            },
        );
        assert_eq!(with_offline.mode, SttMode::Offline);
        assert_eq!(with_offline.adapter_id, "stt-local");

        // With no offline models, cloud is also forbidden — must block.
        let no_offline = SttRouter.route(
            &policy,
            &SttReadiness {
                offline_models_ready: false,
                cloud_ready: true,
                last_cloud_latency_ms: Some(80),
            },
        );
        assert_eq!(no_offline.adapter_id, "stt-blocked");
        assert!(no_offline.reason.to_lowercase().contains("offline mode"));
    }

    #[test]
    fn blocks_when_data_miser_on_and_no_offline() {
        let policy = SttRoutingPolicy::default();
        let readiness = SttReadiness {
            offline_models_ready: false,
            cloud_ready: true,
            last_cloud_latency_ms: Some(100),
        };
        let plan = SttRouter.route(&policy, &readiness);
        assert_eq!(plan.adapter_id, "stt-blocked");
    }
}
