#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

[[ $# -eq 1 ]] || die "usage: restore-alpha-state.sh <backup-dir>"
backup_dir="$1"
[[ -d "${backup_dir}" ]] || die "backup directory not found: ${backup_dir}"

load_env
require_cmd expect
require_cmd rsync
require_cmd ssh

remote_tmp_dir="${DEPLOY_ROOT}/tmp/restore-$(date +%Y%m%d%H%M%S)"

for required in node-data.tar.gz ipfs-data.tar.gz ipfs-staging.tar.gz node.env media.env; do
	[[ -f "${backup_dir}/${required}" ]] || die "backup is missing ${required}"
done

remote_root_bash <<EOF
set -euo pipefail
rm -rf "${remote_tmp_dir}"
mkdir -p "${remote_tmp_dir}"
EOF

rsync_to_remote "${backup_dir}/" "${remote_tmp_dir}/"

remote_root_bash <<EOF
set -euo pipefail
systemctl stop "${REMOTE_NODE_SERVICE_NAME}.service" >/dev/null 2>&1 || true
${REMOTE_DOCKER_COMPOSE_CMD} down >/dev/null 2>&1 || true

ipfs_data_mount="\$(docker volume inspect -f '{{ .Mountpoint }}' "${REMOTE_IPFS_DATA_VOLUME}")"
ipfs_staging_mount="\$(docker volume inspect -f '{{ .Mountpoint }}' "${REMOTE_IPFS_STAGING_VOLUME}")"

rm -rf "${REMOTE_NODE_DATA_DIR}"
mkdir -p "${REMOTE_NODE_DATA_DIR}"
chown -R "${DEPLOY_USER}:${DEPLOY_USER}" "${REMOTE_NODE_DATA_DIR}"
find "\${ipfs_data_mount}" -mindepth 1 -maxdepth 1 -exec rm -rf {} +
find "\${ipfs_staging_mount}" -mindepth 1 -maxdepth 1 -exec rm -rf {} +

tar xzf "${remote_tmp_dir}/node-data.tar.gz" -C "${REMOTE_NODE_DATA_DIR}"
tar xzf "${remote_tmp_dir}/ipfs-data.tar.gz" -C "\${ipfs_data_mount}"
tar xzf "${remote_tmp_dir}/ipfs-staging.tar.gz" -C "\${ipfs_staging_mount}"

install -m 0644 "${remote_tmp_dir}/node.env" "${REMOTE_NODE_ENV_FILE}"
install -m 0644 "${remote_tmp_dir}/media.env" "${REMOTE_MEDIA_ENV_FILE}"
chown root:root "${REMOTE_NODE_ENV_FILE}" "${REMOTE_MEDIA_ENV_FILE}"

systemctl start "${REMOTE_NODE_SERVICE_NAME}.service"
${REMOTE_DOCKER_COMPOSE_CMD} up -d
rm -rf "${remote_tmp_dir}"
EOF

log "alpha restore complete from ${backup_dir}"
