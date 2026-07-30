#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

fresh=false
candidate_output=""
promotion_manifest=""
evidence_output=""

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
		--help|-h)
			cat <<'EOF'
Usage: deploy-media.sh [--fresh]
       deploy-media.sh --build-candidate OUTPUT.json
       deploy-media.sh --promote-candidate CANDIDATE.json --evidence OUTPUT.json

Development deploys may build and reconcile in place. Release deploys are two-phase:
--build-candidate builds immutable media/Kubo image evidence without changing running services;
--promote-candidate verifies those exact images and cuts over without building, pulling, or
resetting persistent IPFS volumes.
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
require_cmd rsync
require_cmd shasum
require_cmd ssh

if [[ -n "$candidate_output" && -n "$promotion_manifest" ]]; then
	die "--build-candidate and --promote-candidate are mutually exclusive"
fi
if $fresh && { [[ -n "$candidate_output" ]] || [[ -n "$promotion_manifest" ]]; }; then
	die "--fresh cannot be combined with a release candidate operation"
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

CHAIN_SOURCE_COMMIT="$(require_release_source "${REPO_ROOT}" "alpha deploy tooling" "${ETERRA_EXPECTED_CHAIN_COMMIT}")"
MEDIA_SOURCE_COMMIT="$(require_release_source "${MEDIA_REPO_DIR}" "media service" "${ETERRA_EXPECTED_MEDIA_COMMIT}")"
export CHAIN_SOURCE_COMMIT MEDIA_SOURCE_COMMIT

if [[ "$ETERRA_RELEASE_VERSION" != "dev" && "$fresh" == "true" ]]; then
	die "release deploys must preserve media/IPFS state; --fresh is forbidden"
fi

bundle_dir="$(make_temp_dir)"
render_runtime_env_bundle "$bundle_dir"
media_build_hash="$(compute_media_build_hash)"
media_runtime_hash="$(compute_media_runtime_hash "${bundle_dir}/media.env")"
media_image_ref="${REMOTE_MEDIA_PROJECT_NAME}-service:${media_build_hash}"
remote_tmp_dir="${DEPLOY_ROOT}/tmp/media-deploy"

remote_bash <<EOF
set -euo pipefail
mkdir -p "${REMOTE_MEDIA_DIR}" "${remote_tmp_dir}"
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

if [[ -n "$candidate_output" ]]; then
	remote_manifest="${remote_tmp_dir}/media-image-candidate.json"
	rsync_to_remote_no_delete "${bundle_dir}/media.env" "${remote_tmp_dir}/media.env"
	log "building immutable media release candidate without changing running services"
	remote_bash <<EOF
set -euo pipefail
cd "${REMOTE_MEDIA_DIR}"
docker pull '${KUBO_IMAGE}' >/dev/null
MEDIA_IMAGE_REF='${media_image_ref}' \
  docker compose --project-name '${REMOTE_MEDIA_PROJECT_NAME}' \
  -f '${REMOTE_MEDIA_COMPOSE_BASE}' -f '${REMOTE_MEDIA_COMPOSE_OVERRIDE}' \
  --env-file '${remote_tmp_dir}/media.env' build media-service
media_image_id="\$(docker image inspect --format '{{.Id}}' '${media_image_ref}')"
kubo_image_id="\$(docker image inspect --format '{{.Id}}' '${KUBO_IMAGE}')"
[[ "\${media_image_id}" == sha256:* && "\${kubo_image_id}" == sha256:* ]] || {
	echo "candidate image IDs must be immutable SHA-256 identifiers" >&2
	exit 1
}
printf '{\n  "schemaVersion": 1,\n  "releaseVersion": "%s",\n  "chainDeployCommit": "%s",\n  "mediaSourceCommit": "%s",\n  "mediaBuildHash": "%s",\n  "mediaImageRef": "%s",\n  "mediaImageId": "%s",\n  "kuboImageRef": "%s",\n  "kuboImageId": "%s"\n}\n' \
  '${ETERRA_RELEASE_VERSION}' '${CHAIN_SOURCE_COMMIT}' '${MEDIA_SOURCE_COMMIT}' \
  '${media_build_hash}' '${media_image_ref}' "\${media_image_id}" \
  '${KUBO_IMAGE}' "\${kubo_image_id}" >'${remote_manifest}'
rm -f '${remote_tmp_dir}/media.env'
EOF
	mkdir -p "$(dirname "$candidate_output")"
	rsync_from_remote_no_delete "$remote_manifest" "$candidate_output"
	remote_bash <<EOF
rm -f '${remote_manifest}'
rmdir '${remote_tmp_dir}' >/dev/null 2>&1 || true
EOF
	log "candidate manifest written to $candidate_output; no media service was deployed"
	exit 0
fi

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
if $promote_candidate; then
	actual_media_id="\$(docker image inspect --format '{{.Id}}' '${media_image_ref}')"
	actual_kubo_id="\$(docker image inspect --format '{{.Id}}' '${KUBO_IMAGE}')"
	[[ "\${actual_media_id}" == '${candidate_media_image_id}' ]] || { echo "media candidate image ID mismatch" >&2; exit 1; }
	[[ "\${actual_kubo_id}" == '${candidate_kubo_image_id}' ]] || { echo "Kubo candidate image ID mismatch" >&2; exit 1; }
	MEDIA_IMAGE_REF='${media_image_ref}' ${REMOTE_DOCKER_COMPOSE_CMD} \
	  up -d --no-build --pull never --remove-orphans
	media_action="promoted"
elif $fresh; then
	echo "[alpha-macmini2010] fresh deploy: removing media/IPFS volumes and cached deploy hashes"
	${REMOTE_DOCKER_COMPOSE_CMD} down --volumes --remove-orphans >/dev/null 2>&1 || true
	rm -f "${REMOTE_MEDIA_BUILD_HASH_FILE}" "${REMOTE_MEDIA_RUNTIME_HASH_FILE}"
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
	health_url="${SITE_PUBLIC_ORIGIN}/media-api/health/ready"
	health_file="${bundle_dir}/media-health.json"
	content_file="${bundle_dir}/media-content.bin"
	curl --fail --silent --show-error --max-time 15 "$health_url" >"$health_file"
	jq -e \
		--arg release "$ETERRA_RELEASE_VERSION" \
		--arg source "$MEDIA_SOURCE_COMMIT" \
		--arg codeHash "$RUNTIME_CODE_HASH" \
		'.ok == true and .releaseVersion == $release and .sourceCommit == $source and
		 .runtime.specVersion == 104 and .runtime.codeHash == $codeHash and
		 .dependencies.chain.connected == true and .dependencies.ipfs == true and .dependencies.ffmpeg == true' \
		"$health_file" >/dev/null || die "media readiness/provenance validation failed"
	curl --fail --silent --show-error --max-time 30 "$MEDIA_RELEASE_CONTENT_SMOKE_URL" >"$content_file"
	[[ -s "$content_file" ]] || die "representative media content response is empty"

	mkdir -p "$(dirname "$evidence_output")"
	python3 - "$evidence_output" "$ETERRA_RELEASE_VERSION" "$CHAIN_SOURCE_COMMIT" "$MEDIA_SOURCE_COMMIT" "$media_build_hash" "$media_runtime_hash" "$remote_media_image_digest" "$KUBO_IMAGE" "$health_url" "$MEDIA_RELEASE_CONTENT_SMOKE_URL" "$health_file" "$content_file" "${promotion_manifest:-}" <<'PY'
import datetime
import hashlib
import json
import pathlib
import sys

(output, release, chain_commit, media_commit, build_hash, runtime_hash,
 image_id, kubo_ref, health_url, content_url, health_file, content_file,
 candidate_file) = sys.argv[1:]

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
    "verifiedAtUtc": datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).isoformat(),
}
pathlib.Path(output).write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
fi

log "alpha media deploy complete release=${ETERRA_RELEASE_VERSION} source=${MEDIA_SOURCE_COMMIT} build_sha256=${media_build_hash} runtime_env_sha256=${media_runtime_hash} image_digest=${remote_media_image_digest}"
