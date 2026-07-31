#!/usr/bin/env python3
"""
Finalize alpha plain/raw specs from the built-in alpha baseline plus overrides.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any


def fail(message: str) -> None:
    print(f"[finalize-alpha] {message}", file=sys.stderr)
    raise SystemExit(1)


def load_json(path: Path) -> dict[str, Any]:
    try:
        data = json.loads(path.read_text())
    except FileNotFoundError:
        fail(f"input config not found: {path}")
    except json.JSONDecodeError as exc:
        fail(f"invalid JSON in {path}: {exc}")
    if not isinstance(data, dict):
        fail(f"top-level JSON must be an object: {path}")
    return data


def run(cmd: list[str], *, output_path: Path | None = None) -> None:
    try:
        if output_path is None:
            subprocess.check_call(cmd)
            return
        with output_path.open("w", encoding="utf-8") as handle:
            subprocess.check_call(cmd, stdout=handle)
    except FileNotFoundError:
        fail(f"command not found: {cmd[0]}")
    except subprocess.CalledProcessError as exc:
        fail(f"command failed ({exc.returncode}): {' '.join(cmd)}")


def node_command(
    node_bin: str,
    arguments: list[str],
    *,
    node_runner: str | None,
    node_workspace: str | None,
) -> list[str]:
    if node_runner is None:
        if node_workspace is not None:
            fail("--node-workspace requires --node-runner")
        return [node_bin, *arguments]
    if node_workspace is None:
        fail("--node-runner requires --node-workspace")
    runner = Path(node_runner).resolve()
    workspace = Path(node_workspace).resolve()
    node = Path(node_bin).resolve()
    if not runner.is_file() or not os.access(runner, os.X_OK):
        fail("node runner must be an executable regular file")
    if not workspace.is_dir() or workspace.is_symlink():
        fail("node workspace must be a regular directory")
    try:
        node_name = str(node.relative_to(workspace))
    except ValueError:
        fail("runner node must be inside node workspace")
    if "/" in node_name or not node.is_file() or not os.access(node, os.X_OK):
        fail("runner node must be an executable workspace basename")
    mapped: list[str] = []
    for argument in arguments:
        candidate = Path(argument)
        if candidate.is_absolute():
            try:
                relative = candidate.resolve().relative_to(workspace)
            except ValueError:
                fail(f"node argument path escapes runner workspace: {argument}")
            mapped.append("/work/" + relative.as_posix())
        else:
            mapped.append(argument)
    return [
        str(runner),
        "--workspace",
        str(workspace),
        "--node",
        node_name,
        "--",
        *mapped,
    ]


def ensure_list_of_strings(name: str, value: Any, *, allow_empty: bool = False) -> list[str]:
    if not isinstance(value, list):
        fail(f"{name} must be an array")
    if not allow_empty and not value:
        fail(f"{name} must be non-empty")
    normalized: list[str] = []
    for idx, item in enumerate(value):
        if not isinstance(item, str) or not item:
            fail(f"{name}[{idx}] must be a non-empty string")
        normalized.append(item)
    return normalized


def ensure_grandpa(value: Any) -> list[list[Any]]:
    if not isinstance(value, list) or not value:
        fail("grandpa_authorities must be a non-empty array")
    normalized: list[list[Any]] = []
    seen: set[str] = set()
    for idx, item in enumerate(value):
        if not isinstance(item, list) or len(item) != 2:
            fail(f"grandpa_authorities[{idx}] must be [address, weight]")
        address, weight = item
        if not isinstance(address, str) or not address:
            fail(f"grandpa_authorities[{idx}][0] must be a non-empty string")
        try:
            weight_int = int(weight)
        except Exception as exc:
            fail(f"grandpa_authorities[{idx}][1] must be an integer: {exc}")
        if weight_int <= 0:
            fail(f"grandpa_authorities[{idx}][1] must be > 0")
        if address in seen:
            fail(f"grandpa_authorities contains duplicate address: {address}")
        seen.add(address)
        normalized.append([address, weight_int])
    return normalized


def ensure_balances(value: Any) -> list[list[Any]]:
    if not isinstance(value, list) or not value:
        fail("balances must be a non-empty array")
    normalized: list[list[Any]] = []
    for idx, item in enumerate(value):
        if not isinstance(item, list) or len(item) != 2:
            fail(f"balances[{idx}] must be [account, amount]")
        account, amount = item
        if not isinstance(account, str) or not account:
            fail(f"balances[{idx}][0] must be a non-empty string")
        try:
            amount_int = int(amount)
        except Exception as exc:
            fail(f"balances[{idx}][1] must be an integer: {exc}")
        if amount_int <= 0:
            fail(f"balances[{idx}][1] must be > 0")
        normalized.append([account, amount_int])
    return normalized


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Finalize alpha plain/raw chain specs.")
    parser.add_argument(
        "--overrides",
        required=True,
        help="Address-only alpha overrides JSON.",
    )
    parser.add_argument(
        "--node-bin",
        default="./target/release/solochain-eterra-node",
        help="Node binary used for build-spec generation.",
    )
    parser.add_argument(
        "--node-runner",
        help="Digest-pinned Linux runner; when set the node is never executed natively.",
    )
    parser.add_argument(
        "--node-workspace",
        help="Read-only workspace mounted at /work by --node-runner.",
    )
    parser.add_argument(
        "--out-dir",
        default="chain-specs/finalized/alpha",
        help="Output directory for alpha-plain.json and alpha-raw.json.",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    overrides_path = Path(args.overrides)
    overrides = load_json(overrides_path)

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    out_plain = out_dir / "alpha-plain.json"
    out_raw = out_dir / "alpha-raw.json"

    with tempfile.TemporaryDirectory(prefix="eterra-alpha-spec.") as temp_dir:
        baseline_plain = Path(temp_dir) / "alpha-plain.baseline.json"
        run(
            node_command(
                args.node_bin,
                [
                "build-spec",
                "--disable-default-bootnode",
                "--chain",
                "alpha",
                ],
                node_runner=args.node_runner,
                node_workspace=args.node_workspace,
            ),
            output_path=baseline_plain,
        )
        spec = load_json(baseline_plain)

    if spec.get("id") != "eterra_alpha":
        fail(f"alpha source spec id mismatch: {spec.get('id')} != eterra_alpha")

    name = overrides.get("name", "Eterra Alpha")
    if not isinstance(name, str) or not name:
        fail("overrides.name must be a non-empty string when provided")

    bootnodes = ensure_list_of_strings(
        "bootnodes", overrides.get("bootnodes", []), allow_empty=True
    )
    aura = ensure_list_of_strings("aura_authorities", overrides.get("aura_authorities"))
    grandpa = ensure_grandpa(overrides.get("grandpa_authorities"))
    balances = ensure_balances(overrides.get("balances"))
    balance_accounts = {entry[0] for entry in balances}

    sudo_key = overrides.get("sudo_key")
    if not isinstance(sudo_key, str) or not sudo_key:
        fail("sudo_key must be a non-empty string")
    if sudo_key not in balance_accounts:
        fail("balances must include sudo_key account")

    faucet_account = overrides.get("faucet_account", sudo_key)
    if not isinstance(faucet_account, str) or not faucet_account:
        fail("faucet_account must be a non-empty string")
    if faucet_account not in balance_accounts:
        fail("balances must include faucet_account")

    faucet_payout_amount = overrides.get("faucet_payout_amount", 1_000_000_000_000_000)
    try:
        faucet_payout_amount = int(faucet_payout_amount)
    except Exception as exc:
        fail(f"faucet_payout_amount must be an integer: {exc}")
    if faucet_payout_amount <= 0:
        fail("faucet_payout_amount must be > 0")

    initial_servers = ensure_list_of_strings(
        "initial_servers", overrides.get("initial_servers", []), allow_empty=True
    )
    for server in initial_servers:
        if server not in balance_accounts:
            fail(f"balances must include initial_servers account: {server}")

    season_admins = ensure_list_of_strings("season_admins", overrides.get("season_admins"))
    for admin in season_admins:
        if admin not in balance_accounts:
            fail(f"balances must include season_admin account: {admin}")

    media_collection_owner = overrides.get("media_collection_owner")
    if not isinstance(media_collection_owner, str) or not media_collection_owner:
        fail("media_collection_owner must be a non-empty string")
    if media_collection_owner not in balance_accounts:
        fail("balances must include media_collection_owner")

    council_members = ensure_list_of_strings("council_members", overrides.get("council_members"))
    for member in council_members:
        if member not in balance_accounts:
            fail(f"balances must include council member account: {member}")

    asset_owner = overrides.get("asset_owner", sudo_key)
    if not isinstance(asset_owner, str) or not asset_owner:
        fail("asset_owner must be a non-empty string")
    if asset_owner not in balance_accounts:
        fail("balances must include asset_owner")

    spec["name"] = name
    spec["chainType"] = "Live"
    spec["bootNodes"] = bootnodes

    patch = (
        spec.setdefault("genesis", {})
        .setdefault("runtimeGenesis", {})
        .setdefault("patch", {})
    )

    patch.setdefault("aura", {})["authorities"] = aura
    patch.setdefault("grandpa", {})["authorities"] = grandpa
    patch.setdefault("balances", {})["balances"] = balances
    patch.setdefault("sudo", {})["key"] = sudo_key
    patch.setdefault("eterraFaucet", {})["faucetAccount"] = faucet_account
    patch.setdefault("eterraFaucet", {})["payoutAmount"] = faucet_payout_amount
    patch.setdefault("eterraGameAuthority", {})["initialServers"] = initial_servers
    patch.setdefault("eterraSeasons", {})["admins"] = season_admins
    patch.setdefault("eterraMedia", {})["defaultCollectionOwner"] = media_collection_owner
    patch.setdefault("councilMembership", {})["members"] = council_members

    assets_patch = patch.setdefault("assets", {})
    for entry in assets_patch.get("assets", []):
        if isinstance(entry, list) and len(entry) >= 2:
            entry[1] = asset_owner
    for entry in assets_patch.get("accounts", []):
        if isinstance(entry, list) and len(entry) >= 2:
            entry[1] = asset_owner

    out_plain.write_text(json.dumps(spec, indent=2) + "\n")
    run(
        node_command(
            args.node_bin,
            [
            "build-spec",
            "--disable-default-bootnode",
            "--chain",
            str(out_plain),
            "--raw",
            ],
            node_runner=args.node_runner,
            node_workspace=args.node_workspace,
        ),
        output_path=out_raw,
    )

    verify_script = Path(__file__).with_name("verify-alpha-spec.py")
    verify_command = [sys.executable, str(verify_script), "--node-bin", args.node_bin]
    if args.node_runner is not None:
        verify_command.extend(
            [
                "--node-runner",
                args.node_runner,
                "--node-workspace",
                args.node_workspace,
            ]
        )
    verify_command.append(str(out_plain))
    run(verify_command)

    print(f"[finalize-alpha] wrote finalized alpha plain spec: {out_plain}")
    print(f"[finalize-alpha] wrote finalized alpha raw spec: {out_raw}")


if __name__ == "__main__":
    main()
