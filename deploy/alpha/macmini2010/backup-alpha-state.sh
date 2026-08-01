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
