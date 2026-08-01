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

confirmation="PRIVATE_ALPHA_ROLLBACK_ONLY"
component_id="chain-media"
context_path=""
result_path=""

while [[ $# -gt 0 ]]; do
	case "$1" in
		--context)
			[[ $# -ge 2 ]] || die "--context requires a path"
			context_path="$2"
			shift
			;;
		--result)
			[[ $# -ge 2 ]] || die "--result requires a path"
			result_path="$2"
			shift
			;;
		--help|-h)
			cat <<'EOF'
Usage: nexus-v2-rollback-protected-host-action.sh \
  --context CONTEXT.json --result RESULT.json

This helper is called only by the hash-pinned Nexus V2 rollback component
driver. It loads the existing protected Alpha deployment environment. Dry-run
performs read-only credential/archive preflight. Execute supports only the five
closed coordinator actions and writes an immutable remote idempotency marker.
EOF
			exit 0
			;;
		*)
			die "unknown argument: $1"
			;;
	esac
	shift
done

[[ "${NEXUS_V2_ROLLBACK_PROTECTED_CONFIRMATION:-}" == "${confirmation}" ]] ||
	die "protected rollback confirmation is absent"
[[ -n "${context_path}" && -n "${result_path}" ]] ||
	die "--context and --result are required"
[[ -f "${context_path}" && ! -L "${context_path}" ]] ||
	die "protected action context is unavailable"
[[ ! -e "${result_path}" && ! -L "${result_path}" ]] ||
	die "refusing to overwrite protected action result"

require_cmd curl
require_cmd jq
require_cmd rsync
require_cmd shasum
require_cmd ssh
require_cmd tar

jq -e '
	keys == [
		"acceptanceAssetsExist",
		"acceptanceInventorySha256",
		"action",
		"componentId",
		"componentSourceCommits",
		"economicGatesSha256",
		"finalBackupManifestSha256",
		"kind",
		"mode",
		"operationId",
		"planSha256",
		"postCutoverObservationSha256",
		"releaseId",
		"requiredResetArchives",
		"resetReadinessPath",
		"restoreEvidenceSha256",
		"schemaVersion",
		"scripts",
		"sourceCommit",
		"stagingPath"
	] and
	.schemaVersion == 1 and
	.kind == "nexus-v2-private-alpha-protected-host-action-context" and
	.componentId == "chain-media" and
	(.componentSourceCommits | keys == ["chain", "media"]) and
	(.requiredResetArchives | keys == ["media", "node"]) and
	(.scripts | keys == ["deployMedia", "deployNode", "restoreState", "status"])
' "${context_path}" >/dev/null ||
	die "protected action context does not match the closed schema"

action="$(jq -er '.action' "${context_path}")"
mode="$(jq -er '.mode' "${context_path}")"
operation_id="$(jq -er '.operationId' "${context_path}")"
plan_sha256="$(jq -er '.planSha256' "${context_path}")"
release_id="$(jq -er '.releaseId' "${context_path}")"
source_commit="$(jq -er '.sourceCommit' "${context_path}")"
chain_commit="$(jq -er '.componentSourceCommits.chain' "${context_path}")"
media_commit="$(jq -er '.componentSourceCommits.media' "${context_path}")"
manifest_sha256="$(jq -er '.finalBackupManifestSha256' "${context_path}")"
restore_evidence_sha256="$(jq -er '.restoreEvidenceSha256' "${context_path}")"
observation_sha256="$(jq -er '.postCutoverObservationSha256' "${context_path}")"
economic_gates_sha256="$(jq -er '.economicGatesSha256' "${context_path}")"
inventory_sha256="$(jq -er '.acceptanceInventorySha256' "${context_path}")"
acceptance_assets_exist="$(jq -r '.acceptanceAssetsExist' "${context_path}")"
staging_path="$(jq -r '.stagingPath // ""' "${context_path}")"
reset_readiness_path="$(jq -er '.resetReadinessPath' "${context_path}")"
node_reset_archive="$(jq -er '.requiredResetArchives.node' "${context_path}")"
media_reset_archive="$(jq -er '.requiredResetArchives.media' "${context_path}")"

[[ "${action}" =~ ^(prepare-reset-archives|post-cutover-smoke|pause-v2-writes|archive-failed-v2|restore-final-backup|restored-smoke)$ ]] ||
	die "unsupported protected action"
[[ "${mode}" =~ ^(prepare|dry-run|execute)$ ]] || die "unsupported protected mode"
if [[ "${mode}" == "prepare" ]]; then
	[[ "${action}" == "prepare-reset-archives" ]] ||
		die "prepare mode supports only reset archive preparation"
else
	[[ "${action}" != "prepare-reset-archives" ]] ||
		die "reset archive preparation requires prepare mode"
fi
[[ "${operation_id}" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$ ]] ||
	die "invalid protected operation ID"
[[ "${release_id}" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$ ]] ||
	die "invalid protected release ID"
for value in "${plan_sha256}" "${manifest_sha256}" "${restore_evidence_sha256}" \
	"${observation_sha256}" "${economic_gates_sha256}" "${inventory_sha256}"
do
	[[ "${value}" =~ ^[0-9a-f]{64}$ ]] || die "protected SHA-256 identity is invalid"
done
for value in "${source_commit}" "${chain_commit}" "${media_commit}"; do
	[[ "${value}" =~ ^[0-9a-f]{40}$ ]] || die "protected source identity is invalid"
done
[[ "${source_commit}" == "${chain_commit}" ]] ||
	die "protected chain source identity mismatch"
[[ "${acceptance_assets_exist}" == "true" || "${acceptance_assets_exist}" == "false" ]] ||
	die "protected acceptance boundary is invalid"
if [[ "${acceptance_assets_exist}" == "true" &&
	"${action}" =~ ^(archive-failed-v2|restore-final-backup|restored-smoke)$ ]]; then
	die "post-acceptance recovery is pause-and-forward-fix only"
fi

pinned_restore_state=""
pinned_deploy_node=""
pinned_deploy_media=""
pinned_status=""
for role in restoreState deployNode deployMedia status; do
	pinned_script="$(jq -er --arg role "${role}" '.scripts[$role]' "${context_path}")"
	case "${role}" in
		restoreState)
			expected_script="${SCRIPT_DIR}/restore-alpha-state.sh"
			pinned_restore_state="${pinned_script}"
			;;
		deployNode)
			expected_script="${SCRIPT_DIR}/deploy-node.sh"
			pinned_deploy_node="${pinned_script}"
			;;
		deployMedia)
			expected_script="${SCRIPT_DIR}/deploy-media.sh"
			pinned_deploy_media="${pinned_script}"
			;;
		status)
			expected_script="${SCRIPT_DIR}/status.sh"
			pinned_status="${pinned_script}"
			;;
	esac
	pinned_parent="$(cd -- "$(dirname -- "${pinned_script}")" 2>/dev/null && pwd)" ||
		die "protected ${role} parent is unavailable"
	pinned_absolute="${pinned_parent}/$(basename -- "${pinned_script}")"
	[[ "${pinned_absolute}" == "${expected_script}" ]] ||
		die "protected ${role} path is not the existing deployment script"
	[[ -f "${pinned_script}" && -x "${pinned_script}" && ! -L "${pinned_script}" ]] ||
		die "protected ${role} script is unavailable"
done

[[ "${node_reset_archive}" =~ ^/opt/eterra-alpha/archive/nexus-v2-fresh-reset/([0-9a-f]{64})/node$ ]] ||
	die "protected node reset archive path is not exact"
readiness_sha256="${BASH_REMATCH[1]}"
[[ "${media_reset_archive}" == "/opt/eterra-alpha/archive/nexus-v2-fresh-reset/${readiness_sha256}/media" ]] ||
	die "protected media reset archive path is not paired with the node archive"
[[ -f "${reset_readiness_path}" && ! -L "${reset_readiness_path}" ]] ||
	die "protected reset-readiness packet is unavailable"
[[ "$(shasum -a 256 "${reset_readiness_path}" | awk '{print $1}')" == "${readiness_sha256}" ]] ||
	die "protected reset-readiness packet hash mismatch"

if [[ "${action}" == "restore-final-backup" ]]; then
	[[ -n "${staging_path}" && -d "${staging_path}" && ! -L "${staging_path}" ]] ||
		die "protected restore staging is unavailable"
	[[ -f "${staging_path}/staging-contract.json" ]] ||
		die "protected restore staging contract is absent"
	[[ "$(jq -r '.backupManifestSha256' "${staging_path}/staging-contract.json")" == "${manifest_sha256}" ]] ||
		die "protected restore staging manifest identity mismatch"
	[[ "$(jq -r '.releaseId' "${staging_path}/staging-contract.json")" == "${release_id}" ]] ||
		die "protected restore staging release mismatch"
	[[ "$(jq -r '.sourceCommit' "${staging_path}/staging-contract.json")" == "${source_commit}" ]] ||
		die "protected restore staging source mismatch"
else
	[[ -z "${staging_path}" ]] ||
		die "protected non-restore action unexpectedly received restore staging"
fi

load_env

[[ "${DEPLOY_ROOT}" == "/opt/eterra-alpha" ]] ||
	die "protected rollback requires DEPLOY_ROOT=/opt/eterra-alpha"
[[ "${REMOTE_NODE_DATA_DIR}" == "/var/lib/eterra-alpha-node" ]] ||
	die "protected rollback requires REMOTE_NODE_DATA_DIR=/var/lib/eterra-alpha-node"
[[ "${CHAIN_RPC_PORT}" == "9944" && "${CHAIN_P2P_PORT}" == "30333" ]] ||
	die "protected rollback requires chain ports 9944/30333"
[[ "${MEDIA_PORT}" == "4000" && "${AUTHORITY_PORT}" == "8787" ]] ||
	die "protected rollback requires media/authority ports 4000/8787"
[[ "${IPFS_API_PORT}" == "5001" && "${IPFS_GATEWAY_PORT}" == "8080" ]] ||
	die "protected rollback requires IPFS ports 5001/8080"
[[ "${ETERRA_RELEASE_VERSION}" == "${release_id}" ]] ||
	die "protected deployment release does not match the coordinator"
[[ "${ETERRA_EXPECTED_CHAIN_COMMIT}" == "${chain_commit}" ]] ||
	die "protected expected chain commit does not match the coordinator"
[[ "${ETERRA_EXPECTED_MEDIA_COMMIT}" == "${media_commit}" ]] ||
	die "protected expected media commit does not match the coordinator"

work_dir="$(make_temp_dir)"
if [[ "${mode}" == "prepare" ]]; then
	remote_readiness="${REMOTE_SCRIPT_DIR}/nexus-v2-${operation_id}-reset-readiness.json"
	remote_root_bash <<EOF
set -euo pipefail
mkdir -p "${REMOTE_SCRIPT_DIR}"
rm -f "${remote_readiness}"
EOF
	rsync_to_remote_no_delete "${reset_readiness_path}" "${remote_readiness}"
	preparation_state="$(
		remote_root_bash <<EOF
set -euo pipefail
test "\$(shasum -a 256 "${remote_readiness}" | awk '{print \$1}')" = "${readiness_sha256}"
node_dir="${node_reset_archive}"
media_dir="${media_reset_archive}"
for archive_dir in "\${node_dir}" "\${media_dir}"; do
	if [[ -e "\${archive_dir}" && ! -f "\${archive_dir}/prepared.marker.json" ]]; then
		echo "partial reset archive requires manual review" >&2
		exit 1
	fi
	test ! -e "\${archive_dir}/reset-applied.marker"
done
if [[ -f "\${node_dir}/prepared.marker.json" && -f "\${media_dir}/prepared.marker.json" ]]; then
	for pair in "node:\${node_dir}" "media:\${media_dir}"; do
		component="\${pair%%:*}"
		archive_dir="\${pair#*:}"
		test -z "\$(find "\${archive_dir}" -perm /222 -print -quit)"
		(cd "\${archive_dir}" && shasum -a 256 -c file-sha256.prepared >/dev/null)
		jq -e \
			--arg component "\${component}" \
			--arg operationId "${operation_id}" \
			--arg planSha256 "${plan_sha256}" \
			--arg releaseId "${release_id}" \
			--arg sourceCommit "${source_commit}" \
			--arg mediaSourceCommit "${media_commit}" \
			--arg readinessSha256 "${readiness_sha256}" \
			'.schemaVersion == 1 and
			 .kind == "nexus-v2-private-alpha-prepared-reset-archive" and
			 .componentId == \$component and
			 .operationId == \$operationId and
			 .planSha256 == \$planSha256 and
			 .releaseId == \$releaseId and
			 .sourceCommit == \$sourceCommit and
			 .mediaSourceCommit == \$mediaSourceCommit and
			 .resetReadinessSha256 == \$readinessSha256 and
			 .currentAlphaStatePreserved == true and
			 .resetApplied == false' \
			"\${archive_dir}/prepared.marker.json" >/dev/null
	done
	rm -f "${remote_readiness}"
	printf 'reused'
	exit 0
fi
[[ ! -e "\${node_dir}" && ! -e "\${media_dir}" ]] || {
	echo "paired reset archives must be wholly absent or wholly prepared" >&2
	exit 1
}
before_state="\$(
	{
		systemctl show "${REMOTE_NODE_SERVICE_NAME}.service" "${AUTHORITY_SERVICE_NAME}.service" \
			--property=Id,LoadState,ActiveState,SubState 2>/dev/null || true
		${REMOTE_DOCKER_COMPOSE_CMD} ps --format json 2>/dev/null || true
	} | shasum -a 256 | awk '{print \$1}'
)"
for archive_dir in "\${node_dir}" "\${media_dir}"; do
	mkdir -p "\${archive_dir}/shared-state.before"
	install -m 0400 "${remote_readiness}" "\${archive_dir}/reset-readiness.json"
	if [[ -d "${REMOTE_STATE_DIR}" ]]; then
		cp -a "${REMOTE_STATE_DIR}/." "\${archive_dir}/shared-state.before/"
	fi
done
{
	printf 'deploy_root=%s\n' "${DEPLOY_ROOT}"
	printf 'node_data_dir=%s\n' "${REMOTE_NODE_DATA_DIR}"
	printf 'node_service=%s\n' "${REMOTE_NODE_SERVICE_NAME}.service"
	printf 'readiness_sha256=%s\n' "${readiness_sha256}"
	printf 'release_id=%s\n' "${release_id}"
	printf 'source_commit=%s\n' "${source_commit}"
	printf 'media_source_commit=%s\n' "${media_commit}"
} >"\${node_dir}/deployment-identifiers.before"
{
	printf 'deploy_root=%s\n' "${DEPLOY_ROOT}"
	printf 'compose_project=%s\n' "${REMOTE_MEDIA_PROJECT_NAME}"
	printf 'ipfs_data_volume=%s\n' "${REMOTE_IPFS_DATA_VOLUME}"
	printf 'ipfs_staging_volume=%s\n' "${REMOTE_IPFS_STAGING_VOLUME}"
	printf 'readiness_sha256=%s\n' "${readiness_sha256}"
	printf 'release_id=%s\n' "${release_id}"
	printf 'source_commit=%s\n' "${source_commit}"
	printf 'media_source_commit=%s\n' "${media_commit}"
} >"\${media_dir}/deployment-identifiers.before"
for path in "${REMOTE_NODE_ENV_FILE}" "${REMOTE_NODE_BIN}" "${REMOTE_NODE_SPEC}" \
	"${REMOTE_NODE_PLAIN_SPEC}" "${REMOTE_NODE_SERVICE_UNIT_FILE}"; do
	[[ ! -f "\${path}" ]] || shasum -a 256 "\${path}" >>"\${node_dir}/file-sha256.before"
done
for path in "${REMOTE_MEDIA_ENV_FILE}" "${REMOTE_MEDIA_COMPOSE_BASE}" \
	"${REMOTE_MEDIA_COMPOSE_OVERRIDE}" "${REMOTE_MEDIA_COMPOSE_PHASE1}"; do
	[[ ! -f "\${path}" ]] || shasum -a 256 "\${path}" >>"\${media_dir}/file-sha256.before"
done
systemctl show "${REMOTE_NODE_SERVICE_NAME}.service" \
	--property=Id,LoadState,ActiveState,SubState,FragmentPath \
	>"\${node_dir}/service-identity.before" 2>/dev/null || true
docker volume inspect "${REMOTE_IPFS_DATA_VOLUME}" "${REMOTE_IPFS_STAGING_VOLUME}" \
	>"\${media_dir}/volume-identities.before.json"
${REMOTE_DOCKER_COMPOSE_CMD} ps --format json \
	>"\${media_dir}/compose-services.before.json" 2>/dev/null || true
for pair in "node:\${node_dir}" "media:\${media_dir}"; do
	component="\${pair%%:*}"
	archive_dir="\${pair#*:}"
	(
		cd "\${archive_dir}"
		find . -type f ! -name prepared.marker.json ! -name file-sha256.prepared \
			-print0 | LC_ALL=C sort -z | xargs -0 shasum -a 256
	) >"\${archive_dir}/file-sha256.prepared"
	jq -cn \
		--arg componentId "\${component}" \
		--arg operationId "${operation_id}" \
		--arg planSha256 "${plan_sha256}" \
		--arg releaseId "${release_id}" \
		--arg sourceCommit "${source_commit}" \
		--arg mediaSourceCommit "${media_commit}" \
		--arg resetReadinessSha256 "${readiness_sha256}" \
		--arg preparedAtUtc "\$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
		'{schemaVersion:1,kind:"nexus-v2-private-alpha-prepared-reset-archive",
		  componentId:\$componentId,operationId:\$operationId,planSha256:\$planSha256,
		  releaseId:\$releaseId,sourceCommit:\$sourceCommit,
		  mediaSourceCommit:\$mediaSourceCommit,
		  resetReadinessSha256:\$resetReadinessSha256,
		  currentAlphaStatePreserved:true,resetApplied:false,
		  preparedAtUtc:\$preparedAtUtc}' >"\${archive_dir}/prepared.marker.json"
	chmod -R a-w "\${archive_dir}"
	test -z "\$(find "\${archive_dir}" -perm /222 -print -quit)"
	(cd "\${archive_dir}" && shasum -a 256 -c file-sha256.prepared >/dev/null)
done
after_state="\$(
	{
		systemctl show "${REMOTE_NODE_SERVICE_NAME}.service" "${AUTHORITY_SERVICE_NAME}.service" \
			--property=Id,LoadState,ActiveState,SubState 2>/dev/null || true
		${REMOTE_DOCKER_COMPOSE_CMD} ps --format json 2>/dev/null || true
	} | shasum -a 256 | awk '{print \$1}'
)"
test "\${before_state}" = "\${after_state}"
rm -f "${remote_readiness}"
printf 'created'
EOF
	)"
	[[ "${preparation_state}" == "created" || "${preparation_state}" == "reused" ]] ||
		die "protected reset archive preparation failed"
fi

archive_preflight="$(
	remote_root_bash <<EOF
set -euo pipefail
for archive_dir in "${node_reset_archive}" "${media_reset_archive}"; do
	test -d "\${archive_dir}"
	test -f "\${archive_dir}/reset-readiness.json"
	test -f "\${archive_dir}/deployment-identifiers.before"
	test -f "\${archive_dir}/prepared.marker.json"
	test -f "\${archive_dir}/file-sha256.prepared"
	(cd "\${archive_dir}" && shasum -a 256 -c file-sha256.prepared >/dev/null)
done
test "\$(shasum -a 256 "${node_reset_archive}/reset-readiness.json" | awk '{print \$1}')" = "${readiness_sha256}"
test "\$(shasum -a 256 "${media_reset_archive}/reset-readiness.json" | awk '{print \$1}')" = "${readiness_sha256}"
if [[ "${action}" == "post-cutover-smoke" && "${mode}" == "execute" ]]; then
	test -f "${node_reset_archive}/reset-applied.marker"
	test -f "${media_reset_archive}/reset-applied.marker"
fi
printf 'ready'
EOF
)"
[[ "${archive_preflight}" == "ready" ]] ||
	die "protected reset archive preflight failed"

marker_dir="${DEPLOY_ROOT}/shared/rollback/nexus-v2-post-cutover/${operation_id}/${component_id}/actions"
marker_path="${marker_dir}/${action}.json"

dry_checks() {
	case "$1" in
		post-cutover-smoke)
			jq -cn '{
				sourceIdentityPinned:true,
				credentialsResolvable:true,
				requiredResetArchivesPresent:true,
				smokeProbePlanned:true
			}'
			;;
		pause-v2-writes)
			jq -cn '{
				sourceIdentityPinned:true,
				credentialsResolvable:true,
				requiredResetArchivesPresent:true,
				pausePlanSafe:true,
				restoreExcluded:true
			}'
			;;
		archive-failed-v2)
			jq -cn '{
				sourceIdentityPinned:true,
				credentialsResolvable:true,
				requiredResetArchivesPresent:true,
				archivePlanSafe:true,
				restoreExcluded:true
			}'
			;;
		restore-final-backup)
			jq -cn '{
				sourceIdentityPinned:true,
				credentialsResolvable:true,
				requiredResetArchivesPresent:true,
				finalBackupInputsMatched:true,
				failedV2ArchiveRequired:true,
				existingRestoreScriptPinned:true,
				existingDeployScriptsPinned:true,
				restorePlanSafe:true
			}'
			;;
		restored-smoke)
			jq -cn '{
				sourceIdentityPinned:true,
				credentialsResolvable:true,
				requiredResetArchivesPresent:true,
				restoredSmokeProbePlanned:true
			}'
			;;
	esac
}

write_result() {
	local remote_actions="$1"
	local already_applied="$2"
	local failed_hash="$3"
	local marker_hash="$4"
	local checks_file="$5"
	local completed_at="$6"
	local failed_json="null"
	local marker_json="null"

	[[ -z "${failed_hash}" ]] || failed_json="\"${failed_hash}\""
	[[ -z "${marker_hash}" ]] || marker_json="\"${marker_hash}\""
	umask 077
	jq -cn \
		--slurpfile context "${context_path}" \
		--slurpfile checks "${checks_file}" \
		--arg operationId "${operation_id}" \
		--arg planSha256 "${plan_sha256}" \
		--arg releaseId "${release_id}" \
		--arg sourceCommit "${source_commit}" \
		--arg componentId "${component_id}" \
		--arg action "${action}" \
		--arg mode "${mode}" \
		--arg completedAtUtc "${completed_at}" \
		--argjson remoteActionsExecuted "${remote_actions}" \
		--argjson alreadyApplied "${already_applied}" \
		--argjson failedV2RootArchiveSha256 "${failed_json}" \
		--argjson remoteIdempotencyMarkerSha256 "${marker_json}" \
		'{
			schemaVersion:1,
			kind:"nexus-v2-private-alpha-component-action-result",
			operationId:$operationId,
			planSha256:$planSha256,
			releaseId:$releaseId,
			sourceCommit:$sourceCommit,
			componentSourceCommits:$context[0].componentSourceCommits,
			componentId:$componentId,
			action:$action,
			mode:$mode,
			result:"passed",
			remoteActionsExecuted:$remoteActionsExecuted,
			alreadyApplied:$alreadyApplied,
			requiredResetArchives:{media:true,node:true},
			failedV2RootArchiveSha256:$failedV2RootArchiveSha256,
			remoteIdempotencyMarkerSha256:$remoteIdempotencyMarkerSha256,
			checks:$checks[0],
			completedAtUtc:$completedAtUtc
		}' >"${result_path}"
	chmod 0440 "${result_path}"
}

if [[ "${mode}" == "dry-run" ]]; then
	checks_path="${work_dir}/checks.json"
	dry_checks "${action}" >"${checks_path}"
	write_result false false "" "" "${checks_path}" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
	exit 0
fi

read_marker() {
	local output="$1"
	local marker_text

	marker_text="$(
		remote_root_bash <<EOF
set -euo pipefail
test -f "${marker_path}"
cat "${marker_path}"
EOF
	)"
	printf '%s\n' "${marker_text}" >"${output}"
	jq -e \
		--arg operationId "${operation_id}" \
		--arg planSha256 "${plan_sha256}" \
		--arg releaseId "${release_id}" \
		--arg sourceCommit "${source_commit}" \
		--arg componentId "${component_id}" \
		--arg action "${action}" \
		--arg manifestSha256 "${manifest_sha256}" \
		'
		keys == [
			"action",
			"checks",
			"completedAtUtc",
			"componentId",
			"componentSourceCommits",
			"failedV2RootArchiveSha256",
			"finalBackupManifestSha256",
			"kind",
			"operationId",
			"planSha256",
			"releaseId",
			"schemaVersion",
			"sourceCommit"
		] and
		.schemaVersion == 1 and
		.kind == "nexus-v2-private-alpha-host-action-marker" and
		.operationId == $operationId and
		.planSha256 == $planSha256 and
		.releaseId == $releaseId and
		.sourceCommit == $sourceCommit and
		.componentId == $componentId and
		.action == $action and
		.finalBackupManifestSha256 == $manifestSha256 and
		(.componentSourceCommits | keys == ["chain", "media"]) and
		(.completedAtUtc | type == "string" and endswith("Z"))
		' "${output}" >/dev/null ||
		die "protected remote idempotency marker drifted"
	[[ "$(jq -r '.componentSourceCommits.chain' "${output}")" == "${chain_commit}" ]] ||
		die "protected marker chain source drifted"
	[[ "$(jq -r '.componentSourceCommits.media' "${output}")" == "${media_commit}" ]] ||
		die "protected marker media source drifted"
}

marker_exists="$(
	remote_root_bash <<EOF
set -euo pipefail
if [[ -f "${marker_path}" ]]; then printf 'yes'; else printf 'no'; fi
EOF
)"
if [[ "${marker_exists}" == "yes" ]]; then
	existing_marker="${work_dir}/existing-marker.json"
	read_marker "${existing_marker}"
	existing_checks="${work_dir}/existing-checks.json"
	jq -c '.checks' "${existing_marker}" >"${existing_checks}"
	existing_failed="$(jq -r '.failedV2RootArchiveSha256 // ""' "${existing_marker}")"
	write_result \
		false \
		true \
		"${existing_failed}" \
		"$(shasum -a 256 "${existing_marker}" | awk '{print $1}')" \
		"${existing_checks}" \
		"$(jq -r '.completedAtUtc' "${existing_marker}")"
	exit 0
fi
[[ "${marker_exists}" == "no" ]] ||
	die "cannot determine protected remote idempotency state"

read_action_marker() {
	local required_action="$1"
	local output="$2"
	local required_path="${marker_dir}/${required_action}.json"
	local value

	value="$(
		remote_root_bash <<EOF
set -euo pipefail
test -f "${required_path}"
cat "${required_path}"
EOF
	)"
	printf '%s\n' "${value}" >"${output}"
	jq -e \
		--arg operationId "${operation_id}" \
		--arg planSha256 "${plan_sha256}" \
		--arg action "${required_action}" \
		--arg manifestSha256 "${manifest_sha256}" \
		'
		.schemaVersion == 1 and
		.kind == "nexus-v2-private-alpha-host-action-marker" and
		.operationId == $operationId and
		.planSha256 == $planSha256 and
		.action == $action and
		.finalBackupManifestSha256 == $manifestSha256
		' "${output}" >/dev/null ||
		die "required protected action marker is absent or drifted"
}

strict_post_cutover_smoke() {
	local status_output="${work_dir}/status.log"
	"${pinned_status}" >"${status_output}" 2>&1 ||
		die "pinned status script failed"
	remote_root_bash <<EOF
set +e
ok=1
test "\$(cat "${REMOTE_RELEASE_VERSION_FILE}" 2>/dev/null)" = "${release_id}" || ok=0
test "\$(cat "${REMOTE_CHAIN_SOURCE_COMMIT_FILE}" 2>/dev/null)" = "${chain_commit}" || ok=0
test "\$(cat "${REMOTE_MEDIA_SOURCE_COMMIT_FILE}" 2>/dev/null)" = "${media_commit}" || ok=0
systemctl is-active --quiet "${REMOTE_NODE_SERVICE_NAME}.service" || ok=0
systemctl is-active --quiet "${AUTHORITY_SERVICE_NAME}.service" || ok=0
${REMOTE_DOCKER_COMPOSE_CMD} ps --status running --services 2>/dev/null | grep -qx 'media-service' || ok=0
${REMOTE_DOCKER_COMPOSE_CMD} ps --status running --services 2>/dev/null | grep -qx 'ipfs' || ok=0
runtime_response="\$(curl -fsS --max-time 10 -H 'Content-Type: application/json' \
	-d '{"id":1,"jsonrpc":"2.0","method":"state_getRuntimeVersion","params":[]}' \
	"http://127.0.0.1:${CHAIN_RPC_PORT}" 2>/dev/null)" || ok=0
jq -e --argjson spec "${RUNTIME_SPEC_VERSION}" '.result.specVersion == \$spec' \
	<<<"\${runtime_response:-{}}" >/dev/null 2>&1 || ok=0
media_response="\$(curl -fsS --max-time 15 "http://127.0.0.1:${MEDIA_PORT}/health/ready" 2>/dev/null)" || ok=0
jq -e --arg release "${release_id}" --arg source "${media_commit}" \
	'.ok == true and .releaseVersion == \$release and .sourceCommit == \$source and
	 .dependencies.chain.connected == true and .dependencies.ipfs == true and
	 .dependencies.ffmpeg == true' <<<"\${media_response:-{}}" >/dev/null 2>&1 || ok=0
authority_response="\$(curl -fsS --max-time 10 "http://127.0.0.1:${AUTHORITY_PORT}/v1/status" 2>/dev/null)" || ok=0
jq -e '.ok == true and .submitter_mode == "live_alpha" and .submitter_authorized == true' \
	<<<"\${authority_response:-{}}" >/dev/null 2>&1 || ok=0
curl -fsS --max-time 10 -X POST "http://127.0.0.1:${IPFS_API_PORT}/api/v0/version" 2>/dev/null |
	jq -e '.Version | type == "string" and length > 0' >/dev/null 2>&1 || ok=0
upload_status="\$(curl -sS --max-time 10 -o /dev/null -w '%{http_code}' \
	-X POST "http://127.0.0.1:${MEDIA_PORT}/media/upload" 2>/dev/null)" || ok=0
case "\${upload_status:-000}" in 2*) ok=0 ;; esac
if [[ "\${ok}" -eq 1 ]]; then printf 'true'; else printf 'false'; fi
exit 0
EOF
}

failed_hash=""
checks_path="${work_dir}/checks.json"

case "${action}" in
	prepare-reset-archives)
		jq -cn '{
			sourceIdentityPinned:true,
			credentialsResolvable:true,
			archivePreparationNonDestructive:true,
			archivesPreparedAndReadOnly:true,
			currentAlphaStatePreserved:true,
			noResetApplied:true,
			readinessIdentityBound:true
		}' >"${checks_path}"
		;;
	post-cutover-smoke)
		smoke_passed="$(strict_post_cutover_smoke)"
		[[ "${smoke_passed}" == "true" || "${smoke_passed}" == "false" ]] ||
			die "protected post-cutover smoke did not produce a boolean"
		jq -cn --argjson smoke "${smoke_passed}" '{
			sourceIdentityPinned:true,
			requiredResetArchivesPresent:true,
			smokePassed:$smoke
		}' >"${checks_path}"
		;;
	pause-v2-writes)
		remote_root_bash <<EOF
set -euo pipefail
systemctl stop "${REMOTE_NODE_SERVICE_NAME}.service"
systemctl stop "${AUTHORITY_SERVICE_NAME}.service"
${REMOTE_DOCKER_COMPOSE_CMD} stop media-service
! systemctl is-active --quiet "${REMOTE_NODE_SERVICE_NAME}.service"
! systemctl is-active --quiet "${AUTHORITY_SERVICE_NAME}.service"
! ${REMOTE_DOCKER_COMPOSE_CMD} ps --status running --services | grep -qx 'media-service'
${REMOTE_DOCKER_COMPOSE_CMD} ps --status running --services | grep -qx 'ipfs'
test -d "${REMOTE_NODE_DATA_DIR}"
docker volume inspect "${REMOTE_IPFS_DATA_VOLUME}" "${REMOTE_IPFS_STAGING_VOLUME}" >/dev/null
EOF
		jq -cn '{
			sourceIdentityPinned:true,
			requiredResetArchivesPresent:true,
			v2WritesPaused:true,
			statePreserved:true,
			restoreNotAttempted:true
		}' >"${checks_path}"
		;;
	archive-failed-v2)
		pause_marker="${work_dir}/pause-marker.json"
		read_action_marker "pause-v2-writes" "${pause_marker}"
		failed_archive_dir="${DEPLOY_ROOT}/shared/rollback/nexus-v2-post-cutover/${operation_id}/${component_id}/failed-v2"
		failed_hash="$(
			remote_root_bash <<EOF
set -euo pipefail
umask 077
archive_dir="${failed_archive_dir}"
manifest="\${archive_dir}/archive-manifest.txt"
if [[ -f "\${manifest}" ]]; then
	test -z "\$(find "\${archive_dir}" -perm /222 -print -quit)"
	(cd "\${archive_dir}" && shasum -a 256 -c file-sha256.txt >/dev/null)
	shasum -a 256 "\${manifest}" | awk '{print \$1}'
	exit 0
fi
[[ ! -e "\${archive_dir}" ]] || {
	echo "partial failed-V2 archive requires manual review" >&2
	exit 1
}
mkdir -p "\${archive_dir}"
systemctl is-active --quiet "${REMOTE_NODE_SERVICE_NAME}.service" && exit 1 || true
systemctl is-active --quiet "${AUTHORITY_SERVICE_NAME}.service" && exit 1 || true
${REMOTE_DOCKER_COMPOSE_CMD} stop ipfs >/dev/null
! ${REMOTE_DOCKER_COMPOSE_CMD} ps --status running --services | grep -qx 'media-service'
! ${REMOTE_DOCKER_COMPOSE_CMD} ps --status running --services | grep -qx 'ipfs'
tar czf "\${archive_dir}/node-root.tar.gz" -C "${DEPLOY_ROOT}" node/current
tar czf "\${archive_dir}/media-root.tar.gz" -C "${DEPLOY_ROOT}" media/current
tar czf "\${archive_dir}/authority-root.tar.gz" -C "${DEPLOY_ROOT}" arcade-authority/current
tar czf "\${archive_dir}/shared-env.tar.gz" -C "${DEPLOY_ROOT}" shared/env
tar czf "\${archive_dir}/shared-state.tar.gz" -C "${DEPLOY_ROOT}" shared/state
tar czf "\${archive_dir}/node-data.tar.gz" -C "${REMOTE_NODE_DATA_DIR}" .
ipfs_data_mount="\$(docker volume inspect -f '{{ .Mountpoint }}' "${REMOTE_IPFS_DATA_VOLUME}")"
ipfs_staging_mount="\$(docker volume inspect -f '{{ .Mountpoint }}' "${REMOTE_IPFS_STAGING_VOLUME}")"
tar czf "\${archive_dir}/ipfs-data.tar.gz" -C "\${ipfs_data_mount}" .
tar czf "\${archive_dir}/ipfs-staging.tar.gz" -C "\${ipfs_staging_mount}" .
cp "${REMOTE_NODE_SERVICE_UNIT_FILE}" "\${archive_dir}/node-service.service"
cp "${REMOTE_AUTHORITY_SERVICE_UNIT_FILE}" "\${archive_dir}/authority-service.service"
docker image inspect \
	"\$(${REMOTE_DOCKER_COMPOSE_CMD} images -q media-service)" \
	"\$(${REMOTE_DOCKER_COMPOSE_CMD} images -q ipfs)" \
	>"\${archive_dir}/docker-images.json"
(
	cd "\${archive_dir}"
	for file in *.gz *.json *.service; do
		shasum -a 256 "\${file}"
	done | LC_ALL=C sort
) >"\${archive_dir}/file-sha256.txt"
{
	printf 'schemaVersion=1\n'
	printf 'kind=nexus-v2-private-alpha-failed-v2-chain-media-archive\n'
	printf 'operationId=%s\n' "${operation_id}"
	printf 'planSha256=%s\n' "${plan_sha256}"
	printf 'releaseId=%s\n' "${release_id}"
	printf 'sourceCommit=%s\n' "${source_commit}"
	printf 'mediaSourceCommit=%s\n' "${media_commit}"
	printf 'finalBackupManifestSha256=%s\n' "${manifest_sha256}"
	printf 'createdAtUtc=%s\n' "\$(date -u +%Y-%m-%dT%H:%M:%SZ)"
	printf '%s\n' 'files:'
	cat "\${archive_dir}/file-sha256.txt"
} >"\${manifest}"
chmod -R a-w "\${archive_dir}"
test -z "\$(find "\${archive_dir}" -perm /222 -print -quit)"
(cd "\${archive_dir}" && shasum -a 256 -c file-sha256.txt >/dev/null)
shasum -a 256 "\${manifest}" | awk '{print \$1}'
EOF
		)"
		[[ "${failed_hash}" =~ ^[0-9a-f]{64}$ ]] ||
			die "failed V2 archive manifest hash is invalid"
		jq -cn '{
			sourceIdentityPinned:true,
			requiredResetArchivesPresent:true,
			failedV2RootArchived:true,
			archiveManifestImmutable:true
		}' >"${checks_path}"
		;;
	restore-final-backup)
		pause_marker="${work_dir}/pause-marker.json"
		archive_marker="${work_dir}/archive-marker.json"
		read_action_marker "pause-v2-writes" "${pause_marker}"
		read_action_marker "archive-failed-v2" "${archive_marker}"
		failed_hash="$(jq -er '.failedV2RootArchiveSha256' "${archive_marker}")"
		[[ "${failed_hash}" =~ ^[0-9a-f]{64}$ ]] ||
			die "protected failed V2 archive identity is invalid"
		remote_failed_hash="$(
			remote_root_bash <<EOF
set -euo pipefail
archive_dir="${DEPLOY_ROOT}/shared/rollback/nexus-v2-post-cutover/${operation_id}/${component_id}/failed-v2"
manifest="\${archive_dir}/archive-manifest.txt"
test -f "\${manifest}"
test -z "\$(find "\${archive_dir}" -perm /222 -print -quit)"
(cd "\${archive_dir}" && shasum -a 256 -c file-sha256.txt >/dev/null)
shasum -a 256 "\${manifest}" | awk '{print \$1}'
EOF
		)"
		[[ "${remote_failed_hash}" == "${failed_hash}" ]] ||
			die "failed V2 archive identity drifted before restore"
		"${pinned_restore_state}" --verified-final-backup "${staging_path}"
		"${pinned_deploy_node}" --verify-restored-final-backup "${staging_path}"
		"${pinned_deploy_media}" --verify-restored-final-backup "${staging_path}"
		jq -cn '{
			sourceIdentityPinned:true,
			requiredResetArchivesPresent:true,
			failedV2RootArchivePresent:true,
			finalBackupHashesVerified:true,
			restoreEvidenceMatched:true,
			existingRestoreScriptUsed:true,
			existingDeployScriptsUsed:true,
			restoreCompleted:true
		}' >"${checks_path}"
		;;
	restored-smoke)
		archive_marker="${work_dir}/archive-marker.json"
		restore_marker="${work_dir}/restore-marker.json"
		read_action_marker "archive-failed-v2" "${archive_marker}"
		read_action_marker "restore-final-backup" "${restore_marker}"
		failed_hash="$(jq -er '.failedV2RootArchiveSha256' "${archive_marker}")"
		[[ "$(jq -r '.failedV2RootArchiveSha256' "${restore_marker}")" == "${failed_hash}" ]] ||
			die "protected restore marker lost the failed V2 archive identity"
		"${pinned_status}" >"${work_dir}/restored-status.log" 2>&1 ||
			die "pinned status script failed after restore"
		remote_root_bash <<EOF
set -euo pipefail
marker="${REMOTE_STATE_DIR}/nexus-v2-final-backup-restored.json"
test -f "\${marker}"
test "\$(jq -r '.releaseId' "\${marker}")" = "${release_id}"
test "\$(jq -r '.sourceCommit' "\${marker}")" = "${source_commit}"
test "\$(jq -r '.mediaSourceCommit' "\${marker}")" = "${media_commit}"
test "\$(jq -r '.backupManifestSha256' "\${marker}")" = "${manifest_sha256}"
systemctl is-active --quiet "${REMOTE_NODE_SERVICE_NAME}.service"
! systemctl is-active --quiet "${AUTHORITY_SERVICE_NAME}.service"
${REMOTE_DOCKER_COMPOSE_CMD} ps --status running --services | grep -qx 'media-service'
${REMOTE_DOCKER_COMPOSE_CMD} ps --status running --services | grep -qx 'ipfs'
curl -fsS --max-time 10 -H 'Content-Type: application/json' \
	-d '{"id":1,"jsonrpc":"2.0","method":"chain_getFinalizedHead","params":[]}' \
	"http://127.0.0.1:${CHAIN_RPC_PORT}" |
	jq -e '.result | type == "string" and test("^0x[0-9a-fA-F]{64}$")' >/dev/null
curl -fsS --max-time 15 "http://127.0.0.1:${MEDIA_PORT}/health/ready" |
	jq -e '.ok == true and .dependencies.chain.connected == true and
		.dependencies.ipfs == true and .dependencies.ffmpeg == true' >/dev/null
curl -fsS --max-time 10 -X POST "http://127.0.0.1:${IPFS_API_PORT}/api/v0/version" |
	jq -e '.Version | type == "string" and length > 0' >/dev/null
upload_status="\$(curl -sS --max-time 10 -o /dev/null -w '%{http_code}' \
	-X POST "http://127.0.0.1:${MEDIA_PORT}/media/upload")"
case "\${upload_status}" in 2*) exit 1 ;; esac
gate_kind="\$(jq -r '.kind' "${REMOTE_STATE_DIR}/nexus-v2-restored-economic-gates.json")"
case "\${gate_kind}" in
	nexus-v2-private-alpha-economic-gates)
		jq -e '
			.tcg.features.Packs == false and
			.tcg.features.Conversion == false and
			.tcg.features.Ranked == false and
			.randomness.productionEconomicUseAllowed == false and
			.issuance.paidV2IssuanceCallAvailable == false and
			.legacyEconomy.economicWritesEnabled == false
		' "${REMOTE_STATE_DIR}/nexus-v2-restored-economic-gates.json" >/dev/null
		;;
	nexus-v2-private-alpha-pre-v16-fresh-reset-gates)
		jq -e '
			.operationScope.paidOrPublicActivationAllowed == false and
			.externalReviewFlags.cryptographyApproved == false and
			.externalReviewFlags.paidFeaturesApproved == false and
			.externalReviewFlags.publicProductionApproved == false
		' "${REMOTE_STATE_DIR}/nexus-v2-restored-economic-gates.json" >/dev/null
		;;
	*) exit 1 ;;
esac
EOF
		jq -cn '{
			sourceIdentityPinned:true,
			requiredResetArchivesPresent:true,
			failedV2RootArchivePresent:true,
			componentHealthy:true,
			backupIdentityReadback:true,
			economicFlagsDisabled:true
		}' >"${checks_path}"
		;;
esac

completed_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
host_marker="${work_dir}/host-marker.json"
failed_json="null"
[[ -z "${failed_hash}" ]] || failed_json="\"${failed_hash}\""
jq -cn \
	--slurpfile context "${context_path}" \
	--slurpfile checks "${checks_path}" \
	--arg operationId "${operation_id}" \
	--arg planSha256 "${plan_sha256}" \
	--arg releaseId "${release_id}" \
	--arg sourceCommit "${source_commit}" \
	--arg componentId "${component_id}" \
	--arg action "${action}" \
	--arg manifestSha256 "${manifest_sha256}" \
	--arg completedAtUtc "${completed_at}" \
	--argjson failedV2RootArchiveSha256 "${failed_json}" \
	'{
		schemaVersion:1,
		kind:"nexus-v2-private-alpha-host-action-marker",
		operationId:$operationId,
		planSha256:$planSha256,
		releaseId:$releaseId,
		sourceCommit:$sourceCommit,
		componentSourceCommits:$context[0].componentSourceCommits,
		componentId:$componentId,
		action:$action,
		finalBackupManifestSha256:$manifestSha256,
		failedV2RootArchiveSha256:$failedV2RootArchiveSha256,
		checks:$checks[0],
		completedAtUtc:$completedAtUtc
	}' >"${host_marker}"
chmod 0400 "${host_marker}"

remote_pending="${REMOTE_SCRIPT_DIR}/nexus-v2-${operation_id}-${component_id}-${action}.json"
remote_bash <<EOF
set -euo pipefail
mkdir -p "${REMOTE_SCRIPT_DIR}"
rm -f "${remote_pending}"
EOF
rsync_to_remote_no_delete "${host_marker}" "${remote_pending}"
remote_root_bash <<EOF
set -euo pipefail
mkdir -p "${marker_dir}"
[[ ! -e "${marker_path}" ]] || {
	echo "protected action marker appeared concurrently" >&2
	exit 1
}
install -m 0440 "${remote_pending}" "${marker_path}"
rm -f "${remote_pending}"
EOF

marker_copy="${work_dir}/marker-readback.json"
read_marker "${marker_copy}"
marker_sha256="$(shasum -a 256 "${marker_copy}" | awk '{print $1}')"
readback_checks="${work_dir}/readback-checks.json"
jq -c '.checks' "${marker_copy}" >"${readback_checks}"
write_result \
	true \
	false \
	"${failed_hash}" \
	"${marker_sha256}" \
	"${readback_checks}" \
	"${completed_at}"
