#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

load_env
require_cmd expect
require_cmd rsync
require_cmd ssh

bundle_dir="$(make_temp_dir)"
mkdir -p "${bundle_dir}/node" "${bundle_dir}/ssh"
render_runtime_env_bundle "${bundle_dir}"
cp "${SCRIPT_DIR}/eterra-alpha-node.service" "${bundle_dir}/eterra-alpha-node.service"
cp "${SCRIPT_DIR}/start-alpha-node.sh" "${bundle_dir}/node/start-alpha-node.sh"
cp "${SSH_PUBLIC_KEY_FILE}" "${bundle_dir}/ssh/deploy-key.pub"

remote_tmp_dir="/tmp/alpha-macmini2010-bootstrap"
log "syncing alpha bootstrap bundle to ${SSH_TARGET}"
remote_bash <<EOF
set -euo pipefail
mkdir -p "${remote_tmp_dir}"
EOF
rsync_to_remote "${bundle_dir}/" "${remote_tmp_dir}/"

log "bootstrapping alpha target ${SSH_TARGET}"
remote_root_bash <<EOF
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive

apt-get update
compose_pkg=""
extra_pkgs=()
if apt-cache show docker-compose-v2 >/dev/null 2>&1; then
	compose_pkg="docker-compose-v2"
elif apt-cache show docker-compose-plugin >/dev/null 2>&1; then
	compose_pkg="docker-compose-plugin"
else
	echo "[bootstrap] unable to find a docker compose plugin package" >&2
	exit 1
fi
if [[ "${ENABLE_REMOTE_SCCACHE}" == "1" ]] && apt-cache show sccache >/dev/null 2>&1; then
	extra_pkgs+=(sccache)
fi
apt-get install -y \
	ca-certificates \
	build-essential \
	clang \
	cmake \
	curl \
	docker.io \
	expect \
	git \
	libssl-dev \
	pkg-config \
	protobuf-compiler \
	rsync \
	ufw \
	"\${compose_pkg}" \
	"\${extra_pkgs[@]}"

systemctl enable --now docker
usermod -aG docker "${DEPLOY_USER}" || true

mkdir -p \
	"${REMOTE_NODE_DIR}" \
	"${REMOTE_MEDIA_DIR}" \
	"${REMOTE_SHARED_ENV_DIR}" \
	"${REMOTE_STATE_DIR}" \
	"${REMOTE_CARGO_TARGET_DIR}" \
	"${REMOTE_SCCACHE_DIR}" \
	"${DEPLOY_ROOT}/tmp" \
	"${REMOTE_NODE_DATA_DIR}" \
	"/home/${DEPLOY_USER}/.ssh"
chown -R "${DEPLOY_USER}:${DEPLOY_USER}" \
	"${DEPLOY_ROOT}" \
	"${REMOTE_NODE_DATA_DIR}" \
	"${REMOTE_CARGO_TARGET_DIR}" \
	"${REMOTE_SCCACHE_DIR}" \
	"/home/${DEPLOY_USER}/.ssh"
chmod 700 "/home/${DEPLOY_USER}/.ssh"
touch "/home/${DEPLOY_USER}/.ssh/authorized_keys"
chmod 600 "/home/${DEPLOY_USER}/.ssh/authorized_keys"
grep -qxF "\$(cat "${remote_tmp_dir}/ssh/deploy-key.pub")" "/home/${DEPLOY_USER}/.ssh/authorized_keys" || \
	cat "${remote_tmp_dir}/ssh/deploy-key.pub" >> "/home/${DEPLOY_USER}/.ssh/authorized_keys"
chown "${DEPLOY_USER}:${DEPLOY_USER}" "/home/${DEPLOY_USER}/.ssh/authorized_keys"

install -m 0755 "${remote_tmp_dir}/node/start-alpha-node.sh" "${REMOTE_START_SCRIPT}"
install -m 0644 "${remote_tmp_dir}/eterra-alpha-node.service" "/etc/systemd/system/${REMOTE_NODE_SERVICE_NAME}.service"
install -m 0644 "${remote_tmp_dir}/node.env" "${REMOTE_NODE_ENV_FILE}"
install -m 0644 "${remote_tmp_dir}/media.env" "${REMOTE_MEDIA_ENV_FILE}"

cat >/etc/ssh/sshd_config.d/90-eterra-alpha.conf <<SSHCONF
PasswordAuthentication no
KbdInteractiveAuthentication no
ChallengeResponseAuthentication no
PubkeyAuthentication yes
PermitRootLogin no
UsePAM yes
SSHCONF
sshd -t
systemctl restart ssh

ufw --force delete allow OpenSSH >/dev/null 2>&1 || true
ufw --force delete allow from "${LAN_CIDR}" to any port "${CHAIN_RPC_PORT}" proto tcp >/dev/null 2>&1 || true
ufw --force delete allow from "${LAN_CIDR}" to any port "${MEDIA_PORT}" proto tcp >/dev/null 2>&1 || true
ufw --force delete allow from "${LAN_CIDR}" to any port "${IPFS_API_PORT}" proto tcp >/dev/null 2>&1 || true
ufw --force delete allow from "${LAN_CIDR}" to any port "${IPFS_GATEWAY_PORT}" proto tcp >/dev/null 2>&1 || true
ufw allow from "${LAN_CIDR}" to any port "${SSH_PORT}" proto tcp comment 'eterra-alpha-ssh' >/dev/null
ufw allow from "${SITE_PROXY_LAN_IP}" to any port "${CHAIN_RPC_PORT}" proto tcp comment 'eterra-alpha-chain-rpc' >/dev/null
ufw allow from 172.16.0.0/12 to any port "${CHAIN_RPC_PORT}" proto tcp comment 'eterra-alpha-docker-chain-rpc' >/dev/null
ufw allow from "${SITE_PROXY_LAN_IP}" to any port "${MEDIA_PORT}" proto tcp comment 'eterra-alpha-media' >/dev/null
ufw allow from "${SITE_PROXY_LAN_IP}" to any port "${IPFS_API_PORT}" proto tcp comment 'eterra-alpha-ipfs-api' >/dev/null
ufw allow from "${SITE_PROXY_LAN_IP}" to any port "${IPFS_GATEWAY_PORT}" proto tcp comment 'eterra-alpha-ipfs-gateway' >/dev/null
ufw --force enable >/dev/null

systemctl daemon-reload
systemctl enable "${REMOTE_NODE_SERVICE_NAME}.service"
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

source "${REMOTE_CARGO_ENV_FILE}"
rustup toolchain install "${REMOTE_RUST_TOOLCHAIN}" --profile minimal
rustup default "${REMOTE_RUST_TOOLCHAIN}"
rustup component add rust-src
rustup target add wasm32-unknown-unknown
if [[ "${ENABLE_REMOTE_SCCACHE}" == "1" ]] && ! command -v sccache >/dev/null 2>&1; then
	cargo install --locked sccache
fi
rustc --version
cargo --version
if command -v sccache >/dev/null 2>&1; then
	sccache --version
fi
EOF

log "alpha bootstrap complete"
log "next: run ${SCRIPT_DIR}/deploy-node.sh and ${SCRIPT_DIR}/deploy-media.sh"
