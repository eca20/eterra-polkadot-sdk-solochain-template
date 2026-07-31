#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import frozen_snapshot_proof as tool


class FrozenSnapshotProofTests(unittest.TestCase):
    def setUp(self) -> None:
        self.context = tempfile.TemporaryDirectory(prefix="nexus-v2-frozen-snapshot-proof-test-")
        self.root = Path(self.context.name)
        for name, payload in {
            "snapshot": b"snapshot",
            "node-data.tar.gz": b"node-data",
            "node-binary": b"node",
            "chain-spec.json": b"{}\n",
            "try-runtime": b"try-runtime",
            "try-runtime.log": b"created snapshot\n",
        }.items():
            (self.root / name).write_bytes(payload)
        block_hash = "0x" + "1" * 64
        capture = {
            "finalizedHead": {"id": 1, "jsonrpc": "2.0", "result": block_hash},
            "blockHashAtNumber": {"id": 2, "jsonrpc": "2.0", "result": block_hash},
            "header": {"id": 3, "jsonrpc": "2.0", "result": {"number": "0x2a"}},
            "genesisHash": {"id": 4, "jsonrpc": "2.0", "result": "0x" + "2" * 64},
            "runtimeCodeHash": {"id": 5, "jsonrpc": "2.0", "result": "0x" + "3" * 64},
            "runtimeVersion": {"id": 6, "jsonrpc": "2.0", "result": {"specVersion": 1}},
        }
        (self.root / "rpc.json").write_text(json.dumps(capture), encoding="utf-8")
        self.args = argparse.Namespace(
            transaction_id="freeze-test",
            release_id="release-test",
            source_commit="a" * 40,
            frozen_at_utc="2026-07-31T12:00:00Z",
            created_at_utc="2026-07-31T12:01:00Z",
            block_number=42,
            block_hash=block_hash,
            snapshot=str(self.root / "snapshot"),
            node_data_archive=str(self.root / "node-data.tar.gz"),
            node_binary=str(self.root / "node-binary"),
            chain_spec=str(self.root / "chain-spec.json"),
            try_runtime=str(self.root / "try-runtime"),
            try_runtime_revision="b" * 40,
            try_runtime_version="try-runtime 0.42.0",
            try_runtime_log=str(self.root / "try-runtime.log"),
            rpc_capture=str(self.root / "rpc.json"),
            output=str(self.root / "proof.json"),
        )

    def tearDown(self) -> None:
        self.context.cleanup()

    def create(self) -> dict[str, object]:
        return tool.create(self.args)

    def test_binds_snapshot_to_exact_frozen_archive_and_block(self) -> None:
        result = self.create()
        self.assertEqual(result["frozenFinalizedBlock"], {"number": 42, "hash": self.args.block_hash})
        self.assertEqual(result["snapshotSha256"], tool.sha256_file(self.root / "snapshot"))
        self.assertEqual(result["nodeDataArchiveSha256"], tool.sha256_file(self.root / "node-data.tar.gz"))

    def test_rejects_isolated_head_different_from_marker(self) -> None:
        capture = json.loads((self.root / "rpc.json").read_text(encoding="utf-8"))
        capture["finalizedHead"]["result"] = "0x" + "f" * 64
        (self.root / "rpc.json").write_text(json.dumps(capture), encoding="utf-8")
        with self.assertRaisesRegex(tool.ProofError, "finalized head differs"):
            self.create()

    def test_rejects_snapshot_tampering(self) -> None:
        self.create()
        (self.root / "snapshot").write_bytes(b"tampered")
        with self.assertRaisesRegex(tool.ProofError, "snapshot hash mismatch"):
            tool.verify(
                self.root / "proof.json",
                self.root / "snapshot",
                self.root / "node-data.tar.gz",
                self.root / "node-binary",
                self.root / "chain-spec.json",
                42,
                self.args.block_hash,
            )

    def test_rejects_source_archive_tampering(self) -> None:
        self.create()
        (self.root / "node-data.tar.gz").write_bytes(b"different")
        with self.assertRaisesRegex(tool.ProofError, "node-data archive hash mismatch"):
            tool.verify(
                self.root / "proof.json",
                self.root / "snapshot",
                self.root / "node-data.tar.gz",
                self.root / "node-binary",
                self.root / "chain-spec.json",
                42,
                self.args.block_hash,
            )


if __name__ == "__main__":
    unittest.main()
