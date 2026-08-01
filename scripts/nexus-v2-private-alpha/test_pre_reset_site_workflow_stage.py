#!/usr/bin/env python3
"""Offline subprocess tests for the pre-reset site Phase-1 stage helper."""

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


HERE = Path(__file__).resolve().parent
DRIVER = HERE / "pre_reset_site_workflow_stage.py"
sys.path.insert(0, str(HERE))
import pre_reset_site_workflow_stage as stage_tool  # noqa: E402

WORKSPACE = HERE.parents[4]
CADDY_TEMPLATE = (
    WORKSPACE
    / ".worktrees/nexus-v2-wave-c-20260730/web/tcg/deploy/alpha/macmini2014/nexus-v2-phase1-readonly.Caddyfile"
)
OPERATION_ID = "replace-20260801"
RELEASE_ID = "nexus-v2-alpha"
SITE_RELEASE = "v0.1.0-alpha.1"
GENESIS_HASH = "0x" + "9" * 64
FROZEN_BLOCK = {"number": 4242, "hash": "0x" + "8" * 64}


def canonical(value: Mapping[str, Any]) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")


def write_json(path: Path, value: Mapping[str, Any], mode: int = 0o600) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(canonical(value))
    os.chmod(path, mode)


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def pin_file(path: Path) -> dict[str, str]:
    return {"path": str(path.resolve()), "sha256": sha(path)}


DUMMY = r'''#!/usr/bin/env python3
raise SystemExit("unexpected fixture tool invocation")
'''


SITE_DEPLOY = r'''#!/usr/bin/env python3
import datetime, hashlib, json, os, sys
from pathlib import Path

def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def canonical(value):
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()

mode = "dry-run" if "--dry-run" in sys.argv[1:] else "execute"
with open(os.environ["TEST_SITE_STAGE_TRACE"], "a", encoding="utf-8") as handle:
    handle.write(json.dumps({
        "argv": sys.argv[1:],
        "chainEnv": os.environ.get("ALPHA_MACMINI2010_ENV_FILE"),
        "localOnly": os.environ.get("NEXUS_V2_LOCAL_ONLY_RELEASE"),
        "siteEnv": os.environ.get("ALPHA_MACMINI2014_ENV_FILE"),
        "tool": "siteDeploy",
    }, sort_keys=True) + "\n")
if os.environ.get("TEST_FAIL_SITE_DEPLOY") == mode:
    raise SystemExit(44)
args = sys.argv[1:]
identity_output = args[args.index("--post-deploy-identity-output") + 1]
if mode == "execute":
    candidate_path = args[args.index("--promote-candidate") + 1]
    readiness_path = args[args.index("--fresh-reset-readiness") + 1]
    closure_path = args[args.index("--pre-reset-closure-handoff") + 1]
    caddy_path = args[args.index("--phase1-caddyfile") + 1]
    with open(candidate_path, encoding="utf-8") as handle:
        candidate = json.load(handle)
    with open(closure_path, encoding="utf-8") as handle:
        closure = json.load(handle)
    compose_path = Path(__file__).resolve().parents[4] / "tcg/deploy/alpha/macmini2014/docker-compose.yaml"
    def service(marker, reference, image_id, publications):
        return {
            "containerId": hashlib.sha256(marker.encode()).hexdigest(),
            "imageReference": reference,
            "imageId": image_id,
            "publications": publications,
        }
    value = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-site-post-deploy-identity",
        "releaseId": closure["releaseId"],
        "siteReleaseVersion": candidate["releaseVersion"],
        "sourceCommit": closure["sourceCommit"],
        "siteSourceCommit": candidate["candidateSourceCommit"],
        "readinessSha256": digest(readiness_path),
        "preResetClosureHandoffSha256": digest(closure_path),
        "automaticRestoreArmSha256": closure["automaticRestoreArmSha256"],
        "compose": {
            "path": "/opt/eterra-alpha/site/current/deploy/alpha/macmini2014/docker-compose.yaml",
            "projectName": "eterra-tcg-site-alpha",
            "sha256": digest(compose_path),
        },
        "phase1": {
            "ingressMode": "AllExternalWriteIngressClosed",
            "caddyfileSha256": digest(caddy_path),
            "publicActionSubmission": False,
        },
        "services": {
            "caddy": service("caddy", (
                "caddy:2.10.2-alpine@sha256:"
                "4c6e91c6ed0e2fa03efd5b44747b625f"
                "ec79bc9cd06ac5235a779726618e530d"
            ), (
                "sha256:4c6e91c6ed0e2fa03efd5b44747b625f"
                "ec79bc9cd06ac5235a779726618e530d"
            ), [
                {"containerPort": 80, "protocol": "tcp", "hostIp": "0.0.0.0", "hostPort": 80},
                {"containerPort": 443, "protocol": "tcp", "hostIp": "0.0.0.0", "hostPort": 443},
            ]),
            "indexer-api": service("indexer", candidate["indexerImageRef"], candidate["indexerImageId"], [
                {"containerPort": 8787, "protocol": "tcp", "hostIp": "127.0.0.1", "hostPort": 8787},
            ]),
            "mongo": service("mongo", "mongo:7", "sha256:" + "6" * 64, []),
            "site": service("site", candidate["siteImageRef"], candidate["siteImageId"], [
                {"containerPort": 3000, "protocol": "tcp", "hostIp": "127.0.0.1", "hostPort": 3000},
            ]),
        },
        "authorityStatus": {"available": False, "baseUrl": None, "fps": None, "eterraLegends": None},
        "safety": {
            "phase1Closed": True,
            "paidOrPublicActivationAuthorized": False,
            "publicActionSubmissionEnabled": False,
            "economicFeaturesEnabled": False,
        },
        "capturedAtUtc": datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).strftime("%Y-%m-%dT%H:%M:%SZ"),
    }
    fd = os.open(identity_output, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o400)
    with os.fdopen(fd, "wb") as handle:
        handle.write(canonical(value))
'''


PHASE1_TOOL = r'''#!/usr/bin/env python3
import argparse, datetime, hashlib, json, os, sys

def digest(path):
    return hashlib.sha256(open(path, "rb").read()).hexdigest()

def canonical(value):
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()

def write_new(path, value):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o400)
    with os.fdopen(fd, "wb") as handle:
        handle.write(canonical(value))

p = argparse.ArgumentParser()
mode = p.add_mutually_exclusive_group(required=True)
mode.add_argument("--dry-run", action="store_true")
mode.add_argument("--execute", action="store_true")
p.add_argument("--output-root", required=True)
p.add_argument("--inputs-file", required=True)
p.add_argument("--expected-inputs-sha256", required=True)
p.add_argument("--execute-token")
p.add_argument("--expected-execute-token-sha256")
a = p.parse_args()
selected_mode = "dry-run" if a.dry_run else "execute"
with open(os.environ["TEST_SITE_STAGE_TRACE"], "a", encoding="utf-8") as handle:
    handle.write(json.dumps({
        "argv": sys.argv[1:],
        "chainEnv": os.environ.get("ALPHA_MACMINI2010_ENV_FILE"),
        "localOnly": os.environ.get("NEXUS_V2_LOCAL_ONLY_RELEASE"),
        "siteEnv": os.environ.get("ALPHA_MACMINI2014_ENV_FILE"),
        "tool": "phase1IngressClosure",
    }, sort_keys=True) + "\n")
if os.environ.get("TEST_FAIL_PHASE1") == selected_mode:
    raise SystemExit(45)
with open(a.inputs_file, encoding="utf-8") as handle:
    inputs = json.load(handle)
with open(inputs["preResetClosureHandoff"]["path"], encoding="utf-8") as handle:
    closure = json.load(handle)
tools = inputs["tools"]
pins = {
    "inputsSha256": a.expected_inputs_sha256,
    "driverSha256": digest(__file__),
    "chainCandidateSha256": inputs["chainCandidate"]["sha256"],
    "siteCandidateSha256": inputs["siteCandidate"]["sha256"],
    "targetIdentitySha256": inputs["targetIdentity"]["sha256"],
    "preResetClosureHandoffSha256": inputs["preResetClosureHandoff"]["sha256"],
    "automaticRestoreArmSha256": closure["automaticRestoreArmSha256"],
    "automaticRestoreArmPath": closure["automaticRestoreArmPath"],
    "chainEnvironmentSha256": inputs["chainEnvironment"]["sha256"],
    "siteEnvironmentSha256": inputs["siteEnvironment"]["sha256"],
    "chainLibrarySha256": tools["chainDeploymentLibrary"]["sha256"],
    "siteLibrarySha256": tools["siteDeploymentLibrary"]["sha256"],
    "chainRemoteScriptSha256": tools["chainRemoteAction"]["sha256"],
    "siteRemoteScriptSha256": tools["siteRemoteAction"]["sha256"],
    "readOnlyCaddyfileSha256": tools["readOnlyCaddyfile"]["sha256"],
    "acceptanceBoundaryToolSha256": tools["acceptanceBoundary"]["sha256"],
    "nodeCandidateToolSha256": tools["nodeCandidateTool"]["sha256"],
    "runtimeBundleManifestSha256": inputs["runtimeBundle"]["manifestSha256"],
}
if a.dry_run:
    os.mkdir(a.output_root, 0o700)
    value = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-phase1-ingress-closure-dry-run",
        "operationId": inputs["operationId"],
        "releaseId": inputs["releaseId"],
        "sourceCommit": inputs["sourceCommit"],
        "genesisHash": inputs["genesisHash"],
        "driverSha256": pins["driverSha256"],
        "inputsSha256": a.expected_inputs_sha256,
        "siteSourceCommit": inputs["siteSourceCommit"],
        "siteReleaseVersion": inputs["siteReleaseVersion"],
        "siteCandidateUsableForExecute": True,
        "pins": pins,
        "preResetClosureHandoffSha256": inputs["preResetClosureHandoff"]["sha256"],
        "automaticRestoreArmSha256": closure["automaticRestoreArmSha256"],
        "automaticRestoreArmPath": closure["automaticRestoreArmPath"],
        "stabilityWindowSeconds": inputs["stabilityWindowSeconds"],
        "plannedActions": {"fixture": ["validated"]},
        "exactClosureObservationCount": 2,
        "protectedExecuteTokenRequired": True,
        "remoteConnectionsAttempted": False,
        "liveMutationPerformed": False,
        "automaticEarlyFailureRollbackHandoffPreserved": True,
        "automaticReopenAuthorized": False,
        "paidOrPublicActivationAuthorized": False,
        "completedAtUtc": datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).strftime("%Y-%m-%dT%H:%M:%SZ"),
    }
    write_new(os.path.join(a.output_root, "dry-run-evidence.json"), value)
else:
    with open(a.execute_token, encoding="utf-8") as handle:
        token = json.load(handle)
    if digest(a.execute_token) != a.expected_execute_token_sha256 or token["pins"] != pins:
        raise SystemExit(46)
    os.mkdir(a.output_root, 0o700)
    ingress_path = os.path.join(a.output_root, "ingress-closed-evidence.json")
    write_new(ingress_path, {"schemaVersion": 1, "kind": "fixture-ingress", "closed": True})
    write_new(os.path.join(a.output_root, "acceptance-boundary-rpc-capture.json"), {"fixture": True})
    write_new(os.path.join(a.output_root, "post-v16-economic-gates.json"), {"fixture": True})
    write_new(os.path.join(a.output_root, "post-v16-acceptance-inventory.json"), {"counts": {"current": 0, "lifetime": 0}})
    value = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-phase1-ingress-closure-execute-evidence",
        "operationId": inputs["operationId"],
        "releaseId": inputs["releaseId"],
        "sourceCommit": inputs["sourceCommit"],
        "siteSourceCommit": inputs["siteSourceCommit"],
        "siteReleaseVersion": inputs["siteReleaseVersion"],
        "siteCandidateUsableForExecute": True,
        "genesisHash": inputs["genesisHash"],
        "inputsSha256": a.expected_inputs_sha256,
        "preResetClosureHandoffSha256": inputs["preResetClosureHandoff"]["sha256"],
        "automaticRestoreArmSha256": closure["automaticRestoreArmSha256"],
        "automaticRestoreArmPath": closure["automaticRestoreArmPath"],
        "executeTokenSha256": a.expected_execute_token_sha256,
        "observedAtFinalizedBlock": {"number": 4243, "hash": "0x" + "7" * 64},
        "ingressClosedEvidenceSha256": digest(ingress_path),
        "stabilityWindowSeconds": inputs["stabilityWindowSeconds"],
        "stabilityWindowElapsedMilliseconds": inputs["stabilityWindowSeconds"] * 1000,
        "allExternalWriteIngressClosed": True,
        "blockProductionContinues": True,
        "authorityLocalServicePreserved": True,
        "readOnlySiteStackPreserved": True,
        "automaticReopenAuthorized": False,
        "paidOrPublicActivationAuthorized": False,
    }
    write_new(os.path.join(a.output_root, "execute-evidence.json"), value)
'''


class SiteWorkflowStageTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="pre-reset-site-stage-")
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

    def repository(self, path: Path, files: Mapping[str, tuple[str | bytes, int]]) -> str:
        path.mkdir(parents=True, mode=0o700)
        for relative, (content, mode) in files.items():
            destination = path / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            if isinstance(content, bytes):
                destination.write_bytes(content)
            else:
                destination.write_text(content, encoding="utf-8")
            os.chmod(destination, mode)
        for command in (
            ["git", "init", "-q"],
            ["git", "config", "user.email", "fixture@example.invalid"],
            ["git", "config", "user.name", "Fixture"],
            ["git", "add", "."],
            ["git", "commit", "-q", "-m", "fixture"],
        ):
            completed = subprocess.run(command, cwd=path, capture_output=True, text=True)
            self.assertEqual(completed.returncode, 0, completed.stderr)
        return subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=path,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()

    def fixture(self, label: str, *, fixture_only: bool = False) -> dict[str, Any]:
        case = self.root / label
        chain = case / "immutable-chain"
        site = case / "immutable-site"
        media = case / "immutable-media"
        caddy = CADDY_TEMPLATE.read_bytes()
        chain_commit = self.repository(
            chain,
            {
                "scripts/nexus-v2-private-alpha/pre_reset_closure.py": (DUMMY, 0o700),
                "scripts/nexus-v2-private-alpha/acceptance_boundary.py": (DUMMY, 0o700),
                "scripts/nexus-v2-private-alpha/node_candidate.py": (DUMMY, 0o700),
                "deploy/alpha/macmini2010/deploy-all.sh": (DUMMY, 0o700),
                "deploy/alpha/macmini2010/nexus-v2-post-cutover-coordinator.py": (DUMMY, 0o700),
                "deploy/alpha/macmini2010/lib.sh": ("# fixture chain library\n", 0o600),
            },
        )
        site_commit = self.repository(
            site,
            {
                "tcg/deploy/alpha/macmini2014/deploy-site.sh": (SITE_DEPLOY, 0o700),
                "tcg/deploy/alpha/macmini2014/nexus_v2_phase1_ingress_closure.py": (PHASE1_TOOL, 0o700),
                "tcg/deploy/alpha/macmini2014/nexus-v2-phase1-chain-ingress-remote.sh": (DUMMY, 0o700),
                "tcg/deploy/alpha/macmini2014/nexus-v2-phase1-site-ingress-remote.sh": (DUMMY, 0o700),
                "tcg/deploy/alpha/macmini2014/nexus-v2-phase1-readonly.Caddyfile": (caddy, 0o600),
                "tcg/deploy/alpha/macmini2014/docker-compose.yaml": (
                    "services:\n  fixture: {}\n",
                    0o600,
                ),
                "tcg/deploy/alpha/macmini2014/lib.sh": ("# fixture site library\n", 0o600),
            },
        )
        media_commit = self.repository(media, {"README": ("fixture media\n", 0o600)})
        source_roots = {"chain": chain, "media": media, "site": site}
        commits = {"chain": chain_commit, "media": media_commit, "site": site_commit}
        roots: dict[str, Path] = {}
        for source_id, source_root in source_roots.items():
            clone = case / f"execution-{source_id}"
            completed = subprocess.run(
                ["git", "clone", "-q", "--no-hardlinks", str(source_root), str(clone)],
                capture_output=True,
                text=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            completed = subprocess.run(
                ["git", "checkout", "-q", "--detach", commits[source_id]],
                cwd=clone,
                capture_output=True,
                text=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            roots[source_id] = clone

        chain_env = case / "selected-chain.env"
        site_env = case / "selected-site.env"
        chain_env.write_text("ETERRA_RELEASE_VERSION=nexus-v2-alpha\n", encoding="utf-8")
        site_env.write_text("RELEASE_VERSION=v0.1.0-alpha.1\n", encoding="utf-8")
        os.chmod(chain_env, 0o600)
        os.chmod(site_env, 0o600)
        node_candidate = case / "node-candidate.json"
        target_identity = case / "target-identity.json"
        media_candidate = case / "media-candidate.json"
        site_candidate = case / "site-candidate.json"
        write_json(node_candidate, {"fixture": "node", "releaseId": RELEASE_ID})
        write_json(target_identity, {"genesisHash": GENESIS_HASH, "releaseId": RELEASE_ID})
        write_json(media_candidate, {"fixture": "media"})
        write_json(
            site_candidate,
            {
                "candidateSourceCommit": site_commit,
                "indexerImageId": "sha256:" + "2" * 64,
                "indexerImageRef": "fixture-indexer",
                "releaseVersion": SITE_RELEASE,
                "schemaVersion": 1,
                "siteBuildHash": "3" * 64,
                "siteImageId": "sha256:" + "4" * 64,
                "siteImageRef": "fixture-site",
            },
        )
        runtime_root = case / "runtime-bundle"
        runtime_root.mkdir(mode=0o700)
        runtime_manifest = runtime_root / "runtime-bundle-manifest.json"
        write_json(runtime_manifest, {"fixture": "runtime"})
        replacement_lock = case / "replacement-lock.json"
        write_json(
            replacement_lock,
            {
                "schemaVersion": 1,
                "kind": "nexus-v2-private-alpha-pre-cutover-replacement-lock",
                "releaseId": RELEASE_ID,
                "createdAtUtc": "2026-08-01T00:00:00Z",
                "repositories": {
                    "chain": {"root": str(chain), "head": chain_commit, "tree": "1" * 40},
                    "media": {"root": str(media), "head": media_commit, "tree": "2" * 40},
                    "web": {"root": str(site), "head": site_commit, "tree": "3" * 40},
                },
                "artifacts": {
                    "deploymentEnvironment": pin_file(chain_env),
                    "siteDeploymentEnvironment": pin_file(site_env),
                    "nodeCandidateManifest": pin_file(node_candidate),
                    "targetIdentity": pin_file(target_identity),
                    "runtimeBundleManifest": pin_file(runtime_manifest),
                },
                "policy": {
                    "liveHostContactAuthorized": False,
                    "paidOrPublicActivationAuthorized": False,
                    "staleEnvironmentSelectionAllowed": False,
                },
            },
        )
        artifacts: dict[str, dict[str, str]] = {"replacementLock": pin_file(replacement_lock)}
        for name in (
            "finalFreezePlan",
            "resetReadiness",
            "finalFreezeEvidence",
            "backupManifest",
            "restoreEvidence",
            "migrationEvidence",
        ):
            path = case / "artifacts" / f"{name}.json"
            write_json(path, {"releaseId": RELEASE_ID, "sourceCommit": chain_commit})
            artifacts[name] = pin_file(path)

        def tool_pin(role: str, source: str, relative: str) -> tuple[str, dict[str, str]]:
            path = roots[source] / relative
            return role, {"sourceId": source, "path": relative, "sha256": sha(path)}

        tool_pins = dict(
            [
                tool_pin("preResetClosure", "chain", "scripts/nexus-v2-private-alpha/pre_reset_closure.py"),
                tool_pin("chainDeployAll", "chain", "deploy/alpha/macmini2010/deploy-all.sh"),
                tool_pin("siteDeploy", "site", "tcg/deploy/alpha/macmini2014/deploy-site.sh"),
                tool_pin("phase1IngressClosure", "site", "tcg/deploy/alpha/macmini2014/nexus_v2_phase1_ingress_closure.py"),
                tool_pin("acceptanceBoundary", "chain", "scripts/nexus-v2-private-alpha/acceptance_boundary.py"),
                tool_pin("postCutoverCoordinator", "chain", "deploy/alpha/macmini2010/nexus-v2-post-cutover-coordinator.py"),
            ]
        )
        stage_inputs = {
            "createPreResetClosure": {},
            "deployChainMediaAuthority": {
                "nodeCandidatePath": str(node_candidate),
                "nodeCandidateSha256": sha(node_candidate),
                "nodeTargetIdentityPath": str(target_identity),
                "nodeTargetIdentitySha256": sha(target_identity),
                "mediaCandidatePath": str(media_candidate),
                "mediaCandidateSha256": sha(media_candidate),
            },
            "deploySiteIndexer": {
                "siteCandidatePath": str(site_candidate),
                "siteCandidateSha256": sha(site_candidate),
                "phase1CaddyfilePath": str(site / "tcg/deploy/alpha/macmini2014/nexus-v2-phase1-readonly.Caddyfile"),
                "phase1CaddyfileSha256": sha(site / "tcg/deploy/alpha/macmini2014/nexus-v2-phase1-readonly.Caddyfile"),
            },
            "closeIngressAndObserve": {
                "stabilityWindowSeconds": 30,
                "runtimeBundleRoot": str(runtime_root),
                "runtimeBundleManifestSha256": sha(runtime_manifest),
            },
            "createZeroAssetAcceptanceFence": {},
        }
        contract = {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-replacement-workflow-contract",
            "operationId": OPERATION_ID,
            "releaseId": RELEASE_ID,
            "siteReleaseVersion": SITE_RELEASE,
            "sourceCommit": chain_commit,
            "frozenFinalizedBlock": FROZEN_BLOCK,
            "artifactSha256": {name: value["sha256"] for name, value in sorted(artifacts.items())},
            "toolPins": tool_pins,
            "stageOrder": [
                "createPreResetClosure",
                "deployChainMediaAuthority",
                "deploySiteIndexer",
                "closeIngressAndObserve",
                "createZeroAssetAcceptanceFence",
            ],
            "stageInputs": stage_inputs,
            "fixtureOnly": fixture_only,
            "acceptanceStartFencePath": str(case / "acceptance-start.json"),
            "bootstrapOrAcceptanceWritesAllowed": False,
            "paidOrPublicActivationAllowed": False,
        }
        contract_path = case / "workflow-contract.json"
        write_json(contract_path, contract)
        backend = "fixture-nondeployable" if fixture_only else "protected-private-alpha"
        plan = {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-pre-reset-rollback-supervisor-plan",
            "operationId": OPERATION_ID,
            "releaseId": RELEASE_ID,
            "siteReleaseVersion": SITE_RELEASE,
            "sourceCommit": chain_commit,
            "backend": backend,
            "fixtureRoot": str(case / "offline.NONDEPLOYABLE") if fixture_only else None,
            "createdAtUtc": "2026-08-01T00:00:00Z",
            "expiresAtUtc": "2026-08-01T01:00:00Z",
            "frozenFinalizedBlock": FROZEN_BLOCK,
            "bundleRoot": str(case),
            "selectedDeploymentEnvironment": str(chain_env),
            "selectedSiteDeploymentEnvironment": str(site_env),
            "artifacts": artifacts,
            "sources": {
                source: {
                    "root": str(source_roots[source]),
                    "expectedCommit": commits[source],
                }
                for source in source_roots
            },
            "workflow": {
                "contract": {"path": str(contract_path), "sha256": sha(contract_path)}
            },
            "acceptanceStartFence": {"handoffPath": str(case / "acceptance-start.json")},
            "components": {},
            "policy": {},
        }
        plan_path = case / "supervisor-plan.json"
        write_json(plan_path, plan)
        arm = case / "automatic-restore-arm.json"
        arm_value = {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-pre-reset-automatic-restore-arm",
            "operationId": OPERATION_ID,
            "releaseId": RELEASE_ID,
            "siteReleaseVersion": SITE_RELEASE,
            "sourceCommit": chain_commit,
            "planSha256": sha(plan_path),
            "fixtureOnly": fixture_only,
            "automaticRestoreArmed": True,
            "paidOrPublicActivationAllowed": False,
            "replacementLockSha256": artifacts["replacementLock"]["sha256"],
            "resetReadinessSha256": artifacts["resetReadiness"]["sha256"],
            "finalFreezeEvidenceSha256": artifacts["finalFreezeEvidence"]["sha256"],
            "backupManifestSha256": artifacts["backupManifest"]["sha256"],
            "restoreEvidenceSha256": artifacts["restoreEvidence"]["sha256"],
            "migrationEvidenceSha256": artifacts["migrationEvidence"]["sha256"],
        }
        write_json(arm, arm_value, 0o600)
        workflow_root = case / "workflow-state"
        workflow_root.mkdir(mode=0o700)
        closure_root = workflow_root / "stages/createPreResetClosure"
        closure_root.mkdir(parents=True, mode=0o700)
        closure = closure_root / "pre-reset-closure.json"
        write_json(
            closure,
            {
                "schemaVersion": 1,
                "kind": "nexus-v2-private-alpha-pre-reset-closure-handoff",
                "releaseId": RELEASE_ID,
                "sourceCommit": chain_commit,
                "automaticRestoreArmPath": str(arm),
                "automaticRestoreArmSha256": sha(arm),
                "automaticRestoreArmed": True,
                "mutationPerformed": False,
                **{
                    field: arm_value[field]
                    for field in (
                        "replacementLockSha256",
                        "resetReadinessSha256",
                        "finalFreezeEvidenceSha256",
                        "backupManifestSha256",
                        "restoreEvidenceSha256",
                        "migrationEvidenceSha256",
                    )
                },
            },
            0o600,
        )
        self.prior_result(
            closure_root / "result.json",
            "createPreResetClosure",
            sha(plan_path),
            sha(contract_path),
            chain_commit,
            fixture_only,
            False,
        )
        chain_stage = workflow_root / "stages/deployChainMediaAuthority"
        chain_stage.mkdir(parents=True, mode=0o700)
        self.prior_result(
            chain_stage / "result.json",
            "deployChainMediaAuthority",
            sha(plan_path),
            sha(contract_path),
            chain_commit,
            fixture_only,
            True,
        )
        environment = os.environ.copy()
        environment.update(
            {
                "TEST_SITE_STAGE_TRACE": str(self.trace),
                "NEXUS_V2_PRE_RESET_IMMUTABLE_CHAIN_ROOT": str(roots["chain"]),
                "NEXUS_V2_PRE_RESET_IMMUTABLE_MEDIA_ROOT": str(roots["media"]),
                "NEXUS_V2_PRE_RESET_IMMUTABLE_SITE_ROOT": str(roots["site"]),
            }
        )
        if fixture_only:
            environment.pop("NEXUS_V2_PRE_RESET_PRODUCTION_CONFIRMATION", None)
        else:
            environment["NEXUS_V2_PRE_RESET_PRODUCTION_CONFIRMATION"] = "PRIVATE_ALPHA_ROLLBACK_ONLY"
        return {
            "case": case,
            "roots": roots,
            "commits": commits,
            "environment": environment,
            "plan": plan_path,
            "planSha256": sha(plan_path),
            "contract": contract_path,
            "contractSha256": sha(contract_path),
            "arm": arm,
            "armSha256": sha(arm),
            "closure": closure,
            "workflowRoot": workflow_root,
            "chainEnv": chain_env,
            "siteEnv": site_env,
            "siteCandidate": site_candidate,
            "caddy": roots["site"] / "tcg/deploy/alpha/macmini2014/nexus-v2-phase1-readonly.Caddyfile",
            "resetReadiness": Path(artifacts["resetReadiness"]["path"]),
            "runtimeRoot": runtime_root,
        }

    def prior_result(
        self,
        path: Path,
        stage: str,
        plan_sha: str,
        contract_sha: str,
        source_commit: str,
        fixture_only: bool,
        mutation: bool,
    ) -> None:
        write_json(
            path,
            {
                "schemaVersion": 1,
                "kind": "nexus-v2-private-alpha-replacement-workflow-stage-result",
                "operationId": OPERATION_ID,
                "releaseId": RELEASE_ID,
                "siteReleaseVersion": SITE_RELEASE,
                "sourceCommit": source_commit,
                "planSha256": plan_sha,
                "workflowContractSha256": contract_sha,
                "stage": stage,
                "result": "passed",
                "fixtureOnly": fixture_only,
                "mutationPerformed": mutation,
                "acceptanceStartFenceWritten": False,
                "checks": {"fixturePriorStage": True},
                "completedAtUtc": "2026-08-01T00:00:00Z",
            },
        )

    def arguments(self, fixture: Mapping[str, Any], stage: str) -> tuple[list[str], Path]:
        stage_root = fixture["workflowRoot"] / "stages" / stage
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
                str(fixture["workflowRoot"]),
                "--stage-state-root",
                str(stage_root),
                "--result",
                str(result),
            ],
            result,
        )

    def invoke(
        self,
        fixture: Mapping[str, Any],
        stage: str,
        environment: Mapping[str, str] | None = None,
    ) -> tuple[subprocess.CompletedProcess[str], Path]:
        arguments, result = self.arguments(fixture, stage)
        completed = subprocess.run(
            [sys.executable, str(DRIVER), *arguments],
            env=dict(environment or fixture["environment"]),
            capture_output=True,
            text=True,
            check=False,
        )
        return completed, result

    def trace_values(self) -> list[dict[str, Any]]:
        if not self.trace.exists():
            return []
        return [json.loads(line) for line in self.trace.read_text().splitlines()]

    def deploy_then_close(self, fixture: Mapping[str, Any]) -> tuple[Path, Path]:
        deployed, deploy_result = self.invoke(fixture, "deploySiteIndexer")
        self.assertEqual(deployed.returncode, 0, deployed.stderr)
        closed, close_result = self.invoke(fixture, "closeIngressAndObserve")
        self.assertEqual(closed.returncode, 0, closed.stderr)
        return deploy_result, close_result

    def test_production_deploy_and_close_use_exact_argv_inputs_and_token(self) -> None:
        fixture = self.fixture("production-happy")
        deploy_result, close_result = self.deploy_then_close(fixture)
        trace = self.trace_values()
        self.assertEqual([item["tool"] for item in trace], [
            "siteDeploy",
            "siteDeploy",
            "phase1IngressClosure",
            "phase1IngressClosure",
        ])
        deploy_base = [
            "--fresh",
            "--fresh-reset-readiness",
            str(fixture["resetReadiness"]),
            "--pre-reset-closure-handoff",
            str(fixture["closure"]),
            "--pre-reset-closure-handoff-sha256",
            sha(fixture["closure"]),
            "--promote-candidate",
            str(fixture["siteCandidate"]),
            "--phase1-closed",
            "--phase1-caddyfile",
            str(fixture["caddy"]),
            "--phase1-caddyfile-sha256",
            sha(fixture["caddy"]),
            "--post-deploy-identity-output",
            str(
                fixture["workflowRoot"]
                / "stages/deploySiteIndexer/site-post-deploy-identity.json"
            ),
        ]
        self.assertEqual(trace[0]["argv"], [*deploy_base, "--dry-run"])
        self.assertEqual(trace[1]["argv"], deploy_base)
        for item in trace:
            self.assertEqual(item["chainEnv"], str(fixture["chainEnv"]))
            self.assertEqual(item["siteEnv"], str(fixture["siteEnv"]))
            self.assertEqual(item["localOnly"], "1")

        close_root = fixture["workflowRoot"] / "stages/closeIngressAndObserve"
        inputs_path = close_root / "phase1-inputs.json"
        inputs_sha = sha(inputs_path)
        dry_root = close_root / "phase1-dry-run"
        output_root = close_root / "phase1-output"
        token_path = close_root / "phase1-execute-token.json"
        self.assertEqual(
            trace[2]["argv"],
            [
                "--dry-run",
                "--output-root",
                str(dry_root),
                "--inputs-file",
                str(inputs_path),
                "--expected-inputs-sha256",
                inputs_sha,
            ],
        )
        self.assertEqual(
            trace[3]["argv"],
            [
                "--execute",
                "--output-root",
                str(output_root),
                "--inputs-file",
                str(inputs_path),
                "--expected-inputs-sha256",
                inputs_sha,
                "--execute-token",
                str(token_path),
                "--expected-execute-token-sha256",
                sha(token_path),
            ],
        )
        inputs_payload = inputs_path.read_bytes()
        inputs = json.loads(inputs_payload)
        self.assertEqual(inputs_payload, canonical(inputs))
        self.assertEqual(inputs["schemaVersion"], 2)
        self.assertEqual(inputs["kind"], "nexus-v2-private-alpha-phase1-ingress-closure-inputs.v2")
        self.assertEqual(inputs["chainEnvironment"], pin_file(fixture["chainEnv"]))
        self.assertEqual(inputs["siteEnvironment"], pin_file(fixture["siteEnv"]))
        self.assertEqual(inputs["chainSource"]["commit"], fixture["commits"]["chain"])
        self.assertEqual(inputs["siteSource"]["commit"], fixture["commits"]["site"])
        token_payload = token_path.read_bytes()
        token = json.loads(token_payload)
        self.assertEqual(token_payload, canonical(token))
        self.assertEqual(stat.S_IMODE(token_path.stat().st_mode) & 0o077, 0)
        self.assertEqual(token["authorizations"], {
            "automaticReopen": False,
            "closePhase1ExternalWriteIngress": True,
            "paidOrPublicActivation": False,
            "preserveAuthorityLocalService": True,
            "preserveBlockProduction": True,
            "preserveReadOnlySiteStack": True,
            "publicActionSubmission": False,
            "sshLocalRpcObservation": True,
        })
        self.assertEqual(token["pins"], json.loads((dry_root / "dry-run-evidence.json").read_text())["pins"])
        for result in (deploy_result, close_result):
            payload = result.read_bytes()
            self.assertEqual(payload, canonical(json.loads(payload)))
        identity_path = (
            fixture["workflowRoot"]
            / "stages/deploySiteIndexer/site-post-deploy-identity.json"
        )
        identity_payload = identity_path.read_bytes()
        identity = json.loads(identity_payload)
        self.assertEqual(identity_payload, canonical(identity))
        self.assertEqual(stat.S_IMODE(identity_path.stat().st_mode), 0o400)
        self.assertEqual(
            identity["phase1"],
            {
                "ingressMode": "AllExternalWriteIngressClosed",
                "caddyfileSha256": sha(fixture["caddy"]),
                "publicActionSubmission": False,
            },
        )
        self.assertTrue(
            {
                "postDeployIdentityPinned",
                "phase1PostDeployIdentityVerified",
            }
            <= set(json.loads(deploy_result.read_text())["checks"])
        )

    def test_phase1_execute_failure_writes_no_stage_result(self) -> None:
        fixture = self.fixture("phase1-execute-failure")
        deployed, _ = self.invoke(fixture, "deploySiteIndexer")
        self.assertEqual(deployed.returncode, 0, deployed.stderr)
        environment = dict(fixture["environment"])
        environment["TEST_FAIL_PHASE1"] = "execute"
        closed, result = self.invoke(fixture, "closeIngressAndObserve", environment)
        self.assertEqual(closed.returncode, 2)
        self.assertIn("nested site tool failed", closed.stderr)
        self.assertFalse(result.exists())
        self.assertEqual(
            [item["tool"] for item in self.trace_values()],
            ["siteDeploy", "siteDeploy", "phase1IngressClosure", "phase1IngressClosure"],
        )

    def test_close_rejects_post_deploy_identity_drift_before_phase1(self) -> None:
        fixture = self.fixture("post-deploy-identity-drift")
        deployed, _ = self.invoke(fixture, "deploySiteIndexer")
        self.assertEqual(deployed.returncode, 0, deployed.stderr)
        identity = (
            fixture["workflowRoot"]
            / "stages/deploySiteIndexer/site-post-deploy-identity.json"
        )
        value = json.loads(identity.read_text())
        value["safety"]["economicFeaturesEnabled"] = True
        os.chmod(identity, 0o600)
        identity.write_bytes(canonical(value))
        os.chmod(identity, 0o400)
        closed, result = self.invoke(fixture, "closeIngressAndObserve")
        self.assertEqual(closed.returncode, 2)
        self.assertIn("site post-deploy identity safety mismatch", closed.stderr)
        self.assertFalse(result.exists())
        self.assertEqual(
            [item["tool"] for item in self.trace_values()],
            ["siteDeploy", "siteDeploy"],
        )

    def test_close_rejects_unsafe_available_authority_identity(self) -> None:
        fixture = self.fixture("post-deploy-unsafe-authority")
        deployed, _ = self.invoke(fixture, "deploySiteIndexer")
        self.assertEqual(deployed.returncode, 0, deployed.stderr)
        identity = (
            fixture["workflowRoot"]
            / "stages/deploySiteIndexer/site-post-deploy-identity.json"
        )
        value = json.loads(identity.read_text())
        value["authorityStatus"] = {
            "available": True,
            "baseUrl": "http://127.0.0.1:5016",
            "fps": {
                "path": "/v1/fps/status",
                "sourceDocumentSha256": "a" * 64,
                "facts": {**stage_tool.FPS_AUTHORITY_FACTS, "wagering": True},
            },
            "eterraLegends": {
                "path": "/v1/eterra-legends/status",
                "sourceDocumentSha256": "b" * 64,
                "facts": dict(stage_tool.LEGENDS_AUTHORITY_FACTS),
            },
        }
        os.chmod(identity, 0o600)
        identity.write_bytes(canonical(value))
        os.chmod(identity, 0o400)
        closed, result = self.invoke(fixture, "closeIngressAndObserve")
        self.assertEqual(closed.returncode, 2)
        self.assertIn("authority status facts are unsafe: fps", closed.stderr)
        self.assertFalse(result.exists())
        self.assertEqual(
            [item["tool"] for item in self.trace_values()],
            ["siteDeploy", "siteDeploy"],
        )

    def test_existing_result_rejects_before_validation_or_nested_tool(self) -> None:
        fixture = self.fixture("no-overwrite")
        arguments, result = self.arguments(fixture, "deploySiteIndexer")
        sentinel = canonical({"immutable": "prior result"})
        result.write_bytes(sentinel)
        completed = subprocess.run(
            [sys.executable, str(DRIVER), *arguments],
            env=fixture["environment"],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 2)
        self.assertIn("refusing to overwrite stage result", completed.stderr)
        self.assertEqual(result.read_bytes(), sentinel)
        self.assertEqual(self.trace_values(), [])

    def test_existing_post_deploy_identity_rejects_before_site_dry_run(self) -> None:
        fixture = self.fixture("post-deploy-no-overwrite")
        arguments, result = self.arguments(fixture, "deploySiteIndexer")
        identity = (
            fixture["workflowRoot"]
            / "stages/deploySiteIndexer/site-post-deploy-identity.json"
        )
        sentinel = canonical({"immutable": "prior identity"})
        identity.write_bytes(sentinel)
        completed = subprocess.run(
            [sys.executable, str(DRIVER), *arguments],
            env=fixture["environment"],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 2)
        self.assertIn("refusing to reuse site deployment output", completed.stderr)
        self.assertEqual(identity.read_bytes(), sentinel)
        self.assertFalse(result.exists())
        self.assertEqual(self.trace_values(), [])

    def test_production_confirmation_is_required_before_nested_tool(self) -> None:
        fixture = self.fixture("missing-confirmation")
        environment = dict(fixture["environment"])
        environment.pop("NEXUS_V2_PRE_RESET_PRODUCTION_CONFIRMATION", None)
        completed, result = self.invoke(
            fixture, "deploySiteIndexer", environment
        )
        self.assertEqual(completed.returncode, 2)
        self.assertIn("PRIVATE_ALPHA_ROLLBACK_ONLY confirmation", completed.stderr)
        self.assertFalse(result.exists())
        self.assertEqual(self.trace_values(), [])

    def test_nondeployable_fixture_contacts_no_tool_and_emits_minimal_phase1(self) -> None:
        fixture = self.fixture("fixture", fixture_only=True)
        deploy_result, close_result = self.deploy_then_close(fixture)
        self.assertEqual(self.trace_values(), [])
        marker = fixture["workflowRoot"] / "stages/deploySiteIndexer/NONDEPLOYABLE.fixture.json"
        marker_payload = marker.read_bytes()
        marker_value = json.loads(marker_payload)
        self.assertEqual(marker_payload, canonical(marker_value))
        self.assertTrue(marker_value["fixtureOnly"])
        self.assertFalse(marker_value["protectedHostContacted"])
        execute = fixture["workflowRoot"] / "stages/closeIngressAndObserve/phase1-output/execute-evidence.json"
        execute_payload = execute.read_bytes()
        execute_value = json.loads(execute_payload)
        self.assertEqual(execute_payload, canonical(execute_value))
        self.assertTrue(execute_value["fixtureOnly"])
        self.assertTrue(execute_value["allExternalWriteIngressClosed"])
        self.assertFalse(execute_value["paidOrPublicActivationAuthorized"])
        for result in (deploy_result, close_result):
            value = json.loads(result.read_text())
            self.assertTrue(value["fixtureOnly"])
            self.assertTrue(value["mutationPerformed"])


if __name__ == "__main__":
    unittest.main()
