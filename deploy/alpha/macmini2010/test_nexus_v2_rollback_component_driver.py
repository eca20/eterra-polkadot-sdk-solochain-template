#!/usr/bin/env python3
"""Fixture-only tests for the concrete chain/media coordinator adapter."""

from __future__ import annotations

import hashlib
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import tarfile
import unittest
from pathlib import Path
from typing import Any


HERE = Path(__file__).resolve().parent
DRIVER = HERE / "nexus-v2-rollback-component-driver"
PROTECTED_ACTION = HERE / "nexus-v2-rollback-protected-host-action.sh"
RESTORE_SCRIPT = HERE / "restore-alpha-state.sh"
DEPLOY_NODE = HERE / "deploy-node.sh"
DEPLOY_MEDIA = HERE / "deploy-media.sh"
READINESS = "9" * 64
ARTIFACTS = {
    "node": {
        "node-data",
        "node-binary",
        "runtime-v14-wasm",
        "runtime-v16-production-wasm",
        "runtime-v16-try-runtime-wasm",
        "legacy-source-inventory",
        "tcg-storage-version-observation",
        "try-runtime-snapshot",
        "try-runtime-snapshot-proof",
    },
    "media": {"media-state", "media-image-lock"},
    "ipfs": {"ipfs-data", "ipfs-staging"},
    "config": {
        "node-env",
        "media-env",
        "indexer-env",
        "chain-spec",
        "economic-gates",
    },
    "service": {"node-service", "media-service", "indexer-service"},
    "indexer": {"indexer-state", "indexer-checkpoint"},
}
SCRIPT_PATHS = {
    "restoreState": "deploy/alpha/macmini2010/restore-alpha-state.sh",
    "deployNode": "deploy/alpha/macmini2010/deploy-node.sh",
    "deployMedia": "deploy/alpha/macmini2010/deploy-media.sh",
    "status": "deploy/alpha/macmini2010/status.sh",
}
PROTECTED_HELPER = (
    "deploy/alpha/macmini2010/nexus-v2-rollback-protected-host-action.sh"
)

MOCK_PROTECTED_HELPER = r"""#!/usr/bin/env python3
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path

p = argparse.ArgumentParser()
p.add_argument("--context", required=True)
p.add_argument("--result", required=True)
a = p.parse_args()
context = json.loads(Path(a.context).read_text())
root = Path(os.environ["MOCK_PROTECTED_HOST_ROOT"])
root.mkdir(parents=True, exist_ok=True)
trace = root / "trace.txt"
with trace.open("a", encoding="utf-8") as handle:
    handle.write(context["action"] + ":" + context["mode"] + "\n")
if context["action"] == "restore-final-backup":
    staging = Path(context["stagingPath"])
    expected = {
        "backup-economic-gates.json", "chain-spec.json",
        "ipfs-data.tar.gz", "ipfs-staging.tar.gz",
        "media-image-lock.json", "media-service.json",
        "media-state.tar.gz", "media.env", "node-binary",
        "node-data.tar.gz", "node-service.service", "node.env",
    }
    contract = json.loads((staging / "staging-contract.json").read_text())
    assert set(contract["files"]) == expected
    for name, expected_hash in contract["files"].items():
        actual = hashlib.sha256((staging / name).read_bytes()).hexdigest()
        assert actual == expected_hash
    (root / "restore-staging-verified").write_text("yes\n", encoding="utf-8")

dry = {
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
execute = {
    "post-cutover-smoke": {
        "sourceIdentityPinned", "requiredResetArchivesPresent", "smokePassed",
    },
    "pause-v2-writes": {
        "sourceIdentityPinned", "requiredResetArchivesPresent",
        "v2WritesPaused", "statePreserved", "restoreNotAttempted",
    },
    "archive-failed-v2": {
        "sourceIdentityPinned", "requiredResetArchivesPresent",
        "failedV2RootArchived", "archiveManifestImmutable",
    },
    "restore-final-backup": {
        "sourceIdentityPinned", "requiredResetArchivesPresent",
        "failedV2RootArchivePresent", "finalBackupHashesVerified",
        "restoreEvidenceMatched", "existingRestoreScriptUsed",
        "existingDeployScriptsUsed", "restoreCompleted",
    },
    "restored-smoke": {
        "sourceIdentityPinned", "requiredResetArchivesPresent",
        "failedV2RootArchivePresent", "componentHealthy",
        "backupIdentityReadback", "economicFlagsDisabled",
    },
}
marker = root / (context["action"] + ".json")
already = context["mode"] == "execute" and marker.exists()
if context["mode"] == "execute" and not already:
    marker.write_text(
        json.dumps(
            {
                "action": context["action"],
                "operationId": context["operationId"],
                "planSha256": context["planSha256"],
            },
            sort_keys=True,
            separators=(",", ":"),
        ) + "\n",
        encoding="utf-8",
    )
marker_hash = (
    hashlib.sha256(marker.read_bytes()).hexdigest()
    if context["mode"] == "execute"
    else None
)
failed_hash = (
    hashlib.sha256(b"mock-failed-v2-chain-media-archive").hexdigest()
    if context["mode"] == "execute"
    and context["action"] in {
        "archive-failed-v2", "restore-final-backup", "restored-smoke"
    }
    else None
)
checks = {
    name: True
    for name in (
        dry[context["action"]]
        if context["mode"] == "dry-run"
        else execute[context["action"]]
    )
}
result = {
    "schemaVersion": 1,
    "kind": "nexus-v2-private-alpha-component-action-result",
    "operationId": context["operationId"],
    "planSha256": context["planSha256"],
    "releaseId": context["releaseId"],
    "sourceCommit": context["sourceCommit"],
    "componentSourceCommits": context["componentSourceCommits"],
    "componentId": context["componentId"],
    "action": context["action"],
    "mode": context["mode"],
    "result": "passed",
    "remoteActionsExecuted": context["mode"] == "execute" and not already,
    "alreadyApplied": already,
    "requiredResetArchives": {"media": True, "node": True},
    "failedV2RootArchiveSha256": failed_hash,
    "remoteIdempotencyMarkerSha256": marker_hash,
    "checks": checks,
    "completedAtUtc": datetime.datetime.now(
        datetime.timezone.utc
    ).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
}
Path(a.result).write_text(
    json.dumps(result, sort_keys=True, separators=(",", ":")) + "\n",
    encoding="utf-8",
)
"""


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )


def write_tar(path: Path, members: dict[str, bytes]) -> None:
    with tarfile.open(path, "w:gz") as archive:
        for name, payload in sorted(members.items()):
            info = tarfile.TarInfo(name)
            info.mode = 0o600
            info.size = len(payload)
            archive.addfile(info, io.BytesIO(payload))


def git(root: Path, *arguments: str) -> str:
    completed = subprocess.run(
        ["git", "-C", str(root), *arguments],
        check=True,
        capture_output=True,
        text=True,
    )
    return completed.stdout.strip()


def init_repo(root: Path) -> None:
    root.mkdir()
    git(root, "init", "-q")
    git(root, "config", "user.name", "Nexus Test")
    git(root, "config", "user.email", "nexus-test@example.invalid")


class ChainMediaDriverTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.chain = self.root / "chain"
        self.media = self.root / "media"
        init_repo(self.chain)
        init_repo(self.media)

        target = self.chain / "deploy/alpha/macmini2010/nexus-v2-rollback-component-driver"
        target.parent.mkdir(parents=True)
        shutil.copyfile(DRIVER, target)
        target.chmod(0o755)
        for relative in SCRIPT_PATHS.values():
            script = self.chain / relative
            script.write_text("#!/usr/bin/env bash\nexit 0\n", encoding="utf-8")
            script.chmod(0o755)
        protected_helper = self.chain / PROTECTED_HELPER
        protected_helper.write_text(MOCK_PROTECTED_HELPER, encoding="utf-8")
        protected_helper.chmod(0o755)
        git(self.chain, "add", ".")
        git(self.chain, "commit", "-qm", "chain fixture")
        (self.media / "README.md").write_text("fixture\n", encoding="utf-8")
        git(self.media, "add", ".")
        git(self.media, "commit", "-qm", "media fixture")
        self.chain_commit = git(self.chain, "rev-parse", "HEAD")
        self.media_commit = git(self.media, "rev-parse", "HEAD")
        self.driver = target

        self.gates = self.root / "economic-gates.json"
        self.inventory = self.root / "acceptance-inventory.json"
        write_json(self.gates, {"schemaVersion": 1, "allEconomicFlagsDisabled": True})
        write_json(self.inventory, {"schemaVersion": 1, "allAcceptanceCountsZero": True})
        self.bundle = self.root / "bundle"
        self.bundle.mkdir()
        entries = []
        for group, names in sorted(ARTIFACTS.items()):
            for name in sorted(names):
                artifact = self.bundle / "artifacts" / group / f"{name}.bin"
                artifact.parent.mkdir(parents=True, exist_ok=True)
                artifact.write_bytes(f"{group}:{name}\n".encode())

                entries.append(
                    {
                        "group": group,
                        "name": name,
                        "path": artifact.relative_to(self.bundle).as_posix(),
                        "bytes": artifact.stat().st_size,
                        "sha256": digest(artifact),
                    }
                )
        self.manifest = self.bundle / "backup-manifest.json"
        write_json(
            self.manifest,
            {
                "schemaVersion": 1,
                "kind": "nexus-v2-private-alpha-backup",
                "releaseId": "nexus-v2-driver-fixture",
                "sourceCommit": self.chain_commit,
                "artifacts": entries,
            },
        )
        self.restore = self.root / "restore-evidence.json"
        write_json(
            self.restore,
            {
                "schemaVersion": 1,
                "kind": "nexus-v2-isolated-restore-evidence",
                "releaseId": "nexus-v2-driver-fixture",
                "sourceCommit": self.chain_commit,
                "backupManifestSha256": digest(self.manifest),
                "result": "passed",
                "liveAlphaTouched": False,
            },
        )
        self.observation = self.root / "observation.json"
        write_json(
            self.observation,
            {
                "schemaVersion": 1,
                "economicGatesSha256": digest(self.gates),
                "acceptanceInventorySha256": digest(self.inventory),
            },
        )
        component = {
            "id": "chain-media",
            "sourcePins": [
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
            ],
            "driverSourceId": "chain",
            "driverPath": self.driver.relative_to(self.chain).as_posix(),
            "driverSha256": digest(self.driver),
            "requiredResetArchives": {
                "node": f"/opt/eterra-alpha/archive/nexus-v2-fresh-reset/{READINESS}/node",
                "media": f"/opt/eterra-alpha/archive/nexus-v2-fresh-reset/{READINESS}/media",
            },
            "scriptPins": {
                role: {
                    "sourceId": "chain",
                    "path": relative,
                    "sha256": digest(self.chain / relative),
                }
                for role, relative in SCRIPT_PATHS.items()
            },
        }
        self.plan = self.root / "plan.json"
        write_json(
            self.plan,
            {
                "schemaVersion": 1,
                "operationId": "fixture-operation",
                "releaseId": "nexus-v2-driver-fixture",
                "sourceCommit": self.chain_commit,
                "freshResetReadinessSha256": READINESS,
                "finalBackupManifestSha256": digest(self.manifest),
                "restoreEvidenceSha256": digest(self.restore),
                "postCutoverObservationSha256": digest(self.observation),
                "components": [component],
            },
        )
        self.fixture = self.root / "chain-media.NONDEPLOYABLE"
        self.fixture.mkdir()
        write_json(
            self.fixture / "fixture-contract.json",
            {
                "schemaVersion": 1,
                "kind": "nexus-v2-rollback-component-host-fixture",
                "fixtureOnly": True,
                "componentId": "chain-media",
                "credentialsResolvable": True,
                "requiredResetArchives": {"node": True, "media": True},
                "postCutoverSmokePassed": False,
                "economicFlagsDisabled": True,
            },
        )
        self.protected_host = self.root / "protected-host.NONDEPLOYABLE"

    def test_isolated_copied_driver_has_no_repository_import_dependency(self) -> None:
        environment = {
            "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
            "PYTHONPATH": "",
        }
        completed = subprocess.run(
            [sys.executable, "-I", str(self.driver), "--help"],
            cwd=self.root,
            env=environment,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertNotIn("ModuleNotFoundError", completed.stderr)

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def invoke(
        self,
        action: str,
        mode: str,
        result_name: str,
        *,
        fixture: bool = True,
        protected: bool = False,
        confirmation: bool = True,
    ) -> subprocess.CompletedProcess[str]:
        result = self.root / result_name
        command = [
            str(self.driver),
            "--component",
            "chain-media",
            "--action",
            action,
            "--mode",
            mode,
            "--operation-id",
            "fixture-operation",
            "--plan",
            str(self.plan),
            "--plan-sha256",
            digest(self.plan),
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
            str(self.inventory),
            "--result",
            str(result),
        ]
        environment = os.environ.copy()
        if protected:
            environment.pop("NEXUS_V2_ROLLBACK_FIXTURE_ROOT", None)
            environment["NEXUS_V2_ROLLBACK_BACKEND"] = "protected-alpha"
            environment["MOCK_PROTECTED_HOST_ROOT"] = str(self.protected_host)
            if confirmation:
                environment[
                    "NEXUS_V2_ROLLBACK_PROTECTED_CONFIRMATION"
                ] = "PRIVATE_ALPHA_ROLLBACK_ONLY"
            else:
                environment.pop(
                    "NEXUS_V2_ROLLBACK_PROTECTED_CONFIRMATION", None
                )
        elif fixture:
            environment["NEXUS_V2_ROLLBACK_FIXTURE_ROOT"] = str(self.fixture)
        else:
            environment.pop("NEXUS_V2_ROLLBACK_FIXTURE_ROOT", None)
            environment.pop("NEXUS_V2_ROLLBACK_BACKEND", None)
        completed = subprocess.run(
            command,
            check=False,
            capture_output=True,
            text=True,
            env=environment,
        )
        completed.result_path = result  # type: ignore[attr-defined]
        return completed

    def prepare_protected_inputs(self, *, cards_v2: int = 0) -> None:
        false_paths = {
            "tcg": {
                "features": {
                    "Packs": False,
                    "Conversion": False,
                    "Ranked": False,
                    "MythicalAscension": False,
                }
            },
            "randomness": {
                "cryptographyReviewApproved": False,
                "drandQuicknetEnabled": False,
                "productionEconomicUseAllowed": False,
            },
            "issuance": {"paidV2IssuanceCallAvailable": False},
            "reforge": {"dispatchableAvailable": False},
            "magic": {"productionTransferEnabled": False},
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
        }
        write_json(
            self.gates,
            {
                "schemaVersion": 1,
                "kind": "nexus-v2-private-alpha-economic-gates",
                "releaseId": "nexus-v2-driver-fixture",
                "sourceCommit": self.chain_commit,
                **false_paths,
            },
        )
        write_json(
            self.inventory,
            {
                "schemaVersion": 1,
                "kind": "nexus-v2-acceptance-inventory",
                "releaseId": "nexus-v2-driver-fixture",
                "sourceCommit": self.chain_commit,
                "counts": {"cardsV2": cards_v2},
            },
        )
        write_json(
            self.observation,
            {
                "schemaVersion": 1,
                "economicGatesSha256": digest(self.gates),
                "acceptanceInventorySha256": digest(self.inventory),
                "writeBarrier": {
                    "mode": "AllV2WritesPaused",
                    "chainWritesPaused": True,
                    "authorityResultsPaused": True,
                    "webMutationsPaused": True,
                    "gameplaySessionIngressPaused": True,
                    "inventoryObservedAfterPause": True,
                },
            },
        )
        plan = json.loads(self.plan.read_text())
        plan["postCutoverObservationSha256"] = digest(self.observation)
        write_json(self.plan, plan)

    def prepare_restore_layout(self) -> None:
        compose_base = b"services:\n  media-service:\n    image: pinned\n"
        compose_override = b"services:\n  ipfs:\n    image: pinned\n"
        manifest = json.loads(self.manifest.read_text())
        entries = {
            (entry["group"], entry["name"]): entry
            for entry in manifest["artifacts"]
        }

        def artifact(group: str, name: str) -> Path:
            return self.bundle / entries[(group, name)]["path"]

        for group, name in (
            ("node", "node-data"),
            ("ipfs", "ipfs-data"),
            ("ipfs", "ipfs-staging"),
        ):
            write_tar(
                artifact(group, name),
                {"state/marker.txt": f"{group}:{name}\n".encode()},
            )
        write_tar(
            artifact("media", "media-state"),
            {
                "docker-compose.yaml": compose_base,
                "docker-compose.macmini2010.yaml": compose_override,
            },
        )
        artifact("node", "node-binary").write_bytes(
            b"\x7fELF\x02\x01\x01\x00mock-node"
        )
        write_json(
            artifact("config", "chain-spec"),
            {"name": "Mock restored Alpha", "id": "mock-restored-alpha"},
        )
        artifact("config", "node-env").write_text(
            "ETERRA_RELEASE_VERSION=nexus-v2-driver-fixture\n",
            encoding="utf-8",
        )
        artifact("config", "media-env").write_text(
            "PUBLIC_MEDIA_UPLOAD_ENABLED=false\nALLOW_DEV_ADMIN_RESET=0\n",
            encoding="utf-8",
        )
        artifact("service", "node-service").write_text(
            "[Service]\nExecStart=/opt/eterra-alpha/node/current/start-alpha-node.sh\n",
            encoding="utf-8",
        )
        write_json(
            artifact("media", "media-image-lock"),
            {
                "schemaVersion": 1,
                "kind": "nexus-v2-private-alpha-media-image-lock",
                "projectName": "eterra-alpha-media",
                "images": [
                    {
                        "service": "media-service",
                        "reference": "registry.invalid/media@sha256:" + "1" * 64,
                        "imageId": "sha256:" + "2" * 64,
                    },
                    {
                        "service": "ipfs",
                        "reference": "registry.invalid/ipfs@sha256:" + "3" * 64,
                        "imageId": "sha256:" + "4" * 64,
                    },
                ],
            },
        )
        write_json(
            artifact("service", "media-service"),
            {
                "schemaVersion": 1,
                "kind": "nexus-v2-private-alpha-media-service-lock",
                "projectName": "eterra-alpha-media",
                "composeFiles": [
                    {
                        "path": "docker-compose.yaml",
                        "sha256": hashlib.sha256(compose_base).hexdigest(),
                    },
                    {
                        "path": "docker-compose.macmini2010.yaml",
                        "sha256": hashlib.sha256(compose_override).hexdigest(),
                    },
                ],
            },
        )
        artifact("config", "economic-gates").write_bytes(
            self.gates.read_bytes()
        )

        for entry in manifest["artifacts"]:
            path = self.bundle / entry["path"]
            entry["bytes"] = path.stat().st_size
            entry["sha256"] = digest(path)
        write_json(self.manifest, manifest)
        restore = json.loads(self.restore.read_text())
        restore["backupManifestSha256"] = digest(self.manifest)
        write_json(self.restore, restore)
        plan = json.loads(self.plan.read_text())
        plan["finalBackupManifestSha256"] = digest(self.manifest)
        plan["restoreEvidenceSha256"] = digest(self.restore)
        write_json(self.plan, plan)

    def test_dry_run_receipts_are_closed_and_do_not_create_action_markers(self) -> None:
        actions = [
            "post-cutover-smoke",
            "pause-v2-writes",
            "archive-failed-v2",
            "restore-final-backup",
            "restored-smoke",
        ]
        for index, action in enumerate(actions):
            completed = self.invoke(action, "dry-run", f"dry-{index}.json")
            self.assertEqual(completed.returncode, 0, completed.stderr)
            result = json.loads(completed.result_path.read_text())  # type: ignore[attr-defined]
            self.assertFalse(result["remoteActionsExecuted"])
            self.assertFalse(result["alreadyApplied"])
            self.assertIsNone(result["remoteIdempotencyMarkerSha256"])
            self.assertTrue(all(result["requiredResetArchives"].values()))
            self.assertTrue(all(result["checks"].values()))
        marker_root = self.fixture / "remote-markers" / "fixture-operation"
        self.assertFalse(marker_root.exists())

    def test_execute_sequence_is_ordered_and_idempotent(self) -> None:
        premature = self.invoke("restore-final-backup", "execute", "premature.json")
        self.assertNotEqual(premature.returncode, 0)
        self.assertFalse(premature.result_path.exists())  # type: ignore[attr-defined]

        for index, action in enumerate(
            [
                "post-cutover-smoke",
                "pause-v2-writes",
                "archive-failed-v2",
                "restore-final-backup",
                "restored-smoke",
            ]
        ):
            completed = self.invoke(action, "execute", f"execute-{index}.json")
            self.assertEqual(completed.returncode, 0, completed.stderr)
            result = json.loads(completed.result_path.read_text())  # type: ignore[attr-defined]
            self.assertTrue(result["remoteActionsExecuted"])
            self.assertFalse(result["alreadyApplied"])
            self.assertRegex(result["remoteIdempotencyMarkerSha256"], r"^[0-9a-f]{64}$")
            if action == "post-cutover-smoke":
                self.assertFalse(result["checks"]["smokePassed"])

        repeated = self.invoke("restore-final-backup", "execute", "restore-repeat.json")
        self.assertEqual(repeated.returncode, 0, repeated.stderr)
        repeated_result = json.loads(repeated.result_path.read_text())  # type: ignore[attr-defined]
        original_result = json.loads((self.root / "execute-3.json").read_text())
        self.assertFalse(repeated_result["remoteActionsExecuted"])
        self.assertTrue(repeated_result["alreadyApplied"])
        self.assertEqual(
            repeated_result["remoteIdempotencyMarkerSha256"],
            original_result["remoteIdempotencyMarkerSha256"],
        )
        self.assertEqual(
            repeated_result["failedV2RootArchiveSha256"],
            original_result["failedV2RootArchiveSha256"],
        )

    def test_no_fixture_backend_fails_closed_without_a_result(self) -> None:
        completed = self.invoke(
            "post-cutover-smoke",
            "dry-run",
            "no-fixture.json",
            fixture=False,
        )
        self.assertNotEqual(completed.returncode, 0)
        self.assertIn("requires NEXUS_V2_ROLLBACK_FIXTURE_ROOT", completed.stderr)
        self.assertFalse(completed.result_path.exists())  # type: ignore[attr-defined]

    def test_protected_backend_requires_confirmation_before_host_helper(self) -> None:
        self.prepare_protected_inputs()
        completed = self.invoke(
            "post-cutover-smoke",
            "dry-run",
            "protected-no-confirmation.json",
            fixture=False,
            protected=True,
            confirmation=False,
        )
        self.assertNotEqual(completed.returncode, 0)
        self.assertIn("requires NEXUS_V2_ROLLBACK_PROTECTED_CONFIRMATION", completed.stderr)
        self.assertFalse(self.protected_host.exists())
        self.assertFalse(completed.result_path.exists())  # type: ignore[attr-defined]

    def test_protected_backend_is_closed_and_idempotent_with_mock_host(self) -> None:
        self.prepare_protected_inputs()
        dry = self.invoke(
            "post-cutover-smoke",
            "dry-run",
            "protected-dry.json",
            fixture=False,
            protected=True,
        )
        self.assertEqual(dry.returncode, 0, dry.stderr)
        dry_result = json.loads(dry.result_path.read_text())  # type: ignore[attr-defined]
        self.assertFalse(dry_result["remoteActionsExecuted"])
        self.assertFalse(dry_result["alreadyApplied"])

        first = self.invoke(
            "post-cutover-smoke",
            "execute",
            "protected-first.json",
            fixture=False,
            protected=True,
        )
        self.assertEqual(first.returncode, 0, first.stderr)
        first_result = json.loads(first.result_path.read_text())  # type: ignore[attr-defined]
        self.assertTrue(first_result["remoteActionsExecuted"])
        self.assertFalse(first_result["alreadyApplied"])

        repeated = self.invoke(
            "post-cutover-smoke",
            "execute",
            "protected-repeat.json",
            fixture=False,
            protected=True,
        )
        self.assertEqual(repeated.returncode, 0, repeated.stderr)
        repeated_result = json.loads(repeated.result_path.read_text())  # type: ignore[attr-defined]
        self.assertFalse(repeated_result["remoteActionsExecuted"])
        self.assertTrue(repeated_result["alreadyApplied"])
        self.assertEqual(
            repeated_result["remoteIdempotencyMarkerSha256"],
            first_result["remoteIdempotencyMarkerSha256"],
        )

    def test_protected_restore_is_blocked_after_any_acceptance_asset(self) -> None:
        self.prepare_protected_inputs(cards_v2=1)
        completed = self.invoke(
            "restore-final-backup",
            "execute",
            "protected-post-acceptance-restore.json",
            fixture=False,
            protected=True,
        )
        self.assertNotEqual(completed.returncode, 0)
        self.assertIn("pause and forward-fix only", completed.stderr)
        self.assertFalse(self.protected_host.exists())
        self.assertFalse(completed.result_path.exists())  # type: ignore[attr-defined]

    def test_protected_restore_stages_and_hashes_the_closed_backup_layout(self) -> None:
        self.prepare_protected_inputs()
        self.prepare_restore_layout()
        for index, action in enumerate(
            ("pause-v2-writes", "archive-failed-v2", "restore-final-backup")
        ):
            completed = self.invoke(
                action,
                "execute",
                f"protected-restore-{index}.json",
                fixture=False,
                protected=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(
            (self.protected_host / "restore-staging-verified").read_text(),
            "yes\n",
        )

    def test_operational_scripts_expose_only_guarded_restore_entrypoints(self) -> None:
        for script in (
            PROTECTED_ACTION,
            RESTORE_SCRIPT,
            DEPLOY_NODE,
            DEPLOY_MEDIA,
        ):
            syntax = subprocess.run(
                ["bash", "-n", str(script)],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(syntax.returncode, 0, syntax.stderr)

        for script in (PROTECTED_ACTION, DEPLOY_NODE, DEPLOY_MEDIA):
            help_result = subprocess.run(
                [str(script), "--help"],
                check=False,
                capture_output=True,
                text=True,
                env={},
            )
            self.assertEqual(help_result.returncode, 0, help_result.stderr)
        self.assertIn(
            "--verify-restored-final-backup",
            subprocess.run(
                [str(DEPLOY_NODE), "--help"],
                check=True,
                capture_output=True,
                text=True,
                env={},
            ).stdout,
        )

        empty_staging = self.root / "empty-restore-staging"
        empty_staging.mkdir()
        no_confirmation = subprocess.run(
            [
                str(RESTORE_SCRIPT),
                "--verified-final-backup",
                str(empty_staging),
            ],
            check=False,
            capture_output=True,
            text=True,
            env={},
        )
        self.assertNotEqual(no_confirmation.returncode, 0)
        self.assertIn(
            "NEXUS_V2_ROLLBACK_PROTECTED_CONFIRMATION",
            no_confirmation.stderr,
        )
        self.assertNotIn("missing deploy env file", no_confirmation.stderr)

    def test_actual_protected_helper_dry_run_uses_only_stubbed_read_only_remote(
        self,
    ) -> None:
        helper_root = self.root / "actual-protected-helper"
        helper_root.mkdir()
        helper = helper_root / PROTECTED_ACTION.name
        shutil.copyfile(PROTECTED_ACTION, helper)
        helper.chmod(0o755)
        for name in (
            "restore-alpha-state.sh",
            "deploy-node.sh",
            "deploy-media.sh",
            "status.sh",
        ):
            script = helper_root / name
            script.write_text("#!/usr/bin/env bash\nexit 0\n", encoding="utf-8")
            script.chmod(0o755)
        (helper_root / "lib.sh").write_text(
            """
die() { printf '[stub] %s\\n' "$*" >&2; exit 1; }
require_cmd() { command -v "$1" >/dev/null 2>&1 || die "missing $1"; }
load_env() {
  DEPLOY_ROOT=/opt/eterra-alpha
  REMOTE_NODE_DATA_DIR=/var/lib/eterra-alpha-node
  CHAIN_RPC_PORT=9944
  CHAIN_P2P_PORT=30333
  MEDIA_PORT=4000
  AUTHORITY_PORT=8787
  IPFS_API_PORT=5001
  IPFS_GATEWAY_PORT=8080
  ETERRA_RELEASE_VERSION=mock-release
  ETERRA_EXPECTED_CHAIN_COMMIT=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
  ETERRA_EXPECTED_MEDIA_COMMIT=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
  REMOTE_SCRIPT_DIR=/tmp/mock-protected-helper
  REMOTE_NODE_SERVICE_NAME=mock-node
  AUTHORITY_SERVICE_NAME=mock-authority
  REMOTE_DOCKER_COMPOSE_CMD='mock compose'
  REMOTE_NODE_DATA_DIR=/var/lib/eterra-alpha-node
  REMOTE_IPFS_DATA_VOLUME=mock-ipfs-data
  REMOTE_IPFS_STAGING_VOLUME=mock-ipfs-staging
}
remote_root_bash() {
  payload="$(cat)"
  test -n "${payload}"
  case "${payload}" in
    *"for archive_dir in"*) printf 'ready' ;;
    *"if [[ -f "*"then printf 'yes'"*)
      if [[ -f "${STUB_REMOTE_ROOT}/marker.json" ]]; then
        printf 'yes'
      else
        printf 'no'
      fi
      ;;
    *"systemctl stop"*)
      printf 'paused\\n' >"${STUB_REMOTE_ROOT}/pause-performed"
      ;;
    *"install -m 0440"*)
      cp "${STUB_REMOTE_ROOT}/pending.json" "${STUB_REMOTE_ROOT}/marker.json"
      ;;
    *"cat "*"actions/"*)
      test -f "${STUB_REMOTE_ROOT}/marker.json"
      cat "${STUB_REMOTE_ROOT}/marker.json"
      ;;
    *) die "unexpected stubbed remote action" ;;
  esac
}
remote_bash() { payload="$(cat)"; test -n "${payload}"; }
rsync_to_remote_no_delete() { cp "$1" "${STUB_REMOTE_ROOT}/pending.json"; }
make_temp_dir() { mktemp -d "${TMPDIR}/protected-helper.XXXXXX"; }
""".lstrip(),
            encoding="utf-8",
        )
        context = self.root / "actual-protected-context.json"
        result = self.root / "actual-protected-result.json"
        reset_readiness = self.root / "actual-protected-reset-readiness.json"
        reset_readiness.write_bytes(b'{"kind":"fixture-reset-readiness"}\n')
        reset_readiness_sha256 = digest(reset_readiness)
        write_json(
            context,
            {
                "schemaVersion": 1,
                "kind": "nexus-v2-private-alpha-protected-host-action-context",
                "componentId": "chain-media",
                "action": "post-cutover-smoke",
                "mode": "dry-run",
                "operationId": "mock-operation",
                "planSha256": "1" * 64,
                "releaseId": "mock-release",
                "sourceCommit": "a" * 40,
                "componentSourceCommits": {
                    "chain": "a" * 40,
                    "media": "b" * 40,
                },
                "requiredResetArchives": {
                    "node": (
                        "/opt/eterra-alpha/archive/nexus-v2-fresh-reset/"
                        + reset_readiness_sha256
                        + "/node"
                    ),
                    "media": (
                        "/opt/eterra-alpha/archive/nexus-v2-fresh-reset/"
                        + reset_readiness_sha256
                        + "/media"
                    ),
                },
                "resetReadinessPath": str(reset_readiness),
                "scripts": {
                    "restoreState": str(helper_root / "restore-alpha-state.sh"),
                    "deployNode": str(helper_root / "deploy-node.sh"),
                    "deployMedia": str(helper_root / "deploy-media.sh"),
                    "status": str(helper_root / "status.sh"),
                },
                "finalBackupManifestSha256": "2" * 64,
                "restoreEvidenceSha256": "3" * 64,
                "postCutoverObservationSha256": "4" * 64,
                "economicGatesSha256": "5" * 64,
                "acceptanceInventorySha256": "6" * 64,
                "acceptanceAssetsExist": False,
                "stagingPath": None,
            },
        )
        environment = os.environ.copy()
        environment[
            "NEXUS_V2_ROLLBACK_PROTECTED_CONFIRMATION"
        ] = "PRIVATE_ALPHA_ROLLBACK_ONLY"
        environment["TMPDIR"] = str(self.root)
        environment["STUB_REMOTE_ROOT"] = str(self.root / "stub-remote")
        Path(environment["STUB_REMOTE_ROOT"]).mkdir()
        completed = subprocess.run(
            [
                str(helper),
                "--context",
                str(context),
                "--result",
                str(result),
            ],
            check=False,
            capture_output=True,
            text=True,
            env=environment,
        )
        self.assertEqual(
            completed.returncode,
            0,
            f"stdout={completed.stdout}\nstderr={completed.stderr}",
        )
        receipt = json.loads(result.read_text())
        self.assertEqual(receipt["mode"], "dry-run")
        self.assertFalse(receipt["remoteActionsExecuted"])
        self.assertTrue(receipt["checks"]["credentialsResolvable"])

        execute_context = json.loads(context.read_text())
        execute_context["action"] = "pause-v2-writes"
        execute_context["mode"] = "execute"
        write_json(context, execute_context)
        first_result = self.root / "actual-protected-execute.json"
        first = subprocess.run(
            [
                str(helper),
                "--context",
                str(context),
                "--result",
                str(first_result),
            ],
            check=False,
            capture_output=True,
            text=True,
            env=environment,
        )
        self.assertEqual(
            first.returncode,
            0,
            f"stdout={first.stdout}\nstderr={first.stderr}",
        )
        first_receipt = json.loads(first_result.read_text())
        self.assertTrue(first_receipt["remoteActionsExecuted"])
        self.assertFalse(first_receipt["alreadyApplied"])
        self.assertTrue(
            (Path(environment["STUB_REMOTE_ROOT"]) / "pause-performed").is_file()
        )

        repeat_result = self.root / "actual-protected-repeat.json"
        repeated = subprocess.run(
            [
                str(helper),
                "--context",
                str(context),
                "--result",
                str(repeat_result),
            ],
            check=False,
            capture_output=True,
            text=True,
            env=environment,
        )
        self.assertEqual(
            repeated.returncode,
            0,
            f"stdout={repeated.stdout}\nstderr={repeated.stderr}",
        )
        repeated_receipt = json.loads(repeat_result.read_text())
        self.assertFalse(repeated_receipt["remoteActionsExecuted"])
        self.assertTrue(repeated_receipt["alreadyApplied"])
        self.assertEqual(
            repeated_receipt["remoteIdempotencyMarkerSha256"],
            first_receipt["remoteIdempotencyMarkerSha256"],
        )


if __name__ == "__main__":
    unittest.main()
