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
# Closed list of every credential value accepted or derived by this deployment
# library. Never let ambient values cross even the early command substitutions
# used while the library initializes.
declare -ar NEXUS_V2_DEPLOYMENT_SECRET_VARIABLES=(
	DEPLOY_PASSWORD
	REMOTE_SUDO_PASSWORD
	AURA_SURI
	GRAN_SURI
	MEDIA_SIGNER_SEED
	MEDIA_ADMIN_API_KEY
	AUTHORITY_RELAY_MNEMONIC
	AUTHORITY_RELAY_DERIVATION_PASSWORD
	ETERRA_LEGENDS_SIGNER_MNEMONIC
	ETERRA_LEGENDS_SIGNER_DERIVATION_PASSWORD
	ETERRA_LEGENDS_PRIVATE_ALPHA_ACCESS_KEY
	ETERRA_ALPHA_SUDO_MNEMONIC
	ETERRA_ALPHA_SUDO_DERIVATION_PASSWORD
	ADMIN_SESSION_SECRET
	ALPHA_ACCESS_SESSION_SECRET
	DISCORD_CLIENT_SECRET
	DISCORD_BOT_TOKEN
	MONGODB_URI
	ETERRA_LEGENDS_PLAYER_ACCESS_TOKEN
	NEXUS_V2_PRIVATE_ALPHA_ACCESS_KEY
	NEXUS_V2_SESSION_AUTHORIZATION_PROFILES_JSON
	ADMIN_API_KEY
	ETERRA_FPS_V2_OWNER_SECRET_PATH
	ETERRA_FPS_V2_PLAYER_GATEWAY_ACCESS_TOKEN
	ETERRA_FPS_V2_ROOT_SECRET_PATH
	ETERRA_FPS_V2_SUDO_SECRET_PATH
)
export -n "${NEXUS_V2_DEPLOYMENT_SECRET_VARIABLES[@]}" 2>/dev/null || true

DEPLOY_LIB_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${DEPLOY_LIB_DIR}/../../.." && pwd)"
WORKSPACE_ROOT="$(cd -- "${REPO_ROOT}/.." && pwd)"
ENV_FILE_DEFAULT="${REPO_ROOT}/deploy/alpha/macmini2010.env"
ENV_FILE_EXAMPLE="${REPO_ROOT}/deploy/alpha/macmini2010.env.example"
MEDIA_REPO_DIR_DEFAULT="${WORKSPACE_ROOT}/eterra-ipfs-media-service"
AUTHORITY_REPO_DIR_DEFAULT="${WORKSPACE_ROOT}/SDKGen/Eterra"
ARTIFACTS_DIR="${DEPLOY_LIB_DIR}/.artifacts"
RESET_READINESS_VERIFIER="${REPO_ROOT}/scripts/nexus-v2-private-alpha/verify_reset_readiness.py"
PRE_RESET_CLOSURE_TOOL="${REPO_ROOT}/scripts/nexus-v2-private-alpha/pre_reset_closure.py"
NODE_CANDIDATE_TOOL="${REPO_ROOT}/scripts/nexus-v2-private-alpha/node_candidate.py"
AUTHORITY_CANDIDATE_TOOL="${REPO_ROOT}/scripts/nexus-v2-private-alpha/authority_candidate.py"
SSH_HOST_PIN_VALIDATOR="${REPO_ROOT}/scripts/nexus-v2-private-alpha/capture_ssh_host_pins.py"
NEXUS_V2_SSH_HOST_KEY_ALGORITHMS="ssh-ed25519,ecdsa-sha2-nistp256,rsa-sha2-512,rsa-sha2-256"
NEXUS_V2_SSH_PUBLIC_KEY_ALGORITHMS="ssh-ed25519,ecdsa-sha2-nistp256,rsa-sha2-512,rsa-sha2-256"
NEXUS_V2_SSH_KEX_ALGORITHMS="curve25519-sha256,curve25519-sha256@libssh.org,diffie-hellman-group16-sha512,diffie-hellman-group14-sha256"
NEXUS_V2_SSH_CIPHERS="chacha20-poly1305@openssh.com,aes256-gcm@openssh.com,aes128-gcm@openssh.com,aes256-ctr,aes192-ctr,aes128-ctr"
NEXUS_V2_SSH_MACS="hmac-sha2-512-etm@openssh.com,hmac-sha2-256-etm@openssh.com,hmac-sha2-512,hmac-sha2-256"
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

	if [[ "${ETERRA_RELEASE_VERSION:-dev}" != "dev" ]] &&
		! is_truthy "${NEXUS_V2_LOCAL_ONLY_RELEASE:-0}"; then
		local release_branch="release/${ETERRA_RELEASE_VERSION}"
		[[ "$(git -C "${root}" rev-parse --verify "refs/heads/${release_branch}")" == "${actual_commit}" ]] ||
			die "${label} local ${release_branch} is not pinned to ${actual_commit}"
		[[ "$(git -C "${root}" ls-remote origin "refs/heads/${release_branch}" | awk '{print $1}')" == "${actual_commit}" ]] ||
			die "${label} remote ${release_branch} is not pinned to ${actual_commit}"
		[[ -z "$(git -C "${root}" show-ref --verify "refs/tags/${ETERRA_RELEASE_VERSION}" 2>/dev/null || true)" ]] ||
			die "${label} local release tag already exists; deploy and validate before tagging"
		[[ -z "$(git -C "${root}" ls-remote origin "refs/tags/${ETERRA_RELEASE_VERSION}")" ]] ||
			die "${label} remote release tag already exists; deploy and validate before tagging"
	elif [[ "${ETERRA_RELEASE_VERSION:-dev}" != "dev" ]]; then
		log "${label} uses explicit local-only release provenance; no release branch, tag, or remote lookup is required" >&2
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

read_protected_sudo_secret_value() {
	local reference="$1"
	python3 -I -S - "${reference}" <<'PY' ||
import os
import pathlib
import stat
import sys


def reject(message: str) -> None:
    print(f"protected Nexus V2 sudo credential: {message}", file=sys.stderr)
    raise SystemExit(2)


reference = sys.argv[1]
if not reference.startswith("@/"):
    reject("must be an @/absolute/path owner-only file reference")
path = pathlib.Path(reference[1:])
if not path.is_absolute() or ".." in path.parts:
    reject("path must be canonical and absolute")
try:
    resolved = path.resolve(strict=True)
except OSError:
    reject("file is unavailable")
if path != resolved:
    reject("path traverses a symlink")

directory_flags = (
    os.O_RDONLY
    | os.O_DIRECTORY
    | getattr(os, "O_CLOEXEC", 0)
    | getattr(os, "O_NOFOLLOW", 0)
)
descriptor = os.open("/", directory_flags)
try:
    for component in path.parts[1:-1]:
        next_descriptor = os.open(component, directory_flags, dir_fd=descriptor)
        os.close(descriptor)
        descriptor = next_descriptor
    file_flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    secret_descriptor = os.open(path.name, file_flags, dir_fd=descriptor)
    try:
        before = os.fstat(secret_descriptor)
        if not stat.S_ISREG(before.st_mode):
            reject("file is not regular")
        if before.st_nlink != 1:
            reject("file must have exactly one hard link")
        if before.st_uid != os.getuid():
            reject("file must be owned by the current user")
        if stat.S_IMODE(before.st_mode) not in {0o400, 0o600}:
            reject("file mode must be 0600 or 0400")
        raw = bytearray()
        while len(raw) <= 4097:
            chunk = os.read(secret_descriptor, min(4098 - len(raw), 1024))
            if not chunk:
                break
            raw.extend(chunk)
        if len(raw) > 4097 or os.read(secret_descriptor, 1):
            reject("payload exceeds 4096 bytes")
        after = os.fstat(secret_descriptor)
        observed = os.stat(path.name, dir_fd=descriptor, follow_symlinks=False)
        current = os.lstat(path)
        identity = lambda item: (
            item.st_dev,
            item.st_ino,
            item.st_size,
            item.st_mtime_ns,
            item.st_ctime_ns,
            item.st_nlink,
            stat.S_IMODE(item.st_mode),
            item.st_uid,
            item.st_gid,
        )
        if not (identity(before) == identity(after) == identity(observed) == identity(current)):
            reject("file or ancestor changed while read")
    finally:
        os.close(secret_descriptor)
finally:
    os.close(descriptor)

if raw.endswith(b"\n"):
    raw = raw[:-1]
if not raw or len(raw) > 4096 or b"\n" in raw or b"\r" in raw or b"\x00" in raw:
    reject("must contain one bounded nonempty line")
try:
    value = raw.decode("utf-8")
except UnicodeDecodeError:
    reject("must be valid UTF-8")
if not all(character.isprintable() for character in value):
    reject("must contain only printable characters")
sys.stdout.write(value)
PY
		die "protected Nexus V2 sudo credential source is invalid"
}

clear_deployment_secret_exports() {
	export -n "${NEXUS_V2_DEPLOYMENT_SECRET_VARIABLES[@]}"
}

clear_transport_secret_exports() {
	clear_deployment_secret_exports
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
	case "$1" in
		1|true|TRUE|True|yes|YES|Yes|on|ON|On) return 0 ;;
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

is_protected_nexus_v2_transport() {
	[[ "${ETERRA_RELEASE_VERSION:-dev}" == nexus-v2-private-alpha-* ]] ||
		is_truthy "${NEXUS_V2_LOCAL_ONLY_RELEASE:-0}" ||
		is_truthy "${NEXUS_V2_PHASE1_CLOSED:-0}" ||
		[[ "${NEXUS_V2_POST_ACCEPTANCE_REOPEN_BACKEND:-}" == "protected-alpha" ]]
}

verify_nexus_v2_ssh_host_pins() {
	require_cmd python3
	require_cmd shasum
	[[ -z "${SSH_OPTS}" ]] ||
		die "protected Nexus V2 transport rejects SSH_OPTS; use only release-locked host pins"
	[[ "${DEPLOY_HOST}" =~ ^[0-9]{1,3}(\.[0-9]{1,3}){3}$ ]] ||
		die "protected Nexus V2 DEPLOY_HOST must be an exact IPv4 literal"
	[[ "${SSH_PORT}" == "22" ]] || die "protected Nexus V2 SSH port must be 22"
	[[ "${DEPLOY_USER}" =~ ^[A-Za-z_][A-Za-z0-9_-]{0,31}$ ]] ||
		die "protected Nexus V2 SSH user is invalid"
	[[ "${SSH_IDENTITY_FILE}" = /* && -f "${SSH_IDENTITY_FILE}" && ! -L "${SSH_IDENTITY_FILE}" ]] ||
		die "protected Nexus V2 SSH identity must be an absolute regular file"
	[[ "${SSH_IDENTITY_FILE}" =~ ^/[A-Za-z0-9._/+:-]+$ ]] ||
		die "protected Nexus V2 SSH identity path contains OpenSSH expansion or whitespace"
	[[ "${NEXUS_V2_SSH_KNOWN_HOSTS_FILE}" = /* && -f "${NEXUS_V2_SSH_KNOWN_HOSTS_FILE}" && ! -L "${NEXUS_V2_SSH_KNOWN_HOSTS_FILE}" ]] ||
		die "release-locked dedicated SSH known_hosts is unavailable"
	[[ "${NEXUS_V2_SSH_HOST_PIN_MANIFEST}" = /* && -f "${NEXUS_V2_SSH_HOST_PIN_MANIFEST}" && ! -L "${NEXUS_V2_SSH_HOST_PIN_MANIFEST}" ]] ||
		die "release-locked SSH host-pin manifest is unavailable"
	[[ "${NEXUS_V2_SSH_KNOWN_HOSTS_FILE}" =~ ^/[A-Za-z0-9._/+:-]+$ && "${NEXUS_V2_SSH_HOST_PIN_MANIFEST}" =~ ^/[A-Za-z0-9._/+:-]+$ ]] ||
		die "release-locked SSH host-pin paths contain OpenSSH expansion or whitespace"
	[[ "${NEXUS_V2_SSH_KNOWN_HOSTS_SHA256}" =~ ^[0-9a-f]{64}$ ]] ||
		die "release-locked dedicated SSH known_hosts SHA-256 is invalid"
	[[ "${NEXUS_V2_SSH_HOST_PIN_MANIFEST_SHA256}" =~ ^[0-9a-f]{64}$ ]] ||
		die "release-locked SSH host-pin manifest SHA-256 is invalid"
	[[ "$(shasum -a 256 "${NEXUS_V2_SSH_KNOWN_HOSTS_FILE}" | awk '{print $1}')" == "${NEXUS_V2_SSH_KNOWN_HOSTS_SHA256}" ]] ||
		die "release-locked dedicated SSH known_hosts hash mismatch"
	[[ "$(shasum -a 256 "${NEXUS_V2_SSH_HOST_PIN_MANIFEST}" | awk '{print $1}')" == "${NEXUS_V2_SSH_HOST_PIN_MANIFEST_SHA256}" ]] ||
		die "release-locked SSH host-pin manifest hash mismatch"
	[[ -f "${SSH_HOST_PIN_VALIDATOR}" && ! -L "${SSH_HOST_PIN_VALIDATOR}" ]] ||
		die "SSH host-pin validator is unavailable"
	python3 "${SSH_HOST_PIN_VALIDATOR}" verify \
		--known-hosts "${NEXUS_V2_SSH_KNOWN_HOSTS_FILE}" \
		--manifest "${NEXUS_V2_SSH_HOST_PIN_MANIFEST}" >/dev/null ||
		die "release-locked SSH host-pin validation failed"
	python3 - "${NEXUS_V2_SSH_HOST_PIN_MANIFEST}" "${DEPLOY_HOST}" <<'PY' ||
import json
import pathlib
import sys

manifest = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
target = sys.argv[2]
hosts = manifest.get("hosts")
if not isinstance(hosts, list) or not any(
    isinstance(item, dict) and item.get("host") == target and item.get("port") == 22
    for item in hosts
):
    raise SystemExit("deployment host is absent from the pinned manifest")
PY
		die "deployment host is absent from the release-locked SSH host-pin manifest"
}

build_nexus_v2_pinned_ssh_transport() {
	local -a transport_options=(
		-o "Hostname=${DEPLOY_HOST}"
		-o "HostKeyAlias=${DEPLOY_HOST}"
		-o "UserKnownHostsFile=${NEXUS_V2_SSH_KNOWN_HOSTS_FILE}"
		-o "GlobalKnownHostsFile=/dev/null"
		-o "StrictHostKeyChecking=yes"
		-o "UpdateHostKeys=no"
		-o "KnownHostsCommand=none"
		-o "VerifyHostKeyDNS=no"
		-o "CheckHostIP=yes"
		-o "CanonicalizeHostname=no"
		-o "ProxyCommand=none"
		-o "ProxyJump=none"
		-o "HostKeyAlgorithms=${NEXUS_V2_SSH_HOST_KEY_ALGORITHMS}"
		-o "PubkeyAcceptedAlgorithms=${NEXUS_V2_SSH_PUBLIC_KEY_ALGORITHMS}"
		-o "KexAlgorithms=${NEXUS_V2_SSH_KEX_ALGORITHMS}"
		-o "Ciphers=${NEXUS_V2_SSH_CIPHERS}"
		-o "MACs=${NEXUS_V2_SSH_MACS}"
		-o "IdentitiesOnly=yes"
		-o "IdentityAgent=none"
		-o "IdentityFile=${SSH_IDENTITY_FILE}"
		-o "Port=${SSH_PORT}"
		-o "AddressFamily=inet"
		-o "ClearAllForwardings=yes"
		-o "ForwardAgent=no"
		-o "ForwardX11=no"
		-o "PermitLocalCommand=no"
		-o "LocalCommand=none"
		-o "RequestTTY=no"
		-o "BatchMode=yes"
		-o "PasswordAuthentication=no"
		-o "KbdInteractiveAuthentication=no"
		-o "PreferredAuthentications=publickey"
		-o "ConnectionAttempts=1"
		-o "NumberOfPasswordPrompts=0"
	)
	local -a rsync_ssh_command=(ssh -F /dev/null "${transport_options[@]}")
	SSH_CMD=("${rsync_ssh_command[@]}" "${SSH_TARGET}")
	SCP_CMD=(scp -F /dev/null "${transport_options[@]}")
	printf -v RSYNC_RSH '%q ' "${rsync_ssh_command[@]}"
	RSYNC_RSH="${RSYNC_RSH% }"
	NEXUS_V2_SSH_TRANSPORT_CONTRACT_VERSION="nexus-v2-pinned-host-v1"
	export NEXUS_V2_SSH_TRANSPORT_CONTRACT_VERSION
}

load_env() {
	local env_file="${ALPHA_MACMINI2010_ENV_FILE:-${ENV_FILE_DEFAULT}}"

	[[ -f "${env_file}" ]] || die "missing deploy env file: ${env_file} (copy ${ENV_FILE_EXAMPLE} to ${ENV_FILE_DEFAULT})"

	set -a
	# shellcheck disable=SC1090
	source "${env_file}"
	set +a
	# The environment file is intentionally loaded with set -a for legacy public
	# configuration compatibility. Strip every credential export immediately,
	# before defaults, validation helpers, command substitutions, or transports
	# can launch a child process.
	clear_deployment_secret_exports

	DEPLOY_HOST="${DEPLOY_HOST:-}"
	DEPLOY_USER="${DEPLOY_USER:-eterra2010}"
	DEPLOY_PASSWORD="${DEPLOY_PASSWORD:-}"
	REMOTE_SUDO_PASSWORD="${REMOTE_SUDO_PASSWORD:-}"
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
	MEDIA_BIND_HOST="${MEDIA_BIND_HOST:-0.0.0.0}"
	AUTHORITY_PORT="${AUTHORITY_PORT:-8787}"
	AUTHORITY_BIND_HOST="${AUTHORITY_BIND_HOST:-0.0.0.0}"
	IPFS_API_PORT="${IPFS_API_PORT:-5001}"
	IPFS_GATEWAY_PORT="${IPFS_GATEWAY_PORT:-8080}"
	SSH_PORT="${SSH_PORT:-22}"
	SSH_OPTS="${SSH_OPTS:-}"
	NEXUS_V2_SSH_KNOWN_HOSTS_FILE="${NEXUS_V2_SSH_KNOWN_HOSTS_FILE:-}"
	NEXUS_V2_SSH_KNOWN_HOSTS_SHA256="${NEXUS_V2_SSH_KNOWN_HOSTS_SHA256:-}"
	NEXUS_V2_SSH_HOST_PIN_MANIFEST="${NEXUS_V2_SSH_HOST_PIN_MANIFEST:-}"
	NEXUS_V2_SSH_HOST_PIN_MANIFEST_SHA256="${NEXUS_V2_SSH_HOST_PIN_MANIFEST_SHA256:-}"
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
	NEXUS_V2_AUTHORITY_CANDIDATE_PATH="${NEXUS_V2_AUTHORITY_CANDIDATE_PATH:-}"
	NEXUS_V2_AUTHORITY_CANDIDATE_SHA256="${NEXUS_V2_AUTHORITY_CANDIDATE_SHA256:-}"
	ETERRA_LEGENDS_SIGNER_MNEMONIC="${ETERRA_LEGENDS_SIGNER_MNEMONIC:-}"
	ETERRA_LEGENDS_SIGNER_DERIVATION_PASSWORD="${ETERRA_LEGENDS_SIGNER_DERIVATION_PASSWORD:-}"
	ETERRA_LEGENDS_PRIVATE_ALPHA_ACCESS_KEY="${ETERRA_LEGENDS_PRIVATE_ALPHA_ACCESS_KEY:-}"
	ETERRA_LEGENDS_READ_MODEL_URL="${ETERRA_LEGENDS_READ_MODEL_URL:-}"
	ETERRA_LEGENDS_READ_MODEL_ADAPTER_VERSION="${ETERRA_LEGENDS_READ_MODEL_ADAPTER_VERSION:-}"
	ETERRA_LEGENDS_AUTHORITY_EPOCH="${ETERRA_LEGENDS_AUTHORITY_EPOCH:-}"
	ETERRA_LEGENDS_READ_MODEL_TIMEOUT_SECONDS="${ETERRA_LEGENDS_READ_MODEL_TIMEOUT_SECONDS:-10}"
	ETERRA_LEGENDS_OWNER_AUTHORIZATION_TTL_SECONDS="${ETERRA_LEGENDS_OWNER_AUTHORIZATION_TTL_SECONDS:-30}"
	ETERRA_RELEASE_VERSION="${ETERRA_RELEASE_VERSION:-dev}"
	ETERRA_EXPECTED_CHAIN_COMMIT="${ETERRA_EXPECTED_CHAIN_COMMIT:-}"
	ETERRA_EXPECTED_RUNTIME_SOURCE_COMMIT="${ETERRA_EXPECTED_RUNTIME_SOURCE_COMMIT:-${ETERRA_EXPECTED_CHAIN_COMMIT}}"
	ETERRA_EXPECTED_MEDIA_COMMIT="${ETERRA_EXPECTED_MEDIA_COMMIT:-}"
	ETERRA_EXPECTED_SDKGEN_COMMIT="${ETERRA_EXPECTED_SDKGEN_COMMIT:-}"
	ALLOW_DIRTY_DEPLOY="${ALLOW_DIRTY_DEPLOY:-0}"
	NEXUS_V2_LOCAL_ONLY_RELEASE="${NEXUS_V2_LOCAL_ONLY_RELEASE:-0}"
	NEXUS_V2_RESET_READINESS_SHA256="${NEXUS_V2_RESET_READINESS_SHA256:-}"
	NEXUS_V2_NODE_CANDIDATE_SHA256="${NEXUS_V2_NODE_CANDIDATE_SHA256:-}"
	NEXUS_V2_TARGET_IDENTITY_SHA256="${NEXUS_V2_TARGET_IDENTITY_SHA256:-}"
	NEXUS_V2_ALPHA_GENESIS_HASH="${NEXUS_V2_ALPHA_GENESIS_HASH:-}"
	NEXUS_V2_PHASE1_CLOSED="${NEXUS_V2_PHASE1_CLOSED:-0}"
	RPC_BIND_HOST="${RPC_BIND_HOST:-0.0.0.0}"
	RUNTIME_SPEC_VERSION="${RUNTIME_SPEC_VERSION:-106}"
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
	[[ -z "${DEPLOY_PASSWORD}" ]] ||
		die "DEPLOY_PASSWORD is rejected; SSH authentication must be key-only"
	if is_protected_nexus_v2_transport; then
		REMOTE_SUDO_PASSWORD="$(read_protected_sudo_secret_value "${REMOTE_SUDO_PASSWORD}")"
	else
		if [[ -n "${REMOTE_SUDO_PASSWORD}" ]]; then
			REMOTE_SUDO_PASSWORD="$(read_secret_value "${REMOTE_SUDO_PASSWORD}")"
		fi
	fi
	clear_transport_secret_exports
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
	clear_deployment_secret_exports
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
		[[ "${RUNTIME_SPEC_VERSION}" == "106" ]] || die "release deploy requires runtime spec 106"
		[[ "${RUNTIME_CODE_HASH}" =~ ^0x[0-9a-fA-F]{64}$ ]] || die "release deploy requires a verified runtime code hash"
		[[ "${KUBO_IMAGE}" == *@sha256:* ]] || die "release deploy requires KUBO_IMAGE pinned by registry digest"
		[[ -n "${MEDIA_RELEASE_CONTENT_SMOKE_URL}" ]] || die "release deploy requires MEDIA_RELEASE_CONTENT_SMOKE_URL"
	elif is_truthy "${NEXUS_V2_LOCAL_ONLY_RELEASE}"; then
		die "NEXUS_V2_LOCAL_ONLY_RELEASE is valid only for a non-dev private-alpha release"
	fi

	SSH_TARGET="${DEPLOY_USER}@${DEPLOY_HOST}"
	REMOTE_NODE_DIR="${DEPLOY_ROOT}/node/current"
	REMOTE_MEDIA_DIR="${DEPLOY_ROOT}/media/current"
	REMOTE_AUTHORITY_DIR="${DEPLOY_ROOT}/arcade-authority/current"
	REMOTE_AUTHORITY_RELEASES_DIR="${DEPLOY_ROOT}/arcade-authority/releases"
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
	REMOTE_LEGENDS_SIGNER_MNEMONIC_FILE="${REMOTE_SHARED_SECRET_DIR}/nexus-v2-legends-authority.mnemonic"
	REMOTE_LEGENDS_SIGNER_DERIVATION_PASSWORD_FILE="${REMOTE_SHARED_SECRET_DIR}/nexus-v2-legends-authority.derivation-password"
	REMOTE_LEGENDS_PRIVATE_ALPHA_ACCESS_KEY_FILE="${REMOTE_SHARED_SECRET_DIR}/nexus-v2-legends-authority.access-key"
	REMOTE_LEGENDS_RESULT_JOURNAL_PATH="/var/lib/eterra/legends-authority-journal"
	REMOTE_NODE_BIN="${REMOTE_NODE_DIR}/solochain-eterra-node"
	REMOTE_NODE_SPEC="${REMOTE_NODE_DIR}/alpha-raw.json"
	REMOTE_NODE_PLAIN_SPEC="${REMOTE_NODE_DIR}/alpha-plain.json"
	REMOTE_START_SCRIPT="${REMOTE_NODE_DIR}/start-alpha-node.sh"
	REMOTE_MEDIA_PROJECT_NAME="${REMOTE_MEDIA_PROJECT_NAME:-eterra-alpha-media}"
	REMOTE_IPFS_DATA_VOLUME="${REMOTE_MEDIA_PROJECT_NAME}_ipfs_data"
	REMOTE_IPFS_STAGING_VOLUME="${REMOTE_MEDIA_PROJECT_NAME}_ipfs_staging"
	REMOTE_MEDIA_COMPOSE_BASE="${REMOTE_MEDIA_DIR}/docker-compose.yaml"
	REMOTE_MEDIA_COMPOSE_OVERRIDE="${REMOTE_MEDIA_DIR}/docker-compose.macmini2010.yaml"
	REMOTE_MEDIA_COMPOSE_PHASE1="${REMOTE_MEDIA_DIR}/docker-compose.phase1-closed.yaml"
	REMOTE_DOCKER_COMPOSE_NORMAL_CMD="docker compose --project-name '${REMOTE_MEDIA_PROJECT_NAME}' -f '${REMOTE_MEDIA_COMPOSE_BASE}' -f '${REMOTE_MEDIA_COMPOSE_OVERRIDE}' --env-file '${REMOTE_MEDIA_ENV_FILE}'"
	REMOTE_DOCKER_COMPOSE_PHASE1_CMD="docker compose --project-name '${REMOTE_MEDIA_PROJECT_NAME}' -f '${REMOTE_MEDIA_COMPOSE_BASE}' -f '${REMOTE_MEDIA_COMPOSE_OVERRIDE}' -f '${REMOTE_MEDIA_COMPOSE_PHASE1}' --env-file '${REMOTE_MEDIA_ENV_FILE}'"
	REMOTE_DOCKER_COMPOSE_CMD="${REMOTE_DOCKER_COMPOSE_NORMAL_CMD}"
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
	REMOTE_RUNTIME_SOURCE_COMMIT_FILE="${REMOTE_STATE_DIR}/runtime-source-commit.txt"
	REMOTE_NODE_CANDIDATE_SHA256_FILE="${REMOTE_STATE_DIR}/node-candidate.sha256"
	REMOTE_ALPHA_GENESIS_HASH_FILE="${REMOTE_STATE_DIR}/alpha-genesis-hash.txt"
	REMOTE_PHASE1_CLOSED_STATE_FILE="${REMOTE_STATE_DIR}/nexus-v2-phase1-closed-start.json"
	REMOTE_TARGET_IDENTITY_FILE="${REMOTE_STATE_DIR}/eterra-spec106-target-identity.v2.json"
	REMOTE_MEDIA_SOURCE_COMMIT_FILE="${REMOTE_STATE_DIR}/media-source-commit.txt"
	REMOTE_AUTHORITY_SOURCE_COMMIT_FILE="${REMOTE_STATE_DIR}/authority-source-commit.txt"
	REMOTE_AUTHORITY_CANDIDATE_SHA256_FILE="${REMOTE_STATE_DIR}/authority-candidate.sha256"
	REMOTE_AUTHORITY_RELEASE_MANIFEST_SHA256_FILE="${REMOTE_STATE_DIR}/authority-release-manifest.sha256"
	REMOTE_AUTHORITY_DEPLOYMENT_RECEIPT_FILE="${REMOTE_STATE_DIR}/authority-deployment-receipt.json"
	REMOTE_NODE_SERVICE_UNIT_FILE="/etc/systemd/system/${REMOTE_NODE_SERVICE_NAME}.service"
	REMOTE_AUTHORITY_SERVICE_UNIT_FILE="/etc/systemd/system/${AUTHORITY_SERVICE_NAME}.service"
	REMOTE_SCRIPT_DIR="${REMOTE_SCRIPT_DIR:-/tmp/alpha-macmini2010-${DEPLOY_USER}}"
	LEGACY_NODE_SERVICE_NAME="${LEGACY_NODE_SERVICE_NAME:-eterra-node}"
	LEGACY_MEDIA_COMPOSE_BASE="${LEGACY_DEPLOY_ROOT}/media/current/docker-compose.yaml"
	LEGACY_MEDIA_COMPOSE_OVERRIDE="${LEGACY_DEPLOY_ROOT}/media/current/docker-compose.macmini2010.yaml"
	LEGACY_MEDIA_ENV_FILE="${LEGACY_DEPLOY_ROOT}/shared/env/media.env"
	LEGACY_MEDIA_COMPOSE_CMD="docker compose -f '${LEGACY_MEDIA_COMPOSE_BASE}' -f '${LEGACY_MEDIA_COMPOSE_OVERRIDE}' --env-file '${LEGACY_MEDIA_ENV_FILE}'"
	LOCAL_FINALIZED_ALPHA_DIR="${LOCAL_FINALIZED_ALPHA_DIR:-${REPO_ROOT}/chain-specs/finalized/alpha}"

	if is_protected_nexus_v2_transport; then
		verify_nexus_v2_ssh_host_pins
		build_nexus_v2_pinned_ssh_transport
	else
		SSH_CMD=(
			ssh -F /dev/null -o StrictHostKeyChecking=yes -o IdentitiesOnly=yes
			-o IdentityAgent=none -o BatchMode=yes -o PasswordAuthentication=no
			-o KbdInteractiveAuthentication=no -o PreferredAuthentications=publickey
			-o NumberOfPasswordPrompts=0 -o RequestTTY=no
			-i "${SSH_IDENTITY_FILE}" -p "${SSH_PORT}"
		)
		SCP_CMD=(
			scp -F /dev/null -o StrictHostKeyChecking=yes -o IdentitiesOnly=yes
			-o IdentityAgent=none -o BatchMode=yes -o PasswordAuthentication=no
			-o KbdInteractiveAuthentication=no -o PreferredAuthentications=publickey
			-o NumberOfPasswordPrompts=0 -o RequestTTY=no
			-i "${SSH_IDENTITY_FILE}" -P "${SSH_PORT}"
		)
		RSYNC_RSH="ssh -F /dev/null -o StrictHostKeyChecking=yes -o IdentitiesOnly=yes -o IdentityAgent=none -o BatchMode=yes -o PasswordAuthentication=no -o KbdInteractiveAuthentication=no -o PreferredAuthentications=publickey -o NumberOfPasswordPrompts=0 -o RequestTTY=no -i $(printf '%q' "${SSH_IDENTITY_FILE}") -p ${SSH_PORT}"
	fi
	if ! is_protected_nexus_v2_transport && [[ -n "${SSH_OPTS}" ]]; then
		local extra_ssh_opts=()
		# shellcheck disable=SC2206
		extra_ssh_opts=(${SSH_OPTS})
		SSH_CMD+=("${extra_ssh_opts[@]}")
		SCP_CMD+=("${extra_ssh_opts[@]}")
		RSYNC_RSH+=" ${SSH_OPTS}"
	fi
	if ! is_protected_nexus_v2_transport; then
		SSH_CMD+=("${SSH_TARGET}")
	fi
}

protected_remote_root_stream() {
	local script_path="$1"
	local script_sha remote_command root_receiver root_receiver_escaped
	[[ -f "${script_path}" && ! -L "${script_path}" ]] || die "protected root script is unavailable"
	[[ -n "${REMOTE_SUDO_PASSWORD}" && "${REMOTE_SUDO_PASSWORD}" != *$'\n'* && "${REMOTE_SUDO_PASSWORD}" != *$'\r'* ]] ||
		die "protected Nexus V2 sudo credential framing is invalid"
	script_sha="$(shasum -a 256 "${script_path}" | awk '{print $1}')"
	root_receiver='set -euo pipefail
umask 077
expected_sha="$1"
[[ "${expected_sha}" =~ ^[0-9a-f]{64}$ ]]
stage="$(mktemp -d /run/nexus-v2-root-exec.XXXXXX)"
cleanup_root_payload() { rm -rf -- "${stage}"; }
trap cleanup_root_payload EXIT HUP INT TERM
chmod 0700 "${stage}"
payload="${stage}/payload"
python3 -I -S -c "import os, sys; data = sys.stdin.buffer.read(); descriptor = os.open(sys.argv[1], os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, \"O_NOFOLLOW\", 0), 0o500); handle = os.fdopen(descriptor, \"wb\"); handle.write(data); handle.flush(); os.fsync(handle.fileno()); os.fchmod(handle.fileno(), 0o500); handle.close()" "${payload}"
[[ "$(stat -c "%U:%G:%a" "${payload}")" == root:root:500 ]]
exec 9<"${payload}"
read -r actual_sha _ < <(sha256sum /proc/self/fd/9)
[[ "${actual_sha}" == "${expected_sha}" ]]
/bin/bash /proc/self/fd/9'
	printf -v root_receiver_escaped '%q' "${root_receiver}"
	printf -v remote_command '%s' \
		"/usr/bin/sudo -S -k -p '' -- /bin/bash -c ${root_receiver_escaped} nexus-v2-root-receiver '${script_sha}'"
	{
		printf '%s\n' "${REMOTE_SUDO_PASSWORD}"
		cat "${script_path}"
	} | "${SSH_CMD[@]}" "${remote_command}"
}

ssh_to_remote() {
	"${SSH_CMD[@]}" "$@"
}

rsync_with_remote() {
	rsync "$@"
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
	chmod 0600 "${local_script}"

	if [[ "${run_as}" == root ]]; then
		protected_remote_root_stream "${local_script}"
		return
	fi

	remote_script="${REMOTE_SCRIPT_DIR}/$(basename "${local_script}")"
	remote_script_escaped="$(shell_escape "${remote_script}")"

	ssh_to_remote "mkdir -p $(shell_escape "${REMOTE_SCRIPT_DIR}")"
	rsync_to_remote_no_delete "${local_script}" "${remote_script}"

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

stage_fresh_reset_readiness() {
	local input_path="$1"
	local staged_path="$2"
	local summary

	require_cmd jq
	require_cmd python3
	require_cmd shasum
	[[ "${ETERRA_RELEASE_VERSION}" != "dev" ]] ||
		die "fresh-reset readiness is valid only for a non-dev private-alpha release"
	is_truthy "${NEXUS_V2_LOCAL_ONLY_RELEASE}" ||
		die "guarded release reset requires NEXUS_V2_LOCAL_ONLY_RELEASE=1"
	[[ -n "${NEXUS_V2_RESET_READINESS_SHA256}" ]] ||
		die "NEXUS_V2_RESET_READINESS_SHA256 is required for a guarded release reset"
	[[ "${NEXUS_V2_RESET_READINESS_SHA256}" =~ ^[0-9a-f]{64}$ ]] ||
		die "NEXUS_V2_RESET_READINESS_SHA256 must be 64 lowercase hex characters"
	[[ -e "${input_path}" ]] || die "fresh-reset readiness packet not found: ${input_path}"
	[[ ! -L "${input_path}" ]] || die "fresh-reset readiness packet must not be a symlink"
	[[ -f "${input_path}" ]] || die "fresh-reset readiness packet must be a regular file"

	install -m 0400 "${input_path}" "${staged_path}"
	summary="$(
		python3 "${RESET_READINESS_VERIFIER}" \
			--readiness "${staged_path}" \
			--expected-sha256 "${NEXUS_V2_RESET_READINESS_SHA256}"
	)" || die "fresh-reset readiness verification failed"
	FRESH_RESET_READINESS_SHA256="$(jq -r '.sha256' <<<"${summary}")"
	FRESH_RESET_RELEASE_ID="$(jq -r '.releaseId' <<<"${summary}")"
	FRESH_RESET_SOURCE_COMMIT="$(jq -r '.sourceCommit' <<<"${summary}")"
	FRESH_RESET_GATE_BLOCK_NUMBER="$(jq -r '.gateFinalizedBlock.number' <<<"${summary}")"
	FRESH_RESET_GATE_BLOCK_HASH="$(jq -r '.gateFinalizedBlock.hash' <<<"${summary}")"
	[[ -n "${CHAIN_SOURCE_COMMIT:-}" ]] ||
		die "guarded release reset requires the exact current chain source commit"
	[[ "${FRESH_RESET_SOURCE_COMMIT}" == "${CHAIN_SOURCE_COMMIT}" ]] ||
		die "fresh-reset readiness chain source commit does not match the current chain source commit"
	FRESH_RESET_READINESS_STAGED_PATH="${staged_path}"
	export FRESH_RESET_READINESS_SHA256 FRESH_RESET_RELEASE_ID FRESH_RESET_SOURCE_COMMIT
	export FRESH_RESET_GATE_BLOCK_NUMBER FRESH_RESET_GATE_BLOCK_HASH FRESH_RESET_READINESS_STAGED_PATH
}

verify_pre_reset_closure_handoff() {
	local input_path="$1"
	local expected_sha256="$2"
	local max_age_seconds="$3"

	[[ -n "${CHAIN_SOURCE_COMMIT:-}" ]] ||
		die "pre-reset closure verification requires the exact current chain source commit"
	[[ "${expected_sha256}" =~ ^[0-9a-f]{64}$ ]] ||
		die "pre-reset closure handoff SHA-256 must be 64 lowercase hex characters"
	[[ "${max_age_seconds}" =~ ^[0-9]+$ ]] ||
		die "pre-reset closure maximum age must be an unsigned integer"
	python3 "${PRE_RESET_CLOSURE_TOOL}" verify \
		--handoff "${input_path}" \
		--expected-sha256 "${expected_sha256}" \
		--release-id "${ETERRA_RELEASE_VERSION}" \
		--source-commit "${CHAIN_SOURCE_COMMIT}" \
		--max-age-seconds "${max_age_seconds}" >/dev/null ||
		die "pre-reset closure handoff verification failed"
	PRE_RESET_CLOSURE_HANDOFF_SHA256="${expected_sha256}"
	export PRE_RESET_CLOSURE_HANDOFF_SHA256
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
		"$(hash_repo_paths "${REPO_ROOT}" deploy/alpha/macmini2010/start-alpha-node.sh deploy/alpha/macmini2010/nexus-v2-phase1-closed-ingress.sh deploy/alpha/macmini2010/eterra-alpha-node.service)"
}

compute_media_build_hash() {
	hash_repo_paths "${MEDIA_REPO_DIR}" Dockerfile package.json package-lock.json yarn.lock tsconfig.json src
}

compute_media_runtime_hash() {
	local env_file="$1"

	combine_hash_values \
		"$(hash_file "${env_file}")" \
		"$(hash_repo_paths "${MEDIA_REPO_DIR}" docker-compose.yaml docker-compose.macmini2010.yaml docker-compose.phase1-closed.yaml)"
}

ensure_local_artifacts_dir() {
	mkdir -p "${ARTIFACTS_DIR}" "${LOCAL_FINALIZED_ALPHA_DIR}"
}

write_exact_env_file() {
	local path="$1" assignment key value observed
	shift
	local -a keys=()
	[[ ! -e "${path}" && ! -L "${path}" ]] || die "refusing to replace rendered environment: ${path}"
	set -C
	exec 8>"${path}"
	set +C
	chmod 0600 "${path}"
	for assignment in "$@"; do
		[[ "${assignment}" == *=* ]] || die "rendered environment assignment is malformed"
		key="${assignment%%=*}"
		value="${assignment#*=}"
		[[ "${key}" =~ ^[A-Z][A-Z0-9_]*$ ]] || die "rendered environment key is invalid: ${key}"
		for observed in "${keys[@]:-}"; do
			[[ "${observed}" != "${key}" ]] || die "duplicate rendered environment key: ${key}"
		done
		[[ "${value}" != *$'\n'* && "${value}" != *$'\r'* ]] || die "rendered environment value contains a line break: ${key}"
		[[ "${value}" != " "* && "${value}" != *" " ]] || die "rendered environment value has ambiguous outer whitespace: ${key}"
		[[ "${value}" =~ ^[A-Za-z0-9._:/@,+%*=\ -]*$ ]] || die "rendered environment value contains unsafe metacharacters: ${key}"
		keys+=("${key}")
		printf '%s=%s\n' "${key}" "${value}" >&8
	done
	exec 8>&-
	python3 -I -S - "${path}" "${keys[@]}" <<'PY' || die "rendered environment closed-set validation failed"
import pathlib
import re
import sys

path = pathlib.Path(sys.argv[1])
expected = sys.argv[2:]
raw = path.read_bytes()
if b"\r" in raw or b"\0" in raw or not raw.endswith(b"\n"):
    raise SystemExit("environment contains controls or lacks final newline")
values = {}
for line in raw.decode("utf-8").splitlines():
    if "=" not in line:
        raise SystemExit("environment assignment is malformed")
    key, value = line.split("=", 1)
    if key in values or re.fullmatch(r"[A-Z][A-Z0-9_]*", key) is None:
        raise SystemExit("environment key is duplicate or invalid")
    if value != value.strip() or re.fullmatch(r"[A-Za-z0-9._:/@,+%*=\- ]*", value) is None:
        raise SystemExit("environment value contains unsafe metacharacters")
    values[key] = value
if list(values) != expected or set(values) != set(expected):
    raise SystemExit("environment closed key set mismatch")
PY
}

write_node_env() {
	local path="$1"
	write_exact_env_file "${path}" \
		"ETERRA_RELEASE_VERSION=${ETERRA_RELEASE_VERSION}" \
		"ETERRA_SOURCE_COMMIT=${CHAIN_SOURCE_COMMIT:-unknown}" \
		"ETERRA_RUNTIME_SOURCE_COMMIT=${NODE_RUNTIME_SOURCE_COMMIT:-${ETERRA_EXPECTED_RUNTIME_SOURCE_COMMIT:-unknown}}" \
		"ETERRA_ALPHA_GENESIS_HASH=${NODE_ALPHA_GENESIS_HASH:-${NEXUS_V2_ALPHA_GENESIS_HASH:-unknown}}" \
		"NODE_BIN=${REMOTE_NODE_BIN}" \
		"RAW_SPEC=${REMOTE_NODE_SPEC}" \
		"BASE_PATH=${REMOTE_NODE_DATA_DIR}" \
		"CHAIN_RPC_PORT=${CHAIN_RPC_PORT}" \
		"CHAIN_P2P_PORT=${CHAIN_P2P_PORT}" \
		"MINI_LAN_IP=${MINI_LAN_IP}" \
		"RPC_CORS=${ALPHA_RPC_CORS}" \
		"NEXUS_V2_PHASE1_CLOSED=${NEXUS_V2_PHASE1_CLOSED}" \
		"RPC_BIND_HOST=${RPC_BIND_HOST}" \
		"AURA_SURI=${AURA_SURI}" \
		"GRAN_SURI=${GRAN_SURI}"
}

write_media_env() {
	local path="$1"
	local media_node_env="production"
	local chain_ws="ws://host.docker.internal:${CHAIN_RPC_PORT}"
	local ipfs_api="http://ipfs:${IPFS_API_PORT}"
	local ipfs_gateway="${SITE_PUBLIC_ORIGIN}/ipfs"
	if [[ "${ETERRA_RELEASE_VERSION}" == "dev" ]]; then
		media_node_env="development"
	fi
	if is_truthy "${NEXUS_V2_PHASE1_CLOSED}"; then
		[[ "${MEDIA_BIND_HOST}" == "127.0.0.1" ]] || die "Phase-1 media env requires MEDIA_BIND_HOST=127.0.0.1"
		chain_ws="ws://127.0.0.1:${CHAIN_RPC_PORT}"
		ipfs_api="http://127.0.0.1:${IPFS_API_PORT}"
		ipfs_gateway="http://127.0.0.1:${IPFS_GATEWAY_PORT}"
	fi
	write_exact_env_file "${path}" \
		"RELEASE_VERSION=${ETERRA_RELEASE_VERSION}" \
		"SOURCE_COMMIT=${MEDIA_SOURCE_COMMIT:-unknown}" \
		"RUNTIME_SPEC_VERSION=${RUNTIME_SPEC_VERSION}" \
		"RUNTIME_CODE_HASH=${RUNTIME_CODE_HASH}" \
		"DOCKERIZED=1" \
		"NODE_ENV=${media_node_env}" \
		"KUBO_IMAGE=${KUBO_IMAGE}" \
		"CHAIN_WS=${chain_ws}" \
		"MEDIA_SIGNER_SEED=${MEDIA_SIGNER_SEED}" \
		"IPFS_API=${ipfs_api}" \
		"IPFS_GATEWAY=${ipfs_gateway}" \
		"PUBLIC_BASE_URL=${SITE_PUBLIC_ORIGIN}/media-api" \
		"ADMIN_API_KEY=${MEDIA_ADMIN_API_KEY}" \
		"PORT=${MEDIA_PORT}" \
		"BIND_HOST=${MEDIA_BIND_HOST}" \
		"NEXUS_V2_PHASE1_CLOSED=${NEXUS_V2_PHASE1_CLOSED}" \
		"MAX_UPLOAD_BYTES=${MAX_UPLOAD_BYTES}" \
		"PUBLIC_MEDIA_UPLOAD_ENABLED=${PUBLIC_MEDIA_UPLOAD_ENABLED}" \
		"CORS_ALLOWED_ORIGINS=${SITE_PUBLIC_ORIGIN}" \
		"CHAIN_REQUEST_TIMEOUT_MS=${CHAIN_REQUEST_TIMEOUT_MS}" \
		"IPFS_REQUEST_TIMEOUT_MS=${IPFS_REQUEST_TIMEOUT_MS}" \
		"RENDER_TIMEOUT_MS=${RENDER_TIMEOUT_MS}" \
		"RENDER_CONCURRENCY=${RENDER_CONCURRENCY}" \
		"PUBLIC_RATE_LIMIT_MAX=${PUBLIC_RATE_LIMIT_MAX}" \
		"PUBLIC_RATE_LIMIT_WINDOW_MS=${PUBLIC_RATE_LIMIT_WINDOW_MS}" \
		"ADMIN_RATE_LIMIT_MAX=${ADMIN_RATE_LIMIT_MAX}" \
		"ADMIN_RATE_LIMIT_WINDOW_MS=${ADMIN_RATE_LIMIT_WINDOW_MS}" \
		"ALLOW_DEV_ADMIN_RESET=${ALLOW_DEV_ADMIN_RESET}" \
		"IPFS_API_PORT=${IPFS_API_PORT}" \
		"IPFS_GATEWAY_PORT=${IPFS_GATEWAY_PORT}"
}

write_authority_env() {
	local path="$1"
	local -a assignments=(
		"ETERRA_RELEASE_VERSION=${ETERRA_RELEASE_VERSION}"
		"ETERRA_SOURCE_COMMIT=${AUTHORITY_SOURCE_COMMIT:-unknown}"
		"ASPNETCORE_URLS=http://${AUTHORITY_BIND_HOST}:${AUTHORITY_PORT}"
		"AUTHORITY_SUBMITTER_MODE=${AUTHORITY_SUBMITTER_MODE}"
		"ALPHA_RPC_URL=${AUTHORITY_RPC_URL}"
	)
	if [[ -n "${AUTHORITY_CANDIDATE_SHA256:-}" ]]; then
		assignments+=(
			"ETERRA_AUTHORITY_CANDIDATE_SHA256=${AUTHORITY_CANDIDATE_SHA256}"
			"ETERRA_AUTHORITY_RELEASE_MANIFEST_SHA256=${AUTHORITY_RELEASE_MANIFEST_SHA256}"
			"ETERRA_LEGENDS_AUTHORITY_MODE=live_alpha"
			"ETERRA_LEGENDS_SIGNER_MNEMONIC_FILE=${REMOTE_LEGENDS_SIGNER_MNEMONIC_FILE}"
			"ETERRA_LEGENDS_PRIVATE_ALPHA_ACCESS_KEY_FILE=${REMOTE_LEGENDS_PRIVATE_ALPHA_ACCESS_KEY_FILE}"
			"ETERRA_LEGENDS_READ_MODEL_URL=${ETERRA_LEGENDS_READ_MODEL_URL}"
			"ETERRA_LEGENDS_EXPECTED_GENESIS_HASH=${AUTHORITY_CANDIDATE_GENESIS_HASH}"
			"ETERRA_LEGENDS_READ_MODEL_RUNTIME_SPEC_VERSION=106"
			"ETERRA_LEGENDS_READ_MODEL_METADATA_SHA256=${AUTHORITY_CANDIDATE_METADATA_SHA256}"
			"ETERRA_LEGENDS_READ_MODEL_ADAPTER_VERSION=${AUTHORITY_CANDIDATE_ADAPTER_VERSION}"
			"ETERRA_LEGENDS_AUTHORITY_EPOCH=${AUTHORITY_CANDIDATE_EPOCH}"
			"ETERRA_LEGENDS_ENCOUNTER_CATALOG_PATH=${AUTHORITY_REMOTE_RELEASE_ROOT}/api/catalog/eterra-legends.encounters.private-alpha.v1.json"
			"ETERRA_LEGENDS_ENCOUNTER_CATALOG_SHA256=f2846a4ce742f881cce87edd373061d42b720d10a6c324e782c5487060ae7964"
			"ETERRA_LEGENDS_READ_MODEL_TIMEOUT_SECONDS=${ETERRA_LEGENDS_READ_MODEL_TIMEOUT_SECONDS}"
			"ETERRA_LEGENDS_OWNER_AUTHORIZATION_TTL_SECONDS=${ETERRA_LEGENDS_OWNER_AUTHORIZATION_TTL_SECONDS}"
			"ETERRA_LEGENDS_RESULT_JOURNAL_PATH=${REMOTE_LEGENDS_RESULT_JOURNAL_PATH}"
		)
		if [[ -n "${ETERRA_LEGENDS_SIGNER_DERIVATION_PASSWORD}" ]]; then
			assignments+=("ETERRA_LEGENDS_SIGNER_DERIVATION_PASSWORD_FILE=${REMOTE_LEGENDS_SIGNER_DERIVATION_PASSWORD_FILE}")
		fi
	else
		assignments+=(
			"NOVA_RAIL_RELAY_ACCOUNT=${AUTHORITY_RELAY_ACCOUNT}"
			"NOVA_RAIL_RELAY_MNEMONIC_FILE=${REMOTE_AUTHORITY_RELAY_MNEMONIC_FILE}"
			"NOVA_RAIL_RELAY_DERIVATION_PASSWORD_FILE=${REMOTE_AUTHORITY_RELAY_DERIVATION_PASSWORD_FILE}"
			"NOVA_RAIL_RELAY_FINALITY_TIMEOUT_SECONDS=${AUTHORITY_FINALITY_TIMEOUT_SECONDS}"
		)
	fi
	write_exact_env_file "${path}" "${assignments[@]}"
}

render_runtime_env_bundle() {
	local dir="$1"
	write_node_env "${dir}/node.env"
	write_media_env "${dir}/media.env"
	write_authority_env "${dir}/arcade-authority.env"
}
