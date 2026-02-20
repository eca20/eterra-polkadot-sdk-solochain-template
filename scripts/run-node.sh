#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:-default}"          # default | production
CHAIN="${2:-testnet}"         # dev | testnet | production
PROFILE="${3:-release}"       # debug | release

if [[ "$MODE" != "default" && "$MODE" != "production" ]]; then
  echo "usage: scripts/run-node.sh [default|production] [dev|testnet|production] [debug|release]" >&2
  exit 1
fi
if [[ "$CHAIN" != "dev" && "$CHAIN" != "testnet" && "$CHAIN" != "production" ]]; then
  echo "usage: scripts/run-node.sh [default|production] [dev|testnet|production] [debug|release]" >&2
  exit 1
fi
if [[ "$PROFILE" != "debug" && "$PROFILE" != "release" ]]; then
  echo "usage: scripts/run-node.sh [default|production] [dev|testnet|production] [debug|release]" >&2
  exit 1
fi

OUT_DIR="${ROOT_DIR}/chain-specs/generated/${MODE}"
BASE_PATH="${ROOT_DIR}/data/${MODE}-${CHAIN}-alice"

"${ROOT_DIR}/scripts/deploy.sh" build "$MODE" "$PROFILE"
"${ROOT_DIR}/scripts/deploy.sh" specs "$MODE" "$OUT_DIR"
"${ROOT_DIR}/scripts/deploy.sh" verify-specs "$OUT_DIR"

if [[ "$PROFILE" == "release" ]]; then
  NODE_BIN="${ROOT_DIR}/target/release/solochain-eterra-node"
else
  NODE_BIN="${ROOT_DIR}/target/debug/solochain-eterra-node"
fi

RAW_SPEC="${OUT_DIR}/${CHAIN}-raw.json"
mkdir -p "${BASE_PATH}/network"

# Keep a stable node key per base-path so p2p identity is persistent.
if [[ ! -f "${BASE_PATH}/network/secret_ed25519" ]]; then
  "$NODE_BIN" key generate-node-key \
    --chain "$RAW_SPEC" \
    --file "${BASE_PATH}/network/secret_ed25519" >/dev/null 2>&1
fi

# Insert validator keys for local single-validator operation.
"$NODE_BIN" key insert \
  --base-path "$BASE_PATH" \
  --chain "$RAW_SPEC" \
  --key-type aura \
  --scheme Sr25519 \
  --suri //Alice >/dev/null

"$NODE_BIN" key insert \
  --base-path "$BASE_PATH" \
  --chain "$RAW_SPEC" \
  --key-type gran \
  --scheme Ed25519 \
  --suri //Alice >/dev/null

echo "[run-node] mode=${MODE} chain=${CHAIN} profile=${PROFILE}"
echo "[run-node] base-path=${BASE_PATH}"

exec "$NODE_BIN" \
  --chain "$RAW_SPEC" \
  --base-path "$BASE_PATH" \
  --node-key-file "${BASE_PATH}/network/secret_ed25519" \
  --validator \
  --force-authoring \
  --port 30333 \
  --rpc-port 9944 \
  --public-addr /ip4/127.0.0.1/tcp/30333 \
  --unsafe-rpc-external \
  --rpc-cors all
