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

# Host-local guard used by the fresh Nexus V2 Phase-1 deployment.  The guard
# removes every UFW allow rule for the write-capable chain, authority, media,
# and IPFS ports before any replacement service is started. It then proves
# that every Phase-1 service listener is loopback-only. It deliberately leaves
# SSH untouched. Firewall closure is defense in depth; listener verification
# is mandatory because Docker-published ports may bypass UFW policy.

action="${1:-}"
CHAIN_RPC_PORT="${CHAIN_RPC_PORT:-9944}"
CHAIN_P2P_PORT="${CHAIN_P2P_PORT:-30333}"
AUTHORITY_PORT="${AUTHORITY_PORT:-8787}"
MEDIA_PORT="${MEDIA_PORT:-4000}"
IPFS_API_PORT="${IPFS_API_PORT:-5001}"
IPFS_GATEWAY_PORT="${IPFS_GATEWAY_PORT:-8080}"

die() {
	printf 'nexus-v2-phase1-closed-ingress: %s\n' "$*" >&2
	exit 2
}

protected_ports=(
	"${CHAIN_RPC_PORT}"
	"${CHAIN_P2P_PORT}"
	"${AUTHORITY_PORT}"
	"${MEDIA_PORT}"
	"${IPFS_API_PORT}"
	"${IPFS_GATEWAY_PORT}"
)

for value in "${protected_ports[@]}"; do
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
	for port in "${protected_ports[@]}"; do
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
	for port in "${protected_ports[@]}"; do
		[[ -z "$(matching_rule_numbers "${status}" "${port}")" ]] ||
			die "external allow rule remains for protected port ${port}"
	done
}

verify_no_listener() {
	local port="$1"
	local label="$2"
	[[ -z "$(ss -H -lnt "sport = :${port}")" ]] ||
		die "${label} unexpectedly has a TCP listener on ${port}"
}

verify_pre_reset_listeners_absent() {
	verify_no_listener "${CHAIN_RPC_PORT}" "chain RPC"
	verify_no_listener "${CHAIN_P2P_PORT}" "chain P2P"
	verify_no_listener "${AUTHORITY_PORT}" "legacy authority"
	verify_no_listener "${MEDIA_PORT}" "media service"
	verify_no_listener "${IPFS_API_PORT}" "IPFS API"
	verify_no_listener "${IPFS_GATEWAY_PORT}" "IPFS gateway"
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
		verify_loopback_listener "${CHAIN_P2P_PORT}" "chain P2P"
		;;
	verify-media)
		verify_firewall_closed
		verify_loopback_listener "${CHAIN_RPC_PORT}" "chain RPC"
		verify_loopback_listener "${CHAIN_P2P_PORT}" "chain P2P"
		verify_loopback_listener "${MEDIA_PORT}" "media service"
		verify_loopback_listener "${IPFS_API_PORT}" "IPFS API"
		verify_loopback_listener "${IPFS_GATEWAY_PORT}" "IPFS gateway"
		;;
	verify-authority)
		verify_firewall_closed
		verify_loopback_listener "${CHAIN_RPC_PORT}" "chain RPC"
		verify_loopback_listener "${CHAIN_P2P_PORT}" "chain P2P"
		verify_loopback_listener "${MEDIA_PORT}" "media service"
		verify_loopback_listener "${IPFS_API_PORT}" "IPFS API"
		verify_loopback_listener "${IPFS_GATEWAY_PORT}" "IPFS gateway"
		verify_loopback_listener "${AUTHORITY_PORT}" "legacy authority"
		;;
	verify-pre-reset)
		verify_firewall_closed
		verify_pre_reset_listeners_absent
		;;
	*)
		die "usage: $0 {preclose|verify-pre-reset|verify-node|verify-media|verify-authority}"
		;;
esac
