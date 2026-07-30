#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
RELEASE_VERSION="v0.1.0-alpha.1"
PLAN=""
STATE_RPC=""
SNAPSHOT=""
EVIDENCE=""
EXPECTED_TRY_RUNTIME_REVISION=""
EXPECTED_TRY_RUNTIME_SHA256=""
TRY_RUNTIME_BIN="${TRY_RUNTIME_BIN:-try-runtime}"

usage() {
	cat <<'EOF'
Usage: rehearse-runtime-upgrade.sh \
  --plan FILE \
  (--state-rpc ws://... | --snapshot FILE) \
  --try-runtime-revision REVISION \
  --try-runtime-sha256 SHA256 \
  --evidence OUTPUT.json

Creates or consumes a copied Alpha snapshot and runs the pinned try-runtime
on-runtime-upgrade rehearsal. It never submits a chain extrinsic.
EOF
}

while [[ $# -gt 0 ]]; do
	case "$1" in
		--plan) [[ $# -ge 2 ]] || { echo "--plan requires a file" >&2; exit 2; }; PLAN="$2"; shift ;;
		--state-rpc) [[ $# -ge 2 ]] || { echo "--state-rpc requires a URI" >&2; exit 2; }; STATE_RPC="$2"; shift ;;
		--snapshot) [[ $# -ge 2 ]] || { echo "--snapshot requires a file" >&2; exit 2; }; SNAPSHOT="$2"; shift ;;
		--try-runtime-revision) [[ $# -ge 2 ]] || { echo "--try-runtime-revision requires a revision" >&2; exit 2; }; EXPECTED_TRY_RUNTIME_REVISION="$2"; shift ;;
		--try-runtime-sha256) [[ $# -ge 2 ]] || { echo "--try-runtime-sha256 requires a hash" >&2; exit 2; }; EXPECTED_TRY_RUNTIME_SHA256="$2"; shift ;;
		--evidence) [[ $# -ge 2 ]] || { echo "--evidence requires an output file" >&2; exit 2; }; EVIDENCE="$2"; shift ;;
		--help|-h) usage; exit 0 ;;
		*) echo "unknown argument: $1" >&2; usage; exit 2 ;;
	esac
	shift
done

[[ -f "$PLAN" ]] || { echo "--plan is required" >&2; exit 2; }
[[ -n "$STATE_RPC" || -n "$SNAPSHOT" ]] || { echo "provide --state-rpc or --snapshot" >&2; exit 2; }
[[ -z "$STATE_RPC" || "$STATE_RPC" == ws://* || "$STATE_RPC" == wss://* ]] || { echo "--state-rpc must be ws:// or wss://" >&2; exit 2; }
[[ "$EXPECTED_TRY_RUNTIME_REVISION" =~ ^[0-9a-f]{7,40}$ ]] || { echo "invalid --try-runtime-revision" >&2; exit 2; }
[[ "$EXPECTED_TRY_RUNTIME_SHA256" =~ ^[0-9a-f]{64}$ ]] || { echo "invalid --try-runtime-sha256" >&2; exit 2; }
[[ -n "$EVIDENCE" && ! -e "$EVIDENCE" ]] || { echo "--evidence must name a new output file" >&2; exit 2; }
[[ -z "$(git -C "$ROOT_DIR" status --porcelain --untracked-files=all)" ]] || { echo "rehearsal requires a clean chain worktree" >&2; exit 2; }

command -v "$TRY_RUNTIME_BIN" >/dev/null 2>&1 || { echo "try-runtime CLI is required" >&2; exit 2; }
TRY_RUNTIME_PATH="$(command -v "$TRY_RUNTIME_BIN")"
ACTUAL_TRY_RUNTIME_SHA256="$(shasum -a 256 "$TRY_RUNTIME_PATH" | awk '{print $1}')"
[[ "$ACTUAL_TRY_RUNTIME_SHA256" == "$EXPECTED_TRY_RUNTIME_SHA256" ]] || { echo "try-runtime binary hash mismatch" >&2; exit 2; }
TRY_RUNTIME_VERSION="$("$TRY_RUNTIME_PATH" --version)"

plan_dir="$(cd -- "$(dirname -- "$PLAN")" && pwd)"
runtime="${plan_dir}/$(jq -r '.wasmPath' "$PLAN")"
try_runtime="${plan_dir}/runtime-spec-104.try-runtime.wasm"
previous_runtime="${plan_dir}/$(jq -r '.previousWasmPath' "$PLAN")"
[[ -f "$runtime" && -f "$try_runtime" && -f "$previous_runtime" ]] || { echo "upgrade, try-runtime, or recovery Wasm is missing" >&2; exit 2; }
[[ "$(jq -r '.releaseVersion' "$PLAN")" == "$RELEASE_VERSION" ]] || { echo "release plan version mismatch" >&2; exit 2; }
[[ "$(jq -r '.expectedCurrentSpecVersion' "$PLAN")" == "103" ]] || { echo "release plan must expect spec 103" >&2; exit 2; }
[[ "$(jq -r '.targetSpecVersion' "$PLAN")" == "104" ]] || { echo "release plan must target spec 104" >&2; exit 2; }

source_commit="$(git -C "$ROOT_DIR" rev-parse HEAD)"
[[ "$(jq -r '.sourceCommit' "$PLAN")" == "$source_commit" ]] || { echo "release plan source commit mismatch" >&2; exit 2; }
[[ "$(git -C "$ROOT_DIR" rev-parse --verify "refs/heads/release/$RELEASE_VERSION")" == "$source_commit" ]] || { echo "local release branch is not pinned" >&2; exit 2; }
[[ "$(git -C "$ROOT_DIR" ls-remote origin "refs/heads/release/$RELEASE_VERSION" | awk '{print $1}')" == "$source_commit" ]] || { echo "remote release branch is not pinned" >&2; exit 2; }

check_hash() {
	local file="$1"
	local expected="$2"
	local actual
	actual="$(shasum -a 256 "$file" | awk '{print $1}')"
	[[ "$actual" == "$expected" ]] || { echo "artifact hash mismatch: $file" >&2; exit 2; }
}
check_hash "$runtime" "$(jq -r '.wasmSha256' "$PLAN")"
check_hash "$try_runtime" "$(jq -r '.tryRuntimeWasmSha256' "$PLAN")"
check_hash "$previous_runtime" "$(jq -r '.previousWasmSha256' "$PLAN")"

if [[ -n "$STATE_RPC" ]]; then
	SNAPSHOT="${SNAPSHOT:-${plan_dir}/alpha-state.snap}"
	[[ ! -e "$SNAPSHOT" ]] || { echo "refusing to overwrite copied-state snapshot: $SNAPSHOT" >&2; exit 2; }
	"$TRY_RUNTIME_PATH" --runtime existing create-snapshot --uri "$STATE_RPC" -- "$SNAPSHOT"
fi
[[ -f "$SNAPSHOT" ]] || { echo "copied-state snapshot is missing" >&2; exit 2; }

snapshot_sha256="$(shasum -a 256 "$SNAPSHOT" | awk '{print $1}')"
log_path="${EVIDENCE}.try-runtime.log"
[[ ! -e "$log_path" ]] || { echo "refusing to overwrite try-runtime log" >&2; exit 2; }
"$TRY_RUNTIME_PATH" --runtime "$try_runtime" on-runtime-upgrade snap -p "$SNAPSHOT" 2>&1 | tee "$log_path"
log_sha256="$(shasum -a 256 "$log_path" | awk '{print $1}')"

mkdir -p "$(dirname "$EVIDENCE")"
python3 - "$EVIDENCE" "$RELEASE_VERSION" "$source_commit" "$EXPECTED_TRY_RUNTIME_REVISION" "$EXPECTED_TRY_RUNTIME_SHA256" "$TRY_RUNTIME_VERSION" "$snapshot_sha256" "$log_sha256" "$(shasum -a 256 "$PLAN" | awk '{print $1}')" <<'PY'
import datetime
import json
import pathlib
import sys

(output, release, commit, revision, binary_hash, version,
 snapshot_hash, log_hash, plan_hash) = sys.argv[1:]
evidence = {
    "schemaVersion": 1,
    "releaseVersion": release,
    "sourceCommit": commit,
    "tryRuntimeRevision": revision,
    "tryRuntimeBinarySha256": binary_hash,
    "tryRuntimeVersion": version,
    "snapshotSha256": snapshot_hash,
    "runtimeUpgradePlanSha256": plan_hash,
    "rehearsalLogSha256": log_hash,
    "result": "passed",
    "completedAtUtc": datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).isoformat(),
}
pathlib.Path(output).write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

echo "try-runtime copied-state rehearsal passed snapshot_sha256=$snapshot_sha256"
echo "evidence=$EVIDENCE log_sha256=$log_sha256"
