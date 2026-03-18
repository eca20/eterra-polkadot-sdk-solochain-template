#!/usr/bin/env bash
set -euo pipefail

DEPLOY_LIB_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${DEPLOY_LIB_DIR}/../.." && pwd)"
WORKSPACE_ROOT="$(cd -- "${REPO_ROOT}/.." && pwd)"
ENV_FILE_DEFAULT="${REPO_ROOT}/deploy/macmini2010.env"
ENV_FILE_EXAMPLE="${REPO_ROOT}/deploy/macmini2010.env.example"
MEDIA_REPO_DIR_DEFAULT="${WORKSPACE_ROOT}/eterra-ipfs-media-service"
ARTIFACTS_DIR="${DEPLOY_LIB_DIR}/.artifacts"
cleanup_paths=()

die() {
	echo "[macmini2010] $*" >&2
	exit 1
}

log() {
	echo "[macmini2010] $*"
}

require_cmd() {
	command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

shell_escape() {
	printf '%q' "$1"
}

append_csv_unique() {
	local csv="$1"
	local item="$2"
	local candidate

	if [[ -z "${item}" ]]; then
		printf '%s' "${csv}"
		return
	fi

	IFS=',' read -r -a candidates <<<"${csv}"
	for candidate in "${candidates[@]:-}"; do
		candidate="${candidate#"${candidate%%[![:space:]]*}"}"
		candidate="${candidate%"${candidate##*[![:space:]]}"}"
		if [[ "${candidate}" == "${item}" ]]; then
			printf '%s' "${csv}"
			return
		fi
	done

	if [[ -n "${csv}" ]]; then
		printf '%s,%s' "${csv}" "${item}"
	else
		printf '%s' "${item}"
	fi
}

register_cleanup_path() {
	cleanup_paths+=("$1")
}

cleanup() {
	local path
	for path in "${cleanup_paths[@]:-}"; do
		if [[ -n "${path}" && -e "${path}" ]]; then
			rm -rf "${path}"
		fi
	done
}

trap cleanup EXIT

load_env() {
	local env_file="${MACMINI2010_ENV_FILE:-${ENV_FILE_DEFAULT}}"

	[[ -f "${env_file}" ]] || die "missing deploy env file: ${env_file} (copy ${ENV_FILE_EXAMPLE} to ${ENV_FILE_DEFAULT})"
	[[ -d "${MEDIA_REPO_DIR:-${MEDIA_REPO_DIR_DEFAULT}}" ]] || die "media repo not found: ${MEDIA_REPO_DIR:-${MEDIA_REPO_DIR_DEFAULT}}"

	set -a
	# shellcheck disable=SC1090
	source "${env_file}"
	set +a

	DEPLOY_HOST="${DEPLOY_HOST:-}"
	DEPLOY_USER="${DEPLOY_USER:-eterra2010}"
	DEPLOY_ROOT="${DEPLOY_ROOT:-/opt/eterra}"
	MINI_LAN_IP="${MINI_LAN_IP:-}"
	LAN_CIDR="${LAN_CIDR:-}"
	CHAIN_RPC_PORT="${CHAIN_RPC_PORT:-9944}"
	CHAIN_P2P_PORT="${CHAIN_P2P_PORT:-30333}"
	MEDIA_PORT="${MEDIA_PORT:-4000}"
	IPFS_API_PORT="${IPFS_API_PORT:-5001}"
	IPFS_GATEWAY_PORT="${IPFS_GATEWAY_PORT:-8080}"
	SSH_PORT="${SSH_PORT:-22}"
	SSH_OPTS="${SSH_OPTS:-}"
	REMOTE_SUDO_PASSWORD="${REMOTE_SUDO_PASSWORD:-}"
	MEDIA_REPO_DIR="${MEDIA_REPO_DIR:-${MEDIA_REPO_DIR_DEFAULT}}"
	MEDIA_ADMIN_API_KEY="${MEDIA_ADMIN_API_KEY:-change-me}"
	MEDIA_SIGNER_SEED="${MEDIA_SIGNER_SEED:-//Alice}"
	SITE_PUBLIC_ORIGIN="${SITE_PUBLIC_ORIGIN:-}"
	CORS_ALLOWED_ORIGINS="${CORS_ALLOWED_ORIGINS:-http://localhost:5173,http://127.0.0.1:5173}"
	MAX_UPLOAD_BYTES="${MAX_UPLOAD_BYTES:-10485760}"
	RENDER_TIMEOUT_MS="${RENDER_TIMEOUT_MS:-15000}"
	RENDER_CONCURRENCY="${RENDER_CONCURRENCY:-4}"
	PUBLIC_RATE_LIMIT_MAX="${PUBLIC_RATE_LIMIT_MAX:-120}"
	PUBLIC_RATE_LIMIT_WINDOW_MS="${PUBLIC_RATE_LIMIT_WINDOW_MS:-60000}"
	ADMIN_RATE_LIMIT_MAX="${ADMIN_RATE_LIMIT_MAX:-30}"
	ADMIN_RATE_LIMIT_WINDOW_MS="${ADMIN_RATE_LIMIT_WINDOW_MS:-60000}"
	ALLOW_DEV_ADMIN_RESET="${ALLOW_DEV_ADMIN_RESET:-1}"
	AURA_SURI="${AURA_SURI:-//Alice}"
	GRAN_SURI="${GRAN_SURI:-//Alice}"
	REMOTE_NODE_DATA_DIR="${REMOTE_NODE_DATA_DIR:-/var/lib/eterra-node-dev}"
	REMOTE_NODE_SERVICE_NAME="${REMOTE_NODE_SERVICE_NAME:-eterra-node}"
	NODE_BUILD_MODE="${NODE_BUILD_MODE:-remote-native}"
	REMOTE_RUST_TOOLCHAIN="${REMOTE_RUST_TOOLCHAIN:-stable}"
	REMOTE_CARGO_JOBS="${REMOTE_CARGO_JOBS:-2}"
	REMOTE_CARGO_HOME="${REMOTE_CARGO_HOME:-/home/${DEPLOY_USER}/.cargo}"
	REMOTE_CARGO_ENV_FILE="${REMOTE_CARGO_ENV_FILE:-${REMOTE_CARGO_HOME}/env}"

	[[ -n "${DEPLOY_HOST}" ]] || die "DEPLOY_HOST must be set in ${env_file}"
	[[ -n "${MINI_LAN_IP}" ]] || die "MINI_LAN_IP must be set in ${env_file}"
	CORS_ALLOWED_ORIGINS="$(append_csv_unique "${CORS_ALLOWED_ORIGINS}" "${SITE_PUBLIC_ORIGIN}")"

	SSH_TARGET="${DEPLOY_USER}@${DEPLOY_HOST}"
	REMOTE_NODE_DIR="${DEPLOY_ROOT}/node/current"
	REMOTE_MEDIA_DIR="${DEPLOY_ROOT}/media/current"
	REMOTE_SHARED_ENV_DIR="${DEPLOY_ROOT}/shared/env"
	REMOTE_NODE_ENV_FILE="${REMOTE_SHARED_ENV_DIR}/node.env"
	REMOTE_MEDIA_ENV_FILE="${REMOTE_SHARED_ENV_DIR}/media.env"
	REMOTE_NODE_BIN="${REMOTE_NODE_DIR}/solochain-eterra-node"
	REMOTE_NODE_SPEC="${REMOTE_NODE_DIR}/dev-raw.json"
	REMOTE_START_SCRIPT="${REMOTE_NODE_DIR}/start-dev-node.sh"
	REMOTE_MEDIA_COMPOSE_BASE="${REMOTE_MEDIA_DIR}/docker-compose.yaml"
	REMOTE_MEDIA_COMPOSE_OVERRIDE="${REMOTE_MEDIA_DIR}/docker-compose.macmini2010.yaml"
	REMOTE_DOCKER_COMPOSE_CMD="docker compose -f '${REMOTE_MEDIA_COMPOSE_BASE}' -f '${REMOTE_MEDIA_COMPOSE_OVERRIDE}' --env-file '${REMOTE_MEDIA_ENV_FILE}'"
	REMOTE_SCRIPT_DIR="${REMOTE_SCRIPT_DIR:-/tmp/macmini2010-${DEPLOY_USER}}"

	SSH_CMD=(ssh -p "${SSH_PORT}")
	if [[ -n "${SSH_OPTS}" ]]; then
		local extra_ssh_opts=()
		# shellcheck disable=SC2206
		extra_ssh_opts=(${SSH_OPTS})
		SSH_CMD+=("${extra_ssh_opts[@]}")
	fi
	SSH_CMD+=("${SSH_TARGET}")

	SSH_TTY_CMD=(ssh -tt -p "${SSH_PORT}")
	if [[ -n "${SSH_OPTS}" ]]; then
		local extra_tty_opts=()
		# shellcheck disable=SC2206
		extra_tty_opts=(${SSH_OPTS})
		SSH_TTY_CMD+=("${extra_tty_opts[@]}")
	fi
	SSH_TTY_CMD+=("${SSH_TARGET}")

	RSYNC_RSH="ssh -p ${SSH_PORT}"
	if [[ -n "${SSH_OPTS}" ]]; then
		RSYNC_RSH+=" ${SSH_OPTS}"
	fi
}

write_remote_askpass_script() {
	local path="$1"
	local encoded_password

	encoded_password="$(printf '%s' "${REMOTE_SUDO_PASSWORD}" | base64)"
	cat >"${path}" <<EOF
#!/bin/sh
printf '%s' '${encoded_password}' | base64 -d
EOF
	chmod 0700 "${path}"
}

remote_exec_script() {
	local run_as="$1"
	local local_script
	local remote_script
	local remote_cmd
	local remote_script_escaped

	local_script="$(mktemp "${TMPDIR:-/tmp}/macmini2010.remote.XXXXXX")"
	register_cleanup_path "${local_script}"
	cat >"${local_script}"
	remote_script="${REMOTE_SCRIPT_DIR}/$(basename "${local_script}")"
	remote_script_escaped="$(shell_escape "${remote_script}")"

	"${SSH_CMD[@]}" "mkdir -p $(shell_escape "${REMOTE_SCRIPT_DIR}")"
	rsync_to_remote_no_delete "${local_script}" "${remote_script}"

	if [[ "${run_as}" == "root" ]]; then
		local local_askpass
		local remote_askpass
		local remote_askpass_escaped
		if [[ -n "${REMOTE_SUDO_PASSWORD}" ]]; then
			local_askpass="$(mktemp "${TMPDIR:-/tmp}/macmini2010.askpass.XXXXXX")"
			register_cleanup_path "${local_askpass}"
			write_remote_askpass_script "${local_askpass}"
			remote_askpass="${REMOTE_SCRIPT_DIR}/$(basename "${local_askpass}")"
			remote_askpass_escaped="$(shell_escape "${remote_askpass}")"
			rsync_to_remote_no_delete "${local_askpass}" "${remote_askpass}"
			printf -v remote_cmd '%s' "chmod 700 ${remote_script_escaped} ${remote_askpass_escaped} && SUDO_ASKPASS=${remote_askpass_escaped} sudo -A bash ${remote_script_escaped}; rc=\$?; rm -f ${remote_script_escaped} ${remote_askpass_escaped}; exit \$rc"
			"${SSH_CMD[@]}" "${remote_cmd}"
			return
		fi

		printf -v remote_cmd '%s' "chmod 700 ${remote_script_escaped} && sudo bash ${remote_script_escaped}; rc=\$?; rm -f ${remote_script_escaped}; exit \$rc"
		"${SSH_TTY_CMD[@]}" "${remote_cmd}"
		return
	fi

	printf -v remote_cmd '%s' "chmod 700 ${remote_script_escaped} && bash ${remote_script_escaped}; rc=\$?; rm -f ${remote_script_escaped}; exit \$rc"
	"${SSH_CMD[@]}" "${remote_cmd}"
}

remote_bash() {
	remote_exec_script user "$@"
}

remote_root_bash() {
	remote_exec_script root "$@"
}

rsync_to_remote() {
	local src="$1"
	local dest="$2"
	rsync -az --delete -e "${RSYNC_RSH}" "${src}" "${SSH_TARGET}:${dest}"
}

rsync_to_remote_no_delete() {
	local src="$1"
	local dest="$2"
	rsync -az -e "${RSYNC_RSH}" "${src}" "${SSH_TARGET}:${dest}"
}

make_temp_dir() {
	local dir
	dir="$(mktemp -d "${TMPDIR:-/tmp}/macmini2010.XXXXXX")"
	register_cleanup_path "${dir}"
	printf '%s\n' "${dir}"
}

ensure_local_artifacts_dir() {
	mkdir -p "${ARTIFACTS_DIR}"
}

write_node_env() {
	local path="$1"
	cat >"${path}" <<EOF
NODE_BIN=${REMOTE_NODE_BIN}
RAW_SPEC=${REMOTE_NODE_SPEC}
BASE_PATH=${REMOTE_NODE_DATA_DIR}
CHAIN_RPC_PORT=${CHAIN_RPC_PORT}
CHAIN_P2P_PORT=${CHAIN_P2P_PORT}
MINI_LAN_IP=${MINI_LAN_IP}
RPC_CORS=all
AURA_SURI=${AURA_SURI}
GRAN_SURI=${GRAN_SURI}
EOF
}

write_media_env() {
	local path="$1"
	cat >"${path}" <<EOF
DOCKERIZED=1
NODE_ENV=production
CHAIN_WS=ws://host.docker.internal:${CHAIN_RPC_PORT}
MEDIA_SIGNER_SEED=${MEDIA_SIGNER_SEED}
IPFS_API=http://ipfs:${IPFS_API_PORT}
IPFS_GATEWAY=http://${MINI_LAN_IP}:${IPFS_GATEWAY_PORT}
PUBLIC_BASE_URL=http://${MINI_LAN_IP}:${MEDIA_PORT}
ADMIN_API_KEY=${MEDIA_ADMIN_API_KEY}
PORT=${MEDIA_PORT}
MAX_UPLOAD_BYTES=${MAX_UPLOAD_BYTES}
CORS_ALLOWED_ORIGINS=${CORS_ALLOWED_ORIGINS}
RENDER_TIMEOUT_MS=${RENDER_TIMEOUT_MS}
RENDER_CONCURRENCY=${RENDER_CONCURRENCY}
PUBLIC_RATE_LIMIT_MAX=${PUBLIC_RATE_LIMIT_MAX}
PUBLIC_RATE_LIMIT_WINDOW_MS=${PUBLIC_RATE_LIMIT_WINDOW_MS}
ADMIN_RATE_LIMIT_MAX=${ADMIN_RATE_LIMIT_MAX}
ADMIN_RATE_LIMIT_WINDOW_MS=${ADMIN_RATE_LIMIT_WINDOW_MS}
ALLOW_DEV_ADMIN_RESET=${ALLOW_DEV_ADMIN_RESET}
IPFS_API_PORT=${IPFS_API_PORT}
IPFS_GATEWAY_PORT=${IPFS_GATEWAY_PORT}
EOF
}

render_runtime_env_bundle() {
	local dir="$1"
	write_node_env "${dir}/node.env"
	write_media_env "${dir}/media.env"
}

sync_runtime_env_bundle() {
	local dir="$1"
	local remote_tmp_dir="${DEPLOY_ROOT}/tmp/runtime-env"

	remote_bash <<EOF
mkdir -p "${remote_tmp_dir}"
EOF
	rsync_to_remote "${dir}/" "${remote_tmp_dir}/"
	remote_root_bash <<EOF
set -euo pipefail

mkdir -p "${REMOTE_SHARED_ENV_DIR}"
install -m 0644 "${remote_tmp_dir}/node.env" "${REMOTE_NODE_ENV_FILE}"
install -m 0644 "${remote_tmp_dir}/media.env" "${REMOTE_MEDIA_ENV_FILE}"
chown root:root "${REMOTE_NODE_ENV_FILE}" "${REMOTE_MEDIA_ENV_FILE}"
rm -rf "${remote_tmp_dir}"
EOF
}
