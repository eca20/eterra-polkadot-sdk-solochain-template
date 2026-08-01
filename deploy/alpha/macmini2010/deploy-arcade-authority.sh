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

authorize_after=0
seed_config_after=0
phase1_closed=0
dry_run=0
promotion_manifest=""
deployment_evidence=""
pre_reset_closure_handoff=""
pre_reset_closure_handoff_sha256=""

while [[ $# -gt 0 ]]; do
	case "$1" in
		--authorize)
			authorize_after=1
			;;
		--seed-config)
			seed_config_after=1
			;;
		--phase1-closed)
			phase1_closed=1
			;;
		--pre-reset-closure-handoff)
			[[ $# -ge 2 ]] || die "--pre-reset-closure-handoff requires a receipt path"
			pre_reset_closure_handoff="$2"
			shift
			;;
		--pre-reset-closure-handoff-sha256)
			[[ $# -ge 2 ]] || die "--pre-reset-closure-handoff-sha256 requires a SHA-256"
			pre_reset_closure_handoff_sha256="$2"
			shift
			;;
		--dry-run)
			dry_run=1
			;;
		--promote-candidate)
			[[ $# -ge 2 ]] || die "--promote-candidate requires authority-candidate.json"
			promotion_manifest="$2"
			shift
			;;
		--evidence)
			[[ $# -ge 2 ]] || die "--evidence requires an output receipt path"
			deployment_evidence="$2"
			shift
			;;
		--help|-h)
			cat <<'EOF'
Usage: deploy-arcade-authority.sh [--authorize] [--seed-config] [--phase1-closed] [--dry-run]
       [--promote-candidate authority-candidate.json --evidence receipt.json]
       [--pre-reset-closure-handoff HANDOFF.json --pre-reset-closure-handoff-sha256 SHA256]

Builds and deploys the self-hosted Nova Rail authority relay API and operator.
Pass --authorize to run the one-shot operator after the service is deployed.
Pass --seed-config to idempotently seed the Nova Rail ArcadeCore game config.
Release/Phase-1 deployments promote only an immutable, pre-published Nexus V2
authority candidate. They never publish locally, authorize, seed, or write to
the chain. --phase1-closed binds the service to 127.0.0.1 and precloses every
protected firewall rule before restart. --dry-run is strictly local-only.
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
require_cmd base64
require_cmd jq
require_cmd python3
require_cmd rsync
require_cmd shasum
require_cmd ssh

if [[ "${phase1_closed}" -eq 1 ]]; then
	[[ "${authorize_after}" -eq 0 && "${seed_config_after}" -eq 0 ]] ||
		die "--phase1-closed forbids authority authorization and config seeding"
	[[ "${ETERRA_RELEASE_VERSION}" != "dev" ]] ||
		die "--phase1-closed is valid only for a non-dev private-alpha release"
	[[ -n "${pre_reset_closure_handoff}" && -n "${pre_reset_closure_handoff_sha256}" ]] ||
		die "--phase1-closed requires the pre-reset closure handoff and SHA-256"
	NEXUS_V2_PHASE1_CLOSED=1
	RPC_BIND_HOST=127.0.0.1
	AUTHORITY_BIND_HOST=127.0.0.1
	AUTHORITY_RPC_URL="ws://127.0.0.1:${CHAIN_RPC_PORT}"
	export NEXUS_V2_PHASE1_CLOSED RPC_BIND_HOST AUTHORITY_BIND_HOST AUTHORITY_RPC_URL
elif [[ -n "${pre_reset_closure_handoff}" || -n "${pre_reset_closure_handoff_sha256}" ]]; then
	die "pre-reset closure handoff options are valid only with --phase1-closed"
fi

CHAIN_SOURCE_COMMIT="$(require_release_source "${REPO_ROOT}" "alpha deploy tooling" "${ETERRA_EXPECTED_CHAIN_COMMIT}")"
AUTHORITY_SOURCE_COMMIT="$(require_release_source "$(cd -- "${AUTHORITY_REPO_DIR}/.." && pwd)" "SDKGen authority" "${ETERRA_EXPECTED_SDKGEN_COMMIT}")"
export CHAIN_SOURCE_COMMIT AUTHORITY_SOURCE_COMMIT
if [[ "${phase1_closed}" -eq 1 ]]; then
	verify_pre_reset_closure_handoff \
		"${pre_reset_closure_handoff}" \
		"${pre_reset_closure_handoff_sha256}" \
		0
fi

if [[ -z "${promotion_manifest}" && "${AUTHORITY_SUBMITTER_MODE}" == "live_alpha" ]]; then
	[[ -n "${AUTHORITY_RELAY_ACCOUNT}" ]] || die "AUTHORITY_RELAY_ACCOUNT or NOVA_RAIL_RELAY_ACCOUNT is required for live alpha authority"
	[[ "${AUTHORITY_RELAY_ACCOUNT}" != "replace-with-nova-rail-relay-ss58-account" ]] || die "AUTHORITY_RELAY_ACCOUNT must be replaced with the relay SS58 account"
	[[ -n "${AUTHORITY_RELAY_MNEMONIC}" ]] || die "AUTHORITY_RELAY_MNEMONIC is required for live alpha authority; use @/secure/path for file-backed local env"
fi

if [[ "${ETERRA_RELEASE_VERSION}" != "dev" ]]; then
	[[ -n "${promotion_manifest}" && -n "${deployment_evidence}" ]] ||
		die "release authority deployment requires --promote-candidate and --evidence"
	[[ "${authorize_after}" -eq 0 && "${seed_config_after}" -eq 0 ]] ||
		die "immutable Nexus V2 authority promotion forbids authorization and seeding"
elif [[ -n "${promotion_manifest}" || -n "${deployment_evidence}" ]]; then
	[[ -n "${promotion_manifest}" && -n "${deployment_evidence}" ]] ||
		die "--promote-candidate and --evidence must be provided together"
fi

authority_candidate_summary=""
AUTHORITY_CANDIDATE_SHA256=""
AUTHORITY_RELEASE_MANIFEST_SHA256=""
AUTHORITY_CANDIDATE_GENESIS_HASH=""
AUTHORITY_CANDIDATE_METADATA_SHA256=""
AUTHORITY_CANDIDATE_ADAPTER_VERSION=""
AUTHORITY_CANDIDATE_EPOCH=""
AUTHORITY_REMOTE_RELEASE_ROOT=""
if [[ -n "${promotion_manifest}" ]]; then
	[[ "${phase1_closed}" -eq 1 ]] ||
		die "immutable authority candidate promotion is allowed only during Phase-1 closed ingress"
	[[ "${DEPLOY_ROOT}" == "/opt/eterra-alpha" ]] ||
		die "Nexus V2 authority promotion requires DEPLOY_ROOT=/opt/eterra-alpha"
	[[ "${DEPLOY_USER}" == "eterra2010" ]] ||
		die "Nexus V2 authority promotion requires DEPLOY_USER=eterra2010"
	[[ "${AUTHORITY_SERVICE_NAME}" == "eterra-arcade-authority" ]] ||
		die "Nexus V2 authority promotion requires AUTHORITY_SERVICE_NAME=eterra-arcade-authority"
	[[ "${AUTHORITY_PORT}" == "8787" ]] ||
		die "Nexus V2 authority promotion requires AUTHORITY_PORT=8787"
	[[ "${promotion_manifest}" = /* && -f "${promotion_manifest}" && ! -L "${promotion_manifest}" ]] ||
		die "authority candidate path must be an absolute regular non-symlink file"
	[[ "${NEXUS_V2_AUTHORITY_CANDIDATE_PATH}" = /* ]] ||
		die "NEXUS_V2_AUTHORITY_CANDIDATE_PATH must pin the absolute selected candidate"
	[[ "$(cd -- "$(dirname -- "${promotion_manifest}")" && pwd)/$(basename -- "${promotion_manifest}")" == "${NEXUS_V2_AUTHORITY_CANDIDATE_PATH}" ]] ||
		die "selected authority candidate differs from NEXUS_V2_AUTHORITY_CANDIDATE_PATH"
	[[ "${NEXUS_V2_AUTHORITY_CANDIDATE_SHA256}" =~ ^[0-9a-f]{64}$ ]] ||
		die "NEXUS_V2_AUTHORITY_CANDIDATE_SHA256 must be 64 lowercase hex characters"
	authority_candidate_summary="$(
		python3 "${AUTHORITY_CANDIDATE_TOOL}" verify \
			--candidate "${promotion_manifest}" \
			--expected-sha256 "${NEXUS_V2_AUTHORITY_CANDIDATE_SHA256}" \
			--expected-release-id "${ETERRA_RELEASE_VERSION}" \
			--expected-chain-commit "${CHAIN_SOURCE_COMMIT}" \
			--expected-sdkgen-commit "${AUTHORITY_SOURCE_COMMIT}"
	)" || die "authority candidate verification failed"
	AUTHORITY_CANDIDATE_SHA256="$(jq -er '.candidateSha256' <<<"${authority_candidate_summary}")"
	AUTHORITY_RELEASE_MANIFEST_SHA256="$(jq -er '.releaseManifestSha256' <<<"${authority_candidate_summary}")"
	AUTHORITY_CANDIDATE_GENESIS_HASH="$(jq -er '.genesisHash' <<<"${authority_candidate_summary}")"
	AUTHORITY_CANDIDATE_METADATA_SHA256="$(jq -er '.runtimeMetadataScaleSha256' <<<"${authority_candidate_summary}")"
	AUTHORITY_CANDIDATE_ADAPTER_VERSION="$(jq -er '.readModelAdapterVersion' <<<"${authority_candidate_summary}")"
	AUTHORITY_CANDIDATE_EPOCH="$(jq -er '.authorityEpoch' <<<"${authority_candidate_summary}")"
	AUTHORITY_REMOTE_RELEASE_ROOT="${REMOTE_AUTHORITY_RELEASES_DIR}/${AUTHORITY_CANDIDATE_SHA256}"
	[[ "$(jq -er '.runtimeCodeHash' <<<"${authority_candidate_summary}")" == "${RUNTIME_CODE_HASH}" ]] ||
		die "authority candidate runtime code hash differs from selected environment"
	[[ "${AUTHORITY_CANDIDATE_GENESIS_HASH}" == "${NEXUS_V2_ALPHA_GENESIS_HASH}" ]] ||
		die "authority candidate genesis hash differs from selected environment"
	[[ "${AUTHORITY_CANDIDATE_ADAPTER_VERSION}" == "${ETERRA_LEGENDS_READ_MODEL_ADAPTER_VERSION}" ]] ||
		die "authority candidate read-model adapter differs from selected environment"
	[[ "${AUTHORITY_CANDIDATE_EPOCH}" == "${ETERRA_LEGENDS_AUTHORITY_EPOCH}" ]] ||
		die "authority candidate epoch differs from selected environment"
	[[ "$(jq -er '.serviceUnitSha256' <<<"${authority_candidate_summary}")" == "$(shasum -a 256 "${SCRIPT_DIR}/eterra-arcade-authority.service" | awk '{print $1}')" ]] ||
		die "authority candidate service-unit pin differs from chain source"
	[[ "${AUTHORITY_SUBMITTER_MODE}" == "in_memory" ]] ||
		die "Nexus V2 authority promotion requires AUTHORITY_SUBMITTER_MODE=in_memory"
	[[ "${ETERRA_LEGENDS_READ_MODEL_URL}" == https://* ]] ||
		die "Nexus V2 authority read model must use a pinned HTTPS URL"
	[[ "${ETERRA_LEGENDS_READ_MODEL_TIMEOUT_SECONDS}" =~ ^([1-9]|[1-9][0-9]|1[01][0-9]|120)$ ]] ||
		die "ETERRA_LEGENDS_READ_MODEL_TIMEOUT_SECONDS must be 1..120"
	[[ "${ETERRA_LEGENDS_OWNER_AUTHORIZATION_TTL_SECONDS}" =~ ^([5-9]|[1-9][0-9]|[12][0-9][0-9]|300)$ ]] ||
		die "ETERRA_LEGENDS_OWNER_AUTHORIZATION_TTL_SECONDS must be 5..300"
	[[ "${deployment_evidence}" = /* && ! -e "${deployment_evidence}" && ! -L "${deployment_evidence}" ]] ||
		die "authority deployment receipt must be a new absolute path"
	for secret_source in "${ETERRA_LEGENDS_SIGNER_MNEMONIC}" "${ETERRA_LEGENDS_PRIVATE_ALPHA_ACCESS_KEY}"; do
		[[ "${secret_source}" == @/* ]] || die "Nexus V2 authority secrets must use absolute @file sources"
		secret_path="${secret_source#@}"
		[[ -f "${secret_path}" && ! -L "${secret_path}" ]] || die "Nexus V2 authority secret source is unavailable"
		secret_mode="$(stat -f '%OLp' "${secret_path}")"
		(( (8#${secret_mode} & 8#077) == 0 )) || die "Nexus V2 authority secret source must be owner-only"
	done
	if [[ -n "${ETERRA_LEGENDS_SIGNER_DERIVATION_PASSWORD}" ]]; then
		[[ "${ETERRA_LEGENDS_SIGNER_DERIVATION_PASSWORD}" == @/* ]] || die "authority derivation password must use an absolute @file source"
		secret_path="${ETERRA_LEGENDS_SIGNER_DERIVATION_PASSWORD#@}"
		[[ -f "${secret_path}" && ! -L "${secret_path}" ]] || die "authority derivation password source is unavailable"
		secret_mode="$(stat -f '%OLp' "${secret_path}")"
		(( (8#${secret_mode} & 8#077) == 0 )) || die "authority derivation password source must be owner-only"
	fi
	export AUTHORITY_CANDIDATE_SHA256 AUTHORITY_RELEASE_MANIFEST_SHA256
	export AUTHORITY_CANDIDATE_GENESIS_HASH AUTHORITY_CANDIDATE_METADATA_SHA256
	export AUTHORITY_CANDIDATE_ADAPTER_VERSION AUTHORITY_CANDIDATE_EPOCH AUTHORITY_REMOTE_RELEASE_ROOT
fi

phase1_guard_sha256=""
if [[ "${phase1_closed}" -eq 1 ]]; then
	phase1_guard_sha256="$(shasum -a 256 "${SCRIPT_DIR}/nexus-v2-phase1-closed-ingress.sh" | awk '{print $1}')"
fi

if [[ "${dry_run}" -eq 1 ]]; then
	log "dry-run: authority source and Phase-1 closed-start contract validated; no build, SSH, or live mutation performed"
	log "dry-run: release=${ETERRA_RELEASE_VERSION} authority_source=${AUTHORITY_SOURCE_COMMIT} candidate_sha256=${AUTHORITY_CANDIDATE_SHA256:-none} release_manifest_sha256=${AUTHORITY_RELEASE_MANIFEST_SHA256:-none} phase1_closed=${phase1_closed} bind_host=${AUTHORITY_BIND_HOST} phase1_guard_sha256=${phase1_guard_sha256:-none} pre_reset_closure_sha256=${PRE_RESET_CLOSURE_HANDOFF_SHA256:-none}"
	exit 0
fi

# In Phase-1 this is the first remote operation. It reasserts closure before
# local publish time and requires the node's closed-start marker.
if [[ "${phase1_closed}" -eq 1 ]]; then
	phase1_guard_base64="$(base64 <"${SCRIPT_DIR}/nexus-v2-phase1-closed-ingress.sh" | tr -d '\r\n')"
	remote_root_bash <<EOF
set -euo pipefail
test -f "${REMOTE_PHASE1_CLOSED_STATE_FILE}"
test "\$(jq -r '.nodeRpcLoopbackOnly' "${REMOTE_PHASE1_CLOSED_STATE_FILE}")" = "true"
test "\$(jq -r '.nodeP2pLoopbackOnly' "${REMOTE_PHASE1_CLOSED_STATE_FILE}")" = "true"
test "\$(jq -r '.protectedFirewallRulesClosedBeforeNodeStart' "${REMOTE_PHASE1_CLOSED_STATE_FILE}")" = "true"
test "\$(jq -r '.releaseId' "${REMOTE_PHASE1_CLOSED_STATE_FILE}")" = "${ETERRA_RELEASE_VERSION}"
test "\$(jq -r '.sourceCommit' "${REMOTE_PHASE1_CLOSED_STATE_FILE}")" = "${ETERRA_EXPECTED_CHAIN_COMMIT}"
test "\$(jq -r '.preResetClosureHandoffSha256' "${REMOTE_PHASE1_CLOSED_STATE_FILE}")" = "${PRE_RESET_CLOSURE_HANDOFF_SHA256}"
test "\$(jq -r '.mediaLoopbackOnly' "${REMOTE_PHASE1_CLOSED_STATE_FILE}")" = "true"
test "\$(jq -r '.ipfsApiLoopbackOnly' "${REMOTE_PHASE1_CLOSED_STATE_FILE}")" = "true"
test "\$(jq -r '.ipfsGatewayLoopbackOnly' "${REMOTE_PHASE1_CLOSED_STATE_FILE}")" = "true"
test "\$(jq -r '.mediaChainLoopbackConnected' "${REMOTE_PHASE1_CLOSED_STATE_FILE}")" = "true"
test "\$(jq -r '.mediaSourceCommit' "${REMOTE_PHASE1_CLOSED_STATE_FILE}")" = "${ETERRA_EXPECTED_MEDIA_COMMIT}"
guard="\$(mktemp /tmp/nexus-v2-phase1-closed-ingress.XXXXXX)"
trap 'rm -f "\${guard}"' EXIT
printf '%s' '${phase1_guard_base64}' | base64 -d >"\${guard}"
test "\$(shasum -a 256 "\${guard}" | awk '{print \$1}')" = "${phase1_guard_sha256}"
chmod 0700 "\${guard}"
CHAIN_RPC_PORT="${CHAIN_RPC_PORT}" CHAIN_P2P_PORT="${CHAIN_P2P_PORT}" AUTHORITY_PORT="${AUTHORITY_PORT}" \
	MEDIA_PORT="${MEDIA_PORT}" IPFS_API_PORT="${IPFS_API_PORT}" IPFS_GATEWAY_PORT="${IPFS_GATEWAY_PORT}" \
	"\${guard}" preclose
EOF
fi

if [[ -n "${promotion_manifest}" ]]; then
	bundle_dir="$(make_temp_dir)"
	mkdir -p "${bundle_dir}/secrets"
	write_authority_env "${bundle_dir}/arcade-authority.env"
	chmod 0600 "${bundle_dir}/arcade-authority.env"
	install -m 0600 "${ETERRA_LEGENDS_SIGNER_MNEMONIC#@}" \
		"${bundle_dir}/secrets/nexus-v2-legends-authority.mnemonic"
	install -m 0600 "${ETERRA_LEGENDS_PRIVATE_ALPHA_ACCESS_KEY#@}" \
		"${bundle_dir}/secrets/nexus-v2-legends-authority.access-key"
	if [[ -n "${ETERRA_LEGENDS_SIGNER_DERIVATION_PASSWORD}" ]]; then
		install -m 0600 "${ETERRA_LEGENDS_SIGNER_DERIVATION_PASSWORD#@}" \
			"${bundle_dir}/secrets/nexus-v2-legends-authority.derivation-password"
	fi
	authority_env_sha256="$(shasum -a 256 "${bundle_dir}/arcade-authority.env" | awk '{print $1}')"
	legends_mnemonic_sha256="$(shasum -a 256 "${bundle_dir}/secrets/nexus-v2-legends-authority.mnemonic" | awk '{print $1}')"
	legends_access_key_sha256="$(shasum -a 256 "${bundle_dir}/secrets/nexus-v2-legends-authority.access-key" | awk '{print $1}')"
	legends_derivation_password_sha256=""
	if [[ -f "${bundle_dir}/secrets/nexus-v2-legends-authority.derivation-password" ]]; then
		legends_derivation_password_sha256="$(shasum -a 256 "${bundle_dir}/secrets/nexus-v2-legends-authority.derivation-password" | awk '{print $1}')"
	fi
	remote_tmp_dir="${DEPLOY_ROOT}/tmp/arcade-authority-${AUTHORITY_CANDIDATE_SHA256}"
	remote_observation_root="/run/nexus-v2-authority-observation-${AUTHORITY_CANDIDATE_SHA256}"
	remote_observation="${remote_observation_root}/authority-deployment-observation.json"
	local_observation="${bundle_dir}/authority-deployment-observation.json"
	remote_root_bash <<EOF
set -euo pipefail
test ! -e "${remote_tmp_dir}"
test ! -L "${remote_tmp_dir}"
install -d -m 0750 -o "${DEPLOY_USER}" -g "${DEPLOY_USER}" "${remote_tmp_dir}"
install -d -m 0750 -o "${DEPLOY_USER}" -g "${DEPLOY_USER}" "${remote_tmp_dir}/candidate" "${remote_tmp_dir}/config" "${remote_tmp_dir}/secrets"
EOF
	log "transferring exact immutable authority candidate to closed Phase-1 staging"
	rsync_to_remote "$(dirname -- "${promotion_manifest}")/" "${remote_tmp_dir}/candidate/"
	rsync_to_remote_no_delete "${bundle_dir}/arcade-authority.env" "${remote_tmp_dir}/config/arcade-authority.env"
	rsync_to_remote_no_delete "${SCRIPT_DIR}/eterra-arcade-authority.service" "${remote_tmp_dir}/config/eterra-arcade-authority.service"
	rsync_to_remote_no_delete "${SCRIPT_DIR}/nexus-v2-phase1-closed-ingress.sh" "${remote_tmp_dir}/config/nexus-v2-phase1-closed-ingress.sh"
	rsync_to_remote "${bundle_dir}/secrets/" "${remote_tmp_dir}/secrets/"
remote_root_bash <<EOF
set -euo pipefail
incoming_candidate="${remote_tmp_dir}/candidate"
release_root="${AUTHORITY_REMOTE_RELEASE_ROOT}"
authority_root="${DEPLOY_ROOT}/arcade-authority"
if test -e "\${authority_root}" || test -L "\${authority_root}"; then
	test -d "\${authority_root}"
	test ! -L "\${authority_root}"
	chown root:root "\${authority_root}"
	chmod 0755 "\${authority_root}"
else
	install -d -m 0755 -o root -g root "\${authority_root}"
fi
install -d -m 0755 -o root -g root "${REMOTE_AUTHORITY_RELEASES_DIR}"
candidate_stage="${REMOTE_AUTHORITY_RELEASES_DIR}/.pending-${AUTHORITY_CANDIDATE_SHA256}"
test ! -e "\${candidate_stage}"
test ! -L "\${candidate_stage}"
install -d -m 0700 -o root -g root "\${candidate_stage}"
cp -a --no-preserve=ownership -- "\${incoming_candidate}/." "\${candidate_stage}/"
chown -R root:root "\${candidate_stage}"
test -z "\$(find "\${candidate_stage}" -type l -print -quit)"
test -z "\$(find "\${candidate_stage}" ! -type d ! -type f -print -quit)"
test "\$(shasum -a 256 "\${candidate_stage}/authority-candidate.json" | awk '{print \$1}')" = "${AUTHORITY_CANDIDATE_SHA256}"
test "\$(shasum -a 256 "\${candidate_stage}/authority-release-manifest.json" | awk '{print \$1}')" = "${AUTHORITY_RELEASE_MANIFEST_SHA256}"
test "\$(shasum -a 256 "\${candidate_stage}/authority-signer.public.json" | awk '{print \$1}')" = "$(jq -er '.publicSignerSha256' <<<"${authority_candidate_summary}")"
test "\$(shasum -a 256 "\${candidate_stage}/api/catalog/eterra-legends.encounters.private-alpha.v1.json" | awk '{print \$1}')" = "f2846a4ce742f881cce87edd373061d42b720d10a6c324e782c5487060ae7964"
test "\$(find "\${candidate_stage}" -mindepth 1 -maxdepth 1 -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" = "api authority-candidate.json authority-release-manifest.json authority-signer.public.json operator "
test -x "\${candidate_stage}/operator/Eterra.Arcade.Authority.Operator"
test -x "\${candidate_stage}/api/Eterra.Arcade.Authority.Api"
manifest_output="${remote_tmp_dir}/config/release-manifest-verification.txt"
"\${candidate_stage}/operator/Eterra.Arcade.Authority.Operator" \
	verify-release-manifest \
	"\${candidate_stage}/authority-release-manifest.json" \
	"\${candidate_stage}/api" \
	"\${candidate_stage}/operator" >"\${manifest_output}"

test ! -e "\${release_root}"
test ! -L "\${release_root}"
mv -T "\${candidate_stage}" "\${release_root}"
chmod 0755 "\${release_root}"

current="${DEPLOY_ROOT}/arcade-authority/current"
next_link="${DEPLOY_ROOT}/arcade-authority/.current-${AUTHORITY_CANDIDATE_SHA256}"
test ! -e "\${next_link}"
test ! -L "\${next_link}"
if test -e "\${current}" && ! test -L "\${current}"; then
	legacy_archive="${DEPLOY_ROOT}/archive/authority-pre-${ETERRA_RELEASE_VERSION}"
	install -d -m 0750 "${DEPLOY_ROOT}/archive"
	test ! -e "\${legacy_archive}"
	test ! -L "\${legacy_archive}"
	mv "\${current}" "\${legacy_archive}"
elif test -L "\${current}"; then
	unlink "\${current}"
fi
ln -s "\${release_root}" "\${next_link}"
mv -T "\${next_link}" "\${current}"

install -d -m 0755 -o root -g root "${REMOTE_SHARED_ENV_DIR}"
install -d -m 0755 "${REMOTE_STATE_DIR}"
if test -e "${REMOTE_SHARED_SECRET_DIR}" || test -L "${REMOTE_SHARED_SECRET_DIR}"; then
	test -d "${REMOTE_SHARED_SECRET_DIR}"
	test ! -L "${REMOTE_SHARED_SECRET_DIR}"
fi
install -d -m 0750 -o root -g "${DEPLOY_USER}" "${REMOTE_SHARED_SECRET_DIR}"
seal_service_file() {
	local incoming="\$1" target="\$2" expected_sha="\$3" mode="\$4" group="\$5" pending
	pending="\${target}.pending-${AUTHORITY_CANDIDATE_SHA256}"
	test ! -e "\${pending}"
	test ! -L "\${pending}"
	install -m 0400 -o root -g root "\${incoming}" "\${pending}"
	test "\$(shasum -a 256 "\${pending}" | awk '{print \$1}')" = "\${expected_sha}"
	chown root:"\${group}" "\${pending}"
	chmod "\${mode}" "\${pending}"
	mv -T "\${pending}" "\${target}"
	test -f "\${target}"
	test ! -L "\${target}"
	test "\$(shasum -a 256 "\${target}" | awk '{print \$1}')" = "\${expected_sha}"
	test "\$(stat -c '%a %U:%G' "\${target}")" = "\${mode} root:\${group}"
}
seal_service_file \
	"${remote_tmp_dir}/secrets/nexus-v2-legends-authority.mnemonic" \
	"${REMOTE_LEGENDS_SIGNER_MNEMONIC_FILE}" \
	"${legends_mnemonic_sha256}" 640 "${DEPLOY_USER}"
seal_service_file \
	"${remote_tmp_dir}/secrets/nexus-v2-legends-authority.access-key" \
	"${REMOTE_LEGENDS_PRIVATE_ALPHA_ACCESS_KEY_FILE}" \
	"${legends_access_key_sha256}" 640 "${DEPLOY_USER}"
if test -f "${remote_tmp_dir}/secrets/nexus-v2-legends-authority.derivation-password"; then
	seal_service_file \
		"${remote_tmp_dir}/secrets/nexus-v2-legends-authority.derivation-password" \
		"${REMOTE_LEGENDS_SIGNER_DERIVATION_PASSWORD_FILE}" \
		"${legends_derivation_password_sha256}" 640 "${DEPLOY_USER}"
else
	if test -e "${REMOTE_LEGENDS_SIGNER_DERIVATION_PASSWORD_FILE}" || test -L "${REMOTE_LEGENDS_SIGNER_DERIVATION_PASSWORD_FILE}"; then
		test -f "${REMOTE_LEGENDS_SIGNER_DERIVATION_PASSWORD_FILE}"
		test ! -L "${REMOTE_LEGENDS_SIGNER_DERIVATION_PASSWORD_FILE}"
		retired_derivation_password="${DEPLOY_ROOT}/archive/retired-legends-authority.derivation-password-${ETERRA_RELEASE_VERSION}"
		install -d -m 0750 "${DEPLOY_ROOT}/archive"
		test ! -e "\${retired_derivation_password}"
		test ! -L "\${retired_derivation_password}"
		mv -- "${REMOTE_LEGENDS_SIGNER_DERIVATION_PASSWORD_FILE}" "\${retired_derivation_password}"
	fi
fi
if test -e "${REMOTE_LEGENDS_RESULT_JOURNAL_PATH}" || test -L "${REMOTE_LEGENDS_RESULT_JOURNAL_PATH}"; then
	test -d "${REMOTE_LEGENDS_RESULT_JOURNAL_PATH}"
	test ! -L "${REMOTE_LEGENDS_RESULT_JOURNAL_PATH}"
	chown "${DEPLOY_USER}:${DEPLOY_USER}" "${REMOTE_LEGENDS_RESULT_JOURNAL_PATH}"
	chmod 0700 "${REMOTE_LEGENDS_RESULT_JOURNAL_PATH}"
else
	install -d -m 0700 -o "${DEPLOY_USER}" -g "${DEPLOY_USER}" "${REMOTE_LEGENDS_RESULT_JOURNAL_PATH}"
fi
seal_service_file "${remote_tmp_dir}/config/arcade-authority.env" "${REMOTE_AUTHORITY_ENV_FILE}" \
	"${authority_env_sha256}" 640 "${DEPLOY_USER}"
seal_service_file "${remote_tmp_dir}/config/eterra-arcade-authority.service" "${REMOTE_AUTHORITY_SERVICE_UNIT_FILE}" \
	"$(shasum -a 256 "${SCRIPT_DIR}/eterra-arcade-authority.service" | awk '{print $1}')" 644 root
test "\$(shasum -a 256 "${REMOTE_AUTHORITY_SERVICE_UNIT_FILE}" | awk '{print \$1}')" = "$(shasum -a 256 "${SCRIPT_DIR}/eterra-arcade-authority.service" | awk '{print $1}')"
test "\$(shasum -a 256 "${REMOTE_AUTHORITY_ENV_FILE}" | awk '{print \$1}')" = "${authority_env_sha256}"
test "\$(stat -c '%a %U:%G' "${REMOTE_LEGENDS_SIGNER_MNEMONIC_FILE}")" = "640 root:${DEPLOY_USER}"
test "\$(stat -c '%a %U:%G' "${REMOTE_LEGENDS_PRIVATE_ALPHA_ACCESS_KEY_FILE}")" = "640 root:${DEPLOY_USER}"

guard="${remote_tmp_dir}/config/nexus-v2-phase1-closed-ingress.sh"
test "\$(shasum -a 256 "\${guard}" | awk '{print \$1}')" = "${phase1_guard_sha256}"
chmod 0755 "\${guard}"
CHAIN_RPC_PORT="${CHAIN_RPC_PORT}" CHAIN_P2P_PORT="${CHAIN_P2P_PORT}" AUTHORITY_PORT="${AUTHORITY_PORT}" \
	MEDIA_PORT="${MEDIA_PORT}" IPFS_API_PORT="${IPFS_API_PORT}" IPFS_GATEWAY_PORT="${IPFS_GATEWAY_PORT}" \
	"\${guard}" preclose

systemctl daemon-reload
systemctl enable "${AUTHORITY_SERVICE_NAME}.service"
systemctl restart "${AUTHORITY_SERVICE_NAME}.service"
systemctl is-active --quiet "${AUTHORITY_SERVICE_NAME}.service"
CHAIN_RPC_PORT="${CHAIN_RPC_PORT}" CHAIN_P2P_PORT="${CHAIN_P2P_PORT}" AUTHORITY_PORT="${AUTHORITY_PORT}" \
	MEDIA_PORT="${MEDIA_PORT}" IPFS_API_PORT="${IPFS_API_PORT}" IPFS_GATEWAY_PORT="${IPFS_GATEWAY_PORT}" \
	"\${guard}" verify-authority

deployed_manifest_output="${remote_tmp_dir}/config/deployed-release-manifest-verification.txt"
"\${release_root}/operator/Eterra.Arcade.Authority.Operator" \
	verify-release-manifest \
	"\${release_root}/authority-release-manifest.json" \
	"\${release_root}/api" \
	"\${release_root}/operator" >"\${deployed_manifest_output}"
pid="\$(systemctl show --property MainPID --value "${AUTHORITY_SERVICE_NAME}.service")"
test "\${pid}" -gt 1
proc_executable="\$(readlink -f "/proc/\${pid}/exe")"
test "\${proc_executable}" = "\${release_root}/api/Eterra.Arcade.Authority.Api"
proc_sha="\$(shasum -a 256 "/proc/\${pid}/exe" | awk '{print \$1}')"
python3 -I -S - "${REMOTE_AUTHORITY_ENV_FILE}" "/proc/\${pid}/environ" <<'PY'
import pathlib
import sys

environment_path, process_environment_path = sys.argv[1:]
expected = {}
for line in pathlib.Path(environment_path).read_text(encoding="utf-8").splitlines():
    key, value = line.split("=", 1)
    if key in expected:
        raise SystemExit("duplicate installed authority environment key")
    expected[key] = value
observed = {}
for assignment in pathlib.Path(process_environment_path).read_bytes().split(b"\0"):
    if not assignment:
        continue
    key, value = assignment.decode("utf-8").split("=", 1)
    if key in observed:
        raise SystemExit("duplicate live authority environment key")
    observed[key] = value
for key, value in expected.items():
    if observed.get(key) != value:
        raise SystemExit(f"live authority environment drifted: {key}")
PY
nonce_hex="\$(python3 -c 'import secrets; print("0x" + secrets.token_hex(32))')"
liveness_file="${remote_tmp_dir}/config/liveness.json"
http_status="\$(curl --silent --show-error --output "\${liveness_file}" --write-out '%{http_code}' \
	--header 'content-type: application/json' \
	--data "{\"nonceHex\":\"\${nonce_hex}\"}" \
	"http://127.0.0.1:${AUTHORITY_PORT}/v1/authority/liveness-challenge")"
test "\${http_status}" = "200"
verify_sha="\$(shasum -a 256 "\${deployed_manifest_output}" | awk '{print \$1}')"

test ! -e "${remote_observation_root}"
test ! -L "${remote_observation_root}"
install -d -m 0700 -o root -g root "${remote_observation_root}"

python3 - "${remote_observation}" "\${liveness_file}" "\${nonce_hex}" "\${pid}" "\${proc_executable}" "\${proc_sha}" "\${verify_sha}" <<'PY'
import datetime
import grp
import hashlib
import json
import os
import pathlib
import pwd
import stat
import sys

def duplicate_rejecting(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise SystemExit("duplicate liveness field")
        value[key] = item
    return value

def file_observation(path):
    value = pathlib.Path(path)
    info = value.lstat()
    if not stat.S_ISREG(info.st_mode) or stat.S_ISLNK(info.st_mode):
        raise SystemExit("observed authority file is not regular")
    digest = hashlib.sha256(value.read_bytes()).hexdigest()
    return {
        "path": str(value),
        "sha256": digest,
        "mode": format(stat.S_IMODE(info.st_mode), "04o"),
        "owner": pwd.getpwuid(info.st_uid).pw_name + ":" + grp.getgrgid(info.st_gid).gr_name,
    }

output, liveness_path, nonce, pid, proc_executable, proc_sha, verify_sha = sys.argv[1:]
liveness = json.loads(pathlib.Path(liveness_path).read_text(encoding="utf-8"), object_pairs_hook=duplicate_rejecting)
journal_path = pathlib.Path("${REMOTE_LEGENDS_RESULT_JOURNAL_PATH}")
journal_info = journal_path.lstat()
if not stat.S_ISDIR(journal_info.st_mode) or stat.S_ISLNK(journal_info.st_mode):
    raise SystemExit("authority journal is not a non-symlink directory")
value = {
    "schemaVersion": 1,
    "kind": "nexus-v2-private-alpha-authority-deployment-observation",
    "releaseId": "${ETERRA_RELEASE_VERSION}",
    "candidateSha256": "${AUTHORITY_CANDIDATE_SHA256}",
    "releaseManifestSha256": "${AUTHORITY_RELEASE_MANIFEST_SHA256}",
    "chainSourceCommit": "${CHAIN_SOURCE_COMMIT}",
    "sdkgenSourceCommit": "${AUTHORITY_SOURCE_COMMIT}",
    "deploymentRoot": "${AUTHORITY_REMOTE_RELEASE_ROOT}",
    "serviceUnit": file_observation("${REMOTE_AUTHORITY_SERVICE_UNIT_FILE}"),
    "environment": file_observation("${REMOTE_AUTHORITY_ENV_FILE}"),
    "secrets": {
        "signerMnemonic": file_observation("${REMOTE_LEGENDS_SIGNER_MNEMONIC_FILE}"),
        "privateAlphaAccessKey": file_observation("${REMOTE_LEGENDS_PRIVATE_ALPHA_ACCESS_KEY_FILE}"),
        "signerDerivationPassword": (
            file_observation("${REMOTE_LEGENDS_SIGNER_DERIVATION_PASSWORD_FILE}")
            if pathlib.Path("${REMOTE_LEGENDS_SIGNER_DERIVATION_PASSWORD_FILE}").exists()
            else None
        ),
    },
    "process": {
        "serviceActive": True,
        "mainPid": int(pid),
        "user": "${DEPLOY_USER}",
        "executablePath": proc_executable,
        "procExecutableSha256": proc_sha,
        "listenerHost": "127.0.0.1",
        "listenerPort": ${AUTHORITY_PORT},
        "environmentMatched": True,
    },
    "catalog": file_observation("${AUTHORITY_REMOTE_RELEASE_ROOT}/api/catalog/eterra-legends.encounters.private-alpha.v1.json"),
    "manifestVerification": {
        "operatorCliPath": "${AUTHORITY_REMOTE_RELEASE_ROOT}/operator/Eterra.Arcade.Authority.Operator",
        "operatorCliSha256": hashlib.sha256(pathlib.Path("${AUTHORITY_REMOTE_RELEASE_ROOT}/operator/Eterra.Arcade.Authority.Operator").read_bytes()).hexdigest(),
        "stdoutSha256": verify_sha,
        "ok": True,
    },
    "journal": {
        "path": str(journal_path),
        "mode": format(stat.S_IMODE(journal_info.st_mode), "04o"),
        "owner": pwd.getpwuid(journal_info.st_uid).pw_name + ":" + grp.getgrgid(journal_info.st_gid).gr_name,
        "nonSymlinkDirectory": True,
    },
    "liveness": {"httpStatus": 200, "requestNonceHex": nonce, "response": liveness},
    "observedAtUtc": datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
}
destination = pathlib.Path(output)
descriptor = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o400)
with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
    json.dump(value, handle, indent=2, sort_keys=True)
    handle.write("\n")
PY
test "\$(stat -c '%a %U:%G' "${remote_observation_root}")" = "700 root:root"
test "\$(stat -c '%a %U:%G' "${remote_observation}")" = "400 root:root"
EOF

	observation_envelope="$(remote_root_bash <<EOF
set -euo pipefail
test -d "${remote_observation_root}"
test ! -L "${remote_observation_root}"
test "\$(stat -c '%a %U:%G' "${remote_observation_root}")" = "700 root:root"
test -f "${remote_observation}"
test ! -L "${remote_observation}"
test "\$(stat -c '%a %U:%G' "${remote_observation}")" = "400 root:root"
printf 'NEXUS_V2_AUTHORITY_OBSERVATION:%s:%s\n' \
	"\$(shasum -a 256 "${remote_observation}" | awk '{print \$1}')" \
	"\$(base64 <"${remote_observation}" | tr -d '\\r\\n')"
rm -rf -- "${remote_observation_root}"
EOF
)"
	observation_line="$(printf '%s\n' "${observation_envelope}" | grep '^NEXUS_V2_AUTHORITY_OBSERVATION:' || true)"
	[[ "$(printf '%s\n' "${observation_line}" | awk 'NF {count++} END {print count+0}')" -eq 1 ]] ||
		die "privileged authority observation stream was invalid"
	observation_payload="${observation_line#NEXUS_V2_AUTHORITY_OBSERVATION:}"
	remote_observation_sha256="${observation_payload%%:*}"
	observation_base64="${observation_payload#*:}"
	[[ "${remote_observation_sha256}" =~ ^[0-9a-f]{64}$ && ! -e "${local_observation}" && ! -L "${local_observation}" ]] ||
		die "privileged authority observation envelope was invalid"
	set -C
	exec 8>"${local_observation}"
	set +C
	chmod 0400 "${local_observation}"
	printf '%s' "${observation_base64}" | base64 -d >&8
	exec 8>&-
	[[ "$(shasum -a 256 "${local_observation}" | awk '{print $1}')" == "${remote_observation_sha256}" ]] ||
		die "privileged authority observation hash mismatch"
	python3 "${AUTHORITY_CANDIDATE_TOOL}" create-receipt \
		--candidate "${promotion_manifest}" \
		--expected-candidate-sha256 "${AUTHORITY_CANDIDATE_SHA256}" \
		--observation "${local_observation}" \
		--output "${deployment_evidence}" || die "authority deployment receipt validation failed"
	authority_receipt_sha256="$(shasum -a 256 "${deployment_evidence}" | awk '{print $1}')"
	rsync_to_remote_no_delete "${deployment_evidence}" "${remote_tmp_dir}/config/authority-deployment-receipt.json"
	remote_root_bash <<EOF
set -euo pipefail
test "\$(shasum -a 256 "${remote_tmp_dir}/config/authority-deployment-receipt.json" | awk '{print \$1}')" = "${authority_receipt_sha256}"
install -m 0440 -o root -g root "${remote_tmp_dir}/config/authority-deployment-receipt.json" "${REMOTE_AUTHORITY_DEPLOYMENT_RECEIPT_FILE}"
printf '%s\n' "${ETERRA_RELEASE_VERSION}" >"${REMOTE_RELEASE_VERSION_FILE}"
printf '%s\n' "${AUTHORITY_SOURCE_COMMIT}" >"${REMOTE_AUTHORITY_SOURCE_COMMIT_FILE}"
printf '%s\n' "${AUTHORITY_CANDIDATE_SHA256}" >"${REMOTE_AUTHORITY_CANDIDATE_SHA256_FILE}"
printf '%s\n' "${AUTHORITY_RELEASE_MANIFEST_SHA256}" >"${REMOTE_AUTHORITY_RELEASE_MANIFEST_SHA256_FILE}"
printf '%s\n' "${AUTHORITY_RELEASE_MANIFEST_SHA256}" >"${REMOTE_AUTHORITY_ARTIFACT_HASH_FILE}"
chown root:root \
	"${REMOTE_RELEASE_VERSION_FILE}" \
	"${REMOTE_AUTHORITY_SOURCE_COMMIT_FILE}" \
	"${REMOTE_AUTHORITY_CANDIDATE_SHA256_FILE}" \
	"${REMOTE_AUTHORITY_RELEASE_MANIFEST_SHA256_FILE}" \
	"${REMOTE_AUTHORITY_ARTIFACT_HASH_FILE}"
chmod 0440 \
	"${REMOTE_RELEASE_VERSION_FILE}" \
	"${REMOTE_AUTHORITY_SOURCE_COMMIT_FILE}" \
	"${REMOTE_AUTHORITY_CANDIDATE_SHA256_FILE}" \
	"${REMOTE_AUTHORITY_RELEASE_MANIFEST_SHA256_FILE}" \
	"${REMOTE_AUTHORITY_ARTIFACT_HASH_FILE}"
python3 - "${REMOTE_PHASE1_CLOSED_STATE_FILE}" <<'PY'
import datetime
import json
import os
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
value = json.loads(path.read_text(encoding="utf-8"))
value["legacyAuthorityLoopbackOnly"] = False
value["nexusV2AuthorityLoopbackOnly"] = True
value["protectedFirewallRulesClosedBeforeAuthorityStart"] = True
value["authoritySourceCommit"] = "${AUTHORITY_SOURCE_COMMIT}"
value["authorityCandidateSha256"] = "${AUTHORITY_CANDIDATE_SHA256}"
value["authorityReleaseManifestSha256"] = "${AUTHORITY_RELEASE_MANIFEST_SHA256}"
value["authorityDeploymentReceiptSha256"] = "${authority_receipt_sha256}"
value["phase1IngressGuardSha256"] = "${phase1_guard_sha256}"
value["updatedAtUtc"] = datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
temporary = path.with_suffix(".tmp")
temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
os.chmod(temporary, 0o440)
os.replace(temporary, path)
PY
case "${remote_tmp_dir}" in
	"${DEPLOY_ROOT}/tmp/arcade-authority-"[0-9a-f]*) rm -rf -- "${remote_tmp_dir}" ;;
	*) echo 'unsafe authority staging cleanup path' >&2; exit 1 ;;
esac
EOF
	log "Nexus V2 authority candidate promoted release=${ETERRA_RELEASE_VERSION} sdk_source=${AUTHORITY_SOURCE_COMMIT} candidate_sha256=${AUTHORITY_CANDIDATE_SHA256} release_manifest_sha256=${AUTHORITY_RELEASE_MANIFEST_SHA256} receipt_sha256=${authority_receipt_sha256}"
	exit 0
fi

DOTNET_BIN="${DOTNET_BIN:-/opt/homebrew/bin/dotnet}"
if [[ ! -x "${DOTNET_BIN}" ]]; then
	DOTNET_BIN="$(command -v dotnet || true)"
fi
[[ -n "${DOTNET_BIN}" && -x "${DOTNET_BIN}" ]] || die "dotnet CLI not found; set DOTNET_BIN"
[[ -d "${AUTHORITY_REPO_DIR}" ]] || die "authority SDK repo not found: ${AUTHORITY_REPO_DIR}"

bundle_dir="$(make_temp_dir)"
publish_api_dir="${bundle_dir}/api"
publish_operator_dir="${bundle_dir}/operator"
remote_tmp_dir="${DEPLOY_ROOT}/tmp/arcade-authority-deploy"
mkdir -p "${publish_api_dir}" "${publish_operator_dir}" "${bundle_dir}/secrets"
render_runtime_env_bundle "${bundle_dir}"
authority_env_sha256="$(shasum -a 256 "${bundle_dir}/arcade-authority.env" | awk '{print $1}')"

log "publishing authority API for ${AUTHORITY_RUNTIME_IDENTIFIER}"
"${DOTNET_BIN}" publish \
	"${AUTHORITY_REPO_DIR}/Eterra.Arcade.Authority.Api/Eterra.Arcade.Authority.Api.csproj" \
	-c Release \
	-f net6.0 \
	-r "${AUTHORITY_RUNTIME_IDENTIFIER}" \
	--self-contained "${AUTHORITY_PUBLISH_SELF_CONTAINED}" \
	-o "${publish_api_dir}"

log "publishing authority operator for ${AUTHORITY_RUNTIME_IDENTIFIER}"
"${DOTNET_BIN}" publish \
	"${AUTHORITY_REPO_DIR}/Eterra.Arcade.Authority.Operator/Eterra.Arcade.Authority.Operator.csproj" \
	-c Release \
	-f net6.0 \
	-r "${AUTHORITY_RUNTIME_IDENTIFIER}" \
	--self-contained "${AUTHORITY_PUBLISH_SELF_CONTAINED}" \
	-o "${publish_operator_dir}"

authority_artifact_hash="$(
	find "${publish_api_dir}" "${publish_operator_dir}" -type f -print0 |
		LC_ALL=C sort -z |
		xargs -0 shasum -a 256 |
		shasum -a 256 |
		awk '{print $1}'
)"

if [[ "${AUTHORITY_SUBMITTER_MODE}" == "live_alpha" ]]; then
	printf '%s\n' "$(read_secret_value "${AUTHORITY_RELAY_MNEMONIC}")" >"${bundle_dir}/secrets/nova-rail-relay.mnemonic"
	chmod 0600 "${bundle_dir}/secrets/nova-rail-relay.mnemonic"
fi
if [[ -n "${AUTHORITY_RELAY_DERIVATION_PASSWORD}" ]]; then
	printf '%s\n' "$(read_secret_value "${AUTHORITY_RELAY_DERIVATION_PASSWORD}")" >"${bundle_dir}/secrets/nova-rail-relay.derivation-password"
	chmod 0600 "${bundle_dir}/secrets/nova-rail-relay.derivation-password"
fi

remote_bash <<EOF
set -euo pipefail
mkdir -p "${remote_tmp_dir}" "${REMOTE_AUTHORITY_API_DIR}" "${REMOTE_AUTHORITY_OPERATOR_DIR}"
EOF

log "syncing authority publish output to ${SSH_TARGET}"
rsync_to_remote "${publish_api_dir}/" "${REMOTE_AUTHORITY_API_DIR}/"
rsync_to_remote "${publish_operator_dir}/" "${REMOTE_AUTHORITY_OPERATOR_DIR}/"
rsync_to_remote_no_delete "${bundle_dir}/arcade-authority.env" "${remote_tmp_dir}/arcade-authority.env"
rsync_to_remote_no_delete "${SCRIPT_DIR}/eterra-arcade-authority.service" "${remote_tmp_dir}/eterra-arcade-authority.service"
if [[ "${phase1_closed}" -eq 1 ]]; then
	rsync_to_remote_no_delete "${SCRIPT_DIR}/nexus-v2-phase1-closed-ingress.sh" "${remote_tmp_dir}/nexus-v2-phase1-closed-ingress.sh"
fi
if [[ -f "${bundle_dir}/secrets/nova-rail-relay.mnemonic" ]]; then
	rsync_to_remote_no_delete "${bundle_dir}/secrets/nova-rail-relay.mnemonic" "${remote_tmp_dir}/nova-rail-relay.mnemonic"
fi
if [[ -f "${bundle_dir}/secrets/nova-rail-relay.derivation-password" ]]; then
	rsync_to_remote_no_delete "${bundle_dir}/secrets/nova-rail-relay.derivation-password" "${remote_tmp_dir}/nova-rail-relay.derivation-password"
fi

remote_root_bash <<EOF
set -euo pipefail

install -d -m 0755 -o root -g root "${REMOTE_SHARED_ENV_DIR}"
mkdir -p "${REMOTE_SHARED_SECRET_DIR}" "${REMOTE_STATE_DIR}" "${REMOTE_AUTHORITY_API_DIR}" "${REMOTE_AUTHORITY_OPERATOR_DIR}"
install -m 0640 -o root -g "${DEPLOY_USER}" "${remote_tmp_dir}/arcade-authority.env" "${REMOTE_AUTHORITY_ENV_FILE}"
test "\$(shasum -a 256 "${REMOTE_AUTHORITY_ENV_FILE}" | awk '{print \$1}')" = "${authority_env_sha256}"
test "\$(stat -c '%a %U:%G' "${REMOTE_AUTHORITY_ENV_FILE}")" = "640 root:${DEPLOY_USER}"

if [[ -f "${remote_tmp_dir}/nova-rail-relay.mnemonic" ]]; then
	install -m 0640 "${remote_tmp_dir}/nova-rail-relay.mnemonic" "${REMOTE_AUTHORITY_RELAY_MNEMONIC_FILE}"
	chown root:"${DEPLOY_USER}" "${REMOTE_AUTHORITY_RELAY_MNEMONIC_FILE}"
fi
if [[ -f "${remote_tmp_dir}/nova-rail-relay.derivation-password" ]]; then
	install -m 0640 "${remote_tmp_dir}/nova-rail-relay.derivation-password" "${REMOTE_AUTHORITY_RELAY_DERIVATION_PASSWORD_FILE}"
	chown root:"${DEPLOY_USER}" "${REMOTE_AUTHORITY_RELAY_DERIVATION_PASSWORD_FILE}"
else
	rm -f "${REMOTE_AUTHORITY_RELAY_DERIVATION_PASSWORD_FILE}"
fi

install -m 0644 "${remote_tmp_dir}/eterra-arcade-authority.service" "${REMOTE_AUTHORITY_SERVICE_UNIT_FILE}"
chown root:root "${REMOTE_AUTHORITY_SERVICE_UNIT_FILE}"
chown -R "${DEPLOY_USER}:${DEPLOY_USER}" "${REMOTE_AUTHORITY_DIR}"
chmod 0755 "${REMOTE_AUTHORITY_API_DIR}/Eterra.Arcade.Authority.Api" "${REMOTE_AUTHORITY_OPERATOR_BIN}"

if [[ "${phase1_closed}" -eq 1 ]]; then
	test "\$(shasum -a 256 "${remote_tmp_dir}/nexus-v2-phase1-closed-ingress.sh" | awk '{print \$1}')" = "${phase1_guard_sha256}"
	chmod 0755 "${remote_tmp_dir}/nexus-v2-phase1-closed-ingress.sh"
	CHAIN_RPC_PORT="${CHAIN_RPC_PORT}" CHAIN_P2P_PORT="${CHAIN_P2P_PORT}" AUTHORITY_PORT="${AUTHORITY_PORT}" \
		MEDIA_PORT="${MEDIA_PORT}" IPFS_API_PORT="${IPFS_API_PORT}" IPFS_GATEWAY_PORT="${IPFS_GATEWAY_PORT}" \
		"${remote_tmp_dir}/nexus-v2-phase1-closed-ingress.sh" preclose
else
	ufw --force delete allow from "${LAN_CIDR}" to any port "${AUTHORITY_PORT}" proto tcp >/dev/null 2>&1 || true
	ufw allow from "${LAN_CIDR}" to any port "${AUTHORITY_PORT}" proto tcp comment 'eterra-alpha-arcade-authority' >/dev/null
	ufw --force delete allow from "${LAN_CIDR}" to any port "${CHAIN_RPC_PORT}" proto tcp >/dev/null 2>&1 || true
	ufw allow from "${LAN_CIDR}" to any port "${CHAIN_RPC_PORT}" proto tcp comment 'eterra-alpha-chain-rpc-lan-wallet' >/dev/null
fi

systemctl daemon-reload
systemctl enable "${AUTHORITY_SERVICE_NAME}.service"
systemctl restart "${AUTHORITY_SERVICE_NAME}.service"
systemctl is-active --quiet "${AUTHORITY_SERVICE_NAME}.service"
authority_pid="\$(systemctl show --property MainPID --value "${AUTHORITY_SERVICE_NAME}.service")"
test "\${authority_pid}" -gt 1
python3 -I -S - "${REMOTE_AUTHORITY_ENV_FILE}" "/proc/\${authority_pid}/environ" <<'PY'
import pathlib
import sys

environment_path, process_environment_path = sys.argv[1:]
expected = dict(line.split("=", 1) for line in pathlib.Path(environment_path).read_text(encoding="utf-8").splitlines())
observed = dict(
    assignment.decode("utf-8").split("=", 1)
    for assignment in pathlib.Path(process_environment_path).read_bytes().split(b"\0")
    if assignment
)
for key, value in expected.items():
    if observed.get(key) != value:
        raise SystemExit(f"live authority environment drifted: {key}")
PY
if [[ "${phase1_closed}" -eq 1 ]]; then
	CHAIN_RPC_PORT="${CHAIN_RPC_PORT}" CHAIN_P2P_PORT="${CHAIN_P2P_PORT}" AUTHORITY_PORT="${AUTHORITY_PORT}" \
		MEDIA_PORT="${MEDIA_PORT}" IPFS_API_PORT="${IPFS_API_PORT}" IPFS_GATEWAY_PORT="${IPFS_GATEWAY_PORT}" \
		"${remote_tmp_dir}/nexus-v2-phase1-closed-ingress.sh" verify-authority
	python3 - "${REMOTE_PHASE1_CLOSED_STATE_FILE}" <<'PY'
import datetime
import json
import os
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
value = json.loads(path.read_text(encoding="utf-8"))
if value.get("preResetClosureHandoffSha256") != "${PRE_RESET_CLOSURE_HANDOFF_SHA256}":
    raise SystemExit("Phase-1 closed-start marker closure handoff mismatch")
value["legacyAuthorityLoopbackOnly"] = True
value["protectedFirewallRulesClosedBeforeAuthorityStart"] = True
value["authoritySourceCommit"] = "${AUTHORITY_SOURCE_COMMIT}"
value["phase1IngressGuardSha256"] = "${phase1_guard_sha256}"
value["updatedAtUtc"] = datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
temporary = path.with_suffix(".tmp")
temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
os.chmod(temporary, 0o440)
os.replace(temporary, path)
PY
fi
systemctl --no-pager --full status "${AUTHORITY_SERVICE_NAME}.service" || true
printf '%s\n' "${ETERRA_RELEASE_VERSION}" >"${REMOTE_RELEASE_VERSION_FILE}"
printf '%s\n' "${AUTHORITY_SOURCE_COMMIT}" >"${REMOTE_AUTHORITY_SOURCE_COMMIT_FILE}"
printf '%s\n' "${authority_artifact_hash}" >"${REMOTE_AUTHORITY_ARTIFACT_HASH_FILE}"
chown root:root "${REMOTE_RELEASE_VERSION_FILE}" "${REMOTE_AUTHORITY_SOURCE_COMMIT_FILE}" "${REMOTE_AUTHORITY_ARTIFACT_HASH_FILE}"
rm -rf "${remote_tmp_dir}"
EOF

log "alpha arcade authority deploy complete release=${ETERRA_RELEASE_VERSION} source=${AUTHORITY_SOURCE_COMMIT} artifact_sha256=${authority_artifact_hash}"

if [[ "${authorize_after}" -eq 1 ]]; then
	"${SCRIPT_DIR}/authorize-arcade-authority.sh"
fi

if [[ "${seed_config_after}" -eq 1 ]]; then
	"${SCRIPT_DIR}/seed-nova-rail-config.sh"
fi
