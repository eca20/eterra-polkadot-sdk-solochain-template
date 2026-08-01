#!/usr/bin/env python3
"""Offline lifecycle tests for the foreground pre-reset rollback supervisor."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import signal
import stat
import sys
import tempfile
import threading
import unittest
from pathlib import Path
from unittest import mock


TOOL_ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOL_ROOT))
import pre_reset_rollback_supervisor as supervisor  # noqa: E402


SOURCE_COMMIT = "a" * 40
SITE_RELEASE_VERSION = "v0.1.0-alpha.1"
FROZEN_BLOCK = {"number": 4242, "hash": "0x" + "b" * 64}


def canonical(value: dict) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")


def write_json(path: Path, value: dict, mode: int = 0o600) -> None:
    path.write_bytes(canonical(value))
    os.chmod(path, mode)


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


COMPONENT_DRIVER = r'''#!/usr/bin/env python3
import argparse, datetime, hashlib, json, os, sys
p = argparse.ArgumentParser()
p.add_argument("--plan", required=True)
p.add_argument("--plan-sha256", required=True)
p.add_argument("--component", required=True)
p.add_argument("--mode", required=True)
p.add_argument("--action", required=True)
p.add_argument("--result", required=True)
a = p.parse_args()
with open(os.environ["TEST_PRE_RESET_TRACE"], "a", encoding="utf-8") as h:
    h.write(f"component:{a.mode}:{a.action}:{a.component}\n")
if os.environ.get("TEST_FAIL_COMPONENT") == f"{a.mode}:{a.action}:{a.component}":
    sys.exit(9)
preflight = {
    "archivesReadable", "credentialsResolvable", "driverHashVerified",
    "helperHashesVerified", "noMutation", "restoreInputsVerified",
    "scriptHashesVerified", "sourcePinsVerified",
}
preparation = {
    "archivePreparationNonDestructive", "archivesPreparedAndReadOnly",
    "currentAlphaStatePreserved", "noResetApplied", "readinessIdentityBound",
    "restoreInputsVerified", "sourcePinsVerified",
}
actions = {
    "pause-v2-writes": {"noV2RpcRequired", "statePreserved", "v2WritesPaused"},
    "archive-failed-v2": {"failedV2RootArchived", "failedV2RootPreserved", "noV2RpcRequired"},
    "restore-final-backup": {"failedV2RootPreserved", "finalBackupRestored", "noV2RpcRequired"},
    "restored-smoke": {"backupIdentityMatched", "economicFlagsDisabled", "failedV2RootPreserved", "restoredComponentHealthy"},
}
archive = None
if a.mode == "execute" and a.action != "pause-v2-writes":
    archive = hashlib.sha256(("failed-v2:" + a.component).encode()).hexdigest()
value = {
    "schemaVersion": 1,
    "kind": "nexus-v2-private-alpha-pre-reset-recovery-result",
    "operationId": "replace-20260731",
    "releaseId": "nexus-v2-alpha",
    "siteReleaseVersion": "v0.1.0-alpha.1",
    "planSha256": a.plan_sha256,
    "componentId": a.component,
    "mode": a.mode,
    "action": a.action,
    "result": "passed",
    "fixtureOnly": os.environ.get("TEST_PRODUCTION_MODE") != "1",
    "mutationPerformed": a.mode in {"prepare", "execute"},
    "credentialsResolvable": True,
    "requiredResetArchivesPresent": True,
    "failedV2RootArchiveSha256": archive,
    "checks": {name: True for name in (
        preparation if a.mode == "prepare" else
        preflight if a.mode == "preflight" else actions[a.action]
    )},
    "completedAtUtc": datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).strftime("%Y-%m-%dT%H:%M:%SZ"),
}
fd = os.open(a.result, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
with os.fdopen(fd, "wb") as h:
    h.write((json.dumps(value, indent=2, sort_keys=True) + "\n").encode())
'''


WORKFLOW_DRIVER = r'''#!/usr/bin/env python3
import argparse, datetime, json, os, sys, time
p = argparse.ArgumentParser()
p.add_argument("--plan", required=True)
p.add_argument("--plan-sha256", required=True)
p.add_argument("--workflow-contract", required=True)
p.add_argument("--workflow-contract-sha256", required=True)
p.add_argument("--automatic-restore-arm", required=True)
p.add_argument("--automatic-restore-arm-sha256", required=True)
p.add_argument("--result", required=True)
a = p.parse_args()
with open(os.environ["TEST_PRE_RESET_TRACE"], "a", encoding="utf-8") as h:
    h.write("workflow\n")
if os.environ.get("TEST_WORKFLOW_SLEEP") == "1":
    time.sleep(10)
if os.environ.get("TEST_FAIL_WORKFLOW") == "1":
    sys.exit(8)
fence = {
    "zeroCurrentAndLifetimeAcceptanceInventory": os.environ.get("TEST_NONZERO_FENCE") != "1",
    "bootstrapActionsBegun": False,
    "automaticRestorePermanentlyDisabled": True,
}
fence_path = os.environ["TEST_ACCEPTANCE_FENCE"]
fd = os.open(fence_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
with os.fdopen(fd, "wb") as h:
    h.write((json.dumps(fence, indent=2, sort_keys=True) + "\n").encode())
value = {
    "schemaVersion": 1,
    "kind": "nexus-v2-private-alpha-replacement-workflow-result",
    "operationId": "replace-20260731",
    "releaseId": "nexus-v2-alpha",
    "siteReleaseVersion": "v0.1.0-alpha.1",
    "planSha256": a.plan_sha256,
    "result": "passed",
    "fixtureOnly": os.environ.get("TEST_PRODUCTION_MODE") != "1",
    "mutationPerformed": True,
    "acceptanceStartFenceWritten": True,
    "completedAtUtc": datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).strftime("%Y-%m-%dT%H:%M:%SZ"),
}
fd = os.open(a.result, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
with os.fdopen(fd, "wb") as h:
    h.write((json.dumps(value, indent=2, sort_keys=True) + "\n").encode())
'''


ACCEPTANCE_VERIFIER = r'''#!/usr/bin/env python3
import argparse, hashlib, json, os, sys
p = argparse.ArgumentParser()
p.add_argument("command")
p.add_argument("--receipt", required=True)
p.add_argument("--expected-sha256", required=True)
p.add_argument("--release-id", required=True)
p.add_argument("--source-commit", required=True)
p.add_argument("--genesis-hash", required=True)
p.add_argument("--runtime-code-sha256", required=True)
p.add_argument("--runtime-metadata-scale-sha256", required=True)
a = p.parse_args()
with open(os.environ["TEST_PRE_RESET_TRACE"], "a", encoding="utf-8") as h:
    h.write("acceptance-start-fence-verifier\n")
payload = open(a.receipt, "rb").read()
if hashlib.sha256(payload).hexdigest() != a.expected_sha256:
    sys.exit(7)
value = json.loads(payload)
if not value.get("zeroCurrentAndLifetimeAcceptanceInventory"):
    sys.exit(8)
if value.get("bootstrapActionsBegun") is not False:
    sys.exit(9)
if value.get("automaticRestorePermanentlyDisabled") is not True:
    sys.exit(10)
'''


class PreResetRollbackSupervisorTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="nexus-v2-pre-reset-supervisor-")
        self.root = Path(self.temporary.name).resolve()
        os.chmod(self.root, 0o700)
        self.trace = self.root / "trace.log"
        self.fixture_root = self.root / "offline.NONDEPLOYABLE"
        self.fixture_root.mkdir(mode=0o700)
        self.plan_path = self.root / "plan.json"
        write_json(self.plan_path, {})
        self.component_driver = self.executable("component-driver", COMPONENT_DRIVER)
        self.workflow_driver = self.executable("workflow-driver", WORKFLOW_DRIVER)
        self.verifier = self.executable("acceptance-verifier", ACCEPTANCE_VERIFIER)
        self.fence = self.root / "zero-asset-acceptance-start-fence.json"
        self.outputs = {
            "state_root": self.root / "state",
            "arm": self.root / "arm.json",
            "lease": self.root / "lease.json",
            "evidence": self.root / "evidence.json",
        }
        self.plan = self.normalized_plan()

    def tearDown(self) -> None:
        for current, directories, files in os.walk(self.root, topdown=False, followlinks=False):
            for name in files:
                path = Path(current) / name
                if not path.is_symlink():
                    os.chmod(path, stat.S_IMODE(path.stat().st_mode) | 0o600)
            for name in directories:
                path = Path(current) / name
                if not path.is_symlink():
                    os.chmod(path, stat.S_IMODE(path.stat().st_mode) | 0o700)
        self.temporary.cleanup()

    def executable(self, name: str, body: str) -> Path:
        path = self.root / name
        path.write_text(body, encoding="utf-8")
        os.chmod(path, 0o700)
        return path

    def normalized_plan(self, *, fixture_only: bool = True) -> dict:
        now = dt.datetime.now(dt.timezone.utc).replace(microsecond=0)
        artifact_hashes = {
            name: digest(self.plan_path) for name in supervisor.ARTIFACT_IDS
        }
        component_pin = {
            "path": self.component_driver,
            "sha256": digest(self.component_driver),
        }
        return {
            "path": self.plan_path,
            "sha256": digest(self.plan_path),
            "raw": {},
            "operationId": "replace-20260731",
            "releaseId": "nexus-v2-alpha",
            "siteReleaseVersion": SITE_RELEASE_VERSION,
            "sourceCommit": SOURCE_COMMIT,
            "backend": (
                supervisor.FIXTURE_BACKEND
                if fixture_only
                else supervisor.PRODUCTION_BACKEND
            ),
            "fixtureOnly": fixture_only,
            "fixtureRoot": self.fixture_root if fixture_only else None,
            "fixtureContract": {},
            "createdAt": now - dt.timedelta(seconds=2),
            "expiresAt": now + dt.timedelta(minutes=10),
            "frozenFinalizedBlock": dict(FROZEN_BLOCK),
            "sources": {},
            "supervisor": {
                "path": Path(supervisor.__file__).resolve(),
                "sha256": digest(Path(supervisor.__file__).resolve()),
            },
            "workflow": {
                "driver": {
                    "path": self.workflow_driver,
                    "sha256": digest(self.workflow_driver),
                },
                "helperPins": {},
                "contract": {"path": self.plan_path, "sha256": digest(self.plan_path)},
            },
            "acceptanceStartFence": {
                "handoffPath": self.fence,
                "verifier": {"path": self.verifier, "sha256": digest(self.verifier)},
                "genesisHash": "0x" + "c" * 64,
                "runtimeCodeSha256": "d" * 64,
                "runtimeMetadataScaleSha256": "e" * 64,
                "pollMilliseconds": 10,
            },
            "components": {
                component: {
                    "driver": dict(component_pin),
                    "helperPins": {},
                    "scriptPins": {},
                    "requiredResetArchives": {},
                }
                for component in supervisor.COMPONENTS
            },
            "artifacts": {
                name: {"path": self.plan_path, "sha256": artifact_hashes[name]}
                for name in supervisor.ARTIFACT_IDS
            },
            "boundInputs": None,
        }

    def args(self) -> argparse.Namespace:
        return argparse.Namespace(
            plan=str(self.plan_path),
            expected_plan_sha256=digest(self.plan_path),
            **{name: str(path) for name, path in self.outputs.items()},
        )

    def run_supervisor(self, environment: dict[str, str] | None = None) -> int:
        variables = {
            "TEST_PRE_RESET_TRACE": str(self.trace),
            "TEST_ACCEPTANCE_FENCE": str(self.fence),
        }
        variables.update(environment or {})
        with (
            mock.patch.object(supervisor, "validate_plan", return_value=self.plan),
            mock.patch.object(supervisor, "process_start_token", return_value="f" * 64),
            mock.patch.dict(os.environ, variables, clear=False),
        ):
            return supervisor.run_supervisor(self.args())

    def read_evidence(self) -> dict:
        return json.loads(self.outputs["evidence"].read_text(encoding="utf-8"))

    def test_success_retires_lease_and_preserves_immutable_arm(self) -> None:
        self.assertEqual(self.run_supervisor(), 0)
        evidence_hash = digest(self.outputs["evidence"])
        arm_hash = digest(self.outputs["arm"])
        evidence = supervisor.validate_retirement_evidence(
            self.outputs["evidence"],
            evidence_hash,
            self.outputs["arm"],
            arm_hash,
            expected_release_id=self.plan["releaseId"],
            expected_site_release_version=SITE_RELEASE_VERSION,
            expected_source_commit=SOURCE_COMMIT,
            allow_fixture=True,
        )
        self.assertTrue(evidence["automaticRestoreRetired"])
        self.assertEqual(evidence["acceptanceStartFenceSha256"], digest(self.fence))
        lease = json.loads(self.outputs["lease"].read_text(encoding="utf-8"))
        self.assertEqual(lease["state"], "retired")
        self.assertEqual(lease["retirementEvidenceSha256"], evidence_hash)
        self.assertEqual(evidence["automaticRestoreArmSha256"], arm_hash)
        trace = self.trace.read_text(encoding="utf-8").splitlines()
        self.assertEqual(
            trace,
            [
                "component:prepare:prepare-reset-archives:chain-media",
                "component:prepare:prepare-reset-archives:site-indexer",
                "component:preflight:preflight:chain-media",
                "component:preflight:preflight:site-indexer",
                "workflow",
                "acceptance-start-fence-verifier",
            ],
        )

    def test_preflight_failure_never_runs_workflow_or_recovery(self) -> None:
        code = self.run_supervisor(
            {"TEST_FAIL_COMPONENT": "preflight:preflight:site-indexer"}
        )
        self.assertEqual(code, 2)
        self.assertFalse(self.outputs["arm"].exists())
        evidence = self.read_evidence()
        self.assertEqual(
            evidence["outcome"],
            "pre-arm-archive-preparation-or-preflight-failed",
        )
        self.assertFalse(evidence["automaticRestorePerformed"])
        trace = self.trace.read_text(encoding="utf-8")
        self.assertNotIn("workflow", trace)
        self.assertNotIn("component:execute", trace)

    def test_archive_preparation_failure_never_arms_or_claims_no_mutation(self) -> None:
        code = self.run_supervisor(
            {
                "TEST_FAIL_COMPONENT": (
                    "prepare:prepare-reset-archives:site-indexer"
                )
            }
        )
        self.assertEqual(code, 2)
        self.assertFalse(self.outputs["arm"].exists())
        evidence = self.read_evidence()
        self.assertEqual(
            evidence["outcome"],
            "pre-arm-archive-preparation-or-preflight-failed",
        )
        self.assertEqual(
            set(evidence["archivePreparationResultSha256"]),
            {"chain-media"},
        )
        self.assertNotIn("component:preflight", self.trace.read_text())

    def test_workflow_failure_runs_both_recovery_lanes_in_closed_order(self) -> None:
        self.assertEqual(self.run_supervisor({"TEST_FAIL_WORKFLOW": "1"}), 3)
        evidence = self.read_evidence()
        self.assertEqual(evidence["outcome"], "automatic-recovery-complete")
        self.assertTrue(evidence["automaticRestorePerformed"])
        trace = self.trace.read_text(encoding="utf-8").splitlines()
        recovery = [line for line in trace if line.startswith("component:execute")]
        expected = [
            f"component:execute:{action}:{component}"
            for action in supervisor.RECOVERY_ACTIONS
            for component in supervisor.COMPONENTS
        ]
        self.assertEqual(recovery, expected)

    def test_recovery_failure_is_immutable_evidence_and_other_lane_continues(self) -> None:
        code = self.run_supervisor(
            {
                "TEST_FAIL_WORKFLOW": "1",
                "TEST_FAIL_COMPONENT": "execute:restore-final-backup:site-indexer",
            }
        )
        self.assertEqual(code, 4)
        evidence = self.read_evidence()
        self.assertEqual(evidence["outcome"], "automatic-recovery-failed")
        self.assertEqual(
            evidence["recovery"]["restore-final-backup"]["site-indexer"]["status"],
            "failed",
        )
        self.assertEqual(
            evidence["recovery"]["restored-smoke"]["chain-media"]["status"],
            "passed",
        )
        self.assertEqual(stat.S_IMODE(self.outputs["evidence"].stat().st_mode), 0o400)

    def test_nonzero_acceptance_fence_is_rejected_and_recovers(self) -> None:
        self.assertEqual(self.run_supervisor({"TEST_NONZERO_FENCE": "1"}), 3)
        evidence = self.read_evidence()
        self.assertFalse(evidence["automaticRestoreRetired"])
        self.assertTrue(evidence["automaticRestorePerformed"])
        self.assertIn("acceptance-start fence verification failed", evidence["trigger"]["message"])

    def test_signal_during_workflow_runs_recovery(self) -> None:
        timer = threading.Timer(1.5, lambda: os.kill(os.getpid(), signal.SIGTERM))
        timer.start()
        try:
            code = self.run_supervisor({"TEST_WORKFLOW_SLEEP": "1"})
        finally:
            timer.cancel()
            timer.join(timeout=2)
        self.assertEqual(code, 3)
        evidence = self.read_evidence()
        self.assertEqual(evidence["trigger"]["type"], "signal")
        self.assertEqual(evidence["trigger"]["signal"], signal.SIGTERM)
        self.assertTrue(evidence["automaticRestorePerformed"])

    def test_stale_dead_and_hash_drifted_arms_fail_closed(self) -> None:
        self.assertEqual(self.run_supervisor({"TEST_FAIL_WORKFLOW": "1"}), 3)
        arm_path = self.outputs["arm"]
        arm_hash = digest(arm_path)
        with self.assertRaisesRegex(supervisor.SupervisorError, "stale"):
            supervisor.validate_arm(
                arm_path,
                arm_hash,
                expected_release_id=self.plan["releaseId"],
                expected_site_release_version=SITE_RELEASE_VERSION,
                expected_source_commit=SOURCE_COMMIT,
                full_binding=False,
                allow_fixture=True,
                now=self.plan["expiresAt"] + dt.timedelta(seconds=1),
            )
        with mock.patch.object(
            supervisor,
            "process_start_token",
            side_effect=supervisor.SupervisorError("supervisor process is not live"),
        ):
            with self.assertRaisesRegex(supervisor.SupervisorError, "not live"):
                supervisor.validate_arm(
                    arm_path,
                    arm_hash,
                    expected_release_id=self.plan["releaseId"],
                    expected_site_release_version=SITE_RELEASE_VERSION,
                    expected_source_commit=SOURCE_COMMIT,
                    full_binding=False,
                    allow_fixture=True,
                )
        with self.assertRaisesRegex(supervisor.SupervisorError, "hash mismatch"):
            supervisor.validate_arm(
                arm_path,
                "0" * 64,
                expected_release_id=self.plan["releaseId"],
                expected_site_release_version=SITE_RELEASE_VERSION,
                expected_source_commit=SOURCE_COMMIT,
                full_binding=False,
                allow_fixture=True,
            )

    def test_existing_output_is_never_overwritten(self) -> None:
        self.outputs["arm"].write_text("preserve", encoding="utf-8")
        with mock.patch.object(supervisor, "validate_plan", return_value=self.plan):
            with self.assertRaisesRegex(supervisor.closure.ClosureError, "overwrite"):
                supervisor.run_supervisor(self.args())
        self.assertEqual(self.outputs["arm"].read_text(encoding="utf-8"), "preserve")
        self.assertFalse(self.trace.exists())

    def test_production_requires_explicit_confirmation_before_preflight(self) -> None:
        self.plan = self.normalized_plan(fixture_only=False)
        code = self.run_supervisor({"TEST_PRODUCTION_MODE": "1"})
        self.assertEqual(code, 2)
        self.assertFalse(self.trace.exists())
        self.assertIn("PRIVATE_ALPHA_ROLLBACK_ONLY", self.read_evidence()["trigger"]["message"])


if __name__ == "__main__":
    unittest.main()
