#!/usr/bin/env python3
"""Evidence-first Nexus V2 private-alpha release safety orchestration.

This tool does not contain a live Alpha reset or deploy implementation.
Restore, migration-completion, and rollback actions use separately supplied,
hash-pinned executables after the local safety contract has been validated.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import re
import stat
import subprocess
import sys
from pathlib import Path
from typing import Any, Iterable, Mapping, Sequence


REPO_ROOT = Path(__file__).resolve().parents[2]

REQUIRED_ARTIFACTS: dict[str, set[str]] = {
    "node": {
        "node-data",
        "node-binary",
        "runtime-v14-wasm",
        "runtime-v14-metadata",
        "runtime-v16-production-wasm",
        "runtime-v16-try-runtime-wasm",
        "tcg-storage-version-observation",
        "try-runtime-snapshot",
        "try-runtime-snapshot-proof",
    },
    "media": {"media-state", "media-image-lock"},
    "ipfs": {"ipfs-data", "ipfs-staging"},
    "config": {
        "authority-env",
        "acceptance-inventory",
        "node-env",
        "media-env",
        "indexer-env",
        "site-env",
        "chain-spec",
        "deployment-fingerprints",
        "economic-gates",
        "release-identifiers",
        "write-barrier-evidence",
    },
    "service": {
        "authority-service",
        "caddy-service",
        "indexer-service",
        "media-service",
        "node-service",
        "site-service",
    },
    "indexer": {"indexer-state", "indexer-checkpoint", "mongo-state"},
    "site": {"site-image-lock", "site-state"},
    "authority": {"authority-state"},
    "ingress": {"caddy-config", "caddy-state"},
}

CURRENT_ALPHA_COORDINATION_FILES = {
    "backup": "deploy/alpha/macmini2010/backup-alpha-state.sh",
    "finalFreeze": "scripts/nexus-v2-private-alpha/final_freeze.py",
    "nodeCandidate": "scripts/nexus-v2-private-alpha/node_candidate.py",
    "frozenSnapshotProof": "scripts/nexus-v2-private-alpha/frozen_snapshot_proof.py",
    "restore": "deploy/alpha/macmini2010/restore-alpha-state.sh",
    "resetNode": "deploy/alpha/macmini2010/reset-node.sh",
    "resetMedia": "deploy/alpha/macmini2010/reset-media.sh",
    "currentRuntimeRehearsal": "scripts/release/rehearse-runtime-upgrade.sh",
}

REQUIRED_ISOLATED_PORTS = {
    "nodeRpc",
    "nodeP2p",
    "media",
    "ipfsApi",
    "ipfsGateway",
    "indexer",
}

REQUIRED_RESTORE_CHECKS = {
    "backupHashesVerified",
    "nodeStateReadable",
    "nodeRpcHealthy",
    "mediaStateReadable",
    "mediaHealthHealthy",
    "ipfsRepoVerified",
    "ipfsGatewayHealthy",
    "configLoaded",
    "serviceDefinitionsLoaded",
    "indexerStateReadable",
    "indexerHealthHealthy",
    "crossServiceReadPathHealthy",
    "teardownComplete",
}

REQUIRED_MIGRATION_CHECKS = {
    "interruptedResumeSafe",
    "noCardLost",
    "noCardDuplicated",
    "noSilentReclassification",
    "ownershipIndexesMatch",
    "subjectIndexesMatch",
    "custodyDomainsMatch",
    "lifecycleQuiescent",
    "retiredEconomiesQuiescent",
    "v2SidecarPrefixesAbsent",
    "anomaliesAccounted",
    "nextCardIdMonotonic",
    "safeLegacyExitsPreserved",
    "v2WritesRemainPaused",
    "boundedBatchWeightRespected",
}

ACCEPTANCE_COUNT_FIELDS = {
    "cardsV2",
    "entitiesV2",
    "trainingPackCredits",
    "productionPackCredits",
    "pendingPackOpenings",
    "conversionCommitments",
    "reforgeCommitments",
    "productionMagicBalances",
    "trainingMagicBalances",
    "essenceBalances",
    "spellChargeBalances",
    "prismSpells",
    "activeV2Sessions",
    "acceptedProductionResults",
    "acceptedTrainingResults",
    "founderEntitlements",
    "rankedTeams",
    "playerAdvancementRecords",
    "packProgressRecords",
    "lifetimeCardsV2Created",
    "lifetimeEntitiesV2Created",
    "lifetimePackCreditsIssued",
    "lifetimePackOpeningsRequested",
    "lifetimeConversionsCommitted",
    "lifetimeReforgesCommitted",
    "lifetimeMagicAssetsCreated",
    "lifetimeV2SessionsAuthorized",
    "lifetimeV2ResultsAccepted",
    "lifetimeFounderEntitlementsIssued",
    "lifetimeRankedTeamsCreated",
    "lifetimeProgressionRecordsCreated",
}

MIGRATION_COUNT_FIELDS = {
    "legacyCardsBefore",
    "legacyCardsAfter",
    "cardsSeen",
    "ordinary",
    "nftWrapped",
    "knownEscrow",
    "anomalies",
    "nextCardId",
}

HEX_256_RE = re.compile(r"^0x[0-9a-fA-F]{64}$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
RELEASE_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
POST_V16_ECONOMIC_GATE_KIND = "nexus-v2-private-alpha-economic-gates"
PRE_V16_FRESH_RESET_GATE_KIND = (
    "nexus-v2-private-alpha-pre-v16-fresh-reset-gates"
)
POST_V16_GATE_MODE = "post-v16-disabled"
PRE_V16_FRESH_RESET_GATE_MODE = "pre-v16-fresh-reset-frozen"
PRE_V16_ABSENT_V2_PALLETS = [
    "EterraRandomness",
    "EterraCreatures",
    "EterraMagic",
    "EterraGameResults",
]
PRE_V16_ABSENT_V2_PALLET_INDICES = [35, 36, 37, 38]


class SafetyError(RuntimeError):
    """A release safety contract failed."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SafetyError(message)


def utc_now() -> str:
    return (
        dt.datetime.now(dt.timezone.utc)
        .replace(microsecond=0)
        .isoformat()
        .replace("+00:00", "Z")
    )


def parse_utc(value: str, label: str) -> dt.datetime:
    require(isinstance(value, str) and value, f"{label} must be an ISO-8601 string")
    try:
        parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise SafetyError(f"{label} is not valid ISO-8601") from exc
    require(parsed.tzinfo is not None, f"{label} must include a timezone")
    return parsed.astimezone(dt.timezone.utc)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def read_json(path: Path) -> dict[str, Any]:
    require(path.is_file(), f"JSON file not found: {path}")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise SafetyError(f"invalid JSON file: {path}") from exc
    require(isinstance(value, dict), f"JSON root must be an object: {path}")
    return value


def write_new_bytes(path: Path, value: bytes, mode: int = 0o600) -> None:
    require(not path.exists(), f"refusing to overwrite output: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, mode)
    with os.fdopen(descriptor, "wb") as handle:
        handle.write(value)


def write_new_json(path: Path, value: Mapping[str, Any]) -> None:
    encoded = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")
    write_new_bytes(path, encoded)


def ensure_release_id(value: str) -> str:
    require(bool(RELEASE_RE.fullmatch(value)), "invalid release ID")
    return value


def ensure_commit(value: str) -> str:
    require(bool(COMMIT_RE.fullmatch(value)), "source commit must be 40 lowercase hex characters")
    return value


def ensure_sha256(value: Any, label: str) -> str:
    require(isinstance(value, str) and bool(SHA256_RE.fullmatch(value)), f"invalid {label}")
    return value


def ensure_hash256(value: Any, label: str) -> str:
    require(isinstance(value, str) and bool(HEX_256_RE.fullmatch(value)), f"invalid {label}")
    return value.lower()


def ensure_regular_file(path: Path, label: str, executable: bool = False) -> Path:
    require(path.exists(), f"{label} not found: {path}")
    require(not path.is_symlink(), f"{label} must not be a symlink: {path}")
    require(path.is_file(), f"{label} must be a regular file: {path}")
    if executable:
        mode = path.stat().st_mode
        require(bool(mode & (stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)), f"{label} is not executable: {path}")
    return path.resolve()


def is_within(path: Path, root: Path) -> bool:
    try:
        path.resolve().relative_to(root.resolve())
        return True
    except ValueError:
        return False


def resolve_bundle_file(bundle_root: Path, relative: str, label: str) -> Path:
    require(isinstance(relative, str) and relative, f"{label} path is missing")
    relative_path = Path(relative)
    require(not relative_path.is_absolute(), f"{label} path must be relative")
    require(".." not in relative_path.parts, f"{label} path may not contain '..'")
    candidate = bundle_root / relative_path
    ensure_regular_file(candidate, label)
    require(is_within(candidate, bundle_root), f"{label} escapes bundle root")
    for parent in (candidate, *candidate.parents):
        if parent == bundle_root.parent:
            break
        require(not parent.is_symlink(), f"{label} traverses a symlink")
        if parent == bundle_root:
            break
    return candidate.resolve()


def artifact_key(group: str, name: str) -> tuple[str, str]:
    return group, name


def artifact_map(manifest: Mapping[str, Any]) -> dict[tuple[str, str], Mapping[str, Any]]:
    artifacts = manifest.get("artifacts")
    require(isinstance(artifacts, list), "artifact manifest artifacts must be an array")
    mapped: dict[tuple[str, str], Mapping[str, Any]] = {}
    for entry in artifacts:
        require(isinstance(entry, dict), "artifact entry must be an object")
        group = entry.get("group")
        name = entry.get("name")
        require(isinstance(group, str) and isinstance(name, str), "artifact group/name must be strings")
        key = artifact_key(group, name)
        require(key not in mapped, f"duplicate artifact role: {group}:{name}")
        mapped[key] = entry
    return mapped


def current_alpha_coordination() -> dict[str, Any]:
    scripts: list[dict[str, Any]] = []
    for role, relative in sorted(CURRENT_ALPHA_COORDINATION_FILES.items()):
        path = ensure_regular_file(REPO_ROOT / relative, f"current Alpha {role} script")
        scripts.append(
            {
                "role": role,
                "path": relative,
                "sha256": sha256_file(path),
            }
        )
    return {
        "mode": "hash-pinned-reference-only",
        "liveCommandsInvokedByThisTool": False,
        "scripts": scripts,
    }


def verify_backup_manifest(manifest_path: Path, bundle_root: Path) -> dict[str, Any]:
    bundle_root = bundle_root.resolve()
    require(bundle_root.is_dir(), f"bundle root not found: {bundle_root}")
    require(not bundle_root.is_symlink(), "bundle root must not be a symlink")
    manifest = read_json(manifest_path)
    require(manifest.get("schemaVersion") == 1, "unsupported backup manifest schema")
    require(manifest.get("kind") == "nexus-v2-private-alpha-backup", "backup manifest kind mismatch")
    release_id = ensure_release_id(str(manifest.get("releaseId", "")))
    source_commit = ensure_commit(str(manifest.get("sourceCommit", "")))
    parse_utc(str(manifest.get("createdAtUtc", "")), "backup createdAtUtc")

    mapped = artifact_map(manifest)
    expected_keys = {
        artifact_key(group, name)
        for group, names in REQUIRED_ARTIFACTS.items()
        for name in names
    }
    require(set(mapped) == expected_keys, "backup manifest artifact roles do not match the required closed set")

    for (group, name), entry in mapped.items():
        path = resolve_bundle_file(bundle_root, str(entry.get("path", "")), f"{group}:{name}")
        size = entry.get("bytes")
        require(isinstance(size, int) and not isinstance(size, bool) and size >= 0, f"invalid byte count for {group}:{name}")
        require(path.stat().st_size == size, f"artifact byte count mismatch: {group}:{name}")
        expected_hash = ensure_sha256(entry.get("sha256"), f"SHA-256 for {group}:{name}")
        require(sha256_file(path) == expected_hash, f"artifact SHA-256 mismatch: {group}:{name}")

    coordination = manifest.get("currentAlphaCoordination")
    require(isinstance(coordination, dict), "current Alpha coordination block is missing")
    require(coordination.get("mode") == "hash-pinned-reference-only", "current Alpha coordination mode mismatch")
    require(
        coordination.get("liveCommandsInvokedByThisTool") is False,
        "backup manifest must record that this tool invoked no live command",
    )
    expected_coordination = current_alpha_coordination()["scripts"]
    require(coordination.get("scripts") == expected_coordination, "current Alpha script hashes drifted from the manifest")

    return {
        "manifest": manifest,
        "releaseId": release_id,
        "sourceCommit": source_commit,
        "sha256": sha256_file(manifest_path),
        "artifacts": mapped,
    }


def find_artifact(verified: Mapping[str, Any], bundle_root: Path, group: str, name: str) -> Path:
    entry = verified["artifacts"].get((group, name))
    require(isinstance(entry, dict), f"required artifact not found: {group}:{name}")
    return resolve_bundle_file(bundle_root.resolve(), str(entry["path"]), f"{group}:{name}")


def parse_artifact_argument(value: str) -> tuple[str, str, str]:
    parts = value.split(":", 2)
    require(len(parts) == 3 and all(parts), "--artifact must be GROUP:NAME:RELATIVE_PATH")
    return parts[0], parts[1], parts[2]


def command_backup_manifest(args: argparse.Namespace) -> None:
    release_id = ensure_release_id(args.release_id)
    source_commit = ensure_commit(args.source_commit)
    bundle_root = Path(args.bundle_root).resolve()
    require(bundle_root.is_dir(), f"bundle root not found: {bundle_root}")
    require(not bundle_root.is_symlink(), "bundle root must not be a symlink")

    supplied: dict[tuple[str, str], str] = {}
    for raw in args.artifact:
        group, name, relative = parse_artifact_argument(raw)
        key = artifact_key(group, name)
        require(key not in supplied, f"duplicate --artifact role: {group}:{name}")
        supplied[key] = relative

    expected = {
        artifact_key(group, name)
        for group, names in REQUIRED_ARTIFACTS.items()
        for name in names
    }
    require(set(supplied) == expected, "--artifact roles must match the required closed set")

    artifacts: list[dict[str, Any]] = []
    for (group, name), relative in sorted(supplied.items()):
        path = resolve_bundle_file(bundle_root, relative, f"{group}:{name}")
        artifacts.append(
            {
                "group": group,
                "name": name,
                "path": Path(relative).as_posix(),
                "bytes": path.stat().st_size,
                "sha256": sha256_file(path),
            }
        )

    created_at = args.created_at or utc_now()
    parse_utc(created_at, "--created-at")
    manifest = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-backup",
        "releaseId": release_id,
        "sourceCommit": source_commit,
        "createdAtUtc": created_at,
        "artifactHashAlgorithm": "sha256",
        "artifacts": artifacts,
        "currentAlphaCoordination": current_alpha_coordination(),
        "containsPrivateConfiguration": True,
        "publicReleaseAllowed": False,
    }
    output = Path(args.output)
    write_new_json(output, manifest)
    print(f"backup manifest created: {output} sha256={sha256_file(output)}")


def command_verify_backup(args: argparse.Namespace) -> None:
    verified = verify_backup_manifest(Path(args.manifest), Path(args.bundle_root))
    print(
        "backup manifest verified: "
        f"release={verified['releaseId']} sha256={verified['sha256']}"
    )


def safe_isolation_root(path: Path) -> Path:
    resolved = path.resolve()
    forbidden = {
        Path("/"),
        Path("/opt"),
        Path("/var"),
        Path("/var/lib"),
        Path.home().resolve(),
        REPO_ROOT.resolve(),
        REPO_ROOT.parent.resolve(),
    }
    require(resolved not in forbidden, f"unsafe isolation root: {resolved}")
    require(resolved.name.startswith("nexus-v2-isolated-restore-"), "isolation root name must start with nexus-v2-isolated-restore-")
    return resolved


def command_init_isolation_root(args: argparse.Namespace) -> None:
    release_id = ensure_release_id(args.release_id)
    root = safe_isolation_root(Path(args.root))
    if root.exists():
        require(root.is_dir(), "isolation root exists and is not a directory")
        require(not any(root.iterdir()), "existing isolation root must be empty")
    else:
        root.mkdir(parents=False)
    sentinel = {
        "schemaVersion": 1,
        "purpose": "nexus-v2-private-alpha-isolated-restore",
        "releaseId": release_id,
        "root": str(root),
        "liveAlphaWritesAllowed": False,
        "createdAtUtc": args.created_at or utc_now(),
    }
    parse_utc(sentinel["createdAtUtc"], "--created-at")
    write_new_json(root / ".nexus-v2-isolated-restore.json", sentinel)
    print(f"isolated restore root initialized: {root}")


def verify_isolation_root(path: Path, release_id: str) -> Path:
    root = safe_isolation_root(path)
    require(root.is_dir(), f"isolation root not found: {root}")
    sentinel_path = root / ".nexus-v2-isolated-restore.json"
    sentinel = read_json(sentinel_path)
    require(sentinel.get("schemaVersion") == 1, "isolation sentinel schema mismatch")
    require(sentinel.get("purpose") == "nexus-v2-private-alpha-isolated-restore", "isolation sentinel purpose mismatch")
    require(sentinel.get("releaseId") == release_id, "isolation sentinel release mismatch")
    require(sentinel.get("root") == str(root), "isolation sentinel root mismatch")
    require(sentinel.get("liveAlphaWritesAllowed") is False, "isolation sentinel permits live Alpha writes")
    parse_utc(str(sentinel.get("createdAtUtc", "")), "isolation sentinel createdAtUtc")
    return root


def validate_ports(path: Path) -> dict[str, Any]:
    value = read_json(path)
    require(value.get("schemaVersion") == 1, "port plan schema mismatch")
    bind_host = value.get("bindHost")
    require(bind_host in {"127.0.0.1", "::1"}, "isolated services must bind to loopback")
    ports = value.get("ports")
    live_ports = value.get("livePorts")
    require(isinstance(ports, dict), "port plan ports must be an object")
    require(isinstance(live_ports, dict) and live_ports, "port plan livePorts must be a non-empty object")
    require(set(ports) == REQUIRED_ISOLATED_PORTS, "isolated port names do not match the required closed set")
    all_isolated: list[int] = []
    for name, port in ports.items():
        require(isinstance(port, int) and not isinstance(port, bool) and 1024 <= port <= 65535, f"invalid isolated port: {name}")
        all_isolated.append(port)
    require(len(set(all_isolated)) == len(all_isolated), "isolated ports must be unique")
    all_live: list[int] = []
    for name, port in live_ports.items():
        require(isinstance(name, str) and name, "live port name must be non-empty")
        require(isinstance(port, int) and not isinstance(port, bool) and 1 <= port <= 65535, f"invalid live port: {name}")
        all_live.append(port)
    require(set(all_isolated).isdisjoint(all_live), "isolated ports overlap declared live ports")
    return value


def validate_external_driver(path: Path, label: str) -> Path:
    driver = ensure_regular_file(path, label, executable=True)
    deploy_root = REPO_ROOT / "deploy"
    require(not is_within(driver, deploy_root), f"{label} may not be an existing deploy/restore/reset helper")
    return driver


def run_and_capture(
    command: Sequence[str],
    log_path: Path,
    *,
    environment: Mapping[str, str] | None = None,
) -> subprocess.CompletedProcess[bytes]:
    require(not log_path.exists(), f"refusing to overwrite log: {log_path}")
    completed = subprocess.run(
        list(command),
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
        env=environment,
    )
    write_new_bytes(log_path, completed.stdout)
    return completed


def strict_true_checks(value: Any, required: set[str], label: str) -> None:
    require(isinstance(value, dict), f"{label} checks must be an object")
    require(set(value) == required, f"{label} checks do not match the required closed set")
    for name, result in value.items():
        require(result is True, f"{label} check did not pass: {name}")


def validate_restore_result(
    result: Mapping[str, Any],
    verified: Mapping[str, Any],
    ports: Mapping[str, Any],
) -> None:
    require(result.get("schemaVersion") == 1, "restore result schema mismatch")
    require(result.get("kind") == "nexus-v2-isolated-restore-result", "restore result kind mismatch")
    require(result.get("releaseId") == verified["releaseId"], "restore result release mismatch")
    require(result.get("sourceCommit") == verified["sourceCommit"], "restore result source commit mismatch")
    require(result.get("mode") == "isolated", "restore result mode must be isolated")
    require(result.get("bindHost") == ports["bindHost"], "restore result bind host mismatch")
    require(result.get("ports") == ports["ports"], "restore result ports mismatch")
    require(result.get("liveAlphaTouched") is False, "restore result reports live Alpha access")
    require(result.get("backupManifestSha256") == verified["sha256"], "restore result backup manifest hash mismatch")
    require(
        result.get("restoredArtifactGroups") == sorted(REQUIRED_ARTIFACTS),
        "restore result artifact groups mismatch",
    )
    strict_true_checks(result.get("checks"), REQUIRED_RESTORE_CHECKS, "restore")


def command_rehearse_restore(args: argparse.Namespace) -> None:
    manifest_path = Path(args.manifest).resolve()
    bundle_root = Path(args.bundle_root).resolve()
    verified = verify_backup_manifest(manifest_path, bundle_root)
    isolation_root = verify_isolation_root(Path(args.isolation_root), verified["releaseId"])
    ports_path = Path(args.ports).resolve()
    ports = validate_ports(ports_path)
    driver = validate_external_driver(Path(args.driver), "isolated restore driver")
    output = Path(args.evidence)
    require(not output.exists(), f"refusing to overwrite evidence: {output}")
    log_path = Path(f"{output}.restore.log")
    result_path = isolation_root / "restore-result.json"
    require(not result_path.exists(), f"restore result already exists: {result_path}")

    command = [
        str(driver),
        "--manifest",
        str(manifest_path),
        "--bundle-root",
        str(bundle_root),
        "--isolation-root",
        str(isolation_root),
        "--bind-host",
        str(ports["bindHost"]),
        "--ports-json",
        str(ports_path),
        "--result",
        str(result_path),
    ]
    completed = run_and_capture(command, log_path)
    require(completed.returncode == 0, f"isolated restore driver failed; see {log_path}")
    result = read_json(result_path)
    validate_restore_result(result, verified, ports)

    evidence = {
        "schemaVersion": 1,
        "kind": "nexus-v2-isolated-restore-evidence",
        "releaseId": verified["releaseId"],
        "sourceCommit": verified["sourceCommit"],
        "backupManifestSha256": verified["sha256"],
        "restoreDriverSha256": sha256_file(driver),
        "portsPlanSha256": sha256_file(ports_path),
        "restoreLogSha256": sha256_file(log_path),
        "restoreResultSha256": sha256_file(result_path),
        "isolatedRoot": str(isolation_root),
        "bindHost": ports["bindHost"],
        "ports": ports["ports"],
        "result": "passed",
        "completedAtUtc": utc_now(),
        "liveAlphaTouched": False,
    }
    write_new_json(output, evidence)
    print(f"isolated restore rehearsal passed: {output}")


def validate_migration_result(
    result: Mapping[str, Any],
    verified: Mapping[str, Any],
    snapshot_hash: str,
    runtime_hash: str,
    try_log_hash: str,
) -> None:
    require(result.get("schemaVersion") == 1, "migration result schema mismatch")
    require(result.get("kind") == "nexus-v2-v14-v16-migration-result", "migration result kind mismatch")
    require(result.get("releaseId") == verified["releaseId"], "migration result release mismatch")
    require(result.get("sourceCommit") == verified["sourceCommit"], "migration result source commit mismatch")
    require(result.get("snapshotSha256") == snapshot_hash, "migration snapshot hash mismatch")
    require(result.get("runtimeWasmSha256") == runtime_hash, "migration runtime hash mismatch")
    require(result.get("tryRuntimeLogSha256") == try_log_hash, "migration log hash mismatch")
    require(result.get("fromStorageVersion") == 14, "migration must start at storage version 14")
    require(result.get("toStorageVersion") == 16, "migration must finish at storage version 16")
    require(result.get("migrationPhase") == "Completed", "migration did not complete")
    require(result.get("legacyCreationSealed") is True, "legacy creation is not sealed")
    require(result.get("legacyWritesPaused") is False, "legacy safe-exit writes remained paused after completion")
    require(
        result.get("v2Features")
        == {
            "Conversion": False,
            "Packs": False,
            "Ranked": False,
            "MythicalAscension": False,
        },
        "migration result must keep every V2 feature paused",
    )
    strict_true_checks(result.get("checks"), REQUIRED_MIGRATION_CHECKS, "migration")

    counts = result.get("counts")
    require(isinstance(counts, dict), "migration counts must be an object")
    require(set(counts) == MIGRATION_COUNT_FIELDS | {"maxCardIdSeen"}, "migration counts do not match the required closed set")
    for name in MIGRATION_COUNT_FIELDS:
        value = counts[name]
        require(isinstance(value, int) and not isinstance(value, bool) and value >= 0, f"invalid migration count: {name}")
    max_seen = counts["maxCardIdSeen"]
    require(max_seen is None or (isinstance(max_seen, int) and not isinstance(max_seen, bool) and max_seen >= 0), "invalid maxCardIdSeen")
    require(counts["legacyCardsBefore"] == counts["legacyCardsAfter"], "legacy card count changed during migration")
    require(counts["cardsSeen"] == counts["legacyCardsBefore"], "migration did not inspect every legacy card")
    classified = counts["ordinary"] + counts["nftWrapped"] + counts["knownEscrow"] + counts["anomalies"]
    require(classified == counts["cardsSeen"], "migration classification counts do not reconcile")
    if max_seen is not None:
        require(counts["nextCardId"] > max_seen, "NextCardId is not greater than maxCardIdSeen")


def command_rehearse_migration(args: argparse.Namespace) -> None:
    manifest_path = Path(args.manifest).resolve()
    bundle_root = Path(args.bundle_root).resolve()
    verified = verify_backup_manifest(manifest_path, bundle_root)
    try_runtime = ensure_regular_file(Path(args.try_runtime_bin), "try-runtime binary", executable=True)
    verifier = validate_external_driver(Path(args.migration_verifier), "migration completion verifier")
    expected_try_hash = ensure_sha256(args.try_runtime_sha256, "try-runtime SHA-256")
    expected_verifier_hash = ensure_sha256(args.migration_verifier_sha256, "migration verifier SHA-256")
    require(sha256_file(try_runtime) == expected_try_hash, "try-runtime binary hash mismatch")
    require(sha256_file(verifier) == expected_verifier_hash, "migration verifier hash mismatch")
    require(bool(re.fullmatch(r"[0-9a-f]{7,40}", args.try_runtime_revision)), "invalid try-runtime revision")
    require(
        isinstance(args.migration_blocks, int) and 1 <= args.migration_blocks <= 1_000_000,
        "migration blocks must be in 1..1000000",
    )

    snapshot = find_artifact(verified, bundle_root, "node", "try-runtime-snapshot")
    runtime = find_artifact(verified, bundle_root, "node", "runtime-v16-try-runtime-wasm")
    snapshot_hash = sha256_file(snapshot)
    runtime_hash = sha256_file(runtime)
    output = Path(args.evidence)
    require(not output.exists(), f"refusing to overwrite evidence: {output}")
    try_log = Path(f"{output}.try-runtime.log")
    verifier_log = Path(f"{output}.migration-verifier.log")
    result_path = Path(f"{output}.migration-result.json")
    require(not result_path.exists(), f"refusing to overwrite migration result: {result_path}")

    version_result = subprocess.run(
        [str(try_runtime), "--version"],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
        text=True,
    )
    require(version_result.returncode == 0, "try-runtime --version failed")
    try_version = version_result.stdout.strip()
    require(try_version, "try-runtime --version returned an empty value")

    try_command = [
        str(try_runtime),
        "--runtime",
        str(runtime),
        "fast-forward",
        "--n-blocks",
        str(args.migration_blocks),
        "--blocktime",
        "6000",
        "--try-state",
        "EterraTCG",
        "--run-migrations",
        "snap",
        "-p",
        str(snapshot),
    ]
    try_environment = dict(os.environ)
    existing_log_filter = try_environment.get("RUST_LOG", "").strip().strip(",")
    required_log_filter = "runtime::eterra_tcg=info,try_runtime_core=info"
    try_environment["RUST_LOG"] = (
        f"{existing_log_filter},{required_log_filter}"
        if existing_log_filter
        else required_log_filter
    )
    completed = run_and_capture(
        try_command,
        try_log,
        environment=try_environment,
    )
    require(completed.returncode == 0, f"try-runtime V14-to-V16 rehearsal failed; see {try_log}")
    require(
        "ETERRA_V16_MIGRATION_AWAITING_VERIFICATION" in try_log.read_text(encoding="utf-8"),
        "try-runtime fast-forward did not reach the V16 independent-verification gate",
    )
    try_log_hash = sha256_file(try_log)

    verifier_command = [
        str(verifier),
        "--manifest",
        str(manifest_path),
        "--snapshot",
        str(snapshot),
        "--runtime-wasm",
        str(runtime),
        "--try-runtime-log",
        str(try_log),
        "--result",
        str(result_path),
    ]
    verified_run = run_and_capture(verifier_command, verifier_log)
    require(verified_run.returncode == 0, f"migration completion verifier failed; see {verifier_log}")
    result = read_json(result_path)
    validate_migration_result(result, verified, snapshot_hash, runtime_hash, try_log_hash)

    evidence = {
        "schemaVersion": 1,
        "kind": "nexus-v2-v14-v16-migration-evidence",
        "releaseId": verified["releaseId"],
        "sourceCommit": verified["sourceCommit"],
        "backupManifestSha256": verified["sha256"],
        "fromStorageVersion": 14,
        "toStorageVersion": 16,
        "snapshotSha256": snapshot_hash,
        "runtimeWasmSha256": runtime_hash,
        "tryRuntimeRevision": args.try_runtime_revision,
        "tryRuntimeBinarySha256": expected_try_hash,
        "tryRuntimeVersion": try_version,
        "tryRuntimeFastForwardBlocks": args.migration_blocks,
        "tryRuntimeLogSha256": try_log_hash,
        "migrationVerifierSha256": expected_verifier_hash,
        "migrationVerifierLogSha256": sha256_file(verifier_log),
        "migrationResultSha256": sha256_file(result_path),
        "result": "passed",
        "completedAtUtc": utc_now(),
        "liveRpcUsed": False,
        "extrinsicSubmitted": False,
    }
    write_new_json(output, evidence)
    print(f"V14-to-V16 copied-state rehearsal passed: {output}")


def finalized_block(value: Any, label: str) -> tuple[int, str]:
    require(isinstance(value, dict), f"{label} finalized block must be an object")
    require(set(value) == {"number", "hash"}, f"{label} finalized block fields mismatch")
    number = value["number"]
    require(isinstance(number, int) and not isinstance(number, bool) and number >= 0, f"invalid {label} block number")
    block_hash = ensure_hash256(value["hash"], f"{label} block hash")
    return number, block_hash


def require_path_value(root: Mapping[str, Any], path: Sequence[str], expected: Any) -> None:
    value: Any = root
    dotted = ".".join(path)
    for component in path:
        require(isinstance(value, dict) and component in value, f"economic gate is missing {dotted}")
        value = value[component]
    require(type(value) is type(expected) and value == expected, f"economic gate must set {dotted}={expected!r}")


def require_all_false(value: Any, label: str) -> None:
    require(isinstance(value, dict), f"{label} must be an object")
    for name, item in value.items():
        require(isinstance(name, str) and name, f"{label} contains an invalid key")
        if isinstance(item, dict):
            require_all_false(item, f"{label}.{name}")
        else:
            require(item is False, f"{label}.{name} must be false")


def require_exact_keys(value: Any, expected: set[str], label: str) -> Mapping[str, Any]:
    require(isinstance(value, dict), f"{label} must be an object")
    require(set(value) == expected, f"{label} fields do not match the required closed set")
    return value


def validate_post_v16_economic_gates(
    path: Path,
    release_id: str | None = None,
    source_commit: str | None = None,
) -> dict[str, Any]:
    gates = read_json(path)
    require_exact_keys(
        gates,
        {
            "schemaVersion",
            "kind",
            "releaseId",
            "sourceCommit",
            "observedAtFinalizedBlock",
            "tcg",
            "randomness",
            "gameResults",
            "issuance",
            "reforge",
            "magic",
            "legacyEconomy",
            "arcadeTickets",
            "additionalEconomicFlags",
        },
        "economic gates",
    )
    require(gates.get("schemaVersion") == 1, "economic gate schema mismatch")
    require(gates.get("kind") == POST_V16_ECONOMIC_GATE_KIND, "economic gate kind mismatch")
    gate_release = ensure_release_id(str(gates.get("releaseId", "")))
    gate_commit = ensure_commit(str(gates.get("sourceCommit", "")))
    if release_id is not None:
        require(gate_release == release_id, "economic gate release mismatch")
    if source_commit is not None:
        require(gate_commit == source_commit, "economic gate source commit mismatch")
    block_number, block_hash = finalized_block(gates.get("observedAtFinalizedBlock"), "economic gate")
    require_exact_keys(gates["tcg"], {"features", "legacyCreationSealed"}, "tcg gates")
    require_exact_keys(
        gates["tcg"]["features"],
        {"Packs", "Conversion", "Ranked", "MythicalAscension"},
        "tcg feature gates",
    )
    require_exact_keys(
        gates["randomness"],
        {
            "mode",
            "privateAlphaSeedRecorded",
            "cryptographyReviewApproved",
            "drandQuicknetEnabled",
            "productionEconomicUseAllowed",
        },
        "randomness gates",
    )
    require_exact_keys(
        gates["gameResults"],
        {"activeProductionPolicyCount", "allAlphaPoliciesPracticeOnlyOrValuelessTraining"},
        "game-results gates",
    )
    require_exact_keys(
        gates["issuance"],
        {"trainingPackCreditRejectsProduction", "paidV2IssuanceCallAvailable"},
        "issuance gates",
    )
    require_exact_keys(gates["reforge"], {"dispatchableAvailable"}, "reforge gates")
    require_exact_keys(
        gates["magic"],
        {"seedTrainingOnly", "productionTransferEnabled"},
        "magic gates",
    )
    require_exact_keys(
        gates["legacyEconomy"],
        {
            "marketplaceEnabled",
            "purchaseEnabled",
            "faucetEnabled",
            "economicWritesEnabled",
        },
        "legacy economy gates",
    )
    require_exact_keys(
        gates["arcadeTickets"],
        {
            "earningEnabled",
            "transferEnabled",
            "redemptionEnabled",
            "randomVendingEnabled",
            "featuredVendingEnabled",
        },
        "Arcade Ticket gates",
    )

    expected_values = {
        ("tcg", "features", "Packs"): False,
        ("tcg", "features", "Conversion"): False,
        ("tcg", "features", "Ranked"): False,
        ("tcg", "features", "MythicalAscension"): False,
        ("tcg", "legacyCreationSealed"): True,
        ("randomness", "cryptographyReviewApproved"): False,
        ("randomness", "drandQuicknetEnabled"): False,
        ("randomness", "productionEconomicUseAllowed"): False,
        ("gameResults", "activeProductionPolicyCount"): 0,
        ("gameResults", "allAlphaPoliciesPracticeOnlyOrValuelessTraining"): True,
        ("issuance", "trainingPackCreditRejectsProduction"): True,
        ("issuance", "paidV2IssuanceCallAvailable"): False,
        ("reforge", "dispatchableAvailable"): False,
        ("magic", "seedTrainingOnly"): True,
        ("magic", "productionTransferEnabled"): False,
        ("legacyEconomy", "marketplaceEnabled"): False,
        ("legacyEconomy", "purchaseEnabled"): False,
        ("legacyEconomy", "faucetEnabled"): False,
        ("legacyEconomy", "economicWritesEnabled"): False,
        ("arcadeTickets", "earningEnabled"): False,
        ("arcadeTickets", "transferEnabled"): False,
        ("arcadeTickets", "redemptionEnabled"): False,
        ("arcadeTickets", "randomVendingEnabled"): False,
        ("arcadeTickets", "featuredVendingEnabled"): False,
    }
    for key_path, expected in expected_values.items():
        require_path_value(gates, key_path, expected)

    randomness = gates.get("randomness")
    require(isinstance(randomness, dict), "randomness gate must be an object")
    mode = randomness.get("mode")
    require(mode in {"Disabled", "DeterministicPrivateAlpha"}, "randomness mode is unsafe for private Alpha")
    seed_recorded = randomness.get("privateAlphaSeedRecorded")
    require(isinstance(seed_recorded, bool), "randomness.privateAlphaSeedRecorded must be boolean")
    if mode == "DeterministicPrivateAlpha":
        require(seed_recorded is True, "deterministic private-alpha randomness requires recorded seed evidence")

    require_all_false(gates.get("additionalEconomicFlags"), "additionalEconomicFlags")
    return {
        "value": gates,
        "mode": POST_V16_GATE_MODE,
        "releaseId": gate_release,
        "sourceCommit": gate_commit,
        "blockNumber": block_number,
        "blockHash": block_hash,
        "sha256": sha256_file(path),
    }


def validate_pre_v16_fresh_reset_gates(
    path: Path,
    release_id: str | None = None,
    source_commit: str | None = None,
) -> dict[str, Any]:
    gates = read_json(path)
    require_exact_keys(
        gates,
        {
            "schemaVersion",
            "kind",
            "releaseId",
            "sourceCommit",
            "observedAtFinalizedBlock",
            "operationScope",
            "sourceRuntime",
            "v2StructuralAbsence",
            "knownLegacyEconomicSurfaces",
            "legacyWriteBarrier",
            "externalReviewFlags",
            "additionalEconomicFlags",
        },
        "pre-V16 fresh-reset gates",
    )
    require(gates.get("schemaVersion") == 1, "pre-V16 fresh-reset gate schema mismatch")
    require(
        gates.get("kind") == PRE_V16_FRESH_RESET_GATE_KIND,
        "pre-V16 fresh-reset gate kind mismatch",
    )
    gate_release = ensure_release_id(str(gates.get("releaseId", "")))
    gate_commit = ensure_commit(str(gates.get("sourceCommit", "")))
    if release_id is not None:
        require(gate_release == release_id, "pre-V16 fresh-reset gate release mismatch")
    if source_commit is not None:
        require(gate_commit == source_commit, "pre-V16 fresh-reset gate source commit mismatch")
    block_number, block_hash = finalized_block(
        gates.get("observedAtFinalizedBlock"),
        "pre-V16 fresh-reset gate",
    )

    operation_scope = require_exact_keys(
        gates["operationScope"],
        {
            "freshGenesisReplacementOnly",
            "inPlaceRuntimeUpgradeAllowed",
            "v2ActivationAllowed",
            "paidOrPublicActivationAllowed",
        },
        "pre-V16 operation scope",
    )
    expected_operation_scope = {
        "freshGenesisReplacementOnly": True,
        "inPlaceRuntimeUpgradeAllowed": False,
        "v2ActivationAllowed": False,
        "paidOrPublicActivationAllowed": False,
    }
    require(
        operation_scope == expected_operation_scope,
        "pre-V16 gates authorize only a fresh, economically disabled genesis replacement",
    )

    source_runtime = require_exact_keys(
        gates["sourceRuntime"],
        {
            "deployedSourceCommit",
            "specVersion",
            "metadataVersion",
            "tcgPalletIndex",
            "tcgStorageVersion",
            "flowPalletIndex",
            "runtimeV14WasmSha256",
            "runtimeMetadataScaleSha256",
            "tcgStorageVersionObservationSha256",
        },
        "pre-V16 source runtime",
    )
    deployed_source_commit = ensure_commit(
        str(source_runtime.get("deployedSourceCommit", ""))
    )
    require(source_runtime.get("specVersion") == 1, "pre-V16 source runtime must be spec 1")
    require(
        source_runtime.get("metadataVersion") == 14,
        "pre-V16 source runtime metadata must be V14",
    )
    require(
        source_runtime.get("tcgPalletIndex") == 9,
        "pre-V16 source runtime must retain EterraTCG index 9",
    )
    require(
        source_runtime.get("tcgStorageVersion") == 14,
        "pre-V16 source runtime TCG storage must be V14",
    )
    require(
        source_runtime.get("flowPalletIndex") == 29,
        "pre-V16 source runtime must retain EterraFlow index 29",
    )
    runtime_v14_hash = ensure_sha256(
        source_runtime.get("runtimeV14WasmSha256"),
        "pre-V16 runtime V14 Wasm SHA-256",
    )
    metadata_hash = ensure_sha256(
        source_runtime.get("runtimeMetadataScaleSha256"),
        "pre-V16 runtime metadata SHA-256",
    )
    observation_hash = ensure_sha256(
        source_runtime.get("tcgStorageVersionObservationSha256"),
        "pre-V16 TCG storage-version observation SHA-256",
    )

    v2_absence = require_exact_keys(
        gates["v2StructuralAbsence"],
        {
            "absentPallets",
            "absentPalletIndices",
            "tcgV2StoragePresent",
            "tcgV2DispatchablesPresent",
            "v2EventsPresent",
        },
        "pre-V16 V2 structural absence",
    )
    require(
        v2_absence.get("absentPallets") == PRE_V16_ABSENT_V2_PALLETS,
        "pre-V16 V2 sidecar pallet absence list mismatch",
    )
    require(
        v2_absence.get("absentPalletIndices") == PRE_V16_ABSENT_V2_PALLET_INDICES,
        "pre-V16 V2 sidecar pallet-index absence list mismatch",
    )
    for field in ("tcgV2StoragePresent", "tcgV2DispatchablesPresent", "v2EventsPresent"):
        require(
            v2_absence.get(field) is False,
            f"pre-V16 V2 structural absence must set {field}=false",
        )

    known_surfaces = require_exact_keys(
        gates["knownLegacyEconomicSurfaces"],
        {
            "tcgPaidMintDispatchablesPresent",
            "tcgMarketplaceDispatchablesPresent",
            "faucetDispatchablePresent",
            "economyDispatchablesPresent",
            "arcadePayContinueDispatchablePresent",
            "reachableThroughWriteIngress",
        },
        "pre-V16 known legacy economic surfaces",
    )
    for field in (
        "tcgPaidMintDispatchablesPresent",
        "tcgMarketplaceDispatchablesPresent",
        "faucetDispatchablePresent",
        "economyDispatchablesPresent",
        "arcadePayContinueDispatchablePresent",
    ):
        require(
            known_surfaces.get(field) is True,
            f"pre-V16 gate must truthfully acknowledge {field}=true",
        )
    require(
        known_surfaces.get("reachableThroughWriteIngress") is False,
        "pre-V16 legacy economic surfaces must be unreachable through write ingress",
    )

    write_barrier = require_exact_keys(
        gates["legacyWriteBarrier"],
        {
            "mode",
            "nodeServiceStopped",
            "authorityServiceStopped",
            "publicRpcWriteIngressStopped",
            "p2pIngressStopped",
            "blockProductionStopped",
            "offlineFinalizedHeadMatchesGateBlock",
            "inventoryCapturedAfterWriteStop",
            "stoppedAtUtc",
            "stabilityWindowSeconds",
            "writeBarrierEvidenceSha256",
        },
        "pre-V16 legacy write barrier",
    )
    require(
        write_barrier.get("mode") == "AllIngressStopped",
        "pre-V16 legacy write barrier must be AllIngressStopped",
    )
    for field in (
        "nodeServiceStopped",
        "authorityServiceStopped",
        "publicRpcWriteIngressStopped",
        "p2pIngressStopped",
        "blockProductionStopped",
        "offlineFinalizedHeadMatchesGateBlock",
        "inventoryCapturedAfterWriteStop",
    ):
        require(
            write_barrier.get(field) is True,
            f"pre-V16 legacy write barrier must set {field}=true",
        )
    stopped_at = parse_utc(
        str(write_barrier.get("stoppedAtUtc", "")),
        "pre-V16 write barrier stoppedAtUtc",
    )
    stability_window = write_barrier.get("stabilityWindowSeconds")
    require(
        isinstance(stability_window, int)
        and not isinstance(stability_window, bool)
        and stability_window >= 30,
        "pre-V16 write barrier stability window must be at least 30 seconds",
    )
    write_barrier_hash = ensure_sha256(
        write_barrier.get("writeBarrierEvidenceSha256"),
        "pre-V16 write-barrier evidence SHA-256",
    )

    external_reviews = require_exact_keys(
        gates["externalReviewFlags"],
        {
            "cryptographyApproved",
            "paidFeaturesApproved",
            "publicProductionApproved",
        },
        "pre-V16 external-review flags",
    )
    require_all_false(external_reviews, "pre-V16 externalReviewFlags")
    require_all_false(gates.get("additionalEconomicFlags"), "additionalEconomicFlags")
    return {
        "value": gates,
        "mode": PRE_V16_FRESH_RESET_GATE_MODE,
        "releaseId": gate_release,
        "sourceCommit": gate_commit,
        "deployedSourceCommit": deployed_source_commit,
        "blockNumber": block_number,
        "blockHash": block_hash,
        "runtimeV14WasmSha256": runtime_v14_hash,
        "runtimeMetadataScaleSha256": metadata_hash,
        "tcgStorageVersionObservationSha256": observation_hash,
        "writeBarrierEvidenceSha256": write_barrier_hash,
        "writeBarrierStoppedAtUtc": stopped_at.isoformat(),
        "sha256": sha256_file(path),
    }


def validate_economic_gates(
    path: Path,
    release_id: str | None = None,
    source_commit: str | None = None,
    *,
    allow_pre_v16_fresh_reset: bool = False,
) -> dict[str, Any]:
    kind = read_json(path).get("kind")
    if kind == POST_V16_ECONOMIC_GATE_KIND:
        return validate_post_v16_economic_gates(path, release_id, source_commit)
    if kind == PRE_V16_FRESH_RESET_GATE_KIND and allow_pre_v16_fresh_reset:
        return validate_pre_v16_fresh_reset_gates(path, release_id, source_commit)
    if kind == PRE_V16_FRESH_RESET_GATE_KIND:
        raise SafetyError(
            "pre-V16 gates are fresh-reset-only and cannot authorize rollback or in-place activation"
        )
    raise SafetyError("economic gate kind mismatch")


def validate_acceptance_inventory(
    path: Path,
    release_id: str | None = None,
    source_commit: str | None = None,
) -> dict[str, Any]:
    inventory = read_json(path)
    require_exact_keys(
        inventory,
        {
            "schemaVersion",
            "kind",
            "releaseId",
            "sourceCommit",
            "observedAtFinalizedBlock",
            "counts",
        },
        "acceptance inventory",
    )
    require(inventory.get("schemaVersion") == 1, "acceptance inventory schema mismatch")
    require(inventory.get("kind") == "nexus-v2-acceptance-inventory", "acceptance inventory kind mismatch")
    inventory_release = ensure_release_id(str(inventory.get("releaseId", "")))
    inventory_commit = ensure_commit(str(inventory.get("sourceCommit", "")))
    if release_id is not None:
        require(inventory_release == release_id, "acceptance inventory release mismatch")
    if source_commit is not None:
        require(inventory_commit == source_commit, "acceptance inventory source commit mismatch")
    block_number, block_hash = finalized_block(inventory.get("observedAtFinalizedBlock"), "acceptance inventory")
    counts = inventory.get("counts")
    require(isinstance(counts, dict), "acceptance inventory counts must be an object")
    require(set(counts) == ACCEPTANCE_COUNT_FIELDS, "acceptance inventory counts do not match the required closed set")
    nonzero: dict[str, int] = {}
    for name, value in counts.items():
        require(isinstance(value, int) and not isinstance(value, bool) and value >= 0, f"invalid acceptance count: {name}")
        if value:
            nonzero[name] = value
    return {
        "value": inventory,
        "releaseId": inventory_release,
        "sourceCommit": inventory_commit,
        "blockNumber": block_number,
        "blockHash": block_hash,
        "sha256": sha256_file(path),
        "nonzero": nonzero,
    }


def validate_restore_evidence(
    path: Path,
    release_id: str,
    source_commit: str,
    manifest_hash: str,
) -> dict[str, Any]:
    evidence = read_json(path)
    require(evidence.get("schemaVersion") == 1, "restore evidence schema mismatch")
    require(evidence.get("kind") == "nexus-v2-isolated-restore-evidence", "restore evidence kind mismatch")
    require(evidence.get("releaseId") == release_id, "restore evidence release mismatch")
    require(evidence.get("sourceCommit") == source_commit, "restore evidence source commit mismatch")
    require(evidence.get("backupManifestSha256") == manifest_hash, "restore evidence manifest mismatch")
    require(evidence.get("result") == "passed", "isolated restore did not pass")
    require(evidence.get("liveAlphaTouched") is False, "restore evidence reports live Alpha access")
    parse_utc(str(evidence.get("completedAtUtc", "")), "restore completedAtUtc")
    return evidence


def validate_migration_evidence(
    path: Path,
    release_id: str,
    source_commit: str,
    manifest_hash: str,
) -> dict[str, Any]:
    evidence = read_json(path)
    require(evidence.get("schemaVersion") == 1, "migration evidence schema mismatch")
    require(evidence.get("kind") == "nexus-v2-v14-v16-migration-evidence", "migration evidence kind mismatch")
    require(evidence.get("releaseId") == release_id, "migration evidence release mismatch")
    require(evidence.get("sourceCommit") == source_commit, "migration evidence source commit mismatch")
    require(evidence.get("backupManifestSha256") == manifest_hash, "migration evidence manifest mismatch")
    require(evidence.get("fromStorageVersion") == 14 and evidence.get("toStorageVersion") == 16, "migration evidence version mismatch")
    require(evidence.get("result") == "passed", "V14-to-V16 rehearsal did not pass")
    require(evidence.get("liveRpcUsed") is False, "migration evidence used live RPC")
    require(evidence.get("extrinsicSubmitted") is False, "migration evidence submitted an extrinsic")
    parse_utc(str(evidence.get("completedAtUtc", "")), "migration completedAtUtc")
    return evidence


def validate_pre_v16_fresh_reset_artifact_binding(
    gates: Mapping[str, Any],
    verified: Mapping[str, Any],
    bundle_root: Path,
) -> None:
    runtime_v14 = find_artifact(verified, bundle_root, "node", "runtime-v14-wasm")
    observation_path = find_artifact(
        verified,
        bundle_root,
        "node",
        "tcg-storage-version-observation",
    )
    require(
        sha256_file(runtime_v14) == gates["runtimeV14WasmSha256"],
        "pre-V16 gate runtime hash does not match the pinned V14 Wasm",
    )
    require(
        sha256_file(observation_path) == gates["tcgStorageVersionObservationSha256"],
        "pre-V16 gate observation hash does not match the pinned TCG observation",
    )

    observation = read_json(observation_path)
    observed_block_number, observed_block_hash = finalized_block(
        observation.get("finalizedBlock"),
        "pinned TCG observation",
    )
    require(
        (observed_block_number, observed_block_hash)
        == (gates["blockNumber"], gates["blockHash"]),
        "pre-V16 gate block does not match the pinned TCG observation",
    )
    decoded = observation.get("decoded")
    require(
        isinstance(decoded, dict)
        and decoded.get("scaleType") == "u16"
        and decoded.get("storageVersion") == 14,
        "pinned TCG observation does not decode storage version 14",
    )
    read_only_rpc = observation.get("readOnlyRpc")
    require(
        isinstance(read_only_rpc, dict)
        and read_only_rpc.get("method") == "state_getStorage"
        and read_only_rpc.get("result") == "0x0e00",
        "pinned TCG observation does not prove SCALE storage version 14",
    )
    ensure_hash256(
        read_only_rpc.get("storageKey"),
        "pinned TCG storage-version key",
    )
    live_source = observation.get("liveSource")
    require(
        isinstance(live_source, dict)
        and live_source.get("declaredStorageVersion") == 14,
        "pinned TCG observation live source does not declare storage version 14",
    )
    require(
        ensure_commit(str(live_source.get("commit", "")))
        == gates["deployedSourceCommit"],
        "pre-V16 deployed source commit does not match the pinned TCG observation",
    )


def command_prepare_reset(args: argparse.Namespace) -> None:
    manifest_path = Path(args.manifest).resolve()
    bundle_root = Path(args.bundle_root).resolve()
    verified = verify_backup_manifest(manifest_path, bundle_root)
    restore_path = Path(args.restore_evidence).resolve()
    migration_path = Path(args.migration_evidence).resolve()
    gates_path = Path(args.economic_gates).resolve()
    inventory_path = Path(args.acceptance_inventory).resolve()
    pinned_gates_path = find_artifact(verified, bundle_root, "config", "economic-gates")
    require(
        sha256_file(gates_path) == sha256_file(pinned_gates_path),
        "economic gates do not match the hash-pinned backup artifact",
    )
    validate_restore_evidence(
        restore_path,
        verified["releaseId"],
        verified["sourceCommit"],
        verified["sha256"],
    )
    validate_migration_evidence(
        migration_path,
        verified["releaseId"],
        verified["sourceCommit"],
        verified["sha256"],
    )
    gates = validate_economic_gates(
        gates_path,
        verified["releaseId"],
        verified["sourceCommit"],
        allow_pre_v16_fresh_reset=True,
    )
    if gates["mode"] == PRE_V16_FRESH_RESET_GATE_MODE:
        validate_pre_v16_fresh_reset_artifact_binding(gates, verified, bundle_root)
    inventory = validate_acceptance_inventory(
        inventory_path,
        verified["releaseId"],
        verified["sourceCommit"],
    )
    require(
        (gates["blockNumber"], gates["blockHash"])
        == (inventory["blockNumber"], inventory["blockHash"]),
        "economic gates and acceptance inventory must come from the same finalized block",
    )
    require(not inventory["nonzero"], "reset readiness requires zero V2 acceptance assets")
    reset_mode = (
        "fresh-genesis-replacement"
        if gates["mode"] == PRE_V16_FRESH_RESET_GATE_MODE
        else "post-v16-disabled-state"
    )

    output = Path(args.output)
    readiness = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-reset-readiness",
        "releaseId": verified["releaseId"],
        "sourceCommit": verified["sourceCommit"],
        "backupManifestSha256": verified["sha256"],
        "restoreEvidenceSha256": sha256_file(restore_path),
        "migrationEvidenceSha256": sha256_file(migration_path),
        "economicGatesSha256": gates["sha256"],
        "acceptanceInventorySha256": inventory["sha256"],
        "economicGateMode": gates["mode"],
        "resetMode": reset_mode,
        "freshGenesisReplacementOnly": gates["mode"] == PRE_V16_FRESH_RESET_GATE_MODE,
        "inPlaceRuntimeActivationAuthorized": False,
        "gateFinalizedBlock": {
            "number": gates["blockNumber"],
            "hash": gates["blockHash"],
        },
        "readyForSeparateOperatorResetAuthorization": True,
        "automaticRollbackEligibleAtGateBlock": True,
        "economicFlagsDisabled": True,
        "v2AcceptanceAssetsExist": False,
        "resetExecuted": False,
        "deployExecuted": False,
        "createdAtUtc": utc_now(),
    }
    write_new_json(output, readiness)
    print(f"reset readiness packet created without reset/deploy: {output}")


def validate_readiness(path: Path) -> dict[str, Any]:
    value = read_json(path)
    require(value.get("schemaVersion") == 1, "readiness schema mismatch")
    require(value.get("kind") == "nexus-v2-private-alpha-reset-readiness", "readiness kind mismatch")
    ensure_release_id(str(value.get("releaseId", "")))
    ensure_commit(str(value.get("sourceCommit", "")))
    ensure_sha256(value.get("backupManifestSha256"), "readiness backup manifest SHA-256")
    gate_mode = value.get("economicGateMode")
    require(
        gate_mode in {POST_V16_GATE_MODE, PRE_V16_FRESH_RESET_GATE_MODE},
        "readiness economic gate mode mismatch",
    )
    expected_reset_mode = (
        "fresh-genesis-replacement"
        if gate_mode == PRE_V16_FRESH_RESET_GATE_MODE
        else "post-v16-disabled-state"
    )
    require(value.get("resetMode") == expected_reset_mode, "readiness reset mode mismatch")
    require(
        value.get("freshGenesisReplacementOnly")
        is (gate_mode == PRE_V16_FRESH_RESET_GATE_MODE),
        "readiness fresh-genesis scope mismatch",
    )
    require(
        value.get("inPlaceRuntimeActivationAuthorized") is False,
        "reset readiness may never authorize in-place runtime activation",
    )
    require(value.get("readyForSeparateOperatorResetAuthorization") is True, "reset readiness is not approved")
    require(value.get("automaticRollbackEligibleAtGateBlock") is True, "readiness was not pre-acceptance")
    require(value.get("economicFlagsDisabled") is True, "readiness did not keep economic flags disabled")
    require(value.get("v2AcceptanceAssetsExist") is False, "readiness already observed V2 acceptance assets")
    require(value.get("resetExecuted") is False and value.get("deployExecuted") is False, "readiness claims a live operation")
    parse_utc(str(value.get("createdAtUtc", "")), "readiness createdAtUtc")
    finalized_block(value.get("gateFinalizedBlock"), "readiness gate")
    return value


def validate_rollback_result(
    result: Mapping[str, Any],
    readiness: Mapping[str, Any],
    inventory_hash: str,
) -> None:
    require(result.get("schemaVersion") == 1, "rollback result schema mismatch")
    require(result.get("kind") == "nexus-v2-private-alpha-rollback-result", "rollback result kind mismatch")
    require(result.get("releaseId") == readiness["releaseId"], "rollback result release mismatch")
    require(result.get("sourceCommit") == readiness["sourceCommit"], "rollback result source commit mismatch")
    require(result.get("acceptanceInventorySha256") == inventory_hash, "rollback result inventory mismatch")
    require(result.get("result") == "passed", "rollback driver did not report success")
    strict_true_checks(
        result.get("checks"),
        {
            "rollbackCompleted",
            "backupHashesVerified",
            "nodeHealthy",
            "mediaHealthy",
            "ipfsHealthy",
            "indexerHealthy",
            "economicFlagsDisabled",
        },
        "rollback",
    )


def command_automatic_rollback(args: argparse.Namespace) -> None:
    require(args.execute, "automatic rollback requires explicit --execute")
    readiness_path = Path(args.readiness).resolve()
    inventory_path = Path(args.acceptance_inventory).resolve()
    gates_path = Path(args.economic_gates).resolve()
    approval_path = Path(args.approval).resolve()
    readiness = validate_readiness(readiness_path)
    inventory = validate_acceptance_inventory(
        inventory_path,
        str(readiness["releaseId"]),
        str(readiness["sourceCommit"]),
    )
    gates = validate_economic_gates(
        gates_path,
        str(readiness["releaseId"]),
        str(readiness["sourceCommit"]),
    )
    require(
        gates["mode"] == POST_V16_GATE_MODE,
        "automatic rollback requires post-V16 disabled-state gates",
    )
    gate_number, _ = finalized_block(readiness["gateFinalizedBlock"], "readiness gate")
    require(inventory["blockNumber"] >= gate_number, "rollback inventory predates reset readiness")
    require(
        (inventory["blockNumber"], inventory["blockHash"])
        == (gates["blockNumber"], gates["blockHash"]),
        "rollback inventory and economic gates must come from the same finalized block",
    )

    output = Path(args.evidence)
    require(not output.exists(), f"refusing to overwrite rollback evidence: {output}")
    if inventory["nonzero"]:
        blocked = {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-rollback-decision",
            "releaseId": readiness["releaseId"],
            "sourceCommit": readiness["sourceCommit"],
            "decision": "blocked-after-v2-acceptance",
            "nonzeroAcceptanceAssets": inventory["nonzero"],
            "requiredResponse": "pause-v2-writes-and-preserve-state",
            "rollbackDriverInvoked": False,
            "createdAtUtc": utc_now(),
        }
        write_new_json(output, blocked)
        raise SafetyError("automatic rollback is permanently blocked after any V2 acceptance asset exists")

    driver = validate_external_driver(Path(args.driver), "automatic rollback driver")
    driver_hash = sha256_file(driver)
    approval = read_json(approval_path)
    require(approval.get("schemaVersion") == 1, "rollback approval schema mismatch")
    require(approval.get("kind") == "nexus-v2-private-alpha-rollback-approval", "rollback approval kind mismatch")
    require(approval.get("releaseId") == readiness["releaseId"], "rollback approval release mismatch")
    require(approval.get("sourceCommit") == readiness["sourceCommit"], "rollback approval source mismatch")
    require(approval.get("approved") is True, "automatic rollback is not approved")
    require(approval.get("readinessSha256") == sha256_file(readiness_path), "rollback approval readiness hash mismatch")
    require(approval.get("rollbackDriverSha256") == driver_hash, "rollback approval driver hash mismatch")
    expires_at = parse_utc(str(approval.get("expiresAtUtc", "")), "rollback approval expiresAtUtc")
    require(expires_at > dt.datetime.now(dt.timezone.utc), "rollback approval expired")

    result_path = Path(f"{output}.rollback-result.json")
    log_path = Path(f"{output}.rollback.log")
    command = [
        str(driver),
        "--readiness",
        str(readiness_path),
        "--acceptance-inventory",
        str(inventory_path),
        "--economic-gates",
        str(gates_path),
        "--result",
        str(result_path),
    ]
    completed = run_and_capture(command, log_path)
    require(completed.returncode == 0, f"automatic rollback driver failed; see {log_path}")
    result = read_json(result_path)
    validate_rollback_result(result, readiness, inventory["sha256"])

    evidence = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-rollback-evidence",
        "releaseId": readiness["releaseId"],
        "sourceCommit": readiness["sourceCommit"],
        "readinessSha256": sha256_file(readiness_path),
        "acceptanceInventorySha256": inventory["sha256"],
        "economicGatesSha256": gates["sha256"],
        "approvalSha256": sha256_file(approval_path),
        "rollbackDriverSha256": driver_hash,
        "rollbackLogSha256": sha256_file(log_path),
        "rollbackResultSha256": sha256_file(result_path),
        "result": "passed",
        "automaticRollbackWasPreAcceptance": True,
        "completedAtUtc": utc_now(),
    }
    write_new_json(output, evidence)
    print(f"pre-acceptance automatic rollback completed: {output}")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Nexus V2 private-alpha evidence and safety orchestration",
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    backup = subparsers.add_parser("backup-manifest", help="hash a complete private-alpha backup bundle")
    backup.add_argument("--bundle-root", required=True)
    backup.add_argument("--release-id", required=True)
    backup.add_argument("--source-commit", required=True)
    backup.add_argument("--artifact", action="append", default=[], required=True)
    backup.add_argument("--created-at")
    backup.add_argument("--output", required=True)
    backup.set_defaults(handler=command_backup_manifest)

    verify = subparsers.add_parser("verify-backup", help="verify every artifact and coordination hash")
    verify.add_argument("--bundle-root", required=True)
    verify.add_argument("--manifest", required=True)
    verify.set_defaults(handler=command_verify_backup)

    init_root = subparsers.add_parser("init-isolation-root", help="create a guarded isolated restore root")
    init_root.add_argument("--root", required=True)
    init_root.add_argument("--release-id", required=True)
    init_root.add_argument("--created-at")
    init_root.set_defaults(handler=command_init_isolation_root)

    restore = subparsers.add_parser("rehearse-restore", help="run a supplied restore driver on isolated ports")
    restore.add_argument("--manifest", required=True)
    restore.add_argument("--bundle-root", required=True)
    restore.add_argument("--isolation-root", required=True)
    restore.add_argument("--ports", required=True)
    restore.add_argument("--driver", required=True)
    restore.add_argument("--evidence", required=True)
    restore.set_defaults(handler=command_rehearse_restore)

    migration = subparsers.add_parser("rehearse-migration", help="record V14-to-V16 copied-state evidence")
    migration.add_argument("--manifest", required=True)
    migration.add_argument("--bundle-root", required=True)
    migration.add_argument("--try-runtime-bin", required=True)
    migration.add_argument("--try-runtime-revision", required=True)
    migration.add_argument("--try-runtime-sha256", required=True)
    migration.add_argument("--migration-blocks", required=True, type=int)
    migration.add_argument("--migration-verifier", required=True)
    migration.add_argument("--migration-verifier-sha256", required=True)
    migration.add_argument("--evidence", required=True)
    migration.set_defaults(handler=command_rehearse_migration)

    reset = subparsers.add_parser("prepare-reset", help="emit readiness evidence without reset/deploy")
    reset.add_argument("--manifest", required=True)
    reset.add_argument("--bundle-root", required=True)
    reset.add_argument("--restore-evidence", required=True)
    reset.add_argument("--migration-evidence", required=True)
    reset.add_argument("--economic-gates", required=True)
    reset.add_argument("--acceptance-inventory", required=True)
    reset.add_argument("--output", required=True)
    reset.set_defaults(handler=command_prepare_reset)

    rollback = subparsers.add_parser("automatic-rollback", help="run an approved external rollback driver before acceptance only")
    rollback.add_argument("--readiness", required=True)
    rollback.add_argument("--acceptance-inventory", required=True)
    rollback.add_argument("--economic-gates", required=True)
    rollback.add_argument("--approval", required=True)
    rollback.add_argument("--driver", required=True)
    rollback.add_argument("--evidence", required=True)
    rollback.add_argument("--execute", action="store_true")
    rollback.set_defaults(handler=command_automatic_rollback)

    return parser


def main(argv: Sequence[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        args.handler(args)
    except SafetyError as exc:
        print(f"nexus-v2-private-alpha: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
