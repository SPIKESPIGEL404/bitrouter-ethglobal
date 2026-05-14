//! # bitrouter-sdk
//!
//! The BitRouter SDK. Merges v0's `bitrouter-core` + `bitrouter-api` +
//! `bitrouter-config` + `bitrouter-providers` into a single crate.
//!
//! Each routing protocol is a module with its own `Pipeline`, `PipelineContext`,
//! `RoutingTable`, `Router` and hook traits. There is **no protocol generic and
//! no cross-protocol shared hook trait** (see design doc 003 §0). Reuse is via
//! shared library code (structs / fns) at the crate root, not shared traits.
//!
//! ## Feature flags
//!
//! - `server` — axum HTTP handlers, SSE, admin endpoints.
//! - `config_file` — yaml config loading (`serde_saphyr` + `tokio::fs`).

#![forbid(unsafe_code)]

// ===== shared library code (crate root) =====
pub mod error;
pub mod event;

// ===== per-protocol modules =====
pub mod acp;
pub mod language_model;
pub mod mcp;

pub use error::{BitrouterError, Result};
pub use event::{EventBus, PipelineEvent};
