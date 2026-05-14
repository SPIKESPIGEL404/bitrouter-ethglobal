//! # bitrouter-observe
//!
//! Observability plugin. Provides `ObserveHook` implementations: `PrometheusHook`
//! always, `OtlpExportHook` behind the `otlp` feature. See design doc 003 §4.6.
//!
//! Filled in by Phase 5.

#![forbid(unsafe_code)]

/// Whether the OTLP exporter is compiled in.
pub const OTLP_ENABLED: bool = cfg!(feature = "otlp");
