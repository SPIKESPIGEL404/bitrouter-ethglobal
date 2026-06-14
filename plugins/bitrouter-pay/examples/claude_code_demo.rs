//! Claude Code autonomous payment demo.
//!
//! Demonstrates an AI agent that pays for its own inference: it signs a USDC
//! `transferWithAuthorization` with an OWS-backed wallet, settles an x402
//! paywall on Arc testnet through Proceeds (with automatic MPP fallback inside
//! [`ArcPaymentGate`] when the Proceeds upstream is unavailable), reads the
//! model's reply, and obtains a Chainlink confidential-inference receipt.
//!
//! Run with:
//!   OWS_VAULT_PATH=/path/to/wallets \
//!   OWS_WALLET_NAME=agent-treasury \
//!   CHAINLINK_ATTESTER_API_KEY=<key> \
//!   cargo run -p bitrouter-pay --example claude_code_demo

use bitrouter_attestation::{IntegrityProof, VerifiedExchange};
use bitrouter_pay::{
    AGENT_WALLET_ADDRESS, ArcPaymentGate, ArcPaymentGateConfig, run_attested_inference,
};
use bitrouter_sdk::{PaymentGate, PaymentRouteRequest};
use serde_json::Value;

const WALLET_NAME: &str = "agent-treasury";
const PROCEEDS_URL: &str = "https://myproceeds.xyz/api/x402/pay/cmqblj2m60004l704lp0jmr7u/infer";
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

    // ── Build the payment gate (OWS signer + x402 + MPP fallback + Chainlink) ─
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
    // `gate.pay` runs x402-first and falls back to MPP internally, so the demo
    // just submits one request and renders whatever settled.
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
            match extract_tx_hash(&result.body.to_string()) {
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
        Err(e) => {
            println!("[CHAIN] payment failed — {e}");
            return Err(e.into());
        }
    };

    // ── Obtain a Chainlink confidential-inference receipt of the run ──────────
    match chainlink_api_key {
        Some(key) => {
            let attest_prompt = match &model_reply {
                Some(reply) => format!(
                    "Confirm and restate this agent's report of its on-chain payment: {reply}"
                ),
                None => PROMPT.to_string(),
            };
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            match run_attested_inference(&key, MODEL, &attest_prompt, PROMPT.as_bytes(), now).await
            {
                Ok(verified) => print_receipt(&verified),
                Err(e) => println!("[RECEIPT] Chainlink confidential receipt unavailable — {e}"),
            }
        }
        None => println!(
            "[RECEIPT] Chainlink confidential receipt skipped (set CHAINLINK_ATTESTER_API_KEY to enable)"
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

/// Render the Chainlink confidential-inference receipt. The dev-preview exposes
/// no enclave signature, so this is evidence (per-resource digests), not a
/// signed TEE attestation — `verified` reflects that honestly.
fn print_receipt(exchange: &VerifiedExchange) {
    println!(
        "[RECEIPT] Chainlink confidential inference — model={} verified={}",
        exchange.model, exchange.verified
    );
    println!("          request_hash:  {}", exchange.request_hash);
    println!("          response_hash: {}", exchange.response_hash);
    match &exchange.integrity {
        IntegrityProof::ChainlinkResourceDigests {
            inference_id,
            request_digest,
            response_digest,
            digests_consistent,
            ..
        } => {
            println!("          inference_id:       {inference_id}");
            println!("          request_digest:     {request_digest}");
            println!("          response_digest:    {response_digest}");
            println!("          digests_consistent: {digests_consistent}");
        }
        other => println!("          integrity: {other:?}"),
    }
}
