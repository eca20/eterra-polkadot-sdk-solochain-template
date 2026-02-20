#!/usr/bin/env python3
"""
Generate chain-specs/production-overrides.json from validator/owner key material.

This script converts SURI inputs into SS58 addresses using the local node binary.
It keeps secret material out of generated overrides by only writing public addresses.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


def fail(message: str) -> None:
    print(f"[generate-overrides] {message}", file=sys.stderr)
    raise SystemExit(1)


def load_json(path: Path) -> dict[str, Any]:
    try:
        data = json.loads(path.read_text())
    except FileNotFoundError:
        fail(f"input config not found: {path}")
    except json.JSONDecodeError as exc:
        fail(f"invalid JSON in {path}: {exc}")
    if not isinstance(data, dict):
        fail("top-level input JSON must be an object")
    return data


def read_secret(value: str) -> str:
    if not value:
        fail("empty secret value is not allowed")
    if value.startswith("@"):
        secret_path = Path(value[1:]).expanduser()
        try:
            secret = secret_path.read_text().strip()
        except FileNotFoundError:
            fail(f"secret file not found: {secret_path}")
        if not secret:
            fail(f"secret file is empty: {secret_path}")
        return secret
    return value


def inspect_ss58(node_bin: str, secret_or_suri: str, scheme: str) -> str:
    cmd = [
        node_bin,
        "key",
        "inspect",
        "--scheme",
        scheme,
        "--output-type",
        "json",
        secret_or_suri,
    ]
    try:
        output = subprocess.check_output(cmd, text=True, stderr=subprocess.STDOUT)
    except FileNotFoundError:
        fail(f"node binary not found: {node_bin}")
    except subprocess.CalledProcessError as exc:
        fail(f"key inspect failed ({scheme}): {exc.output.strip()}")

    try:
        data = json.loads(output)
    except json.JSONDecodeError as exc:
        fail(f"unexpected key inspect JSON output: {exc}")

    address = data.get("ss58Address")
    if not isinstance(address, str) or not address:
        fail(f"key inspect output missing ss58Address ({scheme})")
    return address


def require_str(obj: dict[str, Any], field: str) -> str:
    value = obj.get(field)
    if not isinstance(value, str) or not value:
        fail(f"missing/invalid string field: {field}")
    return value


def dedup_keep_order(values: list[str]) -> list[str]:
    seen: set[str] = set()
    result: list[str] = []
    for value in values:
        if value not in seen:
            seen.add(value)
            result.append(value)
    return result


def ensure_non_local_bootnode(bootnode: str) -> None:
    if "127.0.0.1" in bootnode or "localhost" in bootnode:
        fail(f"bootnode must not be localhost: {bootnode}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate production overrides JSON from key material."
    )
    parser.add_argument(
        "--in",
        dest="input_path",
        required=True,
        help="Input JSON config (keys + bootnodes).",
    )
    parser.add_argument(
        "--out",
        dest="output_path",
        default="chain-specs/production-overrides.json",
        help="Output overrides JSON path.",
    )
    parser.add_argument(
        "--node-bin",
        default="./target/debug/solochain-eterra-node",
        help="Node binary path used for `key inspect`.",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    input_path = Path(args.input_path)
    output_path = Path(args.output_path)
    cfg = load_json(input_path)

    name = cfg.get("name", "Eterra Production")
    if not isinstance(name, str) or not name:
        fail("field `name` must be a non-empty string when provided")

    validators = cfg.get("validators")
    if not isinstance(validators, list) or not validators:
        fail("field `validators` must be a non-empty array")

    bootnodes: list[str] = []
    aura_authorities: list[str] = []
    grandpa_authorities: list[list[Any]] = []

    for idx, raw in enumerate(validators):
        if not isinstance(raw, dict):
            fail(f"validators[{idx}] must be an object")

        bootnode = require_str(raw, "bootnode")
        ensure_non_local_bootnode(bootnode)
        bootnodes.append(bootnode)

        aura_suri = read_secret(require_str(raw, "aura_suri"))
        grandpa_suri = read_secret(require_str(raw, "grandpa_suri"))

        aura_address = inspect_ss58(args.node_bin, aura_suri, "sr25519")
        grandpa_address = inspect_ss58(args.node_bin, grandpa_suri, "ed25519")
        aura_authorities.append(aura_address)

        weight = raw.get("grandpa_weight", 1)
        if not isinstance(weight, int) or weight <= 0:
            fail(f"validators[{idx}].grandpa_weight must be a positive integer")
        grandpa_authorities.append([grandpa_address, weight])

    sudo_suri = read_secret(require_str(cfg, "sudo_suri"))
    sudo_key = inspect_ss58(args.node_bin, sudo_suri, "sr25519")

    faucet_suri_raw = cfg.get("faucet_suri")
    if faucet_suri_raw is None:
        faucet_account = sudo_key
    else:
        if not isinstance(faucet_suri_raw, str) or not faucet_suri_raw:
            fail("field `faucet_suri` must be a non-empty string when provided")
        faucet_account = inspect_ss58(args.node_bin, read_secret(faucet_suri_raw), "sr25519")

    initial_server_suris_raw = cfg.get("initial_server_suris", [])
    if not isinstance(initial_server_suris_raw, list):
        fail("field `initial_server_suris` must be an array when provided")
    initial_servers: list[str] = []
    for idx, value in enumerate(initial_server_suris_raw):
        if not isinstance(value, str) or not value:
            fail(f"initial_server_suris[{idx}] must be a non-empty string")
        initial_servers.append(inspect_ss58(args.node_bin, read_secret(value), "sr25519"))

    extra_endowed_suris_raw = cfg.get("extra_endowed_suris", [])
    if not isinstance(extra_endowed_suris_raw, list):
        fail("field `extra_endowed_suris` must be an array when provided")
    extra_endowed: list[str] = []
    for idx, value in enumerate(extra_endowed_suris_raw):
        if not isinstance(value, str) or not value:
            fail(f"extra_endowed_suris[{idx}] must be a non-empty string")
        extra_endowed.append(inspect_ss58(args.node_bin, read_secret(value), "sr25519"))

    endowment = cfg.get("endowment", 1 << 60)
    if not isinstance(endowment, int) or endowment <= 0:
        fail("field `endowment` must be a positive integer when provided")

    faucet_payout_amount = cfg.get("faucet_payout_amount", 1_000_000_000_000_000)
    if not isinstance(faucet_payout_amount, int) or faucet_payout_amount <= 0:
        fail("field `faucet_payout_amount` must be a positive integer when provided")

    balance_accounts = dedup_keep_order(
        aura_authorities + [sudo_key, faucet_account] + initial_servers + extra_endowed
    )
    balances = [[address, endowment] for address in balance_accounts]

    overrides = {
        "name": name,
        "bootnodes": bootnodes,
        "aura_authorities": aura_authorities,
        "grandpa_authorities": grandpa_authorities,
        "balances": balances,
        "sudo_key": sudo_key,
        "faucet_account": faucet_account,
        "faucet_payout_amount": faucet_payout_amount,
        "initial_servers": initial_servers,
    }

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(overrides, indent=2) + "\n")

    print(f"[generate-overrides] wrote overrides to {output_path}")
    print(f"[generate-overrides] validators: {len(aura_authorities)}")
    print(f"[generate-overrides] balances: {len(balances)}")


if __name__ == "__main__":
    main()
