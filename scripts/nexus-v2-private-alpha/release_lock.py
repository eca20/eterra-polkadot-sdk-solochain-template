#!/usr/bin/env python3
"""Capture or verify Nexus V2 private-alpha replacement/release locks.

The pre-cutover replacement lock binds the immutable replacement inputs needed
by offline final-freeze preflight.  It deliberately makes no post-cutover
read-model or acceptance-receipt claim.  The final release lock is a separate,
strictly post-receipt contract: it additionally binds the acceptance-boundary
receipt and a read-model manifest that names that exact receipt.
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
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import Any, Mapping, Sequence


TOOL_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOL_DIR))
import deployment_secret_environment  # noqa: E402,F401
import acceptance_boundary  # noqa: E402
import authority_candidate  # noqa: E402
import capture_ssh_host_pins as ssh_host_pins  # noqa: E402


SHA_RE = re.compile(r"^[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
HASH256_RE = re.compile(r"^0x[0-9a-f]{64}$")
ID_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
UTC_RE = re.compile(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$")
REPOSITORY_IDS = {
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
LOCK_KEYS = {"schemaVersion", "kind", "releaseId", "createdAtUtc", "repositories", "artifacts", "policy"}
REPOSITORY_KEYS = {"root", "head", "tree"}
COMMON_ARTIFACT_KEYS = {
    "deploymentEnvironment",
    "siteDeploymentEnvironment",
    "unityFpsDeploymentEnvironment",
    "sshKnownHosts",
    "sshHostPinManifest",
    "forbiddenDeploymentEnvironments",
    "runtimeBundleManifest",
    "targetIdentity",
    "nodeCandidateManifest",
    "mediaCandidateManifest",
    "authorityCandidateManifest",
    "siteDeploymentCandidateManifest",
    "snapshotManifest",
    "unityTestResults",
}
REPLACEMENT_ARTIFACT_KEYS = set(COMMON_ARTIFACT_KEYS)
FINAL_ARTIFACT_KEYS = COMMON_ARTIFACT_KEYS | {
    "replacementLock",
    "acceptanceBoundaryReceipt",
    "readModelManifest",
    "unityFpsCandidateManifest",
    "sitePhase1PostDeployIdentity",
    "fullLoopIndexerActivationReceipt",
    "sitePostPhase2DeploymentIdentity",
    "phase2InternalTransportHandoff",
}
# Retain the public constant for downstream importers; it now names the final
# post-receipt lock's closed artifact set.
ARTIFACT_KEYS = FINAL_ARTIFACT_KEYS
FILE_PIN_KEYS = {"path", "sha256"}
UNITY_RESULT_KEYS = {"path", "sha256", "mode", "result", "total", "passed", "failed"}
FINAL_LOCK_KIND = "nexus-v2-private-alpha-release-lock"
REPLACEMENT_LOCK_KIND = "nexus-v2-private-alpha-pre-cutover-replacement-lock"
READ_MODEL_ACCEPTANCE_KEYS = {
    "automaticRestorePermanentlyDisabled",
    "chainSourceCommit",
    "coordinatorDecision",
    "genesisHash",
    "observedAtFinalizedBlock",
    "receiptKind",
    "receiptSha256",
    "releaseId",
    "runtimeCodeSha256",
    "runtimeMetadataScaleSha256",
}
POLICY = {
    "liveHostContactAuthorized": False,
    "paidOrPublicActivationAuthorized": False,
    "staleEnvironmentSelectionAllowed": False,
}

SITE_CANDIDATE_KEYS = {
    "schemaVersion",
    "releaseVersion",
    "candidateSourceCommit",
    "siteBuildHash",
    "siteImageRef",
    "siteImageId",
    "indexerImageRef",
    "indexerImageId",
}
PHASE2_TRANSPORT_HANDOFF_KEYS = {
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
}


class ReleaseLockError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ReleaseLockError(message)


def canonical_bytes(value: Mapping[str, Any]) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")


def duplicate_rejecting_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        require(key not in value, f"duplicate JSON field: {key}")
        value[key] = item
    return value


def decode_json_object(raw: bytes, label: str) -> dict[str, Any]:
    try:
        value = json.loads(
            raw.decode("utf-8"), object_pairs_hook=duplicate_rejecting_object
        )
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ReleaseLockError(f"invalid {label}") from exc
    require(isinstance(value, dict), f"{label} root must be an object")
    return value


def canonical_existing_path(path: Path, label: str, *, directory: bool = False) -> Path:
    require(path.is_absolute() and ".." not in path.parts, f"{label} path must be canonical and absolute")
    try:
        resolved = path.resolve(strict=True)
    except OSError as exc:
        raise ReleaseLockError(f"{label} is unavailable") from exc
    require(path == resolved, f"{label} path is not canonical or traverses a symlink")
    cursor = path
    while cursor != cursor.parent:
        try:
            observed = os.lstat(cursor)
        except OSError as exc:
            raise ReleaseLockError(f"{label} is unavailable") from exc
        require(not stat.S_ISLNK(observed.st_mode), f"{label} path traverses a symlink")
        cursor = cursor.parent
    observed = os.lstat(path)
    expected_kind = stat.S_ISDIR if directory else stat.S_ISREG
    require(expected_kind(observed.st_mode), f"{label} has the wrong file type")
    return path


def read_stable_regular_file(path: Path, label: str) -> bytes:
    path = canonical_existing_path(path, label)
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(path, flags)
    except OSError as exc:
        raise ReleaseLockError(f"cannot open {label}") from exc
    try:
        before = os.fstat(descriptor)
        require(stat.S_ISREG(before.st_mode), f"{label} is not a regular file")
        chunks: list[bytes] = []
        while True:
            chunk = os.read(descriptor, 1024 * 1024)
            if not chunk:
                break
            chunks.append(chunk)
        after = os.fstat(descriptor)
    finally:
        os.close(descriptor)
    observed = os.lstat(path)

    def identity(item: os.stat_result) -> tuple[int, int, int, int, int]:
        return (
            item.st_dev,
            item.st_ino,
            item.st_size,
            item.st_mtime_ns,
            item.st_ctime_ns,
        )

    require(
        identity(before) == identity(after) == identity(observed),
        f"{label} changed while it was read",
    )
    return b"".join(chunks)


def read_json(path: Path, label: str) -> dict[str, Any]:
    return decode_json_object(read_stable_regular_file(path, label), label)


def exact_keys(value: Any, keys: set[str], label: str) -> Mapping[str, Any]:
    require(isinstance(value, dict) and set(value) == keys, f"{label} does not match the closed schema")
    return value


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def ensure_sha(value: Any, label: str) -> str:
    require(isinstance(value, str) and SHA_RE.fullmatch(value) is not None, f"invalid {label}")
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


def git_output(root: Path, *arguments: str) -> str:
    completed = subprocess.run(
        ["git", "-C", str(root), *arguments], capture_output=True, text=True, check=False
    )
    require(completed.returncode == 0, f"cannot inspect repository: {root}")
    return completed.stdout.strip()


def repository_pin(root_value: str, label: str) -> dict[str, str]:
    root = canonical_existing_path(Path(root_value), f"{label} root", directory=True)
    git_root = canonical_existing_path(
        Path(git_output(root, "rev-parse", "--show-toplevel")),
        f"{label} Git root",
        directory=True,
    )
    require(
        git_root == root,
        f"{label} must name the Git worktree root",
    )
    require(
        git_output(root, "status", "--porcelain", "--untracked-files=all") == "",
        f"{label} worktree is dirty",
    )
    return {
        "root": str(root),
        "head": ensure_commit(git_output(root, "rev-parse", "HEAD"), f"{label} HEAD"),
        "tree": ensure_commit(git_output(root, "rev-parse", "HEAD^{tree}"), f"{label} tree"),
    }


def file_pin(path_value: str, label: str, *, canonical_json: bool = False) -> dict[str, str]:
    path = canonical_existing_path(Path(path_value), label)
    raw = read_stable_regular_file(path, label)
    if canonical_json:
        value = decode_json_object(raw, label)
        require(raw == canonical_bytes(value), f"{label} is not canonical JSON")
    return {"path": str(path), "sha256": hashlib.sha256(raw).hexdigest()}


def parse_repository_arguments(values: Sequence[str]) -> dict[str, str]:
    result: dict[str, str] = {}
    for value in values:
        identifier, separator, root = value.partition("=")
        require(separator == "=" and identifier in REPOSITORY_IDS and root, "invalid --repository value")
        require(identifier not in result, f"duplicate repository: {identifier}")
        result[identifier] = root
    require(set(result) == REPOSITORY_IDS, "release lock must pin exactly nine repositories")
    return result


def parse_environment(path: Path) -> dict[str, str]:
    raw_bytes = read_stable_regular_file(path, "deployment environment")
    try:
        text = raw_bytes.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise ReleaseLockError("deployment environment is not UTF-8") from exc
    require("\r" not in text and "\x00" not in text, "deployment environment contains control characters")
    result: dict[str, str] = {}
    for line_number, raw in enumerate(text.splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        require("=" in line, f"deployment environment line {line_number} is not an assignment")
        key, value = line.split("=", 1)
        key = key.strip()
        require(
            re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", key) is not None,
            f"deployment environment line {line_number} has an invalid key",
        )
        require(key not in result, f"deployment environment has duplicate key: {key}")
        value = value.strip()
        if value[:1] in {"'", '"'} or value[-1:] in {"'", '"'}:
            require(
                len(value) >= 2 and value[0] == value[-1],
                f"deployment environment line {line_number} has unmatched quotes",
            )
            value = value[1:-1]
        require(
            all(ord(character) >= 0x20 and character != "\x7f" for character in value),
            f"deployment environment line {line_number} contains control characters",
        )
        result[key] = value
    return result


def unity_result(path_value: str, mode: str) -> dict[str, Any]:
    label = f"Unity {mode} result"
    path = canonical_existing_path(Path(path_value), label)
    raw = read_stable_regular_file(path, label)
    pin = {"path": str(path), "sha256": hashlib.sha256(raw).hexdigest()}
    try:
        root = ET.fromstring(raw)
    except ET.ParseError as exc:
        raise ReleaseLockError(f"invalid Unity {mode} result XML") from exc
    require(root.tag == "test-run", f"Unity {mode} result root mismatch")
    try:
        total = int(root.attrib["total"])
        passed = int(root.attrib["passed"])
        failed = int(root.attrib["failed"])
    except (KeyError, ValueError) as exc:
        raise ReleaseLockError(f"Unity {mode} result counts are invalid") from exc
    require(root.attrib.get("result") == "Passed", f"Unity {mode} result did not pass")
    require(total > 0 and passed == total and failed == 0, f"Unity {mode} result is not fully green")
    return {**pin, "mode": mode, "result": "Passed", "total": total, "passed": passed, "failed": failed}


def validate_unity_fps_candidate(
    manifest_path: Path,
    repositories: Mapping[str, Any],
    release_id: str,
    genesis_hash: str,
    runtime_code_sha256: str,
    metadata_scale_sha256: str,
    acceptance_receipt_sha256: str,
) -> dict[str, Any]:
    require(
        manifest_path.is_absolute()
        and manifest_path.name == "candidate-manifest.json"
        and manifest_path.is_file()
        and not manifest_path.is_symlink()
        and manifest_path.parent.is_dir()
        and not manifest_path.parent.is_symlink(),
        "Unity FPS candidate manifest path is invalid",
    )
    unity_root = Path(repositories["unity"]["root"])
    verifier = unity_root / "scripts/release/fps-server-candidate.py"
    require(verifier.is_file() and not verifier.is_symlink(), "Unity FPS candidate verifier is unavailable")
    completed = subprocess.run(
        [sys.executable, str(verifier), "verify", str(manifest_path.parent)],
        capture_output=True,
        text=True,
        check=False,
    )
    require(completed.returncode == 0, "Unity FPS candidate verifier rejected the final candidate")
    value = read_json(manifest_path, "Unity FPS candidate manifest")
    compact = (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode("utf-8")
    require(manifest_path.read_bytes() == compact, "Unity FPS candidate manifest is not canonical compact JSON")
    require(
        value.get("schema") == "eterra.nexus-v2-fps-dedicated-server-candidate.v2"
        and value.get("environment") == "private_alpha",
        "Unity FPS candidate identity mismatch",
    )
    source = value.get("source")
    require(
        isinstance(source, dict)
        and source.get("repository") == "Eterra-Arcade-Unity"
        and source.get("commit") == repositories["unity"]["head"]
        and source.get("tree") == repositories["unity"]["tree"],
        "Unity FPS candidate source pin is stale",
    )
    sdk = value.get("sdk")
    require(
        isinstance(sdk, dict)
        and COMMIT_RE.fullmatch(str(sdk.get("commit", ""))) is not None
        and SHA_RE.fullmatch(str(sdk.get("manifest_sha256", ""))) is not None
        and SHA_RE.fullmatch(str(sdk.get("metadata_json_sha256", ""))) is not None,
        "Unity FPS candidate SDK artifact pin is invalid",
    )
    unity_sdk_manifest_path = manifest_path.parent / "evidence/unity-sdk-manifest.json"
    require(
        unity_sdk_manifest_path.is_file()
        and not unity_sdk_manifest_path.is_symlink()
        and sha256_file(unity_sdk_manifest_path) == sdk["manifest_sha256"],
        "Unity FPS candidate SDK manifest bytes are stale",
    )
    unity_sdk_manifest = read_json(unity_sdk_manifest_path, "Unity FPS candidate SDK manifest")
    unity_sdk_source = unity_sdk_manifest.get("sdkSource")
    require(
        isinstance(unity_sdk_source, dict)
        and unity_sdk_source.get("commit") == sdk["commit"],
        "Unity FPS candidate SDK source does not match its frozen Unity SDK artifact",
    )
    runtime = value.get("runtime")
    require(
        isinstance(runtime, dict)
        and runtime.get("spec_version") == 106
        and runtime.get("chain_release_id") == release_id
        and runtime.get("deployment_source_commit") == repositories["chain"]["head"]
        and runtime.get("genesis_hash") == genesis_hash
        and runtime.get("runtime_code_sha256") == runtime_code_sha256
        and runtime.get("runtime_metadata_scale_sha256") == metadata_scale_sha256,
        "Unity FPS candidate runtime pins are stale",
    )
    proof = value.get("game_results_acceptance")
    require(
        isinstance(proof, dict)
        and proof.get("acceptance_boundary_sha256") == acceptance_receipt_sha256
        and proof.get("proof_policy_deactivated") is True,
        "Unity FPS candidate does not bind the final acceptance boundary and deactivation proof",
    )
    safety = value.get("safety")
    for name in ("paid_entry", "wagering", "permanent_asset_loss", "marketplace", "public_production"):
        require(isinstance(safety, dict) and safety.get(name) is False, f"Unity FPS candidate must disable {name}")
    require(
        safety.get("economic_realm") == "Training"
        and safety.get("normalized_legacy_rejects_persistent_power") is True,
        "Unity FPS candidate safety policy is stale",
    )
    return value


def run_checked(command: Sequence[str], label: str) -> None:
    completed = subprocess.run(
        list(command), capture_output=True, text=True, check=False, timeout=120
    )
    require(completed.returncode == 0, f"{label} rejected the release evidence")


def validate_site_candidate(
    path: Path,
    repositories: Mapping[str, Any],
) -> dict[str, Any]:
    value = exact_keys(read_json(path, "site deployment candidate"), SITE_CANDIDATE_KEYS, "site deployment candidate")
    require(value["schemaVersion"] == 1, "site deployment candidate version mismatch")
    require(
        value["candidateSourceCommit"] == repositories["web"]["head"],
        "site deployment candidate source is stale",
    )
    require(
        isinstance(value["releaseVersion"], str)
        and re.fullmatch(r"v[A-Za-z0-9][A-Za-z0-9._-]{0,126}", value["releaseVersion"]),
        "site deployment candidate release is invalid",
    )
    ensure_sha(value["siteBuildHash"], "site build hash")
    for field in ("siteImageId", "indexerImageId"):
        require(
            isinstance(value[field], str)
            and re.fullmatch(r"sha256:[0-9a-f]{64}", value[field]),
            f"site deployment candidate {field} is invalid",
        )
    for field in ("siteImageRef", "indexerImageRef"):
        require(
            isinstance(value[field], str)
            and re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._/@:-]{0,255}", value[field]),
            f"site deployment candidate {field} is invalid",
        )
    return dict(value)


def validate_unity_fps_environment(
    path: Path,
    repositories: Mapping[str, Any],
    artifacts: Mapping[str, Any],
    target: Mapping[str, Any],
    release_id: str,
) -> dict[str, str]:
    environment = parse_environment(path)
    deployment_node = target.get("deploymentNode")
    require(isinstance(deployment_node, dict), "target identity deployment-node contract is invalid")
    node = read_json(Path(artifacts["nodeCandidateManifest"]["path"]), "node candidate manifest")
    runtime_bundle = node.get("runtimeBundle")
    require(isinstance(runtime_bundle, dict), "node candidate runtime bundle is invalid")
    target_platform = target.get("targetPlatform")
    require(isinstance(target_platform, dict), "target identity platform contract is invalid")
    target_platform_sha256 = hashlib.sha256(
        json.dumps(target_platform, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()
    expected = {
        "DEPLOY_HOST": "192.168.1.218",
        "DEPLOY_USER": "eterra2014",
        "SSH_PORT": "22",
        "NEXUS_V2_SSH_KNOWN_HOSTS_FILE": artifacts["sshKnownHosts"]["path"],
        "NEXUS_V2_SSH_KNOWN_HOSTS_SHA256": artifacts["sshKnownHosts"]["sha256"],
        "NEXUS_V2_SSH_HOST_PIN_MANIFEST": artifacts["sshHostPinManifest"]["path"],
        "NEXUS_V2_SSH_HOST_PIN_MANIFEST_SHA256": artifacts["sshHostPinManifest"]["sha256"],
        "FPS_ALPHA_PUBLIC_HOST": "fps.eterra.online",
        "FPS_ALPHA_ABILITY_DEATHMATCH_PORT": "9999",
        "FPS_ALPHA_EXTRACTION_PORT": "10000",
        "FPS_ALPHA_EXTRACTION_BR_PORT": "10001",
        "FPS_ALPHA_GAME_MODE": "fps",
        "FPS_ALPHA_CHAIN_DISABLED": "0",
        "FPS_ALPHA_LIVE_CHAIN_REQUIRED": "1",
        "FPS_ALPHA_V2_GAME_RESULTS_REQUIRED": "1",
        "FPS_ALPHA_PRACTICE_TARGET": "0",
        "FPS_ALPHA_PAID_ENTRY_ENABLED": "0",
        "FPS_ALPHA_WAGERING_ENABLED": "0",
        "FPS_ALPHA_PERMANENT_ASSET_LOSS_ENABLED": "0",
        "FPS_ALPHA_MARKETPLACE_ENABLED": "0",
        "FPS_ALPHA_PUBLIC_PRODUCTION_ENABLED": "0",
        "FPS_ALPHA_EXPECT_CHAIN_RELEASE_ID": release_id,
        "FPS_ALPHA_EXPECT_GENESIS_HASH": target.get("genesisHash"),
        "FPS_ALPHA_EXPECT_RUNTIME_CODE_HASH": target.get("runtimeCodeHash"),
        "FPS_ALPHA_EXPECT_RUNTIME_CODE_SHA256": runtime_bundle.get("productionWasmSha256"),
        "FPS_ALPHA_EXPECT_RUNTIME_SPEC_VERSION": "106",
        "FPS_ALPHA_EXPECT_TCG_STORAGE_VERSION": "16",
        "FPS_ALPHA_EXPECT_RUNTIME_METADATA_SHA256": target.get("runtimeMetadata", {}).get("scaleSha256"),
        "FPS_ALPHA_EXPECT_RUNTIME_METADATA_VERSION": "15",
        "FPS_ALPHA_EXPECT_RUNTIME_SOURCE_COMMIT": target.get("runtimeSourceCommit"),
        "FPS_ALPHA_EXPECT_DEPLOYMENT_SOURCE_COMMIT": repositories["chain"]["head"],
        "FPS_ALPHA_EXPECT_CHAIN_DEPLOYMENT_SOURCE_COMMIT": repositories["chain"]["head"],
        "FPS_ALPHA_EXPECT_NODE_CANDIDATE_MANIFEST_SHA256": artifacts["nodeCandidateManifest"]["sha256"],
        "FPS_ALPHA_EXPECT_TARGET_IDENTITY_KIND": "eterra-spec106-target-identity.v2",
        "FPS_ALPHA_EXPECT_TARGET_IDENTITY_SCHEMA_VERSION": "2",
        "FPS_ALPHA_EXPECT_TARGET_IDENTITY_SHA256": artifacts["targetIdentity"]["sha256"],
        "FPS_ALPHA_EXPECT_TARGET_PLATFORM_SHA256": target_platform_sha256,
        "FPS_ALPHA_EXPECT_DEPLOYMENT_NODE_SOURCE_COMMIT": deployment_node.get("sourceCommit"),
        "FPS_ALPHA_EXPECT_DEPLOYMENT_NODE_SHA256": deployment_node.get("sha256"),
        "FPS_ALPHA_EXPECT_DEPLOYMENT_NODE_BUILD_ATTESTATION_SHA256": deployment_node.get("buildAttestationSha256"),
        "FPS_ALPHA_EXPECT_DEPLOYMENT_NODE_RUNNER_SHA256": deployment_node.get("runnerSha256"),
        "FPS_ALPHA_EXPECT_UNITY_SOURCE_COMMIT": repositories["unity"]["head"],
        "FPS_ALPHA_EXPECT_UNITY_SOURCE_TREE": repositories["unity"]["tree"],
        "ETERRA_NODE_WS_URL": "ws://127.0.0.1:9944",
        "FPS_ALPHA_DEPLOY_ROOT": "/opt/eterra-alpha",
        "FPS_ALPHA_SERVICE_NAME": "eterra-fps-alpha",
    }
    for name, wanted in expected.items():
        require(isinstance(wanted, str) and environment.get(name) == wanted, f"Unity FPS deployment environment pin is stale: {name}")
    require(
        environment.get("FPS_ALPHA_NET_DEBUG", "0") in {"0", "false"},
        "Unity FPS deployment environment enables network debugging",
    )
    identity = environment.get("SSH_IDENTITY_FILE", "")
    require(
        identity.startswith("/") and not any(character.isspace() for character in identity),
        "Unity FPS deployment SSH identity path is invalid",
    )
    sudo_source = environment.get("REMOTE_SUDO_PASSWORD", "")
    require(
        sudo_source.startswith("@/")
        and not any(character.isspace() for character in sudo_source),
        "Unity FPS deployment sudo credential must use an absolute owner-only file source",
    )
    require(
        environment.get("DEPLOY_PASSWORD", "") == "",
        "Unity FPS deployment environment may not define DEPLOY_PASSWORD",
    )
    require("SSH_OPTS" not in environment, "Unity FPS deployment environment may not define SSH_OPTS")
    return environment


def validate_site_final_artifacts(
    lock: Mapping[str, Any],
    site_candidate: Mapping[str, Any],
) -> None:
    artifacts = lock["artifacts"]
    repositories = lock["repositories"]
    web_root = Path(repositories["web"]["root"])
    activation_verifier = web_root / "tcg/deploy/alpha/macmini2014/nexus_v2_full_loop_activation_contract.py"
    identity_verifier = web_root / "tcg/scripts/release/verify_nexus_v2_site_deployment_identity.py"
    compose = web_root / "tcg/deploy/alpha/macmini2014/docker-compose.yaml"
    normalizer = web_root / "tcg/scripts/release/nexus_v2_docker_runtime_config.py"
    for path, label in (
        (activation_verifier, "full-loop activation verifier"),
        (identity_verifier, "site deployment-identity verifier"),
        (compose, "base site Compose"),
        (normalizer, "site runtime normalizer"),
    ):
        require(path.is_file() and not path.is_symlink(), f"official {label} is unavailable")

    phase1_pin = artifacts["sitePhase1PostDeployIdentity"]
    activation_pin = artifacts["fullLoopIndexerActivationReceipt"]
    identity_pin = artifacts["sitePostPhase2DeploymentIdentity"]
    phase1 = read_json(Path(phase1_pin["path"]), "site Phase-1 post-deploy identity")
    activation = read_json(Path(activation_pin["path"]), "full-loop activation receipt")
    identity = read_json(Path(identity_pin["path"]), "site post-Phase2 deployment identity")
    for pin, value, label in (
        (phase1_pin, phase1, "site Phase-1 post-deploy identity"),
        (activation_pin, activation, "full-loop activation receipt"),
        (identity_pin, identity, "site post-Phase2 deployment identity"),
    ):
        require(Path(pin["path"]).read_bytes() == canonical_bytes(value), f"{label} is not canonical JSON")

    site_environment = parse_environment(Path(artifacts["siteDeploymentEnvironment"]["path"]))
    home_hostname = site_environment.get("HOME_HOSTNAME", "")
    require(
        re.fullmatch(r"[a-z0-9](?:[a-z0-9.-]{0,251}[a-z0-9])?", home_hostname) is not None
        and not home_hostname.endswith("."),
        "site HOME_HOSTNAME is invalid",
    )
    expected_url = f"https://{home_hostname}/nexus-api"
    projection = activation.get("projection")
    require(isinstance(projection, dict), "full-loop activation projection is invalid")
    run_checked(
        [
            sys.executable,
            str(activation_verifier),
            "verify-receipt",
            "--receipt", activation_pin["path"],
            "--receipt-sha256", activation_pin["sha256"],
            "--expected-release-version", site_candidate["releaseVersion"],
            "--expected-site-source-commit", repositories["web"]["head"],
            "--expected-phase1-identity-sha256", phase1_pin["sha256"],
            "--expected-base-compose-sha256", sha256_file(compose),
            "--expected-activation-id", str(activation.get("activationId", "")),
            "--expected-release-id", lock["releaseId"],
            "--expected-private-alpha-access-key-sha256", str(activation.get("privateAlphaAccessKeySha256", "")),
            "--expected-readiness-projection-sha256", str(projection.get("readinessProjectionSha256", "")),
            "--expected-readiness-evidence-sha256", str(projection.get("readinessEvidenceSha256", "")),
            "--expected-economic-evidence-sha256", str(projection.get("economicEvidenceSha256", "")),
            "--expected-access-evidence-sha256", str(projection.get("accessEvidenceSha256", "")),
            "--expected-driver-sha256", str(projection.get("driverSha256", "")),
            "--expected-authority-visible-base-url", expected_url,
            "--expected-home-hostname", home_hostname,
        ],
        "official full-loop activation verifier",
    )

    statuses = identity.get("authorityStatus")
    require(isinstance(statuses, dict), "site post-Phase2 authority status is invalid")
    fps_config = str(statuses.get("fps", {}).get("authorityConfigHash", ""))
    legends_config = str(statuses.get("legends", {}).get("authorityConfigHash", ""))
    run_checked(
        [
            sys.executable,
            str(identity_verifier),
            "verify",
            "--identity", identity_pin["path"],
            "--release-version", site_candidate["releaseVersion"],
            "--site-source-commit", repositories["web"]["head"],
            "--fps-config-hash", fps_config,
            "--legends-config-hash", legends_config,
            "--candidate-manifest", artifacts["siteDeploymentCandidateManifest"]["path"],
            "--candidate-manifest-sha256", artifacts["siteDeploymentCandidateManifest"]["sha256"],
            "--phase1-post-deploy-identity", phase1_pin["path"],
            "--phase1-post-deploy-identity-sha256", phase1_pin["sha256"],
            "--full-loop-activation-receipt", activation_pin["path"],
            "--full-loop-activation-receipt-sha256", activation_pin["sha256"],
            "--full-loop-activation-verifier", str(activation_verifier),
            "--compose-file", str(compose),
            "--runtime-normalizer", str(normalizer),
        ],
        "official site deployment-identity verifier",
    )
    source_contract = identity.get("sourceContract")
    require(
        isinstance(source_contract, dict)
        and set(source_contract)
        == {
            "composeSha256",
            "candidateManifestSha256",
            "phase1PostDeployIdentitySha256",
            "runtimeNormalizerSha256",
            "fullLoopActivationReceiptSha256",
            "fullLoopActivationOverrideSha256",
            "fullLoopProjectionManifestSha256",
            "fullLoopActivationVerifierSha256",
        },
        "site post-Phase2 source contract is not the exact eight-key contract",
    )
    require(
        identity.get("publications", {}).get("indexer-api") == ["127.0.0.1:8787:8787/tcp"],
        "site post-Phase2 indexer publication is not loopback-only",
    )


def validate_phase2_transport_handoff(
    lock: Mapping[str, Any],
    acceptance_sha256: str,
    site_release_version: str,
    activation_sha256: str,
) -> None:
    pin = lock["artifacts"]["phase2InternalTransportHandoff"]
    value = exact_keys(
        read_json(Path(pin["path"]), "Phase-2 internal transport handoff"),
        PHASE2_TRANSPORT_HANDOFF_KEYS,
        "Phase-2 internal transport handoff",
    )
    require(Path(pin["path"]).read_bytes() == canonical_bytes(value), "Phase-2 internal transport handoff is not canonical JSON")
    require(value["schemaVersion"] == 1 and value["kind"] == "nexus-v2-private-alpha-phase2-internal-transport-handoff", "Phase-2 internal transport handoff kind mismatch")
    require(
        value["releaseId"] == lock["releaseId"]
        and value["siteReleaseVersion"] == site_release_version
        and value["sourceCommit"] == lock["repositories"]["chain"]["head"]
        and value["siteSourceCommit"] == lock["repositories"]["web"]["head"]
        and value["acceptanceBoundaryReceiptSha256"] == acceptance_sha256,
        "Phase-2 internal transport handoff release/source binding mismatch",
    )
    require(
        value["replacementLockSha256"]
        == lock["artifacts"]["replacementLock"]["sha256"],
        "Phase-2 handoff is not bound to the final-lock-pinned replacement lock",
    )
    require(
        value["sitePhase1PostDeployIdentitySha256"]
        == lock["artifacts"]["sitePhase1PostDeployIdentity"]["sha256"]
        and value["sitePostPhase2DeploymentIdentitySha256"]
        == lock["artifacts"]["sitePostPhase2DeploymentIdentity"]["sha256"],
        "Phase-2 handoff is not bound to the verified site deployment identities",
    )
    require(
        value["network"] == {"chainLanIp": "192.168.1.159", "siteLanIp": "192.168.1.218", "allowedSourceIp": "192.168.1.218"},
        "Phase-2 internal transport network is not exact",
    )
    require(value["ports"] == {"chainRpc": 9944, "authority": 8787, "media": 4000, "ipfsGateway": 8080, "forbidden": [30333, 5001]}, "Phase-2 internal transport port contract mismatch")
    lease = exact_keys(
        value["lease"],
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
        "Phase-2 internal transport lease",
    )
    ensure_id(lease["operationId"], "Phase-2 transport operation ID")
    for field in (
        "planSha256",
        "markerSha256",
        "watchdogUnitSha256",
        "watchdogPayloadSha256",
    ):
        ensure_sha(lease[field], f"Phase-2 transport {field}")
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
            and re.fullmatch(r"nexus-v2-phase2-internal-transport-[A-Za-z0-9_.@-]+", lease[field]),
            f"Phase-2 transport {field} is invalid",
        )
    require(lease["armed"] is True, "Phase-2 internal transport watchdog is not armed")
    parse_utc(lease["expiresAtUtc"], "Phase-2 transport lease expiry")
    require(
        value["phase2"] == {"publicIngressClosed": True, "siteIndexerSynchronized": True, "authorityReady": True, "fullLoopActivationReceiptSha256": activation_sha256},
        "Phase-2 internal transport proof is incomplete",
    )
    require(
        value["safety"] == {"chainStateMutationAuthorized": False, "paidOrPublicActivationAuthorized": False, "sourceRestricted": True, "loopbackBackendsPreserved": True, "forbiddenPortsClosed": True},
        "Phase-2 internal transport safety contract mismatch",
    )
    parse_utc(value["capturedAtUtc"], "Phase-2 transport handoff capture time")
    verifier = (
        Path(lock["repositories"]["chain"]["root"])
        / "scripts/nexus-v2-private-alpha/phase2_internal_transport.py"
    )
    require(
        verifier.is_file() and not verifier.is_symlink(),
        "official Phase-2 transport handoff verifier is unavailable",
    )
    run_checked(
        [
            sys.executable,
            str(verifier),
            "verify-handoff",
            "--handoff",
            pin["path"],
            "--expected-handoff-sha256",
            pin["sha256"],
            "--replacement-lock",
            lock["artifacts"]["replacementLock"]["path"],
            "--expected-replacement-lock-sha256",
            lock["artifacts"]["replacementLock"]["sha256"],
            "--acceptance-boundary-receipt",
            lock["artifacts"]["acceptanceBoundaryReceipt"]["path"],
            "--expected-acceptance-boundary-receipt-sha256",
            lock["artifacts"]["acceptanceBoundaryReceipt"]["sha256"],
            "--site-phase1-post-deploy-identity",
            lock["artifacts"]["sitePhase1PostDeployIdentity"]["path"],
            "--expected-site-phase1-post-deploy-identity-sha256",
            lock["artifacts"]["sitePhase1PostDeployIdentity"]["sha256"],
            "--full-loop-indexer-activation-receipt",
            lock["artifacts"]["fullLoopIndexerActivationReceipt"]["path"],
            "--expected-full-loop-indexer-activation-receipt-sha256",
            lock["artifacts"]["fullLoopIndexerActivationReceipt"]["sha256"],
            "--site-post-phase2-deployment-identity",
            lock["artifacts"]["sitePostPhase2DeploymentIdentity"]["path"],
            "--expected-site-post-phase2-deployment-identity-sha256",
            lock["artifacts"]["sitePostPhase2DeploymentIdentity"]["sha256"],
            "--selected-deployment-environment",
            lock["artifacts"]["deploymentEnvironment"]["path"],
            "--selected-site-deployment-environment",
            lock["artifacts"]["siteDeploymentEnvironment"]["path"],
        ],
        "official Phase-2 transport handoff verifier",
    )


def validate_semantic_pins(lock: Mapping[str, Any], *, final: bool) -> None:
    repositories = lock["repositories"]
    artifacts = lock["artifacts"]
    chain = repositories["chain"]
    media = repositories["media"]
    ai = repositories["ai"]

    try:
        ssh_host_pins.verify(
            Path(artifacts["sshKnownHosts"]["path"]),
            Path(artifacts["sshHostPinManifest"]["path"]),
        )
    except ssh_host_pins.PinError as exc:
        raise ReleaseLockError(f"SSH host-pin artifacts are invalid: {exc}") from exc
    for artifact_name in ("sshKnownHosts", "sshHostPinManifest"):
        require(
            re.fullmatch(r"/[A-Za-z0-9._/+:-]+", artifacts[artifact_name]["path"])
            is not None,
            f"artifact {artifact_name} path is unsafe for an OpenSSH option",
        )

    target = read_json(Path(artifacts["targetIdentity"]["path"]), "target identity")
    require(target.get("releaseId") == lock["releaseId"], "target identity release is stale")
    require(target.get("deploymentSourceCommit") == chain["head"], "target identity chain commit is stale")
    genesis_hash = target.get("genesisHash")
    require(
        isinstance(genesis_hash, str) and HASH256_RE.fullmatch(genesis_hash) is not None,
        "target identity genesis hash is invalid",
    )
    target_metadata = target.get("runtimeMetadata")
    require(isinstance(target_metadata, dict), "target identity runtime metadata is invalid")
    metadata_scale_sha256 = ensure_sha(
        target_metadata.get("scaleSha256"), "target identity metadata SHA-256"
    )
    node = read_json(Path(artifacts["nodeCandidateManifest"]["path"]), "node candidate manifest")
    require(node.get("releaseId") == lock["releaseId"], "node candidate release is stale")
    require(node.get("deploymentSourceCommit") == chain["head"], "node candidate chain commit is stale")
    runtime_bundle = node.get("runtimeBundle")
    require(isinstance(runtime_bundle, dict), "node candidate runtime bundle is invalid")
    require(
        runtime_bundle.get("manifestSha256") == artifacts["runtimeBundleManifest"]["sha256"],
        "node candidate runtime bundle pin is stale",
    )
    runtime_code_sha256 = ensure_sha(
        runtime_bundle.get("productionWasmSha256"), "node candidate production Wasm SHA-256"
    )
    require(
        runtime_bundle.get("metadataScaleSha256") == metadata_scale_sha256,
        "node candidate and target identity metadata pins differ",
    )
    media_candidate = read_json(
        Path(artifacts["mediaCandidateManifest"]["path"]), "media candidate manifest"
    )
    require(media_candidate.get("chainDeployCommit") == chain["head"], "media candidate chain commit is stale")
    require(media_candidate.get("mediaSourceCommit") == media["head"], "media candidate source is stale")
    site_candidate = validate_site_candidate(
        Path(artifacts["siteDeploymentCandidateManifest"]["path"]), repositories
    )
    try:
        authority = authority_candidate.validate_candidate(
            Path(artifacts["authorityCandidateManifest"]["path"]),
            artifacts["authorityCandidateManifest"]["sha256"],
            expected_release_id=lock["releaseId"],
            expected_chain_commit=chain["head"],
            expected_sdkgen_commit=repositories["sdkgen"]["head"],
        )
    except authority_candidate.CandidateError as exc:
        raise ReleaseLockError(f"authority candidate is invalid: {exc}") from exc
    authority_target = authority["target"]
    require(authority_target.get("genesisHash") == genesis_hash, "authority candidate genesis pin is stale")
    require(authority_target.get("runtimeSpecVersion") == 106, "authority candidate runtime spec is stale")
    require(authority_target.get("runtimeCodeHash") == target.get("runtimeCodeHash"), "authority candidate runtime code hash is stale")
    require(authority_target.get("runtimeCodeSha256") == runtime_code_sha256, "authority candidate runtime Wasm pin is stale")
    require(authority_target.get("runtimeMetadataScaleSha256") == metadata_scale_sha256, "authority candidate runtime metadata pin is stale")
    unit_path = Path(chain["root"]) / "deploy/alpha/macmini2010/eterra-arcade-authority.service"
    require(
        unit_path.is_file()
        and not unit_path.is_symlink()
        and sha256_file(unit_path) == authority["deployment"]["serviceUnitSha256"],
        "authority candidate service-unit pin is stale",
    )
    require(
        authority["services"]["legendsAuthority"]["releaseSha256"]
        == authority["artifacts"]["releaseManifest"]["sha256"],
        "Legends authority release provenance is stale",
    )
    require(authority["safety"].get("fpsReleaseIncluded") is False, "pre-cutover authority candidate must remain Legends-only")
    if final:
        replacement_pin = artifacts["replacementLock"]
        try:
            replacement = validate_replacement_lock(
                Path(replacement_pin["path"]),
                replacement_pin["sha256"],
                artifacts["deploymentEnvironment"]["path"],
                artifacts["siteDeploymentEnvironment"]["path"],
            )
        except ReleaseLockError as exc:
            raise ReleaseLockError(
                f"final lock replacement-lock authority is invalid: {exc}"
            ) from exc
        require(
            replacement["releaseId"] == lock["releaseId"],
            "final and replacement lock release IDs differ",
        )
        require(
            replacement["repositories"] == repositories,
            "final and replacement lock repository pins differ",
        )
        require(
            replacement["artifacts"]
            == {name: artifacts[name] for name in REPLACEMENT_ARTIFACT_KEYS},
            "final and replacement lock common artifact pins differ",
        )
        receipt_pin = artifacts["acceptanceBoundaryReceipt"]
        try:
            receipt = acceptance_boundary.validate_receipt(
                Path(receipt_pin["path"]),
                receipt_pin["sha256"],
                release_id=lock["releaseId"],
                source_commit=chain["head"],
                genesis_hash=genesis_hash,
                runtime_code_sha256=runtime_code_sha256,
                runtime_metadata_scale_sha256=metadata_scale_sha256,
            )
        except acceptance_boundary.BoundaryError as exc:
            raise ReleaseLockError(f"acceptance-boundary receipt is invalid: {exc}") from exc

        read_model = read_json(
            Path(artifacts["readModelManifest"]["path"]), "read-model manifest"
        )
        require(
            read_model.get("kind") == "nexus-v2-private-alpha-exact-block-read-model-candidate",
            "read-model manifest kind mismatch",
        )
        source = read_model.get("source")
        require(isinstance(source, dict), "read-model manifest source is invalid")
        require(source.get("commit") == ai["head"], "read-model manifest commit is stale")
        require(source.get("tree") == ai["tree"], "read-model manifest tree is stale")
        runtime_pins = read_model.get("runtimePins")
        require(isinstance(runtime_pins, dict), "read-model runtime pins are invalid")
        require(runtime_pins.get("specVersion") == 106, "read-model runtime spec is stale")
        require(runtime_pins.get("genesisHash") == genesis_hash, "read-model genesis pin is stale")
        require(
            runtime_pins.get("metadataSha256") == metadata_scale_sha256,
            "read-model metadata pin is stale",
        )
        safety = read_model.get("releaseSafety")
        require(isinstance(safety, dict), "read-model release safety is invalid")
        for name in (
            "actionSubmissionEnabled",
            "economicActivationEnabled",
            "paidAcquisitionEnabled",
            "publicReleaseEnabled",
        ):
            require(safety.get(name) is False, f"read-model manifest must set {name}=false")
        require(safety.get("readModelOnly") is True, "read-model manifest is not read-only")

        binding = exact_keys(
            read_model.get("acceptanceBoundary"),
            READ_MODEL_ACCEPTANCE_KEYS,
            "read-model acceptance binding",
        )
        expected_binding = {
            "automaticRestorePermanentlyDisabled": True,
            "chainSourceCommit": chain["head"],
            "coordinatorDecision": "keep-v2",
            "genesisHash": genesis_hash,
            "observedAtFinalizedBlock": receipt["observedAtFinalizedBlock"],
            "receiptKind": "nexus-v2-private-alpha-acceptance-boundary-receipt",
            "receiptSha256": receipt_pin["sha256"],
            "releaseId": lock["releaseId"],
            "runtimeCodeSha256": runtime_code_sha256,
            "runtimeMetadataScaleSha256": metadata_scale_sha256,
        }
        require(binding == expected_binding, "read-model acceptance binding is stale or incomplete")
        validate_unity_fps_candidate(
            Path(artifacts["unityFpsCandidateManifest"]["path"]),
            repositories,
            lock["releaseId"],
            genesis_hash,
            runtime_code_sha256,
            metadata_scale_sha256,
            receipt_pin["sha256"],
        )
        unity_environment = parse_environment(
            Path(artifacts["unityFpsDeploymentEnvironment"]["path"])
        )
        previous_environment = os.environ.copy()
        try:
            os.environ.update(unity_environment)
            run_checked(
                [
                    sys.executable,
                    str(Path(repositories["unity"]["root"]) / "scripts/release/fps-server-candidate.py"),
                    "validate-environment",
                    str(Path(artifacts["unityFpsCandidateManifest"]["path"]).parent),
                ],
                "official Unity FPS candidate environment verifier",
            )
        finally:
            os.environ.clear()
            os.environ.update(previous_environment)
        validate_site_final_artifacts(lock, site_candidate)
        validate_phase2_transport_handoff(
            lock,
            receipt_pin["sha256"],
            site_candidate["releaseVersion"],
            artifacts["fullLoopIndexerActivationReceipt"]["sha256"],
        )

    environment_path = Path(artifacts["deploymentEnvironment"]["path"])
    environment = parse_environment(environment_path)
    required_environment = {
        "ETERRA_RELEASE_VERSION": lock["releaseId"],
        "ETERRA_EXPECTED_CHAIN_COMMIT": chain["head"],
        "ETERRA_EXPECTED_MEDIA_COMMIT": media["head"],
        "ETERRA_EXPECTED_SDKGEN_COMMIT": repositories["sdkgen"]["head"],
        "NEXUS_V2_NODE_CANDIDATE_SHA256": artifacts["nodeCandidateManifest"]["sha256"],
        "NEXUS_V2_TARGET_IDENTITY_SHA256": artifacts["targetIdentity"]["sha256"],
        "NEXUS_V2_AUTHORITY_CANDIDATE_PATH": artifacts["authorityCandidateManifest"]["path"],
        "NEXUS_V2_AUTHORITY_CANDIDATE_SHA256": artifacts["authorityCandidateManifest"]["sha256"],
        "NEXUS_V2_ALPHA_GENESIS_HASH": genesis_hash,
        "RUNTIME_SPEC_VERSION": "106",
        "RUNTIME_CODE_HASH": target.get("runtimeCodeHash"),
        "AUTHORITY_SUBMITTER_MODE": "in_memory",
        "ETERRA_LEGENDS_READ_MODEL_ADAPTER_VERSION": authority_target["readModelAdapterVersion"],
        "ETERRA_LEGENDS_AUTHORITY_EPOCH": str(authority_target["authorityEpoch"]),
        "NEXUS_V2_SSH_KNOWN_HOSTS_FILE": artifacts["sshKnownHosts"]["path"],
        "NEXUS_V2_SSH_KNOWN_HOSTS_SHA256": artifacts["sshKnownHosts"]["sha256"],
        "NEXUS_V2_SSH_HOST_PIN_MANIFEST": artifacts["sshHostPinManifest"]["path"],
        "NEXUS_V2_SSH_HOST_PIN_MANIFEST_SHA256": artifacts["sshHostPinManifest"]["sha256"],
    }
    for name, expected in required_environment.items():
        require(environment.get(name) == expected, f"deployment environment pin is stale: {name}")
    require(
        environment.get("DEPLOY_PASSWORD", "") == "",
        "deployment environment may not define DEPLOY_PASSWORD",
    )
    require(
        environment.get("SSH_IDENTITY_FILE", "").startswith("/")
        and not any(
            character.isspace()
            for character in environment.get("SSH_IDENTITY_FILE", "")
        ),
        "deployment environment SSH identity path is invalid",
    )
    require(
        environment.get("REMOTE_SUDO_PASSWORD", "").startswith("@/")
        and not any(
            character.isspace()
            for character in environment.get("REMOTE_SUDO_PASSWORD", "")
        ),
        "deployment environment sudo credential must use an absolute owner-only file source",
    )
    read_model_url = environment.get("ETERRA_LEGENDS_READ_MODEL_URL", "")
    require(
        read_model_url.startswith("https://")
        and "@" not in read_model_url
        and not any(character.isspace() for character in read_model_url),
        "deployment environment authority read-model URL is unsafe",
    )
    for name in ("ETERRA_LEGENDS_SIGNER_MNEMONIC", "ETERRA_LEGENDS_PRIVATE_ALPHA_ACCESS_KEY"):
        source = environment.get(name, "")
        require(
            source.startswith("@/") and not any(character.isspace() for character in source),
            f"deployment environment must use an absolute file source: {name}",
        )
    site_environment = parse_environment(Path(artifacts["siteDeploymentEnvironment"]["path"]))
    require(
        site_environment.get("EXPECTED_SOURCE_COMMIT") == repositories["web"]["head"],
        "site deployment environment source pin is stale",
    )
    for name, expected in {
        "NEXUS_V2_SSH_KNOWN_HOSTS_FILE": artifacts["sshKnownHosts"]["path"],
        "NEXUS_V2_SSH_KNOWN_HOSTS_SHA256": artifacts["sshKnownHosts"]["sha256"],
        "NEXUS_V2_SSH_HOST_PIN_MANIFEST": artifacts["sshHostPinManifest"]["path"],
        "NEXUS_V2_SSH_HOST_PIN_MANIFEST_SHA256": artifacts["sshHostPinManifest"]["sha256"],
    }.items():
        require(site_environment.get(name) == expected, f"site deployment environment pin is stale: {name}")
    require(
        site_environment.get("DEPLOY_PASSWORD", "") == "",
        "site deployment environment may not define DEPLOY_PASSWORD",
    )
    require(
        site_environment.get("SSH_IDENTITY_FILE", "").startswith("/")
        and not any(
            character.isspace()
            for character in site_environment.get("SSH_IDENTITY_FILE", "")
        ),
        "site deployment environment SSH identity path is invalid",
    )
    require(
        site_environment.get("REMOTE_SUDO_PASSWORD", "").startswith("@/")
        and not any(
            character.isspace()
            for character in site_environment.get("REMOTE_SUDO_PASSWORD", "")
        ),
        "site deployment environment sudo credential must use an absolute owner-only file source",
    )
    require(
        site_environment.get("RELEASE_VERSION") == site_candidate["releaseVersion"],
        "site deployment environment release differs from the immutable candidate",
    )
    home_hostname = site_environment.get("HOME_HOSTNAME", "")
    require(
        re.fullmatch(r"[a-z0-9](?:[a-z0-9.-]{0,251}[a-z0-9])?", home_hostname) is not None
        and not home_hostname.endswith("."),
        "site HOME_HOSTNAME is invalid",
    )
    require(
        site_environment.get("SITE_PUBLIC_URL") == f"https://{home_hostname}",
        "site public URL does not match locked HOME_HOSTNAME",
    )
    require(
        site_environment.get("INDEXER_CHAIN_WS_URL") == "ws://192.168.1.159:9944",
        "site indexer chain endpoint is not the exact Phase-2 internal transport target",
    )
    require(
        site_environment.get("NEXUS_V2_FULL_LOOP_ACCEPTANCE_READS_ENABLED", "").lower()
        == "false"
        and site_environment.get("NEXUS_V2_FULL_LOOP_ACCEPTANCE_PROJECTION_DIRECTORY")
        == "/var/lib/eterra/full-loop"
        and site_environment.get("NEXUS_V2_FULL_LOOP_ACCEPTANCE_TARGET_JSON", "") == "",
        "base site environment must keep full-loop reads disabled and target-free",
    )
    for name in (
        "PUBLIC_MEDIA_UPLOAD_ENABLED",
        "PUBLIC_AVATAR_UPLOAD_ENABLED",
        "NEXUS_V2_SESSION_AUTHORIZATION_PRODUCTION_ENABLED",
    ):
        require(
            site_environment.get(name, "").lower() == "false",
            f"site deployment environment must disable {name}",
        )
    validate_unity_fps_environment(
        Path(artifacts["unityFpsDeploymentEnvironment"]["path"]),
        repositories,
        artifacts,
        target,
        lock["releaseId"],
    )


def _validate_lock(
    path: Path,
    expected_sha256: str,
    selected_environment: str,
    selected_site_environment: str,
    *,
    expected_kind: str,
    artifact_keys: set[str],
    final: bool,
) -> dict[str, Any]:
    expected_sha256 = ensure_sha(expected_sha256, "release-lock SHA-256")
    raw = read_stable_regular_file(path, "release lock")
    lock = decode_json_object(raw, "release lock")
    require(raw == canonical_bytes(lock), "release lock is not canonical JSON")
    require(hashlib.sha256(raw).hexdigest() == expected_sha256, "release-lock hash mismatch")
    exact_keys(lock, LOCK_KEYS, "release lock")
    require(lock["schemaVersion"] == 1, "release-lock schema mismatch")
    require(lock["kind"] == expected_kind, "release-lock kind mismatch")
    ensure_id(lock["releaseId"], "release ID")
    parse_utc(lock["createdAtUtc"], "release-lock createdAtUtc")
    require(lock["policy"] == POLICY, "release-lock policy mismatch")

    repositories = exact_keys(lock["repositories"], REPOSITORY_IDS, "release-lock repositories")
    for identifier, pin in repositories.items():
        exact_keys(pin, REPOSITORY_KEYS, f"repository {identifier}")
        actual = repository_pin(pin["root"], f"repository {identifier}")
        require(actual == pin, f"repository pin drifted: {identifier}")

    artifacts = exact_keys(lock["artifacts"], artifact_keys, "release-lock artifacts")
    for name in artifact_keys - {"forbiddenDeploymentEnvironments", "unityTestResults"}:
        pin = exact_keys(artifacts[name], FILE_PIN_KEYS, f"artifact {name}")
        path_value = pin["path"]
        actual = file_pin(
            path_value,
            f"artifact {name}",
            canonical_json=False,
        )
        require(actual == pin, f"artifact pin drifted: {name}")
    forbidden = artifacts["forbiddenDeploymentEnvironments"]
    require(isinstance(forbidden, list) and all(isinstance(item, str) for item in forbidden), "forbidden environment list is invalid")
    require(forbidden == sorted(set(forbidden)), "forbidden environment list is not unique/sorted")
    selected = canonical_existing_path(
        Path(selected_environment), "selected deployment environment"
    )
    required_selected = canonical_existing_path(
        Path(artifacts["deploymentEnvironment"]["path"]),
        "release-locked deployment environment",
    )
    require(selected == required_selected, "selected deployment environment is not the release-locked file")
    require(str(selected) not in forbidden, "selected deployment environment is forbidden")
    selected_site = canonical_existing_path(
        Path(selected_site_environment), "selected site deployment environment"
    )
    required_site = canonical_existing_path(
        Path(artifacts["siteDeploymentEnvironment"]["path"]),
        "release-locked site deployment environment",
    )
    require(
        selected_site == required_site,
        "selected site deployment environment is not the release-locked file",
    )

    unity_results = exact_keys(
        artifacts["unityTestResults"], {"editMode", "playMode"}, "Unity result pins"
    )
    for key, mode in (("editMode", "EditMode"), ("playMode", "PlayMode")):
        pin = exact_keys(unity_results[key], UNITY_RESULT_KEYS, f"Unity {mode} pin")
        require(unity_result(pin["path"], mode) == pin, f"Unity {mode} result pin drifted")

    validate_semantic_pins(lock, final=final)
    return lock


def validate_lock(
    path: Path,
    expected_sha256: str,
    selected_environment: str,
    selected_site_environment: str,
) -> dict[str, Any]:
    """Validate only the final, post-acceptance release lock."""

    return _validate_lock(
        path,
        expected_sha256,
        selected_environment,
        selected_site_environment,
        expected_kind=FINAL_LOCK_KIND,
        artifact_keys=FINAL_ARTIFACT_KEYS,
        final=True,
    )


def validate_replacement_lock(
    path: Path,
    expected_sha256: str,
    selected_environment: str,
    selected_site_environment: str,
) -> dict[str, Any]:
    """Validate only the pre-cutover replacement-input lock."""

    return _validate_lock(
        path,
        expected_sha256,
        selected_environment,
        selected_site_environment,
        expected_kind=REPLACEMENT_LOCK_KIND,
        artifact_keys=REPLACEMENT_ARTIFACT_KEYS,
        final=False,
    )


def open_output_parent(path: Path) -> int:
    require(path.is_absolute(), "output parent must be absolute")
    require(".." not in path.parts, "output parent may not contain parent traversal")
    flags = (
        os.O_RDONLY
        | os.O_DIRECTORY
        | getattr(os, "O_CLOEXEC", 0)
        | getattr(os, "O_NOFOLLOW", 0)
    )
    descriptor = os.open("/", flags)
    try:
        for component in path.parts[1:]:
            try:
                next_descriptor = os.open(component, flags, dir_fd=descriptor)
            except FileNotFoundError:
                try:
                    os.mkdir(component, 0o700, dir_fd=descriptor)
                except FileExistsError:
                    pass
                next_descriptor = os.open(component, flags, dir_fd=descriptor)
            os.close(descriptor)
            descriptor = next_descriptor
        return descriptor
    except OSError as exc:
        os.close(descriptor)
        raise ReleaseLockError(f"output parent is unsafe: {path}") from exc


def require_new_output(path: Path) -> None:
    require(path.is_absolute(), "output must be absolute")
    require(
        ".." not in path.parts and path.name not in {"", ".", ".."},
        "output path is invalid",
    )
    cursor = Path("/")
    for component in path.parts[1:-1]:
        cursor /= component
        try:
            observed = os.lstat(cursor)
        except FileNotFoundError:
            break
        require(stat.S_ISDIR(observed.st_mode), f"output parent is unsafe: {cursor}")
    try:
        os.lstat(path)
    except FileNotFoundError:
        return
    raise ReleaseLockError(f"refusing to overwrite output: {path}")


def write_new(path: Path, value: Mapping[str, Any]) -> None:
    require_new_output(path)
    parent_fd = open_output_parent(path.parent)
    descriptor: int | None = None
    created = False
    try:
        flags = (
            os.O_WRONLY
            | os.O_CREAT
            | os.O_EXCL
            | getattr(os, "O_CLOEXEC", 0)
            | getattr(os, "O_NOFOLLOW", 0)
        )
        descriptor = os.open(path.name, flags, 0o440, dir_fd=parent_fd)
        created = True
        os.fchmod(descriptor, 0o440)
        payload = canonical_bytes(value)
        offset = 0
        while offset < len(payload):
            written = os.write(descriptor, payload[offset:])
            require(written > 0, "release-lock output write made no progress")
            offset += written
        os.fsync(descriptor)
        opened = os.fstat(descriptor)
        observed = os.stat(path.name, dir_fd=parent_fd, follow_symlinks=False)
        require(
            stat.S_ISREG(opened.st_mode)
            and (opened.st_dev, opened.st_ino) == (observed.st_dev, observed.st_ino),
            "release-lock output target changed while it was written",
        )
    except OSError as exc:
        if created:
            try:
                os.unlink(path.name, dir_fd=parent_fd)
            except OSError:
                pass
        raise ReleaseLockError(f"cannot create output: {path}") from exc
    except Exception:
        if created:
            try:
                os.unlink(path.name, dir_fd=parent_fd)
            except OSError:
                pass
        raise
    finally:
        if descriptor is not None:
            os.close(descriptor)
        os.close(parent_fd)


def _capture(args: argparse.Namespace, *, final: bool) -> None:
    repositories = {
        identifier: repository_pin(root, f"repository {identifier}")
        for identifier, root in parse_repository_arguments(args.repository).items()
    }
    forbidden = sorted(
        {
            str(canonical_existing_path(Path(value), "forbidden deployment environment"))
            for value in args.forbidden_deployment_environment
        }
    )
    selected_environment = str(
        canonical_existing_path(Path(args.deployment_environment), "deployment environment")
    )
    require(selected_environment not in forbidden, "selected environment is also forbidden")
    artifacts: dict[str, Any] = {
        "deploymentEnvironment": file_pin(selected_environment, "deployment environment"),
        "siteDeploymentEnvironment": file_pin(
            args.site_deployment_environment, "site deployment environment"
        ),
        "unityFpsDeploymentEnvironment": file_pin(
            args.unity_fps_deployment_environment,
            "Unity FPS deployment environment",
        ),
        "sshKnownHosts": file_pin(args.ssh_known_hosts, "dedicated SSH known_hosts"),
        "sshHostPinManifest": file_pin(
            args.ssh_host_pin_manifest,
            "SSH host-pin manifest",
            canonical_json=True,
        ),
        "forbiddenDeploymentEnvironments": forbidden,
        "runtimeBundleManifest": file_pin(args.runtime_bundle_manifest, "runtime bundle manifest"),
        "targetIdentity": file_pin(args.target_identity, "target identity"),
        "nodeCandidateManifest": file_pin(args.node_candidate_manifest, "node candidate manifest"),
        "mediaCandidateManifest": file_pin(args.media_candidate_manifest, "media candidate manifest"),
        "authorityCandidateManifest": file_pin(
            args.authority_candidate_manifest,
            "authority candidate manifest",
            canonical_json=True,
        ),
        "siteDeploymentCandidateManifest": file_pin(
            args.site_deployment_candidate_manifest,
            "site deployment candidate manifest",
        ),
        "snapshotManifest": file_pin(args.snapshot_manifest, "snapshot manifest"),
        "unityTestResults": {
            "editMode": unity_result(args.unity_editmode_results, "EditMode"),
            "playMode": unity_result(args.unity_playmode_results, "PlayMode"),
        },
    }
    if final:
        artifacts["replacementLock"] = file_pin(
            args.replacement_lock,
            "pre-cutover replacement lock",
            canonical_json=True,
        )
        artifacts["acceptanceBoundaryReceipt"] = file_pin(
            args.acceptance_boundary_receipt,
            "acceptance-boundary receipt",
            canonical_json=True,
        )
        artifacts["readModelManifest"] = file_pin(
            args.read_model_manifest,
            "read-model manifest",
            canonical_json=True,
        )
        artifacts["unityFpsCandidateManifest"] = file_pin(
            args.unity_fps_candidate_manifest,
            "Unity FPS dedicated-server candidate manifest",
        )
        artifacts["sitePhase1PostDeployIdentity"] = file_pin(
            args.site_phase1_post_deploy_identity,
            "site Phase-1 post-deploy identity",
            canonical_json=True,
        )
        artifacts["fullLoopIndexerActivationReceipt"] = file_pin(
            args.full_loop_indexer_activation_receipt,
            "full-loop indexer activation receipt",
            canonical_json=True,
        )
        artifacts["sitePostPhase2DeploymentIdentity"] = file_pin(
            args.site_post_phase2_deployment_identity,
            "site post-Phase2 deployment identity",
            canonical_json=True,
        )
        artifacts["phase2InternalTransportHandoff"] = file_pin(
            args.phase2_internal_transport_handoff,
            "Phase-2 internal transport handoff",
            canonical_json=True,
        )
    lock = {
        "schemaVersion": 1,
        "kind": FINAL_LOCK_KIND if final else REPLACEMENT_LOCK_KIND,
        "releaseId": ensure_id(args.release_id, "release ID"),
        "createdAtUtc": args.created_at or utc_now(),
        "repositories": repositories,
        "artifacts": artifacts,
        "policy": POLICY,
    }
    parse_utc(lock["createdAtUtc"], "release-lock createdAtUtc")
    validate_semantic_pins(lock, final=final)
    output = Path(args.output)
    write_new(output, lock)
    digest = hashlib.sha256(
        read_stable_regular_file(output, "release-lock output")
    ).hexdigest()
    validator = validate_lock if final else validate_replacement_lock
    validator(output, digest, selected_environment, args.site_deployment_environment)
    stage = "final release" if final else "pre-cutover replacement"
    print(f"{stage} lock captured: {output} sha256={digest}")


def command_capture(args: argparse.Namespace) -> None:
    _capture(args, final=True)


def command_capture_replacement(args: argparse.Namespace) -> None:
    _capture(args, final=False)


def command_verify(args: argparse.Namespace) -> None:
    validate_lock(
        Path(args.lock),
        args.expected_sha256,
        args.selected_deployment_environment,
        args.selected_site_deployment_environment,
    )
    print(f"release lock verified: sha256={args.expected_sha256}")


def command_verify_replacement(args: argparse.Namespace) -> None:
    validate_replacement_lock(
        Path(args.lock),
        args.expected_sha256,
        args.selected_deployment_environment,
        args.selected_site_deployment_environment,
    )
    print(f"pre-cutover replacement lock verified: sha256={args.expected_sha256}")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)

    def add_capture_arguments(command: argparse.ArgumentParser, *, final: bool) -> None:
        command.add_argument("--release-id", required=True)
        command.add_argument(
            "--repository",
            action="append",
            default=[],
            required=True,
            metavar="ID=/ABS/ROOT",
        )
        command.add_argument("--deployment-environment", required=True)
        command.add_argument("--site-deployment-environment", required=True)
        command.add_argument("--unity-fps-deployment-environment", required=True)
        command.add_argument("--ssh-known-hosts", required=True)
        command.add_argument("--ssh-host-pin-manifest", required=True)
        command.add_argument("--forbidden-deployment-environment", action="append", default=[])
        command.add_argument("--runtime-bundle-manifest", required=True)
        command.add_argument("--target-identity", required=True)
        command.add_argument("--node-candidate-manifest", required=True)
        command.add_argument("--media-candidate-manifest", required=True)
        command.add_argument("--authority-candidate-manifest", required=True)
        command.add_argument("--site-deployment-candidate-manifest", required=True)
        if final:
            command.add_argument("--replacement-lock", required=True)
            command.add_argument("--acceptance-boundary-receipt", required=True)
            command.add_argument("--read-model-manifest", required=True)
            command.add_argument("--unity-fps-candidate-manifest", required=True)
            command.add_argument("--site-phase1-post-deploy-identity", required=True)
            command.add_argument("--full-loop-indexer-activation-receipt", required=True)
            command.add_argument("--site-post-phase2-deployment-identity", required=True)
            command.add_argument("--phase2-internal-transport-handoff", required=True)
        command.add_argument("--snapshot-manifest", required=True)
        command.add_argument("--unity-editmode-results", required=True)
        command.add_argument("--unity-playmode-results", required=True)
        command.add_argument("--created-at")
        command.add_argument("--output", required=True)

    def add_verify_arguments(command: argparse.ArgumentParser) -> None:
        command.add_argument("--lock", required=True)
        command.add_argument("--expected-sha256", required=True)
        command.add_argument("--selected-deployment-environment", required=True)
        command.add_argument("--selected-site-deployment-environment", required=True)

    replacement_capture = commands.add_parser(
        "capture-replacement",
        help="capture pre-cutover replacement inputs without post-cutover claims",
    )
    add_capture_arguments(replacement_capture, final=False)
    replacement_capture.set_defaults(func=command_capture_replacement)
    replacement_verify = commands.add_parser(
        "verify-replacement",
        help="verify only a pre-cutover replacement lock",
    )
    add_verify_arguments(replacement_verify)
    replacement_verify.set_defaults(func=command_verify_replacement)

    capture = commands.add_parser("capture", help="capture the final post-receipt release lock")
    add_capture_arguments(capture, final=True)
    capture.set_defaults(func=command_capture)
    verify = commands.add_parser("verify", help="verify only the final post-receipt release lock")
    add_verify_arguments(verify)
    verify.set_defaults(func=command_verify)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        args.func(args)
    except (ReleaseLockError, OSError) as exc:
        print(f"release_lock: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
