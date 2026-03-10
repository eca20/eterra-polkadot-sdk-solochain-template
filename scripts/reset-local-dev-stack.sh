#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
WORKSPACE_ROOT="$(cd -- "${ROOT_DIR}/.." && pwd)"

MODE="default"
CHAIN="dev"
ROLE="validator"
BASE_PATH=""
RESET_CHAIN=1
RESET_MEDIA=1
RESTART_MEDIA=0
KILL_NODE=0
YES=0

MEDIA_DIR_DEFAULT="${WORKSPACE_ROOT}/eterra-ipfs-media-service"
MEDIA_DIR="${MEDIA_DIR:-${MEDIA_DIR_DEFAULT}}"
COMPOSE_FILE=""

usage() {
	cat <<'EOF'
usage: scripts/reset-local-dev-stack.sh [options]

Reset the local dev stack back to a clean state.

By default this script:
  - removes the local node base-path for MODE=default CHAIN=dev ROLE=validator
  - runs docker compose down --volumes for ../eterra-ipfs-media-service

options:
  --yes                 required confirmation flag; otherwise the script only prints what it would delete
  --mode <value>        chain mode (default: default)
  --chain <value>       chain profile (default: dev)
  --role <value>        node role (default: validator)
  --base-path <path>    explicit node base-path to delete
  --media-dir <path>    explicit eterra-ipfs-media-service path
  --chain-only          reset only the local node base-path
  --media-only          reset only dockerized media/IPFS state
  --restart-media       restart docker compose after resetting media/IPFS volumes
  --kill-node           if a matching local node is running, stop it automatically instead of aborting
  -h, --help            show this help

examples:
  ./scripts/reset-local-dev-stack.sh --yes
  ./scripts/reset-local-dev-stack.sh --yes --media-only --restart-media
  ./scripts/reset-local-dev-stack.sh --yes --mode default --chain dev --role validator
EOF
}

while [[ $# -gt 0 ]]; do
	case "$1" in
		--yes)
			YES=1
			shift
			;;
		--mode)
			MODE="${2:-}"
			shift 2
			;;
		--chain)
			CHAIN="${2:-}"
			shift 2
			;;
		--role)
			ROLE="${2:-}"
			shift 2
			;;
		--base-path)
			BASE_PATH="${2:-}"
			shift 2
			;;
		--media-dir)
			MEDIA_DIR="${2:-}"
			shift 2
			;;
		--chain-only)
			RESET_CHAIN=1
			RESET_MEDIA=0
			shift
			;;
		--media-only)
			RESET_CHAIN=0
			RESET_MEDIA=1
			shift
			;;
		--restart-media)
			RESTART_MEDIA=1
			shift
			;;
		--kill-node)
			KILL_NODE=1
			shift
			;;
		-h|--help)
			usage
			exit 0
			;;
		*)
			echo "[reset-local-dev] unknown argument: $1" >&2
			usage >&2
			exit 1
			;;
	esac
done

if [[ "${RESET_CHAIN}" != "1" && "${RESET_MEDIA}" != "1" ]]; then
	echo "[reset-local-dev] nothing selected; choose default behavior or pass --chain-only / --media-only" >&2
	exit 1
fi

if [[ -z "${BASE_PATH}" ]]; then
	BASE_PATH="${ROOT_DIR}/data/${MODE}-${CHAIN}-${ROLE}"
fi

if [[ -f "${MEDIA_DIR}/docker-compose.yaml" ]]; then
	COMPOSE_FILE="${MEDIA_DIR}/docker-compose.yaml"
elif [[ -f "${MEDIA_DIR}/docker-compose.yml" ]]; then
	COMPOSE_FILE="${MEDIA_DIR}/docker-compose.yml"
fi

print_plan() {
	echo "[reset-local-dev] workspace root: ${WORKSPACE_ROOT}"
	if [[ "${RESET_CHAIN}" == "1" ]]; then
		echo "[reset-local-dev] chain data path: ${BASE_PATH}"
	fi
	if [[ "${RESET_MEDIA}" == "1" ]]; then
		echo "[reset-local-dev] media dir: ${MEDIA_DIR}"
		if [[ -n "${COMPOSE_FILE}" ]]; then
			echo "[reset-local-dev] compose file: ${COMPOSE_FILE}"
			echo "[reset-local-dev] docker compose down --volumes will remove the local IPFS volumes declared there"
		else
			echo "[reset-local-dev] compose file: not found"
		fi
	fi
}

print_plan

if [[ "${YES}" != "1" ]]; then
	echo "[reset-local-dev] refusing destructive reset without --yes" >&2
	exit 1
fi

ensure_command() {
	local cmd="$1"
	if ! command -v "${cmd}" >/dev/null 2>&1; then
		echo "[reset-local-dev] required command not found: ${cmd}" >&2
		exit 1
	fi
}

if [[ "${RESET_CHAIN}" == "1" ]]; then
	if [[ -d "${BASE_PATH}" ]]; then
		local_node_pids="$(pgrep -f "solochain-eterra-node.*${BASE_PATH}" || true)"
		if [[ -n "${local_node_pids}" ]]; then
			if [[ "${KILL_NODE}" == "1" ]]; then
				echo "[reset-local-dev] stopping node pid(s): ${local_node_pids}"
				while read -r pid; do
					[[ -n "${pid}" ]] && kill "${pid}"
				done <<< "${local_node_pids}"
				sleep 1
			else
				echo "[reset-local-dev] node appears to still be running against ${BASE_PATH}" >&2
				echo "[reset-local-dev] stop it first or rerun with --kill-node" >&2
				exit 1
			fi
		fi

		echo "[reset-local-dev] removing chain base-path ${BASE_PATH}"
		rm -rf "${BASE_PATH}"
	else
		echo "[reset-local-dev] chain base-path not present, nothing to remove"
	fi
fi

if [[ "${RESET_MEDIA}" == "1" ]]; then
	ensure_command docker
	if [[ -z "${COMPOSE_FILE}" ]]; then
		echo "[reset-local-dev] could not find docker compose file under ${MEDIA_DIR}" >&2
		exit 1
	fi

	echo "[reset-local-dev] stopping media/IPFS containers and removing compose volumes"
	docker compose -f "${COMPOSE_FILE}" down --volumes --remove-orphans

	if [[ "${RESTART_MEDIA}" == "1" ]]; then
		echo "[reset-local-dev] restarting media/IPFS stack"
		docker compose -f "${COMPOSE_FILE}" up -d --build
	fi
fi

echo "[reset-local-dev] reset complete"
if [[ "${RESET_CHAIN}" == "1" ]]; then
	echo "[reset-local-dev] next: restart the node with make run-node MODE=${MODE} CHAIN=${CHAIN} PROFILE=debug ROLE=${ROLE}"
fi
if [[ "${RESET_MEDIA}" == "1" && "${RESTART_MEDIA}" != "1" ]]; then
	echo "[reset-local-dev] next: restart media/IPFS with cd ${MEDIA_DIR} && docker compose up -d --build"
fi
