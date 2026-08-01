#!/usr/bin/env python3
"""Stage the exact chain/media final-backup subset for protected restore.

This module performs local file copies only.  It is loaded by the hash-pinned
pre-reset component adapter immediately before the protected restore helper is
invoked; it never opens a network connection or reads deployment credentials.
"""

from __future__ import annotations

import datetime as dt
import hashlib
import json
import os
import shutil
from pathlib import Path
from typing import Any, Mapping


STAGING_NAMES = {
    ("node", "node-data"): "node-data.tar.gz",
    ("node", "node-binary"): "node-binary",
    ("media", "media-state"): "media-state.tar.gz",
    ("media", "media-image-lock"): "media-image-lock.json",
    ("ipfs", "ipfs-data"): "ipfs-data.tar.gz",
    ("ipfs", "ipfs-staging"): "ipfs-staging.tar.gz",
    ("config", "node-env"): "node.env",
    ("config", "media-env"): "media.env",
    ("config", "economic-gates"): "backup-economic-gates.json",
    ("config", "chain-spec"): "chain-spec.json",
    ("service", "node-service"): "node-service.service",
    ("service", "media-service"): "media-service.json",
}
ENTRY_KEYS = {"group", "name", "path", "bytes", "sha256"}


class StagingError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise StagingError(message)


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def read_json(path: Path, label: str) -> dict[str, Any]:
    require(path.is_file() and not path.is_symlink(), f"{label} is unavailable")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise StagingError(f"invalid {label}") from exc
    require(isinstance(value, dict), f"{label} must be an object")
    return value


def canonical_bytes(value: Mapping[str, Any]) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")


def write_new(path: Path, payload: bytes, mode: int = 0o400) -> None:
    require(not os.path.lexists(path), f"refusing to overwrite restore staging: {path}")
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, mode)
    with os.fdopen(descriptor, "wb") as handle:
        handle.write(payload)
        handle.flush()
        os.fsync(handle.fileno())
    os.chmod(path, mode)


def stage_final_backup(context: Mapping[str, Any], destination: Path) -> None:
    required_context = {
        "plan",
        "planSha256",
        "manifestPath",
        "bundleRoot",
        "manifestSha256",
        "componentCommits",
    }
    require(set(context) == required_context, "restore staging context schema mismatch")
    plan = context["plan"]
    require(isinstance(plan, dict), "restore staging plan is unavailable")
    release_id = plan.get("releaseId")
    source_commit = plan.get("sourceCommit")
    require(isinstance(release_id, str) and release_id, "restore staging release is invalid")
    require(isinstance(source_commit, str) and len(source_commit) == 40, "restore staging source is invalid")
    component_commits = context["componentCommits"]
    require(
        isinstance(component_commits, dict)
        and set(component_commits) == {"chain", "media"}
        and component_commits.get("chain") == source_commit,
        "restore staging component sources mismatch",
    )

    manifest_path = Path(context["manifestPath"]).resolve()
    bundle_root = Path(context["bundleRoot"]).resolve()
    require(bundle_root.is_dir() and not bundle_root.is_symlink(), "backup bundle root is unavailable")
    require(
        manifest_path.is_file()
        and not manifest_path.is_symlink()
        and bundle_root in manifest_path.parents,
        "backup manifest escapes the bundle root",
    )
    manifest_sha256 = context["manifestSha256"]
    require(digest(manifest_path) == manifest_sha256, "backup manifest hash drifted")
    manifest = read_json(manifest_path, "final backup manifest")
    require(
        manifest.get("schemaVersion") == 1
        and manifest.get("kind") == "nexus-v2-private-alpha-backup"
        and manifest.get("releaseId") == release_id
        and manifest.get("sourceCommit") == source_commit,
        "backup manifest identity mismatch",
    )
    entries = manifest.get("artifacts")
    require(isinstance(entries, list), "backup artifact list is unavailable")
    indexed: dict[tuple[str, str], Mapping[str, Any]] = {}
    for entry in entries:
        require(isinstance(entry, dict) and set(entry) == ENTRY_KEYS, "backup artifact schema mismatch")
        key = (entry.get("group"), entry.get("name"))
        require(
            all(isinstance(item, str) and item for item in key)
            and key not in indexed,
            "backup artifact identity is invalid or duplicated",
        )
        indexed[key] = entry
    require(set(STAGING_NAMES) <= set(indexed), "backup restore subset is incomplete")

    destination = Path(destination)
    require(destination.is_absolute(), "restore staging destination must be absolute")
    require(not os.path.lexists(destination), "restore staging destination already exists")
    require(
        destination.parent.is_dir() and not destination.parent.is_symlink(),
        "restore staging parent is unavailable",
    )
    destination.mkdir(mode=0o700)
    staged_hashes: dict[str, str] = {}
    for key, output_name in STAGING_NAMES.items():
        entry = indexed[key]
        relative = entry["path"]
        require(
            isinstance(relative, str)
            and relative
            and not Path(relative).is_absolute()
            and ".." not in Path(relative).parts,
            f"invalid backup artifact path: {key}",
        )
        source = (bundle_root / relative).resolve()
        require(
            source.is_file()
            and not source.is_symlink()
            and bundle_root in source.parents,
            f"backup artifact is unavailable: {key}",
        )
        expected_size = entry["bytes"]
        expected_hash = entry["sha256"]
        require(
            isinstance(expected_size, int)
            and not isinstance(expected_size, bool)
            and expected_size >= 0
            and source.stat().st_size == expected_size,
            f"backup artifact byte count drifted: {key}",
        )
        require(
            isinstance(expected_hash, str)
            and len(expected_hash) == 64
            and digest(source) == expected_hash,
            f"backup artifact hash drifted: {key}",
        )
        target = destination / output_name
        require(target.parent == destination, "restore staging output escaped destination")
        descriptor = os.open(
            target,
            os.O_WRONLY | os.O_CREAT | os.O_EXCL,
            0o400,
        )
        with source.open("rb") as source_handle, os.fdopen(
            descriptor, "wb"
        ) as target_handle:
            shutil.copyfileobj(source_handle, target_handle, length=1024 * 1024)
            target_handle.flush()
            os.fsync(target_handle.fileno())
        os.chmod(target, 0o400)
        require(digest(target) == expected_hash, f"staged restore artifact drifted: {key}")
        staged_hashes[output_name] = expected_hash

    contract = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-chain-media-restore-staging",
        "releaseId": release_id,
        "sourceCommit": source_commit,
        "componentSourceCommits": dict(component_commits),
        "planSha256": context["planSha256"],
        "backupManifestSha256": manifest_sha256,
        "files": staged_hashes,
        "createdAtUtc": dt.datetime.now(dt.timezone.utc)
        .replace(microsecond=0)
        .strftime("%Y-%m-%dT%H:%M:%SZ"),
    }
    write_new(destination / "staging-contract.json", canonical_bytes(contract))
    directory = os.open(destination, os.O_RDONLY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)
