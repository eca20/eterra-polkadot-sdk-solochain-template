#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
TARGET_SPEC=106
CODE_STORAGE_KEY="0x3a636f6465"
PENDING_REVIEWS="${ROOT_DIR}/docs/nexus-v2-private-alpha/external-reviews.pending.json"
SOURCE_COMMIT=""
LIVE_V14_WASM=""
TCG_VERSION_OBSERVATION=""
TRY_RUNTIME_BIN=""
TRY_RUNTIME_REVISION=""
OUTPUT_DIR=""
RPC_PORT=19945
P2P_PORT=31345

usage() {
	cat <<'EOF'
Usage: build-nexus-v2-alpha-runtime-bundle.sh \
  --source-commit 40_HEX \
  --live-v14-wasm FILE \
  --tcg-version-observation FILE \
  --try-runtime-bin FILE \
  --try-runtime-revision REVISION \
  --output DIR \
  [--rpc-port PORT] \
  [--p2p-port PORT]

Builds a local, unpushed Nexus V2 private-alpha spec-106 release bundle. It
does not connect to Alpha, submit an extrinsic, push a branch, create a public
release, or deploy any service.
EOF
}

while [[ $# -gt 0 ]]; do
	case "$1" in
		--source-commit) SOURCE_COMMIT="${2:?missing source commit}"; shift ;;
		--live-v14-wasm) LIVE_V14_WASM="${2:?missing live V14 Wasm}"; shift ;;
		--tcg-version-observation) TCG_VERSION_OBSERVATION="${2:?missing observation}"; shift ;;
		--try-runtime-bin) TRY_RUNTIME_BIN="${2:?missing try-runtime binary}"; shift ;;
		--try-runtime-revision) TRY_RUNTIME_REVISION="${2:?missing try-runtime revision}"; shift ;;
		--output) OUTPUT_DIR="${2:?missing output directory}"; shift ;;
		--rpc-port) RPC_PORT="${2:?missing RPC port}"; shift ;;
		--p2p-port) P2P_PORT="${2:?missing P2P port}"; shift ;;
		--help|-h) usage; exit 0 ;;
		*) echo "unknown argument: $1" >&2; usage; exit 2 ;;
	esac
	shift
done

[[ "${SOURCE_COMMIT}" =~ ^[0-9a-f]{40}$ ]] || { echo "invalid --source-commit" >&2; exit 2; }
[[ "${TRY_RUNTIME_REVISION}" =~ ^[0-9a-f]{7,40}$ ]] || {
	echo "invalid --try-runtime-revision" >&2
	exit 2
}
[[ -f "${LIVE_V14_WASM}" ]] || { echo "--live-v14-wasm must be a file" >&2; exit 2; }
[[ -f "${TCG_VERSION_OBSERVATION}" ]] || {
	echo "--tcg-version-observation must be a file" >&2
	exit 2
}
[[ -f "${PENDING_REVIEWS}" ]] || { echo "pending external-review record is missing" >&2; exit 2; }
[[ -x "${TRY_RUNTIME_BIN}" && -f "${TRY_RUNTIME_BIN}" ]] || {
	echo "--try-runtime-bin must be an executable regular file" >&2
	exit 2
}
[[ -n "${OUTPUT_DIR}" ]] || { echo "--output is required" >&2; exit 2; }
[[ "${RPC_PORT}" =~ ^[0-9]+$ && "${P2P_PORT}" =~ ^[0-9]+$ ]] || {
	echo "ports must be numeric" >&2
	exit 2
}
[[ "${RPC_PORT}" != "${P2P_PORT}" ]] || { echo "RPC and P2P ports must differ" >&2; exit 2; }
(( RPC_PORT >= 1024 && RPC_PORT <= 65535 && P2P_PORT >= 1024 && P2P_PORT <= 65535 )) || {
	echo "ports must be in 1024..65535" >&2
	exit 2
}

actual_commit="$(git -C "${ROOT_DIR}" rev-parse HEAD)"
[[ "${actual_commit}" == "${SOURCE_COMMIT}" ]] || {
	echo "chain HEAD does not match --source-commit" >&2
	exit 2
}
[[ -z "$(git -C "${ROOT_DIR}" status --porcelain --untracked-files=all)" ]] || {
	echo "runtime bundle requires a clean isolated chain worktree" >&2
	exit 2
}
[[ ! -e "${OUTPUT_DIR}" ]] || {
	echo "refusing to overwrite or merge into existing output: ${OUTPUT_DIR}" >&2
	exit 2
}

observation_version="$(python3 - "${TCG_VERSION_OBSERVATION}" <<'PY'
import json
import sys

value = json.load(open(sys.argv[1], encoding="utf-8"))
raw = (
    value.get("value")
    or value.get("storageValue")
    or value.get("finalizedValue")
    or value.get("readOnlyRpc", {}).get("result")
)
decoded = value.get("decoded", {}).get("storageVersion")
if raw != "0x0e00" or decoded != 14:
    raise SystemExit("TCG observation does not prove SCALE StorageVersion 14 (0x0e00)")
print(14)
PY
)"
[[ "${observation_version}" == "14" ]] || exit 2

mkdir -p "${OUTPUT_DIR}"
OUTPUT_DIR="$(cd -- "${OUTPUT_DIR}" && pwd)"
node_log="${OUTPUT_DIR}/metadata-node.log"
node_pid=""

cleanup() {
	if [[ -n "${node_pid}" ]] && kill -0 "${node_pid}" 2>/dev/null; then
		kill "${node_pid}" 2>/dev/null || true
		wait "${node_pid}" 2>/dev/null || true
	fi
}
trap cleanup EXIT INT TERM

export CARGO_INCREMENTAL=0
export SOURCE_DATE_EPOCH="$(git -C "${ROOT_DIR}" show -s --format=%ct "${SOURCE_COMMIT}")"

(
	cd "${ROOT_DIR}"
	cargo build --locked --release -p solochain-eterra-node --features runtime-production
)

node_binary="${ROOT_DIR}/target/release/solochain-eterra-node"
production_wasm="${ROOT_DIR}/target/release/wbuild/solochain-eterra-runtime/solochain_eterra_runtime.compact.compressed.wasm"
[[ -x "${node_binary}" ]] || { echo "release node was not produced" >&2; exit 2; }
[[ -f "${production_wasm}" ]] || { echo "production compact Wasm was not produced" >&2; exit 2; }

cp "${node_binary}" "${OUTPUT_DIR}/solochain-eterra-node"
cp "${production_wasm}" "${OUTPUT_DIR}/runtime-spec-106.compact.compressed.wasm"
cp "${LIVE_V14_WASM}" "${OUTPUT_DIR}/runtime-spec-live-v14.recovery.wasm"
cp "${TCG_VERSION_OBSERVATION}" "${OUTPUT_DIR}/tcg-storage-version-observation.json"
cp "${TRY_RUNTIME_BIN}" "${OUTPUT_DIR}/try-runtime"
cp "${PENDING_REVIEWS}" "${OUTPUT_DIR}/external-reviews.pending.json"

(
	cd "${ROOT_DIR}"
	cargo build --locked --release -p nexus-v2-migration-verifier
)
migration_verifier="${ROOT_DIR}/target/release/nexus-v2-migration-verifier"
[[ -x "${migration_verifier}" ]] || { echo "migration verifier was not produced" >&2; exit 2; }
cp "${migration_verifier}" "${OUTPUT_DIR}/nexus-v2-migration-verifier"

(
	cd "${ROOT_DIR}"
	cargo build --locked --release -p solochain-eterra-runtime \
		--features try-runtime,runtime-production
)
try_wasm="${ROOT_DIR}/target/release/wbuild/solochain-eterra-runtime/solochain_eterra_runtime.compact.compressed.wasm"
[[ -f "${try_wasm}" ]] || { echo "try-runtime Wasm was not produced" >&2; exit 2; }
cp "${try_wasm}" "${OUTPUT_DIR}/runtime-spec-106.try-runtime.wasm"

"${OUTPUT_DIR}/solochain-eterra-node" build-spec \
	--chain dev \
	--disable-default-bootnode \
	--raw >"${OUTPUT_DIR}/runtime-spec-106.dev-chain-spec.raw.json"

"${OUTPUT_DIR}/solochain-eterra-node" \
	--dev \
	--tmp \
	--rpc-port "${RPC_PORT}" \
	--port "${P2P_PORT}" \
	--no-telemetry \
	--rpc-methods Safe >"${node_log}" 2>&1 &
node_pid="$!"

rpc_uri="http://127.0.0.1:${RPC_PORT}"
rpc_payload='{"id":1,"jsonrpc":"2.0","method":"state_getRuntimeVersion","params":[]}'
ready=0
for _ in $(seq 1 90); do
	if ! kill -0 "${node_pid}" 2>/dev/null; then
		echo "temporary metadata node exited; see ${node_log}" >&2
		exit 2
	fi
	if curl -fsS -H "Content-Type: application/json" -d "${rpc_payload}" "${rpc_uri}" \
		>"${OUTPUT_DIR}/runtime-version.rpc.json" 2>/dev/null; then
		ready=1
		break
	fi
	sleep 1
done
[[ "${ready}" -eq 1 ]] || { echo "temporary metadata RPC did not become ready" >&2; exit 2; }

embedded_code_rpc="${OUTPUT_DIR}/temporary-node-embedded-code.rpc.json"
embedded_code_wasm="${OUTPUT_DIR}/runtime-spec-106.temporary-node-embedded-code.wasm"
embedded_code_payload="{\"id\":2,\"jsonrpc\":\"2.0\",\"method\":\"state_getStorage\",\"params\":[\"${CODE_STORAGE_KEY}\"]}"
curl -fsS -H "Content-Type: application/json" -d "${embedded_code_payload}" "${rpc_uri}" \
	>"${embedded_code_rpc}"
python3 - "${embedded_code_rpc}" "${embedded_code_wasm}" <<'PY'
import json
import sys

response_path, output_path = sys.argv[1:]
with open(response_path, encoding="utf-8") as handle:
    response = json.load(handle)
encoded = response.get("result")
if not isinstance(encoded, str) or not encoded.startswith("0x"):
    raise SystemExit("state_getStorage(:code) did not return a hex result")
payload = encoded[2:]
if not payload or len(payload) % 2:
    raise SystemExit("state_getStorage(:code) returned empty or odd-length hex")
try:
    runtime_code = bytes.fromhex(payload)
except ValueError as error:
    raise SystemExit("state_getStorage(:code) returned invalid hex") from error
with open(output_path, "xb") as handle:
    handle.write(runtime_code)
PY

staged_production_wasm_sha="$(
	shasum -a 256 "${OUTPUT_DIR}/runtime-spec-106.compact.compressed.wasm" | awk '{print $1}'
)"
temporary_node_embedded_wasm_sha="$(
	shasum -a 256 "${embedded_code_wasm}" | awk '{print $1}'
)"
[[ "${temporary_node_embedded_wasm_sha}" == "${staged_production_wasm_sha}" ]] || {
	echo "temporary node embedded :code does not match staged production compact-compressed Wasm" >&2
	exit 2
}

subxt metadata \
	--url "ws://127.0.0.1:${RPC_PORT}" \
	--allow-insecure \
	--format bytes \
	--output-file "${OUTPUT_DIR}/runtime-metadata.scale"
subxt metadata \
	--url "ws://127.0.0.1:${RPC_PORT}" \
	--allow-insecure \
	--format json \
	--output-file "${OUTPUT_DIR}/runtime-metadata.json"

cleanup
node_pid=""

runtime_version="$(jq -r '.result.specVersion' "${OUTPUT_DIR}/runtime-version.rpc.json")"
[[ "${runtime_version}" == "${TARGET_SPEC}" ]] || {
	echo "temporary node reported specVersion ${runtime_version}, expected ${TARGET_SPEC}" >&2
	exit 2
}

node_sha="$(shasum -a 256 "${OUTPUT_DIR}/solochain-eterra-node" | awk '{print $1}')"
migration_verifier_sha="$(shasum -a 256 "${OUTPUT_DIR}/nexus-v2-migration-verifier" | awk '{print $1}')"
try_wasm_sha="$(shasum -a 256 "${OUTPUT_DIR}/runtime-spec-106.try-runtime.wasm" | awk '{print $1}')"
live_sha="$(shasum -a 256 "${OUTPUT_DIR}/runtime-spec-live-v14.recovery.wasm" | awk '{print $1}')"
metadata_sha="$(shasum -a 256 "${OUTPUT_DIR}/runtime-metadata.scale" | awk '{print $1}')"
metadata_json_sha="$(shasum -a 256 "${OUTPUT_DIR}/runtime-metadata.json" | awk '{print $1}')"
try_cli_sha="$(shasum -a 256 "${OUTPUT_DIR}/try-runtime" | awk '{print $1}')"
observation_sha="$(shasum -a 256 "${OUTPUT_DIR}/tcg-storage-version-observation.json" | awk '{print $1}')"
embedded_code_rpc_sha="$(shasum -a 256 "${embedded_code_rpc}" | awk '{print $1}')"
pending_reviews_sha="$(shasum -a 256 "${OUTPUT_DIR}/external-reviews.pending.json" | awk '{print $1}')"

jq -n \
	--arg releaseId "nexus-v2-private-alpha-${SOURCE_COMMIT:0:12}" \
	--arg sourceCommit "${SOURCE_COMMIT}" \
	--argjson sourceDateEpoch "${SOURCE_DATE_EPOCH}" \
	--argjson sourceStorageVersion 14 \
	--argjson targetStorageVersion 16 \
	--argjson targetSpecVersion "${TARGET_SPEC}" \
	--arg nodeSha256 "${node_sha}" \
	--arg migrationVerifierSha256 "${migration_verifier_sha}" \
	--arg codeStorageKey "${CODE_STORAGE_KEY}" \
	--arg stagedProductionWasmSha256 "${staged_production_wasm_sha}" \
	--arg temporaryNodeEmbeddedWasmSha256 "${temporary_node_embedded_wasm_sha}" \
	--arg temporaryNodeEmbeddedCodeRpcSha256 "${embedded_code_rpc_sha}" \
	--arg tryRuntimeWasmSha256 "${try_wasm_sha}" \
	--arg liveV14WasmSha256 "${live_sha}" \
	--arg metadataSha256 "${metadata_sha}" \
	--arg metadataJsonSha256 "${metadata_json_sha}" \
	--arg tryRuntimeCliSha256 "${try_cli_sha}" \
	--arg tryRuntimeRevision "${TRY_RUNTIME_REVISION}" \
	--arg tcgStorageVersionObservationSha256 "${observation_sha}" \
	--arg pendingExternalReviewsSha256 "${pending_reviews_sha}" \
	--arg rustcVersion "$(rustc --version)" \
	--arg cargoVersion "$(cargo --version)" \
		--arg subxtVersion "$(subxt version)" \
	'{
		schemaVersion: 1,
		kind: "nexus-v2-private-alpha-runtime-bundle",
		releaseId: $releaseId,
		sourceCommit: $sourceCommit,
		sourceDateEpoch: $sourceDateEpoch,
		sourceStorageVersion: $sourceStorageVersion,
		targetStorageVersion: $targetStorageVersion,
		targetSpecVersion: $targetSpecVersion,
		runtimeIdentity: {
			codeStorageKey: $codeStorageKey,
			stagedProductionMatchesTemporaryNodeEmbeddedCode: true
		},
		artifacts: {
			nativeNodeSha256: $nodeSha256,
			migrationVerifierSha256: $migrationVerifierSha256,
			stagedProductionWasmSha256: $stagedProductionWasmSha256,
			temporaryNodeEmbeddedWasmSha256: $temporaryNodeEmbeddedWasmSha256,
			temporaryNodeEmbeddedCodeRpcSha256: $temporaryNodeEmbeddedCodeRpcSha256,
			tryRuntimeWasmSha256: $tryRuntimeWasmSha256,
			liveV14WasmSha256: $liveV14WasmSha256,
			metadataScaleSha256: $metadataSha256,
			metadataJsonSha256: $metadataJsonSha256,
			tryRuntimeCliSha256: $tryRuntimeCliSha256,
			tcgStorageVersionObservationSha256: $tcgStorageVersionObservationSha256,
			pendingExternalReviewsSha256: $pendingExternalReviewsSha256
		},
		tools: {
			tryRuntimeRevision: $tryRuntimeRevision,
			rustc: $rustcVersion,
			cargo: $cargoVersion,
			subxt: $subxtVersion
		},
		authorizations: {
			localBuildOnly: true,
			publicRelease: false,
			publicDeploy: false,
			paidProduction: false,
			externalReviewsSelfApproved: false
		}
	}' >"${OUTPUT_DIR}/runtime-bundle-manifest.json"

(
	cd "${OUTPUT_DIR}"
	shasum -a 256 \
		solochain-eterra-node \
		nexus-v2-migration-verifier \
		runtime-spec-106.compact.compressed.wasm \
		runtime-spec-106.try-runtime.wasm \
		runtime-spec-live-v14.recovery.wasm \
		tcg-storage-version-observation.json \
		external-reviews.pending.json \
		try-runtime \
		runtime-spec-106.dev-chain-spec.raw.json \
		runtime-version.rpc.json \
		temporary-node-embedded-code.rpc.json \
		runtime-spec-106.temporary-node-embedded-code.wasm \
		runtime-metadata.scale \
		runtime-metadata.json \
		runtime-bundle-manifest.json >SHA256SUMS
	shasum -a 256 -c SHA256SUMS
)

echo "Nexus V2 private-alpha runtime bundle ready: ${OUTPUT_DIR}"
echo "source=${SOURCE_COMMIT} spec=${TARGET_SPEC} staged_production_wasm_sha256=${staged_production_wasm_sha}"
echo "temporary_node_embedded_wasm_sha256=${temporary_node_embedded_wasm_sha} metadata_sha256=${metadata_sha}"
