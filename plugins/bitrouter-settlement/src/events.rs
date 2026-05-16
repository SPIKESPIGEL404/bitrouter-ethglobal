//! Pipeline events emitted by `bitrouter-settlement`.

use serde::Serialize;

use bitrouter_sdk::PipelineEvent;

/// BYOK matched: `ByokRouteHook` found an active `byok_provider_keys` row for
/// the caller against this provider. `byok_used` is derived **only** from the
/// presence of this event — never reverse-inferred from
/// `target.api_key_override.is_some()`.
#[derive(Debug, Clone, Serialize)]
pub struct ByokKeyApplied {
    /// The provider the caller's own key was applied to.
    pub provider: String,
}

impl PipelineEvent for ByokKeyApplied {
    fn event_name(&self) -> &'static str {
        "settlement.byok_key_applied"
    }
}

/// Pricing was missing for the resolved target — charging is skipped (WARN, not
/// a pipeline error). v0 #180 / #440 / #443: a missing price must never be
/// silently treated as a zero price.
#[derive(Debug, Clone, Serialize)]
pub struct PricingUnavailable {
    /// The provider whose pricing was missing.
    pub provider_id: String,
    /// The service / model id whose pricing was missing.
    pub service_id: String,
}

impl PipelineEvent for PricingUnavailable {
    fn event_name(&self) -> &'static str {
        "settlement.pricing_unavailable"
    }
}

/// An MPP streaming checkpoint was signed — incremental settlement progress.
#[derive(Debug, Clone, Serialize)]
pub struct MppCheckpointSigned {
    /// The MPP channel session id.
    pub session_id: String,
    /// Cumulative micro-USD settled up to this checkpoint.
    pub cumulative_micro_usd: i64,
}

impl PipelineEvent for MppCheckpointSigned {
    fn event_name(&self) -> &'static str {
        "settlement.mpp_checkpoint_signed"
    }
}
