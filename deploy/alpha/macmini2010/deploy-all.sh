#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

node_args=()
media_args=()
authority_args=()
deploy_authority=0
seed_nova_rail_config=0
fresh=0
fresh_reset_readiness=""
dry_run=0

while [[ $# -gt 0 ]]; do
	case "$1" in
		--purge-state)
			node_args+=("$1")
			;;
		--fresh)
			fresh=1
			node_args+=("--purge-state")
			media_args+=("--fresh")
			;;
		--fresh-reset-readiness)
			[[ $# -ge 2 ]] || { echo "--fresh-reset-readiness requires a packet path" >&2; exit 2; }
			fresh_reset_readiness="$2"
			shift
			;;
		--dry-run)
			dry_run=1
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
		--build-media-candidate)
			[[ $# -ge 2 ]] || { echo "--build-media-candidate requires an output path" >&2; exit 2; }
			media_args+=("--build-candidate" "$2")
			shift
			;;
		--promote-media-candidate)
			[[ $# -ge 2 ]] || { echo "--promote-media-candidate requires a manifest" >&2; exit 2; }
			media_args+=("--promote-candidate" "$2")
			shift
			;;
		--media-evidence)
			[[ $# -ge 2 ]] || { echo "--media-evidence requires an output path" >&2; exit 2; }
			media_args+=("--evidence" "$2")
			shift
			;;
		--help|-h)
			cat <<'EOF'
Usage: deploy-all.sh [--purge-state] [--fresh] [--with-arcade-authority] [--authorize-arcade-authority] [--seed-nova-rail-config]
                     [--fresh-reset-readiness READINESS.json] [--dry-run]
                     [--build-media-candidate OUTPUT.json]
                     [--promote-media-candidate CANDIDATE.json --media-evidence OUTPUT.json]

Normal deploys preserve alpha chain state.
Pass --purge-state to wipe the remote alpha node base path before restart.
Alpha spec/genesis changes are only applied when --purge-state is set.
Release --fresh is accepted only with --fresh-reset-readiness and immutable
media candidate promotion. --dry-run validates that guarded plan before SSH.
Pass --with-arcade-authority to deploy the Nova Rail authority relay API/operator.
Pass --authorize-arcade-authority to deploy and run the one-shot relay authorization operator.
Pass --seed-nova-rail-config to run the explicit idempotent ArcadeCore seed after deploy.
Normal deploys never mutate live chain configuration beyond a separately authorized runtime upgrade.
Release media deployment requires a candidate build followed by a separate immutable promotion.
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

if [[ -n "${fresh_reset_readiness}" ]]; then
	[[ "${fresh}" -eq 1 ]] || { echo "--fresh-reset-readiness requires --fresh" >&2; exit 2; }
	node_args+=("--fresh-reset-readiness" "${fresh_reset_readiness}")
	media_args+=("--fresh-reset-readiness" "${fresh_reset_readiness}")
fi
if [[ "${dry_run}" -eq 1 ]]; then
	[[ "${fresh}" -eq 1 ]] || { echo "--dry-run requires --fresh" >&2; exit 2; }
	node_args+=("--dry-run")
	media_args+=("--dry-run")
fi

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
