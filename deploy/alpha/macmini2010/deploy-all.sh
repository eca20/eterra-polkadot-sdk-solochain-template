#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

node_args=()
media_args=()

while [[ $# -gt 0 ]]; do
	case "$1" in
		--purge-state)
			node_args+=("$1")
			;;
		--fresh)
			node_args+=("--purge-state")
			media_args+=("--fresh")
			;;
		--help|-h)
			cat <<'EOF'
Usage: deploy-all.sh [--purge-state] [--fresh]

Normal deploys preserve alpha chain state.
Pass --purge-state to wipe the remote alpha node base path before restart.
Alpha spec/genesis changes are only applied when --purge-state is set.
Pass --fresh to purge chain state and reset the alpha media/IPFS volumes after deploy.
EOF
			exit 0
			;;
		*)
			echo "[alpha-macmini2010] unknown argument: $1" >&2
			exit 1
			;;
	esac
	shift
done

"${SCRIPT_DIR}/deploy-node.sh" "${node_args[@]}"
"${SCRIPT_DIR}/deploy-media.sh" "${media_args[@]}"
