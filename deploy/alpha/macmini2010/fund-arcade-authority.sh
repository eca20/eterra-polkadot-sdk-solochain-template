#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

while [[ $# -gt 0 ]]; do
	case "$1" in
		--help|-h)
			cat <<'EOF'
Usage: fund-arcade-authority.sh

Runs the deployed Authority Operator once to ensure the Nova Rail relay
account has enough native token balance to pay submit_result transaction fees.
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
require_cmd ssh

remote_bash <<EOF
set -euo pipefail
test -x "${REMOTE_AUTHORITY_OPERATOR_BIN}" || {
	echo "authority operator not found at ${REMOTE_AUTHORITY_OPERATOR_BIN}; run deploy-arcade-authority.sh first" >&2
	exit 2
}
test -f "${REMOTE_AUTHORITY_ENV_FILE}" || {
	echo "authority env not found at ${REMOTE_AUTHORITY_ENV_FILE}; run deploy-arcade-authority.sh first" >&2
	exit 2
}
set -a
source "${REMOTE_AUTHORITY_ENV_FILE}"
set +a
"${REMOTE_AUTHORITY_OPERATOR_BIN}" fund-nova-rail-relay
EOF

log "alpha arcade authority native funding check complete"
