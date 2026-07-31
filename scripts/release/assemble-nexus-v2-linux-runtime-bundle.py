#!/usr/bin/env python3
"""Assemble the closed Nexus V2 runtime bundle from an attested Linux build."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import re
import shutil
import stat
import struct
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path
from typing import Any, Mapping, Sequence


REPO_ROOT = Path(__file__).resolve().parents[2]
NODE_DOCKERFILE = REPO_ROOT / "scripts/release/Dockerfile.node-linux-amd64"
SUPPORT_DOCKERFILE = REPO_ROOT / "scripts/release/Dockerfile.nexus-v2-runtime-support-linux-amd64"
PROBE = REPO_ROOT / "scripts/release/linux-runtime-bundle-probe.py"
PROBE_RUNNER = REPO_ROOT / "scripts/release/linux-runtime-bundle-probe-runner.sh"
NODE_RUNNER = REPO_ROOT / "scripts/release/linux-amd64-node-runner.sh"
PINNED_IMAGE = "docker.io/library/rust:1.89-bookworm@sha256:948f9b08a66e7fe01b03a98ef1c7568292e07ec2e4fe90d88c07bb14563c84ff"
RUSTC_VERSION = "rustc 1.89.0 (29483883e 2025-08-04)"
TARGET_SPEC_VERSION = 106
TARGET_PLATFORM = {
    "architecture": "x86_64",
    "binaryFormat": "elf64",
    "deploymentHostContract": "ubuntu-24.04-x86_64",
    "elfMachine": 62,
    "endianness": "little",
    "libc": "glibc",
    "os": "linux",
}
LINUX_BUILD_FILES = {
    "SHA256SUMS",
    "buildkit-metadata.json",
    "deployment-node-attestation.json",
    "runtime-spec-106.compact.compressed.wasm",
    "solochain-eterra-node",
}
PRIOR_REQUIRED_FILES = {
    "SHA256SUMS",
    "external-reviews.pending.json",
    "nexus-v2-migration-verifier",
    "runtime-bundle-manifest.json",
    "runtime-metadata.json",
    "runtime-metadata.scale",
    "runtime-spec-106.compact.compressed.wasm",
    "runtime-spec-106.try-runtime.wasm",
    "runtime-spec-live-v14.recovery.wasm",
    "tcg-storage-version-observation.json",
    "try-runtime",
}
PROBE_OUTPUT_FILES = {
    "genesis-hash.rpc.json",
    "linux-runtime-probe-result.json",
    "metadata-v15.rpc-proof.json",
    "runtime-metadata.scale",
    "runtime-spec-106.dev-chain-spec.raw.json",
    "runtime-spec-106.temporary-node-embedded-code.wasm",
    "runtime-version.rpc.json",
    "temporary-node-embedded-code.rpc.json",
    "temporary-node.log",
}
SUPPORT_OUTPUT_FILES = {
    "cargo-version.txt",
    "nexus-v2-migration-verifier.linux-amd64",
    "runtime-spec-106.try-runtime.wasm",
    "rustc-version.txt",
    "source-commit.txt",
    "source-date-epoch.txt",
    "source-tree.txt",
}
SHA_RE = re.compile(r"^[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
HASH_RE = re.compile(r"^0x[0-9a-f]{64}$")


class AssemblyError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AssemblyError(message)


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def regular_file(path: Path, label: str, *, executable: bool = False) -> Path:
    require(path.is_file() and not path.is_symlink(), f"{label} must be a regular file")
    if executable:
        require(bool(path.stat().st_mode & stat.S_IXUSR), f"{label} must be executable")
    return path


def read_json(path: Path, label: str) -> dict[str, Any]:
    regular_file(path, label)
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise AssemblyError(f"invalid {label}: {path}") from exc
    require(isinstance(value, dict), f"{label} must be a JSON object")
    return value


def write_json(path: Path, value: Mapping[str, Any], mode: int = 0o600) -> None:
    payload = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, mode)
    with os.fdopen(descriptor, "wb") as handle:
        handle.write(payload)


def ensure_sha(value: Any, label: str) -> str:
    require(isinstance(value, str) and bool(SHA_RE.fullmatch(value)), f"invalid {label}")
    return value


def ensure_commit(value: Any, label: str) -> str:
    require(isinstance(value, str) and bool(COMMIT_RE.fullmatch(value)), f"invalid {label}")
    return value


def git(*args: str) -> str:
    try:
        return subprocess.check_output(
            ["git", "-C", str(REPO_ROOT), *args],
            text=True,
            stderr=subprocess.STDOUT,
        ).strip()
    except (OSError, subprocess.CalledProcessError) as exc:
        raise AssemblyError(f"git {' '.join(args)} failed") from exc


def verify_checksums(root: Path, required: set[str], label: str) -> tuple[dict[str, str], str]:
    root = root.resolve()
    require(root.is_dir() and not root.is_symlink(), f"{label} root must be a regular directory")
    sums = regular_file(root / "SHA256SUMS", f"{label} SHA256SUMS")
    listed: dict[str, str] = {}
    for line in sums.read_text(encoding="utf-8").splitlines():
        parts = line.strip().split(None, 1)
        require(len(parts) == 2, f"invalid {label} SHA256SUMS line")
        digest, relative = parts
        relative = relative.lstrip("*")
        ensure_sha(digest, f"{label} checksum")
        rel = Path(relative)
        require(not rel.is_absolute() and ".." not in rel.parts, f"{label} checksum path escapes root")
        require(relative not in listed, f"duplicate {label} checksum path: {relative}")
        artifact = regular_file(root / rel, f"{label} artifact {relative}")
        require(artifact.resolve().is_relative_to(root), f"{label} artifact escapes root: {relative}")
        require(sha256_file(artifact) == digest, f"{label} artifact hash mismatch: {relative}")
        listed[relative] = digest
    require(required <= set(listed) | {"SHA256SUMS"}, f"{label} checksum set is incomplete")
    return listed, sha256_file(sums)


def inspect_linux_elf(path: Path, label: str) -> None:
    regular_file(path, label, executable=True)
    with path.open("rb") as handle:
        header = handle.read(64)
    require(len(header) == 64, f"{label} ELF header is truncated")
    require(header[:7] == b"\x7fELF\x02\x01\x01", f"{label} is not little-endian ELF64")
    require(struct.unpack_from("<H", header, 18)[0] == 62, f"{label} is not x86-64")


def committed_file_sha(commit: str, path: str) -> str:
    try:
        payload = subprocess.check_output(
            ["git", "-C", str(REPO_ROOT), "show", f"{commit}:{path}"],
            stderr=subprocess.STDOUT,
        )
    except (OSError, subprocess.CalledProcessError) as exc:
        raise AssemblyError(f"cannot read {path} from source commit {commit}") from exc
    return sha256_bytes(payload)


def verify_linux_build(root: Path, expected_source: str, expected_wasm: str) -> dict[str, Any]:
    root = root.resolve()
    require({path.name for path in root.iterdir()} == LINUX_BUILD_FILES, "Linux build root does not match the closed file set")
    listed, sums_sha = verify_checksums(root, LINUX_BUILD_FILES, "Linux build")
    require(set(listed) == LINUX_BUILD_FILES - {"SHA256SUMS"}, "Linux build checksum set is not closed")
    attestation_path = root / "deployment-node-attestation.json"
    value = read_json(attestation_path, "Linux deployment-node attestation")
    require(value.get("schemaVersion") == 1, "Linux build attestation schema mismatch")
    require(value.get("kind") == "nexus-v2-linux-amd64-deployment-node-build", "Linux build attestation kind mismatch")
    require(value.get("sourceCommit") == expected_source, "Linux build source commit mismatch")
    source_epoch = value.get("sourceDateEpoch")
    require(isinstance(source_epoch, int) and source_epoch > 0, "Linux build source epoch is invalid")
    require(source_epoch == int(git("show", "-s", "--format=%ct", expected_source)), "Linux build source epoch mismatch")
    require(value.get("targetPlatform") == TARGET_PLATFORM, "Linux build target platform mismatch")
    environment = value.get("buildEnvironment")
    require(isinstance(environment, dict), "Linux build environment is missing")
    require(environment.get("buildkitPlatform") == "linux/amd64", "Linux BuildKit platform mismatch")
    require(environment.get("containerImage") == PINNED_IMAGE, "Linux build image is not digest-pinned")
    require(environment.get("rustc") == RUSTC_VERSION, "Linux build rustc mismatch")
    require(environment.get("cargoLocked") is True, "Linux build did not lock Cargo dependencies")
    require(environment.get("incremental") is False, "Linux build used incremental compilation")
    require(environment.get("runtimeProductionFeature") is True, "Linux build omitted runtime-production")
    dockerfile_sha = ensure_sha(environment.get("dockerfileSha256"), "Linux build Dockerfile SHA-256")
    require(
        dockerfile_sha == committed_file_sha(expected_source, "scripts/release/Dockerfile.node-linux-amd64"),
        "Linux build Dockerfile does not match its source commit",
    )
    artifacts = value.get("artifacts")
    require(isinstance(artifacts, dict), "Linux build artifact pins are missing")
    node = root / "solochain-eterra-node"
    wasm = root / "runtime-spec-106.compact.compressed.wasm"
    inspect_linux_elf(node, "Linux deployment node")
    require(artifacts.get("nativeNodeSha256") == listed[node.name], "Linux node attestation hash mismatch")
    require(artifacts.get("productionWasmSha256") == listed[wasm.name], "Linux Wasm attestation hash mismatch")
    require(listed[wasm.name] == expected_wasm, "Linux production Wasm does not match its explicit release pin")
    require(
        value.get("authorizations") == {"paidProduction": False, "publicDeploy": False, "publicRelease": False},
        "Linux build activation flags are unsafe",
    )
    return {
        "root": root,
        "attestation": value,
        "attestationSha256": sha256_file(attestation_path),
        "sumsSha256": sums_sha,
        "sourceDateEpoch": source_epoch,
        "nodeSha256": listed[node.name],
        "wasmSha256": listed[wasm.name],
        "buildkitMetadataSha256": listed["buildkit-metadata.json"],
    }


def verify_pending_reviews(path: Path) -> None:
    value = read_json(path, "pending external reviews")
    require(value.get("privateAlphaPermittedOnlyWithEconomicGatesDisabled") is True, "pending-review private-alpha gate is missing")
    reviews = value.get("reviews")
    require(isinstance(reviews, dict) and reviews, "pending external reviews are missing")
    for name, review in reviews.items():
        require(isinstance(review, dict), f"pending review {name} is invalid")
        status = review.get("status")
        require(isinstance(status, str) and status.startswith("pending-"), f"external review {name} is not pending")


def verify_prior_bundle(root: Path, expected_wasm: str, expected_metadata_scale: str, expected_metadata_json: str) -> dict[str, Any]:
    root = root.resolve()
    listed, sums_sha = verify_checksums(root, PRIOR_REQUIRED_FILES, "prior runtime bundle")
    manifest_path = root / "runtime-bundle-manifest.json"
    manifest = read_json(manifest_path, "prior runtime-bundle manifest")
    require(manifest.get("schemaVersion") == 1, "prior runtime bundle schema mismatch")
    require(manifest.get("kind") == "nexus-v2-private-alpha-runtime-bundle", "prior runtime bundle kind mismatch")
    require(manifest.get("targetSpecVersion") == TARGET_SPEC_VERSION, "prior runtime bundle spec mismatch")
    source_commit = ensure_commit(manifest.get("sourceCommit"), "prior runtime source commit")
    artifacts = manifest.get("artifacts")
    require(isinstance(artifacts, dict), "prior runtime artifact pins are missing")
    require(artifacts.get("stagedProductionWasmSha256") == expected_wasm, "superseded production Wasm pin mismatch")
    require(listed["runtime-spec-106.compact.compressed.wasm"] == expected_wasm, "superseded production Wasm file mismatch")
    require(artifacts.get("metadataScaleSha256") == expected_metadata_scale, "prior metadata SCALE pin mismatch")
    require(artifacts.get("metadataJsonSha256") == expected_metadata_json, "prior metadata JSON pin mismatch")
    require(listed["runtime-metadata.scale"] == expected_metadata_scale, "prior metadata SCALE file mismatch")
    require(listed["runtime-metadata.json"] == expected_metadata_json, "prior metadata JSON file mismatch")
    require(manifest.get("runtimeIdentity", {}).get("stagedProductionMatchesTemporaryNodeEmbeddedCode") is True, "prior bundle lacks its old embedded-code proof")
    authorizations = manifest.get("authorizations")
    require(isinstance(authorizations, dict), "prior bundle authorizations are missing")
    for field in ("publicRelease", "publicDeploy", "paidProduction", "externalReviewsSelfApproved"):
        require(authorizations.get(field) is False, f"prior bundle unsafe authorization: {field}")
    observation = read_json(root / "tcg-storage-version-observation.json", "TCG storage-version observation")
    require(observation.get("decoded", {}).get("storageVersion") == 14, "TCG observation does not decode StorageVersion 14")
    require(observation.get("readOnlyRpc", {}).get("result") == "0x0e00", "TCG observation lacks SCALE u16 V14")
    verify_pending_reviews(root / "external-reviews.pending.json")
    host_verifier = regular_file(root / "nexus-v2-migration-verifier", "preserved host migration verifier", executable=True)
    try_runtime = regular_file(root / "try-runtime", "preserved try-runtime CLI", executable=True)
    require(artifacts.get("migrationVerifierSha256") == listed[host_verifier.name], "prior migration verifier pin mismatch")
    require(artifacts.get("tryRuntimeCliSha256") == listed[try_runtime.name], "prior try-runtime CLI pin mismatch")
    return {
        "root": root,
        "manifest": manifest,
        "manifestSha256": sha256_file(manifest_path),
        "sumsSha256": sums_sha,
        "sourceCommit": source_commit,
        "nodeSha256": listed["solochain-eterra-node"],
        "wasmSha256": expected_wasm,
        "tryWasmSha256": listed["runtime-spec-106.try-runtime.wasm"],
        "metadataScaleSha256": expected_metadata_scale,
        "metadataJsonSha256": expected_metadata_json,
        "tryRuntimeSha256": listed[try_runtime.name],
        "migrationVerifierSha256": listed[host_verifier.name],
    }


def prove_tooling_only_delta(old_commit: str, new_commit: str) -> list[str]:
    ensure_commit(old_commit, "old source commit")
    ensure_commit(new_commit, "new source commit")
    changed_text = git("diff", "--name-only", f"{old_commit}..{new_commit}")
    changed = [line for line in changed_text.splitlines() if line]
    disallowed = [path for path in changed if not path.startswith(("deploy/", "docs/", "scripts/"))]
    require(not disallowed, "runtime-affecting source changed since the prior bundle: " + ", ".join(disallowed))
    return changed


def extract_source(commit: str, destination: Path) -> None:
    archive = destination.parent / "source.tar"
    with archive.open("xb") as handle:
        completed = subprocess.run(
            ["git", "-C", str(REPO_ROOT), "archive", "--format=tar", commit],
            stdout=handle,
            stderr=subprocess.PIPE,
            check=False,
        )
    require(completed.returncode == 0, "failed to archive the attested runtime source commit")
    destination.mkdir(mode=0o700)
    with tarfile.open(archive, "r") as tar:
        for member in tar.getmembers():
            target = (destination / member.name).resolve()
            require(target.is_relative_to(destination.resolve()), "source archive path escapes extraction root")
        tar.extractall(destination)


def run_support_build(source_commit: str, source_tree: str, source_epoch: int, temporary: Path) -> dict[str, Any]:
    source = temporary / "source"
    extract_source(source_commit, source)
    export = temporary / "support-export"
    metadata = temporary / "support-buildkit-metadata.json"
    command = [
        "docker",
        "buildx",
        "build",
        "--platform",
        "linux/amd64",
        "--file",
        str(SUPPORT_DOCKERFILE),
        "--build-arg",
        f"SOURCE_COMMIT={source_commit}",
        "--build-arg",
        f"SOURCE_TREE={source_tree}",
        "--build-arg",
        f"SOURCE_DATE_EPOCH={source_epoch}",
        "--metadata-file",
        str(metadata),
        "--output",
        f"type=local,dest={export}",
        str(source),
    ]
    completed = subprocess.run(command, check=False)
    require(completed.returncode == 0, "pinned Linux runtime-support build failed")
    require(export.is_dir(), "Linux runtime-support export is missing")
    require({path.name for path in export.iterdir()} == SUPPORT_OUTPUT_FILES, "Linux runtime-support export is not closed")
    require((export / "source-commit.txt").read_text().strip() == source_commit, "runtime-support source commit mismatch")
    require((export / "source-tree.txt").read_text().strip() == source_tree, "runtime-support source tree mismatch")
    require((export / "source-date-epoch.txt").read_text().strip() == str(source_epoch), "runtime-support source epoch mismatch")
    require((export / "rustc-version.txt").read_text().strip() == RUSTC_VERSION, "runtime-support rustc mismatch")
    inspect_linux_elf(export / "nexus-v2-migration-verifier.linux-amd64", "Linux migration verifier")
    return {
        "root": export,
        "metadata": regular_file(metadata, "runtime-support BuildKit metadata"),
        "tryWasmSha256": sha256_file(export / "runtime-spec-106.try-runtime.wasm"),
        "linuxVerifierSha256": sha256_file(export / "nexus-v2-migration-verifier.linux-amd64"),
        "rustc": (export / "rustc-version.txt").read_text().strip(),
        "cargo": (export / "cargo-version.txt").read_text().strip(),
    }


def validate_probe_boundary(runner: Path, probe: Path, node: Path) -> None:
    regular_file(runner, "Linux runtime-probe runner", executable=True)
    regular_file(probe, "Linux runtime probe", executable=True)
    inspect_linux_elf(node, "Linux runtime-probe node")
    require(runner.resolve() == PROBE_RUNNER.resolve(), "Linux runtime-probe runner was swapped")
    require(probe.resolve() == PROBE.resolve(), "Linux runtime probe was swapped")
    require(node.resolve() not in {runner.resolve(), probe.resolve()}, "Linux runner/probe and node identities were swapped")


def runtime_probe_command(workspace: Path) -> list[str]:
    return [
        str(PROBE_RUNNER),
        "--workspace",
        str(workspace),
        "--probe",
        PROBE.name,
        "--",
        "--node",
        "/work/solochain-eterra-node",
        "--production-wasm",
        "/work/runtime-spec-106.compact.compressed.wasm",
        "--output",
        "/work/probe-output",
    ]


def run_linux_probe(linux_root: Path, temporary: Path) -> dict[str, Any]:
    validate_probe_boundary(PROBE_RUNNER, PROBE, linux_root / "solochain-eterra-node")
    workspace = temporary / "probe-workspace"
    workspace.mkdir(mode=0o700)
    shutil.copy2(linux_root / "solochain-eterra-node", workspace / "solochain-eterra-node")
    os.chmod(workspace / "solochain-eterra-node", 0o755)
    shutil.copyfile(
        linux_root / "runtime-spec-106.compact.compressed.wasm",
        workspace / "runtime-spec-106.compact.compressed.wasm",
    )
    shutil.copy2(PROBE, workspace / PROBE.name)
    os.chmod(workspace / PROBE.name, 0o755)
    command = runtime_probe_command(workspace)
    completed = subprocess.run(command, check=False)
    require(completed.returncode == 0, "pinned network-disabled Linux runtime probe failed")
    output = workspace / "probe-output"
    require(output.is_dir(), "Linux runtime probe output is missing")
    require({path.name for path in output.iterdir()} == PROBE_OUTPUT_FILES, "Linux runtime probe output is not closed")
    return {"root": output, "command": command}


def raw_runtime_code(path: Path) -> bytes:
    value = read_json(path, "dev raw chain spec")
    encoded = value.get("genesis", {}).get("raw", {}).get("top", {}).get("0x3a636f6465")
    require(isinstance(encoded, str) and encoded.startswith("0x"), "dev raw chain spec has no :code")
    try:
        return bytes.fromhex(encoded[2:])
    except ValueError as exc:
        raise AssemblyError("dev raw chain spec :code is invalid hex") from exc


def derive_metadata_json(subxt: Path, expected_sha: str, scale: Path, output: Path) -> str:
    regular_file(subxt, "subxt CLI", executable=True)
    require(sha256_file(subxt) == expected_sha, "subxt CLI hash mismatch")
    version = subprocess.run(
        [str(subxt), "version"],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    require(version.returncode == 0 and version.stdout.strip(), "subxt CLI version probe failed")
    completed = subprocess.run(
        [
            str(subxt),
            "metadata",
            "--file",
            str(scale),
            "--format",
            "json",
            "--output-file",
            str(output),
        ],
        check=False,
    )
    require(completed.returncode == 0, "subxt failed to decode Linux-derived Metadata V15")
    regular_file(output, "decoded runtime metadata JSON")
    return version.stdout.strip()


def copy_artifact(source: Path, destination: Path, *, executable: bool = False) -> None:
    regular_file(source, f"source artifact {source.name}", executable=executable)
    shutil.copyfile(source, destination)
    os.chmod(destination, 0o755 if executable else 0o600)


def assemble(args: argparse.Namespace) -> Path:
    source_commit = ensure_commit(args.source_commit, "runtime source commit")
    expected_wasm = ensure_sha(args.expected_production_wasm_sha256, "production Wasm SHA-256")
    old_wasm = ensure_sha(args.expected_superseded_wasm_sha256, "superseded Wasm SHA-256")
    expected_metadata_scale = ensure_sha(args.expected_metadata_scale_sha256, "metadata SCALE SHA-256")
    expected_metadata_json = ensure_sha(args.expected_metadata_json_sha256, "metadata JSON SHA-256")
    subxt_sha = ensure_sha(args.subxt_sha256, "subxt CLI SHA-256")
    try_revision = ensure_commit(args.try_runtime_revision, "try-runtime source revision")
    require(expected_wasm != old_wasm, "old macOS production Wasm cannot be accepted as the Linux release target")
    require(git("cat-file", "-t", source_commit) == "commit", "runtime source commit is unavailable")

    output = Path(args.output).resolve()
    require(not output.exists(), "refusing to overwrite or merge a runtime bundle")
    require(not git("status", "--porcelain", "--untracked-files=all"), "runtime assembly requires a clean release-tool worktree")
    assembly_commit = ensure_commit(git("rev-parse", "HEAD"), "assembly source commit")
    assembly_tree = git("rev-parse", "HEAD^{tree}")
    source_tree = git("rev-parse", f"{source_commit}^{{tree}}")
    for relative, path in (
        ("scripts/release/assemble-nexus-v2-linux-runtime-bundle.py", Path(__file__).resolve()),
        ("scripts/release/Dockerfile.nexus-v2-runtime-support-linux-amd64", SUPPORT_DOCKERFILE),
        ("scripts/release/linux-runtime-bundle-probe.py", PROBE),
        ("scripts/release/linux-runtime-bundle-probe-runner.sh", PROBE_RUNNER),
        ("scripts/release/linux-amd64-node-runner.sh", NODE_RUNNER),
    ):
        require(
            committed_file_sha(assembly_commit, relative) == sha256_file(path),
            f"release tool does not match clean assembly commit: {relative}",
        )

    linux = verify_linux_build(Path(args.linux_build_root), source_commit, expected_wasm)
    prior = verify_prior_bundle(
        Path(args.prior_runtime_bundle),
        old_wasm,
        expected_metadata_scale,
        expected_metadata_json,
    )
    runtime_delta = prove_tooling_only_delta(prior["sourceCommit"], source_commit)
    assembly_delta = prove_tooling_only_delta(source_commit, assembly_commit)
    prior_manifest = prior["manifest"]
    require(prior_manifest.get("tools", {}).get("tryRuntimeRevision") == try_revision, "try-runtime revision changed from the prior closed bundle")

    try_runtime = prior["root"] / "try-runtime"
    try_version = subprocess.run(
        [str(try_runtime), "--version"],
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    require(try_version.returncode == 0 and try_version.stdout.strip(), "preserved try-runtime CLI cannot execute on the coordinator host")
    host_verifier = prior["root"] / "nexus-v2-migration-verifier"
    verifier_help = subprocess.run(
        [str(host_verifier), "--help"],
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    require(verifier_help.returncode == 0, "preserved host migration verifier cannot execute")

    subxt = Path(args.subxt_bin).resolve()
    with tempfile.TemporaryDirectory(prefix="nexus-v2-linux-runtime-assembly.") as temporary_name:
        temporary = Path(temporary_name)
        support = run_support_build(source_commit, source_tree, linux["sourceDateEpoch"], temporary)
        probe = run_linux_probe(linux["root"], temporary)
        probe_root = probe["root"]
        production_wasm = linux["root"] / "runtime-spec-106.compact.compressed.wasm"
        embedded_wasm = probe_root / "runtime-spec-106.temporary-node-embedded-code.wasm"
        require(sha256_file(embedded_wasm) == expected_wasm, "Linux temporary node embedded :code hash mismatch")
        require(raw_runtime_code(probe_root / "runtime-spec-106.dev-chain-spec.raw.json") == production_wasm.read_bytes(), "Linux dev spec embeds different :code")
        metadata_scale = probe_root / "runtime-metadata.scale"
        require(metadata_scale.read_bytes()[:5] == b"meta\x0f", "Linux-derived runtime metadata is not V15")
        require(sha256_file(metadata_scale) == expected_metadata_scale, "Linux-derived Metadata V15 is not byte-compatible with the prior baseline")
        metadata_json = probe_root / "runtime-metadata.json"
        subxt_version = derive_metadata_json(subxt, subxt_sha, metadata_scale, metadata_json)
        require(sha256_file(metadata_json) == expected_metadata_json, "Linux-derived metadata JSON is not byte-compatible with the prior baseline")
        require(metadata_json.read_bytes() == (prior["root"] / "runtime-metadata.json").read_bytes(), "metadata JSON exact-equality proof failed")
        require(metadata_scale.read_bytes() == (prior["root"] / "runtime-metadata.scale").read_bytes(), "metadata SCALE exact-equality proof failed")
        probe_value = read_json(probe_root / "linux-runtime-probe-result.json", "Linux runtime probe result")
        require(probe_value.get("specVersion") == TARGET_SPEC_VERSION, "Linux runtime probe spec mismatch")
        genesis_hash = probe_value.get("genesisHash")
        require(isinstance(genesis_hash, str) and bool(HASH_RE.fullmatch(genesis_hash)), "Linux runtime probe genesis hash is invalid")

        output.mkdir(mode=0o700, parents=True)
        copy_artifact(linux["root"] / "solochain-eterra-node", output / "solochain-eterra-node", executable=True)
        copy_artifact(production_wasm, output / "runtime-spec-106.compact.compressed.wasm")
        copy_artifact(support["root"] / "runtime-spec-106.try-runtime.wasm", output / "runtime-spec-106.try-runtime.wasm")
        copy_artifact(prior["root"] / "runtime-spec-live-v14.recovery.wasm", output / "runtime-spec-live-v14.recovery.wasm")
        copy_artifact(prior["root"] / "tcg-storage-version-observation.json", output / "tcg-storage-version-observation.json")
        copy_artifact(prior["root"] / "external-reviews.pending.json", output / "external-reviews.pending.json")
        copy_artifact(try_runtime, output / "try-runtime", executable=True)
        copy_artifact(host_verifier, output / "nexus-v2-migration-verifier", executable=True)
        copy_artifact(
            support["root"] / "nexus-v2-migration-verifier.linux-amd64",
            output / "nexus-v2-migration-verifier.linux-amd64",
            executable=True,
        )
        for name in sorted(PROBE_OUTPUT_FILES - {"temporary-node.log"}):
            copy_artifact(probe_root / name, output / name)
        copy_artifact(metadata_json, output / "runtime-metadata.json")
        copy_artifact(linux["root"] / "deployment-node-attestation.json", output / "deployment-node-attestation.json")
        copy_artifact(linux["root"] / "buildkit-metadata.json", output / "deployment-node-buildkit-metadata.json")
        copy_artifact(linux["root"] / "SHA256SUMS", output / "deployment-node-SHA256SUMS")
        copy_artifact(support["metadata"], output / "runtime-support-buildkit-metadata.json")

        support_attestation = {
            "schemaVersion": 1,
            "kind": "nexus-v2-linux-runtime-support-build",
            "sourceCommit": source_commit,
            "sourceTree": source_tree,
            "sourceDateEpoch": linux["sourceDateEpoch"],
            "targetPlatform": dict(TARGET_PLATFORM),
            "buildEnvironment": {
                "buildkitPlatform": "linux/amd64",
                "cargoLocked": True,
                "containerImage": PINNED_IMAGE,
                "dockerfileSha256": sha256_file(SUPPORT_DOCKERFILE),
                "incremental": False,
                "rustc": support["rustc"],
                "cargo": support["cargo"],
            },
            "artifacts": {
                "buildkitMetadataSha256": sha256_file(support["metadata"]),
                "linuxMigrationVerifierSha256": support["linuxVerifierSha256"],
                "tryRuntimeWasmSha256": support["tryWasmSha256"],
            },
            "authorizations": {"paidProduction": False, "publicDeploy": False, "publicRelease": False},
        }
        write_json(output / "runtime-support-build-attestation.json", support_attestation)

        compatibility = {
            "schemaVersion": 1,
            "kind": "nexus-v2-runtime-metadata-compatibility-proof",
            "baseline": {
                "bundleManifestSha256": prior["manifestSha256"],
                "sourceCommit": prior["sourceCommit"],
                "metadataScaleSha256": prior["metadataScaleSha256"],
                "metadataJsonSha256": prior["metadataJsonSha256"],
            },
            "candidate": {
                "sourceCommit": source_commit,
                "metadataScaleSha256": sha256_file(metadata_scale),
                "metadataJsonSha256": sha256_file(metadata_json),
                "metadataVersion": 15,
            },
            "exactScaleBytesEqual": True,
            "exactDecodedJsonBytesEqual": True,
            "sourceDeltaIsReleaseToolingOnly": True,
            "runtimeSourceDeltaPaths": runtime_delta,
            "result": "compatible",
        }
        write_json(output / "metadata-compatibility.json", compatibility)

        superseded = {
            "schemaVersion": 1,
            "kind": "nexus-v2-superseded-runtime-identity",
            "sourceCommit": prior["sourceCommit"],
            "bundleManifestSha256": prior["manifestSha256"],
            "bundleSha256SumsSha256": prior["sumsSha256"],
            "nativeNodeSha256": prior["nodeSha256"],
            "productionWasmSha256": prior["wasmSha256"],
            "tryRuntimeWasmSha256": prior["tryWasmSha256"],
            "status": "superseded-not-a-release-target",
            "reason": "The macOS build embedded absolute host source paths; the digest-pinned Linux /source build is authoritative for the Ubuntu x86_64 deployment target.",
            "productionWasmCopiedIntoBundle": False,
            "authorizations": {"deploy": False, "release": False, "restoreTarget": False},
        }
        write_json(output / "superseded-runtime-identity.json", superseded)

        created = dt.datetime.fromtimestamp(linux["sourceDateEpoch"], tz=dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
        artifact_hashes = {
            path.name: sha256_file(path)
            for path in sorted(output.iterdir(), key=lambda item: item.name)
            if path.is_file()
        }
        manifest = {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-runtime-bundle",
            "releaseId": f"nexus-v2-private-alpha-linux-{source_commit[:12]}",
            "createdAtUtc": created,
            "sourceCommit": source_commit,
            "sourceTree": source_tree,
            "sourceDateEpoch": linux["sourceDateEpoch"],
            "sourceStorageVersion": 14,
            "targetStorageVersion": 16,
            "targetSpecVersion": TARGET_SPEC_VERSION,
            "runtimeIdentity": {
                "codeStorageKey": "0x3a636f6465",
                "authoritativeTargetPlatform": dict(TARGET_PLATFORM),
                "stagedProductionMatchesTemporaryNodeEmbeddedCode": True,
                "devChainSpecMatchesStagedProductionCode": True,
                "metadataScaleAndJsonExactlyMatchCompatibilityBaseline": True,
                "nativeHostExecutionAllowed": False,
                "candidateRunnerSha256": sha256_file(NODE_RUNNER),
                "probeRunnerNetworkDisabled": True,
                "probeReadOnlyRootFilesystem": True,
                "probeEphemeralWorkspaceOnly": True,
                "probeRunnerSha256": sha256_file(PROBE_RUNNER),
                "probeSha256": sha256_file(PROBE),
                "genesisHash": genesis_hash,
            },
            "artifacts": {
                "nativeNodeSha256": linux["nodeSha256"],
                "migrationVerifierSha256": prior["migrationVerifierSha256"],
                "linuxMigrationVerifierSha256": support["linuxVerifierSha256"],
                "stagedProductionWasmSha256": expected_wasm,
                "temporaryNodeEmbeddedWasmSha256": expected_wasm,
                "temporaryNodeEmbeddedCodeRpcSha256": artifact_hashes["temporary-node-embedded-code.rpc.json"],
                "tryRuntimeWasmSha256": support["tryWasmSha256"],
                "liveV14WasmSha256": artifact_hashes["runtime-spec-live-v14.recovery.wasm"],
                "metadataScaleSha256": expected_metadata_scale,
                "metadataJsonSha256": expected_metadata_json,
                "tryRuntimeCliSha256": prior["tryRuntimeSha256"],
                "tcgStorageVersionObservationSha256": artifact_hashes["tcg-storage-version-observation.json"],
                "pendingExternalReviewsSha256": artifact_hashes["external-reviews.pending.json"],
                "deploymentNodeAttestationSha256": artifact_hashes["deployment-node-attestation.json"],
                "deploymentNodeSha256SumsSha256": artifact_hashes["deployment-node-SHA256SUMS"],
                "runtimeSupportBuildAttestationSha256": artifact_hashes["runtime-support-build-attestation.json"],
                "metadataCompatibilityProofSha256": artifact_hashes["metadata-compatibility.json"],
                "supersededRuntimeIdentitySha256": artifact_hashes["superseded-runtime-identity.json"],
                "devChainSpecSha256": artifact_hashes["runtime-spec-106.dev-chain-spec.raw.json"],
                "genesisHashRpcSha256": artifact_hashes["genesis-hash.rpc.json"],
                "runtimeVersionRpcSha256": artifact_hashes["runtime-version.rpc.json"],
            },
            "buildProvenance": {
                "deploymentNodeBuildAttestationSha256": linux["attestationSha256"],
                "deploymentNodeBuildkitMetadataSha256": linux["buildkitMetadataSha256"],
                "deploymentNodeSha256SumsSha256": linux["sumsSha256"],
                "containerImage": PINNED_IMAGE,
                "runtimeSupportDockerfileSha256": sha256_file(SUPPORT_DOCKERFILE),
                "nodeDockerfileSha256": linux["attestation"]["buildEnvironment"]["dockerfileSha256"],
            },
            "preservedHostTools": {
                "sourceBundleManifestSha256": prior["manifestSha256"],
                "sourceCommit": prior["sourceCommit"],
                "sourceDeltaIsReleaseToolingOnly": True,
                "tryRuntimeCliSha256": prior["tryRuntimeSha256"],
                "migrationVerifierSha256": prior["migrationVerifierSha256"],
            },
            "tools": {
                "assemblySourceCommit": assembly_commit,
                "assemblySourceTree": assembly_tree,
                "assemblyDeltaIsReleaseToolingOnly": True,
                "assemblyDeltaPaths": assembly_delta,
                "assemblerSha256": sha256_file(Path(__file__).resolve()),
                "tryRuntimeRevision": try_revision,
                "tryRuntimeVersion": try_version.stdout.strip(),
                "rustc": support["rustc"],
                "cargo": support["cargo"],
                "subxt": subxt_version,
                "subxtSha256": subxt_sha,
            },
            "authorizations": {
                "localBuildOnly": True,
                "publicRelease": False,
                "publicDeploy": False,
                "paidProduction": False,
                "externalReviewsSelfApproved": False,
            },
        }
        write_json(output / "runtime-bundle-manifest.json", manifest)

    checksummed = sorted(path for path in output.iterdir() if path.is_file() and path.name != "SHA256SUMS")
    require(all(not path.is_symlink() for path in checksummed), "runtime bundle contains a symlink")
    sums_payload = "".join(f"{sha256_file(path)}  {path.name}\n" for path in checksummed).encode()
    descriptor = os.open(output / "SHA256SUMS", os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "wb") as handle:
        handle.write(sums_payload)
    verify_checksums(output, {path.name for path in output.iterdir()}, "assembled runtime bundle")
    return output


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Assemble a closed Nexus V2 bundle from the attested Linux/amd64 runtime build without live-chain contact."
    )
    parser.add_argument("--linux-build-root", required=True)
    parser.add_argument("--prior-runtime-bundle", required=True)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--expected-production-wasm-sha256", required=True)
    parser.add_argument("--expected-superseded-wasm-sha256", required=True)
    parser.add_argument("--expected-metadata-scale-sha256", required=True)
    parser.add_argument("--expected-metadata-json-sha256", required=True)
    parser.add_argument("--try-runtime-revision", required=True)
    parser.add_argument("--subxt-bin", required=True)
    parser.add_argument("--subxt-sha256", required=True)
    parser.add_argument("--output", required=True)
    return parser.parse_args(argv)


def main() -> None:
    try:
        output = assemble(parse_args())
    except AssemblyError as exc:
        print(f"runtime bundle assembly failed: {exc}", file=sys.stderr)
        raise SystemExit(2)
    print(f"Nexus V2 Linux runtime bundle ready: {output}")


if __name__ == "__main__":
    main()
