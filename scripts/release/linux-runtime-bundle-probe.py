#!/usr/bin/env python3
"""Derive release evidence from a Linux node inside a networkless container."""

from __future__ import annotations

import argparse
import http.client
import json
import os
import re
import signal
import subprocess
import sys
import time
from pathlib import Path
from typing import Any


CODE_STORAGE_KEY = "0x3a636f6465"
HASH_RE = re.compile(r"^0x[0-9a-f]{64}$")


def fail(message: str) -> None:
    raise SystemExit(message)


def write_json(path: Path, value: Any) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def rpc(port: int, request_id: int, method: str, params: list[Any]) -> dict[str, Any]:
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
        fail(f"RPC {method} returned HTTP {response.status}")
    value = json.loads(payload)
    if value.get("error") is not None:
        fail(f"RPC {method} failed: {value['error']}")
    return value


def decode_compact(payload: bytes, offset: int) -> tuple[int, int]:
    if offset >= len(payload):
        fail("truncated SCALE compact value")
    mode = payload[offset] & 0b11
    if mode == 0:
        return payload[offset] >> 2, offset + 1
    if mode == 1:
        end = offset + 2
        if end > len(payload):
            fail("truncated two-byte SCALE compact value")
        return int.from_bytes(payload[offset:end], "little") >> 2, end
    if mode == 2:
        end = offset + 4
        if end > len(payload):
            fail("truncated four-byte SCALE compact value")
        return int.from_bytes(payload[offset:end], "little") >> 2, end
    byte_count = (payload[offset] >> 2) + 4
    start = offset + 1
    end = start + byte_count
    if end > len(payload):
        fail("truncated big-integer SCALE compact value")
    return int.from_bytes(payload[start:end], "little"), end


def decode_metadata_at_version(encoded: str) -> bytes:
    if not isinstance(encoded, str) or not encoded.startswith("0x"):
        fail("Metadata_metadata_at_version returned no hex result")
    try:
        payload = bytes.fromhex(encoded[2:])
    except ValueError as exc:
        fail(f"Metadata_metadata_at_version returned invalid hex: {exc}")
    if not payload or payload[0] != 1:
        fail("Metadata V15 is unavailable")
    length, offset = decode_compact(payload, 1)
    metadata = payload[offset:]
    if len(metadata) != length:
        fail("Metadata V15 opaque payload length mismatch")
    if not metadata.startswith(b"meta\x0f"):
        fail("Metadata response is not SCALE Metadata V15")
    return metadata


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--node", required=True)
    parser.add_argument("--production-wasm", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--rpc-port", type=int, default=19945)
    parser.add_argument("--p2p-port", type=int, default=31345)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    node = Path(args.node).resolve()
    production_wasm = Path(args.production_wasm).resolve()
    output = Path(args.output).resolve()
    if not node.is_file() or not os.access(node, os.X_OK):
        fail("Linux node is unavailable")
    if not production_wasm.is_file():
        fail("production Wasm is unavailable")
    if output.exists():
        fail("probe output must not exist")
    if not 1024 <= args.rpc_port <= 65535 or not 1024 <= args.p2p_port <= 65535:
        fail("probe ports must be in 1024..65535")
    if args.rpc_port == args.p2p_port:
        fail("probe ports must differ")
    output.mkdir(mode=0o700)

    raw_spec = output / "runtime-spec-106.dev-chain-spec.raw.json"
    with raw_spec.open("xb") as handle:
        completed = subprocess.run(
            [str(node), "build-spec", "--chain", "dev", "--disable-default-bootnode", "--raw"],
            stdout=handle,
            stderr=subprocess.PIPE,
            check=False,
        )
    if completed.returncode != 0:
        fail("Linux node failed to build the dev raw chain spec")

    log = (output / "temporary-node.log").open("xb")
    process = subprocess.Popen(
        [
            str(node),
            "--dev",
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
        stdout=log,
        stderr=subprocess.STDOUT,
    )
    try:
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            if process.poll() is not None:
                fail("temporary Linux metadata node exited")
            try:
                version_response = rpc(args.rpc_port, 1, "state_getRuntimeVersion", [])
                break
            except (ConnectionError, OSError, TimeoutError, json.JSONDecodeError):
                time.sleep(0.25)
        else:
            fail("temporary Linux metadata node did not become ready")

        code_response = rpc(args.rpc_port, 2, "state_getStorage", [CODE_STORAGE_KEY])
        genesis_response = rpc(args.rpc_port, 3, "chain_getBlockHash", [0])
        chain_response = rpc(args.rpc_port, 4, "system_chain", [])
        versions_response = rpc(args.rpc_port, 5, "state_call", ["Metadata_metadata_versions", "0x"])
        metadata_response = rpc(
            args.rpc_port,
            6,
            "state_call",
            ["Metadata_metadata_at_version", "0x0f000000"],
        )
    finally:
        if process.poll() is None:
            process.send_signal(signal.SIGTERM)
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=10)
        log.close()

    version = version_response.get("result")
    if not isinstance(version, dict) or version.get("specVersion") != 106:
        fail("temporary Linux node did not report specVersion 106")
    encoded_code = code_response.get("result")
    if not isinstance(encoded_code, str) or not encoded_code.startswith("0x"):
        fail("state_getStorage(:code) returned no hex result")
    try:
        embedded_code = bytes.fromhex(encoded_code[2:])
    except ValueError as exc:
        fail(f"state_getStorage(:code) returned invalid hex: {exc}")
    if embedded_code != production_wasm.read_bytes():
        fail("temporary Linux node embedded :code differs from production Wasm")
    genesis_hash = genesis_response.get("result")
    if not isinstance(genesis_hash, str) or not HASH_RE.fullmatch(genesis_hash):
        fail("temporary Linux node returned an invalid genesis hash")
    chain_name = chain_response.get("result")
    if not isinstance(chain_name, str) or not chain_name:
        fail("temporary Linux node returned no chain name")

    versions_hex = versions_response.get("result")
    if versions_hex != "0x080e0000000f000000":
        fail("runtime metadata version negotiation did not return exactly V14 and V15")
    metadata = decode_metadata_at_version(metadata_response.get("result"))

    (output / "runtime-spec-106.temporary-node-embedded-code.wasm").write_bytes(embedded_code)
    (output / "runtime-metadata.scale").write_bytes(metadata)
    write_json(output / "runtime-version.rpc.json", version_response)
    write_json(output / "genesis-hash.rpc.json", genesis_response)
    write_json(output / "temporary-node-embedded-code.rpc.json", code_response)
    write_json(
        output / "metadata-v15.rpc-proof.json",
        {
            "schemaVersion": 1,
            "kind": "nexus-v2-linux-runtime-metadata-v15-rpc-proof",
            "metadataAtVersionMethod": "Metadata_metadata_at_version",
            "metadataVersionInputScale": "0x0f000000",
            "metadataVersionsMethod": "Metadata_metadata_versions",
            "metadataVersionsResultScale": versions_hex,
            "negotiatedVersion": 15,
        },
    )
    write_json(
        output / "linux-runtime-probe-result.json",
        {
            "schemaVersion": 1,
            "kind": "nexus-v2-linux-runtime-probe-result",
            "chainName": chain_name,
            "genesisHash": genesis_hash,
            "specVersion": 106,
            "embeddedCodeMatchesProductionWasm": True,
            "metadataVersion": 15,
            "networkDisabledByRunner": True,
            "readOnlyRootFilesystemByRunner": True,
            "ephemeralWritableEvidenceWorkspace": True,
        },
    )


if __name__ == "__main__":
    main()
