#!/usr/bin/env bash
set -euo pipefail

PLAN=""
STATE_RPC=""
SNAPSHOT=""
TRY_RUNTIME_BIN="${TRY_RUNTIME_BIN:-try-runtime}"

while [[ $# -gt 0 ]]; do
	case "$1" in
		--plan) PLAN="$2"; shift ;;
		--state-rpc) STATE_RPC="$2"; shift ;;
		--snapshot) SNAPSHOT="$2"; shift ;;
		--help|-h)
			echo "Usage: rehearse-runtime-upgrade.sh --plan FILE (--state-rpc ws://... | --snapshot FILE)"
			exit 0
			;;
		*) echo "unknown argument: $1" >&2; exit 2 ;;
	esac
	shift
done

[[ -f "${PLAN}" ]] || { echo "--plan is required" >&2; exit 2; }
command -v "${TRY_RUNTIME_BIN}" >/dev/null 2>&1 || { echo "try-runtime CLI is required; pin and install paritytech/try-runtime-cli before rehearsal" >&2; exit 2; }
plan_dir="$(cd -- "$(dirname -- "${PLAN}")" && pwd)"
runtime="${plan_dir}/$(jq -r '.wasmPath' "${PLAN}")"
try_runtime="${plan_dir}/runtime-spec-104.try-runtime.wasm"
[[ -f "${runtime}" && -f "${try_runtime}" ]] || { echo "upgrade and try-runtime Wasm files are missing" >&2; exit 2; }
expected="$(jq -r '.wasmSha256' "${PLAN}")"
actual="$(shasum -a 256 "${runtime}" | awk '{print $1}')"
[[ "${actual}" == "${expected}" ]] || { echo "runtime plan hash mismatch" >&2; exit 2; }

if [[ -n "${STATE_RPC}" ]]; then
	SNAPSHOT="${SNAPSHOT:-${plan_dir}/alpha-state.snap}"
	"${TRY_RUNTIME_BIN}" --runtime existing create-snapshot --uri "${STATE_RPC}" -- "${SNAPSHOT}"
fi
[[ -f "${SNAPSHOT}" ]] || { echo "provide --state-rpc or an existing --snapshot" >&2; exit 2; }

"${TRY_RUNTIME_BIN}" --runtime "${try_runtime}" on-runtime-upgrade snap -p "${SNAPSHOT}"
echo "try-runtime rehearsal passed snapshot=${SNAPSHOT} runtime_sha256=${actual}"
