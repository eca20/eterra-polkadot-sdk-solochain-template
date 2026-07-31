#!/usr/bin/env python3
"""Inspect an Alpha genesis using a Linux node inside the isolated runner."""

from __future__ import annotations

import argparse
import http.client
import json
import re
import signal
import subprocess
import sys
import time
from pathlib import Path
from typing import Any


HASH_RE = re.compile(r"^0x[0-9a-f]{64}$")


def rpc(port: int, request_id: int, method: str, params: list[Any]) -> Any:
    body = json.dumps(
        {"id": request_id, "jsonrpc": "2.0", "method": method, "params": params},
        separators=(",", ":"),
    ).encode()
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=2)
    try:
        connection.request("POST", "/", body, {"Content-Type": "application/json"})
        response = connection.getresponse()
        payload = response.read()
    finally:
        connection.close()
    if response.status != 200:
        raise RuntimeError(f"RPC {method} returned HTTP {response.status}")
    value = json.loads(payload)
    if value.get("error") is not None:
        raise RuntimeError(f"RPC {method} failed: {value['error']}")
    return value.get("result")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--node", required=True)
    parser.add_argument("--chain", required=True)
    parser.add_argument("--rpc-port", required=True, type=int)
    parser.add_argument("--p2p-port", required=True, type=int)
    args = parser.parse_args()
    node = Path(args.node)
    chain = Path(args.chain)
    if not node.is_file() or not chain.is_file():
        raise SystemExit("node or Alpha raw spec is unavailable")
    if not 1024 <= args.rpc_port <= 65535 or not 1024 <= args.p2p_port <= 65535:
        raise SystemExit("probe ports must be in 1024..65535")
    if args.rpc_port == args.p2p_port:
        raise SystemExit("probe ports must differ")

    process = subprocess.Popen(
        [
            str(node),
            "--chain",
            str(chain),
            "--tmp",
            "--rpc-port",
            str(args.rpc_port),
            "--listen-addr",
            f"/ip4/127.0.0.1/tcp/{args.p2p_port}",
            "--rpc-methods",
            "Safe",
            "--no-telemetry",
            "--no-prometheus",
            "--no-mdns",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    try:
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError("temporary Alpha genesis node exited")
            try:
                version = rpc(args.rpc_port, 1, "state_getRuntimeVersion", [])
                genesis_hash = rpc(args.rpc_port, 2, "chain_getBlockHash", [0])
                chain_name = rpc(args.rpc_port, 3, "system_chain", [])
                break
            except (ConnectionError, OSError, TimeoutError, json.JSONDecodeError):
                time.sleep(0.25)
        else:
            raise RuntimeError("temporary Alpha genesis node did not become ready")
    finally:
        if process.poll() is None:
            process.send_signal(signal.SIGTERM)
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=10)

    if not isinstance(version, dict) or version.get("specVersion") != 106:
        raise RuntimeError("temporary Alpha node spec version mismatch")
    if not isinstance(genesis_hash, str) or not HASH_RE.fullmatch(genesis_hash):
        raise RuntimeError("temporary Alpha node genesis hash is invalid")
    if not isinstance(chain_name, str) or not chain_name:
        raise RuntimeError("temporary Alpha node chain name is missing")
    json.dump(
        {
            "schemaVersion": 1,
            "kind": "nexus-v2-linux-alpha-genesis-probe",
            "chainName": chain_name,
            "genesisHash": genesis_hash,
            "specVersion": version["specVersion"],
            "runnerNetworkDisabled": True,
        },
        sys.stdout,
        indent=2,
        sort_keys=True,
    )
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
