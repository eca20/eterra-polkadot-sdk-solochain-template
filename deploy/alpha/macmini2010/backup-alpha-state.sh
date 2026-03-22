#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

load_env
require_cmd expect
require_cmd rsync
require_cmd ssh

backup_name="${1:-alpha-backup-$(date +%Y%m%d%H%M%S)}"
local_out_dir="${ARTIFACTS_DIR}/backups/${backup_name}"
remote_tmp_dir="${DEPLOY_ROOT}/tmp/${backup_name}"

mkdir -p "${local_out_dir}"

remote_root_bash <<EOF
set -euo pipefail
rm -rf "${remote_tmp_dir}"
mkdir -p "${remote_tmp_dir}"

systemctl stop "${REMOTE_NODE_SERVICE_NAME}.service" >/dev/null 2>&1 || true
${REMOTE_DOCKER_COMPOSE_CMD} stop media-service ipfs >/dev/null 2>&1 || true

ipfs_data_mount="\$(docker volume inspect -f '{{ .Mountpoint }}' "${REMOTE_IPFS_DATA_VOLUME}")"
ipfs_staging_mount="\$(docker volume inspect -f '{{ .Mountpoint }}' "${REMOTE_IPFS_STAGING_VOLUME}")"

tar czf "${remote_tmp_dir}/node-data.tar.gz" -C "${REMOTE_NODE_DATA_DIR}" .
tar czf "${remote_tmp_dir}/ipfs-data.tar.gz" -C "\${ipfs_data_mount}" .
tar czf "${remote_tmp_dir}/ipfs-staging.tar.gz" -C "\${ipfs_staging_mount}" .
cp "${REMOTE_NODE_ENV_FILE}" "${remote_tmp_dir}/node.env"
cp "${REMOTE_MEDIA_ENV_FILE}" "${remote_tmp_dir}/media.env"

cat >"${remote_tmp_dir}/backup-meta.txt" <<META
backup_name=${backup_name}
created_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
node_data_dir=${REMOTE_NODE_DATA_DIR}
ipfs_data_volume=${REMOTE_IPFS_DATA_VOLUME}
ipfs_staging_volume=${REMOTE_IPFS_STAGING_VOLUME}
META

systemctl start "${REMOTE_NODE_SERVICE_NAME}.service" >/dev/null 2>&1 || true
${REMOTE_DOCKER_COMPOSE_CMD} up -d >/dev/null 2>&1 || true
EOF

rsync_from_remote_no_delete "${remote_tmp_dir}/" "${local_out_dir}/"
remote_root_bash "rm -rf $(shell_escape "${remote_tmp_dir}")"

log "alpha backup complete: ${local_out_dir}"
