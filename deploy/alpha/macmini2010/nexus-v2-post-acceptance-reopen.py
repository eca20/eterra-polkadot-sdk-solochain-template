#!/usr/bin/env python3
"""Plan and coordinate the Nexus V2 restricted post-acceptance reopen.

This coordinator owns no credentials.  It verifies the final release lock, the
acceptance-boundary receipt, the post-proof Phase-2 final seal, the inherited
Phase-2 internal-transport lease, and the immutable site/FPS release evidence
before it invokes three hash-pinned component drivers.  A failed open is closed
back to the Phase-1 transport boundary and the prior FPS deployment is restored;
chain state is never restored or modified here.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import importlib.util
import ipaddress
import json
import os
import re
import signal
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any, Callable, Mapping, Sequence


SCRIPT_PATH = Path(__file__).resolve()
REPO_ROOT = SCRIPT_PATH.parents[3]
RELEASE_LOCK_PATH = REPO_ROOT / "scripts/nexus-v2-private-alpha/release_lock.py"
SAFETY_TOOL_DIR = REPO_ROOT / "scripts/nexus-v2-private-alpha"
sys.path.insert(0, str(SAFETY_TOOL_DIR))
from deployment_secret_environment import child_environment  # noqa: E402

SHA_RE = re.compile(r"^[0-9a-f]{64}$")
NONZERO_SHA_RE = re.compile(r"^(?!0{64}$)[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
HASH256_RE = re.compile(r"^0x[0-9a-f]{64}$")
ID_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
HOST_RE = re.compile(r"^[A-Za-z0-9](?:[A-Za-z0-9.-]{0,251}[A-Za-z0-9])?$")
UTC_RE = re.compile(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$")

PLAN_KIND = "nexus-v2-private-alpha-post-acceptance-reopen-plan"
PLAN_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "releaseId",
    "siteReleaseVersion",
    "sourceCommit",
    "siteSourceCommit",
    "genesisHash",
    "createdAtUtc",
    "expiresAtUtc",
    "finalReleaseLock",
    "replacementLock",
    "acceptanceBoundaryReceipt",
    "phase2FinalSeal",
    "phase2BootstrapPrerequisite",
    "authorityManifest",
    "selectedDeploymentEnvironment",
    "selectedSiteDeploymentEnvironment",
    "caddyfiles",
    "drivers",
    "helpers",
    "network",
    "ports",
    "smoke",
    "runtimeAuthority",
    "indexerReadiness",
    "fullLoopIndexerActivationReceipt",
    "siteDeploymentIdentity",
    "sitePostPhase2DeploymentIdentity",
    "siteDeploymentCandidateManifest",
    "sitePhase1PostDeployIdentity",
    "siteRuntimeConfigNormalizer",
    "unityFpsCandidateManifest",
    "unityFpsDeploymentEnvironment",
    "phase2InternalTransportHandoff",
    "phase2InternalTransport",
    "sshHostPins",
    "emergencyClosure",
    "policy",
}
FILE_PIN_KEYS = {"path", "sha256"}
EXECUTABLE_PIN_KEYS = {"path", "sha256", "sourceCommit"}
CADDY_KEYS = {"normal", "phase1"}
COMPONENTS = ("chain-transport", "fps-server", "site-ingress")
SITE_ADOPTION_ACTIONS = frozenset({"open", "verify", "prepare-commit", "commit"})
NETWORK_KEYS = {"chainLanIp", "siteLanIp", "publicHostname"}
PORT_KEYS = {
    "chainRpc",
    "chainP2p",
    "media",
    "ipfsApi",
    "ipfsGateway",
    "authority",
    "siteHttp",
    "siteHttps",
}
PORTS = {
    "chainRpc": 9944,
    "chainP2p": 30333,
    "media": 4000,
    "ipfsApi": 5001,
    "ipfsGateway": 8080,
    "authority": 8787,
    "siteHttp": 80,
    "siteHttps": 443,
}
SMOKE_KEYS = {"mediaPath", "mediaSha256", "ipfsPath", "ipfsSha256"}
RUNTIME_AUTHORITY_KEYS = {
    "runtimeSpecVersion",
    "runtimeCodeSha256",
    "runtimeMetadataScaleSha256",
    "alphaAccess",
    "authorityEpochs",
    "proofPolicy",
    "storageCounts",
}
ALPHA_ACCESS_KEYS = {
    "mode",
    "ownerAccountId",
    "sourceKind",
    "sourceChainId",
    "sourceContract",
    "sourceEventId",
    "expiresAtUnix",
}
AUTHORITY_EPOCH_KEYS = {
    "gameId",
    "gameVersion",
    "modeId",
    "modeName",
    "serviceId",
    "authorityEpoch",
    "publicKey",
    "authorityConfigHash",
    "activeFrom",
    "activeUntil",
    "revoked",
}
PROOF_POLICY_KEYS = {
    "key",
    "policyHash",
    "active",
    "everActivated",
    "economicRealm",
    "practiceOnly",
    "rewardBudget",
}
RUNTIME_STORAGE_COUNTS = {
    "allowedSources": 1,
    "whitelist": 1,
    "processedSources": 1,
    "managers": 0,
    "authorityEpochs": 4,
    "rewardPolicies": 1,
    "rewardBudgets": 1,
    "rewardActivations": 1,
    "rewardEverActivated": 1,
}
SITE_DEPLOYMENT_IDENTITY_KEYS = {
    "schemaVersion",
    "kind",
    "releaseVersion",
    "siteSourceCommit",
    "composeFileSha256",
    "sourceContract",
    "images",
    "publications",
    "authorityStatus",
    "capturedAtUtc",
}
SITE_IMAGE_KEYS = {
    "service",
    "reference",
    "imageId",
    "runtimeConfigSha256",
    "resolvedComposeServiceSha256",
    "composeServiceConfigHash",
}
SITE_SOURCE_CONTRACT_KEYS = {
    "composeSha256",
    "candidateManifestSha256",
    "phase1PostDeployIdentitySha256",
    "runtimeNormalizerSha256",
    "fullLoopActivationReceiptSha256",
    "fullLoopActivationOverrideSha256",
    "fullLoopProjectionManifestSha256",
    "fullLoopActivationVerifierSha256",
}
SSH_HOST_PIN_KEYS = {"knownHosts", "manifest", "validator"}
INDEXER_READINESS_KEYS = {
    "releaseVersion",
    "sourceCommit",
    "privateAlphaAccessKeySha256",
    "projectionDirectory",
    "fullLoopAcceptanceTargetSha256",
    "readinessProjectionSha256",
    "healthReadySha256",
    "acceptanceReadinessSha256",
    "authorityVisibleBaseUrl",
    "activationReceiptSha256",
    "activationOverrideSha256",
    "projectionManifestSha256",
}
FULL_LOOP_ACCEPTANCE_TARGET_KEYS = {
    "schema",
    "release_id",
    "account_id",
    "readiness_sha256",
    "economic_evidence_sha256",
    "access_evidence_sha256",
    "driver_sha256",
    "snapshot_id",
    "snapshot_manifest_sha256",
    "content_version",
    "content_manifest_sha256",
    "content_fixture_sha256",
    "event_fixture_sha256",
    "runtime_spec_version",
    "genesis_hash",
    "runtime_metadata_sha256",
    "services",
    "catalog",
}
EMERGENCY_CLOSURE_KEYS = {
    "bundleRoot",
    "driver",
    "helpers",
    "libraries",
    "caddyfiles",
    "sshHostPins",
    "targets",
    "unityFpsDeploymentEnvironment",
    "fps",
}
EMERGENCY_TARGET_KEYS = {"chainHost", "chainUser", "siteHost", "siteUser"}
EMERGENCY_FPS_KEYS = {
    "candidateRoot",
    "candidateManifestSha256",
    "snapshotPath",
    "deploymentReceiptPath",
    "rollbackReceiptPath",
    "rollbackScript",
    "candidateVerifier",
    "receiptVerifier",
    "pinVerifier",
}
POLICY = {
    "privateAlphaOnly": True,
    "sourceRestrictedToSiteHost": True,
    "phase1BackendsRemainLoopbackOnly": True,
    "chainStateMutationAuthorized": False,
    "chainStateRollbackAuthorized": False,
    "paidOrPublicProductionActivationAuthorized": False,
    "exposedServices": ["authority", "chainRpc", "ipfsGateway", "media"],
    "forbiddenExposedPorts": [30333, 5001],
}

RESULT_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "planSha256",
    "releaseId",
    "siteReleaseVersion",
    "sourceCommit",
    "siteSourceCommit",
    "componentId",
    "action",
    "mode",
    "result",
    "mutationPerformed",
    "alreadyApplied",
    "finalReleaseLockSha256",
    "acceptanceBoundaryReceiptSha256",
    "phase2FinalSealSha256",
    "fpsAdoptionSealSha256",
    "driverSha256",
    "remoteMarkerSha256",
    "componentReceipt",
    "checks",
    "completedAtUtc",
}
CHECKS = {
    ("chain-transport", "preflight"): {
        "credentialsResolved",
        "sourcePinned",
        "phase1LoopbackPreserved",
        "systemdSocketProxyAvailable",
        "firewallDefaultDeny",
        "noForbiddenExposure",
    },
    ("chain-transport", "open"): {
        "sourcePinned",
        "finalSealPinned",
        "currentRuntimeAuthorityVerified",
        "loopbackBackendsPreserved",
        "restrictedProxyUnitsInstalled",
        "sourceFirewallRulesExact",
        "dedicatedNftGuardExact",
        "forbiddenPortsClosed",
        "localReadPathsHealthy",
        "coordinatorGuardStateValid",
    },
    ("chain-transport", "adopt"): {
        "sourcePinned",
        "finalSealPinned",
        "phase2TransportHandoffPinned",
        "inheritedLeaseExact",
        "currentRuntimeAuthorityVerified",
        "restrictedTransportVerified",
        "coordinatorWatchdogAdopted",
    },
    ("chain-transport", "verify"): {
        "sourcePinned",
        "finalSealPinned",
        "currentRuntimeAuthorityVerified",
        "loopbackBackendsPreserved",
        "restrictedProxyUnitsInstalled",
        "sourceFirewallRulesExact",
        "dedicatedNftGuardExact",
        "forbiddenPortsClosed",
        "localReadPathsHealthy",
        "coordinatorGuardStateValid",
    },
    ("chain-transport", "commit"): {
        "currentRuntimeAuthorityVerified",
        "restrictedTransportVerified",
        "siteIngressCommitTokenVerified",
        "coordinatorWatchdogDisarmed",
    },
    ("chain-transport", "close"): {
        "phase1LoopbackPreserved",
        "proxyUnitsAbsent",
        "reopenFirewallRulesAbsent",
        "dedicatedNftGuardAbsent",
        "forbiddenPortsClosed",
        "chainStateUntouched",
        "markerAnomalyHealed",
        "coordinatorWatchdogAbsent",
    },
    ("site-ingress", "preflight"): {
        "credentialsResolved",
        "sourcePinned",
        "phase1CaddyPinned",
        "candidateCaddyValidated",
        "siteFirewallPreserved",
        "loopbackServicesPrivate",
    },
    ("site-ingress", "open"): {
        "sourcePinned",
        "finalSealPinned",
        "fpsAdoptionSealPinned",
        "currentRuntimeAuthorityVerified",
        "deploymentIdentityExact",
        "authorityStatusesSafe",
        "normalCaddyPinned",
        "caddyReloaded",
        "upstreamReadPathsHealthy",
        "publicReadPathsHealthy",
        "unsafeEconomicRoutesDisabled",
        "loopbackServicesPrivate",
        "coordinatorGuardStateValid",
    },
    ("site-ingress", "verify"): {
        "sourcePinned",
        "finalSealPinned",
        "fpsAdoptionSealPinned",
        "currentRuntimeAuthorityVerified",
        "deploymentIdentityExact",
        "authorityStatusesSafe",
        "normalCaddyPinned",
        "caddyReloaded",
        "upstreamReadPathsHealthy",
        "publicReadPathsHealthy",
        "unsafeEconomicRoutesDisabled",
        "loopbackServicesPrivate",
        "coordinatorGuardStateValid",
    },
    ("site-ingress", "prepare-commit"): {
        "currentRuntimeAuthorityVerified",
        "deploymentIdentityExact",
        "authorityStatusesSafe",
        "restrictedIngressVerified",
        "fpsAdoptionSealPinned",
        "coordinatorWatchdogArmed",
    },
    ("site-ingress", "commit"): {
        "currentRuntimeAuthorityVerified",
        "deploymentIdentityExact",
        "authorityStatusesSafe",
        "restrictedIngressVerified",
        "fpsAdoptionSealPinned",
        "siteIngressPrepareTokenVerified",
        "coordinatorWatchdogDisarmed",
    },
    ("site-ingress", "close"): {
        "publicIngressFailClosed",
        "phase1WriteIngressClosed",
        "siteFirewallPreserved",
        "loopbackServicesPrivate",
        "markerAnomalyHealed",
        "coordinatorWatchdogAbsent",
    },
    ("fps-server", "preflight"): {
        "credentialsResolved",
        "sourcePinned",
        "candidatePinned",
        "environmentPinned",
        "sshPinsExact",
        "proxyContractPinned",
        "threeModesRequired",
        "rollbackContractPinned",
    },
    ("fps-server", "promote"): {
        "candidatePromoted",
        "deploymentReceiptPinned",
        "priorDeploymentCaptured",
        "siteLocalChainProxyExact",
        "servicesActive",
        "threeModesReady",
        "safetyExact",
    },
    ("fps-server", "verify"): {
        "deploymentReceiptPinned",
        "currentCandidateExact",
        "siteLocalChainProxyExact",
        "abilityDeathmatchReady",
        "extractionReady",
        "extractionBattleRoyaleReady",
        "immutableDeploymentReceipt",
        "safetyExact",
    },
    ("fps-server", "rollback"): {
        "fpsServicesStopped",
        "priorDeploymentRestored",
        "siteLocalChainProxyAbsent",
        "candidateNoLongerActive",
        "rollbackReceiptPinned",
    },
}
DRY_PREFLIGHT_CHECKS = {
    "chain-transport": {
        "credentialsResolved",
        "sourcePinned",
        "authorityArtifactsPinned",
        "systemdSocketProxyRequired",
        "firewallDefaultDenyRequired",
        "forbiddenExposureProhibited",
    },
    "site-ingress": {
        "credentialsResolved",
        "sourcePinned",
        "caddyArtifactsPinned",
        "candidateCaddyStaticContractValidated",
        "siteFirewallContractPinned",
        "loopbackPublicationContractPinned",
    },
    "fps-server": {
        "credentialsResolved",
        "sourcePinned",
        "candidatePinned",
        "environmentPinned",
        "sshPinsExact",
        "proxyContractPinned",
        "threeModesRequired",
        "rollbackContractPinned",
    },
}


def expected_checks(component: str, action: str, mode: str) -> set[str]:
    if mode == "dry-run":
        require(action == "preflight", "dry-run is valid only for preflight")
        return DRY_PREFLIGHT_CHECKS[component]
    return CHECKS[(component, action)]

EVIDENCE_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "planSha256",
    "releaseId",
    "siteReleaseVersion",
    "sourceCommit",
    "siteSourceCommit",
    "decision",
    "finalReleaseLockSha256",
    "acceptanceBoundaryReceiptSha256",
    "phase2FinalSealSha256",
    "fpsAdoptionSeal",
    "transport",
    "steps",
    "chainStateMutationPerformed",
    "chainStateRollbackPerformed",
    "paidOrPublicProductionActivationAuthorized",
    "completedAtUtc",
}

FPS_ADOPTION_SEAL_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "planSha256",
    "releaseId",
    "finalReleaseLockSha256",
    "candidateManifestSha256",
    "deploymentEnvironmentSha256",
    "deploymentReceipt",
    "promoteResult",
    "verifyResult",
    "paidOrPublicProductionActivationAuthorized",
    "capturedAtUtc",
    "expiresAtUtc",
}


class ReopenError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ReopenError(message)


def canonical_bytes(value: Mapping[str, Any]) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")


def duplicate_rejecting_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        require(key not in result, f"duplicate JSON field: {key}")
        result[key] = value
    return result


def read_json(path: Path, label: str) -> dict[str, Any]:
    require(path.is_absolute(), f"{label} path must be absolute")
    require(path.is_file() and not path.is_symlink(), f"{label} must be a regular file")
    try:
        value = json.loads(
            path.read_text(encoding="utf-8"), object_pairs_hook=duplicate_rejecting_object
        )
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ReopenError(f"invalid {label}") from exc
    require(isinstance(value, dict), f"{label} root must be an object")
    return value


def exact_keys(value: Any, keys: set[str], label: str) -> Mapping[str, Any]:
    require(isinstance(value, dict) and set(value) == keys, f"{label} closed schema mismatch")
    return value


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def ensure_sha(value: Any, label: str, *, nonzero: bool = False) -> str:
    pattern = NONZERO_SHA_RE if nonzero else SHA_RE
    require(isinstance(value, str) and pattern.fullmatch(value) is not None, f"invalid {label}")
    return value


def ensure_commit(value: Any, label: str) -> str:
    require(isinstance(value, str) and COMMIT_RE.fullmatch(value) is not None, f"invalid {label}")
    return value


def ensure_id(value: Any, label: str) -> str:
    require(isinstance(value, str) and ID_RE.fullmatch(value) is not None, f"invalid {label}")
    return value


def parse_utc(value: Any, label: str) -> dt.datetime:
    require(isinstance(value, str) and UTC_RE.fullmatch(value) is not None, f"invalid {label}")
    return dt.datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=dt.timezone.utc)


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).replace(microsecond=0).strftime("%Y-%m-%dT%H:%M:%SZ")


def file_pin(path_value: str, label: str, *, canonical_json: bool = False) -> dict[str, str]:
    path = Path(path_value)
    require(path.is_absolute() and path.is_file() and not path.is_symlink(), f"{label} is unavailable")
    path = path.resolve()
    if canonical_json:
        value = read_json(path, label)
        require(path.read_bytes() == canonical_bytes(value), f"{label} is not canonical JSON")
    return {"path": str(path), "sha256": sha256_file(path)}


def parse_environment(path: Path) -> dict[str, str]:
    result: dict[str, str] = {}
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", key):
            result[key] = value.strip().strip("'\"")
    return result


def build_indexer_readiness(
    activation: Mapping[str, Any],
    *,
    activation_receipt_sha256: str,
    release_version: str,
    site_source_commit: str,
) -> dict[str, Any]:
    projection = exact_keys(
        activation.get("projection"),
        {
            "hostPath",
            "containerPath",
            "manifestSha256",
            "readinessProjectionSha256",
            "targetSha256",
            "readinessEvidenceSha256",
            "economicEvidenceSha256",
            "accessEvidenceSha256",
            "driverSha256",
            "appendOnlyRuns",
        },
        "full-loop activation projection",
    )
    verification = exact_keys(
        activation.get("verification"),
        {
            "healthReady",
            "healthReadySha256",
            "authenticatedReadinessExact",
            "acceptanceReadinessSha256",
            "anonymousAccessDenied",
        },
        "full-loop activation verification",
    )
    read_model = exact_keys(
        activation.get("readModel"),
        {
            "siteLocalBaseUrl",
            "authorityVisibleBaseUrl",
            "healthReadyPath",
            "acceptanceReadinessPath",
        },
        "full-loop activation read-model route",
    )
    override = exact_keys(
        activation.get("activationOverride"),
        {"hostPath", "sha256"},
        "full-loop activation override",
    )
    require(
        activation.get("releaseVersion") == release_version
        and activation.get("siteSourceCommit") == site_source_commit,
        "full-loop activation release/source mismatch",
    )
    require(
        projection["containerPath"] == "/var/lib/eterra/full-loop"
        and projection["appendOnlyRuns"] is True
        and projection["targetSha256"] == activation.get("activationId"),
        "full-loop activation target/projection contract mismatch",
    )
    require(
        verification["healthReady"] is True
        and verification["authenticatedReadinessExact"] is True
        and verification["anonymousAccessDenied"] is True
        and verification["acceptanceReadinessSha256"]
        == projection["readinessProjectionSha256"],
        "full-loop activation exact-readiness proof is incomplete",
    )
    for value, label in (
        (activation_receipt_sha256, "activation receipt"),
        (activation.get("privateAlphaAccessKeySha256"), "private-Alpha access key"),
        (projection["targetSha256"], "activation target"),
        (projection["readinessProjectionSha256"], "readiness projection"),
        (projection["manifestSha256"], "projection manifest"),
        (verification["healthReadySha256"], "health readiness"),
        (verification["acceptanceReadinessSha256"], "acceptance readiness"),
        (override["sha256"], "activation override"),
    ):
        ensure_sha(value, f"{label} SHA-256", nonzero=True)
    return {
        "releaseVersion": release_version,
        "sourceCommit": site_source_commit,
        "privateAlphaAccessKeySha256": activation["privateAlphaAccessKeySha256"],
        "projectionDirectory": projection["containerPath"],
        "fullLoopAcceptanceTargetSha256": projection["targetSha256"],
        "readinessProjectionSha256": projection["readinessProjectionSha256"],
        "healthReadySha256": verification["healthReadySha256"],
        "acceptanceReadinessSha256": verification["acceptanceReadinessSha256"],
        "authorityVisibleBaseUrl": read_model["authorityVisibleBaseUrl"],
        "activationReceiptSha256": activation_receipt_sha256,
        "activationOverrideSha256": override["sha256"],
        "projectionManifestSha256": projection["manifestSha256"],
    }


def validate_pin(value: Any, label: str, *, executable: bool = False) -> dict[str, str]:
    keys = EXECUTABLE_PIN_KEYS if executable else FILE_PIN_KEYS
    pin = dict(exact_keys(value, keys, label))
    path = Path(pin["path"])
    require(path.is_absolute() and path.is_file() and not path.is_symlink(), f"{label} is unavailable")
    if executable:
        ensure_commit(pin["sourceCommit"], f"{label} source commit")
        require(os.access(path, os.X_OK), f"{label} is not executable")
    ensure_sha(pin["sha256"], f"{label} SHA-256")
    require(sha256_file(path) == pin["sha256"], f"{label} hash mismatch")
    return pin


def validate_pin_shape(value: Any, label: str, *, executable: bool = False) -> dict[str, str]:
    keys = EXECUTABLE_PIN_KEYS if executable else FILE_PIN_KEYS
    pin = dict(exact_keys(value, keys, label))
    require(isinstance(pin["path"], str) and Path(pin["path"]).is_absolute(), f"{label} path is invalid")
    ensure_sha(pin["sha256"], f"{label} SHA-256")
    if executable:
        ensure_commit(pin["sourceCommit"], f"{label} source commit")
    return pin


def validate_ssh_host_pins(
    value: Any,
    label: str,
    *,
    shape_only: bool = False,
) -> dict[str, dict[str, str]]:
    pins = exact_keys(value, SSH_HOST_PIN_KEYS, label)
    validator = validate_pin_shape if shape_only else validate_pin
    result = {
        "knownHosts": validator(pins["knownHosts"], f"{label} dedicated known_hosts"),
        "manifest": validator(pins["manifest"], f"{label} manifest"),
        "validator": validator(pins["validator"], f"{label} validator"),
    }
    for name, pin in result.items():
        require(
            re.fullmatch(r"/[A-Za-z0-9._/+:-]+", pin["path"]) is not None,
            f"{label} {name} path is unsafe for an OpenSSH option",
        )
    if shape_only:
        return result
    validator_path = Path(result["validator"]["path"])
    require(os.access(validator_path, os.X_OK), f"{label} validator is not executable")
    completed = subprocess.run(
        [
            sys.executable,
            str(validator_path),
            "verify",
            "--known-hosts",
            result["knownHosts"]["path"],
            "--manifest",
            result["manifest"]["path"],
        ],
        capture_output=True,
        text=True,
        check=False,
        timeout=30,
        env=child_environment(),
    )
    require(completed.returncode == 0, f"{label} validation failed")
    try:
        summary = json.loads(completed.stdout)
    except json.JSONDecodeError as exc:
        raise ReopenError(f"{label} validator returned invalid output") from exc
    require(
        isinstance(summary, dict)
        and summary.get("knownHostsSha256") == result["knownHosts"]["sha256"]
        and summary.get("manifestSha256") == result["manifest"]["sha256"],
        f"{label} validator summary mismatch",
    )
    return result


def safe_smoke_path(value: Any, prefix: str, label: str) -> str:
    require(isinstance(value, str) and value.startswith(prefix), f"invalid {label}")
    require("?" not in value and "#" not in value and "\\" not in value, f"invalid {label}")
    require(all(part not in {"", ".", ".."} for part in value.split("/")[1:]), f"invalid {label}")
    require(len(value) <= 1024, f"{label} is too long")
    return value


def validate_runtime_authority(value: Any) -> dict[str, Any]:
    authority = dict(exact_keys(value, RUNTIME_AUTHORITY_KEYS, "runtime authority"))
    require(authority["runtimeSpecVersion"] == 106, "runtime authority spec-version mismatch")
    ensure_sha(authority["runtimeCodeSha256"], "runtime authority code SHA-256", nonzero=True)
    ensure_sha(
        authority["runtimeMetadataScaleSha256"],
        "runtime authority metadata SHA-256",
        nonzero=True,
    )
    access = exact_keys(authority["alphaAccess"], ALPHA_ACCESS_KEYS, "runtime AlphaAccess")
    require(
        access["mode"] == "Enforced"
        and access["sourceKind"] == "ManualAdmin"
        and access["sourceChainId"] == 0
        and access["sourceContract"] == "0x" + "00" * 20,
        "runtime AlphaAccess authority is not narrowly enforced",
    )
    for field in ("ownerAccountId", "sourceEventId"):
        require(
            isinstance(access[field], str) and HASH256_RE.fullmatch(access[field]) is not None,
            f"runtime AlphaAccess {field} is invalid",
        )
    require(
        isinstance(access["expiresAtUnix"], int)
        and not isinstance(access["expiresAtUnix"], bool)
        and access["expiresAtUnix"] > 0,
        "runtime AlphaAccess expiry is invalid",
    )
    epochs = authority["authorityEpochs"]
    require(isinstance(epochs, list) and len(epochs) == 4, "runtime authority epoch count mismatch")
    expected_modes = [(1005, 1, 1), (1005, 1, 2), (1005, 1, 3), (1006, 1, 1)]
    observed_modes: list[tuple[int, int, int]] = []
    for index, epoch in enumerate(epochs):
        item = exact_keys(epoch, AUTHORITY_EPOCH_KEYS, f"runtime authority epoch {index}")
        observed_modes.append((item["gameId"], item["gameVersion"], item["modeId"]))
        require(item["serviceId"] in {"eterra-fps-authority", "eterra-legends-authority"}, "runtime authority service is invalid")
        require(
            isinstance(item["authorityEpoch"], int)
            and not isinstance(item["authorityEpoch"], bool)
            and item["authorityEpoch"] > 0,
            "runtime authority epoch is invalid",
        )
        for field in ("publicKey", "authorityConfigHash"):
            require(isinstance(item[field], str) and HASH256_RE.fullmatch(item[field]) is not None, f"runtime authority {field} is invalid")
        require(
            isinstance(item["activeFrom"], int)
            and isinstance(item["activeUntil"], int)
            and 0 < item["activeFrom"] < item["activeUntil"] <= 0xFFFFFFFF
            and item["revoked"] is False,
            "runtime authority activation window is invalid",
        )
    require(observed_modes == expected_modes, "runtime authority mode set/order mismatch")
    policy = exact_keys(authority["proofPolicy"], PROOF_POLICY_KEYS, "runtime proof policy")
    require(policy["key"] == [1005, 1, 1, 0xFFFFFFFE], "runtime proof-policy key mismatch")
    require(
        isinstance(policy["policyHash"], str)
        and HASH256_RE.fullmatch(policy["policyHash"]) is not None,
        "runtime proof-policy hash is invalid",
    )
    require(
        policy["active"] is False
        and policy["everActivated"] is True
        and policy["economicRealm"] == "Training"
        and policy["practiceOnly"] is True,
        "runtime proof policy is not safely deactivated Training state",
    )
    budget = policy["rewardBudget"]
    require(isinstance(budget, dict) and budget, "runtime proof-policy reward budget is invalid")
    require(
        all(isinstance(name, str) and str(amount) == "0" for name, amount in budget.items()),
        "runtime proof-policy reward budget is not zero",
    )
    require(authority["storageCounts"] == RUNTIME_STORAGE_COUNTS, "runtime authority storage counts mismatch")
    return authority


def build_runtime_authority(
    prerequisite: Mapping[str, Any],
    manifest: Mapping[str, Any],
    seal: Mapping[str, Any],
) -> dict[str, Any]:
    require(
        seal["artifacts"]["bootstrap_prerequisite_sha256"]
        == prerequisite["__artifact_sha256"],
        "Phase-2 seal does not pin the selected bootstrap prerequisite",
    )
    require(
        seal["authority_manifest"]["sha256"] == manifest["__artifact_sha256"],
        "Phase-2 seal does not pin the selected authority manifest",
    )
    prerequisite = {key: value for key, value in prerequisite.items() if key != "__artifact_sha256"}
    manifest = {key: value for key, value in manifest.items() if key != "__artifact_sha256"}
    authority = prerequisite.get("authority")
    alpha = prerequisite.get("alpha_access")
    proof = prerequisite.get("proof_policy")
    require(isinstance(authority, dict) and isinstance(alpha, dict) and isinstance(proof, dict), "bootstrap prerequisite authority contract is incomplete")
    service_pins = manifest.get("service_pins")
    registrations = manifest.get("registrations")
    require(isinstance(service_pins, list) and isinstance(registrations, list), "authority manifest registrations are incomplete")
    configurations: dict[str, str] = {}
    for pin in service_pins:
        require(isinstance(pin, dict), "authority service pin is invalid")
        service_id = pin.get("service_id")
        configuration = pin.get("configuration_sha256")
        require(
            service_id in {"eterra-fps-authority", "eterra-legends-authority"}
            and isinstance(configuration, str)
            and SHA_RE.fullmatch(configuration) is not None,
            "authority service configuration pin is invalid",
        )
        require(service_id not in configurations, "duplicate authority service pin")
        configurations[service_id] = "0x" + configuration
    require(set(configurations) == {"eterra-fps-authority", "eterra-legends-authority"}, "authority service pin set mismatch")
    prerequisite_registrations = authority.get("registrations")
    require(
        isinstance(prerequisite_registrations, list)
        and len(prerequisite_registrations) == 4
        and len(registrations) == 4,
        "bootstrap/manifest authority registration count mismatch",
    )
    epochs: list[dict[str, Any]] = []
    for index, registration in enumerate(registrations):
        require(isinstance(registration, dict), "authority registration is invalid")
        service_id = registration.get("service_id")
        prerequisite_registration = prerequisite_registrations[index]
        require(isinstance(prerequisite_registration, dict), "bootstrap authority registration is invalid")
        require(
            {
                key: registration.get(key)
                for key in ("game_id", "game_version", "mode_id", "mode_name", "service_id")
            }
            == {
                key: prerequisite_registration.get(key)
                for key in ("game_id", "game_version", "mode_id", "mode_name", "service_id")
            }
            and registration.get("economic_realm") == prerequisite_registration.get("economic_realm") == "Training"
            and registration.get("authority_epoch") == authority.get("authority_epoch")
            and str(registration.get("public_key_hex", "")).lower()
            == str(authority.get("public_key_hex", "")).lower()
            and int(registration.get("active_from_block")) == authority.get("active_from_block")
            and int(registration.get("active_until_block")) == authority.get("active_until_block")
            and registration.get("revoked") is False
            and configurations.get(str(service_id))
            == str(prerequisite_registration.get("authority_config_hash_hex", "")).lower(),
            f"authority manifest registration {index} differs from bootstrap prerequisite",
        )
        epochs.append(
            {
                "gameId": registration.get("game_id"),
                "gameVersion": registration.get("game_version"),
                "modeId": registration.get("mode_id"),
                "modeName": registration.get("mode_name"),
                "serviceId": service_id,
                "authorityEpoch": registration.get("authority_epoch"),
                "publicKey": str(registration.get("public_key_hex", "")).lower(),
                "authorityConfigHash": configurations.get(str(service_id), ""),
                "activeFrom": int(registration.get("active_from_block")),
                "activeUntil": int(registration.get("active_until_block")),
                "revoked": registration.get("revoked"),
            }
        )
    value = {
        "runtimeSpecVersion": seal["target"]["runtime_spec_version"],
        "runtimeCodeSha256": seal["target"]["runtime_code_sha256"],
        "runtimeMetadataScaleSha256": seal["target"]["runtime_metadata_scale_sha256"],
        "alphaAccess": {
            "mode": seal["alpha_access"]["mode"],
            "ownerAccountId": seal["alpha_access"]["owner_account_id"],
            "sourceKind": seal["alpha_access"]["source_kind"],
            "sourceChainId": alpha.get("source_chain_id"),
            "sourceContract": alpha.get("source_contract_hex"),
            "sourceEventId": seal["alpha_access"]["source_event_id"],
            "expiresAtUnix": seal["alpha_access"]["expires_at_unix"],
        },
        "authorityEpochs": epochs,
        "proofPolicy": {
            "key": seal["proof_policy"]["key"],
            "policyHash": seal["proof_policy"]["policy_hash"],
            "active": seal["proof_policy"]["active"],
            "everActivated": True,
            "economicRealm": proof.get("economic_realm"),
            "practiceOnly": proof.get("practice_only"),
            "rewardBudget": proof.get("budget"),
        },
        "storageCounts": dict(RUNTIME_STORAGE_COUNTS),
    }
    return validate_runtime_authority(value)


def validate_indexer_readiness(value: Any, plan: Mapping[str, Any]) -> dict[str, Any]:
    readiness = dict(exact_keys(value, INDEXER_READINESS_KEYS, "indexer readiness authority"))
    require(
        readiness["releaseVersion"] == plan["siteReleaseVersion"]
        and readiness["sourceCommit"] == plan["siteSourceCommit"],
        "indexer release identity mismatch",
    )
    for field in (
        "privateAlphaAccessKeySha256",
        "fullLoopAcceptanceTargetSha256",
        "readinessProjectionSha256",
        "healthReadySha256",
        "acceptanceReadinessSha256",
        "activationReceiptSha256",
        "activationOverrideSha256",
        "projectionManifestSha256",
    ):
        ensure_sha(readiness[field], f"indexer {field}", nonzero=True)
    require(
        readiness["projectionDirectory"] == "/var/lib/eterra/full-loop",
        "indexer projection directory is not the exact read-only container mount",
    )
    require(
        readiness["activationReceiptSha256"]
        == plan["fullLoopIndexerActivationReceipt"]["sha256"],
        "indexer readiness does not bind the final-lock activation receipt",
    )
    require(
        readiness["fullLoopAcceptanceTargetSha256"]
        == read_json(
            Path(plan["fullLoopIndexerActivationReceipt"]["path"]),
            "full-loop activation receipt",
        ).get("activationId"),
        "indexer readiness target differs from the verified activation",
    )
    require(
        readiness["authorityVisibleBaseUrl"]
        == f"https://{plan['network']['publicHostname']}/nexus-api",
        "indexer authority-visible URL differs from locked HOME_HOSTNAME",
    )
    return readiness


def validate_site_deployment_identity(
    value: Any,
    plan: Mapping[str, Any],
    *,
    verify_source_artifact: bool = True,
) -> dict[str, Any]:
    identity = dict(exact_keys(value, SITE_DEPLOYMENT_IDENTITY_KEYS, "site deployment identity"))
    require(
        identity["schemaVersion"] == 1
        and identity["kind"] == "nexus-v2-private-alpha-site-deployment-identity",
        "site deployment identity type mismatch",
    )
    require(identity["releaseVersion"] == plan["siteReleaseVersion"], "site deployment identity release mismatch")
    require(identity["siteSourceCommit"] == plan["siteSourceCommit"], "site deployment identity source mismatch")
    ensure_sha(identity["composeFileSha256"], "site Compose SHA-256", nonzero=True)
    source_contract = exact_keys(
        identity["sourceContract"],
        SITE_SOURCE_CONTRACT_KEYS,
        "site deployment source contract",
    )
    for field, label in (
        ("composeSha256", "source Compose SHA-256"),
        ("candidateManifestSha256", "site candidate-manifest SHA-256"),
        ("phase1PostDeployIdentitySha256", "Phase-1 site identity SHA-256"),
        ("runtimeNormalizerSha256", "runtime normalizer SHA-256"),
    ):
        ensure_sha(source_contract[field], label, nonzero=True)
    require(
        source_contract
        == {
            "composeSha256": identity["composeFileSha256"],
            "candidateManifestSha256": plan["siteDeploymentCandidateManifest"]["sha256"],
            "phase1PostDeployIdentitySha256": plan["sitePhase1PostDeployIdentity"]["sha256"],
            "runtimeNormalizerSha256": plan["siteRuntimeConfigNormalizer"]["sha256"],
            "fullLoopActivationReceiptSha256": plan["fullLoopIndexerActivationReceipt"]["sha256"],
            "fullLoopActivationOverrideSha256": plan["indexerReadiness"]["activationOverrideSha256"],
            "fullLoopProjectionManifestSha256": plan["indexerReadiness"]["projectionManifestSha256"],
            "fullLoopActivationVerifierSha256": source_contract["fullLoopActivationVerifierSha256"],
        },
        "site deployment source contract is not independently pinned",
    )
    if verify_source_artifact:
        lock = read_json(Path(plan["finalReleaseLock"]["path"]), "final release lock")
        activation_verifier = (
            Path(lock["repositories"]["web"]["root"])
            / "tcg/deploy/alpha/macmini2014/nexus_v2_full_loop_activation_contract.py"
        )
        require(
            activation_verifier.is_file()
            and not activation_verifier.is_symlink()
            and source_contract["fullLoopActivationVerifierSha256"]
            == sha256_file(activation_verifier),
            "site deployment activation-verifier source pin is stale",
        )
    parse_utc(identity["capturedAtUtc"], "site deployment identity capture time")
    images = identity["images"]
    require(isinstance(images, list) and len(images) == 4, "site deployment image count mismatch")
    observed_services: set[str] = set()
    for image in images:
        exact_keys(image, SITE_IMAGE_KEYS, "site deployment image")
        service = image["service"]
        require(service in {"site", "indexer-api", "mongo", "caddy"} and service not in observed_services, "site deployment image service mismatch")
        observed_services.add(service)
        require(isinstance(image["reference"], str) and image["reference"], "site deployment image reference is invalid")
        require(isinstance(image["imageId"], str) and re.fullmatch(r"sha256:[0-9a-f]{64}", image["imageId"]) is not None, "site deployment image ID is invalid")
        ensure_sha(
            image["runtimeConfigSha256"],
            f"{service} runtime configuration SHA-256",
            nonzero=True,
        )
        ensure_sha(
            image["resolvedComposeServiceSha256"],
            f"{service} resolved Compose service SHA-256",
            nonzero=True,
        )
        ensure_sha(
            image["composeServiceConfigHash"],
            f"{service} Compose service configuration hash",
            nonzero=True,
        )
    require(observed_services == {"site", "indexer-api", "mongo", "caddy"}, "site deployment image set mismatch")
    require(
        [image["service"] for image in images]
        == ["caddy", "indexer-api", "mongo", "site"],
        "site deployment images are not in canonical service order",
    )
    publications = exact_keys(identity["publications"], observed_services, "site deployment publications")
    require(sorted(publications["site"]) == ["127.0.0.1:3000:3000/tcp"], "site publication is not exact loopback")
    require(sorted(publications["indexer-api"]) == ["127.0.0.1:8787:8787/tcp"], "indexer publication is not exact loopback")
    require(publications["mongo"] == [], "Mongo must not publish a host port")
    caddy_publications = set(publications["caddy"])
    required_caddy = {"0.0.0.0:80:80/tcp", "0.0.0.0:443:443/tcp"}
    allowed_caddy = required_caddy | {":::80:80/tcp", ":::443:443/tcp"}
    require(
        required_caddy <= caddy_publications <= allowed_caddy
        and len(caddy_publications) == len(publications["caddy"]),
        "Caddy publication contract mismatch",
    )
    statuses = exact_keys(identity["authorityStatus"], {"fps", "legends"}, "authority status identity")
    runtime = plan["runtimeAuthority"]
    config_by_service = {entry["serviceId"]: entry["authorityConfigHash"] for entry in runtime["authorityEpochs"]}
    fps = exact_keys(statuses["fps"], {"sourceEndpoint", "sourceDocumentSha256", "ok", "signerAvailable", "authorityStateAvailable", "runtimeDerivesRewards", "privateAlphaOnly", "paidEntry", "wagering", "permanentAssetLoss", "publicProduction", "authorityConfigHash"}, "FPS authority status")
    ensure_sha(fps["sourceDocumentSha256"], "FPS source document SHA-256", nonzero=True)
    require(fps == {**fps, "sourceEndpoint": "http://127.0.0.1:8787/v1/fps/status", "ok": True, "signerAvailable": True, "authorityStateAvailable": True, "runtimeDerivesRewards": True, "privateAlphaOnly": True, "paidEntry": False, "wagering": False, "permanentAssetLoss": False, "publicProduction": False, "authorityConfigHash": config_by_service["eterra-fps-authority"]}, "FPS authority status is unsafe or drifted")
    legends = exact_keys(statuses["legends"], {"sourceEndpoint", "sourceDocumentSha256", "ok", "gameId", "gameVersion", "modeId", "signerAvailable", "authorityStateAvailable", "encounterCatalogAvailable", "ownerAuthorizationAvailable", "resultJournalAvailable", "runtimeDerivesRewards", "authorityConfigHash"}, "Legends authority status")
    ensure_sha(legends["sourceDocumentSha256"], "Legends source document SHA-256", nonzero=True)
    require(legends == {**legends, "sourceEndpoint": "http://127.0.0.1:8787/v1/eterra-legends/status", "ok": True, "gameId": 1006, "gameVersion": 1, "modeId": 1, "signerAvailable": True, "authorityStateAvailable": True, "encounterCatalogAvailable": True, "ownerAuthorizationAvailable": True, "resultJournalAvailable": True, "runtimeDerivesRewards": True, "authorityConfigHash": config_by_service["eterra-legends-authority"]}, "Legends authority status is unsafe or drifted")
    return identity


def validate_phase2_transport_handoff(
    value: Any, plan: Mapping[str, Any]
) -> dict[str, Any]:
    handoff = dict(
        exact_keys(
            value,
            {
                "schemaVersion",
                "kind",
                "releaseId",
                "siteReleaseVersion",
                "sourceCommit",
                "siteSourceCommit",
                "acceptanceBoundaryReceiptSha256",
                "replacementLockSha256",
                "sitePhase1PostDeployIdentitySha256",
                "sitePostPhase2DeploymentIdentitySha256",
                "network",
                "ports",
                "lease",
                "phase2",
                "safety",
                "capturedAtUtc",
            },
            "Phase-2 internal transport handoff",
        )
    )
    require(
        handoff["schemaVersion"] == 1
        and handoff["kind"]
        == "nexus-v2-private-alpha-phase2-internal-transport-handoff",
        "Phase-2 transport handoff identity mismatch",
    )
    require(
        handoff["releaseId"] == plan["releaseId"]
        and handoff["siteReleaseVersion"] == plan["siteReleaseVersion"]
        and handoff["sourceCommit"] == plan["sourceCommit"]
        and handoff["siteSourceCommit"] == plan["siteSourceCommit"]
        and handoff["acceptanceBoundaryReceiptSha256"]
        == plan["acceptanceBoundaryReceipt"]["sha256"],
        "Phase-2 transport handoff release/source mismatch",
    )
    require(
        handoff["replacementLockSha256"] == plan["replacementLock"]["sha256"],
        "Phase-2 transport handoff is not bound to the pinned replacement lock",
    )
    require(
        handoff["sitePhase1PostDeployIdentitySha256"]
        == plan["sitePhase1PostDeployIdentity"]["sha256"]
        and handoff["sitePostPhase2DeploymentIdentitySha256"]
        == plan["sitePostPhase2DeploymentIdentity"]["sha256"],
        "Phase-2 transport handoff is not bound to the pinned site identities",
    )
    require(
        handoff["network"]
        == {
            "chainLanIp": plan["network"]["chainLanIp"],
            "siteLanIp": plan["network"]["siteLanIp"],
            "allowedSourceIp": plan["network"]["siteLanIp"],
        },
        "Phase-2 transport handoff network mismatch",
    )
    require(
        handoff["ports"]
        == {
            "chainRpc": 9944,
            "authority": 8787,
            "media": 4000,
            "ipfsGateway": 8080,
            "forbidden": [30333, 5001],
        },
        "Phase-2 transport handoff port contract mismatch",
    )
    lease = exact_keys(
        handoff["lease"],
        {
            "operationId",
            "planSha256",
            "markerPath",
            "markerSha256",
            "heartbeatPath",
            "heartbeatNonce",
            "watchdogService",
            "watchdogTimer",
            "watchdogUnitSha256",
            "watchdogPayloadSha256",
            "armed",
            "expiresAtUtc",
        },
        "Phase-2 transport lease",
    )
    ensure_id(lease["operationId"], "Phase-2 transport operation ID")
    require(
        lease["operationId"] == plan["operationId"],
        "reopen operation must adopt the exact Phase-2 transport operation",
    )
    for field in (
        "planSha256",
        "markerSha256",
        "watchdogUnitSha256",
        "watchdogPayloadSha256",
    ):
        ensure_sha(lease[field], f"Phase-2 transport {field}", nonzero=True)
    require(
        isinstance(lease["markerPath"], str)
        and lease["markerPath"].startswith(
            "/opt/eterra-alpha/shared/phase2-internal-transport/"
        )
        and ".." not in lease["markerPath"],
        "Phase-2 transport marker path is unsafe",
    )
    require(
        isinstance(lease["heartbeatPath"], str)
        and lease["heartbeatPath"].startswith(
            "/opt/eterra-alpha/shared/phase2-internal-transport/"
        )
        and ".." not in lease["heartbeatPath"]
        and isinstance(lease["heartbeatNonce"], str)
        and re.fullmatch(r"[0-9a-f]{32,128}", lease["heartbeatNonce"]),
        "Phase-2 transport heartbeat identity is unsafe",
    )
    for field in ("watchdogService", "watchdogTimer"):
        require(
            isinstance(lease[field], str)
            and re.fullmatch(
                r"nexus-v2-phase2-internal-transport-[A-Za-z0-9_.@-]+",
                lease[field],
            ),
            f"Phase-2 transport {field} is invalid",
        )
    require(lease["armed"] is True, "Phase-2 transport watchdog is not armed")
    lease_expiry = parse_utc(lease["expiresAtUtc"], "Phase-2 transport lease expiry")
    require(
        lease_expiry >= parse_utc(plan["expiresAtUtc"], "reopen plan expiry"),
        "Phase-2 transport lease expires before the reopen plan",
    )
    require(
        handoff["phase2"]
        == {
            "publicIngressClosed": True,
            "siteIndexerSynchronized": True,
            "authorityReady": True,
            "fullLoopActivationReceiptSha256": plan["fullLoopIndexerActivationReceipt"]["sha256"],
        },
        "Phase-2 transport proof is incomplete",
    )
    require(
        handoff["safety"]
        == {
            "chainStateMutationAuthorized": False,
            "paidOrPublicActivationAuthorized": False,
            "sourceRestricted": True,
            "loopbackBackendsPreserved": True,
            "forbiddenPortsClosed": True,
        },
        "Phase-2 transport handoff safety mismatch",
    )
    parse_utc(handoff["capturedAtUtc"], "Phase-2 transport capture time")
    return handoff


def run_official_phase2_transport_handoff_validator(plan: Mapping[str, Any]) -> None:
    lock = read_json(Path(plan["finalReleaseLock"]["path"]), "final release lock")
    verifier = (
        Path(lock["repositories"]["chain"]["root"])
        / "scripts/nexus-v2-private-alpha/phase2_internal_transport.py"
    )
    require(
        verifier.is_file() and not verifier.is_symlink(),
        "official Phase-2 transport handoff verifier is unavailable",
    )
    command = [
        sys.executable,
        str(verifier),
        "verify-handoff",
        "--handoff",
        plan["phase2InternalTransportHandoff"]["path"],
        "--expected-handoff-sha256",
        plan["phase2InternalTransportHandoff"]["sha256"],
        "--replacement-lock",
        plan["replacementLock"]["path"],
        "--expected-replacement-lock-sha256",
        plan["replacementLock"]["sha256"],
        "--acceptance-boundary-receipt",
        plan["acceptanceBoundaryReceipt"]["path"],
        "--expected-acceptance-boundary-receipt-sha256",
        plan["acceptanceBoundaryReceipt"]["sha256"],
        "--site-phase1-post-deploy-identity",
        plan["sitePhase1PostDeployIdentity"]["path"],
        "--expected-site-phase1-post-deploy-identity-sha256",
        plan["sitePhase1PostDeployIdentity"]["sha256"],
        "--full-loop-indexer-activation-receipt",
        plan["fullLoopIndexerActivationReceipt"]["path"],
        "--expected-full-loop-indexer-activation-receipt-sha256",
        plan["fullLoopIndexerActivationReceipt"]["sha256"],
        "--site-post-phase2-deployment-identity",
        plan["sitePostPhase2DeploymentIdentity"]["path"],
        "--expected-site-post-phase2-deployment-identity-sha256",
        plan["sitePostPhase2DeploymentIdentity"]["sha256"],
        "--selected-deployment-environment",
        plan["selectedDeploymentEnvironment"]["path"],
        "--selected-site-deployment-environment",
        plan["selectedSiteDeploymentEnvironment"]["path"],
    ]
    completed = subprocess.run(
        command,
        capture_output=True,
        text=True,
        check=False,
        timeout=120,
        env=child_environment(),
    )
    require(
        completed.returncode == 0,
        "official Phase-2 transport handoff validation failed",
    )


def load_release_lock_module() -> Any:
    require(
        (REPO_ROOT / "Cargo.toml").is_file()
        and (REPO_ROOT / "deploy/alpha/macmini2010").is_dir()
        and RELEASE_LOCK_PATH.is_file(),
        "restricted reopen source root is invalid",
    )
    spec = importlib.util.spec_from_file_location("nexus_v2_release_lock_for_reopen", RELEASE_LOCK_PATH)
    require(spec is not None and spec.loader is not None, "cannot load release-lock verifier")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def validate_phase2_seal_shape(
    seal: Mapping[str, Any], receipt: Mapping[str, Any], lock: Mapping[str, Any]
) -> None:
    exact_keys(
        seal,
        {
            "schema",
            "generated_at_utc",
            "environment",
            "status",
            "target",
            "source",
            "artifacts",
            "authority_manifest",
            "proof_baseline",
            "proof_policy",
            "alpha_access",
            "safety",
        },
        "Phase-2 final seal",
    )
    require(seal["schema"] == "eterra.nexus-v2-runtime-seeder-phase2-final-seal.v1", "Phase-2 final-seal schema mismatch")
    require(seal["environment"] == "private_alpha" and seal["status"] == "post_proof_finalized", "Phase-2 final seal is not finalized private Alpha evidence")
    parse_utc(seal["generated_at_utc"], "Phase-2 final-seal time")
    target = exact_keys(
        seal["target"],
        {
            "release_id",
            "source_commit",
            "genesis_hash",
            "runtime_code_sha256",
            "runtime_metadata_scale_sha256",
            "runtime_spec_version",
            "target_identity_sha256",
            "acceptance_boundary_sha256",
        },
        "Phase-2 final-seal target",
    )
    target_pin = lock["artifacts"]["targetIdentity"]
    expected = {
        "release_id": receipt["releaseId"],
        "source_commit": receipt["sourceCommit"],
        "genesis_hash": receipt["genesisHash"],
        "runtime_code_sha256": receipt["runtimeCodeSha256"],
        "runtime_metadata_scale_sha256": receipt["runtimeMetadataScaleSha256"],
        "runtime_spec_version": 106,
        "target_identity_sha256": target_pin["sha256"],
        "acceptance_boundary_sha256": lock["artifacts"]["acceptanceBoundaryReceipt"]["sha256"],
    }
    require(dict(target) == expected, "Phase-2 final seal does not bind the final release lock and acceptance receipt")
    artifacts = exact_keys(
        seal["artifacts"],
        {
            "bootstrap_prerequisite_sha256",
            "bootstrap_finalized_evidence_sha256",
            "bootstrap_journal_sha256",
            "pre_deactivation_proof_sha256",
            "proof_run_handoff_sha256",
            "deactivation_evidence_sha256",
            "fps_acceptance_proof_sha256",
        },
        "Phase-2 final-seal artifacts",
    )
    for name, digest in artifacts.items():
        ensure_sha(digest, f"Phase-2 final-seal {name}", nonzero=True)
    authority = exact_keys(
        seal["authority_manifest"],
        {"schema", "sha256", "fixture_only", "registrations"},
        "Phase-2 authority manifest",
    )
    require(
        authority == {
            "schema": "eterra.authority-registration-manifest.v1",
            "sha256": authority["sha256"],
            "fixture_only": False,
            "registrations": 4,
        },
        "Phase-2 authority manifest is not the final non-fixture four-registration manifest",
    )
    ensure_sha(authority["sha256"], "Phase-2 authority manifest SHA-256", nonzero=True)
    policy = exact_keys(
        seal["proof_policy"],
        {"key", "policy_hash", "active", "extra_deactivated_policy_count", "pre_deactivation_active"},
        "Phase-2 proof policy",
    )
    require(policy["key"] == [1005, 1, 1, 0xFFFFFFFE], "Phase-2 proof-policy key mismatch")
    require(policy["active"] is False and policy["pre_deactivation_active"] is True and policy["extra_deactivated_policy_count"] == 1, "Phase-2 proof policy was not deactivated")
    access = exact_keys(
        seal["alpha_access"],
        {"mode", "owner_account_id", "source_kind", "source_event_id", "expires_at_unix", "grant_count"},
        "Phase-2 AlphaAccess",
    )
    require(access["mode"] == "Enforced" and access["source_kind"] == "ManualAdmin" and access["grant_count"] == 1, "Phase-2 AlphaAccess is not narrowly enforced")
    safety = {
        "alpha_access_mode": "Enforced",
        "bootstrap_only": False,
        "canonical_seed_eligible": True,
        "economically_valued_rewards": False,
        "marketplace": False,
        "paid_entry": False,
        "permanent_asset_loss": False,
        "private_alpha_only": True,
        "proof_policy_active": False,
        "public_production": False,
        "transfers": False,
        "wagering": False,
    }
    require(seal["safety"] == safety, "Phase-2 final-seal safety contract mismatch")


def run_official_phase2_validator(plan: Mapping[str, Any], lock: Mapping[str, Any]) -> None:
    web_root = Path(lock["repositories"]["web"]["root"])
    chain_root = Path(lock["repositories"]["chain"]["root"])
    expected_driver = (
        chain_root
        / "deploy/alpha/macmini2010/nexus-v2-post-acceptance-reopen-component-driver"
    ).resolve()
    expected_helpers = {
        "chain-transport": (
            chain_root
            / "deploy/alpha/macmini2010/nexus-v2-post-acceptance-reopen-host-action.sh"
        ).resolve(),
        "site-ingress": (
            chain_root
            / "deploy/alpha/macmini2010/nexus-v2-post-acceptance-reopen-site-action.sh"
        ).resolve(),
        "fps-server": (
            Path(lock["repositories"]["unity"]["root"])
            / "deploy/alpha/macmini2014/nexus-v2-fps-reopen-component.sh"
        ).resolve(),
    }
    for component in COMPONENTS:
        require(
            Path(plan["drivers"][component]["path"]).resolve() == expected_driver,
            f"{component} driver is not from the final-lock-pinned chain source",
        )
        require(
            Path(plan["helpers"][component]["path"]).resolve()
            == expected_helpers[component],
            f"{component} helper is not from the final-lock-pinned chain source",
        )
    module = web_root / "tcg/apps/web/scripts/nexus-v2-runtime-seeder-phase2-lib.mjs"
    require(module.is_file() and not module.is_symlink(), "official Phase-2 final-seal validator is unavailable")
    script = (
        "const { loadPhase2FinalSeal } = await import(process.argv[1]);"
        "loadPhase2FinalSeal({sealPath:process.argv[2],sealSha256:process.argv[3]});"
    )
    completed = subprocess.run(
        [
            "node",
            "--input-type=module",
            "-e",
            script,
            module.resolve().as_uri(),
            plan["phase2FinalSeal"]["path"],
            plan["phase2FinalSeal"]["sha256"],
        ],
        capture_output=True,
        text=True,
        check=False,
        timeout=60,
        env=child_environment(),
    )
    require(completed.returncode == 0, "official Phase-2 final-seal validation failed")


def run_official_site_identity_validator(
    plan: Mapping[str, Any], lock: Mapping[str, Any]
) -> None:
    web_root = Path(lock["repositories"]["web"]["root"])
    verifier = (
        web_root
        / "tcg/scripts/release/verify_nexus_v2_site_deployment_identity.py"
    ).resolve()
    compose = (web_root / "tcg/deploy/alpha/macmini2014/docker-compose.yaml").resolve()
    normalizer = Path(plan["siteRuntimeConfigNormalizer"]["path"]).resolve()
    require(
        verifier.is_file()
        and not verifier.is_symlink()
        and compose.is_file()
        and not compose.is_symlink(),
        "official site deployment-identity verifier is unavailable",
    )
    config_by_service = {
        epoch["serviceId"]: epoch["authorityConfigHash"]
        for epoch in plan["runtimeAuthority"]["authorityEpochs"]
    }
    completed = subprocess.run(
        [
            sys.executable,
            str(verifier),
            "verify",
            "--identity",
            plan["sitePostPhase2DeploymentIdentity"]["path"],
            "--release-version",
            plan["siteReleaseVersion"],
            "--site-source-commit",
            plan["siteSourceCommit"],
            "--fps-config-hash",
            config_by_service["eterra-fps-authority"],
            "--legends-config-hash",
            config_by_service["eterra-legends-authority"],
            "--candidate-manifest",
            plan["siteDeploymentCandidateManifest"]["path"],
            "--candidate-manifest-sha256",
            plan["siteDeploymentCandidateManifest"]["sha256"],
            "--phase1-post-deploy-identity",
            plan["sitePhase1PostDeployIdentity"]["path"],
            "--phase1-post-deploy-identity-sha256",
            plan["sitePhase1PostDeployIdentity"]["sha256"],
            "--full-loop-activation-receipt",
            plan["fullLoopIndexerActivationReceipt"]["path"],
            "--full-loop-activation-receipt-sha256",
            plan["fullLoopIndexerActivationReceipt"]["sha256"],
            "--full-loop-activation-verifier",
            str(
                web_root
                / "tcg/deploy/alpha/macmini2014/nexus_v2_full_loop_activation_contract.py"
            ),
            "--compose-file",
            str(compose),
            "--runtime-normalizer",
            str(normalizer),
        ],
        capture_output=True,
        text=True,
        check=False,
        timeout=60,
        env=child_environment(),
    )
    require(
        completed.returncode == 0,
        "official site deployment-identity validation failed",
    )


def validate_emergency_closure(value: Any, plan: Mapping[str, Any]) -> dict[str, Any]:
    closure = dict(exact_keys(value, EMERGENCY_CLOSURE_KEYS, "emergency closure authority"))
    root = Path(closure["bundleRoot"]).resolve()
    require(root.is_absolute() and root.is_dir() and not root.is_symlink(), "emergency closure bundle is unavailable")
    root = root.resolve()

    def validate_bundle_pin(pin_value: Any, label: str, *, executable: bool = False) -> dict[str, str]:
        pin = validate_pin(pin_value, label)
        path = Path(pin["path"]).resolve()
        require(root in path.parents, f"{label} escapes emergency closure bundle")
        if executable:
            require(os.access(path, os.X_OK), f"{label} is not executable")
        return pin

    driver = validate_bundle_pin(closure["driver"], "emergency closure driver", executable=True)
    helpers = exact_keys(closure["helpers"], set(COMPONENTS), "emergency closure helpers")
    for component in COMPONENTS:
        validate_bundle_pin(helpers[component], f"{component} emergency helper", executable=True)
    libraries = exact_keys(closure["libraries"], {"chain", "site", "unity"}, "emergency closure libraries")
    for component in ("chain", "site", "unity"):
        validate_bundle_pin(libraries[component], f"{component} emergency library")
    fps_environment = validate_bundle_pin(
        closure["unityFpsDeploymentEnvironment"],
        "emergency Unity FPS deployment environment",
    )
    require(
        fps_environment["sha256"]
        == plan["unityFpsDeploymentEnvironment"]["sha256"],
        "emergency Unity FPS environment differs from reopen authority",
    )
    fps = exact_keys(closure["fps"], EMERGENCY_FPS_KEYS, "emergency FPS authority")
    candidate_root = Path(fps["candidateRoot"]).resolve()
    require(
        candidate_root.is_dir()
        and not candidate_root.is_symlink()
        and root in candidate_root.parents,
        "emergency FPS candidate escapes the closure bundle",
    )
    candidate_manifest = candidate_root / "candidate-manifest.json"
    ensure_sha(fps["candidateManifestSha256"], "emergency FPS candidate SHA-256", nonzero=True)
    require(
        candidate_manifest.is_file()
        and not candidate_manifest.is_symlink()
        and sha256_file(candidate_manifest) == fps["candidateManifestSha256"]
        == plan["unityFpsCandidateManifest"]["sha256"],
        "emergency FPS candidate differs from reopen authority",
    )
    for name in ("snapshotPath", "deploymentReceiptPath", "rollbackReceiptPath"):
        output_path = Path(fps[name]).resolve(strict=False)
        require(
            root in output_path.parents and output_path != root,
            f"emergency FPS {name} escapes the closure bundle",
        )
    for name in ("rollbackScript", "candidateVerifier", "receiptVerifier", "pinVerifier"):
        validate_bundle_pin(fps[name], f"emergency FPS {name}", executable=True)
    caddyfiles = exact_keys(closure["caddyfiles"], CADDY_KEYS, "emergency closure Caddyfiles")
    for name in CADDY_KEYS:
        pin = validate_bundle_pin(caddyfiles[name], f"emergency {name} Caddyfile")
        require(pin["sha256"] == plan["caddyfiles"][name]["sha256"], f"emergency {name} Caddyfile differs from reopen authority")
    emergency_ssh_pins = validate_ssh_host_pins(
        closure["sshHostPins"], "emergency closure SSH host pins"
    )
    source_ssh_pins = validate_ssh_host_pins(
        plan["sshHostPins"], "reopen SSH host pins", shape_only=True
    )
    for name in SSH_HOST_PIN_KEYS:
        pin_path = Path(emergency_ssh_pins[name]["path"]).resolve()
        require(root in pin_path.parents, f"emergency SSH {name} escapes emergency closure bundle")
        require(
            emergency_ssh_pins[name]["sha256"] == source_ssh_pins[name]["sha256"],
            f"emergency SSH {name} differs from reopen authority",
        )
    targets = exact_keys(closure["targets"], EMERGENCY_TARGET_KEYS, "emergency closure targets")
    require(targets["chainHost"] == plan["network"]["chainLanIp"], "emergency chain target mismatch")
    require(targets["siteHost"] == plan["network"]["siteLanIp"], "emergency site target mismatch")
    for field in ("chainUser", "siteUser"):
        require(isinstance(targets[field], str) and re.fullmatch(r"[A-Za-z_][A-Za-z0-9_-]{0,31}", targets[field]) is not None, f"emergency {field} is invalid")
    require(Path(driver["path"]).name == "driver", "emergency closure driver name mismatch")
    return closure


def validate_plan_shape(
    value: Mapping[str, Any],
    *,
    now: dt.datetime | None = None,
    allow_expired: bool = False,
    closure_only: bool = False,
) -> dict[str, Any]:
    plan = dict(exact_keys(value, PLAN_KEYS, "reopen plan"))
    require(plan["schemaVersion"] == 1 and plan["kind"] == PLAN_KIND, "reopen plan identity mismatch")
    ensure_id(plan["operationId"], "operation ID")
    ensure_id(plan["releaseId"], "release ID")
    require(
        re.fullmatch(r"nexus-v2-private-alpha-[A-Za-z0-9._-]+", plan["releaseId"])
        is not None,
        "invalid Nexus V2 private-Alpha release ID",
    )
    require(
        isinstance(plan["siteReleaseVersion"], str)
        and re.fullmatch(r"v[A-Za-z0-9][A-Za-z0-9._-]{0,126}", plan["siteReleaseVersion"])
        is not None,
        "invalid site release version",
    )
    ensure_commit(plan["sourceCommit"], "source commit")
    ensure_commit(plan["siteSourceCommit"], "site source commit")
    require(isinstance(plan["genesisHash"], str) and HASH256_RE.fullmatch(plan["genesisHash"]) is not None, "invalid genesis hash")
    created = parse_utc(plan["createdAtUtc"], "plan creation time")
    expires = parse_utc(plan["expiresAtUtc"], "plan expiry")
    require(created < expires and expires - created <= dt.timedelta(hours=24), "reopen plan lifetime must be in (0,24h]")
    current = now or dt.datetime.now(dt.timezone.utc)
    require(created <= current + dt.timedelta(seconds=30), "reopen plan is from the future")
    if not allow_expired:
        require(expires > current, "reopen plan expired")
    for field in (
        "finalReleaseLock",
        "replacementLock",
        "acceptanceBoundaryReceipt",
        "phase2FinalSeal",
        "phase2BootstrapPrerequisite",
        "authorityManifest",
        "selectedDeploymentEnvironment",
        "selectedSiteDeploymentEnvironment",
        "fullLoopIndexerActivationReceipt",
        "sitePostPhase2DeploymentIdentity",
        "siteDeploymentCandidateManifest",
        "sitePhase1PostDeployIdentity",
        "siteRuntimeConfigNormalizer",
        "unityFpsCandidateManifest",
        "unityFpsDeploymentEnvironment",
        "phase2InternalTransportHandoff",
    ):
        (validate_pin_shape if closure_only else validate_pin)(plan[field], field)
    validate_ssh_host_pins(
        plan["sshHostPins"],
        "reopen SSH host pins",
        shape_only=closure_only,
    )
    caddyfiles = exact_keys(plan["caddyfiles"], CADDY_KEYS, "Caddyfile pins")
    for name in CADDY_KEYS:
        (validate_pin_shape if closure_only else validate_pin)(caddyfiles[name], f"{name} Caddyfile")
    drivers = exact_keys(plan["drivers"], set(COMPONENTS), "component drivers")
    helpers = exact_keys(plan["helpers"], set(COMPONENTS), "component helpers")
    if closure_only:
        unity_source_commit = helpers["fps-server"].get("sourceCommit")
    else:
        final_lock = read_json(Path(plan["finalReleaseLock"]["path"]), "final release lock")
        unity_source_commit = final_lock.get("repositories", {}).get("unity", {}).get("head")
    for component in COMPONENTS:
        validator = validate_pin_shape if closure_only else validate_pin
        driver = validator(drivers[component], f"{component} driver", executable=True)
        helper = validator(helpers[component], f"{component} helper", executable=True)
        require(driver["sourceCommit"] == plan["sourceCommit"], f"{component} driver source mismatch")
        expected_helper_source = (
            unity_source_commit if component == "fps-server" else plan["sourceCommit"]
        )
        require(helper["sourceCommit"] == expected_helper_source, f"{component} helper source mismatch")
    network = exact_keys(plan["network"], NETWORK_KEYS, "network")
    chain_ip = ipaddress.ip_address(network["chainLanIp"])
    site_ip = ipaddress.ip_address(network["siteLanIp"])
    require(chain_ip.version == site_ip.version == 4 and chain_ip.is_private and site_ip.is_private, "reopen hosts must be private IPv4 addresses")
    require(chain_ip != site_ip, "chain and site host addresses must differ")
    require(isinstance(network["publicHostname"], str) and HOST_RE.fullmatch(network["publicHostname"]) is not None, "invalid public hostname")
    require(plan["ports"] == PORTS, "reopen port contract mismatch")
    smoke = exact_keys(plan["smoke"], SMOKE_KEYS, "smoke inputs")
    safe_smoke_path(smoke["mediaPath"], "/nft/", "media smoke path")
    safe_smoke_path(smoke["ipfsPath"], "/ipfs/", "IPFS smoke path")
    ensure_sha(smoke["mediaSha256"], "media smoke SHA-256", nonzero=True)
    ensure_sha(smoke["ipfsSha256"], "IPFS smoke SHA-256", nonzero=True)
    validate_runtime_authority(plan["runtimeAuthority"])
    validate_indexer_readiness(plan["indexerReadiness"], plan)
    if not closure_only:
        identity_artifact = read_json(
            Path(plan["sitePostPhase2DeploymentIdentity"]["path"]),
            "site deployment identity artifact",
        )
        require(
            Path(plan["sitePostPhase2DeploymentIdentity"]["path"]).read_bytes()
            == canonical_bytes(identity_artifact),
            "site deployment identity artifact is not canonical JSON",
        )
        require(
            identity_artifact == plan["siteDeploymentIdentity"],
            "embedded site deployment identity differs from its pinned artifact",
        )
    else:
        identity_artifact = plan["siteDeploymentIdentity"]
    site_identity = validate_site_deployment_identity(
        identity_artifact,
        plan,
        verify_source_artifact=not closure_only,
    )
    identity_time = parse_utc(site_identity["capturedAtUtc"], "site deployment identity capture time")
    require(
        identity_time <= created + dt.timedelta(seconds=30)
        and created - identity_time <= dt.timedelta(minutes=30),
        "site deployment identity observation is stale or from the future",
    )
    if not closure_only:
        handoff_path = Path(plan["phase2InternalTransportHandoff"]["path"])
        handoff = read_json(handoff_path, "Phase-2 internal transport handoff")
        require(
            handoff_path.read_bytes() == canonical_bytes(handoff),
            "Phase-2 internal transport handoff is not canonical JSON",
        )
        validate_phase2_transport_handoff(handoff, plan)
        require(
            handoff == plan["phase2InternalTransport"],
            "embedded Phase-2 transport handoff differs from the final-lock artifact",
        )
        run_official_phase2_transport_handoff_validator(plan)
    else:
        validate_phase2_transport_handoff(plan["phase2InternalTransport"], plan)
    validate_emergency_closure(plan["emergencyClosure"], plan)
    require(plan["policy"] == POLICY, "reopen policy mismatch")
    return plan


def validate_authorities(plan: Mapping[str, Any]) -> tuple[dict[str, Any], dict[str, Any], dict[str, Any]]:
    lock_module = load_release_lock_module()
    lock_path = Path(plan["finalReleaseLock"]["path"])
    try:
        lock = lock_module.validate_lock(
            lock_path,
            plan["finalReleaseLock"]["sha256"],
            plan["selectedDeploymentEnvironment"]["path"],
            plan["selectedSiteDeploymentEnvironment"]["path"],
        )
    except Exception as exc:
        raise ReopenError(f"final release lock validation failed: {exc}") from exc
    require(lock["releaseId"] == plan["releaseId"], "release-lock release mismatch")
    require(lock["repositories"]["chain"]["head"] == plan["sourceCommit"], "release-lock chain source mismatch")
    require(lock["repositories"]["web"]["head"] == plan["siteSourceCommit"], "release-lock site source mismatch")
    require(lock["artifacts"]["acceptanceBoundaryReceipt"] == plan["acceptanceBoundaryReceipt"], "acceptance receipt is not the final-lock artifact")
    require(lock["artifacts"]["replacementLock"] == plan["replacementLock"], "replacement lock is not the final-lock artifact")
    require(lock["artifacts"]["deploymentEnvironment"] == plan["selectedDeploymentEnvironment"], "chain environment is not final-lock selected")
    require(lock["artifacts"]["siteDeploymentEnvironment"] == plan["selectedSiteDeploymentEnvironment"], "site environment is not final-lock selected")
    for artifact_name, plan_name in (
        ("siteDeploymentCandidateManifest", "siteDeploymentCandidateManifest"),
        ("sitePhase1PostDeployIdentity", "sitePhase1PostDeployIdentity"),
        ("fullLoopIndexerActivationReceipt", "fullLoopIndexerActivationReceipt"),
        ("sitePostPhase2DeploymentIdentity", "sitePostPhase2DeploymentIdentity"),
        ("unityFpsCandidateManifest", "unityFpsCandidateManifest"),
        ("unityFpsDeploymentEnvironment", "unityFpsDeploymentEnvironment"),
        ("phase2InternalTransportHandoff", "phase2InternalTransportHandoff"),
    ):
        require(
            lock["artifacts"][artifact_name] == plan[plan_name],
            f"{artifact_name} is not the final-lock artifact",
        )
    require(
        lock["artifacts"]["sshKnownHosts"] == plan["sshHostPins"]["knownHosts"],
        "dedicated SSH known_hosts is not final-lock selected",
    )
    require(
        lock["artifacts"]["sshHostPinManifest"] == plan["sshHostPins"]["manifest"],
        "SSH host-pin manifest is not final-lock selected",
    )
    expected_ssh_validator = (
        Path(lock["repositories"]["chain"]["root"])
        / "scripts/nexus-v2-private-alpha/capture_ssh_host_pins.py"
    ).resolve()
    require(
        Path(plan["sshHostPins"]["validator"]["path"]).resolve()
        == expected_ssh_validator,
        "SSH host-pin validator is not from the final-lock-pinned chain source",
    )
    site_environment = parse_environment(Path(plan["selectedSiteDeploymentEnvironment"]["path"]))
    require(
        site_environment.get("RELEASE_VERSION") == plan["siteReleaseVersion"],
        "site release version is not the final-lock-pinned site environment value",
    )
    require(
        site_environment.get("EXPECTED_SOURCE_COMMIT") == plan["siteSourceCommit"],
        "site source commit is not the final-lock-pinned site environment value",
    )
    require(
        site_environment.get("NEXUS_V2_FULL_LOOP_ACCEPTANCE_READS_ENABLED", "").lower()
        == "false"
        and site_environment.get("NEXUS_V2_FULL_LOOP_ACCEPTANCE_TARGET_JSON", "") == ""
        and site_environment.get("INDEXER_CHAIN_WS_URL")
        == f"ws://{plan['network']['chainLanIp']}:9944",
        "base site environment is not disabled/target-free or its internal chain endpoint drifted",
    )
    require(
        site_environment.get("HOME_HOSTNAME") == plan["network"]["publicHostname"],
        "reopen public hostname differs from locked HOME_HOSTNAME",
    )
    receipt_path = Path(plan["acceptanceBoundaryReceipt"]["path"])
    receipt = read_json(receipt_path, "acceptance-boundary receipt")
    require(receipt_path.read_bytes() == canonical_bytes(receipt), "acceptance-boundary receipt is not canonical JSON")
    require(receipt["releaseId"] == plan["releaseId"] and receipt["sourceCommit"] == plan["sourceCommit"], "acceptance receipt release/source mismatch")
    require(receipt["genesisHash"] == plan["genesisHash"], "acceptance receipt genesis mismatch")
    require(receipt["coordinatorDecision"] == "keep-v2" and receipt["phase1SmokePassed"] is True, "acceptance did not keep a healthy V2 deployment")
    require(receipt["automaticRestorePermanentlyDisabled"] is True, "acceptance did not retire automatic state restore")
    require(receipt["operatorV2WriteScope"].get("paidOrPublicActivation") is False, "acceptance receipt permits paid/public activation")
    seal_path = Path(plan["phase2FinalSeal"]["path"])
    seal = read_json(seal_path, "Phase-2 final seal")
    require(seal_path.read_bytes() == canonical_bytes(seal), "Phase-2 final seal is not canonical JSON")
    validate_phase2_seal_shape(seal, receipt, lock)
    prerequisite_path = Path(plan["phase2BootstrapPrerequisite"]["path"])
    prerequisite = read_json(prerequisite_path, "Phase-2 bootstrap prerequisite")
    prerequisite["__artifact_sha256"] = plan["phase2BootstrapPrerequisite"]["sha256"]
    manifest_path = Path(plan["authorityManifest"]["path"])
    manifest = read_json(manifest_path, "authority manifest")
    manifest["__artifact_sha256"] = plan["authorityManifest"]["sha256"]
    require(
        build_runtime_authority(prerequisite, manifest, seal) == plan["runtimeAuthority"],
        "runtime authority differs from the Phase-2 artifacts",
    )
    require(
        parse_utc(seal["generated_at_utc"], "Phase-2 final-seal time")
        >= parse_utc(receipt["createdAtUtc"], "acceptance receipt creation time"),
        "Phase-2 final seal predates the acceptance receipt",
    )
    require(
        parse_utc(plan["createdAtUtc"], "reopen plan creation time")
        >= parse_utc(seal["generated_at_utc"], "Phase-2 final-seal time"),
        "reopen plan predates the Phase-2 final seal",
    )
    require(
        isinstance(seal["alpha_access"]["expires_at_unix"], int)
        and not isinstance(seal["alpha_access"]["expires_at_unix"], bool)
        and seal["alpha_access"]["expires_at_unix"]
        > int(parse_utc(plan["expiresAtUtc"], "reopen plan expiry").timestamp()),
        "AlphaAccess grant expires before the reopen plan",
    )
    run_official_phase2_validator(plan, lock)
    run_official_site_identity_validator(plan, lock)
    web_root = Path(lock["repositories"]["web"]["root"])
    expected_normal = (
        web_root
        / "tcg/deploy/alpha/macmini2014/nexus-v2-restricted-alpha.Caddyfile"
    ).resolve()
    expected_phase1 = (web_root / "tcg/deploy/alpha/macmini2014/nexus-v2-phase1-readonly.Caddyfile").resolve()
    expected_normalizer = (
        web_root / "tcg/scripts/release/nexus_v2_docker_runtime_config.py"
    ).resolve()
    require(Path(plan["caddyfiles"]["normal"]["path"]).resolve() == expected_normal, "normal Caddyfile is not from the final-lock-pinned web source")
    require(Path(plan["caddyfiles"]["phase1"]["path"]).resolve() == expected_phase1, "Phase-1 Caddyfile is not from the final-lock-pinned web source")
    require(
        Path(plan["siteRuntimeConfigNormalizer"]["path"]).resolve()
        == expected_normalizer,
        "site runtime normalizer is not from the final-lock-pinned web source",
    )
    normal_text = expected_normal.read_text(encoding="utf-8")
    phase1_text = expected_phase1.read_text(encoding="utf-8")
    require("AllExternalWriteIngressClosed" not in normal_text, "normal Caddyfile is still Phase-1 closed")
    for token in ("{$CHAIN_UPSTREAM_HOST}:{$CHAIN_RPC_PORT}", "{$MEDIA_UPSTREAM_HOST}:{$MEDIA_PORT}", "{$IPFS_UPSTREAM_HOST}:{$IPFS_GATEWAY_PORT}", "{$AUTHORITY_UPSTREAM_HOST}:{$AUTHORITY_PORT}"):
        require(token in normal_text, f"normal Caddyfile lacks required upstream: {token}")
    for forbidden in ("crypto-strike", "blockchainia", ":8094", "192.168.", "10."):
        require(forbidden not in normal_text, f"restricted Caddyfile contains forbidden ingress token: {forbidden}")
    allowed_upstreams = {
        "{$CHAIN_UPSTREAM_HOST}:{$CHAIN_RPC_PORT}",
        "{$MEDIA_UPSTREAM_HOST}:{$MEDIA_PORT}",
        "{$IPFS_UPSTREAM_HOST}:{$IPFS_GATEWAY_PORT}",
        "{$AUTHORITY_UPSTREAM_HOST}:{$AUTHORITY_PORT}",
        "site:3000",
        "indexer-api:{$INDEXER_API_PORT}",
    }
    observed_upstreams = {
        match.group(1)
        for match in re.finditer(r"(?m)^\s*reverse_proxy\s+(\S+)", normal_text)
    }
    require(
        observed_upstreams == allowed_upstreams,
        "restricted Caddyfile reverse-proxy set is not exact",
    )
    require("AllExternalWriteIngressClosed" in phase1_text and "Phase-1 public RPC ingress closed" in phase1_text, "Phase-1 Caddyfile does not fail closed")
    return lock, receipt, seal


def load_plan(
    path: Path,
    expected_sha256: str,
    *,
    allow_expired: bool = False,
    closure_only: bool = False,
) -> dict[str, Any]:
    ensure_sha(expected_sha256, "plan SHA-256")
    plan = read_json(path, "reopen plan")
    require(path.read_bytes() == canonical_bytes(plan), "reopen plan is not canonical JSON")
    require(sha256_file(path) == expected_sha256, "reopen plan hash mismatch")
    validate_plan_shape(plan, allow_expired=allow_expired, closure_only=closure_only)
    if not closure_only:
        validate_authorities(plan)
        require_outside_pinned_repositories(path, plan, "reopen plan")
    return plan


def write_new(path: Path, value: Mapping[str, Any], *, mode: int = 0o400) -> None:
    require(path.is_absolute(), "output path must be absolute")
    require(not path.exists() and not path.is_symlink(), f"refusing to overwrite output: {path}")
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, mode)
    with os.fdopen(descriptor, "wb") as handle:
        handle.write(canonical_bytes(value))


def pinned_repository_roots(plan: Mapping[str, Any]) -> list[Path]:
    lock = read_json(Path(plan["finalReleaseLock"]["path"]), "final release lock")
    repositories = lock.get("repositories")
    require(isinstance(repositories, dict), "final release lock repositories are invalid")
    roots: list[Path] = []
    for identifier, pin in repositories.items():
        require(isinstance(identifier, str) and isinstance(pin, dict), "release-lock repository pin is invalid")
        root = pin.get("root")
        require(isinstance(root, str) and Path(root).is_absolute(), "release-lock repository root is invalid")
        roots.append(Path(root).resolve())
    return roots


def require_outside_pinned_repositories(
    path: Path,
    plan: Mapping[str, Any],
    label: str,
) -> None:
    require(path.is_absolute(), f"{label} path must be absolute")
    candidate = path.resolve(strict=False)
    for root in pinned_repository_roots(plan):
        require(
            candidate != root and root not in candidate.parents,
            f"{label} must remain outside final-lock-pinned repositories",
        )


def executable_pin(path_value: str, source_commit: str, label: str) -> dict[str, str]:
    pin = file_pin(path_value, label)
    path = Path(pin["path"])
    require(os.access(path, os.X_OK), f"{label} is not executable")
    return {**pin, "sourceCommit": ensure_commit(source_commit, f"{label} source")}


def prepare_emergency_closure_bundle(
    args: argparse.Namespace,
    output: Path,
    lock: Mapping[str, Any],
    chain_environment: Mapping[str, str],
    site_environment: Mapping[str, str],
) -> dict[str, Any]:
    root = output.parent / f"{output.name}.emergency-closure"
    require(not root.exists() and not root.is_symlink(), "emergency closure bundle path must be new")
    root.mkdir(parents=False, mode=0o700)

    unity_root = Path(lock["repositories"]["unity"]["root"])

    sources = {
        "driver": SCRIPT_PATH.with_name("nexus-v2-post-acceptance-emergency-close-driver"),
        "chain-helper": Path(args.chain_helper),
        "site-helper": Path(args.site_helper),
        "fps-helper": Path(args.fps_helper),
        "chain-library": SCRIPT_PATH.with_name("lib.sh"),
        "site-library": Path(lock["repositories"]["web"]["root"])
        / "tcg/deploy/alpha/macmini2014/lib.sh",
        "unity-library": unity_root / "deploy/alpha/macmini2014/lib.sh",
        "fps-rollback-script": unity_root
        / "deploy/alpha/macmini2014/rollback-fps-server.sh",
        "fps-candidate-verifier": unity_root
        / "scripts/release/fps-server-candidate.py",
        "fps-receipt-verifier": unity_root
        / "scripts/release/fps-deployment-receipt.py",
        "fps-pin-verifier": unity_root
        / "deploy/alpha/macmini2014/verify_ssh_host_pins.py",
        "fps-environment": Path(lock["artifacts"]["unityFpsDeploymentEnvironment"]["path"]),
        "normal-caddy": Path(args.normal_caddyfile),
        "phase1-caddy": Path(args.phase1_caddyfile),
        "ssh-known-hosts": Path(args.ssh_known_hosts),
        "ssh-host-pin-manifest": Path(args.ssh_host_pin_manifest),
        "ssh-host-pin-validator": REPO_ROOT
        / "scripts/nexus-v2-private-alpha/capture_ssh_host_pins.py",
    }
    copied: dict[str, Path] = {}
    try:
        for name, source in sources.items():
            require(source.is_absolute() and source.is_file() and not source.is_symlink(), f"emergency closure source is unavailable: {name}")
            destination = root / name
            shutil.copyfile(source, destination)
            if name in {
                "driver",
                "chain-helper",
                "site-helper",
                "fps-helper",
                "fps-rollback-script",
                "fps-candidate-verifier",
                "fps-receipt-verifier",
                "fps-pin-verifier",
                "ssh-host-pin-validator",
            }:
                mode = 0o700
            elif name in {"ssh-known-hosts", "ssh-host-pin-manifest", "fps-environment"}:
                mode = 0o600
            else:
                mode = 0o400
            os.chmod(destination, mode)
            copied[name] = destination

        # Reconstruct the minimal Unity tool layout expected by the guarded
        # wrapper. The copied candidate is immutable evidence; mutable snapshot
        # and receipt outputs live in a separate empty state directory.
        tool_deploy = root / "unity-tool/deploy/alpha/macmini2014"
        tool_release = root / "unity-tool/scripts/release"
        tool_deploy.mkdir(parents=True, mode=0o700)
        tool_release.mkdir(parents=True, mode=0o700)
        layout = {
            copied["fps-helper"]: tool_deploy / "nexus-v2-fps-reopen-component.sh",
            copied["unity-library"]: tool_deploy / "lib.sh",
            copied["fps-rollback-script"]: tool_deploy / "rollback-fps-server.sh",
            copied["fps-pin-verifier"]: tool_deploy / "verify_ssh_host_pins.py",
            copied["fps-candidate-verifier"]: tool_release / "fps-server-candidate.py",
            copied["fps-receipt-verifier"]: tool_release / "fps-deployment-receipt.py",
        }
        for source, destination in layout.items():
            shutil.copyfile(source, destination)
            os.chmod(destination, 0o700)
        candidate_source = Path(
            lock["artifacts"]["unityFpsCandidateManifest"]["path"]
        ).parent
        candidate_copy = root / "fps-candidate"
        require(
            candidate_source.is_absolute()
            and candidate_source.is_dir()
            and not candidate_source.is_symlink(),
            "Unity FPS candidate root is unavailable for emergency closure",
        )
        for source in candidate_source.rglob("*"):
            require(not source.is_symlink(), "Unity FPS candidate contains a symlink")
        shutil.copytree(candidate_source, candidate_copy)
        for copied_path in candidate_copy.rglob("*"):
            if copied_path.is_file():
                os.chmod(copied_path, 0o400)
            elif copied_path.is_dir():
                os.chmod(copied_path, 0o500)
        fps_state = root / "fps-state"
        fps_state.mkdir(mode=0o700)
    except Exception:
        shutil.rmtree(root)
        raise

    chain_host = chain_environment.get("DEPLOY_HOST", "")
    site_host = site_environment.get("DEPLOY_HOST", "")
    require(chain_host == args.chain_lan_ip, "chain credential target differs from emergency closure target")
    require(site_host == args.site_lan_ip, "site credential target differs from emergency closure target")
    closure = {
        "bundleRoot": str(root.resolve()),
        "driver": file_pin(str(copied["driver"]), "emergency closure driver"),
        "helpers": {
            "chain-transport": file_pin(str(copied["chain-helper"]), "emergency chain helper"),
            "fps-server": file_pin(
                str(root / "unity-tool/deploy/alpha/macmini2014/nexus-v2-fps-reopen-component.sh"),
                "emergency FPS helper",
            ),
            "site-ingress": file_pin(str(copied["site-helper"]), "emergency site helper"),
        },
        "libraries": {
            "chain": file_pin(str(copied["chain-library"]), "emergency chain library"),
            "site": file_pin(str(copied["site-library"]), "emergency site library"),
            "unity": file_pin(
                str(root / "unity-tool/deploy/alpha/macmini2014/lib.sh"),
                "emergency Unity library",
            ),
        },
        "unityFpsDeploymentEnvironment": file_pin(
            str(copied["fps-environment"]),
            "emergency Unity FPS deployment environment",
        ),
        "fps": {
            "candidateRoot": str((root / "fps-candidate").resolve()),
            "candidateManifestSha256": lock["artifacts"]["unityFpsCandidateManifest"]["sha256"],
            "snapshotPath": str((root / "fps-state/deployment-snapshot").resolve()),
            "deploymentReceiptPath": str((root / "fps-state/deployment-receipt.json").resolve()),
            "rollbackReceiptPath": str((root / "fps-state/rollback-receipt.json").resolve()),
            "rollbackScript": file_pin(
                str(root / "unity-tool/deploy/alpha/macmini2014/rollback-fps-server.sh"),
                "emergency FPS rollback script",
            ),
            "candidateVerifier": file_pin(
                str(root / "unity-tool/scripts/release/fps-server-candidate.py"),
                "emergency FPS candidate verifier",
            ),
            "receiptVerifier": file_pin(
                str(root / "unity-tool/scripts/release/fps-deployment-receipt.py"),
                "emergency FPS receipt verifier",
            ),
            "pinVerifier": file_pin(
                str(root / "unity-tool/deploy/alpha/macmini2014/verify_ssh_host_pins.py"),
                "emergency FPS host-pin verifier",
            ),
        },
        "caddyfiles": {
            "normal": file_pin(str(copied["normal-caddy"]), "emergency normal Caddyfile"),
            "phase1": file_pin(str(copied["phase1-caddy"]), "emergency Phase-1 Caddyfile"),
        },
        "sshHostPins": {
            "knownHosts": file_pin(
                str(copied["ssh-known-hosts"]),
                "emergency dedicated SSH known_hosts",
            ),
            "manifest": file_pin(
                str(copied["ssh-host-pin-manifest"]),
                "emergency SSH host-pin manifest",
            ),
            "validator": file_pin(
                str(copied["ssh-host-pin-validator"]),
                "emergency SSH host-pin validator",
            ),
        },
        "targets": {
            "chainHost": chain_host,
            "chainUser": chain_environment.get("DEPLOY_USER", "eterra2010"),
            "siteHost": site_host,
            "siteUser": site_environment.get("DEPLOY_USER", "eterra2014"),
        },
    }
    return closure


def command_capture(args: argparse.Namespace) -> None:
    created_at = args.created_at or utc_now()
    created = parse_utc(created_at, "plan creation time")
    expires_at = args.expires_at or (created + dt.timedelta(hours=1)).strftime("%Y-%m-%dT%H:%M:%SZ")
    release_pin = file_pin(args.final_release_lock, "final release lock", canonical_json=True)
    seal_pin = file_pin(args.phase2_final_seal, "Phase-2 final seal", canonical_json=True)
    prerequisite_pin = file_pin(args.phase2_bootstrap_prerequisite, "Phase-2 bootstrap prerequisite")
    authority_manifest_pin = file_pin(args.authority_manifest, "authority manifest")
    lock = read_json(Path(release_pin["path"]), "final release lock")
    lock_artifacts = lock.get("artifacts")
    repositories = lock.get("repositories")
    require(isinstance(lock_artifacts, dict) and isinstance(repositories, dict), "final release lock is incomplete")

    def locked_artifact(name: str, label: str, *, canonical_json: bool = False) -> dict[str, str]:
        expected = lock_artifacts.get(name)
        require(isinstance(expected, dict), f"final release lock lacks {name}")
        actual = file_pin(expected.get("path", ""), label, canonical_json=canonical_json)
        require(actual == expected, f"final release lock artifact drifted: {name}")
        return actual

    receipt_pin = locked_artifact(
        "acceptanceBoundaryReceipt", "acceptance-boundary receipt", canonical_json=True
    )
    replacement_lock_pin = locked_artifact(
        "replacementLock", "pre-cutover replacement lock", canonical_json=True
    )
    chain_env_pin = locked_artifact("deploymentEnvironment", "selected deployment environment")
    site_env_pin = locked_artifact("siteDeploymentEnvironment", "selected site deployment environment")
    fps_env_pin = locked_artifact(
        "unityFpsDeploymentEnvironment", "Unity FPS deployment environment"
    )
    site_candidate_pin = locked_artifact(
        "siteDeploymentCandidateManifest", "site deployment candidate manifest"
    )
    site_phase1_identity_pin = locked_artifact(
        "sitePhase1PostDeployIdentity", "site Phase-1 post-deploy identity", canonical_json=True
    )
    activation_pin = locked_artifact(
        "fullLoopIndexerActivationReceipt", "full-loop indexer activation receipt", canonical_json=True
    )
    site_identity_pin = locked_artifact(
        "sitePostPhase2DeploymentIdentity", "site post-Phase2 deployment identity", canonical_json=True
    )
    fps_candidate_pin = locked_artifact(
        "unityFpsCandidateManifest", "Unity FPS candidate manifest"
    )
    transport_handoff_pin = locked_artifact(
        "phase2InternalTransportHandoff", "Phase-2 internal transport handoff", canonical_json=True
    )
    transport_handoff = read_json(
        Path(transport_handoff_pin["path"]), "Phase-2 internal transport handoff"
    )
    site_environment = parse_environment(Path(site_env_pin["path"]))
    chain_environment = parse_environment(Path(chain_env_pin["path"]))
    site_release_version = site_environment.get("RELEASE_VERSION", "")
    source_commit = lock.get("repositories", {}).get("chain", {}).get("head", "")
    site_source_commit = lock.get("repositories", {}).get("web", {}).get("head", "")
    release_id = lock.get("releaseId", "")
    target = read_json(Path(lock_artifacts["targetIdentity"]["path"]), "target identity")
    genesis_hash = target.get("genesisHash", "")
    ssh_host_pins = {
        "knownHosts": locked_artifact("sshKnownHosts", "dedicated SSH known_hosts"),
        "manifest": locked_artifact("sshHostPinManifest", "SSH host-pin manifest", canonical_json=True),
        "validator": file_pin(
            str(REPO_ROOT / "scripts/nexus-v2-private-alpha/capture_ssh_host_pins.py"),
            "SSH host-pin validator",
        ),
    }
    validate_ssh_host_pins(ssh_host_pins, "capture-plan SSH host pins")
    web_root = Path(lock["repositories"]["web"]["root"])
    chain_root = Path(lock["repositories"]["chain"]["root"])
    unity_root = Path(lock["repositories"]["unity"]["root"])
    runtime_normalizer_pin = file_pin(
        str(web_root / "tcg/scripts/release/nexus_v2_docker_runtime_config.py"),
        "site runtime configuration normalizer",
    )
    prerequisite = read_json(Path(prerequisite_pin["path"]), "Phase-2 bootstrap prerequisite")
    prerequisite["__artifact_sha256"] = prerequisite_pin["sha256"]
    manifest = read_json(Path(authority_manifest_pin["path"]), "authority manifest")
    manifest["__artifact_sha256"] = authority_manifest_pin["sha256"]
    seal = read_json(Path(seal_pin["path"]), "Phase-2 final seal")
    runtime_authority = build_runtime_authority(prerequisite, manifest, seal)
    site_deployment_identity = read_json(Path(site_identity_pin["path"]), "site deployment identity")
    activation = read_json(Path(activation_pin["path"]), "full-loop activation receipt")
    indexer_readiness = build_indexer_readiness(
        activation,
        activation_receipt_sha256=activation_pin["sha256"],
        release_version=site_release_version,
        site_source_commit=site_source_commit,
    )
    args.chain_helper = str(chain_root / "deploy/alpha/macmini2010/nexus-v2-post-acceptance-reopen-host-action.sh")
    args.site_helper = str(chain_root / "deploy/alpha/macmini2010/nexus-v2-post-acceptance-reopen-site-action.sh")
    args.fps_helper = str(unity_root / "deploy/alpha/macmini2014/nexus-v2-fps-reopen-component.sh")
    args.normal_caddyfile = str(web_root / "tcg/deploy/alpha/macmini2014/nexus-v2-restricted-alpha.Caddyfile")
    args.phase1_caddyfile = str(web_root / "tcg/deploy/alpha/macmini2014/nexus-v2-phase1-readonly.Caddyfile")
    args.component_driver = str(chain_root / "deploy/alpha/macmini2010/nexus-v2-post-acceptance-reopen-component-driver")
    args.ssh_known_hosts = ssh_host_pins["knownHosts"]["path"]
    args.ssh_host_pin_manifest = ssh_host_pins["manifest"]["path"]
    args.chain_lan_ip = chain_environment.get("MINI_LAN_IP", "")
    args.site_lan_ip = site_environment.get("SITE_LAN_IP", "")
    args.public_hostname = site_environment.get("HOME_HOSTNAME", "")
    output = Path(args.output)
    require(output.is_absolute(), "reopen plan output must be absolute")
    emergency_closure = prepare_emergency_closure_bundle(
        args,
        output,
        lock,
        chain_environment,
        site_environment,
    )
    plan = {
        "schemaVersion": 1,
        "kind": PLAN_KIND,
        "operationId": args.operation_id,
        "releaseId": release_id,
        "siteReleaseVersion": site_release_version,
        "sourceCommit": source_commit,
        "siteSourceCommit": site_source_commit,
        "genesisHash": genesis_hash,
        "createdAtUtc": created_at,
        "expiresAtUtc": expires_at,
        "finalReleaseLock": release_pin,
        "replacementLock": replacement_lock_pin,
        "acceptanceBoundaryReceipt": receipt_pin,
        "phase2FinalSeal": seal_pin,
        "phase2BootstrapPrerequisite": prerequisite_pin,
        "authorityManifest": authority_manifest_pin,
        "selectedDeploymentEnvironment": chain_env_pin,
        "selectedSiteDeploymentEnvironment": site_env_pin,
        "caddyfiles": {
            "normal": file_pin(args.normal_caddyfile, "normal Caddyfile"),
            "phase1": file_pin(args.phase1_caddyfile, "Phase-1 Caddyfile"),
        },
        "drivers": {
            component: executable_pin(args.component_driver, source_commit, f"{component} driver")
            for component in COMPONENTS
        },
        "helpers": {
            "chain-transport": executable_pin(args.chain_helper, source_commit, "chain-transport helper"),
            "fps-server": executable_pin(
                args.fps_helper,
                lock["repositories"]["unity"]["head"],
                "FPS server helper",
            ),
            "site-ingress": executable_pin(args.site_helper, source_commit, "site-ingress helper"),
        },
        "network": {
            "chainLanIp": args.chain_lan_ip,
            "siteLanIp": args.site_lan_ip,
            "publicHostname": args.public_hostname,
        },
        "ports": PORTS,
        "smoke": {
            "mediaPath": args.media_smoke_path,
            "mediaSha256": args.media_smoke_sha256,
            "ipfsPath": args.ipfs_smoke_path,
            "ipfsSha256": args.ipfs_smoke_sha256,
        },
        "runtimeAuthority": runtime_authority,
        "indexerReadiness": indexer_readiness,
        "fullLoopIndexerActivationReceipt": activation_pin,
        "siteDeploymentIdentity": site_deployment_identity,
        "sitePostPhase2DeploymentIdentity": site_identity_pin,
        "siteDeploymentCandidateManifest": site_candidate_pin,
        "sitePhase1PostDeployIdentity": site_phase1_identity_pin,
        "siteRuntimeConfigNormalizer": runtime_normalizer_pin,
        "unityFpsCandidateManifest": fps_candidate_pin,
        "unityFpsDeploymentEnvironment": fps_env_pin,
        "phase2InternalTransportHandoff": transport_handoff_pin,
        "phase2InternalTransport": transport_handoff,
        "sshHostPins": ssh_host_pins,
        "emergencyClosure": emergency_closure,
        "policy": POLICY,
    }
    try:
        validate_plan_shape(plan)
        validate_authorities(plan)
        require_outside_pinned_repositories(output, plan, "reopen plan output")
        require_outside_pinned_repositories(Path(emergency_closure["bundleRoot"]), plan, "emergency closure bundle")
        write_new(output, plan)
    except Exception:
        shutil.rmtree(Path(emergency_closure["bundleRoot"]), ignore_errors=True)
        raise
    print(f"restricted reopen plan captured: {output} sha256={sha256_file(output)}")


def validate_result(
    path: Path,
    plan: Mapping[str, Any],
    plan_sha256: str,
    component: str,
    action: str,
    mode: str,
    fps_adoption_seal: Mapping[str, str] | None = None,
) -> dict[str, Any]:
    value = read_json(path, f"{component} {action} result")
    exact_keys(value, RESULT_KEYS, f"{component} {action} result")
    require(path.read_bytes() == canonical_bytes(value), f"{component} {action} result is not canonical JSON")
    require(value["schemaVersion"] == 1 and value["kind"] == "nexus-v2-private-alpha-post-acceptance-reopen-component-result", "component result identity mismatch")
    require(value["operationId"] == plan["operationId"] and value["planSha256"] == plan_sha256, "component result plan mismatch")
    require(
        value["releaseId"] == plan["releaseId"]
        and value["siteReleaseVersion"] == plan["siteReleaseVersion"]
        and value["sourceCommit"] == plan["sourceCommit"]
        and value["siteSourceCommit"] == plan["siteSourceCommit"],
        "component result release/source mismatch",
    )
    require(value["componentId"] == component and value["action"] == action and value["mode"] == mode, "component result action mismatch")
    require(value["result"] == "passed", "component action did not pass")
    require(isinstance(value["mutationPerformed"], bool) and isinstance(value["alreadyApplied"], bool), "component mutation flags are invalid")
    if action == "preflight":
        require(value["mutationPerformed"] is False and value["alreadyApplied"] is False, "preflight reported mutation/idempotent apply")
    if mode == "dry-run":
        require(value["mutationPerformed"] is False, "dry-run reported mutation")
        require(value["remoteMarkerSha256"] is None, "dry-run returned a remote marker")
    else:
        ensure_sha(value["remoteMarkerSha256"], "remote marker SHA-256")
    receipt_value = value["componentReceipt"]
    receipt_required = component == "fps-server" and action in {"promote", "verify", "rollback"}
    if receipt_required:
        receipt_pin = validate_pin(receipt_value, f"FPS {action} receipt")
        receipt_path = Path(receipt_pin["path"])
        receipt = read_json(receipt_path, f"FPS {action} receipt")
        require(
            receipt_path.read_bytes() == canonical_bytes(receipt),
            f"FPS {action} receipt is not canonical JSON",
        )
        expected_schema = (
            "eterra.nexus-v2-fps-deployment-receipt.v1"
            if action in {"promote", "verify"}
            else "eterra.nexus-v2-fps-deployment-rollback-receipt.v1"
        )
        require(receipt.get("schema") == expected_schema, f"FPS {action} receipt schema mismatch")
        candidate = (
            receipt.get("candidate")
            if action in {"promote", "verify"}
            else receipt.get("rolledBackCandidate")
        )
        require(
            isinstance(candidate, dict)
            and candidate.get("candidateManifestSha256")
            == plan["unityFpsCandidateManifest"]["sha256"],
            f"FPS {action} receipt candidate pin mismatch",
        )
    else:
        require(receipt_value is None, f"{component} {action} returned an unexpected component receipt")
    require(value["finalReleaseLockSha256"] == plan["finalReleaseLock"]["sha256"], "component final-lock pin mismatch")
    require(value["acceptanceBoundaryReceiptSha256"] == plan["acceptanceBoundaryReceipt"]["sha256"], "component acceptance pin mismatch")
    require(value["phase2FinalSealSha256"] == plan["phase2FinalSeal"]["sha256"], "component final-seal pin mismatch")
    adoption_required = (
        component == "site-ingress"
        and action in SITE_ADOPTION_ACTIONS
        and mode == "execute"
    )
    require(
        adoption_required == (fps_adoption_seal is not None),
        "FPS adoption seal is required exactly for protected site-ingress actions",
    )
    if fps_adoption_seal is None:
        require(
            value["fpsAdoptionSealSha256"] is None,
            f"{component} {action} returned an unexpected FPS adoption-seal pin",
        )
    else:
        expected_adoption_pin = validate_pin(
            fps_adoption_seal,
            f"{component} {action} FPS adoption seal",
        )
        require(
            value["fpsAdoptionSealSha256"] == expected_adoption_pin["sha256"],
            f"{component} {action} FPS adoption-seal pin mismatch",
        )
    expected_driver = (
        plan["emergencyClosure"]["driver"]["sha256"]
        if action in {"close", "rollback"}
        else plan["drivers"][component]["sha256"]
    )
    require(value["driverSha256"] == expected_driver, "component driver pin mismatch")
    checks = exact_keys(
        value["checks"],
        expected_checks(component, action, mode),
        f"{component} {action} checks",
    )
    require(all(item is True for item in checks.values()), f"{component} {action} check failed")
    completed = parse_utc(value["completedAtUtc"], "component completion time")
    require(completed <= dt.datetime.now(dt.timezone.utc) + dt.timedelta(seconds=30), "component completion is in the future")
    return value


def validate_fps_deployment_receipt_for_adoption(
    plan: Mapping[str, Any],
    receipt_pin_value: Any,
) -> dict[str, str]:
    receipt_pin = validate_pin(receipt_pin_value, "FPS adoption deployment receipt")
    receipt_path = Path(receipt_pin["path"])
    receipt = read_json(receipt_path, "FPS adoption deployment receipt")
    require(
        receipt_path.read_bytes() == canonical_bytes(receipt),
        "FPS adoption deployment receipt is not canonical JSON",
    )
    require(
        receipt.get("schema") == "eterra.nexus-v2-fps-deployment-receipt.v1"
        and receipt.get("environment") == "private_alpha"
        and receipt.get("action") == "promote",
        "FPS adoption deployment receipt identity mismatch",
    )
    candidate = receipt.get("candidate")
    require(
        isinstance(candidate, dict)
        and candidate.get("candidateManifestSha256")
        == plan["unityFpsCandidateManifest"]["sha256"]
        and candidate.get("chainReleaseId") == plan["releaseId"],
        "FPS adoption deployment receipt candidate mismatch",
    )
    require(
        receipt.get("selectedDeploymentEnvironmentSha256")
        == plan["unityFpsDeploymentEnvironment"]["sha256"],
        "FPS adoption deployment receipt environment mismatch",
    )
    require(
        receipt.get("safety")
        == {
            "privateAlphaOnly": True,
            "chainRequired": True,
            "gameResultsV2Required": True,
            "paidEntry": False,
            "wagering": False,
            "permanentAssetLoss": False,
            "marketplace": False,
            "publicProduction": False,
        },
        "FPS adoption deployment receipt safety mismatch",
    )
    parse_utc(receipt.get("capturedAtUtc"), "FPS adoption receipt capture time")

    fps = exact_keys(
        plan["emergencyClosure"]["fps"],
        EMERGENCY_FPS_KEYS,
        "FPS emergency authority",
    )
    verifier_pins = {
        name: validate_pin(fps[name], f"FPS adoption {name}")
        for name in ("receiptVerifier", "candidateVerifier", "pinVerifier")
    }
    for name, pin in verifier_pins.items():
        require(os.access(pin["path"], os.X_OK), f"FPS adoption {name} is not executable")
    ssh_pins = validate_ssh_host_pins(
        plan["sshHostPins"], "FPS adoption SSH host pins"
    )
    environment_pin = validate_pin(
        plan["unityFpsDeploymentEnvironment"],
        "FPS adoption deployment environment",
    )
    command = [
        verifier_pins["receiptVerifier"]["path"],
        "verify-receipt",
        receipt_pin["path"],
        "--candidate",
        fps["candidateRoot"],
        "--snapshot",
        fps["snapshotPath"],
        "--environment",
        environment_pin["path"],
        "--known-hosts",
        ssh_pins["knownHosts"]["path"],
        "--host-pin-manifest",
        ssh_pins["manifest"]["path"],
        "--candidate-tool",
        verifier_pins["candidateVerifier"]["path"],
        "--pin-verifier",
        verifier_pins["pinVerifier"]["path"],
    ]
    completed = subprocess.run(
        command,
        capture_output=True,
        text=True,
        check=False,
        timeout=180,
        env=child_environment(),
    )
    require(
        completed.returncode == 0,
        "FPS adoption deployment receipt failed the pinned offline verifier",
    )
    return receipt_pin


def validate_fps_adoption_seal(
    path: Path,
    expected_sha256: str,
    plan: Mapping[str, Any],
    plan_sha256: str,
    *,
    now: dt.datetime | None = None,
) -> dict[str, str]:
    ensure_sha(expected_sha256, "FPS adoption seal SHA-256", nonzero=True)
    seal_pin = validate_pin(
        {"path": str(path), "sha256": expected_sha256},
        "FPS adoption seal",
    )
    seal = read_json(path, "FPS adoption seal")
    exact_keys(seal, FPS_ADOPTION_SEAL_KEYS, "FPS adoption seal")
    require(path.read_bytes() == canonical_bytes(seal), "FPS adoption seal is not canonical JSON")
    require(
        seal["schemaVersion"] == 1
        and seal["kind"] == "nexus-v2-private-alpha-post-fps-deployment-seal",
        "FPS adoption seal identity mismatch",
    )
    require(
        seal["operationId"] == plan["operationId"]
        and seal["planSha256"] == plan_sha256
        and seal["releaseId"] == plan["releaseId"],
        "FPS adoption seal plan/release mismatch",
    )
    require(
        seal["finalReleaseLockSha256"] == plan["finalReleaseLock"]["sha256"]
        and seal["candidateManifestSha256"]
        == plan["unityFpsCandidateManifest"]["sha256"]
        and seal["deploymentEnvironmentSha256"]
        == plan["unityFpsDeploymentEnvironment"]["sha256"],
        "FPS adoption seal release/candidate/environment mismatch",
    )
    require(
        seal["paidOrPublicProductionActivationAuthorized"] is False,
        "FPS adoption seal authorizes paid or public production",
    )
    current = now or dt.datetime.now(dt.timezone.utc)
    created = parse_utc(plan["createdAtUtc"], "reopen plan creation time")
    captured = parse_utc(seal["capturedAtUtc"], "FPS adoption seal capture time")
    expires = parse_utc(seal["expiresAtUtc"], "FPS adoption seal expiry")
    require(seal["expiresAtUtc"] == plan["expiresAtUtc"], "FPS adoption seal expiry mismatch")
    require(created <= captured <= expires, "FPS adoption seal timestamp is outside the plan lifetime")
    require(captured <= current + dt.timedelta(seconds=30), "FPS adoption seal is from the future")
    require(current <= expires, "FPS adoption seal is stale")

    verify_pin = validate_pin(seal["verifyResult"], "FPS adoption verify result")
    verify_result = validate_result(
        Path(verify_pin["path"]),
        plan,
        plan_sha256,
        "fps-server",
        "verify",
        "execute",
    )
    receipt_pin = validate_fps_deployment_receipt_for_adoption(
        plan, verify_result["componentReceipt"]
    )
    require(
        dict(seal["deploymentReceipt"]) == receipt_pin,
        "FPS adoption seal does not pin the verified deployment receipt",
    )
    verify_completed = parse_utc(
        verify_result["completedAtUtc"], "FPS adoption verification completion time"
    )
    require(
        verify_completed <= captured
        and captured - verify_completed <= dt.timedelta(minutes=5),
        "FPS adoption seal was not captured from a fresh FPS verification",
    )
    receipt = read_json(Path(receipt_pin["path"]), "FPS adoption deployment receipt")
    require(
        parse_utc(receipt["capturedAtUtc"], "FPS adoption receipt capture time")
        <= captured + dt.timedelta(seconds=30),
        "FPS adoption receipt postdates the seal",
    )

    promote_pin_value = seal["promoteResult"]
    if promote_pin_value is not None:
        promote_pin = validate_pin(promote_pin_value, "FPS adoption promote result")
        promote_result = validate_result(
            Path(promote_pin["path"]),
            plan,
            plan_sha256,
            "fps-server",
            "promote",
            "execute",
        )
        require(
            dict(promote_result["componentReceipt"]) == receipt_pin,
            "FPS adoption promotion and verification receipts differ",
        )
    return seal_pin


def invoke_driver(
    plan_path: Path,
    plan: Mapping[str, Any],
    plan_sha256: str,
    evidence_dir: Path,
    component: str,
    action: str,
    mode: str,
    peer_commit_result: Mapping[str, str] | None = None,
    fps_adoption_seal: Mapping[str, str] | None = None,
) -> tuple[dict[str, Any], dict[str, str]]:
    require(
        (action == "commit") == (peer_commit_result is not None),
        "peer commit result is required exactly for final component commit",
    )
    adoption_required = (
        component == "site-ingress"
        and action in SITE_ADOPTION_ACTIONS
        and mode == "execute"
    )
    require(
        adoption_required == (fps_adoption_seal is not None),
        "FPS adoption seal is required exactly for protected site-ingress actions",
    )
    driver = (
        plan["emergencyClosure"]["driver"]
        if action in {"close", "rollback"}
        else plan["drivers"][component]
    )
    filename = f"{len(list(evidence_dir.glob('*.result.json'))) + 1:02d}-{component}-{action}-{mode}.result.json"
    result_path = evidence_dir / filename
    command = [
        driver["path"],
        "--component", component,
        "--action", action,
        "--mode", mode,
        "--operation-id", plan["operationId"],
        "--plan", str(plan_path),
        "--plan-sha256", plan_sha256,
        "--result", str(result_path),
    ]
    if peer_commit_result is not None:
        peer_pin = validate_pin(peer_commit_result, "peer commit result")
        command.extend(
            [
                "--peer-commit-result",
                peer_pin["path"],
                "--peer-commit-result-sha256",
                peer_pin["sha256"],
            ]
        )
    adoption_pin: dict[str, str] | None = None
    if fps_adoption_seal is not None:
        supplied_adoption_pin = validate_pin(
            fps_adoption_seal,
            f"{component} {action} FPS adoption seal",
        )
        adoption_pin = validate_fps_adoption_seal(
            Path(supplied_adoption_pin["path"]),
            supplied_adoption_pin["sha256"],
            plan,
            plan_sha256,
        )
        command.extend(
            [
                "--fps-adoption-seal",
                adoption_pin["path"],
                "--fps-adoption-seal-sha256",
                adoption_pin["sha256"],
            ]
        )
    raw_ssh_pins = exact_keys(
        plan["sshHostPins"], SSH_HOST_PIN_KEYS, "driver SSH host pins"
    )
    ssh_pins = {
        name: validate_pin(raw_ssh_pins[name], f"driver SSH {name}")
        for name in SSH_HOST_PIN_KEYS
    }
    driver_environment = child_environment(
        {
            "NEXUS_V2_SSH_KNOWN_HOSTS_FILE": ssh_pins["knownHosts"]["path"],
            "NEXUS_V2_SSH_KNOWN_HOSTS_SHA256": ssh_pins["knownHosts"]["sha256"],
            "NEXUS_V2_SSH_HOST_PIN_MANIFEST": ssh_pins["manifest"]["path"],
            "NEXUS_V2_SSH_HOST_PIN_MANIFEST_SHA256": ssh_pins["manifest"]["sha256"],
        }
    )
    process = subprocess.Popen(
        command,
        env=driver_environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        start_new_session=True,
    )

    def terminate_process_group() -> None:
        if process.poll() is not None:
            return
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            return
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()

    previous_handlers: dict[int, Any] = {}

    def interrupted(signum: int, _frame: Any) -> None:
        terminate_process_group()
        raise ReopenError(
            f"{component} {action} {mode} driver interrupted by signal {signum}"
        )

    for signum in (signal.SIGHUP, signal.SIGINT, signal.SIGTERM):
        previous_handlers[signum] = signal.signal(signum, interrupted)
    try:
        try:
            stdout, stderr = process.communicate(timeout=600)
        except subprocess.TimeoutExpired as exc:
            terminate_process_group()
            raise ReopenError(f"{component} {action} {mode} driver timed out") from exc
    except BaseException:
        terminate_process_group()
        raise
    finally:
        for signum, handler in previous_handlers.items():
            signal.signal(signum, handler)
    completed = subprocess.CompletedProcess(command, process.returncode, stdout, stderr)
    require(completed.returncode == 0, f"{component} {action} {mode} driver failed")
    require(result_path.is_file() and not result_path.is_symlink(), f"{component} driver did not create a result")
    result = validate_result(
        result_path,
        plan,
        plan_sha256,
        component,
        action,
        mode,
        fps_adoption_seal=adoption_pin,
    )
    return result, {"path": str(result_path), "sha256": sha256_file(result_path)}


def prepare_evidence_dir(path: Path) -> None:
    require(path.is_absolute(), "evidence directory must be absolute")
    require(not path.exists() and not path.is_symlink(), "evidence directory must be new")
    path.mkdir(parents=True, mode=0o700)
    os.chmod(path, 0o700)


def execute_sequence(
    plan_path: Path,
    plan: Mapping[str, Any],
    plan_sha256: str,
    evidence_dir: Path,
    sequence: Sequence[tuple[str, str, str]],
    runner: Callable[..., tuple[dict[str, Any], dict[str, str]]] = invoke_driver,
) -> list[dict[str, Any]]:
    steps: list[dict[str, Any]] = []
    for component, action, mode in sequence:
        result, pin = runner(plan_path, plan, plan_sha256, evidence_dir, component, action, mode)
        steps.append({"componentId": component, "action": action, "mode": mode, "result": pin, "alreadyApplied": result["alreadyApplied"]})
    return steps


def close_sequence() -> list[tuple[str, str, str]]:
    return [
        ("chain-transport", "close", "execute"),
        ("fps-server", "rollback", "execute"),
        ("site-ingress", "close", "execute"),
    ]


def capture_fps_adoption_seal(
    evidence_dir: Path,
    plan: Mapping[str, Any],
    plan_sha256: str,
    verify_result: Mapping[str, Any],
    verify_result_pin: Mapping[str, str],
    promote_result: Mapping[str, Any] | None,
    promote_result_pin: Mapping[str, str] | None,
) -> dict[str, str]:
    deployment_receipt = validate_pin(
        verify_result.get("componentReceipt"), "verified FPS deployment receipt"
    )
    if promote_result is not None:
        promoted_receipt = validate_pin(
            promote_result.get("componentReceipt"), "promoted FPS deployment receipt"
        )
        require(
            promoted_receipt["sha256"] == deployment_receipt["sha256"],
            "FPS verification did not bind the promoted deployment receipt",
        )
    require(
        (promote_result is None) == (promote_result_pin is None),
        "FPS promote result pin is incomplete",
    )
    seal = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-post-fps-deployment-seal",
        "operationId": plan["operationId"],
        "planSha256": plan_sha256,
        "releaseId": plan["releaseId"],
        "finalReleaseLockSha256": plan["finalReleaseLock"]["sha256"],
        "candidateManifestSha256": plan["unityFpsCandidateManifest"]["sha256"],
        "deploymentEnvironmentSha256": plan["unityFpsDeploymentEnvironment"]["sha256"],
        "deploymentReceipt": deployment_receipt,
        "promoteResult": dict(promote_result_pin) if promote_result_pin else None,
        "verifyResult": dict(validate_pin(verify_result_pin, "FPS verify result")),
        "paidOrPublicProductionActivationAuthorized": False,
        "capturedAtUtc": utc_now(),
        "expiresAtUtc": plan["expiresAtUtc"],
    }
    exact_keys(seal, FPS_ADOPTION_SEAL_KEYS, "post-FPS deployment seal")
    output = evidence_dir / "fps-adoption-seal.json"
    write_new(output, seal)
    output_pin = file_pin(str(output), "post-FPS deployment seal", canonical_json=True)
    return validate_fps_adoption_seal(
        output,
        output_pin["sha256"],
        plan,
        plan_sha256,
    )


def validate_fresh_fps_verification_against_adoption_seal(
    plan: Mapping[str, Any],
    plan_sha256: str,
    verify_result: Mapping[str, Any],
    adoption_seal_pin: Mapping[str, str],
) -> dict[str, str]:
    supplied_seal_pin = validate_pin(
        adoption_seal_pin,
        "active FPS adoption seal",
    )
    validated_seal_pin = validate_fps_adoption_seal(
        Path(supplied_seal_pin["path"]),
        supplied_seal_pin["sha256"],
        plan,
        plan_sha256,
    )
    seal = read_json(Path(validated_seal_pin["path"]), "active FPS adoption seal")
    current_receipt_pin = validate_fps_deployment_receipt_for_adoption(
        plan,
        verify_result.get("componentReceipt"),
    )
    require(
        dict(seal["deploymentReceipt"]) == current_receipt_pin,
        "fresh FPS verification differs from the immutable adopted deployment receipt",
    )
    return validated_seal_pin


def command_operate(args: argparse.Namespace) -> None:
    plan_path = Path(args.plan)
    plan = load_plan(
        plan_path,
        args.expected_sha256,
        allow_expired=args.command in {"close", "validate-close"},
        closure_only=args.command in {"close", "validate-close"},
    )
    if args.command in {"validate", "validate-close"}:
        stage = "closure authority" if args.command == "validate-close" else "plan"
        print(f"restricted reopen {stage} verified: sha256={args.expected_sha256}")
        return
    require(os.environ.get("NEXUS_V2_POST_ACCEPTANCE_REOPEN_CONFIRMATION") == "PRIVATE_ALPHA_RESTRICTED_REOPEN", "protected reopen confirmation is missing")
    initial_fps_adoption_seal_pin: dict[str, str] | None = None
    if args.command == "verify":
        seal_path = getattr(args, "fps_adoption_seal", None)
        seal_sha256 = getattr(args, "fps_adoption_seal_sha256", None)
        require(
            isinstance(seal_path, str) and isinstance(seal_sha256, str),
            "active verification requires the immutable FPS adoption seal and SHA-256",
        )
        initial_fps_adoption_seal_pin = validate_fps_adoption_seal(
            Path(seal_path),
            seal_sha256,
            plan,
            args.expected_sha256,
        )
    evidence_dir = Path(args.evidence_dir)
    if args.command == "close":
        require(evidence_dir.is_absolute(), "reopen evidence directory must be absolute")
        candidate = evidence_dir.resolve(strict=False)
        closure_root = Path(plan["emergencyClosure"]["bundleRoot"]).resolve()
        require(candidate != closure_root and closure_root not in candidate.parents, "close evidence may not overwrite closure authority")
        require(candidate != REPO_ROOT and REPO_ROOT not in candidate.parents, "close evidence must remain outside the coordinator source")
    else:
        require_outside_pinned_repositories(evidence_dir, plan, "reopen evidence directory")
    prepare_evidence_dir(evidence_dir)
    if args.command == "execute":
        sequence = [
            ("chain-transport", "preflight", "dry-run"),
            ("fps-server", "preflight", "dry-run"),
            ("site-ingress", "preflight", "dry-run"),
            ("chain-transport", "preflight", "execute"),
            ("fps-server", "preflight", "execute"),
            ("site-ingress", "preflight", "execute"),
            ("chain-transport", "adopt", "execute"),
            ("fps-server", "promote", "execute"),
            ("fps-server", "verify", "execute"),
            ("site-ingress", "open", "execute"),
            ("chain-transport", "verify", "execute"),
            ("site-ingress", "verify", "execute"),
            ("site-ingress", "prepare-commit", "execute"),
            ("site-ingress", "commit", "execute"),
            ("chain-transport", "commit", "execute"),
        ]
        decision = "restricted-reopen-active"
    elif args.command == "verify":
        sequence = [
            ("chain-transport", "verify", "execute"),
            ("fps-server", "verify", "execute"),
            ("site-ingress", "verify", "execute"),
        ]
        decision = "restricted-reopen-verified"
    else:
        sequence = close_sequence()
        decision = "phase1-transport-restored"
    steps: list[dict[str, Any]] = []
    mutation_attempted = False
    # Verify/close operate on an already promoted deployment. During execute,
    # arm rollback before the promotion call so a remote commit followed by
    # lost stdout/transport still invokes the idempotent emergency rollback.
    fps_promotion_attempted = args.command in {"verify", "close"}
    fps_promote_result: dict[str, Any] | None = None
    fps_promote_result_pin: dict[str, str] | None = None
    fps_adoption_seal_pin = initial_fps_adoption_seal_pin
    site_prepare_result_pin: dict[str, str] | None = None
    site_commit_result_pin: dict[str, str] | None = None
    try:
        for component, action, mode in sequence:
            if component == "site-ingress" and action == "open":
                require(
                    fps_adoption_seal_pin is not None,
                    "site ingress cannot open before the immutable post-FPS deployment seal",
                )
                fps_adoption_seal_pin = validate_pin(
                    fps_adoption_seal_pin,
                    "post-FPS deployment seal before site ingress open",
                )
            if action in {"adopt", "promote", "open", "rollback", "close"} and mode == "execute":
                mutation_attempted = True
            if component == "fps-server" and action == "promote" and mode == "execute":
                fps_promotion_attempted = True
            peer_commit_result = (
                site_prepare_result_pin
                if component == "site-ingress" and action == "commit"
                else site_commit_result_pin
                if component == "chain-transport" and action == "commit"
                else None
            )
            require(
                not (component == "chain-transport" and action == "commit")
                or peer_commit_result is not None,
                "chain watchdog commit requires the verified final site-ingress commit result",
            )
            require(
                not (component == "site-ingress" and action == "commit")
                or peer_commit_result is not None,
                "site watchdog commit requires its verified durable prepare result",
            )
            step_adoption_seal = (
                fps_adoption_seal_pin
                if component == "site-ingress"
                and action in SITE_ADOPTION_ACTIONS
                and mode == "execute"
                else None
            )
            result, pin = invoke_driver(
                plan_path,
                plan,
                args.expected_sha256,
                evidence_dir,
                component,
                action,
                mode,
                peer_commit_result=peer_commit_result,
                fps_adoption_seal=step_adoption_seal,
            )
            steps.append({"componentId": component, "action": action, "mode": mode, "result": pin, "alreadyApplied": result["alreadyApplied"]})
            if component == "site-ingress" and action == "prepare-commit":
                site_prepare_result_pin = pin
            elif component == "site-ingress" and action == "commit":
                site_commit_result_pin = pin
            elif component == "fps-server" and action == "promote":
                fps_promote_result = result
                fps_promote_result_pin = pin
            elif component == "fps-server" and action == "verify":
                if args.command == "execute":
                    fps_adoption_seal_pin = capture_fps_adoption_seal(
                        evidence_dir,
                        plan,
                        args.expected_sha256,
                        result,
                        pin,
                        fps_promote_result,
                        fps_promote_result_pin,
                    )
                else:
                    require(
                        fps_adoption_seal_pin is not None,
                        "active verification lost the immutable FPS adoption seal",
                    )
                    fps_adoption_seal_pin = (
                        validate_fresh_fps_verification_against_adoption_seal(
                            plan,
                            args.expected_sha256,
                            result,
                            fps_adoption_seal_pin,
                        )
                    )
    except ReopenError as exc:
        if args.command == "close":
            closure_errors = [str(exc)]
            failed_step = (component, action, mode)
            failed_index = sequence.index(failed_step)
            for close_component, close_action, close_mode in sequence[failed_index + 1 :]:
                try:
                    result, pin = invoke_driver(
                        plan_path,
                        plan,
                        args.expected_sha256,
                        evidence_dir,
                        close_component,
                        close_action,
                        close_mode,
                    )
                    steps.append(
                        {
                            "componentId": close_component,
                            "action": close_action,
                            "mode": close_mode,
                            "result": pin,
                            "alreadyApplied": result["alreadyApplied"],
                        }
                    )
                except ReopenError as close_exc:
                    closure_errors.append(str(close_exc))
            failure = {
                "schemaVersion": 1,
                "kind": "nexus-v2-private-alpha-post-acceptance-reopen-failure",
                "operationId": plan["operationId"],
                "planSha256": args.expected_sha256,
                "releaseId": plan["releaseId"],
                "siteReleaseVersion": plan["siteReleaseVersion"],
                "failedReason": str(exc),
                "transportCloseCompleted": False,
                "closeErrors": closure_errors,
                "chainStateMutationPerformed": False,
                "chainStateRollbackPerformed": False,
                "paidOrPublicProductionActivationAuthorized": False,
                "steps": steps,
                "completedAtUtc": utc_now(),
            }
            write_new(evidence_dir / "reopen-failure.json", failure)
            raise ReopenError(
                "transport close is incomplete; every remaining host was attempted and manual fail-closed intervention is required"
            ) from exc
        if (args.command == "execute" and mutation_attempted) or args.command == "verify":
            closure_errors: list[str] = []
            failure_close_sequence = [
                ("chain-transport", "close", "execute"),
                *(
                    [("fps-server", "rollback", "execute")]
                    if fps_promotion_attempted
                    else []
                ),
                ("site-ingress", "close", "execute"),
            ]
            for component, action, mode in failure_close_sequence:
                try:
                    result, pin = invoke_driver(plan_path, plan, args.expected_sha256, evidence_dir, component, action, mode)
                    steps.append({"componentId": component, "action": action, "mode": mode, "result": pin, "alreadyApplied": result["alreadyApplied"]})
                except ReopenError as close_exc:
                    closure_errors.append(str(close_exc))
            failure = {
                "schemaVersion": 1,
                "kind": "nexus-v2-private-alpha-post-acceptance-reopen-failure",
                "operationId": plan["operationId"],
                "planSha256": args.expected_sha256,
                "releaseId": plan["releaseId"],
                "siteReleaseVersion": plan["siteReleaseVersion"],
                "failedReason": str(exc),
                "transportCloseCompleted": not closure_errors,
                "closeErrors": closure_errors,
                "chainStateMutationPerformed": False,
                "chainStateRollbackPerformed": False,
                "paidOrPublicProductionActivationAuthorized": False,
                "steps": steps,
                "completedAtUtc": utc_now(),
            }
            failure_path = evidence_dir / "reopen-failure.json"
            write_new(failure_path, failure)
            if closure_errors:
                raise ReopenError("restricted reopen failed and transport close is incomplete; manual fail-closed intervention required") from exc
            raise ReopenError("restricted reopen failed; Phase-1 transport was restored") from exc
        raise
    if args.command in {"execute", "verify"}:
        require(
            fps_adoption_seal_pin is not None,
            "successful restricted reopen evidence requires a post-FPS deployment seal",
        )
        fps_adoption_seal_pin = validate_pin(
            fps_adoption_seal_pin,
            "post-FPS deployment seal before final evidence",
        )
    evidence = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-post-acceptance-reopen-evidence",
        "operationId": plan["operationId"],
        "planSha256": args.expected_sha256,
        "releaseId": plan["releaseId"],
        "siteReleaseVersion": plan["siteReleaseVersion"],
        "sourceCommit": plan["sourceCommit"],
        "siteSourceCommit": plan["siteSourceCommit"],
        "decision": decision,
        "finalReleaseLockSha256": plan["finalReleaseLock"]["sha256"],
        "acceptanceBoundaryReceiptSha256": plan["acceptanceBoundaryReceipt"]["sha256"],
        "phase2FinalSealSha256": plan["phase2FinalSeal"]["sha256"],
        "fpsAdoptionSeal": fps_adoption_seal_pin,
        "transport": {
            "allowedSourceIp": plan["network"]["siteLanIp"],
            "chainHostIp": plan["network"]["chainLanIp"],
            "exposedPorts": [4000, 8080, 8787, 9944] if args.command != "close" else [],
            "forbiddenPorts": [30333, 5001],
            "underlyingBackendsLoopbackOnly": True,
        },
        "steps": steps,
        "chainStateMutationPerformed": False,
        "chainStateRollbackPerformed": False,
        "paidOrPublicProductionActivationAuthorized": False,
        "completedAtUtc": utc_now(),
    }
    exact_keys(evidence, EVIDENCE_KEYS, "reopen evidence")
    evidence_path = evidence_dir / "reopen-evidence.json"
    write_new(evidence_path, evidence)
    print(f"{decision}: {evidence_path} sha256={sha256_file(evidence_path)}")


def add_plan_input_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--plan", required=True)
    parser.add_argument("--expected-sha256", required=True)


def command_validate_adoption_seal(args: argparse.Namespace) -> None:
    plan_path = Path(args.plan)
    plan = load_plan(plan_path, args.expected_sha256)
    pin = validate_fps_adoption_seal(
        Path(args.fps_adoption_seal),
        args.fps_adoption_seal_sha256,
        plan,
        args.expected_sha256,
    )
    print(f"FPS adoption seal verified: sha256={pin['sha256']}")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    capture = commands.add_parser("capture-plan", help="capture the immutable offline reopen plan")
    capture.add_argument("--operation-id", required=True)
    capture.add_argument("--final-release-lock", required=True)
    capture.add_argument("--phase2-final-seal", required=True)
    capture.add_argument("--phase2-bootstrap-prerequisite", required=True)
    capture.add_argument("--authority-manifest", required=True)
    capture.add_argument("--media-smoke-path", required=True)
    capture.add_argument("--media-smoke-sha256", required=True)
    capture.add_argument("--ipfs-smoke-path", required=True)
    capture.add_argument("--ipfs-smoke-sha256", required=True)
    capture.add_argument("--created-at")
    capture.add_argument("--expires-at")
    capture.add_argument("--output", required=True)
    capture.set_defaults(func=command_capture)
    validate = commands.add_parser("validate", help="verify all authority without host contact")
    add_plan_input_arguments(validate)
    validate.set_defaults(func=command_operate)
    validate_close = commands.add_parser(
        "validate-close",
        help="verify exact closure authority even after plan expiry",
    )
    add_plan_input_arguments(validate_close)
    validate_close.set_defaults(func=command_operate)
    validate_adoption = commands.add_parser(
        "validate-adoption-seal",
        help="verify the immutable post-FPS adoption authority without host contact",
    )
    add_plan_input_arguments(validate_adoption)
    validate_adoption.add_argument("--fps-adoption-seal", required=True)
    validate_adoption.add_argument("--fps-adoption-seal-sha256", required=True)
    validate_adoption.set_defaults(func=command_validate_adoption_seal)
    for name, help_text in (
        ("execute", "open and verify the restricted transport"),
        ("close", "restore the Phase-1 transport boundary"),
    ):
        command = commands.add_parser(name, help=help_text)
        add_plan_input_arguments(command)
        command.add_argument("--evidence-dir", required=True)
        command.set_defaults(func=command_operate)
    verify = commands.add_parser(
        "verify",
        help="reverify the active restricted transport against its immutable FPS adoption seal",
    )
    add_plan_input_arguments(verify)
    verify.add_argument("--evidence-dir", required=True)
    verify.add_argument("--fps-adoption-seal", required=True)
    verify.add_argument("--fps-adoption-seal-sha256", required=True)
    verify.set_defaults(func=command_operate)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        args.func(args)
    except (ReopenError, OSError, subprocess.SubprocessError) as exc:
        print(f"nexus-v2-post-acceptance-reopen: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
