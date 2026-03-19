#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

load_env
require_cmd expect
require_cmd git
require_cmd rsync
require_cmd shasum
require_cmd ssh

bundle_dir="$(make_temp_dir)"
render_runtime_env_bundle "${bundle_dir}"
media_build_hash="$(compute_media_build_hash)"
media_runtime_hash="$(compute_media_runtime_hash "${bundle_dir}/media.env")"

remote_bash <<EOF
set -euo pipefail
mkdir -p "${REMOTE_MEDIA_DIR}" "${DEPLOY_ROOT}/tmp"
EOF

log "syncing alpha media service working tree to ${SSH_TARGET}"
rsync_with_remote \
	-az \
	--delete \
	-e "${RSYNC_RSH}" \
	--exclude '.git/' \
	--exclude 'node_modules/' \
	--exclude 'dist/' \
	--exclude '.env' \
	--exclude '.env.local' \
	--exclude '.DS_Store' \
	--exclude 'coverage/' \
	"${MEDIA_REPO_DIR}/" "${SSH_TARGET}:${REMOTE_MEDIA_DIR}/"

rsync_to_remote_no_delete "${bundle_dir}/media.env" "${DEPLOY_ROOT}/tmp/media.env"

log "cutting over media stack and starting alpha media compose project"
remote_root_bash <<EOF
set -euo pipefail

mkdir -p "${REMOTE_SHARED_ENV_DIR}" "${DEPLOY_ROOT}/tmp" "${REMOTE_STATE_DIR}"
install -m 0644 "${DEPLOY_ROOT}/tmp/media.env" "${REMOTE_MEDIA_ENV_FILE}"
chown root:root "${REMOTE_MEDIA_ENV_FILE}"
rm -f "${DEPLOY_ROOT}/tmp/media.env"

if [[ -f "${LEGACY_MEDIA_COMPOSE_BASE}" && -f "${LEGACY_MEDIA_ENV_FILE}" ]]; then
	${LEGACY_MEDIA_COMPOSE_CMD} down --remove-orphans >/dev/null 2>&1 || true
fi

media_action="skip"
if [[ ! -f "${REMOTE_MEDIA_BUILD_HASH_FILE}" ]] || [[ "\$(cat "${REMOTE_MEDIA_BUILD_HASH_FILE}")" != "${media_build_hash}" ]]; then
	media_action="build"
elif [[ ! -f "${REMOTE_MEDIA_RUNTIME_HASH_FILE}" ]] || [[ "\$(cat "${REMOTE_MEDIA_RUNTIME_HASH_FILE}")" != "${media_runtime_hash}" ]]; then
	media_action="reconcile"
elif ! ${REMOTE_DOCKER_COMPOSE_CMD} ps --status running --services 2>/dev/null | grep -qx 'ipfs'; then
	media_action="reconcile"
elif ! ${REMOTE_DOCKER_COMPOSE_CMD} ps --status running --services 2>/dev/null | grep -qx 'media-service'; then
	media_action="reconcile"
fi

case "\${media_action}" in
	build)
		echo "[alpha-macmini2010] media action: rebuild image"
		${REMOTE_DOCKER_COMPOSE_CMD} up -d --build --remove-orphans
		;;
	reconcile)
		echo "[alpha-macmini2010] media action: reuse image and reconcile services"
		${REMOTE_DOCKER_COMPOSE_CMD} up -d --remove-orphans
		;;
	*)
		echo "[alpha-macmini2010] media action: stack already up to date"
		;;
esac

${REMOTE_DOCKER_COMPOSE_CMD} ps
printf '%s\n' "${media_build_hash}" >"${REMOTE_MEDIA_BUILD_HASH_FILE}"
printf '%s\n' "${media_runtime_hash}" >"${REMOTE_MEDIA_RUNTIME_HASH_FILE}"
chown root:root "${REMOTE_MEDIA_BUILD_HASH_FILE}" "${REMOTE_MEDIA_RUNTIME_HASH_FILE}"
EOF

log "alpha media deploy complete"
