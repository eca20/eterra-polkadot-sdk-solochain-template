#!/usr/bin/env python3

from __future__ import annotations

import dataclasses
import datetime as dt
import hashlib
import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock


SCRIPT = Path(__file__).with_name("acceptance_boundary.py")
SPEC = importlib.util.spec_from_file_location("acceptance_boundary_tested", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
tool = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = tool
SPEC.loader.exec_module(tool)


SOURCE = "a" * 40
RELEASE = "nexus-v2-acceptance-test"
BLOCK_HASH = "0x" + "b" * 64
GENESIS_HASH = "0x" + "c" * 64
WASM = b"frozen-production-wasm"
METADATA_SCALE = b"frozen-metadata-scale"


def sha(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def write_json(path: Path, value: object) -> None:
    path.write_bytes(tool.canonical_bytes(value))


def metadata_fixture() -> tool.Metadata:
    types = {
        1: {"def": {"primitive": "u64"}},
        2: {"def": {"primitive": "bool"}},
        3: {
            "def": {
                "variant": {
                    "variants": [
                        {"name": "Packs", "index": 0},
                        {"name": "Conversion", "index": 1},
                        {"name": "Ranked", "index": 2},
                        {"name": "MythicalAscension", "index": 3},
                    ]
                }
            }
        },
        4: {
            "def": {
                "variant": {
                    "variants": [
                        {"name": "TicketEarning", "index": 0},
                        {"name": "TicketTransfers", "index": 1},
                        {"name": "TicketRedemption", "index": 2},
                        {"name": "RandomVending", "index": 3},
                        {"name": "FeaturedVending", "index": 4},
                        {"name": "PackCreditRedemptionV2", "index": 5},
                    ]
                }
            }
        },
        5: {
            "def": {
                "variant": {
                    "variants": [
                        {"name": "Disabled", "index": 0},
                        {"name": "DeterministicPrivateAlpha", "index": 1},
                        {"name": "DrandQuicknet", "index": 2},
                    ]
                }
            }
        },
    }
    by_pallet: dict[str, list[dict[str, object]]] = {}
    for pallet, storage in set(tool.PLAIN_QUERIES.values()):
        type_id = 5 if storage == "CurrentMode" else 2 if storage in {
            "LegacyCreationSealedV16",
            "CryptographyReviewApproved",
        } else 1
        default = [1] if storage == "LegacyCreationSealedV16" else [0] * (8 if type_id == 1 else 1)
        by_pallet.setdefault(pallet, []).append(
            {"name": storage, "ty": {"Plain": type_id}, "default": default}
        )
    for pallet, storage, _ in tool.ENUM_MAP_QUERIES.values():
        key_type = 3 if storage == "V2FeatureEnabled" else 4
        default = [1] if storage == "PausedDomains" else [0]
        if not any(item["name"] == storage for item in by_pallet.setdefault(pallet, [])):
            by_pallet[pallet].append(
                {
                    "name": storage,
                    "ty": {"Map": {"hashers": ["Blake2_128Concat"], "key": key_type, "value": 2}},
                    "default": default,
                }
            )
    for pallet, storage in tool.PREFIX_QUERIES.values():
        if not any(item["name"] == storage for item in by_pallet.setdefault(pallet, [])):
            by_pallet[pallet].append(
                {
                    "name": storage,
                    "ty": {"Map": {"hashers": ["Blake2_128Concat"], "key": 1, "value": 1}},
                    "default": [0],
                }
            )
    pallets = {
        name: {
            "name": name,
            "index": index,
            "storage": {"prefix": name, "entries": entries},
        }
        for index, (name, entries) in enumerate(sorted(by_pallet.items()))
    }
    return tool.Metadata(value={}, pallets=pallets, types=types)


class FakeRpc:
    def __init__(self, metadata: tool.Metadata) -> None:
        self.metadata = metadata
        self.values: dict[str, str | None] = {
            tool.CODE_STORAGE_KEY: "0x" + WASM.hex(),
        }
        self.prefix_keys: dict[str, list[str]] = {}

    def call(self, method: str, params: list[object]) -> object:
        if method == "chain_getFinalizedHead":
            return BLOCK_HASH
        if method == "chain_getHeader":
            return {"number": "0x2a"}
        if method == "chain_getBlockHash":
            return GENESIS_HASH if params[0] == 0 else BLOCK_HASH
        if method == "state_getRuntimeVersion":
            return {"specVersion": 106, "transactionVersion": 7, "stateVersion": 1}
        if method == "state_getMetadata":
            return "0x" + METADATA_SCALE.hex()
        if method == "state_getStorage":
            return self.values.get(str(params[0]))
        raise AssertionError(method)

    def keys(self, prefix: str, block_hash: str) -> list[str]:
        self.assert_block(block_hash)
        return self.prefix_keys.get(prefix, [])

    def assert_block(self, block_hash: str) -> None:
        if block_hash != BLOCK_HASH:
            raise AssertionError("mixed block")


class BoundaryTests(unittest.TestCase):
    def setUp(self) -> None:
        self.metadata = metadata_fixture()
        self.pins = dataclasses.replace(
            tool.runtime_bundle.PRODUCTION_PINS,
            production_wasm_sha256=sha(WASM),
            metadata_scale_sha256=sha(METADATA_SCALE),
            metadata_json_sha256="d" * 64,
            manifest_sha256="e" * 64,
        )
        self.artifacts = tool.RuntimeArtifacts(
            metadata=self.metadata,
            metadata_scale=METADATA_SCALE,
            metadata_json=b"fixture-json",
            bundle_manifest_sha256=self.pins.manifest_sha256,
        )
        self.rpc = FakeRpc(self.metadata)
        self.patch = mock.patch.object(tool.runtime_bundle, "PRODUCTION_PINS", self.pins)
        self.patch.start()
        self.addCleanup(self.patch.stop)

    def capture(self, observed_at: str = "2026-07-31T12:00:00Z") -> dict[str, object]:
        capture = tool.collect_capture(
            self.rpc,
            self.artifacts,
            RELEASE,
            SOURCE,
            GENESIS_HASH,
            observed_at,
        )
        # The production loader, not collect_capture, verifies decoded JSON.
        capture["runtime"]["runtimeMetadataJsonSha256"] = self.pins.metadata_json_sha256
        return capture

    def test_twox_known_vector_and_zero_boundary(self) -> None:
        self.assertEqual(
            tool.storage_prefix("System", "Account").hex(),
            "26aa394eea5630e07c48ae0c9558cef7"
            "b99d880ec681799c0cf30e8886371da9",
        )
        capture = self.capture()
        identity = tool.validate_capture(capture, self.artifacts)
        self.assertEqual(identity["blockNumber"], 42)
        gates = tool.disabled_gates(capture, self.metadata)
        inventory = tool.acceptance_inventory(capture, self.metadata)
        self.assertFalse(any(inventory["counts"].values()))
        self.assertTrue(gates["tcg"]["legacyCreationSealed"])
        self.assertEqual(gates["randomness"]["mode"], "Disabled")

    def add_prefix_record(self, capture: dict[str, object], alias: str, suffix: str = "00") -> None:
        item = capture["storage"]["prefixes"][alias]
        key = item["prefix"] + suffix
        item["keys"] = [key]
        item["values"] = {key: "0x00"}

    def test_legacy_or_v2_game_write_permanently_blocks_restore(self) -> None:
        capture = self.capture()
        capture["storage"]["plain"]["gameAuthorityNextGameId"]["value"] = "0x0100000000000000"
        self.add_prefix_record(capture, "gameAuthorityGames")
        inventory = tool.acceptance_inventory(capture, self.metadata)
        self.assertEqual(inventory["counts"]["lifetimeLegacyAuthorityGamesCreated"], 1)
        self.assertEqual(inventory["counts"]["lifetimeLegacyAuthorityAcceptanceWritesLowerBound"], 1)
        capture["storage"]["plain"]["gameResultsNextSessionId"]["value"] = "0x0100000000000000"
        self.add_prefix_record(capture, "gameResultsSessions", "01")
        inventory = tool.acceptance_inventory(capture, self.metadata)
        self.assertEqual(inventory["counts"]["lifetimeV2SessionsAuthorized"], 1)
        self.assertEqual(inventory["counts"]["currentV2GameResultSessions"], 1)

    def test_unsafe_gate_and_hand_authored_artifact_fail(self) -> None:
        capture = self.capture()
        capture["storage"]["plain"]["randomnessCurrentMode"]["value"] = "0x01"
        with self.assertRaises(tool.BoundaryError):
            tool.disabled_gates(capture, self.metadata)

        capture = self.capture()
        with tempfile.TemporaryDirectory(prefix="acceptance-boundary-test-") as temporary:
            root = Path(temporary)
            capture_path = root / "capture.json"
            gates_path = root / "gates.json"
            inventory_path = root / "inventory.json"
            write_json(capture_path, capture)
            gates = tool.disabled_gates(capture, self.metadata)
            gates["tcg"]["features"]["Packs"] = True
            write_json(gates_path, gates)
            write_json(inventory_path, tool.acceptance_inventory(capture, self.metadata))
            with self.assertRaises(tool.BoundaryError):
                tool.derive_and_validate_artifacts(
                    capture_path,
                    gates_path,
                    inventory_path,
                    self.artifacts,
                )

    def test_receipt_verifier_rejects_scope_or_digest_drift(self) -> None:
        receipt = {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-acceptance-boundary-receipt",
            "releaseId": RELEASE,
            "sourceCommit": SOURCE,
            "genesisHash": GENESIS_HASH,
            "runtimeCodeSha256": self.pins.production_wasm_sha256,
            "runtimeMetadataScaleSha256": self.pins.metadata_scale_sha256,
            "observedAtFinalizedBlock": {"number": 42, "hash": BLOCK_HASH},
            "acceptanceBoundaryCaptureSha256": "1" * 64,
            "economicGatesSha256": "2" * 64,
            "acceptanceInventorySha256": "3" * 64,
            "postCutoverObservationSha256": "4" * 64,
            "coordinatorExecuteEvidenceSha256": "5" * 64,
            "coordinatorDecision": "keep-v2",
            "ingressClosedEvidenceSha256": "6" * 64,
            "ingressMode": "AllExternalWriteIngressClosed",
            "phase1SmokePassed": True,
            "automaticRestorePermanentlyDisabled": True,
            "operatorV2WriteScope": dict(tool.OPERATOR_SCOPE),
            "createdAtUtc": "2026-07-31T12:01:00Z",
        }
        with tempfile.TemporaryDirectory(prefix="acceptance-receipt-test-") as temporary:
            path = Path(temporary) / "receipt.json"
            write_json(path, receipt)
            digest = tool.sha256_file(path)
            tool.validate_receipt(
                path,
                digest,
                release_id=RELEASE,
                source_commit=SOURCE,
                genesis_hash=GENESIS_HASH,
                runtime_code_sha256=self.pins.production_wasm_sha256,
                runtime_metadata_scale_sha256=self.pins.metadata_scale_sha256,
            )
            receipt["operatorV2WriteScope"]["alphaAccessModeOpen"] = True
            write_json(Path(temporary) / "unsafe.json", receipt)
            with self.assertRaises(tool.BoundaryError):
                tool.validate_receipt(
                    Path(temporary) / "unsafe.json",
                    tool.sha256_file(Path(temporary) / "unsafe.json"),
                    release_id=RELEASE,
                    source_commit=SOURCE,
                    genesis_hash=GENESIS_HASH,
                    runtime_code_sha256=self.pins.production_wasm_sha256,
                    runtime_metadata_scale_sha256=self.pins.metadata_scale_sha256,
                )
            receipt["operatorV2WriteScope"] = dict(tool.OPERATOR_SCOPE)
            receipt["createdAtUtc"] = "2026-07-31T12:01:00+00:00"
            write_json(Path(temporary) / "noncanonical-time.json", receipt)
            with self.assertRaises(tool.BoundaryError):
                tool.validate_receipt(
                    Path(temporary) / "noncanonical-time.json",
                    tool.sha256_file(Path(temporary) / "noncanonical-time.json"),
                    release_id=RELEASE,
                    source_commit=SOURCE,
                    genesis_hash=GENESIS_HASH,
                    runtime_code_sha256=self.pins.production_wasm_sha256,
                    runtime_metadata_scale_sha256=self.pins.metadata_scale_sha256,
                )

    def test_create_receipt_requires_closed_execute_marker_and_is_immutable(self) -> None:
        now = dt.datetime.now(dt.timezone.utc).replace(microsecond=0)

        def timestamp(delta: int) -> str:
            return (now + dt.timedelta(seconds=delta)).isoformat().replace("+00:00", "Z")

        capture = self.capture(timestamp(-30))
        gates = tool.disabled_gates(capture, self.metadata)
        inventory = tool.acceptance_inventory(capture, self.metadata)
        with tempfile.TemporaryDirectory(prefix="acceptance-create-test-") as temporary:
            root = Path(temporary)
            paths = {
                name: root / f"{name}.json"
                for name in (
                    "capture",
                    "gates",
                    "inventory",
                    "ingress",
                    "observation",
                    "coordinator",
                    "marker",
                    "receipt",
                )
            }
            write_json(paths["capture"], capture)
            write_json(paths["gates"], gates)
            write_json(paths["inventory"], inventory)
            ingress = {
                "schemaVersion": 1,
                "kind": "nexus-v2-private-alpha-ingress-closed-evidence",
                "releaseId": RELEASE,
                "sourceCommit": SOURCE,
                "genesisHash": GENESIS_HASH,
                "observedAtFinalizedBlock": {"number": 42, "hash": BLOCK_HASH},
                "observedAtUtc": timestamp(-30),
                "mode": "AllExternalWriteIngressClosed",
                "components": {
                    "chain-media": {
                        "publicRpcWriteIngressClosed": True,
                        "authorityOperatorIngressClosed": True,
                        "gameplaySessionIngressClosed": True,
                        "componentEvidenceSha256": "7" * 64,
                    },
                    "site-indexer": {
                        "webMutationIngressClosed": True,
                        "indexerMutationIngressClosed": True,
                        "componentEvidenceSha256": "8" * 64,
                    },
                },
                "blockProductionContinues": True,
                "paidOrPublicActivationAuthorized": False,
            }
            write_json(paths["ingress"], ingress)
            ingress_hash = tool.sha256_file(paths["ingress"])
            component_commits = {
                "chain-media": {"chain": SOURCE, "media": "b" * 40},
                "site-indexer": {"chain": SOURCE, "site": "c" * 40},
            }
            observation = {
                "schemaVersion": 1,
                "kind": "nexus-v2-private-alpha-post-cutover-rollback-observation",
                "releaseId": RELEASE,
                "sourceCommit": SOURCE,
                "componentSourceCommits": component_commits,
                "observedAtFinalizedBlock": {"number": 42, "hash": BLOCK_HASH},
                "observedAtUtc": timestamp(-30),
                "writeBarrier": {
                    "mode": "AllV2WritesPaused",
                    "chainWritesPaused": True,
                    "authorityResultsPaused": True,
                    "webMutationsPaused": True,
                    "gameplaySessionIngressPaused": True,
                    "inventoryObservedAfterPause": True,
                    "pausedAtUtc": timestamp(-60),
                    "stabilityWindowSeconds": 30,
                    "evidenceSha256": ingress_hash,
                },
                "acceptanceBoundaryCaptureSha256": tool.sha256_file(paths["capture"]),
                "ingressClosedEvidenceSha256": ingress_hash,
                "economicGatesSha256": tool.sha256_file(paths["gates"]),
                "acceptanceInventorySha256": tool.sha256_file(paths["inventory"]),
            }
            write_json(paths["observation"], observation)
            coordinator = {
                "schemaVersion": 1,
                "kind": "nexus-v2-private-alpha-post-cutover-coordinator-evidence",
                "operationId": "acceptance-create-test",
                "planSha256": "1" * 64,
                "releaseId": RELEASE,
                "sourceCommit": SOURCE,
                "genesisHash": GENESIS_HASH,
                "runtimeCodeSha256": self.pins.production_wasm_sha256,
                "runtimeMetadataScaleSha256": self.pins.metadata_scale_sha256,
                "componentSourceCommits": component_commits,
                "decision": "keep-v2",
                "postCutoverSmokePassed": True,
                "automaticRestorePerformed": False,
                "postAcceptanceContainmentPerformed": False,
                "finalBackupManifestSha256": "2" * 64,
                "restoreEvidenceSha256": "3" * 64,
                "postCutoverObservationSha256": tool.sha256_file(paths["observation"]),
                "acceptanceBoundaryCaptureSha256": tool.sha256_file(paths["capture"]),
                "ingressClosedEvidenceSha256": ingress_hash,
                "economicGatesSha256": tool.sha256_file(paths["gates"]),
                "acceptanceInventorySha256": tool.sha256_file(paths["inventory"]),
                "observedAtFinalizedBlock": {"number": 42, "hash": BLOCK_HASH},
                "nonzeroAcceptanceAssets": {},
                "componentMarkerSha256": {
                    "chain-media.post-cutover-smoke.execute.json": "4" * 64,
                    "site-indexer.post-cutover-smoke.execute.json": "5" * 64,
                },
                "completedAtUtc": timestamp(-2),
            }
            write_json(paths["coordinator"], coordinator)
            coordinator_hash = tool.sha256_file(paths["coordinator"])
            marker = {
                "schemaVersion": 1,
                "kind": "nexus-v2-private-alpha-post-cutover-final-marker",
                "operationId": coordinator["operationId"],
                "planSha256": coordinator["planSha256"],
                "evidencePath": str(paths["coordinator"].resolve()),
                "evidenceSha256": coordinator_hash,
                "completedAtUtc": timestamp(-1),
            }
            write_json(paths["marker"], marker)
            args = SimpleNamespace(
                runtime_bundle_root=str(root / "unused-runtime-bundle"),
                runtime_bundle_manifest_sha256=self.pins.manifest_sha256,
                capture=str(paths["capture"]),
                economic_gates=str(paths["gates"]),
                acceptance_inventory=str(paths["inventory"]),
                release_id=RELEASE,
                source_commit=SOURCE,
                genesis_hash=GENESIS_HASH,
                observation=str(paths["observation"]),
                ingress_closed_evidence=str(paths["ingress"]),
                ingress_closed_evidence_sha256=ingress_hash,
                coordinator_evidence=str(paths["coordinator"]),
                coordinator_evidence_sha256=coordinator_hash,
                coordinator_final_marker=str(paths["marker"]),
                coordinator_final_marker_sha256=tool.sha256_file(paths["marker"]),
                created_at=timestamp(0),
                output=str(paths["receipt"]),
            )
            with mock.patch.object(tool, "load_runtime_artifacts", return_value=self.artifacts):
                tool.command_create_receipt(args)
                with self.assertRaises(tool.BoundaryError):
                    tool.command_create_receipt(args)
            digest = tool.sha256_file(paths["receipt"])
            tool.validate_receipt(
                paths["receipt"],
                digest,
                release_id=RELEASE,
                source_commit=SOURCE,
                genesis_hash=GENESIS_HASH,
                runtime_code_sha256=self.pins.production_wasm_sha256,
                runtime_metadata_scale_sha256=self.pins.metadata_scale_sha256,
            )
            self.assertEqual(paths["receipt"].stat().st_mode & 0o777, 0o440)

            marker["evidenceSha256"] = "9" * 64
            write_json(root / "bad-marker.json", marker)
            args.coordinator_final_marker = str(root / "bad-marker.json")
            args.coordinator_final_marker_sha256 = tool.sha256_file(root / "bad-marker.json")
            args.output = str(root / "bad-receipt.json")
            with mock.patch.object(tool, "load_runtime_artifacts", return_value=self.artifacts):
                with self.assertRaises(tool.BoundaryError):
                    tool.command_create_receipt(args)


if __name__ == "__main__":
    unittest.main()
