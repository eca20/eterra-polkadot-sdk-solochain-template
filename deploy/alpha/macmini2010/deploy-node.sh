#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

purge_state=0
dry_run=0
fresh_reset_readiness=""
verify_restored_final_backup=""
promotion_manifest=""
target_identity=""
evidence_output=""
while [[ $# -gt 0 ]]; do
	case "$1" in
		--purge-state)
			purge_state=1
			;;
		--fresh-reset-readiness)
			[[ $# -ge 2 ]] || die "--fresh-reset-readiness requires a packet path"
			fresh_reset_readiness="$2"
			shift
			;;
		--dry-run)
			dry_run=1
			;;
		--verify-restored-final-backup)
			[[ $# -ge 2 ]] || die "--verify-restored-final-backup requires a staging path"
			verify_restored_final_backup="$2"
			shift
			;;
		--promote-candidate)
			[[ $# -ge 2 ]] || die "--promote-candidate requires node-candidate.json"
			promotion_manifest="$2"
			shift
			;;
		--evidence)
			[[ $# -ge 2 ]] || die "--evidence requires an output path"
			evidence_output="$2"
			shift
			;;
		--target-identity)
			[[ $# -ge 2 ]] || die "--target-identity requires eterra-spec106-target-identity.v1.json"
			target_identity="$2"
			shift
			;;
		--help|-h)
			cat <<'EOF'
Usage: deploy-node.sh [--purge-state]
       deploy-node.sh --purge-state --fresh-reset-readiness READINESS.json \
         --promote-candidate /path/to/node-candidate.json \
         --target-identity /path/to/eterra-spec106-target-identity.v1.json \
         [--evidence OUTPUT.json] [--dry-run]
       deploy-node.sh --verify-restored-final-backup STAGING_DIR

Normal deploys preserve the alpha node base path and chain state.
Development deploys may pass --purge-state directly.
Release deploys accept it only with a SHA-256-pinned, frozen pre-V16
--fresh-reset-readiness packet. --dry-run validates the guarded local plan and
exits before SSH.
Release node deployment requires immutable candidate promotion. The candidate
must be built locally from the exact runtime bundle and address-only private
Alpha overrides. Promotion installs its pinned native binary and plain/raw
spec bytes without a remote Cargo build or remote spec finalization.
The restore-verification mode is read-only. It proves that the exact staged
node binary, chain spec, service unit, and environment are installed and that
the restored node is healthy; it never builds, syncs, restarts, or deploys.
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
require_cmd jq
require_cmd python3
require_cmd rsync
require_cmd shasum
require_cmd ssh

if [[ -n "${verify_restored_final_backup}" ]]; then
	[[ "${purge_state}" -eq 0 && "${dry_run}" -eq 0 && -z "${fresh_reset_readiness}" && -z "${promotion_manifest}" && -z "${target_identity}" && -z "${evidence_output}" ]] ||
		die "--verify-restored-final-backup cannot be combined with deploy/reset options"
	staging_dir="$(cd -- "${verify_restored_final_backup}" 2>/dev/null && pwd)" ||
		die "restore staging directory not found: ${verify_restored_final_backup}"
	[[ -f "${staging_dir}/staging-contract.json" ]] ||
		die "restore staging contract is missing"
	for restored_file in node-binary chain-spec.json node-service.service node.env; do
		[[ -f "${staging_dir}/${restored_file}" && ! -L "${staging_dir}/${restored_file}" ]] ||
			die "restore staging node file is unavailable: ${restored_file}"
		expected_sha256="$(jq -er --arg name "${restored_file}" '.files[$name]' "${staging_dir}/staging-contract.json")" ||
			die "restore staging contract does not pin ${restored_file}"
		[[ "${expected_sha256}" =~ ^[0-9a-f]{64}$ ]] ||
			die "restore staging hash is invalid: ${restored_file}"
		[[ "$(shasum -a 256 "${staging_dir}/${restored_file}" | awk '{print $1}')" == "${expected_sha256}" ]] ||
			die "restore staging hash mismatch: ${restored_file}"
	done
	[[ "$(jq -r '.schemaVersion' "${staging_dir}/staging-contract.json")" == "1" ]] ||
		die "unsupported restore staging schema"
	[[ "$(jq -r '.kind' "${staging_dir}/staging-contract.json")" == "nexus-v2-private-alpha-chain-media-restore-staging" ]] ||
		die "restore staging kind mismatch"
	[[ "$(jq -r '.releaseId' "${staging_dir}/staging-contract.json")" == "${ETERRA_RELEASE_VERSION}" ]] ||
		die "restore staging release mismatch"
	[[ "$(jq -r '.sourceCommit' "${staging_dir}/staging-contract.json")" == "${ETERRA_EXPECTED_CHAIN_COMMIT}" ]] ||
		die "restore staging chain source mismatch"
	[[ "$(jq -r '.componentSourceCommits.chain' "${staging_dir}/staging-contract.json")" == "${ETERRA_EXPECTED_CHAIN_COMMIT}" ]] ||
		die "restore staging chain component source mismatch"
	manifest_sha256="$(jq -r '.backupManifestSha256' "${staging_dir}/staging-contract.json")"
	[[ "${manifest_sha256}" =~ ^[0-9a-f]{64}$ ]] ||
		die "restore staging manifest hash is invalid"
	expected_binary_sha256="$(jq -r '.files[\"node-binary\"]' "${staging_dir}/staging-contract.json")"
	expected_spec_sha256="$(jq -r '.files[\"chain-spec.json\"]' "${staging_dir}/staging-contract.json")"
	expected_service_sha256="$(jq -r '.files[\"node-service.service\"]' "${staging_dir}/staging-contract.json")"
	expected_env_sha256="$(jq -r '.files[\"node.env\"]' "${staging_dir}/staging-contract.json")"
	jq -e '.name | type == "string" and length > 0' "${staging_dir}/chain-spec.json" >/dev/null ||
		die "restored chain spec name is invalid"
	require_cmd curl
	remote_root_bash <<EOF
set -euo pipefail
test "${DEPLOY_ROOT}" = "/opt/eterra-alpha"
test "${REMOTE_NODE_DATA_DIR}" = "/var/lib/eterra-alpha-node"
test "${CHAIN_RPC_PORT}" = "9944"
test -f "${REMOTE_STATE_DIR}/nexus-v2-final-backup-restored.json"
test "\$(jq -r '.schemaVersion' "${REMOTE_STATE_DIR}/nexus-v2-final-backup-restored.json")" = "1"
test "\$(jq -r '.kind' "${REMOTE_STATE_DIR}/nexus-v2-final-backup-restored.json")" = "nexus-v2-private-alpha-final-backup-restored"
test "\$(jq -r '.releaseId' "${REMOTE_STATE_DIR}/nexus-v2-final-backup-restored.json")" = "${ETERRA_RELEASE_VERSION}"
test "\$(jq -r '.sourceCommit' "${REMOTE_STATE_DIR}/nexus-v2-final-backup-restored.json")" = "${ETERRA_EXPECTED_CHAIN_COMMIT}"
test "\$(jq -r '.backupManifestSha256' "${REMOTE_STATE_DIR}/nexus-v2-final-backup-restored.json")" = "${manifest_sha256}"
test "\$(shasum -a 256 "${REMOTE_NODE_BIN}" | awk '{print \$1}')" = "${expected_binary_sha256}"
test "\$(shasum -a 256 "${REMOTE_NODE_SPEC}" | awk '{print \$1}')" = "${expected_spec_sha256}"
test "\$(shasum -a 256 "${REMOTE_NODE_SERVICE_UNIT_FILE}" | awk '{print \$1}')" = "${expected_service_sha256}"
test "\$(shasum -a 256 "${REMOTE_NODE_ENV_FILE}" | awk '{print \$1}')" = "${expected_env_sha256}"
systemctl is-active --quiet "${REMOTE_NODE_SERVICE_NAME}.service"
chain_response="\$(curl -fsS --max-time 10 -H 'Content-Type: application/json' \
	-d '{"id":1,"jsonrpc":"2.0","method":"system_chain","params":[]}' \
	"http://127.0.0.1:${CHAIN_RPC_PORT}")"
jq -e '.result | type == "string" and length > 0' <<<"\${chain_response}" >/dev/null
genesis_response="\$(curl -fsS --max-time 10 -H 'Content-Type: application/json' \
	-d '{"id":1,"jsonrpc":"2.0","method":"chain_getBlockHash","params":[0]}' \
	"http://127.0.0.1:${CHAIN_RPC_PORT}")"
jq -e '.result | type == "string" and test("^0x[0-9a-fA-F]{64}$")' <<<"\${genesis_response}" >/dev/null
runtime_response="\$(curl -fsS --max-time 10 -H 'Content-Type: application/json' \
	-d '{"id":1,"jsonrpc":"2.0","method":"state_getRuntimeVersion","params":[]}' \
	"http://127.0.0.1:${CHAIN_RPC_PORT}")"
jq -e '.result.specVersion | type == "number" and . > 0' <<<"\${runtime_response}" >/dev/null
EOF
	log "restored final-backup node verified release=${ETERRA_RELEASE_VERSION} manifest_sha256=${manifest_sha256}"
	exit 0
fi

if [[ -n "${fresh_reset_readiness}" && "${purge_state}" -ne 1 ]]; then
	die "--fresh-reset-readiness requires --purge-state"
fi
if [[ "${dry_run}" -eq 1 && "${purge_state}" -ne 1 ]]; then
	die "--dry-run is supported only for the guarded purge plan"
fi
if [[ -n "${fresh_reset_readiness}" ]]; then
	[[ "${ETERRA_RELEASE_VERSION}" != "dev" ]] ||
		die "--fresh-reset-readiness is valid only for a non-dev private-alpha release"
	is_truthy "${NEXUS_V2_LOCAL_ONLY_RELEASE}" ||
		die "guarded release reset requires NEXUS_V2_LOCAL_ONLY_RELEASE=1"
fi
if [[ "${ETERRA_RELEASE_VERSION}" != "dev" && "${purge_state}" -eq 1 && -z "${fresh_reset_readiness}" ]]; then
	die "release deploys preserve live state unless --purge-state is paired with --fresh-reset-readiness"
fi
if [[ "${ETERRA_RELEASE_VERSION}" != "dev" && -z "${promotion_manifest}" ]]; then
	die "release node deploys require --promote-candidate; remote builds are forbidden"
fi
if [[ -n "${promotion_manifest}" && -z "${target_identity}" ]]; then
	die "immutable node candidate promotion requires --target-identity"
fi
if [[ "${ETERRA_RELEASE_VERSION}" != "dev" && "${dry_run}" -eq 0 && -z "${evidence_output}" ]]; then
	die "release node candidate promotion requires --evidence"
fi
if [[ -n "${evidence_output}" && -z "${promotion_manifest}" ]]; then
	die "--evidence requires --promote-candidate"
fi
if [[ -n "${evidence_output}" && -e "${evidence_output}" ]]; then
	die "refusing to overwrite node promotion evidence: ${evidence_output}"
fi

CHAIN_SOURCE_COMMIT="$(require_release_source "${REPO_ROOT}" "alpha chain" "${ETERRA_EXPECTED_CHAIN_COMMIT}")"
export CHAIN_SOURCE_COMMIT

promote_candidate=0
candidate_summary=""
candidate_root=""
candidate_manifest_sha256=""
candidate_node_sha256=""
candidate_plain_spec_sha256=""
candidate_raw_spec_sha256=""
candidate_service_sha256=""
candidate_start_sha256=""
candidate_runtime_source_commit=""
candidate_genesis_hash=""
candidate_runtime_code_hash=""
target_identity_sha256=""
if [[ -n "${promotion_manifest}" ]]; then
	promote_candidate=1
	[[ "${NEXUS_V2_NODE_CANDIDATE_SHA256}" =~ ^[0-9a-f]{64}$ ]] ||
		die "immutable promotion requires NEXUS_V2_NODE_CANDIDATE_SHA256"
	[[ "${ETERRA_EXPECTED_RUNTIME_SOURCE_COMMIT}" =~ ^[0-9a-f]{40}$ ]] ||
		die "immutable promotion requires ETERRA_EXPECTED_RUNTIME_SOURCE_COMMIT"
	[[ "${NEXUS_V2_ALPHA_GENESIS_HASH}" =~ ^0x[0-9a-fA-F]{64}$ ]] ||
		die "immutable promotion requires NEXUS_V2_ALPHA_GENESIS_HASH"
	candidate_summary="$(${NODE_CANDIDATE_TOOL} verify \
		--candidate-manifest "${promotion_manifest}" \
		--expected-manifest-sha256 "${NEXUS_V2_NODE_CANDIDATE_SHA256}" \
		--expected-release-id "${ETERRA_RELEASE_VERSION}" \
		--expected-deployment-source-commit "${CHAIN_SOURCE_COMMIT}" \
		--expected-runtime-source-commit "${ETERRA_EXPECTED_RUNTIME_SOURCE_COMMIT}" \
		--expected-genesis-hash "${NEXUS_V2_ALPHA_GENESIS_HASH}" \
		--expected-runtime-code-hash "${RUNTIME_CODE_HASH}")" ||
		die "immutable node candidate verification failed"
	candidate_root="$(cd -- "$(dirname -- "${promotion_manifest}")" && pwd)"
	candidate_manifest_sha256="$(jq -er '.manifestSha256' <<<"${candidate_summary}")"
	candidate_node_sha256="$(jq -er '.nativeNodeSha256' <<<"${candidate_summary}")"
	candidate_plain_spec_sha256="$(jq -er '.plainSpecSha256' <<<"${candidate_summary}")"
	candidate_raw_spec_sha256="$(jq -er '.rawSpecSha256' <<<"${candidate_summary}")"
	candidate_service_sha256="$(jq -er '.serviceUnitSha256' <<<"${candidate_summary}")"
	candidate_start_sha256="$(jq -er '.startScriptSha256' <<<"${candidate_summary}")"
	candidate_runtime_source_commit="$(jq -er '.runtimeSourceCommit' <<<"${candidate_summary}")"
	candidate_genesis_hash="$(jq -er '.genesisHash' <<<"${candidate_summary}")"
	candidate_runtime_code_hash="$(jq -er '.runtimeCodeHash' <<<"${candidate_summary}")"
	[[ "${NEXUS_V2_TARGET_IDENTITY_SHA256}" =~ ^[0-9a-f]{64}$ ]] ||
		die "immutable promotion requires NEXUS_V2_TARGET_IDENTITY_SHA256"
	target_summary="$(${NODE_CANDIDATE_TOOL} verify-target-identity \
		--target-identity "${target_identity}" \
		--candidate-manifest "${promotion_manifest}" \
		--expected-sha256 "${NEXUS_V2_TARGET_IDENTITY_SHA256}")" ||
		die "spec-106 target identity verification failed"
	target_identity_sha256="$(jq -er '.sha256' <<<"${target_summary}")"
	NODE_RUNTIME_SOURCE_COMMIT="${candidate_runtime_source_commit}"
	NODE_ALPHA_GENESIS_HASH="${candidate_genesis_hash}"
	export NODE_RUNTIME_SOURCE_COMMIT NODE_ALPHA_GENESIS_HASH
fi

bundle_dir="$(make_temp_dir)"
remote_tmp_dir="${DEPLOY_ROOT}/tmp/node-deploy"
render_runtime_env_bundle "${bundle_dir}"
ensure_local_artifacts_dir
if [[ -n "${fresh_reset_readiness}" ]]; then
	stage_fresh_reset_readiness \
		"${fresh_reset_readiness}" \
		"${bundle_dir}/reset-readiness.json"
fi
if [[ "${ETERRA_RELEASE_VERSION}" != "dev" && "${purge_state}" -eq 1 ]]; then
	[[ -n "${FRESH_RESET_READINESS_SHA256:-}" ]] ||
		die "release purge requires a validated fresh-reset readiness packet"
fi
if [[ "${dry_run}" -eq 1 ]]; then
	log "dry-run: guarded node purge and immutable candidate promotion validated; no SSH or live mutation performed"
	log "dry-run: release=${ETERRA_RELEASE_VERSION} replacement_source=${CHAIN_SOURCE_COMMIT} runtime_source=${candidate_runtime_source_commit:-none} candidate_sha256=${candidate_manifest_sha256:-none} genesis=${candidate_genesis_hash:-none} readiness_sha256=${FRESH_RESET_READINESS_SHA256:-none}"
	exit 0
fi

if [[ "${promote_candidate}" -eq 1 ]]; then
	node_code_hash="${candidate_node_sha256}"
	node_spec_hash="${candidate_raw_spec_sha256}"
else
	node_code_hash="$(compute_node_code_hash)"
	node_spec_hash="$(compute_node_spec_hash)"
fi
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

if [[ "${promote_candidate}" -eq 1 ]]; then
	node_spec_needed=1
	node_build_action="promote-immutable-candidate"
	node_restart_reason="immutable-candidate"
elif [[ "${remote_node_bin_state}" != "1" ]] || [[ "${remote_node_code_hash}" != "${node_code_hash}" ]]; then
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

if [[ "${purge_state}" -eq 1 ]] && [[ "${node_restart_reason}" == "none" ]]; then
	node_restart_reason="purge-state"
fi

if [[ "${node_spec_pending_apply}" -eq 1 ]]; then
	log "alpha spec/genesis changes detected but deferred to preserve live chain state; rerun with --purge-state to apply them"
fi

remote_bash <<EOF
set -euo pipefail
mkdir -p "${remote_tmp_dir}" "${REMOTE_NODE_DIR}"
EOF
if [[ "${ETERRA_RELEASE_VERSION}" != "dev" && "${purge_state}" -eq 1 ]]; then
	rsync_to_remote_no_delete \
		"${FRESH_RESET_READINESS_STAGED_PATH}" \
		"${remote_tmp_dir}/reset-readiness.json"
	remote_root_bash <<EOF
set -euo pipefail
archive_root="${DEPLOY_ROOT}/archive/nexus-v2-fresh-reset/${FRESH_RESET_READINESS_SHA256}"
component_dir="\${archive_root}/node"
applied_marker="\${component_dir}/reset-applied.marker"
[[ ! -e "\${applied_marker}" ]] || {
	echo "[alpha-macmini2010] readiness packet was already consumed for the node reset" >&2
	exit 1
}
mkdir -p "\${component_dir}"
if [[ ! -e "\${component_dir}/deployment-identifiers.before" ]]; then
	install -m 0400 "${remote_tmp_dir}/reset-readiness.json" "\${component_dir}/reset-readiness.json"
	mkdir -p "\${component_dir}/shared-state.before"
	if [[ -d "${REMOTE_STATE_DIR}" ]]; then
		cp -a "${REMOTE_STATE_DIR}/." "\${component_dir}/shared-state.before/"
	fi
	{
		printf 'deploy_root=%s\n' "${DEPLOY_ROOT}"
		printf 'node_data_dir=%s\n' "${REMOTE_NODE_DATA_DIR}"
		printf 'node_service=%s\n' "${REMOTE_NODE_SERVICE_NAME}.service"
		printf 'node_dir=%s\n' "${REMOTE_NODE_DIR}"
		printf 'readiness_sha256=%s\n' "${FRESH_RESET_READINESS_SHA256}"
		printf 'readiness_release_id=%s\n' "${FRESH_RESET_RELEASE_ID}"
		printf 'frozen_chain_source_commit=%s\n' "${FRESH_RESET_SOURCE_COMMIT}"
		printf 'replacement_chain_source_commit=%s\n' "${CHAIN_SOURCE_COMMIT}"
		printf 'replacement_runtime_source_commit=%s\n' "${candidate_runtime_source_commit:-none}"
		printf 'node_candidate_manifest_sha256=%s\n' "${candidate_manifest_sha256:-none}"
		printf 'target_identity_sha256=%s\n' "${target_identity_sha256:-none}"
		printf 'replacement_genesis_hash=%s\n' "${candidate_genesis_hash:-none}"
		printf 'replacement_runtime_code_hash=%s\n' "${candidate_runtime_code_hash:-none}"
		printf 'frozen_block_number=%s\n' "${FRESH_RESET_GATE_BLOCK_NUMBER}"
		printf 'frozen_block_hash=%s\n' "${FRESH_RESET_GATE_BLOCK_HASH}"
	} >"\${component_dir}/deployment-identifiers.before"
	: >"\${component_dir}/file-sha256.before"
	for path in \
		"${REMOTE_NODE_ENV_FILE}" \
		"${REMOTE_NODE_BIN}" \
		"${REMOTE_NODE_SPEC}" \
		"${REMOTE_NODE_PLAIN_SPEC}" \
		"${REMOTE_NODE_SERVICE_UNIT_FILE}"
	do
		if [[ -f "\${path}" ]]; then
			shasum -a 256 "\${path}" >>"\${component_dir}/file-sha256.before"
		fi
	done
	systemctl show "${REMOTE_NODE_SERVICE_NAME}.service" \
		--property=Id,LoadState,ActiveState,SubState,FragmentPath \
		>"\${component_dir}/service-identity.before" 2>/dev/null || true
	chmod -R a-w "\${component_dir}"
	chmod u+w "\${component_dir}"
fi
EOF
fi

if [[ "${promote_candidate}" -eq 1 ]]; then
	log "staging exact immutable node candidate on ${SSH_TARGET}; no source tree or build context is transferred"
	rsync_to_remote "${candidate_root}/" "${remote_tmp_dir}/candidate/"
	rsync_to_remote_no_delete "${target_identity}" "${remote_tmp_dir}/target-identity.json"
else
	log "syncing alpha solochain working tree to ${SSH_TARGET}"
	rsync_with_remote \
		-az \
		--delete \
		-e "${RSYNC_RSH}" \
		--exclude '.git/' \
		--exclude '.worktrees/' \
		--exclude 'target/' \
		--exclude 'data/' \
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
fi

rsync_to_remote_no_delete "${bundle_dir}/node.env" "${remote_tmp_dir}/node.env"
if [[ "${promote_candidate}" -eq 0 ]]; then
	rsync_to_remote_no_delete "${ALPHA_OVERRIDES_FILE}" "${remote_tmp_dir}/alpha-overrides.json"
fi

log "determining alpha node build action on ${SSH_TARGET}"
remote_bash <<EOF
set -euo pipefail

echo "[alpha-macmini2010] node action: ${node_build_action}"

if [[ "${promote_candidate}" -eq 1 ]]; then
	candidate_dir="${remote_tmp_dir}/candidate"
	test "\$(shasum -a 256 "\${candidate_dir}/node-candidate.json" | awk '{print \$1}')" = "${candidate_manifest_sha256}"
	test "\$(shasum -a 256 "\${candidate_dir}/solochain-eterra-node" | awk '{print \$1}')" = "${candidate_node_sha256}"
	test "\$(shasum -a 256 "\${candidate_dir}/alpha-plain.json" | awk '{print \$1}')" = "${candidate_plain_spec_sha256}"
	test "\$(shasum -a 256 "\${candidate_dir}/alpha-raw.json" | awk '{print \$1}')" = "${candidate_raw_spec_sha256}"
	test "\$(shasum -a 256 "\${candidate_dir}/start-alpha-node.sh" | awk '{print \$1}')" = "${candidate_start_sha256}"
	test "\$(shasum -a 256 "\${candidate_dir}/eterra-alpha-node.service" | awk '{print \$1}')" = "${candidate_service_sha256}"
	test "\$(shasum -a 256 "${remote_tmp_dir}/target-identity.json" | awk '{print \$1}')" = "${target_identity_sha256}"
	install -m 0755 "\${candidate_dir}/solochain-eterra-node" "${REMOTE_NODE_BIN}"
	install -m 0644 "\${candidate_dir}/alpha-plain.json" "${REMOTE_NODE_PLAIN_SPEC}"
	install -m 0644 "\${candidate_dir}/alpha-raw.json" "${REMOTE_NODE_SPEC}"
	mkdir -p "${remote_tmp_dir}/finalized-alpha"
	install -m 0644 "\${candidate_dir}/alpha-plain.json" "${remote_tmp_dir}/finalized-alpha/alpha-plain.json"
	install -m 0644 "\${candidate_dir}/alpha-raw.json" "${remote_tmp_dir}/finalized-alpha/alpha-raw.json"
else
	source "${REMOTE_CARGO_ENV_FILE}"
	cd "${REMOTE_NODE_DIR}"
	if [[ "${node_build_needed}" -eq 1 ]]; then
		export CARGO_TARGET_DIR="${REMOTE_CARGO_TARGET_DIR}"
		export SCCACHE_DIR="${REMOTE_SCCACHE_DIR}"
		if [[ "${REMOTE_CARGO_INCREMENTAL}" == "1" ]]; then
			export CARGO_PROFILE_RELEASE_INCREMENTAL=true
		fi
		if [[ "${ENABLE_REMOTE_SCCACHE}" == "1" ]] && command -v sccache >/dev/null 2>&1; then
			export RUSTC_WRAPPER="\$(command -v sccache)"
		fi
		CARGO_TERM_COLOR=never cargo build --locked -p solochain-eterra-node --release -j "${REMOTE_CARGO_JOBS}"
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
fi
EOF

if [[ "${node_spec_needed}" -eq 1 ]]; then
	rsync_from_remote_no_delete "${remote_tmp_dir}/finalized-alpha/" "${LOCAL_FINALIZED_ALPHA_DIR}/"
fi

remote_candidate_start="${REMOTE_NODE_DIR}/deploy/alpha/macmini2010/start-alpha-node.sh"
remote_candidate_service="${REMOTE_NODE_DIR}/deploy/alpha/macmini2010/eterra-alpha-node.service"
if [[ "${promote_candidate}" -eq 1 ]]; then
	remote_candidate_start="${remote_tmp_dir}/candidate/start-alpha-node.sh"
	remote_candidate_service="${remote_tmp_dir}/candidate/eterra-alpha-node.service"
fi

remote_root_bash <<EOF
set -euo pipefail

mkdir -p "${REMOTE_SHARED_ENV_DIR}" "${REMOTE_NODE_DATA_DIR}" "${REMOTE_STATE_DIR}"
install -m 0644 "${remote_tmp_dir}/node.env" "${REMOTE_NODE_ENV_FILE}"
chown root:root "${REMOTE_NODE_ENV_FILE}"
install -m 0755 "${remote_candidate_start}" "${REMOTE_START_SCRIPT}"
install -m 0644 "${remote_candidate_service}" "${REMOTE_NODE_SERVICE_UNIT_FILE}"
chown root:root "${REMOTE_START_SCRIPT}" "${REMOTE_NODE_SERVICE_UNIT_FILE}"
systemctl disable --now "${LEGACY_NODE_SERVICE_NAME}.service" >/dev/null 2>&1 || true

systemctl daemon-reload
systemctl enable "${REMOTE_NODE_SERVICE_NAME}.service"

if [[ "${purge_state}" -eq 1 ]]; then
	echo "[alpha-macmini2010] node action: purge-state"
	if [[ "${ETERRA_RELEASE_VERSION}" != "dev" ]]; then
		archive_component_dir="${DEPLOY_ROOT}/archive/nexus-v2-fresh-reset/${FRESH_RESET_READINESS_SHA256:-}/node"
		[[ ! -e "\${archive_component_dir}/reset-applied.marker" ]] || {
			echo "[alpha-macmini2010] readiness packet was already consumed for the node reset" >&2
			exit 1
		}
	fi
	systemctl stop "${REMOTE_NODE_SERVICE_NAME}.service" >/dev/null 2>&1 || true
	rm -rf "${REMOTE_NODE_DATA_DIR}"
	mkdir -p "${REMOTE_NODE_DATA_DIR}"
	chown -R "${DEPLOY_USER}:${DEPLOY_USER}" "${REMOTE_NODE_DATA_DIR}"
	if [[ "${ETERRA_RELEASE_VERSION}" != "dev" ]]; then
		printf 'component=node\nreset_applied_at_utc=%s\nreplacement_source_commit=%s\nruntime_source_commit=%s\nnode_candidate_sha256=%s\ntarget_identity_sha256=%s\nalpha_genesis_hash=%s\n' \
			"\$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
			"${CHAIN_SOURCE_COMMIT}" \
			"${candidate_runtime_source_commit:-none}" \
			"${candidate_manifest_sha256:-none}" \
			"${target_identity_sha256:-none}" \
			"${candidate_genesis_hash:-none}" \
			>"\${archive_component_dir}/reset-applied.marker"
		chmod 0440 "\${archive_component_dir}/reset-applied.marker"
	fi
fi

if [[ "${node_restart_reason}" == "none" ]]; then
	echo "[alpha-macmini2010] node action: service already up to date"
else
	echo "[alpha-macmini2010] node action: restart (${node_restart_reason})"
	systemctl restart "${REMOTE_NODE_SERVICE_NAME}.service"
fi

systemctl is-active --quiet "${REMOTE_NODE_SERVICE_NAME}.service"
if [[ "${promote_candidate}" -eq 1 ]]; then
	genesis_response="\$(curl -fsS --max-time 15 -H 'Content-Type: application/json' \
		-d '{"id":1,"jsonrpc":"2.0","method":"chain_getBlockHash","params":[0]}' \
		'http://127.0.0.1:${CHAIN_RPC_PORT}')"
	test "\$(jq -r '.result' <<<"\${genesis_response}")" = "${candidate_genesis_hash}"
	code_hash_response="\$(curl -fsS --max-time 15 -H 'Content-Type: application/json' \
		-d '{"id":1,"jsonrpc":"2.0","method":"state_getStorageHash","params":["0x3a636f6465"]}' \
		'http://127.0.0.1:${CHAIN_RPC_PORT}')"
	test "\$(jq -r '.result' <<<"\${code_hash_response}")" = "${candidate_runtime_code_hash}"
	runtime_response="\$(curl -fsS --max-time 15 -H 'Content-Type: application/json' \
		-d '{"id":1,"jsonrpc":"2.0","method":"state_getRuntimeVersion","params":[]}' \
		'http://127.0.0.1:${CHAIN_RPC_PORT}')"
	test "\$(jq -r '.result.specVersion' <<<"\${runtime_response}")" = "${RUNTIME_SPEC_VERSION}"
fi
printf '%s\n' "${node_code_hash}" >"${REMOTE_NODE_CODE_HASH_FILE}"
printf '%s\n' "${node_runtime_hash}" >"${REMOTE_NODE_RUNTIME_HASH_FILE}"
printf '%s\n' "${ETERRA_RELEASE_VERSION}" >"${REMOTE_RELEASE_VERSION_FILE}"
printf '%s\n' "${CHAIN_SOURCE_COMMIT}" >"${REMOTE_CHAIN_SOURCE_COMMIT_FILE}"
if [[ "${promote_candidate}" -eq 1 ]]; then
	printf '%s\n' "${candidate_runtime_source_commit}" >"${REMOTE_RUNTIME_SOURCE_COMMIT_FILE}"
	printf '%s\n' "${candidate_manifest_sha256}" >"${REMOTE_NODE_CANDIDATE_SHA256_FILE}"
	printf '%s\n' "${candidate_genesis_hash}" >"${REMOTE_ALPHA_GENESIS_HASH_FILE}"
	install -m 0440 "${remote_tmp_dir}/target-identity.json" "${REMOTE_TARGET_IDENTITY_FILE}"
fi
if [[ "${node_spec_needed}" -eq 1 ]]; then
	printf '%s\n' "${node_spec_hash}" >"${REMOTE_NODE_SPEC_HASH_FILE}"
	chown root:root "${REMOTE_NODE_SPEC_HASH_FILE}"
fi
chown root:root "${REMOTE_NODE_CODE_HASH_FILE}" "${REMOTE_NODE_RUNTIME_HASH_FILE}" \
	"${REMOTE_RELEASE_VERSION_FILE}" "${REMOTE_CHAIN_SOURCE_COMMIT_FILE}"
if [[ "${promote_candidate}" -eq 1 ]]; then
	chown root:root "${REMOTE_RUNTIME_SOURCE_COMMIT_FILE}" "${REMOTE_NODE_CANDIDATE_SHA256_FILE}" \
		"${REMOTE_ALPHA_GENESIS_HASH_FILE}" "${REMOTE_TARGET_IDENTITY_FILE}"
fi
systemctl --no-pager --full status "${REMOTE_NODE_SERVICE_NAME}.service" || true
rm -rf "${remote_tmp_dir}"
EOF

if [[ "${REMOTE_CARGO_CLEAN_AFTER_DEPLOY}" == "1" && "${promote_candidate}" -eq 0 ]]; then
	log "removing remote Cargo build cache after successful node installation"
	remote_bash <<EOF
set -euo pipefail
target_dir="${REMOTE_CARGO_TARGET_DIR}"
deploy_root="${DEPLOY_ROOT}"
if [[ "\${target_dir}" != "\${deploy_root}/"* ]] || [[ "\${target_dir}" == "\${deploy_root}" ]] || [[ "\${target_dir}" == "/" ]]; then
	echo "[alpha-macmini2010] refusing unsafe Cargo cleanup path: \${target_dir}" >&2
	exit 1
fi
rm -rf -- "\${target_dir}"
echo "[alpha-macmini2010] removed Cargo build cache: \${target_dir}"
EOF
fi

if [[ -n "${evidence_output}" ]]; then
	mkdir -p "$(dirname -- "${evidence_output}")"
	python3 - \
		"${evidence_output}" \
		"${candidate_summary}" \
		"${CHAIN_SOURCE_COMMIT}" \
		"${node_runtime_hash}" \
		"${FRESH_RESET_READINESS_SHA256:-}" \
		"${target_identity_sha256}" <<'PY'
import datetime
import json
import os
import pathlib
import sys

output, summary_raw, deployment_commit, runtime_env_sha256, readiness_sha256, target_identity_sha256 = sys.argv[1:]
summary = json.loads(summary_raw)
value = {
    "schemaVersion": 1,
    "kind": "nexus-v2-private-alpha-node-promotion-evidence",
    "releaseId": summary["releaseId"],
    "deploymentSourceCommit": deployment_commit,
    "runtimeSourceCommit": summary["runtimeSourceCommit"],
    "nodeCandidateManifestSha256": summary["manifestSha256"],
    "targetIdentitySha256": target_identity_sha256,
    "nativeNodeSha256": summary["nativeNodeSha256"],
    "plainSpecSha256": summary["plainSpecSha256"],
    "rawSpecSha256": summary["rawSpecSha256"],
    "alphaGenesisHash": summary["genesisHash"],
    "runtimeCodeHash": summary["runtimeCodeHash"],
    "runtimeEnvironmentSha256": runtime_env_sha256,
    "freshResetReadinessSha256": readiness_sha256 or None,
    "remoteBuildPerformed": False,
    "remoteSpecFinalizationPerformed": False,
    "candidateBytesVerifiedBeforeAndAfterTransfer": True,
    "paidOrPublicActivationAllowed": False,
    "promotedAtUtc": datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
}
path = pathlib.Path(output)
payload = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()
descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
with os.fdopen(descriptor, "wb") as handle:
    handle.write(payload)
PY
fi

log "alpha node deploy complete release=${ETERRA_RELEASE_VERSION} deployment_source=${CHAIN_SOURCE_COMMIT} runtime_source=${candidate_runtime_source_commit:-${CHAIN_SOURCE_COMMIT}} code_sha256=${node_code_hash} spec_sha256=${node_spec_hash} genesis=${candidate_genesis_hash:-preserved} runtime_env_sha256=${node_runtime_hash}"
