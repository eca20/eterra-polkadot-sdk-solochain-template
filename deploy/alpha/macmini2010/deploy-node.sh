#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

purge_state=0
while [[ $# -gt 0 ]]; do
	case "$1" in
		--purge-state)
			purge_state=1
			;;
		--help|-h)
			cat <<'EOF'
Usage: deploy-node.sh [--purge-state]

Normal deploys preserve the alpha node base path and chain state.
Pass --purge-state to wipe the remote alpha chain state before restarting.
Alpha spec/genesis changes are only applied when --purge-state is set.
EOF
			exit 0
			;;
		*)
			die "unknown argument: $1"
			;;
	esac
	shift
done

load_env
require_cmd expect
require_cmd git
require_cmd rsync
require_cmd shasum
require_cmd ssh

bundle_dir="$(make_temp_dir)"
remote_tmp_dir="${DEPLOY_ROOT}/tmp/node-deploy"
render_runtime_env_bundle "${bundle_dir}"
ensure_local_artifacts_dir

node_code_hash="$(compute_node_code_hash)"
node_spec_hash="$(compute_node_spec_hash)"
node_runtime_hash="$(compute_node_runtime_hash "${bundle_dir}/node.env")"
remote_node_bin_state="$(ssh_to_remote "if [[ -x $(shell_escape "${REMOTE_NODE_BIN}") ]]; then printf '1'; else printf '0'; fi")"
remote_node_spec_file_state="$(ssh_to_remote "if [[ -f $(shell_escape "${REMOTE_NODE_SPEC}") ]]; then printf '1'; else printf '0'; fi")"
remote_node_service_state="$(ssh_to_remote "if systemctl is-active --quiet $(shell_escape "${REMOTE_NODE_SERVICE_NAME}.service"); then printf '1'; else printf '0'; fi")"
remote_node_code_hash="$(ssh_to_remote "cat $(shell_escape "${REMOTE_NODE_CODE_HASH_FILE}") 2>/dev/null || true" | tr -d '\r\n')"
remote_node_spec_hash="$(ssh_to_remote "cat $(shell_escape "${REMOTE_NODE_SPEC_HASH_FILE}") 2>/dev/null || true" | tr -d '\r\n')"
remote_node_runtime_hash="$(ssh_to_remote "cat $(shell_escape "${REMOTE_NODE_RUNTIME_HASH_FILE}") 2>/dev/null || true" | tr -d '\r\n')"

node_build_needed=0
node_spec_needed=0
node_build_action="reuse-existing-build"
node_restart_reason="none"
node_spec_pending_apply=0

if [[ "${remote_node_bin_state}" != "1" ]] || [[ "${remote_node_code_hash}" != "${node_code_hash}" ]]; then
	node_build_needed=1
	node_spec_needed=1
	node_build_action="build"
	node_restart_reason="build"
elif [[ "${remote_node_spec_file_state}" != "1" ]]; then
	node_spec_needed=1
	node_build_action="finalize-spec"
	node_restart_reason="finalize-spec"
elif [[ "${remote_node_spec_hash}" != "${node_spec_hash}" ]]; then
	if [[ "${purge_state}" -eq 1 ]]; then
		node_spec_needed=1
		node_build_action="finalize-spec"
		node_restart_reason="finalize-spec"
	else
		node_spec_pending_apply=1
	fi
elif [[ "${remote_node_runtime_hash}" != "${node_runtime_hash}" ]]; then
	node_restart_reason="runtime-config"
elif [[ "${remote_node_service_state}" != "1" ]]; then
	node_restart_reason="service-not-running"
fi

if [[ "${node_spec_pending_apply}" -eq 1 ]]; then
	log "alpha spec/genesis changes detected but deferred to preserve live chain state; rerun with --purge-state to apply them"
fi

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
	--exclude 'deploy/alpha/macmini2010/.artifacts/' \
	--exclude '.DS_Store' \
	--exclude 'deploy/macmini2010.env' \
	--exclude 'deploy/alpha/macmini2010.env' \
	--exclude 'chain-specs/alpha-keys.json' \
	--exclude 'chain-specs/alpha-overrides.json' \
	--exclude 'solochain-eterra-node' \
	--exclude 'alpha-raw.json' \
	--exclude 'alpha-plain.json' \
	"${REPO_ROOT}/" \
	"${SSH_TARGET}:${REMOTE_NODE_DIR}/"

rsync_to_remote_no_delete "${bundle_dir}/node.env" "${remote_tmp_dir}/node.env"
rsync_to_remote_no_delete "${ALPHA_OVERRIDES_FILE}" "${remote_tmp_dir}/alpha-overrides.json"

log "determining alpha node build action on ${SSH_TARGET}"
remote_bash <<EOF
set -euo pipefail

source "${REMOTE_CARGO_ENV_FILE}"
cd "${REMOTE_NODE_DIR}"
echo "[alpha-macmini2010] node action: ${node_build_action}"

if [[ "${node_build_needed}" -eq 1 ]]; then
	export CARGO_TARGET_DIR="${REMOTE_CARGO_TARGET_DIR}"
	export SCCACHE_DIR="${REMOTE_SCCACHE_DIR}"
	if [[ "${REMOTE_CARGO_INCREMENTAL}" == "1" ]]; then
		export CARGO_PROFILE_RELEASE_INCREMENTAL=true
	fi
	if [[ "${ENABLE_REMOTE_SCCACHE}" == "1" ]] && command -v sccache >/dev/null 2>&1; then
		export RUSTC_WRAPPER="$(command -v sccache)"
	fi
	CARGO_TERM_COLOR=never cargo build -p solochain-eterra-node --release -j "${REMOTE_CARGO_JOBS}"
	install -m 0755 "${REMOTE_CARGO_TARGET_DIR}/release/solochain-eterra-node" "${REMOTE_NODE_BIN}"
fi

if [[ "${node_spec_needed}" -eq 1 ]]; then
	python3 "${REMOTE_NODE_DIR}/scripts/finalize-alpha-spec.py" \
		--node-bin "${REMOTE_NODE_BIN}" \
		--overrides "${remote_tmp_dir}/alpha-overrides.json" \
		--out-dir "${remote_tmp_dir}/finalized-alpha"
	install -m 0644 "${remote_tmp_dir}/finalized-alpha/alpha-plain.json" "${REMOTE_NODE_PLAIN_SPEC}"
	install -m 0644 "${remote_tmp_dir}/finalized-alpha/alpha-raw.json" "${REMOTE_NODE_SPEC}"
	mkdir -p "${remote_tmp_dir}/finalized-alpha"
	install -m 0644 "${REMOTE_NODE_PLAIN_SPEC}" "${remote_tmp_dir}/finalized-alpha/alpha-plain.json"
	install -m 0644 "${REMOTE_NODE_SPEC}" "${remote_tmp_dir}/finalized-alpha/alpha-raw.json"
fi
EOF

if [[ "${node_spec_needed}" -eq 1 ]]; then
	rsync_from_remote_no_delete "${remote_tmp_dir}/finalized-alpha/" "${LOCAL_FINALIZED_ALPHA_DIR}/"
fi

remote_root_bash <<EOF
set -euo pipefail

mkdir -p "${REMOTE_SHARED_ENV_DIR}" "${REMOTE_NODE_DATA_DIR}" "${REMOTE_STATE_DIR}"
install -m 0644 "${remote_tmp_dir}/node.env" "${REMOTE_NODE_ENV_FILE}"
chown root:root "${REMOTE_NODE_ENV_FILE}"
install -m 0755 "${REMOTE_NODE_DIR}/deploy/alpha/macmini2010/start-alpha-node.sh" "${REMOTE_START_SCRIPT}"
install -m 0644 "${REMOTE_NODE_DIR}/deploy/alpha/macmini2010/eterra-alpha-node.service" "${REMOTE_NODE_SERVICE_UNIT_FILE}"
chown root:root "${REMOTE_START_SCRIPT}" "${REMOTE_NODE_SERVICE_UNIT_FILE}"
systemctl disable --now "${LEGACY_NODE_SERVICE_NAME}.service" >/dev/null 2>&1 || true

systemctl daemon-reload
systemctl enable "${REMOTE_NODE_SERVICE_NAME}.service"

if [[ "${purge_state}" -eq 1 ]]; then
	echo "[alpha-macmini2010] node action: purge-state"
	systemctl stop "${REMOTE_NODE_SERVICE_NAME}.service" >/dev/null 2>&1 || true
	rm -rf "${REMOTE_NODE_DATA_DIR}"
	mkdir -p "${REMOTE_NODE_DATA_DIR}"
	chown -R "${DEPLOY_USER}:${DEPLOY_USER}" "${REMOTE_NODE_DATA_DIR}"
fi

if [[ "${node_restart_reason}" == "none" ]]; then
	echo "[alpha-macmini2010] node action: service already up to date"
else
	echo "[alpha-macmini2010] node action: restart (${node_restart_reason})"
	systemctl restart "${REMOTE_NODE_SERVICE_NAME}.service"
fi

systemctl is-active --quiet "${REMOTE_NODE_SERVICE_NAME}.service"
printf '%s\n' "${node_code_hash}" >"${REMOTE_NODE_CODE_HASH_FILE}"
printf '%s\n' "${node_runtime_hash}" >"${REMOTE_NODE_RUNTIME_HASH_FILE}"
if [[ "${node_spec_needed}" -eq 1 ]]; then
	printf '%s\n' "${node_spec_hash}" >"${REMOTE_NODE_SPEC_HASH_FILE}"
	chown root:root "${REMOTE_NODE_SPEC_HASH_FILE}"
fi
chown root:root "${REMOTE_NODE_CODE_HASH_FILE}" "${REMOTE_NODE_RUNTIME_HASH_FILE}"
systemctl --no-pager --full status "${REMOTE_NODE_SERVICE_NAME}.service" || true
rm -rf "${remote_tmp_dir}"
EOF

log "alpha node deploy complete"
