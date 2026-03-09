#!/usr/bin/env bash

set -euo pipefail

CHAIN_RPC_URL="${CHAIN_RPC_URL:-http://127.0.0.1:9944}"
MEDIA_BASE_URL="${MEDIA_BASE_URL:-http://127.0.0.1:4000}"
WEB_BASE_URL="${WEB_BASE_URL:-}"
CARD_ID="${CARD_ID:-}"

json_rpc() {
	local method="$1"
	local params="${2:-[]}"
	curl -fsS \
		-H 'Content-Type: application/json' \
		-d "{\"id\":1,\"jsonrpc\":\"2.0\",\"method\":\"${method}\",\"params\":${params}}" \
		"${CHAIN_RPC_URL}"
}

require_http_ok() {
	local url="$1"
	local label="$2"
	local status
	status="$(curl -sS -o /dev/null -w '%{http_code}' "$url")"
	if [[ "$status" != "200" ]]; then
		echo "[smoke] ${label} failed: ${url} -> HTTP ${status}" >&2
		exit 1
	fi
	echo "[smoke] ${label}: ok"
}

require_json_contains() {
	local body="$1"
	local needle="$2"
	local label="$3"
	if [[ "$body" != *"$needle"* ]]; then
		echo "[smoke] ${label} failed: expected payload to contain ${needle}" >&2
		echo "$body" >&2
		exit 1
	fi
	echo "[smoke] ${label}: ok"
}

echo "[smoke] chain rpc: ${CHAIN_RPC_URL}"
echo "[smoke] media service: ${MEDIA_BASE_URL}"
if [[ -n "$WEB_BASE_URL" ]]; then
	echo "[smoke] web: ${WEB_BASE_URL}"
fi
if [[ -n "$CARD_ID" ]]; then
	echo "[smoke] card id: ${CARD_ID}"
fi

health_payload="$(json_rpc system_health)"
require_json_contains "$health_payload" '"result"' 'chain system_health'

runtime_payload="$(json_rpc state_getRuntimeVersion)"
require_json_contains "$runtime_payload" '"specName"' 'chain runtime version'

require_http_ok "${MEDIA_BASE_URL}/health/live" 'media /health/live'
require_http_ok "${MEDIA_BASE_URL}/health/ready" 'media /health/ready'

if [[ -n "$WEB_BASE_URL" ]]; then
	require_http_ok "${WEB_BASE_URL}" 'web root'
fi

if [[ -n "$CARD_ID" ]]; then
	require_http_ok "${MEDIA_BASE_URL}/nft/cards/${CARD_ID}/metadata.json" 'nft metadata'
	require_http_ok "${MEDIA_BASE_URL}/nft/cards/${CARD_ID}/image.png" 'nft image'
fi

echo "[smoke] controlled-beta smoke checks passed"
