//! # bitrouter-guardrails
//!
//! Content firewall plugin. Provides `GuardrailPreHook` (upstream / request
//! content) and `GuardrailStreamHook` (downstream / response stream, can
//! Redact / Abort). See design doc 004 §5.
//!
//! Filled in by Phase 4.

#![forbid(unsafe_code)]
