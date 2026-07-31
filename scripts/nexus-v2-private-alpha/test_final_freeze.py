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
                    frozen_at = (
                        datetime.datetime.now(datetime.timezone.utc)
                        - datetime.timedelta(seconds=45)
                    ).replace(microsecond=0).isoformat().replace("+00:00", "Z")
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
                "chain": source,
                "media": "b" * 40,
                "sdkgen": "c" * 40,
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


if __name__ == "__main__":
    unittest.main()
