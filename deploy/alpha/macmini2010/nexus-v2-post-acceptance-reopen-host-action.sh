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

# Root-only chain-host action. The underlying node, media, Kubo, and authority
# listeners stay on loopback. systemd-socket-proxyd listens on the single LAN
# address and UFW admits only the final-lock-pinned site host.

[[ $# -ge 1 ]] || {
	printf 'post-acceptance-reopen-host-action: action is required\n' >&2
	exit 2
}
if [[ "$1" == commit ]]; then
	[[ $# -eq 8 ]] || { printf 'post-acceptance-reopen-host-action: commit expects 8 arguments\n' >&2; exit 2; }
else
	[[ $# -eq 7 ]] || { printf 'post-acceptance-reopen-host-action: non-commit action expects 7 arguments\n' >&2; exit 2; }
fi

action="$1"
plan_base64="$2"
plan_sha256="$3"
driver_sha256="$4"
helper_sha256="$5"
# Caddy payloads are deliberately ignored by the chain-host component.
normal_caddy_base64="$6"
phase1_caddy_base64="$7"
site_commit_result_base64="${8:-}"

die() {
	printf 'post-acceptance-reopen-host-action: %s\n' "$*" >&2
	exit 2
}

[[ "${action}" =~ ^(preflight|open|adopt|verify|commit|close)$ ]] || die "invalid action"
[[ "${plan_sha256}" =~ ^[0-9a-f]{64}$ && "${driver_sha256}" =~ ^[0-9a-f]{64}$ ]] || die "invalid authorization hash"
[[ "${helper_sha256}" =~ ^[0-9a-f]{64}$ ]] || die "invalid helper hash"
[[ "$(sha256sum "$0" | awk '{print $1}')" == "${helper_sha256}" ]] || die "helper self-hash mismatch"
[[ -n "${normal_caddy_base64}" && -n "${phase1_caddy_base64}" ]] || die "closed transport envelope is incomplete"

for command in base64 curl flock install ip6tables-save iptables-save jq nft python3 sed sha256sum ss stat systemctl systemd-analyze ufw xxd; do
	command -v "${command}" >/dev/null 2>&1 || die "missing required command: ${command}"
done

plan_path="$(mktemp /tmp/nexus-v2-reopen-plan.XXXXXX)"
site_commit_result_path="$(mktemp /tmp/nexus-v2-site-commit-result.XXXXXX)"
cleanup_temporary() { rm -f "${plan_path}" "${site_commit_result_path}"; }
trap cleanup_temporary EXIT
printf '%s' "${plan_base64}" | base64 -d >"${plan_path}" || die "cannot decode reopen plan"
[[ "$(sha256sum "${plan_path}" | awk '{print $1}')" == "${plan_sha256}" ]] || die "reopen plan hash mismatch"
if [[ "${action}" == commit ]]; then
	[[ -n "${site_commit_result_base64}" ]] || die "chain commit requires a final site-ingress commit token"
	printf '%s' "${site_commit_result_base64}" | base64 -d >"${site_commit_result_path}" ||
		die "cannot decode final site-ingress commit token"
else
	[[ -z "${site_commit_result_base64}" ]] || die "site-ingress commit token is valid only for chain commit"
fi

jq -e '
  .schemaVersion == 1 and
  .kind == "nexus-v2-private-alpha-post-acceptance-reopen-plan" and
  .ports == {authority:8787,chainP2p:30333,chainRpc:9944,ipfsApi:5001,ipfsGateway:8080,media:4000,siteHttp:80,siteHttps:443} and
  .policy.privateAlphaOnly == true and
  .policy.sourceRestrictedToSiteHost == true and
  .policy.phase1BackendsRemainLoopbackOnly == true and
  .policy.chainStateMutationAuthorized == false and
  .policy.chainStateRollbackAuthorized == false and
  .policy.paidOrPublicProductionActivationAuthorized == false and
  .policy.exposedServices == ["authority","chainRpc","ipfsGateway","media"] and
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
final_lock_sha256="$(jq -er '.finalReleaseLock.sha256 | select(test("^[0-9a-f]{64}$"))' "${plan_path}")"
receipt_sha256="$(jq -er '.acceptanceBoundaryReceipt.sha256 | select(test("^[0-9a-f]{64}$"))' "${plan_path}")"
seal_sha256="$(jq -er '.phase2FinalSeal.sha256 | select(test("^[0-9a-f]{64}$"))' "${plan_path}")"
runtime_spec_version="$(jq -er '.runtimeAuthority.runtimeSpecVersion | select(. == 106)' "${plan_path}")"
runtime_code_sha256="$(jq -er '.runtimeAuthority.runtimeCodeSha256 | select(test("^[0-9a-f]{64}$"))' "${plan_path}")"
runtime_metadata_sha256="$(jq -er '.runtimeAuthority.runtimeMetadataScaleSha256 | select(test("^[0-9a-f]{64}$"))' "${plan_path}")"
expected_driver_sha256="$(jq -er '.drivers["chain-transport"].sha256' "${plan_path}")"
expected_helper_sha256="$(jq -er '.helpers["chain-transport"].sha256' "${plan_path}")"
expected_site_driver_sha256="$(jq -er '.drivers["site-ingress"].sha256' "${plan_path}")"
if [[ "${action}" == close ]]; then
	emergency_driver_sha256="$(jq -er '.emergencyClosure.driver.sha256' "${plan_path}")"
	emergency_helper_sha256="$(jq -er '.emergencyClosure.helpers["chain-transport"].sha256' "${plan_path}")"
	[[ "${driver_sha256}" == "${expected_driver_sha256}" || "${driver_sha256}" == "${emergency_driver_sha256}" ]] || die "closure driver plan pin mismatch"
	[[ "${helper_sha256}" == "${expected_helper_sha256}" || "${helper_sha256}" == "${emergency_helper_sha256}" ]] || die "closure helper plan pin mismatch"
else
	[[ "${driver_sha256}" == "${expected_driver_sha256}" && "${helper_sha256}" == "${expected_helper_sha256}" ]] || die "driver/helper plan pin mismatch"
fi
[[ "${chain_ip}" =~ ^192\.168\.[0-9]{1,3}\.[0-9]{1,3}$ || "${chain_ip}" =~ ^10\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}$ || "${chain_ip}" =~ ^172\.(1[6-9]|2[0-9]|3[01])\.[0-9]{1,3}\.[0-9]{1,3}$ ]] || die "chain address is not private IPv4"
[[ "${site_ip}" =~ ^192\.168\.[0-9]{1,3}\.[0-9]{1,3}$ || "${site_ip}" =~ ^10\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}$ || "${site_ip}" =~ ^172\.(1[6-9]|2[0-9]|3[01])\.[0-9]{1,3}\.[0-9]{1,3}$ ]] || die "site address is not private IPv4"
[[ "${chain_ip}" != "${site_ip}" ]] || die "chain and site addresses collide"

site_commit_result_sha256=""
require_site_commit_token() {
	[[ -s "${site_commit_result_path}" && ! -L "${site_commit_result_path}" ]] ||
		die "final site-ingress commit token is unavailable"
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
		.componentId == "site-ingress" and .action == "commit" and .mode == "execute" and
		.result == "passed" and .mutationPerformed == true and
		.componentReceipt == null and
		(.alreadyApplied | type == "boolean") and
		.finalReleaseLockSha256 == $finalReleaseLockSha256 and
		.acceptanceBoundaryReceiptSha256 == $acceptanceBoundaryReceiptSha256 and
		.phase2FinalSealSha256 == $phase2FinalSealSha256 and
		(.fpsAdoptionSealSha256 | test("^[0-9a-f]{64}$")) and
		.driverSha256 == $driverSha256 and
		(.remoteMarkerSha256 | test("^[0-9a-f]{64}$")) and
		.checks == {authorityStatusesSafe:true,coordinatorWatchdogDisarmed:true,fpsAdoptionSealPinned:true,
		 deploymentIdentityExact:true,currentRuntimeAuthorityVerified:true,
		 restrictedIngressVerified:true,siteIngressPrepareTokenVerified:true} and
		(.completedAtUtc | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$"))
	' "${site_commit_result_path}" >/dev/null || die "final site-ingress commit token contract mismatch"
	site_commit_result_sha256="$(sha256sum "${site_commit_result_path}" | awk '{print $1}')"
}

declare -A PORT_BY_SERVICE=(
	[chain-rpc]=9944
	[media]=4000
	[ipfs-gateway]=8080
	[authority]=8787
)
SERVICES=(authority chain-rpc ipfs-gateway media)
FORBIDDEN_PORTS=(30333 5001)
PROTECTED_PORTS=(9944 30333 4000 5001 8080 8787)
UNIT_PREFIX="eterra-alpha-restricted-reopen"
STATE_ROOT="/opt/eterra-alpha/shared/post-acceptance-reopen/${operation_id}/chain-transport"
OPEN_MARKER="${STATE_ROOT}/open.json"
CLOSED_MARKER="${STATE_ROOT}/closed.json"
COMMITTED_MARKER="${STATE_ROOT}/committed.json"
ANOMALY_DIR="${STATE_ROOT}/anomalies"
WATCHDOG_SCRIPT="${STATE_ROOT}/watchdog-close.sh"
WATCHDOG_HELPER="${STATE_ROOT}/watchdog-helper"
WATCHDOG_PLAN="${STATE_ROOT}/watchdog-plan.json"
WATCHDOG_NORMAL_CADDY="${STATE_ROOT}/watchdog-normal.Caddyfile"
WATCHDOG_PHASE1_CADDY="${STATE_ROOT}/watchdog-phase1.Caddyfile"
WATCHDOG_MANIFEST="${STATE_ROOT}/watchdog-manifest.json"
WATCHDOG_SERVICE="${UNIT_PREFIX}-${operation_id}-watchdog.service"
WATCHDOG_TIMER="${UNIT_PREFIX}-${operation_id}-watchdog.timer"
NFT_TABLE="nexus_v2_reopen"
PHASE1_MARKER="/opt/eterra-alpha/shared/state/nexus-v2-phase1-closed-start.json"
RELEASE_FILE="/opt/eterra-alpha/shared/state/release-version.txt"
SOURCE_FILE="/opt/eterra-alpha/shared/state/chain-source-commit.txt"
GENESIS_FILE="/opt/eterra-alpha/shared/state/alpha-genesis-hash.txt"
LOCK_FILE="/run/lock/nexus-v2-post-acceptance-reopen-chain-transport.lock"
exec 9>"${LOCK_FILE}"
flock -x -w 180 9 || die "could not acquire the chain-transport operation lock"

proxy_binary=""
for candidate in /lib/systemd/systemd-socket-proxyd /usr/lib/systemd/systemd-socket-proxyd; do
	if [[ -x "${candidate}" && -f "${candidate}" && ! -L "${candidate}" ]]; then
		proxy_binary="${candidate}"
		break
	fi
done
[[ -n "${proxy_binary}" ]] || die "systemd-socket-proxyd is unavailable"

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
		--arg remoteMarkerSha256 "${marker_sha256}" --arg completedAtUtc "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
		--argjson alreadyApplied "${already_applied}" --argjson mutationPerformed "${mutation_performed}" \
		--argjson checks "${checks_json}" \
		'{schemaVersion:1,kind:"nexus-v2-private-alpha-post-acceptance-reopen-component-result",
		  operationId:$operationId,planSha256:$planSha256,releaseId:$releaseId,
		  siteReleaseVersion:$siteReleaseVersion,
		  sourceCommit:$sourceCommit,siteSourceCommit:$siteSourceCommit,componentId:"chain-transport",
		  action:$action,mode:"execute",result:"passed",mutationPerformed:$mutationPerformed,
		  alreadyApplied:$alreadyApplied,finalReleaseLockSha256:$finalReleaseLockSha256,
		  acceptanceBoundaryReceiptSha256:$acceptanceBoundaryReceiptSha256,
		  phase2FinalSealSha256:$phase2FinalSealSha256,fpsAdoptionSealSha256:null,
		  driverSha256:$driverSha256,
		  remoteMarkerSha256:$remoteMarkerSha256,componentReceipt:null,
		  checks:$checks,completedAtUtc:$completedAtUtc}')"
	printf 'NEXUS_V2_REOPEN_RESULT:%s\n' "$(printf '%s\n' "${payload}" | base64 | tr -d '\n')"
}

require_identity() {
	[[ -r "${RELEASE_FILE}" && "$(cat "${RELEASE_FILE}")" == "${release_id}" ]] || die "deployed release identity mismatch"
	[[ -r "${SOURCE_FILE}" && "$(cat "${SOURCE_FILE}")" == "${source_commit}" ]] || die "deployed chain source identity mismatch"
	[[ -r "${GENESIS_FILE}" && "$(cat "${GENESIS_FILE}")" == "${genesis_hash}" ]] || die "deployed genesis identity mismatch"
	[[ -r "${PHASE1_MARKER}" && ! -L "${PHASE1_MARKER}" ]] || die "Phase-1 start marker is unavailable"
	jq -e --arg releaseId "${release_id}" --arg sourceCommit "${source_commit}" '
	  .schemaVersion == 1 and .kind == "nexus-v2-private-alpha-phase1-closed-start" and
	  .releaseId == $releaseId and .sourceCommit == $sourceCommit and
	  .nodeRpcLoopbackOnly == true and .nodeP2pLoopbackOnly == true and
	  .mediaLoopbackOnly == true and .ipfsApiLoopbackOnly == true and
	  .ipfsGatewayLoopbackOnly == true and .legacyAuthorityLoopbackOnly == true
	' "${PHASE1_MARKER}" >/dev/null || die "Phase-1 start marker contract mismatch"
}

listener_addresses() {
	local port="$1"
	ss -H -lnt "sport = :${port}" | awk '{print $4}' | LC_ALL=C sort -u
}

require_loopback_backend() {
	local port="$1"
	local label="$2"
	local addresses address found=0
	addresses="$(listener_addresses "${port}")"
	[[ -n "${addresses}" ]] || die "${label} listener is missing"
	while IFS= read -r address; do
		case "${address}" in
			127.0.0.1:${port}|\[::1\]:${port}) found=1 ;;
			${chain_ip}:${port}) ;;
			*) die "${label} has an unexpected listener: ${address}" ;;
		esac
	done <<<"${addresses}"
	[[ "${found}" -eq 1 ]] || die "${label} loopback backend is missing"
}

require_phase1_listener() {
	local port="$1"
	local label="$2"
	local addresses address
	addresses="$(listener_addresses "${port}")"
	[[ -n "${addresses}" ]] || die "${label} listener is missing"
	while IFS= read -r address; do
		case "${address}" in
			127.0.0.1:${port}|\[::1\]:${port}) ;;
			*) die "${label} is not Phase-1 loopback-only: ${address}" ;;
		esac
	done <<<"${addresses}"
}

require_closed_or_absent_listener() {
	local port="$1"
	local label="$2"
	local addresses address
	addresses="$(listener_addresses "${port}")"
	[[ -n "${addresses}" ]] || return 0
	while IFS= read -r address; do
		case "${address}" in
			127.0.0.1:${port}|\[::1\]:${port}) ;;
			*) die "${label} remains exposed after closure: ${address}" ;;
		esac
	done <<<"${addresses}"
}

require_proxy_listener() {
	local port="$1"
	local label="$2"
	require_loopback_backend "${port}" "${label}"
	listener_addresses "${port}" | grep -Fx "${chain_ip}:${port}" >/dev/null || die "${label} restricted proxy listener is missing"
}

matching_permit_numbers() {
	local port="$1"
	ufw status numbered | awk -v port="${port}" '
	  /^\[[[:space:]]*[0-9]+\]/ && $0 ~ /(ALLOW|LIMIT) IN/ &&
	  $0 ~ "(^|[[:space:]])" port "(/tcp)?([[:space:]]|$)" {
	    line=$0; sub(/^\[[[:space:]]*/, "", line); sub(/\].*$/, "", line); print line
	  }' | LC_ALL=C sort -rn
}

remove_permit_rules() {
	local port number
	for port in "${PROTECTED_PORTS[@]}"; do
		while :; do
			number="$(matching_permit_numbers "${port}" | sed -n '1p')"
			[[ -n "${number}" ]] || break
			ufw --force delete "${number}" >/dev/null
		done
	done
}

require_firewall_base() {
	local verbose
	verbose="$(ufw status verbose)"
	grep -q '^Status: active$' <<<"${verbose}" || die "UFW is not active"
	grep -q '^Default: deny (incoming)' <<<"${verbose}" || die "UFW default incoming policy is not deny"
}

require_firewall_closed() {
	local port
	require_firewall_base
	for port in "${PROTECTED_PORTS[@]}"; do
		[[ -z "$(matching_permit_numbers "${port}")" ]] || die "firewall permit remains on protected port ${port}"
	done
}

require_firewall_open() {
	local port lines count
	require_firewall_base
	for service in "${SERVICES[@]}"; do
		port="${PORT_BY_SERVICE[${service}]}"
		lines="$(ufw status numbered | awk -v port="${port}" '$0 ~ /(ALLOW|LIMIT) IN/ && $0 ~ ("(^|[[:space:]])" port "(/tcp)?([[:space:]]|$)") {print}')"
		count="$(awk 'NF {count++} END {print count+0}' <<<"${lines}")"
		[[ "${count}" -eq 1 ]] || die "firewall rule count mismatch on ${port}"
		grep -F 'ALLOW IN' <<<"${lines}" >/dev/null || die "firewall action mismatch on ${port}"
		grep -F "${site_ip}" <<<"${lines}" >/dev/null || die "firewall source mismatch on ${port}"
		grep -F "${chain_ip}" <<<"${lines}" >/dev/null || die "firewall destination mismatch on ${port}"
	done
	for port in "${FORBIDDEN_PORTS[@]}"; do
		[[ -z "$(matching_permit_numbers "${port}")" ]] || die "forbidden port ${port} is exposed"
	done
}

remove_nft_guard() {
	if nft list table inet "${NFT_TABLE}" >/dev/null 2>&1; then
		nft delete table inet "${NFT_TABLE}"
	fi
}

install_nft_guard() {
	local rules
	rules="$(mktemp /tmp/nexus-v2-reopen-nft.XXXXXX)"
	cat >"${rules}" <<EOF
table inet ${NFT_TABLE} {
  chain prerouting {
    type filter hook prerouting priority -310; policy accept;
    iifname "lo" accept comment "nexus-v2-prerouting-loopback"
    ip saddr ${site_ip} ip daddr ${chain_ip} tcp dport { 4000, 8080, 8787, 9944 } accept comment "nexus-v2-prerouting-site-source"
    tcp dport { 30333, 4000, 5001, 8080, 8787, 9944 } drop comment "nexus-v2-prerouting-deny-all-other-sources"
  }
  chain input {
    type filter hook input priority -310; policy accept;
    iifname "lo" accept comment "nexus-v2-loopback"
    ip saddr ${site_ip} ip daddr ${chain_ip} tcp dport { 4000, 8080, 8787, 9944 } accept comment "nexus-v2-site-source"
    tcp dport { 30333, 4000, 5001, 8080, 8787, 9944 } drop comment "nexus-v2-deny-all-other-sources"
  }
  chain forward {
    type filter hook forward priority -310; policy accept;
    tcp dport { 30333, 4000, 5001, 8080, 8787, 9944 } drop comment "nexus-v2-forward-deny-protected-services"
  }
}
EOF
	nft -c -f "${rules}"
	remove_nft_guard
	nft -f "${rules}"
	rm -f "${rules}"
}

require_nft_guard_open() {
	local snapshot
	snapshot="$(mktemp /tmp/nexus-v2-reopen-nft-json.XXXXXX)"
	if ! nft -j list table inet "${NFT_TABLE}" >"${snapshot}"; then
		rm -f "${snapshot}"
		die "dedicated nft guard is missing"
	fi
	if ! python3 - "${snapshot}" "${NFT_TABLE}" "${site_ip}" "${chain_ip}" <<'PY'
import json
import pathlib
import sys

path, table_name, site_ip, chain_ip = sys.argv[1:]
value = json.loads(pathlib.Path(path).read_text(encoding="utf-8"))

def normalize(item):
    if isinstance(item, dict):
        result = {
            key: normalize(child)
            for key, child in item.items()
            if key not in {"handle", "index"}
        }
        if set(result) == {"set"} and isinstance(result["set"], list):
            result["set"] = sorted(result["set"], key=lambda child: json.dumps(child, sort_keys=True))
        if (
            set(result) == {"match"}
            and isinstance(result["match"], dict)
            and result["match"].get("op") == "in"
            and isinstance(result["match"].get("right"), dict)
            and set(result["match"]["right"]) == {"set"}
        ):
            result["match"]["op"] = "=="
        return result
    if isinstance(item, list):
        return [normalize(child) for child in item]
    return item

objects = []
for entry in value.get("nftables", []):
    if set(entry) == {"metainfo"}:
        continue
    objects.append(normalize(entry))

expected = [
    {"table": {"family": "inet", "name": table_name}},
    {"chain": {
        "family": "inet", "table": table_name, "name": "prerouting",
        "type": "filter", "hook": "prerouting", "prio": -310, "policy": "accept",
    }},
    {"rule": {
        "family": "inet", "table": table_name, "chain": "prerouting",
        "expr": [
            {"match": {"op": "==", "left": {"meta": {"key": "iifname"}}, "right": "lo"}},
            {"accept": None},
        ],
        "comment": "nexus-v2-prerouting-loopback",
    }},
    {"rule": {
        "family": "inet", "table": table_name, "chain": "prerouting",
        "expr": [
            {"match": {"op": "==", "left": {"payload": {"protocol": "ip", "field": "saddr"}}, "right": site_ip}},
            {"match": {"op": "==", "left": {"payload": {"protocol": "ip", "field": "daddr"}}, "right": chain_ip}},
            {"match": {"op": "==", "left": {"payload": {"protocol": "tcp", "field": "dport"}}, "right": {"set": [4000, 8080, 8787, 9944]}}},
            {"accept": None},
        ],
        "comment": "nexus-v2-prerouting-site-source",
    }},
    {"rule": {
        "family": "inet", "table": table_name, "chain": "prerouting",
        "expr": [
            {"match": {"op": "==", "left": {"payload": {"protocol": "tcp", "field": "dport"}}, "right": {"set": [30333, 4000, 5001, 8080, 8787, 9944]}}},
            {"drop": None},
        ],
        "comment": "nexus-v2-prerouting-deny-all-other-sources",
    }},
    {"chain": {
        "family": "inet", "table": table_name, "name": "input",
        "type": "filter", "hook": "input", "prio": -310, "policy": "accept",
    }},
    {"rule": {
        "family": "inet", "table": table_name, "chain": "input",
        "expr": [
            {"match": {"op": "==", "left": {"meta": {"key": "iifname"}}, "right": "lo"}},
            {"accept": None},
        ],
        "comment": "nexus-v2-loopback",
    }},
    {"rule": {
        "family": "inet", "table": table_name, "chain": "input",
        "expr": [
            {"match": {"op": "==", "left": {"payload": {"protocol": "ip", "field": "saddr"}}, "right": site_ip}},
            {"match": {"op": "==", "left": {"payload": {"protocol": "ip", "field": "daddr"}}, "right": chain_ip}},
            {"match": {"op": "==", "left": {"payload": {"protocol": "tcp", "field": "dport"}}, "right": {"set": [4000, 8080, 8787, 9944]}}},
            {"accept": None},
        ],
        "comment": "nexus-v2-site-source",
    }},
    {"rule": {
        "family": "inet", "table": table_name, "chain": "input",
        "expr": [
            {"match": {"op": "==", "left": {"payload": {"protocol": "tcp", "field": "dport"}}, "right": {"set": [30333, 4000, 5001, 8080, 8787, 9944]}}},
            {"drop": None},
        ],
        "comment": "nexus-v2-deny-all-other-sources",
    }},
    {"chain": {
        "family": "inet", "table": table_name, "name": "forward",
        "type": "filter", "hook": "forward", "prio": -310, "policy": "accept",
    }},
    {"rule": {
        "family": "inet", "table": table_name, "chain": "forward",
        "expr": [
            {"match": {"op": "==", "left": {"payload": {"protocol": "tcp", "field": "dport"}}, "right": {"set": [30333, 4000, 5001, 8080, 8787, 9944]}}},
            {"drop": None},
        ],
        "comment": "nexus-v2-forward-deny-protected-services",
    }},
]
if objects != expected:
    raise SystemExit("dedicated nft guard semantic contract mismatch")
PY
	then
		rm -f "${snapshot}"
		die "dedicated nft guard semantic contract drifted"
	fi
	rm -f "${snapshot}"
	require_no_protected_port_translation
}

require_nft_guard_absent() {
	! nft list table inet "${NFT_TABLE}" >/dev/null 2>&1 || die "dedicated nft guard remains"
	require_no_protected_port_translation
}

require_no_protected_port_translation() {
	local snapshot
	snapshot="$(mktemp /tmp/nexus-v2-reopen-ruleset-json.XXXXXX)"
	if ! nft -j list ruleset >"${snapshot}"; then
		rm -f "${snapshot}"
		die "cannot inspect nftables translation paths"
	fi
	if ! python3 - "${snapshot}" <<'PY'
import json
import ipaddress
import pathlib
import sys

protected = {30333, 4000, 5001, 8080, 8787, 9944}
ruleset = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))

def contains_translation(value):
    if isinstance(value, dict):
        if any(key in value for key in ("dnat", "redirect", "tproxy")):
            return True
        return any(contains_translation(child) for child in value.values())
    if isinstance(value, list):
        return any(contains_translation(child) for child in value)
    return False

def translation_payloads(value):
    result = []
    if isinstance(value, dict):
        for key, child in value.items():
            if key in {"dnat", "redirect", "tproxy"}:
                result.append((key, child))
            result.extend(translation_payloads(child))
    elif isinstance(value, list):
        for child in value:
            result.extend(translation_payloads(child))
    return result

def finite_ports(value):
    if isinstance(value, int) and not isinstance(value, bool):
        return {value}
    if isinstance(value, str) and value.isdigit():
        return {int(value)}
    if isinstance(value, dict) and set(value) == {"set"} and isinstance(value["set"], list):
        result = set()
        for item in value["set"]:
            child = finite_ports(item)
            if child is None:
                return None
            result.update(child)
        return result
    if isinstance(value, dict) and set(value) == {"range"}:
        bounds = value["range"]
        if isinstance(bounds, list) and len(bounds) == 2 and all(isinstance(v, int) for v in bounds):
            low, high = bounds
            return {port for port in protected if low <= port <= high}
    return None

def explicit_udp_only(expr):
    for statement in expr:
        match = statement.get("match") if isinstance(statement, dict) else None
        if not isinstance(match, dict) or match.get("op") != "==":
            continue
        left = match.get("left")
        if left == {"meta": {"key": "l4proto"}} and match.get("right") in {"udp", 17}:
            return True
    return False

def constrained_ports(expr):
    constraints = []
    for statement in expr:
        match = statement.get("match") if isinstance(statement, dict) else None
        if not isinstance(match, dict) or match.get("op") not in {"==", "in"}:
            continue
        left = match.get("left")
        if not (
            isinstance(left, dict)
            and isinstance(left.get("payload"), dict)
            and left["payload"].get("field") == "dport"
            and left["payload"].get("protocol") in {"tcp", "th"}
        ):
            continue
        ports = finite_ports(match.get("right"))
        if ports is None:
            return None
        constraints.append(ports)
    if not constraints:
        return None
    result = constraints[0]
    for item in constraints[1:]:
        result &= item
    return result

def translated_ports(expr, original_ports):
    result = set()
    for _kind, payload in translation_payloads(expr):
        if payload is None:
            ports = original_ports
        elif isinstance(payload, dict) and "port" not in payload:
            ports = original_ports
        elif isinstance(payload, dict):
            ports = finite_ports(payload.get("port"))
        else:
            return None
        if ports is None:
            return None
        result.update(ports)
    return result

def addresses(value):
    if isinstance(value, str):
        try:
            return [ipaddress.ip_network(value, strict=False)]
        except ValueError:
            return None
    if isinstance(value, dict) and set(value) == {"set"} and isinstance(value["set"], list):
        result = []
        for item in value["set"]:
            child = addresses(item)
            if child is None:
                return None
            result.extend(child)
        return result
    return None

def loopback_destination_only(expr):
    constraints = []
    for statement in expr:
        match = statement.get("match") if isinstance(statement, dict) else None
        if not isinstance(match, dict) or match.get("op") not in {"==", "in"}:
            continue
        left = match.get("left")
        if not (
            isinstance(left, dict)
            and isinstance(left.get("payload"), dict)
            and left["payload"].get("field") == "daddr"
            and left["payload"].get("protocol") in {"ip", "ip6"}
        ):
            continue
        values = addresses(match.get("right"))
        if values is None:
            return False
        constraints.extend(values)
    return bool(constraints) and all(network.is_loopback for network in constraints)

for entry in ruleset.get("nftables", []):
    rule = entry.get("rule") if isinstance(entry, dict) else None
    if not isinstance(rule, dict) or not contains_translation(rule.get("expr", [])):
        continue
    expr = rule.get("expr")
    if not isinstance(expr, list):
        raise SystemExit("uninspectable nftables translation rule")
    if explicit_udp_only(expr):
        continue
    ports = constrained_ports(expr)
    targets = translated_ports(expr, ports)
    may_reach_protected = (
        ports is None
        or targets is None
        or bool(ports & protected)
        or bool(targets & protected)
    )
    if may_reach_protected and not loopback_destination_only(expr):
        family = rule.get("family", "?")
        table = rule.get("table", "?")
        chain = rule.get("chain", "?")
        raise SystemExit(f"protected-port translation may precede input guard: {family}/{table}/{chain}")
PY
	then
		rm -f "${snapshot}"
		die "nftables contains a protected-port translation path"
	fi
	rm -f "${snapshot}"
	local legacy4 legacy6
	legacy4="$(mktemp /tmp/nexus-v2-reopen-iptables4.XXXXXX)"
	legacy6="$(mktemp /tmp/nexus-v2-reopen-iptables6.XXXXXX)"
	if ! iptables-save >"${legacy4}" || ! ip6tables-save >"${legacy6}"; then
		rm -f "${legacy4}" "${legacy6}"
		die "cannot inspect legacy iptables translation paths"
	fi
	if ! python3 - "${legacy4}" "${legacy6}" <<'PY'
import ipaddress
import pathlib
import shlex
import sys

protected = {30333, 4000, 5001, 8080, 8787, 9944}

def ports(value):
    result = set()
    try:
        for member in value.split(","):
            member = member.strip()
            if not member:
                return None
            if ":" in member or "-" in member:
                separator = ":" if ":" in member else "-"
                low_raw, high_raw = member.split(separator, 1)
                low, high = int(low_raw), int(high_raw)
                if low > high:
                    return None
                result.update(port for port in protected if low <= port <= high)
            else:
                result.add(int(member))
    except (TypeError, ValueError):
        return None
    return result

def option(tokens, names):
    values = []
    for index, token in enumerate(tokens[:-1]):
        if token in names:
            values.append(tokens[index + 1])
    return values

def destination_is_loopback(tokens):
    values = option(tokens, {"-d", "--destination"})
    if not values:
        return False
    try:
        return all(ipaddress.ip_network(value, strict=False).is_loopback for value in values)
    except ValueError:
        return False

def target_port_values(target, tokens, original):
    values = option(tokens, {"--to-ports", "--on-port"})
    if target == "DNAT":
        for value in option(tokens, {"--to-destination"}):
            candidate = None
            if value.startswith("[") and "]:" in value:
                candidate = value.rsplit("]:", 1)[1]
            elif value.count(":") == 1:
                candidate = value.rsplit(":", 1)[1]
            if candidate is not None:
                values.append(candidate)
    if not values:
        return original
    result = set()
    for value in values:
        child = ports(value)
        if child is None:
            return None
        result.update(child)
    return result

for path in map(pathlib.Path, sys.argv[1:]):
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.startswith("-A "):
            continue
        try:
            tokens = shlex.split(line)
        except ValueError:
            raise SystemExit(f"uninspectable legacy firewall rule: {line}")
        if any(value.lower() == "udp" for value in option(tokens, {"-p", "--protocol"})):
            continue
        jumps = option(tokens, {"-j", "--jump"})
        targets = [value.upper() for value in jumps if value.upper() in {"DNAT", "REDIRECT", "TPROXY"}]
        if not targets:
            continue
        original_values = option(tokens, {"--dport", "--dports", "--destination-port"})
        original = None
        if original_values:
            original = set()
            for value in original_values:
                child = ports(value)
                if child is None:
                    original = None
                    break
                original.update(child)
        target_ports = target_port_values(targets[0], tokens, original)
        may_reach_protected = (
            original is None
            or target_ports is None
            or bool(original & protected)
            or bool(target_ports & protected)
        )
        if may_reach_protected and not destination_is_loopback(tokens):
            raise SystemExit(f"legacy firewall exposes a protected translation path: {line}")
PY
	then
		rm -f "${legacy4}" "${legacy6}"
		die "legacy firewall contains a protected-port translation path"
	fi
	rm -f "${legacy4}" "${legacy6}"
}

unit_name() {
	printf '%s-%s' "${UNIT_PREFIX}" "$1"
}

write_units() {
	local service port stem socket_path service_path stage_root staged_socket staged_service
	stage_root="$(mktemp -d /tmp/nexus-v2-reopen-units.XXXXXX)"
	for service in "${SERVICES[@]}"; do
		port="${PORT_BY_SERVICE[${service}]}"
		stem="$(unit_name "${service}")"
		socket_path="/etc/systemd/system/${stem}.socket"
		service_path="/etc/systemd/system/${stem}.service"
		staged_socket="${stage_root}/${stem}.socket"
		staged_service="${stage_root}/${stem}.service"
		cat >"${staged_socket}" <<EOF
[Unit]
Description=Eterra private Alpha restricted ${service} socket

[Socket]
ListenStream=${chain_ip}:${port}
NoDelay=true
Service=${stem}.service
EOF
		cat >"${staged_service}" <<EOF
[Unit]
Description=Eterra private Alpha restricted ${service} loopback proxy
Requires=${stem}.socket
After=${stem}.socket

[Service]
ExecStart=${proxy_binary} 127.0.0.1:${port}
DynamicUser=yes
NoNewPrivileges=yes
PrivateDevices=yes
PrivateTmp=yes
ProtectHome=yes
ProtectSystem=strict
RestrictAddressFamilies=AF_INET AF_INET6
SystemCallArchitectures=native
EOF
		chmod 0644 "${staged_socket}" "${staged_service}"
		systemd-analyze verify "${staged_socket}" "${staged_service}" >/dev/null
		install -o root -g root -m 0644 "${staged_socket}" "${socket_path}"
		install -o root -g root -m 0644 "${staged_service}" "${service_path}"
	done
	rm -rf -- "${stage_root}"
}

remove_units() {
	local service stem
	for service in "${SERVICES[@]}"; do
		stem="$(unit_name "${service}")"
		systemctl disable --now "${stem}.socket" >/dev/null 2>&1 || true
		systemctl stop "${stem}.service" >/dev/null 2>&1 || true
		rm -f "/etc/systemd/system/${stem}.socket" "/etc/systemd/system/${stem}.service"
	done
	systemctl daemon-reload
}

require_units_open() {
	local service stem port socket_path service_path observed
	for service in "${SERVICES[@]}"; do
		stem="$(unit_name "${service}")"
		port="${PORT_BY_SERVICE[${service}]}"
		socket_path="/etc/systemd/system/${stem}.socket"
		service_path="/etc/systemd/system/${stem}.service"
		[[ -f "${socket_path}" && ! -L "${socket_path}" ]] || die "proxy socket unit missing: ${service}"
		[[ -f "${service_path}" && ! -L "${service_path}" ]] || die "proxy service unit missing: ${service}"
		observed="$(awk -F= '$1 == "ListenStream" {count++; value=substr($0, index($0, "=") + 1)} END {if (count == 1) print value; else exit 1}' "${socket_path}")" || die "proxy socket ListenStream drifted: ${service}"
		[[ "${observed}" == "${chain_ip}:${port}" ]] || die "proxy socket ListenStream mismatch: ${service}"
		observed="$(awk -F= '$1 == "Service" {count++; value=substr($0, index($0, "=") + 1)} END {if (count == 1) print value; else exit 1}' "${socket_path}")" || die "proxy socket Service directive drifted: ${service}"
		[[ "${observed}" == "${stem}.service" ]] || die "proxy socket Service mismatch: ${service}"
		observed="$(awk -F= '$1 == "ExecStart" {count++; value=substr($0, index($0, "=") + 1)} END {if (count == 1) print value; else exit 1}' "${service_path}")" || die "proxy service ExecStart drifted: ${service}"
		[[ "${observed}" == "${proxy_binary} 127.0.0.1:${port}" ]] || die "proxy service target mismatch: ${service}"
		! systemctl is-enabled --quiet "${stem}.socket" || die "proxy socket must remain boot-disabled: ${service}"
		systemctl is-active --quiet "${stem}.socket" || die "proxy socket is not active: ${service}"
		require_proxy_listener "${port}" "${service}"
	done
}

require_units_absent() {
	local service stem suffix unit root load_state
	for service in "${SERVICES[@]}"; do
		stem="$(unit_name "${service}")"
		for suffix in socket service; do
			unit="${stem}.${suffix}"
			for root in /etc/systemd/system /run/systemd/system /usr/local/lib/systemd/system /usr/lib/systemd/system /lib/systemd/system; do
				[[ ! -e "${root}/${unit}" ]] || die "proxy unit remains in ${root}: ${unit}"
			done
			! systemctl is-active --quiet "${unit}" || die "proxy unit remains active: ${unit}"
			! systemctl is-enabled --quiet "${unit}" 2>/dev/null || die "proxy unit remains enabled: ${unit}"
			load_state="$(systemctl show "${unit}" -p LoadState --value 2>/dev/null || true)"
			[[ -z "${load_state}" || "${load_state}" == not-found ]] || die "proxy unit remains loaded: ${unit} (${load_state})"
		done
	done
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

committed_marker_matches() {
	[[ -r "${COMMITTED_MARKER}" && ! -L "${COMMITTED_MARKER}" ]] || return 1
	jq -e --arg operationId "${operation_id}" --arg planSha256 "${plan_sha256}" \
		'.schemaVersion == 1 and .kind == "nexus-v2-private-alpha-reopen-transport-commit" and
		 .operationId == $operationId and .planSha256 == $planSha256 and
		 .coordinatorSequenceCommitted == true and .automaticClosureWatchdogDisarmed == true and
		 (.siteIngressCommitResultSha256 | test("^[0-9a-f]{64}$"))' \
		"${COMMITTED_MARKER}" >/dev/null
}

write_committed_marker() {
	[[ "${site_commit_result_sha256}" =~ ^[0-9a-f]{64}$ ]] ||
		die "final site-ingress commit token was not verified"
	mkdir -p "${STATE_ROOT}"
	jq -n --sort-keys --arg operationId "${operation_id}" --arg planSha256 "${plan_sha256}" \
		--arg siteIngressCommitResultSha256 "${site_commit_result_sha256}" \
		--arg committedAtUtc "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
		'{schemaVersion:1,kind:"nexus-v2-private-alpha-reopen-transport-commit",
		  operationId:$operationId,planSha256:$planSha256,coordinatorSequenceCommitted:true,
		  siteIngressCommitResultSha256:$siteIngressCommitResultSha256,
		  automaticClosureWatchdogDisarmed:true,committedAtUtc:$committedAtUtc}' \
		>"${COMMITTED_MARKER}.pending"
	chmod 0400 "${COMMITTED_MARKER}.pending"
	mv "${COMMITTED_MARKER}.pending" "${COMMITTED_MARKER}"
}

require_guard_state() {
	local manifest_sha expected actual unit fragment dropins payload
	if committed_marker_matches; then
		require_watchdog_absent
		return
	fi
	[[ -r "${WATCHDOG_MANIFEST}" && ! -L "${WATCHDOG_MANIFEST}" ]] || die "coordinator watchdog manifest is unavailable"
	for payload in "${WATCHDOG_SCRIPT}" "${WATCHDOG_HELPER}" "${WATCHDOG_PLAN}" \
		"${WATCHDOG_NORMAL_CADDY}" "${WATCHDOG_PHASE1_CADDY}" \
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
	[[ "$(sha256sum "${WATCHDOG_NORMAL_CADDY}" | awk '{print $1}')" == "$(jq -er '.caddyfiles.normal.sha256' "${plan_path}")" ]] || die "coordinator normal Caddy payload differs from plan"
	[[ "$(sha256sum "${WATCHDOG_PHASE1_CADDY}" | awk '{print $1}')" == "$(jq -er '.caddyfiles.phase1.sha256' "${plan_path}")" ]] || die "coordinator Phase-1 Caddy payload differs from plan"
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
	printf '%s' "${normal_caddy_base64}" | base64 -d >"${WATCHDOG_NORMAL_CADDY}"
	printf '%s' "${phase1_caddy_base64}" | base64 -d >"${WATCHDOG_PHASE1_CADDY}"
	chmod 0400 "${WATCHDOG_NORMAL_CADDY}" "${WATCHDOG_PHASE1_CADDY}"
	cat >"${WATCHDOG_SCRIPT}" <<EOF
#!/bin/bash
set -euo pipefail
plan_base64="\$(base64 <'${WATCHDOG_PLAN}' | tr -d '\\r\\n')"
normal_base64="\$(base64 <'${WATCHDOG_NORMAL_CADDY}' | tr -d '\\r\\n')"
phase1_base64="\$(base64 <'${WATCHDOG_PHASE1_CADDY}' | tr -d '\\r\\n')"
'${WATCHDOG_HELPER}' close "\${plan_base64}" '${plan_sha256}' '${expected_driver_sha256}' '${helper_sha256}' "\${normal_base64}" "\${phase1_base64}" >>'${STATE_ROOT}/watchdog.log' 2>&1
EOF
	chmod 0700 "${WATCHDOG_SCRIPT}"
	cat >"/etc/systemd/system/${WATCHDOG_SERVICE}" <<EOF
[Unit]
Description=Fail closed Nexus V2 chain transport if reopen coordinator disappears
StartLimitIntervalSec=0

[Service]
Type=oneshot
ExecStart=${WATCHDOG_SCRIPT}
Restart=on-failure
RestartSec=5s
EOF
	cat >"/etc/systemd/system/${WATCHDOG_TIMER}" <<EOF
[Unit]
Description=Nexus V2 restricted reopen chain fail-closed watchdog

[Timer]
OnActiveSec=300
AccuracySec=1s
Persistent=true
Unit=${WATCHDOG_SERVICE}

[Install]
WantedBy=timers.target
EOF
	chmod 0644 "/etc/systemd/system/${WATCHDOG_SERVICE}" "/etc/systemd/system/${WATCHDOG_TIMER}"
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
	systemctl enable --now "${WATCHDOG_TIMER}" >/dev/null
	require_guard_state
}

hash_url() {
	local url="$1"
	local temporary
	temporary="$(mktemp /tmp/nexus-v2-reopen-smoke.XXXXXX)"
	curl -fsS --max-time 15 "${url}" >"${temporary}" || {
		rm -f "${temporary}"
		return 1
	}
	sha256sum "${temporary}" | awk '{print $1}'
	rm -f "${temporary}"
}

rpc_call() {
	local method="$1"
	local params="$2"
	curl -fsS --max-time 20 -H 'Content-Type: application/json' \
		--data-binary "$(jq -cn --arg method "${method}" --argjson params "${params}" '{id:1,jsonrpc:"2.0",method:$method,params:$params}')" \
		'http://127.0.0.1:9944'
}

hex_payload_sha256() {
	local value="$1"
	[[ "${value}" =~ ^0x([0-9a-f][0-9a-f])+$ ]] || die "RPC returned noncanonical byte payload"
	printf '%s' "${value#0x}" | xxd -r -p | sha256sum | awk '{print $1}'
}

require_current_runtime_identity() {
	local finalized genesis version code metadata observed
	finalized="$(jq -er '.result | select(test("^0x[0-9a-f]{64}$"))' <<<"$(rpc_call chain_getFinalizedHead '[]')")" || die "finalized head is unavailable"
	genesis="$(jq -er '.result' <<<"$(rpc_call chain_getBlockHash '[0]')")" || die "genesis query failed"
	[[ "${genesis}" == "${genesis_hash}" ]] || die "finalized runtime genesis mismatch"
	version="$(rpc_call state_getRuntimeVersion "[\"${finalized}\"]")" || die "runtime-version query failed"
	observed="$(jq -er '.result.specVersion' <<<"${version}")" || die "runtime spec version is unavailable"
	[[ "${observed}" == "${runtime_spec_version}" ]] || die "current runtime spec version drifted"
	code="$(jq -er '.result' <<<"$(rpc_call state_getStorage "[\"0x3a636f6465\",\"${finalized}\"]")")" || die "runtime code query failed"
	[[ "$(hex_payload_sha256 "${code}")" == "${runtime_code_sha256}" ]] || die "current runtime code hash drifted"
	metadata="$(jq -er '.result' <<<"$(rpc_call state_getMetadata "[\"${finalized}\"]")")" || die "runtime metadata query failed"
	[[ "$(hex_payload_sha256 "${metadata}")" == "${runtime_metadata_sha256}" ]] || die "current runtime metadata hash drifted"
}

require_local_health() {
	local rpc genesis media_path media_sha ipfs_path ipfs_sha
	rpc="$(curl -fsS --max-time 15 -H 'Content-Type: application/json' \
		--data-binary '{"id":1,"jsonrpc":"2.0","method":"chain_getBlockHash","params":[0]}' \
		'http://127.0.0.1:9944')" || die "loopback chain RPC is unavailable"
	[[ "$(jq -er '.result' <<<"${rpc}")" == "${genesis_hash}" ]] || die "loopback chain genesis mismatch"
	curl -fsS --max-time 15 'http://127.0.0.1:4000/health/ready' >/dev/null || die "loopback media readiness failed"
	curl -fsS --max-time 15 'http://127.0.0.1:8787/v1/status' >/dev/null || die "loopback authority readiness failed"
	media_path="$(jq -er '.smoke.mediaPath' "${plan_path}")"
	media_sha="$(jq -er '.smoke.mediaSha256' "${plan_path}")"
	ipfs_path="$(jq -er '.smoke.ipfsPath' "${plan_path}")"
	ipfs_sha="$(jq -er '.smoke.ipfsSha256' "${plan_path}")"
	[[ "$(hash_url "http://127.0.0.1:4000${media_path}")" == "${media_sha}" ]] || die "loopback media content mismatch"
	[[ "$(hash_url "http://127.0.0.1:8080${ipfs_path}")" == "${ipfs_sha}" ]] || die "loopback IPFS gateway content mismatch"
	require_current_runtime_identity
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
		--arg helperSha256 "${helper_sha256}" --arg siteReleaseVersion "${site_release_version}" \
		--arg chainLanIp "${chain_ip}" --arg allowedSourceIp "${site_ip}" \
		--arg state "${expected_state}" '
		.schemaVersion == 1 and .kind == "nexus-v2-private-alpha-post-acceptance-chain-transport-marker" and
		.operationId == $operationId and .planSha256 == $planSha256 and .releaseId == $releaseId and
		.sourceCommit == $sourceCommit and .siteSourceCommit == $siteSourceCommit and
		.siteReleaseVersion == $siteReleaseVersion and .genesisHash == $genesisHash and
		.finalReleaseLockSha256 == $finalReleaseLockSha256 and
		.acceptanceBoundaryReceiptSha256 == $acceptanceBoundaryReceiptSha256 and
		.phase2FinalSealSha256 == $finalSealSha256 and .driverSha256 == $driverSha256 and
		.helperSha256 == $helperSha256 and .chainLanIp == $chainLanIp and
		.allowedSourceIp == $allowedSourceIp and .state == $state and
		((if $state == "open" then .exposedPorts == [4000,8080,8787,9944]
		  else .exposedPorts == [] end)) and
		.forbiddenPorts == [30333,5001] and .underlyingBackendsLoopbackOnly == true and
		((if $state == "open" then (.watchdogManifestSha256 | test("^[0-9a-f]{64}$"))
		  else .watchdogManifestSha256 == null end)) and
		.chainStateMutationPerformed == false and
		.paidOrPublicProductionActivationAuthorized == false
	' "${marker}" >/dev/null
}

current_units_sha256() {
	local service stem suffix path
	{
		for service in "${SERVICES[@]}"; do
			stem="$(unit_name "${service}")"
			for suffix in socket service; do
				path="/etc/systemd/system/${stem}.${suffix}"
				if [[ -f "${path}" ]]; then sha256sum "${path}"; fi
			done
		done
	} | LC_ALL=C sort | sha256sum | awk '{print $1}'
}

write_marker() {
	local marker="$1"
	local state="$2"
	local listeners_sha firewall_sha units_sha watchdog_manifest_sha
	mkdir -p "${STATE_ROOT}"
	chmod 0700 "${STATE_ROOT}"
	listeners_sha="$(ss -H -lnt | LC_ALL=C sort | sha256sum | awk '{print $1}')"
	firewall_sha="$(ufw status verbose | sha256sum | awk '{print $1}')"
	units_sha="$(current_units_sha256)"
	watchdog_manifest_sha=""
	if [[ "${state}" == open ]]; then
		[[ -r "${WATCHDOG_MANIFEST}" && ! -L "${WATCHDOG_MANIFEST}" ]] || die "watchdog manifest is unavailable while writing open marker"
		watchdog_manifest_sha="$(sha256sum "${WATCHDOG_MANIFEST}" | awk '{print $1}')"
	fi
	jq -n --sort-keys \
		--arg operationId "${operation_id}" --arg planSha256 "${plan_sha256}" \
		--arg releaseId "${release_id}" --arg siteReleaseVersion "${site_release_version}" \
		--arg sourceCommit "${source_commit}" \
		--arg siteSourceCommit "${site_source_commit}" --arg genesisHash "${genesis_hash}" \
		--arg finalReleaseLockSha256 "${final_lock_sha256}" --arg acceptanceBoundaryReceiptSha256 "${receipt_sha256}" \
		--arg phase2FinalSealSha256 "${seal_sha256}" --arg driverSha256 "${driver_sha256}" \
		--arg helperSha256 "${helper_sha256}" --arg state "${state}" \
		--arg chainLanIp "${chain_ip}" --arg allowedSourceIp "${site_ip}" \
		--arg listenersSha256 "${listeners_sha}" --arg firewallSha256 "${firewall_sha}" \
		--arg unitsSha256 "${units_sha}" --arg watchdogManifestSha256 "${watchdog_manifest_sha}" \
		--arg observedAtUtc "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
		'{schemaVersion:1,kind:"nexus-v2-private-alpha-post-acceptance-chain-transport-marker",
		  operationId:$operationId,planSha256:$planSha256,releaseId:$releaseId,
		  siteReleaseVersion:$siteReleaseVersion,sourceCommit:$sourceCommit,
		  siteSourceCommit:$siteSourceCommit,genesisHash:$genesisHash,
		  finalReleaseLockSha256:$finalReleaseLockSha256,
		  acceptanceBoundaryReceiptSha256:$acceptanceBoundaryReceiptSha256,
		  phase2FinalSealSha256:$phase2FinalSealSha256,driverSha256:$driverSha256,helperSha256:$helperSha256,
		  state:$state,chainLanIp:$chainLanIp,allowedSourceIp:$allowedSourceIp,
		  exposedPorts:(if $state == "open" then [4000,8080,8787,9944] else [] end),
		  forbiddenPorts:[30333,5001],listenersSha256:$listenersSha256,firewallSha256:$firewallSha256,
		  unitsSha256:$unitsSha256,
		  watchdogManifestSha256:(if $state == "open" then $watchdogManifestSha256 else null end),
		  underlyingBackendsLoopbackOnly:true,chainStateMutationPerformed:false,
		  paidOrPublicProductionActivationAuthorized:false,observedAtUtc:$observedAtUtc}' >"${marker}.pending"
	chmod 0400 "${marker}.pending"
	mv "${marker}.pending" "${marker}"
}

verify_open() {
	local marker_units_sha256
	require_identity
	require_units_open
	require_firewall_open
	require_nft_guard_open
	require_phase1_listener 30333 "chain P2P"
	require_phase1_listener 5001 "IPFS API"
	require_local_health
	marker_matches "${OPEN_MARKER}" open || die "open marker mismatch"
	marker_units_sha256="$(jq -er '.unitsSha256 | select(test("^[0-9a-f]{64}$"))' "${OPEN_MARKER}")" || die "open marker unit hash is invalid"
	[[ "$(current_units_sha256)" == "${marker_units_sha256}" ]] || die "proxy unit bytes drifted after reopen"
	require_guard_state
}

archive_marker_anomalies() {
	local marker name stamp digest
	stamp="$(date -u +%Y%m%dT%H%M%SZ)"
	mkdir -p "${ANOMALY_DIR}"
	chmod 0700 "${ANOMALY_DIR}"
	for marker in "${OPEN_MARKER}" "${CLOSED_MARKER}" "${COMMITTED_MARKER}"; do
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

close_transport() {
	remove_units
	remove_permit_rules
	remove_nft_guard
	require_firewall_closed
	require_nft_guard_absent
	require_units_absent
	require_closed_or_absent_listener 9944 "chain RPC"
	require_closed_or_absent_listener 30333 "chain P2P"
	require_closed_or_absent_listener 4000 "media"
	require_closed_or_absent_listener 5001 "IPFS API"
	require_closed_or_absent_listener 8080 "IPFS gateway"
	require_closed_or_absent_listener 8787 "authority"
	remove_watchdog
	require_watchdog_absent
}

require_phase2_transport_lease() {
	local marker heartbeat expected_marker_sha heartbeat_nonce lease_operation
	local watchdog_service watchdog_timer watchdog_units_sha observed_units_sha
	local watchdog_payload_sha observed_payload_sha producer_root producer_helper producer_plan producer_script
	local updated expires now fragment dropins producer_helper_sha
	marker="$(jq -er '.phase2InternalTransport.lease.markerPath' "${plan_path}")"
	heartbeat="$(jq -er '.phase2InternalTransport.lease.heartbeatPath' "${plan_path}")"
	expected_marker_sha="$(jq -er '.phase2InternalTransport.lease.markerSha256' "${plan_path}")"
	heartbeat_nonce="$(jq -er '.phase2InternalTransport.lease.heartbeatNonce' "${plan_path}")"
	lease_operation="$(jq -er '.phase2InternalTransport.lease.operationId' "${plan_path}")"
	watchdog_service="$(jq -er '.phase2InternalTransport.lease.watchdogService' "${plan_path}")"
	watchdog_timer="$(jq -er '.phase2InternalTransport.lease.watchdogTimer' "${plan_path}")"
	watchdog_units_sha="$(jq -er '.phase2InternalTransport.lease.watchdogUnitSha256' "${plan_path}")"
	watchdog_payload_sha="$(jq -er '.phase2InternalTransport.lease.watchdogPayloadSha256' "${plan_path}")"
	[[ "${marker}" == /opt/eterra-alpha/shared/phase2-internal-transport/* &&
		"${heartbeat}" == /opt/eterra-alpha/shared/phase2-internal-transport/* ]] ||
		die "Phase-2 transport lease paths are outside the protected root"
	[[ "${marker}" == "/opt/eterra-alpha/shared/phase2-internal-transport/${lease_operation}/open.json" &&
		"${heartbeat}" == "/opt/eterra-alpha/shared/phase2-internal-transport/${lease_operation}/heartbeat.json" ]] ||
		die "Phase-2 transport lease paths do not match the locked operation"
	for path in "${marker}" "${heartbeat}"; do
		[[ -f "${path}" && ! -L "${path}" && "$(stat -c '%U:%G:%a' "${path}")" == root:root:400 ]] ||
			die "Phase-2 transport lease artifact is not immutable/root-owned: ${path}"
	done
	[[ "$(sha256sum "${marker}" | awk '{print $1}')" == "${expected_marker_sha}" ]] ||
		die "Phase-2 transport marker hash drifted"
	producer_helper_sha="$(jq -er '.helperSha256 | select(test("^[0-9a-f]{64}$"))' "${marker}")" ||
		die "Phase-2 transport marker helper hash is invalid"
	jq -e --arg operationId "${lease_operation}" \
		--arg producerPlanSha "$(jq -er '.phase2InternalTransport.lease.planSha256' "${plan_path}")" \
		--arg releaseId "${release_id}" --arg sourceCommit "${source_commit}" \
		--arg siteSourceCommit "${site_source_commit}" --arg siteReleaseVersion "${site_release_version}" \
		--arg chainLanIp "${chain_ip}" --arg allowedSourceIp "${site_ip}" '
	  keys == ["allowedSourceIp","chainLanIp","chainStateMutationPerformed","exposedPorts",
	    "forbiddenPorts","helperSha256","kind","openedAtUtc","operationId",
	    "paidOrPublicProductionActivationAuthorized","planSha256","releaseId","schemaVersion",
	    "siteReleaseVersion","siteSourceCommit","sourceCommit","underlyingBackendsLoopbackOnly"] and
	  .schemaVersion == 1 and
	  .kind == "nexus-v2-private-alpha-phase2-internal-transport-marker" and
	  .operationId == $operationId and .planSha256 == $producerPlanSha and
	  .releaseId == $releaseId and .sourceCommit == $sourceCommit and
	  .siteSourceCommit == $siteSourceCommit and .siteReleaseVersion == $siteReleaseVersion and
	  .chainLanIp == $chainLanIp and .allowedSourceIp == $allowedSourceIp and
	  (.helperSha256 | test("^[0-9a-f]{64}$")) and
	  .exposedPorts == [4000,8080,8787,9944] and .forbiddenPorts == [30333,5001] and
	  .underlyingBackendsLoopbackOnly == true and .chainStateMutationPerformed == false and
	  .paidOrPublicProductionActivationAuthorized == false and
	  (.openedAtUtc | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$"))
	' "${marker}" >/dev/null || die "Phase-2 transport marker contract mismatch"
	jq -e --arg operation "${lease_operation}" --arg nonce "${heartbeat_nonce}" \
		--arg markerSha "${expected_marker_sha}" '
	  keys == ["active","expiresAtUtc","kind","markerSha256","nonce","operationId","schemaVersion","updatedAtUtc"] and
	  .schemaVersion == 1 and
	  .kind == "nexus-v2-private-alpha-phase2-internal-transport-heartbeat" and
	  .operationId == $operation and .nonce == $nonce and .markerSha256 == $markerSha and
	  .active == true and
	  (.updatedAtUtc | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$")) and
	  (.expiresAtUtc | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$"))
	' "${heartbeat}" >/dev/null || die "Phase-2 transport heartbeat contract mismatch"
	updated="$(date -u -d "$(jq -er '.updatedAtUtc' "${heartbeat}")" +%s)"
	expires="$(date -u -d "$(jq -er '.expiresAtUtc' "${heartbeat}")" +%s)"
	now="$(date -u +%s)"
	(( updated <= now + 5 && now - updated <= 60 && expires >= now + 300 )) ||
		die "Phase-2 transport heartbeat is stale or expiring"
	for unit in "${watchdog_service}" "${watchdog_timer}"; do
		fragment="$(systemctl show "${unit}" -p FragmentPath --value)"
		[[ "${fragment}" == "/etc/systemd/system/${unit}" && -f "${fragment}" && ! -L "${fragment}" ]] ||
			die "Phase-2 transport watchdog fragment drifted: ${unit}"
		dropins="$(systemctl show "${unit}" -p DropInPaths --value)"
		[[ -z "${dropins}" ]] || die "Phase-2 transport watchdog has unpinned drop-ins: ${unit}"
	done
	systemctl is-active --quiet "${watchdog_timer}" || die "Phase-2 transport watchdog timer is not active"
	systemctl is-enabled --quiet "${watchdog_timer}" || die "Phase-2 transport watchdog timer is not enabled"
	observed_units_sha="$(
		cat "/etc/systemd/system/${watchdog_service}" "/etc/systemd/system/${watchdog_timer}" |
			sha256sum | awk '{print $1}'
	)"
	[[ "${observed_units_sha}" == "${watchdog_units_sha}" ]] ||
		die "Phase-2 transport watchdog unit bytes drifted"
	producer_root="/opt/eterra-alpha/shared/phase2-internal-transport/${lease_operation}"
	producer_helper="${producer_root}/watchdog-helper"
	producer_plan="${producer_root}/watchdog-plan.json"
	producer_script="${producer_root}/watchdog-check.sh"
	[[ "$(stat -c '%U:%G:%a' "${producer_helper}")" == root:root:700 &&
		"$(stat -c '%U:%G:%a' "${producer_plan}")" == root:root:400 &&
		"$(stat -c '%U:%G:%a' "${producer_script}")" == root:root:700 ]] ||
		die "Phase-2 transport watchdog payload ownership/mode drifted"
	for path in "${producer_helper}" "${producer_plan}" "${producer_script}"; do
		[[ -f "${path}" && ! -L "${path}" ]] || die "Phase-2 watchdog payload is unavailable: ${path}"
	done
	[[ "$(sha256sum "${producer_helper}" | awk '{print $1}')" == "${producer_helper_sha}" ]] ||
		die "Phase-2 watchdog helper bytes drifted"
	[[ "$(sha256sum "${producer_plan}" | awk '{print $1}')" == "$(jq -er '.phase2InternalTransport.lease.planSha256' "${plan_path}")" ]] ||
		die "Phase-2 watchdog plan bytes drifted"
	observed_payload_sha="$(
		sha256sum "${producer_helper}" "${producer_plan}" "${producer_script}" |
			sha256sum | awk '{print $1}'
	)"
	[[ "${observed_payload_sha}" == "${watchdog_payload_sha}" ]] ||
		die "Phase-2 transport watchdog payload bytes drifted"
	require_units_open
	require_firewall_open
	require_nft_guard_open
	require_phase1_listener 30333 "chain P2P"
	require_phase1_listener 5001 "IPFS API"
	require_local_health
}

retire_phase2_watchdog() {
	local service timer
	service="$(jq -er '.phase2InternalTransport.lease.watchdogService' "${plan_path}")"
	timer="$(jq -er '.phase2InternalTransport.lease.watchdogTimer' "${plan_path}")"
	for unit in "${timer}" "${service}"; do
		systemctl disable --now "${unit}" >/dev/null 2>&1 || true
	done
	rm -f -- "/etc/systemd/system/${timer}" "/etc/systemd/system/${service}"
	systemctl daemon-reload
	for unit in "${timer}" "${service}"; do
		! systemctl is-active --quiet "${unit}" || die "Phase-2 watchdog remains active: ${unit}"
		! systemctl is-enabled --quiet "${unit}" 2>/dev/null || die "Phase-2 watchdog remains enabled: ${unit}"
	done
}

case "${action}" in
	preflight)
		require_identity
		require_local_health
		require_phase1_listener 30333 "chain P2P"
		require_phase1_listener 5001 "IPFS API"
		if marker_matches "${OPEN_MARKER}" open; then
			verify_open
		else
			require_phase2_transport_lease
		fi
		preflight_marker="$(sha256sum "${PHASE1_MARKER}" | awk '{print $1}')"
		emit_result false false "${preflight_marker}" \
			'{"credentialsResolved":true,"firewallDefaultDeny":true,"noForbiddenExposure":true,"phase1LoopbackPreserved":true,"sourcePinned":true,"systemdSocketProxyAvailable":true}'
		;;
	adopt)
		require_identity
		if marker_matches "${OPEN_MARKER}" open; then
			verify_open
			emit_result true false "$(sha256sum "${OPEN_MARKER}" | awk '{print $1}')" \
				'{"coordinatorWatchdogAdopted":true,"currentRuntimeAuthorityVerified":true,"finalSealPinned":true,"inheritedLeaseExact":true,"phase2TransportHandoffPinned":true,"restrictedTransportVerified":true,"sourcePinned":true}'
			exit 0
		fi
		require_phase2_transport_lease
		[[ ! -e "${CLOSED_MARKER}" && ! -e "${COMMITTED_MARKER}" ]] ||
			die "reopen operation already closed or committed"
		arm_watchdog
		write_marker "${OPEN_MARKER}" open
		retire_phase2_watchdog
		verify_open
		emit_result false true "$(sha256sum "${OPEN_MARKER}" | awk '{print $1}')" \
			'{"coordinatorWatchdogAdopted":true,"currentRuntimeAuthorityVerified":true,"finalSealPinned":true,"inheritedLeaseExact":true,"phase2TransportHandoffPinned":true,"restrictedTransportVerified":true,"sourcePinned":true}'
		;;
	open)
		require_identity
		if [[ -e "${OPEN_MARKER}" ]]; then
			marker_matches "${OPEN_MARKER}" open || die "existing open marker drifted"
			verify_open
			emit_result true false "$(sha256sum "${OPEN_MARKER}" | awk '{print $1}')" \
				'{"coordinatorGuardStateValid":true,"currentRuntimeAuthorityVerified":true,"dedicatedNftGuardExact":true,"finalSealPinned":true,"forbiddenPortsClosed":true,"localReadPathsHealthy":true,"loopbackBackendsPreserved":true,"restrictedProxyUnitsInstalled":true,"sourceFirewallRulesExact":true,"sourcePinned":true}'
			exit 0
		fi
		[[ ! -e "${CLOSED_MARKER}" ]] || die "this operation was closed; capture a new operation to reopen"
		[[ ! -e "${COMMITTED_MARKER}" ]] || die "commit marker exists without an open marker"
		require_units_absent
		require_firewall_closed
		require_nft_guard_absent
		for port in 9944 30333 4000 5001 8080 8787; do require_phase1_listener "${port}" "protected service ${port}"; done
		open_completed=0
		fail_closed_on_exit() {
			local rc="$?"
			trap - EXIT HUP INT TERM
			set +e
			if [[ "${open_completed}" -ne 1 ]]; then close_transport >/dev/null 2>&1; fi
			cleanup_temporary
			exit "${rc:-2}"
		}
		trap fail_closed_on_exit EXIT HUP INT TERM
		arm_watchdog
		remove_permit_rules
		install_nft_guard
		for service in "${SERVICES[@]}"; do
			port="${PORT_BY_SERVICE[${service}]}"
			ufw allow proto tcp from "${site_ip}" to "${chain_ip}" port "${port}" comment "nexus-v2-reopen-${operation_id}-${service}" >/dev/null
		done
		write_units
		systemctl daemon-reload
		# Reopen proxies are deliberately runtime-only. A reboot is fail-closed and
		# requires a newly verified operation before any LAN listener returns.
		for service in "${SERVICES[@]}"; do systemctl start "$(unit_name "${service}").socket" >/dev/null; done
		require_units_open
		require_firewall_open
		require_nft_guard_open
		require_local_health
		write_marker "${OPEN_MARKER}" open
		verify_open
		open_completed=1
		trap cleanup_temporary EXIT
		trap - HUP INT TERM
		emit_result false true "$(sha256sum "${OPEN_MARKER}" | awk '{print $1}')" \
			'{"coordinatorGuardStateValid":true,"currentRuntimeAuthorityVerified":true,"dedicatedNftGuardExact":true,"finalSealPinned":true,"forbiddenPortsClosed":true,"localReadPathsHealthy":true,"loopbackBackendsPreserved":true,"restrictedProxyUnitsInstalled":true,"sourceFirewallRulesExact":true,"sourcePinned":true}'
		;;
	verify)
		verify_completed=0
		fail_closed_on_verify_exit() {
			local rc="$?"
			trap - EXIT HUP INT TERM
			set +e
			if [[ "${verify_completed}" -ne 1 ]]; then close_transport >/dev/null 2>&1; fi
			cleanup_temporary
			exit "${rc:-2}"
		}
		trap fail_closed_on_verify_exit EXIT HUP INT TERM
		verify_open
		verify_completed=1
		trap cleanup_temporary EXIT
		trap - HUP INT TERM
		emit_result true false "$(sha256sum "${OPEN_MARKER}" | awk '{print $1}')" \
			'{"coordinatorGuardStateValid":true,"currentRuntimeAuthorityVerified":true,"dedicatedNftGuardExact":true,"finalSealPinned":true,"forbiddenPortsClosed":true,"localReadPathsHealthy":true,"loopbackBackendsPreserved":true,"restrictedProxyUnitsInstalled":true,"sourceFirewallRulesExact":true,"sourcePinned":true}'
		;;
	commit)
		require_site_commit_token
		commit_completed=0
		fail_closed_on_commit_exit() {
			local rc="$?"
			trap - EXIT HUP INT TERM
			set +e
			if [[ "${commit_completed}" -ne 1 ]]; then close_transport >/dev/null 2>&1; fi
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
			'{"coordinatorWatchdogDisarmed":true,"currentRuntimeAuthorityVerified":true,"restrictedTransportVerified":true,"siteIngressCommitTokenVerified":true}'
		;;
	close)
		retire_phase2_watchdog
		archive_marker_anomalies
		close_transport
		write_marker "${CLOSED_MARKER}" closed
		emit_result false true "$(sha256sum "${CLOSED_MARKER}" | awk '{print $1}')" \
			'{"chainStateUntouched":true,"coordinatorWatchdogAbsent":true,"dedicatedNftGuardAbsent":true,"forbiddenPortsClosed":true,"markerAnomalyHealed":true,"phase1LoopbackPreserved":true,"proxyUnitsAbsent":true,"reopenFirewallRulesAbsent":true}'
		;;
esac
