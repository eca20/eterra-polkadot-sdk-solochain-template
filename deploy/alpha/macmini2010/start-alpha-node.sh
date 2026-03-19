#!/usr/bin/env bash
set -euo pipefail

SELF_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
NODE_BIN="${NODE_BIN:-${SELF_DIR}/solochain-eterra-node}"
RAW_SPEC="${RAW_SPEC:-${SELF_DIR}/alpha-raw.json}"
BASE_PATH="${BASE_PATH:-/var/lib/eterra-alpha-node}"
CHAIN_RPC_PORT="${CHAIN_RPC_PORT:-9944}"
CHAIN_P2P_PORT="${CHAIN_P2P_PORT:-30333}"
RPC_CORS="${RPC_CORS:-https://eterra.online}"
MINI_LAN_IP="${MINI_LAN_IP:-}"
AURA_SURI="${AURA_SURI:-}"
GRAN_SURI="${GRAN_SURI:-}"
KEY_MARKER="${BASE_PATH}/.alpha-keys-inserted"

[[ -x "${NODE_BIN}" ]] || {
	echo "[start-alpha-node] missing node binary: ${NODE_BIN}" >&2
	exit 1
}
[[ -f "${RAW_SPEC}" ]] || {
	echo "[start-alpha-node] missing chain spec: ${RAW_SPEC}" >&2
	exit 1
}
[[ -n "${AURA_SURI}" ]] || {
	echo "[start-alpha-node] AURA_SURI must be set" >&2
	exit 1
}
[[ -n "${GRAN_SURI}" ]] || {
	echo "[start-alpha-node] GRAN_SURI must be set" >&2
	exit 1
}

mkdir -p "${BASE_PATH}/network"

if [[ ! -f "${BASE_PATH}/network/secret_ed25519" ]]; then
	"${NODE_BIN}" key generate-node-key --chain "${RAW_SPEC}" --file "${BASE_PATH}/network/secret_ed25519" >/dev/null 2>&1
fi

if [[ ! -f "${KEY_MARKER}" ]]; then
	"${NODE_BIN}" key insert \
		--base-path "${BASE_PATH}" \
		--chain "${RAW_SPEC}" \
		--key-type aura \
		--scheme Sr25519 \
		--suri "${AURA_SURI}" >/dev/null

	"${NODE_BIN}" key insert \
		--base-path "${BASE_PATH}" \
		--chain "${RAW_SPEC}" \
		--key-type gran \
		--scheme Ed25519 \
		--suri "${GRAN_SURI}" >/dev/null

	touch "${KEY_MARKER}"
fi

args=(
	--chain "${RAW_SPEC}"
	--base-path "${BASE_PATH}"
	--node-key-file "${BASE_PATH}/network/secret_ed25519"
	--validator
	--force-authoring
	--port "${CHAIN_P2P_PORT}"
	--rpc-port "${CHAIN_RPC_PORT}"
	--rpc-methods Safe
	--unsafe-rpc-external
	--rpc-cors "${RPC_CORS}"
)

if [[ -n "${MINI_LAN_IP}" ]]; then
	args+=(--public-addr "/ip4/${MINI_LAN_IP}/tcp/${CHAIN_P2P_PORT}")
fi

echo "[start-alpha-node] rpc=${CHAIN_RPC_PORT} p2p=${CHAIN_P2P_PORT} base_path=${BASE_PATH}"
exec "${NODE_BIN}" "${args[@]}"
