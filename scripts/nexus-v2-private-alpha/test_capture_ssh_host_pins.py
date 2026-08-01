#!/usr/bin/env python3

from __future__ import annotations

import base64
import hashlib
import hmac
import importlib.util
import json
import os
from pathlib import Path
import socket
import stat
import subprocess
import tempfile
import unittest
from unittest import mock


SCRIPT = Path(__file__).with_name("capture_ssh_host_pins.py")
SPEC = importlib.util.spec_from_file_location("capture_ssh_host_pins", SCRIPT)
assert SPEC and SPEC.loader
pins = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(pins)


def ssh_string(value: bytes) -> bytes:
    return len(value).to_bytes(4, "big") + value


def encoded_key(key_type: str, seed: int) -> str:
    algorithm = ssh_string(key_type.encode("ascii"))
    if key_type == "ssh-ed25519":
        blob = algorithm + ssh_string(bytes([seed]) * 32)
    elif key_type == "ecdsa-sha2-nistp256":
        point = b"\x04" + bytes([seed]) * 64
        blob = algorithm + ssh_string(b"nistp256") + ssh_string(point)
    elif key_type == "ssh-rsa":
        exponent = b"\x01\x00\x01"
        # A canonical positive 2048-bit mpint requires the leading zero.
        modulus = b"\x00\x80" + bytes([seed]) * 255
        blob = algorithm + ssh_string(exponent) + ssh_string(modulus)
    else:
        raise AssertionError(key_type)
    return base64.b64encode(blob).decode("ascii")


def record(host: str, key_type: str, seed: int) -> str:
    return f"{host} {key_type} {encoded_key(key_type, seed)}"


def valid_source_lines() -> list[str]:
    lines = ["# unrelated local trust material", record("example.invalid", "ssh-ed25519", 42)]
    seed = 1
    # Deliberately reverse both orders; capture output must still be canonical.
    for host in reversed(pins.TARGET_HOSTS):
        for key_type in reversed(pins.EXPECTED_KEY_TYPES):
            lines.append(record(host, key_type, seed))
            seed += 1
    return lines


def write_source(path: Path, lines: list[str]) -> None:
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    path.chmod(0o600)


class HostPinCaptureTests(unittest.TestCase):
    def capture(self, lines: list[str] | None = None):
        temporary = tempfile.TemporaryDirectory()
        root = Path(temporary.name)
        source = root / "source-known-hosts"
        dedicated = root / "nexus-v2-alpha.known_hosts"
        manifest = root / "nexus-v2-alpha.known_hosts.json"
        write_source(source, valid_source_lines() if lines is None else lines)
        result = pins.capture(source.resolve(), dedicated.resolve(), manifest.resolve())
        return temporary, source, dedicated, manifest, result

    def test_capture_is_canonical_hash_pinned_and_mode_0600(self):
        temporary, source, dedicated, manifest, result = self.capture()
        self.addCleanup(temporary.cleanup)

        expected_lines = []
        source_records = {}
        for line in source.read_text(encoding="utf-8").splitlines():
            fields = line.split()
            if len(fields) == 3 and fields[0] in pins.TARGET_HOSTS:
                source_records[(fields[0], fields[1])] = fields[2]
        for host in pins.TARGET_HOSTS:
            for key_type in pins.EXPECTED_KEY_TYPES:
                expected_lines.append(f"{host} {key_type} {source_records[(host, key_type)]}")
        self.assertEqual(dedicated.read_text(encoding="ascii"), "\n".join(expected_lines) + "\n")
        self.assertEqual(stat.S_IMODE(dedicated.stat().st_mode), 0o600)
        self.assertEqual(stat.S_IMODE(manifest.stat().st_mode), 0o600)

        value = json.loads(manifest.read_bytes())
        self.assertEqual(manifest.read_bytes(), pins.canonical_json(value))
        self.assertEqual(value["knownHosts"]["sha256"], hashlib.sha256(dedicated.read_bytes()).hexdigest())
        self.assertEqual(value["source"]["sha256"], hashlib.sha256(source.read_bytes()).hexdigest())
        self.assertEqual(result["manifestSha256"], hashlib.sha256(manifest.read_bytes()).hexdigest())
        self.assertEqual(
            pins.verify(dedicated.resolve(), manifest.resolve(), source.resolve())["knownHostsSha256"],
            value["knownHosts"]["sha256"],
        )

    def test_same_source_produces_byte_identical_artifacts(self):
        first = self.capture()
        second = self.capture()
        self.addCleanup(first[0].cleanup)
        self.addCleanup(second[0].cleanup)
        self.assertEqual(first[2].read_bytes(), second[2].read_bytes())
        self.assertEqual(first[3].read_bytes(), second[3].read_bytes())

    def assert_source_rejected(self, mutator, expected: str):
        lines = valid_source_lines()
        mutator(lines)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "source-known-hosts"
            write_source(source, lines)
            with self.assertRaisesRegex(pins.PinError, expected):
                pins.capture(
                    source.resolve(),
                    (root / "dedicated").resolve(),
                    (root / "manifest").resolve(),
                )

    def target_line_index(self, lines: list[str], host: str, key_type: str) -> int:
        prefix = f"{host} {key_type} "
        return next(index for index, line in enumerate(lines) if line.startswith(prefix))

    def test_alias_is_rejected(self):
        def mutate(lines):
            index = self.target_line_index(lines, pins.TARGET_HOSTS[0], "ssh-ed25519")
            lines[index] = lines[index].replace(pins.TARGET_HOSTS[0], f"{pins.TARGET_HOSTS[0]},chain-alpha", 1)

        self.assert_source_rejected(mutate, "exact plain IP")

    def test_marker_is_rejected(self):
        def mutate(lines):
            index = self.target_line_index(lines, pins.TARGET_HOSTS[0], "ssh-ed25519")
            lines[index] = "@revoked " + lines[index]

        self.assert_source_rejected(mutate, "forbidden known_hosts marker")

    def test_trailing_comment_or_option_is_rejected(self):
        def mutate(lines):
            index = self.target_line_index(lines, pins.TARGET_HOSTS[0], "ssh-ed25519")
            lines[index] += " locally-captured"

        self.assert_source_rejected(mutate, "forbidden options, aliases, or trailing fields")

    def test_duplicate_key_type_is_rejected(self):
        def mutate(lines):
            index = self.target_line_index(lines, pins.TARGET_HOSTS[0], "ssh-ed25519")
            lines.append(lines[index])

        self.assert_source_rejected(mutate, "duplicate ssh-ed25519")

    def test_missing_key_type_is_rejected(self):
        def mutate(lines):
            index = self.target_line_index(lines, pins.TARGET_HOSTS[0], "ssh-rsa")
            lines.pop(index)

        self.assert_source_rejected(mutate, "missing required key types")

    def test_bracketed_default_port_entry_is_rejected(self):
        def mutate(lines):
            index = self.target_line_index(lines, pins.TARGET_HOSTS[0], "ssh-ed25519")
            lines[index] = lines[index].replace(
                pins.TARGET_HOSTS[0], f"[{pins.TARGET_HOSTS[0]}]:22", 1
            )

        self.assert_source_rejected(mutate, "exact plain IP")

    def test_hashed_target_entry_is_rejected_as_non_plain(self):
        def mutate(lines):
            index = self.target_line_index(lines, pins.TARGET_HOSTS[0], "ssh-ed25519")
            salt = bytes(range(20))
            digest = hmac.new(salt, pins.TARGET_HOSTS[0].encode(), hashlib.sha1).digest()
            hashed = "|1|" + base64.b64encode(salt).decode() + "|" + base64.b64encode(digest).decode()
            lines[index] = lines[index].replace(pins.TARGET_HOSTS[0], hashed, 1)

        self.assert_source_rejected(mutate, "exact plain IP")

    def test_key_type_blob_mismatch_is_rejected(self):
        def mutate(lines):
            index = self.target_line_index(lines, pins.TARGET_HOSTS[0], "ssh-ed25519")
            lines[index] = f"{pins.TARGET_HOSTS[0]} ssh-ed25519 {encoded_key('ssh-rsa', 9)}"

        self.assert_source_rejected(mutate, "key type differs")

    def test_existing_output_is_not_overwritten(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        source = root / "source"
        dedicated = root / "dedicated"
        manifest = root / "manifest"
        write_source(source, valid_source_lines())
        dedicated.write_text("sentinel\n", encoding="utf-8")
        dedicated.chmod(0o600)
        with self.assertRaisesRegex(pins.PinError, "cannot create new dedicated known_hosts"):
            pins.capture(source.resolve(), dedicated.resolve(), manifest.resolve())
        self.assertEqual(dedicated.read_text(encoding="utf-8"), "sentinel\n")
        self.assertFalse(manifest.exists())

    def test_verify_rejects_tampering_and_permissive_mode(self):
        temporary, source, dedicated, manifest, _ = self.capture()
        self.addCleanup(temporary.cleanup)
        dedicated.write_bytes(dedicated.read_bytes().replace(b"192.168.1.159", b"192.168.1.158", 1))
        with self.assertRaises(pins.PinError):
            pins.verify(dedicated.resolve(), manifest.resolve(), source.resolve())

        # Restore a clean artifact, then prove mode drift is independently fatal.
        dedicated.unlink()
        manifest.unlink()
        pins.capture(source.resolve(), dedicated.resolve(), manifest.resolve())
        dedicated.chmod(0o644)
        with self.assertRaisesRegex(pins.PinError, "mode must be exactly 0600"):
            pins.verify(dedicated.resolve(), manifest.resolve())

    def test_source_symlink_is_rejected(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        actual = root / "actual"
        source = root / "source-link"
        write_source(actual, valid_source_lines())
        source.symlink_to(actual)
        with self.assertRaisesRegex(pins.PinError, "regular file|symlink"):
            pins.capture(source, (root / "dedicated").resolve(), (root / "manifest").resolve())

    def test_capture_and_verify_never_use_network_or_subprocess(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        source = root / "source"
        dedicated = root / "dedicated"
        manifest = root / "manifest"
        write_source(source, valid_source_lines())
        forbidden = AssertionError("network or subprocess use is forbidden")
        with (
            mock.patch.object(socket, "socket", side_effect=forbidden),
            mock.patch.object(socket, "create_connection", side_effect=forbidden),
            mock.patch.object(subprocess, "Popen", side_effect=forbidden),
            mock.patch.object(subprocess, "run", side_effect=forbidden),
            mock.patch.object(subprocess, "check_call", side_effect=forbidden),
            mock.patch.object(subprocess, "check_output", side_effect=forbidden),
        ):
            pins.capture(source.resolve(), dedicated.resolve(), manifest.resolve())
            pins.verify(dedicated.resolve(), manifest.resolve(), source.resolve())


if __name__ == "__main__":
    unittest.main()
