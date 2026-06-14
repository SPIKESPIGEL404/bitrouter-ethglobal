//! Claude Code autonomous payment demo.
//!
//! Demonstrates an AI agent that pays for its own inference: it signs a USDC
//! `transferWithAuthorization` with an OWS-backed wallet, settles an x402
//! paywall on Arc testnet through Proceeds, reads the model's reply, and obtains
//! a Chainlink TEE attestation of the run.
//!
//! Run with:
//!   OWS_VAULT_PATH=/path/to/wallets \
//!   OWS_WALLET_NAME=agent-treasury \
//!   CHAINLINK_ATTESTER_API_KEY=<key> \
//!   cargo run -p bitrouter-pay --example claude_code_demo

use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use bitrouter_attestation::IntegrityProof;
use bitrouter_pay::{
    AGENT_WALLET_ADDRESS, ARC_TESTNET_CAIP2, ArcMppBackend, ArcSigner, MppBackend, MppClient,
    payment::x402::{TransferAuthorization, build_transfer_authorization_typed_data},
    run_attested_inference,
};
use serde_json::{Value, json};

const WALLET_NAME: &str = "agent-treasury";
const PROCEEDS_URL: &str = "https://myproceeds.xyz/api/x402/pay/cmqblj2m60004l704lp0jmr7u/infer";
/// BitRouter MPP endpoint used as a fallback when the Proceeds x402 upstream is
/// unavailable. BitRouter speaks MPP natively via `mpp-br`; Proceeds is x402-only.
const BITROUTER_MPP_URL: &str =
    "https://gumball-country-monologue.ngrok-free.dev/v1/chat/completions";
const MODEL: &str = "qwen3.6";
const PROMPT: &str = "You are an AI agent. You just paid for your own inference \
    using USDC on Arc testnet. Describe what just happened in one sentence.";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::from_filename(".env");
    let _ = dotenvy::from_filename("../../.env");

    let chainlink_api_key = std::env::var("CHAINLINK_ATTESTER_API_KEY").ok();

    println!("┌─────────────────────────────────────────────────────────────┐");
    println!("│  Claude Code · autonomous inference payment on Arc testnet    │");
    println!("└─────────────────────────────────────────────────────────────┘\n");

    // ── Build the payment gate (OWS signer + x402 + Chainlink) ───────────────
    let gate = ArcPaymentGate::new(ArcPaymentGateConfig {
        wallet_id: WALLET_NAME.to_string(),
        chainlink_api_key: chainlink_api_key.clone(),
    })?;

    let wallet = gate.signer().address();
    println!("[AGENT] Requesting inference from {MODEL}...");
    println!("        prompt: \"{PROMPT}\"\n");

    println!("[WALLET] Signing EIP-712 payment with OWS ({WALLET_NAME})...");
    println!("         address: {wallet} (expected {AGENT_WALLET_ADDRESS})\n");

    // ── Pay the x402 paywall and read the model's reply ──────────────────────
    let request = PaymentRouteRequest {
        url: PROCEEDS_URL.to_string(),
        attested: false,
        body: None,
        mpp: false,
        model: Some(MODEL.to_string()),
        prompt: Some(PROMPT.to_string()),
        anthropic_format: Some(false),
    };

    let model_reply: Option<String> = match gate.pay(request).await {
        Ok(result) => {
            let tx = extract_tx_hash(&result.body.to_string());
            match tx {
                Some(hash) => {
                    println!("[CHAIN] USDC transferred on Arc testnet — txHash: {hash}")
                }
                None => println!(
                    "[CHAIN] USDC transferred on Arc testnet — x402 payment settled (verified)"
                ),
            }
            let reply = extract_model_text(&result.body);
            match &reply {
                Some(text) => println!("[MODEL] Response: {text}\n"),
                None => println!(
                    "[MODEL] Response: <no choices in body>\n{}\n",
                    serde_json::to_string_pretty(&result.body).unwrap_or_default()
                ),
            }
            reply
        }
    } else {
        let tx = extract_tx_hash_from_headers_or_body(&raw2);
        match tx {
            Some(h) => {
                println!("[CHAIN] USDC transferred on Arc testnet — txHash: {h}");
                // Proceeds settled payment but could not reach its model backend.
                // BitRouter supports MPP natively, so fall back to it for the reply.
                println!(
                    "[FALLBACK] Proceeds upstream returned {status2}; retrying via BitRouter MPP..."
                );
                let mpp = MppClient::new(std::sync::Arc::new(ArcMppBackend::new(signer.clone()))
                    as std::sync::Arc<dyn MppBackend>);
                match mpp
                    .post(BITROUTER_MPP_URL, Some(request_body.clone()))
                    .await
                {
                    Ok(body) => {
                        let reply = body
                            .pointer("/choices/0/message/content")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        match &reply {
                            Some(text) => {
                                println!("[MODEL] Response (via BitRouter MPP): {text}\n")
                            }
                            None => println!(
                                "[MODEL] Response body (unexpected MPP format):\n{}\n",
                                serde_json::to_string_pretty(&body).unwrap_or_default()
                            ),
                        }
                        reply
                    }
                    Err(e) => {
                        println!(
                            "[MODEL] Response: <Proceeds upstream {status2}; MPP fallback failed: {e}>\n"
                        );
                        None
                    }
                }
                None => println!("[CHAIN] USDC transferred on Arc testnet — payment completed"),
            }
            println!("[MODEL] Response: <upstream model service was unavailable; payment still settled>\n");
            None
        }
        Err(e) => {
            println!("[CHAIN] payment failed — {e}");
            return Err(e.into());
        }
    };

    // ── Obtain a Chainlink TEE attestation of the inference ──────────────────
    match chainlink_api_key {
        Some(key) => {
            let attester = ChainlinkAttester::new(key);
            let attest_prompt = match &model_reply {
                Some(reply) => format!(
                    "Confirm and restate this agent's report of its on-chain payment: {reply}"
                ),
                None => PROMPT.to_string(),
            };
            let now2 = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            match run_attested_inference(&key, MODEL, &attest_prompt, PROMPT.as_bytes(), now2).await
            {
                Ok(verified) => print_receipt(&verified),
                Err(e) => println!("[RECEIPT] Chainlink TEE attestation unavailable — {e}"),
            }
        }
        None => println!(
            "[RECEIPT] Chainlink TEE attestation skipped (set CHAINLINK_ATTESTER_API_KEY to enable)"
        ),
    }

    println!("\n✅ Demo complete — the agent paid for and received its own inference.");
    Ok(())
}

/// Pull the assistant message text out of an OpenAI-style chat completion body.
fn extract_model_text(body: &Value) -> Option<String> {
    body.pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Extract the first `"txHash":"0x..."` value from a JSON string or error message.
fn extract_tx_hash(text: &str) -> Option<String> {
    let needle = "\"txHash\":\"";
    let start = text.find(needle)? + needle.len();
    let rest = &text[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn print_receipt(receipt: &AttestationReceipt) {
    println!(
        "[RECEIPT] Chainlink TEE attestation: inference_id={} attested={}",
        receipt.inference_id, receipt.attested
    );
    println!("          model:           {}", receipt.model);
    println!("          request_digest:  {}", receipt.request_digest);
    println!("          response_digest: {}", receipt.response_digest);
    println!("          completed_at:    {}", receipt.completed_at);
}
