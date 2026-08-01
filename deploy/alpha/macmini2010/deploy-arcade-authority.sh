#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
source "${SCRIPT_DIR}/lib.sh"

authorize_after=0
seed_config_after=0
phase1_closed=0
dry_run=0

while [[ $# -gt 0 ]]; do
	case "$1" in
		--authorize)
			authorize_after=1
			;;
		--seed-config)
			seed_config_after=1
			;;
		--phase1-closed)
			phase1_closed=1
			;;
		--dry-run)
			dry_run=1
			;;
		--help|-h)
			cat <<'EOF'
Usage: deploy-arcade-authority.sh [--authorize] [--seed-config] [--phase1-closed] [--dry-run]

Builds and deploys the self-hosted Nova Rail authority relay API and operator.
Pass --authorize to run the one-shot operator after the service is deployed.
Pass --seed-config to idempotently seed the Nova Rail ArcadeCore game config.
--phase1-closed starts the legacy authority on 127.0.0.1 only, precloses
protected firewall rules before restart, and forbids authorization or seeding.
--dry-run is local-only and is intended for the guarded Phase-1 deployment.
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
require_cmd base64
require_cmd rsync
require_cmd shasum
require_cmd ssh

if [[ "${phase1_closed}" -eq 1 ]]; then
	[[ "${authorize_after}" -eq 0 && "${seed_config_after}" -eq 0 ]] ||
		die "--phase1-closed forbids authority authorization and config seeding"
	[[ "${ETERRA_RELEASE_VERSION}" != "dev" ]] ||
		die "--phase1-closed is valid only for a non-dev private-alpha release"
	NEXUS_V2_PHASE1_CLOSED=1
	RPC_BIND_HOST=127.0.0.1
	AUTHORITY_BIND_HOST=127.0.0.1
	AUTHORITY_RPC_URL="ws://127.0.0.1:${CHAIN_RPC_PORT}"
	export NEXUS_V2_PHASE1_CLOSED RPC_BIND_HOST AUTHORITY_BIND_HOST AUTHORITY_RPC_URL
fi

DOTNET_BIN="${DOTNET_BIN:-/opt/homebrew/bin/dotnet}"
if [[ ! -x "${DOTNET_BIN}" ]]; then
	DOTNET_BIN="$(command -v dotnet || true)"
fi
[[ -n "${DOTNET_BIN}" && -x "${DOTNET_BIN}" ]] || die "dotnet CLI not found; set DOTNET_BIN"
[[ -d "${AUTHORITY_REPO_DIR}" ]] || die "authority SDK repo not found: ${AUTHORITY_REPO_DIR}"

require_release_source "${REPO_ROOT}" "alpha deploy tooling" "${ETERRA_EXPECTED_CHAIN_COMMIT}" >/dev/null
AUTHORITY_SOURCE_COMMIT="$(require_release_source "$(cd -- "${AUTHORITY_REPO_DIR}/.." && pwd)" "SDKGen authority" "${ETERRA_EXPECTED_SDKGEN_COMMIT}")"
export AUTHORITY_SOURCE_COMMIT

if [[ "${AUTHORITY_SUBMITTER_MODE}" == "live_alpha" ]]; then
	[[ -n "${AUTHORITY_RELAY_ACCOUNT}" ]] || die "AUTHORITY_RELAY_ACCOUNT or NOVA_RAIL_RELAY_ACCOUNT is required for live alpha authority"
	[[ "${AUTHORITY_RELAY_ACCOUNT}" != "replace-with-nova-rail-relay-ss58-account" ]] || die "AUTHORITY_RELAY_ACCOUNT must be replaced with the relay SS58 account"
	[[ -n "${AUTHORITY_RELAY_MNEMONIC}" ]] || die "AUTHORITY_RELAY_MNEMONIC is required for live alpha authority; use @/secure/path for file-backed local env"
fi

phase1_guard_sha256=""
if [[ "${phase1_closed}" -eq 1 ]]; then
	phase1_guard_sha256="$(shasum -a 256 "${SCRIPT_DIR}/nexus-v2-phase1-closed-ingress.sh" | awk '{print $1}')"
fi

if [[ "${dry_run}" -eq 1 ]]; then
	log "dry-run: authority source and Phase-1 closed-start contract validated; no build, SSH, or live mutation performed"
	log "dry-run: release=${ETERRA_RELEASE_VERSION} authority_source=${AUTHORITY_SOURCE_COMMIT} phase1_closed=${phase1_closed} bind_host=${AUTHORITY_BIND_HOST} phase1_guard_sha256=${phase1_guard_sha256:-none}"
	exit 0
fi

# In Phase-1 this is the first remote operation. It reasserts closure before
# local publish time and requires the node's closed-start marker.
if [[ "${phase1_closed}" -eq 1 ]]; then
	phase1_guard_base64="$(base64 <"${SCRIPT_DIR}/nexus-v2-phase1-closed-ingress.sh" | tr -d '\r\n')"
	remote_root_bash <<EOF
set -euo pipefail
test -f "${REMOTE_PHASE1_CLOSED_STATE_FILE}"
test "\$(jq -r '.nodeRpcLoopbackOnly' "${REMOTE_PHASE1_CLOSED_STATE_FILE}")" = "true"
test "\$(jq -r '.protectedFirewallRulesClosedBeforeNodeStart' "${REMOTE_PHASE1_CLOSED_STATE_FILE}")" = "true"
test "\$(jq -r '.releaseId' "${REMOTE_PHASE1_CLOSED_STATE_FILE}")" = "${ETERRA_RELEASE_VERSION}"
test "\$(jq -r '.sourceCommit' "${REMOTE_PHASE1_CLOSED_STATE_FILE}")" = "${ETERRA_EXPECTED_CHAIN_COMMIT}"
guard="\$(mktemp /tmp/nexus-v2-phase1-closed-ingress.XXXXXX)"
trap 'rm -f "\${guard}"' EXIT
printf '%s' '${phase1_guard_base64}' | base64 -d >"\${guard}"
test "\$(shasum -a 256 "\${guard}" | awk '{print \$1}')" = "${phase1_guard_sha256}"
chmod 0700 "\${guard}"
CHAIN_RPC_PORT="${CHAIN_RPC_PORT}" CHAIN_P2P_PORT="${CHAIN_P2P_PORT}" AUTHORITY_PORT="${AUTHORITY_PORT}" \
	"\${guard}" preclose
EOF
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

authority_artifact_hash="$(
	find "${publish_api_dir}" "${publish_operator_dir}" -type f -print0 |
		LC_ALL=C sort -z |
		xargs -0 shasum -a 256 |
		shasum -a 256 |
		awk '{print $1}'
)"

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
if [[ "${phase1_closed}" -eq 1 ]]; then
	rsync_to_remote_no_delete "${SCRIPT_DIR}/nexus-v2-phase1-closed-ingress.sh" "${remote_tmp_dir}/nexus-v2-phase1-closed-ingress.sh"
fi
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

if [[ "${phase1_closed}" -eq 1 ]]; then
	test "\$(shasum -a 256 "${remote_tmp_dir}/nexus-v2-phase1-closed-ingress.sh" | awk '{print \$1}')" = "${phase1_guard_sha256}"
	chmod 0755 "${remote_tmp_dir}/nexus-v2-phase1-closed-ingress.sh"
	CHAIN_RPC_PORT="${CHAIN_RPC_PORT}" CHAIN_P2P_PORT="${CHAIN_P2P_PORT}" AUTHORITY_PORT="${AUTHORITY_PORT}" \
		"${remote_tmp_dir}/nexus-v2-phase1-closed-ingress.sh" preclose
else
	ufw --force delete allow from "${LAN_CIDR}" to any port "${AUTHORITY_PORT}" proto tcp >/dev/null 2>&1 || true
	ufw allow from "${LAN_CIDR}" to any port "${AUTHORITY_PORT}" proto tcp comment 'eterra-alpha-arcade-authority' >/dev/null
	ufw --force delete allow from "${LAN_CIDR}" to any port "${CHAIN_RPC_PORT}" proto tcp >/dev/null 2>&1 || true
	ufw allow from "${LAN_CIDR}" to any port "${CHAIN_RPC_PORT}" proto tcp comment 'eterra-alpha-chain-rpc-lan-wallet' >/dev/null
fi

systemctl daemon-reload
systemctl enable "${AUTHORITY_SERVICE_NAME}.service"
systemctl restart "${AUTHORITY_SERVICE_NAME}.service"
systemctl is-active --quiet "${AUTHORITY_SERVICE_NAME}.service"
if [[ "${phase1_closed}" -eq 1 ]]; then
	CHAIN_RPC_PORT="${CHAIN_RPC_PORT}" CHAIN_P2P_PORT="${CHAIN_P2P_PORT}" AUTHORITY_PORT="${AUTHORITY_PORT}" \
		"${remote_tmp_dir}/nexus-v2-phase1-closed-ingress.sh" verify-authority
	python3 - "${REMOTE_PHASE1_CLOSED_STATE_FILE}" <<'PY'
import datetime
import json
import os
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
value = json.loads(path.read_text(encoding="utf-8"))
value["legacyAuthorityLoopbackOnly"] = True
value["protectedFirewallRulesClosedBeforeAuthorityStart"] = True
value["authoritySourceCommit"] = "${AUTHORITY_SOURCE_COMMIT}"
value["phase1IngressGuardSha256"] = "${phase1_guard_sha256}"
value["updatedAtUtc"] = datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
temporary = path.with_suffix(".tmp")
temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
os.chmod(temporary, 0o440)
os.replace(temporary, path)
PY
fi
systemctl --no-pager --full status "${AUTHORITY_SERVICE_NAME}.service" || true
printf '%s\n' "${ETERRA_RELEASE_VERSION}" >"${REMOTE_RELEASE_VERSION_FILE}"
printf '%s\n' "${AUTHORITY_SOURCE_COMMIT}" >"${REMOTE_AUTHORITY_SOURCE_COMMIT_FILE}"
printf '%s\n' "${authority_artifact_hash}" >"${REMOTE_AUTHORITY_ARTIFACT_HASH_FILE}"
chown root:root "${REMOTE_RELEASE_VERSION_FILE}" "${REMOTE_AUTHORITY_SOURCE_COMMIT_FILE}" "${REMOTE_AUTHORITY_ARTIFACT_HASH_FILE}"
rm -rf "${remote_tmp_dir}"
EOF

log "alpha arcade authority deploy complete release=${ETERRA_RELEASE_VERSION} source=${AUTHORITY_SOURCE_COMMIT} artifact_sha256=${authority_artifact_hash}"

if [[ "${authorize_after}" -eq 1 ]]; then
	"${SCRIPT_DIR}/authorize-arcade-authority.sh"
fi

if [[ "${seed_config_after}" -eq 1 ]]; then
	"${SCRIPT_DIR}/seed-nova-rail-config.sh"
fi
