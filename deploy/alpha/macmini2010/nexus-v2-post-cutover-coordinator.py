#!/usr/bin/env python3
"""Coordinate a hash-pinned Nexus V2 private-alpha rollback without owning secrets.

The coordinator performs no SSH, RPC, Docker, or deployment operation itself.
It validates the complete local evidence contract and invokes exactly pinned,
clean-worktree component drivers. Every executable component action is preceded
by, and bound to, a successful dry-run receipt.
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
from typing import Any, Mapping, Sequence


REPO_ROOT = Path(__file__).resolve().parents[3]
SAFETY_TOOL_DIR = REPO_ROOT / "scripts/nexus-v2-private-alpha"
sys.path.insert(0, str(SAFETY_TOOL_DIR))
import alpha_v2_release as safety  # noqa: E402
import acceptance_boundary as boundary  # noqa: E402


SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
OPERATION_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
EXPECTED_COMPONENTS = {"chain-media", "site-indexer"}
EXPECTED_ARCHIVES = {
    "chain-media": {"node", "media"},
    "site-indexer": {"site"},
}
REQUIRED_SOURCE_PINS = {
    "chain-media": {"chain", "media"},
    "site-indexer": {"chain", "site"},
}
REQUIRED_SCRIPT_ROLES = {
    "chain-media": {
        "restoreState",
        "deployNode",
        "deployMedia",
        "status",
    },
    "site-indexer": {
        "restoreState",
        "deploySite",
        "status",
    },
}
ACTIONS = (
    "post-cutover-smoke",
    "pause-v2-writes",
    "archive-failed-v2",
    "restore-final-backup",
    "restored-smoke",
)
PLAN_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "releaseId",
    "sourceCommit",
    "genesisHash",
    "runtimeCodeSha256",
    "runtimeMetadataScaleSha256",
    "runtimeBundleManifestSha256",
    "freshResetReadinessSha256",
    "finalBackupManifestSha256",
    "restoreEvidenceSha256",
    "postCutoverObservationSha256",
    "acceptanceBoundaryCaptureSha256",
    "ingressClosedEvidenceSha256",
    "coordinatorSha256",
    "maxObservationAgeSeconds",
    "automaticRestoreApproved",
    "paidOrPublicActivationAuthorized",
    "createdAtUtc",
    "expiresAtUtc",
    "components",
}
COMPONENT_KEYS = {
    "id",
    "sourcePins",
    "driverSourceId",
    "driverPath",
    "driverSha256",
    "requiredResetArchives",
    "scriptPins",
}
SOURCE_PIN_KEYS = {"id", "root", "expectedCommit"}
SCRIPT_PIN_KEYS = {"sourceId", "path", "sha256"}
OBSERVATION_KEYS = {
    "schemaVersion",
    "kind",
    "releaseId",
    "sourceCommit",
    "componentSourceCommits",
    "observedAtFinalizedBlock",
    "observedAtUtc",
    "writeBarrier",
    "acceptanceBoundaryCaptureSha256",
    "ingressClosedEvidenceSha256",
    "economicGatesSha256",
    "acceptanceInventorySha256",
}
WRITE_BARRIER_KEYS = {
    "mode",
    "chainWritesPaused",
    "authorityResultsPaused",
    "webMutationsPaused",
    "gameplaySessionIngressPaused",
    "inventoryObservedAfterPause",
    "pausedAtUtc",
    "stabilityWindowSeconds",
    "evidenceSha256",
}
RESTORE_EVIDENCE_KEYS = {
    "schemaVersion",
    "kind",
    "releaseId",
    "sourceCommit",
    "backupManifestSha256",
    "restoreDriverSha256",
    "portsPlanSha256",
    "restoreLogSha256",
    "restoreResultSha256",
    "isolatedRoot",
    "bindHost",
    "ports",
    "result",
    "completedAtUtc",
    "liveAlphaTouched",
}
RESULT_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "planSha256",
    "releaseId",
    "sourceCommit",
    "componentSourceCommits",
    "componentId",
    "action",
    "mode",
    "result",
    "remoteActionsExecuted",
    "alreadyApplied",
    "requiredResetArchives",
    "failedV2RootArchiveSha256",
    "remoteIdempotencyMarkerSha256",
    "checks",
    "completedAtUtc",
}
EVIDENCE_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "planSha256",
    "releaseId",
    "sourceCommit",
    "genesisHash",
    "runtimeCodeSha256",
    "runtimeMetadataScaleSha256",
    "componentSourceCommits",
    "decision",
    "postCutoverSmokePassed",
    "automaticRestorePerformed",
    "postAcceptanceContainmentPerformed",
    "finalBackupManifestSha256",
    "restoreEvidenceSha256",
    "postCutoverObservationSha256",
    "acceptanceBoundaryCaptureSha256",
    "ingressClosedEvidenceSha256",
    "economicGatesSha256",
    "acceptanceInventorySha256",
    "observedAtFinalizedBlock",
    "nonzeroAcceptanceAssets",
    "componentMarkerSha256",
    "completedAtUtc",
}
DRY_CHECKS = {
    "post-cutover-smoke": {
        "sourceIdentityPinned",
        "credentialsResolvable",
        "requiredResetArchivesPresent",
        "smokeProbePlanned",
    },
    "pause-v2-writes": {
        "sourceIdentityPinned",
        "credentialsResolvable",
        "requiredResetArchivesPresent",
        "pausePlanSafe",
        "restoreExcluded",
    },
    "archive-failed-v2": {
        "sourceIdentityPinned",
        "credentialsResolvable",
        "requiredResetArchivesPresent",
        "archivePlanSafe",
        "restoreExcluded",
    },
    "restore-final-backup": {
        "sourceIdentityPinned",
        "credentialsResolvable",
        "requiredResetArchivesPresent",
        "finalBackupInputsMatched",
        "failedV2ArchiveRequired",
        "existingRestoreScriptPinned",
        "existingDeployScriptsPinned",
        "restorePlanSafe",
    },
    "restored-smoke": {
        "sourceIdentityPinned",
        "credentialsResolvable",
        "requiredResetArchivesPresent",
        "restoredSmokeProbePlanned",
    },
}
EXECUTE_CHECKS = {
    "post-cutover-smoke": {
        "sourceIdentityPinned",
        "requiredResetArchivesPresent",
        "smokePassed",
    },
    "pause-v2-writes": {
        "sourceIdentityPinned",
        "requiredResetArchivesPresent",
        "v2WritesPaused",
        "statePreserved",
        "restoreNotAttempted",
    },
    "archive-failed-v2": {
        "sourceIdentityPinned",
        "requiredResetArchivesPresent",
        "failedV2RootArchived",
        "archiveManifestImmutable",
    },
    "restore-final-backup": {
        "sourceIdentityPinned",
        "requiredResetArchivesPresent",
        "failedV2RootArchivePresent",
        "finalBackupHashesVerified",
        "restoreEvidenceMatched",
        "existingRestoreScriptUsed",
        "existingDeployScriptsUsed",
        "restoreCompleted",
    },
    "restored-smoke": {
        "sourceIdentityPinned",
        "requiredResetArchivesPresent",
        "failedV2RootArchivePresent",
        "componentHealthy",
        "backupIdentityReadback",
        "economicFlagsDisabled",
    },
}


class CoordinatorError(RuntimeError):
    """A local or component rollback safety contract failed."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CoordinatorError(message)


def utc_now() -> str:
    return (
        dt.datetime.now(dt.timezone.utc)
        .replace(microsecond=0)
        .isoformat()
        .replace("+00:00", "Z")
    )


def parse_utc(value: Any, label: str) -> dt.datetime:
    require(isinstance(value, str) and value, f"{label} must be an ISO-8601 string")
    try:
        parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise CoordinatorError(f"{label} is not valid ISO-8601") from exc
    require(parsed.tzinfo is not None, f"{label} must include a timezone")
    return parsed.astimezone(dt.timezone.utc)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def ensure_sha256(value: Any, label: str) -> str:
    require(
        isinstance(value, str) and bool(SHA256_RE.fullmatch(value)),
        f"{label} must be 64 lowercase hex characters",
    )
    return value


def ensure_regular_file(path: Path, label: str, *, executable: bool = False) -> Path:
    require(path.exists(), f"{label} not found: {path}")
    require(not path.is_symlink(), f"{label} must not be a symlink: {path}")
    require(path.is_file(), f"{label} must be a regular file: {path}")
    if executable:
        mode = path.stat().st_mode
        require(
            bool(mode & (stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)),
            f"{label} must be executable: {path}",
        )
    return path.resolve()


def duplicate_rejecting_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        require(key not in value, f"duplicate JSON field: {key}")
        value[key] = item
    return value


def read_json(path: Path, label: str) -> dict[str, Any]:
    ensure_regular_file(path, label)
    try:
        value = json.loads(
            path.read_text(encoding="utf-8"),
            object_pairs_hook=duplicate_rejecting_object,
        )
    except (OSError, json.JSONDecodeError) as exc:
        raise CoordinatorError(f"invalid {label}: {path}") from exc
    require(isinstance(value, dict), f"{label} root must be an object")
    return value


def exact_keys(value: Any, expected: set[str], label: str) -> Mapping[str, Any]:
    require(isinstance(value, dict), f"{label} must be an object")
    require(set(value) == expected, f"{label} fields do not match the closed schema")
    return value


def require_schema_one(value: Mapping[str, Any], label: str) -> None:
    version = value.get("schemaVersion")
    require(
        isinstance(version, int)
        and not isinstance(version, bool)
        and version == 1,
        f"{label} schema mismatch",
    )


def write_new_json(path: Path, value: Mapping[str, Any], mode: int = 0o600) -> None:
    require(not path.exists(), f"refusing to overwrite immutable output: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, mode)
    with os.fdopen(descriptor, "wb") as handle:
        handle.write(payload)


def git_output(root: Path, *args: str) -> str:
    completed = subprocess.run(
        ["git", "-C", str(root), *args],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    require(
        completed.returncode == 0,
        f"git {' '.join(args)} failed for component root: {root}",
    )
    return completed.stdout.strip()


def validate_clean_component_root(root_value: Any, expected_commit: str, label: str) -> Path:
    require(isinstance(root_value, str) and root_value, f"{label} sourceRoot is missing")
    root = Path(root_value)
    require(root.exists() and root.is_dir(), f"{label} source root not found: {root}")
    require(not root.is_symlink(), f"{label} source root must not be a symlink")
    root = root.resolve()
    require(
        git_output(root, "rev-parse", "--is-inside-work-tree") == "true",
        f"{label} source root is not a Git worktree",
    )
    require(
        Path(git_output(root, "rev-parse", "--show-toplevel")).resolve() == root,
        f"{label} sourceRoot must be the Git worktree root",
    )
    actual_commit = git_output(root, "rev-parse", "HEAD")
    require(actual_commit == expected_commit, f"{label} source commit is not pinned")
    require(
        not git_output(root, "status", "--porcelain", "--untracked-files=all"),
        f"{label} source worktree is not clean",
    )
    return root


def resolve_pinned_script(
    root: Path,
    raw: Mapping[str, Any],
    label: str,
    *,
    executable: bool,
) -> Path:
    path_value = raw["path"]
    require(isinstance(path_value, str) and path_value, f"{label} path is missing")
    relative = Path(path_value)
    require(not relative.is_absolute(), f"{label} path must be relative to sourceRoot")
    require(".." not in relative.parts, f"{label} path may not contain '..'")
    path = ensure_regular_file(root / relative, label, executable=executable)
    git_output(root, "ls-files", "--error-unmatch", relative.as_posix())
    for parent in (root / relative, *(root / relative).parents):
        if parent == root.parent:
            break
        require(not parent.is_symlink(), f"{label} traverses a symlink")
        if parent == root:
            break
    try:
        path.relative_to(root)
    except ValueError as exc:
        raise CoordinatorError(f"{label} escapes sourceRoot") from exc
    expected_hash = ensure_sha256(raw["sha256"], f"{label} SHA-256")
    require(sha256_file(path) == expected_hash, f"{label} hash drifted")
    return path


def validate_plan(path: Path, expected_hash: str) -> dict[str, Any]:
    ensure_sha256(expected_hash, "NEXUS_V2_ROLLBACK_PLAN_SHA256")
    plan_path = ensure_regular_file(path, "rollback plan")
    actual_hash = sha256_file(plan_path)
    require(actual_hash == expected_hash, "rollback plan hash does not match NEXUS_V2_ROLLBACK_PLAN_SHA256")
    plan = read_json(plan_path, "rollback plan")
    exact_keys(plan, PLAN_KEYS, "rollback plan")
    require_schema_one(plan, "rollback plan")
    require(
        plan["kind"] == "nexus-v2-private-alpha-post-cutover-coordinator-plan",
        "rollback plan kind mismatch",
    )
    operation_id = plan["operationId"]
    require(
        isinstance(operation_id, str) and bool(OPERATION_RE.fullmatch(operation_id)),
        "invalid rollback operationId",
    )
    release_id = safety.ensure_release_id(str(plan["releaseId"]))
    source_commit = safety.ensure_commit(str(plan["sourceCommit"]))
    genesis_hash = safety.ensure_hash256(plan["genesisHash"], "coordinator genesis hash")
    runtime_code_hash = ensure_sha256(
        plan["runtimeCodeSha256"],
        "coordinator runtime code SHA-256",
    )
    metadata_hash = ensure_sha256(
        plan["runtimeMetadataScaleSha256"],
        "coordinator runtime metadata SCALE SHA-256",
    )
    runtime_manifest_hash = ensure_sha256(
        plan["runtimeBundleManifestSha256"],
        "coordinator runtime bundle manifest SHA-256",
    )
    require(
        runtime_code_hash
        == boundary.runtime_bundle.PRODUCTION_PINS.production_wasm_sha256,
        "coordinator runtime code is not the frozen production Wasm SHA-256",
    )
    require(
        metadata_hash == boundary.runtime_bundle.PRODUCTION_PINS.metadata_scale_sha256,
        "coordinator runtime metadata is not the frozen SCALE SHA-256",
    )
    require(
        runtime_manifest_hash == boundary.runtime_bundle.PRODUCTION_PINS.manifest_sha256,
        "coordinator runtime bundle is not the frozen Linux release",
    )
    readiness_hash = ensure_sha256(
        plan["freshResetReadinessSha256"],
        "fresh-reset readiness SHA-256",
    )
    manifest_hash = ensure_sha256(
        plan["finalBackupManifestSha256"],
        "final backup manifest SHA-256",
    )
    restore_hash = ensure_sha256(
        plan["restoreEvidenceSha256"],
        "restore evidence SHA-256",
    )
    observation_hash = ensure_sha256(
        plan["postCutoverObservationSha256"],
        "post-cutover observation SHA-256",
    )
    capture_hash = ensure_sha256(
        plan["acceptanceBoundaryCaptureSha256"],
        "acceptance-boundary capture SHA-256",
    )
    ingress_hash = ensure_sha256(
        plan["ingressClosedEvidenceSha256"],
        "ingress-closed evidence SHA-256",
    )
    coordinator_hash = ensure_sha256(plan["coordinatorSha256"], "coordinator SHA-256")
    require(
        coordinator_hash == sha256_file(Path(__file__).resolve()),
        "rollback coordinator does not match the plan pin",
    )
    require(
        plan["automaticRestoreApproved"] is True,
        "pre-acceptance automatic restore is not approved by the pinned plan",
    )
    require(
        plan["paidOrPublicActivationAuthorized"] is False,
        "rollback plan may not authorize paid or public activation",
    )
    maximum_age = plan["maxObservationAgeSeconds"]
    require(
        isinstance(maximum_age, int)
        and not isinstance(maximum_age, bool)
        and 30 <= maximum_age <= 900,
        "maxObservationAgeSeconds must be in 30..900",
    )
    created_at = parse_utc(plan["createdAtUtc"], "rollback plan createdAtUtc")
    expires_at = parse_utc(plan["expiresAtUtc"], "rollback plan expiresAtUtc")
    now = dt.datetime.now(dt.timezone.utc)
    require(created_at <= now + dt.timedelta(seconds=30), "rollback plan creation time is in the future")
    require(expires_at > now, "rollback plan expired")
    require(expires_at > created_at, "rollback plan expiry must follow creation")
    require(
        expires_at - created_at <= dt.timedelta(hours=1),
        "rollback plan authorization window may not exceed one hour",
    )

    components_raw = plan["components"]
    require(isinstance(components_raw, list), "rollback plan components must be an array")
    require(len(components_raw) == len(EXPECTED_COMPONENTS), "rollback plan must contain exactly two components")
    components: dict[str, dict[str, Any]] = {}
    for raw in components_raw:
        exact_keys(raw, COMPONENT_KEYS, "rollback component")
        component_id = raw["id"]
        require(component_id in EXPECTED_COMPONENTS, "unexpected rollback component")
        require(component_id not in components, f"duplicate rollback component: {component_id}")
        source_pins_raw = raw["sourcePins"]
        require(isinstance(source_pins_raw, list), f"{component_id} sourcePins must be an array")
        source_roots: dict[str, Path] = {}
        source_commits: dict[str, str] = {}
        for source_pin in source_pins_raw:
            exact_keys(source_pin, SOURCE_PIN_KEYS, f"{component_id} source pin")
            source_id = source_pin["id"]
            require(
                source_id in REQUIRED_SOURCE_PINS[component_id],
                f"{component_id} has an unexpected source pin",
            )
            require(source_id not in source_roots, f"duplicate source pin: {component_id}:{source_id}")
            expected_commit = safety.ensure_commit(str(source_pin["expectedCommit"]))
            source_roots[source_id] = validate_clean_component_root(
                source_pin["root"],
                expected_commit,
                f"{component_id}:{source_id}",
            )
            source_commits[source_id] = expected_commit
        require(
            set(source_roots) == REQUIRED_SOURCE_PINS[component_id],
            f"{component_id} source pin set mismatch",
        )
        if component_id == "chain-media":
            require(
                source_commits["chain"] == source_commit,
                "chain source pin must match the coordinated chain source commit",
            )
        elif component_id == "site-indexer":
            require(
                source_commits["chain"] == source_commit,
                "site coordinator chain pin must match the coordinated chain source commit",
            )
        driver_source_id = raw["driverSourceId"]
        require(
            driver_source_id in source_roots,
            f"{component_id} driverSourceId is not pinned",
        )
        driver_value = {
            "sourceId": driver_source_id,
            "path": raw["driverPath"],
            "sha256": raw["driverSha256"],
        }
        driver = resolve_pinned_script(
            source_roots[driver_source_id],
            driver_value,
            f"{component_id} action driver",
            executable=True,
        )
        archives = raw["requiredResetArchives"]
        require(isinstance(archives, dict), f"{component_id} requiredResetArchives must be an object")
        require(
            set(archives) == EXPECTED_ARCHIVES[component_id],
            f"{component_id} reset archive names mismatch",
        )
        archive_suffixes: dict[str, str] = {}
        for archive_name, archive_path in archives.items():
            require(
                isinstance(archive_path, str) and archive_path.startswith("/"),
                f"{component_id} archive path must be absolute",
            )
            suffix = (
                f"/archive/nexus-v2-fresh-reset/{readiness_hash}/{archive_name}"
            )
            require(
                archive_path.rstrip("/").endswith(suffix),
                f"{component_id} archive path is not bound to the readiness hash",
            )
            archive_suffixes[archive_name] = archive_path.rstrip("/")

        pins = raw["scriptPins"]
        require(isinstance(pins, dict), f"{component_id} scriptPins must be an object")
        require(
            set(pins) == REQUIRED_SCRIPT_ROLES[component_id],
            f"{component_id} script pin roles mismatch",
        )
        resolved_pins: dict[str, dict[str, str]] = {}
        for role, pin in pins.items():
            exact_keys(pin, SCRIPT_PIN_KEYS, f"{component_id} {role}")
            pin_source_id = pin["sourceId"]
            require(
                pin_source_id in source_roots,
                f"{component_id} {role} sourceId is not pinned",
            )
            script = resolve_pinned_script(
                source_roots[pin_source_id],
                pin,
                f"{component_id} {role}",
                executable=True,
            )
            resolved_pins[role] = {
                "sourceId": pin_source_id,
                "path": str(script),
                "sha256": sha256_file(script),
            }
        components[component_id] = {
            "id": component_id,
            "sourceRoots": {key: str(value) for key, value in source_roots.items()},
            "sourceCommits": source_commits,
            "driverSourceId": driver_source_id,
            "driverPath": str(driver),
            "driverSha256": sha256_file(driver),
            "requiredResetArchives": archive_suffixes,
            "scriptPins": resolved_pins,
        }
    require(set(components) == EXPECTED_COMPONENTS, "rollback component set mismatch")
    global_sources: dict[str, tuple[str, str]] = {}
    for component in components.values():
        for source_id, source_root in component["sourceRoots"].items():
            identity = (source_root, component["sourceCommits"][source_id])
            if source_id in global_sources:
                require(
                    global_sources[source_id] == identity,
                    f"source pin identity differs across components: {source_id}",
                )
            else:
                global_sources[source_id] = identity
    require(set(global_sources) == {"chain", "media", "site"}, "global source pin set mismatch")
    require(
        len({identity[0] for identity in global_sources.values()}) == 3,
        "chain, media, and site source roots must be distinct",
    )
    return {
        "value": plan,
        "path": str(plan_path),
        "sha256": actual_hash,
        "operationId": operation_id,
        "releaseId": release_id,
        "sourceCommit": source_commit,
        "genesisHash": genesis_hash,
        "runtimeCodeSha256": runtime_code_hash,
        "runtimeMetadataScaleSha256": metadata_hash,
        "runtimeBundleManifestSha256": runtime_manifest_hash,
        "freshResetReadinessSha256": readiness_hash,
        "finalBackupManifestSha256": manifest_hash,
        "restoreEvidenceSha256": restore_hash,
        "postCutoverObservationSha256": observation_hash,
        "acceptanceBoundaryCaptureSha256": capture_hash,
        "ingressClosedEvidenceSha256": ingress_hash,
        "maxObservationAgeSeconds": maximum_age,
        "components": components,
    }


def validate_inputs(args: argparse.Namespace, plan: Mapping[str, Any]) -> dict[str, Any]:
    manifest_path = ensure_regular_file(Path(args.manifest), "final backup manifest")
    bundle_root = Path(args.bundle_root)
    manifest_value = read_json(manifest_path, "final backup manifest")
    require_schema_one(manifest_value, "final backup manifest")
    verified = safety.verify_backup_manifest(manifest_path, bundle_root)
    require(
        verified["sha256"] == plan["finalBackupManifestSha256"],
        "final backup manifest does not match the rollback plan",
    )
    require(verified["releaseId"] == plan["releaseId"], "final backup release mismatch")
    require(verified["sourceCommit"] == plan["sourceCommit"], "final backup source commit mismatch")

    restore_path = ensure_regular_file(Path(args.restore_evidence), "restore evidence")
    restore = read_json(restore_path, "restore evidence")
    exact_keys(restore, RESTORE_EVIDENCE_KEYS, "restore evidence")
    require_schema_one(restore, "restore evidence")
    require(
        sha256_file(restore_path) == plan["restoreEvidenceSha256"],
        "restore evidence does not match the rollback plan",
    )
    safety.validate_restore_evidence(
        restore_path,
        plan["releaseId"],
        plan["sourceCommit"],
        verified["sha256"],
    )
    manifest_created = parse_utc(
        verified["manifest"]["createdAtUtc"],
        "final backup createdAtUtc",
    )
    restore_completed = parse_utc(
        restore["completedAtUtc"],
        "restore evidence completedAtUtc",
    )
    now = dt.datetime.now(dt.timezone.utc)
    require(
        manifest_created <= now + dt.timedelta(seconds=30),
        "final backup creation time is in the future",
    )
    require(
        restore_completed >= manifest_created,
        "restore rehearsal predates the final backup",
    )
    require(
        restore_completed <= now + dt.timedelta(seconds=30),
        "restore rehearsal completion time is in the future",
    )

    runtime_root = Path(args.runtime_bundle_root).resolve()
    runtime_artifacts = boundary.load_runtime_artifacts(
        runtime_root,
        plan["runtimeBundleManifestSha256"],
    )
    capture_path = ensure_regular_file(
        Path(args.acceptance_boundary_capture),
        "acceptance-boundary RPC capture",
    )
    gates_path = ensure_regular_file(Path(args.economic_gates), "post-V16 economic gates")
    inventory_path = ensure_regular_file(
        Path(args.acceptance_inventory),
        "V2 and legacy acceptance inventory",
    )
    derived = boundary.derive_and_validate_artifacts(
        capture_path,
        gates_path,
        inventory_path,
        runtime_artifacts,
    )
    require(
        derived["captureSha256"] == plan["acceptanceBoundaryCaptureSha256"],
        "acceptance-boundary capture does not match the coordinator plan",
    )
    require(derived["releaseId"] == plan["releaseId"], "acceptance capture release mismatch")
    require(derived["sourceCommit"] == plan["sourceCommit"], "acceptance capture source mismatch")
    require(derived["genesisHash"] == plan["genesisHash"], "acceptance capture genesis mismatch")
    require(
        derived["runtimeCodeSha256"] == plan["runtimeCodeSha256"],
        "acceptance capture runtime code mismatch",
    )
    require(
        derived["runtimeMetadataScaleSha256"] == plan["runtimeMetadataScaleSha256"],
        "acceptance capture runtime metadata mismatch",
    )
    ingress_path = ensure_regular_file(
        Path(args.ingress_closed_evidence),
        "ingress-closed evidence",
    )
    ingress = boundary.validate_ingress_evidence(
        ingress_path,
        plan["ingressClosedEvidenceSha256"],
        derived,
    )

    observation_path = ensure_regular_file(
        Path(args.observation),
        "post-cutover observation",
    )
    observation = read_json(observation_path, "post-cutover observation")
    exact_keys(observation, OBSERVATION_KEYS, "post-cutover observation")
    require(
        sha256_file(observation_path) == plan["postCutoverObservationSha256"],
        "post-cutover observation does not match the rollback plan",
    )
    require_schema_one(observation, "post-cutover observation")
    require(
        observation["kind"]
        == "nexus-v2-private-alpha-post-cutover-rollback-observation",
        "post-cutover observation kind mismatch",
    )
    require(observation["releaseId"] == plan["releaseId"], "post-cutover observation release mismatch")
    require(observation["sourceCommit"] == plan["sourceCommit"], "post-cutover observation source mismatch")
    require(
        observation["acceptanceBoundaryCaptureSha256"]
        == plan["acceptanceBoundaryCaptureSha256"],
        "post-cutover observation capture mismatch",
    )
    require(
        observation["ingressClosedEvidenceSha256"]
        == plan["ingressClosedEvidenceSha256"],
        "post-cutover observation ingress evidence mismatch",
    )
    expected_component_commits = {
        component_id: component["sourceCommits"]
        for component_id, component in plan["components"].items()
    }
    require(
        observation["componentSourceCommits"] == expected_component_commits,
        "post-cutover observation component source pins mismatch",
    )
    observed_number, observed_hash = safety.finalized_block(
        observation["observedAtFinalizedBlock"],
        "post-cutover observation",
    )
    observed_at = parse_utc(observation["observedAtUtc"], "post-cutover observedAtUtc")
    require(
        observation["observedAtUtc"] == derived["observedAtUtc"],
        "post-cutover observation timestamp differs from the RPC capture",
    )
    barrier = exact_keys(
        observation["writeBarrier"],
        WRITE_BARRIER_KEYS,
        "post-cutover write barrier",
    )
    require(barrier["mode"] == "AllV2WritesPaused", "post-cutover write barrier mode mismatch")
    for field in (
        "chainWritesPaused",
        "authorityResultsPaused",
        "webMutationsPaused",
        "gameplaySessionIngressPaused",
        "inventoryObservedAfterPause",
    ):
        require(barrier[field] is True, f"post-cutover write barrier must set {field}=true")
    paused_at = parse_utc(barrier["pausedAtUtc"], "post-cutover pausedAtUtc")
    stability_window = barrier["stabilityWindowSeconds"]
    require(
        isinstance(stability_window, int)
        and not isinstance(stability_window, bool)
        and 30 <= stability_window <= 900,
        "post-cutover write-barrier stability window must be in 30..900",
    )
    require(
        ensure_sha256(
            barrier["evidenceSha256"],
            "post-cutover write-barrier evidence SHA-256",
        )
        == plan["ingressClosedEvidenceSha256"],
        "post-cutover write barrier is not bound to ingress-closed evidence",
    )
    require(paused_at <= observed_at, "post-cutover inventory predates the write barrier")
    require(
        observed_at - paused_at >= dt.timedelta(seconds=stability_window),
        "post-cutover write barrier was not stable before inventory capture",
    )
    now = dt.datetime.now(dt.timezone.utc)
    require(observed_at <= now + dt.timedelta(seconds=30), "post-cutover observation is in the future")
    require(
        now - observed_at
        <= dt.timedelta(seconds=int(plan["maxObservationAgeSeconds"])),
        "post-cutover observation is stale",
    )

    gates_value = read_json(gates_path, "post-V16 economic gates")
    inventory_value = read_json(inventory_path, "V2/legacy acceptance inventory")
    require_schema_one(gates_value, "post-V16 economic gates")
    require_schema_one(inventory_value, "V2/legacy acceptance inventory")
    gates_hash = sha256_file(gates_path)
    inventory_hash = sha256_file(inventory_path)
    require(gates_hash == derived["gatesSha256"], "derived economic-gates hash mismatch")
    require(inventory_hash == derived["inventorySha256"], "derived acceptance-inventory hash mismatch")
    require(
        gates_hash
        == ensure_sha256(
            observation["economicGatesSha256"],
            "observation economic-gates SHA-256",
        ),
        "economic gates do not match the fresh observation",
    )
    require(
        inventory_hash
        == ensure_sha256(
            observation["acceptanceInventorySha256"],
            "observation acceptance-inventory SHA-256",
        ),
        "acceptance inventory does not match the fresh observation",
    )
    gates = safety.validate_economic_gates(
        gates_path,
        plan["releaseId"],
        plan["sourceCommit"],
    )
    require(
        gates["mode"] == safety.POST_V16_GATE_MODE,
        "rollback requires a post-V16 disabled-economics observation",
    )
    inventory = safety.validate_acceptance_inventory(
        inventory_path,
        plan["releaseId"],
        plan["sourceCommit"],
    )
    expected_block = (observed_number, observed_hash)
    require(
        (derived["blockNumber"], derived["blockHash"]) == expected_block,
        "RPC capture and observation use mixed finalized blocks",
    )
    require(
        (gates["blockNumber"], gates["blockHash"]) == expected_block,
        "economic gates and observation use mixed finalized blocks",
    )
    require(
        (inventory["blockNumber"], inventory["blockHash"]) == expected_block,
        "acceptance inventory and observation use mixed finalized blocks",
    )
    return {
        "manifestPath": str(manifest_path),
        "bundleRoot": str(bundle_root.resolve()),
        "runtimeBundleRoot": str(runtime_root),
        "runtimeBundleManifestSha256": plan["runtimeBundleManifestSha256"],
        "manifestSha256": verified["sha256"],
        "restoreEvidencePath": str(restore_path),
        "restoreEvidenceSha256": sha256_file(restore_path),
        "observationPath": str(observation_path),
        "observationSha256": sha256_file(observation_path),
        "acceptanceBoundaryCapturePath": str(capture_path),
        "acceptanceBoundaryCaptureSha256": derived["captureSha256"],
        "ingressClosedEvidencePath": str(ingress_path),
        "ingressClosedEvidenceSha256": sha256_file(ingress_path),
        "economicGatesPath": str(gates_path),
        "economicGatesSha256": gates_hash,
        "acceptanceInventoryPath": str(inventory_path),
        "acceptanceInventorySha256": inventory_hash,
        "blockNumber": observed_number,
        "blockHash": observed_hash,
        "observedAtUtc": observed_at.isoformat(),
        "maxObservationAgeSeconds": plan["maxObservationAgeSeconds"],
        "nonzeroAcceptanceAssets": inventory["nonzero"],
        "genesisHash": derived["genesisHash"],
        "runtimeCodeSha256": derived["runtimeCodeSha256"],
        "runtimeMetadataScaleSha256": derived["runtimeMetadataScaleSha256"],
    }


def state_contract(plan: Mapping[str, Any], inputs: Mapping[str, Any]) -> dict[str, Any]:
    return {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-post-cutover-coordinator-state",
        "operationId": plan["operationId"],
        "planSha256": plan["sha256"],
        "releaseId": plan["releaseId"],
        "sourceCommit": plan["sourceCommit"],
        "genesisHash": inputs["genesisHash"],
        "runtimeCodeSha256": inputs["runtimeCodeSha256"],
        "runtimeMetadataScaleSha256": inputs["runtimeMetadataScaleSha256"],
        "finalBackupManifestSha256": inputs["manifestSha256"],
        "restoreEvidenceSha256": inputs["restoreEvidenceSha256"],
        "observationSha256": inputs["observationSha256"],
        "acceptanceBoundaryCaptureSha256": inputs["acceptanceBoundaryCaptureSha256"],
        "ingressClosedEvidenceSha256": inputs["ingressClosedEvidenceSha256"],
        "economicGatesSha256": inputs["economicGatesSha256"],
        "acceptanceInventorySha256": inputs["acceptanceInventorySha256"],
    }


def prepare_state_dir(
    path: Path,
    plan: Mapping[str, Any],
    inputs: Mapping[str, Any],
) -> tuple[Path, dict[str, Any]]:
    require(not path.is_symlink(), "coordinator state directory must not be a symlink")
    path.mkdir(parents=True, exist_ok=True)
    require(path.is_dir(), "coordinator state path is not a directory")
    sentinel_path = path / "state-contract.json"
    expected = state_contract(plan, inputs)
    if sentinel_path.exists():
        actual = read_json(sentinel_path, "coordinator state contract")
        require(actual == expected, "coordinator state contract does not match this invocation")
    else:
        write_new_json(sentinel_path, expected)
        sentinel_path.chmod(0o440)
    (path / "attempts").mkdir(exist_ok=True)
    (path / "markers").mkdir(exist_ok=True)
    return path.resolve(), expected


def expected_checks(action: str, mode: str) -> set[str]:
    return DRY_CHECKS[action] if mode == "dry-run" else EXECUTE_CHECKS[action]


def validate_action_result(
    result: Mapping[str, Any],
    plan: Mapping[str, Any],
    component: Mapping[str, Any],
    action: str,
    mode: str,
) -> dict[str, Any]:
    exact_keys(result, RESULT_KEYS, "component action result")
    require_schema_one(result, "component result")
    require(
        result["kind"] == "nexus-v2-private-alpha-component-action-result",
        "component result kind mismatch",
    )
    require(result["operationId"] == plan["operationId"], "component result operation mismatch")
    require(result["planSha256"] == plan["sha256"], "component result plan mismatch")
    require(result["releaseId"] == plan["releaseId"], "component result release mismatch")
    require(result["sourceCommit"] == plan["sourceCommit"], "component result source mismatch")
    require(
        result["componentSourceCommits"] == component["sourceCommits"],
        "component result source pins mismatch",
    )
    require(result["componentId"] == component["id"], "component result identity mismatch")
    require(result["action"] == action, "component result action mismatch")
    require(result["mode"] == mode, "component result mode mismatch")
    require(result["result"] == "passed", "component action did not pass")
    require(isinstance(result["alreadyApplied"], bool), "alreadyApplied must be boolean")
    require(
        isinstance(result["remoteActionsExecuted"], bool),
        "remoteActionsExecuted must be boolean",
    )
    if mode == "dry-run":
        require(result["remoteActionsExecuted"] is False, "dry-run executed a remote action")
        require(result["alreadyApplied"] is False, "dry-run may not claim prior application")
        require(
            result["remoteIdempotencyMarkerSha256"] is None,
            "dry-run may not emit a remote idempotency marker",
        )
    elif result["alreadyApplied"]:
        require(
            result["remoteActionsExecuted"] is False,
            "already-applied result may not repeat remote actions",
        )
        ensure_sha256(
            result["remoteIdempotencyMarkerSha256"],
            "remote idempotency marker SHA-256",
        )
    else:
        require(result["remoteActionsExecuted"] is True, "execute result reports no remote action")
        ensure_sha256(
            result["remoteIdempotencyMarkerSha256"],
            "remote idempotency marker SHA-256",
        )

    archives = result["requiredResetArchives"]
    require(isinstance(archives, dict), "component result archives must be an object")
    require(
        set(archives) == set(component["requiredResetArchives"]),
        "component result archive names mismatch",
    )
    for name, present in archives.items():
        require(present is True, f"required reset archive is missing: {component['id']}:{name}")

    failed_archive = result["failedV2RootArchiveSha256"]
    if mode == "execute" and action in {
        "archive-failed-v2",
        "restore-final-backup",
        "restored-smoke",
    }:
        ensure_sha256(failed_archive, "failed V2 root archive SHA-256")
    else:
        require(
            failed_archive is None,
            f"{action} {mode} may not claim a failed V2 root archive",
        )

    checks = result["checks"]
    require(isinstance(checks, dict), "component result checks must be an object")
    expected = expected_checks(action, mode)
    require(set(checks) == expected, "component result checks do not match the closed action contract")
    for name, value in checks.items():
        require(isinstance(value, bool), f"component check must be boolean: {name}")
        if not (mode == "execute" and action == "post-cutover-smoke" and name == "smokePassed"):
            require(value is True, f"component check failed: {name}")
    completed_at = parse_utc(result["completedAtUtc"], "component completedAtUtc")
    now = dt.datetime.now(dt.timezone.utc)
    require(
        completed_at <= now + dt.timedelta(seconds=30),
        "component result completion time is in the future",
    )
    plan_created_at = parse_utc(
        plan["value"]["createdAtUtc"],
        "rollback plan createdAtUtc",
    )
    require(
        completed_at >= plan_created_at,
        "component result predates the rollback plan",
    )
    return dict(result)


def marker_path(state_dir: Path, component_id: str, action: str, mode: str) -> Path:
    safe_action = action.replace("-", "_")
    safe_mode = mode.replace("-", "_")
    return state_dir / "markers" / f"{component_id}.{safe_action}.{safe_mode}.json"


def load_marker_result(
    state_dir: Path,
    marker: Mapping[str, Any],
    plan: Mapping[str, Any],
    component: Mapping[str, Any],
    action: str,
    mode: str,
) -> dict[str, Any]:
    exact_keys(
        marker,
        {
            "schemaVersion",
            "kind",
            "operationId",
            "planSha256",
            "componentId",
            "action",
            "mode",
            "driverSha256",
            "resultPath",
            "resultSha256",
            "logPath",
            "logSha256",
            "completedAtUtc",
        },
        "component action marker",
    )
    require_schema_one(marker, "component marker")
    require(
        marker["kind"] == "nexus-v2-private-alpha-component-action-marker",
        "component marker kind mismatch",
    )
    require(marker["operationId"] == plan["operationId"], "component marker operation mismatch")
    require(marker["planSha256"] == plan["sha256"], "component marker plan mismatch")
    require(marker["componentId"] == component["id"], "component marker identity mismatch")
    require(marker["action"] == action and marker["mode"] == mode, "component marker phase mismatch")
    require(marker["driverSha256"] == component["driverSha256"], "component marker driver mismatch")
    result_path = ensure_regular_file(Path(marker["resultPath"]), "marked component result")
    log_path = ensure_regular_file(Path(marker["logPath"]), "marked component log")
    require(
        result_path.parent.parent == state_dir / "attempts",
        "component result is outside the coordinator state directory",
    )
    require(
        log_path.parent == result_path.parent,
        "component log/result attempt directories differ",
    )
    require(sha256_file(result_path) == marker["resultSha256"], "component result hash drifted")
    require(sha256_file(log_path) == marker["logSha256"], "component log hash drifted")
    result = read_json(result_path, "marked component result")
    return validate_action_result(result, plan, component, action, mode)


def existing_action_result(
    state_dir: Path,
    plan: Mapping[str, Any],
    component: Mapping[str, Any],
    action: str,
    mode: str,
) -> dict[str, Any] | None:
    path = marker_path(state_dir, component["id"], action, mode)
    if not path.exists():
        return None
    marker = read_json(path, "component action marker")
    return load_marker_result(
        state_dir,
        marker,
        plan,
        component,
        action,
        mode,
    )


def next_attempt_dir(
    state_dir: Path,
    component_id: str,
    action: str,
    mode: str,
) -> Path:
    prefix = f"{component_id}.{action.replace('-', '_')}.{mode.replace('-', '_')}."
    existing = [
        child
        for child in (state_dir / "attempts").iterdir()
        if child.is_dir() and child.name.startswith(prefix)
    ]
    attempt = state_dir / "attempts" / f"{prefix}{len(existing) + 1:04d}"
    attempt.mkdir(mode=0o700)
    return attempt


def run_component_action(
    state_dir: Path,
    plan: Mapping[str, Any],
    inputs: Mapping[str, Any],
    component: Mapping[str, Any],
    action: str,
    mode: str,
) -> dict[str, Any]:
    existing = existing_action_result(
        state_dir,
        plan,
        component,
        action,
        mode,
    )
    if existing is not None:
        return existing
    if mode == "execute":
        require(
            existing_action_result(
                state_dir,
                plan,
                component,
                action,
                "dry-run",
            )
            is not None,
            f"{component['id']}:{action} has no successful dry-run receipt",
        )

    attempt = next_attempt_dir(state_dir, component["id"], action, mode)
    result_path = attempt / "result.json"
    log_path = attempt / "driver.log"
    command = [
        component["driverPath"],
        "--component",
        component["id"],
        "--action",
        action,
        "--mode",
        mode,
        "--operation-id",
        plan["operationId"],
        "--plan",
        plan["path"],
        "--plan-sha256",
        plan["sha256"],
        "--manifest",
        inputs["manifestPath"],
        "--bundle-root",
        inputs["bundleRoot"],
        "--restore-evidence",
        inputs["restoreEvidencePath"],
        "--observation",
        inputs["observationPath"],
        "--economic-gates",
        inputs["economicGatesPath"],
        "--acceptance-inventory",
        inputs["acceptanceInventoryPath"],
        "--result",
        str(result_path),
    ]
    with log_path.open("xb") as log:
        completed = subprocess.run(
            command,
            stdout=log,
            stderr=subprocess.STDOUT,
            check=False,
            env=os.environ.copy(),
        )
    require(
        completed.returncode == 0,
        f"component action failed: {component['id']}:{action}:{mode}; see {log_path}",
    )
    result = read_json(result_path, "component action result")
    validated = validate_action_result(
        result,
        plan,
        component,
        action,
        mode,
    )
    result_path.chmod(0o440)
    log_path.chmod(0o400)
    marker = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-component-action-marker",
        "operationId": plan["operationId"],
        "planSha256": plan["sha256"],
        "componentId": component["id"],
        "action": action,
        "mode": mode,
        "driverSha256": component["driverSha256"],
        "resultPath": str(result_path.resolve()),
        "resultSha256": sha256_file(result_path),
        "logPath": str(log_path.resolve()),
        "logSha256": sha256_file(log_path),
        "completedAtUtc": utc_now(),
    }
    path = marker_path(state_dir, component["id"], action, mode)
    write_new_json(path, marker)
    path.chmod(0o440)
    return validated


def marker_hashes(state_dir: Path) -> dict[str, str]:
    return {
        path.name: sha256_file(path)
        for path in sorted((state_dir / "markers").glob("*.json"))
    }


def final_marker_path(state_dir: Path) -> Path:
    return state_dir / "final-evidence.marker.json"


def recover_completed(
    state_dir: Path,
    evidence_path: Path,
    plan: Mapping[str, Any],
    inputs: Mapping[str, Any],
) -> bool:
    marker_path_value = final_marker_path(state_dir)
    if not marker_path_value.exists():
        return False
    marker = read_json(marker_path_value, "final evidence marker")
    exact_keys(
        marker,
        {
            "schemaVersion",
            "kind",
            "operationId",
            "planSha256",
            "evidencePath",
            "evidenceSha256",
            "completedAtUtc",
        },
        "final evidence marker",
    )
    require_schema_one(marker, "final evidence marker")
    require(
        marker["kind"] == "nexus-v2-private-alpha-post-cutover-final-marker",
        "final marker kind mismatch",
    )
    require(marker["operationId"] == plan["operationId"], "final marker operation mismatch")
    require(marker["planSha256"] == plan["sha256"], "final marker plan mismatch")
    require(Path(marker["evidencePath"]).resolve() == evidence_path.resolve(), "final marker output path mismatch")
    ensure_regular_file(evidence_path, "final coordinator evidence")
    require(sha256_file(evidence_path) == marker["evidenceSha256"], "final evidence hash drifted")
    evidence = read_json(evidence_path, "final coordinator evidence")
    validate_final_evidence(evidence, state_dir, plan, inputs)
    return True


def validate_final_evidence(
    value: Mapping[str, Any],
    state_dir: Path,
    plan: Mapping[str, Any],
    inputs: Mapping[str, Any],
) -> None:
    exact_keys(value, EVIDENCE_KEYS, "final coordinator evidence")
    require_schema_one(value, "final coordinator evidence")
    require(
        value["kind"]
        == "nexus-v2-private-alpha-post-cutover-coordinator-evidence",
        "final coordinator evidence kind mismatch",
    )
    require(value["operationId"] == plan["operationId"], "final evidence operation mismatch")
    require(value["planSha256"] == plan["sha256"], "final evidence plan mismatch")
    require(value["releaseId"] == plan["releaseId"], "final evidence release mismatch")
    require(value["sourceCommit"] == plan["sourceCommit"], "final evidence source mismatch")
    require(value["genesisHash"] == inputs["genesisHash"], "final evidence genesis mismatch")
    require(
        value["runtimeCodeSha256"] == inputs["runtimeCodeSha256"],
        "final evidence runtime code mismatch",
    )
    require(
        value["runtimeMetadataScaleSha256"]
        == inputs["runtimeMetadataScaleSha256"],
        "final evidence runtime metadata mismatch",
    )
    expected_component_commits = {
        component_id: component["sourceCommits"]
        for component_id, component in plan["components"].items()
    }
    require(
        value["componentSourceCommits"] == expected_component_commits,
        "final evidence component source pins mismatch",
    )
    require(
        value["finalBackupManifestSha256"] == inputs["manifestSha256"],
        "final evidence backup mismatch",
    )
    require(
        value["restoreEvidenceSha256"] == inputs["restoreEvidenceSha256"],
        "final evidence restore mismatch",
    )
    require(
        value["postCutoverObservationSha256"] == inputs["observationSha256"],
        "final evidence observation mismatch",
    )
    require(
        value["acceptanceBoundaryCaptureSha256"]
        == inputs["acceptanceBoundaryCaptureSha256"],
        "final evidence acceptance capture mismatch",
    )
    require(
        value["ingressClosedEvidenceSha256"]
        == inputs["ingressClosedEvidenceSha256"],
        "final evidence ingress evidence mismatch",
    )
    require(
        value["economicGatesSha256"] == inputs["economicGatesSha256"],
        "final evidence economic-gates mismatch",
    )
    require(
        value["acceptanceInventorySha256"] == inputs["acceptanceInventorySha256"],
        "final evidence inventory mismatch",
    )
    require(
        value["observedAtFinalizedBlock"]
        == {"number": inputs["blockNumber"], "hash": inputs["blockHash"]},
        "final evidence block mismatch",
    )
    require(
        value["nonzeroAcceptanceAssets"] == inputs["nonzeroAcceptanceAssets"],
        "final evidence acceptance counts mismatch",
    )
    require(
        value["componentMarkerSha256"] == marker_hashes(state_dir),
        "final evidence component markers drifted",
    )
    decision = value["decision"]
    require(
        decision
        in {
            "dry-run-complete",
            "keep-v2",
            "post-acceptance-pause-and-forward-fix",
            "pre-acceptance-automatic-restore",
        },
        "final evidence decision mismatch",
    )
    require(
        value["automaticRestorePerformed"]
        is (decision == "pre-acceptance-automatic-restore"),
        "final evidence automatic-restore flag mismatch",
    )
    require(
        value["postAcceptanceContainmentPerformed"]
        is (decision == "post-acceptance-pause-and-forward-fix"),
        "final evidence containment flag mismatch",
    )
    expected_smoke: bool | None
    if decision == "dry-run-complete":
        expected_smoke = None
    elif decision == "keep-v2":
        expected_smoke = True
    else:
        expected_smoke = False
    require(
        value["postCutoverSmokePassed"] is expected_smoke,
        "final evidence smoke decision mismatch",
    )
    parse_utc(value["completedAtUtc"], "final evidence completedAtUtc")


def recover_unmarked_final(
    state_dir: Path,
    evidence_path: Path,
    plan: Mapping[str, Any],
    inputs: Mapping[str, Any],
) -> bool:
    if not evidence_path.exists():
        return False
    value = read_json(evidence_path, "unmarked final coordinator evidence")
    validate_final_evidence(value, state_dir, plan, inputs)
    evidence_path.chmod(0o440)
    marker = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-post-cutover-final-marker",
        "operationId": plan["operationId"],
        "planSha256": plan["sha256"],
        "evidencePath": str(evidence_path.resolve()),
        "evidenceSha256": sha256_file(evidence_path),
        "completedAtUtc": utc_now(),
    }
    write_new_json(final_marker_path(state_dir), marker)
    final_marker_path(state_dir).chmod(0o440)
    return True


def require_observation_still_fresh(inputs: Mapping[str, Any]) -> None:
    observed_at = parse_utc(inputs["observedAtUtc"], "post-cutover observedAtUtc")
    require(
        dt.datetime.now(dt.timezone.utc) - observed_at
        <= dt.timedelta(seconds=int(inputs["maxObservationAgeSeconds"])),
        "post-cutover observation became stale during coordination",
    )


def revalidate_immutable_inputs(
    plan: Mapping[str, Any],
    inputs: Mapping[str, Any],
) -> None:
    boundary.load_runtime_artifacts(
        Path(inputs["runtimeBundleRoot"]),
        inputs["runtimeBundleManifestSha256"],
    )
    require(
        sha256_file(Path(plan["path"])) == plan["sha256"],
        "rollback plan changed during coordination",
    )
    require(
        sha256_file(Path(inputs["manifestPath"])) == inputs["manifestSha256"],
        "final backup manifest changed during coordination",
    )
    require(
        sha256_file(Path(inputs["restoreEvidencePath"]))
        == inputs["restoreEvidenceSha256"],
        "restore evidence changed during coordination",
    )
    require(
        sha256_file(Path(inputs["observationPath"])) == inputs["observationSha256"],
        "post-cutover observation changed during coordination",
    )
    require(
        sha256_file(Path(inputs["acceptanceBoundaryCapturePath"]))
        == inputs["acceptanceBoundaryCaptureSha256"],
        "acceptance-boundary capture changed during coordination",
    )
    require(
        sha256_file(Path(inputs["ingressClosedEvidencePath"]))
        == inputs["ingressClosedEvidenceSha256"],
        "ingress-closed evidence changed during coordination",
    )
    require(
        sha256_file(Path(inputs["economicGatesPath"]))
        == inputs["economicGatesSha256"],
        "economic gates changed during coordination",
    )
    require(
        sha256_file(Path(inputs["acceptanceInventoryPath"]))
        == inputs["acceptanceInventorySha256"],
        "acceptance inventory changed during coordination",
    )
    verified = safety.verify_backup_manifest(
        Path(inputs["manifestPath"]),
        Path(inputs["bundleRoot"]),
    )
    require(
        verified["sha256"] == inputs["manifestSha256"],
        "final backup artifacts changed during coordination",
    )
    refreshed = validate_plan(Path(plan["path"]), plan["sha256"])
    require(
        {
            component_id: component["sourceCommits"]
            for component_id, component in refreshed["components"].items()
        }
        == {
            component_id: component["sourceCommits"]
            for component_id, component in plan["components"].items()
        },
        "component source identities changed during coordination",
    )


def write_final_evidence(
    state_dir: Path,
    evidence_path: Path,
    plan: Mapping[str, Any],
    inputs: Mapping[str, Any],
    decision: str,
    smoke_passed: bool | None,
) -> None:
    require(not final_marker_path(state_dir).exists(), "final evidence marker already exists")
    evidence = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-post-cutover-coordinator-evidence",
        "operationId": plan["operationId"],
        "planSha256": plan["sha256"],
        "releaseId": plan["releaseId"],
        "sourceCommit": plan["sourceCommit"],
        "genesisHash": inputs["genesisHash"],
        "runtimeCodeSha256": inputs["runtimeCodeSha256"],
        "runtimeMetadataScaleSha256": inputs["runtimeMetadataScaleSha256"],
        "componentSourceCommits": {
            component_id: component["sourceCommits"]
            for component_id, component in plan["components"].items()
        },
        "decision": decision,
        "postCutoverSmokePassed": smoke_passed,
        "automaticRestorePerformed": decision == "pre-acceptance-automatic-restore",
        "postAcceptanceContainmentPerformed": decision
        == "post-acceptance-pause-and-forward-fix",
        "finalBackupManifestSha256": inputs["manifestSha256"],
        "restoreEvidenceSha256": inputs["restoreEvidenceSha256"],
        "postCutoverObservationSha256": inputs["observationSha256"],
        "acceptanceBoundaryCaptureSha256": inputs["acceptanceBoundaryCaptureSha256"],
        "ingressClosedEvidenceSha256": inputs["ingressClosedEvidenceSha256"],
        "economicGatesSha256": inputs["economicGatesSha256"],
        "acceptanceInventorySha256": inputs["acceptanceInventorySha256"],
        "observedAtFinalizedBlock": {
            "number": inputs["blockNumber"],
            "hash": inputs["blockHash"],
        },
        "nonzeroAcceptanceAssets": inputs["nonzeroAcceptanceAssets"],
        "componentMarkerSha256": marker_hashes(state_dir),
        "completedAtUtc": utc_now(),
    }
    write_new_json(evidence_path, evidence)
    evidence_path.chmod(0o440)
    validate_final_evidence(evidence, state_dir, plan, inputs)
    marker = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-post-cutover-final-marker",
        "operationId": plan["operationId"],
        "planSha256": plan["sha256"],
        "evidencePath": str(evidence_path.resolve()),
        "evidenceSha256": sha256_file(evidence_path),
        "completedAtUtc": utc_now(),
    }
    write_new_json(final_marker_path(state_dir), marker)
    final_marker_path(state_dir).chmod(0o440)


def run(args: argparse.Namespace) -> None:
    expected_plan_hash = os.environ.get("NEXUS_V2_ROLLBACK_PLAN_SHA256", "")
    require(expected_plan_hash, "NEXUS_V2_ROLLBACK_PLAN_SHA256 must be set")
    plan = validate_plan(Path(args.plan), expected_plan_hash)
    inputs = validate_inputs(args, plan)
    state_dir, _ = prepare_state_dir(Path(args.state_dir), plan, inputs)
    evidence_path = Path(args.evidence)
    if recover_completed(state_dir, evidence_path, plan, inputs):
        print(f"coordinator already completed: {evidence_path}")
        return
    if recover_unmarked_final(state_dir, evidence_path, plan, inputs):
        print(f"coordinator completion marker recovered: {evidence_path}")
        return

    if args.validate_only:
        print(
            "post-cutover rollback inputs validated locally; "
            "no component driver or remote action was invoked"
        )
        return

    components = [plan["components"][name] for name in sorted(EXPECTED_COMPONENTS)]
    for action in ACTIONS:
        for component in components:
            run_component_action(
                state_dir,
                plan,
                inputs,
                component,
                action,
                "dry-run",
            )
    revalidate_immutable_inputs(plan, inputs)
    if args.dry_run:
        write_final_evidence(
            state_dir,
            evidence_path,
            plan,
            inputs,
            "dry-run-complete",
            None,
        )
        print(f"all cross-host actions dry-ran without mutation: {evidence_path}")
        return

    revalidate_immutable_inputs(plan, inputs)
    require_observation_still_fresh(inputs)
    post_smoke = [
        run_component_action(
            state_dir,
            plan,
            inputs,
            component,
            "post-cutover-smoke",
            "execute",
        )
        for component in components
    ]
    smoke_passed = all(result["checks"]["smokePassed"] for result in post_smoke)
    if smoke_passed:
        write_final_evidence(
            state_dir,
            evidence_path,
            plan,
            inputs,
            "keep-v2",
            True,
        )
        print(f"post-cutover smoke passed; V2 retained: {evidence_path}")
        return

    if inputs["nonzeroAcceptanceAssets"]:
        revalidate_immutable_inputs(plan, inputs)
        for component in components:
            run_component_action(
                state_dir,
                plan,
                inputs,
                component,
                "pause-v2-writes",
                "execute",
            )
        write_final_evidence(
            state_dir,
            evidence_path,
            plan,
            inputs,
            "post-acceptance-pause-and-forward-fix",
            False,
        )
        print(
            "post-cutover smoke failed after V2/legacy acceptance; "
            f"writes paused and restore refused: {evidence_path}"
        )
        return

    revalidate_immutable_inputs(plan, inputs)
    require_observation_still_fresh(inputs)
    for component in components:
        run_component_action(
            state_dir,
            plan,
            inputs,
            component,
            "pause-v2-writes",
            "execute",
        )
    archive_results: dict[str, dict[str, Any]] = {}
    for component in components:
        archive_results[component["id"]] = run_component_action(
            state_dir,
            plan,
            inputs,
            component,
            "archive-failed-v2",
            "execute",
        )
    for component in components:
        revalidate_immutable_inputs(plan, inputs)
        restored = run_component_action(
            state_dir,
            plan,
            inputs,
            component,
            "restore-final-backup",
            "execute",
        )
        require(
            restored["failedV2RootArchiveSha256"]
            == archive_results[component["id"]]["failedV2RootArchiveSha256"],
            f"{component['id']} restore did not preserve the archived failed V2 root",
        )
    restored_smoke = []
    for component in components:
        result = run_component_action(
            state_dir,
            plan,
            inputs,
            component,
            "restored-smoke",
            "execute",
        )
        require(
            result["failedV2RootArchiveSha256"]
            == archive_results[component["id"]]["failedV2RootArchiveSha256"],
            f"{component['id']} restored smoke references a different failed V2 archive",
        )
        restored_smoke.append(result)
    require(
        all(result["checks"]["componentHealthy"] for result in restored_smoke),
        "restored cross-host smoke did not pass",
    )
    write_final_evidence(
        state_dir,
        evidence_path,
        plan,
        inputs,
        "pre-acceptance-automatic-restore",
        False,
    )
    print(f"pre-acceptance cross-host restore completed: {evidence_path}")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", required=True)
    parser.add_argument("--manifest", required=True)
    parser.add_argument("--bundle-root", required=True)
    parser.add_argument("--runtime-bundle-root", required=True)
    parser.add_argument("--restore-evidence", required=True)
    parser.add_argument("--observation", required=True)
    parser.add_argument("--acceptance-boundary-capture", required=True)
    parser.add_argument("--ingress-closed-evidence", required=True)
    parser.add_argument("--economic-gates", required=True)
    parser.add_argument("--acceptance-inventory", required=True)
    parser.add_argument("--state-dir", required=True)
    parser.add_argument("--evidence", required=True)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--validate-only", action="store_true")
    mode.add_argument("--dry-run", action="store_true")
    mode.add_argument("--execute", action="store_true")
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        run(args)
    except (CoordinatorError, safety.SafetyError, boundary.BoundaryError, OSError) as exc:
        print(f"nexus-v2-post-cutover-coordinator: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
