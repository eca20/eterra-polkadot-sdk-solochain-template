#!/usr/bin/env python3

from __future__ import annotations

import contextlib
import datetime as dt
import hashlib
import importlib.util
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from typing import Any, Iterator


SCRIPT_DIR = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location(
    "nexus_v2_post_cutover_coordinator",
    SCRIPT_DIR / "nexus-v2-post-cutover-coordinator.py",
)
assert SPEC is not None and SPEC.loader is not None
tool = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(tool)
safety = tool.safety


RELEASE_ID = "nexus-v2-post-cutover-test"
BLOCK_HASH = "0x" + ("7" * 64)


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def file_hash(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run_git(root: Path, *args: str) -> str:
    completed = subprocess.run(
        ["git", "-C", str(root), *args],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=True,
    )
    return completed.stdout.strip()


def economic_gates(source_commit: str, block_number: int = 500) -> dict[str, Any]:
    return {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-economic-gates",
        "releaseId": RELEASE_ID,
        "sourceCommit": source_commit,
        "observedAtFinalizedBlock": {
            "number": block_number,
            "hash": BLOCK_HASH,
        },
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
        "magic": {
            "seedTrainingOnly": True,
            "productionTransferEnabled": False,
        },
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
            "futurePaidSurface": False,
        },
    }


def inventory(
    source_commit: str,
    block_number: int = 500,
    **overrides: int,
) -> dict[str, Any]:
    counts = {name: 0 for name in safety.ACCEPTANCE_COUNT_FIELDS}
    counts.update(overrides)
    return {
        "schemaVersion": 1,
        "kind": "nexus-v2-acceptance-inventory",
        "releaseId": RELEASE_ID,
        "sourceCommit": source_commit,
        "observedAtFinalizedBlock": {
            "number": block_number,
            "hash": BLOCK_HASH,
        },
        "counts": counts,
    }


MOCK_DRIVER = r"""#!/usr/bin/env python3
import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path

p = argparse.ArgumentParser()
for name in (
    "component", "action", "mode", "operation-id", "plan", "plan-sha256",
    "manifest", "bundle-root", "restore-evidence", "observation",
    "economic-gates", "acceptance-inventory", "result",
):
    p.add_argument("--" + name, required=True)
a = p.parse_args()
plan = json.loads(Path(a.plan).read_text())
component = next(value for value in plan["components"] if value["id"] == a.component)
source_commits = {
    value["id"]: value["expectedCommit"] for value in component["sourcePins"]
}
archive_names = sorted(component["requiredResetArchives"])
missing = os.environ.get("MOCK_MISSING_ARCHIVE", "")
archives = {name: missing not in {name, a.component} for name in archive_names}
dry_checks = {
    "post-cutover-smoke": {
        "sourceIdentityPinned", "credentialsResolvable",
        "requiredResetArchivesPresent", "smokeProbePlanned",
    },
    "pause-v2-writes": {
        "sourceIdentityPinned", "credentialsResolvable",
        "requiredResetArchivesPresent", "pausePlanSafe", "restoreExcluded",
    },
    "archive-failed-v2": {
        "sourceIdentityPinned", "credentialsResolvable",
        "requiredResetArchivesPresent", "archivePlanSafe", "restoreExcluded",
    },
    "restore-final-backup": {
        "sourceIdentityPinned", "credentialsResolvable",
        "requiredResetArchivesPresent", "finalBackupInputsMatched",
        "failedV2ArchiveRequired", "existingRestoreScriptPinned",
        "existingDeployScriptsPinned", "restorePlanSafe",
    },
    "restored-smoke": {
        "sourceIdentityPinned", "credentialsResolvable",
        "requiredResetArchivesPresent", "restoredSmokeProbePlanned",
    },
}
execute_checks = {
    "post-cutover-smoke": {
        "sourceIdentityPinned": True,
        "requiredResetArchivesPresent": True,
        "smokePassed": os.environ.get("MOCK_POST_SMOKE_FAIL", "") not in {
            "1", a.component
        },
    },
    "pause-v2-writes": {
        "sourceIdentityPinned": True,
        "requiredResetArchivesPresent": True,
        "v2WritesPaused": True,
        "statePreserved": True,
        "restoreNotAttempted": True,
    },
    "archive-failed-v2": {
        "sourceIdentityPinned": True,
        "requiredResetArchivesPresent": True,
        "failedV2RootArchived": True,
        "archiveManifestImmutable": True,
    },
    "restore-final-backup": {
        "sourceIdentityPinned": True,
        "requiredResetArchivesPresent": True,
        "failedV2RootArchivePresent": True,
        "finalBackupHashesVerified": True,
        "restoreEvidenceMatched": True,
        "existingRestoreScriptUsed": True,
        "existingDeployScriptsUsed": True,
        "restoreCompleted": True,
    },
    "restored-smoke": {
        "sourceIdentityPinned": True,
        "requiredResetArchivesPresent": True,
        "failedV2RootArchivePresent": True,
        "componentHealthy": True,
        "backupIdentityReadback": True,
        "economicFlagsDisabled": True,
    },
}
if a.mode == "dry-run":
    checks = {name: True for name in dry_checks[a.action]}
    remote = False
    marker = None
    failed_archive = None
else:
    checks = execute_checks[a.action]
    remote = True
    marker = hashlib.sha256(
        f"{a.operation_id}:{a.component}:{a.action}".encode()
    ).hexdigest()
    failed_archive = (
        hashlib.sha256(f"failed:{a.component}".encode()).hexdigest()
        if a.action in {
            "archive-failed-v2", "restore-final-backup", "restored-smoke"
        }
        else None
    )
value = {
    "schemaVersion": 1,
    "kind": "nexus-v2-private-alpha-component-action-result",
    "operationId": a.operation_id,
    "planSha256": a.plan_sha256,
    "releaseId": plan["releaseId"],
    "sourceCommit": plan["sourceCommit"],
    "componentSourceCommits": source_commits,
    "componentId": a.component,
    "action": a.action,
    "mode": a.mode,
    "result": "passed",
    "remoteActionsExecuted": remote,
    "alreadyApplied": False,
    "requiredResetArchives": archives,
    "failedV2RootArchiveSha256": failed_archive,
    "remoteIdempotencyMarkerSha256": marker,
    "checks": checks,
    "completedAtUtc": dt.datetime.now(dt.timezone.utc).isoformat(),
}
Path(a.result).write_text(json.dumps(value, sort_keys=True))
trace = os.environ.get("MOCK_DRIVER_TRACE")
if trace:
    with Path(trace).open("a") as handle:
        handle.write(f"{a.mode}:{a.component}:{a.action}\n")
"""


class CoordinatorTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp_context = tempfile.TemporaryDirectory(
            prefix="nexus-v2-post-cutover-test-"
        )
        self.root = Path(self.temp_context.name)
        self.chain = self.make_repo(
            "chain",
            {
                "driver.py": MOCK_DRIVER,
                "restore.sh": "#!/bin/sh\nexit 0\n",
                "deploy-node.sh": "#!/bin/sh\nexit 0\n",
                "deploy-media.sh": "#!/bin/sh\nexit 0\n",
                "status.sh": "#!/bin/sh\nexit 0\n",
            },
        )
        self.media = self.make_repo(
            "media",
            {"source-marker.sh": "#!/bin/sh\nexit 0\n"},
        )
        self.site = self.make_repo(
            "site",
            {
                "restore.sh": "#!/bin/sh\nexit 0\n",
                "deploy-site.sh": "#!/bin/sh\nexit 0\n",
                "status.sh": "#!/bin/sh\nexit 0\n",
            },
        )
        self.chain_commit = run_git(self.chain, "rev-parse", "HEAD")
        self.media_commit = run_git(self.media, "rev-parse", "HEAD")
        self.site_commit = run_git(self.site, "rev-parse", "HEAD")
        self.gates = self.root / "economic-gates.json"
        self.acceptance = self.root / "acceptance-inventory.json"
        write_json(self.gates, economic_gates(self.chain_commit))
        write_json(self.acceptance, inventory(self.chain_commit))
        self.bundle, self.manifest = self.make_bundle()
        self.restore = self.root / "restore-evidence.json"
        self.make_restore_evidence()
        self.observation = self.root / "post-cutover-observation.json"
        self.plan = self.root / "coordinator-plan.json"
        self.write_observation()
        self.write_plan()
        self.state = self.root / "state"
        self.evidence = self.root / "evidence.json"
        self.trace = self.root / "driver-trace.txt"

    def tearDown(self) -> None:
        self.temp_context.cleanup()

    def make_repo(self, name: str, files: dict[str, str]) -> Path:
        root = self.root / name
        root.mkdir()
        run_git(root, "init", "-q")
        run_git(root, "config", "user.name", "Nexus Test")
        run_git(root, "config", "user.email", "nexus-test@example.invalid")
        for relative, contents in files.items():
            path = root / relative
            path.write_text(contents, encoding="utf-8")
            path.chmod(0o755)
        run_git(root, "add", ".")
        run_git(root, "commit", "-qm", "fixture")
        return root

    def make_bundle(self) -> tuple[Path, Path]:
        bundle = self.root / "bundle"
        bundle.mkdir()
        arguments: list[str] = []
        for group, names in sorted(safety.REQUIRED_ARTIFACTS.items()):
            for name in sorted(names):
                relative = Path("artifacts") / group / f"{name}.bin"
                path = bundle / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                if (group, name) == ("config", "economic-gates"):
                    path.write_bytes(self.gates.read_bytes())
                else:
                    path.write_bytes(f"{group}:{name}".encode())
                arguments.append(f"{group}:{name}:{relative.as_posix()}")
        manifest = bundle / "backup-manifest.json"
        argv = [
            "backup-manifest",
            "--bundle-root",
            str(bundle),
            "--release-id",
            RELEASE_ID,
            "--source-commit",
            self.chain_commit,
            "--created-at",
            "2026-07-30T18:00:00Z",
            "--output",
            str(manifest),
        ]
        for value in arguments:
            argv.extend(["--artifact", value])
        self.assertEqual(safety.main(argv), 0)
        return bundle, manifest

    def make_restore_evidence(self) -> None:
        write_json(
            self.restore,
            {
                "schemaVersion": 1,
                "kind": "nexus-v2-isolated-restore-evidence",
                "releaseId": RELEASE_ID,
                "sourceCommit": self.chain_commit,
                "backupManifestSha256": file_hash(self.manifest),
                "restoreDriverSha256": "1" * 64,
                "portsPlanSha256": "2" * 64,
                "restoreLogSha256": "3" * 64,
                "restoreResultSha256": "4" * 64,
                "isolatedRoot": str(self.root / "nexus-v2-isolated-restore-test"),
                "bindHost": "127.0.0.1",
                "ports": {
                    "nodeRpc": 19944,
                    "nodeP2p": 31333,
                    "media": 14000,
                    "ipfsApi": 15001,
                    "ipfsGateway": 18080,
                    "indexer": 18788,
                },
                "result": "passed",
                "completedAtUtc": "2026-07-30T19:00:00Z",
                "liveAlphaTouched": False,
            },
        )

    def source_commits(self) -> dict[str, dict[str, str]]:
        return {
            "chain-media": {
                "chain": self.chain_commit,
                "media": self.media_commit,
            },
            "site-indexer": {
                "chain": self.chain_commit,
                "site": self.site_commit,
            },
        }

    def write_observation(
        self,
        *,
        observed_at: dt.datetime | None = None,
        block_number: int = 500,
    ) -> None:
        observed_at = observed_at or dt.datetime.now(dt.timezone.utc)
        paused_at = observed_at - dt.timedelta(seconds=30)
        write_json(
            self.observation,
            {
                "schemaVersion": 1,
                "kind": "nexus-v2-private-alpha-post-cutover-rollback-observation",
                "releaseId": RELEASE_ID,
                "sourceCommit": self.chain_commit,
                "componentSourceCommits": self.source_commits(),
                "observedAtFinalizedBlock": {
                    "number": block_number,
                    "hash": BLOCK_HASH,
                },
                "observedAtUtc": observed_at.isoformat(),
                "writeBarrier": {
                    "mode": "AllV2WritesPaused",
                    "chainWritesPaused": True,
                    "authorityResultsPaused": True,
                    "webMutationsPaused": True,
                    "gameplaySessionIngressPaused": True,
                    "inventoryObservedAfterPause": True,
                    "pausedAtUtc": paused_at.isoformat(),
                    "stabilityWindowSeconds": 30,
                    "evidenceSha256": "8" * 64,
                },
                "economicGatesSha256": file_hash(self.gates),
                "acceptanceInventorySha256": file_hash(self.acceptance),
            },
        )

    def component_plan(
        self,
        component_id: str,
    ) -> dict[str, Any]:
        readiness_hash = "9" * 64
        driver = self.chain / "driver.py"
        if component_id == "chain-media":
            source_pins = [
                {
                    "id": "chain",
                    "root": str(self.chain),
                    "expectedCommit": self.chain_commit,
                },
                {
                    "id": "media",
                    "root": str(self.media),
                    "expectedCommit": self.media_commit,
                },
            ]
            archives = {
                name: f"/opt/eterra-alpha/archive/nexus-v2-fresh-reset/{readiness_hash}/{name}"
                for name in ("node", "media")
            }
            pin_paths = {
                "restoreState": ("chain", self.chain / "restore.sh"),
                "deployNode": ("chain", self.chain / "deploy-node.sh"),
                "deployMedia": ("chain", self.chain / "deploy-media.sh"),
                "status": ("chain", self.chain / "status.sh"),
            }
        else:
            source_pins = [
                {
                    "id": "chain",
                    "root": str(self.chain),
                    "expectedCommit": self.chain_commit,
                },
                {
                    "id": "site",
                    "root": str(self.site),
                    "expectedCommit": self.site_commit,
                },
            ]
            archives = {
                "site": f"/opt/eterra-alpha/archive/nexus-v2-fresh-reset/{readiness_hash}/site"
            }
            pin_paths = {
                "restoreState": ("site", self.site / "restore.sh"),
                "deploySite": ("site", self.site / "deploy-site.sh"),
                "status": ("site", self.site / "status.sh"),
            }
        return {
            "id": component_id,
            "sourcePins": source_pins,
            "driverSourceId": "chain",
            "driverPath": driver.relative_to(self.chain).as_posix(),
            "driverSha256": file_hash(driver),
            "requiredResetArchives": archives,
            "scriptPins": {
                role: {
                    "sourceId": source_id,
                    "path": path.relative_to(
                        self.chain if source_id == "chain" else self.site
                    ).as_posix(),
                    "sha256": file_hash(path),
                }
                for role, (source_id, path) in pin_paths.items()
            },
        }

    def write_plan(self, value: dict[str, Any] | None = None) -> None:
        now = dt.datetime.now(dt.timezone.utc)
        plan = value or {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-post-cutover-coordinator-plan",
            "operationId": "nexus-v2-post-cutover-test",
            "releaseId": RELEASE_ID,
            "sourceCommit": self.chain_commit,
            "freshResetReadinessSha256": "9" * 64,
            "finalBackupManifestSha256": file_hash(self.manifest),
            "restoreEvidenceSha256": file_hash(self.restore),
            "postCutoverObservationSha256": file_hash(self.observation),
            "coordinatorSha256": file_hash(
                SCRIPT_DIR / "nexus-v2-post-cutover-coordinator.py"
            ),
            "maxObservationAgeSeconds": 600,
            "automaticRestoreApproved": True,
            "paidOrPublicActivationAuthorized": False,
            "createdAtUtc": now.isoformat(),
            "expiresAtUtc": (now + dt.timedelta(minutes=15)).isoformat(),
            "components": [
                self.component_plan("chain-media"),
                self.component_plan("site-indexer"),
            ],
        }
        write_json(self.plan, plan)

    @contextlib.contextmanager
    def environment(self, **overrides: str) -> Iterator[None]:
        previous = os.environ.copy()
        os.environ["NEXUS_V2_ROLLBACK_PLAN_SHA256"] = file_hash(self.plan)
        os.environ["MOCK_DRIVER_TRACE"] = str(self.trace)
        os.environ.update(overrides)
        try:
            yield
        finally:
            os.environ.clear()
            os.environ.update(previous)

    def argv(
        self,
        mode: str,
        *,
        state: Path | None = None,
        evidence: Path | None = None,
    ) -> list[str]:
        return [
            "--plan",
            str(self.plan),
            "--manifest",
            str(self.manifest),
            "--bundle-root",
            str(self.bundle),
            "--restore-evidence",
            str(self.restore),
            "--observation",
            str(self.observation),
            "--economic-gates",
            str(self.gates),
            "--acceptance-inventory",
            str(self.acceptance),
            "--state-dir",
            str(state or self.state),
            "--evidence",
            str(evidence or self.evidence),
            mode,
        ]

    def test_validate_only_invokes_no_driver_and_dry_run_covers_every_action(self) -> None:
        with self.environment():
            self.assertEqual(tool.main(self.argv("--validate-only")), 0)
        self.assertFalse(self.trace.exists())

        dry_state = self.root / "dry-state"
        dry_evidence = self.root / "dry-evidence.json"
        with self.environment():
            self.assertEqual(
                tool.main(
                    self.argv(
                        "--dry-run",
                        state=dry_state,
                        evidence=dry_evidence,
                    )
                ),
                0,
            )
        trace = self.trace.read_text().splitlines()
        self.assertEqual(len(trace), len(tool.ACTIONS) * 2)
        self.assertTrue(all(line.startswith("dry-run:") for line in trace))
        self.assertEqual(
            json.loads(dry_evidence.read_text())["decision"],
            "dry-run-complete",
        )

    def test_pre_acceptance_failure_archives_both_hosts_before_restore_and_is_idempotent(self) -> None:
        with self.environment(MOCK_POST_SMOKE_FAIL="1"):
            self.assertEqual(tool.main(self.argv("--execute")), 0)
            trace_before = self.trace.read_text()
            (self.state / "final-evidence.marker.json").unlink()
            self.assertEqual(tool.main(self.argv("--execute")), 0)
            self.assertTrue((self.state / "final-evidence.marker.json").exists())
            self.assertEqual(tool.main(self.argv("--execute")), 0)
        self.assertEqual(self.trace.read_text(), trace_before)
        evidence = json.loads(self.evidence.read_text())
        self.assertEqual(evidence["decision"], "pre-acceptance-automatic-restore")
        self.assertTrue(evidence["automaticRestorePerformed"])
        trace = trace_before.splitlines()
        archive_positions = [
            trace.index(f"execute:{component}:archive-failed-v2")
            for component in tool.EXPECTED_COMPONENTS
        ]
        restore_positions = [
            trace.index(f"execute:{component}:restore-final-backup")
            for component in tool.EXPECTED_COMPONENTS
        ]
        self.assertLess(max(archive_positions), min(restore_positions))

    def test_post_acceptance_failure_pauses_and_never_restores(self) -> None:
        write_json(
            self.acceptance,
            inventory(self.chain_commit, lifetimeCardsV2Created=1),
        )
        self.write_observation()
        self.write_plan()
        with self.environment(MOCK_POST_SMOKE_FAIL="1"):
            self.assertEqual(tool.main(self.argv("--execute")), 0)
        evidence = json.loads(self.evidence.read_text())
        self.assertEqual(
            evidence["decision"],
            "post-acceptance-pause-and-forward-fix",
        )
        trace = self.trace.read_text()
        self.assertIn("execute:chain-media:pause-v2-writes", trace)
        self.assertIn("execute:site-indexer:pause-v2-writes", trace)
        self.assertNotIn("execute:chain-media:restore-final-backup", trace)
        self.assertNotIn("execute:site-indexer:restore-final-backup", trace)

    def test_stale_or_mixed_observations_fail_before_driver(self) -> None:
        self.write_observation(
            observed_at=dt.datetime.now(dt.timezone.utc) - dt.timedelta(hours=1)
        )
        self.write_plan()
        with self.environment():
            self.assertEqual(tool.main(self.argv("--execute")), 2)
        self.assertFalse(self.trace.exists())

        self.acceptance = self.root / "mixed-acceptance.json"
        write_json(self.acceptance, inventory(self.chain_commit, block_number=501))
        self.write_observation()
        self.write_plan()
        with self.environment():
            self.assertEqual(
                tool.main(
                    self.argv(
                        "--execute",
                        state=self.root / "mixed-state",
                        evidence=self.root / "mixed-evidence.json",
                    )
                ),
                2,
            )
        self.assertFalse(self.trace.exists())

    def test_enabled_economics_and_missing_archives_fail_closed(self) -> None:
        unsafe = economic_gates(self.chain_commit)
        unsafe["legacyEconomy"]["purchaseEnabled"] = True
        write_json(self.gates, unsafe)
        self.write_observation()
        self.write_plan()
        with self.environment():
            self.assertEqual(tool.main(self.argv("--execute")), 2)
        self.assertFalse(self.trace.exists())

        write_json(self.gates, economic_gates(self.chain_commit))
        unsafe_observation = json.loads(self.observation.read_text())
        unsafe_observation["economicGatesSha256"] = file_hash(self.gates)
        unsafe_observation["writeBarrier"]["webMutationsPaused"] = False
        write_json(self.observation, unsafe_observation)
        self.write_plan()
        with self.environment():
            self.assertEqual(
                tool.main(
                    self.argv(
                        "--execute",
                        state=self.root / "barrier-state",
                        evidence=self.root / "barrier-evidence.json",
                    )
                ),
                2,
            )
        self.assertFalse(self.trace.exists())

        self.write_observation()
        self.write_plan()
        with self.environment(MOCK_MISSING_ARCHIVE="site"):
            self.assertEqual(
                tool.main(
                    self.argv(
                        "--execute",
                        state=self.root / "missing-state",
                        evidence=self.root / "missing-evidence.json",
                    )
                ),
                2,
            )

    def test_backup_restore_plan_component_and_script_pins_are_exact(self) -> None:
        plan = json.loads(self.plan.read_text())
        plan["restoreEvidenceSha256"] = "0" * 64
        self.write_plan(plan)
        with self.environment():
            self.assertEqual(tool.main(self.argv("--validate-only")), 2)

        self.write_plan()
        (self.media / "untracked").write_text("dirty", encoding="utf-8")
        with self.environment():
            self.assertEqual(tool.main(self.argv("--validate-only")), 2)

        (self.media / "untracked").unlink()
        plan = json.loads(self.plan.read_text())
        plan["components"][1]["sourcePins"][1]["expectedCommit"] = "0" * 40
        self.write_plan(plan)
        with self.environment():
            self.assertEqual(tool.main(self.argv("--validate-only")), 2)

    def test_plan_hash_closed_schema_and_duplicate_fields_are_rejected(self) -> None:
        with self.environment():
            os.environ["NEXUS_V2_ROLLBACK_PLAN_SHA256"] = "0" * 64
            self.assertEqual(tool.main(self.argv("--validate-only")), 2)

        value = json.loads(self.plan.read_text())
        value["unreviewedAction"] = "restore-anything"
        self.write_plan(value)
        with self.environment():
            self.assertEqual(tool.main(self.argv("--validate-only")), 2)

        valid = json.loads(self.plan.read_text())
        valid.pop("unreviewedAction")
        duplicate = (
            '{"schemaVersion":1,' + json.dumps(valid, sort_keys=True)[1:]
        )
        self.plan.write_text(duplicate, encoding="utf-8")
        with self.environment():
            self.assertEqual(tool.main(self.argv("--validate-only")), 2)


if __name__ == "__main__":
    unittest.main()
