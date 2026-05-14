//! `SqliteMetricsStore` — the `MetricsStore` implementation, backed by the
//! `requests` table. It is the **single writer** of that table: the
//! `ReceiptRecorder` (a `SettlementRecorder`) calls `record_request`, and the
//! PreRequest hooks (`BalanceCheckHook` / policy hooks) call the read methods.
//!
//! This is the only module that touches the `requests` table (004 §7.2).

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
use sqlx::{Row, SqlitePool};

use bitrouter_sdk::caller::FundingSource;
use bitrouter_sdk::metrics::{MetricsStore, RateMetrics, RequestMetric, TimeWindow, TokenUsage};
use bitrouter_sdk::{BitrouterError, Result};

/// A `MetricsStore` over a sqlite `requests` table.
pub struct SqliteMetricsStore {
    pool: SqlitePool,
}

impl SqliteMetricsStore {
    /// Build a store over a sqlite pool. The pool must already carry this
    /// plugin's tables (`crate::db::migrate`).
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Shared access to the underlying pool (used by sibling settlement hooks).
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

fn funding_source_str(f: FundingSource) -> &'static str {
    match f {
        FundingSource::Unsettled => "unsettled",
        FundingSource::Credits => "credits",
        FundingSource::Mpp => "mpp",
        FundingSource::Byok => "byok",
    }
}

/// Resolve a [`TimeWindow`] into an inclusive lower-bound timestamp.
fn window_start(window: TimeWindow) -> DateTime<Utc> {
    let now = Utc::now();
    match window {
        TimeWindow::LastMinute => now - Duration::minutes(1),
        TimeWindow::LastHour => now - Duration::hours(1),
        TimeWindow::Today => Utc
            .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
            .single()
            .unwrap_or(now),
        TimeWindow::ThisWeek => {
            let days_from_monday = now.weekday().num_days_from_monday() as i64;
            let midnight = Utc
                .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
                .single()
                .unwrap_or(now);
            midnight - Duration::days(days_from_monday)
        }
        TimeWindow::ThisMonth => Utc
            .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
            .single()
            .unwrap_or(now),
        TimeWindow::Custom { start, .. } => start,
    }
}

#[async_trait]
impl MetricsStore for SqliteMetricsStore {
    async fn get_spend(&self, key: &str, window: TimeWindow) -> Result<u64> {
        let start = window_start(window).to_rfc3339();
        let row = sqlx::query(
            "SELECT COALESCE(SUM(final_charge_micro_usd), 0) AS total \
             FROM requests WHERE api_key_id = ? AND created_at >= ?",
        )
        .bind(key)
        .bind(&start)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| BitrouterError::internal(format!("get_spend: {e}")))?;
        Ok(row.get::<i64, _>("total").max(0) as u64)
    }

    async fn get_request_count(&self, key: &str, window: TimeWindow) -> Result<u64> {
        let start = window_start(window).to_rfc3339();
        let row = sqlx::query(
            "SELECT COUNT(*) AS n FROM requests WHERE api_key_id = ? AND created_at >= ?",
        )
        .bind(key)
        .bind(&start)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| BitrouterError::internal(format!("get_request_count: {e}")))?;
        Ok(row.get::<i64, _>("n").max(0) as u64)
    }

    async fn get_token_usage(
        &self,
        key: &str,
        model: &str,
        window: TimeWindow,
    ) -> Result<TokenUsage> {
        let start = window_start(window).to_rfc3339();
        let row = sqlx::query(
            "SELECT COALESCE(SUM(prompt_tokens), 0) AS pt, \
             COALESCE(SUM(completion_tokens), 0) AS ct \
             FROM requests WHERE api_key_id = ? AND model_id = ? AND created_at >= ?",
        )
        .bind(key)
        .bind(model)
        .bind(&start)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| BitrouterError::internal(format!("get_token_usage: {e}")))?;
        let prompt = row.get::<i64, _>("pt").max(0) as u64;
        let completion = row.get::<i64, _>("ct").max(0) as u64;
        Ok(TokenUsage {
            prompt_tokens: prompt,
            completion_tokens: completion,
            total_tokens: prompt + completion,
        })
    }

    async fn get_rate(&self, key: &str) -> Result<RateMetrics> {
        let start = window_start(TimeWindow::LastMinute).to_rfc3339();
        let row = sqlx::query(
            "SELECT COUNT(*) AS n, \
             COALESCE(SUM(prompt_tokens + completion_tokens), 0) AS tok \
             FROM requests WHERE api_key_id = ? AND created_at >= ?",
        )
        .bind(key)
        .bind(&start)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| BitrouterError::internal(format!("get_rate: {e}")))?;
        Ok(RateMetrics {
            requests_per_minute: row.get::<i64, _>("n").max(0) as f64,
            tokens_per_minute: row.get::<i64, _>("tok").max(0) as f64,
        })
    }

    async fn record_request(&self, record: RequestMetric) -> Result<()> {
        // The authoritative receipt write. Every billing + identity column is
        // populated (cloud #207 / #198); failed requests are recorded too,
        // with a non-null `error` (#198).
        sqlx::query(
            "INSERT OR REPLACE INTO requests \
             (request_id, user_id, api_key_id, model_id, provider_id, \
              prompt_tokens, completion_tokens, reasoning_tokens, \
              final_charge_micro_usd, funding_source, byok_used, streamed, \
              latency_ms, generation_time_ms, error, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&record.request_id)
        .bind(&record.user_id)
        .bind(&record.api_key_id)
        .bind(&record.model_id)
        .bind(&record.provider_id)
        .bind(record.prompt_tokens as i64)
        .bind(record.completion_tokens as i64)
        .bind(record.reasoning_tokens as i64)
        .bind(record.final_charge_micro_usd)
        .bind(funding_source_str(record.funding_source))
        .bind(record.byok_used as i64)
        .bind(record.stream as i64)
        .bind(record.latency_ms as i64)
        .bind(record.generation_time_ms as i64)
        .bind(&record.error)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| BitrouterError::internal(format!("record_request: {e}")))?;
        Ok(())
    }
}
