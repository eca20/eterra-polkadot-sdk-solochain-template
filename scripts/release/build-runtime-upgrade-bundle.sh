#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
RELEASE_VERSION="v0.1.0-alpha.1"
TARGET_SPEC=104
EXPECTED_CURRENT_SPEC=103
OUTPUT_DIR="${ROOT_DIR}/release-artifacts/${RELEASE_VERSION}/runtime-upgrade"
PREVIOUS_WASM=""
EXPECTED_GENESIS_HASH=""
EXPECTED_CURRENT_CODE_HASH=""
ALLOW_DIRTY=0

usage() {
	cat <<'EOF'
Usage: build-runtime-upgrade-bundle.sh \
  --previous-wasm FILE \
  --expected-genesis-hash 0x... \
  --expected-current-code-hash 0x... \
  [--output DIR] [--allow-dirty]

Builds the spec-104 native node, production compact Wasm, try-runtime Wasm,
and a hash-locked upgrade plan. It does not connect to or modify Alpha.
EOF
}

while [[ $# -gt 0 ]]; do
	case "$1" in
		--previous-wasm) PREVIOUS_WASM="$2"; shift ;;
		--expected-genesis-hash) EXPECTED_GENESIS_HASH="$2"; shift ;;
		--expected-current-code-hash) EXPECTED_CURRENT_CODE_HASH="$2"; shift ;;
		--output) OUTPUT_DIR="$2"; shift ;;
		--allow-dirty) ALLOW_DIRTY=1 ;;
		--help|-h) usage; exit 0 ;;
		*) echo "unknown argument: $1" >&2; usage; exit 2 ;;
	esac
	shift
done

[[ -f "${PREVIOUS_WASM}" ]] || { echo "--previous-wasm must name the recovered live spec-103 Wasm" >&2; exit 2; }
[[ "${EXPECTED_GENESIS_HASH}" =~ ^0x[0-9a-fA-F]{64}$ ]] || { echo "invalid --expected-genesis-hash" >&2; exit 2; }
[[ "${EXPECTED_CURRENT_CODE_HASH}" =~ ^0x[0-9a-fA-F]{64}$ ]] || { echo "invalid --expected-current-code-hash" >&2; exit 2; }
if [[ "${ALLOW_DIRTY}" != "1" && -n "$(git -C "${ROOT_DIR}" status --porcelain)" ]]; then
	echo "runtime release bundle requires a clean chain worktree" >&2
	exit 2
fi

source_commit="$(git -C "${ROOT_DIR}" rev-parse HEAD)"
mkdir -p "${OUTPUT_DIR}"

(
	cd "${ROOT_DIR}"
	cargo build -p solochain-eterra-node --release --features runtime-production
)

production_wasm="${ROOT_DIR}/target/release/wbuild/solochain-eterra-runtime/solochain_eterra_runtime.compact.compressed.wasm"
[[ -f "${production_wasm}" ]] || { echo "production compact Wasm not produced: ${production_wasm}" >&2; exit 2; }
cp "${ROOT_DIR}/target/release/solochain-eterra-node" "${OUTPUT_DIR}/solochain-eterra-node"
cp "${production_wasm}" "${OUTPUT_DIR}/runtime-spec-104.compact.compressed.wasm"
cp "${PREVIOUS_WASM}" "${OUTPUT_DIR}/runtime-spec-103.recovery.wasm"

(
	cd "${ROOT_DIR}"
	cargo build -p solochain-eterra-runtime --release --features try-runtime,runtime-production
)
try_runtime_wasm="${ROOT_DIR}/target/release/wbuild/solochain-eterra-runtime/solochain_eterra_runtime.compact.compressed.wasm"
cp "${try_runtime_wasm}" "${OUTPUT_DIR}/runtime-spec-104.try-runtime.wasm"

target_hash="$(shasum -a 256 "${OUTPUT_DIR}/runtime-spec-104.compact.compressed.wasm" | awk '{print $1}')"
previous_hash="$(shasum -a 256 "${OUTPUT_DIR}/runtime-spec-103.recovery.wasm" | awk '{print $1}')"
node_hash="$(shasum -a 256 "${OUTPUT_DIR}/solochain-eterra-node" | awk '{print $1}')"
try_hash="$(shasum -a 256 "${OUTPUT_DIR}/runtime-spec-104.try-runtime.wasm" | awk '{print $1}')"

jq -n \
	--arg releaseVersion "${RELEASE_VERSION}" \
	--arg sourceCommit "${source_commit}" \
	--arg expectedGenesisHash "${EXPECTED_GENESIS_HASH}" \
	--arg expectedCurrentCodeHash "${EXPECTED_CURRENT_CODE_HASH}" \
	--arg wasmSha256 "${target_hash}" \
	--arg previousWasmSha256 "${previous_hash}" \
	--arg nodeSha256 "${node_hash}" \
	--arg tryRuntimeWasmSha256 "${try_hash}" \
	--argjson expectedCurrentSpecVersion "${EXPECTED_CURRENT_SPEC}" \
	--argjson targetSpecVersion "${TARGET_SPEC}" \
	'{
		schemaVersion: 1,
		releaseVersion: $releaseVersion,
		sourceCommit: $sourceCommit,
		expectedGenesisHash: $expectedGenesisHash,
		expectedCurrentSpecVersion: $expectedCurrentSpecVersion,
		expectedCurrentCodeHash: $expectedCurrentCodeHash,
		targetSpecVersion: $targetSpecVersion,
		wasmPath: "runtime-spec-104.compact.compressed.wasm",
		wasmSha256: $wasmSha256,
		previousWasmPath: "runtime-spec-103.recovery.wasm",
		previousWasmSha256: $previousWasmSha256,
		nativeNodeSha256: $nodeSha256,
		tryRuntimeWasmSha256: $tryRuntimeWasmSha256,
		preUpgradeAssertions: ["specVersion == 103", "genesisHash matches", "codeHash matches", "block production healthy"],
		postUpgradeAssertions: ["specVersion == 104", "target code hash matches", "balances readable", "Gamer storage readable", "ArcadeCore config readable", "faucets callable", "authority sessions healthy"]
	}' >"${OUTPUT_DIR}/runtime-upgrade-plan.json"

(
	cd "${OUTPUT_DIR}"
	shasum -a 256 solochain-eterra-node runtime-spec-*.wasm runtime-upgrade-plan.json >SHA256SUMS
)

echo "runtime upgrade bundle ready: ${OUTPUT_DIR}"
echo "release=${RELEASE_VERSION} source=${source_commit} target_spec=${TARGET_SPEC} wasm_sha256=${target_hash} node_sha256=${node_hash}"
