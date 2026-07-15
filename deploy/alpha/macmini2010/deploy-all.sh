#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

node_args=()
media_args=()
authority_args=()
deploy_authority=0
seed_nova_rail_config=0

while [[ $# -gt 0 ]]; do
	case "$1" in
		--purge-state)
			node_args+=("$1")
			;;
		--fresh)
			node_args+=("--purge-state")
			media_args+=("--fresh")
			;;
		--with-arcade-authority)
			deploy_authority=1
			;;
		--authorize-arcade-authority)
			deploy_authority=1
			authority_args+=("--authorize")
			;;
		--seed-nova-rail-config)
			seed_nova_rail_config=1
			;;
		--help|-h)
			cat <<'EOF'
Usage: deploy-all.sh [--purge-state] [--fresh] [--with-arcade-authority] [--authorize-arcade-authority] [--seed-nova-rail-config]

Normal deploys preserve alpha chain state.
Pass --purge-state to wipe the remote alpha node base path before restart.
Alpha spec/genesis changes are only applied when --purge-state is set.
Pass --fresh to purge chain state and reset the alpha media/IPFS volumes after deploy.
Pass --with-arcade-authority to deploy the Nova Rail authority relay API/operator.
Pass --authorize-arcade-authority to deploy and run the one-shot relay authorization operator.
Pass --seed-nova-rail-config to run the explicit idempotent ArcadeCore seed after deploy.
Normal deploys never mutate live chain configuration beyond a separately authorized runtime upgrade.
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
if [[ "${deploy_authority}" -eq 1 ]]; then
	if [[ "${seed_nova_rail_config}" -eq 1 ]]; then
		authority_args+=("--seed-config")
	fi
	"${SCRIPT_DIR}/deploy-arcade-authority.sh" "${authority_args[@]}"
elif [[ "${seed_nova_rail_config}" -eq 1 ]]; then
	"${SCRIPT_DIR}/seed-nova-rail-config.sh"
fi
