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

# Root-only temporary Phase-2 transport producer. It changes only firewall,
# nftables, runtime-only socket proxies, and its protected lease/watchdog state.
# Node, media, IPFS, authority, chain state, and public site ingress are never
# mutated by this helper.

[[ $# -eq 4 ]] || {
	printf 'phase2-internal-transport-host-action: expected 4 arguments\n' >&2
	exit 2
}
action="$1"
plan_base64="$2"
plan_sha256="$3"
helper_sha256="$4"

die() {
	printf 'phase2-internal-transport-host-action: %s\n' "$*" >&2
	exit 2
}

[[ "${action}" =~ ^(execute|renew|verify|close|watchdog)$ ]] || die "invalid action"
[[ "${plan_sha256}" =~ ^[0-9a-f]{64}$ && "${helper_sha256}" =~ ^[0-9a-f]{64}$ ]] || die "invalid authority hash"
[[ "$(sha256sum "$0" | awk '{print $1}')" == "${helper_sha256}" ]] || die "helper self-hash mismatch"
for command in awk base64 cmp curl date flock grep install ip6tables-save iptables-save jq mktemp nft python3 sed sha256sum ss stat systemctl systemd-analyze ufw; do
	command -v "${command}" >/dev/null 2>&1 || die "missing required command: ${command}"
done

plan_path="$(mktemp /tmp/nexus-v2-phase2-transport-plan.XXXXXX)"
cleanup_temporary() { rm -f -- "${plan_path}"; }
trap cleanup_temporary EXIT
printf '%s' "${plan_base64}" | base64 -d >"${plan_path}" || die "cannot decode plan"
[[ "$(sha256sum "${plan_path}" | awk '{print $1}')" == "${plan_sha256}" ]] || die "plan hash mismatch"
jq -e '
  .schemaVersion == 1 and
  .kind == "nexus-v2-private-alpha-phase2-internal-transport-plan" and
  .leaseDurationSeconds == 900 and
  .network == {allowedSourceIp:"192.168.1.218",chainLanIp:"192.168.1.159",siteLanIp:"192.168.1.218"} and
  .ports == {authority:8787,chainRpc:9944,forbidden:[30333,5001],ipfsGateway:8080,media:4000} and
  .policy == {chainStateMutationAuthorized:false,paidOrPublicActivationAuthorized:false,
    phase1PublicCaddyMustRemainUnchanged:true,privateAlphaOnly:true,
    publicIngressMutationAuthorized:false,sourceRestrictedToSiteHost:true,
    underlyingBackendsRemainLoopbackOnly:true}
' "${plan_path}" >/dev/null || die "plan policy mismatch"

operation_id="$(jq -er '.operationId | select(test("^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$"))' "${plan_path}")"
release_id="$(jq -er '.releaseId | select(test("^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$"))' "${plan_path}")"
source_commit="$(jq -er '.sourceCommit | select(test("^[0-9a-f]{40}$"))' "${plan_path}")"
site_source_commit="$(jq -er '.siteSourceCommit | select(test("^[0-9a-f]{40}$"))' "${plan_path}")"
site_release_version="$(jq -er '.siteReleaseVersion' "${plan_path}")"
chain_ip="$(jq -er '.network.chainLanIp' "${plan_path}")"
site_ip="$(jq -er '.network.allowedSourceIp' "${plan_path}")"
plan_expires="$(date -u -d "$(jq -er '.expiresAtUtc' "${plan_path}")" +%s)" || die "invalid plan expiry"
now_epoch="$(date -u +%s)"
if [[ "${action}" != close && "${action}" != watchdog ]]; then
	(( plan_expires > now_epoch )) || die "plan expired"
fi

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
NFT_TABLE="nexus_v2_reopen"
STATE_BASE="/opt/eterra-alpha/shared/phase2-internal-transport"
STATE_ROOT="${STATE_BASE}/${operation_id}"
MARKER="${STATE_ROOT}/open.json"
HEARTBEAT="${STATE_ROOT}/heartbeat.json"
CLOSED_MARKER="${STATE_ROOT}/closed.json"
INSTALLED_HELPER="${STATE_ROOT}/watchdog-helper"
INSTALLED_PLAN="${STATE_ROOT}/watchdog-plan.json"
WATCHDOG_SCRIPT="${STATE_ROOT}/watchdog-check.sh"
WATCHDOG_SERVICE="nexus-v2-phase2-internal-transport-${operation_id}.service"
WATCHDOG_TIMER="nexus-v2-phase2-internal-transport-${operation_id}.timer"
PHASE1_MARKER="/opt/eterra-alpha/shared/state/nexus-v2-phase1-closed-start.json"
RELEASE_FILE="/opt/eterra-alpha/shared/state/release-version.txt"
SOURCE_FILE="/opt/eterra-alpha/shared/state/chain-source-commit.txt"
LOCK_FILE="/run/lock/nexus-v2-post-acceptance-reopen-chain-transport.lock"
exec 9>"${LOCK_FILE}"
flock -x -w 180 9 || die "could not acquire chain-transport lock"

require_root_owned_directory() {
	local path="$1" expected_mode="${2:-}" observed_mode
	[[ -d "${path}" && ! -L "${path}" ]] || die "protected state ancestry is not a directory: ${path}"
	[[ "$(stat -c '%U:%G' "${path}")" == root:root ]] || die "protected state ancestry is not root-owned: ${path}"
	observed_mode="$(stat -c '%a' "${path}")"
	if [[ -n "${expected_mode}" ]]; then
		[[ "${observed_mode}" == "${expected_mode}" ]] || die "protected state directory mode drifted: ${path}"
	else
		(( (8#${observed_mode} & 8#022) == 0 )) || die "protected state ancestry is writable outside root: ${path}"
	fi
}

ensure_state_base() {
	local ancestor
	for ancestor in /opt /opt/eterra-alpha /opt/eterra-alpha/shared; do
		require_root_owned_directory "${ancestor}"
	done
	if [[ -e "${STATE_BASE}" || -L "${STATE_BASE}" ]]; then
		require_root_owned_directory "${STATE_BASE}" 700
	else
		install -d -o root -g root -m 0700 "${STATE_BASE}"
		require_root_owned_directory "${STATE_BASE}" 700
	fi
}

state_root_exists() {
	[[ -e "${STATE_ROOT}" || -L "${STATE_ROOT}" ]]
}

require_state_root() {
	require_root_owned_directory "${STATE_ROOT}" 700
}

create_state_root() {
	state_root_exists && die "refusing to replace existing Phase-2 operation root"
	mkdir -- "${STATE_ROOT}"
	chown root:root "${STATE_ROOT}"
	chmod 0700 "${STATE_ROOT}"
	require_state_root
}

open_exclusive_pending() {
	local path="$1"
	[[ "${path}" == "${STATE_ROOT}/"*.pending ]] || die "pending path escaped the protected operation root"
	[[ ! -e "${path}" && ! -L "${path}" ]] || die "protected pending file already exists: ${path}"
	set -C
	exec 8>"${path}"
	set +C
}

proxy_binary=""
for candidate in /lib/systemd/systemd-socket-proxyd /usr/lib/systemd/systemd-socket-proxyd; do
	if [[ -x "${candidate}" && -f "${candidate}" && ! -L "${candidate}" ]]; then
		proxy_binary="${candidate}"
		break
	fi
done
[[ -n "${proxy_binary}" ]] || die "systemd-socket-proxyd is unavailable"

listener_addresses() {
	local port="$1"
	ss -H -lnt "sport = :${port}" | awk '{print $4}' | LC_ALL=C sort -u
}

require_phase1_listener() {
	local port="$1" label="$2" addresses address
	addresses="$(listener_addresses "${port}")"
	[[ -n "${addresses}" ]] || die "${label} listener is missing"
	while IFS= read -r address; do
		case "${address}" in
			127.0.0.1:${port}|\[::1\]:${port}) ;;
			*) die "${label} is not loopback-only: ${address}" ;;
		esac
	done <<<"${addresses}"
}

require_proxy_listener() {
	local port="$1" label="$2" addresses address loopback=0 proxy=0
	addresses="$(listener_addresses "${port}")"
	while IFS= read -r address; do
		case "${address}" in
			127.0.0.1:${port}|\[::1\]:${port}) loopback=1 ;;
			${chain_ip}:${port}) proxy=1 ;;
			*) die "${label} has an unexpected listener: ${address}" ;;
		esac
	done <<<"${addresses}"
	[[ "${loopback}" -eq 1 && "${proxy}" -eq 1 ]] || die "${label} loopback/proxy listener is incomplete"
}

require_closed_listener() {
	local port="$1" label="$2" address
	while IFS= read -r address; do
		[[ -z "${address}" ]] && continue
		case "${address}" in
			127.0.0.1:${port}|\[::1\]:${port}) ;;
			*) die "${label} remains externally exposed: ${address}" ;;
		esac
	done <<<"$(listener_addresses "${port}")"
}

require_identity() {
	[[ -f "${RELEASE_FILE}" && ! -L "${RELEASE_FILE}" && "$(cat "${RELEASE_FILE}")" == "${release_id}" ]] || die "deployed release mismatch"
	[[ -f "${SOURCE_FILE}" && ! -L "${SOURCE_FILE}" && "$(cat "${SOURCE_FILE}")" == "${source_commit}" ]] || die "deployed source mismatch"
	[[ -f "${PHASE1_MARKER}" && ! -L "${PHASE1_MARKER}" ]] || die "Phase-1 marker is unavailable"
	jq -e --arg releaseId "${release_id}" --arg sourceCommit "${source_commit}" '
      .schemaVersion == 1 and .kind == "nexus-v2-private-alpha-phase1-closed-start" and
      .releaseId == $releaseId and .sourceCommit == $sourceCommit and
      .nodeRpcLoopbackOnly == true and .nodeP2pLoopbackOnly == true and
      .mediaLoopbackOnly == true and .ipfsApiLoopbackOnly == true and
      .ipfsGatewayLoopbackOnly == true and .legacyAuthorityLoopbackOnly == true
    ' "${PHASE1_MARKER}" >/dev/null || die "Phase-1 marker contract mismatch"
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
	local value
	value="$(ufw status verbose)"
	grep -q '^Status: active$' <<<"${value}" || die "UFW is not active"
	grep -q '^Default: deny (incoming)' <<<"${value}" || die "UFW incoming default is not deny"
}

require_firewall_open() {
	local service port lines count
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

require_firewall_closed() {
	local port
	require_firewall_base
	for port in "${PROTECTED_PORTS[@]}"; do
		[[ -z "$(matching_permit_numbers "${port}")" ]] || die "firewall permit remains on ${port}"
	done
}

remove_nft_guard() {
	if nft list table inet "${NFT_TABLE}" >/dev/null 2>&1; then nft delete table inet "${NFT_TABLE}"; fi
}

install_nft_guard() {
	local rules
	rules="$(mktemp /tmp/nexus-v2-phase2-nft.XXXXXX)"
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
	rm -f -- "${rules}"
}

require_nft_guard_open() {
	local snapshot
	snapshot="$(mktemp /tmp/nexus-v2-phase2-nft-json.XXXXXX)"
	nft -j list table inet "${NFT_TABLE}" >"${snapshot}" || { rm -f -- "${snapshot}"; die "dedicated nft guard is missing"; }
	python3 - "${snapshot}" "${site_ip}" "${chain_ip}" <<'PY' || { rm -f -- "${snapshot}"; die "dedicated nft guard semantic contract drifted"; }
import json, pathlib, sys
value = json.loads(pathlib.Path(sys.argv[1]).read_text())
site_ip, chain_ip = sys.argv[2:]
comments = {}
for entry in value.get("nftables", []):
    rule = entry.get("rule") if isinstance(entry, dict) else None
    if isinstance(rule, dict) and isinstance(rule.get("comment"), str):
        comments[rule["comment"]] = rule
required = {
    "nexus-v2-prerouting-loopback", "nexus-v2-prerouting-site-source",
    "nexus-v2-prerouting-deny-all-other-sources", "nexus-v2-loopback",
    "nexus-v2-site-source", "nexus-v2-deny-all-other-sources",
    "nexus-v2-forward-deny-protected-services",
}
if set(comments) != required:
    raise SystemExit("dedicated nft guard comment set mismatch")
serialized = json.dumps(comments, sort_keys=True)
for required_value in (site_ip, chain_ip, "4000", "8080", "8787", "9944", "30333", "5001"):
    if required_value not in serialized:
        raise SystemExit("dedicated nft guard binding is incomplete")
for name in ("nexus-v2-prerouting-site-source", "nexus-v2-site-source"):
    if "accept" not in json.dumps(comments[name]):
        raise SystemExit("site-source rule is not accepting")
for name in required - {"nexus-v2-prerouting-loopback", "nexus-v2-loopback", "nexus-v2-prerouting-site-source", "nexus-v2-site-source"}:
    if "drop" not in json.dumps(comments[name]):
        raise SystemExit("deny rule is not dropping")
PY
	rm -f -- "${snapshot}"
	require_no_protected_port_translation
}

require_nft_guard_absent() {
	! nft list table inet "${NFT_TABLE}" >/dev/null 2>&1 || die "dedicated nft guard remains"
	require_no_protected_port_translation
}

require_no_protected_port_translation() {
	local nft_snapshot legacy4 legacy6
	nft_snapshot="$(mktemp /tmp/nexus-v2-phase2-ruleset-json.XXXXXX)"
	nft -j list ruleset >"${nft_snapshot}" || {
		rm -f -- "${nft_snapshot}"
		die "cannot inspect nftables translation paths"
	}
	python3 - "${nft_snapshot}" <<'PY' || {
import ipaddress
import json
import pathlib
import sys

protected = {30333, 4000, 5001, 8080, 8787, 9944}
ruleset = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))

def translations(value):
    result = []
    if isinstance(value, dict):
        for key, child in value.items():
            if key in {"dnat", "redirect", "tproxy"}:
                result.append(child)
            result.extend(translations(child))
    elif isinstance(value, list):
        for child in value:
            result.extend(translations(child))
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
        if isinstance(bounds, list) and len(bounds) == 2 and all(isinstance(item, int) for item in bounds):
            low, high = bounds
            return {port for port in protected if low <= port <= high}
    return None

def constrained_ports(expr):
    constraints = []
    for statement in expr:
        match = statement.get("match") if isinstance(statement, dict) else None
        if not isinstance(match, dict) or match.get("op") not in {"==", "in"}:
            continue
        left = match.get("left")
        payload = left.get("payload") if isinstance(left, dict) else None
        if not isinstance(payload, dict) or payload.get("field") != "dport" or payload.get("protocol") not in {"tcp", "th"}:
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

def translated_ports(expr, original):
    result = set()
    for payload in translations(expr):
        if payload is None or (isinstance(payload, dict) and "port" not in payload):
            ports = original
        elif isinstance(payload, dict):
            ports = finite_ports(payload.get("port"))
        else:
            return None
        if ports is None:
            return None
        result.update(ports)
    return result

def explicit_udp_only(expr):
    for statement in expr:
        match = statement.get("match") if isinstance(statement, dict) else None
        if isinstance(match, dict) and match.get("op") == "==" and match.get("left") == {"meta": {"key": "l4proto"}} and match.get("right") in {"udp", 17}:
            return True
    return False

def loopback_destination_only(expr):
    networks = []
    for statement in expr:
        match = statement.get("match") if isinstance(statement, dict) else None
        if not isinstance(match, dict) or match.get("op") not in {"==", "in"}:
            continue
        left = match.get("left")
        payload = left.get("payload") if isinstance(left, dict) else None
        if not isinstance(payload, dict) or payload.get("field") != "daddr" or payload.get("protocol") not in {"ip", "ip6"}:
            continue
        value = match.get("right")
        values = value.get("set") if isinstance(value, dict) and set(value) == {"set"} else [value]
        if not isinstance(values, list):
            return False
        try:
            networks.extend(ipaddress.ip_network(item, strict=False) for item in values)
        except (TypeError, ValueError):
            return False
    return bool(networks) and all(network.is_loopback for network in networks)

for entry in ruleset.get("nftables", []):
    rule = entry.get("rule") if isinstance(entry, dict) else None
    if not isinstance(rule, dict):
        continue
    expr = rule.get("expr", [])
    if not translations(expr):
        continue
    if not isinstance(expr, list):
        raise SystemExit("uninspectable nftables translation rule")
    if explicit_udp_only(expr):
        continue
    originals = constrained_ports(expr)
    targets = translated_ports(expr, originals)
    if (originals is None or targets is None or originals & protected or targets & protected) and not loopback_destination_only(expr):
        raise SystemExit("nftables exposes a protected-port translation path")
PY
		rm -f -- "${nft_snapshot}"
		die "nftables contains a protected-port translation path"
	}
	rm -f -- "${nft_snapshot}"

	legacy4="$(mktemp /tmp/nexus-v2-phase2-iptables4.XXXXXX)"
	legacy6="$(mktemp /tmp/nexus-v2-phase2-iptables6.XXXXXX)"
	iptables-save >"${legacy4}" && ip6tables-save >"${legacy6}" || {
		rm -f -- "${legacy4}" "${legacy6}"
		die "cannot inspect legacy iptables translation paths"
	}
	python3 - "${legacy4}" "${legacy6}" <<'PY' || {
import ipaddress
import pathlib
import shlex
import sys

protected = {30333, 4000, 5001, 8080, 8787, 9944}

def option(tokens, names):
    return [tokens[index + 1] for index, token in enumerate(tokens[:-1]) if token in names]

def parse_ports(value):
    result = set()
    try:
        for member in value.split(","):
            separator = ":" if ":" in member else "-" if "-" in member else None
            if separator:
                low, high = map(int, member.split(separator, 1))
                if low > high:
                    return None
                result.update(port for port in protected if low <= port <= high)
            else:
                result.add(int(member))
    except (TypeError, ValueError):
        return None
    return result

def loopback_only(tokens):
    values = option(tokens, {"-d", "--destination"})
    try:
        return bool(values) and all(ipaddress.ip_network(value, strict=False).is_loopback for value in values)
    except ValueError:
        return False

def target_ports(target, tokens, original):
    values = option(tokens, {"--to-ports", "--on-port"})
    if target == "DNAT":
        for value in option(tokens, {"--to-destination"}):
            if value.startswith("[") and "]:" in value:
                values.append(value.rsplit("]:", 1)[1])
            elif value.count(":") == 1:
                values.append(value.rsplit(":", 1)[1])
    if not values:
        return original
    result = set()
    for value in values:
        parsed = parse_ports(value)
        if parsed is None:
            return None
        result.update(parsed)
    return result

for path in map(pathlib.Path, sys.argv[1:]):
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.startswith("-A "):
            continue
        try:
            tokens = shlex.split(line)
        except ValueError:
            raise SystemExit("uninspectable legacy firewall rule")
        if any(value.lower() == "udp" for value in option(tokens, {"-p", "--protocol"})):
            continue
        targets = [value.upper() for value in option(tokens, {"-j", "--jump"}) if value.upper() in {"DNAT", "REDIRECT", "TPROXY"}]
        if not targets:
            continue
        values = option(tokens, {"--dport", "--dports", "--destination-port"})
        original = None
        if values:
            original = set()
            for value in values:
                parsed = parse_ports(value)
                if parsed is None:
                    original = None
                    break
                original.update(parsed)
        translated = target_ports(targets[0], tokens, original)
        if (original is None or translated is None or original & protected or translated & protected) and not loopback_only(tokens):
            raise SystemExit("legacy firewall exposes a protected-port translation path")
PY
		rm -f -- "${legacy4}" "${legacy6}"
		die "legacy firewall contains a protected-port translation path"
	}
	rm -f -- "${legacy4}" "${legacy6}"
}

unit_name() { printf '%s-%s' "${UNIT_PREFIX}" "$1"; }

render_proxy_socket() {
	local service="$1" port="$2" destination="$3" stem
	stem="$(unit_name "${service}")"
	cat >"${destination}" <<EOF
[Unit]
Description=Eterra private Alpha restricted ${service} socket

[Socket]
ListenStream=${chain_ip}:${port}
NoDelay=true
Service=${stem}.service
EOF
}

render_proxy_service() {
	local service="$1" port="$2" destination="$3" stem
	stem="$(unit_name "${service}")"
	cat >"${destination}" <<EOF
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
}

write_units() {
	local service port stem socket_path service_path stage_root staged_socket staged_service
	stage_root="$(mktemp -d /tmp/nexus-v2-phase2-units.XXXXXX)"
	for service in "${SERVICES[@]}"; do
		port="${PORT_BY_SERVICE[${service}]}"
		stem="$(unit_name "${service}")"
		socket_path="/etc/systemd/system/${stem}.socket"
		service_path="/etc/systemd/system/${stem}.service"
		staged_socket="${stage_root}/${stem}.socket"
		staged_service="${stage_root}/${stem}.service"
		render_proxy_socket "${service}" "${port}" "${staged_socket}"
		render_proxy_service "${service}" "${port}" "${staged_service}"
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
		rm -f -- "/etc/systemd/system/${stem}.socket" "/etc/systemd/system/${stem}.service"
	done
	systemctl daemon-reload
}

require_units_open() {
	local service port stem socket_path service_path stage_root expected_socket expected_service unit fragment dropins
	stage_root="$(mktemp -d /tmp/nexus-v2-phase2-unit-verify.XXXXXX)"
	for service in "${SERVICES[@]}"; do
		port="${PORT_BY_SERVICE[${service}]}"
		stem="$(unit_name "${service}")"
		socket_path="/etc/systemd/system/${stem}.socket"
		service_path="/etc/systemd/system/${stem}.service"
		[[ -f "${socket_path}" && ! -L "${socket_path}" && -f "${service_path}" && ! -L "${service_path}" ]] || die "proxy units missing: ${service}"
		[[ "$(stat -c '%U:%G:%a' "${socket_path}")" == root:root:644 ]] || die "proxy socket ownership/mode drifted: ${service}"
		[[ "$(stat -c '%U:%G:%a' "${service_path}")" == root:root:644 ]] || die "proxy service ownership/mode drifted: ${service}"
		expected_socket="${stage_root}/${stem}.socket"
		expected_service="${stage_root}/${stem}.service"
		render_proxy_socket "${service}" "${port}" "${expected_socket}"
		render_proxy_service "${service}" "${port}" "${expected_service}"
		cmp -s "${expected_socket}" "${socket_path}" || die "proxy socket bytes drifted: ${service}"
		cmp -s "${expected_service}" "${service_path}" || die "proxy service bytes drifted: ${service}"
		for unit in "${stem}.socket" "${stem}.service"; do
			fragment="$(systemctl show "${unit}" -p FragmentPath --value)"
			[[ "${fragment}" == "/etc/systemd/system/${unit}" ]] || die "proxy unit fragment drifted: ${unit}"
			dropins="$(systemctl show "${unit}" -p DropInPaths --value)"
			[[ -z "${dropins}" ]] || die "proxy unit has unpinned drop-ins: ${unit}"
			! systemctl is-enabled --quiet "${unit}" || die "proxy unit must remain boot-disabled: ${unit}"
		done
		! systemctl is-enabled --quiet "${stem}.socket" || die "proxy socket must remain boot-disabled"
		systemctl is-active --quiet "${stem}.socket" || die "proxy socket inactive: ${service}"
		require_proxy_listener "${port}" "${service}"
	done
	rm -rf -- "${stage_root}"
}

require_units_absent() {
	local service stem suffix unit root load_state
	for service in "${SERVICES[@]}"; do
		stem="$(unit_name "${service}")"
		for suffix in socket service; do
			unit="${stem}.${suffix}"
			for root in /etc/systemd/system /run/systemd/system /usr/local/lib/systemd/system /usr/lib/systemd/system /lib/systemd/system; do
				[[ ! -e "${root}/${unit}" ]] || die "proxy unit remains: ${root}/${unit}"
			done
			! systemctl is-active --quiet "${unit}" || die "proxy unit remains active: ${unit}"
			! systemctl is-enabled --quiet "${unit}" 2>/dev/null || die "proxy unit remains enabled: ${unit}"
			load_state="$(systemctl show "${unit}" -p LoadState --value 2>/dev/null || true)"
			[[ -z "${load_state}" || "${load_state}" == not-found ]] || die "proxy unit remains loaded: ${unit} (${load_state})"
		done
	done
}

write_marker() {
	require_state_root
	[[ ! -e "${MARKER}" && ! -L "${MARKER}" ]] || die "refusing to replace Phase-2 open marker"
	open_exclusive_pending "${MARKER}.pending"
	jq -n --sort-keys \
		--arg operationId "${operation_id}" --arg planSha256 "${plan_sha256}" \
		--arg releaseId "${release_id}" --arg sourceCommit "${source_commit}" \
		--arg siteSourceCommit "${site_source_commit}" --arg siteReleaseVersion "${site_release_version}" \
		--arg chainLanIp "${chain_ip}" --arg allowedSourceIp "${site_ip}" \
		--arg helperSha256 "${helper_sha256}" --arg openedAtUtc "$(date -u +%Y-%m-%dT%H:%M:%SZ)" '
      {schemaVersion:1,kind:"nexus-v2-private-alpha-phase2-internal-transport-marker",
       operationId:$operationId,planSha256:$planSha256,releaseId:$releaseId,
       sourceCommit:$sourceCommit,siteSourceCommit:$siteSourceCommit,
       siteReleaseVersion:$siteReleaseVersion,chainLanIp:$chainLanIp,
       allowedSourceIp:$allowedSourceIp,helperSha256:$helperSha256,
       exposedPorts:[4000,8080,8787,9944],forbiddenPorts:[30333,5001],
       underlyingBackendsLoopbackOnly:true,chainStateMutationPerformed:false,
       paidOrPublicProductionActivationAuthorized:false,openedAtUtc:$openedAtUtc}
	    ' >&8
	exec 8>&-
	chmod 0400 "${MARKER}.pending"
	chown root:root "${MARKER}.pending"
	mv -T "${MARKER}.pending" "${MARKER}"
}

marker_matches() {
	[[ -f "${MARKER}" && ! -L "${MARKER}" && "$(stat -c '%U:%G:%a' "${MARKER}")" == root:root:400 ]] || return 1
	jq -e --arg operationId "${operation_id}" --arg planSha256 "${plan_sha256}" \
		--arg releaseId "${release_id}" --arg sourceCommit "${source_commit}" \
		--arg siteSourceCommit "${site_source_commit}" --arg siteReleaseVersion "${site_release_version}" \
		--arg chainLanIp "${chain_ip}" --arg allowedSourceIp "${site_ip}" --arg helperSha256 "${helper_sha256}" '
      keys == ["allowedSourceIp","chainLanIp","chainStateMutationPerformed","exposedPorts",
        "forbiddenPorts","helperSha256","kind","openedAtUtc","operationId",
        "paidOrPublicProductionActivationAuthorized","planSha256","releaseId","schemaVersion",
        "siteReleaseVersion","siteSourceCommit","sourceCommit","underlyingBackendsLoopbackOnly"] and
      .schemaVersion == 1 and .kind == "nexus-v2-private-alpha-phase2-internal-transport-marker" and
      .operationId == $operationId and .planSha256 == $planSha256 and .releaseId == $releaseId and
      .sourceCommit == $sourceCommit and .siteSourceCommit == $siteSourceCommit and
      .siteReleaseVersion == $siteReleaseVersion and .chainLanIp == $chainLanIp and
      .allowedSourceIp == $allowedSourceIp and .helperSha256 == $helperSha256 and
      .exposedPorts == [4000,8080,8787,9944] and .forbiddenPorts == [30333,5001] and
      .underlyingBackendsLoopbackOnly == true and .chainStateMutationPerformed == false and
      .paidOrPublicProductionActivationAuthorized == false and
      (.openedAtUtc | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$"))
    ' "${MARKER}" >/dev/null
}

heartbeat_nonce() {
	printf '%s' "${plan_sha256}:${operation_id}:${helper_sha256}" | sha256sum | awk '{print $1}'
}

write_heartbeat() {
	local now expiry proposed marker_sha nonce
	require_state_root
	if [[ -e "${HEARTBEAT}" || -L "${HEARTBEAT}" ]]; then
		[[ -f "${HEARTBEAT}" && ! -L "${HEARTBEAT}" && "$(stat -c '%U:%G:%a' "${HEARTBEAT}")" == root:root:400 ]] ||
			die "refusing to replace unsafe Phase-2 heartbeat"
	fi
	now="$(date -u +%s)"
	proposed="$((now + 900))"
	(( proposed <= plan_expires )) || proposed="${plan_expires}"
	(( proposed >= now + 300 )) || die "plan expires too soon to renew the Phase-2 lease"
	expiry="$(date -u -d "@${proposed}" +%Y-%m-%dT%H:%M:%SZ)"
	marker_sha="$(sha256sum "${MARKER}" | awk '{print $1}')"
	nonce="$(heartbeat_nonce)"
	open_exclusive_pending "${HEARTBEAT}.pending"
	jq -n --sort-keys --arg operationId "${operation_id}" --arg nonce "${nonce}" \
		--arg markerSha256 "${marker_sha}" --arg updatedAtUtc "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
		--arg expiresAtUtc "${expiry}" '
      {schemaVersion:1,kind:"nexus-v2-private-alpha-phase2-internal-transport-heartbeat",
       operationId:$operationId,nonce:$nonce,markerSha256:$markerSha256,
       active:true,updatedAtUtc:$updatedAtUtc,expiresAtUtc:$expiresAtUtc}
	    ' >&8
	exec 8>&-
	chmod 0400 "${HEARTBEAT}.pending"
	chown root:root "${HEARTBEAT}.pending"
	mv -T "${HEARTBEAT}.pending" "${HEARTBEAT}"
}

require_heartbeat() {
	local updated expires now marker_sha nonce
	[[ -f "${HEARTBEAT}" && ! -L "${HEARTBEAT}" && "$(stat -c '%U:%G:%a' "${HEARTBEAT}")" == root:root:400 ]] || die "heartbeat is unavailable or mutable"
	marker_sha="$(sha256sum "${MARKER}" | awk '{print $1}')"
	nonce="$(heartbeat_nonce)"
	jq -e --arg operationId "${operation_id}" --arg nonce "${nonce}" --arg markerSha "${marker_sha}" '
      keys == ["active","expiresAtUtc","kind","markerSha256","nonce","operationId","schemaVersion","updatedAtUtc"] and
      .schemaVersion == 1 and .kind == "nexus-v2-private-alpha-phase2-internal-transport-heartbeat" and
      .operationId == $operationId and .nonce == $nonce and .markerSha256 == $markerSha and .active == true and
      (.updatedAtUtc | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$")) and
      (.expiresAtUtc | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$"))
    ' "${HEARTBEAT}" >/dev/null || die "heartbeat contract mismatch"
	updated="$(date -u -d "$(jq -er '.updatedAtUtc' "${HEARTBEAT}")" +%s)"
	expires="$(date -u -d "$(jq -er '.expiresAtUtc' "${HEARTBEAT}")" +%s)"
	now="$(date -u +%s)"
	(( updated <= now + 5 && now - updated <= 900 && expires > now )) || die "heartbeat is stale or expired"
}

watchdog_units_sha256() {
	cat "/etc/systemd/system/${WATCHDOG_SERVICE}" "/etc/systemd/system/${WATCHDOG_TIMER}" | sha256sum | awk '{print $1}'
}

watchdog_payloads_sha256() {
	sha256sum "${INSTALLED_HELPER}" "${INSTALLED_PLAN}" "${WATCHDOG_SCRIPT}" |
		sha256sum | awk '{print $1}'
}

render_watchdog_script() {
	local destination="$1"
	cat >"${destination}" <<EOF
#!/bin/bash
set -euo pipefail
plan_base64="\$(base64 <'${INSTALLED_PLAN}' | tr -d '\\r\\n')"
'${INSTALLED_HELPER}' watchdog "\${plan_base64}" '${plan_sha256}' '${helper_sha256}' >>'${STATE_ROOT}/watchdog.log' 2>&1
EOF
}

render_watchdog_service() {
	local destination="$1"
	cat >"${destination}" <<EOF
[Unit]
Description=Fail closed Nexus V2 Phase-2 internal transport on lease expiry
StartLimitIntervalSec=0

[Service]
Type=oneshot
ExecStart=${WATCHDOG_SCRIPT}
EOF
}

render_watchdog_timer() {
	local destination="$1"
	cat >"${destination}" <<EOF
[Unit]
Description=Nexus V2 Phase-2 internal transport lease watchdog

[Timer]
OnBootSec=30
OnUnitActiveSec=30
AccuracySec=1s
Persistent=true
Unit=${WATCHDOG_SERVICE}

[Install]
WantedBy=timers.target
EOF
}

arm_watchdog() {
	local stage expected_script expected_service expected_timer
	require_state_root
	install -o root -g root -m 0700 "$0" "${INSTALLED_HELPER}"
	install -o root -g root -m 0400 "${plan_path}" "${INSTALLED_PLAN}"
	stage="$(mktemp -d /tmp/nexus-v2-phase2-watchdog.XXXXXX)"
	expected_script="${stage}/watchdog-check.sh"
	expected_service="${stage}/${WATCHDOG_SERVICE}"
	expected_timer="${stage}/${WATCHDOG_TIMER}"
	render_watchdog_script "${expected_script}"
	render_watchdog_service "${expected_service}"
	render_watchdog_timer "${expected_timer}"
	install -o root -g root -m 0700 "${expected_script}" "${WATCHDOG_SCRIPT}"
	install -o root -g root -m 0644 "${expected_service}" "/etc/systemd/system/${WATCHDOG_SERVICE}"
	install -o root -g root -m 0644 "${expected_timer}" "/etc/systemd/system/${WATCHDOG_TIMER}"
	rm -rf -- "${stage}"
	systemd-analyze verify "/etc/systemd/system/${WATCHDOG_SERVICE}" "/etc/systemd/system/${WATCHDOG_TIMER}" >/dev/null
	systemctl daemon-reload
	systemctl enable --now "${WATCHDOG_TIMER}" >/dev/null
}

require_watchdog() {
	local unit fragment dropins stage expected_script expected_service expected_timer
	[[ -f "${INSTALLED_HELPER}" && ! -L "${INSTALLED_HELPER}" &&
		"$(stat -c '%U:%G:%a' "${INSTALLED_HELPER}")" == root:root:700 &&
		"$(sha256sum "${INSTALLED_HELPER}" | awk '{print $1}')" == "${helper_sha256}" ]] ||
		die "watchdog helper bytes or ownership drifted"
	[[ -f "${INSTALLED_PLAN}" && ! -L "${INSTALLED_PLAN}" &&
		"$(stat -c '%U:%G:%a' "${INSTALLED_PLAN}")" == root:root:400 &&
		"$(sha256sum "${INSTALLED_PLAN}" | awk '{print $1}')" == "${plan_sha256}" ]] ||
		die "watchdog plan bytes or ownership drifted"
	stage="$(mktemp -d /tmp/nexus-v2-phase2-watchdog-verify.XXXXXX)"
	expected_script="${stage}/watchdog-check.sh"
	expected_service="${stage}/${WATCHDOG_SERVICE}"
	expected_timer="${stage}/${WATCHDOG_TIMER}"
	render_watchdog_script "${expected_script}"
	render_watchdog_service "${expected_service}"
	render_watchdog_timer "${expected_timer}"
	[[ -f "${WATCHDOG_SCRIPT}" && ! -L "${WATCHDOG_SCRIPT}" &&
		"$(stat -c '%U:%G:%a' "${WATCHDOG_SCRIPT}")" == root:root:700 ]] ||
		die "watchdog script ownership/mode drifted"
	cmp -s "${expected_script}" "${WATCHDOG_SCRIPT}" || die "watchdog script bytes drifted"
	for unit in "${WATCHDOG_SERVICE}" "${WATCHDOG_TIMER}"; do
		[[ -f "/etc/systemd/system/${unit}" && ! -L "/etc/systemd/system/${unit}" ]] || die "watchdog unit missing: ${unit}"
		[[ "$(stat -c '%U:%G:%a' "/etc/systemd/system/${unit}")" == root:root:644 ]] || die "watchdog unit ownership/mode drifted: ${unit}"
		fragment="$(systemctl show "${unit}" -p FragmentPath --value)"
		[[ "${fragment}" == "/etc/systemd/system/${unit}" ]] || die "watchdog fragment drifted: ${unit}"
		dropins="$(systemctl show "${unit}" -p DropInPaths --value)"
		[[ -z "${dropins}" ]] || die "watchdog has unpinned drop-ins: ${unit}"
	done
	cmp -s "${expected_service}" "/etc/systemd/system/${WATCHDOG_SERVICE}" || die "watchdog service bytes drifted"
	cmp -s "${expected_timer}" "/etc/systemd/system/${WATCHDOG_TIMER}" || die "watchdog timer bytes drifted"
	rm -rf -- "${stage}"
	systemctl is-active --quiet "${WATCHDOG_TIMER}" || die "watchdog timer is inactive"
	systemctl is-enabled --quiet "${WATCHDOG_TIMER}" || die "watchdog timer is disabled"
}

remove_watchdog() {
	systemctl disable --now "${WATCHDOG_TIMER}" >/dev/null 2>&1 || true
	systemctl stop "${WATCHDOG_SERVICE}" >/dev/null 2>&1 || true
	rm -f -- "/etc/systemd/system/${WATCHDOG_TIMER}" "/etc/systemd/system/${WATCHDOG_SERVICE}"
	systemctl daemon-reload
}

require_watchdog_absent() {
	local unit root load_state
	for unit in "${WATCHDOG_TIMER}" "${WATCHDOG_SERVICE}"; do
		for root in /etc/systemd/system /run/systemd/system /usr/local/lib/systemd/system /usr/lib/systemd/system /lib/systemd/system; do
			[[ ! -e "${root}/${unit}" ]] || die "watchdog unit remains: ${root}/${unit}"
		done
		! systemctl is-active --quiet "${unit}" || die "watchdog remains active: ${unit}"
		! systemctl is-enabled --quiet "${unit}" 2>/dev/null || die "watchdog remains enabled: ${unit}"
		load_state="$(systemctl show "${unit}" -p LoadState --value 2>/dev/null || true)"
		[[ -z "${load_state}" || "${load_state}" == not-found ]] || die "watchdog remains loaded: ${unit}"
	done
}

open_exposure() {
	remove_permit_rules
	install_nft_guard
	for service in "${SERVICES[@]}"; do
		ufw allow proto tcp from "${site_ip}" to "${chain_ip}" port "${PORT_BY_SERVICE[${service}]}" comment "nexus-v2-phase2-${operation_id}-${service}" >/dev/null
	done
	write_units
	systemctl daemon-reload
	for service in "${SERVICES[@]}"; do systemctl start "$(unit_name "${service}").socket" >/dev/null; done
}

close_exposure() {
	local failed=0 port
	remove_units || failed=1
	remove_permit_rules || failed=1
	remove_nft_guard || failed=1
	require_firewall_closed || failed=1
	require_nft_guard_absent || failed=1
	require_units_absent || failed=1
	for port in "${PROTECTED_PORTS[@]}"; do
		require_closed_listener "${port}" "protected service ${port}" || failed=1
	done
	[[ "${failed}" -eq 0 ]]
}

verify_open() {
	require_identity
	marker_matches || die "Phase-2 marker mismatch"
	require_heartbeat
	require_watchdog
	require_units_open
	require_firewall_open
	require_nft_guard_open
	require_phase1_listener 30333 "chain P2P"
	require_phase1_listener 5001 "IPFS API"
	curl -fsS --max-time 15 'http://127.0.0.1:4000/health/ready' >/dev/null || die "media readiness failed"
	curl -fsS --max-time 15 'http://127.0.0.1:8787/v1/status' >/dev/null || die "authority readiness failed"
	curl -sS --max-time 15 -o /dev/null 'http://127.0.0.1:8080/' || die "IPFS gateway readiness failed"
	curl -fsS --max-time 15 -H 'Content-Type: application/json' --data-binary '{"id":1,"jsonrpc":"2.0","method":"chain_getBlockHash","params":[0]}' 'http://127.0.0.1:9944' >/dev/null || die "chain RPC readiness failed"
}

write_closed_marker() {
	require_state_root
	[[ ! -e "${CLOSED_MARKER}" && ! -L "${CLOSED_MARKER}" ]] || die "refusing to replace Phase-2 closed marker"
	open_exclusive_pending "${CLOSED_MARKER}.pending"
	jq -n --sort-keys --arg operationId "${operation_id}" --arg planSha256 "${plan_sha256}" \
		--arg closedAtUtc "$(date -u +%Y-%m-%dT%H:%M:%SZ)" '
      {schemaVersion:1,kind:"nexus-v2-private-alpha-phase2-internal-transport-closed",
       operationId:$operationId,planSha256:$planSha256,exposedPorts:[],
       forbiddenPorts:[30333,5001],chainStateMutationPerformed:false,closedAtUtc:$closedAtUtc}
	    ' >&8
	exec 8>&-
	chmod 0400 "${CLOSED_MARKER}.pending"
	chown root:root "${CLOSED_MARKER}.pending"
	mv -T "${CLOSED_MARKER}.pending" "${CLOSED_MARKER}"
}

closed_marker_matches() {
	[[ -f "${CLOSED_MARKER}" && ! -L "${CLOSED_MARKER}" &&
		"$(stat -c '%U:%G:%a' "${CLOSED_MARKER}")" == root:root:400 ]] || return 1
	jq -e --arg operationId "${operation_id}" --arg planSha256 "${plan_sha256}" '
      keys == ["chainStateMutationPerformed","closedAtUtc","exposedPorts","forbiddenPorts",
        "kind","operationId","planSha256","schemaVersion"] and
      .schemaVersion == 1 and .kind == "nexus-v2-private-alpha-phase2-internal-transport-closed" and
      .operationId == $operationId and .planSha256 == $planSha256 and
      .exposedPorts == [] and .forbiddenPorts == [30333,5001] and
      .chainStateMutationPerformed == false and
      (.closedAtUtc | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$"))
    ' "${CLOSED_MARKER}" >/dev/null
}

emit_result() {
	local already_applied="$1" mutation="$2" state="$3" marker_path="$4" marker_sha="$5"
	local heartbeat_json watchdog_json payload
	if [[ "${state}" == open ]]; then
		heartbeat_json="$(jq -c --arg path "${HEARTBEAT}" '{path:$path,nonce:.nonce,expiresAtUtc:.expiresAtUtc}' "${HEARTBEAT}")"
		watchdog_json="$(jq -cn --arg service "${WATCHDOG_SERVICE}" --arg timer "${WATCHDOG_TIMER}" \
			--arg unitSha256 "$(watchdog_units_sha256)" --arg payloadSha256 "$(watchdog_payloads_sha256)" \
			'{service:$service,timer:$timer,unitSha256:$unitSha256,payloadSha256:$payloadSha256,armed:true}')"
	else
		heartbeat_json=null
		watchdog_json='{"armed":false}'
	fi
	payload="$(jq -cn --sort-keys \
		--arg operationId "${operation_id}" --arg planSha256 "${plan_sha256}" \
		--arg releaseId "${release_id}" --arg sourceCommit "${source_commit}" \
		--arg action "${action}" --arg state "${state}" --arg helperSha256 "${helper_sha256}" \
		--arg markerPath "${marker_path}" --arg markerSha256 "${marker_sha}" \
		--arg completedAtUtc "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
		--argjson alreadyApplied "${already_applied}" --argjson mutationPerformed "${mutation}" \
		--argjson heartbeat "${heartbeat_json}" --argjson watchdog "${watchdog_json}" '
      {schemaVersion:1,kind:"nexus-v2-private-alpha-phase2-internal-transport-result",
       operationId:$operationId,planSha256:$planSha256,releaseId:$releaseId,
       sourceCommit:$sourceCommit,action:$action,state:$state,
       mutationPerformed:$mutationPerformed,alreadyApplied:$alreadyApplied,
       helperSha256:$helperSha256,marker:{path:$markerPath,sha256:$markerSha256},
       heartbeat:$heartbeat,watchdog:$watchdog,
       transport:{network:{allowedSourceIp:"192.168.1.218",chainLanIp:"192.168.1.159",siteLanIp:"192.168.1.218"},
         ports:{authority:8787,chainRpc:9944,forbidden:[30333,5001],ipfsGateway:8080,media:4000}},
       safety:{chainStateMutationAuthorized:false,paidOrPublicActivationAuthorized:false,
         phase1PublicCaddyMustRemainUnchanged:true,privateAlphaOnly:true,
         publicIngressMutationAuthorized:false,sourceRestrictedToSiteHost:true,
         underlyingBackendsRemainLoopbackOnly:true},completedAtUtc:$completedAtUtc}
    ')"
	printf 'NEXUS_V2_PHASE2_TRANSPORT_RESULT:%s\n' "$(printf '%s\n' "${payload}" | base64 | tr -d '\r\n')"
}

case "${action}" in
	execute)
		ensure_state_base
		if state_root_exists; then
			require_state_root
			if ! marker_matches; then
				close_exposure >/dev/null 2>&1 || true
				die "existing Phase-2 operation root lacks the exact open marker"
			fi
			verify_open
			emit_result true false open "${MARKER}" "$(sha256sum "${MARKER}" | awk '{print $1}')"
			exit 0
		fi
		create_state_root
		require_identity
		for port in "${PROTECTED_PORTS[@]}"; do require_phase1_listener "${port}" "protected service ${port}"; done
		require_firewall_closed
		require_nft_guard_absent
		require_units_absent
		completed=0
		fail_closed() {
			local rc="$?"
			trap - EXIT HUP INT TERM
			set +e
			if [[ "${completed}" -ne 1 ]] && close_exposure >/dev/null 2>&1; then
				remove_watchdog >/dev/null 2>&1
			fi
			cleanup_temporary
			exit "${rc:-2}"
		}
		trap fail_closed EXIT HUP INT TERM
		write_marker
		write_heartbeat
		arm_watchdog
		open_exposure
		verify_open
		completed=1
		trap cleanup_temporary EXIT
		trap - HUP INT TERM
		emit_result false true open "${MARKER}" "$(sha256sum "${MARKER}" | awk '{print $1}')"
		;;
	renew)
		ensure_state_base
		state_root_exists || die "Phase-2 operation root is unavailable"
		require_state_root
		verify_open
		write_heartbeat
		verify_open
		emit_result false true open "${MARKER}" "$(sha256sum "${MARKER}" | awk '{print $1}')"
		;;
	verify)
		ensure_state_base
		state_root_exists || die "Phase-2 operation root is unavailable"
		require_state_root
		verify_open
		emit_result true false open "${MARKER}" "$(sha256sum "${MARKER}" | awk '{print $1}')"
		;;
	watchdog)
		if ! (ensure_state_base && state_root_exists && require_state_root); then
			close_exposure || exit 1
			exit 1
		fi
		if marker_matches && require_heartbeat >/dev/null 2>&1; then exit 0; fi
		close_exposure || exit 1
		rm -f -- "${HEARTBEAT}"
		write_closed_marker
		;;
	close)
		if ! (ensure_state_base); then
			close_exposure || true
			exit 1
		fi
		if state_root_exists; then
			if ! (require_state_root); then
				close_exposure || true
				exit 1
			fi
			if ! marker_matches && ! closed_marker_matches; then
				close_exposure || true
				die "existing Phase-2 operation root lacks an exact open or closed marker"
			fi
		else
			create_state_root
		fi
		if closed_marker_matches; then
			close_exposure
			if (require_watchdog_absent >/dev/null 2>&1); then
				emit_result true false closed "${CLOSED_MARKER}" "$(sha256sum "${CLOSED_MARKER}" | awk '{print $1}')"
			else
				remove_watchdog
				require_watchdog_absent
				emit_result false true closed "${CLOSED_MARKER}" "$(sha256sum "${CLOSED_MARKER}" | awk '{print $1}')"
			fi
			exit 0
		fi
		close_exposure
		remove_watchdog
		require_watchdog_absent
		rm -f -- "${HEARTBEAT}"
		write_closed_marker
		emit_result false true closed "${CLOSED_MARKER}" "$(sha256sum "${CLOSED_MARKER}" | awk '{print $1}')"
		;;
esac
