#!/usr/bin/env python3
"""
Verify a finalized alpha plain spec contains no Substrate well-known dev accounts.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any


DEV_NAMES = ["Alice", "Bob", "Charlie", "Dave", "Eve", "Ferdie"]


def fail(message: str) -> None:
    print(f"[verify-alpha] {message}", file=sys.stderr)
    raise SystemExit(1)


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
    return [
        str(runner),
        "--workspace",
        str(workspace),
        "--node",
        node_name,
        "--",
        *arguments,
    ]


def inspect_ss58(
    node_bin: str,
    secret_or_suri: str,
    scheme: str,
    *,
    node_runner: str | None,
    node_workspace: str | None,
) -> str:
    cmd = node_command(
        node_bin,
        [
        "key",
        "inspect",
        "--scheme",
        scheme,
        "--output-type",
        "json",
        secret_or_suri,
        ],
        node_runner=node_runner,
        node_workspace=node_workspace,
    )
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


def build_denylist(
    node_bin: str,
    *,
    node_runner: str | None,
    node_workspace: str | None,
) -> set[str]:
    denylist: set[str] = set()
    for name in DEV_NAMES:
        denylist.add(inspect_ss58(node_bin, f"//{name}", "sr25519", node_runner=node_runner, node_workspace=node_workspace))
        denylist.add(inspect_ss58(node_bin, f"//{name}//stash", "sr25519", node_runner=node_runner, node_workspace=node_workspace))
        denylist.add(inspect_ss58(node_bin, f"//{name}", "ed25519", node_runner=node_runner, node_workspace=node_workspace))
    return denylist


def walk_strings(value: Any) -> list[str]:
    if isinstance(value, str):
        return [value]
    if isinstance(value, list):
        out: list[str] = []
        for item in value:
            out.extend(walk_strings(item))
        return out
    if isinstance(value, dict):
        out: list[str] = []
        for item in value.values():
            out.extend(walk_strings(item))
        return out
    return []


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Verify a finalized alpha plain spec.")
    parser.add_argument("spec_path", help="Path to finalized alpha plain spec.")
    parser.add_argument(
        "--node-bin",
        default="./target/release/solochain-eterra-node",
        help="Node binary path used for `key inspect`.",
    )
    parser.add_argument("--node-runner", help="Digest-pinned Linux runner for node commands.")
    parser.add_argument("--node-workspace", help="Read-only workspace mounted at /work by the runner.")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    spec_path = Path(args.spec_path)
    try:
        spec = json.loads(spec_path.read_text())
    except FileNotFoundError:
        fail(f"spec not found: {spec_path}")
    except json.JSONDecodeError as exc:
        fail(f"invalid JSON in {spec_path}: {exc}")

    if spec.get("id") != "eterra_alpha":
        fail(f"alpha spec id mismatch: {spec.get('id')} != eterra_alpha")
    if spec.get("chainType") != "Live":
        fail(f"alpha spec chainType must be Live, got: {spec.get('chainType')}")

    patch = (
        spec.get("genesis", {})
        .get("runtimeGenesis", {})
        .get("patch", {})
    )

    aura_authorities = patch.get("aura", {}).get("authorities", [])
    grandpa_authorities = patch.get("grandpa", {}).get("authorities", [])
    if not isinstance(aura_authorities, list) or not aura_authorities:
        fail("alpha spec must include at least one Aura authority")
    if not isinstance(grandpa_authorities, list) or not grandpa_authorities:
        fail("alpha spec must include at least one Grandpa authority")
    if len(aura_authorities) != len(grandpa_authorities):
        fail("alpha spec must have equal Aura and Grandpa authority counts")

    balances = patch.get("balances", {}).get("balances", [])
    if not isinstance(balances, list) or not balances:
        fail("alpha spec must include non-empty balances allocation")
    balance_accounts = {
        entry[0]
        for entry in balances
        if isinstance(entry, list) and len(entry) >= 2 and isinstance(entry[0], str)
    }

    sudo_key = patch.get("sudo", {}).get("key")
    if not isinstance(sudo_key, str) or not sudo_key:
        fail("alpha spec must include non-empty sudo key")
    if sudo_key not in balance_accounts:
        fail("alpha balances must fund sudo key account")

    council_members = patch.get("councilMembership", {}).get("members", [])
    if not isinstance(council_members, list) or not council_members:
        fail("alpha spec must include at least one council member")

    faucet_account = patch.get("eterraFaucet", {}).get("faucetAccount")
    if not isinstance(faucet_account, str) or not faucet_account:
        fail("alpha spec must include a funded faucet account")

    season_admins = patch.get("eterraSeasons", {}).get("admins", [])
    if not isinstance(season_admins, list) or not season_admins:
        fail("alpha spec must include at least one season admin")

    media_owner = patch.get("eterraMedia", {}).get("defaultCollectionOwner")
    if not isinstance(media_owner, str) or not media_owner:
        fail("alpha spec must include a default media collection owner")

    asset_owner = None
    for entry in patch.get("assets", {}).get("assets", []):
        if isinstance(entry, list) and len(entry) >= 2 and isinstance(entry[1], str):
            asset_owner = entry[1]
            break
    if not asset_owner:
        fail("alpha spec must include at least one asset owner entry")

    initial_servers = patch.get("eterraGameAuthority", {}).get("initialServers", [])
    if not isinstance(initial_servers, list):
        fail("alpha spec initialServers must be an array")

    denylist = build_denylist(
        args.node_bin,
        node_runner=args.node_runner,
        node_workspace=args.node_workspace,
    )
    spec_strings = walk_strings(spec)
    offenders = sorted({value for value in spec_strings if value in denylist})
    if offenders:
        fail(
            "alpha spec still contains Substrate well-known dev accounts: "
            + ", ".join(offenders)
        )

    required_accounts = (
        set(council_members)
        | set(season_admins)
        | set(initial_servers)
        | {sudo_key, faucet_account, media_owner, asset_owner}
    )
    missing = sorted(account for account in required_accounts if account not in balance_accounts)
    if missing:
        fail("alpha balances must fund all operator accounts: " + ", ".join(missing))

    print(f"[verify-alpha] verification passed for {spec_path}")


if __name__ == "__main__":
    main()
