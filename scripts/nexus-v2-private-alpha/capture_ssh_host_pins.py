#!/usr/bin/env python3
"""Capture the two private-alpha SSH host pins without network access.

This utility deliberately has no host-discovery mode.  It accepts an explicit
local OpenSSH known_hosts file, selects only the exact plain-IP records required
by the Nexus V2 private alpha, and writes a new dedicated known_hosts file plus
a canonical provenance manifest.  It never invokes ssh, ssh-keyscan, DNS, a
socket, or any subprocess.
"""

from __future__ import annotations

import argparse
import base64
import binascii
import fnmatch
import hashlib
import hmac
import json
import os
from pathlib import Path
import stat
import sys
from typing import Any, NoReturn


SCHEMA_VERSION = 1
MANIFEST_KIND = "nexus-v2-private-alpha-ssh-host-pins"
TARGET_HOSTS = ("192.168.1.159", "192.168.1.218")
SSH_PORT = 22
EXPECTED_KEY_TYPES = (
    "ssh-ed25519",
    "ssh-rsa",
    "ecdsa-sha2-nistp256",
)
OUTPUT_MODE = 0o600
MAX_SOURCE_BYTES = 16 * 1024 * 1024


class PinError(RuntimeError):
    """Raised when the local trust source or generated artifact is unsafe."""


def fail(message: str) -> NoReturn:
    raise PinError(message)


def require(condition: bool, message: str) -> None:
    if not condition:
        fail(message)


def canonical_json(value: Any) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")


def sha256_hex(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def _duplicate_rejecting_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        if key in value:
            fail(f"duplicate manifest key: {key}")
        value[key] = item
    return value


def _require_absolute_regular_owned_file(path: Path, label: str) -> os.stat_result:
    require(path.is_absolute(), f"{label} path must be absolute")
    try:
        metadata = path.lstat()
    except FileNotFoundError:
        fail(f"{label} does not exist: {path}")
    require(stat.S_ISREG(metadata.st_mode), f"{label} must be a regular file")
    require(not path.is_symlink(), f"{label} must not be a symlink")
    require(metadata.st_uid == os.getuid(), f"{label} must be owned by the current user")
    return metadata


def _read_local_file(path: Path, label: str, *, require_mode: bool) -> tuple[bytes, os.stat_result]:
    before = _require_absolute_regular_owned_file(path, label)
    if require_mode:
        require(
            stat.S_IMODE(before.st_mode) == OUTPUT_MODE,
            f"{label} mode must be exactly 0600",
        )
    require(before.st_size <= MAX_SOURCE_BYTES, f"{label} exceeds the size limit")
    flags = os.O_RDONLY
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        descriptor = os.open(path, flags)
    except OSError as error:
        fail(f"cannot safely open {label}: {error}")
    try:
        chunks: list[bytes] = []
        remaining = MAX_SOURCE_BYTES + 1
        while remaining:
            chunk = os.read(descriptor, min(1024 * 1024, remaining))
            if not chunk:
                break
            chunks.append(chunk)
            remaining -= len(chunk)
        payload = b"".join(chunks)
        require(len(payload) <= MAX_SOURCE_BYTES, f"{label} exceeds the size limit")
        opened = os.fstat(descriptor)
    finally:
        os.close(descriptor)
    require(
        (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns)
        == (opened.st_dev, opened.st_ino, opened.st_size, opened.st_mtime_ns),
        f"{label} changed while it was read",
    )
    require(stat.S_ISREG(opened.st_mode), f"{label} changed file type while it was read")
    require(opened.st_uid == os.getuid(), f"{label} changed owner while it was read")
    if require_mode:
        require(stat.S_IMODE(opened.st_mode) == OUTPUT_MODE, f"{label} mode changed while it was read")
    return payload, opened


def _decode_text(payload: bytes, label: str) -> str:
    require(b"\x00" not in payload, f"{label} contains a NUL byte")
    require(b"\r" not in payload, f"{label} contains a carriage return")
    try:
        return payload.decode("utf-8")
    except UnicodeDecodeError as error:
        fail(f"{label} is not UTF-8: {error}")


def _hashed_host_matches(value: str, candidate: str) -> bool:
    if not value.startswith("|1|"):
        return False
    pieces = value.split("|")
    if len(pieces) != 4 or pieces[0] != "" or pieces[1] != "1":
        return False
    try:
        salt = base64.b64decode(pieces[2], validate=True)
        expected = base64.b64decode(pieces[3], validate=True)
    except (binascii.Error, ValueError):
        return False
    actual = hmac.new(salt, candidate.encode("utf-8"), hashlib.sha1).digest()
    return hmac.compare_digest(actual, expected)


def _host_token_matches(token: str, candidate: str) -> bool:
    token = token.removeprefix("!")
    if token == candidate:
        return True
    if _hashed_host_matches(token, candidate):
        return True
    return fnmatch.fnmatchcase(candidate, token)


def _host_field_references_target(host_field: str, target: str) -> bool:
    candidates = (target, f"[{target}]:{SSH_PORT}")
    return any(
        _host_token_matches(token, candidate)
        for token in host_field.split(",")
        for candidate in candidates
    )


def _read_ssh_string(blob: bytes, offset: int, label: str) -> tuple[bytes, int]:
    require(offset + 4 <= len(blob), f"{label} has a truncated SSH string length")
    length = int.from_bytes(blob[offset : offset + 4], "big")
    offset += 4
    require(offset + length <= len(blob), f"{label} has a truncated SSH string")
    return blob[offset : offset + length], offset + length


def _read_positive_mpint(blob: bytes, offset: int, label: str) -> tuple[int, int]:
    encoded, offset = _read_ssh_string(blob, offset, label)
    require(encoded, f"{label} has an empty positive mpint")
    require(not (encoded[0] & 0x80), f"{label} has a negative mpint")
    if len(encoded) > 1 and encoded[0] == 0:
        require(encoded[1] & 0x80, f"{label} has a non-canonical mpint")
    return int.from_bytes(encoded, "big"), offset


def _validate_key_blob(key_type: str, encoded: str, label: str) -> tuple[bytes, int]:
    require(encoded and not any(character.isspace() for character in encoded), f"{label} has invalid base64")
    try:
        blob = base64.b64decode(encoded, validate=True)
    except (binascii.Error, ValueError) as error:
        fail(f"{label} has invalid base64: {error}")
    require(base64.b64encode(blob).decode("ascii") == encoded, f"{label} base64 is not canonical")
    algorithm, offset = _read_ssh_string(blob, 0, label)
    require(algorithm == key_type.encode("ascii"), f"{label} key type differs from its SSH blob")

    if key_type == "ssh-ed25519":
        public_key, offset = _read_ssh_string(blob, offset, label)
        require(len(public_key) == 32, f"{label} has an invalid Ed25519 key length")
        bits = 256
    elif key_type == "ecdsa-sha2-nistp256":
        curve, offset = _read_ssh_string(blob, offset, label)
        point, offset = _read_ssh_string(blob, offset, label)
        require(curve == b"nistp256", f"{label} has an unexpected ECDSA curve")
        require(len(point) == 65 and point[0] == 4, f"{label} has an invalid ECDSA point")
        bits = 256
    elif key_type == "ssh-rsa":
        exponent, offset = _read_positive_mpint(blob, offset, f"{label} exponent")
        modulus, offset = _read_positive_mpint(blob, offset, f"{label} modulus")
        require(exponent >= 3 and exponent % 2 == 1, f"{label} has an invalid RSA exponent")
        bits = modulus.bit_length()
        require(bits >= 2048, f"{label} RSA modulus is smaller than 2048 bits")
    else:  # Kept defensive even though callers enforce the exact set.
        fail(f"{label} has an unsupported key type")
    require(offset == len(blob), f"{label} has trailing SSH key data")
    return blob, bits


def _fingerprint(blob: bytes) -> str:
    encoded = base64.b64encode(hashlib.sha256(blob).digest()).decode("ascii").rstrip("=")
    return f"SHA256:{encoded}"


def select_pins(payload: bytes, label: str) -> dict[str, dict[str, dict[str, Any]]]:
    text = _decode_text(payload, label)
    selected: dict[str, dict[str, dict[str, Any]]] = {host: {} for host in TARGET_HOSTS}

    for line_number, raw_line in enumerate(text.splitlines(), 1):
        stripped = raw_line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        fields = stripped.split()
        marker = fields[0] if fields and fields[0].startswith("@") else None
        host_index = 1 if marker else 0
        host_field = fields[host_index] if len(fields) > host_index else ""
        referenced = [
            host for host in TARGET_HOSTS if _host_field_references_target(host_field, host)
        ]
        if not referenced:
            continue
        require(len(referenced) == 1, f"{label} line {line_number} ambiguously references both targets")
        host = referenced[0]
        require(marker is None, f"{label} line {line_number} uses a forbidden known_hosts marker")
        require(host_field == host, f"{label} line {line_number} does not use the exact plain IP host field")
        require(
            len(fields) == 3,
            f"{label} line {line_number} has forbidden options, aliases, or trailing fields",
        )
        key_type, encoded = fields[1], fields[2]
        require(
            key_type in EXPECTED_KEY_TYPES,
            f"{label} line {line_number} has unexpected key type {key_type!r}",
        )
        require(
            key_type not in selected[host],
            f"{label} has a duplicate {key_type} record for {host}",
        )
        blob, bits = _validate_key_blob(key_type, encoded, f"{label} line {line_number}")
        selected[host][key_type] = {
            "base64": encoded,
            "bits": bits,
            "fingerprintSha256": _fingerprint(blob),
            "sourceLine": line_number,
        }

    for host in TARGET_HOSTS:
        missing = [key_type for key_type in EXPECTED_KEY_TYPES if key_type not in selected[host]]
        require(not missing, f"{label} is missing required key types for {host}: {', '.join(missing)}")
        require(
            set(selected[host]) == set(EXPECTED_KEY_TYPES),
            f"{label} has an unexpected key set for {host}",
        )
    return selected


def render_known_hosts(selected: dict[str, dict[str, dict[str, Any]]]) -> bytes:
    lines = [
        f"{host} {key_type} {selected[host][key_type]['base64']}"
        for host in TARGET_HOSTS
        for key_type in EXPECTED_KEY_TYPES
    ]
    return ("\n".join(lines) + "\n").encode("ascii")


def build_manifest(
    source_payload: bytes,
    source_mode: int,
    known_hosts_payload: bytes,
    selected: dict[str, dict[str, dict[str, Any]]],
) -> dict[str, Any]:
    hosts = []
    for host in TARGET_HOSTS:
        keys = []
        for key_type in EXPECTED_KEY_TYPES:
            record = selected[host][key_type]
            keys.append(
                {
                    "bits": record["bits"],
                    "fingerprintSha256": record["fingerprintSha256"],
                    "keyType": key_type,
                    "sourceLine": record["sourceLine"],
                }
            )
        hosts.append({"host": host, "keys": keys, "port": SSH_PORT})
    return {
        "hosts": hosts,
        "kind": MANIFEST_KIND,
        "knownHosts": {
            "lineCount": len(TARGET_HOSTS) * len(EXPECTED_KEY_TYPES),
            "mode": "0600",
            "sha256": sha256_hex(known_hosts_payload),
        },
        "policy": {
            "aliasesAllowed": False,
            "expectedKeyTypes": list(EXPECTED_KEY_TYPES),
            "hostFieldEncoding": "plain-ip-only",
            "markersAllowed": False,
            "targetHosts": list(TARGET_HOSTS),
            "trailingFieldsAllowed": False,
        },
        "schemaVersion": SCHEMA_VERSION,
        "source": {
            "mode": f"{source_mode:04o}",
            "sha256": sha256_hex(source_payload),
        },
    }


def _write_new_0600(path: Path, payload: bytes, label: str) -> None:
    require(path.is_absolute(), f"{label} output path must be absolute")
    require(path.parent.is_dir(), f"{label} output parent does not exist")
    require(not path.parent.is_symlink(), f"{label} output parent must not be a symlink")
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        descriptor = os.open(path, flags, OUTPUT_MODE)
    except OSError as error:
        fail(f"cannot create new {label}: {error}")
    try:
        view = memoryview(payload)
        while view:
            written = os.write(descriptor, view)
            require(written > 0, f"short write while creating {label}")
            view = view[written:]
        os.fchmod(descriptor, OUTPUT_MODE)
        os.fsync(descriptor)
    except BaseException:
        os.close(descriptor)
        path.unlink(missing_ok=True)
        raise
    os.close(descriptor)
    metadata = path.lstat()
    require(stat.S_ISREG(metadata.st_mode) and not path.is_symlink(), f"{label} is not a regular file")
    require(metadata.st_uid == os.getuid(), f"{label} is not owned by the current user")
    require(stat.S_IMODE(metadata.st_mode) == OUTPUT_MODE, f"{label} mode is not 0600")


def _load_manifest(path: Path) -> tuple[dict[str, Any], bytes]:
    payload, _ = _read_local_file(path, "manifest", require_mode=True)
    try:
        value = json.loads(payload, object_pairs_hook=_duplicate_rejecting_object)
    except (json.JSONDecodeError, UnicodeDecodeError) as error:
        fail(f"manifest is not valid JSON: {error}")
    require(isinstance(value, dict), "manifest root must be an object")
    require(payload == canonical_json(value), "manifest is not canonical JSON")
    return value, payload


def _require_exact_keys(value: dict[str, Any], expected: set[str], label: str) -> None:
    require(set(value) == expected, f"{label} keys differ from the contract")


def validate_manifest(
    manifest: dict[str, Any],
    known_hosts_payload: bytes,
    known_hosts_selected: dict[str, dict[str, dict[str, Any]]],
) -> None:
    _require_exact_keys(
        manifest,
        {"hosts", "kind", "knownHosts", "policy", "schemaVersion", "source"},
        "manifest",
    )
    require(manifest["schemaVersion"] == SCHEMA_VERSION, "manifest schema version mismatch")
    require(manifest["kind"] == MANIFEST_KIND, "manifest kind mismatch")
    expected_policy = {
        "aliasesAllowed": False,
        "expectedKeyTypes": list(EXPECTED_KEY_TYPES),
        "hostFieldEncoding": "plain-ip-only",
        "markersAllowed": False,
        "targetHosts": list(TARGET_HOSTS),
        "trailingFieldsAllowed": False,
    }
    require(manifest["policy"] == expected_policy, "manifest policy mismatch")
    require(isinstance(manifest["source"], dict), "manifest source must be an object")
    _require_exact_keys(manifest["source"], {"mode", "sha256"}, "manifest source")
    require(manifest["source"]["mode"] == "0600", "manifest source mode mismatch")
    require(
        isinstance(manifest["source"]["sha256"], str)
        and len(manifest["source"]["sha256"]) == 64
        and all(character in "0123456789abcdef" for character in manifest["source"]["sha256"]),
        "manifest source SHA-256 is invalid",
    )
    expected_known_hosts = {
        "lineCount": len(TARGET_HOSTS) * len(EXPECTED_KEY_TYPES),
        "mode": "0600",
        "sha256": sha256_hex(known_hosts_payload),
    }
    require(manifest["knownHosts"] == expected_known_hosts, "manifest known_hosts identity mismatch")
    require(isinstance(manifest["hosts"], list), "manifest hosts must be an array")
    require(len(manifest["hosts"]) == len(TARGET_HOSTS), "manifest host count mismatch")
    for host_index, host in enumerate(TARGET_HOSTS):
        host_value = manifest["hosts"][host_index]
        require(isinstance(host_value, dict), "manifest host record must be an object")
        _require_exact_keys(host_value, {"host", "keys", "port"}, "manifest host record")
        require(host_value["host"] == host and host_value["port"] == SSH_PORT, "manifest target mismatch")
        require(isinstance(host_value["keys"], list), "manifest keys must be an array")
        require(len(host_value["keys"]) == len(EXPECTED_KEY_TYPES), "manifest key count mismatch")
        for key_index, key_type in enumerate(EXPECTED_KEY_TYPES):
            key_value = host_value["keys"][key_index]
            require(isinstance(key_value, dict), "manifest key record must be an object")
            _require_exact_keys(
                key_value,
                {"bits", "fingerprintSha256", "keyType", "sourceLine"},
                "manifest key record",
            )
            selected = known_hosts_selected[host][key_type]
            require(key_value["keyType"] == key_type, "manifest key order mismatch")
            require(key_value["bits"] == selected["bits"], "manifest key size mismatch")
            require(
                key_value["fingerprintSha256"] == selected["fingerprintSha256"],
                "manifest key fingerprint mismatch",
            )
            require(
                isinstance(key_value["sourceLine"], int)
                and not isinstance(key_value["sourceLine"], bool)
                and key_value["sourceLine"] > 0,
                "manifest source line is invalid",
            )


def capture(source: Path, known_hosts_out: Path, manifest_out: Path) -> dict[str, Any]:
    require(known_hosts_out != manifest_out, "known_hosts and manifest outputs must differ")
    require(source not in {known_hosts_out, manifest_out}, "source and outputs must differ")
    require(
        known_hosts_out.parent == manifest_out.parent,
        "known_hosts and manifest outputs must share one dedicated directory",
    )
    source_payload, source_metadata = _read_local_file(source, "source known_hosts", require_mode=True)
    selected = select_pins(source_payload, "source known_hosts")
    known_hosts_payload = render_known_hosts(selected)
    manifest = build_manifest(
        source_payload,
        stat.S_IMODE(source_metadata.st_mode),
        known_hosts_payload,
        selected,
    )
    manifest_payload = canonical_json(manifest)
    created_known_hosts = False
    try:
        _write_new_0600(known_hosts_out, known_hosts_payload, "dedicated known_hosts")
        created_known_hosts = True
        _write_new_0600(manifest_out, manifest_payload, "host-pin manifest")
    except BaseException:
        if created_known_hosts:
            known_hosts_out.unlink(missing_ok=True)
        raise
    verify(known_hosts_out, manifest_out, source)
    return {
        "knownHostsPath": str(known_hosts_out),
        "knownHostsSha256": sha256_hex(known_hosts_payload),
        "manifestPath": str(manifest_out),
        "manifestSha256": sha256_hex(manifest_payload),
    }


def verify(known_hosts: Path, manifest_path: Path, source: Path | None = None) -> dict[str, Any]:
    known_hosts_payload, _ = _read_local_file(
        known_hosts, "dedicated known_hosts", require_mode=True
    )
    manifest, manifest_payload = _load_manifest(manifest_path)
    selected = select_pins(known_hosts_payload, "dedicated known_hosts")
    require(
        known_hosts_payload == render_known_hosts(selected),
        "dedicated known_hosts is not in canonical host/key order",
    )
    validate_manifest(manifest, known_hosts_payload, selected)
    if source is not None:
        source_payload, source_metadata = _read_local_file(
            source, "source known_hosts", require_mode=True
        )
        source_selected = select_pins(source_payload, "source known_hosts")
        expected = build_manifest(
            source_payload,
            stat.S_IMODE(source_metadata.st_mode),
            known_hosts_payload,
            source_selected,
        )
        require(manifest == expected, "manifest does not match the pinned source known_hosts")
    return {
        "knownHostsPath": str(known_hosts),
        "knownHostsSha256": sha256_hex(known_hosts_payload),
        "manifestPath": str(manifest_path),
        "manifestSha256": sha256_hex(manifest_payload),
    }


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    capture_parser = subparsers.add_parser("capture", help="capture pins from a local known_hosts")
    capture_parser.add_argument("--source-known-hosts", type=Path, required=True)
    capture_parser.add_argument("--known-hosts-out", type=Path, required=True)
    capture_parser.add_argument("--manifest-out", type=Path, required=True)

    verify_parser = subparsers.add_parser("verify", help="verify an existing dedicated pin artifact")
    verify_parser.add_argument("--known-hosts", type=Path, required=True)
    verify_parser.add_argument("--manifest", type=Path, required=True)
    verify_parser.add_argument("--source-known-hosts", type=Path)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    os.umask(0o077)
    args = parse_args(sys.argv[1:] if argv is None else argv)
    try:
        if args.command == "capture":
            summary = capture(args.source_known_hosts, args.known_hosts_out, args.manifest_out)
        else:
            summary = verify(args.known_hosts, args.manifest, args.source_known_hosts)
    except PinError as error:
        print(f"capture_ssh_host_pins: {error}", file=sys.stderr)
        return 2
    print(json.dumps(summary, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
