#!/usr/bin/env bash
set -e
source "$HOME/.cargo/env" 2>/dev/null || true

SRC="/mnt/c/Users/akam leinad/bitrouter"
DST="$HOME/bitrouter"

mkdir -p "$DST/plugins/bitrouter-pay/examples"
cp "$SRC/plugins/bitrouter-pay/examples/claude_code_demo.rs" \
   "$DST/plugins/bitrouter-pay/examples/claude_code_demo.rs"

cd "$DST"
export OWS_VAULT_PATH=/home/maka/.ows/wallets
export OWS_WALLET_NAME=agent-treasury
export OWS_BIN=/home/maka/.ows/bin/ows
export CHAINLINK_ATTESTER_API_KEY=RLtYDAmBqQFXkxRpC6zhsQaVPA5qC4DC1gKNJVxn36qv

cargo run -p bitrouter-pay --example claude_code_demo 2>&1
