#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

authorize_after=0

while [[ $# -gt 0 ]]; do
	case "$1" in
		--authorize)
			authorize_after=1
			;;
		--help|-h)
			cat <<'EOF'
Usage: deploy-arcade-authority.sh [--authorize]

Builds and deploys the self-hosted Nova Rail authority relay API and operator.
Pass --authorize to run the one-shot operator after the service is deployed.
EOF
			exit 0
			;;
		*)
			die "unknown argument: $1"
			;;
	esac
	shift
done

load_env
require_cmd expect
require_cmd rsync
require_cmd ssh

DOTNET_BIN="${DOTNET_BIN:-/opt/homebrew/bin/dotnet}"
if [[ ! -x "${DOTNET_BIN}" ]]; then
	DOTNET_BIN="$(command -v dotnet || true)"
fi
[[ -n "${DOTNET_BIN}" && -x "${DOTNET_BIN}" ]] || die "dotnet CLI not found; set DOTNET_BIN"
[[ -d "${AUTHORITY_REPO_DIR}" ]] || die "authority SDK repo not found: ${AUTHORITY_REPO_DIR}"

if [[ "${AUTHORITY_SUBMITTER_MODE}" == "live_alpha" ]]; then
	[[ -n "${AUTHORITY_RELAY_ACCOUNT}" ]] || die "AUTHORITY_RELAY_ACCOUNT or NOVA_RAIL_RELAY_ACCOUNT is required for live alpha authority"
	[[ "${AUTHORITY_RELAY_ACCOUNT}" != "replace-with-nova-rail-relay-ss58-account" ]] || die "AUTHORITY_RELAY_ACCOUNT must be replaced with the relay SS58 account"
	[[ -n "${AUTHORITY_RELAY_MNEMONIC}" ]] || die "AUTHORITY_RELAY_MNEMONIC is required for live alpha authority; use @/secure/path for file-backed local env"
fi

bundle_dir="$(make_temp_dir)"
publish_api_dir="${bundle_dir}/api"
publish_operator_dir="${bundle_dir}/operator"
remote_tmp_dir="${DEPLOY_ROOT}/tmp/arcade-authority-deploy"
mkdir -p "${publish_api_dir}" "${publish_operator_dir}" "${bundle_dir}/secrets"
render_runtime_env_bundle "${bundle_dir}"

log "publishing authority API for ${AUTHORITY_RUNTIME_IDENTIFIER}"
"${DOTNET_BIN}" publish \
	"${AUTHORITY_REPO_DIR}/Eterra.Arcade.Authority.Api/Eterra.Arcade.Authority.Api.csproj" \
	-c Release \
	-f net6.0 \
	-r "${AUTHORITY_RUNTIME_IDENTIFIER}" \
	--self-contained "${AUTHORITY_PUBLISH_SELF_CONTAINED}" \
	-o "${publish_api_dir}"

log "publishing authority operator for ${AUTHORITY_RUNTIME_IDENTIFIER}"
"${DOTNET_BIN}" publish \
	"${AUTHORITY_REPO_DIR}/Eterra.Arcade.Authority.Operator/Eterra.Arcade.Authority.Operator.csproj" \
	-c Release \
	-f net6.0 \
	-r "${AUTHORITY_RUNTIME_IDENTIFIER}" \
	--self-contained "${AUTHORITY_PUBLISH_SELF_CONTAINED}" \
	-o "${publish_operator_dir}"

if [[ "${AUTHORITY_SUBMITTER_MODE}" == "live_alpha" ]]; then
	printf '%s\n' "$(read_secret_value "${AUTHORITY_RELAY_MNEMONIC}")" >"${bundle_dir}/secrets/nova-rail-relay.mnemonic"
	chmod 0600 "${bundle_dir}/secrets/nova-rail-relay.mnemonic"
fi
if [[ -n "${AUTHORITY_RELAY_DERIVATION_PASSWORD}" ]]; then
	printf '%s\n' "$(read_secret_value "${AUTHORITY_RELAY_DERIVATION_PASSWORD}")" >"${bundle_dir}/secrets/nova-rail-relay.derivation-password"
	chmod 0600 "${bundle_dir}/secrets/nova-rail-relay.derivation-password"
fi

remote_bash <<EOF
set -euo pipefail
mkdir -p "${remote_tmp_dir}" "${REMOTE_AUTHORITY_API_DIR}" "${REMOTE_AUTHORITY_OPERATOR_DIR}"
EOF

log "syncing authority publish output to ${SSH_TARGET}"
rsync_to_remote "${publish_api_dir}/" "${REMOTE_AUTHORITY_API_DIR}/"
rsync_to_remote "${publish_operator_dir}/" "${REMOTE_AUTHORITY_OPERATOR_DIR}/"
rsync_to_remote_no_delete "${bundle_dir}/arcade-authority.env" "${remote_tmp_dir}/arcade-authority.env"
rsync_to_remote_no_delete "${SCRIPT_DIR}/eterra-arcade-authority.service" "${remote_tmp_dir}/eterra-arcade-authority.service"
if [[ -f "${bundle_dir}/secrets/nova-rail-relay.mnemonic" ]]; then
	rsync_to_remote_no_delete "${bundle_dir}/secrets/nova-rail-relay.mnemonic" "${remote_tmp_dir}/nova-rail-relay.mnemonic"
fi
if [[ -f "${bundle_dir}/secrets/nova-rail-relay.derivation-password" ]]; then
	rsync_to_remote_no_delete "${bundle_dir}/secrets/nova-rail-relay.derivation-password" "${remote_tmp_dir}/nova-rail-relay.derivation-password"
fi

remote_root_bash <<EOF
set -euo pipefail

mkdir -p "${REMOTE_SHARED_ENV_DIR}" "${REMOTE_SHARED_SECRET_DIR}" "${REMOTE_STATE_DIR}" "${REMOTE_AUTHORITY_API_DIR}" "${REMOTE_AUTHORITY_OPERATOR_DIR}"
install -m 0644 "${remote_tmp_dir}/arcade-authority.env" "${REMOTE_AUTHORITY_ENV_FILE}"
chown root:root "${REMOTE_AUTHORITY_ENV_FILE}"

if [[ -f "${remote_tmp_dir}/nova-rail-relay.mnemonic" ]]; then
	install -m 0640 "${remote_tmp_dir}/nova-rail-relay.mnemonic" "${REMOTE_AUTHORITY_RELAY_MNEMONIC_FILE}"
	chown root:"${DEPLOY_USER}" "${REMOTE_AUTHORITY_RELAY_MNEMONIC_FILE}"
fi
if [[ -f "${remote_tmp_dir}/nova-rail-relay.derivation-password" ]]; then
	install -m 0640 "${remote_tmp_dir}/nova-rail-relay.derivation-password" "${REMOTE_AUTHORITY_RELAY_DERIVATION_PASSWORD_FILE}"
	chown root:"${DEPLOY_USER}" "${REMOTE_AUTHORITY_RELAY_DERIVATION_PASSWORD_FILE}"
else
	rm -f "${REMOTE_AUTHORITY_RELAY_DERIVATION_PASSWORD_FILE}"
fi

install -m 0644 "${remote_tmp_dir}/eterra-arcade-authority.service" "${REMOTE_AUTHORITY_SERVICE_UNIT_FILE}"
chown root:root "${REMOTE_AUTHORITY_SERVICE_UNIT_FILE}"
chown -R "${DEPLOY_USER}:${DEPLOY_USER}" "${REMOTE_AUTHORITY_DIR}"
chmod 0755 "${REMOTE_AUTHORITY_API_DIR}/Eterra.Arcade.Authority.Api" "${REMOTE_AUTHORITY_OPERATOR_BIN}"

ufw --force delete allow from "${LAN_CIDR}" to any port "${AUTHORITY_PORT}" proto tcp >/dev/null 2>&1 || true
ufw allow from "${LAN_CIDR}" to any port "${AUTHORITY_PORT}" proto tcp comment 'eterra-alpha-arcade-authority' >/dev/null
ufw --force delete allow from "${LAN_CIDR}" to any port "${CHAIN_RPC_PORT}" proto tcp >/dev/null 2>&1 || true
ufw allow from "${LAN_CIDR}" to any port "${CHAIN_RPC_PORT}" proto tcp comment 'eterra-alpha-chain-rpc-lan-wallet' >/dev/null

systemctl daemon-reload
systemctl enable "${AUTHORITY_SERVICE_NAME}.service"
systemctl restart "${AUTHORITY_SERVICE_NAME}.service"
systemctl is-active --quiet "${AUTHORITY_SERVICE_NAME}.service"
systemctl --no-pager --full status "${AUTHORITY_SERVICE_NAME}.service" || true
rm -rf "${remote_tmp_dir}"
EOF

log "alpha arcade authority deploy complete"

if [[ "${authorize_after}" -eq 1 ]]; then
	"${SCRIPT_DIR}/authorize-arcade-authority.sh"
fi
