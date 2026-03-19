#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

load_env
require_cmd expect
require_cmd ssh

LINES="${LINES:-150}"

remote_root_bash <<EOF
set -euo pipefail

echo "== alpha node logs =="
journalctl -u "${REMOTE_NODE_SERVICE_NAME}" -n "${LINES}" --no-pager || true
echo
echo "== alpha media logs =="
${REMOTE_DOCKER_COMPOSE_CMD} logs --tail "${LINES}" || true
EOF
