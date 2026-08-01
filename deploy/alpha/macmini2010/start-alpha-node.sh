#!/bin/bash
set -euo pipefail
# Closed union of every credential-bearing environment variable accepted by the
# chain, site, or Unity private-alpha deployment lanes.  Preserve each shell
# value for local use, but remove its export attribute before the first child.
export -n DEPLOY_PASSWORD REMOTE_SUDO_PASSWORD AURA_SURI GRAN_SURI \
	MEDIA_SIGNER_SEED MEDIA_ADMIN_API_KEY AUTHORITY_RELAY_MNEMONIC \
	AUTHORITY_RELAY_DERIVATION_PASSWORD ETERRA_LEGENDS_SIGNER_MNEMONIC \
	ETERRA_LEGENDS_SIGNER_DERIVATION_PASSWORD ETERRA_LEGENDS_PRIVATE_ALPHA_ACCESS_KEY \
	ETERRA_ALPHA_SUDO_MNEMONIC ETERRA_ALPHA_SUDO_DERIVATION_PASSWORD \
	ADMIN_SESSION_SECRET ALPHA_ACCESS_SESSION_SECRET DISCORD_CLIENT_SECRET \
	DISCORD_BOT_TOKEN MONGODB_URI ETERRA_LEGENDS_PLAYER_ACCESS_TOKEN \
	NEXUS_V2_PRIVATE_ALPHA_ACCESS_KEY NEXUS_V2_SESSION_AUTHORIZATION_PROFILES_JSON \
	ADMIN_API_KEY ETERRA_FPS_V2_OWNER_SECRET_PATH \
	ETERRA_FPS_V2_PLAYER_GATEWAY_ACCESS_TOKEN ETERRA_FPS_V2_ROOT_SECRET_PATH \
	ETERRA_FPS_V2_SUDO_SECRET_PATH 2>/dev/null || true

SELF_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
NODE_BIN="${NODE_BIN:-${SELF_DIR}/solochain-eterra-node}"
RAW_SPEC="${RAW_SPEC:-${SELF_DIR}/alpha-raw.json}"
BASE_PATH="${BASE_PATH:-/var/lib/eterra-alpha-node}"
CHAIN_RPC_PORT="${CHAIN_RPC_PORT:-9944}"
CHAIN_P2P_PORT="${CHAIN_P2P_PORT:-30333}"
RPC_CORS="${RPC_CORS:-https://eterra.online}"
MINI_LAN_IP="${MINI_LAN_IP:-}"
NEXUS_V2_PHASE1_CLOSED="${NEXUS_V2_PHASE1_CLOSED:-0}"
RPC_BIND_HOST="${RPC_BIND_HOST:-0.0.0.0}"
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
	--rpc-cors "${RPC_CORS}"
)

case "${NEXUS_V2_PHASE1_CLOSED}" in
	1|true|TRUE|True|yes|YES|Yes|on|ON|On)
		[[ "${RPC_BIND_HOST}" == "127.0.0.1" ]] || {
			echo "[start-alpha-node] Phase-1 closed mode requires RPC_BIND_HOST=127.0.0.1" >&2
			exit 1
		}
		# Substrate binds RPC to loopback by default.  Omission of both external
		# RPC flags is intentional. P2P is explicitly bound to loopback so the
		# closed replacement cannot accept peer ingress even if host firewall
		# policy is later misconfigured. Both listeners are verified after start.
		args+=(--listen-addr "/ip4/127.0.0.1/tcp/${CHAIN_P2P_PORT}")
		;;
	0|false|FALSE|False|no|NO|No|off|OFF|Off)
		args+=(--unsafe-rpc-external)
		if [[ -n "${MINI_LAN_IP}" ]]; then
			args+=(--public-addr "/ip4/${MINI_LAN_IP}/tcp/${CHAIN_P2P_PORT}")
		fi
		;;
	*)
		echo "[start-alpha-node] invalid NEXUS_V2_PHASE1_CLOSED value" >&2
		exit 1
		;;
esac

if [[ "${NEXUS_V2_PHASE1_CLOSED}" =~ ^(1|true|TRUE|True|yes|YES|Yes|on|ON|On)$ ]]; then
	echo "[start-alpha-node] phase1_closed=true rpc_bind=127.0.0.1 rpc=${CHAIN_RPC_PORT} p2p=${CHAIN_P2P_PORT} base_path=${BASE_PATH}"
else
	echo "[start-alpha-node] phase1_closed=false rpc_bind=external rpc=${CHAIN_RPC_PORT} p2p=${CHAIN_P2P_PORT} base_path=${BASE_PATH}"
fi
exec "${NODE_BIN}" "${args[@]}"
