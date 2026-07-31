#!/usr/bin/env python3
"""Build and verify immutable Nexus V2 private-alpha node candidates.

The candidate is assembled entirely from an already-built runtime bundle and
address-only Alpha overrides.  No source build, remote command, or live Alpha
request is performed.  The bundled native node is used only to deterministically
finalize the plain/raw spec and to start an isolated temporary genesis node.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any, Mapping, Sequence


REPO_ROOT = Path(__file__).resolve().parents[2]
FINALIZE_ALPHA = REPO_ROOT / "scripts/finalize-alpha-spec.py"
VERIFY_ALPHA = REPO_ROOT / "scripts/verify-alpha-spec.py"
CODE_STORAGE_KEY = "0x3a636f6465"
TARGET_SPEC_VERSION = 106
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
HASH256_RE = re.compile(r"^0x[0-9a-f]{64}$")
RELEASE_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
PUBLIC_OVERRIDE_KEYS = {
    "name",
    "bootnodes",
    "aura_authorities",
    "grandpa_authorities",
    "balances",
    "sudo_key",
    "faucet_account",
    "faucet_payout_amount",
    "initial_servers",
    "season_admins",
    "media_collection_owner",
    "council_members",
    "asset_owner",
}
CANDIDATE_FILES = {
    "alpha-plain.json",
    "alpha-public-overrides.json",
    "alpha-raw.json",
    "eterra-alpha-node.service",
    "solochain-eterra-node",
    "start-alpha-node.sh",
}
TARGET_IDENTITY_KEYS = {
    "authorizations",
    "deploymentSourceCommit",
    "genesisHash",
    "kind",
    "network",
    "nodeCandidateManifestSha256",
    "releaseId",
    "runtimeCodeHash",
    "runtimeMetadata",
    "runtimeSourceCommit",
    "schemaVersion",
    "specVersion",
    "tcgStorageVersion",
}


class CandidateError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CandidateError(message)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def write_new_json(path: Path, value: Mapping[str, Any], mode: int = 0o600) -> None:
    payload = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, mode)
    with os.fdopen(descriptor, "wb") as handle:
        handle.write(payload)


def read_json(path: Path, label: str) -> dict[str, Any]:
    require(path.is_file() and not path.is_symlink(), f"{label} must be a regular file")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise CandidateError(f"invalid {label}: {path}") from exc
    require(isinstance(value, dict), f"{label} root must be an object")
    return value


def regular_file(path: Path, label: str, executable: bool = False) -> Path:
    require(path.exists() and path.is_file() and not path.is_symlink(), f"{label} must be a regular file: {path}")
    if executable:
        require(bool(path.stat().st_mode & stat.S_IXUSR), f"{label} must be executable: {path}")
    return path.resolve()


def ensure_sha(value: Any, label: str) -> str:
    require(isinstance(value, str) and bool(SHA256_RE.fullmatch(value)), f"invalid {label}")
    return value


def ensure_commit(value: Any, label: str) -> str:
    require(isinstance(value, str) and bool(COMMIT_RE.fullmatch(value)), f"invalid {label}")
    return value


def ensure_release(value: Any) -> str:
    require(isinstance(value, str) and bool(RELEASE_RE.fullmatch(value)), "invalid release ID")
    return value


def ensure_port(value: int, label: str) -> int:
    require(isinstance(value, int) and 1024 <= value <= 65535, f"invalid {label}")
    return value


def path_within(path: Path, root: Path) -> bool:
    try:
        path.resolve().relative_to(root.resolve())
        return True
    except ValueError:
        return False


def validate_public_overrides(path: Path) -> dict[str, Any]:
    value = read_json(path, "Alpha public overrides")
    require(set(value) == PUBLIC_OVERRIDE_KEYS, "Alpha overrides do not match the address-only closed schema")
    encoded = json.dumps(value, sort_keys=True)
    for forbidden in ("mnemonic", "suri", "seed_phrase", "private_key", "password", "secret"):
        require(forbidden not in encoded.lower(), f"Alpha public overrides contain forbidden secret material: {forbidden}")
    def public_address(item: Any, label: str) -> str:
        require(
            isinstance(item, str)
            and item
            and "//" not in item
            and not any(ch.isspace() for ch in item),
            f"{label} must be a public address",
        )
        return item

    for field in (
        "aura_authorities",
        "initial_servers",
        "season_admins",
        "council_members",
    ):
        require(isinstance(value[field], list), f"{field} must be an array")
        for index, item in enumerate(value[field]):
            public_address(item, f"{field}[{index}]")
    for field in ("sudo_key", "faucet_account", "media_collection_owner", "asset_owner"):
        public_address(value[field], field)
    require(isinstance(value["balances"], list) and value["balances"], "balances must be non-empty")
    for index, entry in enumerate(value["balances"]):
        require(isinstance(entry, list) and len(entry) == 2, f"balances[{index}] must be [address, amount]")
        public_address(entry[0], f"balances[{index}][0]")
        require(
            isinstance(entry[1], int) and not isinstance(entry[1], bool) and entry[1] > 0,
            f"balances[{index}][1] must be a positive integer",
        )
    require(isinstance(value["grandpa_authorities"], list) and value["grandpa_authorities"], "grandpa_authorities must be non-empty")
    for index, entry in enumerate(value["grandpa_authorities"]):
        require(isinstance(entry, list) and len(entry) == 2, f"grandpa_authorities[{index}] must be [address, weight]")
        public_address(entry[0], f"grandpa_authorities[{index}][0]")
        require(
            isinstance(entry[1], int) and not isinstance(entry[1], bool) and entry[1] > 0,
            f"grandpa_authorities[{index}][1] must be a positive integer",
        )
    require(isinstance(value["bootnodes"], list), "bootnodes must be an array")
    for index, bootnode in enumerate(value["bootnodes"]):
        require(
            isinstance(bootnode, str)
            and bootnode
            and not any(ch.isspace() for ch in bootnode)
            and "//" not in bootnode,
            f"bootnodes[{index}] must be a public multiaddress",
        )
    require(isinstance(value["name"], str) and value["name"], "name must be non-empty")
    require(isinstance(value["faucet_payout_amount"], int) and value["faucet_payout_amount"] > 0, "faucet payout must be positive")
    return value


def verify_runtime_bundle(root: Path) -> dict[str, Any]:
    root = root.resolve()
    require(root.is_dir() and not root.is_symlink(), f"runtime bundle root not found: {root}")
    sums_path = regular_file(root / "SHA256SUMS", "runtime bundle SHA256SUMS")
    listed: dict[str, str] = {}
    for raw_line in sums_path.read_text(encoding="utf-8").splitlines():
        parts = raw_line.strip().split(None, 1)
        require(len(parts) == 2, "invalid runtime bundle SHA256SUMS line")
        digest, relative = parts
        relative = relative.lstrip("*")
        ensure_sha(digest, "runtime bundle artifact SHA-256")
        rel = Path(relative)
        require(not rel.is_absolute() and ".." not in rel.parts, "runtime bundle checksum path escapes the bundle")
        require(relative not in listed, f"duplicate runtime bundle checksum path: {relative}")
        artifact = regular_file(root / rel, f"runtime bundle artifact {relative}")
        require(path_within(artifact, root), f"runtime bundle artifact escapes root: {relative}")
        require(sha256_file(artifact) == digest, f"runtime bundle artifact hash mismatch: {relative}")
        listed[relative] = digest
    required = {
        "runtime-bundle-manifest.json",
        "runtime-spec-106.compact.compressed.wasm",
        "solochain-eterra-node",
    }
    require(required <= set(listed), "runtime bundle checksum set is incomplete")
    manifest_path = root / "runtime-bundle-manifest.json"
    manifest = read_json(manifest_path, "runtime bundle manifest")
    require(manifest.get("schemaVersion") == 1, "unsupported runtime bundle schema")
    require(manifest.get("kind") == "nexus-v2-private-alpha-runtime-bundle", "runtime bundle kind mismatch")
    require(manifest.get("targetSpecVersion") == TARGET_SPEC_VERSION, "runtime bundle spec version mismatch")
    require(manifest.get("targetStorageVersion") == 16, "runtime bundle target storage version mismatch")
    source_commit = ensure_commit(manifest.get("sourceCommit"), "runtime source commit")
    artifacts = manifest.get("artifacts")
    require(isinstance(artifacts, dict), "runtime bundle artifact pins are missing")
    node_sha = ensure_sha(artifacts.get("nativeNodeSha256"), "runtime native-node SHA-256")
    wasm_sha = ensure_sha(artifacts.get("stagedProductionWasmSha256"), "production Wasm SHA-256")
    metadata_sha = ensure_sha(artifacts.get("metadataScaleSha256"), "runtime metadata SHA-256")
    require(listed["solochain-eterra-node"] == node_sha, "runtime manifest native-node hash mismatch")
    require(listed["runtime-spec-106.compact.compressed.wasm"] == wasm_sha, "runtime manifest Wasm hash mismatch")
    require(listed.get("runtime-metadata.scale") == metadata_sha, "runtime manifest metadata hash mismatch")
    metadata_prefix = (root / "runtime-metadata.scale").read_bytes()[:5]
    require(len(metadata_prefix) == 5 and metadata_prefix[:4] == b"meta", "runtime metadata SCALE magic mismatch")
    metadata_version = metadata_prefix[4]
    require(metadata_version == 15, "spec-106 target metadata must be V15")
    require(manifest.get("runtimeIdentity", {}).get("stagedProductionMatchesTemporaryNodeEmbeddedCode") is True, "runtime bundle did not prove embedded production code")
    authorizations = manifest.get("authorizations")
    require(isinstance(authorizations, dict), "runtime bundle authorization block is missing")
    require(authorizations.get("publicRelease") is False, "runtime bundle permits public release")
    require(authorizations.get("publicDeploy") is False, "runtime bundle permits public deployment")
    require(authorizations.get("paidProduction") is False, "runtime bundle permits paid production")
    return {
        "root": root,
        "manifest": manifest,
        "manifestSha256": sha256_file(manifest_path),
        "sumsSha256": sha256_file(sums_path),
        "sourceCommit": source_commit,
        "nodeSha256": node_sha,
        "wasmSha256": wasm_sha,
        "metadataSha256": metadata_sha,
        "metadataVersion": metadata_version,
    }


def run_checked(command: Sequence[str], label: str, stdout_path: Path | None = None) -> None:
    try:
        if stdout_path is None:
            subprocess.run(list(command), check=True)
        else:
            with stdout_path.open("xb") as handle:
                subprocess.run(list(command), stdout=handle, check=True)
    except (OSError, subprocess.CalledProcessError) as exc:
        raise CandidateError(f"{label} failed") from exc


def finalize_specs(node: Path, overrides: Path, output: Path) -> None:
    run_checked(
        [
            sys.executable,
            str(FINALIZE_ALPHA),
            "--node-bin",
            str(node),
            "--overrides",
            str(overrides),
            "--out-dir",
            str(output),
        ],
        "Alpha spec finalization",
    )
    regular_file(output / "alpha-plain.json", "finalized Alpha plain spec")
    regular_file(output / "alpha-raw.json", "finalized Alpha raw spec")


def rpc_request(port: int, method: str, params: list[Any]) -> Any:
    body = json.dumps({"id": 1, "jsonrpc": "2.0", "method": method, "params": params}).encode()
    request = urllib.request.Request(
        f"http://127.0.0.1:{port}",
        data=body,
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(request, timeout=2) as response:
        value = json.loads(response.read())
    require(value.get("error") is None, f"temporary node RPC error for {method}")
    return value.get("result")


def inspect_genesis(node: Path, raw_spec: Path, rpc_port: int, p2p_port: int) -> dict[str, Any]:
    log_file = tempfile.NamedTemporaryFile(prefix="nexus-v2-alpha-genesis.", suffix=".log", delete=False)
    log_path = Path(log_file.name)
    process: subprocess.Popen[bytes] | None = None
    try:
        process = subprocess.Popen(
            [
                str(node),
                "--chain",
                str(raw_spec),
                "--tmp",
                "--rpc-port",
                str(rpc_port),
                "--port",
                str(p2p_port),
                "--rpc-methods",
                "Safe",
                "--no-telemetry",
                "--no-prometheus",
                "--no-mdns",
            ],
            stdout=log_file,
            stderr=subprocess.STDOUT,
        )
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            require(process.poll() is None, f"temporary Alpha genesis node exited; log: {log_path}")
            try:
                version = rpc_request(rpc_port, "state_getRuntimeVersion", [])
                genesis_hash = rpc_request(rpc_port, "chain_getBlockHash", [0])
                chain_name = rpc_request(rpc_port, "system_chain", [])
                break
            except (CandidateError, OSError, urllib.error.URLError, json.JSONDecodeError):
                time.sleep(0.25)
        else:
            raise CandidateError(f"temporary Alpha genesis RPC did not become ready; log: {log_path}")
    finally:
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=10)
        log_file.close()
    require(isinstance(version, dict) and version.get("specVersion") == TARGET_SPEC_VERSION, "temporary Alpha node spec version mismatch")
    require(isinstance(genesis_hash, str) and bool(HASH256_RE.fullmatch(genesis_hash)), "invalid Alpha genesis hash")
    require(isinstance(chain_name, str) and chain_name, "temporary Alpha chain name is missing")
    try:
        log_path.unlink()
    except OSError:
        pass
    return {"genesisHash": genesis_hash, "chainName": chain_name, "specVersion": version["specVersion"]}


def raw_runtime_code(raw_spec: Mapping[str, Any]) -> bytes:
    top = raw_spec.get("genesis", {}).get("raw", {}).get("top", {})
    require(isinstance(top, dict), "Alpha raw spec genesis top is missing")
    encoded = top.get(CODE_STORAGE_KEY)
    require(isinstance(encoded, str) and encoded.startswith("0x"), "Alpha raw spec :code is missing")
    try:
        return bytes.fromhex(encoded[2:])
    except ValueError as exc:
        raise CandidateError("Alpha raw spec :code is invalid hex") from exc


def current_clean_commit(expected: str) -> None:
    actual = subprocess.check_output(["git", "-C", str(REPO_ROOT), "rev-parse", "HEAD"], text=True).strip()
    require(actual == expected, f"deployment source commit mismatch: expected {expected}, found {actual}")
    status = subprocess.check_output(
        ["git", "-C", str(REPO_ROOT), "status", "--porcelain", "--untracked-files=all"],
        text=True,
    )
    require(not status, "node candidate requires a clean isolated deployment worktree")


def command_build(args: argparse.Namespace) -> None:
    release_id = ensure_release(args.release_id)
    deployment_commit = ensure_commit(args.deployment_source_commit, "deployment source commit")
    current_clean_commit(deployment_commit)
    runtime = verify_runtime_bundle(Path(args.runtime_bundle))
    overrides_path = regular_file(Path(args.public_overrides), "Alpha public overrides")
    overrides = validate_public_overrides(overrides_path)
    rpc_port = ensure_port(args.rpc_port, "RPC port")
    p2p_port = ensure_port(args.p2p_port, "P2P port")
    require(rpc_port != p2p_port, "temporary node ports must differ")
    output = Path(args.output).resolve()
    require(not output.exists(), f"refusing to overwrite node candidate: {output}")
    target_identity_output = Path(args.target_identity_output).resolve()
    require(not target_identity_output.exists(), f"refusing to overwrite target identity: {target_identity_output}")
    require(not path_within(target_identity_output, output), "target identity must be emitted beside, not inside, the closed candidate directory")
    output.parent.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(prefix="nexus-v2-node-candidate.") as temporary:
        temporary_root = Path(temporary)
        first = temporary_root / "first"
        second = temporary_root / "second"
        node = Path(runtime["root"]) / "solochain-eterra-node"
        finalize_specs(node, overrides_path, first)
        finalize_specs(node, overrides_path, second)
        for name in ("alpha-plain.json", "alpha-raw.json"):
            require((first / name).read_bytes() == (second / name).read_bytes(), f"Alpha {name} finalization is not deterministic")

        raw = read_json(first / "alpha-raw.json", "Alpha raw spec")
        plain = read_json(first / "alpha-plain.json", "Alpha plain spec")
        code = raw_runtime_code(raw)
        production_wasm = Path(runtime["root"]) / "runtime-spec-106.compact.compressed.wasm"
        require(hashlib.sha256(code).hexdigest() == runtime["wasmSha256"], "Alpha raw spec :code differs from the pinned production Wasm")
        code_hash = "0x" + hashlib.blake2b(code, digest_size=32).hexdigest()
        genesis = inspect_genesis(node, first / "alpha-raw.json", rpc_port, p2p_port)

        output.mkdir(mode=0o700)
        shutil.copy2(node, output / "solochain-eterra-node")
        os.chmod(output / "solochain-eterra-node", 0o755)
        shutil.copyfile(first / "alpha-plain.json", output / "alpha-plain.json")
        shutil.copyfile(first / "alpha-raw.json", output / "alpha-raw.json")
        write_new_json(output / "alpha-public-overrides.json", overrides, mode=0o600)
        shutil.copyfile(
            REPO_ROOT / "deploy/alpha/macmini2010/eterra-alpha-node.service",
            output / "eterra-alpha-node.service",
        )
        shutil.copyfile(
            REPO_ROOT / "deploy/alpha/macmini2010/start-alpha-node.sh",
            output / "start-alpha-node.sh",
        )
        os.chmod(output / "start-alpha-node.sh", 0o755)

    artifact_hashes = {name: sha256_file(output / name) for name in sorted(CANDIDATE_FILES)}
    source_epoch = runtime["manifest"].get("sourceDateEpoch")
    require(isinstance(source_epoch, int) and source_epoch > 0, "runtime bundle source date epoch is invalid")
    created_at = dt.datetime.fromtimestamp(source_epoch, tz=dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    manifest = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-node-candidate",
        "releaseId": release_id,
        "deploymentSourceCommit": deployment_commit,
        "runtimeSourceCommit": runtime["sourceCommit"],
        "targetSpecVersion": TARGET_SPEC_VERSION,
        "sourceDateEpoch": source_epoch,
        "createdAtUtc": created_at,
        "runtimeBundle": {
            "manifestSha256": runtime["manifestSha256"],
            "sha256SumsSha256": runtime["sumsSha256"],
            "productionWasmSha256": runtime["wasmSha256"],
            "metadataScaleSha256": runtime["metadataSha256"],
            "metadataVersion": runtime["metadataVersion"],
        },
        "alpha": {
            "id": plain.get("id"),
            "chainType": plain.get("chainType"),
            "chainName": genesis["chainName"],
            "genesisHash": genesis["genesisHash"],
            "runtimeCodeHash": code_hash,
            "runtimeCodeSha256": runtime["wasmSha256"],
            "deterministicRepeatMatched": True,
            "privateOverridesAreAddressOnly": True,
        },
        "artifacts": artifact_hashes,
        "builder": {
            "nodeCandidateToolSha256": sha256_file(Path(__file__).resolve()),
            "finalizeAlphaToolSha256": sha256_file(FINALIZE_ALPHA),
            "verifyAlphaToolSha256": sha256_file(VERIFY_ALPHA),
        },
        "containsSecrets": False,
        "remoteBuildAllowed": False,
        "publicDeployAllowed": False,
        "paidProductionAllowed": False,
    }
    write_new_json(output / "node-candidate.json", manifest, mode=0o600)
    summary = verify_candidate(output / "node-candidate.json")
    target_identity = {
        "schemaVersion": 1,
        "kind": "eterra-spec106-target-identity.v1",
        "releaseId": release_id,
        "network": "private-alpha",
        "genesisHash": summary["genesisHash"],
        "runtimeCodeHash": summary["runtimeCodeHash"],
        "runtimeSourceCommit": runtime["sourceCommit"],
        "deploymentSourceCommit": deployment_commit,
        "runtimeMetadata": {
            "scaleSha256": runtime["metadataSha256"],
            "version": runtime["metadataVersion"],
        },
        "specVersion": TARGET_SPEC_VERSION,
        "tcgStorageVersion": 16,
        "nodeCandidateManifestSha256": summary["manifestSha256"],
        "authorizations": {
            "privateAlphaOnly": True,
            "publicProduction": False,
            "paidProduction": False,
        },
    }
    target_identity_output.parent.mkdir(parents=True, exist_ok=True)
    write_new_json(target_identity_output, target_identity, mode=0o600)
    target_summary = verify_target_identity(target_identity_output, output / "node-candidate.json")
    summary["targetIdentitySha256"] = target_summary["sha256"]
    summary["targetIdentityPath"] = str(target_identity_output)
    print(json.dumps(summary, sort_keys=True))


def verify_candidate(manifest_path: Path) -> dict[str, Any]:
    manifest_path = regular_file(manifest_path, "node candidate manifest")
    root = manifest_path.parent
    require(manifest_path.name == "node-candidate.json", "node candidate manifest must be named node-candidate.json")
    present = {path.name for path in root.iterdir()}
    require(present == CANDIDATE_FILES | {"node-candidate.json"}, "node candidate directory does not match the closed file set")
    manifest = read_json(manifest_path, "node candidate manifest")
    require(manifest.get("schemaVersion") == 1, "unsupported node candidate schema")
    require(manifest.get("kind") == "nexus-v2-private-alpha-node-candidate", "node candidate kind mismatch")
    release_id = ensure_release(manifest.get("releaseId"))
    deployment_commit = ensure_commit(manifest.get("deploymentSourceCommit"), "deployment source commit")
    runtime_commit = ensure_commit(manifest.get("runtimeSourceCommit"), "runtime source commit")
    require(manifest.get("targetSpecVersion") == TARGET_SPEC_VERSION, "node candidate spec version mismatch")
    require(manifest.get("containsSecrets") is False, "node candidate claims to contain secrets")
    require(manifest.get("remoteBuildAllowed") is False, "node candidate permits remote build")
    require(manifest.get("publicDeployAllowed") is False, "node candidate permits public deploy")
    require(manifest.get("paidProductionAllowed") is False, "node candidate permits paid production")
    artifacts = manifest.get("artifacts")
    require(isinstance(artifacts, dict) and set(artifacts) == CANDIDATE_FILES, "node candidate artifacts do not match the closed set")
    for name, expected in artifacts.items():
        ensure_sha(expected, f"candidate artifact SHA-256 for {name}")
        path = regular_file(
            root / name,
            f"candidate artifact {name}",
            executable=name in {"solochain-eterra-node", "start-alpha-node.sh"},
        )
        require(path_within(path, root), f"candidate artifact escapes root: {name}")
        require(sha256_file(path) == expected, f"candidate artifact hash mismatch: {name}")
    validate_public_overrides(root / "alpha-public-overrides.json")
    service_text = (root / "eterra-alpha-node.service").read_text(encoding="utf-8")
    require("[Service]" in service_text and "ExecStart=" in service_text, "candidate node service unit is invalid")
    raw = read_json(root / "alpha-raw.json", "candidate Alpha raw spec")
    plain = read_json(root / "alpha-plain.json", "candidate Alpha plain spec")
    alpha = manifest.get("alpha")
    require(isinstance(alpha, dict), "node candidate Alpha identity is missing")
    require(alpha.get("id") == plain.get("id") == "eterra_alpha", "candidate Alpha spec ID mismatch")
    require(alpha.get("chainType") == plain.get("chainType") == "Live", "candidate Alpha chain type mismatch")
    require(alpha.get("deterministicRepeatMatched") is True, "candidate spec was not deterministically repeated")
    require(alpha.get("privateOverridesAreAddressOnly") is True, "candidate overrides were not address-only")
    require(isinstance(alpha.get("chainName"), str) and alpha["chainName"], "candidate chain name is missing")
    require(isinstance(alpha.get("genesisHash"), str) and bool(HASH256_RE.fullmatch(alpha["genesisHash"])), "candidate genesis hash is invalid")
    require(isinstance(alpha.get("runtimeCodeHash"), str) and bool(HASH256_RE.fullmatch(alpha["runtimeCodeHash"])), "candidate runtime code hash is invalid")
    runtime_code = raw_runtime_code(raw)
    runtime_sha = hashlib.sha256(runtime_code).hexdigest()
    require(runtime_sha == ensure_sha(alpha.get("runtimeCodeSha256"), "candidate runtime code SHA-256"), "candidate raw runtime code SHA-256 mismatch")
    require("0x" + hashlib.blake2b(runtime_code, digest_size=32).hexdigest() == alpha["runtimeCodeHash"], "candidate raw runtime code hash mismatch")
    runtime_bundle = manifest.get("runtimeBundle")
    require(isinstance(runtime_bundle, dict), "candidate runtime bundle pins are missing")
    ensure_sha(runtime_bundle.get("manifestSha256"), "runtime bundle manifest SHA-256")
    ensure_sha(runtime_bundle.get("sha256SumsSha256"), "runtime bundle checksum file SHA-256")
    require(runtime_bundle.get("productionWasmSha256") == runtime_sha, "candidate runtime code differs from bundle pin")
    ensure_sha(runtime_bundle.get("metadataScaleSha256"), "runtime bundle metadata SHA-256")
    require(runtime_bundle.get("metadataVersion") == 15, "candidate runtime metadata version mismatch")
    return {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-node-candidate-verification",
        "releaseId": release_id,
        "deploymentSourceCommit": deployment_commit,
        "runtimeSourceCommit": runtime_commit,
        "targetSpecVersion": TARGET_SPEC_VERSION,
        "genesisHash": alpha["genesisHash"],
        "runtimeCodeHash": alpha["runtimeCodeHash"],
        "nativeNodeSha256": artifacts["solochain-eterra-node"],
        "plainSpecSha256": artifacts["alpha-plain.json"],
        "rawSpecSha256": artifacts["alpha-raw.json"],
        "serviceUnitSha256": artifacts["eterra-alpha-node.service"],
        "startScriptSha256": artifacts["start-alpha-node.sh"],
        "manifestSha256": sha256_file(manifest_path),
        "containsSecrets": False,
    }


def command_verify(args: argparse.Namespace) -> None:
    summary = verify_candidate(Path(args.candidate_manifest))
    if args.expected_manifest_sha256:
        require(summary["manifestSha256"] == ensure_sha(args.expected_manifest_sha256, "expected candidate manifest SHA-256"), "node candidate manifest hash mismatch")
    if args.expected_release_id:
        require(summary["releaseId"] == ensure_release(args.expected_release_id), "node candidate release mismatch")
    if args.expected_deployment_source_commit:
        require(summary["deploymentSourceCommit"] == ensure_commit(args.expected_deployment_source_commit, "expected deployment source commit"), "node candidate deployment source mismatch")
    if args.expected_runtime_source_commit:
        require(summary["runtimeSourceCommit"] == ensure_commit(args.expected_runtime_source_commit, "expected runtime source commit"), "node candidate runtime source mismatch")
    if args.expected_genesis_hash:
        require(summary["genesisHash"] == args.expected_genesis_hash.lower(), "node candidate genesis hash mismatch")
    if args.expected_runtime_code_hash:
        require(summary["runtimeCodeHash"] == args.expected_runtime_code_hash.lower(), "node candidate runtime code hash mismatch")
    print(json.dumps(summary, sort_keys=True))


def verify_target_identity(path: Path, candidate_manifest: Path | None = None) -> dict[str, Any]:
    path = regular_file(path, "spec-106 target identity")
    value = read_json(path, "spec-106 target identity")
    require(set(value) == TARGET_IDENTITY_KEYS, "target identity does not match the closed schema")
    require(value.get("schemaVersion") == 1, "target identity schema mismatch")
    require(value.get("kind") == "eterra-spec106-target-identity.v1", "target identity kind mismatch")
    require(value.get("network") == "private-alpha", "target identity is not private-alpha-only")
    ensure_release(value.get("releaseId"))
    ensure_commit(value.get("runtimeSourceCommit"), "target runtime source commit")
    ensure_commit(value.get("deploymentSourceCommit"), "target deployment source commit")
    require(isinstance(value.get("genesisHash"), str) and bool(HASH256_RE.fullmatch(value["genesisHash"])), "target identity genesis hash is invalid")
    require(isinstance(value.get("runtimeCodeHash"), str) and bool(HASH256_RE.fullmatch(value["runtimeCodeHash"])), "target identity runtime code hash is invalid")
    ensure_sha(value.get("nodeCandidateManifestSha256"), "target candidate manifest SHA-256")
    require(value.get("specVersion") == TARGET_SPEC_VERSION, "target identity spec version mismatch")
    require(value.get("tcgStorageVersion") == 16, "target identity TCG storage version mismatch")
    metadata = value.get("runtimeMetadata")
    require(isinstance(metadata, dict) and set(metadata) == {"scaleSha256", "version"}, "target runtime metadata identity mismatch")
    ensure_sha(metadata.get("scaleSha256"), "target runtime metadata SHA-256")
    require(metadata.get("version") == 15, "target runtime metadata must be V15")
    require(
        value.get("authorizations")
        == {
            "privateAlphaOnly": True,
            "publicProduction": False,
            "paidProduction": False,
        },
        "target identity activation flags are unsafe",
    )
    if candidate_manifest is not None:
        candidate = verify_candidate(candidate_manifest)
        candidate_value = read_json(Path(candidate_manifest), "node candidate manifest")
        require(value["releaseId"] == candidate["releaseId"], "target identity release mismatch")
        require(value["deploymentSourceCommit"] == candidate["deploymentSourceCommit"], "target deployment source mismatch")
        require(value["runtimeSourceCommit"] == candidate["runtimeSourceCommit"], "target runtime source mismatch")
        require(value["genesisHash"] == candidate["genesisHash"], "target genesis mismatch")
        require(value["runtimeCodeHash"] == candidate["runtimeCodeHash"], "target runtime code hash mismatch")
        require(value["nodeCandidateManifestSha256"] == candidate["manifestSha256"], "target candidate manifest mismatch")
        require(metadata["scaleSha256"] == candidate_value["runtimeBundle"]["metadataScaleSha256"], "target runtime metadata hash mismatch")
        require(metadata["version"] == candidate_value["runtimeBundle"]["metadataVersion"], "target runtime metadata version mismatch")
    return {
        "schemaVersion": 1,
        "kind": "eterra-spec106-target-identity-verification.v1",
        "releaseId": value["releaseId"],
        "genesisHash": value["genesisHash"],
        "runtimeCodeHash": value["runtimeCodeHash"],
        "runtimeSourceCommit": value["runtimeSourceCommit"],
        "deploymentSourceCommit": value["deploymentSourceCommit"],
        "metadataScaleSha256": metadata["scaleSha256"],
        "metadataVersion": metadata["version"],
        "nodeCandidateManifestSha256": value["nodeCandidateManifestSha256"],
        "sha256": sha256_file(path),
        "publicProduction": False,
        "paidProduction": False,
    }


def command_verify_target_identity(args: argparse.Namespace) -> None:
    candidate = Path(args.candidate_manifest) if args.candidate_manifest else None
    summary = verify_target_identity(Path(args.target_identity), candidate)
    if args.expected_sha256:
        require(summary["sha256"] == ensure_sha(args.expected_sha256, "expected target identity SHA-256"), "target identity SHA-256 mismatch")
    print(json.dumps(summary, sort_keys=True))


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Build or verify an immutable private-alpha node candidate")
    subparsers = parser.add_subparsers(dest="command", required=True)
    build = subparsers.add_parser("build")
    build.add_argument("--runtime-bundle", required=True)
    build.add_argument("--public-overrides", required=True)
    build.add_argument("--release-id", required=True)
    build.add_argument("--deployment-source-commit", required=True)
    build.add_argument("--output", required=True)
    build.add_argument("--target-identity-output", required=True)
    build.add_argument("--rpc-port", type=int, default=19946)
    build.add_argument("--p2p-port", type=int, default=31346)
    build.set_defaults(handler=command_build)
    verify = subparsers.add_parser("verify")
    verify.add_argument("--candidate-manifest", required=True)
    verify.add_argument("--expected-manifest-sha256")
    verify.add_argument("--expected-release-id")
    verify.add_argument("--expected-deployment-source-commit")
    verify.add_argument("--expected-runtime-source-commit")
    verify.add_argument("--expected-genesis-hash")
    verify.add_argument("--expected-runtime-code-hash")
    verify.set_defaults(handler=command_verify)
    target = subparsers.add_parser("verify-target-identity")
    target.add_argument("--target-identity", required=True)
    target.add_argument("--candidate-manifest")
    target.add_argument("--expected-sha256")
    target.set_defaults(handler=command_verify_target_identity)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        args.handler(args)
    except (CandidateError, subprocess.CalledProcessError, OSError) as exc:
        print(f"nexus-v2-node-candidate: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
