#!/usr/bin/env python3
"""Create the zero-asset private-alpha acceptance-start fence.

This is the final, non-chain-mutating stage of the protected replacement
workflow.  It consumes the exact Phase-1 closed-ingress output, composes and
executes the pinned post-cutover coordinator, and creates the canonical
acceptance-boundary receipt only after the replacement remains empty and its
read-only smoke checks pass.  The receipt permanently disables archive
restoration before any bootstrap or acceptance write is allowed.
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

import deployment_secret_environment  # noqa: F401


STAGE = "createZeroAssetAcceptanceFence"
PRIOR_STAGE = "closeIngressAndObserve"
PRODUCTION_BACKEND = "protected-private-alpha"
FIXTURE_BACKEND = "fixture-nondeployable"
PRODUCTION_CONFIRMATION = "PRIVATE_ALPHA_ROLLBACK_ONLY"
EXPECTED_PRODUCTION_ARCHIVE_ROOT = (
    "/opt/eterra-alpha/archive/nexus-v2-fresh-reset"
)
EXPECTED_SITE_COORDINATOR_DRIVER = (
    "tcg/deploy/alpha/macmini2014/nexus-v2-rollback-component-driver"
)
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
HASH256_RE = re.compile(r"^0x[0-9a-f]{64}$")
ID_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
SITE_RELEASE_RE = re.compile(
    r"^v(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
    r"(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$"
)
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
TOOL_ROLES = {
    "preResetClosure",
    "chainDeployAll",
    "siteDeploy",
    "phase1IngressClosure",
    "acceptanceBoundary",
    "postCutoverCoordinator",
}
EXPECTED_TOOL_PATHS = {
    "acceptanceBoundary": (
        "chain",
        "scripts/nexus-v2-private-alpha/acceptance_boundary.py",
    ),
    "postCutoverCoordinator": (
        "chain",
        "deploy/alpha/macmini2010/nexus-v2-post-cutover-coordinator.py",
    ),
}
CONTRACT_KEYS = {
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
STAGE_INPUT_KEYS = {
    "runtimeBundleRoot",
    "runtimeBundleManifestSha256",
    "siteDriverPath",
    "siteRestorePath",
    "siteDeployPath",
    "siteStatusPath",
    "resetArchiveRoot",
    "maxObservationAgeSeconds",
}
STAGE_RESULT_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "releaseId",
    "siteReleaseVersion",
    "sourceCommit",
    "planSha256",
    "workflowContractSha256",
    "stage",
    "result",
    "fixtureOnly",
    "mutationPerformed",
    "acceptanceStartFenceWritten",
    "checks",
    "completedAtUtc",
}
REQUIRED_CHECKS = {
    "acceptanceReceiptVerified",
    "closedIngressVerified",
    "coordinatorKeepV2",
    "exactReleaseIdentitiesVerified",
    "externalRecoveryOwnershipVerified",
    "nestedToolsPinned",
    "noBootstrapOrAcceptanceMutation",
    "phase1SmokePassed",
    "receiptPermanentlyDisablesRestore",
    "runtimeIdentityVerified",
    "zeroCurrentAndLifetimeAcceptanceInventory",
}
EXTERNAL_RECOVERY_OWNERSHIP_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "planSha256",
    "releaseId",
    "siteReleaseVersion",
    "sourceCommit",
    "supervisorPath",
    "supervisorSha256",
    "automaticRestoreArmPath",
    "automaticRestoreArmSha256",
    "fixtureOnly",
    "recoveryOwner",
    "nestedRecoveryActionsAllowed",
    "verificationLogPath",
    "verificationLogSha256",
    "verifiedAtUtc",
}
STAGES = (
    "createPreResetClosure",
    "deployChainMediaAuthority",
    "deploySiteIndexer",
    "closeIngressAndObserve",
    "createZeroAssetAcceptanceFence",
)


class FenceError(RuntimeError):
    """The zero-asset acceptance fence could not be proven."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise FenceError(message)


def canonical_bytes(value: Mapping[str, Any]) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def ensure_sha256(value: Any, label: str) -> str:
    require(
        isinstance(value, str) and SHA256_RE.fullmatch(value) is not None,
        f"invalid {label}",
    )
    return value


def ensure_commit(value: Any, label: str) -> str:
    require(
        isinstance(value, str) and COMMIT_RE.fullmatch(value) is not None,
        f"invalid {label}",
    )
    return value


def ensure_id(value: Any, label: str) -> str:
    require(
        isinstance(value, str) and ID_RE.fullmatch(value) is not None,
        f"invalid {label}",
    )
    return value


def ensure_site_release(value: Any) -> str:
    require(
        isinstance(value, str) and SITE_RELEASE_RE.fullmatch(value) is not None,
        "invalid site release version",
    )
    return value


def read_json(path: Path, label: str, *, canonical: bool = True) -> dict[str, Any]:
    require(path.is_file() and not path.is_symlink(), f"{label} is unavailable")
    try:
        payload = path.read_bytes()
        value = json.loads(payload)
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise FenceError(f"invalid {label}") from exc
    require(isinstance(value, dict), f"{label} must be an object")
    if canonical:
        require(payload == canonical_bytes(value), f"{label} must be canonical JSON")
    return value


def regular_file(path_value: Any, label: str) -> Path:
    require(isinstance(path_value, (str, os.PathLike)), f"invalid {label} path")
    path = Path(path_value)
    require(path.is_absolute(), f"{label} path must be absolute")
    require(path == Path(os.path.normpath(os.fspath(path))), f"{label} path is not normalized")
    require(path.is_file() and not path.is_symlink(), f"{label} is unavailable")
    return path.resolve()


def regular_directory(path_value: Any, label: str) -> Path:
    require(isinstance(path_value, (str, os.PathLike)), f"invalid {label} path")
    path = Path(path_value)
    require(path.is_absolute(), f"{label} path must be absolute")
    require(path == Path(os.path.normpath(os.fspath(path))), f"{label} path is not normalized")
    require(path.is_dir() and not path.is_symlink(), f"{label} is unavailable")
    return path.resolve()


def output_path(path_value: Any, label: str) -> Path:
    require(isinstance(path_value, (str, os.PathLike)), f"invalid {label} path")
    path = Path(path_value)
    require(path.is_absolute(), f"{label} path must be absolute")
    require(path == Path(os.path.normpath(os.fspath(path))), f"{label} path is not normalized")
    require(not os.path.lexists(path), f"refusing to overwrite {label}")
    parent = path.parent
    require(parent.is_dir() and not parent.is_symlink(), f"{label} parent is unavailable")
    require(parent.resolve() == parent, f"{label} parent contains a symlink")
    return path


def relative_path(value: Any, label: str) -> str:
    require(isinstance(value, str) and value, f"invalid {label}")
    path = Path(value)
    require(
        not path.is_absolute() and ".." not in path.parts and path.as_posix() == value,
        f"invalid {label}",
    )
    return value


def git_output(root: Path, *arguments: str) -> str:
    completed = subprocess.run(
        ["git", "-C", str(root), *arguments],
        capture_output=True,
        text=True,
        check=False,
    )
    require(completed.returncode == 0, f"cannot inspect immutable source: {root}")
    return completed.stdout.strip()


def immutable_roots(plan: Mapping[str, Any]) -> dict[str, Path]:
    sources = plan.get("sources")
    require(isinstance(sources, dict) and set(sources) == SOURCE_IDS, "source set mismatch")
    roots: dict[str, Path] = {}
    for source_id in sorted(SOURCE_IDS):
        pin = sources[source_id]
        require(
            isinstance(pin, dict) and set(pin) == {"root", "expectedCommit"},
            f"{source_id} source pin mismatch",
        )
        commit = ensure_commit(pin.get("expectedCommit"), f"{source_id} source commit")
        raw = os.environ.get(
            f"NEXUS_V2_PRE_RESET_IMMUTABLE_{source_id.upper()}_ROOT", ""
        )
        root = regular_directory(raw, f"immutable {source_id} source")
        require(
            Path(git_output(root, "rev-parse", "--show-toplevel")).resolve() == root,
            f"immutable {source_id} root mismatch",
        )
        require(git_output(root, "rev-parse", "HEAD") == commit, f"immutable {source_id} HEAD mismatch")
        require(
            git_output(root, "status", "--porcelain", "--untracked-files=all") == "",
            f"immutable {source_id} source is dirty",
        )
        roots[source_id] = root
    return roots


def resolve_tool(
    role: str,
    pin: Any,
    roots: Mapping[str, Path],
) -> dict[str, Any]:
    require(
        isinstance(pin, dict) and set(pin) == {"sourceId", "path", "sha256"},
        f"{role} tool pin mismatch",
    )
    source_id = pin.get("sourceId")
    require(source_id in roots, f"{role} tool source is unavailable")
    relative = relative_path(pin.get("path"), f"{role} tool path")
    path = (roots[source_id] / relative).resolve()
    require(roots[source_id] in path.parents, f"{role} tool escapes immutable source")
    require(
        path.is_file()
        and not path.is_symlink()
        and bool(path.stat().st_mode & stat.S_IXUSR),
        f"{role} tool is not executable",
    )
    digest = ensure_sha256(pin.get("sha256"), f"{role} tool SHA-256")
    require(sha256_file(path) == digest, f"{role} tool hash drifted")
    expected = EXPECTED_TOOL_PATHS.get(role)
    if expected is not None:
        require((source_id, relative) == expected, f"{role} tool identity mismatch")
    return {"sourceId": source_id, "path": path, "sha256": digest, "relative": relative}


def pinned_artifacts(plan: Mapping[str, Any], contract: Mapping[str, Any]) -> dict[str, dict[str, Any]]:
    value = plan.get("artifacts")
    require(isinstance(value, dict) and set(value) == ARTIFACT_IDS, "artifact set mismatch")
    artifacts: dict[str, dict[str, Any]] = {}
    for artifact_id, pin in value.items():
        require(isinstance(pin, dict) and set(pin) == {"path", "sha256"}, f"{artifact_id} pin mismatch")
        path = regular_file(pin.get("path"), f"{artifact_id} artifact")
        digest = ensure_sha256(pin.get("sha256"), f"{artifact_id} artifact SHA-256")
        require(sha256_file(path) == digest, f"{artifact_id} artifact hash drifted")
        artifacts[artifact_id] = {"path": path, "sha256": digest}
    expected_hashes = {name: pin["sha256"] for name, pin in sorted(artifacts.items())}
    require(contract.get("artifactSha256") == expected_hashes, "workflow artifact hash binding mismatch")
    return artifacts


def validate_replacement_lock(
    pin: Mapping[str, Any],
    plan: Mapping[str, Any],
    roots: Mapping[str, Path],
    runtime_root: Path,
    runtime_manifest_sha256: str,
) -> None:
    lock = read_json(pin["path"], "replacement lock")
    require(lock.get("schemaVersion") == 1, "replacement lock schema mismatch")
    require(
        lock.get("kind") == "nexus-v2-private-alpha-pre-cutover-replacement-lock",
        "replacement lock kind mismatch",
    )
    require(lock.get("releaseId") == plan["releaseId"], "replacement lock release mismatch")
    repositories = lock.get("repositories")
    require(isinstance(repositories, dict), "replacement lock repositories are unavailable")
    for source_id in SOURCE_IDS:
        repository_id = "web" if source_id == "site" else source_id
        repository = repositories.get(repository_id)
        require(isinstance(repository, dict), f"replacement lock {repository_id} repository is unavailable")
        require(
            repository.get("head") == git_output(roots[source_id], "rev-parse", "HEAD"),
            f"replacement lock {repository_id} commit mismatch",
        )
    lock_artifacts = lock.get("artifacts")
    require(isinstance(lock_artifacts, dict), "replacement lock artifacts are unavailable")
    manifest_pin = lock_artifacts.get("runtimeBundleManifest")
    require(
        isinstance(manifest_pin, dict) and {"path", "sha256"}.issubset(manifest_pin),
        "replacement lock runtime bundle pin mismatch",
    )
    manifest_path = regular_file(manifest_pin.get("path"), "replacement runtime bundle manifest")
    require(manifest_path.parent == runtime_root, "replacement runtime bundle root mismatch")
    require(
        manifest_pin.get("sha256") == runtime_manifest_sha256
        and sha256_file(manifest_path) == runtime_manifest_sha256,
        "replacement runtime bundle manifest mismatch",
    )


def validate_stage_inputs(
    value: Any,
    plan: Mapping[str, Any],
    roots: Mapping[str, Path],
    artifacts: Mapping[str, Mapping[str, Any]],
) -> dict[str, Any]:
    require(isinstance(value, dict) and set(value) == STAGE_INPUT_KEYS, "zero-fence stage input mismatch")
    runtime_root = regular_directory(value.get("runtimeBundleRoot"), "runtime bundle root")
    runtime_manifest_sha256 = ensure_sha256(
        value.get("runtimeBundleManifestSha256"), "runtime bundle manifest SHA-256"
    )
    runtime_manifest = regular_file(runtime_root / "runtime-bundle-manifest.json", "runtime bundle manifest")
    require(sha256_file(runtime_manifest) == runtime_manifest_sha256, "runtime bundle manifest hash mismatch")
    validate_replacement_lock(
        artifacts["replacementLock"],
        plan,
        roots,
        runtime_root,
        runtime_manifest_sha256,
    )

    site_paths = {
        name: relative_path(value.get(name), name)
        for name in ("siteDriverPath", "siteRestorePath", "siteDeployPath", "siteStatusPath")
    }
    for name, relative in site_paths.items():
        path = (roots["site"] / relative).resolve()
        require(roots["site"] in path.parents, f"{name} escapes immutable site source")
        require(path.is_file() and not path.is_symlink(), f"{name} is unavailable")
        if name in {"siteDriverPath", "siteRestorePath", "siteDeployPath", "siteStatusPath"}:
            require(bool(path.stat().st_mode & stat.S_IXUSR), f"{name} is not executable")
    require(
        site_paths["siteDriverPath"] == EXPECTED_SITE_COORDINATOR_DRIVER,
        "siteDriverPath is not the reviewed rollback component driver",
    )

    components = plan.get("components")
    require(isinstance(components, dict), "recovery component plan is unavailable")
    site_component = components.get("site-indexer")
    require(isinstance(site_component, dict), "site recovery component is unavailable")
    script_pins = site_component.get("scriptPins")
    require(isinstance(script_pins, dict), "site recovery script pins are unavailable")
    for field, role in (
        ("siteRestorePath", "restoreState"),
        ("siteDeployPath", "deploySite"),
        ("siteStatusPath", "status"),
    ):
        pin = script_pins.get(role)
        require(isinstance(pin, dict), f"site {role} pin is unavailable")
        require(pin.get("sourceId") == "site" and pin.get("path") == site_paths[field], f"{field} differs from supervisor pin")
        require(
            sha256_file(roots["site"] / site_paths[field])
            == ensure_sha256(pin.get("sha256"), f"site {role} SHA-256"),
            f"site {role} hash drifted",
        )

    reset_archive_root = value.get("resetArchiveRoot")
    require(isinstance(reset_archive_root, str) and reset_archive_root.startswith("/"), "invalid reset archive root")
    require(reset_archive_root == os.path.normpath(reset_archive_root), "reset archive root is not normalized")
    if plan["backend"] == PRODUCTION_BACKEND:
        require(reset_archive_root == EXPECTED_PRODUCTION_ARCHIVE_ROOT, "production reset archive root mismatch")
    readiness_sha = artifacts["resetReadiness"]["sha256"]
    expected_archives = {
        "chain-media": {
            "node": f"{reset_archive_root}/{readiness_sha}/node",
            "media": f"{reset_archive_root}/{readiness_sha}/media",
        },
        "site-indexer": {"site": f"{reset_archive_root}/{readiness_sha}/site"},
    }
    for component_id, expected in expected_archives.items():
        component = components.get(component_id)
        require(isinstance(component, dict), f"{component_id} recovery component is unavailable")
        require(component.get("requiredResetArchives") == expected, f"{component_id} archive binding mismatch")

    max_age = value.get("maxObservationAgeSeconds")
    require(isinstance(max_age, int) and not isinstance(max_age, bool) and 30 <= max_age <= 900, "max observation age must be in 30..900 seconds")
    return {
        "runtimeBundleRoot": runtime_root,
        "runtimeBundleManifestSha256": runtime_manifest_sha256,
        **site_paths,
        "resetArchiveRoot": reset_archive_root,
        "maxObservationAgeSeconds": max_age,
    }


def validate_arm(
    path: Path,
    expected_sha256: str,
    plan: Mapping[str, Any],
    fixture_only: bool,
) -> dict[str, Any]:
    require(sha256_file(path) == expected_sha256, "automatic-restore arm hash mismatch")
    arm = read_json(path, "automatic-restore arm")
    for field, expected in (
        ("operationId", plan["operationId"]),
        ("releaseId", plan["releaseId"]),
        ("siteReleaseVersion", plan["siteReleaseVersion"]),
        ("sourceCommit", plan["sourceCommit"]),
        ("fixtureOnly", fixture_only),
    ):
        require(arm.get(field) == expected, f"automatic-restore arm {field} mismatch")
    require(arm.get("automaticRestoreArmed") is True, "automatic restore is not armed")
    require(arm.get("paidOrPublicActivationAllowed") is False, "automatic-restore arm permits paid/public activation")
    return arm


def validate_prior_stage(
    workflow_root: Path,
    context: Mapping[str, Any],
) -> dict[str, Any]:
    prior_root = workflow_root / "stages" / PRIOR_STAGE
    require(prior_root.is_dir() and not prior_root.is_symlink(), "prior closure stage is unavailable")
    result_path = regular_file(prior_root / "result.json", "prior closure stage result")
    result = read_json(result_path, "prior closure stage result")
    require(set(result) == STAGE_RESULT_KEYS, "prior closure stage result schema mismatch")
    require(
        result.get("schemaVersion") == 1
        and result.get("kind") == "nexus-v2-private-alpha-replacement-workflow-stage-result",
        "prior closure stage result kind mismatch",
    )
    for field, expected in (
        ("operationId", context["operationId"]),
        ("releaseId", context["releaseId"]),
        ("siteReleaseVersion", context["siteReleaseVersion"]),
        ("sourceCommit", context["sourceCommit"]),
        ("planSha256", context["planSha256"]),
        ("workflowContractSha256", context["contractSha256"]),
        ("stage", PRIOR_STAGE),
        ("fixtureOnly", context["fixtureOnly"]),
        ("result", "passed"),
        ("mutationPerformed", True),
        ("acceptanceStartFenceWritten", False),
    ):
        require(result.get(field) == expected, f"prior closure stage {field} mismatch")
    checks = result.get("checks")
    require(isinstance(checks, dict) and checks and all(value is True for value in checks.values()), "prior closure stage has a failed check")

    phase1_root = prior_root / "phase1-output"
    require(phase1_root.is_dir() and not phase1_root.is_symlink(), "Phase-1 output root is unavailable")
    execute_path = regular_file(phase1_root / "execute-evidence.json", "Phase-1 execute evidence")
    execute = read_json(execute_path, "Phase-1 execute evidence")
    for field, expected in (
        ("operationId", context["operationId"]),
        ("releaseId", context["releaseId"]),
        ("siteReleaseVersion", context["siteReleaseVersion"]),
        ("sourceCommit", context["sourceCommit"]),
        ("automaticRestoreArmPath", str(context["armPath"])),
        ("automaticRestoreArmSha256", context["armSha256"]),
    ):
        require(execute.get(field) == expected, f"Phase-1 execute evidence {field} mismatch")
    for field in (
        "siteCandidateUsableForExecute",
        "allExternalWriteIngressClosed",
        "blockProductionContinues",
        "authorityLocalServicePreserved",
        "readOnlySiteStackPreserved",
    ):
        require(execute.get(field) is True, f"Phase-1 execute evidence must set {field}=true")
    require(execute.get("automaticReopenAuthorized") is False, "Phase-1 execute evidence authorizes reopen")
    require(execute.get("paidOrPublicActivationAuthorized") is False, "Phase-1 execute evidence authorizes paid/public activation")
    return {
        "root": phase1_root.resolve(),
        "executePath": execute_path,
        "executeSha256": sha256_file(execute_path),
        "execute": execute,
    }


def validate_inputs(args: argparse.Namespace) -> dict[str, Any]:
    require(args.stage == STAGE, "zero-fence helper received the wrong stage")
    plan_path = regular_file(args.plan, "supervisor plan")
    plan_sha = ensure_sha256(args.plan_sha256, "plan SHA-256")
    require(sha256_file(plan_path) == plan_sha, "supervisor plan hash mismatch")
    plan = read_json(plan_path, "supervisor plan")
    require(
        plan.get("schemaVersion") == 1
        and plan.get("kind") == "nexus-v2-private-alpha-pre-reset-rollback-supervisor-plan",
        "supervisor plan kind mismatch",
    )
    operation_id = ensure_id(plan.get("operationId"), "operation ID")
    release_id = ensure_id(plan.get("releaseId"), "chain release ID")
    site_release = ensure_site_release(plan.get("siteReleaseVersion"))
    require(site_release != release_id, "chain and site release identities must remain distinct")
    source_commit = ensure_commit(plan.get("sourceCommit"), "source commit")
    backend = plan.get("backend")
    require(backend in {PRODUCTION_BACKEND, FIXTURE_BACKEND}, "unsupported supervisor backend")
    fixture_only = backend == FIXTURE_BACKEND
    if fixture_only:
        require(not os.environ.get("NEXUS_V2_PRE_RESET_PRODUCTION_CONFIRMATION"), "fixture stage carries production confirmation")
    else:
        require(
            os.environ.get("NEXUS_V2_PRE_RESET_PRODUCTION_CONFIRMATION") == PRODUCTION_CONFIRMATION,
            "production zero-fence stage requires PRIVATE_ALPHA_ROLLBACK_ONLY confirmation",
        )

    contract_path = regular_file(args.workflow_contract, "workflow contract")
    contract_sha = ensure_sha256(args.workflow_contract_sha256, "workflow contract SHA-256")
    require(sha256_file(contract_path) == contract_sha, "workflow contract hash mismatch")
    contract = read_json(contract_path, "workflow contract")
    require(set(contract) == CONTRACT_KEYS, "workflow contract schema mismatch")
    require(
        contract.get("schemaVersion") == 1
        and contract.get("kind") == "nexus-v2-private-alpha-replacement-workflow-contract",
        "workflow contract kind mismatch",
    )
    for field, expected in (
        ("operationId", operation_id),
        ("releaseId", release_id),
        ("siteReleaseVersion", site_release),
        ("sourceCommit", source_commit),
        ("frozenFinalizedBlock", plan.get("frozenFinalizedBlock")),
        ("fixtureOnly", fixture_only),
    ):
        require(contract.get(field) == expected, f"workflow contract {field} mismatch")
    require(contract.get("stageOrder") == list(STAGES), "workflow stage order mismatch")
    require(
        contract.get("bootstrapOrAcceptanceWritesAllowed") is False
        and contract.get("paidOrPublicActivationAllowed") is False,
        "workflow contract authorizes forbidden writes",
    )
    acceptance = plan.get("acceptanceStartFence")
    require(isinstance(acceptance, dict), "acceptance-start fence plan is unavailable")
    receipt_path = output_path(contract.get("acceptanceStartFencePath"), "acceptance-start fence")
    require(str(receipt_path) == acceptance.get("handoffPath"), "acceptance-start fence path mismatch")
    genesis_hash = acceptance.get("genesisHash")
    require(isinstance(genesis_hash, str) and HASH256_RE.fullmatch(genesis_hash), "invalid acceptance genesis hash")
    runtime_code_sha256 = ensure_sha256(acceptance.get("runtimeCodeSha256"), "acceptance runtime code SHA-256")
    metadata_sha256 = ensure_sha256(acceptance.get("runtimeMetadataScaleSha256"), "acceptance metadata SHA-256")

    workflow = plan.get("workflow")
    require(isinstance(workflow, dict), "supervisor workflow plan is unavailable")
    require(
        workflow.get("contract") == {"path": str(contract_path), "sha256": contract_sha},
        "workflow contract is not plan-pinned",
    )
    roots = immutable_roots(plan)
    supervisor = resolve_tool(
        "automaticRestoreSupervisor", plan.get("supervisor"), roots
    )
    require(
        supervisor["sourceId"] == "chain"
        and supervisor["relative"]
        == "scripts/nexus-v2-private-alpha/pre_reset_rollback_supervisor.py",
        "automatic-restore supervisor identity mismatch",
    )
    tool_pins = contract.get("toolPins")
    require(isinstance(tool_pins, dict) and set(tool_pins) == TOOL_ROLES, "workflow tool pin set mismatch")
    tools = {role: resolve_tool(role, pin, roots) for role, pin in tool_pins.items()}
    plan_tool_pins = workflow.get("toolPins")
    if plan_tool_pins is not None:
        require(isinstance(plan_tool_pins, dict) and set(plan_tool_pins) == TOOL_ROLES, "supervisor workflow tool pins mismatch")
        require(
            {
                role: {
                    "sourceId": pin.get("sourceId"),
                    "path": pin.get("path"),
                    "sha256": pin.get("sha256"),
                }
                for role, pin in plan_tool_pins.items()
            }
            == tool_pins,
            "workflow contract tools differ from supervisor pins",
        )

    artifacts = pinned_artifacts(plan, contract)
    stage_inputs = contract.get("stageInputs")
    require(isinstance(stage_inputs, dict) and set(stage_inputs) == set(STAGES), "workflow stage input set mismatch")
    inputs = validate_stage_inputs(stage_inputs.get(STAGE), plan, roots, artifacts)

    arm_path = regular_file(args.automatic_restore_arm, "automatic-restore arm")
    arm_sha = ensure_sha256(args.automatic_restore_arm_sha256, "automatic-restore arm SHA-256")
    validate_arm(arm_path, arm_sha, plan, fixture_only)

    workflow_root = regular_directory(args.workflow_state_root, "workflow state root")
    stage_root = regular_directory(args.stage_state_root, "zero-fence stage state root")
    require(stage_root == workflow_root / "stages" / STAGE, "zero-fence stage state root mismatch")
    result_path = output_path(args.result, "zero-fence stage result")
    require(result_path == stage_root / "result.json", "zero-fence stage result path mismatch")
    context = {
        "plan": plan,
        "planPath": plan_path,
        "planSha256": plan_sha,
        "contract": contract,
        "contractPath": contract_path,
        "contractSha256": contract_sha,
        "operationId": operation_id,
        "releaseId": release_id,
        "siteReleaseVersion": site_release,
        "sourceCommit": source_commit,
        "fixtureOnly": fixture_only,
        "roots": roots,
        "tools": tools,
        "supervisor": supervisor,
        "artifacts": artifacts,
        "inputs": inputs,
        "armPath": arm_path,
        "armSha256": arm_sha,
        "workflowRoot": workflow_root,
        "stageRoot": stage_root,
        "resultPath": result_path,
        "receiptPath": receipt_path,
        "genesisHash": genesis_hash,
        "runtimeCodeSha256": runtime_code_sha256,
        "metadataSha256": metadata_sha256,
    }
    context["phase1"] = validate_prior_stage(workflow_root, context)
    return context


def run_tool(
    tool: Mapping[str, Any],
    arguments: Sequence[str],
    log_path: Path,
    *,
    environment: Mapping[str, str] | None = None,
) -> None:
    path = regular_file(tool["path"], "nested workflow tool")
    require(sha256_file(path) == tool["sha256"], "nested workflow tool drifted before invocation")
    require(not os.path.lexists(log_path), f"refusing to overwrite nested tool log: {log_path}")
    child_environment = os.environ.copy()
    if environment is not None:
        child_environment.update(environment)
    with log_path.open("xb") as log:
        os.chmod(log_path, 0o600)
        completed = subprocess.run(
            [str(path), *arguments],
            stdout=log,
            stderr=subprocess.STDOUT,
            env=child_environment,
            check=False,
        )
    require(completed.returncode == 0, f"nested workflow tool failed; see {log_path}")


def validate_zero_inventory(path: Path) -> None:
    value = read_json(path, "post-V16 acceptance inventory")
    counts = value.get("counts")
    require(isinstance(counts, dict) and counts, "acceptance inventory counts are unavailable")
    require(
        all(isinstance(item, int) and not isinstance(item, bool) and item == 0 for item in counts.values()),
        "acceptance inventory contains a current or lifetime write",
    )


def run_pipeline(context: Mapping[str, Any]) -> dict[str, bool]:
    root: Path = context["stageRoot"]
    phase1 = context["phase1"]
    inputs = context["inputs"]
    artifacts = context["artifacts"]
    tools = context["tools"]
    phase1_root: Path = phase1["root"]
    observation = root / "post-cutover-observation.json"
    coordinator_plan = root / "post-cutover-coordinator-plan.json"
    coordinator_state = root / "post-cutover-coordinator-state"
    coordinator_evidence = root / "post-cutover-coordinator-evidence.json"

    acceptance_tool = tools["acceptanceBoundary"]
    coordinator_tool = tools["postCutoverCoordinator"]
    run_tool(
        acceptance_tool,
        [
            "compose-observation",
            "--phase1-output-root",
            str(phase1_root),
            "--phase1-execute-evidence-sha256",
            phase1["executeSha256"],
            "--media-source-commit",
            git_output(context["roots"]["media"], "rev-parse", "HEAD"),
            "--output",
            str(observation),
        ],
        root / "compose-observation.log",
    )
    observation_pin = regular_file(observation, "post-cutover observation")
    observation_sha = sha256_file(observation_pin)

    run_tool(
        acceptance_tool,
        [
            "compose-coordinator-plan",
            "--phase1-output-root",
            str(phase1_root),
            "--phase1-execute-evidence-sha256",
            phase1["executeSha256"],
            "--observation",
            str(observation_pin),
            "--observation-sha256",
            observation_sha,
            "--media-source-commit",
            git_output(context["roots"]["media"], "rev-parse", "HEAD"),
            "--operation-id",
            context["operationId"],
            "--chain-root",
            str(context["roots"]["chain"]),
            "--media-root",
            str(context["roots"]["media"]),
            "--site-root",
            str(context["roots"]["site"]),
            "--runtime-bundle-root",
            str(inputs["runtimeBundleRoot"]),
            "--runtime-bundle-manifest-sha256",
            inputs["runtimeBundleManifestSha256"],
            "--fresh-reset-readiness",
            str(artifacts["resetReadiness"]["path"]),
            "--fresh-reset-readiness-sha256",
            artifacts["resetReadiness"]["sha256"],
            "--final-backup-manifest",
            str(artifacts["backupManifest"]["path"]),
            "--restore-evidence",
            str(artifacts["restoreEvidence"]["path"]),
            "--site-driver-path",
            inputs["siteDriverPath"],
            "--site-restore-path",
            inputs["siteRestorePath"],
            "--site-deploy-path",
            inputs["siteDeployPath"],
            "--site-status-path",
            inputs["siteStatusPath"],
            "--reset-archive-root",
            inputs["resetArchiveRoot"],
            "--max-observation-age-seconds",
            str(inputs["maxObservationAgeSeconds"]),
            "--output",
            str(coordinator_plan),
        ],
        root / "compose-coordinator-plan.log",
    )
    coordinator_plan_pin = regular_file(coordinator_plan, "post-cutover coordinator plan")
    coordinator_plan_sha256 = sha256_file(coordinator_plan_pin)

    coordinator_arguments = [
        "--plan",
        str(coordinator_plan_pin),
        "--manifest",
        str(artifacts["backupManifest"]["path"]),
        "--bundle-root",
        str(context["plan"]["bundleRoot"]),
        "--runtime-bundle-root",
        str(inputs["runtimeBundleRoot"]),
        "--restore-evidence",
        str(artifacts["restoreEvidence"]["path"]),
        "--observation",
        str(observation_pin),
        "--acceptance-boundary-capture",
        str(phase1_root / "acceptance-boundary-rpc-capture.json"),
        "--ingress-closed-evidence",
        str(phase1_root / "ingress-closed-evidence.json"),
        "--economic-gates",
        str(phase1_root / "post-v16-economic-gates.json"),
        "--acceptance-inventory",
        str(phase1_root / "post-v16-acceptance-inventory.json"),
        "--state-dir",
        str(coordinator_state),
        "--evidence",
        str(coordinator_evidence),
        "--external-recovery-supervisor",
        str(context["supervisor"]["path"]),
        "--external-recovery-supervisor-sha256",
        context["supervisor"]["sha256"],
        "--automatic-restore-arm",
        str(context["armPath"]),
        "--automatic-restore-arm-sha256",
        context["armSha256"],
        "--site-release-version",
        context["siteReleaseVersion"],
    ]
    if context["fixtureOnly"]:
        coordinator_arguments.append(
            "--allow-nondeployable-external-recovery-fixture"
        )
    coordinator_arguments.append("--execute")
    run_tool(
        coordinator_tool,
        coordinator_arguments,
        root / "execute-post-cutover-coordinator.log",
        environment={
            "NEXUS_V2_ROLLBACK_PLAN_SHA256": coordinator_plan_sha256,
        },
    )
    coordinator_evidence_pin = regular_file(coordinator_evidence, "post-cutover coordinator evidence")
    coordinator = read_json(coordinator_evidence_pin, "post-cutover coordinator evidence")
    require(coordinator.get("decision") == "keep-v2", "post-cutover coordinator did not keep V2")
    require(coordinator.get("postCutoverSmokePassed") is True, "post-cutover smoke did not pass")
    require(coordinator.get("automaticRestorePerformed") is False, "coordinator restored the prior alpha")
    require(coordinator.get("postAcceptanceContainmentPerformed") is False, "coordinator entered containment")
    require(coordinator.get("nonzeroAcceptanceAssets") == {}, "coordinator observed acceptance assets")
    require(coordinator.get("releaseId") == context["releaseId"], "coordinator chain release mismatch")
    require(coordinator.get("sourceCommit") == context["sourceCommit"], "coordinator source mismatch")

    ownership_path = regular_file(
        coordinator_state / "external-recovery-ownership.json",
        "external recovery ownership",
    )
    ownership = read_json(ownership_path, "external recovery ownership")
    require(
        set(ownership) == EXTERNAL_RECOVERY_OWNERSHIP_KEYS,
        "external recovery ownership schema mismatch",
    )
    for field, expected in (
        ("schemaVersion", 1),
        (
            "kind",
            "nexus-v2-private-alpha-post-cutover-external-recovery-ownership",
        ),
        ("operationId", context["operationId"]),
        ("planSha256", coordinator_plan_sha256),
        ("releaseId", context["releaseId"]),
        ("siteReleaseVersion", context["siteReleaseVersion"]),
        ("sourceCommit", context["sourceCommit"]),
        ("supervisorPath", str(context["supervisor"]["path"])),
        ("supervisorSha256", context["supervisor"]["sha256"]),
        ("automaticRestoreArmPath", str(context["armPath"])),
        ("automaticRestoreArmSha256", context["armSha256"]),
        ("fixtureOnly", context["fixtureOnly"]),
        ("recoveryOwner", "pre-reset-rollback-supervisor"),
        ("nestedRecoveryActionsAllowed", False),
    ):
        require(ownership.get(field) == expected, f"external recovery ownership {field} mismatch")
    ownership_log = regular_file(
        ownership.get("verificationLogPath"),
        "external recovery ownership verification log",
    )
    require(
        sha256_file(ownership_log)
        == ensure_sha256(
            ownership.get("verificationLogSha256"),
            "external recovery ownership verification log SHA-256",
        ),
        "external recovery ownership verification log drifted",
    )

    final_marker = regular_file(
        coordinator_state / "final-evidence.marker.json",
        "post-cutover coordinator final marker",
    )
    validate_zero_inventory(phase1_root / "post-v16-acceptance-inventory.json")
    run_tool(
        acceptance_tool,
        [
            "create-receipt",
            "--runtime-bundle-root",
            str(inputs["runtimeBundleRoot"]),
            "--runtime-bundle-manifest-sha256",
            inputs["runtimeBundleManifestSha256"],
            "--release-id",
            context["releaseId"],
            "--source-commit",
            context["sourceCommit"],
            "--genesis-hash",
            context["genesisHash"],
            "--capture",
            str(phase1_root / "acceptance-boundary-rpc-capture.json"),
            "--economic-gates",
            str(phase1_root / "post-v16-economic-gates.json"),
            "--acceptance-inventory",
            str(phase1_root / "post-v16-acceptance-inventory.json"),
            "--observation",
            str(observation_pin),
            "--ingress-closed-evidence",
            str(phase1_root / "ingress-closed-evidence.json"),
            "--ingress-closed-evidence-sha256",
            phase1["execute"]["ingressClosedEvidenceSha256"],
            "--coordinator-evidence",
            str(coordinator_evidence_pin),
            "--coordinator-evidence-sha256",
            sha256_file(coordinator_evidence_pin),
            "--coordinator-final-marker",
            str(final_marker),
            "--coordinator-final-marker-sha256",
            sha256_file(final_marker),
            "--output",
            str(context["receiptPath"]),
        ],
        root / "create-acceptance-receipt.log",
    )
    receipt = regular_file(context["receiptPath"], "acceptance-start fence")
    receipt_sha = sha256_file(receipt)
    run_tool(
        acceptance_tool,
        [
            "verify-receipt",
            "--receipt",
            str(receipt),
            "--expected-sha256",
            receipt_sha,
            "--release-id",
            context["releaseId"],
            "--source-commit",
            context["sourceCommit"],
            "--genesis-hash",
            context["genesisHash"],
            "--runtime-code-sha256",
            context["runtimeCodeSha256"],
            "--runtime-metadata-scale-sha256",
            context["metadataSha256"],
        ],
        root / "verify-acceptance-receipt.log",
    )
    receipt_value = read_json(receipt, "acceptance-start fence")
    require(receipt_value.get("coordinatorDecision") == "keep-v2", "acceptance receipt decision mismatch")
    require(receipt_value.get("phase1SmokePassed") is True, "acceptance receipt smoke mismatch")
    require(receipt_value.get("automaticRestorePermanentlyDisabled") is True, "acceptance receipt does not retire restoration")
    require(receipt_value.get("releaseId") == context["releaseId"], "acceptance receipt release mismatch")
    require(receipt_value.get("sourceCommit") == context["sourceCommit"], "acceptance receipt source mismatch")
    require(receipt_value.get("genesisHash") == context["genesisHash"], "acceptance receipt genesis mismatch")
    require(receipt_value.get("runtimeCodeSha256") == context["runtimeCodeSha256"], "acceptance receipt runtime mismatch")
    require(receipt_value.get("runtimeMetadataScaleSha256") == context["metadataSha256"], "acceptance receipt metadata mismatch")
    return {name: True for name in sorted(REQUIRED_CHECKS)}


def write_result(context: Mapping[str, Any], checks: Mapping[str, bool]) -> None:
    require(set(checks) == REQUIRED_CHECKS and all(checks.values()), "zero-fence checks are incomplete")
    value = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-replacement-workflow-stage-result",
        "operationId": context["operationId"],
        "releaseId": context["releaseId"],
        "siteReleaseVersion": context["siteReleaseVersion"],
        "sourceCommit": context["sourceCommit"],
        "planSha256": context["planSha256"],
        "workflowContractSha256": context["contractSha256"],
        "stage": STAGE,
        "result": "passed",
        "fixtureOnly": context["fixtureOnly"],
        "mutationPerformed": False,
        "acceptanceStartFenceWritten": True,
        "checks": dict(checks),
        "completedAtUtc": dt.datetime.now(dt.timezone.utc)
        .replace(microsecond=0)
        .strftime("%Y-%m-%dT%H:%M:%SZ"),
    }
    require(set(value) == STAGE_RESULT_KEYS, "internal zero-fence stage result drifted")
    path: Path = context["resultPath"]
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o400)
    with os.fdopen(descriptor, "wb") as handle:
        handle.write(canonical_bytes(value))
        handle.flush()
        os.fsync(handle.fileno())
    os.chmod(path, 0o400)


def run(args: argparse.Namespace) -> None:
    context = validate_inputs(args)
    checks = run_pipeline(context)
    write_result(context, checks)


def parser() -> argparse.ArgumentParser:
    value = argparse.ArgumentParser(description=__doc__)
    value.add_argument("--plan", required=True)
    value.add_argument("--plan-sha256", required=True)
    value.add_argument("--workflow-contract", required=True)
    value.add_argument("--workflow-contract-sha256", required=True)
    value.add_argument("--automatic-restore-arm", required=True)
    value.add_argument("--automatic-restore-arm-sha256", required=True)
    value.add_argument("--stage", required=True)
    value.add_argument("--workflow-state-root", required=True)
    value.add_argument("--stage-state-root", required=True)
    value.add_argument("--result", required=True)
    return value


def main(argv: Sequence[str] | None = None) -> int:
    try:
        run(parser().parse_args(argv))
    except (FenceError, OSError, subprocess.SubprocessError) as exc:
        print(f"pre_reset_zero_asset_fence_stage: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
