#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

load_env

remote_root_bash <<EOF
set -euo pipefail

if command -v ufw >/dev/null 2>&1; then
	ufw allow from "${LAN_CIDR}" to any port 8094 proto tcp comment 'Crypto-Strike alpha bridge'
	ufw status numbered
	exit 0
fi

if command -v firewall-cmd >/dev/null 2>&1; then
	firewall-cmd --permanent --add-port=8094/tcp
	firewall-cmd --reload
	firewall-cmd --list-ports
	exit 0
fi

echo "No supported firewall command found; bridge is listening but port 8094 may remain blocked." >&2
exit 1
EOF
