//! Per-agent Walrus memory namespace scoping (Strategy A).
//!
//! Walrus Memory is wired as a normal `mcp_servers` upstream with one shared
//! delegate credential. This module enforces per-agent namespace isolation at
//! the gateway: an orchestrator gets full access; each subagent is confined to
//! its configured namespace(s). Agent identity comes from the inbound
//! `x-bitrouter-agent` header.

pub mod config;
pub mod scope_executor;
