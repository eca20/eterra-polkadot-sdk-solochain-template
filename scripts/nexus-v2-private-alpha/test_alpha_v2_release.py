#!/usr/bin/env python3

from __future__ import annotations

import copy
import datetime as dt
import hashlib
import json
import os
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path
from typing import Any


sys.path.insert(0, str(Path(__file__).resolve().parent))
import alpha_v2_release as tool  # noqa: E402


RELEASE_ID = "nexus-v2-private-alpha-test"
SOURCE_COMMIT = "a" * 40
DEPLOYED_SOURCE_COMMIT = "b" * 40
BLOCK_HASH = "0x" + ("1" * 64)
CREATED_AT = "2026-07-30T12:00:00Z"


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def file_hash(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def economic_gates(block_number: int = 100, block_hash: str = BLOCK_HASH) -> dict[str, Any]:
    return {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-economic-gates",
        "releaseId": RELEASE_ID,
        "sourceCommit": SOURCE_COMMIT,
        "observedAtFinalizedBlock": {"number": block_number, "hash": block_hash},
        "tcg": {
            "features": {
                "Packs": False,
                "Conversion": False,
                "Ranked": False,
                "MythicalAscension": False,
            },
            "legacyCreationSealed": True,
        },
        "randomness": {
            "mode": "Disabled",
            "privateAlphaSeedRecorded": False,
            "cryptographyReviewApproved": False,
            "drandQuicknetEnabled": False,
            "productionEconomicUseAllowed": False,
        },
        "gameResults": {
            "activeProductionPolicyCount": 0,
            "allAlphaPoliciesPracticeOnlyOrValuelessTraining": True,
        },
        "issuance": {
            "trainingPackCreditRejectsProduction": True,
            "paidV2IssuanceCallAvailable": False,
        },
        "reforge": {"dispatchableAvailable": False},
        "magic": {"seedTrainingOnly": True, "productionTransferEnabled": False},
        "legacyEconomy": {
            "marketplaceEnabled": False,
            "purchaseEnabled": False,
            "faucetEnabled": False,
            "economicWritesEnabled": False,
        },
        "arcadeTickets": {
            "earningEnabled": False,
            "transferEnabled": False,
            "redemptionEnabled": False,
            "randomVendingEnabled": False,
            "featuredVendingEnabled": False,
        },
        "additionalEconomicFlags": {
            "legacyPackMint": False,
            "nested": {"futureEconomicSurface": False},
        },
    }


def pre_v16_fresh_reset_gates(
    runtime_v14_wasm_sha256: str,
    tcg_observation_sha256: str,
    block_number: int = 100,
    block_hash: str = BLOCK_HASH,
) -> dict[str, Any]:
    return {
        "schemaVersion": 1,
        "kind": tool.PRE_V16_FRESH_RESET_GATE_KIND,
        "releaseId": RELEASE_ID,
        "sourceCommit": SOURCE_COMMIT,
        "observedAtFinalizedBlock": {"number": block_number, "hash": block_hash},
        "operationScope": {
            "freshGenesisReplacementOnly": True,
            "inPlaceRuntimeUpgradeAllowed": False,
            "v2ActivationAllowed": False,
            "paidOrPublicActivationAllowed": False,
        },
        "sourceRuntime": {
            "deployedSourceCommit": DEPLOYED_SOURCE_COMMIT,
            "specVersion": 1,
            "metadataVersion": 14,
            "tcgPalletIndex": 9,
            "tcgStorageVersion": 14,
            "flowPalletIndex": 29,
            "runtimeV14WasmSha256": runtime_v14_wasm_sha256,
            "runtimeMetadataScaleSha256": "c" * 64,
            "tcgStorageVersionObservationSha256": tcg_observation_sha256,
        },
        "v2StructuralAbsence": {
            "absentPallets": tool.PRE_V16_ABSENT_V2_PALLETS,
            "absentPalletIndices": tool.PRE_V16_ABSENT_V2_PALLET_INDICES,
            "tcgV2StoragePresent": False,
            "tcgV2DispatchablesPresent": False,
            "v2EventsPresent": False,
        },
        "knownLegacyEconomicSurfaces": {
            "tcgPaidMintDispatchablesPresent": True,
            "tcgMarketplaceDispatchablesPresent": True,
            "faucetDispatchablePresent": True,
            "economyDispatchablesPresent": True,
            "arcadePayContinueDispatchablePresent": True,
            "reachableThroughWriteIngress": False,
        },
        "legacyWriteBarrier": {
            "mode": "AllIngressStopped",
            "nodeServiceStopped": True,
            "authorityServiceStopped": True,
            "publicRpcWriteIngressStopped": True,
            "p2pIngressStopped": True,
            "blockProductionStopped": True,
            "offlineFinalizedHeadMatchesGateBlock": True,
            "inventoryCapturedAfterWriteStop": True,
            "stoppedAtUtc": CREATED_AT,
            "stabilityWindowSeconds": 60,
            "writeBarrierEvidenceSha256": "d" * 64,
        },
        "externalReviewFlags": {
            "cryptographyApproved": False,
            "paidFeaturesApproved": False,
            "publicProductionApproved": False,
        },
        "additionalEconomicFlags": {
            "legacyStorefrontIngressReachable": False,
            "legacyFaucetIngressReachable": False,
        },
    }


def acceptance_inventory(
    block_number: int = 100,
    block_hash: str = BLOCK_HASH,
    **overrides: int,
) -> dict[str, Any]:
    counts = {name: 0 for name in tool.ACCEPTANCE_COUNT_FIELDS}
    counts.update(overrides)
    return {
        "schemaVersion": 1,
        "kind": "nexus-v2-acceptance-inventory",
        "releaseId": RELEASE_ID,
        "sourceCommit": SOURCE_COMMIT,
        "observedAtFinalizedBlock": {"number": block_number, "hash": block_hash},
        "counts": counts,
    }


def tcg_storage_version_observation(
    block_number: int = 100,
    block_hash: str = BLOCK_HASH,
) -> dict[str, Any]:
    return {
        "schemaVersion": 1,
        "kind": "frame-pallet-storage-version-observation",
        "finalizedBlock": {"number": block_number, "hash": block_hash},
        "decoded": {"scaleType": "u16", "storageVersion": 14},
        "readOnlyRpc": {
            "method": "state_getStorage",
            "storageKey": "0x" + ("4" * 64),
            "result": "0x0e00",
        },
        "liveSource": {
            "commit": DEPLOYED_SOURCE_COMMIT,
            "declaredStorageVersion": 14,
        },
    }


class ReleaseSafetyTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp_context = tempfile.TemporaryDirectory(prefix="nexus-v2-release-test-")
        self.root = Path(self.temp_context.name)

    def tearDown(self) -> None:
        self.temp_context.cleanup()

    def make_bundle(
        self,
        *,
        gates_value: dict[str, Any] | None = None,
        artifact_payloads: dict[tuple[str, str], bytes | dict[str, Any]] | None = None,
    ) -> tuple[Path, Path]:
        bundle = self.root / "bundle"
        bundle.mkdir()
        artifact_payloads = artifact_payloads or {}
        artifact_args: list[str] = []
        for group, names in sorted(tool.REQUIRED_ARTIFACTS.items()):
            for name in sorted(names):
                relative = Path("artifacts") / group / f"{name}.bin"
                path = bundle / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                if (group, name) == ("config", "economic-gates"):
                    write_json(path, gates_value or economic_gates())
                elif (group, name) in artifact_payloads:
                    payload = artifact_payloads[(group, name)]
                    if isinstance(payload, dict):
                        write_json(path, payload)
                    else:
                        path.write_bytes(payload)
                else:
                    path.write_bytes(f"{group}:{name}\n".encode("utf-8"))
                artifact_args.extend(["--artifact", f"{group}:{name}:{relative.as_posix()}"])

        manifest = bundle / "backup-manifest.json"
        result = tool.main(
            [
                "backup-manifest",
                "--bundle-root",
                str(bundle),
                "--release-id",
                RELEASE_ID,
                "--source-commit",
                SOURCE_COMMIT,
                "--created-at",
                CREATED_AT,
                *artifact_args,
                "--output",
                str(manifest),
            ]
        )
        self.assertEqual(result, 0)
        return bundle, manifest

    def make_ports(self, *, overlap_live: bool = False) -> Path:
        ports = {
            "schemaVersion": 1,
            "bindHost": "127.0.0.1",
            "ports": {
                "nodeRpc": 9944 if overlap_live else 19944,
                "nodeP2p": 31333,
                "media": 14000,
                "ipfsApi": 15001,
                "ipfsGateway": 18080,
                "indexer": 18788,
            },
            "livePorts": {
                "nodeRpc": 9944,
                "nodeP2p": 30333,
                "media": 4000,
                "ipfsApi": 5001,
                "ipfsGateway": 8080,
                "indexer": 8788,
            },
        }
        path = self.root / ("ports-overlap.json" if overlap_live else "ports.json")
        write_json(path, ports)
        return path

    def make_executable(self, name: str, body: str) -> Path:
        path = self.root / name
        path.write_text("#!/usr/bin/env python3\n" + textwrap.dedent(body), encoding="utf-8")
        path.chmod(0o700)
        return path

    def make_restore_driver(self) -> Path:
        checks = sorted(tool.REQUIRED_RESTORE_CHECKS)
        groups = sorted(tool.REQUIRED_ARTIFACTS)
        return self.make_executable(
            "restore-driver",
            f"""
            import argparse, hashlib, json
            from pathlib import Path
            p = argparse.ArgumentParser()
            for name in ("manifest", "bundle-root", "isolation-root", "bind-host", "ports-json", "result"):
                p.add_argument("--" + name, required=True)
            a = p.parse_args()
            manifest_path = Path(a.manifest)
            manifest = json.loads(manifest_path.read_text())
            ports = json.loads(Path(a.ports_json).read_text())
            value = {{
                "schemaVersion": 1,
                "kind": "nexus-v2-isolated-restore-result",
                "releaseId": manifest["releaseId"],
                "sourceCommit": manifest["sourceCommit"],
                "mode": "isolated",
                "bindHost": a.bind_host,
                "ports": ports["ports"],
                "liveAlphaTouched": False,
                "backupManifestSha256": hashlib.sha256(manifest_path.read_bytes()).hexdigest(),
                "restoredArtifactGroups": {groups!r},
                "checks": {{name: True for name in {checks!r}}},
            }}
            Path(a.result).write_text(json.dumps(value))
            print("mock isolated restore completed")
            """,
        )

    def make_try_runtime(self) -> Path:
        return self.make_executable(
            "try-runtime",
            """
            import sys
            if sys.argv[1:] == ["--version"]:
                print("try-runtime 0.42.0-test")
            else:
                print("ETERRA_V16_MIGRATION_AWAITING_VERIFICATION copied-state fast-forward passed")
            """,
        )

    def make_migration_verifier(self, *, unsafe_check: str | None = None) -> Path:
        checks = {name: True for name in tool.REQUIRED_MIGRATION_CHECKS}
        if unsafe_check:
            checks[unsafe_check] = False
        return self.make_executable(
            "migration-verifier-" + (unsafe_check or "safe"),
            f"""
            import argparse, hashlib, json
            from pathlib import Path
            p = argparse.ArgumentParser()
            for name in ("manifest", "snapshot", "runtime-wasm", "try-runtime-log", "result"):
                p.add_argument("--" + name, required=True)
            a = p.parse_args()
            manifest = json.loads(Path(a.manifest).read_text())
            sha = lambda name: hashlib.sha256(Path(name).read_bytes()).hexdigest()
            value = {{
                "schemaVersion": 1,
                "kind": "nexus-v2-v14-v16-migration-result",
                "releaseId": manifest["releaseId"],
                "sourceCommit": manifest["sourceCommit"],
                "snapshotSha256": sha(a.snapshot),
                "runtimeWasmSha256": sha(a.runtime_wasm),
                "tryRuntimeLogSha256": sha(a.try_runtime_log),
                "fromStorageVersion": 14,
                "toStorageVersion": 16,
                "migrationPhase": "Completed",
                "legacyCreationSealed": True,
                "legacyWritesPaused": False,
                "v2Features": {{
                    "Packs": False,
                    "Conversion": False,
                    "Ranked": False,
                    "MythicalAscension": False,
                }},
                "checks": {checks!r},
                "counts": {{
                    "legacyCardsBefore": 4,
                    "legacyCardsAfter": 4,
                    "cardsSeen": 4,
                    "ordinary": 1,
                    "nftWrapped": 1,
                    "knownEscrow": 1,
                    "anomalies": 1,
                    "nextCardId": 11,
                    "maxCardIdSeen": 10,
                }},
            }}
            Path(a.result).write_text(json.dumps(value))
            print("mock bounded migration completion verified")
            """,
        )

    def make_restore_evidence(self, bundle: Path, manifest: Path) -> Path:
        isolation = self.root / "nexus-v2-isolated-restore-unit"
        self.assertEqual(
            tool.main(
                [
                    "init-isolation-root",
                    "--root",
                    str(isolation),
                    "--release-id",
                    RELEASE_ID,
                    "--created-at",
                    CREATED_AT,
                ]
            ),
            0,
        )
        evidence = self.root / "restore-evidence.json"
        self.assertEqual(
            tool.main(
                [
                    "rehearse-restore",
                    "--manifest",
                    str(manifest),
                    "--bundle-root",
                    str(bundle),
                    "--isolation-root",
                    str(isolation),
                    "--ports",
                    str(self.make_ports()),
                    "--driver",
                    str(self.make_restore_driver()),
                    "--evidence",
                    str(evidence),
                ]
            ),
            0,
        )
        return evidence

    def make_migration_evidence(self, bundle: Path, manifest: Path) -> Path:
        try_runtime = self.make_try_runtime()
        verifier = self.make_migration_verifier()
        evidence = self.root / "migration-evidence.json"
        self.assertEqual(
            tool.main(
                [
                    "rehearse-migration",
                    "--manifest",
                    str(manifest),
                    "--bundle-root",
                    str(bundle),
                    "--try-runtime-bin",
                    str(try_runtime),
                    "--try-runtime-revision",
                    "abcdef0",
                    "--try-runtime-sha256",
                    file_hash(try_runtime),
                    "--migration-blocks",
                    "2",
                    "--migration-verifier",
                    str(verifier),
                    "--migration-verifier-sha256",
                    file_hash(verifier),
                    "--evidence",
                    str(evidence),
                ]
            ),
            0,
        )
        return evidence

    def make_readiness(self) -> tuple[Path, Path, Path]:
        bundle, manifest = self.make_bundle()
        restore = self.make_restore_evidence(bundle, manifest)
        migration = self.make_migration_evidence(bundle, manifest)
        gates = bundle / "artifacts/config/economic-gates.bin"
        inventory = self.root / "inventory.json"
        write_json(inventory, acceptance_inventory())
        readiness = self.root / "readiness.json"
        self.assertEqual(
            tool.main(
                [
                    "prepare-reset",
                    "--manifest",
                    str(manifest),
                    "--bundle-root",
                    str(bundle),
                    "--restore-evidence",
                    str(restore),
                    "--migration-evidence",
                    str(migration),
                    "--economic-gates",
                    str(gates),
                    "--acceptance-inventory",
                    str(inventory),
                    "--output",
                    str(readiness),
                ]
            ),
            0,
        )
        return readiness, gates, inventory

    def make_pre_v16_bundle(self) -> tuple[Path, Path]:
        runtime_payload = b"captured-v14-runtime-wasm\n"
        observation = tcg_storage_version_observation()
        observation_payload = (
            json.dumps(observation, indent=2, sort_keys=True) + "\n"
        ).encode("utf-8")
        gates = pre_v16_fresh_reset_gates(
            hashlib.sha256(runtime_payload).hexdigest(),
            hashlib.sha256(observation_payload).hexdigest(),
        )
        return self.make_bundle(
            gates_value=gates,
            artifact_payloads={
                ("node", "runtime-v14-wasm"): runtime_payload,
                ("node", "tcg-storage-version-observation"): observation,
            },
        )

    def make_pre_v16_readiness(self) -> tuple[Path, Path, Path]:
        bundle, manifest = self.make_pre_v16_bundle()
        restore = self.make_restore_evidence(bundle, manifest)
        migration = self.make_migration_evidence(bundle, manifest)
        gates = bundle / "artifacts/config/economic-gates.bin"
        inventory = self.root / "pre-v16-inventory.json"
        write_json(inventory, acceptance_inventory())
        readiness = self.root / "pre-v16-readiness.json"
        self.assertEqual(
            tool.main(
                [
                    "prepare-reset",
                    "--manifest",
                    str(manifest),
                    "--bundle-root",
                    str(bundle),
                    "--restore-evidence",
                    str(restore),
                    "--migration-evidence",
                    str(migration),
                    "--economic-gates",
                    str(gates),
                    "--acceptance-inventory",
                    str(inventory),
                    "--output",
                    str(readiness),
                ]
            ),
            0,
        )
        return readiness, gates, inventory

    def test_backup_manifest_hashes_closed_artifact_set_and_detects_tampering(self) -> None:
        bundle, manifest = self.make_bundle()
        self.assertEqual(
            tool.main(["verify-backup", "--bundle-root", str(bundle), "--manifest", str(manifest)]),
            0,
        )
        (bundle / "artifacts/node/node-data.bin").write_text("tampered", encoding="utf-8")
        self.assertEqual(
            tool.main(["verify-backup", "--bundle-root", str(bundle), "--manifest", str(manifest)]),
            2,
        )

    def test_backup_manifest_rejects_missing_role_and_symlink(self) -> None:
        bundle = self.root / "incomplete"
        bundle.mkdir()
        path = bundle / "only.bin"
        path.write_text("one", encoding="utf-8")
        self.assertEqual(
            tool.main(
                [
                    "backup-manifest",
                    "--bundle-root",
                    str(bundle),
                    "--release-id",
                    RELEASE_ID,
                    "--source-commit",
                    SOURCE_COMMIT,
                    "--artifact",
                    "node:node-data:only.bin",
                    "--output",
                    str(bundle / "manifest.json"),
                ]
            ),
            2,
        )

        complete, manifest = self.make_bundle()
        target = complete / "artifacts/node/node-data.bin"
        target.unlink()
        target.symlink_to(complete / "artifacts/node/node-binary.bin")
        self.assertEqual(
            tool.main(["verify-backup", "--bundle-root", str(complete), "--manifest", str(manifest)]),
            2,
        )

    def test_restore_requires_loopback_disjoint_ports_and_records_full_evidence(self) -> None:
        bundle, manifest = self.make_bundle()
        isolation = self.root / "nexus-v2-isolated-restore-ports"
        self.assertEqual(
            tool.main(
                [
                    "init-isolation-root",
                    "--root",
                    str(isolation),
                    "--release-id",
                    RELEASE_ID,
                    "--created-at",
                    CREATED_AT,
                ]
            ),
            0,
        )
        driver = self.make_restore_driver()
        blocked_evidence = self.root / "blocked-restore.json"
        self.assertEqual(
            tool.main(
                [
                    "rehearse-restore",
                    "--manifest",
                    str(manifest),
                    "--bundle-root",
                    str(bundle),
                    "--isolation-root",
                    str(isolation),
                    "--ports",
                    str(self.make_ports(overlap_live=True)),
                    "--driver",
                    str(driver),
                    "--evidence",
                    str(blocked_evidence),
                ]
            ),
            2,
        )
        self.assertFalse(blocked_evidence.exists())
        self.assertFalse((isolation / "restore-result.json").exists())

        evidence = self.root / "restore.json"
        self.assertEqual(
            tool.main(
                [
                    "rehearse-restore",
                    "--manifest",
                    str(manifest),
                    "--bundle-root",
                    str(bundle),
                    "--isolation-root",
                    str(isolation),
                    "--ports",
                    str(self.make_ports()),
                    "--driver",
                    str(driver),
                    "--evidence",
                    str(evidence),
                ]
            ),
            0,
        )
        result = json.loads(evidence.read_text())
        self.assertEqual(result["result"], "passed")
        self.assertFalse(result["liveAlphaTouched"])

    def test_migration_pins_tools_and_rejects_failed_invariant(self) -> None:
        bundle, manifest = self.make_bundle()
        try_runtime = self.make_try_runtime()
        verifier = self.make_migration_verifier()
        evidence = self.root / "migration.json"
        self.assertEqual(
            tool.main(
                [
                    "rehearse-migration",
                    "--manifest",
                    str(manifest),
                    "--bundle-root",
                    str(bundle),
                    "--try-runtime-bin",
                    str(try_runtime),
                    "--try-runtime-revision",
                    "abcdef0",
                    "--try-runtime-sha256",
                    "0" * 64,
                    "--migration-blocks",
                    "2",
                    "--migration-verifier",
                    str(verifier),
                    "--migration-verifier-sha256",
                    file_hash(verifier),
                    "--evidence",
                    str(evidence),
                ]
            ),
            2,
        )
        self.assertFalse(evidence.exists())

        unsafe = self.make_migration_verifier(unsafe_check="interruptedResumeSafe")
        unsafe_evidence = self.root / "unsafe-migration.json"
        self.assertEqual(
            tool.main(
                [
                    "rehearse-migration",
                    "--manifest",
                    str(manifest),
                    "--bundle-root",
                    str(bundle),
                    "--try-runtime-bin",
                    str(try_runtime),
                    "--try-runtime-revision",
                    "abcdef0",
                    "--try-runtime-sha256",
                    file_hash(try_runtime),
                    "--migration-blocks",
                    "2",
                    "--migration-verifier",
                    str(unsafe),
                    "--migration-verifier-sha256",
                    file_hash(unsafe),
                    "--evidence",
                    str(unsafe_evidence),
                ]
            ),
            2,
        )
        self.assertFalse(unsafe_evidence.exists())

        safe_evidence = self.root / "safe-migration.json"
        self.assertEqual(
            tool.main(
                [
                    "rehearse-migration",
                    "--manifest",
                    str(manifest),
                    "--bundle-root",
                    str(bundle),
                    "--try-runtime-bin",
                    str(try_runtime),
                    "--try-runtime-revision",
                    "abcdef0",
                    "--try-runtime-sha256",
                    file_hash(try_runtime),
                    "--migration-blocks",
                    "2",
                    "--migration-verifier",
                    str(verifier),
                    "--migration-verifier-sha256",
                    file_hash(verifier),
                    "--evidence",
                    str(safe_evidence),
                ]
            ),
            0,
        )

    def test_every_exact_economic_gate_is_fail_closed(self) -> None:
        base = economic_gates()
        unsafe_mutations = [
            (("tcg", "features", "Packs"), True),
            (("tcg", "features", "Conversion"), True),
            (("tcg", "features", "Ranked"), True),
            (("tcg", "features", "MythicalAscension"), True),
            (("tcg", "legacyCreationSealed"), False),
            (("randomness", "cryptographyReviewApproved"), True),
            (("randomness", "drandQuicknetEnabled"), True),
            (("randomness", "productionEconomicUseAllowed"), True),
            (("gameResults", "activeProductionPolicyCount"), 1),
            (("gameResults", "allAlphaPoliciesPracticeOnlyOrValuelessTraining"), False),
            (("issuance", "trainingPackCreditRejectsProduction"), False),
            (("issuance", "paidV2IssuanceCallAvailable"), True),
            (("reforge", "dispatchableAvailable"), True),
            (("magic", "seedTrainingOnly"), False),
            (("magic", "productionTransferEnabled"), True),
            (("legacyEconomy", "marketplaceEnabled"), True),
            (("legacyEconomy", "purchaseEnabled"), True),
            (("legacyEconomy", "faucetEnabled"), True),
            (("legacyEconomy", "economicWritesEnabled"), True),
            (("arcadeTickets", "earningEnabled"), True),
            (("arcadeTickets", "transferEnabled"), True),
            (("arcadeTickets", "redemptionEnabled"), True),
            (("arcadeTickets", "randomVendingEnabled"), True),
            (("arcadeTickets", "featuredVendingEnabled"), True),
        ]
        for index, (path, unsafe_value) in enumerate(unsafe_mutations):
            with self.subTest(path=path):
                value = copy.deepcopy(base)
                cursor = value
                for component in path[:-1]:
                    cursor = cursor[component]
                cursor[path[-1]] = unsafe_value
                path_json = self.root / f"unsafe-gate-{index}.json"
                write_json(path_json, value)
                with self.assertRaises(tool.SafetyError):
                    tool.validate_economic_gates(path_json, RELEASE_ID, SOURCE_COMMIT)

        drand = copy.deepcopy(base)
        drand["randomness"]["mode"] = "DrandQuicknet"
        drand_path = self.root / "drand.json"
        write_json(drand_path, drand)
        with self.assertRaises(tool.SafetyError):
            tool.validate_economic_gates(drand_path)

        deterministic_without_seed = copy.deepcopy(base)
        deterministic_without_seed["randomness"]["mode"] = "DeterministicPrivateAlpha"
        deterministic_path = self.root / "deterministic-no-seed.json"
        write_json(deterministic_path, deterministic_without_seed)
        with self.assertRaises(tool.SafetyError):
            tool.validate_economic_gates(deterministic_path)

        extra_enabled = copy.deepcopy(base)
        extra_enabled["additionalEconomicFlags"]["nested"]["futureEconomicSurface"] = True
        extra_path = self.root / "extra-enabled.json"
        write_json(extra_path, extra_enabled)
        with self.assertRaises(tool.SafetyError):
            tool.validate_economic_gates(extra_path)

        unknown_enabled = copy.deepcopy(base)
        unknown_enabled["unreviewedEconomicSurfaceEnabled"] = True
        unknown_path = self.root / "unknown-enabled.json"
        write_json(unknown_path, unknown_enabled)
        with self.assertRaises(tool.SafetyError):
            tool.validate_economic_gates(unknown_path)

    def test_pre_v16_fresh_reset_gate_is_distinct_strict_and_fresh_only(self) -> None:
        base = pre_v16_fresh_reset_gates("a" * 64, "b" * 64)
        path = self.root / "pre-v16-gates.json"
        write_json(path, base)
        with self.assertRaises(tool.SafetyError):
            tool.validate_economic_gates(path, RELEASE_ID, SOURCE_COMMIT)
        validated = tool.validate_economic_gates(
            path,
            RELEASE_ID,
            SOURCE_COMMIT,
            allow_pre_v16_fresh_reset=True,
        )
        self.assertEqual(validated["mode"], tool.PRE_V16_FRESH_RESET_GATE_MODE)

        unsafe_mutations = [
            (("operationScope", "freshGenesisReplacementOnly"), False),
            (("operationScope", "inPlaceRuntimeUpgradeAllowed"), True),
            (("operationScope", "v2ActivationAllowed"), True),
            (("operationScope", "paidOrPublicActivationAllowed"), True),
            (("sourceRuntime", "specVersion"), 2),
            (("sourceRuntime", "metadataVersion"), 15),
            (("sourceRuntime", "tcgPalletIndex"), 10),
            (("sourceRuntime", "tcgStorageVersion"), 15),
            (("sourceRuntime", "flowPalletIndex"), 30),
            (("v2StructuralAbsence", "tcgV2StoragePresent"), True),
            (("v2StructuralAbsence", "tcgV2DispatchablesPresent"), True),
            (("v2StructuralAbsence", "v2EventsPresent"), True),
            (("knownLegacyEconomicSurfaces", "tcgPaidMintDispatchablesPresent"), False),
            (("knownLegacyEconomicSurfaces", "reachableThroughWriteIngress"), True),
            (("legacyWriteBarrier", "nodeServiceStopped"), False),
            (("legacyWriteBarrier", "authorityServiceStopped"), False),
            (("legacyWriteBarrier", "blockProductionStopped"), False),
            (("legacyWriteBarrier", "offlineFinalizedHeadMatchesGateBlock"), False),
            (("legacyWriteBarrier", "inventoryCapturedAfterWriteStop"), False),
            (("externalReviewFlags", "paidFeaturesApproved"), True),
        ]
        for index, (field_path, unsafe_value) in enumerate(unsafe_mutations):
            with self.subTest(path=field_path):
                value = copy.deepcopy(base)
                cursor = value
                for component in field_path[:-1]:
                    cursor = cursor[component]
                cursor[field_path[-1]] = unsafe_value
                unsafe_path = self.root / f"unsafe-pre-v16-{index}.json"
                write_json(unsafe_path, value)
                with self.assertRaises(tool.SafetyError):
                    tool.validate_economic_gates(
                        unsafe_path,
                        RELEASE_ID,
                        SOURCE_COMMIT,
                        allow_pre_v16_fresh_reset=True,
                    )

    def test_pre_v16_reset_readiness_binds_runtime_observation_and_frozen_block(self) -> None:
        readiness_path, _, _ = self.make_pre_v16_readiness()
        readiness = json.loads(readiness_path.read_text(encoding="utf-8"))
        self.assertEqual(readiness["economicGateMode"], tool.PRE_V16_FRESH_RESET_GATE_MODE)
        self.assertEqual(readiness["resetMode"], "fresh-genesis-replacement")
        self.assertTrue(readiness["freshGenesisReplacementOnly"])
        self.assertFalse(readiness["inPlaceRuntimeActivationAuthorized"])

    def test_pre_v16_reset_rejects_runtime_artifact_hash_mismatch(self) -> None:
        observation = tcg_storage_version_observation()
        observation_payload = (
            json.dumps(observation, indent=2, sort_keys=True) + "\n"
        ).encode("utf-8")
        gates = pre_v16_fresh_reset_gates(
            "0" * 64,
            hashlib.sha256(observation_payload).hexdigest(),
        )
        bundle, manifest = self.make_bundle(
            gates_value=gates,
            artifact_payloads={
                ("node", "runtime-v14-wasm"): b"actual-v14-runtime\n",
                ("node", "tcg-storage-version-observation"): observation,
            },
        )
        restore = self.make_restore_evidence(bundle, manifest)
        migration = self.make_migration_evidence(bundle, manifest)
        inventory = self.root / "pre-v16-hash-inventory.json"
        write_json(inventory, acceptance_inventory())
        self.assertEqual(
            tool.main(
                [
                    "prepare-reset",
                    "--manifest",
                    str(manifest),
                    "--bundle-root",
                    str(bundle),
                    "--restore-evidence",
                    str(restore),
                    "--migration-evidence",
                    str(migration),
                    "--economic-gates",
                    str(bundle / "artifacts/config/economic-gates.bin"),
                    "--acceptance-inventory",
                    str(inventory),
                    "--output",
                    str(self.root / "should-not-be-ready.json"),
                ]
            ),
            2,
        )

    def test_pre_v16_reset_rejects_observation_block_mismatch(self) -> None:
        runtime_payload = b"actual-v14-runtime\n"
        observation = tcg_storage_version_observation(
            block_number=101,
            block_hash="0x" + ("2" * 64),
        )
        observation_payload = (
            json.dumps(observation, indent=2, sort_keys=True) + "\n"
        ).encode("utf-8")
        gates = pre_v16_fresh_reset_gates(
            hashlib.sha256(runtime_payload).hexdigest(),
            hashlib.sha256(observation_payload).hexdigest(),
        )
        bundle, manifest = self.make_bundle(
            gates_value=gates,
            artifact_payloads={
                ("node", "runtime-v14-wasm"): runtime_payload,
                ("node", "tcg-storage-version-observation"): observation,
            },
        )
        restore = self.make_restore_evidence(bundle, manifest)
        migration = self.make_migration_evidence(bundle, manifest)
        inventory = self.root / "pre-v16-block-inventory.json"
        write_json(inventory, acceptance_inventory())
        self.assertEqual(
            tool.main(
                [
                    "prepare-reset",
                    "--manifest",
                    str(manifest),
                    "--bundle-root",
                    str(bundle),
                    "--restore-evidence",
                    str(restore),
                    "--migration-evidence",
                    str(migration),
                    "--economic-gates",
                    str(bundle / "artifacts/config/economic-gates.bin"),
                    "--acceptance-inventory",
                    str(inventory),
                    "--output",
                    str(self.root / "should-not-be-ready.json"),
                ]
            ),
            2,
        )

    def test_reset_readiness_requires_restore_migration_zero_assets_and_pinned_gates(self) -> None:
        bundle, manifest = self.make_bundle()
        restore = self.make_restore_evidence(bundle, manifest)
        migration = self.make_migration_evidence(bundle, manifest)
        gates = bundle / "artifacts/config/economic-gates.bin"

        inventory = self.root / "nonzero-inventory.json"
        write_json(inventory, acceptance_inventory(cardsV2=1))
        blocked = self.root / "blocked-readiness.json"
        self.assertEqual(
            tool.main(
                [
                    "prepare-reset",
                    "--manifest",
                    str(manifest),
                    "--bundle-root",
                    str(bundle),
                    "--restore-evidence",
                    str(restore),
                    "--migration-evidence",
                    str(migration),
                    "--economic-gates",
                    str(gates),
                    "--acceptance-inventory",
                    str(inventory),
                    "--output",
                    str(blocked),
                ]
            ),
            2,
        )
        self.assertFalse(blocked.exists())

        unpinned_gates = self.root / "unpinned-gates.json"
        value = economic_gates()
        value["additionalEconomicFlags"]["anotherFlag"] = False
        write_json(unpinned_gates, value)
        zero_inventory = self.root / "zero-inventory.json"
        write_json(zero_inventory, acceptance_inventory())
        self.assertEqual(
            tool.main(
                [
                    "prepare-reset",
                    "--manifest",
                    str(manifest),
                    "--bundle-root",
                    str(bundle),
                    "--restore-evidence",
                    str(restore),
                    "--migration-evidence",
                    str(migration),
                    "--economic-gates",
                    str(unpinned_gates),
                    "--acceptance-inventory",
                    str(zero_inventory),
                    "--output",
                    str(self.root / "unpinned-readiness.json"),
                ]
            ),
            2,
        )

        ready = self.root / "ready.json"
        self.assertEqual(
            tool.main(
                [
                    "prepare-reset",
                    "--manifest",
                    str(manifest),
                    "--bundle-root",
                    str(bundle),
                    "--restore-evidence",
                    str(restore),
                    "--migration-evidence",
                    str(migration),
                    "--economic-gates",
                    str(gates),
                    "--acceptance-inventory",
                    str(zero_inventory),
                    "--output",
                    str(ready),
                ]
            ),
            0,
        )
        value = json.loads(ready.read_text())
        self.assertFalse(value["resetExecuted"])
        self.assertFalse(value["deployExecuted"])

    def test_automatic_rollback_is_pre_acceptance_only(self) -> None:
        readiness, _, _ = self.make_readiness()
        driver = self.make_executable(
            "rollback-driver",
            f"""
            import argparse, hashlib, json
            from pathlib import Path
            p = argparse.ArgumentParser()
            for name in ("readiness", "acceptance-inventory", "economic-gates", "result"):
                p.add_argument("--" + name, required=True)
            a = p.parse_args()
            ready = json.loads(Path(a.readiness).read_text())
            inventory_hash = hashlib.sha256(Path(a.acceptance_inventory).read_bytes()).hexdigest()
            value = {{
                "schemaVersion": 1,
                "kind": "nexus-v2-private-alpha-rollback-result",
                "releaseId": ready["releaseId"],
                "sourceCommit": ready["sourceCommit"],
                "acceptanceInventorySha256": inventory_hash,
                "result": "passed",
                "checks": {{name: True for name in {sorted({
                    "rollbackCompleted",
                    "backupHashesVerified",
                    "nodeHealthy",
                    "mediaHealthy",
                    "ipfsHealthy",
                    "indexerHealthy",
                    "economicFlagsDisabled",
                })!r}}},
            }}
            Path(a.result).write_text(json.dumps(value))
            """,
        )

        newer_hash = "0x" + ("2" * 64)
        nonzero_inventory = self.root / "rollback-nonzero-inventory.json"
        nonzero_gates = self.root / "rollback-nonzero-gates.json"
        write_json(
            nonzero_inventory,
            acceptance_inventory(101, newer_hash, lifetimeCardsV2Created=1),
        )
        write_json(nonzero_gates, economic_gates(101, newer_hash))
        blocked_evidence = self.root / "rollback-blocked.json"
        self.assertEqual(
            tool.main(
                [
                    "automatic-rollback",
                    "--readiness",
                    str(readiness),
                    "--acceptance-inventory",
                    str(nonzero_inventory),
                    "--economic-gates",
                    str(nonzero_gates),
                    "--approval",
                    str(self.root / "not-needed-approval.json"),
                    "--driver",
                    str(driver),
                    "--evidence",
                    str(blocked_evidence),
                    "--execute",
                ]
            ),
            2,
        )
        decision = json.loads(blocked_evidence.read_text())
        self.assertEqual(decision["decision"], "blocked-after-v2-acceptance")
        self.assertFalse(Path(f"{blocked_evidence}.rollback-result.json").exists())

        zero_inventory = self.root / "rollback-zero-inventory.json"
        zero_gates = self.root / "rollback-zero-gates.json"
        write_json(zero_inventory, acceptance_inventory(102, "0x" + ("3" * 64)))
        write_json(zero_gates, economic_gates(102, "0x" + ("3" * 64)))
        approval = self.root / "rollback-approval.json"
        write_json(
            approval,
            {
                "schemaVersion": 1,
                "kind": "nexus-v2-private-alpha-rollback-approval",
                "releaseId": RELEASE_ID,
                "sourceCommit": SOURCE_COMMIT,
                "approved": True,
                "readinessSha256": file_hash(readiness),
                "rollbackDriverSha256": file_hash(driver),
                "expiresAtUtc": (
                    dt.datetime.now(dt.timezone.utc) + dt.timedelta(hours=1)
                ).isoformat(),
            },
        )
        evidence = self.root / "rollback-evidence.json"
        self.assertEqual(
            tool.main(
                [
                    "automatic-rollback",
                    "--readiness",
                    str(readiness),
                    "--acceptance-inventory",
                    str(zero_inventory),
                    "--economic-gates",
                    str(zero_gates),
                    "--approval",
                    str(approval),
                    "--driver",
                    str(driver),
                    "--evidence",
                    str(evidence),
                    "--execute",
                ]
            ),
            0,
        )
        self.assertEqual(json.loads(evidence.read_text())["result"], "passed")

    def test_automatic_rollback_rejects_pre_v16_fresh_reset_gates(self) -> None:
        readiness, gates, inventory = self.make_pre_v16_readiness()
        evidence = self.root / "pre-v16-rollback-must-not-run.json"
        self.assertEqual(
            tool.main(
                [
                    "automatic-rollback",
                    "--readiness",
                    str(readiness),
                    "--acceptance-inventory",
                    str(inventory),
                    "--economic-gates",
                    str(gates),
                    "--approval",
                    str(self.root / "unused-approval.json"),
                    "--driver",
                    str(self.root / "unused-driver"),
                    "--evidence",
                    str(evidence),
                    "--execute",
                ]
            ),
            2,
        )
        self.assertFalse(evidence.exists())

    def test_only_evidence_and_command_free_coordinators_are_bundled(self) -> None:
        directory = Path(__file__).resolve().parent
        executables = {
            path.name
            for path in directory.iterdir()
            if path.is_file() and os.access(path, os.X_OK)
        }
        self.assertEqual(
            executables,
            {"alpha_v2_release.py", "final_freeze.py", "node_candidate.py"},
        )
        source = (directory / "alpha_v2_release.py").read_text(encoding="utf-8")
        self.assertNotIn("subprocess.run([\"./deploy/", source)
        self.assertNotIn("shell=True", source)
        coordinator = (directory / "final_freeze.py").read_text(encoding="utf-8")
        self.assertNotIn("shell=True", coordinator)
        for command in ('"ssh"', '"docker"', '"systemctl"', '"curl"'):
            self.assertNotIn(command, coordinator)

    def test_documented_operator_inputs_match_the_validators(self) -> None:
        docs = tool.REPO_ROOT / "docs/nexus-v2-private-alpha"
        tool.validate_economic_gates(docs / "economic-gates.example.json")
        tool.validate_economic_gates(
            docs / "pre-v16-fresh-reset-gates.example.json",
            allow_pre_v16_fresh_reset=True,
        )
        tool.validate_acceptance_inventory(docs / "acceptance-inventory.example.json")
        tool.validate_ports(docs / "isolated-ports.example.json")

    def test_documented_artifact_roles_match_required_closed_set(self) -> None:
        readme = (
            tool.REPO_ROOT / "scripts/nexus-v2-private-alpha/README.md"
        ).read_text(encoding="utf-8")
        role_block = readme.split("Required artifact roles:\n\n```text\n", 1)[1].split(
            "\n```", 1
        )[0]
        documented = {
            tuple(line.split(":", 1))
            for line in role_block.splitlines()
            if line.strip()
        }
        required = {
            (group, name)
            for group, names in tool.REQUIRED_ARTIFACTS.items()
            for name in names
        }
        self.assertEqual(documented, required)


if __name__ == "__main__":
    unittest.main()
