//! # bitrouter-p2p
//!
//! Experimental P2P mesh layer. New in v1 — v0 mainline had no p2p crate (only
//! a stale git branch). Off by default; gated behind the `experimental` feature.
//!
//! Filled in by Phase 5.

#![forbid(unsafe_code)]

/// Marker for the experimental p2p surface. Replaced with the real mesh layer
/// in Phase 5.
pub const EXPERIMENTAL: bool = cfg!(feature = "experimental");
