#!/usr/bin/env bash
set -euo pipefail

# Host-local guard used by the fresh Nexus V2 Phase-1 deployment.  The guard
# removes every UFW allow rule for the write-capable chain/authority ports
# before either service is started, then proves that RPC and authority
# listeners are loopback-only.  It deliberately leaves SSH untouched.

action="${1:-}"
CHAIN_RPC_PORT="${CHAIN_RPC_PORT:-9944}"
CHAIN_P2P_PORT="${CHAIN_P2P_PORT:-30333}"
AUTHORITY_PORT="${AUTHORITY_PORT:-8787}"

die() {
	printf 'nexus-v2-phase1-closed-ingress: %s\n' "$*" >&2
	exit 2
}

for value in "${CHAIN_RPC_PORT}" "${CHAIN_P2P_PORT}" "${AUTHORITY_PORT}"; do
	[[ "${value}" =~ ^[0-9]+$ ]] && (( value > 0 && value <= 65535 )) ||
		die "invalid protected port: ${value}"
done

command -v ufw >/dev/null 2>&1 || die "ufw is required"
command -v ss >/dev/null 2>&1 || die "ss is required"

ufw_status() {
	ufw status numbered
}

matching_rule_numbers() {
	local status="$1"
	local port="$2"
	printf '%s\n' "${status}" |
		awk -v port="${port}" '
			/^\[[[:space:]]*[0-9]+\]/ &&
			$0 ~ "(^|[[:space:]])" port "(/tcp)?([[:space:]]|$)" &&
			$0 ~ /ALLOW IN/ {
				line = $0
				sub(/^\[[[:space:]]*/, "", line)
				sub(/\].*$/, "", line)
				print line
			}
		' |
		LC_ALL=C sort -rn
}

remove_external_allows() {
	local port status number
	for port in "${CHAIN_RPC_PORT}" "${CHAIN_P2P_PORT}" "${AUTHORITY_PORT}"; do
		while :; do
			status="$(ufw_status)"
			number="$(matching_rule_numbers "${status}" "${port}" | head -n 1)"
			[[ -n "${number}" ]] || break
			ufw --force delete "${number}" >/dev/null
		done
	done
}

verify_firewall_closed() {
	local verbose status port
	verbose="$(ufw status verbose)"
	[[ "${verbose}" == *"Status: active"* ]] || die "ufw is not active"
	[[ "${verbose}" =~ Default:[[:space:]]deny[[:space:]]\(incoming\) ]] ||
		die "ufw incoming default is not deny"
	status="$(ufw_status)"
	for port in "${CHAIN_RPC_PORT}" "${CHAIN_P2P_PORT}" "${AUTHORITY_PORT}"; do
		[[ -z "$(matching_rule_numbers "${status}" "${port}")" ]] ||
			die "external allow rule remains for protected port ${port}"
	done
}

verify_loopback_listener() {
	local port="$1"
	local label="$2"
	local listeners address
	listeners="$(ss -H -lnt "sport = :${port}")"
	[[ -n "${listeners}" ]] || die "${label} has no TCP listener on ${port}"
	while IFS= read -r address; do
		case "${address}" in
			127.0.0.1:${port}|\[::1\]:${port}) ;;
			*) die "${label} is not loopback-only: ${address}" ;;
		esac
	done < <(printf '%s\n' "${listeners}" | awk '{print $4}')
}

case "${action}" in
	preclose)
		remove_external_allows
		verify_firewall_closed
		;;
	verify-node)
		verify_firewall_closed
		verify_loopback_listener "${CHAIN_RPC_PORT}" "chain RPC"
		;;
	verify-authority)
		verify_firewall_closed
		verify_loopback_listener "${CHAIN_RPC_PORT}" "chain RPC"
		verify_loopback_listener "${AUTHORITY_PORT}" "legacy authority"
		;;
	*)
		die "usage: $0 {preclose|verify-node|verify-authority}"
		;;
esac
