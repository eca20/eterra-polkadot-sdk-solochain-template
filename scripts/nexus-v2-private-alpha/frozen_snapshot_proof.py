#!/usr/bin/env python3
"""Create and verify exact-block provenance for final-freeze try-runtime snapshots."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import re
import sys
from pathlib import Path
from typing import Any, Mapping, Sequence


HASH_RE = re.compile(r"^0x[0-9a-f]{64}$")
SHA_RE = re.compile(r"^[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
ID_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
PROOF_KEYS = {
    "authorizations",
    "createdAtUtc",
    "creation",
    "frozenAtUtc",
    "frozenFinalizedBlock",
    "isolatedRpcObservation",
    "kind",
    "releaseId",
    "schemaVersion",
    "snapshot",
    "source",
    "sourceCommit",
    "transactionId",
    "tryRuntime",
}
TARGET_SOURCE_KEYS = {"chainSpecSha256", "nodeBinarySha256", "nodeDataArchiveSha256"}
SNAPSHOT_KEYS = {"bytes", "sha256"}
TRY_RUNTIME_KEYS = {"log", "sha256", "sourceRevision", "version"}
CREATION_KEYS = {
    "explicitAtHash",
    "isolatedCopyOnly",
    "networkIsolated",
    "originalNodeRemainedStopped",
    "sourceArchiveExtracted",
}
RPC_KEYS = {
    "blockHashAtNumber",
    "finalizedHead",
    "genesisHash",
    "headerHash",
    "headerNumber",
    "runtimeCodeHash",
    "runtimeSpecVersion",
}


class ProofError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ProofError(message)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def regular_file(path: Path, label: str) -> Path:
    require(path.is_file() and not path.is_symlink(), f"{label} must be a regular file")
    return path.resolve()


def parse_utc(value: Any, label: str) -> dt.datetime:
    require(isinstance(value, str) and value.endswith("Z"), f"invalid {label}")
    try:
        parsed = dt.datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError as exc:
        raise ProofError(f"invalid {label}") from exc
    require(parsed.tzinfo is not None, f"invalid {label}")
    return parsed


def hash256(value: Any, label: str) -> str:
    require(isinstance(value, str) and bool(HASH_RE.fullmatch(value.lower())), f"invalid {label}")
    return value.lower()


def sha256(value: Any, label: str) -> str:
    require(isinstance(value, str) and bool(SHA_RE.fullmatch(value)), f"invalid {label}")
    return value


def read_json(path: Path, label: str) -> dict[str, Any]:
    path = regular_file(path, label)
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ProofError(f"invalid {label}") from exc
    require(isinstance(value, dict), f"{label} root must be an object")
    return value


def rpc_result(capture: Mapping[str, Any], name: str) -> Any:
    response = capture.get(name)
    require(isinstance(response, dict) and response.get("error") is None, f"isolated RPC {name} failed")
    require(set(response) <= {"id", "jsonrpc", "result"} and "result" in response, f"isolated RPC {name} schema mismatch")
    return response["result"]


def normalize_rpc(capture: Mapping[str, Any], block_number: int, block_hash: str) -> dict[str, Any]:
    finalized = hash256(rpc_result(capture, "finalizedHead"), "isolated finalized head")
    at_number = hash256(rpc_result(capture, "blockHashAtNumber"), "isolated block hash at number")
    genesis = hash256(rpc_result(capture, "genesisHash"), "isolated genesis hash")
    code_hash = hash256(rpc_result(capture, "runtimeCodeHash"), "isolated runtime code hash")
    header = rpc_result(capture, "header")
    require(isinstance(header, dict), "isolated header is missing")
    header_number_hex = header.get("number")
    require(isinstance(header_number_hex, str) and re.fullmatch(r"0x[0-9a-fA-F]+", header_number_hex), "isolated header number is invalid")
    header_number = int(header_number_hex, 16)
    header_hash = hash256(header.get("hash", block_hash), "isolated header hash")
    runtime = rpc_result(capture, "runtimeVersion")
    require(isinstance(runtime, dict), "isolated runtime version is missing")
    spec_version = runtime.get("specVersion")
    require(isinstance(spec_version, int) and not isinstance(spec_version, bool) and spec_version > 0, "isolated runtime spec version is invalid")
    require(finalized == block_hash, "isolated finalized head differs from frozen marker")
    require(at_number == block_hash, "isolated block hash at frozen number differs from frozen marker")
    require(header_number == block_number, "isolated header number differs from frozen marker")
    require(header_hash == block_hash, "isolated header hash differs from frozen marker")
    return {
        "blockHashAtNumber": at_number,
        "finalizedHead": finalized,
        "genesisHash": genesis,
        "headerHash": header_hash,
        "headerNumber": header_number,
        "runtimeCodeHash": code_hash,
        "runtimeSpecVersion": spec_version,
    }


def write_new_json(path: Path, value: Mapping[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "wb") as handle:
        handle.write(payload)


def create(args: argparse.Namespace) -> dict[str, Any]:
    require(bool(ID_RE.fullmatch(args.transaction_id)), "invalid transaction ID")
    require(bool(ID_RE.fullmatch(args.release_id)), "invalid release ID")
    require(bool(COMMIT_RE.fullmatch(args.source_commit)), "invalid source commit")
    require(args.block_number >= 0, "invalid frozen block number")
    block_hash = hash256(args.block_hash, "frozen block hash")
    frozen_at = parse_utc(args.frozen_at_utc, "frozen time")
    created_at = parse_utc(args.created_at_utc, "snapshot creation time")
    require(created_at >= frozen_at, "snapshot predates the final freeze")
    snapshot = regular_file(Path(args.snapshot), "try-runtime snapshot")
    require(snapshot.stat().st_size > 0, "try-runtime snapshot is empty")
    node_data = regular_file(Path(args.node_data_archive), "stopped node-data archive")
    node_binary = regular_file(Path(args.node_binary), "stopped node binary")
    chain_spec = regular_file(Path(args.chain_spec), "stopped chain spec")
    try_runtime = regular_file(Path(args.try_runtime), "try-runtime binary")
    log = regular_file(Path(args.try_runtime_log), "try-runtime snapshot log")
    require(bool(COMMIT_RE.fullmatch(args.try_runtime_revision)), "invalid try-runtime source revision")
    require(isinstance(args.try_runtime_version, str) and 0 < len(args.try_runtime_version) <= 512, "invalid try-runtime version")
    capture = read_json(Path(args.rpc_capture), "isolated frozen-state RPC capture")
    observation = normalize_rpc(capture, args.block_number, block_hash)
    value = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-frozen-try-runtime-snapshot-proof",
        "transactionId": args.transaction_id,
        "releaseId": args.release_id,
        "sourceCommit": args.source_commit,
        "frozenAtUtc": args.frozen_at_utc,
        "createdAtUtc": args.created_at_utc,
        "frozenFinalizedBlock": {"number": args.block_number, "hash": block_hash},
        "source": {
            "chainSpecSha256": sha256_file(chain_spec),
            "nodeBinarySha256": sha256_file(node_binary),
            "nodeDataArchiveSha256": sha256_file(node_data),
        },
        "snapshot": {"bytes": snapshot.stat().st_size, "sha256": sha256_file(snapshot)},
        "tryRuntime": {
            "log": log.read_text(encoding="utf-8", errors="replace"),
            "sha256": sha256_file(try_runtime),
            "sourceRevision": args.try_runtime_revision,
            "version": args.try_runtime_version,
        },
        "isolatedRpcObservation": observation,
        "creation": {
            "explicitAtHash": True,
            "isolatedCopyOnly": True,
            "networkIsolated": True,
            "originalNodeRemainedStopped": True,
            "sourceArchiveExtracted": True,
        },
        "authorizations": {"liveSubmission": False, "paidOrPublicActivation": False},
    }
    write_new_json(Path(args.output), value)
    return verify(
        Path(args.output),
        snapshot,
        node_data,
        node_binary,
        chain_spec,
        args.block_number,
        block_hash,
    )


def verify(
    proof_path: Path,
    snapshot: Path,
    node_data: Path,
    node_binary: Path,
    chain_spec: Path,
    block_number: int,
    block_hash: str,
) -> dict[str, Any]:
    value = read_json(proof_path, "frozen try-runtime snapshot proof")
    require(set(value) == PROOF_KEYS, "frozen snapshot proof does not match the closed schema")
    require(value.get("schemaVersion") == 1, "frozen snapshot proof schema mismatch")
    require(value.get("kind") == "nexus-v2-private-alpha-frozen-try-runtime-snapshot-proof", "frozen snapshot proof kind mismatch")
    require(isinstance(value.get("transactionId"), str) and bool(ID_RE.fullmatch(value["transactionId"])), "invalid proof transaction ID")
    require(isinstance(value.get("releaseId"), str) and bool(ID_RE.fullmatch(value["releaseId"])), "invalid proof release ID")
    require(isinstance(value.get("sourceCommit"), str) and bool(COMMIT_RE.fullmatch(value["sourceCommit"])), "invalid proof source commit")
    frozen_at = parse_utc(value.get("frozenAtUtc"), "proof frozen time")
    created_at = parse_utc(value.get("createdAtUtc"), "proof creation time")
    require(created_at >= frozen_at, "proof snapshot predates freeze")
    frozen = value.get("frozenFinalizedBlock")
    require(isinstance(frozen, dict) and set(frozen) == {"hash", "number"}, "proof frozen block schema mismatch")
    require(frozen.get("number") == block_number, "proof frozen block number mismatch")
    require(hash256(frozen.get("hash"), "proof frozen block hash") == block_hash, "proof frozen block hash mismatch")
    source = value.get("source")
    require(isinstance(source, dict) and set(source) == TARGET_SOURCE_KEYS, "proof source schema mismatch")
    require(source.get("nodeDataArchiveSha256") == sha256_file(regular_file(node_data, "node-data archive")), "proof node-data archive hash mismatch")
    require(source.get("nodeBinarySha256") == sha256_file(regular_file(node_binary, "node binary")), "proof node binary hash mismatch")
    require(source.get("chainSpecSha256") == sha256_file(regular_file(chain_spec, "chain spec")), "proof chain spec hash mismatch")
    snapshot_value = value.get("snapshot")
    require(isinstance(snapshot_value, dict) and set(snapshot_value) == SNAPSHOT_KEYS, "proof snapshot schema mismatch")
    snapshot = regular_file(snapshot, "try-runtime snapshot")
    require(snapshot_value.get("bytes") == snapshot.stat().st_size and snapshot.stat().st_size > 0, "proof snapshot byte count mismatch")
    require(snapshot_value.get("sha256") == sha256_file(snapshot), "proof snapshot hash mismatch")
    creation = value.get("creation")
    require(isinstance(creation, dict) and set(creation) == CREATION_KEYS and all(item is True for item in creation.values()), "proof creation guarantees are incomplete")
    observation = value.get("isolatedRpcObservation")
    require(isinstance(observation, dict) and set(observation) == RPC_KEYS, "proof isolated RPC schema mismatch")
    require(observation.get("finalizedHead") == block_hash, "proof isolated finalized head mismatch")
    require(observation.get("blockHashAtNumber") == block_hash, "proof isolated block lookup mismatch")
    require(observation.get("headerHash") == block_hash and observation.get("headerNumber") == block_number, "proof isolated header mismatch")
    hash256(observation.get("genesisHash"), "proof genesis hash")
    hash256(observation.get("runtimeCodeHash"), "proof runtime code hash")
    require(isinstance(observation.get("runtimeSpecVersion"), int) and observation["runtimeSpecVersion"] > 0, "proof runtime spec version is invalid")
    try_runtime = value.get("tryRuntime")
    require(isinstance(try_runtime, dict) and set(try_runtime) == TRY_RUNTIME_KEYS, "proof try-runtime schema mismatch")
    sha256(try_runtime.get("sha256"), "proof try-runtime SHA-256")
    require(isinstance(try_runtime.get("sourceRevision"), str) and bool(COMMIT_RE.fullmatch(try_runtime["sourceRevision"])), "invalid proof try-runtime revision")
    require(isinstance(try_runtime.get("version"), str) and 0 < len(try_runtime["version"]) <= 512, "invalid proof try-runtime version")
    require(isinstance(try_runtime.get("log"), str) and len(try_runtime["log"]) <= 1024 * 1024, "invalid proof try-runtime log")
    require(value.get("authorizations") == {"liveSubmission": False, "paidOrPublicActivation": False}, "proof activation flags are unsafe")
    return {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-frozen-try-runtime-snapshot-proof-verification",
        "transactionId": value["transactionId"],
        "releaseId": value["releaseId"],
        "sourceCommit": value["sourceCommit"],
        "frozenAtUtc": value["frozenAtUtc"],
        "frozenFinalizedBlock": frozen,
        "snapshotSha256": snapshot_value["sha256"],
        "nodeDataArchiveSha256": source["nodeDataArchiveSha256"],
        "runtimeSpecVersion": observation["runtimeSpecVersion"],
        "proofSha256": sha256_file(proof_path),
    }


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Create or verify frozen-state try-runtime snapshot provenance")
    sub = parser.add_subparsers(dest="command", required=True)
    create_parser = sub.add_parser("create")
    for argument in (
        "transaction-id",
        "release-id",
        "source-commit",
        "frozen-at-utc",
        "created-at-utc",
        "block-hash",
        "snapshot",
        "node-data-archive",
        "node-binary",
        "chain-spec",
        "try-runtime",
        "try-runtime-revision",
        "try-runtime-version",
        "try-runtime-log",
        "rpc-capture",
        "output",
    ):
        create_parser.add_argument(f"--{argument}", required=True)
    create_parser.add_argument("--block-number", required=True, type=int)
    verify_parser = sub.add_parser("verify")
    for argument in ("proof", "snapshot", "node-data-archive", "node-binary", "chain-spec", "block-hash"):
        verify_parser.add_argument(f"--{argument}", required=True)
    verify_parser.add_argument("--block-number", required=True, type=int)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        if args.command == "create":
            result = create(args)
        else:
            result = verify(
                Path(args.proof),
                Path(args.snapshot),
                Path(args.node_data_archive),
                Path(args.node_binary),
                Path(args.chain_spec),
                args.block_number,
                hash256(args.block_hash, "expected frozen block hash"),
            )
        print(json.dumps(result, sort_keys=True))
    except (OSError, ProofError) as exc:
        print(f"nexus-v2-frozen-snapshot-proof: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
