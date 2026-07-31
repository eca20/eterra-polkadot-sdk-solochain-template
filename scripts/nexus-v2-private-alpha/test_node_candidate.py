#!/usr/bin/env python3

from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from typing import Any


sys.path.insert(0, str(Path(__file__).resolve().parent))
import node_candidate as tool  # noqa: E402


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def public_overrides() -> dict[str, Any]:
    accounts = [f"5AlphaPublicAddress{index:02d}" for index in range(1, 8)]
    return {
        "name": "Eterra Alpha",
        "bootnodes": [],
        "aura_authorities": [accounts[0]],
        "grandpa_authorities": [[accounts[1], 1]],
        "balances": [[account, 1_000_000] for account in accounts],
        "sudo_key": accounts[2],
        "faucet_account": accounts[3],
        "faucet_payout_amount": 100,
        "initial_servers": [accounts[4]],
        "season_admins": [accounts[5]],
        "media_collection_owner": accounts[6],
        "council_members": [accounts[2]],
        "asset_owner": accounts[2],
    }


class NodeCandidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.context = tempfile.TemporaryDirectory(prefix="nexus-v2-node-candidate-test-")
        self.root = Path(self.context.name)

    def tearDown(self) -> None:
        self.context.cleanup()

    def make_candidate(self) -> Path:
        code = b"private-alpha-runtime"
        raw = {
            "id": "eterra_alpha",
            "chainType": "Live",
            "genesis": {"raw": {"top": {tool.CODE_STORAGE_KEY: "0x" + code.hex()}}},
        }
        plain = {"id": "eterra_alpha", "chainType": "Live"}
        write_json(self.root / "alpha-raw.json", raw)
        write_json(self.root / "alpha-plain.json", plain)
        write_json(self.root / "alpha-public-overrides.json", public_overrides())
        (self.root / "solochain-eterra-node").write_bytes(b"\x7fELFfixture")
        (self.root / "solochain-eterra-node").chmod(0o700)
        (self.root / "start-alpha-node.sh").write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        (self.root / "start-alpha-node.sh").chmod(0o700)
        (self.root / "eterra-alpha-node.service").write_text(
            "[Service]\nExecStart=/opt/eterra-alpha/node/current/start-alpha-node.sh\n",
            encoding="utf-8",
        )
        artifacts = {
            name: tool.sha256_file(self.root / name) for name in sorted(tool.CANDIDATE_FILES)
        }
        runtime_sha = hashlib.sha256(code).hexdigest()
        manifest = {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-node-candidate",
            "releaseId": "nexus-v2-test",
            "deploymentSourceCommit": "a" * 40,
            "runtimeSourceCommit": "b" * 40,
            "targetSpecVersion": 106,
            "sourceDateEpoch": 1,
            "createdAtUtc": "1970-01-01T00:00:01Z",
            "runtimeBundle": {
                "manifestSha256": "c" * 64,
                "sha256SumsSha256": "d" * 64,
                "productionWasmSha256": runtime_sha,
                "metadataScaleSha256": "2" * 64,
                "metadataVersion": 15,
            },
            "alpha": {
                "id": "eterra_alpha",
                "chainType": "Live",
                "chainName": "Eterra Alpha",
                "genesisHash": "0x" + "1" * 64,
                "runtimeCodeHash": "0x" + hashlib.blake2b(code, digest_size=32).hexdigest(),
                "runtimeCodeSha256": runtime_sha,
                "deterministicRepeatMatched": True,
                "privateOverridesAreAddressOnly": True,
            },
            "artifacts": artifacts,
            "builder": {
                "nodeCandidateToolSha256": "e" * 64,
                "finalizeAlphaToolSha256": "f" * 64,
                "verifyAlphaToolSha256": "0" * 64,
            },
            "containsSecrets": False,
            "remoteBuildAllowed": False,
            "publicDeployAllowed": False,
            "paidProductionAllowed": False,
        }
        manifest_path = self.root / "node-candidate.json"
        write_json(manifest_path, manifest)
        return manifest_path

    def test_verifies_closed_hash_pinned_candidate(self) -> None:
        summary = tool.verify_candidate(self.make_candidate())
        self.assertEqual(summary["releaseId"], "nexus-v2-test")
        self.assertEqual(summary["runtimeSourceCommit"], "b" * 40)
        self.assertFalse(summary["containsSecrets"])

    def test_rejects_artifact_tampering(self) -> None:
        manifest = self.make_candidate()
        (self.root / "alpha-raw.json").write_text("{}\n", encoding="utf-8")
        with self.assertRaisesRegex(tool.CandidateError, "artifact hash mismatch"):
            tool.verify_candidate(manifest)

    def test_rejects_extra_candidate_file(self) -> None:
        manifest = self.make_candidate()
        (self.root / "unreviewed.bin").write_bytes(b"unexpected")
        with self.assertRaisesRegex(tool.CandidateError, "closed file set"):
            tool.verify_candidate(manifest)

    def test_rejects_secret_uri_in_public_overrides(self) -> None:
        path = self.root / "overrides.json"
        value = public_overrides()
        value["sudo_key"] = "//AlphaOwner"
        write_json(path, value)
        with self.assertRaisesRegex(tool.CandidateError, "public address"):
            tool.validate_public_overrides(path)

    def test_rejects_nested_secret_uri_in_public_overrides(self) -> None:
        path = self.root / "nested-overrides.json"
        value = public_overrides()
        value["grandpa_authorities"][0][0] = "//Grandpa"
        write_json(path, value)
        with self.assertRaisesRegex(tool.CandidateError, "public address"):
            tool.validate_public_overrides(path)

    def test_target_identity_is_closed_and_candidate_bound(self) -> None:
        candidate = self.make_candidate()
        summary = tool.verify_candidate(candidate)
        candidate_value = json.loads(candidate.read_text())
        identity = {
            "schemaVersion": 1,
            "kind": "eterra-spec106-target-identity.v1",
            "releaseId": summary["releaseId"],
            "network": "private-alpha",
            "genesisHash": summary["genesisHash"],
            "runtimeCodeHash": summary["runtimeCodeHash"],
            "runtimeSourceCommit": summary["runtimeSourceCommit"],
            "deploymentSourceCommit": summary["deploymentSourceCommit"],
            "runtimeMetadata": {
                "scaleSha256": candidate_value["runtimeBundle"]["metadataScaleSha256"],
                "version": 15,
            },
            "specVersion": 106,
            "tcgStorageVersion": 16,
            "nodeCandidateManifestSha256": summary["manifestSha256"],
            "authorizations": {
                "privateAlphaOnly": True,
                "publicProduction": False,
                "paidProduction": False,
            },
        }
        path = self.root.parent / f"{self.root.name}-target.json"
        write_json(path, identity)
        try:
            verified = tool.verify_target_identity(path, candidate)
            self.assertEqual(verified["metadataVersion"], 15)
            self.assertFalse(verified["paidProduction"])
        finally:
            path.unlink(missing_ok=True)

    def test_genesis_probe_binds_p2p_to_loopback(self) -> None:
        class ProbeProcess:
            def poll(self) -> None:
                return None

            def terminate(self) -> None:
                return None

            def wait(self, timeout: int) -> int:
                return 0

        captured: list[str] = []
        original_popen = subprocess.Popen
        original_rpc = tool.rpc_request
        try:
            def fake_popen(command: list[str], **_: Any) -> ProbeProcess:
                captured.extend(command)
                return ProbeProcess()

            responses = {
                "state_getRuntimeVersion": {"specVersion": tool.TARGET_SPEC_VERSION},
                "chain_getBlockHash": "0x" + "1" * 64,
                "system_chain": "Eterra Alpha",
            }
            subprocess.Popen = fake_popen  # type: ignore[assignment]
            tool.rpc_request = lambda _port, method, _params: responses[method]  # type: ignore[assignment]
            result = tool.inspect_genesis(
                self.root / "node",
                self.root / "raw.json",
                19946,
                31346,
            )
        finally:
            subprocess.Popen = original_popen  # type: ignore[assignment]
            tool.rpc_request = original_rpc  # type: ignore[assignment]
        self.assertEqual(result["chainName"], "Eterra Alpha")
        listen_index = captured.index("--listen-addr")
        self.assertEqual(captured[listen_index + 1], "/ip4/127.0.0.1/tcp/31346")


if __name__ == "__main__":
    unittest.main()
