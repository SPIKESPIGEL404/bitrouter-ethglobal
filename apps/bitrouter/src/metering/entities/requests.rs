//! The `requests` table — one row per settled request.

use sea_orm::entity::prelude::*;

/// One row of the `requests` table.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "requests")]
pub struct Model {
    /// Unique request id.
    #[sea_orm(primary_key, auto_increment = false)]
    pub request_id: String,
    /// Owning user id.
    pub user_id: String,
    /// API key id that made the request.
    pub api_key_id: String,
    /// Resolved model id.
    pub model_id: String,
    /// Resolved provider id.
    pub provider_id: String,
    /// Prompt tokens consumed.
    pub prompt_tokens: i64,
    /// Completion tokens consumed.
    pub completion_tokens: i64,
    /// Reasoning tokens consumed.
    pub reasoning_tokens: i64,
    /// Cache-read prompt tokens.
    pub cache_read_tokens: i64,
    /// Cache-write prompt tokens.
    pub cache_write_tokens: i64,
    /// Estimated charge in micro-USD computed from pricing × tokens.
    pub estimated_charge_micro_usd: i64,
    /// Whether the request was streamed (`1`) or not (`0`).
    pub streamed: i32,
    /// End-to-end latency in milliseconds.
    pub latency_ms: i64,
    /// Upstream generation time in milliseconds.
    pub generation_time_ms: i64,
    /// Error string if the request failed, else `None`.
    pub error: Option<String>,
    /// Payment rail used to settle this call (`x402` / `mpp`), if the
    /// buyer-pay gate paid an upstream 402. `None` for an unpaid call.
    pub rail: Option<String>,
    /// On-chain settlement transaction hash for the payment, if any.
    pub pay_tx: Option<String>,
    /// Confidential-inference id (e.g. the Chainlink attester's id) when the
    /// call ran through an attested endpoint. `None` otherwise.
    pub attestation_id: Option<String>,
    /// Canonical request digest reported by the attester, if any.
    pub request_digest: Option<String>,
    /// Canonical response digest reported by the attester, if any.
    pub response_digest: Option<String>,
    /// Memory delegate (e.g. a subagent's scoped namespace / Sui address) this
    /// call was granted, if it was a delegated subagent call. `None` otherwise.
    pub memory_delegate: Option<String>,
    /// RFC3339 creation timestamp.
    pub created_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
