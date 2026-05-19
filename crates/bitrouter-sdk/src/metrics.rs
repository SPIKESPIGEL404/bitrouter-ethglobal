//! Observability rendering interface — the `GET /metrics` endpoint contract.
//!
//! The SDK ships exactly one observability seam — the [`MetricsRenderer`]
//! trait — because the HTTP server needs to render Prometheus text without
//! knowing where the counters come from. The accumulator side is
//! deployment-specific (the OSS binary uses `bitrouter-observe`'s
//! `PrometheusHook`; a custom deployment may register its own
//! [`ObserveHook`](crate::language_model::ObserveHook)).
//!
//! Spend / token / rate aggregations are *not* SDK concerns. Any deployment
//! that needs them owns its own storage; see the OSS binary's `metering`
//! module for the reference implementation.

/// A renderer of Prometheus-style text-exposition metrics. The SDK's HTTP
/// server mounts `GET /metrics` against this trait. The trait is
/// deliberately tiny — synchronous, returns owned text — so any in-process
/// accumulator (e.g. `bitrouter_observe::PrometheusHook`) can implement it
/// without dragging Prometheus library types into the SDK.
pub trait MetricsRenderer: Send + Sync {
    /// Render the current accumulator state as a Prometheus text-exposition
    /// payload. Called once per `GET /metrics` request.
    fn render(&self) -> String;

    /// The MIME content-type the renderer wants to advertise on the response.
    /// Defaults to the Prometheus text exposition v0.0.4 type.
    fn content_type(&self) -> &'static str {
        "text/plain; version=0.0.4; charset=utf-8"
    }
}
