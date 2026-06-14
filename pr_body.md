## Summary

- Migrates `bitrouter-pay` attestation to the shared `bitrouter-attestation` crate with `ChainlinkResourceDigests` integrity proofs and a `ChainlinkVerifier` (honest, fail-closed)
- Wires the Chainlink attestation plugin into the `bitrouter` server via a new `verify-exchange` CLI subcommand
- Rewrites `claude_code_demo` to perform raw x402 HTTP with a 120s timeout and verbose response logging, with model set to `qwen3.6`
- Adds BitRouter MPP fallback in the demo: when Proceeds settles payment on-chain but its model backend returns 502, the demo retries via `BITROUTER_MPP_URL` using `MppClient` + `ArcMppBackend` and extracts a real model reply

## Test plan

- [ ] `cargo run -p bitrouter-pay --example claude_code_demo` — expect `[MODEL] Response` line with inference text
- [ ] `cargo test -p bitrouter-pay` — unit and integration tests pass
- [ ] `cargo clippy --all-features` — no new warnings
