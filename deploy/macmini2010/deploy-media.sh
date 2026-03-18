#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

load_env
require_cmd ssh
require_cmd rsync

bundle_dir="$(make_temp_dir)"
render_runtime_env_bundle "${bundle_dir}"

remote_bash <<EOF
set -euo pipefail

mkdir -p "${REMOTE_MEDIA_DIR}" "${DEPLOY_ROOT}/tmp"
EOF

log "syncing media service working tree to ${SSH_TARGET}"
rsync -az --delete -e "${RSYNC_RSH}" \
	--exclude '.git/' \
	--exclude 'node_modules/' \
	--exclude 'dist/' \
	--exclude '.env' \
	--exclude '.env.local' \
	--exclude '.DS_Store' \
	--exclude 'coverage/' \
	"${MEDIA_REPO_DIR}/" "${SSH_TARGET}:${REMOTE_MEDIA_DIR}/"

rsync_to_remote_no_delete "${bundle_dir}/media.env" "${DEPLOY_ROOT}/tmp/media.env"

log "building and restarting remote media stack"
remote_root_bash <<EOF
set -euo pipefail

mkdir -p "${REMOTE_SHARED_ENV_DIR}" "${DEPLOY_ROOT}/tmp"
install -m 0644 "${DEPLOY_ROOT}/tmp/media.env" "${REMOTE_MEDIA_ENV_FILE}"
chown root:root "${REMOTE_MEDIA_ENV_FILE}"
rm -f "${DEPLOY_ROOT}/tmp/media.env"
${REMOTE_DOCKER_COMPOSE_CMD} up -d --build
${REMOTE_DOCKER_COMPOSE_CMD} ps
EOF

log "media deploy complete"
