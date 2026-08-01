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

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

load_env
require_cmd ssh

remote_root_bash <<EOF
set -euo pipefail

echo "== alpha node service =="
systemctl --no-pager --full status "${REMOTE_NODE_SERVICE_NAME}" || true
echo
echo "== alpha node runtime version =="
curl -fsS -H 'Content-Type: application/json' \
	-d '{"id":1,"jsonrpc":"2.0","method":"state_getRuntimeVersion","params":[]}' \
	"http://127.0.0.1:${CHAIN_RPC_PORT}" || true
echo
echo
echo "== alpha media stack =="
${REMOTE_DOCKER_COMPOSE_CMD} ps || true
echo
echo "== alpha media health =="
curl -fsS "http://127.0.0.1:${MEDIA_PORT}/health/live" || true
echo
curl -fsS "http://127.0.0.1:${MEDIA_PORT}/health/ready" || true
echo
echo
echo "== alpha media upload should stay blocked =="
curl -sS -o /dev/null -w '%{http_code}\n' -X POST "http://127.0.0.1:${MEDIA_PORT}/media/upload" || true
echo
echo
echo "== alpha arcade authority service =="
systemctl --no-pager --full status "${AUTHORITY_SERVICE_NAME}" || true
echo
echo "== alpha arcade authority status =="
curl -fsS "http://127.0.0.1:${AUTHORITY_PORT}/v1/status" || true
echo
echo
echo "== alpha ipfs gateway =="
curl -sSI "http://127.0.0.1:${IPFS_GATEWAY_PORT}" | head -n 5 || true
EOF
