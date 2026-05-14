//! # bitrouter (library)
//!
//! Assembly layer: turns a config into an `App`, owns daemon control and the
//! management-command logic. This is the home of v0's `load_builtin_plugins`
//! equivalent. The `bin` target (`main.rs`) is the CLI/TUI entry point.
//!
//! Assembly must sit *above* sdk and plugins (`plugins → sdk`, sdk never
//! depends back on plugins) to avoid a Cargo dependency cycle.
//!
//! Filled in by Phase 5.

#![forbid(unsafe_code)]

/// Crate version string, surfaced by `bitrouter --version`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
