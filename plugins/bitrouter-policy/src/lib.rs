//! # bitrouter-policy
//!
//! Policy plugin. Provides `PolicyHook` — per-API-key spend limits, chain
//! restrictions, expiry, tool rules, with explicit combination semantics.
//! Policies are loaded from files; this plugin owns no tables. See doc 004 §4.
//!
//! Filled in by Phase 4.

#![forbid(unsafe_code)]
