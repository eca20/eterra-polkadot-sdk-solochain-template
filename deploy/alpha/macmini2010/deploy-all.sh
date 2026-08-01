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

node_args=()
media_args=()
authority_args=()
deploy_authority=0
seed_nova_rail_config=0
fresh=0
fresh_reset_readiness=""
pre_reset_closure_handoff=""
pre_reset_closure_handoff_sha256=""
dry_run=0
phase1_closed=0
build_media_candidate=0
authority_candidate=""
authority_evidence=""

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
		--pre-reset-closure-handoff)
			[[ $# -ge 2 ]] || { echo "--pre-reset-closure-handoff requires a receipt path" >&2; exit 2; }
			pre_reset_closure_handoff="$2"
			shift
			;;
		--pre-reset-closure-handoff-sha256)
			[[ $# -ge 2 ]] || { echo "--pre-reset-closure-handoff-sha256 requires a SHA-256" >&2; exit 2; }
			pre_reset_closure_handoff_sha256="$2"
			shift
			;;
		--dry-run)
			dry_run=1
			;;
		--phase1-closed)
			phase1_closed=1
			deploy_authority=1
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
			build_media_candidate=1
			media_args+=("--build-candidate" "$2")
			shift
			;;
		--promote-node-candidate)
			[[ $# -ge 2 ]] || { echo "--promote-node-candidate requires node-candidate.json" >&2; exit 2; }
			node_args+=("--promote-candidate" "$2")
			shift
			;;
		--node-evidence)
			[[ $# -ge 2 ]] || { echo "--node-evidence requires an output path" >&2; exit 2; }
			node_args+=("--evidence" "$2")
			shift
			;;
		--node-target-identity)
			[[ $# -ge 2 ]] || { echo "--node-target-identity requires a target identity JSON" >&2; exit 2; }
			node_args+=("--target-identity" "$2")
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
		--promote-authority-candidate)
			[[ $# -ge 2 ]] || { echo "--promote-authority-candidate requires authority-candidate.json" >&2; exit 2; }
			authority_candidate="$2"
			deploy_authority=1
			shift
			;;
		--authority-evidence)
			[[ $# -ge 2 ]] || { echo "--authority-evidence requires an output path" >&2; exit 2; }
			authority_evidence="$2"
			deploy_authority=1
			shift
			;;
		--help|-h)
			cat <<'EOF'
Usage: deploy-all.sh [--purge-state] [--fresh] [--phase1-closed] [--with-arcade-authority] [--authorize-arcade-authority] [--seed-nova-rail-config]
                     [--fresh-reset-readiness READINESS.json] [--dry-run]
                     [--pre-reset-closure-handoff HANDOFF.json --pre-reset-closure-handoff-sha256 SHA256]
                     [--promote-node-candidate NODE-CANDIDATE.json --node-target-identity TARGET.json --node-evidence OUTPUT.json]
                     [--build-media-candidate OUTPUT.json]
                     [--promote-media-candidate CANDIDATE.json --media-evidence OUTPUT.json]
                     [--promote-authority-candidate authority-candidate.json --authority-evidence OUTPUT.json]

Normal deploys preserve alpha chain state.
Pass --purge-state to wipe the remote alpha node base path before restart.
Alpha spec/genesis changes are only applied when --purge-state is set.
Release --fresh is accepted only with --fresh-reset-readiness and immutable
node and media candidate promotion. The node path installs the exact locally
finalized binary/spec/genesis without a remote build. --dry-run validates that
guarded plan before SSH.
Pass --with-arcade-authority to deploy the Nova Rail authority relay API/operator.
Pass --authorize-arcade-authority to deploy and run the one-shot relay authorization operator.
Pass --seed-nova-rail-config to run the explicit idempotent ArcadeCore seed after deploy.
Normal deploys never mutate live chain configuration beyond a separately authorized runtime upgrade.
Release media deployment requires a candidate build followed by a separate immutable promotion.
The media candidate build is a standalone, non-mutating operation and cannot be
combined with node, reset, authority, promotion, or evidence options.
Pass --phase1-closed with --fresh to preclose every RPC, P2P, and authority
firewall rule before restart and launch RPC plus the Nexus V2 authority on
loopback only. It implies --with-arcade-authority and forbids authorization or
configuration seeding. A Phase-1 dry-run never builds or contacts a host.
The authority path promotes only the exact pre-published candidate and emits a
closed deployment receipt; it never invokes dotnet publish during release.
The guarded fresh reset also requires the canonical, short-lived pre-reset
closure handoff and its SHA-256. The node consumes it immediately before the
first remote mutation; media and authority bind to the same immutable receipt.
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

if [[ "${build_media_candidate}" -eq 1 ]]; then
	[[ "${fresh}" -eq 0 && "${dry_run}" -eq 0 && "${phase1_closed}" -eq 0 ]] || {
		echo "--build-media-candidate cannot be combined with reset or Phase-1 options" >&2
		exit 2
	}
	[[ -z "${fresh_reset_readiness}" && -z "${pre_reset_closure_handoff}" && -z "${pre_reset_closure_handoff_sha256}" && "${deploy_authority}" -eq 0 && "${seed_nova_rail_config}" -eq 0 ]] || {
		echo "--build-media-candidate cannot be combined with authority, seed, or readiness options" >&2
		exit 2
	}
	[[ "${#node_args[@]}" -eq 0 && "${#media_args[@]}" -eq 2 ]] || {
		echo "--build-media-candidate must be the only candidate or deployment action" >&2
		exit 2
	}
	"${SCRIPT_DIR}/deploy-media.sh" "${media_args[@]}"
	exit 0
fi

if [[ "${phase1_closed}" -eq 1 ]]; then
	[[ "${fresh}" -eq 1 ]] || { echo "--phase1-closed requires --fresh" >&2; exit 2; }
	[[ "${seed_nova_rail_config}" -eq 0 && ${#authority_args[@]} -eq 0 ]] || {
		echo "--phase1-closed forbids authority authorization and config seeding" >&2
		exit 2
	}
	[[ -n "${pre_reset_closure_handoff}" && -n "${pre_reset_closure_handoff_sha256}" ]] || {
		echo "--phase1-closed requires --pre-reset-closure-handoff and --pre-reset-closure-handoff-sha256" >&2
		exit 2
	}
	[[ -n "${authority_candidate}" && -n "${authority_evidence}" ]] || {
		echo "--phase1-closed requires immutable authority candidate promotion and evidence output" >&2
		exit 2
	}
	[[ "${pre_reset_closure_handoff_sha256}" =~ ^[0-9a-f]{64}$ ]] || {
		echo "--pre-reset-closure-handoff-sha256 must be 64 lowercase hex characters" >&2
		exit 2
	}
	node_args+=("--phase1-closed")
	media_args+=("--phase1-closed")
	authority_args+=("--phase1-closed")
	node_args+=("--pre-reset-closure-handoff" "${pre_reset_closure_handoff}" "--pre-reset-closure-handoff-sha256" "${pre_reset_closure_handoff_sha256}")
	media_args+=("--pre-reset-closure-handoff" "${pre_reset_closure_handoff}" "--pre-reset-closure-handoff-sha256" "${pre_reset_closure_handoff_sha256}")
	authority_args+=("--pre-reset-closure-handoff" "${pre_reset_closure_handoff}" "--pre-reset-closure-handoff-sha256" "${pre_reset_closure_handoff_sha256}")
elif [[ -n "${pre_reset_closure_handoff}" || -n "${pre_reset_closure_handoff_sha256}" ]]; then
	echo "pre-reset closure handoff options are valid only with --phase1-closed" >&2
	exit 2
fi

if [[ -n "${authority_candidate}" || -n "${authority_evidence}" ]]; then
	[[ -n "${authority_candidate}" && -n "${authority_evidence}" ]] || {
		echo "--promote-authority-candidate and --authority-evidence must be provided together" >&2
		exit 2
	}
	authority_args+=("--promote-candidate" "${authority_candidate}" "--evidence" "${authority_evidence}")
fi

if [[ -n "${fresh_reset_readiness}" ]]; then
	[[ "${fresh}" -eq 1 ]] || { echo "--fresh-reset-readiness requires --fresh" >&2; exit 2; }
	node_args+=("--fresh-reset-readiness" "${fresh_reset_readiness}")
	media_args+=("--fresh-reset-readiness" "${fresh_reset_readiness}")
fi
if [[ "${dry_run}" -eq 1 ]]; then
	[[ "${fresh}" -eq 1 ]] || { echo "--dry-run requires --fresh" >&2; exit 2; }
	node_args+=("--dry-run")
	media_args+=("--dry-run")
	if [[ "${deploy_authority}" -eq 1 ]]; then
		authority_args+=("--dry-run")
	fi
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
