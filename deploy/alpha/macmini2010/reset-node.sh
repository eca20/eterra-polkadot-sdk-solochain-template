#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

load_env
require_cmd ssh

if [[ "${ETERRA_RELEASE_VERSION}" != "dev" ]]; then
	die "direct release reset is forbidden; use deploy-node.sh --purge-state --fresh-reset-readiness READINESS.json"
fi
if [[ "${1:-}" != "--yes" ]]; then
	die "refusing destructive reset without --yes"
fi

log "resetting alpha node data at ${REMOTE_NODE_DATA_DIR}"
remote_root_bash <<EOF
set -euo pipefail
systemctl stop "${REMOTE_NODE_SERVICE_NAME}"
rm -rf "${REMOTE_NODE_DATA_DIR}"
mkdir -p "${REMOTE_NODE_DATA_DIR}"
chown -R "${DEPLOY_USER}:${DEPLOY_USER}" "${REMOTE_NODE_DATA_DIR}"
systemctl start "${REMOTE_NODE_SERVICE_NAME}"
systemctl --no-pager --full status "${REMOTE_NODE_SERVICE_NAME}" || true
EOF
