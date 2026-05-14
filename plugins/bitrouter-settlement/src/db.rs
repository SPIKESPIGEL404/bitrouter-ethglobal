//! Database schema for `bitrouter-settlement`.
//!
//! This plugin owns four tables — `requests` (receipts + usage metrics source),
//! `credit_accounts`, `byok_provider_keys`, `mpp_sessions`. Each is touched
//! only by its dedicated hook module (plugin DB isolation, 004 §7.2):
//!
//! | table               | owner module          |
//! |---------------------|-----------------------|
//! | `requests`          | [`crate::metrics_store`] |
//! | `credit_accounts`   | [`crate::charge`] (`CreditCharge`) |
//! | `byok_provider_keys`| [`crate::byok`]       |
//! | `mpp_sessions`      | [`crate::charge`] (`MppCharge`) / [`crate::balance`] |

use sqlx::SqlitePool;

use bitrouter_sdk::{BitrouterError, MigrationItem, Result};

/// SQL that creates every table this plugin owns.
pub const MIGRATION_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS requests (
    request_id             TEXT PRIMARY KEY,
    user_id                TEXT NOT NULL,
    api_key_id             TEXT NOT NULL,
    model_id               TEXT NOT NULL,
    provider_id            TEXT NOT NULL,
    prompt_tokens          INTEGER NOT NULL DEFAULT 0,
    completion_tokens      INTEGER NOT NULL DEFAULT 0,
    reasoning_tokens       INTEGER NOT NULL DEFAULT 0,
    final_charge_micro_usd INTEGER NOT NULL DEFAULT 0,
    funding_source         TEXT NOT NULL,
    byok_used              INTEGER NOT NULL DEFAULT 0,
    streamed               INTEGER NOT NULL DEFAULT 0,
    latency_ms             INTEGER NOT NULL DEFAULT 0,
    generation_time_ms     INTEGER NOT NULL DEFAULT 0,
    error                  TEXT,
    created_at             TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_requests_api_key ON requests(api_key_id, created_at);
CREATE INDEX IF NOT EXISTS idx_requests_user ON requests(user_id, created_at);

CREATE TABLE IF NOT EXISTS credit_accounts (
    user_id           TEXT PRIMARY KEY,
    balance_micro_usd INTEGER NOT NULL DEFAULT 0,
    updated_at        TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS byok_provider_keys (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL,
    provider    TEXT NOT NULL,
    api_key     TEXT NOT NULL,
    api_base    TEXT,
    active      INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_byok_user_provider
    ON byok_provider_keys(user_id, provider);

CREATE TABLE IF NOT EXISTS mpp_sessions (
    session_id        TEXT PRIMARY KEY,
    user_id           TEXT NOT NULL,
    channel           TEXT NOT NULL,
    balance_micro_usd INTEGER NOT NULL DEFAULT 0,
    last_checkpoint_micro_usd INTEGER NOT NULL DEFAULT 0,
    updated_at        TEXT NOT NULL
);
"#;

/// This plugin's migration set, for `Plugin::migrations()`.
pub fn migrations() -> Vec<MigrationItem> {
    vec![MigrationItem::sql(
        2_000,
        vec![
            "requests".to_string(),
            "credit_accounts".to_string(),
            "byok_provider_keys".to_string(),
            "mpp_sessions".to_string(),
        ],
        MIGRATION_SQL,
    )]
}

/// Create this plugin's tables on `pool`. Idempotent.
pub async fn migrate(pool: &SqlitePool) -> Result<()> {
    for stmt in MIGRATION_SQL.split(';') {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }
        sqlx::query(stmt)
            .execute(pool)
            .await
            .map_err(|e| BitrouterError::internal(format!("settlement migration: {e}")))?;
    }
    Ok(())
}
