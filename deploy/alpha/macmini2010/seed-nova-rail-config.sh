#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

while [[ $# -gt 0 ]]; do
	case "$1" in
		--help|-h)
			cat <<'EOF'
Usage: seed-nova-rail-config.sh

Runs the deployed Authority Operator once to idempotently seed the Nova Rail
ArcadeCore game config for game_id=1003, ruleset=1.
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
require_cmd expect
require_cmd rsync
require_cmd ssh

[[ -n "${ETERRA_ALPHA_SUDO_MNEMONIC}" ]] || die "ETERRA_ALPHA_SUDO_MNEMONIC is required for Nova Rail config seeding"

bundle_dir="$(make_temp_dir)"
remote_tmp_dir="${DEPLOY_ROOT}/tmp/nova-rail-config-seed"
sudo_mnemonic="$(read_secret_value "${ETERRA_ALPHA_SUDO_MNEMONIC}")"
sudo_password=""
if [[ -n "${ETERRA_ALPHA_SUDO_DERIVATION_PASSWORD}" ]]; then
	sudo_password="$(read_secret_value "${ETERRA_ALPHA_SUDO_DERIVATION_PASSWORD}")"
fi

{
	printf 'ALPHA_RPC_URL=%q\n' "ws://127.0.0.1:${CHAIN_RPC_PORT}"
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
"${REMOTE_AUTHORITY_OPERATOR_BIN}" seed-nova-rail-config
rc=\$?
set -e
rm -f "${remote_tmp_dir}/operator.env"
rmdir "${remote_tmp_dir}" >/dev/null 2>&1 || true
exit "\${rc}"
EOF

log "alpha Nova Rail game config seed complete"
