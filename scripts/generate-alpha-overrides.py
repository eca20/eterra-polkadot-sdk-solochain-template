#!/usr/bin/env python3
"""
Generate chain-specs/alpha-overrides.json from validator/operator key material.

Unlike the production helper, alpha allows an empty bootnode list because the
current deployment is a single private validator behind the public web edge.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


DEFAULT_ALPHA_ADMIN = "5Dq5eLhbKhUpzcuwbsFYisiWnRQkXonTt2RTmvaiVsFwkUsY"


def fail(message: str) -> None:
    print(f"[generate-alpha-overrides] {message}", file=sys.stderr)
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


def read_address_or_suri(
    cfg: dict[str, Any],
    node_bin: str,
    *,
    address_field: str,
    suri_field: str,
    scheme: str,
    required: bool = True,
) -> str | None:
    address_raw = cfg.get(address_field)
    if address_raw is not None:
        if not isinstance(address_raw, str) or not address_raw:
            fail(f"field `{address_field}` must be a non-empty string when provided")
        return address_raw

    suri_raw = cfg.get(suri_field)
    if suri_raw is None:
        if required:
            fail(f"missing `{address_field}` or `{suri_field}`")
        return None
    if not isinstance(suri_raw, str) or not suri_raw:
        fail(f"field `{suri_field}` must be a non-empty string when provided")
    return inspect_ss58(node_bin, read_secret(suri_raw), scheme)


def read_address_list_or_suris(
    cfg: dict[str, Any],
    node_bin: str,
    *,
    address_field: str,
    suri_field: str,
    scheme: str,
) -> list[str]:
    addresses_raw = cfg.get(address_field)
    if addresses_raw is not None:
        if not isinstance(addresses_raw, list):
            fail(f"field `{address_field}` must be an array when provided")
        result: list[str] = []
        for idx, value in enumerate(addresses_raw):
            if not isinstance(value, str) or not value:
                fail(f"{address_field}[{idx}] must be a non-empty string")
            result.append(value)
        return result

    suris_raw = cfg.get(suri_field, [])
    if not isinstance(suris_raw, list):
        fail(f"field `{suri_field}` must be an array when provided")
    result: list[str] = []
    for idx, value in enumerate(suris_raw):
        if not isinstance(value, str) or not value:
            fail(f"{suri_field}[{idx}] must be a non-empty string")
        result.append(inspect_ss58(node_bin, read_secret(value), scheme))
    return result


def dedup_keep_order(values: list[str]) -> list[str]:
    seen: set[str] = set()
    result: list[str] = []
    for value in values:
        if value not in seen:
            seen.add(value)
            result.append(value)
    return result


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate alpha overrides JSON from key material."
    )
    parser.add_argument(
        "--in",
        dest="input_path",
        required=True,
        help="Input JSON config (keys + optional bootnodes).",
    )
    parser.add_argument(
        "--out",
        dest="output_path",
        default="chain-specs/alpha-overrides.json",
        help="Output overrides JSON path.",
    )
    parser.add_argument(
        "--node-bin",
        default="./target/release/solochain-eterra-node",
        help="Node binary path used for `key inspect`.",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    input_path = Path(args.input_path)
    output_path = Path(args.output_path)
    cfg = load_json(input_path)

    name = cfg.get("name", "Eterra Alpha")
    if not isinstance(name, str) or not name:
        fail("field `name` must be a non-empty string when provided")

    validators = cfg.get("validators")
    if not isinstance(validators, list) or not validators:
        fail("field `validators` must be a non-empty array")

    bootnodes: list[str] = []
    aura_authorities: list[str] = []
    grandpa_authorities: list[list[Any]] = []
    aura_set: set[str] = set()
    grandpa_set: set[str] = set()
    bootnode_set: set[str] = set()

    for idx, raw in enumerate(validators):
        if not isinstance(raw, dict):
            fail(f"validators[{idx}] must be an object")

        bootnode = raw.get("bootnode", "")
        if bootnode is not None:
            if not isinstance(bootnode, str):
                fail(f"validators[{idx}].bootnode must be a string when provided")
            bootnode = bootnode.strip()
            if bootnode:
                if "127.0.0.1" in bootnode or "localhost" in bootnode:
                    fail(f"validators[{idx}].bootnode must not be localhost: {bootnode}")
                if bootnode in bootnode_set:
                    fail(f"duplicate bootnode in validators[{idx}]: {bootnode}")
                bootnode_set.add(bootnode)
                bootnodes.append(bootnode)

        aura_suri = read_secret(require_str(raw, "aura_suri"))
        grandpa_suri = read_secret(require_str(raw, "grandpa_suri"))

        aura_address = inspect_ss58(args.node_bin, aura_suri, "sr25519")
        grandpa_address = inspect_ss58(args.node_bin, grandpa_suri, "ed25519")
        if aura_address in aura_set:
            fail(f"duplicate Aura authority derived for validators[{idx}]: {aura_address}")
        if grandpa_address in grandpa_set:
            fail(
                f"duplicate Grandpa authority derived for validators[{idx}]: {grandpa_address}"
            )
        aura_set.add(aura_address)
        grandpa_set.add(grandpa_address)
        aura_authorities.append(aura_address)

        weight = raw.get("grandpa_weight", 1)
        if not isinstance(weight, int) or weight <= 0:
            fail(f"validators[{idx}].grandpa_weight must be a positive integer")
        grandpa_authorities.append([grandpa_address, weight])

    sudo_key = read_address_or_suri(
        cfg,
        args.node_bin,
        address_field="sudo_address",
        suri_field="sudo_suri",
        scheme="sr25519",
    )
    assert sudo_key is not None

    faucet_account = read_address_or_suri(
        cfg,
        args.node_bin,
        address_field="faucet_address",
        suri_field="faucet_suri",
        scheme="sr25519",
        required=False,
    )
    if faucet_account is None:
        faucet_account = sudo_key

    media_signer = read_address_or_suri(
        cfg,
        args.node_bin,
        address_field="media_signer_address",
        suri_field="media_signer_suri",
        scheme="sr25519",
        required=False,
    )

    initial_servers = read_address_list_or_suris(
        cfg,
        args.node_bin,
        address_field="initial_server_addresses",
        suri_field="initial_server_suris",
        scheme="sr25519",
    )

    extra_endowed = read_address_list_or_suris(
        cfg,
        args.node_bin,
        address_field="extra_endowed_addresses",
        suri_field="extra_endowed_suris",
        scheme="sr25519",
    )

    season_admins = read_address_list_or_suris(
        cfg,
        args.node_bin,
        address_field="season_admin_addresses",
        suri_field="season_admin_suris",
        scheme="sr25519",
    )
    if media_signer:
        season_admins.append(media_signer)
    if not season_admins:
        season_admins = [DEFAULT_ALPHA_ADMIN]

    council_members = read_address_list_or_suris(
        cfg,
        args.node_bin,
        address_field="council_addresses",
        suri_field="council_suris",
        scheme="sr25519",
    )
    if not council_members:
        council_members = [sudo_key]

    media_collection_owner = read_address_or_suri(
        cfg,
        args.node_bin,
        address_field="media_collection_owner_address",
        suri_field="media_collection_owner_suri",
        scheme="sr25519",
        required=False,
    )
    if media_collection_owner is None:
        media_collection_owner = media_signer or sudo_key

    asset_owner = read_address_or_suri(
        cfg,
        args.node_bin,
        address_field="asset_owner_address",
        suri_field="asset_owner_suri",
        scheme="sr25519",
        required=False,
    )
    if asset_owner is None:
        asset_owner = sudo_key

    endowment = cfg.get("endowment", 1 << 60)
    if not isinstance(endowment, int) or endowment <= 0:
        fail("field `endowment` must be a positive integer when provided")

    faucet_payout_amount = cfg.get("faucet_payout_amount", 1_000_000_000_000_000)
    if not isinstance(faucet_payout_amount, int) or faucet_payout_amount <= 0:
        fail("field `faucet_payout_amount` must be a positive integer when provided")

    balance_accounts = dedup_keep_order(
        aura_authorities
        + [sudo_key, faucet_account, asset_owner]
        + council_members
        + initial_servers
        + extra_endowed
        + season_admins
        + [media_collection_owner]
    )
    balances = [[address, endowment] for address in balance_accounts]

    overrides: dict[str, Any] = {
        "name": name,
        "bootnodes": bootnodes,
        "aura_authorities": aura_authorities,
        "grandpa_authorities": grandpa_authorities,
        "balances": balances,
        "sudo_key": sudo_key,
        "faucet_account": faucet_account,
        "faucet_payout_amount": faucet_payout_amount,
        "initial_servers": dedup_keep_order(initial_servers),
        "season_admins": dedup_keep_order(season_admins),
        "media_collection_owner": media_collection_owner,
        "council_members": dedup_keep_order(council_members),
        "asset_owner": asset_owner,
    }

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(overrides, indent=2) + "\n")

    print(f"[generate-alpha-overrides] wrote overrides to {output_path}")
    print(f"[generate-alpha-overrides] validators: {len(aura_authorities)}")
    print(f"[generate-alpha-overrides] balances: {len(balances)}")


if __name__ == "__main__":
    main()
