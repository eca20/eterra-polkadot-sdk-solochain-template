#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

load_env
require_cmd ssh
require_cmd rsync

[[ -n "${LAN_CIDR}" ]] || die "LAN_CIDR must be set in ${MACMINI2010_ENV_FILE:-${ENV_FILE_DEFAULT}} for firewall setup"

bundle_dir="$(make_temp_dir)"
mkdir -p "${bundle_dir}/node"
render_runtime_env_bundle "${bundle_dir}"
cp "${SCRIPT_DIR}/eterra-node.service" "${bundle_dir}/eterra-node.service"
cp "${SCRIPT_DIR}/start-dev-node.sh" "${bundle_dir}/node/start-dev-node.sh"

remote_tmp_dir="${DEPLOY_ROOT}/tmp/bootstrap"
log "syncing bootstrap bundle to ${SSH_TARGET}"
remote_bash <<EOF
set -euo pipefail

mkdir -p "${remote_tmp_dir}"
EOF
rsync_to_remote "${bundle_dir}/" "${remote_tmp_dir}/"

log "bootstrapping ${SSH_TARGET}"
remote_root_bash <<EOF
set -euo pipefail

apt-get update
compose_pkg=""
if apt-cache show docker-compose-v2 >/dev/null 2>&1; then
	compose_pkg="docker-compose-v2"
elif apt-cache show docker-compose-plugin >/dev/null 2>&1; then
	compose_pkg="docker-compose-plugin"
else
	echo "[bootstrap] unable to find a docker compose plugin package" >&2
	exit 1
fi
DEBIAN_FRONTEND=noninteractive apt-get install -y \
	ca-certificates \
	build-essential \
	clang \
	cmake \
	curl \
	docker.io \
	git \
	libssl-dev \
	pkg-config \
	protobuf-compiler \
	rsync \
	ufw \
	"\${compose_pkg}"

systemctl enable --now docker
usermod -aG docker "${DEPLOY_USER}" || true

mkdir -p \
	"${REMOTE_NODE_DIR}" \
	"${REMOTE_MEDIA_DIR}" \
	"${REMOTE_SHARED_ENV_DIR}" \
	"${DEPLOY_ROOT}/tmp" \
	"${REMOTE_NODE_DATA_DIR}"
chown -R "${DEPLOY_USER}:${DEPLOY_USER}" \
	"${DEPLOY_ROOT}/node" \
	"${DEPLOY_ROOT}/media" \
	"${DEPLOY_ROOT}/shared" \
	"${DEPLOY_ROOT}/tmp" \
	"${REMOTE_NODE_DATA_DIR}"

install -m 0755 "${remote_tmp_dir}/node/start-dev-node.sh" "${REMOTE_START_SCRIPT}"
install -m 0644 "${remote_tmp_dir}/eterra-node.service" "/etc/systemd/system/${REMOTE_NODE_SERVICE_NAME}.service"
install -m 0644 "${remote_tmp_dir}/node.env" "${REMOTE_NODE_ENV_FILE}"
install -m 0644 "${remote_tmp_dir}/media.env" "${REMOTE_MEDIA_ENV_FILE}"

systemctl daemon-reload
systemctl enable "${REMOTE_NODE_SERVICE_NAME}.service"

ufw allow OpenSSH >/dev/null
if ! ufw status | grep -Fq "${CHAIN_RPC_PORT}/tcp"; then
	ufw allow from "${LAN_CIDR}" to any port "${CHAIN_RPC_PORT}" proto tcp comment 'eterra-chain-rpc' >/dev/null
fi
if ! ufw status numbered | grep -Fq 'eterra-docker-chain-rpc'; then
	ufw allow from 172.16.0.0/12 to any port "${CHAIN_RPC_PORT}" proto tcp comment 'eterra-docker-chain-rpc' >/dev/null
fi
if ! ufw status | grep -Fq "${MEDIA_PORT}/tcp"; then
	ufw allow from "${LAN_CIDR}" to any port "${MEDIA_PORT}" proto tcp comment 'eterra-media' >/dev/null
fi
if ! ufw status | grep -Fq "${IPFS_API_PORT}/tcp"; then
	ufw allow from "${LAN_CIDR}" to any port "${IPFS_API_PORT}" proto tcp comment 'eterra-ipfs-api' >/dev/null
fi
if ! ufw status | grep -Fq "${IPFS_GATEWAY_PORT}/tcp"; then
	ufw allow from "${LAN_CIDR}" to any port "${IPFS_GATEWAY_PORT}" proto tcp comment 'eterra-ipfs-gateway' >/dev/null
fi
ufw --force enable >/dev/null

rm -rf "${remote_tmp_dir}"
EOF

log "ensuring rust toolchain for ${DEPLOY_USER}"
remote_bash <<EOF
set -euo pipefail

if [[ ! -x "${REMOTE_CARGO_HOME}/bin/rustup" ]]; then
	curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/rustup-init.sh
	sh /tmp/rustup-init.sh -y --profile minimal --default-toolchain "${REMOTE_RUST_TOOLCHAIN}" --target wasm32-unknown-unknown
	rm -f /tmp/rustup-init.sh
fi

# shellcheck disable=SC1090
source "${REMOTE_CARGO_ENV_FILE}"
rustup toolchain install "${REMOTE_RUST_TOOLCHAIN}" --profile minimal
rustup default "${REMOTE_RUST_TOOLCHAIN}"
rustup component add rust-src
rustup target add wasm32-unknown-unknown
rustc --version
cargo --version
EOF

log "bootstrap complete"
log "next: run ${SCRIPT_DIR}/deploy-node.sh and ${SCRIPT_DIR}/deploy-media.sh"
