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
  scripts/deploy.sh verify-production <production-plain.json>
  scripts/deploy.sh finalize-production-spec <default|production> <config.json> [out_dir]
  scripts/deploy.sh smoke <default|production> [out_dir]
  scripts/deploy.sh pipeline-check <default|production>

Commands:
  build          Build runtime + node for selected runtime mode.
  specs          Generate plain/raw specs for dev, testnet, production.
  verify-specs   Validate generated chain-spec ids, sudo policy, and bootnode defaults.
  verify-production
                 Strict production plain-spec checks (owner sudo required, no dev placeholders, explicit bootnodes).
  finalize-production-spec
                 Apply production overrides (authorities/bootnodes/balances) and emit strict-validated
                 production plain/raw specs.
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
if not isinstance(production_sudo, str) or not production_sudo:
    raise SystemExit("[deploy] production spec missing sudo key")

print(f"[deploy] spec verification passed for {out_dir}")
PY
}

verify_production_spec_cmd() {
  local spec_path="$1"

  require_cmd python3

  python3 - "$spec_path" <<'PY'
import json
import pathlib
import sys

spec_path = pathlib.Path(sys.argv[1])
if not spec_path.exists():
    raise SystemExit(f"[deploy] missing production spec file: {spec_path}")

spec = json.loads(spec_path.read_text())

if spec.get("id") != "eterra_production":
    raise SystemExit(f"[deploy] production spec id mismatch: {spec.get('id')} != eterra_production")

if spec.get("chainType") != "Live":
    raise SystemExit(f"[deploy] production spec chainType must be Live, got: {spec.get('chainType')}")

bootnodes = spec.get("bootNodes", [])
if not bootnodes:
    raise SystemExit("[deploy] production spec must define at least one bootnode")
for bootnode in bootnodes:
    if "127.0.0.1" in bootnode or "localhost" in bootnode:
        raise SystemExit(f"[deploy] production spec contains localhost bootnode: {bootnode}")

patch = (
    spec.get("genesis", {})
    .get("runtimeGenesis", {})
    .get("patch", {})
)

sudo_key = patch.get("sudo", {}).get("key")
if not isinstance(sudo_key, str) or not sudo_key:
    raise SystemExit("[deploy] production spec must include non-empty sudo key")

aura_authorities = patch.get("aura", {}).get("authorities", [])
grandpa_authorities = patch.get("grandpa", {}).get("authorities", [])
if not aura_authorities:
    raise SystemExit("[deploy] production spec must include at least one Aura authority")
if not grandpa_authorities:
    raise SystemExit("[deploy] production spec must include at least one Grandpa authority")

# Known dev placeholders from chain_spec.rs (Alice/Bob seeds).
dev_aura = {
    "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY",
    "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty",
}
dev_grandpa = {
    "5FA9nQDVg267DEd8m1ZypXLBnvN7SFxYwV7ndqSYGiN9TTpu",
    "5GoNkf6WdbxCFnPdAnYYQyCjAKPJgLNxXwPjwTh6DGg6gN3E",
}

if any(a in dev_aura for a in aura_authorities):
    raise SystemExit("[deploy] production spec still uses dev Aura authorities (Alice/Bob)")

if sudo_key in dev_aura:
    raise SystemExit("[deploy] production sudo key must not use dev account (Alice/Bob)")

for entry in grandpa_authorities:
    if not isinstance(entry, list) or len(entry) < 1:
        raise SystemExit(f"[deploy] invalid Grandpa authority entry: {entry!r}")
    if entry[0] in dev_grandpa:
        raise SystemExit("[deploy] production spec still uses dev Grandpa authorities (Alice/Bob)")

balances = patch.get("balances", {}).get("balances", [])
if not balances:
    raise SystemExit("[deploy] production spec must include non-empty balances allocation")
for entry in balances:
    if isinstance(entry, list) and len(entry) >= 1 and entry[0] in dev_aura:
        raise SystemExit("[deploy] production balances still fund dev accounts (Alice/Bob)")

faucet_account = patch.get("eterraFaucet", {}).get("faucetAccount")
if faucet_account in dev_aura:
    raise SystemExit("[deploy] production faucet account must not use dev account (Alice/Bob)")

print(f"[deploy] strict production validation passed for {spec_path}")
PY
}

finalize_production_spec_cmd() {
  local mode="$1"
  local config_path="$2"
  local out_dir="${3:-${ROOT_DIR}/chain-specs/finalized/${mode}}"
  local source_dir="${ROOT_DIR}/chain-specs/generated/${mode}"
  local source_plain="${source_dir}/production-plain.json"
  local out_plain="${out_dir}/production-plain.json"
  local out_raw="${out_dir}/production-raw.json"
  local node_bin

  node_bin="$(node_bin_for_profile debug)"
  if [[ ! -x "$node_bin" ]]; then
    echo "[deploy] node binary not found at ${node_bin}; building debug binaries first"
    build_cmd "$mode" debug
  fi

  if [[ ! -f "$source_plain" ]]; then
    echo "[deploy] source production plain spec missing at ${source_plain}; generating specs first"
    specs_cmd "$mode" "$source_dir"
  fi

  if [[ ! -f "$config_path" ]]; then
    echo "[deploy] finalize config missing: ${config_path}" >&2
    exit 1
  fi

  mkdir -p "$out_dir"

  python3 - "$source_plain" "$config_path" "$out_plain" <<'PY'
import json
import pathlib
import sys

source_plain = pathlib.Path(sys.argv[1])
config_path = pathlib.Path(sys.argv[2])
out_plain = pathlib.Path(sys.argv[3])

spec = json.loads(source_plain.read_text())
cfg = json.loads(config_path.read_text())

required = ["bootnodes", "aura_authorities", "grandpa_authorities", "balances", "sudo_key"]
missing = [k for k in required if k not in cfg]
if missing:
    raise SystemExit(f"[deploy] finalize config missing required fields: {', '.join(missing)}")

bootnodes = cfg["bootnodes"]
aura = cfg["aura_authorities"]
grandpa = cfg["grandpa_authorities"]
balances = cfg["balances"]

if not isinstance(bootnodes, list) or not bootnodes:
    raise SystemExit("[deploy] bootnodes must be a non-empty array")
if not isinstance(aura, list) or not aura:
    raise SystemExit("[deploy] aura_authorities must be a non-empty array")
if not isinstance(grandpa, list) or not grandpa:
    raise SystemExit("[deploy] grandpa_authorities must be a non-empty array")
if len(aura) != len(grandpa):
    raise SystemExit("[deploy] aura_authorities and grandpa_authorities must have equal lengths")
if not isinstance(balances, list) or not balances:
    raise SystemExit("[deploy] balances must be a non-empty array of [account, amount]")

for bootnode in bootnodes:
    if not isinstance(bootnode, str) or not bootnode:
        raise SystemExit("[deploy] each bootnode must be a non-empty string")
    if "127.0.0.1" in bootnode or "localhost" in bootnode:
        raise SystemExit(f"[deploy] bootnode must not be localhost: {bootnode}")

for auth in aura:
    if not isinstance(auth, str) or not auth:
        raise SystemExit("[deploy] each aura authority must be a non-empty string")

normalized_grandpa = []
for entry in grandpa:
    if not isinstance(entry, list) or len(entry) != 2:
        raise SystemExit("[deploy] each grandpa authority entry must be [address, weight]")
    addr, weight = entry
    if not isinstance(addr, str) or not addr:
        raise SystemExit("[deploy] each grandpa authority address must be a non-empty string")
    try:
        w = int(weight)
    except Exception as exc:
        raise SystemExit(f"[deploy] invalid grandpa weight for {addr}: {weight!r}") from exc
    if w <= 0:
        raise SystemExit(f"[deploy] grandpa weight must be > 0 for {addr}")
    normalized_grandpa.append([addr, w])

normalized_balances = []
for entry in balances:
    if not isinstance(entry, list) or len(entry) != 2:
        raise SystemExit("[deploy] each balances entry must be [account, amount]")
    account, amount = entry
    if not isinstance(account, str) or not account:
        raise SystemExit("[deploy] each balance account must be a non-empty string")
    try:
        a = int(amount)
    except Exception as exc:
        raise SystemExit(f"[deploy] invalid balance amount for {account}: {amount!r}") from exc
    if a <= 0:
        raise SystemExit(f"[deploy] balance amount must be > 0 for {account}")
    normalized_balances.append([account, a])

if spec.get("id") != "eterra_production":
    raise SystemExit(f"[deploy] source spec id must be eterra_production, got {spec.get('id')!r}")

spec["chainType"] = "Live"
if "name" in cfg:
    spec["name"] = cfg["name"]
spec["bootNodes"] = bootnodes

genesis = spec.setdefault("genesis", {})
runtime_genesis = genesis.setdefault("runtimeGenesis", {})
patch = runtime_genesis.setdefault("patch", {})

sudo_key = cfg["sudo_key"]
if not isinstance(sudo_key, str) or not sudo_key:
    raise SystemExit("[deploy] sudo_key must be a non-empty string")
patch.setdefault("sudo", {})["key"] = sudo_key
patch.setdefault("aura", {})["authorities"] = aura
patch.setdefault("grandpa", {})["authorities"] = normalized_grandpa
patch.setdefault("balances", {})["balances"] = normalized_balances

faucet = patch.setdefault("eterraFaucet", {})
faucet_account = cfg.get("faucet_account", normalized_balances[0][0])
if not isinstance(faucet_account, str) or not faucet_account:
    raise SystemExit("[deploy] faucet_account must be a non-empty string")
faucet["faucetAccount"] = faucet_account

if "faucet_payout_amount" in cfg:
    try:
        payout = int(cfg["faucet_payout_amount"])
    except Exception as exc:
        raise SystemExit(f"[deploy] invalid faucet_payout_amount: {cfg['faucet_payout_amount']!r}") from exc
    if payout <= 0:
        raise SystemExit("[deploy] faucet_payout_amount must be > 0")
    faucet["payoutAmount"] = payout

if "initial_servers" in cfg:
    servers = cfg["initial_servers"]
    if not isinstance(servers, list):
        raise SystemExit("[deploy] initial_servers must be an array when provided")
    for srv in servers:
        if not isinstance(srv, str) or not srv:
            raise SystemExit("[deploy] each initial_servers entry must be a non-empty string")
    patch.setdefault("eterraGameAuthority", {})["initialServers"] = servers

out_plain.write_text(json.dumps(spec, indent=2) + "\n")
print(f"[deploy] wrote finalized production plain spec: {out_plain}")
PY

  "$node_bin" build-spec --disable-default-bootnode --chain "$out_plain" --raw > "$out_raw"
  verify_production_spec_cmd "$out_plain"
  echo "[deploy] wrote finalized production raw spec: ${out_raw}"
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
    verify-production)
      [[ $# -eq 1 ]] || { usage; exit 1; }
      verify_production_spec_cmd "$1"
      ;;
    finalize-production-spec)
      [[ $# -ge 2 ]] || { usage; exit 1; }
      finalize_production_spec_cmd "$@"
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
