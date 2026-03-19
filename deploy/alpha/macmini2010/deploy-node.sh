#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

load_env
require_cmd expect
require_cmd rsync
require_cmd ssh

bundle_dir="$(make_temp_dir)"
remote_tmp_dir="${DEPLOY_ROOT}/tmp/node-deploy"
render_runtime_env_bundle "${bundle_dir}"
ensure_local_artifacts_dir

remote_bash <<EOF
set -euo pipefail
mkdir -p "${remote_tmp_dir}" "${REMOTE_NODE_DIR}"
EOF

log "syncing alpha solochain working tree to ${SSH_TARGET}"
rsync_with_remote \
	-az \
	--delete \
	-e "${RSYNC_RSH}" \
	--exclude '.git/' \
	--exclude 'target/' \
	--exclude 'data/' \
	--exclude '.DS_Store' \
	--exclude 'deploy/macmini2010.env' \
	--exclude 'deploy/alpha/macmini2010.env' \
	--exclude 'chain-specs/alpha-keys.json' \
	--exclude 'chain-specs/alpha-overrides.json' \
	"${REPO_ROOT}/" \
	"${SSH_TARGET}:${REMOTE_NODE_DIR}/"

rsync_to_remote_no_delete "${bundle_dir}/node.env" "${remote_tmp_dir}/node.env"
rsync_to_remote_no_delete "${ALPHA_OVERRIDES_FILE}" "${remote_tmp_dir}/alpha-overrides.json"

log "building alpha node and finalizing alpha chain spec on ${SSH_TARGET}"
remote_bash <<EOF
set -euo pipefail

source "${REMOTE_CARGO_ENV_FILE}"
cd "${REMOTE_NODE_DIR}"
CARGO_TERM_COLOR=never cargo build -p solochain-eterra-node --release -j "${REMOTE_CARGO_JOBS}"
install -m 0755 "target/release/solochain-eterra-node" "${REMOTE_NODE_BIN}"
python3 "${REMOTE_NODE_DIR}/scripts/finalize-alpha-spec.py" \
	--node-bin "${REMOTE_NODE_BIN}" \
	--overrides "${remote_tmp_dir}/alpha-overrides.json" \
	--out-dir "${remote_tmp_dir}/finalized-alpha"
install -m 0644 "${remote_tmp_dir}/finalized-alpha/alpha-plain.json" "${REMOTE_NODE_PLAIN_SPEC}"
install -m 0644 "${remote_tmp_dir}/finalized-alpha/alpha-raw.json" "${REMOTE_NODE_SPEC}"
install -m 0755 "${REMOTE_NODE_DIR}/deploy/alpha/macmini2010/start-alpha-node.sh" "${REMOTE_START_SCRIPT}"
EOF

rsync_from_remote_no_delete "${remote_tmp_dir}/finalized-alpha/" "${LOCAL_FINALIZED_ALPHA_DIR}/"

remote_root_bash <<EOF
set -euo pipefail

mkdir -p "${REMOTE_SHARED_ENV_DIR}" "${REMOTE_NODE_DATA_DIR}"
install -m 0644 "${remote_tmp_dir}/node.env" "${REMOTE_NODE_ENV_FILE}"
chown root:root "${REMOTE_NODE_ENV_FILE}"
systemctl disable --now "${LEGACY_NODE_SERVICE_NAME}.service" >/dev/null 2>&1 || true
systemctl daemon-reload
systemctl enable "${REMOTE_NODE_SERVICE_NAME}.service"
systemctl restart "${REMOTE_NODE_SERVICE_NAME}.service"
systemctl --no-pager --full status "${REMOTE_NODE_SERVICE_NAME}.service" || true
rm -rf "${remote_tmp_dir}"
EOF

log "alpha node deploy complete"
