#!/usr/bin/env python3
"""Subprocess contract tests for the pre-reset production adapters.

These tests intentionally execute the real workflow and component drivers.  All
nested helpers are hash-pinned local fixtures, so the suite proves argv,
ordering, immutable-output, and canonical-JSON behavior without contacting a
protected host.
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from typing import Any, Mapping


TOOL_ROOT = Path(__file__).resolve().parent
REPOSITORY_ROOT = TOOL_ROOT.parents[1]
WORKFLOW_DRIVER = TOOL_ROOT / "pre_reset_replacement_workflow.py"
CHAIN_STAGE_DRIVER = TOOL_ROOT / "pre_reset_chain_workflow_stage.py"
CHAIN_MEDIA_DRIVER = (
    REPOSITORY_ROOT
    / "deploy/alpha/macmini2010/nexus-v2-pre-reset-chain-media-component-driver"
)
ROLLBACK_STAGING = (
    REPOSITORY_ROOT
    / "deploy/alpha/macmini2010/nexus_v2_rollback_staging.py"
)

SOURCE_COMMIT = "a" * 40
OPERATION_ID = "replace-20260801"
RELEASE_ID = "nexus-v2-alpha"
SITE_RELEASE_VERSION = "v0.1.0-alpha.1"
FROZEN_BLOCK = {"hash": "0x" + "b" * 64, "number": 4242}


def canonical(value: Mapping[str, Any]) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")


def write_json(path: Path, value: Mapping[str, Any], mode: int = 0o600) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(canonical(value))
    os.chmod(path, mode)


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def pin(source_id: str, root: Path, path: Path) -> dict[str, str]:
    return {
        "sourceId": source_id,
        "path": str(path.relative_to(root)),
        "sha256": digest(path),
    }


STAGE_HELPER = r'''#!/usr/bin/env python3
import argparse, datetime, json, os, sys
p = argparse.ArgumentParser()
for name in (
    "plan", "plan-sha256", "workflow-contract",
    "workflow-contract-sha256", "automatic-restore-arm",
    "automatic-restore-arm-sha256", "stage", "workflow-state-root",
    "stage-state-root", "result",
):
    p.add_argument("--" + name, required=True)
a = p.parse_args()
with open(os.environ["TEST_ADAPTER_TRACE"], "a", encoding="utf-8") as handle:
    handle.write(json.dumps({"argv": sys.argv[1:], "stage": a.stage}, sort_keys=True) + "\n")
if os.environ.get("TEST_FAIL_STAGE") == a.stage:
    sys.exit(23)
with open(a.plan, encoding="utf-8") as handle:
    plan = json.load(handle)
with open(a.workflow_contract, encoding="utf-8") as handle:
    contract = json.load(handle)
if a.stage == "createZeroAssetAcceptanceFence":
    fence = {
        "automaticRestorePermanentlyDisabled": True,
        "bootstrapActionsBegun": False,
        "zeroCurrentAndLifetimeAcceptanceInventory": True,
    }
    fd = os.open(contract["acceptanceStartFencePath"], os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o400)
    with os.fdopen(fd, "wb") as handle:
        handle.write((json.dumps(fence, indent=2, sort_keys=True) + "\n").encode())
mutating = a.stage in {"deployChainMediaAuthority", "deploySiteIndexer", "closeIngressAndObserve"}
value = {
    "schemaVersion": 1,
    "kind": "nexus-v2-private-alpha-replacement-workflow-stage-result",
    "operationId": plan["operationId"],
    "releaseId": plan["releaseId"],
    "siteReleaseVersion": plan["siteReleaseVersion"],
    "sourceCommit": plan["sourceCommit"],
    "planSha256": a.plan_sha256,
    "workflowContractSha256": a.workflow_contract_sha256,
    "stage": a.stage,
    "result": "passed",
    "fixtureOnly": contract["fixtureOnly"],
    "mutationPerformed": mutating,
    "acceptanceStartFenceWritten": a.stage == "createZeroAssetAcceptanceFence",
    "checks": {"fixtureHelperPassed": True},
    "completedAtUtc": datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).strftime("%Y-%m-%dT%H:%M:%SZ"),
}
fd = os.open(a.result, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o400)
with os.fdopen(fd, "wb") as handle:
    handle.write((json.dumps(value, indent=2, sort_keys=True) + "\n").encode())
'''


CLOSURE_TOOL = r'''#!/usr/bin/env python3
import json, os, sys
with open(os.environ["TEST_ADAPTER_TRACE"], "a", encoding="utf-8") as handle:
    handle.write(json.dumps({"argv": sys.argv[1:], "tool": "preResetClosure"}, sort_keys=True) + "\n")
args = sys.argv[1:]
output = args[args.index("--output") + 1]
value = {"kind": "fixture-pre-reset-closure", "mutationPerformed": False}
fd = os.open(output, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o400)
with os.fdopen(fd, "wb") as handle:
    handle.write((json.dumps(value, indent=2, sort_keys=True) + "\n").encode())
'''


DUMMY_TOOL = r'''#!/usr/bin/env python3
raise SystemExit("NONDEPLOYABLE fixture tool must not execute")
'''


HOST_ACTION = r'''#!/usr/bin/env python3
import argparse, datetime, hashlib, json, os
p = argparse.ArgumentParser()
p.add_argument("--context", required=True)
p.add_argument("--result", required=True)
a = p.parse_args()
if os.environ.get("NEXUS_V2_ROLLBACK_PROTECTED_CONFIRMATION") != "PRIVATE_ALPHA_ROLLBACK_ONLY":
    raise SystemExit(71)
with open(a.context, encoding="utf-8") as handle:
    context = json.load(handle)
with open(os.environ["TEST_ADAPTER_TRACE"], "a", encoding="utf-8") as handle:
    handle.write(json.dumps({"action": context["action"], "mode": context["mode"]}, sort_keys=True) + "\n")
archive = None
if context["mode"] == "execute" and context["action"] != "pause-v2-writes":
    archive = hashlib.sha256(b"fixture-failed-v2-root").hexdigest()
dry_checks = {
    "pause-v2-writes": {"sourceIdentityPinned", "credentialsResolvable", "requiredResetArchivesPresent", "pausePlanSafe", "restoreExcluded"},
    "archive-failed-v2": {"sourceIdentityPinned", "credentialsResolvable", "requiredResetArchivesPresent", "archivePlanSafe", "restoreExcluded"},
    "restore-final-backup": {"sourceIdentityPinned", "credentialsResolvable", "requiredResetArchivesPresent", "finalBackupInputsMatched", "failedV2ArchiveRequired", "existingRestoreScriptPinned", "existingDeployScriptsPinned", "restorePlanSafe"},
    "restored-smoke": {"sourceIdentityPinned", "credentialsResolvable", "requiredResetArchivesPresent", "restoredSmokeProbePlanned"},
}
exec_checks = {
    "pause-v2-writes": {"sourceIdentityPinned", "requiredResetArchivesPresent", "v2WritesPaused", "statePreserved", "restoreNotAttempted"},
    "archive-failed-v2": {"sourceIdentityPinned", "requiredResetArchivesPresent", "failedV2RootArchived", "archiveManifestImmutable"},
    "restore-final-backup": {"sourceIdentityPinned", "requiredResetArchivesPresent", "failedV2RootArchivePresent", "finalBackupHashesVerified", "restoreEvidenceMatched", "existingRestoreScriptUsed", "existingDeployScriptsUsed", "restoreCompleted"},
    "restored-smoke": {"sourceIdentityPinned", "requiredResetArchivesPresent", "failedV2RootArchivePresent", "componentHealthy", "backupIdentityReadback", "economicFlagsDisabled"},
}
prepare_checks = {
    "archivePreparationNonDestructive", "archivesPreparedAndReadOnly",
    "credentialsResolvable", "currentAlphaStatePreserved", "noResetApplied",
    "readinessIdentityBound", "sourceIdentityPinned",
}
executed = context["mode"] in {"prepare", "execute"}
checks = (
    prepare_checks
    if context["mode"] == "prepare"
    else exec_checks[context["action"]]
    if context["mode"] == "execute"
    else dry_checks[context["action"]]
)
value = {
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
    "remoteActionsExecuted": executed,
    "alreadyApplied": False,
    "requiredResetArchives": {"node": True, "media": True},
    "failedV2RootArchiveSha256": archive,
    "remoteIdempotencyMarkerSha256": hashlib.sha256(b"marker").hexdigest() if executed else None,
    "checks": {name: True for name in checks},
    "completedAtUtc": datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).strftime("%Y-%m-%dT%H:%M:%SZ"),
}
fd = os.open(a.result, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o400)
with os.fdopen(fd, "wb") as handle:
    handle.write((json.dumps(value, indent=2, sort_keys=True) + "\n").encode())
'''


STAGING_LIBRARY = r'''import datetime, hashlib, json, os, shutil
from pathlib import Path

STAGING_NAMES = {
    ("node", "node-data"): "node-data.tar.gz",
    ("node", "node-binary"): "node-binary",
    ("media", "media-state"): "media-state.tar.gz",
    ("media", "media-image-lock"): "media-image-lock.json",
    ("ipfs", "ipfs-data"): "ipfs-data.tar.gz",
    ("ipfs", "ipfs-staging"): "ipfs-staging.tar.gz",
    ("config", "node-env"): "node.env",
    ("config", "media-env"): "media.env",
    ("config", "economic-gates"): "backup-economic-gates.json",
    ("config", "chain-spec"): "chain-spec.json",
    ("service", "node-service"): "node-service.service",
    ("service", "media-service"): "media-service.json",
}

def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def stage_final_backup(context, destination):
    destination = Path(destination)
    destination.mkdir(parents=True, exist_ok=False)
    manifest = json.loads(Path(context["manifestPath"]).read_text())
    entries = {(entry["group"], entry["name"]): entry for entry in manifest["artifacts"]}
    hashes = {}
    for key, name in STAGING_NAMES.items():
        source = Path(context["bundleRoot"]) / entries[key]["path"]
        target = destination / name
        shutil.copyfile(source, target)
        hashes[name] = digest(target)
    contract = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-chain-media-restore-staging",
        "releaseId": context["plan"]["releaseId"],
        "sourceCommit": context["plan"]["sourceCommit"],
        "componentSourceCommits": context["componentCommits"],
        "planSha256": context["planSha256"],
        "backupManifestSha256": context["manifestSha256"],
        "files": hashes,
        "createdAtUtc": datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).strftime("%Y-%m-%dT%H:%M:%SZ"),
    }
    (destination / "staging-contract.json").write_text(json.dumps(contract, sort_keys=True, separators=(",", ":")) + "\n")
'''


class AdapterTestCase(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(
            prefix="nexus-v2-pre-reset-adapter-"
        )
        self.root = Path(self.temporary.name).resolve()
        os.chmod(self.root, 0o700)
        self.trace = self.root / "trace.jsonl"

    def tearDown(self) -> None:
        for current, directories, files in os.walk(
            self.root, topdown=False, followlinks=False
        ):
            for name in files:
                path = Path(current) / name
                if not path.is_symlink():
                    os.chmod(path, stat.S_IMODE(path.stat().st_mode) | 0o600)
            for name in directories:
                path = Path(current) / name
                if not path.is_symlink():
                    os.chmod(path, stat.S_IMODE(path.stat().st_mode) | 0o700)
        self.temporary.cleanup()

    def executable(self, path: Path, body: str) -> Path:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(body, encoding="utf-8")
        os.chmod(path, 0o700)
        return path

    def environment(self, roots: Mapping[str, Path]) -> dict[str, str]:
        environment = os.environ.copy()
        environment["TEST_ADAPTER_TRACE"] = str(self.trace)
        for source_id, root in roots.items():
            environment[
                f"NEXUS_V2_PRE_RESET_IMMUTABLE_{source_id.upper()}_ROOT"
            ] = str(root)
        return environment

    def trace_values(self) -> list[dict[str, Any]]:
        if not self.trace.exists():
            return []
        return [json.loads(line) for line in self.trace.read_text().splitlines()]

    def run_python(
        self, script: Path, arguments: list[str], environment: Mapping[str, str]
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(script), *arguments],
            env=dict(environment),
            check=False,
            text=True,
            capture_output=True,
        )


class ReplacementWorkflowDriverTests(AdapterTestCase):
    STAGES = (
        "createPreResetClosure",
        "deployChainMediaAuthority",
        "deploySiteIndexer",
        "closeIngressAndObserve",
        "createZeroAssetAcceptanceFence",
    )

    def fixture(self, label: str) -> dict[str, Any]:
        case = self.root / label
        roots = {name: case / f"immutable-{name}" for name in ("chain", "media", "site")}
        for root in roots.values():
            root.mkdir(parents=True, mode=0o700)
        helper = self.executable(roots["chain"] / "stage-helper", STAGE_HELPER)
        dummy = self.executable(roots["chain"] / "nested-tool", DUMMY_TOOL)
        artifact = case / "frozen-artifact.json"
        write_json(artifact, {"frozen": True})
        acceptance = case / "zero-asset-acceptance-start.json"
        contract = {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-replacement-workflow-contract",
            "operationId": OPERATION_ID,
            "releaseId": RELEASE_ID,
            "siteReleaseVersion": SITE_RELEASE_VERSION,
            "sourceCommit": SOURCE_COMMIT,
            "frozenFinalizedBlock": FROZEN_BLOCK,
            "artifactSha256": {"frozenArtifact": digest(artifact)},
            "toolPins": {
                role: pin("chain", roots["chain"], dummy)
                for role in (
                    "preResetClosure",
                    "chainDeployAll",
                    "siteDeploy",
                    "phase1IngressClosure",
                    "acceptanceBoundary",
                    "postCutoverCoordinator",
                )
            },
            "stageOrder": list(self.STAGES),
            "stageInputs": {stage: {} for stage in self.STAGES},
            "fixtureOnly": True,
            "acceptanceStartFencePath": str(acceptance),
            "bootstrapOrAcceptanceWritesAllowed": False,
            "paidOrPublicActivationAllowed": False,
        }
        contract_path = case / "workflow-contract.json"
        write_json(contract_path, contract)
        helper_pin = pin("chain", roots["chain"], helper)
        plan = {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-pre-reset-rollback-supervisor-plan",
            "operationId": OPERATION_ID,
            "releaseId": RELEASE_ID,
            "siteReleaseVersion": SITE_RELEASE_VERSION,
            "sourceCommit": SOURCE_COMMIT,
            "frozenFinalizedBlock": FROZEN_BLOCK,
            "backend": "fixture-nondeployable",
            "artifacts": {
                "frozenArtifact": {"path": str(artifact), "sha256": digest(artifact)}
            },
            "sources": {
                name: {"root": str(root), "expectedCommit": SOURCE_COMMIT}
                for name, root in roots.items()
            },
            "workflow": {
                "contract": {
                    "path": str(contract_path),
                    "sha256": digest(contract_path),
                },
                "helperPins": {stage: helper_pin for stage in self.STAGES},
            },
            "acceptanceStartFence": {"handoffPath": str(acceptance)},
        }
        plan_path = case / "supervisor-plan.json"
        write_json(plan_path, plan)
        arm = case / "automatic-restore-arm.json"
        write_json(arm, {"automaticRestoreArmed": True})
        state = case / "workflow-state"
        state.mkdir(mode=0o700)
        result = state / "workflow-result.json"
        environment = self.environment(roots)
        arguments = [
            "--plan",
            str(plan_path),
            "--plan-sha256",
            digest(plan_path),
            "--workflow-contract",
            str(contract_path),
            "--workflow-contract-sha256",
            digest(contract_path),
            "--automatic-restore-arm",
            str(arm),
            "--automatic-restore-arm-sha256",
            digest(arm),
            "--result",
            str(result),
        ]
        return {
            "roots": roots,
            "environment": environment,
            "arguments": arguments,
            "plan": plan_path,
            "planSha256": digest(plan_path),
            "contract": contract_path,
            "contractSha256": digest(contract_path),
            "arm": arm,
            "armSha256": digest(arm),
            "acceptance": acceptance,
            "state": state,
            "result": result,
        }

    def test_happy_path_uses_exact_stage_order_and_canonical_result(self) -> None:
        fixture = self.fixture("workflow-happy")
        completed = self.run_python(
            WORKFLOW_DRIVER, fixture["arguments"], fixture["environment"]
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        trace = self.trace_values()
        self.assertEqual([item["stage"] for item in trace], list(self.STAGES))
        for stage, item in zip(self.STAGES, trace, strict=True):
            arguments = item["argv"]
            expected = [
                "--plan",
                str(fixture["plan"]),
                "--plan-sha256",
                fixture["planSha256"],
                "--workflow-contract",
                str(fixture["contract"]),
                "--workflow-contract-sha256",
                fixture["contractSha256"],
                "--automatic-restore-arm",
                str(fixture["arm"]),
                "--automatic-restore-arm-sha256",
                fixture["armSha256"],
                "--stage",
                stage,
                "--workflow-state-root",
                str(fixture["state"]),
                "--stage-state-root",
                str(fixture["state"] / "stages" / stage),
                "--result",
                str(fixture["state"] / "stages" / stage / "result.json"),
            ]
            self.assertEqual(arguments, expected)
        payload = fixture["result"].read_bytes()
        value = json.loads(payload)
        self.assertEqual(payload, canonical(value))
        self.assertEqual(value["result"], "passed")
        self.assertTrue(value["acceptanceStartFenceWritten"])
        self.assertTrue(fixture["acceptance"].is_file())

    def test_stage_failure_stops_later_stages_and_writes_no_result(self) -> None:
        fixture = self.fixture("workflow-stage-failure")
        environment = dict(fixture["environment"])
        environment["TEST_FAIL_STAGE"] = "deploySiteIndexer"
        completed = self.run_python(
            WORKFLOW_DRIVER, fixture["arguments"], environment
        )
        self.assertEqual(completed.returncode, 2)
        self.assertIn("deploySiteIndexer helper failed", completed.stderr)
        self.assertEqual(
            [item["stage"] for item in self.trace_values()],
            list(self.STAGES[:3]),
        )
        self.assertFalse(fixture["result"].exists())
        self.assertFalse(fixture["acceptance"].exists())
        self.assertFalse(
            (fixture["state"] / "stages" / "closeIngressAndObserve").exists()
        )

    def test_existing_result_is_never_overwritten_or_followed_by_helpers(self) -> None:
        fixture = self.fixture("workflow-no-overwrite")
        sentinel = b"immutable prior workflow result\n"
        fixture["result"].write_bytes(sentinel)
        completed = self.run_python(
            WORKFLOW_DRIVER, fixture["arguments"], fixture["environment"]
        )
        self.assertEqual(completed.returncode, 2)
        self.assertIn("refusing to overwrite workflow result", completed.stderr)
        self.assertEqual(fixture["result"].read_bytes(), sentinel)
        self.assertEqual(self.trace_values(), [])


class ChainWorkflowStageDriverTests(AdapterTestCase):
    def fixture(self, label: str) -> dict[str, Any]:
        case = self.root / label
        roots = {name: case / f"immutable-{name}" for name in ("chain", "media", "site")}
        for root in roots.values():
            root.mkdir(parents=True, mode=0o700)
        closure_tool = self.executable(
            roots["chain"] / "pre-reset-closure", CLOSURE_TOOL
        )
        deploy_tool = self.executable(roots["chain"] / "deploy-all", DUMMY_TOOL)
        dummy = self.executable(roots["chain"] / "dummy-tool", DUMMY_TOOL)
        bundle = case / "bundle"
        bundle.mkdir(mode=0o700)
        artifacts: dict[str, dict[str, str]] = {}
        for name in (
            "finalFreezePlan",
            "replacementLock",
            "resetReadiness",
            "finalFreezeEvidence",
            "backupManifest",
            "restoreEvidence",
            "migrationEvidence",
        ):
            path = (
                bundle / "backup-manifest.json"
                if name == "backupManifest"
                else case / "artifacts" / f"{name}.json"
            )
            write_json(path, {"artifact": name})
            artifacts[name] = {"path": str(path), "sha256": digest(path)}
        candidates: dict[str, Path] = {}
        for name in ("nodeCandidate", "nodeTargetIdentity", "mediaCandidate"):
            path = case / "candidates" / f"{name}.json"
            write_json(path, {"candidate": name})
            candidates[name] = path
        state = case / "workflow-state"
        state.mkdir(mode=0o700)
        acceptance = case / "zero-asset-acceptance-start.json"
        tool_pins = {
            "preResetClosure": pin("chain", roots["chain"], closure_tool),
            "chainDeployAll": pin("chain", roots["chain"], deploy_tool),
            **{
                role: pin("chain", roots["chain"], dummy)
                for role in (
                    "siteDeploy",
                    "phase1IngressClosure",
                    "acceptanceBoundary",
                    "postCutoverCoordinator",
                )
            },
        }
        stage_inputs = {
            "createPreResetClosure": {},
            "deployChainMediaAuthority": {
                **{
                    f"{name}Path": str(path) for name, path in candidates.items()
                },
                **{
                    f"{name}Sha256": digest(path)
                    for name, path in candidates.items()
                },
            },
            "deploySiteIndexer": {},
            "closeIngressAndObserve": {},
            "createZeroAssetAcceptanceFence": {},
        }
        contract = {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-replacement-workflow-contract",
            "operationId": OPERATION_ID,
            "releaseId": RELEASE_ID,
            "siteReleaseVersion": SITE_RELEASE_VERSION,
            "sourceCommit": SOURCE_COMMIT,
            "frozenFinalizedBlock": FROZEN_BLOCK,
            "artifactSha256": {
                name: value["sha256"] for name, value in artifacts.items()
            },
            "toolPins": tool_pins,
            "stageOrder": [
                "createPreResetClosure",
                "deployChainMediaAuthority",
                "deploySiteIndexer",
                "closeIngressAndObserve",
                "createZeroAssetAcceptanceFence",
            ],
            "stageInputs": stage_inputs,
            "fixtureOnly": True,
            "acceptanceStartFencePath": str(acceptance),
            "bootstrapOrAcceptanceWritesAllowed": False,
            "paidOrPublicActivationAllowed": False,
        }
        contract_path = case / "workflow-contract.json"
        write_json(contract_path, contract)
        plan = {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-pre-reset-rollback-supervisor-plan",
            "operationId": OPERATION_ID,
            "releaseId": RELEASE_ID,
            "siteReleaseVersion": SITE_RELEASE_VERSION,
            "sourceCommit": SOURCE_COMMIT,
            "frozenFinalizedBlock": FROZEN_BLOCK,
            "backend": "fixture-nondeployable",
            "bundleRoot": str(bundle),
            "selectedDeploymentEnvironment": "private-alpha",
            "selectedSiteDeploymentEnvironment": "private-alpha",
            "artifacts": artifacts,
        }
        plan_path = case / "supervisor-plan.json"
        write_json(plan_path, plan)
        arm = case / "automatic-restore-arm.json"
        write_json(
            arm,
            {
                "releaseId": RELEASE_ID,
                "siteReleaseVersion": SITE_RELEASE_VERSION,
                "sourceCommit": SOURCE_COMMIT,
                "planSha256": digest(plan_path),
                "automaticRestoreArmed": True,
            },
        )
        environment = self.environment(roots)
        environment.update(
            {
                "NEXUS_V2_PRE_RESET_CHAIN_RELEASE_ID": RELEASE_ID,
                "NEXUS_V2_PRE_RESET_SITE_RELEASE_VERSION": SITE_RELEASE_VERSION,
                "NEXUS_V2_PRE_RESET_SOURCE_COMMIT": SOURCE_COMMIT,
            }
        )
        return {
            "environment": environment,
            "plan": plan_path,
            "planSha256": digest(plan_path),
            "contract": contract_path,
            "contractSha256": digest(contract_path),
            "arm": arm,
            "armSha256": digest(arm),
            "state": state,
            "artifacts": artifacts,
        }

    def stage_arguments(
        self, fixture: Mapping[str, Any], stage: str
    ) -> tuple[list[str], Path]:
        stage_root = fixture["state"] / "stages" / stage
        stage_root.mkdir(parents=True, mode=0o700)
        result = stage_root / "result.json"
        return (
            [
                "--plan",
                str(fixture["plan"]),
                "--plan-sha256",
                fixture["planSha256"],
                "--workflow-contract",
                str(fixture["contract"]),
                "--workflow-contract-sha256",
                fixture["contractSha256"],
                "--automatic-restore-arm",
                str(fixture["arm"]),
                "--automatic-restore-arm-sha256",
                fixture["armSha256"],
                "--stage",
                stage,
                "--workflow-state-root",
                str(fixture["state"]),
                "--stage-state-root",
                str(stage_root),
                "--result",
                str(result),
            ],
            result,
        )

    def test_fixture_closure_and_chain_deploy_are_closed_and_canonical(self) -> None:
        fixture = self.fixture("chain-stages")
        create_args, create_result = self.stage_arguments(
            fixture, "createPreResetClosure"
        )
        created = self.run_python(
            CHAIN_STAGE_DRIVER, create_args, fixture["environment"]
        )
        self.assertEqual(created.returncode, 0, created.stderr)
        trace = self.trace_values()
        self.assertEqual(len(trace), 1)
        argv = trace[0]["argv"]
        self.assertEqual(argv[0], "create")
        self.assertEqual(
            argv[argv.index("--automatic-restore-arm") + 1],
            str(fixture["arm"]),
        )
        self.assertEqual(
            argv[argv.index("--expected-automatic-restore-arm-sha256") + 1],
            fixture["armSha256"],
        )
        self.assertEqual(
            argv[argv.index("--output") + 1],
            str(
                fixture["state"]
                / "stages/createPreResetClosure/pre-reset-closure.json"
            ),
        )
        for artifact_flag, artifact_name in (
            ("replacement-lock", "replacementLock"),
            ("reset-readiness", "resetReadiness"),
            ("final-freeze-evidence", "finalFreezeEvidence"),
            ("backup-manifest", "backupManifest"),
            ("restore-evidence", "restoreEvidence"),
            ("migration-evidence", "migrationEvidence"),
        ):
            self.assertEqual(
                argv[argv.index(f"--{artifact_flag}") + 1],
                fixture["artifacts"][artifact_name]["path"],
            )
            self.assertEqual(
                argv[argv.index(f"--expected-{artifact_flag}-sha256") + 1],
                fixture["artifacts"][artifact_name]["sha256"],
            )
        create_payload = create_result.read_bytes()
        self.assertEqual(create_payload, canonical(json.loads(create_payload)))

        deploy_args, deploy_result = self.stage_arguments(
            fixture, "deployChainMediaAuthority"
        )
        deployed = self.run_python(
            CHAIN_STAGE_DRIVER, deploy_args, fixture["environment"]
        )
        self.assertEqual(deployed.returncode, 0, deployed.stderr)
        self.assertEqual(len(self.trace_values()), 1, "fixture invoked deploy tool")
        marker = (
            fixture["state"]
            / "stages/deployChainMediaAuthority/NONDEPLOYABLE.fixture"
        )
        self.assertEqual(marker.read_text(), "no protected host contacted\n")
        deploy_payload = deploy_result.read_bytes()
        deploy_value = json.loads(deploy_payload)
        self.assertEqual(deploy_payload, canonical(deploy_value))
        self.assertTrue(deploy_value["fixtureOnly"])
        self.assertTrue(deploy_value["mutationPerformed"])


class ChainMediaComponentDriverTests(AdapterTestCase):
    def test_isolated_copied_driver_has_no_repository_import_dependency(self) -> None:
        isolated_root = self.root / "isolated-driver"
        isolated_root.mkdir(mode=0o700)
        driver = isolated_root / "chain-media-component-driver"
        shutil.copyfile(CHAIN_MEDIA_DRIVER, driver)
        os.chmod(driver, 0o700)
        environment = {
            "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
            "PYTHONPATH": "",
        }
        completed = subprocess.run(
            [sys.executable, "-I", str(driver), "--help"],
            cwd=isolated_root,
            env=environment,
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertNotIn("ModuleNotFoundError", completed.stderr)

    def fixture(self, label: str) -> dict[str, Any]:
        case = self.root / label
        roots = {name: case / f"immutable-{name}" for name in ("chain", "media", "site")}
        for root in roots.values():
            root.mkdir(parents=True, mode=0o700)
        driver = roots["chain"] / "chain-media-component-driver"
        shutil.copyfile(CHAIN_MEDIA_DRIVER, driver)
        os.chmod(driver, 0o700)
        host = self.executable(roots["chain"] / "host-action", HOST_ACTION)
        command = self.executable(roots["chain"] / "component-command", DUMMY_TOOL)
        staging = roots["chain"] / "rollback-staging-library"
        shutil.copyfile(ROLLBACK_STAGING, staging)
        os.chmod(staging, 0o700)
        bundle = case / "bundle"
        bundle.mkdir(mode=0o700)
        restore_roles = {
            ("node", "node-data"): "node-data.tar.gz",
            ("node", "node-binary"): "node-binary",
            ("media", "media-state"): "media-state.tar.gz",
            ("media", "media-image-lock"): "media-image-lock.json",
            ("ipfs", "ipfs-data"): "ipfs-data.tar.gz",
            ("ipfs", "ipfs-staging"): "ipfs-staging.tar.gz",
            ("config", "node-env"): "node.env",
            ("config", "media-env"): "media.env",
            ("config", "economic-gates"): "backup-economic-gates.json",
            ("config", "chain-spec"): "chain-spec.json",
            ("service", "node-service"): "node-service.service",
            ("service", "media-service"): "media-service.json",
        }
        backup_entries = []
        for (group, name), relative in restore_roles.items():
            path = bundle / relative
            path.write_bytes(f"fixture:{group}:{name}\n".encode())
            backup_entries.append(
                {
                    "group": group,
                    "name": name,
                    "path": relative,
                    "bytes": path.stat().st_size,
                    "sha256": digest(path),
                }
            )
        artifacts: dict[str, dict[str, str]] = {}
        for name in (
            "backupManifest",
            "restoreEvidence",
            "resetReadiness",
            "finalFreezeEvidence",
            "migrationEvidence",
        ):
            path = (
                bundle / "backup-manifest.json"
                if name == "backupManifest"
                else case / "artifacts" / f"{name}.json"
            )
            if name == "backupManifest":
                write_json(
                    path,
                    {
                        "schemaVersion": 1,
                        "kind": "nexus-v2-private-alpha-backup",
                        "releaseId": RELEASE_ID,
                        "sourceCommit": SOURCE_COMMIT,
                        "artifacts": backup_entries,
                    },
                )
            else:
                write_json(path, {"artifact": name})
            artifacts[name] = {"path": str(path), "sha256": digest(path)}
        readiness_hash = artifacts["resetReadiness"]["sha256"]
        plan = {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-pre-reset-rollback-supervisor-plan",
            "operationId": OPERATION_ID,
            "releaseId": RELEASE_ID,
            "siteReleaseVersion": SITE_RELEASE_VERSION,
            "sourceCommit": SOURCE_COMMIT,
            "backend": "protected-private-alpha",
            "fixtureRoot": None,
            "bundleRoot": str(bundle),
            "sources": {
                name: {"root": str(root), "expectedCommit": SOURCE_COMMIT}
                for name, root in roots.items()
            },
            "artifacts": artifacts,
            "components": {
                "chain-media": {
                    "driver": pin("chain", roots["chain"], driver),
                    "helperPins": {
                        "hostAction": pin("chain", roots["chain"], host)
                    },
                    "scriptPins": {
                        **{
                            role: pin("chain", roots["chain"], command)
                            for role in (
                                "restoreState",
                                "deployNode",
                                "deployMedia",
                                "status",
                            )
                        },
                        "rollbackStagingLibrary": pin(
                            "chain", roots["chain"], staging
                        ),
                    },
                    "requiredResetArchives": {
                        role: (
                            "/opt/eterra-alpha/archive/nexus-v2-fresh-reset/"
                            f"{readiness_hash}/{role}"
                        )
                        for role in ("node", "media")
                    },
                },
                "site-indexer": {},
            },
        }
        plan_path = case / "supervisor-plan.json"
        write_json(plan_path, plan)
        environment = self.environment(roots)
        environment.update(
            {
                "NEXUS_V2_PRE_RESET_CHAIN_RELEASE_ID": RELEASE_ID,
                "NEXUS_V2_PRE_RESET_SITE_RELEASE_VERSION": SITE_RELEASE_VERSION,
                "NEXUS_V2_PRE_RESET_SOURCE_COMMIT": SOURCE_COMMIT,
                "NEXUS_V2_PRE_RESET_PLAN_SHA256": digest(plan_path),
            }
        )
        result = case / "component-result.json"
        return {
            "driver": driver,
            "environment": environment,
            "plan": plan_path,
            "planSha256": digest(plan_path),
            "result": result,
        }

    def arguments(
        self, fixture: Mapping[str, Any], mode: str, action: str
    ) -> list[str]:
        return [
            "--plan",
            str(fixture["plan"]),
            "--plan-sha256",
            fixture["planSha256"],
            "--component",
            "chain-media",
            "--mode",
            mode,
            "--action",
            action,
            "--result",
            str(fixture["result"]),
        ]

    def test_protected_preflight_requires_explicit_confirmation(self) -> None:
        fixture = self.fixture("component-confirmation")
        environment = dict(fixture["environment"])
        environment.pop("NEXUS_V2_PRE_RESET_PRODUCTION_CONFIRMATION", None)
        completed = self.run_python(
            fixture["driver"],
            self.arguments(fixture, "preflight", "preflight"),
            environment,
        )
        self.assertEqual(completed.returncode, 2)
        self.assertIn("PRIVATE_ALPHA_ROLLBACK_ONLY confirmation", completed.stderr)
        self.assertFalse(fixture["result"].exists())
        self.assertEqual(self.trace_values(), [])

    def test_protected_preflight_emits_canonical_supervisor_result(self) -> None:
        fixture = self.fixture("component-canonical")
        environment = dict(fixture["environment"])
        environment[
            "NEXUS_V2_PRE_RESET_PRODUCTION_CONFIRMATION"
        ] = "PRIVATE_ALPHA_ROLLBACK_ONLY"
        completed = self.run_python(
            fixture["driver"],
            self.arguments(fixture, "preflight", "preflight"),
            environment,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(
            self.trace_values(),
            [
                {"action": "pause-v2-writes", "mode": "dry-run"},
                {"action": "archive-failed-v2", "mode": "dry-run"},
                {"action": "restore-final-backup", "mode": "dry-run"},
                {"action": "restored-smoke", "mode": "dry-run"},
            ],
        )
        payload = fixture["result"].read_bytes()
        value = json.loads(payload)
        self.assertEqual(payload, canonical(value))
        self.assertEqual(
            set(value),
            {
                "schemaVersion",
                "kind",
                "operationId",
                "releaseId",
                "siteReleaseVersion",
                "planSha256",
                "componentId",
                "mode",
                "action",
                "result",
                "fixtureOnly",
                "mutationPerformed",
                "credentialsResolvable",
                "requiredResetArchivesPresent",
                "failedV2RootArchiveSha256",
                "checks",
                "completedAtUtc",
            },
        )
        self.assertFalse(value["mutationPerformed"])
        self.assertFalse(value["fixtureOnly"])
        self.assertTrue(all(value["checks"].values()))

    def test_protected_archive_preparation_precedes_truthful_preflight(self) -> None:
        fixture = self.fixture("component-prepare")
        environment = dict(fixture["environment"])
        environment[
            "NEXUS_V2_PRE_RESET_PRODUCTION_CONFIRMATION"
        ] = "PRIVATE_ALPHA_ROLLBACK_ONLY"
        completed = self.run_python(
            fixture["driver"],
            self.arguments(
                fixture, "prepare", "prepare-reset-archives"
            ),
            environment,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(
            self.trace_values(),
            [{"action": "prepare-reset-archives", "mode": "prepare"}],
        )
        value = json.loads(fixture["result"].read_text())
        self.assertEqual(value["mode"], "prepare")
        self.assertEqual(value["action"], "prepare-reset-archives")
        self.assertTrue(value["mutationPerformed"])
        self.assertEqual(set(value["checks"]), {
            "archivePreparationNonDestructive",
            "archivesPreparedAndReadOnly",
            "currentAlphaStatePreserved",
            "noResetApplied",
            "readinessIdentityBound",
            "restoreInputsVerified",
            "sourcePinsVerified",
        })
        self.assertTrue(all(value["checks"].values()))

    def test_existing_result_rejects_before_any_protected_host_action(self) -> None:
        fixture = self.fixture("component-no-overwrite")
        environment = dict(fixture["environment"])
        environment[
            "NEXUS_V2_PRE_RESET_PRODUCTION_CONFIRMATION"
        ] = "PRIVATE_ALPHA_ROLLBACK_ONLY"
        sentinel = canonical({"immutable": "prior-result"})
        fixture["result"].write_bytes(sentinel)
        completed = self.run_python(
            fixture["driver"],
            self.arguments(fixture, "execute", "pause-v2-writes"),
            environment,
        )
        self.assertEqual(completed.returncode, 2)
        self.assertIn("refusing to overwrite", completed.stderr)
        self.assertEqual(fixture["result"].read_bytes(), sentinel)
        self.assertEqual(self.trace_values(), [])


if __name__ == "__main__":
    unittest.main()
