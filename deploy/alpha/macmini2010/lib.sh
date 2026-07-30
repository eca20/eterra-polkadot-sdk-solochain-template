#!/usr/bin/env bash
set -euo pipefail

DEPLOY_LIB_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${DEPLOY_LIB_DIR}/../../.." && pwd)"
WORKSPACE_ROOT="$(cd -- "${REPO_ROOT}/.." && pwd)"
ENV_FILE_DEFAULT="${REPO_ROOT}/deploy/alpha/macmini2010.env"
ENV_FILE_EXAMPLE="${REPO_ROOT}/deploy/alpha/macmini2010.env.example"
MEDIA_REPO_DIR_DEFAULT="${WORKSPACE_ROOT}/eterra-ipfs-media-service"
AUTHORITY_REPO_DIR_DEFAULT="${WORKSPACE_ROOT}/SDKGen/Eterra"
ARTIFACTS_DIR="${DEPLOY_LIB_DIR}/.artifacts"
cleanup_paths=()

die() {
	echo "[alpha-macmini2010] $*" >&2
	exit 1
}

log() {
	echo "[alpha-macmini2010] $*"
}

require_cmd() {
	command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

repo_source_commit() {
	local root="$1"

	git -C "${root}" rev-parse HEAD
}

require_release_source() {
	local root="$1"
	local label="$2"
	local expected_commit="${3:-}"
	local actual_commit

	git -C "${root}" rev-parse --is-inside-work-tree >/dev/null 2>&1 || die "${label} source is not a git worktree: ${root}"
	actual_commit="$(repo_source_commit "${root}")"

	if [[ -n "$(git -C "${root}" status --porcelain --untracked-files=all)" ]] &&
		{ [[ "${ETERRA_RELEASE_VERSION:-dev}" != "dev" ]] || ! is_truthy "${ALLOW_DIRTY_DEPLOY:-0}"; }; then
		die "${label} source tree is dirty: ${root}"
	fi

	if [[ -n "${expected_commit}" && "${actual_commit}" != "${expected_commit}" ]]; then
		die "${label} source commit mismatch: expected ${expected_commit}, found ${actual_commit}"
	fi

	if [[ "${ETERRA_RELEASE_VERSION:-dev}" != "dev" && -z "${expected_commit}" ]]; then
		die "${label} expected commit is required when ETERRA_RELEASE_VERSION is not dev"
	fi

	if [[ "${ETERRA_RELEASE_VERSION:-dev}" != "dev" ]]; then
		local release_branch="release/${ETERRA_RELEASE_VERSION}"
		[[ "$(git -C "${root}" rev-parse --verify "refs/heads/${release_branch}")" == "${actual_commit}" ]] ||
			die "${label} local ${release_branch} is not pinned to ${actual_commit}"
		[[ "$(git -C "${root}" ls-remote origin "refs/heads/${release_branch}" | awk '{print $1}')" == "${actual_commit}" ]] ||
			die "${label} remote ${release_branch} is not pinned to ${actual_commit}"
		[[ -z "$(git -C "${root}" show-ref --verify "refs/tags/${ETERRA_RELEASE_VERSION}" 2>/dev/null || true)" ]] ||
			die "${label} local release tag already exists; deploy and validate before tagging"
		[[ -z "$(git -C "${root}" ls-remote origin "refs/tags/${ETERRA_RELEASE_VERSION}")" ]] ||
			die "${label} remote release tag already exists; deploy and validate before tagging"
	fi

	printf '%s\n' "${actual_commit}"
}

shell_escape() {
	printf '%q' "$1"
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

read_secret_value() {
	local value="$1"

	[[ -n "${value}" ]] || die "empty secret value is not allowed"
	if [[ "${value}" == @* ]]; then
		local secret_path="${value#@}"
		[[ -f "${secret_path}" ]] || die "secret file not found: ${secret_path}"
		value="$(<"${secret_path}")"
		value="${value%$'\n'}"
		[[ -n "${value}" ]] || die "secret file is empty: ${secret_path}"
	fi

	printf '%s\n' "${value}"
}

validate_secret() {
	local name="$1"
	local value="$2"

	case "${value}" in
		""|"change-me"|"replace-with-a-strong-random-value")
			die "${name} must be replaced with a strong non-placeholder value"
			;;
	esac
}

is_truthy() {
	case "${1,,}" in
		1|true|yes|on) return 0 ;;
		*) return 1 ;;
	esac
}

ensure_not_dev_seed() {
	local name="$1"
	local value="$2"

	case "${value}" in
		*//Alice*|*//Bob*|*//Charlie*|*//Dave*|*//Eve*|*//Ferdie*|*//AlphaOwner*|*//AlphaValidator*|*//AlphaMediaSigner*)
			die "${name} must not use a development or placeholder seed"
			;;
	esac
}

load_env() {
	local env_file="${ALPHA_MACMINI2010_ENV_FILE:-${ENV_FILE_DEFAULT}}"

	[[ -f "${env_file}" ]] || die "missing deploy env file: ${env_file} (copy ${ENV_FILE_EXAMPLE} to ${ENV_FILE_DEFAULT})"

	set -a
	# shellcheck disable=SC1090
	source "${env_file}"
	set +a

	DEPLOY_HOST="${DEPLOY_HOST:-}"
	DEPLOY_USER="${DEPLOY_USER:-eterra2010}"
	DEPLOY_PASSWORD="${DEPLOY_PASSWORD:-}"
	REMOTE_SUDO_PASSWORD="${REMOTE_SUDO_PASSWORD:-${DEPLOY_PASSWORD:-}}"
	SSH_IDENTITY_FILE="${SSH_IDENTITY_FILE:-}"
	SSH_PUBLIC_KEY_FILE="${SSH_PUBLIC_KEY_FILE:-${SSH_IDENTITY_FILE:+${SSH_IDENTITY_FILE}.pub}}"
	DEPLOY_ROOT="${DEPLOY_ROOT:-/opt/eterra-alpha}"
	LEGACY_DEPLOY_ROOT="${LEGACY_DEPLOY_ROOT:-/opt/eterra}"
	MINI_LAN_IP="${MINI_LAN_IP:-}"
	SITE_PROXY_LAN_IP="${SITE_PROXY_LAN_IP:-}"
	LAN_CIDR="${LAN_CIDR:-}"
	SITE_PUBLIC_ORIGIN="${SITE_PUBLIC_ORIGIN:-}"
	SITE_PUBLIC_ORIGIN="${SITE_PUBLIC_ORIGIN%/}"
	ALPHA_RPC_CORS="${ALPHA_RPC_CORS:-${SITE_PUBLIC_ORIGIN}}"
	ALLOW_LAN_WALLET_RPC_CORS_ALL="${ALLOW_LAN_WALLET_RPC_CORS_ALL:-0}"
	CHAIN_RPC_PORT="${CHAIN_RPC_PORT:-9944}"
	CHAIN_P2P_PORT="${CHAIN_P2P_PORT:-30333}"
	MEDIA_PORT="${MEDIA_PORT:-4000}"
	AUTHORITY_PORT="${AUTHORITY_PORT:-8787}"
	AUTHORITY_BIND_HOST="${AUTHORITY_BIND_HOST:-0.0.0.0}"
	IPFS_API_PORT="${IPFS_API_PORT:-5001}"
	IPFS_GATEWAY_PORT="${IPFS_GATEWAY_PORT:-8080}"
	SSH_PORT="${SSH_PORT:-22}"
	SSH_OPTS="${SSH_OPTS:-}"
	MEDIA_REPO_DIR="${MEDIA_REPO_DIR:-${MEDIA_REPO_DIR_DEFAULT}}"
	AUTHORITY_REPO_DIR="${AUTHORITY_REPO_DIR:-${AUTHORITY_REPO_DIR_DEFAULT}}"
	ALPHA_OVERRIDES_FILE="${ALPHA_OVERRIDES_FILE:-${REPO_ROOT}/chain-specs/alpha-overrides.json}"
	AURA_SURI="${AURA_SURI:-}"
	GRAN_SURI="${GRAN_SURI:-}"
	MEDIA_SIGNER_SEED="${MEDIA_SIGNER_SEED:-}"
	MEDIA_ADMIN_API_KEY="${MEDIA_ADMIN_API_KEY:-}"
	KUBO_IMAGE="${KUBO_IMAGE:-ipfs/kubo:latest}"
	MAX_UPLOAD_BYTES="${MAX_UPLOAD_BYTES:-10485760}"
	PUBLIC_MEDIA_UPLOAD_ENABLED="${PUBLIC_MEDIA_UPLOAD_ENABLED:-false}"
	CHAIN_REQUEST_TIMEOUT_MS="${CHAIN_REQUEST_TIMEOUT_MS:-60000}"
	IPFS_REQUEST_TIMEOUT_MS="${IPFS_REQUEST_TIMEOUT_MS:-8000}"
	RENDER_TIMEOUT_MS="${RENDER_TIMEOUT_MS:-15000}"
	RENDER_CONCURRENCY="${RENDER_CONCURRENCY:-4}"
	PUBLIC_RATE_LIMIT_MAX="${PUBLIC_RATE_LIMIT_MAX:-120}"
	PUBLIC_RATE_LIMIT_WINDOW_MS="${PUBLIC_RATE_LIMIT_WINDOW_MS:-60000}"
	ADMIN_RATE_LIMIT_MAX="${ADMIN_RATE_LIMIT_MAX:-30}"
	ADMIN_RATE_LIMIT_WINDOW_MS="${ADMIN_RATE_LIMIT_WINDOW_MS:-60000}"
	ALLOW_DEV_ADMIN_RESET="${ALLOW_DEV_ADMIN_RESET:-0}"
	AUTHORITY_SUBMITTER_MODE="${AUTHORITY_SUBMITTER_MODE:-live_alpha}"
	AUTHORITY_RPC_URL="${AUTHORITY_RPC_URL:-ws://127.0.0.1:${CHAIN_RPC_PORT}}"
	AUTHORITY_RUNTIME_IDENTIFIER="${AUTHORITY_RUNTIME_IDENTIFIER:-linux-x64}"
	AUTHORITY_PUBLISH_SELF_CONTAINED="${AUTHORITY_PUBLISH_SELF_CONTAINED:-true}"
	AUTHORITY_RELAY_ACCOUNT="${AUTHORITY_RELAY_ACCOUNT:-${NOVA_RAIL_RELAY_ACCOUNT:-}}"
	AUTHORITY_RELAY_MNEMONIC="${AUTHORITY_RELAY_MNEMONIC:-}"
	AUTHORITY_RELAY_DERIVATION_PASSWORD="${AUTHORITY_RELAY_DERIVATION_PASSWORD:-}"
	AUTHORITY_FINALITY_TIMEOUT_SECONDS="${AUTHORITY_FINALITY_TIMEOUT_SECONDS:-90}"
	AUTHORITY_SERVICE_NAME="${AUTHORITY_SERVICE_NAME:-eterra-arcade-authority}"
	ETERRA_RELEASE_VERSION="${ETERRA_RELEASE_VERSION:-dev}"
	ETERRA_EXPECTED_CHAIN_COMMIT="${ETERRA_EXPECTED_CHAIN_COMMIT:-}"
	ETERRA_EXPECTED_MEDIA_COMMIT="${ETERRA_EXPECTED_MEDIA_COMMIT:-}"
	ETERRA_EXPECTED_SDKGEN_COMMIT="${ETERRA_EXPECTED_SDKGEN_COMMIT:-}"
	ALLOW_DIRTY_DEPLOY="${ALLOW_DIRTY_DEPLOY:-0}"
	RUNTIME_SPEC_VERSION="${RUNTIME_SPEC_VERSION:-104}"
	RUNTIME_CODE_HASH="${RUNTIME_CODE_HASH:-unverified}"
	MEDIA_RELEASE_CONTENT_SMOKE_URL="${MEDIA_RELEASE_CONTENT_SMOKE_URL:-}"
	ETERRA_ALPHA_SUDO_MNEMONIC="${ETERRA_ALPHA_SUDO_MNEMONIC:-}"
	ETERRA_ALPHA_SUDO_DERIVATION_PASSWORD="${ETERRA_ALPHA_SUDO_DERIVATION_PASSWORD:-}"
	NOVA_RAIL_RELAY_AUTHORITY_ID="${NOVA_RAIL_RELAY_AUTHORITY_ID:-1}"
	REMOTE_NODE_DATA_DIR="${REMOTE_NODE_DATA_DIR:-/var/lib/eterra-alpha-node}"
	REMOTE_NODE_SERVICE_NAME="${REMOTE_NODE_SERVICE_NAME:-eterra-alpha-node}"
	NODE_BUILD_MODE="${NODE_BUILD_MODE:-remote-native}"
	REMOTE_RUST_TOOLCHAIN="${REMOTE_RUST_TOOLCHAIN:-stable}"
	REMOTE_CARGO_JOBS="${REMOTE_CARGO_JOBS:-2}"
	REMOTE_CARGO_HOME="${REMOTE_CARGO_HOME:-/home/${DEPLOY_USER}/.cargo}"
	REMOTE_CARGO_ENV_FILE="${REMOTE_CARGO_ENV_FILE:-${REMOTE_CARGO_HOME}/env}"
	REMOTE_CARGO_TARGET_DIR="${REMOTE_CARGO_TARGET_DIR:-${DEPLOY_ROOT}/cache/cargo-target}"
	REMOTE_CARGO_INCREMENTAL="${REMOTE_CARGO_INCREMENTAL:-1}"
	REMOTE_CARGO_CLEAN_AFTER_DEPLOY="${REMOTE_CARGO_CLEAN_AFTER_DEPLOY:-1}"
	ENABLE_REMOTE_SCCACHE="${ENABLE_REMOTE_SCCACHE:-1}"
	REMOTE_SCCACHE_DIR="${REMOTE_SCCACHE_DIR:-${DEPLOY_ROOT}/cache/sccache}"

	[[ -n "${DEPLOY_HOST}" ]] || die "DEPLOY_HOST must be set in ${env_file}"
	[[ "${REMOTE_CARGO_TARGET_DIR}" == "${DEPLOY_ROOT}/"* ]] || die "REMOTE_CARGO_TARGET_DIR must remain under DEPLOY_ROOT"
	[[ "${REMOTE_CARGO_TARGET_DIR}" != "${DEPLOY_ROOT}" ]] || die "REMOTE_CARGO_TARGET_DIR must not equal DEPLOY_ROOT"
	if [[ -n "${DEPLOY_PASSWORD}" ]]; then
		DEPLOY_PASSWORD="$(read_secret_value "${DEPLOY_PASSWORD}")"
	fi
	if [[ -n "${REMOTE_SUDO_PASSWORD}" ]]; then
		REMOTE_SUDO_PASSWORD="$(read_secret_value "${REMOTE_SUDO_PASSWORD}")"
	fi
	[[ -n "${SSH_IDENTITY_FILE}" ]] || die "SSH_IDENTITY_FILE must be set in ${env_file}"
	[[ -f "${SSH_IDENTITY_FILE}" ]] || die "SSH identity file not found: ${SSH_IDENTITY_FILE}"
	[[ -n "${SSH_PUBLIC_KEY_FILE}" ]] || die "SSH_PUBLIC_KEY_FILE must be set in ${env_file}"
	[[ -f "${SSH_PUBLIC_KEY_FILE}" ]] || die "SSH public key file not found: ${SSH_PUBLIC_KEY_FILE}"
	[[ -n "${MINI_LAN_IP}" ]] || die "MINI_LAN_IP must be set in ${env_file}"
	[[ -n "${SITE_PROXY_LAN_IP}" ]] || die "SITE_PROXY_LAN_IP must be set in ${env_file}"
	[[ -n "${LAN_CIDR}" ]] || die "LAN_CIDR must be set in ${env_file}"
	[[ -n "${SITE_PUBLIC_ORIGIN}" ]] || die "SITE_PUBLIC_ORIGIN must be set in ${env_file}"
	[[ "${SITE_PUBLIC_ORIGIN}" == https://* ]] || die "SITE_PUBLIC_ORIGIN must use https:// for alpha"
	if [[ "${ALPHA_RPC_CORS}" == "all" ]]; then
		is_truthy "${ALLOW_LAN_WALLET_RPC_CORS_ALL}" || die "ALPHA_RPC_CORS=all is allowed only when ALLOW_LAN_WALLET_RPC_CORS_ALL=1 for local iPhone alpha testing"
	else
		[[ ",${ALPHA_RPC_CORS}," == *",${SITE_PUBLIC_ORIGIN},"* ]] || die "ALPHA_RPC_CORS must include SITE_PUBLIC_ORIGIN for alpha"
	fi
	[[ -n "${AURA_SURI}" ]] || die "AURA_SURI must be set in ${env_file}"
	[[ -n "${GRAN_SURI}" ]] || die "GRAN_SURI must be set in ${env_file}"
	[[ -n "${MEDIA_SIGNER_SEED}" ]] || die "MEDIA_SIGNER_SEED must be set in ${env_file}"
	[[ -n "${MEDIA_ADMIN_API_KEY}" ]] || die "MEDIA_ADMIN_API_KEY must be set in ${env_file}"
	[[ -f "${ALPHA_OVERRIDES_FILE}" ]] || die "alpha overrides file not found: ${ALPHA_OVERRIDES_FILE}"
	[[ -d "${MEDIA_REPO_DIR}" ]] || die "media repo not found: ${MEDIA_REPO_DIR}"

	AURA_SURI="$(read_secret_value "${AURA_SURI}")"
	GRAN_SURI="$(read_secret_value "${GRAN_SURI}")"
	MEDIA_SIGNER_SEED="$(read_secret_value "${MEDIA_SIGNER_SEED}")"
	MEDIA_ADMIN_API_KEY="$(read_secret_value "${MEDIA_ADMIN_API_KEY}")"
	validate_secret "MEDIA_ADMIN_API_KEY" "${MEDIA_ADMIN_API_KEY}"
	ensure_not_dev_seed "AURA_SURI" "${AURA_SURI}"
	ensure_not_dev_seed "GRAN_SURI" "${GRAN_SURI}"
	ensure_not_dev_seed "MEDIA_SIGNER_SEED" "${MEDIA_SIGNER_SEED}"
	if is_truthy "${ALLOW_DEV_ADMIN_RESET}"; then
		die "ALLOW_DEV_ADMIN_RESET must remain disabled for alpha"
	fi
	if is_truthy "${PUBLIC_MEDIA_UPLOAD_ENABLED}"; then
		die "PUBLIC_MEDIA_UPLOAD_ENABLED must remain disabled for alpha"
	fi
	if [[ "${ETERRA_RELEASE_VERSION}" != "dev" ]]; then
		! is_truthy "${ALLOW_DIRTY_DEPLOY}" || die "ALLOW_DIRTY_DEPLOY is forbidden for release deploys"
		[[ "${RUNTIME_SPEC_VERSION}" == "104" ]] || die "release deploy requires runtime spec 104"
		[[ "${RUNTIME_CODE_HASH}" =~ ^0x[0-9a-fA-F]{64}$ ]] || die "release deploy requires a verified runtime code hash"
		[[ "${KUBO_IMAGE}" == *@sha256:* ]] || die "release deploy requires KUBO_IMAGE pinned by registry digest"
		[[ -n "${MEDIA_RELEASE_CONTENT_SMOKE_URL}" ]] || die "release deploy requires MEDIA_RELEASE_CONTENT_SMOKE_URL"
	fi

	SSH_TARGET="${DEPLOY_USER}@${DEPLOY_HOST}"
	REMOTE_NODE_DIR="${DEPLOY_ROOT}/node/current"
	REMOTE_MEDIA_DIR="${DEPLOY_ROOT}/media/current"
	REMOTE_AUTHORITY_DIR="${DEPLOY_ROOT}/arcade-authority/current"
	REMOTE_AUTHORITY_API_DIR="${REMOTE_AUTHORITY_DIR}/api"
	REMOTE_AUTHORITY_OPERATOR_DIR="${REMOTE_AUTHORITY_DIR}/operator"
	REMOTE_AUTHORITY_OPERATOR_BIN="${REMOTE_AUTHORITY_OPERATOR_DIR}/Eterra.Arcade.Authority.Operator"
	REMOTE_SHARED_ENV_DIR="${DEPLOY_ROOT}/shared/env"
	REMOTE_SHARED_SECRET_DIR="${DEPLOY_ROOT}/shared/secrets"
	REMOTE_NODE_ENV_FILE="${REMOTE_SHARED_ENV_DIR}/node.env"
	REMOTE_MEDIA_ENV_FILE="${REMOTE_SHARED_ENV_DIR}/media.env"
	REMOTE_AUTHORITY_ENV_FILE="${REMOTE_SHARED_ENV_DIR}/arcade-authority.env"
	REMOTE_AUTHORITY_RELAY_MNEMONIC_FILE="${REMOTE_SHARED_SECRET_DIR}/nova-rail-relay.mnemonic"
	REMOTE_AUTHORITY_RELAY_DERIVATION_PASSWORD_FILE="${REMOTE_SHARED_SECRET_DIR}/nova-rail-relay.derivation-password"
	REMOTE_NODE_BIN="${REMOTE_NODE_DIR}/solochain-eterra-node"
	REMOTE_NODE_SPEC="${REMOTE_NODE_DIR}/alpha-raw.json"
	REMOTE_NODE_PLAIN_SPEC="${REMOTE_NODE_DIR}/alpha-plain.json"
	REMOTE_START_SCRIPT="${REMOTE_NODE_DIR}/start-alpha-node.sh"
	REMOTE_MEDIA_PROJECT_NAME="${REMOTE_MEDIA_PROJECT_NAME:-eterra-alpha-media}"
	REMOTE_IPFS_DATA_VOLUME="${REMOTE_MEDIA_PROJECT_NAME}_ipfs_data"
	REMOTE_IPFS_STAGING_VOLUME="${REMOTE_MEDIA_PROJECT_NAME}_ipfs_staging"
	REMOTE_MEDIA_COMPOSE_BASE="${REMOTE_MEDIA_DIR}/docker-compose.yaml"
	REMOTE_MEDIA_COMPOSE_OVERRIDE="${REMOTE_MEDIA_DIR}/docker-compose.macmini2010.yaml"
	REMOTE_DOCKER_COMPOSE_CMD="docker compose --project-name '${REMOTE_MEDIA_PROJECT_NAME}' -f '${REMOTE_MEDIA_COMPOSE_BASE}' -f '${REMOTE_MEDIA_COMPOSE_OVERRIDE}' --env-file '${REMOTE_MEDIA_ENV_FILE}'"
	REMOTE_STATE_DIR="${DEPLOY_ROOT}/shared/state"
	REMOTE_NODE_CODE_HASH_FILE="${REMOTE_STATE_DIR}/node-code.sha256"
	REMOTE_NODE_SPEC_HASH_FILE="${REMOTE_STATE_DIR}/node-spec.sha256"
	REMOTE_NODE_RUNTIME_HASH_FILE="${REMOTE_STATE_DIR}/node-runtime.sha256"
	REMOTE_MEDIA_BUILD_HASH_FILE="${REMOTE_STATE_DIR}/media-build.sha256"
	REMOTE_MEDIA_RUNTIME_HASH_FILE="${REMOTE_STATE_DIR}/media-runtime.sha256"
	REMOTE_MEDIA_IMAGE_DIGEST_FILE="${REMOTE_STATE_DIR}/media-image-digest.txt"
	REMOTE_AUTHORITY_BUILD_HASH_FILE="${REMOTE_STATE_DIR}/arcade-authority-build.sha256"
	REMOTE_AUTHORITY_RUNTIME_HASH_FILE="${REMOTE_STATE_DIR}/arcade-authority-runtime.sha256"
	REMOTE_AUTHORITY_ARTIFACT_HASH_FILE="${REMOTE_STATE_DIR}/arcade-authority-artifact.sha256"
	REMOTE_RELEASE_VERSION_FILE="${REMOTE_STATE_DIR}/release-version.txt"
	REMOTE_CHAIN_SOURCE_COMMIT_FILE="${REMOTE_STATE_DIR}/chain-source-commit.txt"
	REMOTE_MEDIA_SOURCE_COMMIT_FILE="${REMOTE_STATE_DIR}/media-source-commit.txt"
	REMOTE_AUTHORITY_SOURCE_COMMIT_FILE="${REMOTE_STATE_DIR}/authority-source-commit.txt"
	REMOTE_NODE_SERVICE_UNIT_FILE="/etc/systemd/system/${REMOTE_NODE_SERVICE_NAME}.service"
	REMOTE_AUTHORITY_SERVICE_UNIT_FILE="/etc/systemd/system/${AUTHORITY_SERVICE_NAME}.service"
	REMOTE_SCRIPT_DIR="${REMOTE_SCRIPT_DIR:-/tmp/alpha-macmini2010-${DEPLOY_USER}}"
	LEGACY_NODE_SERVICE_NAME="${LEGACY_NODE_SERVICE_NAME:-eterra-node}"
	LEGACY_MEDIA_COMPOSE_BASE="${LEGACY_DEPLOY_ROOT}/media/current/docker-compose.yaml"
	LEGACY_MEDIA_COMPOSE_OVERRIDE="${LEGACY_DEPLOY_ROOT}/media/current/docker-compose.macmini2010.yaml"
	LEGACY_MEDIA_ENV_FILE="${LEGACY_DEPLOY_ROOT}/shared/env/media.env"
	LEGACY_MEDIA_COMPOSE_CMD="docker compose -f '${LEGACY_MEDIA_COMPOSE_BASE}' -f '${LEGACY_MEDIA_COMPOSE_OVERRIDE}' --env-file '${LEGACY_MEDIA_ENV_FILE}'"
	LOCAL_FINALIZED_ALPHA_DIR="${LOCAL_FINALIZED_ALPHA_DIR:-${REPO_ROOT}/chain-specs/finalized/alpha}"

	SSH_CMD=(ssh -o StrictHostKeyChecking=accept-new -i "${SSH_IDENTITY_FILE}" -p "${SSH_PORT}")
	SCP_CMD=(scp -o StrictHostKeyChecking=accept-new -i "${SSH_IDENTITY_FILE}" -P "${SSH_PORT}")
	RSYNC_RSH="ssh -o StrictHostKeyChecking=accept-new -i ${SSH_IDENTITY_FILE} -p ${SSH_PORT}"
	if [[ -n "${SSH_OPTS}" ]]; then
		local extra_ssh_opts=()
		# shellcheck disable=SC2206
		extra_ssh_opts=(${SSH_OPTS})
		SSH_CMD+=("${extra_ssh_opts[@]}")
		SCP_CMD+=("${extra_ssh_opts[@]}")
		RSYNC_RSH+=" ${SSH_OPTS}"
	fi
	SSH_CMD+=("${SSH_TARGET}")
}

run_with_optional_password() {
	local password="$1"
	shift

	if [[ -z "${password}" ]]; then
		"$@"
		return
	fi

	expect -f /dev/stdin -- "${password}" "$@" <<'EOF'
set timeout -1
set password [lindex $argv 0]
set cmd [lrange $argv 1 end]

if {[llength $cmd] == 0} {
	puts stderr "missing command"
	exit 1
}

spawn -noecho {*}$cmd
expect {
	-re {(?i)are you sure you want to continue connecting.*} {
		send -- "yes\r"
		exp_continue
	}
	-re {(?i)(password|passphrase).*:} {
		send -- "$password\r"
		exp_continue
	}
	eof {
		catch wait result
		set exit_status [lindex $result 3]
		if {$exit_status eq ""} {
			set exit_status 0
		}
		exit $exit_status
	}
}
EOF
}

ssh_to_remote() {
	run_with_optional_password "${DEPLOY_PASSWORD}" "${SSH_CMD[@]}" "$@"
}

rsync_with_remote() {
	run_with_optional_password "${DEPLOY_PASSWORD}" rsync "$@"
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

	local_script="$(mktemp "${TMPDIR:-/tmp}/alpha-macmini2010.remote.XXXXXX")"
	register_cleanup_path "${local_script}"
	cat >"${local_script}"

	remote_script="${REMOTE_SCRIPT_DIR}/$(basename "${local_script}")"
	remote_script_escaped="$(shell_escape "${remote_script}")"

	ssh_to_remote "mkdir -p $(shell_escape "${REMOTE_SCRIPT_DIR}")"
	rsync_to_remote_no_delete "${local_script}" "${remote_script}"

	if [[ "${run_as}" == "root" ]]; then
		local local_askpass
		local remote_askpass
		local remote_askpass_escaped

		if [[ -n "${REMOTE_SUDO_PASSWORD}" ]]; then
			local_askpass="$(mktemp "${TMPDIR:-/tmp}/alpha-macmini2010.askpass.XXXXXX")"
			register_cleanup_path "${local_askpass}"
			write_remote_askpass_script "${local_askpass}"
			remote_askpass="${REMOTE_SCRIPT_DIR}/$(basename "${local_askpass}")"
			remote_askpass_escaped="$(shell_escape "${remote_askpass}")"
			rsync_to_remote_no_delete "${local_askpass}" "${remote_askpass}"
			printf -v remote_cmd '%s' "chmod 700 ${remote_script_escaped} ${remote_askpass_escaped} && SUDO_ASKPASS=${remote_askpass_escaped} sudo -A bash ${remote_script_escaped}; rc=\$?; rm -f ${remote_script_escaped} ${remote_askpass_escaped}; exit \$rc"
			ssh_to_remote "${remote_cmd}"
			return
		fi

		printf -v remote_cmd '%s' "chmod 700 ${remote_script_escaped} && sudo bash ${remote_script_escaped}; rc=\$?; rm -f ${remote_script_escaped}; exit \$rc"
		ssh_to_remote "${remote_cmd}"
		return
	fi

	printf -v remote_cmd '%s' "chmod 700 ${remote_script_escaped} && bash ${remote_script_escaped}; rc=\$?; rm -f ${remote_script_escaped}; exit \$rc"
	ssh_to_remote "${remote_cmd}"
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
	rsync_with_remote -az --delete -e "${RSYNC_RSH}" "${src}" "${SSH_TARGET}:${dest}"
}

rsync_to_remote_no_delete() {
	local src="$1"
	local dest="$2"
	rsync_with_remote -az -e "${RSYNC_RSH}" "${src}" "${SSH_TARGET}:${dest}"
}

rsync_from_remote_no_delete() {
	local src="$1"
	local dest="$2"
	rsync_with_remote -az -e "${RSYNC_RSH}" "${SSH_TARGET}:${src}" "${dest}"
}

make_temp_dir() {
	local dir
	dir="$(mktemp -d "${TMPDIR:-/tmp}/alpha-macmini2010.XXXXXX")"
	register_cleanup_path "${dir}"
	printf '%s\n' "${dir}"
}

hash_file() {
	local path="$1"

	require_cmd shasum
	[[ -f "${path}" ]] || die "hash input file not found: ${path}"
	shasum -a 256 "${path}" | awk '{print $1}'
}

combine_hash_values() {
	[[ $# -gt 0 ]] || die "combine_hash_values requires at least one hash value"
	require_cmd shasum
	printf '%s\n' "$@" | shasum -a 256 | awk '{print $1}'
}

hash_repo_paths() {
	local root="$1"
	shift

	local list_file

	[[ $# -gt 0 ]] || die "hash_repo_paths requires at least one path"
	require_cmd git
	require_cmd shasum

	list_file="$(mktemp "${TMPDIR:-/tmp}/alpha-macmini2010.hashlist.XXXXXX")"
	register_cleanup_path "${list_file}"

	git -C "${root}" ls-files -z --cached --others --exclude-standard -- "$@" >"${list_file}"
	[[ -s "${list_file}" ]] || die "no deploy hash inputs found under ${root}"

	(
		cd "${root}"
		tr '\0' '\n' <"${list_file}" | LC_ALL=C sort | while IFS= read -r rel_path; do
			[[ -n "${rel_path}" ]] || continue
			shasum -a 256 "${rel_path}"
		done | shasum -a 256 | awk '{print $1}'
	)
}

compute_node_code_hash() {
	hash_repo_paths "${REPO_ROOT}" Cargo.toml Cargo.lock node runtime pallets crates
}

compute_node_spec_hash() {
	combine_hash_values \
		"$(hash_repo_paths "${REPO_ROOT}" scripts/finalize-alpha-spec.py)" \
		"$(hash_file "${ALPHA_OVERRIDES_FILE}")"
}

compute_node_runtime_hash() {
	local env_file="$1"

	combine_hash_values \
		"$(hash_file "${env_file}")" \
		"$(hash_repo_paths "${REPO_ROOT}" deploy/alpha/macmini2010/start-alpha-node.sh deploy/alpha/macmini2010/eterra-alpha-node.service)"
}

compute_media_build_hash() {
	hash_repo_paths "${MEDIA_REPO_DIR}" Dockerfile package.json package-lock.json yarn.lock tsconfig.json src
}

compute_media_runtime_hash() {
	local env_file="$1"

	combine_hash_values \
		"$(hash_file "${env_file}")" \
		"$(hash_repo_paths "${MEDIA_REPO_DIR}" docker-compose.yaml docker-compose.macmini2010.yaml)"
}

ensure_local_artifacts_dir() {
	mkdir -p "${ARTIFACTS_DIR}" "${LOCAL_FINALIZED_ALPHA_DIR}"
}

write_node_env() {
	local path="$1"
	cat >"${path}" <<EOF
ETERRA_RELEASE_VERSION=${ETERRA_RELEASE_VERSION}
ETERRA_SOURCE_COMMIT=${CHAIN_SOURCE_COMMIT:-unknown}
NODE_BIN=${REMOTE_NODE_BIN}
RAW_SPEC=${REMOTE_NODE_SPEC}
BASE_PATH=${REMOTE_NODE_DATA_DIR}
CHAIN_RPC_PORT=${CHAIN_RPC_PORT}
CHAIN_P2P_PORT=${CHAIN_P2P_PORT}
MINI_LAN_IP=${MINI_LAN_IP}
RPC_CORS=${ALPHA_RPC_CORS}
AURA_SURI=${AURA_SURI}
GRAN_SURI=${GRAN_SURI}
EOF
}

write_media_env() {
	local path="$1"
	local media_node_env="production"
	if [[ "${ETERRA_RELEASE_VERSION}" == "dev" ]]; then
		media_node_env="development"
	fi
	cat >"${path}" <<EOF
RELEASE_VERSION=${ETERRA_RELEASE_VERSION}
SOURCE_COMMIT=${MEDIA_SOURCE_COMMIT:-unknown}
RUNTIME_SPEC_VERSION=${RUNTIME_SPEC_VERSION}
RUNTIME_CODE_HASH=${RUNTIME_CODE_HASH}
DOCKERIZED=1
NODE_ENV=${media_node_env}
KUBO_IMAGE=${KUBO_IMAGE}
CHAIN_WS=ws://host.docker.internal:${CHAIN_RPC_PORT}
MEDIA_SIGNER_SEED=${MEDIA_SIGNER_SEED}
IPFS_API=http://ipfs:${IPFS_API_PORT}
IPFS_GATEWAY=${SITE_PUBLIC_ORIGIN}/ipfs
PUBLIC_BASE_URL=${SITE_PUBLIC_ORIGIN}/media-api
ADMIN_API_KEY=${MEDIA_ADMIN_API_KEY}
PORT=${MEDIA_PORT}
MAX_UPLOAD_BYTES=${MAX_UPLOAD_BYTES}
PUBLIC_MEDIA_UPLOAD_ENABLED=${PUBLIC_MEDIA_UPLOAD_ENABLED}
CORS_ALLOWED_ORIGINS=${SITE_PUBLIC_ORIGIN}
CHAIN_REQUEST_TIMEOUT_MS=${CHAIN_REQUEST_TIMEOUT_MS}
IPFS_REQUEST_TIMEOUT_MS=${IPFS_REQUEST_TIMEOUT_MS}
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

write_authority_env() {
	local path="$1"
	cat >"${path}" <<EOF
ETERRA_RELEASE_VERSION=${ETERRA_RELEASE_VERSION}
ETERRA_SOURCE_COMMIT=${AUTHORITY_SOURCE_COMMIT:-unknown}
ASPNETCORE_URLS=http://${AUTHORITY_BIND_HOST}:${AUTHORITY_PORT}
AUTHORITY_SUBMITTER_MODE=${AUTHORITY_SUBMITTER_MODE}
ALPHA_RPC_URL=${AUTHORITY_RPC_URL}
NOVA_RAIL_RELAY_ACCOUNT=${AUTHORITY_RELAY_ACCOUNT}
NOVA_RAIL_RELAY_MNEMONIC_FILE=${REMOTE_AUTHORITY_RELAY_MNEMONIC_FILE}
NOVA_RAIL_RELAY_DERIVATION_PASSWORD_FILE=${REMOTE_AUTHORITY_RELAY_DERIVATION_PASSWORD_FILE}
NOVA_RAIL_RELAY_FINALITY_TIMEOUT_SECONDS=${AUTHORITY_FINALITY_TIMEOUT_SECONDS}
EOF
}

render_runtime_env_bundle() {
	local dir="$1"
	write_node_env "${dir}/node.env"
	write_media_env "${dir}/media.env"
	write_authority_env "${dir}/arcade-authority.env"
}
