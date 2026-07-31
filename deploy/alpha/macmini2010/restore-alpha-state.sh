#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

verified_final=false
manifest_sha256=""
if [[ $# -eq 1 && "$1" != --* ]]; then
	backup_dir="$1"
elif [[ $# -eq 2 && "$1" == "--verified-final-backup" ]]; then
	verified_final=true
	backup_dir="$2"
else
	die "usage: restore-alpha-state.sh <legacy-backup-dir> | --verified-final-backup <staging-dir>"
fi
[[ -d "${backup_dir}" ]] || die "backup directory not found: ${backup_dir}"
if ${verified_final}; then
	[[ "${NEXUS_V2_ROLLBACK_PROTECTED_CONFIRMATION:-}" == "PRIVATE_ALPHA_ROLLBACK_ONLY" ]] ||
		die "verified final restore requires NEXUS_V2_ROLLBACK_PROTECTED_CONFIRMATION=PRIVATE_ALPHA_ROLLBACK_ONLY"
fi

load_env
require_cmd expect
require_cmd jq
require_cmd python3
require_cmd rsync
require_cmd shasum
require_cmd ssh
require_cmd tar

for required in node-data.tar.gz ipfs-data.tar.gz ipfs-staging.tar.gz node.env media.env; do
	[[ -f "${backup_dir}/${required}" ]] || die "backup is missing ${required}"
	[[ ! -L "${backup_dir}/${required}" ]] || die "backup file must not be a symlink: ${required}"
done

if ${verified_final}; then
	for required in \
		staging-contract.json \
		node-binary \
		chain-spec.json \
		node-service.service \
		media-state.tar.gz \
		media-image-lock.json \
		media-service.json \
		backup-economic-gates.json
	do
		[[ -f "${backup_dir}/${required}" ]] || die "verified final backup is missing ${required}"
		[[ ! -L "${backup_dir}/${required}" ]] || die "verified final backup file must not be a symlink: ${required}"
	done
	[[ "$(jq -r '.schemaVersion' "${backup_dir}/staging-contract.json")" == "1" ]] ||
		die "unsupported restore staging schema"
	[[ "$(jq -r '.kind' "${backup_dir}/staging-contract.json")" == "nexus-v2-private-alpha-chain-media-restore-staging" ]] ||
		die "restore staging kind mismatch"
	[[ "$(jq -r '.releaseId' "${backup_dir}/staging-contract.json")" == "${ETERRA_RELEASE_VERSION}" ]] ||
		die "restore staging release does not match ETERRA_RELEASE_VERSION"
	[[ "$(jq -r '.sourceCommit' "${backup_dir}/staging-contract.json")" == "${ETERRA_EXPECTED_CHAIN_COMMIT}" ]] ||
		die "restore staging source does not match ETERRA_EXPECTED_CHAIN_COMMIT"
	[[ "$(jq -r '.componentSourceCommits.chain' "${backup_dir}/staging-contract.json")" == "${ETERRA_EXPECTED_CHAIN_COMMIT}" ]] ||
		die "restore staging chain component source mismatch"
	[[ "$(jq -r '.componentSourceCommits.media' "${backup_dir}/staging-contract.json")" == "${ETERRA_EXPECTED_MEDIA_COMMIT}" ]] ||
		die "restore staging media component source mismatch"
	manifest_sha256="$(jq -r '.backupManifestSha256' "${backup_dir}/staging-contract.json")"
	[[ "${manifest_sha256}" =~ ^[0-9a-f]{64}$ ]] || die "restore staging manifest SHA-256 is invalid"
	jq -e '
		keys == [
			"backupManifestSha256",
			"componentSourceCommits",
			"files",
			"kind",
			"releaseId",
			"schemaVersion",
			"sourceCommit"
		] and
		(.componentSourceCommits | keys == ["chain", "media"]) and
		(.files | keys == [
			"backup-economic-gates.json",
			"chain-spec.json",
			"ipfs-data.tar.gz",
			"ipfs-staging.tar.gz",
			"media-image-lock.json",
			"media-service.json",
			"media-state.tar.gz",
			"media.env",
			"node-binary",
			"node-data.tar.gz",
			"node-service.service",
			"node.env"
		])
	' "${backup_dir}/staging-contract.json" >/dev/null ||
		die "restore staging contract does not match the closed file set"
	while IFS=$'\t' read -r name expected_sha256; do
		[[ -f "${backup_dir}/${name}" ]] || die "restore staging contract names an absent file: ${name}"
		[[ "${expected_sha256}" =~ ^[0-9a-f]{64}$ ]] || die "restore staging file hash is invalid: ${name}"
		[[ "$(shasum -a 256 "${backup_dir}/${name}" | awk '{print $1}')" == "${expected_sha256}" ]] ||
			die "restore staging file hash mismatch: ${name}"
	done < <(jq -r '.files | to_entries[] | [.key,.value] | @tsv' "${backup_dir}/staging-contract.json")
	for archive in node-data.tar.gz ipfs-data.tar.gz ipfs-staging.tar.gz media-state.tar.gz; do
		tar tzf "${backup_dir}/${archive}" >/dev/null ||
			die "restore staging archive is unreadable: ${archive}"
	done
	python3 - "${backup_dir}" <<'PY'
import pathlib
import sys
import tarfile

root = pathlib.Path(sys.argv[1])
for name in (
    "node-data.tar.gz",
    "ipfs-data.tar.gz",
    "ipfs-staging.tar.gz",
    "media-state.tar.gz",
):
    with tarfile.open(root / name, "r:gz") as archive:
        members = archive.getmembers()
        if not members:
            raise SystemExit(f"empty restore archive: {name}")
        for member in members:
            path = pathlib.PurePosixPath(member.name)
            if member.name in {".", "./"} and member.isdir():
                continue
            if (
                not member.name
                or path.is_absolute()
                or ".." in path.parts
                or not (member.isdir() or member.isfile())
            ):
                raise SystemExit(f"unsafe restore archive member in {name}: {member.name}")
PY
	[[ "$(dd if="${backup_dir}/node-binary" bs=4 count=1 2>/dev/null)" == $'\x7fELF' ]] ||
		die "restore staging node binary is not an ELF artifact"
	jq -e '.name | type == "string" and length > 0' "${backup_dir}/chain-spec.json" >/dev/null ||
		die "restore staging chain spec name is invalid"
	jq -e '.id | type == "string" and length > 0' "${backup_dir}/chain-spec.json" >/dev/null ||
		die "restore staging chain spec ID is invalid"
	grep -q '^\[Service\]$' "${backup_dir}/node-service.service" ||
		die "restore staging node service has no Service section"
	grep -q '^ExecStart=' "${backup_dir}/node-service.service" ||
		die "restore staging node service has no ExecStart"
	[[ "$(jq -r '.schemaVersion' "${backup_dir}/media-image-lock.json")" == "1" ]] ||
		die "unsupported media image-lock schema"
	[[ "$(jq -r '.kind' "${backup_dir}/media-image-lock.json")" == "nexus-v2-private-alpha-media-image-lock" ]] ||
		die "media image-lock kind mismatch"
	[[ "$(jq -r '.schemaVersion' "${backup_dir}/media-service.json")" == "1" ]] ||
		die "unsupported media service-lock schema"
	[[ "$(jq -r '.kind' "${backup_dir}/media-service.json")" == "nexus-v2-private-alpha-media-service-lock" ]] ||
		die "media service-lock kind mismatch"
	backup_gate_kind="$(jq -r '.kind' "${backup_dir}/backup-economic-gates.json")"
	case "${backup_gate_kind}" in
		nexus-v2-private-alpha-economic-gates)
			jq -e '
				.tcg.features.Packs == false and
				.tcg.features.Conversion == false and
				.tcg.features.Ranked == false and
				.tcg.features.MythicalAscension == false and
				.randomness.productionEconomicUseAllowed == false and
				.issuance.paidV2IssuanceCallAvailable == false and
				.reforge.dispatchableAvailable == false and
				.legacyEconomy.economicWritesEnabled == false and
				.arcadeTickets.earningEnabled == false and
				.arcadeTickets.transferEnabled == false and
				.arcadeTickets.redemptionEnabled == false
			' "${backup_dir}/backup-economic-gates.json" >/dev/null ||
				die "backup economic gates do not keep economic surfaces disabled"
			;;
		nexus-v2-private-alpha-pre-v16-fresh-reset-gates)
			jq -e '
				.operationScope.paidOrPublicActivationAllowed == false and
				.operationScope.v2ActivationAllowed == false and
				.externalReviewFlags.cryptographyApproved == false and
				.externalReviewFlags.paidFeaturesApproved == false and
				.externalReviewFlags.publicProductionApproved == false and
				.knownLegacyEconomicSurfaces.reachableThroughWriteIngress == false
			' "${backup_dir}/backup-economic-gates.json" >/dev/null ||
				die "backup pre-V16 gates do not keep economic surfaces unreachable"
			;;
		*)
			die "backup economic-gate kind is unsupported"
			;;
	esac
fi

remote_tmp_dir="${DEPLOY_ROOT}/tmp/restore-$(date +%Y%m%d%H%M%S)"

remote_root_bash <<EOF
set -euo pipefail
rm -rf "${remote_tmp_dir}"
mkdir -p "${remote_tmp_dir}"
EOF

rsync_to_remote "${backup_dir}/" "${remote_tmp_dir}/"

if ${verified_final}; then
	remote_root_bash <<EOF
set -euo pipefail
test "${DEPLOY_ROOT}" = "/opt/eterra-alpha"
test "${REMOTE_NODE_DATA_DIR}" = "/var/lib/eterra-alpha-node"
test "${CHAIN_RPC_PORT}" = "9944"
test "${MEDIA_PORT}" = "4000"
test "${AUTHORITY_PORT}" = "8787"
test "${IPFS_API_PORT}" = "5001"
test "${IPFS_GATEWAY_PORT}" = "8080"
[[ "${REMOTE_NODE_DIR}" == "${DEPLOY_ROOT}/"* && "${REMOTE_NODE_DIR}" != "${DEPLOY_ROOT}" ]]
[[ "${REMOTE_MEDIA_DIR}" == "${DEPLOY_ROOT}/"* && "${REMOTE_MEDIA_DIR}" != "${DEPLOY_ROOT}" ]]
[[ "${REMOTE_STATE_DIR}" == "${DEPLOY_ROOT}/"* && "${REMOTE_STATE_DIR}" != "${DEPLOY_ROOT}" ]]
while IFS=\$'\t' read -r name expected_sha256; do
	test -f "${remote_tmp_dir}/\${name}"
	test "\$(shasum -a 256 "${remote_tmp_dir}/\${name}" | awk '{print \$1}')" = "\${expected_sha256}"
done < <(jq -r '.files | to_entries[] | [.key,.value] | @tsv' "${remote_tmp_dir}/staging-contract.json")
for archive in node-data.tar.gz ipfs-data.tar.gz ipfs-staging.tar.gz media-state.tar.gz; do
	tar tzf "${remote_tmp_dir}/\${archive}" >/dev/null
done
test "\$(jq -r '.projectName' "${remote_tmp_dir}/media-image-lock.json")" = "${REMOTE_MEDIA_PROJECT_NAME}"
test "\$(jq -r '.projectName' "${remote_tmp_dir}/media-service.json")" = "${REMOTE_MEDIA_PROJECT_NAME}"
rm -rf "${remote_tmp_dir}/media-preflight"
mkdir -p "${remote_tmp_dir}/media-preflight"
tar xzf "${remote_tmp_dir}/media-state.tar.gz" -C "${remote_tmp_dir}/media-preflight"
while IFS=\$'\t' read -r relative expected_sha256; do
	test -f "${remote_tmp_dir}/media-preflight/\${relative}"
	test "\$(shasum -a 256 "${remote_tmp_dir}/media-preflight/\${relative}" | awk '{print \$1}')" = "\${expected_sha256}"
done < <(jq -r '.composeFiles[] | [.path,.sha256] | @tsv' "${remote_tmp_dir}/media-service.json")
while IFS=\$'\t' read -r reference expected_id; do
	actual_id="\$(docker image inspect --format '{{.Id}}' "\${reference}")"
	test "\${actual_id}" = "\${expected_id}"
done < <(jq -r '.images[] | [.reference,.imageId] | @tsv' "${remote_tmp_dir}/media-image-lock.json")
EOF
fi

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

install -m 0600 "${remote_tmp_dir}/node.env" "${REMOTE_NODE_ENV_FILE}"
install -m 0600 "${remote_tmp_dir}/media.env" "${REMOTE_MEDIA_ENV_FILE}"
chown root:root "${REMOTE_NODE_ENV_FILE}" "${REMOTE_MEDIA_ENV_FILE}"

if ${verified_final}; then
	rm -rf "${REMOTE_MEDIA_DIR}"
	mkdir -p "${REMOTE_MEDIA_DIR}"
	tar xzf "${remote_tmp_dir}/media-state.tar.gz" -C "${REMOTE_MEDIA_DIR}"
	install -m 0755 "${remote_tmp_dir}/node-binary" "${REMOTE_NODE_BIN}"
	install -m 0644 "${remote_tmp_dir}/chain-spec.json" "${REMOTE_NODE_SPEC}"
	install -m 0644 "${remote_tmp_dir}/node-service.service" "${REMOTE_NODE_SERVICE_UNIT_FILE}"
	chown root:root "${REMOTE_NODE_BIN}" "${REMOTE_NODE_SPEC}" "${REMOTE_NODE_SERVICE_UNIT_FILE}"
	systemctl daemon-reload
fi

systemctl start "${REMOTE_NODE_SERVICE_NAME}.service"
if ${verified_final}; then
	restored_media_ref="\$(jq -r '.images[] | select(.service == "media-service") | .reference' "${remote_tmp_dir}/media-image-lock.json")"
	restored_ipfs_ref="\$(jq -r '.images[] | select(.service == "ipfs") | .reference' "${remote_tmp_dir}/media-image-lock.json")"
	MEDIA_IMAGE_REF="\${restored_media_ref}" KUBO_IMAGE="\${restored_ipfs_ref}" \
		${REMOTE_DOCKER_COMPOSE_CMD} up -d --no-build --pull never
	mkdir -p "${REMOTE_STATE_DIR}"
	install -m 0440 "${remote_tmp_dir}/media-image-lock.json" \
		"${REMOTE_STATE_DIR}/nexus-v2-restored-media-image-lock.json"
	install -m 0440 "${remote_tmp_dir}/media-service.json" \
		"${REMOTE_STATE_DIR}/nexus-v2-restored-media-service-lock.json"
	install -m 0440 "${remote_tmp_dir}/backup-economic-gates.json" \
		"${REMOTE_STATE_DIR}/nexus-v2-restored-economic-gates.json"
	rm -f \
		"${REMOTE_NODE_CODE_HASH_FILE}" \
		"${REMOTE_NODE_SPEC_HASH_FILE}" \
		"${REMOTE_NODE_RUNTIME_HASH_FILE}" \
		"${REMOTE_MEDIA_BUILD_HASH_FILE}" \
		"${REMOTE_MEDIA_RUNTIME_HASH_FILE}" \
		"${REMOTE_MEDIA_IMAGE_DIGEST_FILE}" \
		"${REMOTE_AUTHORITY_BUILD_HASH_FILE}" \
		"${REMOTE_AUTHORITY_RUNTIME_HASH_FILE}" \
		"${REMOTE_AUTHORITY_ARTIFACT_HASH_FILE}"
	printf '%s\n' "${ETERRA_RELEASE_VERSION}" >"${REMOTE_RELEASE_VERSION_FILE}"
	printf '%s\n' "${ETERRA_EXPECTED_CHAIN_COMMIT}" >"${REMOTE_CHAIN_SOURCE_COMMIT_FILE}"
	printf '%s\n' "${ETERRA_EXPECTED_MEDIA_COMMIT}" >"${REMOTE_MEDIA_SOURCE_COMMIT_FILE}"
	cat >"${REMOTE_STATE_DIR}/nexus-v2-final-backup-restored.json" <<MARKER
{"schemaVersion":1,"kind":"nexus-v2-private-alpha-final-backup-restored","releaseId":"${ETERRA_RELEASE_VERSION}","sourceCommit":"${ETERRA_EXPECTED_CHAIN_COMMIT}","mediaSourceCommit":"${ETERRA_EXPECTED_MEDIA_COMMIT}","backupManifestSha256":"${manifest_sha256}","backupEconomicGatesSha256":"\$(shasum -a 256 "${remote_tmp_dir}/backup-economic-gates.json" | awk '{print \$1}')"}
MARKER
	chmod 0440 "${REMOTE_STATE_DIR}/nexus-v2-final-backup-restored.json"
else
	${REMOTE_DOCKER_COMPOSE_CMD} up -d
fi
rm -rf "${remote_tmp_dir}"
EOF

if ${verified_final}; then
	log "verified final Alpha backup restored from ${backup_dir}"
else
	log "legacy alpha restore complete from ${backup_dir}"
fi
