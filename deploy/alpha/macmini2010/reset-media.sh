#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

load_env
require_cmd ssh

if [[ "${1:-}" != "--yes" ]]; then
	die "refusing destructive reset without --yes"
fi

log "resetting alpha media docker volumes"
remote_root_bash <<EOF
set -euo pipefail
${REMOTE_DOCKER_COMPOSE_CMD} down --volumes --remove-orphans
${REMOTE_DOCKER_COMPOSE_CMD} up -d --build
${REMOTE_DOCKER_COMPOSE_CMD} ps
EOF
