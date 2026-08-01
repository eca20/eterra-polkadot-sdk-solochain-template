from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock


MODULE_PATH = Path(__file__).with_name("pre_reset_zero_asset_fence_stage.py")
SPEC = importlib.util.spec_from_file_location("pre_reset_zero_asset_fence_stage", MODULE_PATH)
assert SPEC and SPEC.loader
stage = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(stage)


SOURCE_COMMIT_PLACEHOLDER = "a" * 40
RELEASE_ID = "nexus-v2-alpha-20260731"
SITE_RELEASE = "v0.1.0-alpha.1"
OPERATION_ID = "replace-20260731"
GENESIS_HASH = "0x" + "b" * 64


def canonical(value: dict) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def write_json(path: Path, value: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(canonical(value))


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


class Fixture:
    def __init__(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name).resolve()
        self.sources = {
            name: self.root / f"source-{name}" for name in stage.SOURCE_IDS
        }
        for source_id, root in self.sources.items():
            root.mkdir()
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            (root / "README").write_text(source_id + "\n", encoding="utf-8")

        self.tool_paths = {
            "preResetClosure": (
                "chain",
                "scripts/nexus-v2-private-alpha/pre_reset_closure.py",
            ),
            "chainDeployAll": ("chain", "deploy/alpha/macmini2010/deploy-all.sh"),
            "siteDeploy": ("site", "tcg/deploy/alpha/macmini2014/deploy-site.sh"),
            "phase1IngressClosure": (
                "site",
                "tcg/deploy/alpha/macmini2014/nexus_v2_phase1_ingress_closure.py",
            ),
            "acceptanceBoundary": stage.EXPECTED_TOOL_PATHS["acceptanceBoundary"],
            "postCutoverCoordinator": stage.EXPECTED_TOOL_PATHS[
                "postCutoverCoordinator"
            ],
        }
        for _, (source_id, relative) in self.tool_paths.items():
            self.executable(self.sources[source_id] / relative)
        self.supervisor_relative = (
            "scripts/nexus-v2-private-alpha/pre_reset_rollback_supervisor.py"
        )
        self.executable(self.sources["chain"] / self.supervisor_relative)

        self.site_paths = {
            "siteDriverPath": "tcg/deploy/alpha/macmini2014/nexus-v2-rollback-component-driver",
            "siteRestorePath": "tcg/deploy/alpha/macmini2014/restore-alpha-state.sh",
            "siteDeployPath": "tcg/deploy/alpha/macmini2014/deploy-site.sh",
            "siteStatusPath": "tcg/deploy/alpha/macmini2014/status.sh",
        }
        for relative in self.site_paths.values():
            self.executable(self.sources["site"] / relative)

        self.commits: dict[str, str] = {}
        for source_id, root in self.sources.items():
            subprocess.run(["git", "-C", str(root), "add", "."], check=True)
            subprocess.run(
                [
                    "git",
                    "-C",
                    str(root),
                    "-c",
                    "user.name=Codex Test",
                    "-c",
                    "user.email=codex@example.invalid",
                    "commit",
                    "-qm",
                    "fixture",
                ],
                check=True,
            )
            self.commits[source_id] = subprocess.check_output(
                ["git", "-C", str(root), "rev-parse", "HEAD"], text=True
            ).strip()

        self.runtime_root = self.root / "runtime"
        self.runtime_root.mkdir()
        write_json(
            self.runtime_root / "runtime-bundle-manifest.json",
            {"kind": "fixture-runtime-bundle"},
        )
        self.runtime_manifest_sha = digest(
            self.runtime_root / "runtime-bundle-manifest.json"
        )
        self.bundle_root = self.root / "bundle"
        self.bundle_root.mkdir()
        self.artifact_root = self.root / "artifacts"
        self.artifact_root.mkdir()
        self.artifacts: dict[str, Path] = {}
        for artifact_id in sorted(stage.ARTIFACT_IDS - {"replacementLock"}):
            path = self.artifact_root / f"{artifact_id}.json"
            write_json(path, {"artifact": artifact_id})
            self.artifacts[artifact_id] = path

        replacement_lock = self.artifact_root / "replacement-lock.json"
        write_json(
            replacement_lock,
            {
                "schemaVersion": 1,
                "kind": "nexus-v2-private-alpha-pre-cutover-replacement-lock",
                "releaseId": RELEASE_ID,
                "repositories": {
                    "chain": {"head": self.commits["chain"]},
                    "media": {"head": self.commits["media"]},
                    "web": {"head": self.commits["site"]},
                },
                "artifacts": {
                    "runtimeBundleManifest": {
                        "path": str(
                            self.runtime_root / "runtime-bundle-manifest.json"
                        ),
                        "sha256": self.runtime_manifest_sha,
                    }
                },
            },
        )
        self.artifacts["replacementLock"] = replacement_lock

        self.workflow_root = self.root / "workflow"
        self.prior_root = self.workflow_root / "stages" / stage.PRIOR_STAGE
        self.phase1_root = self.prior_root / "phase1-output"
        self.stage_root = self.workflow_root / "stages" / stage.STAGE
        self.phase1_root.mkdir(parents=True)
        self.stage_root.mkdir(parents=True)
        self.receipt = self.root / "zero-asset-receipt.json"
        self.plan_path = self.root / "plan.json"
        self.contract_path = self.root / "workflow-contract.json"
        self.arm_path = self.root / "arm.json"

        self.tool_pins = {
            role: {
                "sourceId": source_id,
                "path": relative,
                "sha256": digest(self.sources[source_id] / relative),
            }
            for role, (source_id, relative) in self.tool_paths.items()
        }
        readiness_sha = digest(self.artifacts["resetReadiness"])
        archive_root = str(self.root / "reset-archives")
        self.stage_input = {
            "runtimeBundleRoot": str(self.runtime_root),
            "runtimeBundleManifestSha256": self.runtime_manifest_sha,
            **self.site_paths,
            "resetArchiveRoot": archive_root,
            "maxObservationAgeSeconds": 120,
        }
        self.plan = {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-pre-reset-rollback-supervisor-plan",
            "operationId": OPERATION_ID,
            "releaseId": RELEASE_ID,
            "siteReleaseVersion": SITE_RELEASE,
            "sourceCommit": self.commits["chain"],
            "backend": stage.FIXTURE_BACKEND,
            "bundleRoot": str(self.bundle_root),
            "frozenFinalizedBlock": {"number": 10, "hash": "0x" + "c" * 64},
            "sources": {
                source_id: {
                    "root": str(root),
                    "expectedCommit": self.commits[source_id],
                }
                for source_id, root in self.sources.items()
            },
            "artifacts": {
                artifact_id: {"path": str(path), "sha256": digest(path)}
                for artifact_id, path in self.artifacts.items()
            },
            "workflow": {},
            "supervisor": {
                "sourceId": "chain",
                "path": self.supervisor_relative,
                "sha256": digest(
                    self.sources["chain"] / self.supervisor_relative
                ),
            },
            "acceptanceStartFence": {
                "handoffPath": str(self.receipt),
                "genesisHash": GENESIS_HASH,
                "runtimeCodeSha256": "d" * 64,
                "runtimeMetadataScaleSha256": "e" * 64,
            },
            "components": {
                "chain-media": {
                    "requiredResetArchives": {
                        "node": f"{archive_root}/{readiness_sha}/node",
                        "media": f"{archive_root}/{readiness_sha}/media",
                    }
                },
                "site-indexer": {
                    "requiredResetArchives": {
                        "site": f"{archive_root}/{readiness_sha}/site"
                    },
                    "scriptPins": {
                        "restoreState": self.site_pin("siteRestorePath"),
                        "deploySite": self.site_pin("siteDeployPath"),
                        "status": self.site_pin("siteStatusPath"),
                    },
                },
            },
        }
        self.contract = {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-replacement-workflow-contract",
            "operationId": OPERATION_ID,
            "releaseId": RELEASE_ID,
            "siteReleaseVersion": SITE_RELEASE,
            "sourceCommit": self.commits["chain"],
            "frozenFinalizedBlock": self.plan["frozenFinalizedBlock"],
            "artifactSha256": {
                artifact_id: pin["sha256"]
                for artifact_id, pin in sorted(self.plan["artifacts"].items())
            },
            "toolPins": self.tool_pins,
            "stageOrder": list(stage.STAGES),
            "stageInputs": {
                name: self.stage_input if name == stage.STAGE else {}
                for name in stage.STAGES
            },
            "fixtureOnly": True,
            "acceptanceStartFencePath": str(self.receipt),
            "bootstrapOrAcceptanceWritesAllowed": False,
            "paidOrPublicActivationAllowed": False,
        }
        write_json(self.contract_path, self.contract)
        self.plan["workflow"] = {
            "contract": {
                "path": str(self.contract_path),
                "sha256": digest(self.contract_path),
            },
            "toolPins": self.tool_pins,
        }
        write_json(self.plan_path, self.plan)
        write_json(
            self.arm_path,
            {
                "operationId": OPERATION_ID,
                "releaseId": RELEASE_ID,
                "siteReleaseVersion": SITE_RELEASE,
                "sourceCommit": self.commits["chain"],
                "fixtureOnly": True,
                "automaticRestoreArmed": True,
                "paidOrPublicActivationAllowed": False,
            },
        )
        self.write_prior_stage()

    def executable(self, path: Path) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        os.chmod(path, 0o700)

    def site_pin(self, field: str) -> dict:
        relative = self.site_paths[field]
        return {
            "sourceId": "site",
            "path": relative,
            "sha256": digest(self.sources["site"] / relative),
        }

    def write_prior_stage(self) -> None:
        write_json(
            self.prior_root / "result.json",
            {
                "schemaVersion": 1,
                "kind": "nexus-v2-private-alpha-replacement-workflow-stage-result",
                "operationId": OPERATION_ID,
                "releaseId": RELEASE_ID,
                "siteReleaseVersion": SITE_RELEASE,
                "sourceCommit": self.commits["chain"],
                "planSha256": digest(self.plan_path),
                "workflowContractSha256": digest(self.contract_path),
                "stage": stage.PRIOR_STAGE,
                "result": "passed",
                "fixtureOnly": True,
                "mutationPerformed": True,
                "acceptanceStartFenceWritten": False,
                "checks": {"closed": True},
                "completedAtUtc": "2026-08-01T00:00:00Z",
            },
        )
        write_json(
            self.phase1_root / "execute-evidence.json",
            {
                "operationId": OPERATION_ID,
                "releaseId": RELEASE_ID,
                "siteReleaseVersion": SITE_RELEASE,
                "sourceCommit": self.commits["chain"],
                "automaticRestoreArmPath": str(self.arm_path),
                "automaticRestoreArmSha256": digest(self.arm_path),
                "siteCandidateUsableForExecute": True,
                "allExternalWriteIngressClosed": True,
                "blockProductionContinues": True,
                "authorityLocalServicePreserved": True,
                "readOnlySiteStackPreserved": True,
                "automaticReopenAuthorized": False,
                "paidOrPublicActivationAuthorized": False,
                "ingressClosedEvidenceSha256": "f" * 64,
            },
        )

    def args(self) -> argparse.Namespace:
        return argparse.Namespace(
            plan=str(self.plan_path),
            plan_sha256=digest(self.plan_path),
            workflow_contract=str(self.contract_path),
            workflow_contract_sha256=digest(self.contract_path),
            automatic_restore_arm=str(self.arm_path),
            automatic_restore_arm_sha256=digest(self.arm_path),
            stage=stage.STAGE,
            workflow_state_root=str(self.workflow_root),
            stage_state_root=str(self.stage_root),
            result=str(self.stage_root / "result.json"),
        )

    def environment(self) -> dict[str, str]:
        return {
            f"NEXUS_V2_PRE_RESET_IMMUTABLE_{source_id.upper()}_ROOT": str(root)
            for source_id, root in self.sources.items()
        }

    def close(self) -> None:
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


class ZeroAssetFenceStageTests(unittest.TestCase):
    def setUp(self) -> None:
        self.fixture = Fixture()

    def tearDown(self) -> None:
        self.fixture.close()

    def validate(self) -> dict:
        with mock.patch.dict(os.environ, self.fixture.environment(), clear=False):
            return stage.validate_inputs(self.fixture.args())

    def test_fixture_inputs_bind_exact_releases_tools_and_prior_stage(self) -> None:
        context = self.validate()
        self.assertEqual(context["releaseId"], RELEASE_ID)
        self.assertEqual(context["siteReleaseVersion"], SITE_RELEASE)
        self.assertEqual(context["phase1"]["root"], self.fixture.phase1_root)
        self.assertEqual(set(context["tools"]), stage.TOOL_ROLES)

    def test_run_emits_closed_nonmutating_stage_result(self) -> None:
        with (
            mock.patch.dict(os.environ, self.fixture.environment(), clear=False),
            mock.patch.object(
                stage,
                "run_pipeline",
                return_value={name: True for name in stage.REQUIRED_CHECKS},
            ),
        ):
            stage.run(self.fixture.args())
        result = json.loads(
            (self.fixture.stage_root / "result.json").read_text(encoding="utf-8")
        )
        self.assertFalse(result["mutationPerformed"])
        self.assertTrue(result["acceptanceStartFenceWritten"])
        self.assertEqual(set(result), stage.STAGE_RESULT_KEYS)

    def test_pipeline_runs_pinned_compose_execute_create_verify_order(self) -> None:
        context = self.validate()
        write_json(
            self.fixture.phase1_root / "post-v16-acceptance-inventory.json",
            {"counts": {"currentCards": 0, "lifetimeSessions": 0}},
        )
        calls: list[str] = []

        def option(arguments: list[str], name: str) -> Path:
            return Path(arguments[arguments.index(name) + 1])

        def fake_run_tool(
            _tool: dict,
            arguments: list[str],
            _log: Path,
            **kwargs: dict,
        ) -> None:
            command = arguments[0]
            calls.append(command)
            if command == "compose-observation":
                write_json(option(arguments, "--output"), {"observation": True})
            elif command == "compose-coordinator-plan":
                write_json(option(arguments, "--output"), {"plan": True})
            elif command == "--plan":
                state_root = option(arguments, "--state-dir")
                state_root.mkdir()
                verification_log = state_root / "external-recovery-ownership.verify.log"
                verification_log.write_text("arm verified\n", encoding="utf-8")
                coordinator_plan = option(arguments, "--plan")
                write_json(
                    state_root / "external-recovery-ownership.json",
                    {
                        "schemaVersion": 1,
                        "kind": "nexus-v2-private-alpha-post-cutover-external-recovery-ownership",
                        "operationId": OPERATION_ID,
                        "planSha256": digest(coordinator_plan),
                        "releaseId": RELEASE_ID,
                        "siteReleaseVersion": SITE_RELEASE,
                        "sourceCommit": self.fixture.commits["chain"],
                        "supervisorPath": str(context["supervisor"]["path"]),
                        "supervisorSha256": context["supervisor"]["sha256"],
                        "automaticRestoreArmPath": str(self.fixture.arm_path),
                        "automaticRestoreArmSha256": digest(self.fixture.arm_path),
                        "fixtureOnly": True,
                        "recoveryOwner": "pre-reset-rollback-supervisor",
                        "nestedRecoveryActionsAllowed": False,
                        "verificationLogPath": str(verification_log),
                        "verificationLogSha256": digest(verification_log),
                        "verifiedAtUtc": "2026-08-01T00:00:00Z",
                    },
                )
                write_json(state_root / "final-evidence.marker.json", {"final": True})
                write_json(
                    option(arguments, "--evidence"),
                    {
                        "decision": "keep-v2",
                        "postCutoverSmokePassed": True,
                        "automaticRestorePerformed": False,
                        "postAcceptanceContainmentPerformed": False,
                        "nonzeroAcceptanceAssets": {},
                        "releaseId": RELEASE_ID,
                        "sourceCommit": self.fixture.commits["chain"],
                    },
                )
                self.assertEqual(
                    kwargs["environment"]["NEXUS_V2_ROLLBACK_PLAN_SHA256"],
                    digest(coordinator_plan),
                )
            elif command == "create-receipt":
                write_json(
                    option(arguments, "--output"),
                    {
                        "coordinatorDecision": "keep-v2",
                        "phase1SmokePassed": True,
                        "automaticRestorePermanentlyDisabled": True,
                        "releaseId": RELEASE_ID,
                        "sourceCommit": self.fixture.commits["chain"],
                        "genesisHash": GENESIS_HASH,
                        "runtimeCodeSha256": "d" * 64,
                        "runtimeMetadataScaleSha256": "e" * 64,
                    },
                )
            elif command != "verify-receipt":
                self.fail(f"unexpected nested tool command: {command}")

        with mock.patch.object(stage, "run_tool", side_effect=fake_run_tool):
            checks = stage.run_pipeline(context)
        self.assertEqual(
            calls,
            [
                "compose-observation",
                "compose-coordinator-plan",
                "--plan",
                "create-receipt",
                "verify-receipt",
            ],
        )
        self.assertEqual(checks, {name: True for name in stage.REQUIRED_CHECKS})
        self.assertTrue(self.fixture.receipt.is_file())

    def test_production_requires_exact_confirmation(self) -> None:
        self.fixture.plan["backend"] = stage.PRODUCTION_BACKEND
        self.fixture.contract["fixtureOnly"] = False
        self.fixture.contract["stageInputs"][stage.STAGE][
            "resetArchiveRoot"
        ] = stage.EXPECTED_PRODUCTION_ARCHIVE_ROOT
        readiness_sha = self.fixture.plan["artifacts"]["resetReadiness"]["sha256"]
        self.fixture.plan["components"]["chain-media"]["requiredResetArchives"] = {
            "node": f"{stage.EXPECTED_PRODUCTION_ARCHIVE_ROOT}/{readiness_sha}/node",
            "media": f"{stage.EXPECTED_PRODUCTION_ARCHIVE_ROOT}/{readiness_sha}/media",
        }
        self.fixture.plan["components"]["site-indexer"]["requiredResetArchives"] = {
            "site": f"{stage.EXPECTED_PRODUCTION_ARCHIVE_ROOT}/{readiness_sha}/site"
        }
        write_json(self.fixture.contract_path, self.fixture.contract)
        self.fixture.plan["workflow"]["contract"]["sha256"] = digest(
            self.fixture.contract_path
        )
        write_json(self.fixture.plan_path, self.fixture.plan)
        self.fixture.write_prior_stage()
        write_json(
            self.fixture.arm_path,
            {
                "operationId": OPERATION_ID,
                "releaseId": RELEASE_ID,
                "siteReleaseVersion": SITE_RELEASE,
                "sourceCommit": self.fixture.commits["chain"],
                "fixtureOnly": False,
                "automaticRestoreArmed": True,
                "paidOrPublicActivationAllowed": False,
            },
        )
        self.fixture.write_prior_stage()
        with mock.patch.dict(
            os.environ,
            {
                **self.fixture.environment(),
                "NEXUS_V2_PRE_RESET_PRODUCTION_CONFIRMATION": "",
            },
            clear=False,
        ):
            with self.assertRaisesRegex(stage.FenceError, "requires PRIVATE_ALPHA"):
                stage.validate_inputs(self.fixture.args())

    def test_site_release_drift_is_rejected(self) -> None:
        prior = json.loads(
            (self.fixture.phase1_root / "execute-evidence.json").read_text()
        )
        prior["siteReleaseVersion"] = "v9.9.9"
        write_json(self.fixture.phase1_root / "execute-evidence.json", prior)
        with self.assertRaisesRegex(stage.FenceError, "siteReleaseVersion mismatch"):
            self.validate()

    def test_runtime_manifest_must_match_replacement_lock(self) -> None:
        self.fixture.contract["stageInputs"][stage.STAGE][
            "runtimeBundleManifestSha256"
        ] = "1" * 64
        write_json(self.fixture.contract_path, self.fixture.contract)
        self.fixture.plan["workflow"]["contract"]["sha256"] = digest(
            self.fixture.contract_path
        )
        write_json(self.fixture.plan_path, self.fixture.plan)
        self.fixture.write_prior_stage()
        with self.assertRaisesRegex(stage.FenceError, "runtime bundle manifest hash"):
            self.validate()

    def test_site_coordinator_driver_substitution_is_rejected(self) -> None:
        self.fixture.contract["stageInputs"][stage.STAGE]["siteDriverPath"] = (
            self.fixture.site_paths["siteDeployPath"]
        )
        write_json(self.fixture.contract_path, self.fixture.contract)
        self.fixture.plan["workflow"]["contract"]["sha256"] = digest(
            self.fixture.contract_path
        )
        write_json(self.fixture.plan_path, self.fixture.plan)
        with self.assertRaisesRegex(
            stage.FenceError, "not the reviewed rollback component driver"
        ):
            self.validate()

    def test_zero_inventory_rejects_lifetime_writes(self) -> None:
        inventory = self.fixture.root / "inventory.json"
        write_json(inventory, {"counts": {"current": 0, "lifetime": 1}})
        with self.assertRaisesRegex(stage.FenceError, "current or lifetime write"):
            stage.validate_zero_inventory(inventory)


if __name__ == "__main__":
    unittest.main()
