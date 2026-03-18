#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

load_env
require_cmd ssh

TARGET="${1:-all}"
LINES="${LINES:-100}"

case "${TARGET}" in
	node)
		remote_root_bash <<EOF
journalctl -u "${REMOTE_NODE_SERVICE_NAME}" -n "${LINES}" --no-pager
EOF
		;;
	media)
		remote_root_bash <<EOF
${REMOTE_DOCKER_COMPOSE_CMD} logs --tail "${LINES}"
EOF
		;;
	all)
		remote_root_bash <<EOF
echo "== node =="
journalctl -u "${REMOTE_NODE_SERVICE_NAME}" -n "${LINES}" --no-pager || true
echo
echo "== media =="
${REMOTE_DOCKER_COMPOSE_CMD} logs --tail "${LINES}" || true
EOF
		;;
	*)
		die "usage: $(basename "$0") [node|media|all]"
		;;
esac
