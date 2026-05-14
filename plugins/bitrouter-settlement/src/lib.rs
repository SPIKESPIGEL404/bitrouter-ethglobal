//! # bitrouter-settlement
//!
//! Settlement plugin. Exports individually-registrable hooks: `ByokRouteHook`,
//! `BalanceCheckHook`, `MppStreamHook`, the `ChargeStrategy` chain
//! (`ByokCharge` / `CreditCharge` / `MppCharge`), `ReceiptRecorder`
//! (`SettlementRecorder`), and `SqliteMetricsStore`. `SettlementBundle` is a
//! convenience packaging. See design doc 004 §1.
//!
//! MPP delivers the **Tempo** channel only in v1.0; `mpp-solana` is a
//! placeholder feature, not wired (see 008 §1.1).
//!
//! Filled in by Phase 3.

#![forbid(unsafe_code)]

/// Whether the Tempo MPP channel is compiled in.
pub const MPP_TEMPO_ENABLED: bool = cfg!(feature = "mpp-tempo");
/// Whether the Solana MPP channel is compiled in. v1.0: placeholder, never wired.
pub const MPP_SOLANA_ENABLED: bool = cfg!(feature = "mpp-solana");
