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
EXPECTED_SOURCE_COMMIT=""

usage() {
	cat <<'EOF'
Usage: build-runtime-upgrade-bundle.sh \
  --previous-wasm FILE \
  --expected-genesis-hash 0x... \
  --expected-current-code-hash 0x... \
  --expected-source-commit COMMIT \
  [--output DIR]

Builds the spec-104 native node, production compact Wasm, try-runtime Wasm,
and a hash-locked upgrade plan. It does not connect to or modify Alpha.
EOF
}

while [[ $# -gt 0 ]]; do
	case "$1" in
		--previous-wasm) [[ $# -ge 2 ]] || { echo "--previous-wasm requires a file" >&2; exit 2; }; PREVIOUS_WASM="$2"; shift ;;
		--expected-genesis-hash) [[ $# -ge 2 ]] || { echo "--expected-genesis-hash requires a hash" >&2; exit 2; }; EXPECTED_GENESIS_HASH="$2"; shift ;;
		--expected-current-code-hash) [[ $# -ge 2 ]] || { echo "--expected-current-code-hash requires a hash" >&2; exit 2; }; EXPECTED_CURRENT_CODE_HASH="$2"; shift ;;
		--expected-source-commit) [[ $# -ge 2 ]] || { echo "--expected-source-commit requires a commit" >&2; exit 2; }; EXPECTED_SOURCE_COMMIT="$2"; shift ;;
		--output) [[ $# -ge 2 ]] || { echo "--output requires a directory" >&2; exit 2; }; OUTPUT_DIR="$2"; shift ;;
		--help|-h) usage; exit 0 ;;
		*) echo "unknown argument: $1" >&2; usage; exit 2 ;;
	esac
	shift
done

[[ -f "${PREVIOUS_WASM}" ]] || { echo "--previous-wasm must name the recovered live spec-103 Wasm" >&2; exit 2; }
[[ "${EXPECTED_GENESIS_HASH}" =~ ^0x[0-9a-fA-F]{64}$ ]] || { echo "invalid --expected-genesis-hash" >&2; exit 2; }
[[ "${EXPECTED_CURRENT_CODE_HASH}" =~ ^0x[0-9a-fA-F]{64}$ ]] || { echo "invalid --expected-current-code-hash" >&2; exit 2; }
[[ "${EXPECTED_SOURCE_COMMIT}" =~ ^[0-9a-f]{40}$ ]] || { echo "invalid --expected-source-commit" >&2; exit 2; }
if [[ -n "$(git -C "${ROOT_DIR}" status --porcelain --untracked-files=all)" ]]; then
	echo "runtime release bundle requires a clean chain worktree" >&2
	exit 2
fi

source_commit="$(git -C "${ROOT_DIR}" rev-parse HEAD)"
[[ "${source_commit}" == "${EXPECTED_SOURCE_COMMIT}" ]] || { echo "chain HEAD does not match --expected-source-commit" >&2; exit 2; }
release_branch="release/${RELEASE_VERSION}"
[[ "$(git -C "${ROOT_DIR}" rev-parse --verify "refs/heads/${release_branch}")" == "${source_commit}" ]] || {
	echo "local ${release_branch} is not pinned to the release commit" >&2
	exit 2
}
[[ "$(git -C "${ROOT_DIR}" ls-remote origin "refs/heads/${release_branch}" | awk '{print $1}')" == "${source_commit}" ]] || {
	echo "remote ${release_branch} is not pinned to the release commit" >&2
	exit 2
}
[[ -z "$(git -C "${ROOT_DIR}" show-ref --verify "refs/tags/${RELEASE_VERSION}" 2>/dev/null || true)" ]] || {
	echo "local release tag already exists; build and validate the bundle before tagging" >&2
	exit 2
}
[[ -z "$(git -C "${ROOT_DIR}" ls-remote origin "refs/tags/${RELEASE_VERSION}")" ]] || {
	echo "remote release tag already exists; build and validate the bundle before tagging" >&2
	exit 2
}
[[ ! -e "${OUTPUT_DIR}" ]] || { echo "refusing to merge a release bundle into existing output: ${OUTPUT_DIR}" >&2; exit 2; }
mkdir -p "${OUTPUT_DIR}"

export CARGO_INCREMENTAL=0
export SOURCE_DATE_EPOCH="$(git -C "${ROOT_DIR}" show -s --format=%ct "${source_commit}")"

(
	cd "${ROOT_DIR}"
	cargo build --locked -p solochain-eterra-node --release --features runtime-production
)

production_wasm="${ROOT_DIR}/target/release/wbuild/solochain-eterra-runtime/solochain_eterra_runtime.compact.compressed.wasm"
[[ -f "${production_wasm}" ]] || { echo "production compact Wasm not produced: ${production_wasm}" >&2; exit 2; }
cp "${ROOT_DIR}/target/release/solochain-eterra-node" "${OUTPUT_DIR}/solochain-eterra-node"
cp "${production_wasm}" "${OUTPUT_DIR}/runtime-spec-104.compact.compressed.wasm"
cp "${PREVIOUS_WASM}" "${OUTPUT_DIR}/runtime-spec-103.recovery.wasm"

(
	cd "${ROOT_DIR}"
	cargo build --locked -p solochain-eterra-runtime --release --features try-runtime,runtime-production
)
try_runtime_wasm="${ROOT_DIR}/target/release/wbuild/solochain-eterra-runtime/solochain_eterra_runtime.compact.compressed.wasm"
cp "${try_runtime_wasm}" "${OUTPUT_DIR}/runtime-spec-104.try-runtime.wasm"

target_hash="$(shasum -a 256 "${OUTPUT_DIR}/runtime-spec-104.compact.compressed.wasm" | awk '{print $1}')"
previous_hash="$(shasum -a 256 "${OUTPUT_DIR}/runtime-spec-103.recovery.wasm" | awk '{print $1}')"
node_hash="$(shasum -a 256 "${OUTPUT_DIR}/solochain-eterra-node" | awk '{print $1}')"
try_hash="$(shasum -a 256 "${OUTPUT_DIR}/runtime-spec-104.try-runtime.wasm" | awk '{print $1}')"
rustc_version="$(rustc --version)"
cargo_version="$(cargo --version)"

jq -n \
	--arg releaseVersion "${RELEASE_VERSION}" \
	--arg sourceCommit "${source_commit}" \
	--arg expectedGenesisHash "${EXPECTED_GENESIS_HASH}" \
	--arg expectedCurrentCodeHash "${EXPECTED_CURRENT_CODE_HASH}" \
	--arg wasmSha256 "${target_hash}" \
	--arg previousWasmSha256 "${previous_hash}" \
	--arg nodeSha256 "${node_hash}" \
	--arg tryRuntimeWasmSha256 "${try_hash}" \
	--arg rustcVersion "${rustc_version}" \
	--arg cargoVersion "${cargo_version}" \
	--argjson sourceDateEpoch "${SOURCE_DATE_EPOCH}" \
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
		buildToolchain: {
			rustc: $rustcVersion,
			cargo: $cargoVersion,
			sourceDateEpoch: $sourceDateEpoch,
			cargoIncremental: false
		},
		preUpgradeAssertions: ["specVersion == 103", "genesisHash matches", "codeHash matches", "block production healthy"],
		postUpgradeAssertions: ["specVersion == 104", "target code hash matches", "balances readable", "Gamer storage readable", "ArcadeCore config readable", "faucets callable", "authority sessions healthy"]
	}' >"${OUTPUT_DIR}/runtime-upgrade-plan.json"

(
	cd "${OUTPUT_DIR}"
	shasum -a 256 solochain-eterra-node runtime-spec-*.wasm runtime-upgrade-plan.json >SHA256SUMS
)

echo "runtime upgrade bundle ready: ${OUTPUT_DIR}"
echo "release=${RELEASE_VERSION} source=${source_commit} target_spec=${TARGET_SPEC} wasm_sha256=${target_hash} node_sha256=${node_hash}"
