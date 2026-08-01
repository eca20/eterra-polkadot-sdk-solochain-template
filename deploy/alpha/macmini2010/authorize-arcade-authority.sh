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

while [[ $# -gt 0 ]]; do
	case "$1" in
		--help|-h)
			cat <<'EOF'
Usage: authorize-arcade-authority.sh

Runs the deployed Authority Operator once to authorize the Nova Rail relay
account for game_id=1003, ruleset=1, event_type=100.
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
require_cmd rsync
require_cmd ssh

[[ -n "${AUTHORITY_RELAY_ACCOUNT}" ]] || die "AUTHORITY_RELAY_ACCOUNT or NOVA_RAIL_RELAY_ACCOUNT is required"
[[ "${AUTHORITY_RELAY_ACCOUNT}" != "replace-with-nova-rail-relay-ss58-account" ]] || die "AUTHORITY_RELAY_ACCOUNT must be replaced with the relay SS58 account"
[[ -n "${ETERRA_ALPHA_SUDO_MNEMONIC}" ]] || die "ETERRA_ALPHA_SUDO_MNEMONIC is required for the one-shot authorization operator"

bundle_dir="$(make_temp_dir)"
remote_tmp_dir="${DEPLOY_ROOT}/tmp/arcade-authority-authorize"
sudo_mnemonic="$(read_secret_value "${ETERRA_ALPHA_SUDO_MNEMONIC}")"
sudo_password=""
if [[ -n "${ETERRA_ALPHA_SUDO_DERIVATION_PASSWORD}" ]]; then
	sudo_password="$(read_secret_value "${ETERRA_ALPHA_SUDO_DERIVATION_PASSWORD}")"
fi

{
	printf 'ALPHA_RPC_URL=%q\n' "ws://127.0.0.1:${CHAIN_RPC_PORT}"
	printf 'NOVA_RAIL_RELAY_ACCOUNT=%q\n' "${AUTHORITY_RELAY_ACCOUNT}"
	printf 'NOVA_RAIL_RELAY_AUTHORITY_ID=%q\n' "${NOVA_RAIL_RELAY_AUTHORITY_ID}"
	printf 'ETERRA_ALPHA_SUDO_MNEMONIC=%q\n' "${sudo_mnemonic}"
	printf 'ETERRA_ALPHA_SUDO_DERIVATION_PASSWORD=%q\n' "${sudo_password}"
} >"${bundle_dir}/operator.env"
chmod 0600 "${bundle_dir}/operator.env"

remote_bash <<EOF
set -euo pipefail
mkdir -p "${remote_tmp_dir}"
test -x "${REMOTE_AUTHORITY_OPERATOR_BIN}" || {
	echo "authority operator not found at ${REMOTE_AUTHORITY_OPERATOR_BIN}; run deploy-arcade-authority.sh first" >&2
	exit 2
}
EOF

rsync_to_remote_no_delete "${bundle_dir}/operator.env" "${remote_tmp_dir}/operator.env"

remote_bash <<EOF
set -euo pipefail
set -a
source "${remote_tmp_dir}/operator.env"
set +a
set +e
"${REMOTE_AUTHORITY_OPERATOR_BIN}" authorize-nova-rail-relay
rc=\$?
set -e
rm -f "${remote_tmp_dir}/operator.env"
rmdir "${remote_tmp_dir}" >/dev/null 2>&1 || true
exit "\${rc}"
EOF

log "alpha arcade authority authorization complete"
