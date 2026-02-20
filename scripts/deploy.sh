#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
NODE_BIN_DEBUG="${ROOT_DIR}/target/debug/solochain-eterra-node"
NODE_BIN_RELEASE="${ROOT_DIR}/target/release/solochain-eterra-node"

usage() {
  cat <<'USAGE'
Usage:
  scripts/deploy.sh build <default|production> [debug|release]
  scripts/deploy.sh specs <default|production> [out_dir]
  scripts/deploy.sh verify-specs [out_dir]
  scripts/deploy.sh smoke <default|production> [out_dir]
  scripts/deploy.sh pipeline-check <default|production>

Commands:
  build          Build runtime + node for selected runtime mode.
  specs          Generate plain/raw specs for dev, testnet, production.
  verify-specs   Validate generated chain-spec ids, sudo policy, and bootnode defaults.
  smoke          Start local ephemeral validators on each generated raw spec and verify block production.
  pipeline-check Run build + spec generation + spec verification + smoke tests.
USAGE
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "[deploy] missing required command: $1" >&2
    exit 1
  fi
}

mode_feature_args() {
  local mode="$1"
  case "$mode" in
    default)
      echo ""
      ;;
    production)
      echo "--features runtime-production"
      ;;
    *)
      echo "[deploy] invalid mode: ${mode} (expected default|production)" >&2
      exit 1
      ;;
  esac
}

node_bin_for_profile() {
  local profile="$1"
  case "$profile" in
    debug) echo "$NODE_BIN_DEBUG" ;;
    release) echo "$NODE_BIN_RELEASE" ;;
    *)
      echo "[deploy] invalid profile: ${profile} (expected debug|release)" >&2
      exit 1
      ;;
  esac
}

build_cmd() {
  local mode="$1"
  local profile="${2:-debug}"
  local feature_args
  feature_args="$(mode_feature_args "$mode")"

  pushd "$ROOT_DIR" >/dev/null
  if [[ "$profile" == "release" ]]; then
    echo "[deploy] building release binaries (${mode})"
    if [[ -n "$feature_args" ]]; then
      cargo build --release -p solochain-eterra-runtime -p solochain-eterra-node $feature_args
    else
      cargo build --release -p solochain-eterra-runtime -p solochain-eterra-node
    fi
  else
    echo "[deploy] building debug binaries (${mode})"
    if [[ -n "$feature_args" ]]; then
      cargo build -p solochain-eterra-runtime -p solochain-eterra-node $feature_args
    else
      cargo build -p solochain-eterra-runtime -p solochain-eterra-node
    fi
  fi
  popd >/dev/null
}

specs_cmd() {
  local mode="$1"
  local out_dir="${2:-${ROOT_DIR}/chain-specs/generated/${mode}}"
  local node_bin

  # specs mode controls which runtime feature set is embedded in the node binary.
  node_bin="$(node_bin_for_profile debug)"

  if [[ ! -x "$node_bin" ]]; then
    echo "[deploy] node binary not found at ${node_bin}; building debug binaries first"
    build_cmd "$mode" debug
  fi

  mkdir -p "$out_dir"

  echo "[deploy] generating specs into ${out_dir}"
  for chain in dev testnet production; do
    "$node_bin" build-spec --disable-default-bootnode --chain "$chain" > "${out_dir}/${chain}-plain.json"
    "$node_bin" build-spec --disable-default-bootnode --chain "${out_dir}/${chain}-plain.json" --raw > "${out_dir}/${chain}-raw.json"
  done
}

verify_specs_cmd() {
  local out_dir="${1:-${ROOT_DIR}/chain-specs/generated/default}"

  require_cmd python3

  python3 - "$out_dir" <<'PY'
import json
import pathlib
import sys

out_dir = pathlib.Path(sys.argv[1])

expected = {
    "dev": "dev",
    "testnet": "eterra_testnet",
    "production": "eterra_production",
}

for name in expected:
    for kind in ("plain", "raw"):
        p = out_dir / f"{name}-{kind}.json"
        if not p.exists():
            raise SystemExit(f"[deploy] missing spec file: {p}")

for name, expected_id in expected.items():
    plain = json.loads((out_dir / f"{name}-plain.json").read_text())
    raw = json.loads((out_dir / f"{name}-raw.json").read_text())

    if plain.get("id") != expected_id:
        raise SystemExit(f"[deploy] {name}-plain id mismatch: {plain.get('id')} != {expected_id}")
    if raw.get("id") != expected_id:
        raise SystemExit(f"[deploy] {name}-raw id mismatch: {raw.get('id')} != {expected_id}")

    # We always disable default bootnodes when generating specs.
    for label, spec in (("plain", plain), ("raw", raw)):
        for bootnode in spec.get("bootNodes", []):
            if "127.0.0.1" in bootnode or "localhost" in bootnode:
                raise SystemExit(f"[deploy] {name}-{label} has localhost bootnode: {bootnode}")

# Sudo policy checks on plain specs.
patch = lambda name: (
    json.loads((out_dir / f"{name}-plain.json").read_text())
    .get("genesis", {})
    .get("runtimeGenesis", {})
    .get("patch", {})
)

dev_sudo = patch("dev").get("sudo", {}).get("key")
testnet_sudo = patch("testnet").get("sudo", {}).get("key")
production_sudo = patch("production").get("sudo", {}).get("key")

if not isinstance(dev_sudo, str) or not dev_sudo:
    raise SystemExit("[deploy] dev spec missing sudo key")
if not isinstance(testnet_sudo, str) or not testnet_sudo:
    raise SystemExit("[deploy] testnet spec missing sudo key")
if production_sudo is not None:
    raise SystemExit("[deploy] production spec must not include sudo key")

print(f"[deploy] spec verification passed for {out_dir}")
PY
}

wait_for_rpc() {
  local rpc_port="$1"
  local max_seconds="${2:-90}"

  local elapsed=0
  local payload='{"id":1,"jsonrpc":"2.0","method":"state_getRuntimeVersion","params":[]}'

  while (( elapsed < max_seconds )); do
    if curl -sSf -H "Content-Type: application/json" -d "$payload" "http://127.0.0.1:${rpc_port}" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
    elapsed=$((elapsed + 1))
  done

  return 1
}

wait_for_block_height() {
  local rpc_port="$1"
  local min_height="${2:-2}"
  local max_seconds="${3:-90}"

  local elapsed=0
  local payload='{"id":1,"jsonrpc":"2.0","method":"chain_getHeader","params":[]}'

  while (( elapsed < max_seconds )); do
    local resp
    resp="$(curl -sSf -H "Content-Type: application/json" -d "$payload" "http://127.0.0.1:${rpc_port}" 2>/dev/null || true)"
    if [[ -n "$resp" ]]; then
      local height
      height="$(python3 - "$resp" <<'PY'
import json
import sys

try:
    data = json.loads(sys.argv[1])
    num_hex = data.get("result", {}).get("number")
    if isinstance(num_hex, str) and num_hex.startswith("0x"):
        print(int(num_hex, 16))
except Exception:
    pass
PY
)"
      if [[ -n "$height" ]] && (( height >= min_height )); then
        return 0
      fi
    fi
    sleep 1
    elapsed=$((elapsed + 1))
  done

  return 1
}

smoke_cmd() {
  local mode="$1"
  local out_dir="${2:-${ROOT_DIR}/chain-specs/generated/${mode}}"
  local node_bin

  node_bin="$(node_bin_for_profile debug)"
  if [[ ! -x "$node_bin" ]]; then
    echo "[deploy] node binary not found at ${node_bin}; building debug binaries first"
    build_cmd "$mode" debug
  fi

  # Ensure specs exist first.
  if [[ ! -f "${out_dir}/dev-raw.json" || ! -f "${out_dir}/testnet-raw.json" || ! -f "${out_dir}/production-raw.json" ]]; then
    specs_cmd "$mode" "$out_dir"
  fi

  require_cmd curl
  require_cmd python3

  local chains=(dev testnet production)
  local idx=0

  for chain in "${chains[@]}"; do
    local raw_spec="${out_dir}/${chain}-raw.json"
    local rpc_port=$((9944 + idx))
    local p2p_port=$((30333 + idx))
    local log_file="${out_dir}/${chain}-smoke.log"
    local base_path
    local node_key_file
    base_path="$(mktemp -d)"
    node_key_file="${base_path}/secret_ed25519"

    echo "[deploy] smoke test: mode=${mode} chain=${chain} rpc=${rpc_port} p2p=${p2p_port}"

    "$node_bin" key generate-node-key --chain "$raw_spec" --file "$node_key_file" >/dev/null 2>&1
    "$node_bin" key insert \
      --base-path "$base_path" \
      --chain "$raw_spec" \
      --key-type aura \
      --scheme Sr25519 \
      --suri //Alice >/dev/null
    "$node_bin" key insert \
      --base-path "$base_path" \
      --chain "$raw_spec" \
      --key-type gran \
      --scheme Ed25519 \
      --suri //Alice >/dev/null

    "$node_bin" \
      --chain "$raw_spec" \
      --base-path "$base_path" \
      --node-key-file "$node_key_file" \
      --validator \
      --force-authoring \
      --rpc-port "$rpc_port" \
      --port "$p2p_port" \
      --rpc-cors all \
      --unsafe-rpc-external \
      >"$log_file" 2>&1 &

    local node_pid=$!

    if ! wait_for_rpc "$rpc_port" 90; then
      echo "[deploy] rpc not ready for ${chain}; log follows:" >&2
      tail -n 200 "$log_file" >&2 || true
      kill "$node_pid" >/dev/null 2>&1 || true
      wait "$node_pid" 2>/dev/null || true
      rm -rf "$base_path"
      exit 1
    fi

    if ! wait_for_block_height "$rpc_port" 2 90; then
      echo "[deploy] block height check failed for ${chain}; log follows:" >&2
      tail -n 200 "$log_file" >&2 || true
      kill "$node_pid" >/dev/null 2>&1 || true
      wait "$node_pid" 2>/dev/null || true
      rm -rf "$base_path"
      exit 1
    fi

    kill "$node_pid" >/dev/null 2>&1 || true
    wait "$node_pid" 2>/dev/null || true
    rm -rf "$base_path"

    idx=$((idx + 1))
  done

  echo "[deploy] smoke tests passed for mode=${mode}"
}

pipeline_check_cmd() {
  local mode="$1"
  local out_dir="${ROOT_DIR}/chain-specs/generated/${mode}"

  build_cmd "$mode" debug
  specs_cmd "$mode" "$out_dir"
  verify_specs_cmd "$out_dir"
  smoke_cmd "$mode" "$out_dir"
}

main() {
  if [[ $# -lt 1 ]]; then
    usage
    exit 1
  fi

  local cmd="$1"
  shift

  case "$cmd" in
    build)
      [[ $# -ge 1 ]] || { usage; exit 1; }
      build_cmd "$@"
      ;;
    specs)
      [[ $# -ge 1 ]] || { usage; exit 1; }
      specs_cmd "$@"
      ;;
    verify-specs)
      verify_specs_cmd "$@"
      ;;
    smoke)
      [[ $# -ge 1 ]] || { usage; exit 1; }
      smoke_cmd "$@"
      ;;
    pipeline-check)
      [[ $# -ge 1 ]] || { usage; exit 1; }
      pipeline_check_cmd "$1"
      ;;
    -h|--help|help)
      usage
      ;;
    *)
      usage
      exit 1
      ;;
  esac
}

main "$@"
