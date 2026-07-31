#!/usr/bin/env python3
"""Validate the one permitted private-alpha fresh-reset readiness contract."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any


SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
HASH256_RE = re.compile(r"^0x[0-9a-fA-F]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
RELEASE_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
READINESS_KEYS = {
    "schemaVersion",
    "kind",
    "releaseId",
    "sourceCommit",
    "backupManifestSha256",
    "restoreEvidenceSha256",
    "migrationEvidenceSha256",
    "economicGatesSha256",
    "acceptanceInventorySha256",
    "economicGateMode",
    "resetMode",
    "freshGenesisReplacementOnly",
    "inPlaceRuntimeActivationAuthorized",
    "gateFinalizedBlock",
    "readyForSeparateOperatorResetAuthorization",
    "automaticRollbackEligibleAtGateBlock",
    "economicFlagsDisabled",
    "v2AcceptanceAssetsExist",
    "resetExecuted",
    "deployExecuted",
    "createdAtUtc",
}
PINNED_HASH_FIELDS = {
    "backupManifestSha256",
    "restoreEvidenceSha256",
    "migrationEvidenceSha256",
    "economicGatesSha256",
    "acceptanceInventorySha256",
}


class ReadinessError(RuntimeError):
    """The packet cannot authorize the guarded fresh-reset lane."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ReadinessError(message)


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        require(key not in value, f"readiness packet contains duplicate field: {key}")
        value[key] = item
    return value


def parse_created_at(value: Any) -> None:
    require(isinstance(value, str) and value, "createdAtUtc must be an ISO-8601 string")
    try:
        parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise ReadinessError("createdAtUtc is not valid ISO-8601") from exc
    require(parsed.tzinfo is not None, "createdAtUtc must include a timezone")


def validate_packet(path: Path, expected_sha256: str) -> dict[str, Any]:
    require(bool(SHA256_RE.fullmatch(expected_sha256)), "expected SHA-256 must be 64 lowercase hex characters")
    require(path.exists(), f"readiness packet not found: {path}")
    require(not path.is_symlink(), "readiness packet must not be a symlink")
    require(path.is_file(), "readiness packet must be a regular file")

    payload = path.read_bytes()
    actual_sha256 = sha256_bytes(payload)
    require(actual_sha256 == expected_sha256, "readiness packet SHA-256 does not match NEXUS_V2_RESET_READINESS_SHA256")
    try:
        value = json.loads(payload, object_pairs_hook=reject_duplicate_keys)
    except json.JSONDecodeError as exc:
        raise ReadinessError("readiness packet is not valid JSON") from exc
    require(isinstance(value, dict), "readiness packet root must be an object")
    require(set(value) == READINESS_KEYS, "readiness packet fields do not match the closed schema")

    require(
        isinstance(value["schemaVersion"], int)
        and not isinstance(value["schemaVersion"], bool)
        and value["schemaVersion"] == 1,
        "readiness schema version mismatch",
    )
    require(value["kind"] == "nexus-v2-private-alpha-reset-readiness", "readiness kind mismatch")
    require(isinstance(value["releaseId"], str) and bool(RELEASE_RE.fullmatch(value["releaseId"])), "invalid readiness releaseId")
    require(isinstance(value["sourceCommit"], str) and bool(COMMIT_RE.fullmatch(value["sourceCommit"])), "invalid readiness sourceCommit")
    for field in PINNED_HASH_FIELDS:
        require(isinstance(value[field], str) and bool(SHA256_RE.fullmatch(value[field])), f"invalid readiness {field}")

    require(value["economicGateMode"] == "pre-v16-fresh-reset-frozen", "only a frozen pre-V16 gate may authorize fresh reset")
    require(value["resetMode"] == "fresh-genesis-replacement", "readiness is not fresh-genesis-only")
    required_true = {
        "freshGenesisReplacementOnly",
        "readyForSeparateOperatorResetAuthorization",
        "automaticRollbackEligibleAtGateBlock",
        "economicFlagsDisabled",
    }
    required_false = {
        "inPlaceRuntimeActivationAuthorized",
        "v2AcceptanceAssetsExist",
        "resetExecuted",
        "deployExecuted",
    }
    for field in required_true:
        require(value[field] is True, f"readiness must set {field}=true")
    for field in required_false:
        require(value[field] is False, f"readiness must set {field}=false")

    gate = value["gateFinalizedBlock"]
    require(isinstance(gate, dict) and set(gate) == {"number", "hash"}, "readiness gate block fields mismatch")
    require(isinstance(gate["number"], int) and not isinstance(gate["number"], bool) and gate["number"] >= 0, "invalid readiness gate block number")
    require(isinstance(gate["hash"], str) and bool(HASH256_RE.fullmatch(gate["hash"])), "invalid readiness gate block hash")
    parse_created_at(value["createdAtUtc"])

    return {
        "sha256": actual_sha256,
        "releaseId": value["releaseId"],
        "sourceCommit": value["sourceCommit"],
        "gateFinalizedBlock": gate,
        "resetMode": value["resetMode"],
        "economicGateMode": value["economicGateMode"],
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--readiness", required=True)
    parser.add_argument("--expected-sha256", required=True)
    args = parser.parse_args(argv)
    try:
        summary = validate_packet(Path(args.readiness), args.expected_sha256)
    except (OSError, ReadinessError) as exc:
        print(f"fresh-reset readiness rejected: {exc}", file=sys.stderr)
        return 2
    print(json.dumps(summary, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
