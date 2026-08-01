#!/usr/bin/env python3
"""Assemble, verify, and receipt immutable Nexus V2 authority releases.

The release candidate contains only pre-published API and Operator trees,
their canonical SDK release manifest, and a public signer identity.  It never
reads or stores a mnemonic, access key, derivation password, or other secret.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import hmac
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import unicodedata
from pathlib import Path
from typing import Any, Mapping, Sequence

import deployment_secret_environment  # noqa: F401


SCHEMA_VERSION = 1
CANDIDATE_KIND = "nexus-v2-private-alpha-authority-candidate"
RECEIPT_KIND = "nexus-v2-private-alpha-authority-deployment-receipt"
OBSERVATION_KIND = "nexus-v2-private-alpha-authority-deployment-observation"
SDK_RELEASE_SCHEMA = "eterra.authority-release-manifest.v1"
SDK_RELEASE_MANIFEST_SHA256 = "f66cb5353df920468627206c41df7f8666b8fbee5f493f17041c1c0a9b75f033"
LIVENESS_SCHEMA = "eterra.authority-liveness-challenge.v1"
LIVENESS_DOMAIN = b"eterra.authority-liveness-challenge.v1\0"
CATALOG_SHA256 = "f2846a4ce742f881cce87edd373061d42b720d10a6c324e782c5487060ae7964"
CATALOG_PATH = "api/catalog/eterra-legends.encounters.private-alpha.v1.json"
RELEASE_MANIFEST_NAME = "authority-release-manifest.json"
PUBLIC_SIGNER_NAME = "authority-signer.public.json"
CANDIDATE_NAME = "authority-candidate.json"
TARGET_RUNTIME_SPEC_VERSION = 106
TARGET_RUNTIME_IDENTIFIER = "linux-x64"
DEPLOYMENT_ROOT = "/opt/eterra-alpha"
DEPLOYMENT_USER = "eterra2010"
SERVICE_NAME = "eterra-arcade-authority"
SERVICE_PORT = 8787

SHA_RE = re.compile(r"^(?!0{64}$)[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
HASH_RE = re.compile(r"^0x(?!0{64}$)[0-9a-f]{64}$")
PUBLIC_KEY_RE = re.compile(r"^0x(?!0{64}$)[0-9a-f]{64}$")
SIGNATURE_RE = re.compile(r"^0x(?!0{128}$)[0-9a-f]{128}$")
ID_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
ADAPTER_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,191}$")
UTC_RE = re.compile(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$")
MODE_RE = re.compile(r"^0[0-7]{3}$")
FORBIDDEN_SECRET_FRAGMENTS = {
    "mnemonic",
    "seedphrase",
    "recoveryphrase",
    "privatekey",
    "accesskey",
    "apikey",
    "signingkey",
    "derivationpassword",
    "credential",
    "secret",
}
FORBIDDEN_SECRET_EXTENSIONS = {".env", ".key", ".pem", ".pfx", ".p12", ".snk"}
ALLOWED_SECRET_NAMED_RUNTIME_FILES = {
    "Microsoft.Extensions.Configuration.UserSecrets.dll",
    "Microsoft.Extensions.Configuration.UserSecrets.pdb",
    "Microsoft.Extensions.Configuration.UserSecrets.ni.pdb",
    "Microsoft.Extensions.Configuration.UserSecrets.ni.r2rmap",
}
BASE58_ALPHABET = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
SS58_PREFIX = b"SS58PRE"
SS58_NETWORK_PREFIX = 42

CANDIDATE_TOP_LEVEL = {"api", "operator", RELEASE_MANIFEST_NAME, PUBLIC_SIGNER_NAME, CANDIDATE_NAME}
CANDIDATE_KEYS = {
    "schemaVersion",
    "kind",
    "releaseId",
    "createdAtUtc",
    "sources",
    "target",
    "deployment",
    "artifacts",
    "services",
    "safety",
}
SOURCE_KEYS = {"chain", "sdkgen"}
SOURCE_PIN_KEYS = {"commit", "tree"}
TARGET_KEYS = {
    "genesisHash",
    "runtimeSpecVersion",
    "runtimeCodeHash",
    "runtimeCodeSha256",
    "runtimeMetadataScaleSha256",
    "readModelAdapterVersion",
    "authorityEpoch",
}
DEPLOYMENT_KEYS = {
    "serviceUnitSha256",
    "runtimeIdentifier",
    "selfContained",
    "submitterMode",
    "journalMode",
    "releasePromotion",
}
ARTIFACT_KEYS = {"apiTree", "operatorTree", "releaseManifest", "publicSigner", "catalog"}
TREE_PIN_KEYS = {"path", "fileCount", "totalBytes"}
FILE_PIN_KEYS = {"path", "sha256"}
SIGNER_PIN_KEYS = FILE_PIN_KEYS | {"scheme", "publicKey", "ss58Address"}
SERVICE_KEYS = {"legendsAuthority"}
LEGENDS_SERVICE_KEYS = {"serviceId", "releaseSha256", "authorityConfigSha256"}
SAFETY = {
    "privateAlphaOnly": True,
    "publicProduction": False,
    "paidEntry": False,
    "paidPacks": False,
    "wagering": False,
    "marketplace": False,
    "permanentAssetLoss": False,
    "authorityAuthorizationIncluded": False,
    "authoritySeedingIncluded": False,
    "chainWritesDuringPhase1": False,
    "fpsReleaseIncluded": False,
}

OBSERVATION_KEYS = {
    "schemaVersion",
    "kind",
    "releaseId",
    "candidateSha256",
    "releaseManifestSha256",
    "chainSourceCommit",
    "sdkgenSourceCommit",
    "deploymentRoot",
    "serviceUnit",
    "environment",
    "secrets",
    "process",
    "catalog",
    "manifestVerification",
    "journal",
    "liveness",
    "observedAtUtc",
}
REMOTE_FILE_KEYS = {"path", "sha256", "mode", "owner"}
SECRET_OBSERVATION_KEYS = {
    "signerMnemonic",
    "privateAlphaAccessKey",
    "signerDerivationPassword",
}
PROCESS_KEYS = {
    "serviceActive",
    "mainPid",
    "user",
    "executablePath",
    "procExecutableSha256",
    "listenerHost",
    "listenerPort",
    "environmentMatched",
}
MANIFEST_VERIFICATION_KEYS = {
    "operatorCliPath",
    "operatorCliSha256",
    "stdoutSha256",
    "ok",
}
JOURNAL_KEYS = {"path", "mode", "owner", "nonSymlinkDirectory"}
LIVENESS_OBSERVATION_KEYS = {"httpStatus", "requestNonceHex", "response"}
LIVENESS_RESPONSE_KEYS = {
    "schema",
    "ok",
    "algorithm",
    "nonceHex",
    "payloadHashHex",
    "publicKeyHex",
    "signatureHex",
    "error",
}


class CandidateError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise CandidateError(message)


def duplicate_rejecting_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        require(key not in value, f"duplicate JSON property: {key}")
        value[key] = item
    return value


def canonical_bytes(value: Mapping[str, Any]) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")


def sdk_manifest_bytes(value: Mapping[str, Any]) -> bytes:
    ordered = {
        "schema": value["schema"],
        "files": [
            {
                "path": item["path"],
                "sha256": item["sha256"],
                "size": item["size"],
                "executable": item["executable"],
            }
            for item in value["files"]
        ],
    }
    return json.dumps(ordered, ensure_ascii=False, separators=(",", ":")).encode("utf-8")


def canonical_input_path(path: Path, label: str, *, directory: bool = False) -> Path:
    require(path.is_absolute() and ".." not in path.parts, f"{label} path must be canonical and absolute")
    try:
        observed = path.lstat()
        resolved = path.resolve(strict=True)
    except OSError as exc:
        raise CandidateError(f"{label} is unavailable") from exc
    expected_kind = stat.S_ISDIR if directory else stat.S_ISREG
    require(not stat.S_ISLNK(observed.st_mode), f"{label} may not be a symlink")
    require(expected_kind(observed.st_mode), f"{label} has the wrong file type")
    require(path == resolved, f"{label} path traverses a symlink")
    return path


def read_json(path: Path, label: str, *, canonical: bool = False) -> dict[str, Any]:
    path = canonical_input_path(path, label)
    try:
        value = json.loads(
            path.read_text(encoding="utf-8"), object_pairs_hook=duplicate_rejecting_object
        )
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise CandidateError(f"invalid {label}") from exc
    require(isinstance(value, dict), f"{label} must be a JSON object")
    if canonical:
        require(path.read_bytes() == canonical_bytes(value), f"{label} is not canonical JSON")
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


def ensure_sha(value: Any, label: str) -> str:
    require(isinstance(value, str) and SHA_RE.fullmatch(value) is not None, f"invalid {label}")
    return value


def ensure_commit(value: Any, label: str) -> str:
    require(isinstance(value, str) and COMMIT_RE.fullmatch(value) is not None, f"invalid {label}")
    return value


def ensure_hash(value: Any, label: str) -> str:
    require(isinstance(value, str) and HASH_RE.fullmatch(value) is not None, f"invalid {label}")
    return value


def parse_utc(value: Any, label: str) -> dt.datetime:
    require(isinstance(value, str) and UTC_RE.fullmatch(value) is not None, f"invalid {label}")
    return dt.datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=dt.timezone.utc)


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).replace(microsecond=0).strftime("%Y-%m-%dT%H:%M:%SZ")


def write_new(path: Path, payload: bytes, mode: int = 0o440) -> None:
    require(not os.path.lexists(path), f"refusing to overwrite output: {path}")
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, mode)
    with os.fdopen(descriptor, "wb") as handle:
        handle.write(payload)
        handle.flush()
        os.fsync(handle.fileno())


def git_output(root: Path, *args: str) -> str:
    completed = subprocess.run(
        ["git", "-C", str(root), *args], capture_output=True, text=True, check=False
    )
    require(completed.returncode == 0, f"cannot inspect source repository: {root}")
    return completed.stdout.strip()


def source_pin(root: Path, expected_commit: str, label: str) -> dict[str, str]:
    root = canonical_input_path(root, f"{label} root", directory=True)
    require(
        Path(git_output(root, "rev-parse", "--show-toplevel")).resolve() == root,
        f"{label} root must be the Git worktree root",
    )
    require(
        git_output(root, "status", "--porcelain", "--untracked-files=all") == "",
        f"{label} worktree must be clean",
    )
    commit = ensure_commit(git_output(root, "rev-parse", "HEAD"), f"{label} commit")
    require(commit == ensure_commit(expected_commit, f"expected {label} commit"), f"{label} commit mismatch")
    return {
        "commit": commit,
        "tree": ensure_commit(git_output(root, "rev-parse", "HEAD^{tree}"), f"{label} tree"),
    }


def canonical_manifest_path(value: Any) -> str:
    require(isinstance(value, str) and 1 < len(value) <= 512, "release-manifest path is invalid")
    require(value == unicodedata.normalize("NFC", value), "release-manifest path is not NFC")
    require(not value.startswith("/") and not value.endswith("/"), "release-manifest path is absolute or empty")
    require("\\" not in value and ":" not in value, "release-manifest path is not portable")
    segments = value.split("/")
    require(segments[0] in {"api", "operator"} and len(segments) >= 2, "release-manifest root is invalid")
    require(all(item not in {"", ".", ".."} for item in segments), "release-manifest path traverses")
    require(not any(any(ord(character) < 32 for character in item) for item in segments), "release-manifest path contains controls")
    for segment in segments:
        lower = segment.lower()
        suffix = Path(lower).suffix
        compact = "".join(character for character in lower if character.isalnum())
        forbidden_fragment = any(
            fragment in compact
            and not (fragment == "secret" and segment in ALLOWED_SECRET_NAMED_RUNTIME_FILES)
            for fragment in FORBIDDEN_SECRET_FRAGMENTS
        )
        require(
            lower not in {".env", "id_rsa", "id_ed25519"}
            and not lower.startswith(".env.")
            and suffix not in FORBIDDEN_SECRET_EXTENSIONS
            and not forbidden_fragment,
            "release-manifest secret-shaped path is forbidden",
        )
    return value


def scan_publish_trees(api_root: Path, operator_root: Path) -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = []
    for logical, root in (("api", api_root), ("operator", operator_root)):
        require(root.is_absolute() and root.is_dir() and not root.is_symlink(), f"{logical} publish root is invalid")
        initial: list[tuple[str, int, int, int]] = []
        for path in sorted(root.rglob("*"), key=lambda item: item.relative_to(root).as_posix()):
            relative = path.relative_to(root).as_posix()
            info = path.lstat()
            require(not stat.S_ISLNK(info.st_mode), f"publish tree symlink is forbidden: {logical}/{relative}")
            if stat.S_ISDIR(info.st_mode):
                continue
            require(stat.S_ISREG(info.st_mode), f"publish tree entry is not regular: {logical}/{relative}")
            manifest_path = canonical_manifest_path(f"{logical}/{relative}")
            before = (manifest_path, info.st_size, info.st_mtime_ns, stat.S_IMODE(info.st_mode))
            digest = sha256_file(path)
            after_info = path.lstat()
            after = (manifest_path, after_info.st_size, after_info.st_mtime_ns, stat.S_IMODE(after_info.st_mode))
            require(before == after and stat.S_ISREG(after_info.st_mode), f"publish file changed while hashing: {manifest_path}")
            initial.append(before)
            records.append(
                {
                    "path": manifest_path,
                    "sha256": digest,
                    "size": after_info.st_size,
                    "executable": os.access(path, os.X_OK),
                }
            )
        require(initial, f"{logical} publish root is empty")
        final: list[tuple[str, int, int, int]] = []
        for path in sorted(root.rglob("*"), key=lambda item: item.relative_to(root).as_posix()):
            info = path.lstat()
            require(not stat.S_ISLNK(info.st_mode), f"publish tree symlink is forbidden: {path}")
            if stat.S_ISDIR(info.st_mode):
                continue
            require(stat.S_ISREG(info.st_mode), f"publish tree entry is not regular: {path}")
            final.append(
                (
                    f"{logical}/{path.relative_to(root).as_posix()}",
                    info.st_size,
                    info.st_mtime_ns,
                    stat.S_IMODE(info.st_mode),
                )
            )
        require(final == initial, f"{logical} publish tree changed while validating")
    records.sort(key=lambda item: item["path"])
    portable = [str(item["path"]).casefold() for item in records]
    require(len(portable) == len(set(portable)), "release-manifest paths collide by case")
    return records


def validate_release_manifest(path: Path, api_root: Path, operator_root: Path) -> dict[str, Any]:
    value = read_json(path, "authority release manifest")
    exact_keys(value, {"schema", "files"}, "authority release manifest")
    require(value.get("schema") == SDK_RELEASE_SCHEMA, "authority release manifest schema mismatch")
    files = value.get("files")
    require(isinstance(files, list) and 2 <= len(files) <= 10000, "authority release manifest file list is invalid")
    previous = ""
    for index, item in enumerate(files):
        item = exact_keys(item, {"path", "sha256", "size", "executable"}, f"release file {index}")
        candidate_path = canonical_manifest_path(item.get("path"))
        require(previous < candidate_path if previous else True, "authority release manifest paths are not strictly sorted")
        previous = candidate_path
        ensure_sha(item.get("sha256"), f"release file {index} SHA-256")
        require(type(item.get("size")) is int and item["size"] >= 0, f"release file {index} size is invalid")
        require(type(item.get("executable")) is bool, f"release file {index} executable flag is invalid")
    require(path.read_bytes() == sdk_manifest_bytes(value), "authority release manifest is not canonical SDK JSON")
    actual = scan_publish_trees(api_root, operator_root)
    require(files == actual, "authority release manifest does not match the complete publish trees")
    require(
        sha256_file(path) == SDK_RELEASE_MANIFEST_SHA256,
        "authority release manifest is not the exact reviewed SDKGen release",
    )
    return value


def decode_base58(value: str) -> bytes:
    require(value != "", "public signer SS58 address is empty")
    number = 0
    for character in value:
        index = BASE58_ALPHABET.find(character)
        require(index >= 0, "public signer SS58 address is not base58")
        number = number * 58 + index
    encoded = number.to_bytes((number.bit_length() + 7) // 8, "big") if number else b""
    return (b"\0" * (len(value) - len(value.lstrip("1")))) + encoded


def validate_public_signer(path: Path) -> dict[str, str]:
    value = read_json(path, "public signer artifact")
    exact_keys(value, {"publicKey", "scheme", "ss58Address"}, "public signer artifact")
    require(path.read_bytes() == (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode(), "public signer artifact is not canonical compact JSON")
    require(value.get("scheme") == "sr25519", "public signer scheme must be sr25519")
    public_key = value.get("publicKey")
    require(isinstance(public_key, str) and PUBLIC_KEY_RE.fullmatch(public_key), "public signer key is invalid")
    address = value.get("ss58Address")
    require(isinstance(address, str) and 30 <= len(address) <= 64 and not any(ch.isspace() for ch in address), "public signer SS58 address is invalid")
    decoded = decode_base58(address)
    public_key_bytes = bytes.fromhex(public_key[2:])
    require(
        len(decoded) == 35
        and decoded[0] == SS58_NETWORK_PREFIX
        and decoded[1:33] == public_key_bytes,
        "public signer SS58 address does not encode the pinned public key on network 42",
    )
    checksum = hashlib.blake2b(SS58_PREFIX + decoded[:33], digest_size=64).digest()[:2]
    require(
        hmac.compare_digest(decoded[33:], checksum),
        "public signer SS58 checksum is invalid",
    )
    return {"scheme": "sr25519", "publicKey": public_key, "ss58Address": address}


def tree_summary(records: list[dict[str, Any]], prefix: str) -> dict[str, Any]:
    selected = [item for item in records if str(item["path"]).startswith(prefix + "/")]
    return {"path": prefix, "fileCount": len(selected), "totalBytes": sum(int(item["size"]) for item in selected)}


def validate_candidate(
    candidate_path: Path,
    expected_sha256: str | None = None,
    *,
    expected_release_id: str | None = None,
    expected_chain_commit: str | None = None,
    expected_sdkgen_commit: str | None = None,
) -> dict[str, Any]:
    candidate_path = canonical_input_path(candidate_path, "authority candidate")
    require(candidate_path.name == CANDIDATE_NAME, f"candidate must be named {CANDIDATE_NAME}")
    candidate = read_json(candidate_path, "authority candidate", canonical=True)
    if expected_sha256 is not None:
        require(sha256_file(candidate_path) == ensure_sha(expected_sha256, "authority candidate SHA-256"), "authority candidate hash mismatch")
    exact_keys(candidate, CANDIDATE_KEYS, "authority candidate")
    require(candidate.get("schemaVersion") == SCHEMA_VERSION and candidate.get("kind") == CANDIDATE_KIND, "authority candidate identity mismatch")
    release_id = candidate.get("releaseId")
    require(isinstance(release_id, str) and ID_RE.fullmatch(release_id), "authority candidate release ID is invalid")
    if expected_release_id is not None:
        require(release_id == expected_release_id, "authority candidate release mismatch")
    parse_utc(candidate.get("createdAtUtc"), "authority candidate timestamp")

    root = candidate_path.parent
    require(root.is_dir() and not root.is_symlink(), "authority candidate root is invalid")
    require({item.name for item in root.iterdir()} == CANDIDATE_TOP_LEVEL, "authority candidate root closed file set mismatch")
    sources = exact_keys(candidate.get("sources"), SOURCE_KEYS, "authority candidate sources")
    for name in SOURCE_KEYS:
        pin = exact_keys(sources[name], SOURCE_PIN_KEYS, f"authority candidate {name} source")
        ensure_commit(pin.get("commit"), f"authority candidate {name} commit")
        ensure_commit(pin.get("tree"), f"authority candidate {name} tree")
    if expected_chain_commit is not None:
        require(sources["chain"]["commit"] == expected_chain_commit, "authority candidate chain source mismatch")
    if expected_sdkgen_commit is not None:
        require(sources["sdkgen"]["commit"] == expected_sdkgen_commit, "authority candidate SDKGen source mismatch")

    target = exact_keys(candidate.get("target"), TARGET_KEYS, "authority candidate target")
    ensure_hash(target.get("genesisHash"), "authority candidate genesis hash")
    require(target.get("runtimeSpecVersion") == TARGET_RUNTIME_SPEC_VERSION, "authority candidate runtime spec mismatch")
    ensure_hash(target.get("runtimeCodeHash"), "authority candidate runtime code hash")
    ensure_sha(target.get("runtimeCodeSha256"), "authority candidate runtime code SHA-256")
    ensure_sha(target.get("runtimeMetadataScaleSha256"), "authority candidate metadata SHA-256")
    adapter = target.get("readModelAdapterVersion")
    require(isinstance(adapter, str) and ADAPTER_RE.fullmatch(adapter), "authority candidate adapter version is invalid")
    require(type(target.get("authorityEpoch")) is int and target["authorityEpoch"] > 0, "authority candidate epoch is invalid")

    deployment = exact_keys(candidate.get("deployment"), DEPLOYMENT_KEYS, "authority candidate deployment")
    ensure_sha(deployment.get("serviceUnitSha256"), "authority service unit SHA-256")
    require(
        deployment
        == {
            "serviceUnitSha256": deployment["serviceUnitSha256"],
            "runtimeIdentifier": TARGET_RUNTIME_IDENTIFIER,
            "selfContained": True,
            "submitterMode": "in_memory",
            "journalMode": "0700",
            "releasePromotion": "immutable-create-once",
        },
        "authority deployment contract is unsafe or unsupported",
    )

    artifacts = exact_keys(candidate.get("artifacts"), ARTIFACT_KEYS, "authority candidate artifacts")
    for name in ("apiTree", "operatorTree"):
        exact_keys(artifacts[name], TREE_PIN_KEYS, f"authority candidate {name}")
    for name in ("releaseManifest", "catalog"):
        pin = exact_keys(artifacts[name], FILE_PIN_KEYS, f"authority candidate {name}")
        ensure_sha(pin.get("sha256"), f"authority candidate {name} SHA-256")
    signer_pin = exact_keys(artifacts["publicSigner"], SIGNER_PIN_KEYS, "authority candidate public signer")
    ensure_sha(signer_pin.get("sha256"), "authority candidate public signer SHA-256")

    expected_paths = {
        "apiTree": "api",
        "operatorTree": "operator",
        "releaseManifest": RELEASE_MANIFEST_NAME,
        "publicSigner": PUBLIC_SIGNER_NAME,
        "catalog": CATALOG_PATH,
    }
    for name, expected_path in expected_paths.items():
        require(artifacts[name].get("path") == expected_path, f"authority candidate {name} path mismatch")

    api_root = root / "api"
    operator_root = root / "operator"
    release_manifest_path = root / RELEASE_MANIFEST_NAME
    signer_path = root / PUBLIC_SIGNER_NAME
    manifest = validate_release_manifest(release_manifest_path, api_root, operator_root)
    records = manifest["files"]
    require(artifacts["apiTree"] == tree_summary(records, "api"), "authority candidate API summary mismatch")
    require(artifacts["operatorTree"] == tree_summary(records, "operator"), "authority candidate Operator summary mismatch")
    require(artifacts["releaseManifest"]["sha256"] == sha256_file(release_manifest_path), "authority candidate release-manifest pin mismatch")
    signer = validate_public_signer(signer_path)
    require(
        artifacts["publicSigner"] == {"path": PUBLIC_SIGNER_NAME, "sha256": sha256_file(signer_path), **signer},
        "authority candidate public-signer pin mismatch",
    )
    catalog = root / CATALOG_PATH
    require(catalog.is_file() and not catalog.is_symlink(), "authority candidate catalog is missing")
    require(sha256_file(catalog) == CATALOG_SHA256, "authority candidate catalog hash mismatch")
    require(artifacts["catalog"] == {"path": CATALOG_PATH, "sha256": CATALOG_SHA256}, "authority candidate catalog pin mismatch")

    services = exact_keys(candidate.get("services"), SERVICE_KEYS, "authority candidate services")
    require(
        exact_keys(services.get("legendsAuthority"), LEGENDS_SERVICE_KEYS, "Legends authority provenance")
        == {
            "serviceId": "eterra-legends-authority",
            "releaseSha256": artifacts["releaseManifest"]["sha256"],
            "authorityConfigSha256": CATALOG_SHA256,
        },
        "Legends authority provenance mismatch",
    )
    require(candidate.get("safety") == SAFETY, "authority candidate safety contract mismatch")
    return candidate


def assemble(args: argparse.Namespace) -> None:
    output = Path(args.output)
    require(output.is_absolute(), "candidate output root must be absolute")
    require(not os.path.lexists(output), f"refusing to overwrite candidate root: {output}")
    api_source = canonical_input_path(Path(args.api_tree), "authority API source", directory=True)
    operator_source = canonical_input_path(Path(args.operator_tree), "authority Operator source", directory=True)
    manifest_source = canonical_input_path(Path(args.release_manifest), "authority release-manifest source")
    signer_source = canonical_input_path(Path(args.public_signer), "authority public-signer source")
    service_unit = canonical_input_path(Path(args.service_unit), "authority service-unit source")
    validate_release_manifest(manifest_source, api_source, operator_source)
    validate_public_signer(signer_source)
    chain = source_pin(Path(args.chain_repository), args.chain_commit, "chain")
    sdkgen = source_pin(Path(args.sdkgen_repository), args.sdkgen_commit, "SDKGen")
    release_id = args.release_id
    require(ID_RE.fullmatch(release_id) is not None, "release ID is invalid")
    created_at = args.created_at or utc_now()
    parse_utc(created_at, "candidate timestamp")
    ensure_hash(args.genesis_hash, "genesis hash")
    ensure_hash(args.runtime_code_hash, "runtime code hash")
    ensure_sha(args.runtime_code_sha256, "runtime code SHA-256")
    ensure_sha(args.runtime_metadata_sha256, "runtime metadata SHA-256")
    require(ADAPTER_RE.fullmatch(args.read_model_adapter_version) is not None, "read-model adapter version is invalid")
    require(args.authority_epoch > 0, "authority epoch must be positive")

    output.mkdir(parents=True, mode=0o750)
    try:
        shutil.copytree(api_source, output / "api", symlinks=False, copy_function=shutil.copy2)
        shutil.copytree(operator_source, output / "operator", symlinks=False, copy_function=shutil.copy2)
        shutil.copy2(manifest_source, output / RELEASE_MANIFEST_NAME, follow_symlinks=False)
        shutil.copy2(signer_source, output / PUBLIC_SIGNER_NAME, follow_symlinks=False)
        os.chmod(output / RELEASE_MANIFEST_NAME, 0o440)
        os.chmod(output / PUBLIC_SIGNER_NAME, 0o440)
        manifest = validate_release_manifest(
            output / RELEASE_MANIFEST_NAME, output / "api", output / "operator"
        )
        signer = validate_public_signer(output / PUBLIC_SIGNER_NAME)
        catalog = output / CATALOG_PATH
        require(catalog.is_file() and sha256_file(catalog) == CATALOG_SHA256, "published catalog pin mismatch")
        release_sha = sha256_file(output / RELEASE_MANIFEST_NAME)
        candidate = {
            "schemaVersion": SCHEMA_VERSION,
            "kind": CANDIDATE_KIND,
            "releaseId": release_id,
            "createdAtUtc": created_at,
            "sources": {"chain": chain, "sdkgen": sdkgen},
            "target": {
                "genesisHash": args.genesis_hash,
                "runtimeSpecVersion": TARGET_RUNTIME_SPEC_VERSION,
                "runtimeCodeHash": args.runtime_code_hash,
                "runtimeCodeSha256": args.runtime_code_sha256,
                "runtimeMetadataScaleSha256": args.runtime_metadata_sha256,
                "readModelAdapterVersion": args.read_model_adapter_version,
                "authorityEpoch": args.authority_epoch,
            },
            "deployment": {
                "serviceUnitSha256": sha256_file(service_unit),
                "runtimeIdentifier": TARGET_RUNTIME_IDENTIFIER,
                "selfContained": True,
                "submitterMode": "in_memory",
                "journalMode": "0700",
                "releasePromotion": "immutable-create-once",
            },
            "artifacts": {
                "apiTree": tree_summary(manifest["files"], "api"),
                "operatorTree": tree_summary(manifest["files"], "operator"),
                "releaseManifest": {"path": RELEASE_MANIFEST_NAME, "sha256": release_sha},
                "publicSigner": {
                    "path": PUBLIC_SIGNER_NAME,
                    "sha256": sha256_file(output / PUBLIC_SIGNER_NAME),
                    **signer,
                },
                "catalog": {"path": CATALOG_PATH, "sha256": CATALOG_SHA256},
            },
            "services": {
                "legendsAuthority": {
                    "serviceId": "eterra-legends-authority",
                    "releaseSha256": release_sha,
                    "authorityConfigSha256": CATALOG_SHA256,
                }
            },
            "safety": SAFETY,
        }
        write_new(output / CANDIDATE_NAME, canonical_bytes(candidate))
        digest = sha256_file(output / CANDIDATE_NAME)
        validate_candidate(output / CANDIDATE_NAME, digest)
        print(f"authority candidate assembled: {output / CANDIDATE_NAME} sha256={digest}")
    except Exception:
        shutil.rmtree(output, ignore_errors=True)
        raise


def validate_liveness(value: Any, signer_public_key: str) -> dict[str, Any]:
    observation = exact_keys(value, LIVENESS_OBSERVATION_KEYS, "authority liveness observation")
    require(observation.get("httpStatus") == 200, "authority liveness HTTP status is not 200")
    nonce_hex = observation.get("requestNonceHex")
    require(isinstance(nonce_hex, str) and re.fullmatch(r"^0x[0-9a-f]{64}$", nonce_hex), "authority liveness request nonce is invalid")
    response = exact_keys(observation.get("response"), LIVENESS_RESPONSE_KEYS, "authority liveness response")
    require(
        response.get("schema") == LIVENESS_SCHEMA
        and response.get("ok") is True
        and response.get("algorithm") == "sr25519"
        and response.get("nonceHex") == nonce_hex
        and response.get("error") == "",
        "authority liveness response failed closed identity checks",
    )
    public_key = response.get("publicKeyHex")
    signature = response.get("signatureHex")
    require(public_key == signer_public_key, "authority liveness signer differs from candidate")
    require(isinstance(signature, str) and SIGNATURE_RE.fullmatch(signature), "authority liveness signature shape is invalid")
    nonce = bytes.fromhex(nonce_hex[2:])
    expected_payload = "0x" + hashlib.sha256(LIVENESS_DOMAIN + nonce).hexdigest()
    require(response.get("payloadHashHex") == expected_payload, "authority liveness payload hash mismatch")
    return dict(response)


def create_receipt(args: argparse.Namespace) -> None:
    candidate_path = canonical_input_path(Path(args.candidate), "authority candidate")
    candidate_sha = ensure_sha(args.expected_candidate_sha256, "expected authority candidate SHA-256")
    candidate = validate_candidate(candidate_path, candidate_sha)
    observation_path = canonical_input_path(Path(args.observation), "authority deployment observation")
    observation = read_json(observation_path, "authority deployment observation", canonical=True)
    exact_keys(observation, OBSERVATION_KEYS, "authority deployment observation")
    require(
        observation.get("schemaVersion") == 1 and observation.get("kind") == OBSERVATION_KIND,
        "authority deployment observation identity mismatch",
    )
    require(observation.get("releaseId") == candidate["releaseId"], "authority observation release mismatch")
    require(observation.get("candidateSha256") == candidate_sha, "authority observation candidate mismatch")
    release_sha = candidate["artifacts"]["releaseManifest"]["sha256"]
    require(observation.get("releaseManifestSha256") == release_sha, "authority observation release manifest mismatch")
    require(observation.get("chainSourceCommit") == candidate["sources"]["chain"]["commit"], "authority observation chain source mismatch")
    require(observation.get("sdkgenSourceCommit") == candidate["sources"]["sdkgen"]["commit"], "authority observation SDKGen source mismatch")
    parse_utc(observation.get("observedAtUtc"), "authority deployment observation timestamp")
    expected_root = f"{DEPLOYMENT_ROOT}/arcade-authority/releases/{candidate_sha}"
    require(observation.get("deploymentRoot") == expected_root, "authority immutable release root mismatch")

    unit = exact_keys(observation.get("serviceUnit"), REMOTE_FILE_KEYS, "authority service unit observation")
    environment = exact_keys(observation.get("environment"), REMOTE_FILE_KEYS, "authority environment observation")
    for item, label in ((unit, "service unit"), (environment, "environment")):
        require(isinstance(item.get("path"), str) and item["path"].startswith("/"), f"authority {label} path is invalid")
        ensure_sha(item.get("sha256"), f"authority {label} SHA-256")
        require(isinstance(item.get("mode"), str) and MODE_RE.fullmatch(item["mode"]), f"authority {label} mode is invalid")
        require(isinstance(item.get("owner"), str) and ":" in item["owner"], f"authority {label} owner is invalid")
    require(
        unit.get("path") == f"/etc/systemd/system/{SERVICE_NAME}.service",
        "authority service-unit path mismatch",
    )
    require(
        environment.get("path") == f"{DEPLOYMENT_ROOT}/shared/env/arcade-authority.env",
        "authority environment path mismatch",
    )
    require(unit.get("sha256") == candidate["deployment"]["serviceUnitSha256"], "deployed authority unit drifted")
    require(unit.get("mode") == "0644" and unit.get("owner") == "root:root", "authority unit permissions mismatch")
    require(
        environment.get("mode") == "0640"
        and environment.get("owner") == f"root:{DEPLOYMENT_USER}",
        "authority environment permissions mismatch",
    )

    secrets = exact_keys(observation.get("secrets"), SECRET_OBSERVATION_KEYS, "authority secret observations")
    expected_secret_paths = {
        "signerMnemonic": f"{DEPLOYMENT_ROOT}/shared/secrets/nexus-v2-legends-authority.mnemonic",
        "privateAlphaAccessKey": f"{DEPLOYMENT_ROOT}/shared/secrets/nexus-v2-legends-authority.access-key",
        "signerDerivationPassword": f"{DEPLOYMENT_ROOT}/shared/secrets/nexus-v2-legends-authority.derivation-password",
    }
    for name, expected_path in expected_secret_paths.items():
        item = secrets.get(name)
        if name == "signerDerivationPassword" and item is None:
            continue
        item = exact_keys(item, REMOTE_FILE_KEYS, f"authority {name} observation")
        require(item.get("path") == expected_path, f"authority {name} path mismatch")
        ensure_sha(item.get("sha256"), f"authority {name} SHA-256")
        require(
            item.get("mode") == "0640"
            and item.get("owner") == f"root:{DEPLOYMENT_USER}",
            f"authority {name} permissions mismatch",
        )

    process = exact_keys(observation.get("process"), PROCESS_KEYS, "authority process observation")
    require(process.get("serviceActive") is True, "authority service is not active")
    require(type(process.get("mainPid")) is int and process["mainPid"] > 1, "authority main PID is invalid")
    require(process.get("user") == DEPLOYMENT_USER, "authority process user mismatch")
    expected_executable = expected_root + "/api/Eterra.Arcade.Authority.Api"
    require(process.get("executablePath") == expected_executable, "authority /proc executable path mismatch")
    manifest = read_json(candidate_path.parent / RELEASE_MANIFEST_NAME, "authority release manifest")
    executable_record = next((item for item in manifest["files"] if item["path"] == "api/Eterra.Arcade.Authority.Api"), None)
    require(executable_record is not None, "authority API executable is absent from release manifest")
    require(process.get("procExecutableSha256") == executable_record["sha256"], "authority /proc executable bytes drifted")
    require(process.get("listenerHost") == "127.0.0.1", "authority Phase-1 listener is not loopback-only")
    require(process.get("listenerPort") == SERVICE_PORT, "authority listener port mismatch")
    require(process.get("environmentMatched") is True, "authority live process environment drifted")

    catalog = exact_keys(observation.get("catalog"), REMOTE_FILE_KEYS, "authority catalog observation")
    require(catalog.get("path") == expected_root + "/" + CATALOG_PATH, "authority deployed catalog path mismatch")
    require(catalog.get("sha256") == CATALOG_SHA256, "authority deployed catalog hash mismatch")
    require(
        catalog.get("mode") == "0644" and catalog.get("owner") == "root:root",
        "authority catalog permissions mismatch",
    )
    verification = exact_keys(observation.get("manifestVerification"), MANIFEST_VERIFICATION_KEYS, "authority manifest verification")
    require(verification.get("ok") is True, "deployed Operator did not verify release manifest")
    operator_path = expected_root + "/operator/Eterra.Arcade.Authority.Operator"
    require(verification.get("operatorCliPath") == operator_path, "authority Operator CLI path mismatch")
    operator_record = next((item for item in manifest["files"] if item["path"] == "operator/Eterra.Arcade.Authority.Operator"), None)
    require(operator_record is not None, "authority Operator executable is absent from release manifest")
    require(verification.get("operatorCliSha256") == operator_record["sha256"], "authority Operator CLI bytes drifted")
    ensure_sha(verification.get("stdoutSha256"), "authority manifest verification output SHA-256")

    journal = exact_keys(observation.get("journal"), JOURNAL_KEYS, "authority journal observation")
    require(
        journal.get("path") == "/var/lib/eterra/legends-authority-journal"
        and journal.get("mode") == "0700"
        and journal.get("owner") == f"{DEPLOYMENT_USER}:{DEPLOYMENT_USER}"
        and journal.get("nonSymlinkDirectory") is True,
        "authority durable journal contract mismatch",
    )
    liveness = validate_liveness(
        observation.get("liveness"), candidate["artifacts"]["publicSigner"]["publicKey"]
    )
    receipt = {
        "schemaVersion": 1,
        "kind": RECEIPT_KIND,
        "releaseId": candidate["releaseId"],
        "candidateSha256": candidate_sha,
        "releaseManifestSha256": release_sha,
        "chainSourceCommit": candidate["sources"]["chain"]["commit"],
        "sdkgenSourceCommit": candidate["sources"]["sdkgen"]["commit"],
        "deploymentRoot": expected_root,
        "serviceUnit": dict(unit),
        "environment": dict(environment),
        "secrets": dict(secrets),
        "process": dict(process),
        "catalog": dict(catalog),
        "manifestVerification": dict(verification),
        "journal": dict(journal),
        "liveness": {
            "response": liveness,
            "signerMatchesCandidate": True,
            "payloadHashVerified": True,
            "signatureShapeVerified": True,
            "signatureCryptographicallyVerified": False,
            "cryptographicVerificationRequiredAtRestrictedReopen": True,
        },
        "safety": SAFETY,
        "observedAtUtc": observation["observedAtUtc"],
    }
    output = Path(args.output)
    require(output.is_absolute(), "authority deployment receipt path must be absolute")
    output.parent.mkdir(parents=True, exist_ok=True)
    write_new(output, canonical_bytes(receipt))
    print(f"authority deployment receipt captured: {output} sha256={sha256_file(output)}")


def command_verify(args: argparse.Namespace) -> None:
    candidate = validate_candidate(
        Path(args.candidate),
        args.expected_sha256,
        expected_release_id=args.expected_release_id,
        expected_chain_commit=args.expected_chain_commit,
        expected_sdkgen_commit=args.expected_sdkgen_commit,
    )
    summary = {
        "candidateSha256": sha256_file(Path(args.candidate)),
        "releaseId": candidate["releaseId"],
        "chainSourceCommit": candidate["sources"]["chain"]["commit"],
        "sdkgenSourceCommit": candidate["sources"]["sdkgen"]["commit"],
        "releaseManifestSha256": candidate["artifacts"]["releaseManifest"]["sha256"],
        "publicSignerSha256": candidate["artifacts"]["publicSigner"]["sha256"],
        "publicKey": candidate["artifacts"]["publicSigner"]["publicKey"],
        "catalogSha256": candidate["artifacts"]["catalog"]["sha256"],
        "genesisHash": candidate["target"]["genesisHash"],
        "runtimeCodeHash": candidate["target"]["runtimeCodeHash"],
        "runtimeCodeSha256": candidate["target"]["runtimeCodeSha256"],
        "runtimeMetadataScaleSha256": candidate["target"]["runtimeMetadataScaleSha256"],
        "readModelAdapterVersion": candidate["target"]["readModelAdapterVersion"],
        "authorityEpoch": candidate["target"]["authorityEpoch"],
        "serviceUnitSha256": candidate["deployment"]["serviceUnitSha256"],
    }
    print(json.dumps(summary, sort_keys=True, separators=(",", ":")))


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    assemble_parser = commands.add_parser("assemble", help="assemble a create-once immutable candidate root")
    assemble_parser.add_argument("--api-tree", required=True)
    assemble_parser.add_argument("--operator-tree", required=True)
    assemble_parser.add_argument("--release-manifest", required=True)
    assemble_parser.add_argument("--public-signer", required=True)
    assemble_parser.add_argument("--service-unit", required=True)
    assemble_parser.add_argument("--release-id", required=True)
    assemble_parser.add_argument("--chain-repository", required=True)
    assemble_parser.add_argument("--chain-commit", required=True)
    assemble_parser.add_argument("--sdkgen-repository", required=True)
    assemble_parser.add_argument("--sdkgen-commit", required=True)
    assemble_parser.add_argument("--genesis-hash", required=True)
    assemble_parser.add_argument("--runtime-code-hash", required=True)
    assemble_parser.add_argument("--runtime-code-sha256", required=True)
    assemble_parser.add_argument("--runtime-metadata-sha256", required=True)
    assemble_parser.add_argument("--read-model-adapter-version", required=True)
    assemble_parser.add_argument("--authority-epoch", required=True, type=int)
    assemble_parser.add_argument("--created-at")
    assemble_parser.add_argument("--output", required=True)
    assemble_parser.set_defaults(func=assemble)

    verify_parser = commands.add_parser("verify", help="verify a candidate without building or contacting a host")
    verify_parser.add_argument("--candidate", required=True)
    verify_parser.add_argument("--expected-sha256")
    verify_parser.add_argument("--expected-release-id")
    verify_parser.add_argument("--expected-chain-commit")
    verify_parser.add_argument("--expected-sdkgen-commit")
    verify_parser.set_defaults(func=command_verify)

    receipt_parser = commands.add_parser("create-receipt", help="validate a remote observation and create a closed receipt")
    receipt_parser.add_argument("--candidate", required=True)
    receipt_parser.add_argument("--expected-candidate-sha256", required=True)
    receipt_parser.add_argument("--observation", required=True)
    receipt_parser.add_argument("--output", required=True)
    receipt_parser.set_defaults(func=create_receipt)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        args.func(args)
    except (CandidateError, OSError, shutil.Error) as exc:
        print(f"authority_candidate: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
