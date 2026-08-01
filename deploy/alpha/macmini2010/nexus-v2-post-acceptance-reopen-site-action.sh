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
umask 077

# Root-only site-host action. It installs only the final-lock-pinned Caddyfile,
# verifies the exact restricted upstream path, and can atomically restore the
# Phase-1 read-only Caddyfile without touching databases or chain state.

[[ $# -ge 1 ]] || {
	printf 'post-acceptance-reopen-site-action: action is required\n' >&2
	exit 2
}
case "$1" in
	commit)
		[[ $# -eq 10 ]] || { printf 'post-acceptance-reopen-site-action: commit expects 10 arguments\n' >&2; exit 2; }
		;;
	open|verify|prepare-commit)
		[[ $# -eq 9 ]] || { printf 'post-acceptance-reopen-site-action: protected ingress action expects 9 arguments\n' >&2; exit 2; }
		;;
	*)
		[[ $# -eq 7 ]] || { printf 'post-acceptance-reopen-site-action: unsealed action expects 7 arguments\n' >&2; exit 2; }
		;;
esac

action="$1"
plan_base64="$2"
plan_sha256="$3"
driver_sha256="$4"
helper_sha256="$5"
normal_caddy_base64="$6"
phase1_caddy_base64="$7"
fps_adoption_seal_base64="${8:-}"
fps_adoption_seal_sha256="${9:-}"
site_prepare_result_base64="${10:-}"

die() {
	printf 'post-acceptance-reopen-site-action: %s\n' "$*" >&2
	exit 2
}

[[ "${action}" =~ ^(preflight|open|verify|prepare-commit|commit|close)$ ]] || die "invalid action"
[[ "${plan_sha256}" =~ ^[0-9a-f]{64}$ && "${driver_sha256}" =~ ^[0-9a-f]{64}$ ]] || die "invalid authorization hash"
[[ "${helper_sha256}" =~ ^[0-9a-f]{64}$ ]] || die "invalid helper hash"
[[ "$(sha256sum "$0" | awk '{print $1}')" == "${helper_sha256}" ]] || die "helper self-hash mismatch"

for command in base64 curl docker flock install jq nft python3 sha256sum ss stat systemctl timeout ufw xargs; do
	command -v "${command}" >/dev/null 2>&1 || die "missing required command: ${command}"
done

plan_path="$(mktemp /tmp/nexus-v2-reopen-plan.XXXXXX)"
normal_candidate="$(mktemp /tmp/nexus-v2-reopen-normal-caddy.XXXXXX)"
phase1_candidate="$(mktemp /tmp/nexus-v2-reopen-phase1-caddy.XXXXXX)"
site_prepare_result_path="$(mktemp /tmp/nexus-v2-site-prepare-result.XXXXXX)"
fps_adoption_seal_path="$(mktemp /tmp/nexus-v2-fps-adoption-seal.XXXXXX)"
cleanup_temporary() { rm -f "${plan_path}" "${normal_candidate}" "${phase1_candidate}" "${site_prepare_result_path}" "${fps_adoption_seal_path}"; }
trap cleanup_temporary EXIT
printf '%s' "${plan_base64}" | base64 -d >"${plan_path}" || die "cannot decode reopen plan"
printf '%s' "${normal_caddy_base64}" | base64 -d >"${normal_candidate}" || die "cannot decode normal Caddyfile"
printf '%s' "${phase1_caddy_base64}" | base64 -d >"${phase1_candidate}" || die "cannot decode Phase-1 Caddyfile"
[[ "$(sha256sum "${plan_path}" | awk '{print $1}')" == "${plan_sha256}" ]] || die "reopen plan hash mismatch"
if [[ "${action}" == commit ]]; then
	[[ -n "${site_prepare_result_base64}" ]] || die "site commit requires its durable prepare token"
	printf '%s' "${site_prepare_result_base64}" | base64 -d >"${site_prepare_result_path}" ||
		die "cannot decode site-ingress prepare token"
else
	[[ -z "${site_prepare_result_base64}" ]] || die "site-ingress prepare token is valid only for site commit"
fi
case "${action}" in
	open|verify|prepare-commit|commit)
		[[ "${fps_adoption_seal_sha256}" =~ ^[0-9a-f]{64}$ && -n "${fps_adoption_seal_base64}" ]] ||
			die "protected site-ingress action requires an FPS adoption seal"
		printf '%s' "${fps_adoption_seal_base64}" | base64 -d >"${fps_adoption_seal_path}" ||
			die "cannot decode FPS adoption seal"
		[[ "$(sha256sum "${fps_adoption_seal_path}" | awk '{print $1}')" == "${fps_adoption_seal_sha256}" ]] ||
			die "FPS adoption seal payload hash mismatch"
		;;
	*)
		[[ -z "${fps_adoption_seal_base64}" && -z "${fps_adoption_seal_sha256}" ]] ||
			die "FPS adoption seal is valid only for protected site-ingress actions"
		;;
esac

jq -e '
  .schemaVersion == 1 and
  .kind == "nexus-v2-private-alpha-post-acceptance-reopen-plan" and
  .ports == {authority:8787,chainP2p:30333,chainRpc:9944,ipfsApi:5001,ipfsGateway:8080,media:4000,siteHttp:80,siteHttps:443} and
  .policy.privateAlphaOnly == true and .policy.sourceRestrictedToSiteHost == true and
  .policy.chainStateMutationAuthorized == false and .policy.chainStateRollbackAuthorized == false and
  .policy.paidOrPublicProductionActivationAuthorized == false and
  .policy.forbiddenExposedPorts == [30333,5001]
' "${plan_path}" >/dev/null || die "reopen plan policy mismatch"

operation_id="$(jq -er '.operationId | select(test("^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$"))' "${plan_path}")"
release_id="$(jq -er '.releaseId | select(test("^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$"))' "${plan_path}")"
site_release_version="$(jq -er '.siteReleaseVersion | select(test("^v[A-Za-z0-9][A-Za-z0-9._-]{0,126}$"))' "${plan_path}")"
source_commit="$(jq -er '.sourceCommit | select(test("^[0-9a-f]{40}$"))' "${plan_path}")"
site_source_commit="$(jq -er '.siteSourceCommit | select(test("^[0-9a-f]{40}$"))' "${plan_path}")"
genesis_hash="$(jq -er '.genesisHash | select(test("^0x[0-9a-f]{64}$"))' "${plan_path}")"
chain_ip="$(jq -er '.network.chainLanIp' "${plan_path}")"
site_ip="$(jq -er '.network.siteLanIp' "${plan_path}")"
public_hostname="$(jq -er '.network.publicHostname | select(test("^[A-Za-z0-9][A-Za-z0-9.-]{0,252}$"))' "${plan_path}")"
final_lock_sha256="$(jq -er '.finalReleaseLock.sha256 | select(test("^[0-9a-f]{64}$"))' "${plan_path}")"
receipt_sha256="$(jq -er '.acceptanceBoundaryReceipt.sha256 | select(test("^[0-9a-f]{64}$"))' "${plan_path}")"
seal_sha256="$(jq -er '.phase2FinalSeal.sha256 | select(test("^[0-9a-f]{64}$"))' "${plan_path}")"
plan_created_at="$(jq -er '.createdAtUtc | select(test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$"))' "${plan_path}")"
plan_expires_at="$(jq -er '.expiresAtUtc | select(test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$"))' "${plan_path}")"
fps_candidate_manifest_sha256="$(jq -er '.unityFpsCandidateManifest.sha256 | select(test("^[0-9a-f]{64}$"))' "${plan_path}")"
fps_deployment_environment_sha256="$(jq -er '.unityFpsDeploymentEnvironment.sha256 | select(test("^[0-9a-f]{64}$"))' "${plan_path}")"
normal_caddy_sha256="$(jq -er '.caddyfiles.normal.sha256 | select(test("^[0-9a-f]{64}$"))' "${plan_path}")"
phase1_caddy_sha256="$(jq -er '.caddyfiles.phase1.sha256 | select(test("^[0-9a-f]{64}$"))' "${plan_path}")"
expected_driver_sha256="$(jq -er '.drivers["site-ingress"].sha256' "${plan_path}")"
expected_helper_sha256="$(jq -er '.helpers["site-ingress"].sha256' "${plan_path}")"
expected_site_driver_sha256="${expected_driver_sha256}"
if [[ "${action}" == close ]]; then
	emergency_driver_sha256="$(jq -er '.emergencyClosure.driver.sha256' "${plan_path}")"
	emergency_helper_sha256="$(jq -er '.emergencyClosure.helpers["site-ingress"].sha256' "${plan_path}")"
	[[ "${driver_sha256}" == "${expected_driver_sha256}" || "${driver_sha256}" == "${emergency_driver_sha256}" ]] || die "closure driver plan pin mismatch"
	[[ "${helper_sha256}" == "${expected_helper_sha256}" || "${helper_sha256}" == "${emergency_helper_sha256}" ]] || die "closure helper plan pin mismatch"
else
	[[ "${driver_sha256}" == "${expected_driver_sha256}" && "${helper_sha256}" == "${expected_helper_sha256}" ]] || die "driver/helper plan pin mismatch"
fi
[[ "$(sha256sum "${normal_candidate}" | awk '{print $1}')" == "${normal_caddy_sha256}" ]] || die "normal Caddyfile payload mismatch"
[[ "$(sha256sum "${phase1_candidate}" | awk '{print $1}')" == "${phase1_caddy_sha256}" ]] || die "Phase-1 Caddyfile payload mismatch"
grep -q 'AllExternalWriteIngressClosed' "${phase1_candidate}" || die "Phase-1 Caddyfile sentinel missing"
grep -q 'Phase-1 public RPC ingress closed' "${phase1_candidate}" || die "Phase-1 RPC denial missing"
! grep -q 'AllExternalWriteIngressClosed' "${normal_candidate}" || die "normal Caddyfile remains Phase-1 closed"

validate_fps_adoption_seal_file() {
	local path="$1"
	python3 -I -S - "${path}" "${fps_adoption_seal_sha256}" "${operation_id}" \
		"${plan_sha256}" "${release_id}" "${final_lock_sha256}" \
		"${fps_candidate_manifest_sha256}" "${fps_deployment_environment_sha256}" \
		"${plan_created_at}" "${plan_expires_at}" <<'PY' || return 1
import datetime as dt
import hashlib
import json
import pathlib
import re
import sys

(
    path_value, expected_sha, operation_id, plan_sha, release_id, final_lock_sha,
    candidate_sha, environment_sha, plan_created, plan_expires,
) = sys.argv[1:]
path = pathlib.Path(path_value)
payload = path.read_bytes()
if not 0 < len(payload) <= 1024 * 1024 or hashlib.sha256(payload).hexdigest() != expected_sha:
    raise SystemExit("FPS adoption seal bytes differ from the invocation pin")
def unique(pairs):
    value = {}
    for key, item in pairs:
        if key in value:
            raise ValueError(f"duplicate field: {key}")
        value[key] = item
    return value
value = json.loads(payload, object_pairs_hook=unique)
expected_keys = {
    "schemaVersion", "kind", "operationId", "planSha256", "releaseId",
    "finalReleaseLockSha256", "candidateManifestSha256",
    "deploymentEnvironmentSha256", "deploymentReceipt", "promoteResult",
    "verifyResult", "paidOrPublicProductionActivationAuthorized",
    "capturedAtUtc", "expiresAtUtc",
}
if not isinstance(value, dict) or set(value) != expected_keys:
    raise SystemExit("FPS adoption seal schema mismatch")
canonical = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()
if payload != canonical:
    raise SystemExit("FPS adoption seal is not canonical JSON")
if not (
    value["schemaVersion"] == 1
    and value["kind"] == "nexus-v2-private-alpha-post-fps-deployment-seal"
    and value["operationId"] == operation_id
    and value["planSha256"] == plan_sha
    and value["releaseId"] == release_id
    and value["finalReleaseLockSha256"] == final_lock_sha
    and value["candidateManifestSha256"] == candidate_sha
    and value["deploymentEnvironmentSha256"] == environment_sha
    and value["paidOrPublicProductionActivationAuthorized"] is False
    and value["expiresAtUtc"] == plan_expires
):
    raise SystemExit("FPS adoption seal authority mismatch")
sha = re.compile(r"^(?!0{64}$)[0-9a-f]{64}$")
def pin(item):
    return (
        isinstance(item, dict) and set(item) == {"path", "sha256"}
        and isinstance(item["path"], str) and item["path"].startswith("/")
        and sha.fullmatch(str(item["sha256"])) is not None
    )
if not pin(value["deploymentReceipt"]) or not pin(value["verifyResult"]):
    raise SystemExit("FPS adoption seal receipt/verification pin mismatch")
if value["promoteResult"] is not None and not pin(value["promoteResult"]):
    raise SystemExit("FPS adoption seal promotion pin mismatch")
parse = lambda item: dt.datetime.strptime(item, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=dt.timezone.utc)
created = parse(plan_created)
captured = parse(value["capturedAtUtc"])
expires = parse(value["expiresAtUtc"])
now = dt.datetime.now(dt.timezone.utc)
if not created <= captured <= expires or captured > now + dt.timedelta(seconds=30) or now > expires:
    raise SystemExit("FPS adoption seal is stale or from the future")
PY
}

if [[ -n "${fps_adoption_seal_sha256}" ]]; then
	validate_fps_adoption_seal_file "${fps_adoption_seal_path}" ||
		die "FPS adoption seal content validation failed"
fi

site_prepare_result_sha256=""
require_site_prepare_token() {
	[[ -s "${site_prepare_result_path}" && ! -L "${site_prepare_result_path}" ]] ||
		die "site-ingress prepare token is unavailable"
	jq -e \
		--arg operationId "${operation_id}" \
		--arg planSha256 "${plan_sha256}" \
		--arg releaseId "${release_id}" \
		--arg siteReleaseVersion "${site_release_version}" \
		--arg sourceCommit "${source_commit}" \
		--arg siteSourceCommit "${site_source_commit}" \
		--arg finalReleaseLockSha256 "${final_lock_sha256}" \
		--arg acceptanceBoundaryReceiptSha256 "${receipt_sha256}" \
		--arg phase2FinalSealSha256 "${seal_sha256}" \
		--arg fpsAdoptionSealSha256 "${fps_adoption_seal_sha256}" \
		--arg driverSha256 "${expected_site_driver_sha256}" '
		keys == ["acceptanceBoundaryReceiptSha256","action","alreadyApplied","checks","componentReceipt",
		         "completedAtUtc","componentId","driverSha256","finalReleaseLockSha256",
		         "fpsAdoptionSealSha256","kind","mode","mutationPerformed","operationId","phase2FinalSealSha256",
		         "planSha256","releaseId","remoteMarkerSha256","result","schemaVersion",
		         "siteReleaseVersion","siteSourceCommit","sourceCommit"] and
		.schemaVersion == 1 and
		.kind == "nexus-v2-private-alpha-post-acceptance-reopen-component-result" and
		.operationId == $operationId and .planSha256 == $planSha256 and
		.releaseId == $releaseId and .siteReleaseVersion == $siteReleaseVersion and
		.sourceCommit == $sourceCommit and .siteSourceCommit == $siteSourceCommit and
		.componentId == "site-ingress" and .action == "prepare-commit" and .mode == "execute" and
		.result == "passed" and .mutationPerformed == true and
		.componentReceipt == null and
		(.alreadyApplied | type == "boolean") and
		.finalReleaseLockSha256 == $finalReleaseLockSha256 and
		.acceptanceBoundaryReceiptSha256 == $acceptanceBoundaryReceiptSha256 and
		.phase2FinalSealSha256 == $phase2FinalSealSha256 and
		.fpsAdoptionSealSha256 == $fpsAdoptionSealSha256 and
		.driverSha256 == $driverSha256 and
		(.remoteMarkerSha256 | test("^[0-9a-f]{64}$")) and
		.checks == {authorityStatusesSafe:true,coordinatorWatchdogArmed:true,fpsAdoptionSealPinned:true,
		 deploymentIdentityExact:true,currentRuntimeAuthorityVerified:true,
		 restrictedIngressVerified:true} and
		(.completedAtUtc | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$"))
	' "${site_prepare_result_path}" >/dev/null || die "site-ingress prepare token contract mismatch"
	site_prepare_result_sha256="$(sha256sum "${site_prepare_result_path}" | awk '{print $1}')"
}

DEPLOY_ROOT="/opt/eterra-alpha"
REMOTE_SITE_DIR="${DEPLOY_ROOT}/site/current"
REMOTE_ENV_FILE="${DEPLOY_ROOT}/shared/env/site.env"
REMOTE_COMPOSE_FILE="${REMOTE_SITE_DIR}/deploy/alpha/macmini2014/docker-compose.yaml"
REMOTE_CADDYFILE="${REMOTE_SITE_DIR}/deploy/alpha/macmini2014/Caddyfile"
RUNTIME_CONFIG_NORMALIZER="${REMOTE_SITE_DIR}/scripts/release/nexus_v2_docker_runtime_config.py"
RUNTIME_CONFIG_NORMALIZER_SHA256="$(jq -er '.siteRuntimeConfigNormalizer.sha256 | select(test("^[0-9a-f]{64}$"))' "${plan_path}")"
PROJECT_NAME="eterra-tcg-site-alpha"
STATE_ROOT="${DEPLOY_ROOT}/shared/post-acceptance-reopen/${operation_id}/site-ingress"
LOCK_FILE="/run/lock/nexus-v2-post-acceptance-reopen-site-ingress.lock"
exec 9>"${LOCK_FILE}"
flock -x -w 180 9 || die "could not acquire the site-ingress operation lock"
OPEN_MARKER="${STATE_ROOT}/open.json"
CLOSED_MARKER="${STATE_ROOT}/closed.json"
COMMITTED_MARKER="${STATE_ROOT}/committed.json"
PREPARED_MARKER="${STATE_ROOT}/prepared.json"
FPS_ADOPTION_SEAL_FILE="${STATE_ROOT}/fps-adoption-seal.json"
ANOMALY_DIR="${STATE_ROOT}/anomalies"
RETAINED_PHASE1="${STATE_ROOT}/phase1-readonly.Caddyfile"
RETAINED_NORMAL="${STATE_ROOT}/normal.Caddyfile"
WATCHDOG_SCRIPT="${STATE_ROOT}/watchdog-close.sh"
WATCHDOG_HELPER="${STATE_ROOT}/watchdog-helper"
WATCHDOG_PLAN="${STATE_ROOT}/watchdog-plan.json"
WATCHDOG_NORMAL_CADDY="${STATE_ROOT}/watchdog-normal.Caddyfile"
WATCHDOG_PHASE1_CADDY="${STATE_ROOT}/watchdog-phase1.Caddyfile"
WATCHDOG_MANIFEST="${STATE_ROOT}/watchdog-manifest.json"
WATCHDOG_PREFIX="eterra-alpha-restricted-reopen-${operation_id}-site"
WATCHDOG_SERVICE="${WATCHDOG_PREFIX}-watchdog.service"
WATCHDOG_TIMER="${WATCHDOG_PREFIX}-watchdog.timer"
BOOT_GUARD_SCRIPT="${STATE_ROOT}/boot-fail-closed.sh"
BOOT_GUARD_PHASE1="${STATE_ROOT}/boot-phase1-readonly.Caddyfile"
BOOT_GUARD_MANIFEST="${STATE_ROOT}/boot-fail-closed-manifest.json"
BOOT_GUARD_SERVICE="${WATCHDOG_PREFIX}-boot-fail-closed.service"
SITE_CLOSE_GUARD_TABLE="eterra_nexus_v2_site_emergency_close"
RELEASE_FILE="${DEPLOY_ROOT}/shared/state/release-version.txt"
SOURCE_FILE="${DEPLOY_ROOT}/shared/state/site-source-commit.txt"

require_fps_adoption_seal_exact() {
	[[ "${fps_adoption_seal_sha256}" =~ ^[0-9a-f]{64}$ ]] || die "FPS adoption seal pin is unavailable"
	[[ -f "${FPS_ADOPTION_SEAL_FILE}" && ! -L "${FPS_ADOPTION_SEAL_FILE}" ]] || die "retained FPS adoption seal is unavailable"
	[[ "$(stat -c '%a %U:%G' "${FPS_ADOPTION_SEAL_FILE}")" == "400 root:root" ]] || die "retained FPS adoption seal owner/mode drifted"
	[[ "$(sha256sum "${FPS_ADOPTION_SEAL_FILE}" | awk '{print $1}')" == "${fps_adoption_seal_sha256}" ]] || die "retained FPS adoption seal hash drifted"
	validate_fps_adoption_seal_file "${FPS_ADOPTION_SEAL_FILE}" || die "retained FPS adoption seal content drifted"
}

retain_fps_adoption_seal() {
	mkdir -p "${STATE_ROOT}"
	chmod 0700 "${STATE_ROOT}"
	if [[ -e "${FPS_ADOPTION_SEAL_FILE}" || -L "${FPS_ADOPTION_SEAL_FILE}" ]]; then
		require_fps_adoption_seal_exact
		return
	fi
	install -o root -g root -m 0400 "${fps_adoption_seal_path}" "${FPS_ADOPTION_SEAL_FILE}.pending"
	mv -T "${FPS_ADOPTION_SEAL_FILE}.pending" "${FPS_ADOPTION_SEAL_FILE}"
	require_fps_adoption_seal_exact
}

compose=(docker compose --project-name "${PROJECT_NAME}" -f "${REMOTE_COMPOSE_FILE}" --env-file "${REMOTE_ENV_FILE}")

emit_result() {
	local already_applied="$1"
	local mutation_performed="$2"
	local marker_sha256="$3"
	local checks_json="$4"
	local payload
	payload="$(jq -cn --sort-keys \
		--arg operationId "${operation_id}" --arg planSha256 "${plan_sha256}" \
		--arg releaseId "${release_id}" --arg siteReleaseVersion "${site_release_version}" \
		--arg sourceCommit "${source_commit}" \
		--arg siteSourceCommit "${site_source_commit}" --arg action "${action}" \
		--arg finalReleaseLockSha256 "${final_lock_sha256}" \
		--arg acceptanceBoundaryReceiptSha256 "${receipt_sha256}" \
		--arg phase2FinalSealSha256 "${seal_sha256}" --arg driverSha256 "${driver_sha256}" \
		--arg fpsAdoptionSealSha256 "${fps_adoption_seal_sha256}" \
		--arg remoteMarkerSha256 "${marker_sha256}" --arg completedAtUtc "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
		--argjson alreadyApplied "${already_applied}" --argjson mutationPerformed "${mutation_performed}" \
		--argjson checks "${checks_json}" \
		'{schemaVersion:1,kind:"nexus-v2-private-alpha-post-acceptance-reopen-component-result",
		  operationId:$operationId,planSha256:$planSha256,releaseId:$releaseId,
		  siteReleaseVersion:$siteReleaseVersion,
		  sourceCommit:$sourceCommit,siteSourceCommit:$siteSourceCommit,componentId:"site-ingress",
		  action:$action,mode:"execute",result:"passed",mutationPerformed:$mutationPerformed,
		  alreadyApplied:$alreadyApplied,finalReleaseLockSha256:$finalReleaseLockSha256,
		  acceptanceBoundaryReceiptSha256:$acceptanceBoundaryReceiptSha256,
		  phase2FinalSealSha256:$phase2FinalSealSha256,
		  fpsAdoptionSealSha256:(if $fpsAdoptionSealSha256 == "" then null else $fpsAdoptionSealSha256 end),
		  driverSha256:$driverSha256,
		  remoteMarkerSha256:$remoteMarkerSha256,componentReceipt:null,
		  checks:$checks,completedAtUtc:$completedAtUtc}')"
	printf 'NEXUS_V2_REOPEN_RESULT:%s\n' "$(printf '%s\n' "${payload}" | base64 | tr -d '\n')"
}

service_running() {
	"${compose[@]}" ps --status running --services 2>/dev/null | grep -qx "$1"
}

service_id() {
	local service="$1"
	local container
	container="$("${compose[@]}" ps -q "${service}" 2>/dev/null || true)"
	if [[ -z "${container}" ]]; then
		container="$(docker ps -q \
			--filter "label=com.docker.compose.project=${PROJECT_NAME}" \
			--filter "label=com.docker.compose.service=${service}" | sed -n '1p')"
	fi
	printf '%s\n' "${container}"
}

caddy_id() {
	service_id caddy
}

read_site_env_value() {
	local key="$1"
	python3 - "${REMOTE_ENV_FILE}" "${key}" <<'PY'
import pathlib
import re
import sys

path, wanted = sys.argv[1:]
observed = {}
for number, raw in enumerate(pathlib.Path(path).read_text(encoding="utf-8").splitlines(), 1):
    line = raw.strip()
    if not line or line.startswith("#"):
        continue
    if "=" not in line:
        raise SystemExit(f"invalid env line {number}")
    key, value = line.split("=", 1)
    if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", key) is None or key in observed:
        raise SystemExit(f"invalid or duplicate env key on line {number}")
    observed[key] = value
if wanted not in observed or observed[wanted] == "":
    raise SystemExit(f"required env key is unavailable: {wanted}")
sys.stdout.write(observed[wanted])
PY
}

require_identity() {
	[[ -r "${RELEASE_FILE}" && "$(cat "${RELEASE_FILE}")" == "${site_release_version}" ]] || die "deployed site release identity mismatch"
	[[ -r "${SOURCE_FILE}" && "$(cat "${SOURCE_FILE}")" == "${site_source_commit}" ]] || die "deployed site source identity mismatch"
	[[ -r "${REMOTE_ENV_FILE}" && ! -L "${REMOTE_ENV_FILE}" ]] || die "site environment is unavailable"
	[[ -r "${REMOTE_COMPOSE_FILE}" && ! -L "${REMOTE_COMPOSE_FILE}" ]] || die "site Compose file is unavailable"
	[[ -r "${REMOTE_CADDYFILE}" && ! -L "${REMOTE_CADDYFILE}" ]] || die "active Caddyfile is unavailable"
	python3 - "${REMOTE_ENV_FILE}" "${site_release_version}" "${site_source_commit}" "${chain_ip}" <<'PY' ||
		die "site runtime environment safety projection is ambiguous or drifted"
import pathlib
import re
import sys

path, release, source, chain_ip = sys.argv[1:]
required = {
    "RELEASE_VERSION": release,
    "SOURCE_COMMIT": source,
    "PUBLIC_MEDIA_UPLOAD_ENABLED": "false",
    "PUBLIC_AVATAR_UPLOAD_ENABLED": "false",
    "NEXUS_V2_SESSION_AUTHORIZATION_PRODUCTION_ENABLED": "false",
    "CHAIN_UPSTREAM_HOST": chain_ip,
    "AUTHORITY_UPSTREAM_HOST": chain_ip,
    "MEDIA_UPSTREAM_HOST": chain_ip,
    "IPFS_UPSTREAM_HOST": chain_ip,
    "CHAIN_RPC_PORT": "9944",
    "AUTHORITY_PORT": "8787",
    "MEDIA_PORT": "4000",
    "IPFS_GATEWAY_PORT": "8080",
    "INDEXER_API_PORT": "8787",
    "INDEXER_CHAIN_WS_URL": f"ws://{chain_ip}:9944",
    "NEXUS_V2_FULL_LOOP_ACCEPTANCE_READS_ENABLED": "false",
    "NEXUS_V2_FULL_LOOP_ACCEPTANCE_PROJECTION_DIRECTORY": "/var/lib/eterra/full-loop",
    "NEXUS_V2_FULL_LOOP_ACCEPTANCE_TARGET_JSON": "",
}
observed = {}
for number, raw in enumerate(pathlib.Path(path).read_text(encoding="utf-8").splitlines(), 1):
    line = raw.strip()
    if not line or line.startswith("#"):
        continue
    if "=" not in line:
        raise SystemExit(f"invalid env line {number}")
    key, value = line.split("=", 1)
    if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", key) is None:
        raise SystemExit(f"invalid env key on line {number}")
    if key in observed:
        raise SystemExit(f"duplicate env assignment: {key}")
    observed[key] = value
for key, expected in required.items():
    if observed.get(key) != expected:
        raise SystemExit(f"unsafe env assignment: {key}")
PY
	for service in site indexer-api mongo caddy; do service_running "${service}" || die "required site service is not running: ${service}"; done
}

container_publications() {
	local container="$1"
	docker inspect --format '{{json .NetworkSettings.Ports}}' "${container}" | jq -c '
	  [to_entries[] as $entry |
	   ($entry.key | split("/")) as $target |
	   ($entry.value // [])[] |
	   "\(.HostIp):\(.HostPort):\($target[0])/\($target[1])"] | sort'
}

require_deployment_identity() {
	local expected_compose service container expected_id expected_ref actual_id actual_ref actual_publications expected_publications
	local expected_runtime_hash expected_resolved_hash expected_config_hash actual_runtime_hash actual_resolved_hash actual_config_hash
	local inspect_path image_inspect_path proof_path resolved_compose compose_hashes temporary_root source_contract
	local -a project_containers expected_containers normalizer_args
	expected_compose="$(jq -er '.siteDeploymentIdentity.composeFileSha256' "${plan_path}")"
	[[ "$(sha256sum "${REMOTE_COMPOSE_FILE}" | awk '{print $1}')" == "${expected_compose}" ]] || die "deployed Compose bytes drifted"
	[[ -f "${RUNTIME_CONFIG_NORMALIZER}" && ! -L "${RUNTIME_CONFIG_NORMALIZER}" ]] ||
		die "runtime configuration normalizer is unavailable"
	[[ "$(sha256sum "${RUNTIME_CONFIG_NORMALIZER}" | awk '{print $1}')" == "${RUNTIME_CONFIG_NORMALIZER_SHA256}" ]] ||
		die "runtime configuration normalizer source drifted"
	source_contract="$(jq -cS '.siteDeploymentIdentity.sourceContract' "${plan_path}")"
	jq -e \
		--arg composeSha256 "${expected_compose}" \
		--arg candidateManifestSha256 "$(jq -er '.siteDeploymentCandidateManifest.sha256' "${plan_path}")" \
		--arg phase1PostDeployIdentitySha256 "$(jq -er '.sitePhase1PostDeployIdentity.sha256' "${plan_path}")" \
		--arg runtimeNormalizerSha256 "${RUNTIME_CONFIG_NORMALIZER_SHA256}" \
		--arg fullLoopActivationReceiptSha256 "$(jq -er '.fullLoopIndexerActivationReceipt.sha256' "${plan_path}")" \
		--arg fullLoopActivationOverrideSha256 "$(jq -er '.indexerReadiness.activationOverrideSha256' "${plan_path}")" \
		--arg fullLoopProjectionManifestSha256 "$(jq -er '.indexerReadiness.projectionManifestSha256' "${plan_path}")" \
		--arg fullLoopActivationVerifierSha256 "$(jq -er '.siteDeploymentIdentity.sourceContract.fullLoopActivationVerifierSha256' "${plan_path}")" '
	  keys == ["candidateManifestSha256","composeSha256",
	           "fullLoopActivationOverrideSha256","fullLoopActivationReceiptSha256",
	           "fullLoopActivationVerifierSha256","fullLoopProjectionManifestSha256",
	           "phase1PostDeployIdentitySha256","runtimeNormalizerSha256"] and
	  .composeSha256 == $composeSha256 and
	  .candidateManifestSha256 == $candidateManifestSha256 and
	  .phase1PostDeployIdentitySha256 == $phase1PostDeployIdentitySha256 and
	  .runtimeNormalizerSha256 == $runtimeNormalizerSha256 and
	  .fullLoopActivationReceiptSha256 == $fullLoopActivationReceiptSha256 and
	  .fullLoopActivationOverrideSha256 == $fullLoopActivationOverrideSha256 and
	  .fullLoopProjectionManifestSha256 == $fullLoopProjectionManifestSha256 and
	  .fullLoopActivationVerifierSha256 == $fullLoopActivationVerifierSha256 and
	  (.candidateManifestSha256 | test("^[0-9a-f]{64}$"))
	' <<<"${source_contract}" >/dev/null || die "deployment source contract drifted"
	temporary_root="$(mktemp -d /tmp/nexus-v2-reopen-compose-proof.XXXXXX)"
	resolved_compose="${temporary_root}/resolved-compose.json"
	compose_hashes="${temporary_root}/compose-hashes.txt"
	"${compose[@]}" config --format json --no-env-resolution --no-path-resolution \
		>"${resolved_compose}" || {
		rm -rf "${temporary_root}"
		die "resolved Compose model is unavailable"
	}
	"${compose[@]}" config --hash '*' >"${compose_hashes}" || {
		rm -rf "${temporary_root}"
		die "effective Compose service hashes are unavailable"
	}
	[[ "$(awk '{print $1}' "${compose_hashes}" | LC_ALL=C sort -u | tr '\n' ' ')" == \
		"caddy indexer-api mongo site " ]] || {
		rm -rf "${temporary_root}"
		die "effective Compose service set drifted"
	}
	mapfile -t project_containers < <(
		docker ps --no-trunc -q \
			--filter "label=com.docker.compose.project=${PROJECT_NAME}" |
			LC_ALL=C sort -u
	)
	[[ "${#project_containers[@]}" -eq 4 ]] || die "running Compose project container count drifted"
	expected_containers=()
	for service in site indexer-api mongo caddy; do
		container="$(service_id "${service}")"
		[[ -n "${container}" ]] || die "deployment identity container missing: ${service}"
		expected_containers+=("${container}")
		expected_id="$(jq -er --arg service "${service}" '.siteDeploymentIdentity.images[] | select(.service == $service) | .imageId' "${plan_path}")"
		expected_ref="$(jq -er --arg service "${service}" '.siteDeploymentIdentity.images[] | select(.service == $service) | .reference' "${plan_path}")"
		actual_id="$(docker inspect --format '{{.Image}}' "${container}")"
		actual_ref="$(docker inspect --format '{{.Config.Image}}' "${container}")"
		[[ "${actual_id}" == "${expected_id}" && "${actual_ref}" == "${expected_ref}" ]] || die "deployed image identity drifted: ${service}"
		actual_publications="$(container_publications "${container}")"
		expected_publications="$(jq -c --arg service "${service}" '.siteDeploymentIdentity.publications[$service] | sort' "${plan_path}")"
		[[ "${actual_publications}" == "${expected_publications}" ]] || die "container publication drifted: ${service}"
		expected_runtime_hash="$(jq -er --arg service "${service}" '.siteDeploymentIdentity.images[] | select(.service == $service) | .runtimeConfigSha256' "${plan_path}")"
		expected_resolved_hash="$(jq -er --arg service "${service}" '.siteDeploymentIdentity.images[] | select(.service == $service) | .resolvedComposeServiceSha256' "${plan_path}")"
		expected_config_hash="$(jq -er --arg service "${service}" '.siteDeploymentIdentity.images[] | select(.service == $service) | .composeServiceConfigHash' "${plan_path}")"
		actual_config_hash="$(awk -v service="${service}" '$1 == service {print $2}' "${compose_hashes}")"
		[[ "${actual_config_hash}" == "${expected_config_hash}" ]] || {
			rm -rf "${temporary_root}"
			die "Compose service hash drifted: ${service}"
		}
		inspect_path="${temporary_root}/inspect-${service}.json"
		image_inspect_path="${temporary_root}/image-inspect-${service}.json"
		proof_path="${temporary_root}/proof-${service}.json"
		docker inspect "${container}" >"${inspect_path}"
		docker image inspect "${expected_ref}" >"${image_inspect_path}"
		normalizer_args=(
			verify-compose
			--inspect "${inspect_path}"
			--image-inspect "${image_inspect_path}"
			--resolved-compose "${resolved_compose}"
			--environment-file "${REMOTE_ENV_FILE}"
			--service "${service}"
			--project "${PROJECT_NAME}"
			--expected-image-ref "${expected_ref}"
			--expected-image-id "${expected_id}"
			--compose-service-config-hash "${expected_config_hash}"
		)
		if [[ "${service}" != mongo ]]; then normalizer_args+=(--require-safety-flags); fi
		python3 "${RUNTIME_CONFIG_NORMALIZER}" "${normalizer_args[@]}" >"${proof_path}" || {
			rm -rf "${temporary_root}"
			die "container runtime configuration is invalid: ${service}"
		}
		actual_runtime_hash="$(jq -er '.runtimeConfigSha256' "${proof_path}")"
		actual_resolved_hash="$(jq -er '.resolvedComposeServiceSha256' "${proof_path}")"
		[[ "${actual_runtime_hash}" == "${expected_runtime_hash}" &&
			"${actual_resolved_hash}" == "${expected_resolved_hash}" ]] || {
			rm -rf "${temporary_root}"
			die "container/Compose configuration drifted: ${service}"
		}
	done
	mapfile -t expected_containers < <(printf '%s\n' "${expected_containers[@]}" | LC_ALL=C sort -u)
	[[ "${#expected_containers[@]}" -eq 4 && "${project_containers[*]}" == "${expected_containers[*]}" ]] ||
		{
			rm -rf "${temporary_root}"
			die "running Compose project contains an orphan or duplicate container"
		}
	rm -rf "${temporary_root}"
}

normalized_authority_statuses() {
	local fps legends fps_canonical legends_canonical fps_sha256 legends_sha256
	fps="$(curl -fsS --max-time 15 "http://${chain_ip}:8787/v1/fps/status")" || die "FPS authority status is unavailable"
	legends="$(curl -fsS --max-time 15 "http://${chain_ip}:8787/v1/eterra-legends/status")" || die "Legends authority status is unavailable"
	jq -e '
	  keys == ["authorityConfigHashHex","authorityStateAvailable","ok","paidEntry",
	           "permanentAssetLoss","privateAlphaOnly","publicProduction","resultCount",
	           "runtimeDerivesRewards","signerAvailable","wagering"] and
	  (.resultCount | type == "number" and . >= 0 and floor == .)
	' <<<"${fps}" >/dev/null || die "FPS authority live status schema drifted"
	jq -e '
	  keys == ["authority_config_hash","authority_state_available","encounter_catalog_available",
	           "game_id","game_version","mode_id","ok","owner_authorization_available",
	           "result_journal_available","results","rewards_derived_by_runtime","service",
	           "sessions","signer_algorithm","signer_available"] and
	  .service == "Eterra.Arcade.Authority" and .signer_algorithm == "sr25519" and
	  (.sessions | type == "number" and . >= 0 and floor == .) and
	  (.results | type == "number" and . >= 0 and floor == .)
	' <<<"${legends}" >/dev/null || die "Legends authority live status schema drifted"
	fps_canonical="$(jq -S . <<<"${fps}")" || die "FPS authority status is not valid JSON"
	legends_canonical="$(jq -S . <<<"${legends}")" || die "Legends authority status is not valid JSON"
	# Source-document digests bind the earlier canonical Phase-2 capture. Live
	# status counters may advance, so reopen revalidates the closed safety facts
	# while retaining (rather than recomputing) those provenance digests.
	fps_sha256="$(jq -er '.siteDeploymentIdentity.authorityStatus.fps.sourceDocumentSha256' "${plan_path}")"
	legends_sha256="$(jq -er '.siteDeploymentIdentity.authorityStatus.legends.sourceDocumentSha256' "${plan_path}")"
	jq -cn --sort-keys \
		--argjson fps "${fps_canonical}" --argjson legends "${legends_canonical}" \
		--arg fpsSha256 "${fps_sha256}" --arg legendsSha256 "${legends_sha256}" '
	  {fps:{sourceEndpoint:"http://127.0.0.1:8787/v1/fps/status",
	        sourceDocumentSha256:$fpsSha256,
	        ok:$fps.ok,signerAvailable:$fps.signerAvailable,
	        authorityStateAvailable:$fps.authorityStateAvailable,
	        runtimeDerivesRewards:$fps.runtimeDerivesRewards,
	        privateAlphaOnly:$fps.privateAlphaOnly,paidEntry:$fps.paidEntry,
	        wagering:$fps.wagering,permanentAssetLoss:$fps.permanentAssetLoss,
	        publicProduction:$fps.publicProduction,
	        authorityConfigHash:($fps.authorityConfigHashHex | ascii_downcase)},
	   legends:{sourceEndpoint:"http://127.0.0.1:8787/v1/eterra-legends/status",
	            sourceDocumentSha256:$legendsSha256,
	            ok:$legends.ok,gameId:$legends.game_id,gameVersion:$legends.game_version,
	            modeId:$legends.mode_id,signerAvailable:$legends.signer_available,
	            authorityStateAvailable:$legends.authority_state_available,
	            encounterCatalogAvailable:$legends.encounter_catalog_available,
	            ownerAuthorizationAvailable:$legends.owner_authorization_available,
	            resultJournalAvailable:$legends.result_journal_available,
	            runtimeDerivesRewards:$legends.rewards_derived_by_runtime,
	            authorityConfigHash:($legends.authority_config_hash | ascii_downcase)}}'
}

require_authority_statuses() {
	local actual expected
	actual="$(normalized_authority_statuses)"
	expected="$(jq -cS '.siteDeploymentIdentity.authorityStatus' "${plan_path}")"
	[[ "${actual}" == "${expected}" ]] || die "FPS/Legends authority status drifted"
}

require_authority_liveness_challenge() {
	local nonce response response_base64 container
	nonce="0x$(python3 -c 'import secrets; print(secrets.token_hex(32))')"
	response="$(curl -fsS --max-time 15 \
		-H 'Content-Type: application/json' \
		--data-binary "$(jq -cn --arg nonceHex "${nonce}" '{nonceHex:$nonceHex}')" \
		"http://${chain_ip}:8787/v1/authority/liveness-challenge")" ||
		die "authority liveness challenge is unavailable"
	response_base64="$(printf '%s' "${response}" | base64 | tr -d '\r\n')"
	container="$(service_id site)"
	[[ -n "${container}" ]] || die "site container is unavailable for authority challenge verification"
	docker exec -i -w /app \
		-e NEXUS_REOPEN_PLAN_BASE64="${plan_base64}" \
		-e NEXUS_AUTHORITY_CHALLENGE_BASE64="${response_base64}" \
		-e NEXUS_AUTHORITY_CHALLENGE_NONCE="${nonce}" \
		"${container}" node --input-type=module <<'JS'
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import { cryptoWaitReady, signatureVerify } from '@polkadot/util-crypto';

const plan = JSON.parse(Buffer.from(
  process.env.NEXUS_REOPEN_PLAN_BASE64,
  'base64').toString('utf8'));
const receipt = JSON.parse(Buffer.from(
  process.env.NEXUS_AUTHORITY_CHALLENGE_BASE64,
  'base64').toString('utf8'));
assert.deepEqual(Object.keys(receipt).sort(), [
  'algorithm', 'error', 'nonceHex', 'ok', 'payloadHashHex',
  'publicKeyHex', 'schema', 'signatureHex'
]);
assert.equal(receipt.schema, 'eterra.authority-liveness-challenge.v1');
assert.equal(receipt.ok, true);
assert.equal(receipt.error, '');
assert.equal(receipt.algorithm, 'sr25519');
assert.equal(receipt.nonceHex, process.env.NEXUS_AUTHORITY_CHALLENGE_NONCE);
const domain = Buffer.from('eterra.authority-liveness-challenge.v1\0', 'utf8');
const nonce = Buffer.from(receipt.nonceHex.slice(2), 'hex');
assert.equal(nonce.length, 32);
const payloadHash = crypto.createHash('sha256').update(domain).update(nonce).digest();
assert.equal(receipt.payloadHashHex, `0x${payloadHash.toString('hex')}`);
const expectedKeys = [...new Set(
  plan.runtimeAuthority.authorityEpochs.map((epoch) => epoch.publicKey.toLowerCase()))
];
assert.equal(expectedKeys.length, 1, 'finalized authority epochs use different keys');
assert.equal(receipt.publicKeyHex.toLowerCase(), expectedKeys[0]);
await cryptoWaitReady();
const verified = signatureVerify(
  payloadHash,
  receipt.signatureHex,
  receipt.publicKeyHex);
assert.equal(verified.isValid, true, 'authority challenge signature is invalid');
assert.equal(verified.crypto, 'sr25519', 'authority challenge signature algorithm drifted');
JS
}

require_current_runtime_authority() {
	local container
	container="$(service_id site)"
	[[ -n "${container}" ]] || die "site container is unavailable for finalized runtime verification"
	docker exec -i -w /app -e NEXUS_REOPEN_PLAN_BASE64="${plan_base64}" \
		"${container}" node --input-type=module <<'JS'
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
const { ApiPromise, WsProvider } = await import('@polkadot/api');

const plan = JSON.parse(Buffer.from(process.env.NEXUS_REOPEN_PLAN_BASE64, 'base64').toString('utf8'));
const expected = plan.runtimeAuthority;
const provider = new WsProvider(`ws://${plan.network.chainLanIp}:9944`, 5_000);
const api = await ApiPromise.create({ provider, noInitWarn: true });
try {
  await api.isReadyOrError;
  const finalized = await api.rpc.chain.getFinalizedHead();
  const apiAt = await api.at(finalized);
  const version = await api.rpc.state.getRuntimeVersion(finalized);
  assert.equal(version.specVersion.toNumber(), expected.runtimeSpecVersion, 'runtime spec drift');
  assert.equal(api.genesisHash.toHex().toLowerCase(), plan.genesisHash, 'genesis drift');
  const code = await api.rpc.state.getStorage('0x3a636f6465', finalized);
  assert.ok(code && !code.isNone, 'runtime code missing');
  const codeBytes = Buffer.from(code.toHex().slice(2), 'hex');
  assert.equal(crypto.createHash('sha256').update(codeBytes).digest('hex'), expected.runtimeCodeSha256, 'runtime code drift');
  const metadata = await api.rpc.state.getMetadata(finalized);
  const metadataBytes = Buffer.from(metadata.toHex().slice(2), 'hex');
  assert.equal(crypto.createHash('sha256').update(metadataBytes).digest('hex'), expected.runtimeMetadataScaleSha256, 'runtime metadata drift');

  const named = (container, candidates, label) => {
    for (const candidate of candidates) if (container?.[candidate]) return container[candidate];
    throw new Error(`${label} missing`);
  };
  const alpha = named(apiAt.query, ['alphaAccess', 'alpha_access'], 'AlphaAccess');
  const results = named(apiAt.query, ['eterraGameResults', 'eterra_game_results'], 'GameResults');
  const query = (container, candidates, label) => named(container, candidates, label);
  const unwrap = (codec) => codec?.isSome ? codec.unwrap() : codec?.isNone ? null : codec;
  const json = (codec) => {
    const value = unwrap(codec);
    return typeof value?.toJSON === 'function' ? value.toJSON() : value;
  };
  const field = (value, snake) => {
    if (!value || typeof value !== 'object') return undefined;
    if (Object.hasOwn(value, snake)) return value[snake];
    const camel = snake.replace(/_([a-z])/g, (_, c) => c.toUpperCase());
    return value[camel];
  };
  const enumValue = (value) => typeof value === 'string' ? value : value && typeof value === 'object' ? Object.keys(value)[0] : value;
  const decimal = (value) => String(value);
  const hex = (value) => typeof value?.toHex === 'function' ? value.toHex().toLowerCase() : String(value).toLowerCase();
  const count = async (container, candidates, label) => {
    const storage = query(container, candidates, label);
    assert.equal(typeof storage.keys, 'function', `${label} is not enumerable`);
    return (await storage.keys()).length;
  };

  assert.equal(enumValue(json(await query(alpha, ['accessMode'], 'access mode')())), 'Enforced');
  const zeroContract = `0x${'00'.repeat(20)}`;
  const allowed = await query(alpha, ['allowedSources'], 'allowed sources')(['ManualAdmin', 0, zeroContract]);
  assert.ok(unwrap(allowed), 'exact ManualAdmin source missing');
  const grantCodec = await query(alpha, ['whitelist'], 'whitelist')(expected.alphaAccess.ownerAccountId);
  const grant = json(grantCodec);
  assert.ok(grant, 'exact AlphaAccess grant missing');
  assert.equal(enumValue(field(grant, 'source_kind')), expected.alphaAccess.sourceKind);
  assert.equal(decimal(field(grant, 'source_chain_id')), String(expected.alphaAccess.sourceChainId));
  assert.equal(hex(field(grant, 'source_contract')), expected.alphaAccess.sourceContract);
  assert.equal(hex(field(grant, 'source_event_id')), expected.alphaAccess.sourceEventId);
  assert.equal(decimal(field(grant, 'expires_at_unix')), String(expected.alphaAccess.expiresAtUnix));
  assert.ok(Number(expected.alphaAccess.expiresAtUnix) > Math.floor(Date.now() / 1000), 'AlphaAccess grant expired');
  const processed = await query(alpha, ['processedSources'], 'processed sources')(expected.alphaAccess.sourceEventId);
  assert.ok(unwrap(processed), 'AlphaAccess source event not consumed');

  const counts = {
    allowedSources: await count(alpha, ['allowedSources'], 'allowed sources'),
    whitelist: await count(alpha, ['whitelist'], 'whitelist'),
    processedSources: await count(alpha, ['processedSources'], 'processed sources'),
    managers: await count(alpha, ['managers'], 'managers'),
    authorityEpochs: await count(results, ['authorityEpochs', 'authorityEpoch'], 'authority epochs'),
    rewardPolicies: await count(results, ['rewardPolicies', 'rewardPolicy'], 'reward policies'),
    rewardBudgets: await count(results, ['rewardBudgets', 'rewardBudget'], 'reward budgets'),
    rewardActivations: await count(results, ['rewardPolicyActivation', 'rewardPolicyActive'], 'reward activations'),
    rewardEverActivated: await count(results, ['rewardPolicyEverActivated'], 'reward ever activated')
  };
  assert.deepEqual(counts, expected.storageCounts, 'authority storage cardinality drift');

  const epochs = query(results, ['authorityEpochs', 'authorityEpoch'], 'authority epochs');
  const finalizedNumber = (await api.rpc.chain.getHeader(finalized)).number.toNumber();
  for (const epoch of expected.authorityEpochs) {
    const record = json(await epochs([epoch.gameId, epoch.gameVersion, epoch.modeId, epoch.authorityEpoch]));
    assert.ok(record, `authority epoch missing ${epoch.gameId}.${epoch.modeId}`);
    assert.equal(hex(field(record, 'public_key')), epoch.publicKey);
    assert.equal(hex(field(record, 'authority_config_hash')), epoch.authorityConfigHash);
    assert.equal(decimal(field(record, 'active_from')), String(epoch.activeFrom));
    assert.equal(decimal(field(record, 'active_until')), String(epoch.activeUntil));
    assert.equal(field(record, 'revoked'), false);
    assert.ok(finalizedNumber >= epoch.activeFrom && finalizedNumber < epoch.activeUntil, 'authority epoch is not currently active');
  }

  const key = expected.proofPolicy.key;
  const policy = json(await query(results, ['rewardPolicies', 'rewardPolicy'], 'reward policy')(key));
  assert.ok(policy, 'proof-only policy missing');
  assert.equal(hex(field(policy, 'policy_hash')), expected.proofPolicy.policyHash);
  assert.equal(enumValue(field(policy, 'economic_realm')), expected.proofPolicy.economicRealm);
  assert.equal(field(policy, 'practice_only'), expected.proofPolicy.practiceOnly);
  for (const name of ['max_player_xp','entity_xp','base_essence','charge_drop_bps','prism_drop_bps','elimination_weight_bps','participation_weight_bps','objective_weight_bps','maximum_xp_per_day','per_entity_encounter_rewards_per_day']) {
    assert.equal(decimal(field(policy, name)), '0', `proof policy ${name} is nonzero`);
  }
  assert.equal(field(policy, 'charge_definition_id'), null);
  assert.equal(field(policy, 'prism_definition_id'), null);
  const budget = json(await query(results, ['rewardBudgets', 'rewardBudget'], 'reward budget')(key));
  assert.ok(budget && Object.values(budget).every((value) => decimal(value) === '0'), 'proof reward budget is nonzero');
  assert.equal(json(await query(results, ['rewardPolicyActivation', 'rewardPolicyActive'], 'reward activation')(key)), false);
  assert.equal(json(await query(results, ['rewardPolicyEverActivated'], 'reward ever activated')(key)), true);
} finally {
  await api.disconnect();
}
JS
}

require_loopback_listener() {
	local port="$1"
	local label="$2"
	local listeners address
	listeners="$(ss -H -lnt "sport = :${port}")"
	[[ -n "${listeners}" ]] || die "${label} listener is missing"
	while IFS= read -r address; do
		case "${address}" in
			127.0.0.1:${port}|\[::1\]:${port}) ;;
			*) die "${label} is not loopback-only: ${address}" ;;
		esac
	done < <(printf '%s\n' "${listeners}" | awk '{print $4}')
}

require_loopback_or_absent() {
	local port="$1"
	local label="$2"
	local listeners address
	listeners="$(ss -H -lnt "sport = :${port}")"
	[[ -n "${listeners}" ]] || return 0
	while IFS= read -r address; do
		case "${address}" in
			127.0.0.1:${port}|\[::1\]:${port}) ;;
			*) die "${label} remains externally published: ${address}" ;;
		esac
	done < <(printf '%s\n' "${listeners}" | awk '{print $4}')
}

require_site_firewall() {
	local status
	status="$(ufw status verbose)"
	grep -q '^Status: active$' <<<"${status}" || die "site UFW is not active"
	grep -q '^Default: deny (incoming)' <<<"${status}" || die "site UFW default incoming policy is not deny"
	grep -Eq '^80/tcp[[:space:]]+ALLOW IN' <<<"${status}" || die "site HTTP ingress rule is missing"
	grep -Eq '^443/tcp[[:space:]]+ALLOW IN' <<<"${status}" || die "site HTTPS ingress rule is missing"
	for port in 3000 8787; do
		[[ "$(awk -v target="${port}/tcp" '$1 == target && $0 ~ /ALLOW IN/ {count++} END {print count+0}' <<<"${status}")" == 0 ]] || die "site internal port ${port} is externally allowed"
	done
}

require_site_close_guard() {
	local rules
	rules="$(nft list table inet "${SITE_CLOSE_GUARD_TABLE}" 2>/dev/null)" ||
		die "site emergency-close nft guard is unavailable"
	grep -Fq "table inet ${SITE_CLOSE_GUARD_TABLE}" <<<"${rules}" ||
		die "site emergency-close nft table drifted"
	grep -Eq 'type filter hook prerouting priority -310; policy accept;' <<<"${rules}" ||
		die "site emergency-close nft hook drifted"
	grep -Eq 'iifname != "lo" tcp dport \{ 80, 443 \}.* drop' <<<"${rules}" ||
		die "site emergency-close nft denial rule drifted"
}

require_site_close_guard_absent() {
	if nft list table inet "${SITE_CLOSE_GUARD_TABLE}" >/dev/null 2>&1; then
		die "a retained emergency-close nft guard blocks this reopen operation"
	fi
}

install_site_close_guard() {
	local rules_path
	rules_path="$(mktemp /tmp/nexus-v2-site-emergency-close-nft.XXXXXX)"
	chmod 0600 "${rules_path}"
	cat >"${rules_path}" <<EOF
table inet ${SITE_CLOSE_GUARD_TABLE} {
	chain prerouting_guard {
		type filter hook prerouting priority -310; policy accept;
		iifname != "lo" tcp dport { 80, 443 } counter drop comment "nexus-v2-site-emergency-close"
	}
}
EOF
	nft delete table inet "${SITE_CLOSE_GUARD_TABLE}" >/dev/null 2>&1 || true
	if ! nft -f "${rules_path}"; then
		rm -f "${rules_path}"
		die "cannot install site emergency-close nft guard"
	fi
	rm -f "${rules_path}"
	require_site_close_guard
}

remove_site_close_guard() {
	require_site_close_guard
	nft delete table inet "${SITE_CLOSE_GUARD_TABLE}" ||
		die "cannot remove site emergency-close nft guard after Phase-1 proof"
	if nft list table inet "${SITE_CLOSE_GUARD_TABLE}" >/dev/null 2>&1; then
		die "site emergency-close nft guard remains after removal"
	fi
}

require_indexer_readiness() {
	local scope="$1"
	local access_key expected_key_sha actual_key_sha temporary_root header_file health_file acceptance_file
	local -a curl_args
	access_key="$(read_site_env_value NEXUS_V2_PRIVATE_ALPHA_ACCESS_KEY)" ||
		die "private-Alpha indexer access key is unavailable"
	[[ "${access_key}" != *$'\n'* && "${access_key}" != *$'\r'* ]] ||
		die "private-Alpha indexer access key contains a line break"
	expected_key_sha="$(jq -er '.indexerReadiness.privateAlphaAccessKeySha256' "${plan_path}")"
	actual_key_sha="$(printf '%s' "${access_key}" | sha256sum | awk '{print $1}')"
	[[ "${actual_key_sha}" == "${expected_key_sha}" ]] ||
		die "private-Alpha indexer access key drifted"
	temporary_root="$(mktemp -d /tmp/nexus-v2-reopen-indexer-readiness.XXXXXX)"
	chmod 0700 "${temporary_root}"
	header_file="${temporary_root}/private-alpha.header"
	health_file="${temporary_root}/health.json"
	acceptance_file="${temporary_root}/acceptance.json"
	printf 'x-eterra-private-alpha-key: %s\n' "${access_key}" >"${header_file}"
	chmod 0600 "${header_file}"
	case "${scope}" in
		loopback)
			curl_args=(-fsS --max-time 20)
			"${curl_args[@]}" 'http://127.0.0.1:8787/health/ready' >"${health_file}" &&
				"${curl_args[@]}" --header "@${header_file}" \
				'http://127.0.0.1:8787/v2/private-alpha/acceptance/readiness' >"${acceptance_file}" || {
				rm -rf "${temporary_root}"
				die "loopback indexer readiness documents are unavailable"
			}
			;;
		public)
			curl_args=(-fsS --max-time 20 --resolve "${public_hostname}:443:127.0.0.1")
			"${curl_args[@]}" "https://${public_hostname}/nexus-api/health/ready" >"${health_file}" &&
				"${curl_args[@]}" --header "@${header_file}" \
				"https://${public_hostname}/nexus-api/v2/private-alpha/acceptance/readiness" >"${acceptance_file}" || {
				rm -rf "${temporary_root}"
				die "public indexer readiness documents are unavailable"
			}
			;;
		*) rm -rf "${temporary_root}"; die "invalid indexer readiness scope" ;;
	esac
	if ! python3 - "${plan_path}" "${health_file}" "${acceptance_file}" <<'PY'
import hashlib
import json
import sys

plan_path, health_path, acceptance_path = sys.argv[1:]
with open(plan_path, encoding="utf-8") as handle:
    plan = json.load(handle)
with open(health_path, encoding="utf-8") as handle:
    health = json.load(handle)
with open(acceptance_path, encoding="utf-8") as handle:
    acceptance = json.load(handle)
authority = plan["indexerReadiness"]
def canonical_digest(value):
    payload = (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode()
    return hashlib.sha256(payload).hexdigest()

if canonical_digest(health) != authority["healthReadySha256"]:
    raise SystemExit("indexer health differs from the activation-receipt proof")
if canonical_digest(acceptance) != authority["acceptanceReadinessSha256"]:
    raise SystemExit("acceptance readiness differs from the immutable projection proof")
if authority["acceptanceReadinessSha256"] != authority["readinessProjectionSha256"]:
    raise SystemExit("activation receipt did not bind readiness to the projection")
PY
	then
		rm -rf "${temporary_root}"
		die "${scope} indexer readiness contract is unsafe or drifted"
	fi
	rm -rf "${temporary_root}"
}

require_stack_boundary() {
	local container ports
	require_loopback_listener 3000 site
	require_loopback_listener 8787 indexer
	for service in site indexer-api; do
		container="$(service_id "${service}")"
		[[ -n "${container}" ]] || die "container is missing: ${service}"
		ports="$(docker inspect --format '{{json .NetworkSettings.Ports}}' "${container}")"
		if [[ "${service}" == site ]]; then container_port=3000; else container_port=8787; fi
		jq -e --arg key "${container_port}/tcp" --arg port "${container_port}" '
		  (. // {}) as $ports | ($ports | keys) == [$key] and
		  ($ports[$key] | type == "array" and length == 1) and
		  $ports[$key][0].HostIp == "127.0.0.1" and $ports[$key][0].HostPort == $port
		' <<<"${ports}" >/dev/null || die "${service} publication is not exact loopback"
	done
	curl -fsS --max-time 15 'http://127.0.0.1:3000/health/ready' >/dev/null || die "site readiness failed"
	require_indexer_readiness loopback
	require_site_firewall
}

validate_caddy_candidate() {
	local candidate="$1"
	local container="$2"
	docker cp "${candidate}" "${container}:/tmp/nexus-v2-reopen-candidate.Caddyfile"
	docker exec "${container}" caddy validate --config /tmp/nexus-v2-reopen-candidate.Caddyfile --adapter caddyfile >/dev/null || die "candidate Caddyfile validation failed"
	docker exec "${container}" rm -f /tmp/nexus-v2-reopen-candidate.Caddyfile
}

require_active_caddy() {
	local expected_hash="$1"
	local container container_hash
	[[ "$(sha256sum "${REMOTE_CADDYFILE}" | awk '{print $1}')" == "${expected_hash}" ]] || die "active host Caddyfile hash mismatch"
	container="$(caddy_id)"
	[[ -n "${container}" ]] || die "Caddy container is missing"
	docker exec "${container}" caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile >/dev/null || die "active Caddy configuration is invalid"
	container_hash="$(docker exec "${container}" sha256sum /etc/caddy/Caddyfile | awk '{print $1}')"
	[[ "${container_hash}" == "${expected_hash}" ]] || die "container Caddyfile differs from host copy"
	# Re-apply the pinned bytes on every verification. This removes any dynamic
	# admin-API route injection before read/write boundary probes are trusted.
	docker exec "${container}" caddy reload --config /etc/caddy/Caddyfile --adapter caddyfile >/dev/null ||
		die "active Caddy configuration could not be reset to the pinned file"
}

hash_url() {
	local url="$1"
	local temporary
	temporary="$(mktemp /tmp/nexus-v2-site-reopen-smoke.XXXXXX)"
	curl -fsS --max-time 15 "${url}" >"${temporary}" || {
		rm -f "${temporary}"
		return 1
	}
	sha256sum "${temporary}" | awk '{print $1}'
	rm -f "${temporary}"
}

https_body_hash() {
	local path="$1"
	local temporary
	temporary="$(mktemp /tmp/nexus-v2-site-public-smoke.XXXXXX)"
	curl -fsS --max-time 20 --resolve "${public_hostname}:443:127.0.0.1" "https://${public_hostname}${path}" >"${temporary}" || {
		rm -f "${temporary}"
		return 1
	}
	sha256sum "${temporary}" | awk '{print $1}'
	rm -f "${temporary}"
}

https_status() {
	local method="$1"
	local path="$2"
	shift 2
	curl -sS --max-time 15 --resolve "${public_hostname}:443:127.0.0.1" -o /dev/null -w '%{http_code}' -X "${method}" "$@" "https://${public_hostname}${path}"
}

require_upstream_reads() {
	local rpc media_path media_sha ipfs_path ipfs_sha
	media_path="$(jq -er '.smoke.mediaPath' "${plan_path}")"
	media_sha="$(jq -er '.smoke.mediaSha256' "${plan_path}")"
	ipfs_path="$(jq -er '.smoke.ipfsPath' "${plan_path}")"
	ipfs_sha="$(jq -er '.smoke.ipfsSha256' "${plan_path}")"
	rpc="$(curl -fsS --max-time 15 -H 'Content-Type: application/json' \
		--data-binary '{"id":1,"jsonrpc":"2.0","method":"chain_getBlockHash","params":[0]}' \
		"http://${chain_ip}:9944")" || die "site-to-chain RPC read failed"
	[[ "$(jq -er '.result' <<<"${rpc}")" == "${genesis_hash}" ]] || die "site-to-chain genesis mismatch"
	curl -fsS --max-time 15 "http://${chain_ip}:4000/health/ready" >/dev/null || die "site-to-media readiness failed"
	curl -fsS --max-time 15 "http://${chain_ip}:8787/v1/status" >/dev/null || die "site-to-authority readiness failed"
	[[ "$(hash_url "http://${chain_ip}:4000${media_path}")" == "${media_sha}" ]] || die "site-to-media content mismatch"
	[[ "$(hash_url "http://${chain_ip}:8080${ipfs_path}")" == "${ipfs_sha}" ]] || die "site-to-IPFS gateway content mismatch"
}

require_public_reads() {
	local rpc media_path media_sha ipfs_path ipfs_sha status
	media_path="$(jq -er '.smoke.mediaPath' "${plan_path}")"
	media_sha="$(jq -er '.smoke.mediaSha256' "${plan_path}")"
	ipfs_path="$(jq -er '.smoke.ipfsPath' "${plan_path}")"
	ipfs_sha="$(jq -er '.smoke.ipfsSha256' "${plan_path}")"
	rpc="$(curl -fsS --max-time 20 --resolve "${public_hostname}:443:127.0.0.1" \
		-H 'Content-Type: application/json' --data-binary '{"id":1,"jsonrpc":"2.0","method":"chain_getBlockHash","params":[0]}' \
		"https://${public_hostname}/rpc")" || die "public Caddy RPC read failed"
	[[ "$(jq -er '.result' <<<"${rpc}")" == "${genesis_hash}" ]] || die "public Caddy RPC genesis mismatch"
	require_indexer_readiness public
	curl -fsS --max-time 20 --resolve "${public_hostname}:443:127.0.0.1" "https://${public_hostname}/arcade-authority/v1/fps/status" >/dev/null || die "public FPS authority status path failed"
	curl -fsS --max-time 20 --resolve "${public_hostname}:443:127.0.0.1" "https://${public_hostname}/arcade-authority/v1/eterra-legends/status" >/dev/null || die "public Legends authority status path failed"
	[[ "$(https_status GET /arcade-authority/v1/status)" == 403 ]] || die "unmodeled authority status route is not closed"
	[[ "$(https_body_hash "/media-api${media_path}")" == "${media_sha}" ]] || die "public media read path mismatch"
	[[ "$(https_body_hash "${ipfs_path}")" == "${ipfs_sha}" ]] || die "public IPFS read path mismatch"
	status="$(https_status POST "/media-api${media_path}")"
	[[ "${status}" == 404 || "${status}" == 405 ]] || die "public media mutation route is not closed"
	[[ "$(https_status POST "${ipfs_path}")" == 405 ]] || die "public IPFS mutation route is not closed"
	[[ "$(https_status POST /nexus-api/v2/sessions -H 'Content-Type: application/json' --data-binary '{}')" == 405 ]] || die "public indexer mutation route is not closed"
}

require_phase1_routes() {
	local probe
	probe="$(curl -fsS --max-time 15 --resolve "${public_hostname}:443:127.0.0.1" "https://${public_hostname}/__nexus_v2_phase1_ingress")" || die "Phase-1 probe failed"
	jq -e '.mode == "AllExternalWriteIngressClosed" and .paidOrPublicActivationAuthorized == false' <<<"${probe}" >/dev/null || die "Phase-1 probe contract mismatch"
	[[ "$(https_status POST /rpc -H 'Content-Type: application/json' --data-binary '{"id":1,"jsonrpc":"2.0","method":"author_submitExtrinsic","params":["0x00"]}')" == 403 ]] || die "Phase-1 RPC write route is not closed"
	[[ "$(https_status POST /arcade-authority/v1/sessions -H 'Content-Type: application/json' --data-binary '{}')" == 403 ]] || die "Phase-1 authority route is not closed"
}

marker_matches() {
	local marker="$1"
	local expected_state="$2"
	[[ -r "${marker}" && ! -L "${marker}" ]] || return 1
	jq -e --arg operationId "${operation_id}" --arg planSha256 "${plan_sha256}" \
		--arg releaseId "${release_id}" --arg sourceCommit "${source_commit}" \
		--arg siteSourceCommit "${site_source_commit}" --arg genesisHash "${genesis_hash}" \
		--arg finalReleaseLockSha256 "${final_lock_sha256}" \
		--arg acceptanceBoundaryReceiptSha256 "${receipt_sha256}" \
		--arg finalSealSha256 "${seal_sha256}" --arg driverSha256 "${driver_sha256}" \
		--arg fpsAdoptionSealSha256 "${fps_adoption_seal_sha256}" \
		--arg helperSha256 "${helper_sha256}" --arg siteReleaseVersion "${site_release_version}" \
		--arg normalCaddyfileSha256 "${normal_caddy_sha256}" \
		--arg phase1CaddyfileSha256 "${phase1_caddy_sha256}" \
		--arg state "${expected_state}" '
		.schemaVersion == 1 and .kind == "nexus-v2-private-alpha-post-acceptance-site-ingress-marker" and
		.operationId == $operationId and .planSha256 == $planSha256 and .releaseId == $releaseId and
		.sourceCommit == $sourceCommit and .siteSourceCommit == $siteSourceCommit and
		.siteReleaseVersion == $siteReleaseVersion and .genesisHash == $genesisHash and
		.finalReleaseLockSha256 == $finalReleaseLockSha256 and
		.acceptanceBoundaryReceiptSha256 == $acceptanceBoundaryReceiptSha256 and
		.phase2FinalSealSha256 == $finalSealSha256 and .driverSha256 == $driverSha256 and
		((if $state == "open" then .fpsAdoptionSealSha256 == $fpsAdoptionSealSha256
		  else .fpsAdoptionSealSha256 == null end)) and
		.helperSha256 == $helperSha256 and .normalCaddyfileSha256 == $normalCaddyfileSha256 and
		.phase1CaddyfileSha256 == $phase1CaddyfileSha256 and .state == $state and
		((if $state == "open" then .activeCaddyfileSha256 == $normalCaddyfileSha256
		  else .activeCaddyfileSha256 == $phase1CaddyfileSha256 end)) and
		(.caddyContainerId | type == "string" and length > 0) and
		(.listenersSha256 | type == "string" and test("^[0-9a-f]{64}$")) and
		(.firewallSha256 | type == "string" and test("^[0-9a-f]{64}$")) and
		((if $state == "open" then
		    (.watchdogManifestSha256 | test("^[0-9a-f]{64}$")) and
		    (.bootGuardManifestSha256 | test("^[0-9a-f]{64}$"))
		  else .watchdogManifestSha256 == null and .bootGuardManifestSha256 == null end)) and
		.loopbackServicesPrivate == true and
		.chainStateMutationPerformed == false and .paidOrPublicProductionActivationAuthorized == false
	' "${marker}" >/dev/null
}

write_marker() {
	local marker="$1"
	local state="$2"
	local listeners_sha firewall_sha active_sha container watchdog_manifest_sha boot_guard_manifest_sha
	mkdir -p "${STATE_ROOT}"
	chmod 0700 "${STATE_ROOT}"
	listeners_sha="$(ss -H -lnt | LC_ALL=C sort | sha256sum | awk '{print $1}')"
	firewall_sha="$(ufw status verbose | sha256sum | awk '{print $1}')"
	active_sha="$(sha256sum "${REMOTE_CADDYFILE}" | awk '{print $1}')"
	container="$(caddy_id)"
	[[ -n "${container}" ]] || container="stopped"
	watchdog_manifest_sha=""
	boot_guard_manifest_sha=""
	if [[ "${state}" == open ]]; then
		[[ -r "${WATCHDOG_MANIFEST}" && -r "${BOOT_GUARD_MANIFEST}" ]] || die "site guard manifests are unavailable while writing open marker"
		watchdog_manifest_sha="$(sha256sum "${WATCHDOG_MANIFEST}" | awk '{print $1}')"
		boot_guard_manifest_sha="$(sha256sum "${BOOT_GUARD_MANIFEST}" | awk '{print $1}')"
	fi
	jq -n --sort-keys \
		--arg operationId "${operation_id}" --arg planSha256 "${plan_sha256}" \
		--arg releaseId "${release_id}" --arg siteReleaseVersion "${site_release_version}" \
		--arg sourceCommit "${source_commit}" \
		--arg siteSourceCommit "${site_source_commit}" --arg genesisHash "${genesis_hash}" \
		--arg finalReleaseLockSha256 "${final_lock_sha256}" --arg acceptanceBoundaryReceiptSha256 "${receipt_sha256}" \
		--arg phase2FinalSealSha256 "${seal_sha256}" --arg driverSha256 "${driver_sha256}" \
		--arg fpsAdoptionSealSha256 "${fps_adoption_seal_sha256}" \
		--arg helperSha256 "${helper_sha256}" --arg state "${state}" --arg activeCaddyfileSha256 "${active_sha}" \
		--arg normalCaddyfileSha256 "${normal_caddy_sha256}" --arg phase1CaddyfileSha256 "${phase1_caddy_sha256}" \
		--arg caddyContainerId "${container}" --arg listenersSha256 "${listeners_sha}" \
		--arg firewallSha256 "${firewall_sha}" --arg watchdogManifestSha256 "${watchdog_manifest_sha}" \
		--arg bootGuardManifestSha256 "${boot_guard_manifest_sha}" \
		--arg observedAtUtc "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
		'{schemaVersion:1,kind:"nexus-v2-private-alpha-post-acceptance-site-ingress-marker",
		  operationId:$operationId,planSha256:$planSha256,releaseId:$releaseId,
		  siteReleaseVersion:$siteReleaseVersion,sourceCommit:$sourceCommit,
		  siteSourceCommit:$siteSourceCommit,genesisHash:$genesisHash,
		  finalReleaseLockSha256:$finalReleaseLockSha256,
		  acceptanceBoundaryReceiptSha256:$acceptanceBoundaryReceiptSha256,
		  phase2FinalSealSha256:$phase2FinalSealSha256,
		  fpsAdoptionSealSha256:(if $state == "open" then $fpsAdoptionSealSha256 else null end),
		  driverSha256:$driverSha256,helperSha256:$helperSha256,
		  state:$state,activeCaddyfileSha256:$activeCaddyfileSha256,
		  normalCaddyfileSha256:$normalCaddyfileSha256,phase1CaddyfileSha256:$phase1CaddyfileSha256,
		  caddyContainerId:$caddyContainerId,listenersSha256:$listenersSha256,firewallSha256:$firewallSha256,
		  watchdogManifestSha256:(if $state == "open" then $watchdogManifestSha256 else null end),
		  bootGuardManifestSha256:(if $state == "open" then $bootGuardManifestSha256 else null end),
		  loopbackServicesPrivate:true,chainStateMutationPerformed:false,
		  paidOrPublicProductionActivationAuthorized:false,observedAtUtc:$observedAtUtc}' >"${marker}.pending"
	chmod 0400 "${marker}.pending"
	mv "${marker}.pending" "${marker}"
}

install_caddy() {
	local candidate="$1"
	local expected_sha="$2"
	local container
	container="$(caddy_id)"
	[[ -n "${container}" ]] || die "Caddy container is missing"
	validate_caddy_candidate "${candidate}" "${container}"
	# Compose bind-mounts this file (not its directory). Preserve the inode so
	# the running container sees the new bytes before the explicit reload.
	cp "${candidate}" "${REMOTE_CADDYFILE}"
	chmod 0644 "${REMOTE_CADDYFILE}"
	[[ "$(sha256sum "${REMOTE_CADDYFILE}" | awk '{print $1}')" == "${expected_sha}" ]] || die "installed Caddyfile hash mismatch"
	if ! docker exec "${container}" caddy reload --config /etc/caddy/Caddyfile --adapter caddyfile >/dev/null; then
		"${compose[@]}" stop caddy >/dev/null 2>&1 || true
		die "Caddy reload failed; Caddy stopped fail-closed"
	fi
	require_active_caddy "${expected_sha}"
}

retain_candidates() {
	mkdir -p "${STATE_ROOT}"
	chmod 0700 "${STATE_ROOT}"
	if [[ ! -e "${RETAINED_PHASE1}" ]]; then
		cp "${phase1_candidate}" "${RETAINED_PHASE1}.pending"
		chmod 0400 "${RETAINED_PHASE1}.pending"
		mv "${RETAINED_PHASE1}.pending" "${RETAINED_PHASE1}"
	fi
	if [[ ! -e "${RETAINED_NORMAL}" ]]; then
		cp "${normal_candidate}" "${RETAINED_NORMAL}.pending"
		chmod 0400 "${RETAINED_NORMAL}.pending"
		mv "${RETAINED_NORMAL}.pending" "${RETAINED_NORMAL}"
	fi
	[[ "$(sha256sum "${RETAINED_PHASE1}" | awk '{print $1}')" == "${phase1_caddy_sha256}" ]] || die "retained Phase-1 Caddyfile drifted"
	[[ "$(sha256sum "${RETAINED_NORMAL}" | awk '{print $1}')" == "${normal_caddy_sha256}" ]] || die "retained normal Caddyfile drifted"
}

remove_watchdog() {
	systemctl disable --now "${WATCHDOG_TIMER}" >/dev/null 2>&1 || true
	rm -f "/etc/systemd/system/${WATCHDOG_TIMER}" "/etc/systemd/system/${WATCHDOG_SERVICE}"
	systemctl daemon-reload
	rm -f "${WATCHDOG_SCRIPT}" "${WATCHDOG_HELPER}" "${WATCHDOG_PLAN}" \
		"${WATCHDOG_NORMAL_CADDY}" "${WATCHDOG_PHASE1_CADDY}" "${WATCHDOG_MANIFEST}"
}

require_watchdog_absent() {
	local unit root load_state payload
	for unit in "${WATCHDOG_TIMER}" "${WATCHDOG_SERVICE}"; do
		for root in /etc/systemd/system /run/systemd/system /usr/local/lib/systemd/system /usr/lib/systemd/system /lib/systemd/system; do
			[[ ! -e "${root}/${unit}" ]] || die "coordinator watchdog unit remains: ${root}/${unit}"
		done
		! systemctl is-active --quiet "${unit}" || die "coordinator watchdog remains active: ${unit}"
		! systemctl is-enabled --quiet "${unit}" 2>/dev/null || die "coordinator watchdog remains enabled: ${unit}"
		load_state="$(systemctl show "${unit}" -p LoadState --value 2>/dev/null || true)"
		[[ -z "${load_state}" || "${load_state}" == not-found ]] || die "coordinator watchdog remains loaded: ${unit}"
	done
	for payload in "${WATCHDOG_SCRIPT}" "${WATCHDOG_HELPER}" "${WATCHDOG_PLAN}" \
		"${WATCHDOG_NORMAL_CADDY}" "${WATCHDOG_PHASE1_CADDY}" "${WATCHDOG_MANIFEST}"; do
		[[ ! -e "${payload}" ]] || die "coordinator watchdog payload remains: ${payload}"
	done
}

require_boot_guard_absent() {
	local root load_state
	for root in /etc/systemd/system /run/systemd/system /usr/local/lib/systemd/system /usr/lib/systemd/system /lib/systemd/system; do
		[[ ! -e "${root}/${BOOT_GUARD_SERVICE}" ]] || die "boot fail-closed unit remains: ${root}/${BOOT_GUARD_SERVICE}"
	done
	! systemctl is-active --quiet "${BOOT_GUARD_SERVICE}" || die "boot fail-closed unit remains active"
	! systemctl is-enabled --quiet "${BOOT_GUARD_SERVICE}" 2>/dev/null || die "boot fail-closed unit remains enabled"
	load_state="$(systemctl show "${BOOT_GUARD_SERVICE}" -p LoadState --value 2>/dev/null || true)"
	[[ -z "${load_state}" || "${load_state}" == not-found ]] || die "boot fail-closed unit remains loaded"
	[[ ! -e "${BOOT_GUARD_SCRIPT}" && ! -e "${BOOT_GUARD_PHASE1}" && ! -e "${BOOT_GUARD_MANIFEST}" ]] ||
		die "boot fail-closed payload remains"
}

remove_boot_guard() {
	systemctl disable --now "${BOOT_GUARD_SERVICE}" >/dev/null 2>&1 || true
	rm -f "/etc/systemd/system/${BOOT_GUARD_SERVICE}" "${BOOT_GUARD_SCRIPT}" \
		"${BOOT_GUARD_PHASE1}" "${BOOT_GUARD_MANIFEST}"
	systemctl daemon-reload
}

require_boot_guard_exact() {
	local expected actual fragment dropins pinned marker_pin
	for pinned in "${BOOT_GUARD_SCRIPT}" "${BOOT_GUARD_PHASE1}" "${BOOT_GUARD_MANIFEST}" \
		"/etc/systemd/system/${BOOT_GUARD_SERVICE}"; do
		[[ -f "${pinned}" && ! -L "${pinned}" ]] || die "boot fail-closed payload is unavailable: ${pinned}"
	done
	jq -e --arg operationId "${operation_id}" --arg planSha256 "${plan_sha256}" '
	  keys == ["files","kind","operationId","planSha256","schemaVersion"] and
	  .schemaVersion == 1 and .kind == "nexus-v2-private-alpha-reopen-boot-guard-manifest" and
	  .operationId == $operationId and .planSha256 == $planSha256 and
	  (.files | keys == ["phase1Caddy","script","serviceUnit"]) and
	  ([.files[]] | all(test("^[0-9a-f]{64}$")))
	' "${BOOT_GUARD_MANIFEST}" >/dev/null || die "boot fail-closed manifest contract mismatch"
	while IFS=$'\t' read -r expected actual; do
		[[ "${expected}" == "${actual}" ]] || die "boot fail-closed payload hash drifted"
	done < <(
		printf '%s\t%s\n' "$(jq -er '.files.phase1Caddy' "${BOOT_GUARD_MANIFEST}")" "$(sha256sum "${BOOT_GUARD_PHASE1}" | awk '{print $1}')"
		printf '%s\t%s\n' "$(jq -er '.files.script' "${BOOT_GUARD_MANIFEST}")" "$(sha256sum "${BOOT_GUARD_SCRIPT}" | awk '{print $1}')"
		printf '%s\t%s\n' "$(jq -er '.files.serviceUnit' "${BOOT_GUARD_MANIFEST}")" "$(sha256sum "/etc/systemd/system/${BOOT_GUARD_SERVICE}" | awk '{print $1}')"
	)
	[[ "$(sha256sum "${BOOT_GUARD_PHASE1}" | awk '{print $1}')" == "${phase1_caddy_sha256}" ]] || die "boot fail-closed Caddy payload differs from plan"
	pinned="$(sha256sum "${BOOT_GUARD_MANIFEST}" | awk '{print $1}')"
	marker_pin=""
	if [[ -r "${COMMITTED_MARKER}" && ! -L "${COMMITTED_MARKER}" ]]; then
		marker_pin="$(jq -er '.bootGuardManifestSha256' "${COMMITTED_MARKER}")"
	elif [[ -r "${OPEN_MARKER}" && ! -L "${OPEN_MARKER}" ]]; then
		marker_pin="$(jq -er '.bootGuardManifestSha256' "${OPEN_MARKER}")"
	fi
	[[ -z "${marker_pin}" || "${marker_pin}" == "${pinned}" ]] || die "site marker boot-guard pin drifted"
	systemctl is-enabled --quiet "${BOOT_GUARD_SERVICE}" || die "boot fail-closed unit is not enabled"
	fragment="$(systemctl show "${BOOT_GUARD_SERVICE}" -p FragmentPath --value)"
	[[ "${fragment}" == "/etc/systemd/system/${BOOT_GUARD_SERVICE}" ]] || die "boot fail-closed fragment drifted"
	dropins="$(systemctl show "${BOOT_GUARD_SERVICE}" -p DropInPaths --value)"
	[[ -z "${dropins}" ]] || die "boot fail-closed drop-in drifted"
}

committed_marker_matches() {
	[[ -r "${COMMITTED_MARKER}" && ! -L "${COMMITTED_MARKER}" ]] || return 1
	jq -e --arg operationId "${operation_id}" --arg planSha256 "${plan_sha256}" \
		--arg fpsAdoptionSealSha256 "${fps_adoption_seal_sha256}" '
	  .schemaVersion == 1 and .kind == "nexus-v2-private-alpha-reopen-ingress-commit" and
	  .operationId == $operationId and .planSha256 == $planSha256 and
		 .fpsAdoptionSealSha256 == $fpsAdoptionSealSha256 and
		 .coordinatorSequenceCommitted == true and .automaticClosureWatchdogDisarmed == true and
		 (.siteIngressPrepareResultSha256 | test("^[0-9a-f]{64}$")) and
		 (.bootGuardManifestSha256 | test("^[0-9a-f]{64}$")) and .bootFailClosedGuardRetained == true
	' "${COMMITTED_MARKER}" >/dev/null
}

prepared_marker_matches() {
	[[ -r "${PREPARED_MARKER}" && ! -L "${PREPARED_MARKER}" ]] || return 1
	jq -e --arg operationId "${operation_id}" --arg planSha256 "${plan_sha256}" \
		--arg fpsAdoptionSealSha256 "${fps_adoption_seal_sha256}" '
	  .schemaVersion == 1 and .kind == "nexus-v2-private-alpha-reopen-ingress-commit-prepared" and
	  .operationId == $operationId and .planSha256 == $planSha256 and
	  .fpsAdoptionSealSha256 == $fpsAdoptionSealSha256 and
	  .siteIngressVerified == true and .automaticClosureWatchdogArmed == true
	' "${PREPARED_MARKER}" >/dev/null
}

write_prepared_marker() {
	mkdir -p "${STATE_ROOT}"
	jq -n --sort-keys --arg operationId "${operation_id}" --arg planSha256 "${plan_sha256}" \
		--arg fpsAdoptionSealSha256 "${fps_adoption_seal_sha256}" \
		--arg preparedAtUtc "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
		'{schemaVersion:1,kind:"nexus-v2-private-alpha-reopen-ingress-commit-prepared",
		  operationId:$operationId,planSha256:$planSha256,siteIngressVerified:true,
		  fpsAdoptionSealSha256:$fpsAdoptionSealSha256,
		  automaticClosureWatchdogArmed:true,preparedAtUtc:$preparedAtUtc}' \
		>"${PREPARED_MARKER}.pending"
	chmod 0400 "${PREPARED_MARKER}.pending"
	mv "${PREPARED_MARKER}.pending" "${PREPARED_MARKER}"
}

write_committed_marker() {
	local boot_guard_manifest_sha256
	[[ "${site_prepare_result_sha256}" =~ ^[0-9a-f]{64}$ ]] ||
		die "site-ingress prepare token was not verified"
	require_boot_guard_exact
	boot_guard_manifest_sha256="$(sha256sum "${BOOT_GUARD_MANIFEST}" | awk '{print $1}')"
	mkdir -p "${STATE_ROOT}"
	jq -n --sort-keys --arg operationId "${operation_id}" --arg planSha256 "${plan_sha256}" \
		--arg fpsAdoptionSealSha256 "${fps_adoption_seal_sha256}" \
		--arg siteIngressPrepareResultSha256 "${site_prepare_result_sha256}" \
		--arg bootGuardManifestSha256 "${boot_guard_manifest_sha256}" \
		--arg committedAtUtc "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
		'{schemaVersion:1,kind:"nexus-v2-private-alpha-reopen-ingress-commit",
		  operationId:$operationId,planSha256:$planSha256,coordinatorSequenceCommitted:true,
		  fpsAdoptionSealSha256:$fpsAdoptionSealSha256,
		  siteIngressPrepareResultSha256:$siteIngressPrepareResultSha256,
		  bootGuardManifestSha256:$bootGuardManifestSha256,bootFailClosedGuardRetained:true,
		  automaticClosureWatchdogDisarmed:true,committedAtUtc:$committedAtUtc}' \
		>"${COMMITTED_MARKER}.pending"
	chmod 0400 "${COMMITTED_MARKER}.pending"
	mv "${COMMITTED_MARKER}.pending" "${COMMITTED_MARKER}"
}

require_guard_state() {
	local expected actual unit fragment dropins payload manifest_sha
	require_fps_adoption_seal_exact
	require_boot_guard_exact
	if committed_marker_matches; then
		require_watchdog_absent
		return
	fi
	for payload in "${WATCHDOG_SCRIPT}" "${WATCHDOG_HELPER}" "${WATCHDOG_PLAN}" \
		"${WATCHDOG_NORMAL_CADDY}" "${WATCHDOG_PHASE1_CADDY}" "${WATCHDOG_MANIFEST}" \
		"/etc/systemd/system/${WATCHDOG_SERVICE}" "/etc/systemd/system/${WATCHDOG_TIMER}"; do
		[[ -f "${payload}" && ! -L "${payload}" ]] || die "coordinator watchdog payload is unavailable: ${payload}"
	done
	jq -e --arg operationId "${operation_id}" --arg planSha256 "${plan_sha256}" '
	  keys == ["files","kind","operationId","planSha256","schemaVersion"] and
	  .schemaVersion == 1 and .kind == "nexus-v2-private-alpha-reopen-watchdog-manifest" and
	  .operationId == $operationId and .planSha256 == $planSha256 and
	  (.files | keys == ["helper","normalCaddy","phase1Caddy","plan","script","serviceUnit","timerUnit"]) and
	  ([.files[]] | all(test("^[0-9a-f]{64}$")))
	' "${WATCHDOG_MANIFEST}" >/dev/null || die "coordinator watchdog manifest contract mismatch"
	while IFS=$'\t' read -r expected actual; do
		[[ "${expected}" == "${actual}" ]] || die "coordinator watchdog payload hash drifted"
	done < <(
		printf '%s\t%s\n' "$(jq -er '.files.helper' "${WATCHDOG_MANIFEST}")" "$(sha256sum "${WATCHDOG_HELPER}" | awk '{print $1}')"
		printf '%s\t%s\n' "$(jq -er '.files.plan' "${WATCHDOG_MANIFEST}")" "$(sha256sum "${WATCHDOG_PLAN}" | awk '{print $1}')"
		printf '%s\t%s\n' "$(jq -er '.files.normalCaddy' "${WATCHDOG_MANIFEST}")" "$(sha256sum "${WATCHDOG_NORMAL_CADDY}" | awk '{print $1}')"
		printf '%s\t%s\n' "$(jq -er '.files.phase1Caddy' "${WATCHDOG_MANIFEST}")" "$(sha256sum "${WATCHDOG_PHASE1_CADDY}" | awk '{print $1}')"
		printf '%s\t%s\n' "$(jq -er '.files.script' "${WATCHDOG_MANIFEST}")" "$(sha256sum "${WATCHDOG_SCRIPT}" | awk '{print $1}')"
		printf '%s\t%s\n' "$(jq -er '.files.serviceUnit' "${WATCHDOG_MANIFEST}")" "$(sha256sum "/etc/systemd/system/${WATCHDOG_SERVICE}" | awk '{print $1}')"
		printf '%s\t%s\n' "$(jq -er '.files.timerUnit' "${WATCHDOG_MANIFEST}")" "$(sha256sum "/etc/systemd/system/${WATCHDOG_TIMER}" | awk '{print $1}')"
	)
	[[ "$(sha256sum "${WATCHDOG_HELPER}" | awk '{print $1}')" == "${helper_sha256}" ]] || die "coordinator watchdog helper differs from plan"
	[[ "$(sha256sum "${WATCHDOG_PLAN}" | awk '{print $1}')" == "${plan_sha256}" ]] || die "coordinator watchdog plan differs from authority"
	if [[ -r "${OPEN_MARKER}" && ! -L "${OPEN_MARKER}" ]]; then
		manifest_sha="$(sha256sum "${WATCHDOG_MANIFEST}" | awk '{print $1}')"
		[[ "$(jq -er '.watchdogManifestSha256' "${OPEN_MARKER}")" == "${manifest_sha}" ]] || die "open marker watchdog pin drifted"
	fi
	systemctl is-enabled --quiet "${WATCHDOG_TIMER}" || die "coordinator watchdog is not enabled"
	systemctl is-active --quiet "${WATCHDOG_TIMER}" || die "coordinator watchdog is not active"
	for unit in "${WATCHDOG_TIMER}" "${WATCHDOG_SERVICE}"; do
		fragment="$(systemctl show "${unit}" -p FragmentPath --value)"
		[[ "${fragment}" == "/etc/systemd/system/${unit}" ]] || die "coordinator watchdog fragment drifted: ${unit}"
		dropins="$(systemctl show "${unit}" -p DropInPaths --value)"
		[[ -z "${dropins}" ]] || die "coordinator watchdog drop-in drifted: ${unit}"
	done
}

arm_watchdog() {
	mkdir -p "${STATE_ROOT}"
	chmod 0700 "${STATE_ROOT}"
	install -o root -g root -m 0700 "$0" "${WATCHDOG_HELPER}"
	install -o root -g root -m 0400 "${plan_path}" "${WATCHDOG_PLAN}"
	install -o root -g root -m 0400 "${normal_candidate}" "${WATCHDOG_NORMAL_CADDY}"
	install -o root -g root -m 0400 "${phase1_candidate}" "${WATCHDOG_PHASE1_CADDY}"
	install -o root -g root -m 0400 "${phase1_candidate}" "${BOOT_GUARD_PHASE1}"
	cat >"${WATCHDOG_SCRIPT}" <<EOF
#!/bin/bash
set -euo pipefail
plan_base64="\$(base64 <'${WATCHDOG_PLAN}' | tr -d '\\r\\n')"
normal_base64="\$(base64 <'${WATCHDOG_NORMAL_CADDY}' | tr -d '\\r\\n')"
phase1_base64="\$(base64 <'${WATCHDOG_PHASE1_CADDY}' | tr -d '\\r\\n')"
'${WATCHDOG_HELPER}' close "\${plan_base64}" '${plan_sha256}' '${expected_driver_sha256}' '${helper_sha256}' "\${normal_base64}" "\${phase1_base64}" >>'${STATE_ROOT}/watchdog.log' 2>&1
EOF
	chmod 0700 "${WATCHDOG_SCRIPT}"
	cat >"${BOOT_GUARD_SCRIPT}" <<EOF
#!/bin/bash
set -euo pipefail
test "\$(sha256sum '${BOOT_GUARD_PHASE1}' | awk '{print \$1}')" = '${phase1_caddy_sha256}'
cp '${BOOT_GUARD_PHASE1}' '${REMOTE_CADDYFILE}'
chmod 0644 '${REMOTE_CADDYFILE}'
test "\$(sha256sum '${REMOTE_CADDYFILE}' | awk '{print \$1}')" = '${phase1_caddy_sha256}'
EOF
	chmod 0700 "${BOOT_GUARD_SCRIPT}"
	cat >"/etc/systemd/system/${WATCHDOG_SERVICE}" <<EOF
[Unit]
Description=Fail closed Nexus V2 site ingress if reopen coordinator disappears
StartLimitIntervalSec=0

[Service]
Type=oneshot
ExecStart=${WATCHDOG_SCRIPT}
Restart=on-failure
RestartSec=5s
EOF
	cat >"/etc/systemd/system/${WATCHDOG_TIMER}" <<EOF
[Unit]
Description=Nexus V2 restricted reopen site fail-closed watchdog

[Timer]
OnActiveSec=300
AccuracySec=1s
Persistent=true
Unit=${WATCHDOG_SERVICE}

[Install]
WantedBy=timers.target
EOF
	cat >"/etc/systemd/system/${BOOT_GUARD_SERVICE}" <<EOF
[Unit]
Description=Restore Nexus V2 Phase-1 ingress before Docker after any reboot
DefaultDependencies=no
Wants=local-fs.target
After=local-fs.target
Before=docker.service

[Service]
Type=oneshot
ExecStart=${BOOT_GUARD_SCRIPT}

[Install]
RequiredBy=docker.service
EOF
	chmod 0644 "/etc/systemd/system/${WATCHDOG_SERVICE}" "/etc/systemd/system/${WATCHDOG_TIMER}" \
		"/etc/systemd/system/${BOOT_GUARD_SERVICE}"
	jq -n --sort-keys --arg operationId "${operation_id}" --arg planSha256 "${plan_sha256}" \
		--arg phase1Caddy "$(sha256sum "${BOOT_GUARD_PHASE1}" | awk '{print $1}')" \
		--arg script "$(sha256sum "${BOOT_GUARD_SCRIPT}" | awk '{print $1}')" \
		--arg serviceUnit "$(sha256sum "/etc/systemd/system/${BOOT_GUARD_SERVICE}" | awk '{print $1}')" \
		'{schemaVersion:1,kind:"nexus-v2-private-alpha-reopen-boot-guard-manifest",
		  operationId:$operationId,planSha256:$planSha256,
		  files:{phase1Caddy:$phase1Caddy,script:$script,serviceUnit:$serviceUnit}}' >"${BOOT_GUARD_MANIFEST}.pending"
	chmod 0400 "${BOOT_GUARD_MANIFEST}.pending"
	mv "${BOOT_GUARD_MANIFEST}.pending" "${BOOT_GUARD_MANIFEST}"
	jq -n --sort-keys --arg operationId "${operation_id}" --arg planSha256 "${plan_sha256}" \
		--arg helper "$(sha256sum "${WATCHDOG_HELPER}" | awk '{print $1}')" \
		--arg plan "$(sha256sum "${WATCHDOG_PLAN}" | awk '{print $1}')" \
		--arg normalCaddy "$(sha256sum "${WATCHDOG_NORMAL_CADDY}" | awk '{print $1}')" \
		--arg phase1Caddy "$(sha256sum "${WATCHDOG_PHASE1_CADDY}" | awk '{print $1}')" \
		--arg script "$(sha256sum "${WATCHDOG_SCRIPT}" | awk '{print $1}')" \
		--arg serviceUnit "$(sha256sum "/etc/systemd/system/${WATCHDOG_SERVICE}" | awk '{print $1}')" \
		--arg timerUnit "$(sha256sum "/etc/systemd/system/${WATCHDOG_TIMER}" | awk '{print $1}')" \
		'{schemaVersion:1,kind:"nexus-v2-private-alpha-reopen-watchdog-manifest",
		  operationId:$operationId,planSha256:$planSha256,
		  files:{helper:$helper,normalCaddy:$normalCaddy,phase1Caddy:$phase1Caddy,plan:$plan,
		         script:$script,serviceUnit:$serviceUnit,timerUnit:$timerUnit}}' >"${WATCHDOG_MANIFEST}.pending"
	chmod 0400 "${WATCHDOG_MANIFEST}.pending"
	mv "${WATCHDOG_MANIFEST}.pending" "${WATCHDOG_MANIFEST}"
	systemctl daemon-reload
	systemctl enable "${BOOT_GUARD_SERVICE}" >/dev/null
	systemctl enable --now "${WATCHDOG_TIMER}" >/dev/null
	require_guard_state
}

archive_marker_anomalies() {
	local marker name stamp digest
	stamp="$(date -u +%Y%m%dT%H%M%SZ)"
	mkdir -p "${ANOMALY_DIR}"
	chmod 0700 "${ANOMALY_DIR}"
	for marker in "${OPEN_MARKER}" "${CLOSED_MARKER}" "${PREPARED_MARKER}" "${COMMITTED_MARKER}" "${FPS_ADOPTION_SEAL_FILE}"; do
		[[ -e "${marker}" ]] || continue
		name="$(basename -- "${marker}")"
		if [[ -f "${marker}" && ! -L "${marker}" ]]; then
			digest="$(sha256sum "${marker}" | awk '{print $1}')"
		else
			digest="$(stat -c '%F:%s:%a' "${marker}" | sha256sum | awk '{print $1}')"
		fi
		mv "${marker}" "${ANOMALY_DIR}/${stamp}-${name}-${digest}"
	done
}

verify_open() {
	require_fps_adoption_seal_exact
	require_site_close_guard_absent
	require_identity
	require_deployment_identity
	require_stack_boundary
	require_active_caddy "${normal_caddy_sha256}"
	require_upstream_reads
	require_current_runtime_authority
	require_authority_statuses
	require_authority_liveness_challenge
	require_public_reads
	marker_matches "${OPEN_MARKER}" open || die "open marker mismatch"
	require_guard_state
}

close_site() {
	local container ports service expected count caddy_publications project_ids service_ids
	local -a running_ids
	# Install the denial before the first Docker/Caddy operation. If either daemon
	# hangs or refuses closure, public HTTP(S) is already blocked and this table is
	# deliberately retained.
	install_site_close_guard
	# Always trust the hash-checked plan payload over retained mutable state.
	mkdir -p "${STATE_ROOT}"
	install -o root -g root -m 0400 "${phase1_candidate}" "${RETAINED_PHASE1}"
	if ! (install_caddy "${RETAINED_PHASE1}" "${phase1_caddy_sha256}" && require_phase1_routes); then
		container="$(timeout 20 docker ps -q \
			--filter "label=com.docker.compose.project=${PROJECT_NAME}" \
			--filter 'label=com.docker.compose.service=caddy' 2>/dev/null | sed -n '1p')" ||
			die "cannot enumerate Caddy during guarded closure"
		[[ -z "${container}" ]] || timeout 30 docker stop "${container}" >/dev/null 2>&1 || true
	fi
	require_site_firewall

	# Stop unknown services, duplicates, and containers with alternate
	# publications. Every stop is followed by a fresh authoritative enumeration.
	project_ids="$(timeout 20 docker ps --no-trunc -q \
		--filter "label=com.docker.compose.project=${PROJECT_NAME}")" ||
		die "cannot enumerate the guarded Compose project"
	mapfile -t running_ids < <(printf '%s\n' "${project_ids}" | awk 'NF' | LC_ALL=C sort -u)
	for container in "${running_ids[@]}"; do
		service="$(timeout 20 docker inspect --format '{{index .Config.Labels "com.docker.compose.service"}}' "${container}")" ||
			die "cannot identify a guarded Compose container"
		case "${service}" in
			site|indexer-api|mongo|caddy) ;;
			*) timeout 30 docker stop "${container}" >/dev/null 2>&1 || true ;;
		esac
	done
	for service in site indexer-api mongo caddy; do
		service_ids="$(timeout 20 docker ps --no-trunc -q \
			--filter "label=com.docker.compose.project=${PROJECT_NAME}" \
			--filter "label=com.docker.compose.service=${service}")" ||
			die "cannot enumerate guarded ${service} containers"
		count="$(awk 'NF {count++} END {print count+0}' <<<"${service_ids}")"
		if [[ "${count}" -gt 1 ]]; then
			while IFS= read -r container; do
				[[ -z "${container}" ]] || timeout 30 docker stop "${container}" >/dev/null 2>&1 || true
			done <<<"${service_ids}"
			continue
		fi
		container="$(awk 'NF {print; exit}' <<<"${service_ids}")"
		[[ -n "${container}" ]] || continue
		ports="$(timeout 20 docker inspect --format '{{json .NetworkSettings.Ports}}' "${container}" | jq -c '
		  [to_entries[] as $entry |
		   ($entry.key | split("/")) as $target |
		   ($entry.value // [])[] |
		   "\(.HostIp):\(.HostPort):\($target[0])/\($target[1])"] | sort')" ||
			die "cannot inspect guarded ${service} publications"
		case "${service}" in
			site) expected='["127.0.0.1:3000:3000/tcp"]' ;;
			indexer-api) expected='["127.0.0.1:8787:8787/tcp"]' ;;
			mongo) expected='[]' ;;
			caddy)
				caddy_publications="${ports}"
				if ! jq -e '
				  (length == 2 or length == 4) and
				  (index("0.0.0.0:80:80/tcp") != null) and
				  (index("0.0.0.0:443:443/tcp") != null) and
				  all(. == "0.0.0.0:80:80/tcp" or . == "0.0.0.0:443:443/tcp" or
				      . == ":::80:80/tcp" or . == ":::443:443/tcp")
				' <<<"${caddy_publications}" >/dev/null; then
					timeout 30 docker stop "${container}" >/dev/null 2>&1 || true
				fi
				continue
				;;
		esac
		if [[ "${ports}" != "${expected}" ]]; then
			timeout 30 docker stop "${container}" >/dev/null 2>&1 || true
		fi
	done

	project_ids="$(timeout 20 docker ps --no-trunc -q \
		--filter "label=com.docker.compose.project=${PROJECT_NAME}")" ||
		die "cannot re-enumerate the guarded Compose project"
	mapfile -t running_ids < <(printf '%s\n' "${project_ids}" | awk 'NF' | LC_ALL=C sort -u)
	for container in "${running_ids[@]}"; do
		service="$(timeout 20 docker inspect --format '{{index .Config.Labels "com.docker.compose.service"}}' "${container}")" ||
			die "cannot identify a remaining guarded Compose container"
		case "${service}" in site|indexer-api|mongo|caddy) ;; *) die "unknown Compose service remains running: ${service}" ;; esac
		count="$(timeout 20 docker ps --no-trunc -q \
			--filter "label=com.docker.compose.project=${PROJECT_NAME}" \
			--filter "label=com.docker.compose.service=${service}" | awk 'NF {count++} END {print count+0}')" ||
			die "cannot count remaining ${service} containers"
		[[ "${count}" -eq 1 ]] || die "duplicate Compose service remains running: ${service}"
		ports="$(timeout 20 docker inspect --format '{{json .NetworkSettings.Ports}}' "${container}" | jq -c '
		  [to_entries[] as $entry |
		   ($entry.key | split("/")) as $target |
		   ($entry.value // [])[] |
		   "\(.HostIp):\(.HostPort):\($target[0])/\($target[1])"] | sort')" ||
			die "cannot inspect remaining ${service} publications"
		case "${service}" in
			site) expected='["127.0.0.1:3000:3000/tcp"]' ;;
			indexer-api) expected='["127.0.0.1:8787:8787/tcp"]' ;;
			mongo) expected='[]' ;;
			caddy)
				jq -e '
				  (length == 2 or length == 4) and
				  (index("0.0.0.0:80:80/tcp") != null) and
				  (index("0.0.0.0:443:443/tcp") != null) and
				  all(. == "0.0.0.0:80:80/tcp" or . == "0.0.0.0:443:443/tcp" or
				      . == ":::80:80/tcp" or . == ":::443:443/tcp")
				' <<<"${ports}" >/dev/null || die "remaining Caddy publication drifted"
				continue
				;;
		esac
		[[ "${ports}" == "${expected}" ]] || die "remaining ${service} publication drifted"
	done
	service_ids="$(timeout 20 docker ps --no-trunc -q \
		--filter "label=com.docker.compose.project=${PROJECT_NAME}" \
		--filter 'label=com.docker.compose.service=caddy')" ||
		die "cannot identify Phase-1 Caddy after guarded closure"
	[[ "$(awk 'NF {count++} END {print count+0}' <<<"${service_ids}")" -eq 1 ]] ||
		die "Phase-1 Caddy is not the unique remaining ingress service"
	require_active_caddy "${phase1_caddy_sha256}"
	require_phase1_routes
	require_loopback_or_absent 3000 site
	require_loopback_or_absent 8787 indexer
	# Only an actively probed Phase-1 Caddy permits removal of this denial.
	remove_site_close_guard
	require_site_firewall
	remove_watchdog
	require_watchdog_absent
	remove_boot_guard
	require_boot_guard_absent
}

case "${action}" in
	preflight)
		require_site_close_guard_absent
		require_identity
		require_deployment_identity
		require_authority_statuses
		require_authority_liveness_challenge
		require_stack_boundary
		container="$(caddy_id)"
		validate_caddy_candidate "${normal_candidate}" "${container}"
		validate_caddy_candidate "${phase1_candidate}" "${container}"
		if [[ -r "${OPEN_MARKER}" && ! -L "${OPEN_MARKER}" ]]; then
			fps_adoption_seal_sha256="$(jq -er '.fpsAdoptionSealSha256 | select(test("^[0-9a-f]{64}$"))' "${OPEN_MARKER}")"
			marker_matches "${OPEN_MARKER}" open || die "existing open marker drifted during preflight"
			verify_open
		else
			require_active_caddy "${phase1_caddy_sha256}"
			require_phase1_routes
		fi
		emit_result false false "$(sha256sum "${REMOTE_CADDYFILE}" | awk '{print $1}')" \
			'{"candidateCaddyValidated":true,"credentialsResolved":true,"loopbackServicesPrivate":true,"phase1CaddyPinned":true,"siteFirewallPreserved":true,"sourcePinned":true}'
		;;
	open)
		require_site_close_guard_absent
		require_identity
		require_deployment_identity
		if [[ -e "${OPEN_MARKER}" ]]; then
			marker_matches "${OPEN_MARKER}" open || die "existing open marker drifted"
			verify_open
			emit_result true false "$(sha256sum "${OPEN_MARKER}" | awk '{print $1}')" \
				'{"authorityStatusesSafe":true,"caddyReloaded":true,"coordinatorGuardStateValid":true,"currentRuntimeAuthorityVerified":true,"deploymentIdentityExact":true,"finalSealPinned":true,"fpsAdoptionSealPinned":true,"loopbackServicesPrivate":true,"normalCaddyPinned":true,"publicReadPathsHealthy":true,"sourcePinned":true,"unsafeEconomicRoutesDisabled":true,"upstreamReadPathsHealthy":true}'
			exit 0
		fi
		[[ ! -e "${CLOSED_MARKER}" ]] || die "this operation was closed; capture a new operation to reopen"
		[[ ! -e "${COMMITTED_MARKER}" ]] || die "commit marker exists without an open marker"
		retain_fps_adoption_seal
		require_active_caddy "${phase1_caddy_sha256}"
		retain_candidates
		require_upstream_reads
		require_current_runtime_authority
		require_authority_statuses
		require_authority_liveness_challenge
		open_completed=0
		fail_closed_on_exit() {
			local rc="$?"
			trap - EXIT HUP INT TERM
			set +e
			if [[ "${open_completed}" -ne 1 ]]; then close_site >/dev/null 2>&1; fi
			cleanup_temporary
			exit "${rc:-2}"
		}
		trap fail_closed_on_exit EXIT HUP INT TERM
		arm_watchdog
		install_caddy "${RETAINED_NORMAL}" "${normal_caddy_sha256}"
		require_stack_boundary
		require_public_reads
		write_marker "${OPEN_MARKER}" open
		verify_open
		open_completed=1
		trap cleanup_temporary EXIT
		trap - HUP INT TERM
		emit_result false true "$(sha256sum "${OPEN_MARKER}" | awk '{print $1}')" \
			'{"authorityStatusesSafe":true,"caddyReloaded":true,"coordinatorGuardStateValid":true,"currentRuntimeAuthorityVerified":true,"deploymentIdentityExact":true,"finalSealPinned":true,"fpsAdoptionSealPinned":true,"loopbackServicesPrivate":true,"normalCaddyPinned":true,"publicReadPathsHealthy":true,"sourcePinned":true,"unsafeEconomicRoutesDisabled":true,"upstreamReadPathsHealthy":true}'
		;;
	verify)
		verify_completed=0
		fail_closed_on_verify_exit() {
			local rc="$?"
			trap - EXIT HUP INT TERM
			set +e
			if [[ "${verify_completed}" -ne 1 ]]; then close_site >/dev/null 2>&1; fi
			cleanup_temporary
			exit "${rc:-2}"
		}
		trap fail_closed_on_verify_exit EXIT HUP INT TERM
		verify_open
		verify_completed=1
		trap cleanup_temporary EXIT
		trap - HUP INT TERM
		emit_result true false "$(sha256sum "${OPEN_MARKER}" | awk '{print $1}')" \
			'{"authorityStatusesSafe":true,"caddyReloaded":true,"coordinatorGuardStateValid":true,"currentRuntimeAuthorityVerified":true,"deploymentIdentityExact":true,"finalSealPinned":true,"fpsAdoptionSealPinned":true,"loopbackServicesPrivate":true,"normalCaddyPinned":true,"publicReadPathsHealthy":true,"sourcePinned":true,"unsafeEconomicRoutesDisabled":true,"upstreamReadPathsHealthy":true}'
		;;
	prepare-commit)
		verify_open
		if [[ -e "${PREPARED_MARKER}" ]]; then
			prepared_marker_matches || die "existing site commit-preparation marker drifted"
			prepare_already_applied=true
			prepare_mutation_performed=false
		else
			write_prepared_marker
			prepare_already_applied=false
			prepare_mutation_performed=true
		fi
		prepared_marker_matches || die "site commit-preparation marker was not retained"
		require_guard_state
		emit_result "${prepare_already_applied}" "${prepare_mutation_performed}" \
			"$(sha256sum "${PREPARED_MARKER}" | awk '{print $1}')" \
			'{"authorityStatusesSafe":true,"coordinatorWatchdogArmed":true,"currentRuntimeAuthorityVerified":true,"deploymentIdentityExact":true,"fpsAdoptionSealPinned":true,"restrictedIngressVerified":true}'
		;;
	commit)
		require_site_prepare_token
		prepared_marker_matches || die "site commit was not durably prepared"
		commit_completed=0
		fail_closed_on_commit_exit() {
			local rc="$?"
			trap - EXIT HUP INT TERM
			set +e
			if [[ "${commit_completed}" -ne 1 ]]; then close_site >/dev/null 2>&1; fi
			cleanup_temporary
			exit "${rc:-2}"
		}
		trap fail_closed_on_commit_exit EXIT HUP INT TERM
		verify_open
		write_committed_marker
		remove_watchdog
		verify_open
		commit_completed=1
		trap cleanup_temporary EXIT
		trap - HUP INT TERM
		emit_result false true "$(sha256sum "${COMMITTED_MARKER}" | awk '{print $1}')" \
			'{"authorityStatusesSafe":true,"coordinatorWatchdogDisarmed":true,"currentRuntimeAuthorityVerified":true,"deploymentIdentityExact":true,"fpsAdoptionSealPinned":true,"restrictedIngressVerified":true,"siteIngressPrepareTokenVerified":true}'
		;;
	close)
		archive_marker_anomalies
		close_site
		write_marker "${CLOSED_MARKER}" closed
		emit_result false true "$(sha256sum "${CLOSED_MARKER}" | awk '{print $1}')" \
			'{"coordinatorWatchdogAbsent":true,"loopbackServicesPrivate":true,"markerAnomalyHealed":true,"phase1WriteIngressClosed":true,"publicIngressFailClosed":true,"siteFirewallPreserved":true}'
		;;
esac
