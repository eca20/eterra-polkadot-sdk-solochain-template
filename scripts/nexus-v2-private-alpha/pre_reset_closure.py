#!/usr/bin/env python3
"""Create or verify the final, short-lived pre-reset closure handoff.

The create command validates the immutable replacement, backup, restore,
migration, and reset-readiness evidence before contacting a protected target.
It then re-runs the five SHA-pinned final-freeze component drivers with the
read-only ``verify-frozen`` action.  A canonical, owner-only receipt is written
only if every component still reports the exact stopped finalized block.

The receipt is deliberately short-lived.  A reset coordinator must verify its
hash and freshness (300 seconds by default) immediately before its first live
mutation.  Later component-binding checks may set the maximum age to zero for
identity-only verification.  This tool never performs a reset, deploy, restore,
or service start.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import re
import signal
import stat
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Mapping, Sequence


TOOL_ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOL_ROOT))
import deployment_secret_environment  # noqa: E402,F401
import alpha_v2_release as release  # noqa: E402
import release_lock  # noqa: E402
import verify_reset_readiness  # noqa: E402


SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
HASH256_RE = re.compile(r"^0x[0-9a-f]{64}$")
ID_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
UTC_RE = re.compile(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$")

ROLES = (
    "site-ingress",
    "site-indexer-mongo",
    "authority",
    "chain",
    "media-ipfs",
)
RECEIPT_COMPONENTS = (
    "chain",
    "media-ipfs",
    "authority",
    "site-indexer-mongo",
    "site-ingress",
)
ROLE_SOURCE_COMPONENT = {
    "site-ingress": "web",
    "site-indexer-mongo": "web",
    "authority": "sdkgen",
    "chain": "chain",
    "media-ipfs": "media",
}
SOURCE_COMPONENTS = {
    "chain",
    "web",
    "sdkgen",
    "unity",
    "media",
    "ip",
    "ai",
    "flow",
    "blockchainia-site",
}
COMPONENT_ARTIFACTS: dict[str, set[tuple[str, str]]] = {
    "site-ingress": {
        ("ingress", "caddy-config"),
        ("ingress", "caddy-state"),
        ("service", "caddy-service"),
    },
    "site-indexer-mongo": {
        ("config", "indexer-env"),
        ("config", "site-env"),
        ("indexer", "indexer-checkpoint"),
        ("indexer", "indexer-state"),
        ("indexer", "mongo-state"),
        ("service", "indexer-service"),
        ("service", "site-service"),
        ("site", "site-image-lock"),
        ("site", "site-state"),
    },
    "authority": {
        ("authority", "authority-state"),
        ("config", "authority-env"),
        ("service", "authority-service"),
    },
    "chain": {
        ("config", "chain-spec"),
        ("config", "node-env"),
        ("node", "legacy-source-inventory"),
        ("node", "node-binary"),
        ("node", "node-data"),
        ("node", "runtime-v14-metadata"),
        ("node", "runtime-v14-wasm"),
        ("node", "runtime-v16-production-wasm"),
        ("node", "runtime-v16-try-runtime-wasm"),
        ("node", "tcg-storage-version-observation"),
        ("node", "try-runtime-snapshot"),
        ("node", "try-runtime-snapshot-proof"),
        ("service", "node-service"),
    },
    "media-ipfs": {
        ("config", "media-env"),
        ("ipfs", "ipfs-data"),
        ("ipfs", "ipfs-staging"),
        ("media", "media-image-lock"),
        ("media", "media-state"),
        ("service", "media-service"),
    },
}
VERIFY_CHECKS = {
    "site-ingress": {
        "caddyStopped",
        "publicHttpIngressStopped",
        "publicRpcWriteIngressStopped",
        "remainsStopped",
    },
    "site-indexer-mongo": {
        "indexerStopped",
        "mongoWritesQuiescent",
        "siteStopped",
        "remainsStopped",
    },
    "authority": {
        "authorityStopped",
        "resultSubmissionStopped",
        "remainsStopped",
    },
    "chain": {
        "blockProductionStopped",
        "finalizedHeadCaptured",
        "nodeP2pStopped",
        "nodeRpcStopped",
        "nodeStopped",
        "remainsStopped",
    },
    "media-ipfs": {
        "ipfsStopped",
        "mediaStopped",
        "uploadIngressStopped",
        "remainsStopped",
    },
}
PLAN_KEYS = {
    "authorizations",
    "componentSourceCommits",
    "components",
    "kind",
    "releaseId",
    "preV16SourceRuntime",
    "schemaVersion",
    "sourceCommit",
    "stabilityWindowSeconds",
    "transactionId",
}
COMPONENT_KEYS = {"arguments", "driver", "driverSha256", "target"}
STANDARD_FLAGS = {
    "--action",
    "--artifact",
    "--bundle-root",
    "--component-source-commit",
    "--dry-run",
    "--frozen-block-hash",
    "--frozen-block-number",
    "--release-id",
    "--result",
    "--role",
    "--source-commit",
    "--target",
    "--transaction-id",
}
RESULT_KEYS = {
    "action",
    "artifacts",
    "checks",
    "dryRun",
    "frozenAtUtc",
    "frozenFinalizedBlock",
    "kind",
    "liveMutationPerformed",
    "planned",
    "releaseId",
    "role",
    "schemaVersion",
    "sourceCommit",
    "target",
    "transactionId",
}
FINAL_FREEZE_EVIDENCE_KEYS = {
    "schemaVersion",
    "kind",
    "transactionId",
    "releaseId",
    "sourceCommit",
    "componentSourceCommits",
    "planSha256",
    "frozenFinalizedBlock",
    "stabilityWindowSeconds",
    "allIngressAndMutatingServicesStopped",
    "automaticResumeAttempted",
    "backupManifestSha256",
    "artifactGroups",
    "driverSha256",
    "completedAtUtc",
    "paidOrPublicActivationAllowed",
}
RECEIPT_KEYS = {
    "schemaVersion",
    "kind",
    "releaseId",
    "sourceCommit",
    "replacementLockSha256",
    "resetReadinessSha256",
    "finalFreezeEvidenceSha256",
    "backupManifestSha256",
    "restoreEvidenceSha256",
    "migrationEvidenceSha256",
    "automaticRestoreArmSha256",
    "automaticRestoreArmPath",
    "observedAtUtc",
    "automaticRestoreArmed",
    "mutationPerformed",
    "components",
    "protectedListeners",
}
COMPONENT_RECEIPT_KEYS = {"driverSha256", "verifyFrozenResultSha256", "stopped"}
PROTECTED_LISTENERS = {
    "host2010": {
        "absentPorts": [30333, 4000, 5001, 8080, 8787, 9944],
        "allAbsent": True,
    },
    "host2014": {"absentPorts": [80, 443, 3000, 8787], "allAbsent": True},
}


class ClosureError(RuntimeError):
    """The pre-reset closure could not be proven."""


@dataclass(frozen=True)
class Component:
    driver: Path
    driver_sha256: str
    target: str
    arguments: tuple[str, ...]


@dataclass(frozen=True)
class Plan:
    path: Path
    sha256: str
    transaction_id: str
    release_id: str
    source_commit: str
    stability_window_seconds: int
    component_source_commits: Mapping[str, str]
    components: Mapping[str, Component]


@dataclass(frozen=True)
class BoundInputs:
    replacement_lock_sha256: str
    reset_readiness_sha256: str
    final_freeze_evidence_sha256: str
    backup_manifest_sha256: str
    restore_evidence_sha256: str
    migration_evidence_sha256: str
    automatic_restore_arm_sha256: str | None
    frozen_block: Mapping[str, Any]
    pinned_files: tuple[tuple[Path, str, str], ...]


@dataclass(frozen=True)
class PreparedDriver:
    path: Path
    environment: Mapping[str, str]


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ClosureError(message)


def canonical_bytes(value: Mapping[str, Any]) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def ensure_sha256(value: Any, label: str) -> str:
    require(isinstance(value, str) and SHA256_RE.fullmatch(value) is not None, f"invalid {label}")
    return value


def ensure_commit(value: Any, label: str) -> str:
    require(isinstance(value, str) and COMMIT_RE.fullmatch(value) is not None, f"invalid {label}")
    return value


def ensure_id(value: Any, label: str) -> str:
    require(isinstance(value, str) and ID_RE.fullmatch(value) is not None, f"invalid {label}")
    return value


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).replace(microsecond=0).strftime("%Y-%m-%dT%H:%M:%SZ")


def format_utc(value: dt.datetime) -> str:
    require(value.tzinfo is not None, "UTC timestamp must include a timezone")
    return value.astimezone(dt.timezone.utc).replace(microsecond=0).strftime("%Y-%m-%dT%H:%M:%SZ")


def parse_utc(value: Any, label: str) -> dt.datetime:
    require(isinstance(value, str) and UTC_RE.fullmatch(value) is not None, f"invalid {label}")
    return dt.datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=dt.timezone.utc)


def duplicate_rejecting_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        require(key not in result, f"duplicate JSON field: {key}")
        result[key] = value
    return result


def lexists(path: Path) -> bool:
    return os.path.lexists(os.fspath(path))


def require_normal_absolute(path: Path, label: str) -> Path:
    require(path.is_absolute(), f"{label} path must be absolute")
    normalized = Path(os.path.normpath(os.fspath(path)))
    require(path == normalized, f"{label} path must not contain traversal components")
    return path


def require_no_symlink_components(path: Path, label: str, *, include_leaf: bool = True) -> None:
    path = require_normal_absolute(path, label)
    parts = path.parts
    current = Path(parts[0])
    stop = len(parts) if include_leaf else len(parts) - 1
    for part in parts[1:stop]:
        current = current / part
        if lexists(current):
            require(not current.is_symlink(), f"{label} path contains a symlink: {current}")


def regular_file(path_value: str | Path, label: str) -> Path:
    path = require_normal_absolute(Path(path_value), label)
    require_no_symlink_components(path, label)
    require(path.is_file() and not path.is_symlink(), f"{label} must be a regular file")
    return path


def regular_directory(path_value: str | Path, label: str) -> Path:
    path = require_normal_absolute(Path(path_value), label)
    require_no_symlink_components(path, label)
    require(path.is_dir() and not path.is_symlink(), f"{label} must be a regular directory")
    return path


def output_path(path_value: str | Path, label: str) -> Path:
    path = require_normal_absolute(Path(path_value), label)
    require_no_symlink_components(path, label, include_leaf=False)
    require(not lexists(path), f"refusing to overwrite {label}: {path}")
    regular_directory(path.parent, f"{label} parent")
    return path


def read_json(path: Path, label: str, *, canonical: bool = False) -> dict[str, Any]:
    path = regular_file(path, label)
    try:
        payload = path.read_bytes()
        value = json.loads(payload, object_pairs_hook=duplicate_rejecting_object)
    except (OSError, json.JSONDecodeError) as exc:
        raise ClosureError(f"invalid {label}") from exc
    require(isinstance(value, dict), f"{label} root must be an object")
    if canonical:
        require(payload == canonical_bytes(value), f"{label} must be canonical JSON")
    return value


def pinned_file(path_value: str, expected_sha256: str, label: str) -> Path:
    path = regular_file(path_value, label)
    expected = ensure_sha256(expected_sha256, f"expected {label} SHA-256")
    require(sha256_file(path) == expected, f"{label} SHA-256 mismatch")
    return path


def finalized_block(value: Any, label: str) -> dict[str, Any]:
    require(isinstance(value, dict) and set(value) == {"number", "hash"}, f"{label} schema mismatch")
    number = value.get("number")
    block_hash = value.get("hash")
    require(isinstance(number, int) and not isinstance(number, bool) and number >= 0, f"invalid {label} number")
    require(isinstance(block_hash, str) and HASH256_RE.fullmatch(block_hash.lower()) is not None, f"invalid {label} hash")
    return {"number": number, "hash": block_hash.lower()}


def validate_plan(path_value: str, expected_sha256: str) -> Plan:
    path = pinned_file(path_value, expected_sha256, "final-freeze plan")
    value = read_json(path, "final-freeze plan")
    require(set(value) == PLAN_KEYS, "final-freeze plan does not match the closed schema")
    require(value.get("schemaVersion") == 1, "final-freeze plan schema mismatch")
    require(value.get("kind") == "nexus-v2-private-alpha-final-freeze-plan", "final-freeze plan kind mismatch")
    release_id = ensure_id(value.get("releaseId"), "final-freeze release ID")
    source_commit = ensure_commit(value.get("sourceCommit"), "final-freeze source commit")
    transaction_id = ensure_id(value.get("transactionId"), "final-freeze transaction ID")
    window = value.get("stabilityWindowSeconds")
    require(isinstance(window, int) and not isinstance(window, bool) and 30 <= window <= 300, "invalid final-freeze stability window")
    require(
        value.get("authorizations")
        == {
            "automaticResumeOnFailure": False,
            "finalFreezeAndBackup": True,
            "freshReset": False,
            "paidOrPublicActivation": False,
            "privateAlphaOnly": True,
        },
        "final-freeze authorization block mismatch",
    )
    pre_v16 = value.get("preV16SourceRuntime")
    require(
        isinstance(pre_v16, dict)
        and pre_v16
        == {
            "deployedSourceCommit": pre_v16.get("deployedSourceCommit"),
            "specVersion": 1,
            "metadataVersion": 14,
            "tcgPalletIndex": 9,
            "tcgStorageVersion": 14,
            "flowPalletIndex": 29,
        },
        "pre-V16 source runtime identity mismatch",
    )
    ensure_commit(pre_v16.get("deployedSourceCommit"), "deployed pre-V16 source commit")
    commits = value.get("componentSourceCommits")
    require(isinstance(commits, dict) and set(commits) == SOURCE_COMPONENTS, "component source commits do not match the closed set")
    normalized_commits = {
        name: ensure_commit(commit, f"{name} source commit") for name, commit in commits.items()
    }
    require(normalized_commits["chain"] == source_commit, "plan chain source commit mismatch")
    raw_components = value.get("components")
    require(isinstance(raw_components, dict) and set(raw_components) == set(ROLES), "final-freeze components do not match the closed set")
    components: dict[str, Component] = {}
    for role in ROLES:
        component = raw_components[role]
        require(isinstance(component, dict) and set(component) == COMPONENT_KEYS, f"{role} component contract mismatch")
        driver = regular_file(str(component.get("driver", "")), f"{role} driver")
        require(bool(driver.stat().st_mode & stat.S_IXUSR), f"{role} driver must be owner-executable")
        driver_sha256 = ensure_sha256(component.get("driverSha256"), f"{role} driver SHA-256")
        require(sha256_file(driver) == driver_sha256, f"{role} driver SHA-256 mismatch")
        target = ensure_id(component.get("target"), f"{role} target")
        arguments = component.get("arguments")
        require(isinstance(arguments, list), f"{role} driver arguments must be an array")
        normalized_arguments: list[str] = []
        for argument in arguments:
            require(isinstance(argument, str) and argument and "\x00" not in argument, f"invalid {role} driver argument")
            lowered = argument.lower()
            require(
                not any(marker in lowered for marker in ("mnemonic", "password", "private-key", "private_key", "secret", "seed-phrase", "seed_phrase", "suri")),
                f"{role} driver arguments may not contain secret material",
            )
            require(
                not any(argument == flag or argument.startswith(f"{flag}=") for flag in STANDARD_FLAGS),
                f"{role} driver arguments may not override standardized flags",
            )
            normalized_arguments.append(argument)
        components[role] = Component(driver, driver_sha256, target, tuple(normalized_arguments))
    return Plan(
        path=path,
        sha256=expected_sha256,
        transaction_id=transaction_id,
        release_id=release_id,
        source_commit=source_commit,
        stability_window_seconds=window,
        component_source_commits=normalized_commits,
        components=components,
    )


def validate_automatic_restore_arm(
    path: Path,
    expected_sha256: str,
    plan: Plan,
    frozen_block: Mapping[str, Any],
) -> None:
    """Validate the live supervisor lease and its complete immutable binding."""

    try:
        import pre_reset_rollback_supervisor as supervisor
    except ImportError as exc:
        raise ClosureError(f"automatic-restore arm validator is unavailable: {exc}") from exc
    try:
        supervisor.validate_arm(
            path,
            expected_sha256,
            expected_release_id=plan.release_id,
            expected_source_commit=plan.source_commit,
            expected_frozen_block=frozen_block,
            full_binding=True,
            max_issue_age_seconds=300,
        )
    except supervisor.SupervisorError as exc:
        raise ClosureError(f"automatic-restore arm rejected: {exc}") from exc


def validate_bound_inputs(
    args: argparse.Namespace,
    plan: Plan,
    bundle_root: Path,
    *,
    require_automatic_restore_arm: bool = True,
) -> BoundInputs:
    specifications = (
        ("replacement_lock", "expected_replacement_lock_sha256", "pre-cutover replacement lock"),
        ("reset_readiness", "expected_reset_readiness_sha256", "reset-readiness packet"),
        ("final_freeze_evidence", "expected_final_freeze_evidence_sha256", "final-freeze evidence"),
        ("backup_manifest", "expected_backup_manifest_sha256", "backup manifest"),
        ("restore_evidence", "expected_restore_evidence_sha256", "restore evidence"),
        ("migration_evidence", "expected_migration_evidence_sha256", "migration evidence"),
    )
    if require_automatic_restore_arm:
        specifications += (
            ("automatic_restore_arm", "expected_automatic_restore_arm_sha256", "automatic-restore arm"),
        )
    paths: dict[str, Path] = {}
    pinned: list[tuple[Path, str, str]] = []
    for path_name, hash_name, label in specifications:
        expected = ensure_sha256(getattr(args, hash_name), f"expected {label} SHA-256")
        path = pinned_file(getattr(args, path_name), expected, label)
        paths[path_name] = path
        pinned.append((path, expected, label))

    lock = release_lock.validate_replacement_lock(
        paths["replacement_lock"],
        args.expected_replacement_lock_sha256,
        args.selected_deployment_environment,
        args.selected_site_deployment_environment,
    )
    require(lock.get("releaseId") == plan.release_id, "replacement lock release mismatch")
    repositories = lock.get("repositories")
    require(isinstance(repositories, Mapping), "replacement lock repositories are missing")
    chain_pin = repositories.get("chain")
    require(isinstance(chain_pin, Mapping) and chain_pin.get("head") == plan.source_commit, "replacement lock chain source mismatch")

    readiness_summary = verify_reset_readiness.validate_packet(
        paths["reset_readiness"], args.expected_reset_readiness_sha256
    )
    require(readiness_summary["releaseId"] == plan.release_id, "reset-readiness release mismatch")
    require(readiness_summary["sourceCommit"] == plan.source_commit, "reset-readiness source mismatch")
    readiness = read_json(paths["reset_readiness"], "reset-readiness packet", canonical=True)
    require(readiness.get("backupManifestSha256") == args.expected_backup_manifest_sha256, "reset-readiness backup hash mismatch")
    require(readiness.get("restoreEvidenceSha256") == args.expected_restore_evidence_sha256, "reset-readiness restore hash mismatch")
    require(readiness.get("migrationEvidenceSha256") == args.expected_migration_evidence_sha256, "reset-readiness migration hash mismatch")
    frozen = finalized_block(readiness.get("gateFinalizedBlock"), "reset-readiness gate block")

    verified_backup = release.verify_backup_manifest(paths["backup_manifest"], bundle_root)
    require(verified_backup.get("sha256") == args.expected_backup_manifest_sha256, "verified backup-manifest hash mismatch")
    require(verified_backup.get("releaseId") == plan.release_id, "backup-manifest release mismatch")
    require(verified_backup.get("sourceCommit") == plan.source_commit, "backup-manifest source mismatch")
    source_inventory = release.validate_legacy_source_inventory(
        release.find_artifact(verified_backup, bundle_root, "node", "legacy-source-inventory"),
        plan.release_id,
        plan.source_commit,
    )
    release.validate_restore_evidence(
        paths["restore_evidence"],
        plan.release_id,
        plan.source_commit,
        args.expected_backup_manifest_sha256,
    )
    release.validate_migration_evidence(
        paths["migration_evidence"],
        plan.release_id,
        plan.source_commit,
        args.expected_backup_manifest_sha256,
        source_inventory,
    )
    if require_automatic_restore_arm:
        validate_automatic_restore_arm(
            paths["automatic_restore_arm"],
            args.expected_automatic_restore_arm_sha256,
            plan,
            frozen,
        )

    evidence = read_json(paths["final_freeze_evidence"], "final-freeze evidence", canonical=True)
    require(set(evidence) == FINAL_FREEZE_EVIDENCE_KEYS, "final-freeze evidence does not match the closed schema")
    require(evidence.get("schemaVersion") == 1, "final-freeze evidence schema mismatch")
    require(evidence.get("kind") == "nexus-v2-private-alpha-final-freeze-evidence", "final-freeze evidence kind mismatch")
    require(evidence.get("transactionId") == plan.transaction_id, "final-freeze evidence transaction mismatch")
    require(evidence.get("releaseId") == plan.release_id, "final-freeze evidence release mismatch")
    require(evidence.get("sourceCommit") == plan.source_commit, "final-freeze evidence source mismatch")
    require(evidence.get("componentSourceCommits") == dict(plan.component_source_commits), "final-freeze component source pins mismatch")
    require(evidence.get("planSha256") == plan.sha256, "final-freeze evidence plan hash mismatch")
    require(evidence.get("stabilityWindowSeconds") == plan.stability_window_seconds, "final-freeze stability window mismatch")
    require(evidence.get("driverSha256") == {role: plan.components[role].driver_sha256 for role in ROLES}, "final-freeze driver pins mismatch")
    require(evidence.get("artifactGroups") == sorted(release.REQUIRED_ARTIFACTS), "final-freeze artifact groups mismatch")
    require(evidence.get("backupManifestSha256") == args.expected_backup_manifest_sha256, "final-freeze backup hash mismatch")
    require(evidence.get("allIngressAndMutatingServicesStopped") is True, "final-freeze evidence does not prove all services stopped")
    require(evidence.get("automaticResumeAttempted") is False, "final-freeze evidence reports an automatic resume")
    require(evidence.get("paidOrPublicActivationAllowed") is False, "final-freeze evidence permits paid/public activation")
    parse_utc(evidence.get("completedAtUtc"), "final-freeze completion time")
    require(finalized_block(evidence.get("frozenFinalizedBlock"), "final-freeze block") == frozen, "final-freeze and reset-readiness blocks differ")
    require(
        (source_inventory.get("blockNumber"), source_inventory.get("blockHash"))
        == (frozen["number"], frozen["hash"]),
        "legacy source inventory does not use the final frozen block",
    )

    return BoundInputs(
        replacement_lock_sha256=args.expected_replacement_lock_sha256,
        reset_readiness_sha256=args.expected_reset_readiness_sha256,
        final_freeze_evidence_sha256=args.expected_final_freeze_evidence_sha256,
        backup_manifest_sha256=args.expected_backup_manifest_sha256,
        restore_evidence_sha256=args.expected_restore_evidence_sha256,
        migration_evidence_sha256=args.expected_migration_evidence_sha256,
        automatic_restore_arm_sha256=(
            args.expected_automatic_restore_arm_sha256
            if require_automatic_restore_arm
            else None
        ),
        frozen_block=frozen,
        pinned_files=tuple(pinned),
    )


def write_new_bytes(path: Path, payload: bytes, mode: int = 0o600) -> None:
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        descriptor = os.open(path, flags, mode)
    except FileExistsError as exc:
        raise ClosureError(f"refusing to overwrite immutable output: {path}") from exc
    with os.fdopen(descriptor, "wb") as handle:
        handle.write(payload)
        handle.flush()
        os.fsync(handle.fileno())
    os.chmod(path, mode)


def create_private_directory(path: Path, label: str) -> None:
    path = require_normal_absolute(path, label)
    require_no_symlink_components(path, label, include_leaf=False)
    require(not lexists(path), f"refusing to reuse {label}: {path}")
    regular_directory(path.parent, f"{label} parent")
    os.mkdir(path, 0o700)
    os.chmod(path, 0o700)


def run_local_git(arguments: Sequence[str], label: str, *, cwd: Path | None = None) -> str:
    environment = os.environ.copy()
    environment.update(
        {
            "GIT_CONFIG_GLOBAL": os.devnull,
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_OPTIONAL_LOCKS": "0",
        }
    )
    completed = subprocess.run(
        ["git", "-c", "core.hooksPath=/dev/null", *arguments],
        cwd=cwd,
        env=environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    require(completed.returncode == 0, f"{label} failed: {completed.stderr.strip()}")
    return completed.stdout.strip()


def make_tree_read_only(root: Path) -> None:
    for current, directories, files in os.walk(root, topdown=False, followlinks=False):
        current_path = Path(current)
        for name in files:
            path = current_path / name
            if path.is_symlink():
                continue
            os.chmod(path, stat.S_IMODE(path.stat().st_mode) & ~0o222)
        for name in directories:
            path = current_path / name
            if path.is_symlink():
                continue
            os.chmod(path, stat.S_IMODE(path.stat().st_mode) & ~0o222)
    os.chmod(root, stat.S_IMODE(root.stat().st_mode) & ~0o222)


def prepare_immutable_drivers(plan: Plan, state_root: Path) -> dict[str, PreparedDriver]:
    """Materialize each driver from its exact clean Git commit before contact.

    A shared local clone copies the tracked working files while referring to
    content-addressed objects in the source repository.  The private checkout
    is made read-only and the copied driver is rehashed.  Consequently a later
    source-path swap cannot change the bytes that are executed.
    """

    copies_root = state_root / "immutable-driver-sources"
    os.mkdir(copies_root, 0o700)
    template = copies_root / "empty-git-template"
    os.mkdir(template, 0o700)
    repositories: dict[tuple[Path, str], Path] = {}
    role_sources: dict[str, tuple[Path, Path, str]] = {}
    for role in ROLES:
        component = plan.components[role]
        root_text = run_local_git(
            ["-C", str(component.driver.parent), "rev-parse", "--show-toplevel"],
            f"resolve {role} driver repository",
        )
        source_root = regular_directory(root_text, f"{role} driver repository")
        head = ensure_commit(
            run_local_git(["-C", str(source_root), "rev-parse", "HEAD"], f"read {role} driver HEAD"),
            f"{role} driver repository HEAD",
        )
        expected_head = (
            plan.component_source_commits["web"]
            if role in {"site-ingress", "site-indexer-mongo"}
            else plan.source_commit
        )
        require(head == expected_head, f"{role} driver repository commit mismatch")
        require(
            run_local_git(
                ["-C", str(source_root), "status", "--porcelain", "--untracked-files=all"],
                f"inspect {role} driver repository",
            )
            == "",
            f"{role} driver repository is dirty",
        )
        try:
            relative = component.driver.relative_to(source_root)
        except ValueError as exc:
            raise ClosureError(f"{role} driver escapes its Git repository") from exc
        run_local_git(
            ["-C", str(source_root), "ls-files", "--error-unmatch", relative.as_posix()],
            f"verify tracked {role} driver",
        )
        committed_bytes = subprocess.run(
            ["git", "-c", "core.hooksPath=/dev/null", "-C", str(source_root), "show", f"{head}:{relative.as_posix()}"],
            env={**os.environ, "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1"},
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        require(committed_bytes.returncode == 0, f"cannot read committed {role} driver bytes")
        require(
            hashlib.sha256(committed_bytes.stdout).hexdigest() == component.driver_sha256,
            f"{role} driver is not the hash-pinned committed file",
        )
        role_sources[role] = (source_root, relative, head)

    for source_root, _, head in role_sources.values():
        key = (source_root, head)
        if key in repositories:
            continue
        destination = copies_root / f"source-{len(repositories) + 1}"
        run_local_git(
            [
                "clone",
                "--shared",
                "--no-checkout",
                "--no-tags",
                "--template",
                str(template),
                "--",
                str(source_root),
                str(destination),
            ],
            f"clone immutable driver source {source_root}",
        )
        run_local_git(
            ["-C", str(destination), "checkout", "--quiet", "--detach", head],
            f"checkout immutable driver source {head}",
        )
        run_local_git(
            ["-C", str(destination), "remote", "remove", "origin"],
            f"remove immutable driver source remote {head}",
        )
        require(
            run_local_git(
                ["-C", str(destination), "status", "--porcelain", "--untracked-files=all"],
                f"verify immutable driver checkout {head}",
            )
            == "",
            "immutable driver checkout is dirty",
        )
        repositories[key] = destination

    prepared: dict[str, PreparedDriver] = {}
    for role, (source_root, relative, head) in role_sources.items():
        copied_root = repositories[(source_root, head)]
        copied_driver = regular_file(copied_root / relative, f"copied {role} driver")
        require(
            sha256_file(copied_driver) == plan.components[role].driver_sha256,
            f"copied {role} driver SHA-256 mismatch",
        )
        environment = os.environ.copy()
        environment.update(
            {
                "GIT_CONFIG_GLOBAL": os.devnull,
                "GIT_CONFIG_NOSYSTEM": "1",
                "GIT_OPTIONAL_LOCKS": "0",
                "PYTHONDONTWRITEBYTECODE": "1",
            }
        )
        if role in {"site-ingress", "site-indexer-mongo"}:
            default_env = source_root / "tcg/deploy/alpha/macmini2014.env"
            if "ALPHA_MACMINI2014_ENV_FILE" not in environment and default_env.is_file():
                environment["ALPHA_MACMINI2014_ENV_FILE"] = str(default_env)
        else:
            default_env = source_root / "deploy/alpha/macmini2010.env"
            if "ALPHA_MACMINI2010_ENV_FILE" not in environment and default_env.is_file():
                environment["ALPHA_MACMINI2010_ENV_FILE"] = str(default_env)
            for ancestor in source_root.parents:
                media_root = ancestor / "eterra-ipfs-media-service"
                authority_root = ancestor / "SDKGen/Eterra"
                if media_root.is_dir() and authority_root.is_dir():
                    environment.setdefault("MEDIA_REPO_DIR", str(media_root))
                    environment.setdefault("AUTHORITY_REPO_DIR", str(authority_root))
                    break
            copied_overrides = copied_root / "chain-specs/alpha-overrides.json"
            if copied_overrides.is_file():
                environment.setdefault("ALPHA_OVERRIDES_FILE", str(copied_overrides))
        prepared[role] = PreparedDriver(copied_driver, environment)

    make_tree_read_only(copies_root)
    for role, driver in prepared.items():
        require(
            sha256_file(regular_file(driver.path, f"immutable {role} driver"))
            == plan.components[role].driver_sha256,
            f"immutable {role} driver changed before execution",
        )
    return prepared


def bounded_subprocess(
    command: Sequence[str],
    environment: Mapping[str, str],
    timeout_seconds: float,
) -> tuple[int, bytes]:
    require(timeout_seconds > 0, "pre-reset closure verification exceeded 300 seconds")
    process = subprocess.Popen(
        list(command),
        env=dict(environment),
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        start_new_session=True,
    )
    try:
        output, _ = process.communicate(timeout=timeout_seconds)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGTERM)
            output, _ = process.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            output, _ = process.communicate()
        raise ClosureError("pre-reset closure verification exceeded 300 seconds")
    return process.returncode, output


def invoke_verify_frozen(
    plan: Plan,
    role: str,
    bundle_root: Path,
    state_root: Path,
    frozen: Mapping[str, Any],
    prepared_driver: PreparedDriver,
    timeout_seconds: float,
) -> tuple[Path, dict[str, Any]]:
    component = plan.components[role]
    require(
        sha256_file(regular_file(prepared_driver.path, f"immutable {role} driver"))
        == component.driver_sha256,
        f"immutable {role} driver changed before verify-frozen",
    )
    role_root = state_root / role
    os.mkdir(role_root, 0o700)
    os.chmod(role_root, 0o700)
    result_path = role_root / "verify-frozen.json"
    log_path = role_root / "verify-frozen.log"
    command = [
        str(prepared_driver.path),
        *component.arguments,
        "--action",
        "verify-frozen",
        "--transaction-id",
        plan.transaction_id,
        "--release-id",
        plan.release_id,
        "--source-commit",
        plan.source_commit,
        "--component-source-commit",
        plan.component_source_commits[ROLE_SOURCE_COMPONENT[role]],
        "--role",
        role,
        "--target",
        component.target,
        "--bundle-root",
        str(bundle_root),
        "--result",
        str(result_path),
        "--frozen-block-number",
        str(frozen["number"]),
        "--frozen-block-hash",
        str(frozen["hash"]),
    ]
    for group, name in sorted(COMPONENT_ARTIFACTS[role]):
        command.extend(["--artifact", f"{group}:{name}"])
    returncode, output = bounded_subprocess(command, prepared_driver.environment, timeout_seconds)
    write_new_bytes(log_path, output)
    require(returncode == 0, f"{role} verify-frozen driver failed; keep every component stopped; see {log_path}")
    result = read_json(result_path, f"{role} verify-frozen result", canonical=True)
    mode = stat.S_IMODE(result_path.stat().st_mode)
    require(mode & 0o077 == 0, f"{role} verify-frozen result is not owner-only")
    require(set(result) == RESULT_KEYS, f"{role} verify-frozen result schema mismatch")
    require(result.get("schemaVersion") == 1, f"{role} verify-frozen schema mismatch")
    require(result.get("kind") == "nexus-v2-private-alpha-final-freeze-component-result", f"{role} verify-frozen kind mismatch")
    require(result.get("transactionId") == plan.transaction_id, f"{role} verify-frozen transaction mismatch")
    require(result.get("releaseId") == plan.release_id, f"{role} verify-frozen release mismatch")
    require(result.get("sourceCommit") == plan.source_commit, f"{role} verify-frozen source mismatch")
    require(result.get("role") == role and result.get("action") == "verify-frozen", f"{role} verify-frozen action mismatch")
    require(result.get("target") == component.target, f"{role} verify-frozen target mismatch")
    require(result.get("dryRun") is False and result.get("planned") is False, f"{role} returned a planned/dry-run result")
    require(result.get("liveMutationPerformed") is False, f"{role} verify-frozen reported a live mutation")
    parse_utc(result.get("frozenAtUtc"), f"{role} frozen time")
    require(finalized_block(result.get("frozenFinalizedBlock"), f"{role} frozen block") == frozen, f"{role} differs from the final frozen block")
    checks = result.get("checks")
    require(isinstance(checks, dict) and set(checks) == VERIFY_CHECKS[role], f"{role} stopped checks do not match the closed set")
    require(all(value is True for value in checks.values()), f"{role} has a failed stopped check")
    require(result.get("artifacts") == [], f"{role} verify-frozen unexpectedly returned artifacts")
    return result_path, result


def validate_receipt(
    path_value: str | Path,
    expected_sha256: str,
    *,
    max_age_seconds: int = 300,
    expected_release_id: str | None = None,
    expected_source_commit: str | None = None,
    now: dt.datetime | None = None,
) -> dict[str, Any]:
    require(isinstance(max_age_seconds, int) and not isinstance(max_age_seconds, bool) and 0 <= max_age_seconds <= 300, "max age must be in 0..300 seconds")
    path = pinned_file(str(path_value), expected_sha256, "pre-reset closure handoff")
    mode = stat.S_IMODE(path.stat().st_mode)
    require(mode & 0o077 == 0, "pre-reset closure handoff must be owner-only")
    value = read_json(path, "pre-reset closure handoff", canonical=True)
    require(set(value) == RECEIPT_KEYS, "pre-reset closure handoff does not match the closed schema")
    require(value.get("schemaVersion") == 1, "pre-reset closure schema mismatch")
    require(value.get("kind") == "nexus-v2-private-alpha-pre-reset-closure-handoff", "pre-reset closure kind mismatch")
    release_id = ensure_id(value.get("releaseId"), "pre-reset closure release ID")
    source_commit = ensure_commit(value.get("sourceCommit"), "pre-reset closure source commit")
    if expected_release_id is not None:
        require(release_id == ensure_id(expected_release_id, "expected release ID"), "pre-reset closure release mismatch")
    if expected_source_commit is not None:
        require(source_commit == ensure_commit(expected_source_commit, "expected source commit"), "pre-reset closure source mismatch")
    for field in (
        "replacementLockSha256",
        "resetReadinessSha256",
        "finalFreezeEvidenceSha256",
        "backupManifestSha256",
        "restoreEvidenceSha256",
        "migrationEvidenceSha256",
        "automaticRestoreArmSha256",
    ):
        ensure_sha256(value.get(field), f"pre-reset closure {field}")
    observed = parse_utc(value.get("observedAtUtc"), "pre-reset closure observation time")
    if max_age_seconds > 0:
        current = now or dt.datetime.now(dt.timezone.utc)
        require(current.tzinfo is not None, "verification clock must include a timezone")
        current = current.astimezone(dt.timezone.utc)
        age = (current - observed).total_seconds()
        require(age >= -5, "pre-reset closure observation is too far in the future")
        require(age <= max_age_seconds, "pre-reset closure handoff is stale")
    require(value.get("automaticRestoreArmed") is True, "automatic restore is not armed")
    require(value.get("mutationPerformed") is False, "pre-reset closure reports a mutation")
    arm_path_value = value.get("automaticRestoreArmPath")
    require(isinstance(arm_path_value, str), "automatic-restore arm path is invalid")
    arm_path = regular_file(arm_path_value, "automatic-restore arm")
    require(
        sha256_file(arm_path) == value["automaticRestoreArmSha256"],
        "automatic-restore arm hash drifted",
    )
    try:
        import pre_reset_rollback_supervisor as supervisor
    except ImportError as exc:
        raise ClosureError(f"automatic-restore supervisor validator is unavailable: {exc}") from exc
    try:
        arm = supervisor.validate_arm(
            arm_path,
            value["automaticRestoreArmSha256"],
            expected_release_id=release_id,
            expected_source_commit=source_commit,
            full_binding=False,
        )
    except supervisor.SupervisorError as exc:
        raise ClosureError(f"automatic-restore supervisor is not live: {exc}") from exc
    arm_binding_fields = {
        "replacementLockSha256": "replacementLockSha256",
        "resetReadinessSha256": "resetReadinessSha256",
        "finalFreezeEvidenceSha256": "finalFreezeEvidenceSha256",
        "backupManifestSha256": "backupManifestSha256",
        "restoreEvidenceSha256": "restoreEvidenceSha256",
        "migrationEvidenceSha256": "migrationEvidenceSha256",
    }
    for receipt_field, arm_field in arm_binding_fields.items():
        require(
            value[receipt_field] == arm[arm_field],
            f"pre-reset closure {receipt_field} does not match the live arm",
        )
    arm_issued = parse_utc(arm.get("issuedAtUtc"), "automatic-restore arm issue time")
    receipt_issue_delta = (observed - arm_issued).total_seconds()
    require(
        -5 <= receipt_issue_delta <= 300,
        "automatic-restore arm was not fresh when the pre-reset closure was created",
    )
    components = value.get("components")
    require(isinstance(components, dict) and set(components) == set(RECEIPT_COMPONENTS), "pre-reset closure components do not match the closed set")
    for role in RECEIPT_COMPONENTS:
        component = components[role]
        require(isinstance(component, dict) and set(component) == COMPONENT_RECEIPT_KEYS, f"{role} closure component schema mismatch")
        ensure_sha256(component.get("driverSha256"), f"{role} closure driver SHA-256")
        ensure_sha256(component.get("verifyFrozenResultSha256"), f"{role} verify-frozen result SHA-256")
        require(component.get("stopped") is True, f"{role} is not stopped")
    require(value.get("protectedListeners") == PROTECTED_LISTENERS, "pre-reset protected-listener contract mismatch")
    return value


def command_create(args: argparse.Namespace) -> None:
    output = output_path(args.output, "pre-reset closure handoff")
    state_root = require_normal_absolute(Path(args.state_root), "pre-reset closure state root")
    require_no_symlink_components(state_root, "pre-reset closure state root", include_leaf=False)
    require(not lexists(state_root), f"refusing to reuse pre-reset closure state root: {state_root}")
    bundle_root = regular_directory(args.bundle_root, "final-freeze bundle root")
    plan = validate_plan(args.plan, args.expected_plan_sha256)
    bound = validate_bound_inputs(args, plan, bundle_root)

    # No protected target is contacted until all immutable evidence has passed.
    create_private_directory(state_root, "pre-reset closure state root")
    prepared_drivers = prepare_immutable_drivers(plan, state_root)
    observed_at = dt.datetime.now(dt.timezone.utc).replace(microsecond=0)
    deadline = time.monotonic() + 300
    results: dict[str, Path] = {}
    for role in ROLES:
        result_path, _ = invoke_verify_frozen(
            plan,
            role,
            bundle_root,
            state_root,
            bound.frozen_block,
            prepared_drivers[role],
            deadline - time.monotonic(),
        )
        results[role] = result_path

    # Close all direct and transitive TOCTOU windows before issuing the
    # short-lived handoff. This repeats repository/artifact and backup-manifest
    # traversal, not merely the six top-level hashes.
    revalidated_plan = validate_plan(args.plan, args.expected_plan_sha256)
    require(revalidated_plan == plan, "final-freeze plan changed during closure verification")
    revalidated_bound = validate_bound_inputs(args, revalidated_plan, bundle_root)
    require(revalidated_bound == bound, "pre-reset evidence changed during closure verification")
    require(time.monotonic() <= deadline, "pre-reset closure verification exceeded 300 seconds")
    for role in ROLES:
        require(
            sha256_file(regular_file(prepared_drivers[role].path, f"immutable {role} driver"))
            == plan.components[role].driver_sha256,
            f"immutable {role} driver changed during closure verification",
        )

    require(
        bound.automatic_restore_arm_sha256 is not None,
        "automatic-restore arm hash is unavailable",
    )
    receipt = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-pre-reset-closure-handoff",
        "releaseId": plan.release_id,
        "sourceCommit": plan.source_commit,
        "replacementLockSha256": bound.replacement_lock_sha256,
        "resetReadinessSha256": bound.reset_readiness_sha256,
        "finalFreezeEvidenceSha256": bound.final_freeze_evidence_sha256,
        "backupManifestSha256": bound.backup_manifest_sha256,
        "restoreEvidenceSha256": bound.restore_evidence_sha256,
        "migrationEvidenceSha256": bound.migration_evidence_sha256,
        "automaticRestoreArmSha256": bound.automatic_restore_arm_sha256,
        "automaticRestoreArmPath": str(
            regular_file(args.automatic_restore_arm, "automatic-restore arm")
        ),
        "observedAtUtc": format_utc(observed_at),
        "automaticRestoreArmed": True,
        "mutationPerformed": False,
        "components": {
            role: {
                "driverSha256": plan.components[role].driver_sha256,
                "verifyFrozenResultSha256": sha256_file(
                    regular_file(results[role], f"{role} verify-frozen result")
                ),
                "stopped": True,
            }
            for role in RECEIPT_COMPONENTS
        },
        "protectedListeners": PROTECTED_LISTENERS,
    }
    write_new_bytes(output, canonical_bytes(receipt), mode=0o600)
    digest = sha256_file(output)
    validate_receipt(
        output,
        digest,
        max_age_seconds=300,
        expected_release_id=plan.release_id,
        expected_source_commit=plan.source_commit,
    )
    print(json.dumps({"path": str(output), "sha256": digest}, sort_keys=True, separators=(",", ":")))


def command_verify(args: argparse.Namespace) -> None:
    value = validate_receipt(
        args.handoff,
        args.expected_sha256,
        max_age_seconds=args.max_age_seconds,
        expected_release_id=args.release_id,
        expected_source_commit=args.source_commit,
    )
    print(
        json.dumps(
            {
                "schemaVersion": value["schemaVersion"],
                "kind": value["kind"],
                "releaseId": value["releaseId"],
                "sourceCommit": value["sourceCommit"],
                "sha256": args.expected_sha256,
                "observedAtUtc": value["observedAtUtc"],
                "maxAgeSeconds": args.max_age_seconds,
            },
            sort_keys=True,
            separators=(",", ":"),
        )
    )


def add_pinned_argument(parser: argparse.ArgumentParser, name: str) -> None:
    parser.add_argument(f"--{name.replace('_', '-')}", required=True)
    parser.add_argument(f"--expected-{name.replace('_', '-')}-sha256", required=True)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    create = commands.add_parser("create", help="re-prove all stopped components and create the handoff")
    create.add_argument("--plan", required=True)
    create.add_argument("--expected-plan-sha256", required=True)
    create.add_argument("--bundle-root", required=True)
    create.add_argument("--state-root", required=True)
    add_pinned_argument(create, "replacement_lock")
    create.add_argument("--selected-deployment-environment", required=True)
    create.add_argument("--selected-site-deployment-environment", required=True)
    add_pinned_argument(create, "reset_readiness")
    add_pinned_argument(create, "final_freeze_evidence")
    add_pinned_argument(create, "backup_manifest")
    add_pinned_argument(create, "restore_evidence")
    add_pinned_argument(create, "migration_evidence")
    add_pinned_argument(create, "automatic_restore_arm")
    create.add_argument("--output", required=True)
    create.set_defaults(func=command_create)

    verify = commands.add_parser("verify", help="verify the hash, schema, owner-only mode, and freshness")
    verify.add_argument("--handoff", required=True)
    verify.add_argument("--expected-sha256", required=True)
    verify.add_argument("--release-id", required=True)
    verify.add_argument("--source-commit", required=True)
    verify.add_argument(
        "--max-age-seconds",
        type=int,
        default=300,
        help="freshness limit in seconds; 0 performs hash/schema/identity-only verification",
    )
    verify.set_defaults(func=command_verify)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        args.func(args)
    except (ClosureError, OSError, release.SafetyError, release_lock.ReleaseLockError, verify_reset_readiness.ReadinessError) as exc:
        print(f"pre_reset_closure: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
