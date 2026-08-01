#!/usr/bin/env python3

from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path
from typing import Any


sys.path.insert(0, str(Path(__file__).resolve().parent))
import final_freeze as tool  # noqa: E402


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


class FinalFreezeTests(unittest.TestCase):
    def setUp(self) -> None:
        self.context = tempfile.TemporaryDirectory(prefix="nexus-v2-final-freeze-test-")
        self.root = Path(self.context.name)
        self.driver = self.make_driver()

    def tearDown(self) -> None:
        self.context.cleanup()

    def make_driver(self) -> Path:
        path = self.root / "fixture-driver"
        path.write_text(
            "#!/usr/bin/env python3\n"
            + textwrap.dedent(
                f"""
                import argparse
                import datetime
                import hashlib
                import json
                import os
                from pathlib import Path

                freeze_checks = {tool.FREEZE_CHECKS!r}
                preflight_checks = {tool.PREFLIGHT_CHECKS!r}
                snapshot_checks = {tool.SNAPSHOT_CHECKS!r}
                verify_snapshot_checks = {tool.VERIFY_SNAPSHOT_CHECKS!r}
                parser = argparse.ArgumentParser()
                parser.add_argument("--action", required=True)
                parser.add_argument("--transaction-id", required=True)
                parser.add_argument("--release-id", required=True)
                parser.add_argument("--source-commit", required=True)
                parser.add_argument("--component-source-commit", required=True)
                parser.add_argument("--role", required=True)
                parser.add_argument("--target", required=True)
                parser.add_argument("--bundle-root", required=True)
                parser.add_argument("--result", required=True)
                parser.add_argument("--artifact", action="append", default=[])
                parser.add_argument("--frozen-block-number", type=int)
                parser.add_argument("--frozen-block-hash")
                parser.add_argument("--dry-run", action="store_true")
                parser.add_argument("--fail-freeze", action="store_true")
                args = parser.parse_args()
                expected_component_commits = {{
                    "authority": "c" * 40,
                    "chain": "a" * 40,
                    "media-ipfs": "b" * 40,
                    "site-indexer-mongo": "df01ffc08a791a73d60461d25d0a9d8a78456466",
                    "site-ingress": "df01ffc08a791a73d60461d25d0a9d8a78456466",
                }}
                if args.component_source_commit != expected_component_commits[args.role]:
                    raise SystemExit("protected component source commit mismatch")
                if args.action == "preflight":
                    checks = preflight_checks
                elif args.action == "freeze":
                    checks = freeze_checks[args.role]
                elif args.action == "verify-frozen":
                    checks = freeze_checks[args.role] | {{"remainsStopped"}}
                elif args.action == "snapshot":
                    checks = snapshot_checks
                else:
                    checks = verify_snapshot_checks
                checks_value = {{name: True for name in checks}}
                if args.fail_freeze and args.action == "freeze" and args.role == "chain":
                    checks_value["nodeStopped"] = False
                artifacts = []
                if not args.dry_run and args.action in {{"snapshot", "verify-snapshot"}}:
                    for artifact_role in args.artifact:
                        group, name = artifact_role.split(":")
                        relative = Path("artifacts") / args.role / f"{{group}}-{{name}}.bin"
                        path = Path(args.bundle_root) / relative
                        path.parent.mkdir(parents=True, exist_ok=True)
                        if not path.exists():
                            if args.role == "chain" and name == "legacy-source-inventory":
                                block = {{"number": args.frozen_block_number, "hash": args.frozen_block_hash}}
                                query = lambda method, params, result: {{"method": method, "params": params, "result": result}}
                                inventory = {{
                                    "schemaVersion": 2,
                                    "kind": {tool.release.LEGACY_SOURCE_INVENTORY_KIND!r},
                                    "releaseId": args.release_id,
                                    "sourceCommit": args.source_commit,
                                    "deployedSourceCommit": "d" * 40,
                                    "observedAtUtc": "2026-07-31T00:00:00Z",
                                    "observedAtFinalizedBlock": block,
                                    "captureMode": "isolated-frozen-copy-read-only",
                                    "finality": {{
                                        "finalizedHead": query("chain_getFinalizedHead", [], args.frozen_block_hash),
                                        "blockHashAtNumber": query("chain_getBlockHash", [args.frozen_block_number], args.frozen_block_hash),
                                        "header": query("chain_getHeader", [args.frozen_block_hash], {{"number": hex(args.frozen_block_number)}}),
                                    }},
                                    "storage": {{
                                        "tcgStorageVersion": {{
                                            "pallet": "EterraTCG", "storage": ":__STORAGE_VERSION__:",
                                            "key": {tool.release.LEGACY_TCG_STORAGE_VERSION_KEY!r},
                                            "query": query("state_getStorage", [{tool.release.LEGACY_TCG_STORAGE_VERSION_KEY!r}, args.frozen_block_hash], "0x0e00"),
                                        }},
                                        "nextCardId": {{
                                            "pallet": "EterraTCG", "storage": "NextCardId",
                                            "key": {tool.release.LEGACY_NEXT_CARD_ID_KEY!r},
                                            "query": query("state_getStorage", [{tool.release.LEGACY_NEXT_CARD_ID_KEY!r}, args.frozen_block_hash], None),
                                        }},
                                        "gameAuthority": {{
                                            "nextGameId": {{
                                                "pallet": "EterraGameAuthority", "storage": "NextGameId",
                                                "key": {tool.release.LEGACY_GAME_AUTHORITY_STORAGE['nextGameId']['key']!r},
                                                "query": query("state_getStorage", [{tool.release.LEGACY_GAME_AUTHORITY_STORAGE['nextGameId']['key']!r}, args.frozen_block_hash], "0x0400000000000000"),
                                            }},
                                            "games": {{
                                                "pallet": "EterraGameAuthority", "storage": "Games",
                                                "prefix": {tool.release.LEGACY_GAME_AUTHORITY_STORAGE['games']['prefix']!r},
                                                "method": "state_getKeysPaged", "at": args.frozen_block_hash,
                                                "pageSize": {tool.release.LEGACY_AUTHORITY_KEY_PAGE_SIZE},
                                                "pages": [{{"startKey": None, "keys": [{(tool.release.LEGACY_GAME_AUTHORITY_STORAGE['games']['prefix'] + '00000000')!r}]}}],
                                            }},
                                            "activeGameByPlayer": {{
                                                "pallet": "EterraGameAuthority", "storage": "ActiveGameByPlayer",
                                                "prefix": {tool.release.LEGACY_GAME_AUTHORITY_STORAGE['activeGameByPlayer']['prefix']!r},
                                                "method": "state_getKeysPaged", "at": args.frozen_block_hash,
                                                "pageSize": {tool.release.LEGACY_AUTHORITY_KEY_PAGE_SIZE},
                                                "pages": [{{"startKey": None, "keys": [{(tool.release.LEGACY_GAME_AUTHORITY_STORAGE['activeGameByPlayer']['prefix'] + '00000000')!r}]}}],
                                            }},
                                            "eliminations": {{
                                                "pallet": "EterraGameAuthority", "storage": "Eliminations",
                                                "prefix": {tool.release.LEGACY_GAME_AUTHORITY_STORAGE['eliminations']['prefix']!r},
                                                "method": "state_getKeysPaged", "at": args.frozen_block_hash,
                                                "pageSize": {tool.release.LEGACY_AUTHORITY_KEY_PAGE_SIZE},
                                                "pages": [{{"startKey": None, "keys": [{(tool.release.LEGACY_GAME_AUTHORITY_STORAGE['eliminations']['prefix'] + '00000000')!r}, {(tool.release.LEGACY_GAME_AUTHORITY_STORAGE['eliminations']['prefix'] + '00000001')!r}]}}],
                                            }},
                                            "processedEndCommands": {{
                                                "pallet": "EterraGameAuthority", "storage": "ProcessedEndCommands",
                                                "prefix": {tool.release.LEGACY_GAME_AUTHORITY_STORAGE['processedEndCommands']['prefix']!r},
                                                "method": "state_getKeysPaged", "at": args.frozen_block_hash,
                                                "pageSize": {tool.release.LEGACY_AUTHORITY_KEY_PAGE_SIZE},
                                                "pages": [{{"startKey": None, "keys": [{(tool.release.LEGACY_GAME_AUTHORITY_STORAGE['processedEndCommands']['prefix'] + '00000000')!r}, {(tool.release.LEGACY_GAME_AUTHORITY_STORAGE['processedEndCommands']['prefix'] + '00000001')!r}]}}],
                                            }},
                                            "processedEliminationEvents": {{
                                                "pallet": "EterraGameAuthority", "storage": "ProcessedEliminationEvents",
                                                "prefix": {tool.release.LEGACY_GAME_AUTHORITY_STORAGE['processedEliminationEvents']['prefix']!r},
                                                "method": "state_getKeysPaged", "at": args.frozen_block_hash,
                                                "pageSize": {tool.release.LEGACY_AUTHORITY_KEY_PAGE_SIZE},
                                                "pages": [{{"startKey": None, "keys": [{(tool.release.LEGACY_GAME_AUTHORITY_STORAGE['processedEliminationEvents']['prefix'] + '00000000')!r}, {(tool.release.LEGACY_GAME_AUTHORITY_STORAGE['processedEliminationEvents']['prefix'] + '00000001')!r}, {(tool.release.LEGACY_GAME_AUTHORITY_STORAGE['processedEliminationEvents']['prefix'] + '00000002')!r}]}}],
                                            }},
                                        }},
                                        "cards": {{
                                            "pallet": "EterraTCG", "storage": "Cards",
                                            "prefix": {tool.release.LEGACY_CARDS_PREFIX!r},
                                            "method": "state_getKeysPaged", "at": args.frozen_block_hash,
                                            "pageSize": {tool.release.LEGACY_CARD_KEY_PAGE_SIZE},
                                            "pages": [{{"startKey": None, "keys": []}}],
                                        }},
                                    }},
                                    "summary": {{
                                        "cardIdsSha256": hashlib.sha256(b"").hexdigest(),
                                        "cardsCount": 0, "maxCardId": None,
                                        "gameAuthorityActivePlayerLocks": 1,
                                        "gameAuthorityEliminationRecords": 2,
                                        "gameAuthorityEndCommandsProcessed": 2,
                                        "gameAuthorityEliminationEventsProcessed": 3,
                                        "gameAuthorityGames": 1,
                                        "gameAuthorityNextGameId": 4,
                                        "minimumMigrationBlocks": 1, "nextCardId": 0,
                                        "tcgStorageVersion": 14,
                                        "v16MigrationBatchSize": {tool.release.V16_MIGRATION_BATCH_SIZE},
                                    }},
                                    "safety": {tool.release.LEGACY_SAFETY!r},
                                }}
                                path.write_text(json.dumps(inventory, indent=2, sort_keys=True) + "\\n", encoding="utf-8")
                            elif args.role == "chain" and name == "tcg-storage-version-observation":
                                observation = {{
                                    "schemaVersion": 1,
                                    "kind": "frame-pallet-storage-version-observation",
                                    "finalizedBlock": {{"number": args.frozen_block_number, "hash": args.frozen_block_hash}},
                                    "decoded": {{"scaleType": "u16", "storageVersion": 14}},
                                    "liveSource": {{"commit": "d" * 40, "declaredStorageVersion": 14}},
                                    "readOnlyRpc": {{
                                        "method": "state_getStorage",
                                        "storageKey": {tool.release.LEGACY_TCG_STORAGE_VERSION_KEY!r},
                                        "result": "0x0e00",
                                    }},
                                }}
                                path.write_text(json.dumps(observation, indent=2, sort_keys=True) + "\\n", encoding="utf-8")
                            elif args.role == "chain" and name == "try-runtime-snapshot-proof":
                                artifact_root = Path(args.bundle_root) / "artifacts" / "chain"
                                source_path = lambda item_group, item_name: artifact_root / f"{{item_group}}-{{item_name}}.bin"
                                source_sha = lambda item_group, item_name: hashlib.sha256(source_path(item_group, item_name).read_bytes()).hexdigest()
                                snapshot = source_path("node", "try-runtime-snapshot")
                                proof = {{
                                    "schemaVersion": 1,
                                    "kind": "nexus-v2-private-alpha-frozen-try-runtime-snapshot-proof",
                                    "transactionId": args.transaction_id,
                                    "releaseId": args.release_id,
                                    "sourceCommit": args.source_commit,
                                    "frozenAtUtc": "2026-07-31T00:00:00Z",
                                    "createdAtUtc": "2026-07-31T00:01:00Z",
                                    "frozenFinalizedBlock": {{"number": args.frozen_block_number, "hash": args.frozen_block_hash}},
                                    "source": {{
                                        "chainSpecSha256": source_sha("config", "chain-spec"),
                                        "nodeBinarySha256": source_sha("node", "node-binary"),
                                        "nodeDataArchiveSha256": source_sha("node", "node-data"),
                                    }},
                                    "snapshot": {{"bytes": snapshot.stat().st_size, "sha256": source_sha("node", "try-runtime-snapshot")}},
                                    "tryRuntime": {{
                                        "log": "fixture exact-block snapshot\\n",
                                        "sha256": "2" * 64,
                                        "sourceRevision": "3" * 40,
                                        "version": "try-runtime 0.42.0-fixture",
                                    }},
                                    "isolatedRpcObservation": {{
                                        "blockHashAtNumber": args.frozen_block_hash,
                                        "finalizedHead": args.frozen_block_hash,
                                        "genesisHash": "0x" + "4" * 64,
                                        "headerHash": args.frozen_block_hash,
                                        "headerNumber": args.frozen_block_number,
                                        "runtimeCodeHash": "0x" + "5" * 64,
                                        "runtimeSpecVersion": 1,
                                    }},
                                    "creation": {{
                                        "explicitAtHash": True,
                                        "isolatedCopyOnly": True,
                                        "networkIsolated": True,
                                        "originalNodeRemainedStopped": True,
                                        "sourceArchiveExtracted": True,
                                    }},
                                    "authorizations": {{"liveSubmission": False, "paidOrPublicActivation": False}},
                                }}
                                path.write_text(json.dumps(proof, sort_keys=True) + "\\n", encoding="utf-8")
                            else:
                                path.write_bytes(f"{{args.role}}:{{group}}:{{name}}\\n".encode())
                        payload = path.read_bytes()
                        artifacts.append({{
                            "group": group,
                            "name": name,
                            "path": relative.as_posix(),
                            "bytes": len(payload),
                            "sha256": hashlib.sha256(payload).hexdigest(),
                        }})
                frozen_block = None
                if not args.dry_run:
                    if args.action == "freeze" and args.role == "chain":
                        frozen_block = {{"number": 123, "hash": "0x" + "1" * 64}}
                    elif args.action in {{"verify-frozen", "snapshot", "verify-snapshot"}}:
                        frozen_block = {{"number": args.frozen_block_number, "hash": args.frozen_block_hash}}
                frozen_at = None
                if not args.dry_run and args.action != "preflight":
                    frozen_at = "2026-07-31T00:00:00Z"
                result = {{
                    "schemaVersion": 1,
                    "kind": "nexus-v2-private-alpha-final-freeze-component-result",
                    "transactionId": args.transaction_id,
                    "releaseId": args.release_id,
                    "sourceCommit": args.source_commit,
                    "role": args.role,
                    "action": args.action,
                    "target": args.target,
                    "dryRun": args.dry_run,
                    "liveMutationPerformed": False if args.dry_run or args.action == "preflight" else args.action == "freeze",
                    "planned": args.dry_run,
                    "frozenAtUtc": frozen_at,
                    "frozenFinalizedBlock": frozen_block,
                    "checks": checks_value,
                    "artifacts": artifacts,
                }}
                output = Path(args.result)
                output.parent.mkdir(parents=True, exist_ok=True)
                descriptor = os.open(output, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
                with os.fdopen(descriptor, "w") as handle:
                    json.dump(result, handle, sort_keys=True)
                """
            ),
            encoding="utf-8",
        )
        path.chmod(0o700)
        return path

    def make_plan(self, *, fail: bool = False) -> tuple[Path, str]:
        driver_sha = hashlib.sha256(self.driver.read_bytes()).hexdigest()
        components: dict[str, Any] = {}
        for role in tool.ROLES:
            arguments = ["--fail-freeze"] if fail else []
            components[role] = {
                "driver": str(self.driver),
                "driverSha256": driver_sha,
                "target": f"fixture-{role}",
                "arguments": arguments,
            }
        source = "a" * 40
        value = {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-final-freeze-plan",
            "transactionId": "freeze-test-1",
            "releaseId": "nexus-v2-test",
            "sourceCommit": source,
            "componentSourceCommits": {
                "ai": "6" * 40,
                "blockchainia-site": "7" * 40,
                "chain": source,
                "flow": "8" * 40,
                "ip": "9" * 40,
                "media": "b" * 40,
                "sdkgen": "c" * 40,
                "unity": "5" * 40,
                "web": "df01ffc08a791a73d60461d25d0a9d8a78456466",
            },
            "preV16SourceRuntime": {
                "deployedSourceCommit": "d" * 40,
                "specVersion": 1,
                "metadataVersion": 14,
                "tcgPalletIndex": 9,
                "tcgStorageVersion": 14,
                "flowPalletIndex": 29,
            },
            "stabilityWindowSeconds": 30,
            "components": components,
            "authorizations": {
                "automaticResumeOnFailure": False,
                "finalFreezeAndBackup": True,
                "freshReset": False,
                "paidOrPublicActivation": False,
                "privateAlphaOnly": True,
            },
        }
        path = self.root / ("failed-plan.json" if fail else "plan.json")
        write_json(path, value)
        return path, hashlib.sha256(path.read_bytes()).hexdigest()

    def test_dry_run_invokes_every_action_without_mutation(self) -> None:
        plan, digest = self.make_plan()
        evidence = self.root / "dry-evidence.json"
        result = tool.main(
            [
                "dry-run",
                "--plan",
                str(plan),
                "--expected-plan-sha256",
                digest,
                "--bundle-root",
                str(self.root / "dry-bundle"),
                "--state-root",
                str(self.root / "dry-state"),
                "--evidence",
                str(evidence),
            ]
        )
        self.assertEqual(result, 0)
        value = json.loads(evidence.read_text())
        self.assertFalse(value["liveMutationPerformed"])
        for role in tool.ROLES:
            self.assertEqual(value["driverActionsValidated"][role], list(tool.ACTIONS))

    def test_execute_creates_complete_verified_backup_manifest(self) -> None:
        plan, digest = self.make_plan()
        bundle = self.root / "bundle"
        evidence = self.root / "evidence.json"
        result = tool.main(
            [
                "execute",
                "--plan",
                str(plan),
                "--expected-plan-sha256",
                digest,
                "--bundle-root",
                str(bundle),
                "--state-root",
                str(self.root / "state"),
                "--evidence",
                str(evidence),
            ]
        )
        self.assertEqual(result, 0)
        value = json.loads(evidence.read_text())
        self.assertTrue(value["allIngressAndMutatingServicesStopped"])
        verified = tool.release.verify_backup_manifest(bundle / "backup-manifest.json", bundle)
        self.assertEqual(verified["sha256"], value["backupManifestSha256"])
        inventory_path = tool.release.find_artifact(
            verified, bundle, "config", "acceptance-inventory"
        )
        inventory = tool.release.validate_acceptance_inventory(inventory_path)
        counts = inventory["value"]["counts"]
        self.assertEqual(counts["currentLegacyAuthorityGames"], 1)
        self.assertEqual(counts["currentLegacyAuthorityActivePlayerLocks"], 1)
        self.assertEqual(counts["currentLegacyAuthorityEliminationRecords"], 2)
        self.assertEqual(counts["lifetimeLegacyAuthorityGamesCreated"], 4)
        self.assertEqual(counts["lifetimeLegacyAuthorityEndCommandsProcessed"], 2)
        self.assertEqual(
            counts["lifetimeLegacyAuthorityEliminationEventsProcessed"], 3
        )
        self.assertEqual(
            counts["lifetimeLegacyAuthorityAcceptanceWritesLowerBound"], 9
        )
        for name in (
            tool.release.V2_ACCEPTANCE_COUNT_FIELDS
            | tool.release.GAME_RESULTS_ACCEPTANCE_COUNT_FIELDS
        ):
            self.assertEqual(counts[name], 0, name)
        source_inventory_path = tool.release.find_artifact(
            verified, bundle, "node", "legacy-source-inventory"
        )
        metadata_path = tool.release.find_artifact(
            verified, bundle, "node", "runtime-v14-metadata"
        )
        observation_path = tool.release.find_artifact(
            verified, bundle, "node", "tcg-storage-version-observation"
        )
        observation_evidence = inventory["observationEvidence"]
        self.assertEqual(
            observation_evidence["legacySourceInventorySha256"],
            tool.sha256_file(source_inventory_path),
        )
        self.assertEqual(
            observation_evidence["runtimeMetadataScaleSha256"],
            tool.sha256_file(metadata_path),
        )
        self.assertEqual(
            observation_evidence["tcgStorageVersionObservationSha256"],
            tool.sha256_file(observation_path),
        )

    def test_pre_v16_inventory_rejects_source_from_another_finalized_block(self) -> None:
        plan, digest = self.make_plan()
        validated_plan = tool.validate_plan(plan, digest)
        source_inventory = {
            "blockNumber": 122,
            "blockHash": "0x" + "1" * 64,
            "sha256": "a" * 64,
            "gameAuthorityGames": 0,
            "gameAuthorityActivePlayerLocks": 0,
            "gameAuthorityEliminationRecords": 0,
            "gameAuthorityNextGameId": 0,
            "gameAuthorityEndCommandsProcessed": 0,
            "gameAuthorityEliminationEventsProcessed": 0,
        }
        with self.assertRaisesRegex(tool.FreezeError, "frozen finalized block"):
            tool.pre_v16_acceptance_inventory(
                validated_plan,
                {"number": 123, "hash": "0x" + "1" * 64},
                source_inventory,
                runtime_metadata_sha256="b" * 64,
                tcg_observation_sha256="c" * 64,
            )

    def test_failed_partial_freeze_never_resumes_services(self) -> None:
        plan, digest = self.make_plan(fail=True)
        evidence = self.root / "failed-evidence.json"
        result = tool.main(
            [
                "execute",
                "--plan",
                str(plan),
                "--expected-plan-sha256",
                digest,
                "--bundle-root",
                str(self.root / "failed-bundle"),
                "--state-root",
                str(self.root / "failed-state"),
                "--evidence",
                str(evidence),
            ]
        )
        self.assertEqual(result, 2)
        value = json.loads(evidence.read_text())
        self.assertTrue(value["writeBarrierMayBePartial"])
        self.assertFalse(value["automaticResumeAttempted"])

    def test_plan_rejects_secret_bearing_driver_arguments(self) -> None:
        plan, digest = self.make_plan()
        value = json.loads(plan.read_text())
        value["components"]["chain"]["arguments"] = ["--password", "do-not-store"]
        write_json(plan, value)
        digest = hashlib.sha256(plan.read_bytes()).hexdigest()
        with self.assertRaisesRegex(tool.FreezeError, "secret material"):
            tool.validate_plan(plan, digest)

    def test_swapped_component_commits_fail_driver_validation(self) -> None:
        plan, _ = self.make_plan()
        value = json.loads(plan.read_text())
        value["componentSourceCommits"]["media"], value["componentSourceCommits"]["sdkgen"] = (
            value["componentSourceCommits"]["sdkgen"],
            value["componentSourceCommits"]["media"],
        )
        write_json(plan, value)
        digest = hashlib.sha256(plan.read_bytes()).hexdigest()
        result = tool.main(
            [
                "dry-run",
                "--plan",
                str(plan),
                "--expected-plan-sha256",
                digest,
                "--bundle-root",
                str(self.root / "swapped-bundle"),
                "--state-root",
                str(self.root / "swapped-state"),
                "--evidence",
                str(self.root / "swapped-evidence.json"),
            ]
        )
        self.assertEqual(result, 2)

    def test_mutated_previously_unused_sdk_commit_fails_authority_driver(self) -> None:
        plan, _ = self.make_plan()
        value = json.loads(plan.read_text())
        value["componentSourceCommits"]["sdkgen"] = "e" * 40
        write_json(plan, value)
        digest = hashlib.sha256(plan.read_bytes()).hexdigest()
        result = tool.main(
            [
                "dry-run",
                "--plan",
                str(plan),
                "--expected-plan-sha256",
                digest,
                "--bundle-root",
                str(self.root / "unused-bundle"),
                "--state-root",
                str(self.root / "unused-state"),
                "--evidence",
                str(self.root / "unused-evidence.json"),
            ]
        )
        self.assertEqual(result, 2)


if __name__ == "__main__":
    unittest.main()
