#!/usr/bin/env python3
"""Foreground supervisor for the guarded Nexus V2 fresh replacement.

The supervisor arms automatic restore only after both protected-host recovery
drivers pass credential/archive/restore preflight.  It then invokes exactly one
hash-pinned replacement workflow driver and remains alive until a separately
verified acceptance-boundary handoff retires rollback.  Any workflow error,
uncaught exception, timeout, or termination signal after arming runs both
recovery lanes in the closed order:

    pause-v2-writes -> archive-failed-v2 -> restore-final-backup -> restored-smoke

This coordinator never reads V2 RPC state.  That is intentional: the earliest
partial reset can fail before a V2 RPC exists.  Production child drivers must
use existing credentials internally and require the explicit
``PRIVATE_ALPHA_ROLLBACK_ONLY`` confirmation.  Fixture mode is permanently
marked NONDEPLOYABLE and cannot create a production closure handoff.
"""

from __future__ import annotations

import argparse
import ctypes
import copy
import datetime as dt
import hashlib
import json
import os
import platform
import re
import secrets
import signal
import stat
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable, Mapping, Sequence


TOOL_ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOL_ROOT))
import deployment_secret_environment  # noqa: E402,F401
import pre_reset_closure as closure  # noqa: E402


SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
HASH256_RE = re.compile(r"^0x[0-9a-f]{64}$")
ID_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
SITE_RELEASE_RE = re.compile(
    r"^v(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
    r"(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$"
)
UTC_RE = re.compile(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$")
PRODUCTION_BACKEND = "protected-private-alpha"
FIXTURE_BACKEND = "fixture-nondeployable"
PRODUCTION_CONFIRMATION = "PRIVATE_ALPHA_ROLLBACK_ONLY"
COMPONENTS = ("chain-media", "site-indexer")
RECOVERY_ACTIONS = (
    "pause-v2-writes",
    "archive-failed-v2",
    "restore-final-backup",
    "restored-smoke",
)
ARCHIVE_PREPARATION_ACTION = "prepare-reset-archives"
SOURCE_IDS = {"chain", "media", "site"}
ARTIFACT_IDS = {
    "finalFreezePlan",
    "replacementLock",
    "resetReadiness",
    "finalFreezeEvidence",
    "backupManifest",
    "restoreEvidence",
    "migrationEvidence",
}
PLAN_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "releaseId",
    "siteReleaseVersion",
    "sourceCommit",
    "backend",
    "fixtureRoot",
    "createdAtUtc",
    "expiresAtUtc",
    "frozenFinalizedBlock",
    "bundleRoot",
    "selectedDeploymentEnvironment",
    "selectedSiteDeploymentEnvironment",
    "artifacts",
    "sources",
    "supervisor",
    "workflow",
    "acceptanceStartFence",
    "components",
    "policy",
}
PIN_KEYS = {"path", "sha256"}
SOURCE_KEYS = {"root", "expectedCommit"}
EXECUTABLE_PIN_KEYS = {"sourceId", "path", "sha256"}
SUPERVISOR_KEYS = {"sourceId", "path", "sha256"}
WORKFLOW_KEYS = {"driver", "helperPins", "contract"}
WORKFLOW_HELPER_ROLES = {
    "createPreResetClosure",
    "deployChainMediaAuthority",
    "deploySiteIndexer",
    "closeIngressAndObserve",
    "createZeroAssetAcceptanceFence",
}
WORKFLOW_CONTRACT_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "releaseId",
    "siteReleaseVersion",
    "sourceCommit",
    "frozenFinalizedBlock",
    "artifactSha256",
    "toolPins",
    "stageOrder",
    "stageInputs",
    "fixtureOnly",
    "acceptanceStartFencePath",
    "bootstrapOrAcceptanceWritesAllowed",
    "paidOrPublicActivationAllowed",
}
WORKFLOW_TOOL_ROLES = {
    "preResetClosure",
    "chainDeployAll",
    "siteDeploy",
    "phase1IngressClosure",
    "acceptanceBoundary",
    "postCutoverCoordinator",
}
WORKFLOW_STAGE_INPUT_KEYS = {
    "createPreResetClosure": set(),
    "deployChainMediaAuthority": {
        "nodeCandidatePath",
        "nodeCandidateSha256",
        "nodeTargetIdentityPath",
        "nodeTargetIdentitySha256",
        "mediaCandidatePath",
        "mediaCandidateSha256",
    },
    "deploySiteIndexer": {
        "siteCandidatePath",
        "siteCandidateSha256",
        "phase1CaddyfilePath",
        "phase1CaddyfileSha256",
    },
    "closeIngressAndObserve": {
        "stabilityWindowSeconds",
        "runtimeBundleRoot",
        "runtimeBundleManifestSha256",
    },
    "createZeroAssetAcceptanceFence": {
        "runtimeBundleRoot",
        "runtimeBundleManifestSha256",
        "siteDriverPath",
        "siteRestorePath",
        "siteDeployPath",
        "siteStatusPath",
        "resetArchiveRoot",
        "maxObservationAgeSeconds",
    },
}
EXPECTED_ZERO_FENCE_SITE_PATHS = {
    "siteDriverPath": (
        "tcg/deploy/alpha/macmini2014/nexus-v2-rollback-component-driver"
    ),
    "siteRestorePath": "tcg/deploy/alpha/macmini2014/restore-alpha-state.sh",
    "siteDeployPath": "tcg/deploy/alpha/macmini2014/deploy-site.sh",
    "siteStatusPath": "tcg/deploy/alpha/macmini2014/status.sh",
}
ACCEPTANCE_START_FENCE_KEYS = {
    "handoffPath",
    "verifier",
    "genesisHash",
    "runtimeCodeSha256",
    "runtimeMetadataScaleSha256",
    "pollMilliseconds",
}
COMPONENT_KEYS = {
    "driver",
    "helperPins",
    "scriptPins",
    "requiredResetArchives",
}
COMPONENT_SCRIPT_ROLES = {
    "chain-media": {
        "restoreState",
        "deployNode",
        "deployMedia",
        "status",
        "rollbackStagingLibrary",
    },
    "site-indexer": {"restoreState", "deploySite", "status"},
}
COMPONENT_ARCHIVES = {
    "chain-media": {"node", "media"},
    "site-indexer": {"site"},
}
EXPECTED_WORKFLOW_DRIVER = (
    "chain",
    "scripts/nexus-v2-private-alpha/pre_reset_replacement_workflow.py",
)
EXPECTED_WORKFLOW_HELPERS = {
    "createPreResetClosure": (
        "chain",
        "scripts/nexus-v2-private-alpha/pre_reset_chain_workflow_stage.py",
    ),
    "deployChainMediaAuthority": (
        "chain",
        "scripts/nexus-v2-private-alpha/pre_reset_chain_workflow_stage.py",
    ),
    "deploySiteIndexer": (
        "chain",
        "scripts/nexus-v2-private-alpha/pre_reset_site_workflow_stage.py",
    ),
    "closeIngressAndObserve": (
        "chain",
        "scripts/nexus-v2-private-alpha/pre_reset_site_workflow_stage.py",
    ),
    "createZeroAssetAcceptanceFence": (
        "chain",
        "scripts/nexus-v2-private-alpha/pre_reset_zero_asset_fence_stage.py",
    ),
}
EXPECTED_WORKFLOW_TOOLS = {
    "preResetClosure": (
        "chain",
        "scripts/nexus-v2-private-alpha/pre_reset_closure.py",
    ),
    "chainDeployAll": ("chain", "deploy/alpha/macmini2010/deploy-all.sh"),
    "siteDeploy": ("site", "tcg/deploy/alpha/macmini2014/deploy-site.sh"),
    "phase1IngressClosure": (
        "site",
        "tcg/deploy/alpha/macmini2014/nexus_v2_phase1_ingress_closure.py",
    ),
    "acceptanceBoundary": (
        "chain",
        "scripts/nexus-v2-private-alpha/acceptance_boundary.py",
    ),
    "postCutoverCoordinator": (
        "chain",
        "deploy/alpha/macmini2010/nexus-v2-post-cutover-coordinator.py",
    ),
}
EXPECTED_COMPONENT_DRIVERS = {
    "chain-media": (
        "chain",
        "deploy/alpha/macmini2010/nexus-v2-pre-reset-chain-media-component-driver",
    ),
    "site-indexer": (
        "site",
        "tcg/deploy/alpha/macmini2014/nexus-v2-pre-reset-recovery-site-host-action",
    ),
}
EXPECTED_COMPONENT_HELPERS = {
    "chain-media": {
        "hostAction": (
            "chain",
            "deploy/alpha/macmini2010/nexus-v2-rollback-protected-host-action.sh",
        )
    },
    "site-indexer": {
        "hostAction": (
            "site",
            "tcg/deploy/alpha/macmini2014/nexus-v2-pre-reset-recovery-site-host-action",
        )
    },
}
EXPECTED_COMPONENT_SCRIPTS = {
    "chain-media": {
        "restoreState": ("chain", "deploy/alpha/macmini2010/restore-alpha-state.sh"),
        "deployNode": ("chain", "deploy/alpha/macmini2010/deploy-node.sh"),
        "deployMedia": ("chain", "deploy/alpha/macmini2010/deploy-media.sh"),
        "status": ("chain", "deploy/alpha/macmini2010/status.sh"),
        "rollbackStagingLibrary": (
            "chain",
            "deploy/alpha/macmini2010/nexus_v2_rollback_staging.py",
        ),
    },
    "site-indexer": {
        "restoreState": (
            "site",
            "tcg/deploy/alpha/macmini2014/restore-alpha-state.sh",
        ),
        "deploySite": ("site", "tcg/deploy/alpha/macmini2014/deploy-site.sh"),
        "status": ("site", "tcg/deploy/alpha/macmini2014/status.sh"),
    },
}
POLICY = {
    "automaticRecoveryRequired": True,
    "fixtureMayContactProtectedHosts": False,
    "paidOrPublicActivationAuthorized": False,
    "preserveFailedV2Roots": True,
    "productionConfirmation": PRODUCTION_CONFIRMATION,
    "requiresV2RpcOrCapture": False,
}
FIXTURE_CONTRACT_KEYS = {
    "schemaVersion",
    "kind",
    "fixtureOnly",
    "artifactBindingsValid",
    "credentialsResolvable",
    "requiredResetArchives",
    "economicFlagsDisabled",
}
COMPONENT_RESULT_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "releaseId",
    "siteReleaseVersion",
    "planSha256",
    "componentId",
    "mode",
    "action",
    "result",
    "fixtureOnly",
    "mutationPerformed",
    "credentialsResolvable",
    "requiredResetArchivesPresent",
    "failedV2RootArchiveSha256",
    "checks",
    "completedAtUtc",
}
WORKFLOW_RESULT_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "releaseId",
    "siteReleaseVersion",
    "planSha256",
    "result",
    "fixtureOnly",
    "mutationPerformed",
    "acceptanceStartFenceWritten",
    "completedAtUtc",
}
PREFLIGHT_CHECKS = {
    "archivesReadable",
    "credentialsResolvable",
    "driverHashVerified",
    "helperHashesVerified",
    "noMutation",
    "restoreInputsVerified",
    "scriptHashesVerified",
    "sourcePinsVerified",
}
ACTION_CHECKS = {
    "pause-v2-writes": {
        "noV2RpcRequired",
        "statePreserved",
        "v2WritesPaused",
    },
    "archive-failed-v2": {
        "failedV2RootArchived",
        "failedV2RootPreserved",
        "noV2RpcRequired",
    },
    "restore-final-backup": {
        "failedV2RootPreserved",
        "finalBackupRestored",
        "noV2RpcRequired",
    },
    "restored-smoke": {
        "backupIdentityMatched",
        "economicFlagsDisabled",
        "failedV2RootPreserved",
        "restoredComponentHealthy",
    },
}
ARCHIVE_PREPARATION_CHECKS = {
    "archivePreparationNonDestructive",
    "archivesPreparedAndReadOnly",
    "currentAlphaStatePreserved",
    "noResetApplied",
    "readinessIdentityBound",
    "restoreInputsVerified",
    "sourcePinsVerified",
}
LEASE_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "releaseId",
    "siteReleaseVersion",
    "sourceCommit",
    "pid",
    "processStartToken",
    "nonce",
    "createdAtUtc",
    "expiresAtUtc",
    "state",
    "retiredAtUtc",
    "retirementEvidenceSha256",
}
ARM_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "releaseId",
    "siteReleaseVersion",
    "sourceCommit",
    "planPath",
    "planSha256",
    "supervisorSha256",
    "workflowDriverSha256",
    "componentDriverSha256",
    "archivePreparationResults",
    "preflightResults",
    "frozenFinalizedBlock",
    "replacementLockSha256",
    "resetReadinessSha256",
    "finalFreezeEvidenceSha256",
    "backupManifestSha256",
    "restoreEvidenceSha256",
    "migrationEvidenceSha256",
    "pid",
    "processStartToken",
    "leasePath",
    "leaseNonceSha256",
    "handlersInstalled",
    "issuedAtUtc",
    "expiresAtUtc",
    "fixtureOnly",
    "automaticRestoreArmed",
    "paidOrPublicActivationAllowed",
}
PREFLIGHT_PIN_KEYS = {"path", "sha256"}
EVIDENCE_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "planSha256",
    "releaseId",
    "siteReleaseVersion",
    "sourceCommit",
    "backend",
    "fixtureOnly",
    "outcome",
    "trigger",
    "automaticRestoreArmSha256",
    "archivePreparationResultSha256",
    "workflowResultSha256",
    "acceptanceStartFenceSha256",
    "recovery",
    "automaticRestorePerformed",
    "automaticRestoreRetired",
    "failedV2RootsPreserved",
    "paidOrPublicActivationAllowed",
    "completedAtUtc",
}
RECOVERY_ENTRY_KEYS = {"status", "resultPath", "resultSha256", "error"}


class SupervisorError(RuntimeError):
    """The foreground rollback guarantee could not be established."""


class SupervisorSignal(SupervisorError):
    def __init__(self, signum: int):
        self.signum = signum
        super().__init__(f"received signal {signum}")


@dataclass
class RuntimeState:
    active_process: subprocess.Popen[bytes] | None = None
    signal_number: int | None = None
    handlers_installed: bool = False


RUNTIME = RuntimeState()


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SupervisorError(message)


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


def ensure_site_release(value: Any, label: str) -> str:
    require(
        isinstance(value, str) and SITE_RELEASE_RE.fullmatch(value) is not None,
        f"invalid {label}; expected a v-prefixed semantic version",
    )
    return value


def parse_utc(value: Any, label: str) -> dt.datetime:
    require(isinstance(value, str) and UTC_RE.fullmatch(value) is not None, f"invalid {label}")
    return dt.datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=dt.timezone.utc)


def format_utc(value: dt.datetime) -> str:
    require(value.tzinfo is not None, "timestamp must include a timezone")
    return value.astimezone(dt.timezone.utc).replace(microsecond=0).strftime("%Y-%m-%dT%H:%M:%SZ")


def utc_now() -> str:
    return format_utc(dt.datetime.now(dt.timezone.utc))


def finalized_block(value: Any, label: str) -> dict[str, Any]:
    require(isinstance(value, dict) and set(value) == {"number", "hash"}, f"{label} schema mismatch")
    number = value.get("number")
    block_hash = value.get("hash")
    require(isinstance(number, int) and not isinstance(number, bool) and number >= 0, f"invalid {label} number")
    require(isinstance(block_hash, str) and HASH256_RE.fullmatch(block_hash.lower()) is not None, f"invalid {label} hash")
    return {"number": number, "hash": block_hash.lower()}


def read_json(path: Path, label: str, *, canonical: bool = True) -> dict[str, Any]:
    path = closure.regular_file(path, label)
    try:
        payload = path.read_bytes()
        value = json.loads(payload, object_pairs_hook=closure.duplicate_rejecting_object)
    except (OSError, json.JSONDecodeError) as exc:
        raise SupervisorError(f"invalid {label}") from exc
    require(isinstance(value, dict), f"{label} root must be an object")
    if canonical:
        require(payload == canonical_bytes(value), f"{label} must be canonical JSON")
    return value


def write_new(path: Path, value: Mapping[str, Any], mode: int = 0o600) -> None:
    closure.output_path(path, "supervisor output")
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    descriptor = os.open(path, flags, mode)
    with os.fdopen(descriptor, "wb") as handle:
        handle.write(canonical_bytes(value))
        handle.flush()
        os.fsync(handle.fileno())
    os.chmod(path, mode)


def git_output(root: Path, *arguments: str) -> str:
    completed = subprocess.run(
        ["git", "-C", str(root), *arguments],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
        env={
            **os.environ,
            "GIT_CONFIG_GLOBAL": os.devnull,
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_OPTIONAL_LOCKS": "0",
        },
    )
    require(completed.returncode == 0, f"cannot inspect source root: {root}")
    return completed.stdout.strip()


def validate_sources(value: Any) -> dict[str, dict[str, Any]]:
    require(isinstance(value, dict) and set(value) == SOURCE_IDS, "source pins do not match the closed set")
    sources: dict[str, dict[str, Any]] = {}
    for source_id in sorted(SOURCE_IDS):
        pin = value[source_id]
        require(isinstance(pin, dict) and set(pin) == SOURCE_KEYS, f"{source_id} source pin schema mismatch")
        root = closure.regular_directory(pin.get("root"), f"{source_id} source root")
        commit = ensure_commit(pin.get("expectedCommit"), f"{source_id} source commit")
        require(git_output(root, "rev-parse", "HEAD") == commit, f"{source_id} source commit drifted")
        require(git_output(root, "status", "--porcelain", "--untracked-files=all") == "", f"{source_id} source is dirty")
        sources[source_id] = {"root": root, "commit": commit}
    return sources


def executable_pin(value: Any, sources: Mapping[str, Mapping[str, Any]], label: str) -> dict[str, Any]:
    require(isinstance(value, dict) and set(value) == EXECUTABLE_PIN_KEYS, f"{label} schema mismatch")
    source_id = value.get("sourceId")
    require(source_id in sources, f"{label} source is not pinned")
    relative = value.get("path")
    require(isinstance(relative, str) and relative and not relative.startswith("/"), f"{label} path must be relative")
    root = sources[source_id]["root"]
    path = (root / relative).resolve()
    require(root in path.parents, f"{label} escapes its source root")
    path = closure.regular_file(path, label)
    require(path.stat().st_mode & stat.S_IXUSR, f"{label} is not owner-executable")
    digest = ensure_sha256(value.get("sha256"), f"{label} SHA-256")
    require(sha256_file(path) == digest, f"{label} SHA-256 drifted")
    require(git_output(root, "ls-files", "--error-unmatch", relative) == relative, f"{label} is not tracked")
    require(
        subprocess.run(
            ["git", "-C", str(root), "diff", "--quiet", "HEAD", "--", relative],
            check=False,
        ).returncode
        == 0,
        f"{label} differs from its clean source commit",
    )
    return {"sourceId": source_id, "path": path, "sha256": digest, "relative": relative}


def pin_map(
    value: Any,
    sources: Mapping[str, Mapping[str, Any]],
    expected_roles: set[str] | None,
    label: str,
) -> dict[str, dict[str, Any]]:
    require(isinstance(value, dict) and bool(value), f"{label} must be a non-empty object")
    if expected_roles is not None:
        require(set(value) == expected_roles, f"{label} roles do not match the closed set")
    return {
        role: executable_pin(pin, sources, f"{label} {role}")
        for role, pin in value.items()
    }


def require_pin_identity(
    pin: Mapping[str, Any],
    expected: tuple[str, str],
    label: str,
) -> None:
    require(
        (pin.get("sourceId"), pin.get("relative")) == expected,
        f"{label} is not the reviewed executable",
    )


def prepare_immutable_plan(
    plan: Mapping[str, Any], state_root: Path
) -> dict[str, Any]:
    """Clone each clean pinned source and execute only committed bytes.

    The immutable roots are local, detached, read-only Git clones.  Child
    adapters receive their locations through closed environment variables and
    must resolve every helper/script pin against those roots, never against the
    user's original worktrees.
    """

    immutable = copy.deepcopy(dict(plan))
    sources = plan.get("sources")
    require(isinstance(sources, Mapping), "normalized plan source pins are invalid")
    if not sources:
        immutable["immutableSources"] = {}
        return immutable
    clone_root = state_root / "immutable-sources"
    require(not os.path.lexists(clone_root), "immutable source root already exists")
    clone_root.mkdir(mode=0o700)
    clones: dict[str, Path] = {}
    for source_id in sorted(sources):
        source = closure.regular_directory(
            sources[source_id]["root"], f"{source_id} source root"
        )
        commit = ensure_commit(
            sources[source_id]["commit"], f"{source_id} immutable source commit"
        )
        destination = clone_root / source_id
        completed = subprocess.run(
            [
                "git",
                "clone",
                "--quiet",
                "--no-hardlinks",
                "--no-checkout",
                str(source),
                str(destination),
            ],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
            timeout=120,
        )
        require(completed.returncode == 0, f"cannot clone immutable {source_id} source")
        completed = subprocess.run(
            ["git", "-C", str(destination), "checkout", "--quiet", "--detach", commit],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
            timeout=120,
        )
        require(
            completed.returncode == 0,
            f"cannot check out immutable {source_id} source commit",
        )
        require(
            git_output(destination, "rev-parse", "HEAD") == commit
            and git_output(destination, "status", "--porcelain", "--untracked-files=all")
            == "",
            f"immutable {source_id} source verification failed",
        )
        clones[source_id] = destination.resolve()

    def remap(pin: Mapping[str, Any], label: str) -> dict[str, Any]:
        source_id = pin["sourceId"]
        relative = pin["relative"]
        path = closure.regular_file(clones[source_id] / relative, label)
        require(sha256_file(path) == pin["sha256"], f"immutable {label} hash mismatch")
        mapped = dict(pin)
        mapped["path"] = path
        return mapped

    immutable["workflow"]["driver"] = remap(
        plan["workflow"]["driver"], "replacement workflow driver"
    )
    immutable["workflow"]["helperPins"] = {
        role: remap(pin, f"replacement workflow helper {role}")
        for role, pin in plan["workflow"]["helperPins"].items()
    }
    immutable["workflow"]["toolPins"] = {
        role: remap(pin, f"replacement workflow nested tool {role}")
        for role, pin in plan["workflow"]["toolPins"].items()
    }
    immutable["acceptanceStartFence"]["verifier"] = remap(
        plan["acceptanceStartFence"]["verifier"], "acceptance-start verifier"
    )
    for component_id in COMPONENTS:
        immutable["components"][component_id]["driver"] = remap(
            plan["components"][component_id]["driver"], f"{component_id} driver"
        )
        for group in ("helperPins", "scriptPins"):
            immutable["components"][component_id][group] = {
                role: remap(pin, f"{component_id} {group} {role}")
                for role, pin in plan["components"][component_id][group].items()
            }
    for source_root in clones.values():
        for current, directories, files in os.walk(source_root, topdown=False):
            for name in files:
                path = Path(current) / name
                if not path.is_symlink():
                    os.chmod(path, stat.S_IMODE(path.stat().st_mode) & ~0o222)
            for name in directories:
                path = Path(current) / name
                if not path.is_symlink():
                    os.chmod(path, stat.S_IMODE(path.stat().st_mode) & ~0o222)
        os.chmod(source_root, stat.S_IMODE(source_root.stat().st_mode) & ~0o222)
    immutable["immutableSources"] = clones
    return immutable


def verify_immutable_plan(plan: Mapping[str, Any]) -> None:
    contract = plan["workflow"]["contract"]
    require(
        sha256_file(
            closure.regular_file(contract["path"], "replacement workflow contract")
        )
        == contract["sha256"],
        "replacement workflow contract drifted",
    )
    for artifact_id, pin in plan["artifacts"].items():
        require(
            sha256_file(closure.regular_file(pin["path"], f"pinned {artifact_id}"))
            == pin["sha256"],
            f"pinned {artifact_id} drifted",
        )
    for label, pin in (
        ("replacement workflow driver", plan["workflow"]["driver"]),
        ("acceptance-start verifier", plan["acceptanceStartFence"]["verifier"]),
    ):
        require(sha256_file(closure.regular_file(pin["path"], label)) == pin["sha256"], f"{label} drifted")
    for role, pin in plan["workflow"].get("toolPins", {}).items():
        require(
            sha256_file(
                closure.regular_file(pin["path"], f"workflow nested tool {role}")
            )
            == pin["sha256"],
            f"workflow nested tool {role} drifted",
        )
    for role, pin in plan["workflow"].get("helperPins", {}).items():
        require(
            sha256_file(
                closure.regular_file(pin["path"], f"workflow stage helper {role}")
            )
            == pin["sha256"],
            f"workflow stage helper {role} drifted",
        )
    for component_id in COMPONENTS:
        pins = [plan["components"][component_id]["driver"]]
        pins.extend(plan["components"][component_id]["helperPins"].values())
        pins.extend(plan["components"][component_id]["scriptPins"].values())
        for pin in pins:
            require(
                sha256_file(closure.regular_file(pin["path"], f"immutable {component_id} pin"))
                == pin["sha256"],
                f"immutable {component_id} pin drifted",
            )


def validate_fixture_root(value: Any, backend: str) -> tuple[Path | None, dict[str, Any] | None]:
    if backend == PRODUCTION_BACKEND:
        require(value is None, "production plan may not name a fixture root")
        return None, None
    require(isinstance(value, str), "fixture plan must name a fixture root")
    root = closure.regular_directory(value, "NONDEPLOYABLE fixture root")
    require(root.name.endswith(".NONDEPLOYABLE"), "fixture root must end in .NONDEPLOYABLE")
    contract = read_json(root / "fixture-contract.json", "NONDEPLOYABLE fixture contract")
    require(set(contract) == FIXTURE_CONTRACT_KEYS, "fixture contract schema mismatch")
    require(contract.get("schemaVersion") == 1, "fixture schema mismatch")
    require(contract.get("kind") == "nexus-v2-pre-reset-supervisor-fixture", "fixture kind mismatch")
    require(contract.get("fixtureOnly") is True, "fixture is not marked NONDEPLOYABLE")
    require(contract.get("artifactBindingsValid") is True, "fixture artifact bindings are invalid")
    require(contract.get("economicFlagsDisabled") is True, "fixture economic flags are enabled")
    credentials = contract.get("credentialsResolvable")
    archives = contract.get("requiredResetArchives")
    require(isinstance(credentials, dict) and set(credentials) == set(COMPONENTS), "fixture credential set mismatch")
    require(all(value is True for value in credentials.values()), "fixture credentials are unavailable")
    require(isinstance(archives, dict) and set(archives) == set(COMPONENTS), "fixture archive set mismatch")
    for component in COMPONENTS:
        require(
            isinstance(archives[component], dict)
            and set(archives[component]) == COMPONENT_ARCHIVES[component]
            and all(value is True for value in archives[component].values()),
            f"fixture archives are incomplete: {component}",
        )
    return root, contract


def validate_artifact_pins(value: Any) -> dict[str, dict[str, Any]]:
    require(isinstance(value, dict) and set(value) == ARTIFACT_IDS, "artifact pins do not match the closed set")
    pins: dict[str, dict[str, Any]] = {}
    for artifact_id in sorted(ARTIFACT_IDS):
        pin = value[artifact_id]
        require(isinstance(pin, dict) and set(pin) == PIN_KEYS, f"{artifact_id} pin schema mismatch")
        path = closure.regular_file(pin.get("path"), artifact_id)
        digest = ensure_sha256(pin.get("sha256"), f"{artifact_id} SHA-256")
        require(sha256_file(path) == digest, f"{artifact_id} hash drifted")
        pins[artifact_id] = {"path": path, "sha256": digest}
    return pins


def validate_full_artifact_binding(
    raw: Mapping[str, Any],
    pins: Mapping[str, Mapping[str, Any]],
    fixture_only: bool,
    fixture_contract: Mapping[str, Any] | None,
) -> closure.BoundInputs | None:
    if fixture_only:
        require(fixture_contract is not None and fixture_contract["artifactBindingsValid"] is True, "fixture bindings failed")
        for artifact_id, pin in pins.items():
            value = read_json(pin["path"], f"fixture {artifact_id}")
            require(value.get("releaseId") == raw["releaseId"], f"fixture {artifact_id} release mismatch")
            require(value.get("sourceCommit") == raw["sourceCommit"], f"fixture {artifact_id} source mismatch")
        return None

    freeze_plan = closure.validate_plan(
        str(pins["finalFreezePlan"]["path"]),
        pins["finalFreezePlan"]["sha256"],
    )
    require(freeze_plan.release_id == raw["releaseId"], "final-freeze plan release mismatch")
    require(freeze_plan.source_commit == raw["sourceCommit"], "final-freeze plan source mismatch")
    namespace = argparse.Namespace(
        replacement_lock=str(pins["replacementLock"]["path"]),
        expected_replacement_lock_sha256=pins["replacementLock"]["sha256"],
        selected_deployment_environment=raw["selectedDeploymentEnvironment"],
        selected_site_deployment_environment=raw["selectedSiteDeploymentEnvironment"],
        reset_readiness=str(pins["resetReadiness"]["path"]),
        expected_reset_readiness_sha256=pins["resetReadiness"]["sha256"],
        final_freeze_evidence=str(pins["finalFreezeEvidence"]["path"]),
        expected_final_freeze_evidence_sha256=pins["finalFreezeEvidence"]["sha256"],
        backup_manifest=str(pins["backupManifest"]["path"]),
        expected_backup_manifest_sha256=pins["backupManifest"]["sha256"],
        restore_evidence=str(pins["restoreEvidence"]["path"]),
        expected_restore_evidence_sha256=pins["restoreEvidence"]["sha256"],
        migration_evidence=str(pins["migrationEvidence"]["path"]),
        expected_migration_evidence_sha256=pins["migrationEvidence"]["sha256"],
    )
    try:
        bound = closure.validate_bound_inputs(
            namespace,
            freeze_plan,
            closure.regular_directory(raw["bundleRoot"], "final backup bundle root"),
            require_automatic_restore_arm=False,
        )
    except closure.ClosureError as exc:
        raise SupervisorError(f"pre-reset artifact binding failed: {exc}") from exc
    require(bound.frozen_block == finalized_block(raw["frozenFinalizedBlock"], "supervisor frozen block"), "supervisor frozen block mismatch")
    return bound


def validate_plan(path: Path, expected_sha256: str, *, full_artifacts: bool = True) -> dict[str, Any]:
    path = closure.regular_file(path, "automatic-restore supervisor plan")
    expected = ensure_sha256(expected_sha256, "expected supervisor plan SHA-256")
    require(sha256_file(path) == expected, "supervisor plan hash mismatch")
    raw = read_json(path, "automatic-restore supervisor plan")
    require(set(raw) == PLAN_KEYS, "supervisor plan schema mismatch")
    require(raw.get("schemaVersion") == 1, "supervisor plan version mismatch")
    require(raw.get("kind") == "nexus-v2-private-alpha-pre-reset-rollback-supervisor-plan", "supervisor plan kind mismatch")
    operation_id = ensure_id(raw.get("operationId"), "supervisor operation ID")
    release_id = ensure_id(raw.get("releaseId"), "supervisor release ID")
    site_release_version = ensure_site_release(
        raw.get("siteReleaseVersion"), "supervisor site release version"
    )
    require(
        site_release_version != release_id,
        "chain release ID and site release version must remain distinct",
    )
    source_commit = ensure_commit(raw.get("sourceCommit"), "supervisor source commit")
    backend = raw.get("backend")
    require(backend in {PRODUCTION_BACKEND, FIXTURE_BACKEND}, "unsupported supervisor backend")
    fixture_root, fixture_contract = validate_fixture_root(raw.get("fixtureRoot"), backend)
    fixture_only = backend == FIXTURE_BACKEND
    created = parse_utc(raw.get("createdAtUtc"), "supervisor plan creation time")
    expires = parse_utc(raw.get("expiresAtUtc"), "supervisor plan expiry")
    require(created < expires and expires - created <= dt.timedelta(hours=1), "supervisor plan validity exceeds one hour")
    frozen = finalized_block(raw.get("frozenFinalizedBlock"), "supervisor frozen block")
    require(raw.get("policy") == POLICY, "supervisor policy mismatch")
    sources = validate_sources(raw.get("sources"))
    require(sources["chain"]["commit"] == source_commit, "chain source does not match supervisor source")

    supervisor_pin = raw.get("supervisor")
    require(isinstance(supervisor_pin, dict) and set(supervisor_pin) == SUPERVISOR_KEYS, "supervisor executable pin mismatch")
    normalized_supervisor = executable_pin(supervisor_pin, sources, "supervisor executable")
    require(normalized_supervisor["path"] == Path(__file__).resolve(), "plan does not pin the executing supervisor")

    workflow = raw.get("workflow")
    require(isinstance(workflow, dict) and set(workflow) == WORKFLOW_KEYS, "workflow contract mismatch")
    normalized_workflow = {
        "driver": executable_pin(workflow.get("driver"), sources, "replacement workflow driver"),
        "helperPins": pin_map(
            workflow.get("helperPins"),
            sources,
            WORKFLOW_HELPER_ROLES,
            "replacement workflow helpers",
        ),
    }
    require_pin_identity(
        normalized_workflow["driver"],
        EXPECTED_WORKFLOW_DRIVER,
        "replacement workflow driver",
    )
    for role, pin in normalized_workflow["helperPins"].items():
        require_pin_identity(
            pin,
            EXPECTED_WORKFLOW_HELPERS[role],
            f"replacement workflow helper {role}",
        )
    contract_pin = workflow.get("contract")
    require(
        isinstance(contract_pin, dict) and set(contract_pin) == PIN_KEYS,
        "replacement workflow contract pin mismatch",
    )
    contract_path = closure.regular_file(
        contract_pin.get("path"), "replacement workflow contract"
    )
    contract_sha256 = ensure_sha256(
        contract_pin.get("sha256"), "replacement workflow contract SHA-256"
    )
    require(
        sha256_file(contract_path) == contract_sha256,
        "replacement workflow contract hash drifted",
    )
    contract = read_json(contract_path, "replacement workflow contract")
    require(
        set(contract) == WORKFLOW_CONTRACT_KEYS,
        "replacement workflow contract schema mismatch",
    )
    require(
        contract.get("schemaVersion") == 1
        and contract.get("kind")
        == "nexus-v2-private-alpha-replacement-workflow-contract",
        "replacement workflow contract kind mismatch",
    )
    for field, expected_value in (
        ("operationId", operation_id),
        ("releaseId", release_id),
        ("siteReleaseVersion", site_release_version),
        ("sourceCommit", source_commit),
        ("frozenFinalizedBlock", frozen),
    ):
        require(
            contract.get(field) == expected_value,
            f"replacement workflow contract {field} mismatch",
        )
    require(
        contract.get("stageOrder")
        == [
            "createPreResetClosure",
            "deployChainMediaAuthority",
            "deploySiteIndexer",
            "closeIngressAndObserve",
            "createZeroAssetAcceptanceFence",
        ],
        "replacement workflow stage order mismatch",
    )
    stage_inputs = contract.get("stageInputs")
    require(
        isinstance(stage_inputs, dict)
        and set(stage_inputs) == set(WORKFLOW_STAGE_INPUT_KEYS),
        "replacement workflow stage input set mismatch",
    )
    for stage, expected_keys in WORKFLOW_STAGE_INPUT_KEYS.items():
        value = stage_inputs[stage]
        require(
            isinstance(value, dict) and set(value) == expected_keys,
            f"replacement workflow {stage} inputs mismatch",
        )
    zero_fence_inputs = stage_inputs["createZeroAssetAcceptanceFence"]
    for field, expected_path in EXPECTED_ZERO_FENCE_SITE_PATHS.items():
        require(
            zero_fence_inputs.get(field) == expected_path,
            f"replacement workflow {field} is not the reviewed site executable",
        )
    require(
        contract.get("fixtureOnly") is fixture_only,
        "replacement workflow fixture mode mismatch",
    )
    require(
        contract.get("acceptanceStartFencePath")
        == raw.get("acceptanceStartFence", {}).get("handoffPath"),
        "replacement workflow acceptance-start fence path mismatch",
    )
    require(
        contract.get("bootstrapOrAcceptanceWritesAllowed") is False
        and contract.get("paidOrPublicActivationAllowed") is False,
        "replacement workflow contract authorizes forbidden writes",
    )
    artifact_bindings = contract.get("artifactSha256")
    raw_artifacts = raw.get("artifacts")
    require(
        isinstance(artifact_bindings, dict)
        and isinstance(raw_artifacts, dict)
        and artifact_bindings
        == {
            name: pin.get("sha256")
            for name, pin in sorted(raw_artifacts.items())
            if isinstance(pin, dict)
        },
        "replacement workflow artifact bindings mismatch",
    )
    normalized_workflow["toolPins"] = pin_map(
        contract.get("toolPins"),
        sources,
        WORKFLOW_TOOL_ROLES,
        "replacement workflow nested tools",
    )
    for role, pin in normalized_workflow["toolPins"].items():
        require_pin_identity(
            pin,
            EXPECTED_WORKFLOW_TOOLS[role],
            f"replacement workflow nested tool {role}",
        )
    normalized_workflow["contract"] = {
        "path": contract_path,
        "sha256": contract_sha256,
    }

    acceptance = raw.get("acceptanceStartFence")
    require(
        isinstance(acceptance, dict)
        and set(acceptance) == ACCEPTANCE_START_FENCE_KEYS,
        "zero-asset acceptance-start fence contract mismatch",
    )
    handoff_path = Path(str(acceptance.get("handoffPath")))
    closure.require_normal_absolute(handoff_path, "zero-asset acceptance-start fence")
    closure.require_no_symlink_components(
        handoff_path, "zero-asset acceptance-start fence", include_leaf=False
    )
    require(
        not os.path.lexists(handoff_path),
        "zero-asset acceptance-start fence already exists before supervision",
    )
    poll = acceptance.get("pollMilliseconds")
    require(
        isinstance(poll, int)
        and not isinstance(poll, bool)
        and 10 <= poll <= 1000,
        "acceptance-start fence poll interval must be 10..1000 ms",
    )
    normalized_acceptance = {
        "handoffPath": handoff_path,
        "verifier": executable_pin(acceptance.get("verifier"), sources, "acceptance verifier"),
        "genesisHash": acceptance.get("genesisHash"),
        "runtimeCodeSha256": ensure_sha256(acceptance.get("runtimeCodeSha256"), "acceptance runtime code SHA-256"),
        "runtimeMetadataScaleSha256": ensure_sha256(acceptance.get("runtimeMetadataScaleSha256"), "acceptance metadata SHA-256"),
        "pollMilliseconds": poll,
    }
    require_pin_identity(
        normalized_acceptance["verifier"],
        EXPECTED_WORKFLOW_TOOLS["acceptanceBoundary"],
        "acceptance-start verifier",
    )
    require(isinstance(normalized_acceptance["genesisHash"], str) and HASH256_RE.fullmatch(normalized_acceptance["genesisHash"]) is not None, "invalid acceptance genesis hash")

    pins = validate_artifact_pins(raw.get("artifacts"))
    require(
        pins["finalFreezePlan"]["sha256"]
        != pins["finalFreezeEvidence"]["sha256"],
        "freeze plan/evidence pins are aliased",
    )
    components = raw.get("components")
    require(isinstance(components, dict) and set(components) == set(COMPONENTS), "recovery components mismatch")
    normalized_components: dict[str, dict[str, Any]] = {}
    for component_id in COMPONENTS:
        component = components[component_id]
        require(isinstance(component, dict) and set(component) == COMPONENT_KEYS, f"{component_id} schema mismatch")
        archives = component.get("requiredResetArchives")
        require(isinstance(archives, dict) and set(archives) == COMPONENT_ARCHIVES[component_id], f"{component_id} archive roles mismatch")
        readiness_hash = pins["resetReadiness"]["sha256"]
        for archive_id, archive_path in archives.items():
            require(isinstance(archive_path, str) and archive_path.startswith("/"), f"{component_id}:{archive_id} archive path must be absolute")
            require(readiness_hash in archive_path, f"{component_id}:{archive_id} archive is not readiness-bound")
        normalized_components[component_id] = {
            "driver": executable_pin(component.get("driver"), sources, f"{component_id} driver"),
            "helperPins": pin_map(component.get("helperPins"), sources, {"hostAction"}, f"{component_id} helpers"),
            "scriptPins": pin_map(component.get("scriptPins"), sources, COMPONENT_SCRIPT_ROLES[component_id], f"{component_id} scripts"),
            "requiredResetArchives": dict(archives),
        }
        require_pin_identity(
            normalized_components[component_id]["driver"],
            EXPECTED_COMPONENT_DRIVERS[component_id],
            f"{component_id} driver",
        )
        for role, pin in normalized_components[component_id]["helperPins"].items():
            require_pin_identity(
                pin,
                EXPECTED_COMPONENT_HELPERS[component_id][role],
                f"{component_id} helper {role}",
            )
        for role, pin in normalized_components[component_id]["scriptPins"].items():
            require_pin_identity(
                pin,
                EXPECTED_COMPONENT_SCRIPTS[component_id][role],
                f"{component_id} script {role}",
            )

    bound = None
    if full_artifacts:
        bound = validate_full_artifact_binding(raw, pins, fixture_only, fixture_contract)
    if fixture_only:
        for source_id in SOURCE_IDS:
            require(sources[source_id]["commit"] == source_commit, "fixture sources must use one synthetic commit")

    return {
        "path": path,
        "sha256": expected,
        "raw": raw,
        "operationId": operation_id,
        "releaseId": release_id,
        "siteReleaseVersion": site_release_version,
        "sourceCommit": source_commit,
        "backend": backend,
        "fixtureOnly": fixture_only,
        "fixtureRoot": fixture_root,
        "fixtureContract": fixture_contract,
        "createdAt": created,
        "expiresAt": expires,
        "frozenFinalizedBlock": frozen,
        "sources": sources,
        "supervisor": normalized_supervisor,
        "workflow": normalized_workflow,
        "acceptanceStartFence": normalized_acceptance,
        "components": normalized_components,
        "artifacts": pins,
        "boundInputs": bound,
    }


def process_start_token(pid: int) -> str:
    require(isinstance(pid, int) and not isinstance(pid, bool) and pid > 0, "invalid supervisor PID")
    if platform.system() == "Darwin":
        class ProcBsdInfo(ctypes.Structure):
            _fields_ = [
                ("pbi_flags", ctypes.c_uint32),
                ("pbi_status", ctypes.c_uint32),
                ("pbi_xstatus", ctypes.c_uint32),
                ("pbi_pid", ctypes.c_uint32),
                ("pbi_ppid", ctypes.c_uint32),
                ("pbi_uid", ctypes.c_uint32),
                ("pbi_gid", ctypes.c_uint32),
                ("pbi_ruid", ctypes.c_uint32),
                ("pbi_rgid", ctypes.c_uint32),
                ("pbi_svuid", ctypes.c_uint32),
                ("pbi_svgid", ctypes.c_uint32),
                ("rfu_1", ctypes.c_uint32),
                ("pbi_comm", ctypes.c_char * 16),
                ("pbi_name", ctypes.c_char * 32),
                ("pbi_nfiles", ctypes.c_uint32),
                ("pbi_pgid", ctypes.c_uint32),
                ("pbi_pjobc", ctypes.c_uint32),
                ("e_tdev", ctypes.c_uint32),
                ("e_tpgid", ctypes.c_uint32),
                ("pbi_nice", ctypes.c_int32),
                ("pbi_start_tvsec", ctypes.c_uint64),
                ("pbi_start_tvusec", ctypes.c_uint64),
            ]

        try:
            library = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
            proc_pidinfo = library.proc_pidinfo
            proc_pidinfo.argtypes = [
                ctypes.c_int,
                ctypes.c_int,
                ctypes.c_uint64,
                ctypes.c_void_p,
                ctypes.c_int,
            ]
            proc_pidinfo.restype = ctypes.c_int
            info = ProcBsdInfo()
            copied = proc_pidinfo(pid, 3, 0, ctypes.byref(info), ctypes.sizeof(info))
            require(
                copied == ctypes.sizeof(info) and info.pbi_pid == pid,
                "supervisor process is not live",
            )
            identity = f"darwin\0{pid}\0{info.pbi_start_tvsec}\0{info.pbi_start_tvusec}"
            return hashlib.sha256(identity.encode("ascii")).hexdigest()
        except OSError as exc:
            raise SupervisorError("cannot inspect supervisor process identity") from exc
    if platform.system() == "Linux":
        try:
            stat_value = Path(f"/proc/{pid}/stat").read_text(encoding="ascii")
            closing = stat_value.rfind(")")
            require(closing > 0, "supervisor process stat is malformed")
            fields = stat_value[closing + 2 :].split()
            require(len(fields) > 19, "supervisor process stat is incomplete")
            boot_id_path = Path("/proc/sys/kernel/random/boot_id")
            boot_id = (
                boot_id_path.read_text(encoding="ascii").strip()
                if boot_id_path.is_file()
                else "unknown-boot"
            )
            identity = f"linux\0{pid}\0{fields[19]}\0{boot_id}"
            return hashlib.sha256(identity.encode("ascii")).hexdigest()
        except (OSError, UnicodeError) as exc:
            raise SupervisorError("supervisor process is not live") from exc
    completed = subprocess.run(
        ["ps", "-o", "lstart=", "-p", str(pid)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    require(completed.returncode == 0 and completed.stdout.strip(), "supervisor process is not live")
    return hashlib.sha256(f"{pid}\0{completed.stdout.strip()}".encode("utf-8")).hexdigest()


def validate_lease(
    path: Path,
    arm: Mapping[str, Any],
    *,
    require_active: bool,
    require_live: bool,
) -> dict[str, Any]:
    path = closure.regular_file(path, "automatic-restore supervisor lease")
    require(stat.S_IMODE(path.stat().st_mode) & 0o077 == 0, "supervisor lease is not owner-only")
    value = read_json(path, "automatic-restore supervisor lease")
    require(set(value) == LEASE_KEYS, "supervisor lease schema mismatch")
    require(value.get("schemaVersion") == 1 and value.get("kind") == "nexus-v2-private-alpha-automatic-restore-lease", "supervisor lease kind mismatch")
    require(value.get("operationId") == arm.get("operationId"), "supervisor lease operation mismatch")
    require(value.get("releaseId") == arm.get("releaseId"), "supervisor lease release mismatch")
    require(
        value.get("siteReleaseVersion") == arm.get("siteReleaseVersion"),
        "supervisor lease site release mismatch",
    )
    require(value.get("sourceCommit") == arm.get("sourceCommit"), "supervisor lease source mismatch")
    require(value.get("pid") == arm.get("pid"), "supervisor lease PID mismatch")
    require(value.get("processStartToken") == arm.get("processStartToken"), "supervisor lease start token mismatch")
    nonce = value.get("nonce")
    require(
        isinstance(nonce, str)
        and len(nonce) == 32
        and re.fullmatch(r"[0-9a-f]{32}", nonce) is not None,
        "supervisor lease nonce is invalid",
    )
    require(
        hashlib.sha256(nonce.encode("ascii")).hexdigest()
        == arm.get("leaseNonceSha256"),
        "supervisor lease nonce binding mismatch",
    )
    created = parse_utc(value.get("createdAtUtc"), "supervisor lease creation time")
    expires = parse_utc(value.get("expiresAtUtc"), "supervisor lease expiry")
    require(
        created == parse_utc(arm.get("issuedAtUtc"), "automatic-restore arm issue time")
        and expires == parse_utc(arm.get("expiresAtUtc"), "automatic-restore arm expiry"),
        "supervisor lease validity does not match the immutable arm",
    )
    state = value.get("state")
    require(state in {"active", "retired"}, "supervisor lease state is invalid")
    if state == "active":
        require(value.get("retiredAtUtc") is None, "active supervisor lease has a retirement time")
        require(
            value.get("retirementEvidenceSha256") is None,
            "active supervisor lease has retirement evidence",
        )
    else:
        retired = parse_utc(value.get("retiredAtUtc"), "supervisor lease retirement time")
        require(created <= retired <= expires, "supervisor lease retirement time is invalid")
        ensure_sha256(
            value.get("retirementEvidenceSha256"),
            "supervisor retirement evidence SHA-256",
        )
    if require_active:
        require(state == "active", "supervisor lease is retired")
    if require_live:
        require(
            process_start_token(value["pid"]) == value["processStartToken"],
            "supervisor PID was reused or exited",
        )
    return value


def validate_arm(
    path: Path,
    expected_sha256: str,
    *,
    expected_release_id: str,
    expected_site_release_version: str | None = None,
    expected_source_commit: str,
    expected_frozen_block: Mapping[str, Any] | None = None,
    full_binding: bool,
    allow_fixture: bool = False,
    now: dt.datetime | None = None,
    max_issue_age_seconds: int | None = None,
    expected_lease_state: str = "active",
) -> dict[str, Any]:
    path = closure.regular_file(path, "automatic-restore arm")
    expected = ensure_sha256(expected_sha256, "automatic-restore arm SHA-256")
    require(sha256_file(path) == expected, "automatic-restore arm hash mismatch")
    require(stat.S_IMODE(path.stat().st_mode) & 0o077 == 0, "automatic-restore arm is not owner-only")
    value = read_json(path, "automatic-restore arm")
    require(set(value) == ARM_KEYS, "automatic-restore arm schema mismatch")
    require(value.get("schemaVersion") == 1 and value.get("kind") == "nexus-v2-private-alpha-automatic-restore-arm", "automatic-restore arm kind mismatch")
    require(value.get("releaseId") == ensure_id(expected_release_id, "expected arm release ID"), "automatic-restore arm release mismatch")
    site_release_version = ensure_site_release(
        value.get("siteReleaseVersion"), "automatic-restore arm site release version"
    )
    if expected_site_release_version is not None:
        require(
            site_release_version
            == ensure_site_release(
                expected_site_release_version, "expected arm site release version"
            ),
            "automatic-restore arm site release mismatch",
        )
    require(value.get("sourceCommit") == ensure_commit(expected_source_commit, "expected arm source commit"), "automatic-restore arm source mismatch")
    frozen = finalized_block(value.get("frozenFinalizedBlock"), "automatic-restore arm frozen block")
    if expected_frozen_block is not None:
        require(frozen == finalized_block(expected_frozen_block, "expected automatic-restore frozen block"), "automatic-restore arm frozen block mismatch")
    issued = parse_utc(value.get("issuedAtUtc"), "automatic-restore arm issue time")
    expires = parse_utc(value.get("expiresAtUtc"), "automatic-restore arm expiry")
    require(
        issued < expires and expires - issued <= dt.timedelta(seconds=3600),
        "automatic-restore arm lease exceeds 3600 seconds",
    )
    current = now or dt.datetime.now(dt.timezone.utc)
    require(current.tzinfo is not None, "arm verification clock lacks a timezone")
    current = current.astimezone(dt.timezone.utc)
    require(
        expected_lease_state in {"active", "retired"},
        "expected supervisor lease state is invalid",
    )
    require(
        issued - dt.timedelta(seconds=5) <= current,
        "automatic-restore arm is future-dated",
    )
    if expected_lease_state == "active":
        require(current <= expires, "automatic-restore arm is stale")
    if max_issue_age_seconds is not None:
        require(
            isinstance(max_issue_age_seconds, int)
            and not isinstance(max_issue_age_seconds, bool)
            and 0 <= max_issue_age_seconds <= 300,
            "arm issue-age limit must be in 0..300 seconds",
        )
        require(
            (current - issued).total_seconds() <= max_issue_age_seconds,
            "automatic-restore arm was not created within the required 300-second window",
        )
    require(value.get("handlersInstalled") is True, "automatic-restore handlers were not installed")
    require(value.get("automaticRestoreArmed") is True, "automatic restore is not armed")
    require(value.get("paidOrPublicActivationAllowed") is False, "automatic-restore arm permits paid/public activation")
    require(value.get("fixtureOnly") is False or allow_fixture, "NONDEPLOYABLE fixture arm cannot authorize a replacement")
    for field in (
        "planSha256",
        "supervisorSha256",
        "workflowDriverSha256",
        "replacementLockSha256",
        "resetReadinessSha256",
        "finalFreezeEvidenceSha256",
        "backupManifestSha256",
        "restoreEvidenceSha256",
        "migrationEvidenceSha256",
        "leaseNonceSha256",
    ):
        ensure_sha256(value.get(field), f"automatic-restore arm {field}")
    drivers = value.get("componentDriverSha256")
    require(isinstance(drivers, dict) and set(drivers) == set(COMPONENTS), "arm component drivers mismatch")
    for component_id, digest in drivers.items():
        ensure_sha256(digest, f"arm {component_id} driver SHA-256")
    preparation = value.get("archivePreparationResults")
    require(
        isinstance(preparation, dict) and set(preparation) == set(COMPONENTS),
        "arm archive preparation results mismatch",
    )
    for component_id, pin in preparation.items():
        require(
            isinstance(pin, dict) and set(pin) == PREFLIGHT_PIN_KEYS,
            f"arm {component_id} archive preparation pin mismatch",
        )
        result_path = closure.regular_file(
            pin.get("path"), f"{component_id} archive preparation result"
        )
        require(
            sha256_file(result_path)
            == ensure_sha256(
                pin.get("sha256"),
                f"{component_id} archive preparation SHA-256",
            ),
            f"{component_id} archive preparation result drifted",
        )
    preflight = value.get("preflightResults")
    require(isinstance(preflight, dict) and set(preflight) == set(COMPONENTS), "arm preflight results mismatch")
    for component_id, pin in preflight.items():
        require(isinstance(pin, dict) and set(pin) == PREFLIGHT_PIN_KEYS, f"arm {component_id} preflight pin mismatch")
        result_path = closure.regular_file(pin.get("path"), f"{component_id} preflight result")
        require(sha256_file(result_path) == ensure_sha256(pin.get("sha256"), f"{component_id} preflight SHA-256"), f"{component_id} preflight result drifted")
    plan_path = closure.regular_file(value.get("planPath"), "automatic-restore supervisor plan")
    require(sha256_file(plan_path) == value["planSha256"], "arm supervisor plan hash drifted")
    lease_path = closure.regular_file(value.get("leasePath"), "automatic-restore supervisor lease")
    lease = validate_lease(
        lease_path,
        value,
        require_active=expected_lease_state == "active",
        require_live=expected_lease_state == "active",
    )
    require(
        lease["state"] == expected_lease_state,
        "supervisor lease state does not match the requested validation mode",
    )

    if full_binding:
        plan = validate_plan(plan_path, value["planSha256"], full_artifacts=True)
        require(plan["releaseId"] == value["releaseId"], "arm plan release mismatch")
        require(
            plan["siteReleaseVersion"] == value["siteReleaseVersion"],
            "arm plan site release mismatch",
        )
        require(plan["sourceCommit"] == value["sourceCommit"], "arm plan source mismatch")
        require(plan["frozenFinalizedBlock"] == frozen, "arm plan frozen block mismatch")
        require(plan["supervisor"]["sha256"] == value["supervisorSha256"], "arm supervisor hash mismatch")
        require(plan["workflow"]["driver"]["sha256"] == value["workflowDriverSha256"], "arm workflow hash mismatch")
        for component_id in COMPONENTS:
            require(plan["components"][component_id]["driver"]["sha256"] == drivers[component_id], f"arm {component_id} driver mismatch")
            prepared = validate_component_result(
                read_json(
                    Path(preparation[component_id]["path"]),
                    f"{component_id} archive preparation result",
                ),
                plan,
                component_id,
                "prepare",
                ARCHIVE_PREPARATION_ACTION,
            )
            require(
                prepared["result"] == "passed",
                f"{component_id} archive preparation did not pass",
            )
            result = validate_component_result(
                read_json(Path(preflight[component_id]["path"]), f"{component_id} preflight result"),
                plan,
                component_id,
                "preflight",
                "preflight",
            )
            require(result["result"] == "passed", f"{component_id} preflight did not pass")
        artifact_field = {
            "replacementLock": "replacementLockSha256",
            "resetReadiness": "resetReadinessSha256",
            "finalFreezeEvidence": "finalFreezeEvidenceSha256",
            "backupManifest": "backupManifestSha256",
            "restoreEvidence": "restoreEvidenceSha256",
            "migrationEvidence": "migrationEvidenceSha256",
        }
        for artifact_id, field in artifact_field.items():
            require(plan["artifacts"][artifact_id]["sha256"] == value[field], f"arm {artifact_id} binding mismatch")
    return value


def install_handlers() -> dict[int, Any]:
    originals: dict[int, Any] = {}

    def handler(signum: int, _frame: Any) -> None:
        RUNTIME.signal_number = signum
        process = RUNTIME.active_process
        if process is not None and process.poll() is None:
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except OSError:
                try:
                    process.terminate()
                except OSError:
                    pass
        raise SupervisorSignal(signum)

    for signum in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        originals[signum] = signal.getsignal(signum)
        signal.signal(signum, handler)
    RUNTIME.handlers_installed = True
    return originals


def restore_handlers(originals: Mapping[int, Any]) -> None:
    for signum, handler in originals.items():
        signal.signal(signum, handler)
    RUNTIME.handlers_installed = False


def child_environment(plan: Mapping[str, Any]) -> dict[str, str]:
    environment = os.environ.copy()
    environment["PYTHONDONTWRITEBYTECODE"] = "1"
    environment["NEXUS_V2_PRE_RESET_CHAIN_RELEASE_ID"] = plan["releaseId"]
    environment["NEXUS_V2_PRE_RESET_SITE_RELEASE_VERSION"] = plan[
        "siteReleaseVersion"
    ]
    environment["NEXUS_V2_PRE_RESET_SOURCE_COMMIT"] = plan["sourceCommit"]
    environment["NEXUS_V2_PRE_RESET_PLAN_SHA256"] = plan["sha256"]
    immutable_sources = plan.get("immutableSources", {})
    require(isinstance(immutable_sources, Mapping), "immutable source map is invalid")
    for source_id, root in immutable_sources.items():
        require(source_id in SOURCE_IDS, "immutable source map is not closed")
        environment[
            f"NEXUS_V2_PRE_RESET_IMMUTABLE_{source_id.upper()}_ROOT"
        ] = str(root)
    if plan["fixtureOnly"]:
        environment.pop("NEXUS_V2_PRE_RESET_PRODUCTION_CONFIRMATION", None)
        environment["NEXUS_V2_PRE_RESET_FIXTURE_ROOT"] = str(plan["fixtureRoot"])
    else:
        require(
            environment.get("NEXUS_V2_PRE_RESET_PRODUCTION_CONFIRMATION")
            == PRODUCTION_CONFIRMATION,
            "production supervisor requires explicit PRIVATE_ALPHA_ROLLBACK_ONLY confirmation",
        )
        environment.pop("NEXUS_V2_PRE_RESET_FIXTURE_ROOT", None)
    return environment


def run_child(
    command: Sequence[str],
    environment: Mapping[str, str],
    log_path: Path,
    *,
    health_check: Callable[[], None] | None = None,
) -> int:
    closure.output_path(log_path, "supervisor child log")
    with log_path.open("xb") as log:
        os.chmod(log_path, 0o600)
        process = subprocess.Popen(
            list(command),
            env=dict(environment),
            stdout=log,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        RUNTIME.active_process = process
        try:
            while True:
                returncode = process.poll()
                if returncode is not None:
                    return returncode
                if RUNTIME.signal_number is not None:
                    raise SupervisorSignal(RUNTIME.signal_number)
                if health_check is not None:
                    try:
                        health_check()
                    except BaseException:
                        try:
                            os.killpg(process.pid, signal.SIGTERM)
                        except OSError:
                            try:
                                process.terminate()
                            except OSError:
                                pass
                        try:
                            process.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            try:
                                os.killpg(process.pid, signal.SIGKILL)
                            except OSError:
                                try:
                                    process.kill()
                                except OSError:
                                    pass
                            process.wait()
                        raise
                time.sleep(0.25)
        finally:
            if process.poll() is None:
                try:
                    os.killpg(process.pid, signal.SIGTERM)
                except OSError:
                    try:
                        process.terminate()
                    except OSError:
                        pass
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except OSError:
                        try:
                            process.kill()
                        except OSError:
                            pass
                    process.wait()
            else:
                process.wait()
            RUNTIME.active_process = None
            log.flush()
            os.fsync(log.fileno())


def validate_component_result(
    value: Mapping[str, Any],
    plan: Mapping[str, Any],
    component_id: str,
    mode: str,
    action: str,
) -> dict[str, Any]:
    require(set(value) == COMPONENT_RESULT_KEYS, f"{component_id}:{action}:{mode} result schema mismatch")
    require(value.get("schemaVersion") == 1 and value.get("kind") == "nexus-v2-private-alpha-pre-reset-recovery-result", "component result kind mismatch")
    require(value.get("operationId") == plan["operationId"], "component result operation mismatch")
    require(value.get("releaseId") == plan["releaseId"], "component result chain release mismatch")
    require(
        value.get("siteReleaseVersion") == plan["siteReleaseVersion"],
        "component result site release mismatch",
    )
    require(value.get("planSha256") == plan["sha256"], "component result plan mismatch")
    require(value.get("componentId") == component_id, "component result identity mismatch")
    require(value.get("mode") == mode and value.get("action") == action, "component result action mismatch")
    require(value.get("fixtureOnly") is plan["fixtureOnly"], "component result fixture mode mismatch")
    require(value.get("result") == "passed", f"component action did not pass: {component_id}:{action}:{mode}")
    parse_utc(value.get("completedAtUtc"), "component completion time")
    checks = value.get("checks")
    if mode == "prepare":
        require(
            action == ARCHIVE_PREPARATION_ACTION,
            "component preparation action mismatch",
        )
        expected_checks = ARCHIVE_PREPARATION_CHECKS
    elif mode == "preflight":
        expected_checks = PREFLIGHT_CHECKS
    else:
        expected_checks = ACTION_CHECKS[action]
    require(isinstance(checks, dict) and set(checks) == expected_checks, "component result checks mismatch")
    require(all(item is True for item in checks.values()), "component result has a failed check")
    require(value.get("requiredResetArchivesPresent") is True, "component reset archives are absent")
    if mode == "prepare":
        require(
            isinstance(value.get("mutationPerformed"), bool),
            "component archive preparation mutation flag is invalid",
        )
        require(value.get("credentialsResolvable") is True, "component credentials are unavailable")
        require(
            value.get("failedV2RootArchiveSha256") is None,
            "component archive preparation reports a failed-root archive",
        )
    elif mode == "preflight":
        require(value.get("mutationPerformed") is False, "component preflight mutated state")
        require(value.get("credentialsResolvable") is True, "component credentials are unavailable")
        require(value.get("failedV2RootArchiveSha256") is None, "component preflight reports a failed-root archive")
    else:
        require(value.get("mutationPerformed") is True, "component execute result reports no mutation")
        if action == "pause-v2-writes":
            require(value.get("failedV2RootArchiveSha256") is None, "pause result reports an archive")
        else:
            ensure_sha256(value.get("failedV2RootArchiveSha256"), "failed V2 root archive SHA-256")
    return dict(value)


def invoke_component(
    plan: Mapping[str, Any],
    state_root: Path,
    component_id: str,
    mode: str,
    action: str,
) -> tuple[dict[str, Any], Path]:
    attempt = state_root / "attempts" / f"{mode}.{action}.{component_id}"
    require(not os.path.lexists(attempt), f"component attempt already exists: {attempt}")
    attempt.mkdir(parents=True, mode=0o700)
    result_path = attempt / "result.json"
    log_path = attempt / "driver.log"
    driver = plan["components"][component_id]["driver"]["path"]
    command = [
        str(driver),
        "--plan",
        str(plan["path"]),
        "--plan-sha256",
        plan["sha256"],
        "--component",
        component_id,
        "--mode",
        mode,
        "--action",
        action,
        "--result",
        str(result_path),
    ]
    returncode = run_child(command, child_environment(plan), log_path)
    require(returncode == 0, f"component driver failed: {component_id}:{action}:{mode}; see {log_path}")
    result = read_json(result_path, f"{component_id}:{action}:{mode} result")
    return validate_component_result(result, plan, component_id, mode, action), result_path


def validate_workflow_result(value: Mapping[str, Any], plan: Mapping[str, Any]) -> dict[str, Any]:
    require(set(value) == WORKFLOW_RESULT_KEYS, "replacement workflow result schema mismatch")
    require(value.get("schemaVersion") == 1 and value.get("kind") == "nexus-v2-private-alpha-replacement-workflow-result", "replacement workflow result kind mismatch")
    require(value.get("operationId") == plan["operationId"], "replacement workflow operation mismatch")
    require(value.get("releaseId") == plan["releaseId"], "replacement workflow chain release mismatch")
    require(
        value.get("siteReleaseVersion") == plan["siteReleaseVersion"],
        "replacement workflow site release mismatch",
    )
    require(value.get("planSha256") == plan["sha256"], "replacement workflow plan mismatch")
    require(value.get("fixtureOnly") is plan["fixtureOnly"], "replacement workflow fixture mismatch")
    require(value.get("result") in {"passed", "failed"}, "replacement workflow result is invalid")
    require(isinstance(value.get("mutationPerformed"), bool), "replacement workflow mutation flag is invalid")
    require(
        isinstance(value.get("acceptanceStartFenceWritten"), bool),
        "replacement workflow acceptance-start fence flag is invalid",
    )
    parse_utc(value.get("completedAtUtc"), "replacement workflow completion time")
    return dict(value)


def invoke_workflow(
    plan: Mapping[str, Any],
    state_root: Path,
    arm_path: Path,
    arm_sha256: str,
) -> tuple[dict[str, Any], Path]:
    attempt = state_root / "workflow"
    require(not os.path.lexists(attempt), "replacement workflow attempt already exists")
    attempt.mkdir(mode=0o700)
    result_path = attempt / "result.json"
    log_path = attempt / "driver.log"
    command = [
        str(plan["workflow"]["driver"]["path"]),
        "--plan",
        str(plan["path"]),
        "--plan-sha256",
        plan["sha256"],
        "--workflow-contract",
        str(plan["workflow"]["contract"]["path"]),
        "--workflow-contract-sha256",
        plan["workflow"]["contract"]["sha256"],
        "--automatic-restore-arm",
        str(arm_path),
        "--automatic-restore-arm-sha256",
        arm_sha256,
        "--result",
        str(result_path),
    ]
    def health_check() -> None:
        verify_immutable_plan(plan)
        validate_arm(
            arm_path,
            arm_sha256,
            expected_release_id=plan["releaseId"],
            expected_site_release_version=plan["siteReleaseVersion"],
            expected_source_commit=plan["sourceCommit"],
            expected_frozen_block=plan["frozenFinalizedBlock"],
            full_binding=False,
            allow_fixture=plan["fixtureOnly"],
        )

    returncode = run_child(
        command,
        child_environment(plan),
        log_path,
        health_check=health_check,
    )
    # A short-lived child can exit between polling ticks.  Re-run the complete
    # immutable/arm check synchronously before accepting its result so the
    # final sub-250ms window cannot bypass the live rollback guarantee.
    health_check()
    require(returncode == 0, f"replacement workflow driver failed; see {log_path}")
    result = validate_workflow_result(read_json(result_path, "replacement workflow result"), plan)
    require(result["result"] == "passed", "replacement workflow reported failure")
    require(
        result["acceptanceStartFenceWritten"] is True,
        "replacement workflow did not write the zero-asset acceptance-start fence",
    )
    return result, result_path


def verify_acceptance_start_fence(
    plan: Mapping[str, Any],
    state_root: Path,
    arm_path: Path,
    arm_sha256: str,
) -> tuple[Path, str]:
    """Verify the zero-asset receipt that fences the first bootstrap write.

    ``acceptance_boundary.py verify-receipt`` rejects any receipt that was not
    derived from closed ingress, a keep-v2 coordinator decision, successful
    Phase-1 smoke, and zero current/lifetime acceptance inventory.  The
    supervisor remains armed while this verifier runs and retires restoration
    immediately afterward.  This is not a post-assets acceptance receipt.
    """

    acceptance = plan["acceptanceStartFence"]
    path = acceptance["handoffPath"]
    verify_immutable_plan(plan)
    validate_arm(
        arm_path,
        arm_sha256,
        expected_release_id=plan["releaseId"],
        expected_site_release_version=plan["siteReleaseVersion"],
        expected_source_commit=plan["sourceCommit"],
        expected_frozen_block=plan["frozenFinalizedBlock"],
        full_binding=False,
        allow_fixture=plan["fixtureOnly"],
    )
    while not os.path.lexists(path):
        if RUNTIME.signal_number is not None:
            raise SupervisorSignal(RUNTIME.signal_number)
        if dt.datetime.now(dt.timezone.utc) >= plan["expiresAt"]:
            raise SupervisorError(
                "zero-asset acceptance-start fence did not arrive before supervisor expiry"
            )
        validate_arm(
            arm_path,
            arm_sha256,
            expected_release_id=plan["releaseId"],
            expected_site_release_version=plan["siteReleaseVersion"],
            expected_source_commit=plan["sourceCommit"],
            expected_frozen_block=plan["frozenFinalizedBlock"],
            full_binding=False,
            allow_fixture=plan["fixtureOnly"],
        )
        time.sleep(acceptance["pollMilliseconds"] / 1000)
    path = closure.regular_file(path, "zero-asset acceptance-start fence")
    digest = sha256_file(path)
    log_path = state_root / "acceptance-verifier.log"
    command = [
        str(acceptance["verifier"]["path"]),
        "verify-receipt",
        "--receipt",
        str(path),
        "--expected-sha256",
        digest,
        "--release-id",
        plan["releaseId"],
        "--source-commit",
        plan["sourceCommit"],
        "--genesis-hash",
        acceptance["genesisHash"],
        "--runtime-code-sha256",
        acceptance["runtimeCodeSha256"],
        "--runtime-metadata-scale-sha256",
        acceptance["runtimeMetadataScaleSha256"],
    ]
    def health_check() -> None:
        verify_immutable_plan(plan)
        validate_arm(
            arm_path,
            arm_sha256,
            expected_release_id=plan["releaseId"],
            expected_site_release_version=plan["siteReleaseVersion"],
            expected_source_commit=plan["sourceCommit"],
            expected_frozen_block=plan["frozenFinalizedBlock"],
            full_binding=False,
            allow_fixture=plan["fixtureOnly"],
        )

    returncode = run_child(
        command,
        child_environment(plan),
        log_path,
        health_check=health_check,
    )
    health_check()
    require(
        returncode == 0,
        f"zero-asset acceptance-start fence verification failed; see {log_path}",
    )
    require(
        sha256_file(path) == digest,
        "zero-asset acceptance-start fence changed during verification",
    )
    return path, digest


def component_preflights(plan: Mapping[str, Any], state_root: Path) -> dict[str, tuple[dict[str, Any], Path]]:
    results: dict[str, tuple[dict[str, Any], Path]] = {}
    for component_id in COMPONENTS:
        results[component_id] = invoke_component(
            plan,
            state_root,
            component_id,
            "preflight",
            "preflight",
        )
    return results


def prepare_component_archives(
    plan: Mapping[str, Any],
    state_root: Path,
    results: dict[str, tuple[dict[str, Any], Path]],
) -> None:
    """Create readiness-bound rollback archives before the restore arm exists.

    This phase may only copy and seal the current Alpha state. It is purposely
    outside the armed destructive window: component preflight cannot truthfully
    assert archive availability until this preparation has succeeded.
    """

    for component_id in COMPONENTS:
        results[component_id] = invoke_component(
            plan,
            state_root,
            component_id,
            "prepare",
            ARCHIVE_PREPARATION_ACTION,
        )


def emit_arm(
    plan: Mapping[str, Any],
    arm_path: Path,
    lease_path: Path,
    archive_preparation: Mapping[str, tuple[Mapping[str, Any], Path]],
    preflight: Mapping[str, tuple[Mapping[str, Any], Path]],
) -> tuple[dict[str, Any], str]:
    require(RUNTIME.handlers_installed, "signal/exception handlers were not installed before arming")
    pid = os.getpid()
    start_token = process_start_token(pid)
    issued = dt.datetime.now(dt.timezone.utc).replace(microsecond=0)
    expires = min(issued + dt.timedelta(seconds=3600), plan["expiresAt"])
    require(
        issued < expires and expires - issued <= dt.timedelta(seconds=3600),
        "cannot issue a live <=3600 second automatic-restore arm",
    )
    nonce = secrets.token_hex(16)
    lease = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-automatic-restore-lease",
        "operationId": plan["operationId"],
        "releaseId": plan["releaseId"],
        "siteReleaseVersion": plan["siteReleaseVersion"],
        "sourceCommit": plan["sourceCommit"],
        "pid": pid,
        "processStartToken": start_token,
        "nonce": nonce,
        "createdAtUtc": format_utc(issued),
        "expiresAtUtc": format_utc(expires),
        "state": "active",
        "retiredAtUtc": None,
        "retirementEvidenceSha256": None,
    }
    write_new(lease_path, lease, mode=0o600)
    artifacts = plan["artifacts"]
    arm = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-automatic-restore-arm",
        "operationId": plan["operationId"],
        "releaseId": plan["releaseId"],
        "siteReleaseVersion": plan["siteReleaseVersion"],
        "sourceCommit": plan["sourceCommit"],
        "planPath": str(plan["path"]),
        "planSha256": plan["sha256"],
        "supervisorSha256": plan["supervisor"]["sha256"],
        "workflowDriverSha256": plan["workflow"]["driver"]["sha256"],
        "componentDriverSha256": {
            component_id: plan["components"][component_id]["driver"]["sha256"]
            for component_id in COMPONENTS
        },
        "archivePreparationResults": {
            component_id: {
                "path": str(archive_preparation[component_id][1]),
                "sha256": sha256_file(archive_preparation[component_id][1]),
            }
            for component_id in COMPONENTS
        },
        "preflightResults": {
            component_id: {
                "path": str(preflight[component_id][1]),
                "sha256": sha256_file(preflight[component_id][1]),
            }
            for component_id in COMPONENTS
        },
        "frozenFinalizedBlock": plan["frozenFinalizedBlock"],
        "replacementLockSha256": artifacts["replacementLock"]["sha256"],
        "resetReadinessSha256": artifacts["resetReadiness"]["sha256"],
        "finalFreezeEvidenceSha256": artifacts["finalFreezeEvidence"]["sha256"],
        "backupManifestSha256": artifacts["backupManifest"]["sha256"],
        "restoreEvidenceSha256": artifacts["restoreEvidence"]["sha256"],
        "migrationEvidenceSha256": artifacts["migrationEvidence"]["sha256"],
        "pid": pid,
        "processStartToken": start_token,
        "leasePath": str(lease_path),
        "leaseNonceSha256": hashlib.sha256(nonce.encode("ascii")).hexdigest(),
        "handlersInstalled": True,
        "issuedAtUtc": format_utc(issued),
        "expiresAtUtc": format_utc(expires),
        "fixtureOnly": plan["fixtureOnly"],
        "automaticRestoreArmed": True,
        "paidOrPublicActivationAllowed": False,
    }
    write_new(arm_path, arm, mode=0o600)
    arm_sha256 = sha256_file(arm_path)
    validate_arm(
        arm_path,
        arm_sha256,
        expected_release_id=plan["releaseId"],
        expected_site_release_version=plan["siteReleaseVersion"],
        expected_source_commit=plan["sourceCommit"],
        expected_frozen_block=plan["frozenFinalizedBlock"],
        full_binding=True,
        allow_fixture=plan["fixtureOnly"],
    )
    return arm, arm_sha256


def recover(
    plan: Mapping[str, Any],
    state_root: Path,
) -> tuple[dict[str, Any], bool, bool]:
    recovery: dict[str, Any] = {}
    archive_hashes: dict[str, str] = {}
    all_passed = True
    preserved = True
    for action in RECOVERY_ACTIONS:
        recovery[action] = {}
        for component_id in COMPONENTS:
            result_path: Path | None = None
            try:
                result, result_path = invoke_component(
                    plan,
                    state_root,
                    component_id,
                    "execute",
                    action,
                )
                archive_hash = result.get("failedV2RootArchiveSha256")
                if action == "archive-failed-v2":
                    archive_hashes[component_id] = archive_hash
                elif action in {"restore-final-backup", "restored-smoke"}:
                    require(
                        archive_hashes.get(component_id) == archive_hash,
                        f"{component_id} changed the preserved failed V2 root identity",
                    )
                recovery[action][component_id] = {
                    "status": "passed",
                    "resultPath": str(result_path),
                    "resultSha256": sha256_file(result_path),
                    "error": None,
                }
            except Exception as exc:  # recovery must continue across both lanes
                all_passed = False
                if action in {"archive-failed-v2", "restore-final-backup", "restored-smoke"}:
                    preserved = False
                recovery[action][component_id] = {
                    "status": "failed",
                    "resultPath": str(result_path) if result_path is not None else None,
                    "resultSha256": (
                        sha256_file(result_path)
                        if result_path is not None and result_path.is_file()
                        else None
                    ),
                    "error": str(exc),
                }
    return recovery, all_passed, preserved and set(archive_hashes) == set(COMPONENTS)


def evidence_value(
    plan: Mapping[str, Any],
    *,
    outcome: str,
    trigger: Mapping[str, Any] | None,
    arm_sha256: str | None,
    archive_preparation_result_sha256: Mapping[str, str],
    workflow_result_sha256: str | None,
    acceptance_sha256: str | None,
    recovery: Mapping[str, Any],
    automatic_restore_performed: bool,
    automatic_restore_retired: bool,
    failed_roots_preserved: bool,
    completed_at: str | None = None,
) -> dict[str, Any]:
    require(
        set(archive_preparation_result_sha256) <= set(COMPONENTS),
        "archive preparation evidence component set mismatch",
    )
    for component_id, digest in archive_preparation_result_sha256.items():
        ensure_sha256(digest, f"{component_id} archive preparation evidence SHA-256")
    evidence = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-pre-reset-supervisor-evidence",
        "operationId": plan["operationId"],
        "planSha256": plan["sha256"],
        "releaseId": plan["releaseId"],
        "siteReleaseVersion": plan["siteReleaseVersion"],
        "sourceCommit": plan["sourceCommit"],
        "backend": plan["backend"],
        "fixtureOnly": plan["fixtureOnly"],
        "outcome": outcome,
        "trigger": dict(trigger) if trigger is not None else None,
        "automaticRestoreArmSha256": arm_sha256,
        "archivePreparationResultSha256": dict(
            archive_preparation_result_sha256
        ),
        "workflowResultSha256": workflow_result_sha256,
        "acceptanceStartFenceSha256": acceptance_sha256,
        "recovery": dict(recovery),
        "automaticRestorePerformed": automatic_restore_performed,
        "automaticRestoreRetired": automatic_restore_retired,
        "failedV2RootsPreserved": failed_roots_preserved,
        "paidOrPublicActivationAllowed": False,
        "completedAtUtc": completed_at or utc_now(),
    }
    require(set(evidence) == EVIDENCE_KEYS, "supervisor evidence schema mismatch")
    return evidence


def write_evidence(path: Path, value: Mapping[str, Any]) -> None:
    require(set(value) == EVIDENCE_KEYS, "supervisor evidence schema mismatch")
    write_new(path, value, mode=0o400)


def replace_mutable_lease(path: Path, value: Mapping[str, Any]) -> None:
    """Atomically replace only the explicitly mutable supervisor lease."""

    closure.regular_file(path, "automatic-restore supervisor lease")
    require(stat.S_IMODE(path.stat().st_mode) & 0o077 == 0, "supervisor lease is not owner-only")
    temporary = path.with_name(f".{path.name}.retire-{secrets.token_hex(8)}")
    try:
        write_new(temporary, value, mode=0o600)
        os.replace(temporary, path)
        descriptor = os.open(path.parent, os.O_RDONLY)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)
    finally:
        if os.path.lexists(temporary):
            temporary.unlink()


def retire_automatic_restore(
    lease_path: Path,
    arm: Mapping[str, Any],
    retirement_evidence_sha256: str,
    retired_at: str,
) -> None:
    lease = validate_lease(
        lease_path,
        arm,
        require_active=True,
        require_live=True,
    )
    retired = dict(lease)
    retired["state"] = "retired"
    retired["retiredAtUtc"] = retired_at
    retired["retirementEvidenceSha256"] = ensure_sha256(
        retirement_evidence_sha256, "retirement evidence SHA-256"
    )
    replace_mutable_lease(lease_path, retired)
    verified = validate_lease(
        lease_path,
        arm,
        require_active=False,
        require_live=False,
    )
    require(verified["state"] == "retired", "automatic restore lease did not retire")
    require(
        verified["retirementEvidenceSha256"] == retirement_evidence_sha256,
        "automatic restore retirement evidence binding drifted",
    )


def validate_retirement_evidence(
    evidence_path: Path,
    expected_evidence_sha256: str,
    arm_path: Path,
    expected_arm_sha256: str,
    *,
    expected_release_id: str,
    expected_site_release_version: str,
    expected_source_commit: str,
    allow_fixture: bool = False,
) -> dict[str, Any]:
    evidence_path = closure.regular_file(
        evidence_path, "automatic-restore retirement evidence"
    )
    require(
        sha256_file(evidence_path)
        == ensure_sha256(
            expected_evidence_sha256, "expected retirement evidence SHA-256"
        ),
        "automatic-restore retirement evidence hash mismatch",
    )
    require(
        stat.S_IMODE(evidence_path.stat().st_mode) & 0o077 == 0,
        "automatic-restore retirement evidence is not owner-only",
    )
    evidence = read_json(evidence_path, "automatic-restore retirement evidence")
    require(set(evidence) == EVIDENCE_KEYS, "retirement evidence schema mismatch")
    require(
        evidence.get("schemaVersion") == 1
        and evidence.get("kind")
        == "nexus-v2-private-alpha-pre-reset-supervisor-evidence",
        "retirement evidence kind mismatch",
    )
    require(
        evidence.get("outcome") == "zero-asset-acceptance-start-fence-verified",
        "retirement evidence is not the zero-asset acceptance-start fence",
    )
    require(
        evidence.get("releaseId") == expected_release_id
        and evidence.get("siteReleaseVersion") == expected_site_release_version
        and evidence.get("sourceCommit") == expected_source_commit,
        "retirement evidence identity mismatch",
    )
    ensure_sha256(
        evidence.get("acceptanceStartFenceSha256"),
        "zero-asset acceptance-start fence SHA-256",
    )
    require(
        evidence.get("automaticRestorePerformed") is False
        and evidence.get("automaticRestoreRetired") is True
        and evidence.get("paidOrPublicActivationAllowed") is False,
        "retirement evidence does not permanently disable automatic restore",
    )
    require(evidence.get("recovery") == {}, "retirement evidence includes recovery actions")
    preparation_hashes = evidence.get("archivePreparationResultSha256")
    require(
        isinstance(preparation_hashes, dict)
        and set(preparation_hashes) == set(COMPONENTS),
        "retirement evidence lacks both pre-arm archive preparations",
    )
    for component_id, digest in preparation_hashes.items():
        ensure_sha256(digest, f"{component_id} retirement preparation SHA-256")
    arm = validate_arm(
        arm_path,
        expected_arm_sha256,
        expected_release_id=expected_release_id,
        expected_site_release_version=expected_site_release_version,
        expected_source_commit=expected_source_commit,
        full_binding=False,
        allow_fixture=allow_fixture,
        expected_lease_state="retired",
    )
    require(
        evidence.get("automaticRestoreArmSha256") == expected_arm_sha256,
        "retirement evidence arm hash mismatch",
    )
    lease = validate_lease(
        Path(arm["leasePath"]), arm, require_active=False, require_live=False
    )
    require(
        lease["retirementEvidenceSha256"] == expected_evidence_sha256,
        "retired lease does not bind the exact retirement evidence",
    )
    return evidence


def prepare_outputs(state_root: Path, arm: Path, lease: Path, evidence: Path) -> None:
    for path, label in ((arm, "arm"), (lease, "lease"), (evidence, "evidence")):
        closure.output_path(path, f"supervisor {label}")
    closure.create_private_directory(state_root, "supervisor state root")
    (state_root / "attempts").mkdir(mode=0o700)


def run_supervisor(args: argparse.Namespace) -> int:
    expected_plan_sha256 = ensure_sha256(args.expected_plan_sha256, "expected plan SHA-256")
    plan = validate_plan(Path(args.plan), expected_plan_sha256, full_artifacts=True)
    now = dt.datetime.now(dt.timezone.utc)
    require(plan["createdAt"] <= now < plan["expiresAt"], "supervisor plan is stale or future-dated")
    state_root = Path(args.state_root)
    arm_path = Path(args.arm)
    lease_path = Path(args.lease)
    evidence_path = Path(args.evidence)
    prepare_outputs(state_root, arm_path, lease_path, evidence_path)
    require(RUNTIME.active_process is None, "another supervised child is already active")
    RUNTIME.signal_number = None
    originals = install_handlers()
    arm_sha256: str | None = None
    arm_value: dict[str, Any] | None = None
    workflow_result_sha256: str | None = None
    fence_verified = False
    retired = False
    retirement_evidence: dict[str, Any] | None = None
    retirement_evidence_sha256: str | None = None
    archive_preparation: dict[str, tuple[dict[str, Any], Path]] = {}
    archive_preparation_hashes: dict[str, str] = {}
    execution_plan = plan
    try:
        execution_plan = prepare_immutable_plan(plan, state_root)
        prepare_component_archives(
            execution_plan, state_root, archive_preparation
        )
        archive_preparation_hashes = {
            component_id: sha256_file(result_path)
            for component_id, (_, result_path) in archive_preparation.items()
        }
        preflight = component_preflights(execution_plan, state_root)
        # Revalidate every source, executable, helper, script, and transitive
        # artifact after child preflight and before publishing the arm.
        revalidated = validate_plan(plan["path"], plan["sha256"], full_artifacts=True)
        require(revalidated["frozenFinalizedBlock"] == plan["frozenFinalizedBlock"], "supervisor inputs drifted during preflight")
        verify_immutable_plan(execution_plan)
        arm_value, arm_sha256 = emit_arm(
            execution_plan,
            arm_path,
            lease_path,
            archive_preparation,
            preflight,
        )
        workflow, workflow_path = invoke_workflow(
            execution_plan, state_root, arm_path, arm_sha256
        )
        workflow_result_sha256 = sha256_file(workflow_path)
        require(workflow.get("mutationPerformed") is True, "replacement workflow performed no replacement mutation")
        _, acceptance_sha256 = verify_acceptance_start_fence(
            execution_plan, state_root, arm_path, arm_sha256
        )
        fence_verified = True
        completed_at = utc_now()
        retirement_evidence = evidence_value(
            execution_plan,
            outcome="zero-asset-acceptance-start-fence-verified",
            trigger=None,
            arm_sha256=arm_sha256,
            archive_preparation_result_sha256=archive_preparation_hashes,
            workflow_result_sha256=workflow_result_sha256,
            acceptance_sha256=acceptance_sha256,
            recovery={},
            automatic_restore_performed=False,
            automatic_restore_retired=True,
            failed_roots_preserved=True,
            completed_at=completed_at,
        )
        retirement_evidence_sha256 = hashlib.sha256(
            canonical_bytes(retirement_evidence)
        ).hexdigest()

        # Keep the immutable arm byte-for-byte.  Only the explicitly mutable
        # lease transitions to retired.  Signals are deferred across the tiny
        # lease/evidence publication critical section so the first bootstrap
        # write can never observe a receipt without its retirement fence.
        blocked = {signal.SIGINT, signal.SIGTERM, signal.SIGHUP}
        old_mask = (
            signal.pthread_sigmask(signal.SIG_BLOCK, blocked)
            if hasattr(signal, "pthread_sigmask")
            else None
        )
        try:
            require(arm_value is not None, "automatic-restore arm disappeared")
            retire_automatic_restore(
                lease_path,
                arm_value,
                retirement_evidence_sha256,
                completed_at,
            )
            write_evidence(evidence_path, retirement_evidence)
            retired = True
        finally:
            if old_mask is not None:
                signal.pthread_sigmask(signal.SIG_SETMASK, old_mask)
        return 0
    except BaseException as exc:
        trigger = {
            "type": "signal" if isinstance(exc, SupervisorSignal) else "exception",
            "signal": exc.signum if isinstance(exc, SupervisorSignal) else None,
            "message": str(exc),
        }
        if fence_verified:
            # The verified receipt is the zero-current/zero-lifetime
            # acceptance-start fence.  Bootstrap must additionally require
            # the retirement evidence.  Once this fence is verified, never
            # restore V1: doing so could race the first acceptance write.
            if (
                not retired
                and arm_value is not None
                and retirement_evidence is not None
                and retirement_evidence_sha256 is not None
            ):
                try:
                    lease = read_json(lease_path, "automatic-restore supervisor lease")
                    if lease.get("state") == "active":
                        retire_automatic_restore(
                            lease_path,
                            arm_value,
                            retirement_evidence_sha256,
                            retirement_evidence["completedAtUtc"],
                        )
                    if not os.path.lexists(evidence_path):
                        write_evidence(evidence_path, retirement_evidence)
                    validate_retirement_evidence(
                        evidence_path,
                        retirement_evidence_sha256,
                        arm_path,
                        arm_sha256,
                        expected_release_id=plan["releaseId"],
                        expected_site_release_version=plan["siteReleaseVersion"],
                        expected_source_commit=plan["sourceCommit"],
                        allow_fixture=plan["fixtureOnly"],
                    )
                    retired = True
                except Exception:
                    return 5
            return 0 if retired else 5
        if arm_sha256 is None:
            write_evidence(evidence_path, evidence_value(
                plan,
                outcome="pre-arm-archive-preparation-or-preflight-failed",
                trigger=trigger,
                arm_sha256=None,
                archive_preparation_result_sha256={
                    component_id: sha256_file(result_path)
                    for component_id, (_, result_path) in archive_preparation.items()
                    if result_path.is_file()
                },
                workflow_result_sha256=workflow_result_sha256,
                acceptance_sha256=None,
                recovery={},
                automatic_restore_performed=False,
                automatic_restore_retired=False,
                failed_roots_preserved=True,
            ))
            return 2
        # The triggering child process group has already been terminated by
        # the signal handler.  Clear the one-shot marker so the same foreground
        # supervisor can run the mandatory recovery drivers; a later signal is
        # still handled and recorded independently.
        RUNTIME.signal_number = None
        recovery, recovered, preserved = recover(execution_plan, state_root)
        write_evidence(evidence_path, evidence_value(
            plan,
            outcome=(
                "automatic-recovery-complete"
                if recovered and preserved
                else "automatic-recovery-failed"
            ),
            trigger=trigger,
            arm_sha256=arm_sha256,
            archive_preparation_result_sha256=archive_preparation_hashes,
            workflow_result_sha256=workflow_result_sha256,
            acceptance_sha256=None,
            recovery=recovery,
            automatic_restore_performed=True,
            automatic_restore_retired=False,
            failed_roots_preserved=preserved,
        ))
        return 3 if recovered and preserved else 4
    finally:
        restore_handlers(originals)


def command_validate(args: argparse.Namespace) -> int:
    validate_plan(Path(args.plan), args.expected_plan_sha256, full_artifacts=True)
    print("pre-reset rollback supervisor plan verified without child or host action")
    return 0


def command_verify_arm(args: argparse.Namespace) -> int:
    value = validate_arm(
        Path(args.arm),
        args.expected_sha256,
        expected_release_id=args.release_id,
        expected_site_release_version=args.site_release_version,
        expected_source_commit=args.source_commit,
        full_binding=args.full_binding,
        allow_fixture=args.allow_nondeployable_fixture,
    )
    print(
        json.dumps(
            {
                "kind": value["kind"],
                "releaseId": value["releaseId"],
                "siteReleaseVersion": value["siteReleaseVersion"],
                "sourceCommit": value["sourceCommit"],
                "pid": value["pid"],
                "expiresAtUtc": value["expiresAtUtc"],
                "sha256": args.expected_sha256,
            },
            sort_keys=True,
            separators=(",", ":"),
        )
    )
    return 0


def command_verify_retirement(args: argparse.Namespace) -> int:
    value = validate_retirement_evidence(
        Path(args.evidence),
        args.expected_evidence_sha256,
        Path(args.arm),
        args.expected_arm_sha256,
        expected_release_id=args.release_id,
        expected_site_release_version=args.site_release_version,
        expected_source_commit=args.source_commit,
        allow_fixture=args.allow_nondeployable_fixture,
    )
    print(
        json.dumps(
            {
                "kind": value["kind"],
                "releaseId": value["releaseId"],
                "siteReleaseVersion": value["siteReleaseVersion"],
                "sourceCommit": value["sourceCommit"],
                "automaticRestoreRetired": True,
                "sha256": args.expected_evidence_sha256,
            },
            sort_keys=True,
            separators=(",", ":"),
        )
    )
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    validate = commands.add_parser("validate-plan")
    validate.add_argument("--plan", required=True)
    validate.add_argument("--expected-plan-sha256", required=True)
    validate.set_defaults(func=command_validate)
    run = commands.add_parser("run")
    run.add_argument("--plan", required=True)
    run.add_argument("--expected-plan-sha256", required=True)
    run.add_argument("--state-root", required=True)
    run.add_argument("--arm", required=True)
    run.add_argument("--lease", required=True)
    run.add_argument("--evidence", required=True)
    run.set_defaults(func=run_supervisor)
    verify = commands.add_parser("verify-arm")
    verify.add_argument("--arm", required=True)
    verify.add_argument("--expected-sha256", required=True)
    verify.add_argument("--release-id", required=True)
    verify.add_argument("--site-release-version", required=True)
    verify.add_argument("--source-commit", required=True)
    verify.add_argument("--full-binding", action="store_true")
    verify.add_argument("--allow-nondeployable-fixture", action="store_true")
    verify.set_defaults(func=command_verify_arm)
    retirement = commands.add_parser(
        "verify-retirement",
        help="verify the immutable zero-asset fence and retired mutable lease",
    )
    retirement.add_argument("--evidence", required=True)
    retirement.add_argument("--expected-evidence-sha256", required=True)
    retirement.add_argument("--arm", required=True)
    retirement.add_argument("--expected-arm-sha256", required=True)
    retirement.add_argument("--release-id", required=True)
    retirement.add_argument("--site-release-version", required=True)
    retirement.add_argument("--source-commit", required=True)
    retirement.add_argument("--allow-nondeployable-fixture", action="store_true")
    retirement.set_defaults(func=command_verify_retirement)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        return int(args.func(args))
    except (OSError, SupervisorError, closure.ClosureError) as exc:
        print(f"pre_reset_rollback_supervisor: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
