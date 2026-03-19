#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

load_env
require_cmd expect
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
echo "== alpha ipfs gateway =="
curl -sSI "http://127.0.0.1:${IPFS_GATEWAY_PORT}" | head -n 5 || true
EOF
