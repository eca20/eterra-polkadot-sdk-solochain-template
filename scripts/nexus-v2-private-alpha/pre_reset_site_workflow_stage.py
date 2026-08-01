#!/usr/bin/env python3
"""Deploy the closed site/indexer and prove the Phase-1 ingress boundary.

This helper implements exactly two steps in the foreground pre-reset workflow:
``deploySiteIndexer`` and ``closeIngressAndObserve``.  It consumes immutable
source clones, the pre-cutover replacement lock, the live automatic-restore
arm, and prior stage evidence.  Protected execution is allowed only under the
private-alpha rollback confirmation.  The fixture backend is permanently
NONDEPLOYABLE and never invokes either protected deployment tool.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import re
import secrets
import stat
import subprocess
import sys
from pathlib import Path
from typing import Any, Mapping, Sequence

import deployment_secret_environment  # noqa: F401


STAGES = (
    "createPreResetClosure",
    "deployChainMediaAuthority",
    "deploySiteIndexer",
    "closeIngressAndObserve",
    "createZeroAssetAcceptanceFence",
)
SUPPORTED_STAGES = {"deploySiteIndexer", "closeIngressAndObserve"}
PRODUCTION_BACKEND = "protected-private-alpha"
FIXTURE_BACKEND = "fixture-nondeployable"
PRODUCTION_CONFIRMATION = "PRIVATE_ALPHA_ROLLBACK_ONLY"
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
INTERNAL_PATHS = {
    "chainDeploymentLibrary": (
        "chain",
        "deploy/alpha/macmini2010/lib.sh",
        False,
    ),
    "chainRemoteAction": (
        "site",
        "tcg/deploy/alpha/macmini2014/nexus-v2-phase1-chain-ingress-remote.sh",
        True,
    ),
    "nodeCandidateTool": (
        "chain",
        "scripts/nexus-v2-private-alpha/node_candidate.py",
        True,
    ),
    "readOnlyCaddyfile": (
        "site",
        "tcg/deploy/alpha/macmini2014/nexus-v2-phase1-readonly.Caddyfile",
        False,
    ),
    "siteDeploymentLibrary": (
        "site",
        "tcg/deploy/alpha/macmini2014/lib.sh",
        False,
    ),
    "siteRemoteAction": (
        "site",
        "tcg/deploy/alpha/macmini2014/nexus-v2-phase1-site-ingress-remote.sh",
        True,
    ),
}
REVIEWED_CADDY_SHA256 = (
    # Cross-repository byte contract: approved Wave-C Phase-1 boundary. The
    # release lock independently pins the clean web source commit/tree.
    "0156313f06dd3e3c9a5f34dd7688b56e44a6c25487788ac7712872ed441619d5"
)
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
DEPLOY_INPUT_KEYS = {
    "siteCandidatePath",
    "siteCandidateSha256",
    "phase1CaddyfilePath",
    "phase1CaddyfileSha256",
}
CHAIN_INPUT_KEYS = {
    "nodeCandidatePath",
    "nodeCandidateSha256",
    "nodeTargetIdentityPath",
    "nodeTargetIdentitySha256",
    "mediaCandidatePath",
    "mediaCandidateSha256",
}
CLOSE_INPUT_KEYS = {
    "stabilityWindowSeconds",
    "runtimeBundleRoot",
    "runtimeBundleManifestSha256",
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
DEPLOY_CHECKS = {
    "paidOrPublicActivationDisabled",
    "phase1ReadOnlyConfigHashVerified",
    "phase1PostDeployIdentityVerified",
    "postDeployIdentityPinned",
    "preResetClosureConsumed",
    "siteCandidateHashVerified",
    "siteDeployDryRunValidated",
    "siteDeployExecuteValidated",
}
POST_DEPLOY_KEYS = {
    "schemaVersion",
    "kind",
    "releaseId",
    "siteReleaseVersion",
    "sourceCommit",
    "siteSourceCommit",
    "readinessSha256",
    "preResetClosureHandoffSha256",
    "automaticRestoreArmSha256",
    "compose",
    "phase1",
    "services",
    "authorityStatus",
    "safety",
    "capturedAtUtc",
}
POST_DEPLOY_COMPOSE_KEYS = {"path", "projectName", "sha256"}
POST_DEPLOY_PHASE1_KEYS = {
    "ingressMode",
    "caddyfileSha256",
    "publicActionSubmission",
}
POST_DEPLOY_SERVICE_KEYS = {
    "containerId",
    "imageReference",
    "imageId",
    "publications",
}
POST_DEPLOY_PUBLICATION_KEYS = {
    "containerPort",
    "protocol",
    "hostIp",
    "hostPort",
}
POST_DEPLOY_AUTHORITY_KEYS = {
    "available",
    "baseUrl",
    "fps",
    "eterraLegends",
}
POST_DEPLOY_AUTHORITY_DOCUMENT_KEYS = {
    "path",
    "sourceDocumentSha256",
    "facts",
}
POST_DEPLOY_SAFETY = {
    "phase1Closed": True,
    "paidOrPublicActivationAuthorized": False,
    "publicActionSubmissionEnabled": False,
    "economicFeaturesEnabled": False,
}
FPS_AUTHORITY_CONFIG_HASH = (
    "0xfb8aaf7ba62ce67cfd6393330bdfd0de961ef6da1d89f2d011ad1b7cd8d02625"
)
LEGENDS_AUTHORITY_CONFIG_HASH = (
    "0xf2846a4ce742f881cce87edd373061d42b720d10a6c324e782c5487060ae7964"
)
FPS_AUTHORITY_FACTS = {
    "ok": True,
    "signerAvailable": True,
    "authorityStateAvailable": True,
    "runtimeDerivesRewards": True,
    "privateAlphaOnly": True,
    "paidEntry": False,
    "wagering": False,
    "permanentAssetLoss": False,
    "publicProduction": False,
    "authorityConfigHashHex": FPS_AUTHORITY_CONFIG_HASH,
}
LEGENDS_AUTHORITY_FACTS = {
    "ok": True,
    "service": "Eterra.Arcade.Authority",
    "gameId": 1006,
    "gameVersion": 1,
    "modeId": 1,
    "signerAlgorithm": "sr25519",
    "signerAvailable": True,
    "authorityStateAvailable": True,
    "encounterCatalogAvailable": True,
    "ownerAuthorizationAvailable": True,
    "resultJournalAvailable": True,
    "authorityConfigHash": LEGENDS_AUTHORITY_CONFIG_HASH,
    "rewardsDerivedByRuntime": True,
}
CLOSE_CHECKS = {
    "automaticRollbackHandoffPreserved",
    "phase1DryRunPinsValidated",
    "phase1ExecuteTokenValidated",
    "phase1ExecuteValidated",
    "phase1InputsCanonicalAndPinned",
    "stableClosedIngressObservationValidated",
}
FAILURE_POLICY = {
    "automaticEarlyFailureRollbackHandoff": True,
    "driverMayReopenIngress": False,
    "driverMayRestoreArchive": False,
    "partialClosureMustRemainClosed": True,
    "propagateFailure": True,
}
TOKEN_AUTHORIZATIONS = {
    "automaticReopen": False,
    "closePhase1ExternalWriteIngress": True,
    "paidOrPublicActivation": False,
    "preserveAuthorityLocalService": True,
    "preserveBlockProduction": True,
    "preserveReadOnlySiteStack": True,
    "publicActionSubmission": False,
    "sshLocalRpcObservation": True,
}
TOKEN_PIN_KEYS = {
    "inputsSha256",
    "driverSha256",
    "chainCandidateSha256",
    "siteCandidateSha256",
    "targetIdentitySha256",
    "preResetClosureHandoffSha256",
    "automaticRestoreArmSha256",
    "automaticRestoreArmPath",
    "chainEnvironmentSha256",
    "siteEnvironmentSha256",
    "chainLibrarySha256",
    "siteLibrarySha256",
    "chainRemoteScriptSha256",
    "siteRemoteScriptSha256",
    "readOnlyCaddyfileSha256",
    "acceptanceBoundaryToolSha256",
    "nodeCandidateToolSha256",
    "runtimeBundleManifestSha256",
}
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
HASH256_RE = re.compile(r"^0x[0-9a-f]{64}$")
ID_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
SITE_RELEASE_RE = re.compile(
    r"^v(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
    r"(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$"
)
UTC_RE = re.compile(
    r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$"
)


class SiteStageError(RuntimeError):
    """The site Phase-1 stage failed closed."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SiteStageError(message)


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


def read_json(path: Path, label: str, *, canonical: bool = True) -> dict[str, Any]:
    require(path.is_file() and not path.is_symlink(), f"{label} is unavailable")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise SiteStageError(f"invalid {label}") from exc
    require(isinstance(value, dict), f"{label} must be an object")
    if canonical:
        require(path.read_bytes() == canonical_bytes(value), f"{label} is not canonical JSON")
    return value


def regular_file(
    value: Any,
    label: str,
    *,
    digest: Any | None = None,
    executable: bool = False,
    private: bool = False,
) -> Path:
    require(isinstance(value, str), f"invalid {label} path")
    path = Path(value)
    require(path.is_absolute(), f"{label} path must be absolute")
    require(path.is_file() and not path.is_symlink(), f"{label} is unavailable")
    if executable:
        require(bool(path.stat().st_mode & stat.S_IXUSR), f"{label} is not executable")
    if private:
        require(
            stat.S_IMODE(path.stat().st_mode) & 0o077 == 0,
            f"{label} must be owner-only",
        )
    if digest is not None:
        expected = ensure_sha256(digest, f"{label} SHA-256")
        require(sha256_file(path) == expected, f"{label} hash drifted")
    return path.resolve()


def regular_directory(value: Any, label: str) -> Path:
    require(isinstance(value, str), f"invalid {label} path")
    path = Path(value)
    require(path.is_absolute(), f"{label} path must be absolute")
    require(path.is_dir() and not path.is_symlink(), f"{label} is unavailable")
    return path.resolve()


def write_new(path: Path, value: Mapping[str, Any], mode: int = 0o400) -> None:
    require(not os.path.lexists(path), f"refusing to overwrite {path}")
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, mode)
    with os.fdopen(descriptor, "wb") as handle:
        handle.write(canonical_bytes(value))
        handle.flush()
        os.fsync(handle.fileno())
    os.chmod(path, mode)


def git_output(root: Path, *arguments: str) -> str:
    environment = os.environ.copy()
    environment["GIT_OPTIONAL_LOCKS"] = "0"
    completed = subprocess.run(
        ["git", "-C", str(root), *arguments],
        capture_output=True,
        text=True,
        env=environment,
        check=False,
    )
    require(completed.returncode == 0, f"cannot inspect immutable source: {root}")
    return completed.stdout.strip()


def immutable_roots(plan: Mapping[str, Any]) -> dict[str, Path]:
    sources = plan.get("sources")
    require(
        isinstance(sources, dict) and set(sources) == SOURCE_IDS,
        "immutable source set mismatch",
    )
    roots: dict[str, Path] = {}
    for source_id in sorted(SOURCE_IDS):
        pin = sources[source_id]
        require(
            isinstance(pin, dict) and set(pin) == {"root", "expectedCommit"},
            f"{source_id} source pin mismatch",
        )
        commit = ensure_commit(pin.get("expectedCommit"), f"{source_id} commit")
        env_value = os.environ.get(
            f"NEXUS_V2_PRE_RESET_IMMUTABLE_{source_id.upper()}_ROOT", ""
        )
        root = regular_directory(env_value, f"immutable {source_id} source")
        require(
            Path(git_output(root, "rev-parse", "--show-toplevel")).resolve() == root,
            f"immutable {source_id} source is not a Git root",
        )
        require(git_output(root, "rev-parse", "HEAD") == commit, f"{source_id} source commit drifted")
        require(
            git_output(root, "status", "--porcelain", "--untracked-files=all") == "",
            f"immutable {source_id} source is dirty",
        )
        roots[source_id] = root
    require(
        plan.get("sourceCommit") == sources["chain"]["expectedCommit"],
        "chain source identity mismatch",
    )
    return roots


def resolve_tool(
    role: str, pin: Any, roots: Mapping[str, Path]
) -> dict[str, Any]:
    require(
        role in EXPECTED_TOOL_PATHS
        and isinstance(pin, dict)
        and set(pin) == {"sourceId", "path", "sha256"},
        f"{role} tool pin mismatch",
    )
    source_id, expected_relative = EXPECTED_TOOL_PATHS[role]
    require(
        pin.get("sourceId") == source_id and pin.get("path") == expected_relative,
        f"{role} tool identity mismatch",
    )
    path = (roots[source_id] / expected_relative).resolve()
    require(roots[source_id] in path.parents, f"{role} tool escapes immutable source")
    path = regular_file(
        str(path), role, digest=pin.get("sha256"), executable=True
    )
    return {"sourceId": source_id, "path": path, "sha256": pin["sha256"]}


def internal_tools(roots: Mapping[str, Path]) -> dict[str, dict[str, Any]]:
    values: dict[str, dict[str, Any]] = {}
    for role, (source_id, relative, executable) in INTERNAL_PATHS.items():
        path = (roots[source_id] / relative).resolve()
        require(roots[source_id] in path.parents, f"{role} escapes immutable source")
        path = regular_file(str(path), role, executable=executable)
        values[role] = {"path": path, "sha256": sha256_file(path)}
    require(
        values["readOnlyCaddyfile"]["sha256"] == REVIEWED_CADDY_SHA256,
        "Phase-1 Caddyfile is not the reviewed read-only boundary",
    )
    return values


def pinned_artifacts(
    plan: Mapping[str, Any], contract: Mapping[str, Any]
) -> dict[str, dict[str, Any]]:
    value = plan.get("artifacts")
    require(isinstance(value, dict) and set(value) == ARTIFACT_IDS, "artifact set mismatch")
    artifacts: dict[str, dict[str, Any]] = {}
    for artifact_id, pin in value.items():
        require(
            isinstance(pin, dict) and set(pin) == {"path", "sha256"},
            f"{artifact_id} pin mismatch",
        )
        path = regular_file(
            pin.get("path"), artifact_id, digest=pin.get("sha256")
        )
        artifacts[artifact_id] = {"path": path, "sha256": pin["sha256"]}
    require(
        contract.get("artifactSha256")
        == {name: pin["sha256"] for name, pin in sorted(artifacts.items())},
        "workflow artifact hash binding mismatch",
    )
    return artifacts


def path_pin(value: Any, label: str, *, private: bool = False) -> dict[str, Any]:
    require(
        isinstance(value, dict) and set(value) == {"path", "sha256"},
        f"{label} pin mismatch",
    )
    path = regular_file(
        value.get("path"), label, digest=value.get("sha256"), private=private
    )
    return {"path": path, "sha256": value["sha256"]}


def validate_replacement_lock(
    artifact: Mapping[str, Any],
    plan: Mapping[str, Any],
    roots: Mapping[str, Path],
    chain_inputs: Mapping[str, Any],
    close_inputs: Mapping[str, Any],
) -> dict[str, Any]:
    lock = read_json(artifact["path"], "replacement lock")
    require(lock.get("schemaVersion") == 1, "replacement lock schema mismatch")
    require(
        lock.get("kind") == "nexus-v2-private-alpha-pre-cutover-replacement-lock",
        "replacement lock kind mismatch",
    )
    require(lock.get("releaseId") == plan["releaseId"], "replacement lock release mismatch")
    repositories = lock.get("repositories")
    require(isinstance(repositories, dict), "replacement lock repositories are unavailable")
    for source_id, repository_id in (
        ("chain", "chain"),
        ("media", "media"),
        ("site", "web"),
    ):
        repository = repositories.get(repository_id)
        require(isinstance(repository, dict), f"replacement lock {repository_id} repository is unavailable")
        require(
            repository.get("head") == plan["sources"][source_id]["expectedCommit"],
            f"replacement lock {repository_id} commit mismatch",
        )
    lock_artifacts = lock.get("artifacts")
    require(isinstance(lock_artifacts, dict), "replacement lock artifacts are unavailable")
    chain_environment = path_pin(
        lock_artifacts.get("deploymentEnvironment"),
        "selected chain environment",
        private=True,
    )
    site_environment = path_pin(
        lock_artifacts.get("siteDeploymentEnvironment"),
        "selected site environment",
        private=True,
    )
    require(
        str(chain_environment["path"]) == plan.get("selectedDeploymentEnvironment"),
        "selected chain environment differs from replacement lock",
    )
    require(
        str(site_environment["path"]) == plan.get("selectedSiteDeploymentEnvironment"),
        "selected site environment differs from replacement lock",
    )
    node = path_pin(lock_artifacts.get("nodeCandidateManifest"), "node candidate")
    target = path_pin(lock_artifacts.get("targetIdentity"), "target identity")
    require(
        (str(node["path"]), node["sha256"])
        == (chain_inputs["nodeCandidatePath"], chain_inputs["nodeCandidateSha256"]),
        "node candidate differs from chain deployment stage",
    )
    require(
        (str(target["path"]), target["sha256"])
        == (
            chain_inputs["nodeTargetIdentityPath"],
            chain_inputs["nodeTargetIdentitySha256"],
        ),
        "target identity differs from chain deployment stage",
    )
    runtime = path_pin(lock_artifacts.get("runtimeBundleManifest"), "runtime bundle manifest")
    runtime_root = regular_directory(
        str(close_inputs.get("runtimeBundleRoot")), "runtime bundle root"
    )
    require(runtime["path"].parent == runtime_root, "runtime bundle root differs from replacement lock")
    require(
        runtime["sha256"] == close_inputs.get("runtimeBundleManifestSha256"),
        "runtime bundle manifest differs from replacement lock",
    )
    target_value = read_json(target["path"], "target identity")
    genesis_hash = target_value.get("genesisHash")
    require(
        isinstance(genesis_hash, str) and HASH256_RE.fullmatch(genesis_hash),
        "target genesis hash is invalid",
    )
    return {
        "chainEnvironment": chain_environment,
        "siteEnvironment": site_environment,
        "nodeCandidate": node,
        "targetIdentity": target,
        "runtimeBundle": {"root": runtime_root, "manifestSha256": runtime["sha256"]},
        "genesisHash": genesis_hash,
        "siteSourceCommit": plan["sources"]["site"]["expectedCommit"],
    }


def validate_site_inputs(
    value: Any, roots: Mapping[str, Path], plan: Mapping[str, Any]
) -> dict[str, Any]:
    require(isinstance(value, dict) and set(value) == DEPLOY_INPUT_KEYS, "site deploy input schema mismatch")
    candidate = regular_file(
        value.get("siteCandidatePath"),
        "site candidate",
        digest=value.get("siteCandidateSha256"),
    )
    candidate_value = read_json(candidate, "site candidate")
    require(
        set(candidate_value)
        == {
            "candidateSourceCommit",
            "indexerImageId",
            "indexerImageRef",
            "releaseVersion",
            "schemaVersion",
            "siteBuildHash",
            "siteImageId",
            "siteImageRef",
        },
        "site candidate schema mismatch",
    )
    require(candidate_value.get("schemaVersion") == 1, "site candidate version mismatch")
    require(
        candidate_value.get("candidateSourceCommit")
        == plan["sources"]["site"]["expectedCommit"],
        "site candidate source mismatch",
    )
    require(
        candidate_value.get("releaseVersion") == plan["siteReleaseVersion"],
        "site candidate release mismatch",
    )
    for field in ("siteImageId", "indexerImageId"):
        require(
            isinstance(candidate_value.get(field), str)
            and re.fullmatch(r"sha256:[0-9a-f]{64}", candidate_value[field]),
            f"site candidate {field} is invalid",
        )
    supplied_caddy = regular_file(
        value.get("phase1CaddyfilePath"),
        "Phase-1 Caddyfile",
        digest=value.get("phase1CaddyfileSha256"),
    )
    expected_caddy = (
        roots["site"]
        / "tcg/deploy/alpha/macmini2014/nexus-v2-phase1-readonly.Caddyfile"
    ).resolve()
    regular_file(
        str(expected_caddy),
        "immutable Phase-1 Caddyfile",
        digest=value.get("phase1CaddyfileSha256"),
    )
    require(
        value.get("phase1CaddyfileSha256") == REVIEWED_CADDY_SHA256,
        "Phase-1 Caddyfile hash is not the reviewed boundary",
    )
    require(
        sha256_file(supplied_caddy) == sha256_file(expected_caddy),
        "supplied and immutable Phase-1 Caddyfile bytes differ",
    )
    return {
        "siteCandidate": {
            "path": candidate,
            "sha256": value["siteCandidateSha256"],
            "value": candidate_value,
        },
        "phase1Caddyfile": {
            "path": expected_caddy,
            "sha256": value["phase1CaddyfileSha256"],
        },
    }


def validate_chain_inputs(value: Any) -> dict[str, Any]:
    require(isinstance(value, dict) and set(value) == CHAIN_INPUT_KEYS, "chain deploy input schema mismatch")
    result: dict[str, Any] = {}
    for prefix in ("nodeCandidate", "nodeTargetIdentity", "mediaCandidate"):
        path = regular_file(
            value.get(f"{prefix}Path"),
            prefix,
            digest=value.get(f"{prefix}Sha256"),
        )
        result[f"{prefix}Path"] = str(path)
        result[f"{prefix}Sha256"] = value[f"{prefix}Sha256"]
    return result


def validate_close_inputs(value: Any) -> dict[str, Any]:
    require(isinstance(value, dict) and set(value) == CLOSE_INPUT_KEYS, "Phase-1 close input schema mismatch")
    window = value.get("stabilityWindowSeconds")
    require(
        isinstance(window, int)
        and not isinstance(window, bool)
        and 30 <= window <= 900,
        "stability window must be in 30..900 seconds",
    )
    root = regular_directory(value.get("runtimeBundleRoot"), "runtime bundle root")
    digest = ensure_sha256(
        value.get("runtimeBundleManifestSha256"),
        "runtime bundle manifest SHA-256",
    )
    regular_file(
        str(root / "runtime-bundle-manifest.json"),
        "runtime bundle manifest",
        digest=digest,
    )
    return {
        "stabilityWindowSeconds": window,
        "runtimeBundleRoot": root,
        "runtimeBundleManifestSha256": digest,
    }


def validate_arm(
    path: Path,
    expected_sha256: str,
    plan: Mapping[str, Any],
    fixture_only: bool,
) -> dict[str, Any]:
    require(sha256_file(path) == expected_sha256, "automatic-restore arm hash mismatch")
    require(stat.S_IMODE(path.stat().st_mode) & 0o077 == 0, "automatic-restore arm must be owner-only")
    value = read_json(path, "automatic-restore arm")
    for field, expected in (
        ("operationId", plan["operationId"]),
        ("releaseId", plan["releaseId"]),
        ("siteReleaseVersion", plan["siteReleaseVersion"]),
        ("sourceCommit", plan["sourceCommit"]),
        ("planSha256", plan["planSha256"]),
        ("fixtureOnly", fixture_only),
    ):
        require(value.get(field) == expected, f"automatic-restore arm {field} mismatch")
    require(value.get("automaticRestoreArmed") is True, "automatic restore is not armed")
    require(value.get("paidOrPublicActivationAllowed") is False, "automatic-restore arm permits paid/public activation")
    return value


def validate_prior_result(
    workflow_root: Path,
    stage: str,
    context: Mapping[str, Any],
) -> Path:
    root = workflow_root / "stages" / stage
    require(root.is_dir() and not root.is_symlink(), f"prior stage is unavailable: {stage}")
    path = regular_file(str(root / "result.json"), f"{stage} result")
    value = read_json(path, f"{stage} result")
    require(set(value) == STAGE_RESULT_KEYS, f"{stage} result schema mismatch")
    for field, expected in (
        ("schemaVersion", 1),
        ("kind", "nexus-v2-private-alpha-replacement-workflow-stage-result"),
        ("operationId", context["operationId"]),
        ("releaseId", context["releaseId"]),
        ("siteReleaseVersion", context["siteReleaseVersion"]),
        ("sourceCommit", context["sourceCommit"]),
        ("planSha256", context["planSha256"]),
        ("workflowContractSha256", context["contractSha256"]),
        ("stage", stage),
        ("result", "passed"),
        ("fixtureOnly", context["fixtureOnly"]),
        ("mutationPerformed", stage != "createPreResetClosure"),
        ("acceptanceStartFenceWritten", False),
    ):
        require(value.get(field) == expected, f"{stage} result {field} mismatch")
    checks = value.get("checks")
    require(
        isinstance(checks, dict) and checks and all(item is True for item in checks.values()),
        f"{stage} result has a failed check",
    )
    return root


def validate_closure(
    workflow_root: Path,
    context: Mapping[str, Any],
    arm: Mapping[str, Any],
) -> dict[str, Any]:
    path = regular_file(
        str(workflow_root / "stages/createPreResetClosure/pre-reset-closure.json"),
        "pre-reset closure handoff",
        private=True,
    )
    value = read_json(path, "pre-reset closure handoff")
    require(
        value.get("kind") == "nexus-v2-private-alpha-pre-reset-closure-handoff",
        "pre-reset closure handoff kind mismatch",
    )
    for field, expected in (
        ("releaseId", context["releaseId"]),
        ("sourceCommit", context["sourceCommit"]),
        ("automaticRestoreArmPath", str(context["armPath"])),
        ("automaticRestoreArmSha256", context["armSha256"]),
        ("automaticRestoreArmed", True),
        ("mutationPerformed", False),
    ):
        require(value.get(field) == expected, f"pre-reset closure {field} mismatch")
    for field in (
        "replacementLockSha256",
        "resetReadinessSha256",
        "finalFreezeEvidenceSha256",
        "backupManifestSha256",
        "restoreEvidenceSha256",
        "migrationEvidenceSha256",
    ):
        require(value.get(field) == arm.get(field), f"pre-reset closure {field} differs from arm")
    return {"path": path, "sha256": sha256_file(path), "value": value}


def publication_sort_key(value: Mapping[str, Any]) -> tuple[int, str, str, int]:
    return (
        int(value["containerPort"]),
        str(value["protocol"]),
        str(value["hostIp"]),
        int(value["hostPort"]),
    )


def validate_publications(value: Any, service: str) -> list[dict[str, Any]]:
    require(isinstance(value, list), f"{service} publications must be an array")
    publications: list[dict[str, Any]] = []
    for index, item in enumerate(value):
        require(
            isinstance(item, dict) and set(item) == POST_DEPLOY_PUBLICATION_KEYS,
            f"{service} publication {index} schema mismatch",
        )
        for field in ("containerPort", "hostPort"):
            port = item.get(field)
            require(
                isinstance(port, int)
                and not isinstance(port, bool)
                and 1 <= port <= 65535,
                f"{service} publication {field} is invalid",
            )
        require(item.get("protocol") in {"tcp", "udp"}, f"{service} publication protocol is invalid")
        require(
            isinstance(item.get("hostIp"), str) and item["hostIp"],
            f"{service} publication host IP is invalid",
        )
        publications.append(dict(item))
    require(
        publications == sorted(publications, key=publication_sort_key),
        f"{service} publications are not canonically sorted",
    )
    require(
        len({publication_sort_key(item) for item in publications}) == len(publications),
        f"{service} publications contain duplicates",
    )
    return publications


def validate_post_deploy_identity(
    context: Mapping[str, Any], path: Path
) -> dict[str, Any]:
    path = regular_file(str(path), "site post-deploy identity", private=True)
    require(
        stat.S_IMODE(path.stat().st_mode) == 0o400,
        "site post-deploy identity must have mode 0400",
    )
    value = read_json(path, "site post-deploy identity")
    require(set(value) == POST_DEPLOY_KEYS, "site post-deploy identity schema mismatch")
    for field, expected in (
        ("schemaVersion", 1),
        ("kind", "nexus-v2-private-alpha-site-post-deploy-identity"),
        ("releaseId", context["releaseId"]),
        ("siteReleaseVersion", context["siteReleaseVersion"]),
        ("sourceCommit", context["sourceCommit"]),
        ("siteSourceCommit", context["replacement"]["siteSourceCommit"]),
        ("readinessSha256", context["artifacts"]["resetReadiness"]["sha256"]),
        ("preResetClosureHandoffSha256", context["closure"]["sha256"]),
        ("automaticRestoreArmSha256", context["armSha256"]),
        ("safety", POST_DEPLOY_SAFETY),
    ):
        require(value.get(field) == expected, f"site post-deploy identity {field} mismatch")
    captured = value.get("capturedAtUtc")
    require(
        isinstance(captured, str) and UTC_RE.fullmatch(captured),
        "site post-deploy capture time is invalid",
    )
    compose = value.get("compose")
    require(
        isinstance(compose, dict) and set(compose) == POST_DEPLOY_COMPOSE_KEYS,
        "site post-deploy Compose schema mismatch",
    )
    expected_compose = (
        context["roots"]["site"]
        / "tcg/deploy/alpha/macmini2014/docker-compose.yaml"
    ).resolve()
    regular_file(str(expected_compose), "immutable site Compose file")
    require(
        compose.get("path")
        == "/opt/eterra-alpha/site/current/deploy/alpha/macmini2014/docker-compose.yaml",
        "site post-deploy Compose path mismatch",
    )
    require(
        compose.get("projectName") == "eterra-tcg-site-alpha",
        "site post-deploy Compose project mismatch",
    )
    require(
        compose.get("sha256") == sha256_file(expected_compose),
        "site post-deploy Compose hash mismatch",
    )
    phase1 = value.get("phase1")
    require(
        isinstance(phase1, dict) and set(phase1) == POST_DEPLOY_PHASE1_KEYS,
        "site post-deploy Phase-1 schema mismatch",
    )
    require(
        phase1
        == {
            "ingressMode": "AllExternalWriteIngressClosed",
            "caddyfileSha256": REVIEWED_CADDY_SHA256,
            "publicActionSubmission": False,
        },
        "site post-deploy Phase-1 boundary mismatch",
    )
    services = value.get("services")
    require(
        isinstance(services, dict)
        and set(services) == {"caddy", "indexer-api", "mongo", "site"},
        "site post-deploy service set mismatch",
    )
    normalized: dict[str, dict[str, Any]] = {}
    candidate = context["siteInputs"]["siteCandidate"]["value"]
    for service, item in services.items():
        require(
            isinstance(item, dict) and set(item) == POST_DEPLOY_SERVICE_KEYS,
            f"site post-deploy service schema mismatch: {service}",
        )
        require(
            isinstance(item.get("containerId"), str)
            and re.fullmatch(r"[0-9a-f]{12,64}", item["containerId"]),
            f"site post-deploy container ID is invalid: {service}",
        )
        require(
            isinstance(item.get("imageReference"), str) and item["imageReference"],
            f"site post-deploy image reference is invalid: {service}",
        )
        require(
            isinstance(item.get("imageId"), str)
            and re.fullmatch(r"sha256:[0-9a-f]{64}", item["imageId"]),
            f"site post-deploy image ID is invalid: {service}",
        )
        publications = validate_publications(item.get("publications"), service)
        normalized[service] = {**item, "publications": publications}
    for service, prefix in (("site", "site"), ("indexer-api", "indexer")):
        require(
            normalized[service]["imageId"] == candidate[f"{prefix}ImageId"]
            and normalized[service]["imageReference"]
            == candidate[f"{prefix}ImageRef"],
            f"site post-deploy candidate image mismatch: {service}",
        )
    require(
        normalized["site"]["publications"]
        == [
            {
                "containerPort": 3000,
                "protocol": "tcp",
                "hostIp": "127.0.0.1",
                "hostPort": 3000,
            }
        ],
        "site service is not loopback-only on 3000",
    )
    require(
        normalized["indexer-api"]["publications"]
        == [
            {
                "containerPort": 8787,
                "protocol": "tcp",
                "hostIp": "127.0.0.1",
                "hostPort": 8787,
            }
        ],
        "indexer API is not loopback-only on 8787",
    )
    require(normalized["mongo"]["publications"] == [], "Mongo is unexpectedly published")
    caddy_publications = normalized["caddy"]["publications"]
    require(caddy_publications, "Caddy publications are unavailable")
    require(
        all(
            item["containerPort"] in {80, 443}
            and item["hostPort"] == item["containerPort"]
            and item["protocol"] == "tcp"
            and item["hostIp"] in {"0.0.0.0", "::"}
            for item in caddy_publications
        )
        and {item["containerPort"] for item in caddy_publications} == {80, 443},
        "Caddy runtime publications differ from the Phase-1 public read-only boundary",
    )
    authority = value.get("authorityStatus")
    require(
        isinstance(authority, dict) and set(authority) == POST_DEPLOY_AUTHORITY_KEYS,
        "site post-deploy authority status schema mismatch",
    )
    available = authority.get("available")
    require(isinstance(available, bool), "site post-deploy authority availability is invalid")
    if not available:
        require(
            authority == {
                "available": False,
                "baseUrl": None,
                "fps": None,
                "eterraLegends": None,
            },
            "unavailable authority status must remain explicitly null",
        )
    else:
        base_url = authority.get("baseUrl")
        require(
            base_url == "http://127.0.0.1:5016",
            "authority status base URL is not canonical loopback",
        )
        for name, expected_path, expected_facts in (
            ("fps", "/v1/fps/status", FPS_AUTHORITY_FACTS),
            (
                "eterraLegends",
                "/v1/eterra-legends/status",
                LEGENDS_AUTHORITY_FACTS,
            ),
        ):
            document = authority.get(name)
            require(
                isinstance(document, dict)
                and set(document) == POST_DEPLOY_AUTHORITY_DOCUMENT_KEYS,
                f"authority status document schema mismatch: {name}",
            )
            require(
                document.get("path") == expected_path,
                f"authority status path is invalid: {name}",
            )
            ensure_sha256(
                document.get("sourceDocumentSha256"),
                f"authority {name} source document SHA-256",
            )
            require(
                document.get("facts") == expected_facts,
                f"authority status facts are unsafe: {name}",
            )
    return {"path": path, "sha256": sha256_file(path), "value": value}


def validate_inputs(args: argparse.Namespace) -> dict[str, Any]:
    require(args.stage in SUPPORTED_STAGES, "unsupported site workflow stage")
    plan_path = regular_file(args.plan, "supervisor plan", digest=args.plan_sha256)
    plan_sha = ensure_sha256(args.plan_sha256, "plan SHA-256")
    plan = read_json(plan_path, "supervisor plan")
    require(
        plan.get("schemaVersion") == 1
        and plan.get("kind") == "nexus-v2-private-alpha-pre-reset-rollback-supervisor-plan",
        "supervisor plan kind mismatch",
    )
    operation_id = ensure_id(plan.get("operationId"), "operation ID")
    release_id = ensure_id(plan.get("releaseId"), "chain release ID")
    site_release = plan.get("siteReleaseVersion")
    require(isinstance(site_release, str) and SITE_RELEASE_RE.fullmatch(site_release), "invalid site release version")
    source_commit = ensure_commit(plan.get("sourceCommit"), "source commit")
    backend = plan.get("backend")
    require(backend in {PRODUCTION_BACKEND, FIXTURE_BACKEND}, "unsupported supervisor backend")
    fixture_only = backend == FIXTURE_BACKEND
    if fixture_only:
        require(
            not os.environ.get("NEXUS_V2_PRE_RESET_PRODUCTION_CONFIRMATION"),
            "NONDEPLOYABLE fixture carries production confirmation",
        )
    else:
        require(
            os.environ.get("NEXUS_V2_PRE_RESET_PRODUCTION_CONFIRMATION")
            == PRODUCTION_CONFIRMATION,
            "production site stage requires PRIVATE_ALPHA_ROLLBACK_ONLY confirmation",
        )
    plan_with_sha = dict(plan)
    plan_with_sha["planSha256"] = plan_sha

    contract_path = regular_file(
        args.workflow_contract,
        "workflow contract",
        digest=args.workflow_contract_sha256,
    )
    contract_sha = ensure_sha256(
        args.workflow_contract_sha256, "workflow contract SHA-256"
    )
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
    workflow = plan.get("workflow")
    require(isinstance(workflow, dict), "supervisor workflow is unavailable")
    require(
        workflow.get("contract") == {"path": str(contract_path), "sha256": contract_sha},
        "workflow contract is not plan-pinned",
    )
    roots = immutable_roots(plan)
    tool_pins = contract.get("toolPins")
    require(isinstance(tool_pins, dict) and set(tool_pins) == TOOL_ROLES, "workflow tool set mismatch")
    tools = {role: resolve_tool(role, tool_pins[role], roots) for role in sorted(TOOL_ROLES)}
    internals = internal_tools(roots)
    artifacts = pinned_artifacts(plan, contract)
    stage_inputs = contract.get("stageInputs")
    require(
        isinstance(stage_inputs, dict) and set(stage_inputs) == set(STAGES),
        "workflow stage input set mismatch",
    )
    site_inputs = validate_site_inputs(stage_inputs.get("deploySiteIndexer"), roots, plan)
    chain_inputs = validate_chain_inputs(stage_inputs.get("deployChainMediaAuthority"))
    close_inputs = validate_close_inputs(stage_inputs.get("closeIngressAndObserve"))
    replacement = validate_replacement_lock(
        artifacts["replacementLock"], plan, roots, chain_inputs, close_inputs
    )

    arm_path = regular_file(args.automatic_restore_arm, "automatic-restore arm", private=True)
    arm_sha = ensure_sha256(args.automatic_restore_arm_sha256, "automatic-restore arm SHA-256")
    arm = validate_arm(arm_path, arm_sha, plan_with_sha, fixture_only)
    workflow_root = regular_directory(args.workflow_state_root, "workflow state root")
    stage_root = regular_directory(args.stage_state_root, "site stage state root")
    require(
        stage_root == workflow_root / "stages" / args.stage,
        "site stage state root mismatch",
    )
    result_path = Path(args.result)
    require(result_path.is_absolute(), "stage result path must be absolute")
    require(result_path.resolve() == stage_root / "result.json", "stage result path mismatch")
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
        "internals": internals,
        "artifacts": artifacts,
        "siteInputs": site_inputs,
        "chainInputs": chain_inputs,
        "closeInputs": close_inputs,
        "replacement": replacement,
        "armPath": arm_path,
        "armSha256": arm_sha,
        "arm": arm,
        "workflowRoot": workflow_root,
        "stageRoot": stage_root,
        "resultPath": result_path.resolve(),
        "stage": args.stage,
        "postDeployIdentityPath": stage_root
        / "site-post-deploy-identity.json",
    }
    validate_prior_result(workflow_root, "createPreResetClosure", context)
    validate_prior_result(workflow_root, "deployChainMediaAuthority", context)
    context["closure"] = validate_closure(workflow_root, context, arm)
    if args.stage == "closeIngressAndObserve":
        validate_prior_result(workflow_root, "deploySiteIndexer", context)
        context["postDeployIdentityPath"] = (
            workflow_root
            / "stages/deploySiteIndexer/site-post-deploy-identity.json"
        )
        context["postDeployIdentity"] = validate_post_deploy_identity(
            context, context["postDeployIdentityPath"]
        )
    return context


def command_environment(context: Mapping[str, Any]) -> dict[str, str]:
    environment = os.environ.copy()
    environment["ALPHA_MACMINI2010_ENV_FILE"] = str(
        context["replacement"]["chainEnvironment"]["path"]
    )
    environment["ALPHA_MACMINI2014_ENV_FILE"] = str(
        context["replacement"]["siteEnvironment"]["path"]
    )
    environment["NEXUS_V2_LOCAL_ONLY_RELEASE"] = "1"
    return environment


def run_command(
    executable: Sequence[str],
    arguments: Sequence[str],
    log_path: Path,
    environment: Mapping[str, str],
) -> None:
    require(not os.path.lexists(log_path), f"refusing to overwrite {log_path}")
    with log_path.open("xb") as log:
        os.chmod(log_path, 0o600)
        completed = subprocess.run(
            [*executable, *arguments],
            stdout=log,
            stderr=subprocess.STDOUT,
            env=dict(environment),
            check=False,
        )
    require(completed.returncode == 0, f"nested site tool failed; see {log_path}")


def deploy_arguments(context: Mapping[str, Any]) -> list[str]:
    return [
        "--fresh",
        "--fresh-reset-readiness",
        str(context["artifacts"]["resetReadiness"]["path"]),
        "--pre-reset-closure-handoff",
        str(context["closure"]["path"]),
        "--pre-reset-closure-handoff-sha256",
        context["closure"]["sha256"],
        "--promote-candidate",
        str(context["siteInputs"]["siteCandidate"]["path"]),
        "--phase1-closed",
        "--phase1-caddyfile",
        str(context["siteInputs"]["phase1Caddyfile"]["path"]),
        "--phase1-caddyfile-sha256",
        context["siteInputs"]["phase1Caddyfile"]["sha256"],
        "--post-deploy-identity-output",
        str(context["postDeployIdentityPath"]),
    ]


def write_fixture_post_deploy_identity(context: Mapping[str, Any]) -> None:
    candidate = context["siteInputs"]["siteCandidate"]["value"]
    compose_path = (
        context["roots"]["site"]
        / "tcg/deploy/alpha/macmini2014/docker-compose.yaml"
    ).resolve()
    regular_file(str(compose_path), "immutable site Compose file")

    def service(
        marker: str,
        reference: str,
        image_id: str,
        publications: list[dict[str, Any]],
    ) -> dict[str, Any]:
        return {
            "containerId": hashlib.sha256(marker.encode("utf-8")).hexdigest(),
            "imageReference": reference,
            "imageId": image_id,
            "publications": publications,
        }

    write_new(
        context["postDeployIdentityPath"],
        {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-site-post-deploy-identity",
            "releaseId": context["releaseId"],
            "siteReleaseVersion": context["siteReleaseVersion"],
            "sourceCommit": context["sourceCommit"],
            "siteSourceCommit": context["replacement"]["siteSourceCommit"],
            "readinessSha256": context["artifacts"]["resetReadiness"]["sha256"],
            "preResetClosureHandoffSha256": context["closure"]["sha256"],
            "automaticRestoreArmSha256": context["armSha256"],
            "compose": {
                "path": "/opt/eterra-alpha/site/current/deploy/alpha/macmini2014/docker-compose.yaml",
                "projectName": "eterra-tcg-site-alpha",
                "sha256": sha256_file(compose_path),
            },
            "phase1": {
                "ingressMode": "AllExternalWriteIngressClosed",
                "caddyfileSha256": REVIEWED_CADDY_SHA256,
                "publicActionSubmission": False,
            },
            "services": {
                "caddy": service(
                    "fixture-caddy",
                    "caddy:2.10.2-alpine",
                    "sha256:" + "5" * 64,
                    [
                        {
                            "containerPort": 80,
                            "protocol": "tcp",
                            "hostIp": "0.0.0.0",
                            "hostPort": 80,
                        },
                        {
                            "containerPort": 443,
                            "protocol": "tcp",
                            "hostIp": "0.0.0.0",
                            "hostPort": 443,
                        },
                    ],
                ),
                "indexer-api": service(
                    "fixture-indexer",
                    candidate["indexerImageRef"],
                    candidate["indexerImageId"],
                    [
                        {
                            "containerPort": 8787,
                            "protocol": "tcp",
                            "hostIp": "127.0.0.1",
                            "hostPort": 8787,
                        }
                    ],
                ),
                "mongo": service(
                    "fixture-mongo", "mongo:7", "sha256:" + "6" * 64, []
                ),
                "site": service(
                    "fixture-site",
                    candidate["siteImageRef"],
                    candidate["siteImageId"],
                    [
                        {
                            "containerPort": 3000,
                            "protocol": "tcp",
                            "hostIp": "127.0.0.1",
                            "hostPort": 3000,
                        }
                    ],
                ),
            },
            "authorityStatus": {
                "available": False,
                "baseUrl": None,
                "fps": None,
                "eterraLegends": None,
            },
            "safety": dict(POST_DEPLOY_SAFETY),
            "capturedAtUtc": dt.datetime.now(dt.timezone.utc)
            .replace(microsecond=0)
            .strftime("%Y-%m-%dT%H:%M:%SZ"),
        },
    )


def run_deploy(context: Mapping[str, Any]) -> dict[str, bool]:
    root: Path = context["stageRoot"]
    preflight_log = root / "site-deploy-preflight.log"
    execute_log = root / "site-deploy-execute.log"
    marker = root / "NONDEPLOYABLE.fixture.json"
    identity_path: Path = context["postDeployIdentityPath"]
    for path in (preflight_log, execute_log, marker, identity_path):
        require(not os.path.lexists(path), f"refusing to reuse site deployment output: {path}")
    if context["fixtureOnly"]:
        write_new(
            marker,
            {
                "schemaVersion": 1,
                "kind": "nexus-v2-private-alpha-site-deploy.NONDEPLOYABLE",
                "operationId": context["operationId"],
                "fixtureOnly": True,
                "protectedHostContacted": False,
                "mutationPerformed": False,
            },
        )
        write_fixture_post_deploy_identity(context)
    else:
        tool = context["tools"]["siteDeploy"]
        require(sha256_file(tool["path"]) == tool["sha256"], "site deploy tool drifted")
        arguments = deploy_arguments(context)
        environment = command_environment(context)
        run_command(
            [str(tool["path"])],
            [*arguments, "--dry-run"],
            preflight_log,
            environment,
        )
        require(
            not os.path.lexists(identity_path),
            "site deploy dry-run unexpectedly wrote post-deploy identity",
        )
        require(sha256_file(tool["path"]) == tool["sha256"], "site deploy tool drifted after dry-run")
        run_command(
            [str(tool["path"])], arguments, execute_log, environment
        )
    validate_post_deploy_identity(context, identity_path)
    return {name: True for name in sorted(DEPLOY_CHECKS)}


def phase1_inputs(context: Mapping[str, Any]) -> dict[str, Any]:
    internals = context["internals"]
    tools = context["tools"]

    def pinned(value: Mapping[str, Any]) -> dict[str, str]:
        return {"path": str(value["path"]), "sha256": value["sha256"]}

    return {
        "schemaVersion": 2,
        "kind": "nexus-v2-private-alpha-phase1-ingress-closure-inputs.v2",
        "operationId": context["operationId"],
        "releaseId": context["releaseId"],
        "sourceCommit": context["sourceCommit"],
        "siteSourceCommit": context["replacement"]["siteSourceCommit"],
        "siteReleaseVersion": context["siteReleaseVersion"],
        "genesisHash": context["replacement"]["genesisHash"],
        "stabilityWindowSeconds": context["closeInputs"]["stabilityWindowSeconds"],
        "chainSource": {
            "root": str(context["roots"]["chain"]),
            "commit": context["plan"]["sources"]["chain"]["expectedCommit"],
        },
        "siteSource": {
            "root": str(context["roots"]["site"]),
            "commit": context["plan"]["sources"]["site"]["expectedCommit"],
        },
        "chainCandidate": {
            "path": context["chainInputs"]["nodeCandidatePath"],
            "sha256": context["chainInputs"]["nodeCandidateSha256"],
        },
        "siteCandidate": pinned(context["siteInputs"]["siteCandidate"]),
        "targetIdentity": {
            "path": context["chainInputs"]["nodeTargetIdentityPath"],
            "sha256": context["chainInputs"]["nodeTargetIdentitySha256"],
        },
        "preResetClosureHandoff": pinned(context["closure"]),
        "chainEnvironment": pinned(context["replacement"]["chainEnvironment"]),
        "siteEnvironment": pinned(context["replacement"]["siteEnvironment"]),
        "runtimeBundle": {
            "root": str(context["closeInputs"]["runtimeBundleRoot"]),
            "manifestSha256": context["closeInputs"]["runtimeBundleManifestSha256"],
        },
        "tools": {
            "acceptanceBoundary": pinned(tools["acceptanceBoundary"]),
            "chainDeploymentLibrary": pinned(internals["chainDeploymentLibrary"]),
            "chainRemoteAction": pinned(internals["chainRemoteAction"]),
            "nodeCandidateTool": pinned(internals["nodeCandidateTool"]),
            "readOnlyCaddyfile": pinned(internals["readOnlyCaddyfile"]),
            "siteDeploymentLibrary": pinned(internals["siteDeploymentLibrary"]),
            "siteRemoteAction": pinned(internals["siteRemoteAction"]),
        },
        "failurePolicy": dict(FAILURE_POLICY),
    }


def expected_phase1_pins(
    context: Mapping[str, Any], inputs_sha256: str
) -> dict[str, str]:
    internals = context["internals"]
    replacement = context["replacement"]
    return {
        "inputsSha256": inputs_sha256,
        "driverSha256": context["tools"]["phase1IngressClosure"]["sha256"],
        "chainCandidateSha256": context["chainInputs"]["nodeCandidateSha256"],
        "siteCandidateSha256": context["siteInputs"]["siteCandidate"]["sha256"],
        "targetIdentitySha256": context["chainInputs"]["nodeTargetIdentitySha256"],
        "preResetClosureHandoffSha256": context["closure"]["sha256"],
        "automaticRestoreArmSha256": context["armSha256"],
        "automaticRestoreArmPath": str(context["armPath"]),
        "chainEnvironmentSha256": replacement["chainEnvironment"]["sha256"],
        "siteEnvironmentSha256": replacement["siteEnvironment"]["sha256"],
        "chainLibrarySha256": internals["chainDeploymentLibrary"]["sha256"],
        "siteLibrarySha256": internals["siteDeploymentLibrary"]["sha256"],
        "chainRemoteScriptSha256": internals["chainRemoteAction"]["sha256"],
        "siteRemoteScriptSha256": internals["siteRemoteAction"]["sha256"],
        "readOnlyCaddyfileSha256": internals["readOnlyCaddyfile"]["sha256"],
        "acceptanceBoundaryToolSha256": context["tools"]["acceptanceBoundary"]["sha256"],
        "nodeCandidateToolSha256": internals["nodeCandidateTool"]["sha256"],
        "runtimeBundleManifestSha256": context["closeInputs"]["runtimeBundleManifestSha256"],
    }


def validate_dry_evidence(
    context: Mapping[str, Any], path: Path, inputs_sha256: str
) -> dict[str, Any]:
    value = read_json(path, "Phase-1 dry-run evidence")
    for field, expected in (
        ("schemaVersion", 1),
        ("kind", "nexus-v2-private-alpha-phase1-ingress-closure-dry-run"),
        ("operationId", context["operationId"]),
        ("releaseId", context["releaseId"]),
        ("sourceCommit", context["sourceCommit"]),
        ("siteSourceCommit", context["replacement"]["siteSourceCommit"]),
        ("siteReleaseVersion", context["siteReleaseVersion"]),
        ("genesisHash", context["replacement"]["genesisHash"]),
        ("inputsSha256", inputs_sha256),
        ("preResetClosureHandoffSha256", context["closure"]["sha256"]),
        ("automaticRestoreArmSha256", context["armSha256"]),
        ("automaticRestoreArmPath", str(context["armPath"])),
        ("stabilityWindowSeconds", context["closeInputs"]["stabilityWindowSeconds"]),
        ("siteCandidateUsableForExecute", True),
        ("remoteConnectionsAttempted", False),
        ("liveMutationPerformed", False),
        ("automaticEarlyFailureRollbackHandoffPreserved", True),
        ("automaticReopenAuthorized", False),
        ("paidOrPublicActivationAuthorized", False),
    ):
        require(value.get(field) == expected, f"Phase-1 dry-run evidence {field} mismatch")
    pins = value.get("pins")
    require(isinstance(pins, dict) and set(pins) == TOKEN_PIN_KEYS, "Phase-1 dry-run pin set mismatch")
    require(pins == expected_phase1_pins(context, inputs_sha256), "Phase-1 dry-run pins drifted")
    return value


def create_execute_token(
    context: Mapping[str, Any], inputs_sha256: str, pins: Mapping[str, str]
) -> dict[str, Any]:
    now = dt.datetime.now(dt.timezone.utc).replace(microsecond=0)
    return {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-phase1-ingress-closure-execute-token",
        "operationId": context["operationId"],
        "releaseId": context["releaseId"],
        "sourceCommit": context["sourceCommit"],
        "siteSourceCommit": context["replacement"]["siteSourceCommit"],
        "siteReleaseVersion": context["siteReleaseVersion"],
        "genesisHash": context["replacement"]["genesisHash"],
        "inputsSha256": inputs_sha256,
        "stabilityWindowSeconds": context["closeInputs"]["stabilityWindowSeconds"],
        "issuedAtUtc": now.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "notBeforeUtc": now.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "expiresAtUtc": (now + dt.timedelta(minutes=10)).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "executeNonce": secrets.token_hex(32),
        "pins": dict(pins),
        "authorizations": dict(TOKEN_AUTHORIZATIONS),
    }


def fixture_phase1(
    context: Mapping[str, Any],
    inputs_path: Path,
    inputs_sha256: str,
    dry_root: Path,
    token_path: Path,
    output_root: Path,
) -> None:
    pins = expected_phase1_pins(context, inputs_sha256)
    dry_root.mkdir(mode=0o700)
    write_new(
        dry_root / "dry-run-evidence.json",
        {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-phase1-ingress-closure-dry-run",
            "operationId": context["operationId"],
            "releaseId": context["releaseId"],
            "sourceCommit": context["sourceCommit"],
            "siteSourceCommit": context["replacement"]["siteSourceCommit"],
            "siteReleaseVersion": context["siteReleaseVersion"],
            "siteCandidateUsableForExecute": True,
            "genesisHash": context["replacement"]["genesisHash"],
            "driverSha256": pins["driverSha256"],
            "inputsSha256": inputs_sha256,
            "pins": pins,
            "preResetClosureHandoffSha256": context["closure"]["sha256"],
            "automaticRestoreArmSha256": context["armSha256"],
            "automaticRestoreArmPath": str(context["armPath"]),
            "stabilityWindowSeconds": context["closeInputs"]["stabilityWindowSeconds"],
            "plannedActions": {"fixture": ["NONDEPLOYABLE"]},
            "exactClosureObservationCount": 0,
            "protectedExecuteTokenRequired": True,
            "remoteConnectionsAttempted": False,
            "liveMutationPerformed": False,
            "automaticEarlyFailureRollbackHandoffPreserved": True,
            "automaticReopenAuthorized": False,
            "paidOrPublicActivationAuthorized": False,
            "completedAtUtc": dt.datetime.now(dt.timezone.utc).replace(microsecond=0).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "fixtureOnly": True,
        },
    )
    write_new(token_path, create_execute_token(context, inputs_sha256, pins))
    output_root.mkdir(mode=0o700)
    ingress = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-ingress-closed-evidence.NONDEPLOYABLE",
        "operationId": context["operationId"],
        "fixtureOnly": True,
        "protectedHostContacted": False,
    }
    ingress_path = output_root / "ingress-closed-evidence.json"
    write_new(ingress_path, ingress)
    for name, value in (
        ("acceptance-boundary-rpc-capture.json", {"fixtureOnly": True, "counts": {}}),
        ("post-v16-economic-gates.json", {"fixtureOnly": True, "paidOrPublicActivationAuthorized": False}),
        ("post-v16-acceptance-inventory.json", {"fixtureOnly": True, "counts": {"current": 0, "lifetime": 0}}),
    ):
        write_new(output_root / name, value)
    block = context["plan"].get("frozenFinalizedBlock")
    write_new(
        output_root / "execute-evidence.json",
        {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-phase1-ingress-closure-execute-evidence.NONDEPLOYABLE",
            "operationId": context["operationId"],
            "releaseId": context["releaseId"],
            "sourceCommit": context["sourceCommit"],
            "siteSourceCommit": context["replacement"]["siteSourceCommit"],
            "siteReleaseVersion": context["siteReleaseVersion"],
            "siteCandidateUsableForExecute": True,
            "genesisHash": context["replacement"]["genesisHash"],
            "inputsSha256": inputs_sha256,
            "preResetClosureHandoffSha256": context["closure"]["sha256"],
            "automaticRestoreArmSha256": context["armSha256"],
            "automaticRestoreArmPath": str(context["armPath"]),
            "executeTokenSha256": sha256_file(token_path),
            "observedAtFinalizedBlock": block,
            "ingressClosedEvidenceSha256": sha256_file(ingress_path),
            "stabilityWindowSeconds": context["closeInputs"]["stabilityWindowSeconds"],
            "stabilityWindowElapsedMilliseconds": context["closeInputs"]["stabilityWindowSeconds"] * 1000,
            "allExternalWriteIngressClosed": True,
            "blockProductionContinues": True,
            "authorityLocalServicePreserved": True,
            "readOnlySiteStackPreserved": True,
            "automaticReopenAuthorized": False,
            "paidOrPublicActivationAuthorized": False,
            "fixtureOnly": True,
        },
    )


def validate_execute_evidence(
    context: Mapping[str, Any], path: Path, inputs_sha256: str
) -> None:
    value = read_json(path, "Phase-1 execute evidence")
    for field, expected in (
        ("operationId", context["operationId"]),
        ("releaseId", context["releaseId"]),
        ("sourceCommit", context["sourceCommit"]),
        ("siteSourceCommit", context["replacement"]["siteSourceCommit"]),
        ("siteReleaseVersion", context["siteReleaseVersion"]),
        ("siteCandidateUsableForExecute", True),
        ("genesisHash", context["replacement"]["genesisHash"]),
        ("inputsSha256", inputs_sha256),
        ("preResetClosureHandoffSha256", context["closure"]["sha256"]),
        ("automaticRestoreArmSha256", context["armSha256"]),
        ("automaticRestoreArmPath", str(context["armPath"])),
        ("stabilityWindowSeconds", context["closeInputs"]["stabilityWindowSeconds"]),
        ("allExternalWriteIngressClosed", True),
        ("blockProductionContinues", True),
        ("authorityLocalServicePreserved", True),
        ("readOnlySiteStackPreserved", True),
        ("automaticReopenAuthorized", False),
        ("paidOrPublicActivationAuthorized", False),
    ):
        require(value.get(field) == expected, f"Phase-1 execute evidence {field} mismatch")
    elapsed = value.get("stabilityWindowElapsedMilliseconds")
    require(
        isinstance(elapsed, int)
        and elapsed >= context["closeInputs"]["stabilityWindowSeconds"] * 1000,
        "Phase-1 stability window was not observed",
    )
    ingress_sha = ensure_sha256(value.get("ingressClosedEvidenceSha256"), "ingress evidence SHA-256")
    ingress = regular_file(str(path.parent / "ingress-closed-evidence.json"), "ingress-closed evidence", digest=ingress_sha)
    require(ingress.parent == path.parent, "ingress evidence escapes Phase-1 output")
    for name in (
        "acceptance-boundary-rpc-capture.json",
        "post-v16-economic-gates.json",
        "post-v16-acceptance-inventory.json",
    ):
        regular_file(str(path.parent / name), name)


def run_close(context: Mapping[str, Any]) -> dict[str, bool]:
    root: Path = context["stageRoot"]
    inputs_path = root / "phase1-inputs.json"
    dry_root = root / "phase1-dry-run"
    token_path = root / "phase1-execute-token.json"
    output_root = root / "phase1-output"
    dry_log = root / "phase1-dry-run.log"
    execute_log = root / "phase1-execute.log"
    for path in (inputs_path, dry_root, token_path, output_root, dry_log, execute_log):
        require(not os.path.lexists(path), f"refusing to reuse Phase-1 output: {path}")
    inputs = phase1_inputs(context)
    write_new(inputs_path, inputs)
    inputs_sha256 = sha256_file(inputs_path)
    if context["fixtureOnly"]:
        fixture_phase1(
            context, inputs_path, inputs_sha256, dry_root, token_path, output_root
        )
    else:
        tool = context["tools"]["phase1IngressClosure"]
        environment = command_environment(context)
        require(sha256_file(tool["path"]) == tool["sha256"], "Phase-1 tool drifted")
        run_command(
            [sys.executable, str(tool["path"])],
            [
                "--dry-run",
                "--output-root",
                str(dry_root),
                "--inputs-file",
                str(inputs_path),
                "--expected-inputs-sha256",
                inputs_sha256,
            ],
            dry_log,
            environment,
        )
        dry = validate_dry_evidence(
            context, dry_root / "dry-run-evidence.json", inputs_sha256
        )
        token = create_execute_token(context, inputs_sha256, dry["pins"])
        write_new(token_path, token)
        require(sha256_file(tool["path"]) == tool["sha256"], "Phase-1 tool drifted after dry-run")
        run_command(
            [sys.executable, str(tool["path"])],
            [
                "--execute",
                "--output-root",
                str(output_root),
                "--inputs-file",
                str(inputs_path),
                "--expected-inputs-sha256",
                inputs_sha256,
                "--execute-token",
                str(token_path),
                "--expected-execute-token-sha256",
                sha256_file(token_path),
            ],
            execute_log,
            environment,
        )
    validate_execute_evidence(
        context, output_root / "execute-evidence.json", inputs_sha256
    )
    post_identity = context.get("postDeployIdentity")
    require(
        isinstance(post_identity, dict),
        "site post-deploy identity was not bound before Phase-1",
    )
    revalidated_identity = validate_post_deploy_identity(
        context, context["postDeployIdentityPath"]
    )
    require(
        revalidated_identity["sha256"] == post_identity["sha256"],
        "site post-deploy identity drifted during Phase-1",
    )
    require(stat.S_IMODE(token_path.stat().st_mode) & 0o077 == 0, "Phase-1 execute token is not private")
    return {name: True for name in sorted(CLOSE_CHECKS)}


def write_stage_result(context: Mapping[str, Any], checks: Mapping[str, bool]) -> None:
    expected = DEPLOY_CHECKS if context["stage"] == "deploySiteIndexer" else CLOSE_CHECKS
    require(set(checks) == expected and all(checks.values()), "site stage checks are incomplete")
    value = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-replacement-workflow-stage-result",
        "operationId": context["operationId"],
        "releaseId": context["releaseId"],
        "siteReleaseVersion": context["siteReleaseVersion"],
        "sourceCommit": context["sourceCommit"],
        "planSha256": context["planSha256"],
        "workflowContractSha256": context["contractSha256"],
        "stage": context["stage"],
        "result": "passed",
        "fixtureOnly": context["fixtureOnly"],
        "mutationPerformed": True,
        "acceptanceStartFenceWritten": False,
        "checks": dict(checks),
        "completedAtUtc": dt.datetime.now(dt.timezone.utc)
        .replace(microsecond=0)
        .strftime("%Y-%m-%dT%H:%M:%SZ"),
    }
    require(set(value) == STAGE_RESULT_KEYS, "internal site stage result schema drifted")
    write_new(context["resultPath"], value)


def run(args: argparse.Namespace) -> None:
    result = Path(args.result)
    require(result.is_absolute(), "stage result path must be absolute")
    require(not os.path.lexists(result), "refusing to overwrite stage result")
    context = validate_inputs(args)
    checks = run_deploy(context) if args.stage == "deploySiteIndexer" else run_close(context)
    write_stage_result(context, checks)


def parser() -> argparse.ArgumentParser:
    value = argparse.ArgumentParser(description=__doc__)
    for name in (
        "plan",
        "plan-sha256",
        "workflow-contract",
        "workflow-contract-sha256",
        "automatic-restore-arm",
        "automatic-restore-arm-sha256",
        "stage",
        "workflow-state-root",
        "stage-state-root",
        "result",
    ):
        value.add_argument(f"--{name}", required=True)
    return value


def main(argv: Sequence[str] | None = None) -> int:
    try:
        run(parser().parse_args(argv))
    except (SiteStageError, OSError, subprocess.SubprocessError) as exc:
        print(f"pre_reset_site_workflow_stage: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
