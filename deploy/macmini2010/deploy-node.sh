#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

load_env
require_cmd ssh
require_cmd rsync

bundle_dir="$(make_temp_dir)"
remote_tmp_dir="${DEPLOY_ROOT}/tmp/node-deploy"

render_runtime_env_bundle "${bundle_dir}"

remote_bash <<EOF
set -euo pipefail

mkdir -p "${remote_tmp_dir}" "${REMOTE_NODE_DIR}"
EOF

log "syncing solochain working tree to ${SSH_TARGET}"
rsync -az --delete -e "${RSYNC_RSH}" \
	--exclude '.git/' \
	--exclude 'target/' \
	--exclude 'data/' \
	--exclude '.DS_Store' \
	--exclude 'deploy/macmini2010.env' \
	"${REPO_ROOT}/" "${SSH_TARGET}:${REMOTE_NODE_DIR}/"

rsync_to_remote_no_delete "${bundle_dir}/node.env" "${remote_tmp_dir}/node.env"

log "installing node bundle on ${SSH_TARGET}"
case "${NODE_BUILD_MODE}" in
	remote-native)
		log "building node natively on ${SSH_TARGET}"
		remote_bash <<EOF
set -euo pipefail

source "${REMOTE_CARGO_ENV_FILE}"
cd "${REMOTE_NODE_DIR}"
CARGO_TERM_COLOR=never cargo build -p solochain-eterra-node --release -j "${REMOTE_CARGO_JOBS}"
install -m 0755 "target/release/solochain-eterra-node" "${REMOTE_NODE_BIN}"
"${REMOTE_NODE_BIN}" build-spec --disable-default-bootnode --chain dev > "${REMOTE_NODE_DIR}/dev-plain.json"
"${REMOTE_NODE_BIN}" build-spec --disable-default-bootnode --chain "${REMOTE_NODE_DIR}/dev-plain.json" --raw > "${REMOTE_NODE_SPEC}"
install -m 0755 "${REMOTE_NODE_DIR}/deploy/macmini2010/start-dev-node.sh" "${REMOTE_START_SCRIPT}"
EOF
		;;
	local-docker)
		die "NODE_BUILD_MODE=local-docker is no longer the default on this setup; use remote-native"
		;;
	*)
		die "unsupported NODE_BUILD_MODE: ${NODE_BUILD_MODE}"
		;;
esac

remote_root_bash <<EOF
set -euo pipefail

mkdir -p "${REMOTE_SHARED_ENV_DIR}" "${REMOTE_NODE_DATA_DIR}"
install -m 0644 "${remote_tmp_dir}/node.env" "${REMOTE_NODE_ENV_FILE}"
chown root:root "${REMOTE_NODE_ENV_FILE}"
systemctl daemon-reload
systemctl restart "${REMOTE_NODE_SERVICE_NAME}.service"
systemctl is-active --quiet "${REMOTE_NODE_SERVICE_NAME}.service"
systemctl --no-pager --full status "${REMOTE_NODE_SERVICE_NAME}.service" || true
rm -rf "${remote_tmp_dir}"
EOF

if [[ "${NODE_BUILD_MODE}" == "remote-native" ]] && [[ "${REMOTE_CARGO_CLEAN_AFTER_DEPLOY}" == "1" ]]; then
	log "removing remote Cargo build directory after successful node installation"
	remote_bash <<EOF
set -euo pipefail
target_dir="${REMOTE_NODE_DIR}/target"
node_dir="${REMOTE_NODE_DIR}"
if [[ "\${target_dir}" != "\${node_dir}/target" ]] || [[ "\${target_dir}" == "/" ]]; then
	echo "[macmini2010] refusing unsafe Cargo cleanup path: \${target_dir}" >&2
	exit 1
fi
rm -rf -- "\${target_dir}"
echo "[macmini2010] removed Cargo build directory: \${target_dir}"
EOF
fi

log "node deploy complete"
