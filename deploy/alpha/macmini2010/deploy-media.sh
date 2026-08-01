#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

fresh=false
candidate_output=""
promotion_manifest=""
evidence_output=""
fresh_reset_readiness=""
dry_run=false
verify_restored_final_backup=""
phase1_closed=false

while [[ $# -gt 0 ]]; do
	case "$1" in
		--build-candidate)
			[[ $# -ge 2 ]] || die "--build-candidate requires an output path"
			candidate_output="$2"
			shift
			;;
		--promote-candidate)
			[[ $# -ge 2 ]] || die "--promote-candidate requires a manifest"
			promotion_manifest="$2"
			shift
			;;
		--evidence)
			[[ $# -ge 2 ]] || die "--evidence requires an output path"
			evidence_output="$2"
			shift
			;;
		--fresh)
			fresh=true
			;;
		--fresh-reset-readiness)
			[[ $# -ge 2 ]] || die "--fresh-reset-readiness requires a packet path"
			fresh_reset_readiness="$2"
			shift
			;;
		--dry-run)
			dry_run=true
			;;
		--phase1-closed)
			phase1_closed=true
			;;
		--verify-restored-final-backup)
			[[ $# -ge 2 ]] || die "--verify-restored-final-backup requires a staging path"
			verify_restored_final_backup="$2"
			shift
			;;
		--help|-h)
			cat <<'EOF'
Usage: deploy-media.sh [--fresh] [--phase1-closed]
       deploy-media.sh --build-candidate OUTPUT.json
       deploy-media.sh --promote-candidate CANDIDATE.json --evidence OUTPUT.json
       deploy-media.sh --fresh --fresh-reset-readiness READINESS.json \
         --promote-candidate CANDIDATE.json --evidence OUTPUT.json [--dry-run]
       deploy-media.sh --verify-restored-final-backup STAGING_DIR

Development deploys may build and reconcile in place. Release deploys are two-phase:
--build-candidate builds immutable media/Kubo image evidence without changing running services;
--promote-candidate verifies those exact images and cuts over without building, pulling, or
resetting persistent IPFS volumes.
The sole release reset exception requires the SHA-256-pinned frozen pre-V16
readiness packet and immutable candidate promotion. --dry-run performs local
validation and exits before SSH.
--phase1-closed is valid only for the guarded fresh replacement. It validates
media readiness and representative IPFS content over SSH loopback without
using the externally closed Caddy ingress.
The restore-verification mode is read-only. It verifies the exact restored
compose definitions, image IDs, environment, service health, IPFS health, and
blocked public-upload surface without building, pulling, restarting, or
deploying.
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
require_cmd curl
require_cmd expect
require_cmd git
require_cmd jq
require_cmd python3
require_cmd rsync
require_cmd shasum
require_cmd ssh

if [[ -n "${verify_restored_final_backup}" ]]; then
	! $fresh && ! $dry_run &&
		[[ -z "${candidate_output}" && -z "${promotion_manifest}" && -z "${evidence_output}" && -z "${fresh_reset_readiness}" ]] ||
		die "--verify-restored-final-backup cannot be combined with deploy/reset options"
	staging_dir="$(cd -- "${verify_restored_final_backup}" 2>/dev/null && pwd)" ||
		die "restore staging directory not found: ${verify_restored_final_backup}"
	[[ -f "${staging_dir}/staging-contract.json" ]] ||
		die "restore staging contract is missing"
	for restored_file in media.env media-image-lock.json media-service.json; do
		[[ -f "${staging_dir}/${restored_file}" && ! -L "${staging_dir}/${restored_file}" ]] ||
			die "restore staging media file is unavailable: ${restored_file}"
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
	[[ "$(jq -r '.componentSourceCommits.media' "${staging_dir}/staging-contract.json")" == "${ETERRA_EXPECTED_MEDIA_COMMIT}" ]] ||
		die "restore staging media component source mismatch"
	manifest_sha256="$(jq -r '.backupManifestSha256' "${staging_dir}/staging-contract.json")"
	[[ "${manifest_sha256}" =~ ^[0-9a-f]{64}$ ]] ||
		die "restore staging manifest hash is invalid"
	expected_env_sha256="$(jq -r '.files[\"media.env\"]' "${staging_dir}/staging-contract.json")"
	expected_image_lock_sha256="$(jq -r '.files[\"media-image-lock.json\"]' "${staging_dir}/staging-contract.json")"
	expected_service_lock_sha256="$(jq -r '.files[\"media-service.json\"]' "${staging_dir}/staging-contract.json")"
	grep -qx 'PUBLIC_MEDIA_UPLOAD_ENABLED=false' "${staging_dir}/media.env" ||
		die "restored media environment does not disable public uploads"
	grep -qx 'ALLOW_DEV_ADMIN_RESET=0' "${staging_dir}/media.env" ||
		die "restored media environment does not disable admin reset"
	require_cmd curl
	remote_root_bash <<EOF
set -euo pipefail
test "${DEPLOY_ROOT}" = "/opt/eterra-alpha"
test "${MEDIA_PORT}" = "4000"
test "${IPFS_API_PORT}" = "5001"
test "${IPFS_GATEWAY_PORT}" = "8080"
test -f "${REMOTE_STATE_DIR}/nexus-v2-final-backup-restored.json"
test "\$(jq -r '.releaseId' "${REMOTE_STATE_DIR}/nexus-v2-final-backup-restored.json")" = "${ETERRA_RELEASE_VERSION}"
test "\$(jq -r '.sourceCommit' "${REMOTE_STATE_DIR}/nexus-v2-final-backup-restored.json")" = "${ETERRA_EXPECTED_CHAIN_COMMIT}"
test "\$(jq -r '.backupManifestSha256' "${REMOTE_STATE_DIR}/nexus-v2-final-backup-restored.json")" = "${manifest_sha256}"
test "\$(shasum -a 256 "${REMOTE_MEDIA_ENV_FILE}" | awk '{print \$1}')" = "${expected_env_sha256}"
test "\$(shasum -a 256 "${REMOTE_STATE_DIR}/nexus-v2-restored-media-image-lock.json" | awk '{print \$1}')" = "${expected_image_lock_sha256}"
test "\$(shasum -a 256 "${REMOTE_STATE_DIR}/nexus-v2-restored-media-service-lock.json" | awk '{print \$1}')" = "${expected_service_lock_sha256}"
while IFS=\$'\t' read -r relative expected_sha256; do
	test -f "${REMOTE_MEDIA_DIR}/\${relative}"
	test "\$(shasum -a 256 "${REMOTE_MEDIA_DIR}/\${relative}" | awk '{print \$1}')" = "\${expected_sha256}"
done < <(jq -r '.composeFiles[] | [.path,.sha256] | @tsv' "${REMOTE_STATE_DIR}/nexus-v2-restored-media-service-lock.json")
while IFS=\$'\t' read -r service reference expected_id; do
	actual_id="\$(docker image inspect --format '{{.Id}}' "\${reference}")"
	test "\${actual_id}" = "\${expected_id}"
	container_id="\$(${REMOTE_DOCKER_COMPOSE_CMD} ps -q "\${service}")"
	test -n "\${container_id}"
	test "\$(docker inspect --format '{{.Image}}' "\${container_id}")" = "\${expected_id}"
done < <(jq -r '.images[] | [.service,.reference,.imageId] | @tsv' "${REMOTE_STATE_DIR}/nexus-v2-restored-media-image-lock.json")
${REMOTE_DOCKER_COMPOSE_CMD} ps --status running --services | grep -qx 'media-service'
${REMOTE_DOCKER_COMPOSE_CMD} ps --status running --services | grep -qx 'ipfs'
curl -fsS --max-time 10 "http://127.0.0.1:${MEDIA_PORT}/health/live" |
	jq -e '.ok == true' >/dev/null
curl -fsS --max-time 15 "http://127.0.0.1:${MEDIA_PORT}/health/ready" |
	jq -e '.ok == true' >/dev/null
curl -fsS --max-time 10 -X POST "http://127.0.0.1:${IPFS_API_PORT}/api/v0/version" |
	jq -e '.Version | type == "string" and length > 0' >/dev/null
ss -ltn | awk '{print \$4}' | grep -Eq '(^|:)${IPFS_GATEWAY_PORT}\$'
upload_status="\$(curl -sS --max-time 10 -o /dev/null -w '%{http_code}' \
	-X POST "http://127.0.0.1:${MEDIA_PORT}/media/upload")"
case "\${upload_status}" in
	2*) echo "public media upload unexpectedly succeeded" >&2; exit 1 ;;
esac
EOF
	log "restored final-backup media verified release=${ETERRA_RELEASE_VERSION} manifest_sha256=${manifest_sha256}"
	exit 0
fi

if [[ -n "$candidate_output" && -n "$promotion_manifest" ]]; then
	die "--build-candidate and --promote-candidate are mutually exclusive"
fi
if $fresh && [[ -n "$candidate_output" ]]; then
	die "--fresh cannot build a candidate; build it in a separate non-mutating phase"
fi
if $phase1_closed && ! $fresh; then
	die "--phase1-closed requires --fresh"
fi
if $phase1_closed && [[ -n "$candidate_output" ]]; then
	die "--phase1-closed cannot be used while building a candidate"
fi
if [[ -n "$fresh_reset_readiness" ]] && ! $fresh; then
	die "--fresh-reset-readiness requires --fresh"
fi
if $dry_run && ! $fresh; then
	die "--dry-run is supported only for the guarded fresh-reset plan"
fi
if [[ "$ETERRA_RELEASE_VERSION" != "dev" && -z "$candidate_output" && -z "$promotion_manifest" ]]; then
	die "release media deploys require --build-candidate or --promote-candidate"
fi
if [[ "$ETERRA_RELEASE_VERSION" != "dev" && -n "$promotion_manifest" && -z "$evidence_output" ]]; then
	die "release candidate promotion requires --evidence"
fi
if [[ -n "$evidence_output" && -e "$evidence_output" ]]; then
	die "refusing to overwrite deployment evidence: $evidence_output"
fi
if [[ -n "$candidate_output" && -e "$candidate_output" ]]; then
	die "refusing to overwrite media candidate: $candidate_output"
fi
if [[ -n "$fresh_reset_readiness" ]]; then
	[[ "$ETERRA_RELEASE_VERSION" != "dev" ]] ||
		die "--fresh-reset-readiness is valid only for a non-dev private-alpha release"
	is_truthy "${NEXUS_V2_LOCAL_ONLY_RELEASE}" ||
		die "guarded release reset requires NEXUS_V2_LOCAL_ONLY_RELEASE=1"
fi
if [[ "$ETERRA_RELEASE_VERSION" != "dev" && "$fresh" == "true" ]]; then
	[[ -n "$fresh_reset_readiness" ]] ||
		die "release deploys preserve media/IPFS state unless --fresh is paired with --fresh-reset-readiness"
	[[ -n "$promotion_manifest" ]] ||
		die "a guarded release media reset requires --promote-candidate"
fi

CHAIN_SOURCE_COMMIT="$(require_release_source "${REPO_ROOT}" "alpha deploy tooling" "${ETERRA_EXPECTED_CHAIN_COMMIT}")"
MEDIA_SOURCE_COMMIT="$(require_release_source "${MEDIA_REPO_DIR}" "media service" "${ETERRA_EXPECTED_MEDIA_COMMIT}")"
export CHAIN_SOURCE_COMMIT MEDIA_SOURCE_COMMIT

bundle_dir="$(make_temp_dir)"
render_runtime_env_bundle "$bundle_dir"
if [[ -n "$fresh_reset_readiness" ]]; then
	stage_fresh_reset_readiness \
		"$fresh_reset_readiness" \
		"$bundle_dir/reset-readiness.json"
fi
media_build_hash="$(compute_media_build_hash)"
media_runtime_hash="$(compute_media_runtime_hash "${bundle_dir}/media.env")"
media_image_ref="${REMOTE_MEDIA_PROJECT_NAME}-service:${media_build_hash}"
remote_tmp_dir="${DEPLOY_ROOT}/tmp/media-deploy"

promote_candidate=false
candidate_media_image_id=""
candidate_kubo_image_id=""
if [[ -n "$promotion_manifest" ]]; then
	[[ -f "$promotion_manifest" ]] || die "candidate manifest not found: $promotion_manifest"
	[[ "$(jq -r '.schemaVersion' "$promotion_manifest")" == "1" ]] || die "unsupported candidate manifest schema"
	[[ "$(jq -r '.releaseVersion' "$promotion_manifest")" == "$ETERRA_RELEASE_VERSION" ]] || die "candidate release mismatch"
	[[ "$(jq -r '.chainDeployCommit' "$promotion_manifest")" == "$CHAIN_SOURCE_COMMIT" ]] || die "candidate chain deploy commit mismatch"
	[[ "$(jq -r '.mediaSourceCommit' "$promotion_manifest")" == "$MEDIA_SOURCE_COMMIT" ]] || die "candidate media source commit mismatch"
	[[ "$(jq -r '.mediaBuildHash' "$promotion_manifest")" == "$media_build_hash" ]] || die "candidate media build hash mismatch"
	[[ "$(jq -r '.mediaImageRef' "$promotion_manifest")" == "$media_image_ref" ]] || die "candidate media image ref mismatch"
	[[ "$(jq -r '.kuboImageRef' "$promotion_manifest")" == "$KUBO_IMAGE" ]] || die "candidate Kubo image ref mismatch"
	candidate_media_image_id="$(jq -r '.mediaImageId' "$promotion_manifest")"
	candidate_kubo_image_id="$(jq -r '.kuboImageId' "$promotion_manifest")"
	[[ "$candidate_media_image_id" == sha256:* && "$candidate_kubo_image_id" == sha256:* ]] || die "candidate image IDs are invalid"
	promote_candidate=true
fi
if $dry_run; then
	log "dry-run: guarded media/IPFS reset and immutable candidate promotion validated; no SSH or live mutation performed"
	log "dry-run: release=${ETERRA_RELEASE_VERSION} chain_source=${CHAIN_SOURCE_COMMIT} media_source=${MEDIA_SOURCE_COMMIT} readiness_sha256=${FRESH_RESET_READINESS_SHA256:-none}"
	exit 0
fi

if [[ -n "$candidate_output" ]]; then
	candidate_nonce="$(python3 -c 'import secrets; print(secrets.token_hex(16))')"
	[[ "$candidate_nonce" =~ ^[0-9a-f]{32}$ ]] || die "candidate staging nonce is invalid"
	candidate_stage="${DEPLOY_ROOT}/tmp/nexus-v2-media-candidate-${candidate_nonce}"
	candidate_source="${candidate_stage}/source"
	candidate_env="${candidate_stage}/media.env"
	candidate_manifest="${candidate_stage}/media-image-candidate.json"
	candidate_project="${REMOTE_MEDIA_PROJECT_NAME}-candidate-${candidate_nonce:0:12}"
	remote_bash <<EOF
set -euo pipefail
case '${candidate_stage}' in
	'${DEPLOY_ROOT}/tmp/nexus-v2-media-candidate-'[0-9a-f]*) ;;
	*) echo 'unsafe media candidate staging path' >&2; exit 1 ;;
esac
[[ ! -e '${candidate_stage}' && ! -L '${candidate_stage}' ]]
mkdir -m 0700 '${candidate_stage}'
mkdir -m 0755 '${candidate_source}'
EOF
	log "syncing media candidate source into isolated staging root"
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
		"${MEDIA_REPO_DIR}/" "${SSH_TARGET}:${candidate_source}/"
	rsync_to_remote_no_delete "${bundle_dir}/media.env" "${candidate_env}"
	log "building immutable media release candidate without touching the active deployment root"
	remote_bash <<EOF
set -euo pipefail
test '${candidate_source}' != '${REMOTE_MEDIA_DIR}'
test ! -L '${candidate_source}'
docker image inspect '${KUBO_IMAGE}' >/dev/null || {
	echo 'pinned Kubo image is absent; candidate build refuses to pull or mutate the host image set' >&2
	exit 1
}
MEDIA_IMAGE_REF='${media_image_ref}' \
  docker compose --project-name '${candidate_project}' \
  -f '${candidate_source}/docker-compose.yaml' \
  -f '${candidate_source}/docker-compose.macmini2010.yaml' \
  --env-file '${candidate_env}' build media-service
test -z "\$(docker compose --project-name '${candidate_project}' \
  -f '${candidate_source}/docker-compose.yaml' \
  -f '${candidate_source}/docker-compose.macmini2010.yaml' \
  --env-file '${candidate_env}' ps -q)"
media_image_id="\$(docker image inspect --format '{{.Id}}' '${media_image_ref}')"
kubo_image_id="\$(docker image inspect --format '{{.Id}}' '${KUBO_IMAGE}')"
[[ "\${media_image_id}" == sha256:* && "\${kubo_image_id}" == sha256:* ]] || {
	echo 'candidate image IDs must be immutable SHA-256 identifiers' >&2
	exit 1
}
printf '{\n  "schemaVersion": 1,\n  "releaseVersion": "%s",\n  "chainDeployCommit": "%s",\n  "mediaSourceCommit": "%s",\n  "mediaBuildHash": "%s",\n  "mediaImageRef": "%s",\n  "mediaImageId": "%s",\n  "kuboImageRef": "%s",\n  "kuboImageId": "%s"\n}\n' \
  '${ETERRA_RELEASE_VERSION}' '${CHAIN_SOURCE_COMMIT}' '${MEDIA_SOURCE_COMMIT}' \
  '${media_build_hash}' '${media_image_ref}' "\${media_image_id}" \
  '${KUBO_IMAGE}' "\${kubo_image_id}" >'${candidate_manifest}'
chmod 0400 '${candidate_manifest}'
EOF
	mkdir -p "$(dirname "$candidate_output")"
	rsync_from_remote_no_delete "$candidate_manifest" "$candidate_output"
	remote_bash <<EOF
set -euo pipefail
case '${candidate_stage}' in
	'${DEPLOY_ROOT}/tmp/nexus-v2-media-candidate-'[0-9a-f]*) rm -rf -- '${candidate_stage}' ;;
	*) echo 'unsafe media candidate cleanup path' >&2; exit 1 ;;
esac
EOF
	log "candidate manifest written to $candidate_output; active media source, services, volumes, and environment were untouched"
	exit 0
fi

remote_bash <<EOF
set -euo pipefail
mkdir -p "${REMOTE_MEDIA_DIR}" "${remote_tmp_dir}"
EOF
if [[ "$ETERRA_RELEASE_VERSION" != "dev" && "$fresh" == "true" ]]; then
	rsync_to_remote_no_delete \
		"${FRESH_RESET_READINESS_STAGED_PATH}" \
		"${remote_tmp_dir}/reset-readiness.json"
	remote_root_bash <<EOF
set -euo pipefail
archive_root="${DEPLOY_ROOT}/archive/nexus-v2-fresh-reset/${FRESH_RESET_READINESS_SHA256}"
component_dir="\${archive_root}/media"
applied_marker="\${component_dir}/reset-applied.marker"
[[ ! -e "\${applied_marker}" ]] || {
	echo "[alpha-macmini2010] readiness packet was already consumed for the media reset" >&2
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
		printf 'media_dir=%s\n' "${REMOTE_MEDIA_DIR}"
		printf 'compose_project=%s\n' "${REMOTE_MEDIA_PROJECT_NAME}"
		printf 'ipfs_data_volume=%s\n' "${REMOTE_IPFS_DATA_VOLUME}"
		printf 'ipfs_staging_volume=%s\n' "${REMOTE_IPFS_STAGING_VOLUME}"
		printf 'readiness_sha256=%s\n' "${FRESH_RESET_READINESS_SHA256}"
		printf 'readiness_release_id=%s\n' "${FRESH_RESET_RELEASE_ID}"
		printf 'frozen_chain_source_commit=%s\n' "${FRESH_RESET_SOURCE_COMMIT}"
		printf 'replacement_chain_source_commit=%s\n' "${CHAIN_SOURCE_COMMIT}"
		printf 'replacement_media_source_commit=%s\n' "${MEDIA_SOURCE_COMMIT}"
		printf 'frozen_block_number=%s\n' "${FRESH_RESET_GATE_BLOCK_NUMBER}"
		printf 'frozen_block_hash=%s\n' "${FRESH_RESET_GATE_BLOCK_HASH}"
	} >"\${component_dir}/deployment-identifiers.before"
	: >"\${component_dir}/file-sha256.before"
	for path in \
		"${REMOTE_MEDIA_ENV_FILE}" \
		"${REMOTE_MEDIA_COMPOSE_BASE}" \
		"${REMOTE_MEDIA_COMPOSE_OVERRIDE}"
	do
		if [[ -f "\${path}" ]]; then
			shasum -a 256 "\${path}" >>"\${component_dir}/file-sha256.before"
		fi
	done
	docker volume inspect \
		"${REMOTE_IPFS_DATA_VOLUME}" \
		"${REMOTE_IPFS_STAGING_VOLUME}" \
		>"\${component_dir}/volume-identities.before.json" 2>/dev/null || printf '[]\n' >"\${component_dir}/volume-identities.before.json"
	if [[ -f "${REMOTE_MEDIA_COMPOSE_BASE}" && -f "${REMOTE_MEDIA_ENV_FILE}" ]]; then
		${REMOTE_DOCKER_COMPOSE_CMD} ps --format json \
			>"\${component_dir}/compose-services.before.json" 2>/dev/null || true
	fi
	chmod -R a-w "\${component_dir}"
	chmod u+w "\${component_dir}"
fi
EOF
fi

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

rsync_to_remote_no_delete "${bundle_dir}/media.env" "${remote_tmp_dir}/media.env"

log "cutting over media stack and starting alpha media compose project"
remote_root_bash <<EOF
set -euo pipefail

mkdir -p "${REMOTE_SHARED_ENV_DIR}" "${remote_tmp_dir}" "${REMOTE_STATE_DIR}"
install -m 0644 "${remote_tmp_dir}/media.env" "${REMOTE_MEDIA_ENV_FILE}"
chown root:root "${REMOTE_MEDIA_ENV_FILE}"
rm -f "${remote_tmp_dir}/media.env"

if [[ -f "${LEGACY_MEDIA_COMPOSE_BASE}" && -f "${LEGACY_MEDIA_ENV_FILE}" ]]; then
	${LEGACY_MEDIA_COMPOSE_CMD} down --remove-orphans >/dev/null 2>&1 || true
fi

media_action="skip"
if $fresh; then
	if [[ "${ETERRA_RELEASE_VERSION}" != "dev" ]]; then
		archive_component_dir="${DEPLOY_ROOT}/archive/nexus-v2-fresh-reset/${FRESH_RESET_READINESS_SHA256:-}/media"
		[[ ! -e "\${archive_component_dir}/reset-applied.marker" ]] || {
			echo "[alpha-macmini2010] readiness packet was already consumed for the media reset" >&2
			exit 1
		}
	fi
	echo "[alpha-macmini2010] fresh deploy: removing media/IPFS volumes and cached deploy hashes"
	${REMOTE_DOCKER_COMPOSE_CMD} down --volumes --remove-orphans >/dev/null 2>&1 || true
	rm -f "${REMOTE_MEDIA_BUILD_HASH_FILE}" "${REMOTE_MEDIA_RUNTIME_HASH_FILE}"
	if [[ "${ETERRA_RELEASE_VERSION}" != "dev" ]]; then
		printf 'component=media\nreset_applied_at_utc=%s\nreplacement_source_commit=%s\n' \
			"\$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
			"${MEDIA_SOURCE_COMMIT}" \
			>"\${archive_component_dir}/reset-applied.marker"
		chmod 0440 "\${archive_component_dir}/reset-applied.marker"
	fi
fi
if $promote_candidate; then
	actual_media_id="\$(docker image inspect --format '{{.Id}}' '${media_image_ref}')"
	actual_kubo_id="\$(docker image inspect --format '{{.Id}}' '${KUBO_IMAGE}')"
	[[ "\${actual_media_id}" == '${candidate_media_image_id}' ]] || { echo "media candidate image ID mismatch" >&2; exit 1; }
	[[ "\${actual_kubo_id}" == '${candidate_kubo_image_id}' ]] || { echo "Kubo candidate image ID mismatch" >&2; exit 1; }
	MEDIA_IMAGE_REF='${media_image_ref}' ${REMOTE_DOCKER_COMPOSE_CMD} \
	  up -d --no-build --pull never --remove-orphans
	media_action="promoted"
elif $fresh; then
	media_action="build"
fi
if [[ "\${media_action}" == "skip" ]] && { [[ ! -f "${REMOTE_MEDIA_BUILD_HASH_FILE}" ]] || [[ "\$(cat "${REMOTE_MEDIA_BUILD_HASH_FILE}")" != "${media_build_hash}" ]]; }; then
	media_action="build"
elif [[ "\${media_action}" == "skip" ]] && { [[ ! -f "${REMOTE_MEDIA_RUNTIME_HASH_FILE}" ]] || [[ "\$(cat "${REMOTE_MEDIA_RUNTIME_HASH_FILE}")" != "${media_runtime_hash}" ]]; }; then
	media_action="reconcile"
elif [[ "\${media_action}" == "skip" ]] && ! ${REMOTE_DOCKER_COMPOSE_CMD} ps --status running --services 2>/dev/null | grep -qx 'ipfs'; then
	media_action="reconcile"
elif [[ "\${media_action}" == "skip" ]] && ! ${REMOTE_DOCKER_COMPOSE_CMD} ps --status running --services 2>/dev/null | grep -qx 'media-service'; then
	media_action="reconcile"
fi

case "\${media_action}" in
	build)
		echo "[alpha-macmini2010] media action: rebuild development image"
		MEDIA_IMAGE_REF='${media_image_ref}' ${REMOTE_DOCKER_COMPOSE_CMD} up -d --build --remove-orphans
		;;
	reconcile)
		echo "[alpha-macmini2010] media action: reuse image and reconcile services"
		MEDIA_IMAGE_REF='${media_image_ref}' ${REMOTE_DOCKER_COMPOSE_CMD} up -d --remove-orphans
		;;
	promoted)
		echo "[alpha-macmini2010] media action: promoted immutable release candidate"
		;;
	*)
		echo "[alpha-macmini2010] media action: stack already up to date"
		;;
esac

${REMOTE_DOCKER_COMPOSE_CMD} ps
media_container_id="\$(${REMOTE_DOCKER_COMPOSE_CMD} ps -q media-service)"
[[ -n "\${media_container_id}" ]] || { echo "media-service container id unavailable" >&2; exit 1; }
media_image_digest="\$(docker inspect --format '{{.Image}}' "\${media_container_id}")"
[[ "\${media_image_digest}" == sha256:* ]] || { echo "media-service image digest unavailable" >&2; exit 1; }
printf '%s\n' "${media_build_hash}" >"${REMOTE_MEDIA_BUILD_HASH_FILE}"
printf '%s\n' "${media_runtime_hash}" >"${REMOTE_MEDIA_RUNTIME_HASH_FILE}"
printf '%s\n' "\${media_image_digest}" >"${REMOTE_MEDIA_IMAGE_DIGEST_FILE}"
printf '%s\n' "${ETERRA_RELEASE_VERSION}" >"${REMOTE_RELEASE_VERSION_FILE}"
printf '%s\n' "${MEDIA_SOURCE_COMMIT}" >"${REMOTE_MEDIA_SOURCE_COMMIT_FILE}"
chown root:root "${REMOTE_MEDIA_BUILD_HASH_FILE}" "${REMOTE_MEDIA_RUNTIME_HASH_FILE}" "${REMOTE_MEDIA_IMAGE_DIGEST_FILE}" \
	"${REMOTE_RELEASE_VERSION_FILE}" "${REMOTE_MEDIA_SOURCE_COMMIT_FILE}"
rmdir "${remote_tmp_dir}" >/dev/null 2>&1 || true
EOF

remote_media_image_digest="$(ssh_to_remote "cat $(shell_escape "${REMOTE_MEDIA_IMAGE_DIGEST_FILE}")")"
if [[ "$ETERRA_RELEASE_VERSION" != "dev" || -n "$evidence_output" ]]; then
	health_file="${bundle_dir}/media-health.json"
	content_file="${bundle_dir}/media-content.bin"
	validation_transport="public_https"
	if $phase1_closed; then
		validation_transport="ssh_loopback"
		health_url="http://127.0.0.1:${MEDIA_PORT}/health/ready"
		read -r content_port content_path < <(
			python3 - "$MEDIA_RELEASE_CONTENT_SMOKE_URL" "$SITE_PUBLIC_ORIGIN" "$MEDIA_PORT" "$IPFS_GATEWAY_PORT" <<'PY'
import sys
from urllib.parse import urlsplit

raw, origin, media_port, gateway_port = sys.argv[1:]
url = urlsplit(raw)
expected = urlsplit(origin)
if (
    url.scheme != "https"
    or url.netloc != expected.netloc
    or url.username is not None
    or url.password is not None
    or url.fragment
):
    raise SystemExit("Phase-1 media smoke URL must use the sealed public origin")
path = url.path
if url.query:
    path += "?" + url.query
if url.path.startswith("/ipfs/"):
    port = gateway_port
elif url.path.startswith("/media-api/"):
    port = media_port
    path = url.path[len("/media-api") :]
    if url.query:
        path += "?" + url.query
else:
    raise SystemExit("Phase-1 media smoke path must target /ipfs/ or /media-api/")
if "'" in path or any(
    character.isspace() or ord(character) < 0x20 for character in path
):
    raise SystemExit("Phase-1 media smoke path is unsafe")
print(port, path)
PY
		) || die "failed to derive Phase-1 loopback content smoke target"
		[[ "$content_port" =~ ^[0-9]+$ && "$content_path" == /* ]] ||
			die "Phase-1 loopback content smoke target is invalid"
		content_url="http://127.0.0.1:${content_port}${content_path}"
		validation_nonce="$(python3 -c 'import secrets; print(secrets.token_hex(16))')"
		remote_validation_dir="${DEPLOY_ROOT}/tmp/nexus-v2-media-validation-${validation_nonce}"
		remote_health_file="${remote_validation_dir}/media-health.json"
		remote_content_file="${remote_validation_dir}/media-content.bin"
		remote_bash <<EOF
set -euo pipefail
case '${remote_validation_dir}' in
	'${DEPLOY_ROOT}/tmp/nexus-v2-media-validation-'[0-9a-f]*) ;;
	*) echo 'unsafe media validation staging path' >&2; exit 1 ;;
esac
[[ ! -e '${remote_validation_dir}' && ! -L '${remote_validation_dir}' ]]
mkdir -m 0700 '${remote_validation_dir}'
curl --fail --silent --show-error --max-time 15 '${health_url}' >'${remote_health_file}'
curl --fail --silent --show-error --max-time 30 '${content_url}' >'${remote_content_file}'
test -s '${remote_content_file}'
EOF
		rsync_from_remote_no_delete "$remote_health_file" "$health_file"
		rsync_from_remote_no_delete "$remote_content_file" "$content_file"
		remote_bash <<EOF
set -euo pipefail
case '${remote_validation_dir}' in
	'${DEPLOY_ROOT}/tmp/nexus-v2-media-validation-'[0-9a-f]*) rm -rf -- '${remote_validation_dir}' ;;
	*) echo 'unsafe media validation cleanup path' >&2; exit 1 ;;
esac
EOF
	else
		health_url="${SITE_PUBLIC_ORIGIN}/media-api/health/ready"
		content_url="$MEDIA_RELEASE_CONTENT_SMOKE_URL"
		curl --fail --silent --show-error --max-time 15 "$health_url" >"$health_file"
		curl --fail --silent --show-error --max-time 30 "$content_url" >"$content_file"
	fi
	jq -e \
		--arg release "$ETERRA_RELEASE_VERSION" \
		--arg source "$MEDIA_SOURCE_COMMIT" \
		--arg codeHash "$RUNTIME_CODE_HASH" \
		--argjson specVersion "$RUNTIME_SPEC_VERSION" \
		'.ok == true and .releaseVersion == $release and .sourceCommit == $source and
		 .runtime.specVersion == $specVersion and .runtime.codeHash == $codeHash and
		 .dependencies.chain.connected == true and .dependencies.ipfs == true and .dependencies.ffmpeg == true' \
		"$health_file" >/dev/null || die "media readiness/provenance validation failed"
	[[ -s "$content_file" ]] || die "representative media content response is empty"

	mkdir -p "$(dirname "$evidence_output")"
	python3 - "$evidence_output" "$ETERRA_RELEASE_VERSION" "$CHAIN_SOURCE_COMMIT" "$MEDIA_SOURCE_COMMIT" "$media_build_hash" "$media_runtime_hash" "$remote_media_image_digest" "$KUBO_IMAGE" "$health_url" "$content_url" "$health_file" "$content_file" "${promotion_manifest:-}" "$validation_transport" "$phase1_closed" <<'PY'
import datetime
import hashlib
import json
import pathlib
import sys

(output, release, chain_commit, media_commit, build_hash, runtime_hash,
 image_id, kubo_ref, health_url, content_url, health_file, content_file,
 candidate_file, validation_transport, phase1_closed) = sys.argv[1:]

def digest(path):
    return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()

evidence = {
    "schemaVersion": 1,
    "releaseVersion": release,
    "chainDeployCommit": chain_commit,
    "mediaSourceCommit": media_commit,
    "mediaBuildHash": build_hash,
    "mediaRuntimeHash": runtime_hash,
    "mediaImageId": image_id,
    "kuboImageRef": kubo_ref,
    "candidateManifestSha256": digest(candidate_file) if candidate_file else None,
    "healthUrl": health_url,
    "healthResponseSha256": digest(health_file),
    "contentSmokeUrl": content_url,
    "contentResponseSha256": digest(content_file),
    "validationTransport": validation_transport,
    "phase1Closed": phase1_closed == "true",
    "verifiedAtUtc": datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).isoformat(),
}
pathlib.Path(output).write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
fi

log "alpha media deploy complete release=${ETERRA_RELEASE_VERSION} source=${MEDIA_SOURCE_COMMIT} build_sha256=${media_build_hash} runtime_env_sha256=${media_runtime_hash} image_digest=${remote_media_image_digest}"
