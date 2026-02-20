#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:-default}"          # default | production
CHAIN="${2:-testnet}"         # dev | testnet | production
PROFILE="${3:-release}"       # debug | release
ROLE="${4:-}"                 # validator | full (auto if omitted)

if [[ "$MODE" != "default" && "$MODE" != "production" ]]; then
  echo "usage: scripts/run-node.sh [default|production] [dev|testnet|production] [debug|release] [validator|full]" >&2
  exit 1
fi
if [[ "$CHAIN" != "dev" && "$CHAIN" != "testnet" && "$CHAIN" != "production" ]]; then
  echo "usage: scripts/run-node.sh [default|production] [dev|testnet|production] [debug|release] [validator|full]" >&2
  exit 1
fi
if [[ "$PROFILE" != "debug" && "$PROFILE" != "release" ]]; then
  echo "usage: scripts/run-node.sh [default|production] [dev|testnet|production] [debug|release] [validator|full]" >&2
  exit 1
fi

if [[ -z "$ROLE" ]]; then
  if [[ "$CHAIN" == "production" ]]; then
    ROLE="full"
  else
    ROLE="validator"
  fi
fi

if [[ "$ROLE" != "validator" && "$ROLE" != "full" ]]; then
  echo "usage: scripts/run-node.sh [default|production] [dev|testnet|production] [debug|release] [validator|full]" >&2
  exit 1
fi

OUT_DIR="${ROOT_DIR}/chain-specs/generated/${MODE}"
BASE_PATH="${BASE_PATH:-${ROOT_DIR}/data/${MODE}-${CHAIN}-${ROLE}}"
RPC_PORT="${RPC_PORT:-9944}"
P2P_PORT="${P2P_PORT:-30333}"
PUBLIC_ADDR="${PUBLIC_ADDR:-}"
EXPOSE_RPC="${EXPOSE_RPC:-0}"                # 1 -> --rpc-external
UNSAFE_RPC="${UNSAFE_RPC:-0}"                # 1 -> --unsafe-rpc-external (blocked for prod unless override)
ALLOW_UNSAFE_RPC_IN_PRODUCTION="${ALLOW_UNSAFE_RPC_IN_PRODUCTION:-0}"
RPC_CORS="${RPC_CORS:-none}"

if [[ "$EXPOSE_RPC" != "0" && "$EXPOSE_RPC" != "1" ]]; then
  echo "[run-node] EXPOSE_RPC must be 0 or 1" >&2
  exit 1
fi
if [[ "$UNSAFE_RPC" != "0" && "$UNSAFE_RPC" != "1" ]]; then
  echo "[run-node] UNSAFE_RPC must be 0 or 1" >&2
  exit 1
fi

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

if [[ "$UNSAFE_RPC" == "1" && "$EXPOSE_RPC" != "1" ]]; then
  echo "[run-node] UNSAFE_RPC=1 requires EXPOSE_RPC=1" >&2
  exit 1
fi

if [[ "$CHAIN" == "production" && "$UNSAFE_RPC" == "1" && "$ALLOW_UNSAFE_RPC_IN_PRODUCTION" != "1" ]]; then
  echo "[run-node] refusing unsafe RPC on production chain (set ALLOW_UNSAFE_RPC_IN_PRODUCTION=1 to override)" >&2
  exit 1
fi

if [[ "$ROLE" == "validator" ]]; then
  if [[ "$CHAIN" == "production" ]]; then
    AURA_SURI="${AURA_SURI:-}"
    GRAN_SURI="${GRAN_SURI:-}"
    if [[ -z "$AURA_SURI" || -z "$GRAN_SURI" ]]; then
      echo "[run-node] production validator requires AURA_SURI and GRAN_SURI environment variables" >&2
      exit 1
    fi
    if [[ "$AURA_SURI" == "//Alice" || "$AURA_SURI" == "//Bob" || "$GRAN_SURI" == "//Alice" || "$GRAN_SURI" == "//Bob" ]]; then
      echo "[run-node] refusing dev key suri for production validator" >&2
      exit 1
    fi
  else
    AURA_SURI="${AURA_SURI:-//Alice}"
    GRAN_SURI="${GRAN_SURI:-//Alice}"
  fi

  # Insert validator keys for single-validator startup.
  "$NODE_BIN" key insert \
    --base-path "$BASE_PATH" \
    --chain "$RAW_SPEC" \
    --key-type aura \
    --scheme Sr25519 \
    --suri "$AURA_SURI" >/dev/null

  "$NODE_BIN" key insert \
    --base-path "$BASE_PATH" \
    --chain "$RAW_SPEC" \
    --key-type gran \
    --scheme Ed25519 \
    --suri "$GRAN_SURI" >/dev/null
fi

ARGS=(
  --chain "$RAW_SPEC"
  --base-path "$BASE_PATH"
  --node-key-file "${BASE_PATH}/network/secret_ed25519"
  --port "$P2P_PORT"
  --rpc-port "$RPC_PORT"
  --rpc-methods Safe
)

if [[ -n "$PUBLIC_ADDR" ]]; then
  ARGS+=(--public-addr "$PUBLIC_ADDR")
fi

if [[ "$EXPOSE_RPC" == "1" ]]; then
  ARGS+=(--rpc-external --rpc-cors "$RPC_CORS")
fi

if [[ "$UNSAFE_RPC" == "1" ]]; then
  ARGS+=(--unsafe-rpc-external)
fi

if [[ "$ROLE" == "validator" ]]; then
  ARGS+=(--validator)
  # Force-authoring is useful for local dev chains; keep off by default on production.
  if [[ "$CHAIN" != "production" ]]; then
    ARGS+=(--force-authoring)
  fi
fi

echo "[run-node] mode=${MODE} chain=${CHAIN} profile=${PROFILE} role=${ROLE}"
echo "[run-node] base-path=${BASE_PATH}"

exec "$NODE_BIN" "${ARGS[@]}"
