//! `BalanceCheckHook` — a `language_model::PreRequestHook` that gates a request
//! on funding availability.
//!
//! cloud #225 lesson: a BYOK caller must **never** be rejected at the verify
//! stage for an empty credit balance. v1 makes the funding model explicit on
//! the API key (`api_keys.payment_method`): a `Byok` key carries no balance
//! check at all, and the real charge-time enforcement lives in `CreditCharge` /
//! `MppCharge`. This hook is only a cheap pre-flight gate for the funding
//! source the key actually declares.

use async_trait::async_trait;
use sqlx::SqlitePool;

use bitrouter_sdk::Result;
use bitrouter_sdk::caller::PaymentMethod;
use bitrouter_sdk::language_model::{DenyReason, HookDecision, PipelineContext, PreRequestHook};

use crate::charge::credit_balance;
use crate::mpp::MppState;

/// Pre-flight funding gate. Checks the balance for the funding source declared
/// on the caller's key; BYOK and unauthenticated callers pass unconditionally.
pub struct BalanceCheckHook {
    pool: SqlitePool,
    mpp: Option<MppState>,
}

impl BalanceCheckHook {
    /// Build a `BalanceCheckHook`. `mpp` is optional — without it, MPP callers
    /// are not balance-gated here (they are still settled by `MppCharge`).
    pub fn new(pool: SqlitePool, mpp: Option<MppState>) -> Self {
        Self { pool, mpp }
    }
}

#[async_trait]
impl PreRequestHook for BalanceCheckHook {
    async fn check(&self, ctx: &mut PipelineContext) -> Result<HookDecision> {
        match ctx.caller().payment_method() {
            // #225: BYOK callers are never balance-gated at verify time.
            PaymentMethod::Byok | PaymentMethod::None => Ok(HookDecision::Allow),

            PaymentMethod::Credits => {
                let balance = credit_balance(&self.pool, ctx.caller().user_id()).await?;
                if balance <= 0 {
                    return Ok(HookDecision::Deny(DenyReason::PaymentRequired(
                        "credit balance exhausted".to_string(),
                    )));
                }
                Ok(HookDecision::Allow)
            }

            PaymentMethod::Mpp => {
                let Some(mpp) = &self.mpp else {
                    // No MPP state wired — defer to charge-time settlement.
                    return Ok(HookDecision::Allow);
                };
                let user = ctx.caller().user_id();
                match mpp.session_for_user(user).await? {
                    Some(session_id) => {
                        let balance = mpp.balance(&session_id).await?;
                        if balance <= 0 {
                            return Ok(HookDecision::Deny(DenyReason::PaymentRequired(
                                "MPP channel balance exhausted".to_string(),
                            )));
                        }
                        Ok(HookDecision::Allow)
                    }
                    None => Ok(HookDecision::Deny(DenyReason::PaymentRequired(
                        "no open MPP channel for caller".to_string(),
                    ))),
                }
            }
        }
    }
}
