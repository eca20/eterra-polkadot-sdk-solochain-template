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
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any, Iterable, Mapping, Sequence

import deployment_secret_environment  # noqa: F401


REPO_ROOT = Path(__file__).resolve().parents[2]

REQUIRED_ARTIFACTS: dict[str, set[str]] = {
    "node": {
        "node-data",
        "node-binary",
        "runtime-v14-wasm",
        "runtime-v14-metadata",
        "runtime-v16-production-wasm",
        "runtime-v16-try-runtime-wasm",
        "legacy-source-inventory",
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

V2_ACCEPTANCE_COUNT_FIELDS = {
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

# Phase-1/Phase-2 cutover boundary signals.  The legacy authority pallet
# predates Nexus V2 and therefore is not covered by the V2 asset counters
# above.  These monotonic/current observations make any legacy FPS write a
# restore-blocking acceptance write as well.  They do not retroactively prevent
# the already-approved, backup-protected pre-V16 fresh-genesis replacement.
LEGACY_AUTHORITY_ACCEPTANCE_COUNT_FIELDS = {
    "currentLegacyAuthorityGames",
    "currentLegacyAuthorityActivePlayerLocks",
    "currentLegacyAuthorityEliminationRecords",
    "lifetimeLegacyAuthorityGamesCreated",
    "lifetimeLegacyAuthorityEndCommandsProcessed",
    "lifetimeLegacyAuthorityEliminationEventsProcessed",
    "lifetimeLegacyAuthorityAcceptanceWritesLowerBound",
}

# Explicit GameResults observations retain the distinction between live maps
# and monotonic/session-history signals.  NextSessionId is the exact lifetime
# authorization count for this runtime.  Sealed terminal sessions are
# conservative: expiry/abort may be included, which is intentionally safe for
# the one-way restore boundary.
GAME_RESULTS_ACCEPTANCE_COUNT_FIELDS = {
    "currentV2GameResultSessions",
    "currentV2ProcessedResults",
    "currentV2SettledSessions",
    "lifetimeV2SessionIdsAllocated",
    "conservativeSealedV2TerminalSessions",
}

ACCEPTANCE_COUNT_FIELDS = (
    V2_ACCEPTANCE_COUNT_FIELDS
    | LEGACY_AUTHORITY_ACCEPTANCE_COUNT_FIELDS
    | GAME_RESULTS_ACCEPTANCE_COUNT_FIELDS
)
PRE_V16_ACCEPTANCE_OBSERVATION_EVIDENCE_KEYS = {
    "captureMode",
    "legacySourceInventorySha256",
    "runtimeMetadataScaleSha256",
    "tcgStorageVersionObservationSha256",
    "v2CountersDerivedFromStructuralAbsence",
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
LEGACY_SOURCE_INVENTORY_KIND = (
    "nexus-v2-private-alpha-frozen-legacy-source-inventory"
)
LEGACY_TCG_STORAGE_VERSION_KEY = (
    "0x2ac14ba6b10e5ff91888f263bf83e9a94e7b9012096b41c4eb3aaf947f6ea429"
)
LEGACY_NEXT_CARD_ID_KEY = (
    "0x2ac14ba6b10e5ff91888f263bf83e9a9e8da9327457f7e23b5dae0a9f0f8a915"
)
LEGACY_CARDS_PREFIX = (
    "0x2ac14ba6b10e5ff91888f263bf83e9a947a302df212e07c474ca486147c54c8b"
)
LEGACY_GAME_AUTHORITY_STORAGE = {
    "nextGameId": {
        "storage": "NextGameId",
        "key": "0x0fe4912f15e6c31fac74ae40589be4f1a7d6c931c9519bfbc114231c189cb500",
        "plain": True,
    },
    "games": {
        "storage": "Games",
        "prefix": "0x0fe4912f15e6c31fac74ae40589be4f1c7888246b3c99d8f35bc494ff792ada0",
    },
    "activeGameByPlayer": {
        "storage": "ActiveGameByPlayer",
        "prefix": "0x0fe4912f15e6c31fac74ae40589be4f14be21b984611c3c606b54656927bb4a4",
    },
    "eliminations": {
        "storage": "Eliminations",
        "prefix": "0x0fe4912f15e6c31fac74ae40589be4f19bdbb1fedcec63769a04867c03631be0",
    },
    "processedEndCommands": {
        "storage": "ProcessedEndCommands",
        "prefix": "0x0fe4912f15e6c31fac74ae40589be4f1a1585f5fe383a9cf284442e868c2f953",
    },
    "processedEliminationEvents": {
        "storage": "ProcessedEliminationEvents",
        "prefix": "0x0fe4912f15e6c31fac74ae40589be4f18a91361e9da1abd51b0d70d8f9d3afef",
    },
}
LEGACY_CARD_KEY_HEX_LENGTH = 2 + (32 + 16 + 4) * 2
LEGACY_CARD_KEY_PAGE_SIZE = 256
MAX_LEGACY_CARD_KEYS = 100_000
LEGACY_AUTHORITY_KEY_PAGE_SIZE = 256
MAX_LEGACY_AUTHORITY_KEYS_PER_STORAGE = 100_000
V16_MIGRATION_BATCH_SIZE = 100
MAX_MIGRATION_BLOCKS = 1_000_000
LEGACY_SOURCE_INVENTORY_KEYS = {
    "captureMode",
    "deployedSourceCommit",
    "finality",
    "kind",
    "observedAtFinalizedBlock",
    "observedAtUtc",
    "releaseId",
    "safety",
    "schemaVersion",
    "sourceCommit",
    "storage",
    "summary",
}
LEGACY_FINALITY_KEYS = {"blockHashAtNumber", "finalizedHead", "header"}
LEGACY_RPC_QUERY_KEYS = {"method", "params", "result"}
LEGACY_STORAGE_KEYS = {"cards", "gameAuthority", "nextCardId", "tcgStorageVersion"}
LEGACY_CARD_STORAGE_KEYS = {
    "at",
    "method",
    "pageSize",
    "pages",
    "pallet",
    "prefix",
    "storage",
}
LEGACY_PAGE_KEYS = {"keys", "startKey"}
LEGACY_AUTHORITY_STORAGE_KEYS = set(LEGACY_GAME_AUTHORITY_STORAGE)
LEGACY_AUTHORITY_MAP_KEYS = {
    "at",
    "method",
    "pageSize",
    "pages",
    "pallet",
    "prefix",
    "storage",
}
LEGACY_SUMMARY_KEYS = {
    "cardIdsSha256",
    "cardsCount",
    "maxCardId",
    "minimumMigrationBlocks",
    "nextCardId",
    "gameAuthorityActivePlayerLocks",
    "gameAuthorityEliminationRecords",
    "gameAuthorityEndCommandsProcessed",
    "gameAuthorityEliminationEventsProcessed",
    "gameAuthorityGames",
    "gameAuthorityNextGameId",
    "tcgStorageVersion",
    "v16MigrationBatchSize",
}
LEGACY_SAFETY = {
    "extrinsicSubmitted": False,
    "isolatedFrozenCopy": True,
    "readOnlyRpc": True,
    "sourceStateMutated": False,
}


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


def canonical_json_bytes(value: Mapping[str, Any]) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")


def write_new_bytes(path: Path, value: bytes, mode: int = 0o600) -> None:
    require(not path.exists(), f"refusing to overwrite output: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, mode)
    with os.fdopen(descriptor, "wb") as handle:
        handle.write(value)


def write_new_json(path: Path, value: Mapping[str, Any]) -> None:
    write_new_bytes(path, canonical_json_bytes(value))


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
    source_inventory: Mapping[str, Any],
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
    require(
        counts["nextCardId"] == source_inventory["nextCardId"],
        "migration result NextCardId differs from the frozen source inventory",
    )
    require(
        counts["legacyCardsBefore"] == source_inventory["cardsCount"],
        "migration result card count differs from the frozen source inventory",
    )
    require(
        max_seen == source_inventory["maxCardId"],
        "migration result max card ID differs from the frozen source inventory",
    )


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
    source_inventory_path = find_artifact(
        verified, bundle_root, "node", "legacy-source-inventory"
    )
    source_inventory = validate_legacy_source_inventory(
        source_inventory_path, verified["releaseId"], verified["sourceCommit"]
    )
    observation_path = find_artifact(
        verified, bundle_root, "node", "tcg-storage-version-observation"
    )
    observation = read_json(observation_path)
    observation_block = finalized_block(
        observation.get("finalizedBlock"), "pinned TCG storage-version observation"
    )
    require(
        observation_block
        == (source_inventory["blockNumber"], source_inventory["blockHash"]),
        "legacy source inventory and TCG storage-version observation use different finalized blocks",
    )
    observation_live_source = observation.get("liveSource")
    require(
        isinstance(observation_live_source, dict)
        and observation_live_source.get("commit") == source_inventory["deployedSourceCommit"],
        "legacy source inventory deployed source differs from the TCG observation",
    )
    snapshot_proof_path = find_artifact(
        verified, bundle_root, "node", "try-runtime-snapshot-proof"
    )
    snapshot_proof_value = read_json(snapshot_proof_path)
    snapshot_block = finalized_block(
        snapshot_proof_value.get("frozenFinalizedBlock"), "try-runtime snapshot proof"
    )
    require(
        snapshot_block
        == (source_inventory["blockNumber"], source_inventory["blockHash"]),
        "legacy source inventory and try-runtime snapshot use different finalized blocks",
    )
    required_migration_blocks = source_inventory["minimumMigrationBlocks"]
    migration_blocks = (
        required_migration_blocks if args.migration_blocks is None else args.migration_blocks
    )
    require(
        isinstance(migration_blocks, int)
        and not isinstance(migration_blocks, bool)
        and 1 <= migration_blocks <= MAX_MIGRATION_BLOCKS,
        f"migration blocks must be in 1..{MAX_MIGRATION_BLOCKS}",
    )
    require(
        required_migration_blocks <= MAX_MIGRATION_BLOCKS,
        "frozen NextCardId exceeds the bounded migration-rehearsal block limit",
    )
    require(
        migration_blocks >= required_migration_blocks,
        "migration blocks are insufficient for frozen NextCardId: "
        f"provided={migration_blocks} required={required_migration_blocks}",
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
        str(migration_blocks),
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
    validate_migration_result(
        result,
        verified,
        snapshot_hash,
        runtime_hash,
        try_log_hash,
        source_inventory,
    )

    evidence = {
        "schemaVersion": 1,
        "kind": "nexus-v2-v14-v16-migration-evidence",
        "releaseId": verified["releaseId"],
        "sourceCommit": verified["sourceCommit"],
        "backupManifestSha256": verified["sha256"],
        "fromStorageVersion": 14,
        "toStorageVersion": 16,
        "snapshotSha256": snapshot_hash,
        "legacySourceInventorySha256": source_inventory["sha256"],
        "legacySourceInventoryFinalizedBlock": {
            "number": source_inventory["blockNumber"],
            "hash": source_inventory["blockHash"],
        },
        "legacyNextCardId": source_inventory["nextCardId"],
        "legacyCardsCount": source_inventory["cardsCount"],
        "legacyMaxCardId": source_inventory["maxCardId"],
        "v16MigrationBatchSize": V16_MIGRATION_BATCH_SIZE,
        "minimumMigrationBlocks": required_migration_blocks,
        "runtimeWasmSha256": runtime_hash,
        "tryRuntimeRevision": args.try_runtime_revision,
        "tryRuntimeBinarySha256": expected_try_hash,
        "tryRuntimeVersion": try_version,
        "tryRuntimeFastForwardBlocks": migration_blocks,
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


def minimum_v16_migration_blocks(next_card_id: int) -> int:
    require(
        isinstance(next_card_id, int)
        and not isinstance(next_card_id, bool)
        and 0 <= next_card_id <= 0xFFFF_FFFF,
        "legacy NextCardId must be a u32",
    )
    return max(1, (next_card_id + V16_MIGRATION_BATCH_SIZE - 1) // V16_MIGRATION_BATCH_SIZE)


def decode_scale_u32(value: Any, label: str) -> int:
    if value is None:
        return 0
    require(
        isinstance(value, str) and re.fullmatch(r"0x[0-9a-f]{8}", value) is not None,
        f"{label} must be null or exactly four lowercase SCALE bytes",
    )
    return int.from_bytes(bytes.fromhex(value[2:]), "little")


def decode_scale_u64(value: Any, label: str) -> int:
    if value is None:
        return 0
    require(
        isinstance(value, str) and re.fullmatch(r"0x[0-9a-f]{16}", value) is not None,
        f"{label} must be null or exactly eight lowercase SCALE bytes",
    )
    return int.from_bytes(bytes.fromhex(value[2:]), "little")


def decode_legacy_card_key(value: Any) -> int:
    require(
        isinstance(value, str)
        and len(value) == LEGACY_CARD_KEY_HEX_LENGTH
        and re.fullmatch(r"0x[0-9a-f]+", value) is not None,
        "legacy Cards key is not canonical lowercase hex",
    )
    raw = bytes.fromhex(value[2:])
    prefix = bytes.fromhex(LEGACY_CARDS_PREFIX[2:])
    require(raw.startswith(prefix), "legacy Cards key has the wrong storage prefix")
    encoded_id = raw[-4:]
    require(
        raw[32:48] == hashlib.blake2b(encoded_id, digest_size=16).digest(),
        "legacy Cards key has an invalid Blake2_128Concat hash",
    )
    return int.from_bytes(encoded_id, "little")


def validate_rpc_query(
    value: Any,
    method: str,
    params: list[Any],
    label: str,
) -> Mapping[str, Any]:
    query = require_exact_keys(value, LEGACY_RPC_QUERY_KEYS, label)
    require(query.get("method") == method, f"{label} method mismatch")
    require(query.get("params") == params, f"{label} parameters mismatch")
    return query


def validate_paged_storage_keys(
    value: Any,
    *,
    pallet: str,
    storage_name: str,
    prefix: str,
    block_hash: str,
    page_size: int,
    maximum_keys: int,
    label: str,
) -> list[str]:
    capture = require_exact_keys(value, LEGACY_AUTHORITY_MAP_KEYS, label)
    require(
        capture.get("pallet") == pallet and capture.get("storage") == storage_name,
        f"{label} identity mismatch",
    )
    require(capture.get("prefix") == prefix, f"{label} prefix mismatch")
    require(capture.get("method") == "state_getKeysPaged", f"{label} query method mismatch")
    require(capture.get("at") == block_hash, f"{label} query block mismatch")
    require(capture.get("pageSize") == page_size, f"{label} page size mismatch")
    pages = capture.get("pages")
    require(isinstance(pages, list) and pages, f"{label} pages must be a non-empty array")
    all_keys: list[str] = []
    expected_start: str | None = None
    for index, raw_page in enumerate(pages):
        page = require_exact_keys(raw_page, LEGACY_PAGE_KEYS, f"{label} page {index}")
        require(page.get("startKey") == expected_start, f"{label} page {index} cursor mismatch")
        keys = page.get("keys")
        require(
            isinstance(keys, list) and len(keys) <= page_size,
            f"{label} page {index} is invalid",
        )
        require(keys == sorted(keys), f"{label} page {index} is not sorted")
        for key in keys:
            require(
                isinstance(key, str)
                and re.fullmatch(r"0x[0-9a-f]+", key) is not None
                and key.startswith(prefix)
                and len(key) > len(prefix),
                f"{label} contains an invalid storage key",
            )
        if expected_start is not None and keys:
            require(keys[0] > expected_start, f"{label} page {index} repeated its cursor")
        all_keys.extend(keys)
        require(len(all_keys) <= maximum_keys, f"{label} exceeds the collector bound")
        expected_start = keys[-1] if keys else expected_start
        if index < len(pages) - 1:
            require(len(keys) == page_size, f"{label} capture continued after a short page")
        else:
            require(len(keys) < page_size, f"{label} capture lacks a terminal short page")
    require(all_keys == sorted(set(all_keys)), f"{label} keys are duplicated or globally unsorted")
    return all_keys


def validate_legacy_source_inventory_value(
    value: Mapping[str, Any],
    *,
    release_id: str | None = None,
    source_commit: str | None = None,
) -> dict[str, Any]:
    require_exact_keys(value, LEGACY_SOURCE_INVENTORY_KEYS, "legacy source inventory")
    require(value.get("schemaVersion") == 2, "legacy source inventory schema mismatch")
    require(value.get("kind") == LEGACY_SOURCE_INVENTORY_KIND, "legacy source inventory kind mismatch")
    inventory_release = ensure_release_id(str(value.get("releaseId", "")))
    inventory_source = ensure_commit(str(value.get("sourceCommit", "")))
    deployed_source = ensure_commit(str(value.get("deployedSourceCommit", "")))
    if release_id is not None:
        require(inventory_release == release_id, "legacy source inventory release mismatch")
    if source_commit is not None:
        require(inventory_source == source_commit, "legacy source inventory source mismatch")
    parse_utc(str(value.get("observedAtUtc", "")), "legacy source inventory observedAtUtc")
    require(
        value.get("captureMode") == "isolated-frozen-copy-read-only",
        "legacy source inventory was not captured from the isolated frozen copy",
    )
    require(value.get("safety") == LEGACY_SAFETY, "legacy source inventory safety contract mismatch")
    block_number, block_hash = finalized_block(
        value.get("observedAtFinalizedBlock"), "legacy source inventory"
    )
    require(block_number > 0, "legacy source inventory may not use genesis block zero")

    finality = require_exact_keys(value.get("finality"), LEGACY_FINALITY_KEYS, "legacy inventory finality")
    finalized = validate_rpc_query(
        finality["finalizedHead"], "chain_getFinalizedHead", [], "legacy finalized-head query"
    )
    require(finalized.get("result") == block_hash, "legacy inventory finalized head mismatch")
    at_number = validate_rpc_query(
        finality["blockHashAtNumber"],
        "chain_getBlockHash",
        [block_number],
        "legacy block-number query",
    )
    require(at_number.get("result") == block_hash, "legacy inventory block-number hash mismatch")
    header = validate_rpc_query(
        finality["header"], "chain_getHeader", [block_hash], "legacy block-header query"
    )
    header_result = header.get("result")
    require(isinstance(header_result, dict), "legacy inventory block header is missing")
    header_number = header_result.get("number")
    require(
        isinstance(header_number, str)
        and re.fullmatch(r"0x[0-9a-f]+", header_number) is not None
        and int(header_number, 16) == block_number,
        "legacy inventory block header number mismatch",
    )

    storage = require_exact_keys(value.get("storage"), LEGACY_STORAGE_KEYS, "legacy inventory storage")
    plain_keys = {"key", "pallet", "query", "storage"}
    version = require_exact_keys(
        storage["tcgStorageVersion"], plain_keys, "legacy TCG storage-version capture"
    )
    require(
        version.get("pallet") == "EterraTCG"
        and version.get("storage") == ":__STORAGE_VERSION__:",
        "legacy TCG storage-version identity mismatch",
    )
    require(version.get("key") == LEGACY_TCG_STORAGE_VERSION_KEY, "legacy TCG storage-version key mismatch")
    version_query = validate_rpc_query(
        version.get("query"),
        "state_getStorage",
        [LEGACY_TCG_STORAGE_VERSION_KEY, block_hash],
        "legacy TCG storage-version query",
    )
    require(version_query.get("result") == "0x0e00", "legacy TCG source storage is not V14")

    next_card = require_exact_keys(storage["nextCardId"], plain_keys, "legacy NextCardId capture")
    require(
        next_card.get("pallet") == "EterraTCG" and next_card.get("storage") == "NextCardId",
        "legacy NextCardId identity mismatch",
    )
    require(next_card.get("key") == LEGACY_NEXT_CARD_ID_KEY, "legacy NextCardId key mismatch")
    next_query = validate_rpc_query(
        next_card.get("query"),
        "state_getStorage",
        [LEGACY_NEXT_CARD_ID_KEY, block_hash],
        "legacy NextCardId query",
    )
    next_card_id = decode_scale_u32(next_query.get("result"), "legacy NextCardId result")

    authority = require_exact_keys(
        storage["gameAuthority"],
        LEGACY_AUTHORITY_STORAGE_KEYS,
        "legacy GameAuthority capture",
    )
    next_game_config = LEGACY_GAME_AUTHORITY_STORAGE["nextGameId"]
    next_game = require_exact_keys(
        authority["nextGameId"], plain_keys, "legacy GameAuthority NextGameId capture"
    )
    require(
        next_game.get("pallet") == "EterraGameAuthority"
        and next_game.get("storage") == next_game_config["storage"],
        "legacy GameAuthority NextGameId identity mismatch",
    )
    require(
        next_game.get("key") == next_game_config["key"],
        "legacy GameAuthority NextGameId key mismatch",
    )
    next_game_query = validate_rpc_query(
        next_game.get("query"),
        "state_getStorage",
        [next_game_config["key"], block_hash],
        "legacy GameAuthority NextGameId query",
    )
    next_game_id = decode_scale_u64(
        next_game_query.get("result"), "legacy GameAuthority NextGameId result"
    )
    authority_counts: dict[str, int] = {}
    for alias in (
        "games",
        "activeGameByPlayer",
        "eliminations",
        "processedEndCommands",
        "processedEliminationEvents",
    ):
        config = LEGACY_GAME_AUTHORITY_STORAGE[alias]
        authority_counts[alias] = len(
            validate_paged_storage_keys(
                authority[alias],
                pallet="EterraGameAuthority",
                storage_name=str(config["storage"]),
                prefix=str(config["prefix"]),
                block_hash=block_hash,
                page_size=LEGACY_AUTHORITY_KEY_PAGE_SIZE,
                maximum_keys=MAX_LEGACY_AUTHORITY_KEYS_PER_STORAGE,
                label=f"legacy GameAuthority {config['storage']} capture",
            )
        )
    require(
        next_game_id >= authority_counts["games"],
        "legacy GameAuthority Games count exceeds NextGameId",
    )

    cards = require_exact_keys(storage["cards"], LEGACY_CARD_STORAGE_KEYS, "legacy Cards capture")
    require(
        cards.get("pallet") == "EterraTCG" and cards.get("storage") == "Cards",
        "legacy Cards identity mismatch",
    )
    require(cards.get("prefix") == LEGACY_CARDS_PREFIX, "legacy Cards storage prefix mismatch")
    require(cards.get("method") == "state_getKeysPaged", "legacy Cards query method mismatch")
    require(cards.get("at") == block_hash, "legacy Cards query block mismatch")
    require(cards.get("pageSize") == LEGACY_CARD_KEY_PAGE_SIZE, "legacy Cards page size mismatch")
    pages = cards.get("pages")
    require(isinstance(pages, list) and pages, "legacy Cards pages must be a non-empty array")
    all_keys: list[str] = []
    expected_start: str | None = None
    for index, raw_page in enumerate(pages):
        page = require_exact_keys(raw_page, LEGACY_PAGE_KEYS, f"legacy Cards page {index}")
        require(page.get("startKey") == expected_start, f"legacy Cards page {index} cursor mismatch")
        keys = page.get("keys")
        require(
            isinstance(keys, list) and len(keys) <= LEGACY_CARD_KEY_PAGE_SIZE,
            f"legacy Cards page {index} is invalid",
        )
        require(keys == sorted(keys), f"legacy Cards page {index} is not sorted")
        if expected_start is not None and keys:
            require(keys[0] > expected_start, f"legacy Cards page {index} repeated its cursor")
        all_keys.extend(keys)
        require(len(all_keys) <= MAX_LEGACY_CARD_KEYS, "legacy Cards inventory exceeds the collector bound")
        expected_start = keys[-1] if keys else expected_start
        if index < len(pages) - 1:
            require(
                len(keys) == LEGACY_CARD_KEY_PAGE_SIZE,
                "legacy Cards capture continued after a short page",
            )
        else:
            require(
                len(keys) < LEGACY_CARD_KEY_PAGE_SIZE,
                "legacy Cards capture lacks a terminal short page",
            )
    require(all_keys == sorted(set(all_keys)), "legacy Cards keys are duplicated or globally unsorted")
    card_ids = [decode_legacy_card_key(key) for key in all_keys]
    require(len(card_ids) == len(set(card_ids)), "legacy Cards decoded IDs are duplicated")
    max_card_id = max(card_ids) if card_ids else None
    if max_card_id is not None:
        require(next_card_id > max_card_id, "legacy Cards contains an ID outside NextCardId")
    card_ids_sha256 = sha256_bytes(
        b"".join(card_id.to_bytes(4, "little") for card_id in sorted(card_ids))
    )
    minimum_blocks = minimum_v16_migration_blocks(next_card_id)

    summary = require_exact_keys(value.get("summary"), LEGACY_SUMMARY_KEYS, "legacy inventory summary")
    expected_summary = {
        "cardIdsSha256": card_ids_sha256,
        "cardsCount": len(card_ids),
        "maxCardId": max_card_id,
        "minimumMigrationBlocks": minimum_blocks,
        "nextCardId": next_card_id,
        "gameAuthorityActivePlayerLocks": authority_counts["activeGameByPlayer"],
        "gameAuthorityEliminationRecords": authority_counts["eliminations"],
        "gameAuthorityEndCommandsProcessed": authority_counts["processedEndCommands"],
        "gameAuthorityEliminationEventsProcessed": authority_counts[
            "processedEliminationEvents"
        ],
        "gameAuthorityGames": authority_counts["games"],
        "gameAuthorityNextGameId": next_game_id,
        "tcgStorageVersion": 14,
        "v16MigrationBatchSize": V16_MIGRATION_BATCH_SIZE,
    }
    require(summary == expected_summary, "legacy source inventory summary is not derived from its RPC evidence")
    return {
        "releaseId": inventory_release,
        "sourceCommit": inventory_source,
        "deployedSourceCommit": deployed_source,
        "blockNumber": block_number,
        "blockHash": block_hash,
        **expected_summary,
    }


def validate_legacy_source_inventory(
    path: Path,
    release_id: str | None = None,
    source_commit: str | None = None,
) -> dict[str, Any]:
    require(path.is_file() and not path.is_symlink(), "legacy source inventory must be a regular file")
    value = read_json(path)
    require(
        path.read_bytes() == canonical_json_bytes(value),
        "legacy source inventory is not canonical JSON",
    )
    result = validate_legacy_source_inventory_value(
        value, release_id=release_id, source_commit=source_commit
    )
    result["sha256"] = sha256_file(path)
    return result


class FrozenInventoryRpc:
    def __init__(self, url: str, timeout_seconds: float) -> None:
        parsed = urllib.parse.urlparse(url)
        require(parsed.scheme == "http", "legacy inventory RPC must use HTTP over the local tunnel")
        require(parsed.hostname in {"127.0.0.1", "::1"}, "legacy inventory RPC must be loopback-only")
        require(parsed.port is not None, "legacy inventory RPC must include an explicit port")
        require(not parsed.username and not parsed.password, "legacy inventory RPC may not contain credentials")
        require(parsed.path in {"", "/"} and not parsed.query and not parsed.fragment, "legacy inventory RPC URL is invalid")
        require(timeout_seconds > 0 and timeout_seconds <= 60, "legacy inventory RPC timeout is invalid")
        self.url = url
        self.timeout_seconds = timeout_seconds
        self.request_id = 0
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    def call(self, method: str, params: list[Any]) -> Any:
        self.request_id += 1
        payload = json.dumps(
            {"id": self.request_id, "jsonrpc": "2.0", "method": method, "params": params},
            separators=(",", ":"),
        ).encode("utf-8")
        request = urllib.request.Request(
            self.url,
            data=payload,
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        try:
            with self.opener.open(request, timeout=self.timeout_seconds) as response:
                body = response.read(8 * 1024 * 1024 + 1)
        except (OSError, urllib.error.URLError) as exc:
            raise SafetyError(f"legacy inventory RPC failed: {method}") from exc
        require(len(body) <= 8 * 1024 * 1024, "legacy inventory RPC response is too large")
        try:
            value = json.loads(body)
        except (UnicodeDecodeError, json.JSONDecodeError) as exc:
            raise SafetyError(f"legacy inventory RPC returned invalid JSON: {method}") from exc
        require(
            isinstance(value, dict)
            and value.get("jsonrpc") == "2.0"
            and value.get("id") == self.request_id
            and "result" in value
            and "error" not in value,
            f"legacy inventory RPC returned an invalid envelope: {method}",
        )
        return value["result"]


def rpc_query(rpc: FrozenInventoryRpc, method: str, params: list[Any]) -> dict[str, Any]:
    return {"method": method, "params": params, "result": rpc.call(method, params)}


def collect_legacy_source_inventory(
    rpc: FrozenInventoryRpc,
    *,
    release_id: str,
    source_commit: str,
    deployed_source_commit: str,
    block_number: int,
    block_hash: str,
    observed_at: str,
) -> dict[str, Any]:
    release_id = ensure_release_id(release_id)
    source_commit = ensure_commit(source_commit)
    deployed_source_commit = ensure_commit(deployed_source_commit)
    require(block_number > 0, "legacy source inventory may not use genesis block zero")
    block_hash = ensure_hash256(block_hash, "legacy source inventory block hash")
    parse_utc(observed_at, "legacy source inventory observedAtUtc")
    finalized = rpc_query(rpc, "chain_getFinalizedHead", [])
    at_number = rpc_query(rpc, "chain_getBlockHash", [block_number])
    header = rpc_query(rpc, "chain_getHeader", [block_hash])
    require(finalized["result"] == block_hash, "isolated RPC finalized head differs from the frozen block")
    require(at_number["result"] == block_hash, "isolated RPC block-number hash differs from the frozen block")

    version_query = rpc_query(
        rpc, "state_getStorage", [LEGACY_TCG_STORAGE_VERSION_KEY, block_hash]
    )
    next_query = rpc_query(rpc, "state_getStorage", [LEGACY_NEXT_CARD_ID_KEY, block_hash])
    next_game_config = LEGACY_GAME_AUTHORITY_STORAGE["nextGameId"]
    next_game_query = rpc_query(
        rpc, "state_getStorage", [next_game_config["key"], block_hash]
    )

    def collect_pages(
        prefix: str, page_size: int, maximum_keys: int, label: str
    ) -> list[dict[str, Any]]:
        pages: list[dict[str, Any]] = []
        start_key: str | None = None
        while True:
            params: list[Any] = [prefix, page_size, start_key, block_hash]
            keys = rpc.call("state_getKeysPaged", params)
            require(isinstance(keys, list), f"{label} RPC page is not an array")
            pages.append({"startKey": start_key, "keys": keys})
            total = sum(len(page["keys"]) for page in pages)
            require(total <= maximum_keys, f"{label} inventory exceeds the collector bound")
            if len(keys) < page_size:
                break
            require(keys, f"{label} RPC returned an impossible full empty page")
            start_key = keys[-1]
        return pages

    pages = collect_pages(
        LEGACY_CARDS_PREFIX,
        LEGACY_CARD_KEY_PAGE_SIZE,
        MAX_LEGACY_CARD_KEYS,
        "legacy Cards",
    )
    authority_pages = {
        alias: collect_pages(
            str(config["prefix"]),
            LEGACY_AUTHORITY_KEY_PAGE_SIZE,
            MAX_LEGACY_AUTHORITY_KEYS_PER_STORAGE,
            f"legacy GameAuthority {config['storage']}",
        )
        for alias, config in LEGACY_GAME_AUTHORITY_STORAGE.items()
        if alias != "nextGameId"
    }
    require(
        rpc.call("chain_getFinalizedHead", []) == block_hash,
        "isolated RPC finalized head changed during inventory capture",
    )

    provisional = {
        "schemaVersion": 2,
        "kind": LEGACY_SOURCE_INVENTORY_KIND,
        "releaseId": release_id,
        "sourceCommit": source_commit,
        "deployedSourceCommit": deployed_source_commit,
        "observedAtUtc": observed_at,
        "observedAtFinalizedBlock": {"number": block_number, "hash": block_hash},
        "captureMode": "isolated-frozen-copy-read-only",
        "finality": {
            "finalizedHead": finalized,
            "blockHashAtNumber": at_number,
            "header": header,
        },
        "storage": {
            "tcgStorageVersion": {
                "pallet": "EterraTCG",
                "storage": ":__STORAGE_VERSION__:",
                "key": LEGACY_TCG_STORAGE_VERSION_KEY,
                "query": version_query,
            },
            "nextCardId": {
                "pallet": "EterraTCG",
                "storage": "NextCardId",
                "key": LEGACY_NEXT_CARD_ID_KEY,
                "query": next_query,
            },
            "gameAuthority": {
                "nextGameId": {
                    "pallet": "EterraGameAuthority",
                    "storage": next_game_config["storage"],
                    "key": next_game_config["key"],
                    "query": next_game_query,
                },
                **{
                    alias: {
                        "pallet": "EterraGameAuthority",
                        "storage": config["storage"],
                        "prefix": config["prefix"],
                        "method": "state_getKeysPaged",
                        "at": block_hash,
                        "pageSize": LEGACY_AUTHORITY_KEY_PAGE_SIZE,
                        "pages": authority_pages[alias],
                    }
                    for alias, config in LEGACY_GAME_AUTHORITY_STORAGE.items()
                    if alias != "nextGameId"
                },
            },
            "cards": {
                "pallet": "EterraTCG",
                "storage": "Cards",
                "prefix": LEGACY_CARDS_PREFIX,
                "method": "state_getKeysPaged",
                "at": block_hash,
                "pageSize": LEGACY_CARD_KEY_PAGE_SIZE,
                "pages": pages,
            },
        },
        "summary": {},
        "safety": dict(LEGACY_SAFETY),
    }
    storage_cards = provisional["storage"]["cards"]
    card_ids = [
        decode_legacy_card_key(key)
        for page in storage_cards["pages"]
        for key in page["keys"]
    ]
    next_card_id = decode_scale_u32(next_query["result"], "legacy NextCardId result")
    authority_counts = {
        alias: sum(len(page["keys"]) for page in pages)
        for alias, pages in authority_pages.items()
    }
    next_game_id = decode_scale_u64(
        next_game_query["result"], "legacy GameAuthority NextGameId result"
    )
    provisional["summary"] = {
        "cardIdsSha256": sha256_bytes(
            b"".join(card_id.to_bytes(4, "little") for card_id in sorted(card_ids))
        ),
        "cardsCount": len(card_ids),
        "maxCardId": max(card_ids) if card_ids else None,
        "minimumMigrationBlocks": minimum_v16_migration_blocks(next_card_id),
        "nextCardId": next_card_id,
        "gameAuthorityActivePlayerLocks": authority_counts["activeGameByPlayer"],
        "gameAuthorityEliminationRecords": authority_counts["eliminations"],
        "gameAuthorityEndCommandsProcessed": authority_counts["processedEndCommands"],
        "gameAuthorityEliminationEventsProcessed": authority_counts[
            "processedEliminationEvents"
        ],
        "gameAuthorityGames": authority_counts["games"],
        "gameAuthorityNextGameId": next_game_id,
        "tcgStorageVersion": 14,
        "v16MigrationBatchSize": V16_MIGRATION_BATCH_SIZE,
    }
    validate_legacy_source_inventory_value(
        provisional, release_id=release_id, source_commit=source_commit
    )
    return provisional


def command_capture_legacy_source_inventory(args: argparse.Namespace) -> None:
    inventory = collect_legacy_source_inventory(
        FrozenInventoryRpc(args.rpc_url, args.rpc_timeout_seconds),
        release_id=args.release_id,
        source_commit=args.source_commit,
        deployed_source_commit=args.deployed_source_commit,
        block_number=args.block_number,
        block_hash=args.block_hash,
        observed_at=args.observed_at or utc_now(),
    )
    output = Path(args.output).resolve()
    observation_output = Path(args.storage_version_observation_output).resolve()
    require(output != observation_output, "inventory and storage-version outputs must differ")
    write_new_json(output, inventory)
    summary = validate_legacy_source_inventory(
        output, inventory["releaseId"], inventory["sourceCommit"]
    )
    version_result = inventory["storage"]["tcgStorageVersion"]["query"]["result"]
    observation = {
        "schemaVersion": 1,
        "kind": "frame-pallet-storage-version-observation",
        "finalizedBlock": inventory["observedAtFinalizedBlock"],
        "decoded": {"scaleType": "u16", "storageVersion": 14},
        "derivation": {
            "palletName": "EterraTCG",
            "postfix": ":__STORAGE_VERSION__:",
            "rustHelper": "alpha_v2_release.py",
            "storageKeyFormula": "Twox128(palletName) ++ Twox128(postfix)",
        },
        "liveSource": {
            "commit": inventory["deployedSourceCommit"],
            "declaredStorageVersion": 14,
        },
        "readOnlyRpc": {
            "method": "state_getStorage",
            "storageKey": LEGACY_TCG_STORAGE_VERSION_KEY,
            "result": version_result,
        },
    }
    write_new_json(observation_output, observation)
    print(
        "frozen legacy source inventory captured: "
        f"block={summary['blockNumber']} nextCardId={summary['nextCardId']} "
        f"cards={summary['cardsCount']} minimumMigrationBlocks={summary['minimumMigrationBlocks']} "
        f"sha256={summary['sha256']}"
    )


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
            "legacySourceInventorySha256",
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
    source_inventory_hash = ensure_sha256(
        source_runtime.get("legacySourceInventorySha256"),
        "pre-V16 legacy source inventory SHA-256",
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
        "legacySourceInventorySha256": source_inventory_hash,
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
    schema_version = inventory.get("schemaVersion")
    expected_keys = {
        "schemaVersion",
        "kind",
        "releaseId",
        "sourceCommit",
        "observedAtFinalizedBlock",
        "counts",
    }
    if schema_version == 2:
        expected_keys.add("observationEvidence")
    require_exact_keys(
        inventory,
        expected_keys,
        "acceptance inventory",
    )
    require(schema_version in {1, 2}, "acceptance inventory schema mismatch")
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
    observation_evidence = None
    if schema_version == 2:
        observation_evidence = require_exact_keys(
            inventory.get("observationEvidence"),
            PRE_V16_ACCEPTANCE_OBSERVATION_EVIDENCE_KEYS,
            "pre-V16 acceptance observation evidence",
        )
        require(
            observation_evidence.get("captureMode")
            == "isolated-frozen-copy-read-only",
            "pre-V16 acceptance observation was not captured from the isolated frozen copy",
        )
        for name in (
            "legacySourceInventorySha256",
            "runtimeMetadataScaleSha256",
            "tcgStorageVersionObservationSha256",
        ):
            ensure_sha256(str(observation_evidence.get(name, "")), name)
        require(
            observation_evidence.get("v2CountersDerivedFromStructuralAbsence") is True,
            "pre-V16 V2 counters were not derived from structural absence",
        )
    return {
        "value": inventory,
        "releaseId": inventory_release,
        "sourceCommit": inventory_commit,
        "blockNumber": block_number,
        "blockHash": block_hash,
        "sha256": sha256_file(path),
        "nonzero": nonzero,
        "observationEvidence": observation_evidence,
        "schemaVersion": schema_version,
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
    source_inventory: Mapping[str, Any],
) -> dict[str, Any]:
    evidence = read_json(path)
    require(evidence.get("schemaVersion") == 1, "migration evidence schema mismatch")
    require(evidence.get("kind") == "nexus-v2-v14-v16-migration-evidence", "migration evidence kind mismatch")
    require(evidence.get("releaseId") == release_id, "migration evidence release mismatch")
    require(evidence.get("sourceCommit") == source_commit, "migration evidence source commit mismatch")
    require(evidence.get("backupManifestSha256") == manifest_hash, "migration evidence manifest mismatch")
    require(evidence.get("fromStorageVersion") == 14 and evidence.get("toStorageVersion") == 16, "migration evidence version mismatch")
    require(
        evidence.get("legacySourceInventorySha256") == source_inventory["sha256"],
        "migration evidence source-inventory hash mismatch",
    )
    require(
        evidence.get("legacySourceInventoryFinalizedBlock")
        == {
            "number": source_inventory["blockNumber"],
            "hash": source_inventory["blockHash"],
        },
        "migration evidence source-inventory block mismatch",
    )
    require(
        evidence.get("legacyNextCardId") == source_inventory["nextCardId"]
        and evidence.get("legacyCardsCount") == source_inventory["cardsCount"]
        and evidence.get("legacyMaxCardId") == source_inventory["maxCardId"],
        "migration evidence source-inventory summary mismatch",
    )
    require(
        evidence.get("v16MigrationBatchSize") == V16_MIGRATION_BATCH_SIZE,
        "migration evidence batch size mismatch",
    )
    require(
        evidence.get("minimumMigrationBlocks") == source_inventory["minimumMigrationBlocks"],
        "migration evidence minimum block calculation mismatch",
    )
    supplied_blocks = evidence.get("tryRuntimeFastForwardBlocks")
    require(
        isinstance(supplied_blocks, int)
        and not isinstance(supplied_blocks, bool)
        and supplied_blocks >= source_inventory["minimumMigrationBlocks"],
        "migration evidence used insufficient fast-forward blocks",
    )
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
    runtime_metadata = find_artifact(
        verified, bundle_root, "node", "runtime-v14-metadata"
    )
    observation_path = find_artifact(
        verified,
        bundle_root,
        "node",
        "tcg-storage-version-observation",
    )
    source_inventory_path = find_artifact(
        verified,
        bundle_root,
        "node",
        "legacy-source-inventory",
    )
    require(
        sha256_file(runtime_v14) == gates["runtimeV14WasmSha256"],
        "pre-V16 gate runtime hash does not match the pinned V14 Wasm",
    )
    require(
        sha256_file(runtime_metadata) == gates["runtimeMetadataScaleSha256"],
        "pre-V16 gate metadata hash does not match the pinned V14 SCALE metadata",
    )
    require(
        sha256_file(observation_path) == gates["tcgStorageVersionObservationSha256"],
        "pre-V16 gate observation hash does not match the pinned TCG observation",
    )
    require(
        sha256_file(source_inventory_path) == gates["legacySourceInventorySha256"],
        "pre-V16 gate source-inventory hash does not match the pinned legacy inventory",
    )

    source_inventory = validate_legacy_source_inventory(
        source_inventory_path,
        verified["releaseId"],
        verified["sourceCommit"],
    )
    require(
        (source_inventory["blockNumber"], source_inventory["blockHash"])
        == (gates["blockNumber"], gates["blockHash"]),
        "pre-V16 gate block does not match the pinned legacy source inventory",
    )
    require(
        source_inventory["deployedSourceCommit"] == gates["deployedSourceCommit"],
        "pre-V16 deployed source commit does not match the pinned legacy inventory",
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
    require(
        ensure_hash256(
            read_only_rpc.get("storageKey"),
            "pinned TCG storage-version key",
        )
        == LEGACY_TCG_STORAGE_VERSION_KEY,
        "pinned TCG storage-version key is not the EterraTCG storage-version key",
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


def validate_pre_v16_acceptance_inventory_binding(
    inventory: Mapping[str, Any],
    gates: Mapping[str, Any],
    source_inventory: Mapping[str, Any],
) -> None:
    require(
        inventory.get("schemaVersion") == 2,
        "pre-V16 reset requires evidence-bound acceptance inventory schema 2",
    )
    evidence = inventory.get("observationEvidence")
    require(isinstance(evidence, Mapping), "pre-V16 acceptance evidence is missing")
    require(
        evidence.get("legacySourceInventorySha256")
        == gates["legacySourceInventorySha256"]
        == source_inventory["sha256"],
        "pre-V16 acceptance inventory source-inventory hash mismatch",
    )
    require(
        evidence.get("runtimeMetadataScaleSha256")
        == gates["runtimeMetadataScaleSha256"],
        "pre-V16 acceptance inventory metadata hash mismatch",
    )
    require(
        evidence.get("tcgStorageVersionObservationSha256")
        == gates["tcgStorageVersionObservationSha256"],
        "pre-V16 acceptance inventory TCG observation hash mismatch",
    )
    counts = {name: 0 for name in ACCEPTANCE_COUNT_FIELDS}
    counts.update(
        {
            "currentLegacyAuthorityGames": source_inventory["gameAuthorityGames"],
            "currentLegacyAuthorityActivePlayerLocks": source_inventory[
                "gameAuthorityActivePlayerLocks"
            ],
            "currentLegacyAuthorityEliminationRecords": source_inventory[
                "gameAuthorityEliminationRecords"
            ],
            "lifetimeLegacyAuthorityGamesCreated": source_inventory[
                "gameAuthorityNextGameId"
            ],
            "lifetimeLegacyAuthorityEndCommandsProcessed": source_inventory[
                "gameAuthorityEndCommandsProcessed"
            ],
            "lifetimeLegacyAuthorityEliminationEventsProcessed": source_inventory[
                "gameAuthorityEliminationEventsProcessed"
            ],
            "lifetimeLegacyAuthorityAcceptanceWritesLowerBound": (
                source_inventory["gameAuthorityNextGameId"]
                + source_inventory["gameAuthorityEndCommandsProcessed"]
                + source_inventory["gameAuthorityEliminationEventsProcessed"]
            ),
        }
    )
    require(
        inventory["value"]["counts"] == counts,
        "pre-V16 acceptance counts are not derived from the frozen RPC inventory",
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
    source_inventory = validate_legacy_source_inventory(
        find_artifact(verified, bundle_root, "node", "legacy-source-inventory"),
        verified["releaseId"],
        verified["sourceCommit"],
    )
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
        source_inventory,
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
    reset_blocking = dict(inventory["nonzero"])
    if gates["mode"] == PRE_V16_FRESH_RESET_GATE_MODE:
        validate_pre_v16_acceptance_inventory_binding(
            inventory, gates, source_inventory
        )
        # Legacy GameAuthority history belongs to the backed-up source Alpha,
        # not the fresh V2 state.  It must remain truthful in the inventory but
        # does not cancel the separately approved fresh-genesis replacement.
        for field in LEGACY_AUTHORITY_ACCEPTANCE_COUNT_FIELDS:
            reset_blocking.pop(field, None)
    else:
        require(
            inventory["schemaVersion"] == 1,
            "post-V16 reset requires deterministic acceptance-boundary inventory schema 1",
        )
    require(
        not reset_blocking,
        "reset readiness requires zero V2 acceptance state",
    )
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
        raise SafetyError(
            "automatic rollback is permanently blocked after any V2 or legacy acceptance write exists"
        )

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

    source_inventory = subparsers.add_parser(
        "capture-legacy-source-inventory",
        help="capture V14 legacy inventory from an isolated frozen loopback RPC",
    )
    source_inventory.add_argument("--rpc-url", required=True)
    source_inventory.add_argument("--rpc-timeout-seconds", type=float, default=10.0)
    source_inventory.add_argument("--release-id", required=True)
    source_inventory.add_argument("--source-commit", required=True)
    source_inventory.add_argument("--deployed-source-commit", required=True)
    source_inventory.add_argument("--block-number", type=int, required=True)
    source_inventory.add_argument("--block-hash", required=True)
    source_inventory.add_argument("--observed-at")
    source_inventory.add_argument("--storage-version-observation-output", required=True)
    source_inventory.add_argument("--output", required=True)
    source_inventory.set_defaults(handler=command_capture_legacy_source_inventory)

    migration = subparsers.add_parser("rehearse-migration", help="record V14-to-V16 copied-state evidence")
    migration.add_argument("--manifest", required=True)
    migration.add_argument("--bundle-root", required=True)
    migration.add_argument("--try-runtime-bin", required=True)
    migration.add_argument("--try-runtime-revision", required=True)
    migration.add_argument("--try-runtime-sha256", required=True)
    migration.add_argument(
        "--migration-blocks",
        type=int,
        help="optional override; omitted uses the deterministic minimum from frozen NextCardId",
    )
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
    except (SafetyError, ValueError) as exc:
        print(f"nexus-v2-private-alpha: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
