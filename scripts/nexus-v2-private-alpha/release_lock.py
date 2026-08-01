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
import subprocess
import sys
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import Any, Mapping, Sequence


TOOL_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOL_DIR))
import acceptance_boundary  # noqa: E402


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
    "forbiddenDeploymentEnvironments",
    "runtimeBundleManifest",
    "targetIdentity",
    "nodeCandidateManifest",
    "mediaCandidateManifest",
    "snapshotManifest",
    "unityTestResults",
}
REPLACEMENT_ARTIFACT_KEYS = set(COMMON_ARTIFACT_KEYS)
FINAL_ARTIFACT_KEYS = COMMON_ARTIFACT_KEYS | {
    "acceptanceBoundaryReceipt",
    "readModelManifest",
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


def read_json(path: Path, label: str) -> dict[str, Any]:
    require(path.is_absolute(), f"{label} path must be absolute")
    require(path.is_file() and not path.is_symlink(), f"{label} must be a regular file")
    try:
        value = json.loads(
            path.read_text(encoding="utf-8"), object_pairs_hook=duplicate_rejecting_object
        )
    except (OSError, json.JSONDecodeError) as exc:
        raise ReleaseLockError(f"invalid {label}") from exc
    require(isinstance(value, dict), f"{label} root must be an object")
    return value


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
    root = Path(root_value)
    require(root.is_absolute() and root.is_dir() and not root.is_symlink(), f"{label} root is invalid")
    root = root.resolve()
    require(
        Path(git_output(root, "rev-parse", "--show-toplevel")).resolve() == root,
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
    path = Path(path_value)
    require(path.is_absolute() and path.is_file() and not path.is_symlink(), f"{label} is unavailable")
    path = path.resolve()
    if canonical_json:
        value = read_json(path, label)
        require(path.read_bytes() == canonical_bytes(value), f"{label} is not canonical JSON")
    return {"path": str(path), "sha256": sha256_file(path)}


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


def unity_result(path_value: str, mode: str) -> dict[str, Any]:
    pin = file_pin(path_value, f"Unity {mode} result")
    try:
        root = ET.parse(pin["path"]).getroot()
    except (ET.ParseError, OSError) as exc:
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


def validate_semantic_pins(lock: Mapping[str, Any], *, final: bool) -> None:
    repositories = lock["repositories"]
    artifacts = lock["artifacts"]
    chain = repositories["chain"]
    media = repositories["media"]
    ai = repositories["ai"]

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
    if final:
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

    environment_path = Path(artifacts["deploymentEnvironment"]["path"])
    environment = parse_environment(environment_path)
    required_environment = {
        "ETERRA_RELEASE_VERSION": lock["releaseId"],
        "ETERRA_EXPECTED_CHAIN_COMMIT": chain["head"],
        "ETERRA_EXPECTED_MEDIA_COMMIT": media["head"],
        "ETERRA_EXPECTED_SDKGEN_COMMIT": repositories["sdkgen"]["head"],
        "NEXUS_V2_NODE_CANDIDATE_SHA256": artifacts["nodeCandidateManifest"]["sha256"],
        "NEXUS_V2_TARGET_IDENTITY_SHA256": artifacts["targetIdentity"]["sha256"],
    }
    for name, expected in required_environment.items():
        require(environment.get(name) == expected, f"deployment environment pin is stale: {name}")
    site_environment = parse_environment(Path(artifacts["siteDeploymentEnvironment"]["path"]))
    require(
        site_environment.get("EXPECTED_SOURCE_COMMIT") == repositories["web"]["head"],
        "site deployment environment source pin is stale",
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
    lock = read_json(path, "release lock")
    require(path.read_bytes() == canonical_bytes(lock), "release lock is not canonical JSON")
    require(sha256_file(path) == expected_sha256, "release-lock hash mismatch")
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
    selected = Path(selected_environment)
    require(selected.is_absolute(), "selected deployment environment must be absolute")
    selected = selected.resolve()
    required_selected = Path(artifacts["deploymentEnvironment"]["path"]).resolve()
    require(selected == required_selected, "selected deployment environment is not the release-locked file")
    require(str(selected) not in forbidden, "selected deployment environment is forbidden")
    selected_site = Path(selected_site_environment)
    require(selected_site.is_absolute(), "selected site deployment environment must be absolute")
    require(
        selected_site.resolve() == Path(artifacts["siteDeploymentEnvironment"]["path"]).resolve(),
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


def write_new(path: Path, value: Mapping[str, Any]) -> None:
    require(not path.exists() and not path.is_symlink(), f"refusing to overwrite output: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o440)
    with os.fdopen(descriptor, "wb") as handle:
        handle.write(canonical_bytes(value))


def _capture(args: argparse.Namespace, *, final: bool) -> None:
    repositories = {
        identifier: repository_pin(root, f"repository {identifier}")
        for identifier, root in parse_repository_arguments(args.repository).items()
    }
    forbidden = sorted({str(Path(value).resolve()) for value in args.forbidden_deployment_environment})
    selected_environment = str(Path(args.deployment_environment).resolve())
    require(selected_environment not in forbidden, "selected environment is also forbidden")
    artifacts: dict[str, Any] = {
        "deploymentEnvironment": file_pin(selected_environment, "deployment environment"),
        "siteDeploymentEnvironment": file_pin(
            args.site_deployment_environment, "site deployment environment"
        ),
        "forbiddenDeploymentEnvironments": forbidden,
        "runtimeBundleManifest": file_pin(args.runtime_bundle_manifest, "runtime bundle manifest"),
        "targetIdentity": file_pin(args.target_identity, "target identity"),
        "nodeCandidateManifest": file_pin(args.node_candidate_manifest, "node candidate manifest"),
        "mediaCandidateManifest": file_pin(args.media_candidate_manifest, "media candidate manifest"),
        "snapshotManifest": file_pin(args.snapshot_manifest, "snapshot manifest"),
        "unityTestResults": {
            "editMode": unity_result(args.unity_editmode_results, "EditMode"),
            "playMode": unity_result(args.unity_playmode_results, "PlayMode"),
        },
    }
    if final:
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
    digest = sha256_file(output)
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
        command.add_argument("--forbidden-deployment-environment", action="append", default=[])
        command.add_argument("--runtime-bundle-manifest", required=True)
        command.add_argument("--target-identity", required=True)
        command.add_argument("--node-candidate-manifest", required=True)
        command.add_argument("--media-candidate-manifest", required=True)
        if final:
            command.add_argument("--acceptance-boundary-receipt", required=True)
            command.add_argument("--read-model-manifest", required=True)
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
