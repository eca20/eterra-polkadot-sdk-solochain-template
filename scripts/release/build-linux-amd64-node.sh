#!/usr/bin/env bash
set -euo pipefail
umask 077

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
DOCKERFILE="${ROOT_DIR}/scripts/release/Dockerfile.node-linux-amd64"
IMAGE="docker.io/library/rust:1.89-bookworm@sha256:948f9b08a66e7fe01b03a98ef1c7568292e07ec2e4fe90d88c07bb14563c84ff"
SOURCE_COMMIT=""
EXPECTED_RUNTIME_WASM_SHA256=""
OUTPUT_DIR=""

usage() {
	cat <<'EOF'
Usage: build-linux-amd64-node.sh \
  --source-commit 40_HEX \
  --expected-runtime-wasm-sha256 64_HEX \
  --output NEW_DIRECTORY

Builds the immutable private-alpha deployment node for Ubuntu 24.04 x86_64
using the digest-pinned Rust 1.89 Debian bookworm environment. The source must
be an exact clean checkout. Dependencies are locked and no image is published.
EOF
}

while [[ $# -gt 0 ]]; do
	case "$1" in
		--source-commit) SOURCE_COMMIT="${2:?missing source commit}"; shift ;;
		--expected-runtime-wasm-sha256) EXPECTED_RUNTIME_WASM_SHA256="${2:?missing runtime Wasm SHA-256}"; shift ;;
		--output) OUTPUT_DIR="${2:?missing output directory}"; shift ;;
		--help|-h) usage; exit 0 ;;
		*) echo "unknown argument: $1" >&2; usage; exit 2 ;;
	esac
	shift
done

[[ "${SOURCE_COMMIT}" =~ ^[0-9a-f]{40}$ ]] || { echo "invalid --source-commit" >&2; exit 2; }
[[ "${EXPECTED_RUNTIME_WASM_SHA256}" =~ ^[0-9a-f]{64}$ ]] || {
	echo "invalid --expected-runtime-wasm-sha256" >&2
	exit 2
}
[[ -n "${OUTPUT_DIR}" && ! -e "${OUTPUT_DIR}" ]] || {
	echo "--output must name a new directory" >&2
	exit 2
}
command -v docker >/dev/null 2>&1 || { echo "docker is required" >&2; exit 2; }
[[ "$(git -C "${ROOT_DIR}" rev-parse HEAD)" == "${SOURCE_COMMIT}" ]] || {
	echo "source checkout does not match --source-commit" >&2
	exit 2
}
[[ -z "$(git -C "${ROOT_DIR}" status --porcelain --untracked-files=all)" ]] || {
	echo "linux/amd64 node build requires a clean isolated worktree" >&2
	exit 2
}

source_date_epoch="$(git -C "${ROOT_DIR}" show -s --format=%ct "${SOURCE_COMMIT}")"
[[ "${source_date_epoch}" =~ ^[0-9]+$ && "${source_date_epoch}" -gt 0 ]] || {
	echo "invalid source date epoch" >&2
	exit 2
}

temporary_root="$(mktemp -d "${TMPDIR:-/tmp}/nexus-v2-linux-amd64-node.XXXXXX")"
cleanup() {
	case "${temporary_root}" in
		"${TMPDIR:-/tmp}"/nexus-v2-linux-amd64-node.*) rm -rf -- "${temporary_root}" ;;
		*) echo "refusing unsafe temporary cleanup: ${temporary_root}" >&2 ;;
	esac
}
trap cleanup EXIT INT TERM

export DOCKER_BUILDKIT=1
docker buildx build \
	--platform linux/amd64 \
	--file "${DOCKERFILE}" \
	--build-arg "SOURCE_COMMIT=${SOURCE_COMMIT}" \
	--build-arg "SOURCE_DATE_EPOCH=${source_date_epoch}" \
	--metadata-file "${temporary_root}/buildkit-metadata.json" \
	--output "type=local,dest=${temporary_root}/export" \
	"${ROOT_DIR}"

node="${temporary_root}/export/solochain-eterra-node"
wasm="${temporary_root}/export/runtime-spec-106.compact.compressed.wasm"
[[ -x "${node}" && -f "${wasm}" ]] || { echo "linux/amd64 build outputs are incomplete" >&2; exit 2; }
[[ "$(<"${temporary_root}/export/source-commit.txt")" == "${SOURCE_COMMIT}" ]] || {
	echo "container source commit attestation mismatch" >&2
	exit 2
}
[[ "$(<"${temporary_root}/export/source-date-epoch.txt")" == "${source_date_epoch}" ]] || {
	echo "container source date epoch attestation mismatch" >&2
	exit 2
}
[[ "$(<"${temporary_root}/export/rustc-version.txt")" == "rustc 1.89.0 (29483883e 2025-08-04)" ]] || {
	echo "container Rust version attestation mismatch" >&2
	exit 2
}
actual_wasm_sha256="$(shasum -a 256 "${wasm}" | awk '{print $1}')"
[[ "${actual_wasm_sha256}" == "${EXPECTED_RUNTIME_WASM_SHA256}" ]] || {
	echo "linux/amd64 build changed the pinned production runtime Wasm" >&2
	exit 2
}

python3 - "${node}" <<'PY'
import pathlib
import struct
import sys

with pathlib.Path(sys.argv[1]).open("rb") as handle:
    payload = handle.read(64)
if len(payload) < 64 or payload[:7] != b"\x7fELF\x02\x01\x01":
    raise SystemExit("deployment node is not little-endian ELF64")
if struct.unpack_from("<H", payload, 18)[0] != 62:
    raise SystemExit("deployment node is not ELF x86-64")
PY

mkdir -m 0700 "${OUTPUT_DIR}"
install -m 0755 "${node}" "${OUTPUT_DIR}/solochain-eterra-node"
install -m 0600 "${wasm}" "${OUTPUT_DIR}/runtime-spec-106.compact.compressed.wasm"
install -m 0600 "${temporary_root}/buildkit-metadata.json" "${OUTPUT_DIR}/buildkit-metadata.json"

python3 - \
	"${OUTPUT_DIR}/deployment-node-attestation.json" \
	"${SOURCE_COMMIT}" "${source_date_epoch}" "${IMAGE}" \
	"${DOCKERFILE}" "${OUTPUT_DIR}/buildkit-metadata.json" \
	"${OUTPUT_DIR}/solochain-eterra-node" \
	"${OUTPUT_DIR}/runtime-spec-106.compact.compressed.wasm" <<'PY'
import hashlib
import json
import os
import pathlib
import sys

(output, source_commit, source_date_epoch, image, dockerfile, metadata, node, wasm) = sys.argv[1:]
sha = lambda path: hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()
value = {
    "schemaVersion": 1,
    "kind": "nexus-v2-linux-amd64-deployment-node-build",
    "sourceCommit": source_commit,
    "sourceDateEpoch": int(source_date_epoch),
    "targetPlatform": {
        "architecture": "x86_64",
        "binaryFormat": "elf64",
        "deploymentHostContract": "ubuntu-24.04-x86_64",
        "elfMachine": 62,
        "endianness": "little",
        "libc": "glibc",
        "os": "linux",
    },
    "buildEnvironment": {
        "buildkitPlatform": "linux/amd64",
        "cargoLocked": True,
        "containerImage": image,
        "dockerfileSha256": sha(dockerfile),
        "incremental": False,
        "rustc": "rustc 1.89.0 (29483883e 2025-08-04)",
        "runtimeProductionFeature": True,
    },
    "artifacts": {
        "buildkitMetadataSha256": sha(metadata),
        "nativeNodeSha256": sha(node),
        "productionWasmSha256": sha(wasm),
    },
    "authorizations": {
        "paidProduction": False,
        "publicDeploy": False,
        "publicRelease": False,
    },
}
payload = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()
descriptor = os.open(output, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
with os.fdopen(descriptor, "wb") as handle:
    handle.write(payload)
PY

(
	cd "${OUTPUT_DIR}"
	shasum -a 256 \
		solochain-eterra-node \
		runtime-spec-106.compact.compressed.wasm \
		buildkit-metadata.json \
		deployment-node-attestation.json >SHA256SUMS
	shasum -a 256 -c SHA256SUMS
)

echo "Linux/amd64 deployment node ready: ${OUTPUT_DIR}"
