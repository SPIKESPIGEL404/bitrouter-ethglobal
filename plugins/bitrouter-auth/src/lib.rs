//! # bitrouter-auth
//!
//! Auth plugin. Provides `AuthHook` (validates `brvk_` virtual keys + MPP
//! credentials — **no JWT**), owns the `users` and `api_keys` tables, and emits
//! the `Authenticated` event. See design doc 004 §3.
//!
//! Filled in by Phase 3.

#![forbid(unsafe_code)]
