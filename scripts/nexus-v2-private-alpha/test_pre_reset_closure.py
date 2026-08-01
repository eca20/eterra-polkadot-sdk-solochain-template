#!/usr/bin/env python3
"""Offline tests for the short-lived pre-reset closure handoff."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


TOOL_ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOL_ROOT))
import pre_reset_closure as closure  # noqa: E402
import pre_reset_rollback_supervisor as supervisor  # noqa: E402


SOURCE_COMMIT = "a" * 40
DEPLOYED_COMMIT = "b" * 40
FROZEN_BLOCK = {"number": 4242, "hash": "0x" + "c" * 64}


def canonical(value: dict) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path: Path, value: dict, mode: int = 0o600) -> None:
    path.write_bytes(canonical(value))
    os.chmod(path, mode)


def fake_driver_body(*, wrong_block: bool = False) -> str:
    offset = " + 1" if wrong_block else ""
    return f'''#!/usr/bin/env python3
import argparse, json, os
p = argparse.ArgumentParser()
p.add_argument("--action", required=True)
p.add_argument("--transaction-id", required=True)
p.add_argument("--release-id", required=True)
p.add_argument("--source-commit", required=True)
p.add_argument("--component-source-commit", required=True)
p.add_argument("--role", required=True)
p.add_argument("--target", required=True)
p.add_argument("--bundle-root", required=True)
p.add_argument("--result", required=True)
p.add_argument("--frozen-block-number", required=True, type=int)
p.add_argument("--frozen-block-hash", required=True)
p.add_argument("--artifact", action="append", default=[])
a = p.parse_args()
checks = {json.dumps({role: sorted(checks) for role, checks in closure.VERIFY_CHECKS.items()})}
value = {{
  "schemaVersion": 1,
  "kind": "nexus-v2-private-alpha-final-freeze-component-result",
  "transactionId": a.transaction_id,
  "releaseId": a.release_id,
  "sourceCommit": a.source_commit,
  "role": a.role,
  "action": a.action,
  "target": a.target,
  "dryRun": False,
  "liveMutationPerformed": False,
  "planned": False,
  "frozenAtUtc": "2026-07-31T20:00:00Z",
  "frozenFinalizedBlock": {{"number": a.frozen_block_number{offset}, "hash": a.frozen_block_hash}},
  "checks": {{name: True for name in checks[a.role]}},
  "artifacts": [],
}}
payload = (json.dumps(value, indent=2, sort_keys=True) + "\\n").encode()
fd = os.open(a.result, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
with os.fdopen(fd, "wb") as handle:
    handle.write(payload)
'''


def make_receipt(observed: str, arm: Path) -> dict:
    return {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-pre-reset-closure-handoff",
        "releaseId": "nexus-v2-alpha",
        "sourceCommit": SOURCE_COMMIT,
        "replacementLockSha256": "1" * 64,
        "resetReadinessSha256": "2" * 64,
        "finalFreezeEvidenceSha256": "3" * 64,
        "backupManifestSha256": "4" * 64,
        "restoreEvidenceSha256": "5" * 64,
        "migrationEvidenceSha256": "6" * 64,
        "automaticRestoreArmSha256": digest(arm),
        "automaticRestoreArmPath": str(arm),
        "observedAtUtc": observed,
        "automaticRestoreArmed": True,
        "mutationPerformed": False,
        "components": {
            role: {
                "driverSha256": "7" * 64,
                "verifyFrozenResultSha256": hashlib.sha256(role.encode()).hexdigest(),
                "stopped": True,
            }
            for role in closure.RECEIPT_COMPONENTS
        },
        "protectedListeners": closure.PROTECTED_LISTENERS,
    }


class PreResetClosureTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="nexus-v2-pre-reset-closure-")
        # macOS exposes /var as a system symlink; use the canonical private path
        # so the production path-component checks remain strict.
        self.root = Path(self.temporary.name).resolve()
        os.chmod(self.root, 0o700)
        self.bundle = self.root / "bundle"
        self.bundle.mkdir(mode=0o700)

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
        os.chmod(self.root, 0o700)
        self.temporary.cleanup()

    def make_plan(self, *, wrong_block: bool = False) -> tuple[Path, closure.Plan]:
        driver = self.root / ("wrong-driver" if wrong_block else "driver")
        driver.write_text(fake_driver_body(wrong_block=wrong_block), encoding="utf-8")
        os.chmod(driver, 0o700)
        driver_hash = digest(driver)
        commits = {name: SOURCE_COMMIT for name in closure.SOURCE_COMPONENTS}
        value = {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-final-freeze-plan",
            "transactionId": "freeze-20260731",
            "releaseId": "nexus-v2-alpha",
            "sourceCommit": SOURCE_COMMIT,
            "componentSourceCommits": commits,
            "stabilityWindowSeconds": 30,
            "preV16SourceRuntime": {
                "deployedSourceCommit": DEPLOYED_COMMIT,
                "specVersion": 1,
                "metadataVersion": 14,
                "tcgPalletIndex": 9,
                "tcgStorageVersion": 14,
                "flowPalletIndex": 29,
            },
            "authorizations": {
                "automaticResumeOnFailure": False,
                "finalFreezeAndBackup": True,
                "freshReset": False,
                "paidOrPublicActivation": False,
                "privateAlphaOnly": True,
            },
            "components": {
                role: {
                    "driver": str(driver),
                    "driverSha256": driver_hash,
                    "target": role,
                    "arguments": [],
                }
                for role in closure.ROLES
            },
        }
        path = self.root / ("wrong-plan.json" if wrong_block else "plan.json")
        write_json(path, value)
        return path, closure.validate_plan(str(path), digest(path))

    def make_args(self, plan_path: Path, state: Path, output: Path) -> argparse.Namespace:
        arguments = [
            "create",
            "--plan",
            str(plan_path),
            "--expected-plan-sha256",
            digest(plan_path),
            "--bundle-root",
            str(self.bundle),
            "--state-root",
            str(state),
        ]
        for name, marker in (
            ("replacement-lock", "1"),
            ("reset-readiness", "2"),
            ("final-freeze-evidence", "3"),
            ("backup-manifest", "4"),
            ("restore-evidence", "5"),
            ("migration-evidence", "6"),
            ("automatic-restore-arm", "0"),
        ):
            path = self.root / name
            if name == "automatic-restore-arm" and not path.exists():
                write_json(path, {})
            expected = digest(path) if name == "automatic-restore-arm" else marker * 64
            arguments.extend([f"--{name}", str(path), f"--expected-{name}-sha256", expected])
        arguments.extend(
            [
                "--selected-deployment-environment",
                str(self.root / "node.env"),
                "--selected-site-deployment-environment",
                str(self.root / "site.env"),
                "--output",
                str(output),
            ]
        )
        return closure.build_parser().parse_args(arguments)

    def bound_inputs(self) -> closure.BoundInputs:
        return closure.BoundInputs(
            replacement_lock_sha256="1" * 64,
            reset_readiness_sha256="2" * 64,
            final_freeze_evidence_sha256="3" * 64,
            backup_manifest_sha256="4" * 64,
            restore_evidence_sha256="5" * 64,
            migration_evidence_sha256="6" * 64,
            automatic_restore_arm_sha256=digest(self.root / "automatic-restore-arm"),
            frozen_block=FROZEN_BLOCK,
            pinned_files=(),
        )

    @staticmethod
    def prepared_drivers(plan: closure.Plan) -> dict[str, closure.PreparedDriver]:
        return {
            role: closure.PreparedDriver(plan.components[role].driver, os.environ.copy())
            for role in closure.ROLES
        }

    @staticmethod
    def live_arm(receipt: dict, issued_at: str) -> dict:
        return {
            "issuedAtUtc": issued_at,
            **{
                field: receipt[field]
                for field in (
                    "replacementLockSha256",
                    "resetReadinessSha256",
                    "finalFreezeEvidenceSha256",
                    "backupManifestSha256",
                    "restoreEvidenceSha256",
                    "migrationEvidenceSha256",
                )
            },
        }

    def test_create_runs_all_five_pinned_drivers_and_writes_exact_receipt(self) -> None:
        plan_path, plan = self.make_plan()
        state = self.root / "closure-state"
        output = self.root / "closure.json"
        args = self.make_args(plan_path, state, output)
        with (
            mock.patch.object(closure, "validate_bound_inputs", return_value=self.bound_inputs()),
            mock.patch.object(
                closure,
                "prepare_immutable_drivers",
                return_value=self.prepared_drivers(plan),
            ),
            mock.patch.object(
                supervisor,
                "validate_arm",
                side_effect=lambda *_args, **_kwargs: self.live_arm(
                    json.loads(output.read_text(encoding="utf-8")),
                    json.loads(output.read_text(encoding="utf-8"))["observedAtUtc"],
                ),
            ),
        ):
            closure.command_create(args)

        receipt = json.loads(output.read_text(encoding="utf-8"))
        with mock.patch.object(
            supervisor,
            "validate_arm",
            return_value=self.live_arm(receipt, receipt["observedAtUtc"]),
        ):
            value = closure.validate_receipt(
                output,
                digest(output),
                expected_release_id=plan.release_id,
                expected_source_commit=plan.source_commit,
            )
        self.assertEqual(set(value), closure.RECEIPT_KEYS)
        self.assertEqual(set(value["components"]), set(closure.RECEIPT_COMPONENTS))
        self.assertEqual(value["protectedListeners"], closure.PROTECTED_LISTENERS)
        self.assertEqual(stat.S_IMODE(output.stat().st_mode), 0o600)
        for role in closure.ROLES:
            result = state / role / "verify-frozen.json"
            self.assertTrue(result.is_file())
            self.assertEqual(value["components"][role]["verifyFrozenResultSha256"], digest(result))

    def test_wrong_frozen_block_is_fail_closed_without_handoff(self) -> None:
        plan_path, plan = self.make_plan(wrong_block=True)
        output = self.root / "wrong-closure.json"
        args = self.make_args(plan_path, self.root / "wrong-state", output)
        with (
            mock.patch.object(closure, "validate_bound_inputs", return_value=self.bound_inputs()),
            mock.patch.object(
                closure,
                "prepare_immutable_drivers",
                return_value=self.prepared_drivers(plan),
            ),
        ):
            with self.assertRaisesRegex(closure.ClosureError, "differs from the final frozen block"):
                closure.command_create(args)
        self.assertFalse(os.path.lexists(output))

    def test_existing_output_rejected_before_evidence_or_driver_work(self) -> None:
        plan_path, _ = self.make_plan()
        output = self.root / "exists.json"
        output.write_text("do not overwrite", encoding="utf-8")
        args = self.make_args(plan_path, self.root / "unused-state", output)
        with mock.patch.object(closure, "validate_bound_inputs") as validate:
            with self.assertRaisesRegex(closure.ClosureError, "refusing to overwrite"):
                closure.command_create(args)
        validate.assert_not_called()
        self.assertEqual(output.read_text(encoding="utf-8"), "do not overwrite")

    def test_verifier_rejects_stale_and_symlink_receipts(self) -> None:
        observed = "2026-07-31T20:00:00Z"
        arm = self.root / "receipt-arm.json"
        write_json(arm, {})
        receipt = self.root / "receipt.json"
        receipt_value = make_receipt(observed, arm)
        write_json(receipt, receipt_value)
        now = dt.datetime(2026, 7, 31, 20, 5, 1, tzinfo=dt.timezone.utc)
        with self.assertRaisesRegex(closure.ClosureError, "stale"):
            closure.validate_receipt(receipt, digest(receipt), now=now)

        with mock.patch.object(
            supervisor,
            "validate_arm",
            return_value=self.live_arm(receipt_value, observed),
        ):
            identity_only = closure.validate_receipt(
                receipt,
                digest(receipt),
                max_age_seconds=0,
                expected_release_id="nexus-v2-alpha",
                expected_source_commit=SOURCE_COMMIT,
                now=now,
            )
        self.assertEqual(identity_only["observedAtUtc"], observed)
        with self.assertRaisesRegex(closure.ClosureError, "0..300"):
            closure.validate_receipt(receipt, digest(receipt), max_age_seconds=301)

        verify_args = closure.build_parser().parse_args(
            [
                "verify",
                "--handoff",
                str(receipt),
                "--expected-sha256",
                digest(receipt),
                "--release-id",
                "nexus-v2-alpha",
                "--source-commit",
                SOURCE_COMMIT,
                "--max-age-seconds",
                "0",
            ]
        )
        self.assertEqual(verify_args.handoff, str(receipt))
        self.assertEqual(verify_args.max_age_seconds, 0)

        link = self.root / "receipt-link.json"
        link.symlink_to(receipt)
        with self.assertRaisesRegex(closure.ClosureError, "symlink"):
            closure.validate_receipt(link, digest(receipt), now=dt.datetime(2026, 7, 31, 20, 0, 1, tzinfo=dt.timezone.utc))

    def test_bound_inputs_cross_bind_all_evidence_to_one_block_and_identity(self) -> None:
        plan_path, plan = self.make_plan()
        replacement = self.root / "replacement.json"
        readiness = self.root / "readiness.json"
        final_evidence = self.root / "final-evidence.json"
        manifest = self.root / "manifest.json"
        restore = self.root / "restore.json"
        migration = self.root / "migration.json"
        automatic_restore_arm = self.root / "automatic-restore-arm.json"
        inventory = self.root / "inventory.json"
        for path in (replacement, manifest, restore, migration, automatic_restore_arm, inventory):
            write_json(path, {})

        readiness_value = {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-reset-readiness",
            "releaseId": plan.release_id,
            "sourceCommit": plan.source_commit,
            "backupManifestSha256": digest(manifest),
            "restoreEvidenceSha256": digest(restore),
            "migrationEvidenceSha256": digest(migration),
            "economicGatesSha256": "8" * 64,
            "acceptanceInventorySha256": "9" * 64,
            "economicGateMode": "pre-v16-fresh-reset-frozen",
            "resetMode": "fresh-genesis-replacement",
            "freshGenesisReplacementOnly": True,
            "inPlaceRuntimeActivationAuthorized": False,
            "gateFinalizedBlock": FROZEN_BLOCK,
            "readyForSeparateOperatorResetAuthorization": True,
            "automaticRollbackEligibleAtGateBlock": True,
            "economicFlagsDisabled": True,
            "v2AcceptanceAssetsExist": False,
            "resetExecuted": False,
            "deployExecuted": False,
            "createdAtUtc": "2026-07-31T20:01:00Z",
        }
        write_json(readiness, readiness_value)
        final_value = {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-final-freeze-evidence",
            "transactionId": plan.transaction_id,
            "releaseId": plan.release_id,
            "sourceCommit": plan.source_commit,
            "componentSourceCommits": dict(plan.component_source_commits),
            "planSha256": plan.sha256,
            "frozenFinalizedBlock": FROZEN_BLOCK,
            "stabilityWindowSeconds": 30,
            "allIngressAndMutatingServicesStopped": True,
            "automaticResumeAttempted": False,
            "backupManifestSha256": digest(manifest),
            "artifactGroups": sorted(closure.release.REQUIRED_ARTIFACTS),
            "driverSha256": {role: plan.components[role].driver_sha256 for role in closure.ROLES},
            "completedAtUtc": "2026-07-31T20:00:00Z",
            "paidOrPublicActivationAllowed": False,
        }
        write_json(final_evidence, final_value)
        args = argparse.Namespace(
            replacement_lock=str(replacement),
            expected_replacement_lock_sha256=digest(replacement),
            selected_deployment_environment=str(self.root / "node.env"),
            selected_site_deployment_environment=str(self.root / "site.env"),
            reset_readiness=str(readiness),
            expected_reset_readiness_sha256=digest(readiness),
            final_freeze_evidence=str(final_evidence),
            expected_final_freeze_evidence_sha256=digest(final_evidence),
            backup_manifest=str(manifest),
            expected_backup_manifest_sha256=digest(manifest),
            restore_evidence=str(restore),
            expected_restore_evidence_sha256=digest(restore),
            migration_evidence=str(migration),
            expected_migration_evidence_sha256=digest(migration),
            automatic_restore_arm=str(automatic_restore_arm),
            expected_automatic_restore_arm_sha256=digest(automatic_restore_arm),
        )
        lock = {"releaseId": plan.release_id, "repositories": {"chain": {"head": plan.source_commit}}}
        summary = {"releaseId": plan.release_id, "sourceCommit": plan.source_commit}
        verified = {"sha256": digest(manifest), "releaseId": plan.release_id, "sourceCommit": plan.source_commit}
        source_inventory = {
            "sha256": digest(inventory),
            "blockNumber": FROZEN_BLOCK["number"],
            "blockHash": FROZEN_BLOCK["hash"],
        }
        with (
            mock.patch.object(closure.release_lock, "validate_replacement_lock", return_value=lock),
            mock.patch.object(closure.verify_reset_readiness, "validate_packet", return_value=summary),
            mock.patch.object(closure.release, "verify_backup_manifest", return_value=verified),
            mock.patch.object(closure.release, "find_artifact", return_value=inventory),
            mock.patch.object(closure.release, "validate_legacy_source_inventory", return_value=source_inventory),
            mock.patch.object(closure.release, "validate_restore_evidence"),
            mock.patch.object(closure.release, "validate_migration_evidence"),
            mock.patch.object(closure, "validate_automatic_restore_arm"),
        ):
            bound = closure.validate_bound_inputs(args, plan, self.bundle)
        self.assertEqual(bound.frozen_block, FROZEN_BLOCK)
        self.assertEqual(bound.backup_manifest_sha256, digest(manifest))

        readiness_value["migrationEvidenceSha256"] = "f" * 64
        write_json(readiness, readiness_value)
        args.expected_reset_readiness_sha256 = digest(readiness)
        with (
            mock.patch.object(closure.release_lock, "validate_replacement_lock", return_value=lock),
            mock.patch.object(closure.verify_reset_readiness, "validate_packet", return_value=summary),
            mock.patch.object(closure, "validate_automatic_restore_arm"),
        ):
            with self.assertRaisesRegex(closure.ClosureError, "reset-readiness migration hash mismatch"):
                closure.validate_bound_inputs(args, plan, self.bundle)

    def test_arm_semantics_are_fail_closed_by_the_installed_supervisor_validator(self) -> None:
        plan_path, plan = self.make_plan()
        arm = self.root / "arm.json"
        write_json(arm, {})
        with self.assertRaisesRegex(closure.ClosureError, "arm schema mismatch"):
            closure.validate_automatic_restore_arm(arm, digest(arm), plan, FROZEN_BLOCK)

    def test_immutable_driver_checkout_executes_committed_bytes_not_source_path(self) -> None:
        source = self.root / "driver-source"
        source.mkdir(mode=0o700)
        empty_template = self.root / "empty-template"
        empty_template.mkdir(mode=0o700)
        subprocess.run(
            ["git", "init", "--quiet", "--template", str(empty_template), str(source)],
            check=True,
        )
        subprocess.run(["git", "-C", str(source), "config", "user.name", "Test"], check=True)
        subprocess.run(["git", "-C", str(source), "config", "user.email", "test@example.invalid"], check=True)
        driver = source / "driver"
        driver.write_text(fake_driver_body(), encoding="utf-8")
        os.chmod(driver, 0o700)
        subprocess.run(["git", "-C", str(source), "add", "driver"], check=True)
        subprocess.run(["git", "-C", str(source), "commit", "--quiet", "-m", "driver"], check=True)
        head = subprocess.run(
            ["git", "-C", str(source), "rev-parse", "HEAD"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        component = closure.Component(driver, digest(driver), "target", ())
        plan = closure.Plan(
            path=self.root / "unused-plan.json",
            sha256="1" * 64,
            transaction_id="transaction",
            release_id="release",
            source_commit=head,
            stability_window_seconds=30,
            component_source_commits={name: head for name in closure.SOURCE_COMPONENTS},
            components={role: component for role in closure.ROLES},
        )
        state = self.root / "immutable-state"
        state.mkdir(mode=0o700)
        prepared = closure.prepare_immutable_drivers(plan, state)
        pinned_digest = digest(driver)
        driver.write_text("#!/bin/sh\nexit 99\n", encoding="utf-8")
        for role in closure.ROLES:
            copied = prepared[role].path
            self.assertNotEqual(copied, driver)
            self.assertEqual(digest(copied), pinned_digest)
            self.assertNotEqual(digest(copied), digest(driver))
            self.assertEqual(stat.S_IMODE(copied.stat().st_mode) & 0o222, 0)

    def test_driver_process_is_killed_at_the_closure_deadline(self) -> None:
        started = dt.datetime.now(dt.timezone.utc)
        with self.assertRaisesRegex(closure.ClosureError, "exceeded 300 seconds"):
            closure.bounded_subprocess(
                [sys.executable, "-c", "import time; time.sleep(10)"],
                os.environ,
                0.05,
            )
        elapsed = (dt.datetime.now(dt.timezone.utc) - started).total_seconds()
        self.assertLess(elapsed, 2)


if __name__ == "__main__":
    unittest.main()
