#!/usr/bin/env python3

from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location(
    "verify_reset_readiness",
    SCRIPT_DIR / "verify_reset_readiness.py",
)
assert SPEC is not None and SPEC.loader is not None
tool = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(tool)


def readiness() -> dict[str, object]:
    return {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-reset-readiness",
        "releaseId": "nexus-v2-private-alpha-test",
        "sourceCommit": "a" * 40,
        "backupManifestSha256": "b" * 64,
        "restoreEvidenceSha256": "c" * 64,
        "migrationEvidenceSha256": "d" * 64,
        "economicGatesSha256": "e" * 64,
        "acceptanceInventorySha256": "f" * 64,
        "economicGateMode": "pre-v16-fresh-reset-frozen",
        "resetMode": "fresh-genesis-replacement",
        "freshGenesisReplacementOnly": True,
        "inPlaceRuntimeActivationAuthorized": False,
        "gateFinalizedBlock": {"number": 126514, "hash": "0x" + "1" * 64},
        "readyForSeparateOperatorResetAuthorization": True,
        "automaticRollbackEligibleAtGateBlock": True,
        "economicFlagsDisabled": True,
        "v2AcceptanceAssetsExist": False,
        "resetExecuted": False,
        "deployExecuted": False,
        "createdAtUtc": "2026-07-30T12:00:00Z",
    }


class FreshResetReadinessTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="nexus-v2-readiness-test-")
        self.root = Path(self.temp.name)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def write(self, name: str, value: dict[str, object]) -> tuple[Path, str]:
        path = self.root / name
        payload = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()
        path.write_bytes(payload)
        return path, hashlib.sha256(payload).hexdigest()

    def test_accepts_only_hash_pinned_unexecuted_pre_v16_fresh_genesis_packet(self) -> None:
        path, digest = self.write("ready.json", readiness())
        summary = tool.validate_packet(path, digest)
        self.assertEqual(summary["sha256"], digest)
        self.assertEqual(summary["resetMode"], "fresh-genesis-replacement")

    def test_rejects_wrong_hash_post_v16_assets_and_execution(self) -> None:
        path, digest = self.write("ready.json", readiness())
        with self.assertRaises(tool.ReadinessError):
            tool.validate_packet(path, "0" * 64)

        mutations = [
            ("schemaVersion", True),
            ("economicGateMode", "post-v16-disabled"),
            ("freshGenesisReplacementOnly", False),
            ("economicFlagsDisabled", False),
            ("v2AcceptanceAssetsExist", True),
            ("resetExecuted", True),
            ("deployExecuted", True),
        ]
        for index, (field, value) in enumerate(mutations):
            with self.subTest(field=field):
                unsafe = copy.deepcopy(readiness())
                unsafe[field] = value
                unsafe_path, unsafe_digest = self.write(f"unsafe-{index}.json", unsafe)
                with self.assertRaises(tool.ReadinessError):
                    tool.validate_packet(unsafe_path, unsafe_digest)

    def test_rejects_symlink_and_unknown_fields(self) -> None:
        path, digest = self.write("ready.json", readiness())
        link = self.root / "ready-link.json"
        link.symlink_to(path)
        with self.assertRaises(tool.ReadinessError):
            tool.validate_packet(link, digest)

        extra = readiness()
        extra["unreviewedResetScope"] = True
        extra_path, extra_digest = self.write("extra.json", extra)
        with self.assertRaises(tool.ReadinessError):
            tool.validate_packet(extra_path, extra_digest)

        duplicate_payload = (
            '{"schemaVersion":1,' + json.dumps(readiness(), sort_keys=True)[1:]
        ).encode()
        duplicate_path = self.root / "duplicate.json"
        duplicate_path.write_bytes(duplicate_payload)
        with self.assertRaises(tool.ReadinessError):
            tool.validate_packet(
                duplicate_path,
                hashlib.sha256(duplicate_payload).hexdigest(),
            )

    def test_deploy_stage_binds_packet_to_exact_chain_source_commit(self) -> None:
        packet, digest = self.write("ready.json", readiness())
        staged = self.root / "staged.json"
        deploy_lib = SCRIPT_DIR.parents[1] / "deploy/alpha/macmini2010/lib.sh"
        command = """
source "$1"
ETERRA_RELEASE_VERSION=v-test
NEXUS_V2_LOCAL_ONLY_RELEASE=1
NEXUS_V2_RESET_READINESS_SHA256="$2"
CHAIN_SOURCE_COMMIT="$3"
stage_fresh_reset_readiness "$4" "$5"
"""

        rejected = subprocess.run(
            [
                "bash",
                "-c",
                command,
                "_",
                str(deploy_lib),
                digest,
                "c" * 40,
                str(packet),
                str(staged),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn(
            "fresh-reset readiness chain source commit does not match",
            rejected.stderr,
        )

        accepted = subprocess.run(
            [
                "bash",
                "-c",
                command,
                "_",
                str(deploy_lib),
                digest,
                "a" * 40,
                str(packet),
                str(staged),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(accepted.returncode, 0, accepted.stderr)


if __name__ == "__main__":
    unittest.main()
