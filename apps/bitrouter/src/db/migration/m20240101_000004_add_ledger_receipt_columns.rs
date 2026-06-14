//! Add the unified-ledger receipt columns to the `requests` table.
//!
//! The OSS metering row already records pipeline-observed usage + the
//! estimated micro-USD. The PRD's "spend + receipts ledger" (§8.6 / §9) also
//! wants, per call, the payment **rail** (x402 / mpp), the settlement **tx
//! hash**, the confidential-inference **attestation / inference id** plus its
//! request/response **digests**, and the **memory delegate** a spawned subagent
//! was granted. These are nullable — a plain inference with no payment or
//! attestation leaves them empty, and the columns are populated only when the
//! buyer-pay gate / Chainlink attester / memory delegation actually ran.
//!
//! Each column is added individually and guarded by `has_column`, so the
//! migration is idempotent and a re-run (or a partially-migrated DB) is a no-op.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// The receipt columns, all `TEXT NULL`.
const COLUMNS: &[&str] = &[
    "rail",
    "pay_tx",
    "attestation_id",
    "request_digest",
    "response_digest",
    "memory_delegate",
];

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for col in COLUMNS {
            if !manager.has_column("requests", col).await? {
                manager
                    .alter_table(
                        Table::alter()
                            .table(Requests::Table)
                            .add_column(ColumnDef::new(Alias::new(*col)).string().null())
                            .to_owned(),
                    )
                    .await?;
            }
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for col in COLUMNS {
            if manager.has_column("requests", col).await? {
                manager
                    .alter_table(
                        Table::alter()
                            .table(Requests::Table)
                            .drop_column(Alias::new(*col))
                            .to_owned(),
                    )
                    .await?;
            }
        }
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Requests {
    Table,
}
