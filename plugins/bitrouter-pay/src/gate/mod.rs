//! [`ArcPaymentGate`] — composes OWS signing, x402, MPP, and Chainlink attestation.

use std::sync::Arc;

use async_trait::async_trait;
use bitrouter_sdk::{PaymentGate, PaymentGateResult, PaymentRouteRequest};
use serde_json::Value;
use tracing::info;

use crate::PayError;
use crate::attester::run_attested_inference;
#[cfg(feature = "mpp")]
use crate::payment::mpp::{ArcMppBackend, MppBackend, MppClient};
#[cfg(all(feature = "x402", feature = "mpp"))]
use crate::payment::x402::build_inference_request_body;
#[cfg(feature = "x402")]
use crate::payment::x402::{InferenceFormat, X402Client};
use crate::wallet::ArcSigner;

/// BitRouter MPP endpoint used as fallback when x402 upstream is unavailable.
///
/// Proceeds is x402-only; MPP negotiation must be directed at a BitRouter
/// instance that speaks `mpp-br`.
const BITROUTER_MPP_URL: &str =
    "https://gumball-country-monologue.ngrok-free.dev/v1/chat/completions";

/// Configuration for [`ArcPaymentGate`].
pub struct ArcPaymentGateConfig {
    pub wallet_id: String,
    pub chainlink_api_key: Option<String>,
}

/// Payment gate for Arc testnet Proceeds paywalls.
pub struct ArcPaymentGate {
    signer: Arc<ArcSigner>,
    #[cfg(feature = "x402")]
    x402: X402Client,
    #[cfg(feature = "mpp")]
    mpp: MppClient,
    chainlink_api_key: Option<String>,
}

impl ArcPaymentGate {
    pub fn new(config: ArcPaymentGateConfig) -> Result<Self, PayError> {
        let signer = Arc::new(ArcSigner::new(config.wallet_id)?);

        Ok(Self {
            #[cfg(feature = "x402")]
            x402: X402Client::new(signer.clone()),
            #[cfg(feature = "mpp")]
            mpp: MppClient::new(Arc::new(ArcMppBackend::new(signer.clone())) as Arc<dyn MppBackend>),
            chainlink_api_key: config.chainlink_api_key,
            signer,
        })
    }

    pub fn signer(&self) -> &ArcSigner {
        &self.signer
    }

    async fn pay_internal(&self, request: PaymentRouteRequest) -> Result<Value, PayError> {
        let body = self.execute_payment_route(&request).await?;

        if request.attested {
            let key = self.chainlink_api_key.as_ref().ok_or_else(|| {
                PayError::AttestError("attested route requires chainlink_api_key".into())
            })?;
            let model = request
                .model
                .ok_or_else(|| PayError::AttestError("attested route requires model".into()))?;
            let prompt = request.prompt.unwrap_or_default();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let verified =
                run_attested_inference(key, &model, &prompt, body.to_string().as_bytes(), now)
                    .await?;
            record_ledger(&request.url, &verified_inference_id(&verified));
            return serde_json::to_value(verified)
                .map_err(|e| PayError::AttestError(e.to_string()));
        }

        record_ledger(&request.url, "x402-or-mpp");
        Ok(body)
    }

    /// Route a payment request: x402-first with automatic MPP fallback on upstream errors.
    ///
    /// When `request.mpp` is true the MPP path is used exclusively. Otherwise x402 is
    /// attempted first; if it fails with an upstream/payment error (i.e. the payment was
    /// submitted but the AI backend returned 4xx/5xx), MPP is tried as a fallback.
    /// Signing errors and malformed-challenge errors are not retried.
    async fn execute_payment_route(
        &self,
        request: &PaymentRouteRequest,
    ) -> Result<Value, PayError> {
        if request.mpp {
            #[cfg(feature = "mpp")]
            return self.mpp.post(&request.url, request.body.clone()).await;
            #[cfg(not(feature = "mpp"))]
            return Err(PayError::PaymentFailed(
                "MPP support not compiled (enable the `mpp` feature)".into(),
            ));
        }

        // x402-first path.
        #[cfg(feature = "x402")]
        {
            let fmt = if request.anthropic_format.unwrap_or(false) {
                InferenceFormat::Anthropic
            } else {
                InferenceFormat::OpenAI
            };
            let model = request
                .model
                .as_deref()
                .ok_or_else(|| PayError::PaymentFailed("x402 route requires model".into()))?;
            let prompt = request.prompt.as_deref().unwrap_or("");

            let x402_result = self.x402.post(&request.url, fmt, model, prompt).await;

            // With MPP compiled in, retry upstream failures via MPP fallback.
            #[cfg(feature = "mpp")]
            {
                return match x402_result {
                    Ok(body) => {
                        info!("x402 payment succeeded");
                        Ok(body)
                    }
                    Err(x402_err)
                        if matches!(
                            x402_err,
                            PayError::PaymentFailed(_) | PayError::UpstreamError(_)
                        ) =>
                    {
                        info!(
                            "x402 failed ({x402_err}), attempting MPP fallback via {BITROUTER_MPP_URL}"
                        );
                        let fallback_body = build_inference_request_body(fmt, model, prompt);
                        match self.mpp.post(BITROUTER_MPP_URL, Some(fallback_body)).await {
                            Ok(b) => {
                                info!("MPP fallback succeeded");
                                Ok(b)
                            }
                            Err(mpp_err) => Err(PayError::PaymentFailed(format!(
                                "x402 failed ({x402_err}); MPP fallback also failed ({mpp_err})"
                            ))),
                        }
                    }
                    Err(e) => Err(e),
                };
            }

            // Without MPP, propagate x402 errors directly.
            #[cfg(not(feature = "mpp"))]
            {
                if x402_result.is_ok() {
                    info!("x402 payment succeeded");
                }
                return x402_result;
            }
        }

        #[cfg(not(feature = "x402"))]
        return Err(PayError::PaymentFailed(
            "x402 support not compiled (enable the `x402` feature)".into(),
        ));
    }
}

#[async_trait]
impl PaymentGate for ArcPaymentGate {
    async fn pay(&self, request: PaymentRouteRequest) -> Result<PaymentGateResult, String> {
        self.pay_internal(request)
            .await
            .map(|body| PaymentGateResult { body })
            .map_err(|e| e.to_string())
    }
}

fn verified_inference_id(v: &bitrouter_attestation::VerifiedExchange) -> String {
    match &v.integrity {
        bitrouter_attestation::IntegrityProof::ChainlinkResourceDigests {
            inference_id, ..
        } => inference_id.clone(),
        _ => String::new(),
    }
}

fn record_ledger(url: &str, reference: &str) {
    #[cfg(feature = "observe-ledger")]
    {
        let _ = bitrouter_observe::OTEL_ENABLED;
        info!(
            target: "bitrouter_pay.ledger",
            url = url,
            reference = reference,
            "payment ledger row"
        );
    }
    #[cfg(not(feature = "observe-ledger"))]
    {
        info!(
            target: "bitrouter_pay.ledger",
            url = url,
            reference = reference,
            "payment ledger row"
        );
    }
}
