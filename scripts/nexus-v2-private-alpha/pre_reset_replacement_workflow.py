#!/usr/bin/env python3
"""Execute the one hash-pinned private-alpha replacement workflow.

The driver owns the closed stage order.  Each stage helper receives the same
immutable plan, workflow contract, and live automatic-restore arm, plus a
private stage directory.  Helpers cannot reorder stages or replace the
acceptance-start fence.  The final helper must create the exact zero-asset
receipt path named by the supervisor plan; this driver never performs a
bootstrap/acceptance write.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import re
import stat
import subprocess
import sys
from pathlib import Path
from typing import Any, Mapping, Sequence

import deployment_secret_environment  # noqa: F401


STAGES = (
    "createPreResetClosure",
    "deployChainMediaAuthority",
    "deploySiteIndexer",
    "closeIngressAndObserve",
    "createZeroAssetAcceptanceFence",
)
MUTATING_STAGES = {
    "deployChainMediaAuthority",
    "deploySiteIndexer",
    "closeIngressAndObserve",
}
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
ID_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
SITE_RELEASE_RE = re.compile(
    r"^v(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
    r"(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$"
)
CONTRACT_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "releaseId",
    "siteReleaseVersion",
    "sourceCommit",
    "frozenFinalizedBlock",
    "artifactSha256",
    "toolPins",
    "stageOrder",
    "stageInputs",
    "fixtureOnly",
    "acceptanceStartFencePath",
    "bootstrapOrAcceptanceWritesAllowed",
    "paidOrPublicActivationAllowed",
}
TOOL_ROLES = {
    "preResetClosure",
    "chainDeployAll",
    "siteDeploy",
    "phase1IngressClosure",
    "acceptanceBoundary",
    "postCutoverCoordinator",
}
STAGE_RESULT_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "releaseId",
    "siteReleaseVersion",
    "sourceCommit",
    "planSha256",
    "workflowContractSha256",
    "stage",
    "result",
    "fixtureOnly",
    "mutationPerformed",
    "acceptanceStartFenceWritten",
    "checks",
    "completedAtUtc",
}
WORKFLOW_RESULT_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "releaseId",
    "siteReleaseVersion",
    "planSha256",
    "result",
    "fixtureOnly",
    "mutationPerformed",
    "acceptanceStartFenceWritten",
    "completedAtUtc",
}


class WorkflowError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise WorkflowError(message)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def ensure_sha256(value: Any, label: str) -> str:
    require(
        isinstance(value, str) and SHA256_RE.fullmatch(value) is not None,
        f"invalid {label}",
    )
    return value


def read_json(path: Path, label: str) -> dict[str, Any]:
    require(path.is_file() and not path.is_symlink(), f"{label} is unavailable")
    try:
        payload = path.read_bytes()
        value = json.loads(payload)
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise WorkflowError(f"invalid {label}") from exc
    require(isinstance(value, dict), f"{label} must be an object")
    require(
        payload == (json.dumps(value, indent=2, sort_keys=True) + "\n").encode(),
        f"{label} must be canonical JSON",
    )
    return value


def write_new(path: Path, value: Mapping[str, Any], mode: int = 0o400) -> None:
    require(not os.path.lexists(path), f"refusing to overwrite {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, mode)
    with os.fdopen(descriptor, "wb") as handle:
        handle.write((json.dumps(value, indent=2, sort_keys=True) + "\n").encode())
        handle.flush()
        os.fsync(handle.fileno())
    os.chmod(path, mode)


def immutable_root(source_id: str) -> Path:
    raw = os.environ.get(
        f"NEXUS_V2_PRE_RESET_IMMUTABLE_{source_id.upper()}_ROOT", ""
    )
    require(raw, f"immutable {source_id} source is unavailable")
    root = Path(raw)
    require(root.is_absolute() and root.is_dir() and not root.is_symlink(), f"invalid immutable {source_id} root")
    return root.resolve()


def resolve_helper(pin: Any, roots: Mapping[str, Path], stage: str) -> Path:
    require(
        isinstance(pin, dict) and set(pin) == {"sourceId", "path", "sha256"},
        f"{stage} helper pin mismatch",
    )
    source_id = pin.get("sourceId")
    relative = pin.get("path")
    require(source_id in roots, f"{stage} helper source is unavailable")
    require(
        isinstance(relative, str)
        and relative
        and not relative.startswith("/")
        and ".." not in Path(relative).parts,
        f"invalid {stage} helper path",
    )
    helper = (roots[source_id] / relative).resolve()
    require(roots[source_id] in helper.parents, f"{stage} helper escapes source")
    require(
        helper.is_file()
        and not helper.is_symlink()
        and helper.stat().st_mode & stat.S_IXUSR,
        f"{stage} helper is not executable",
    )
    require(
        sha256_file(helper) == ensure_sha256(pin.get("sha256"), f"{stage} helper SHA-256"),
        f"{stage} helper hash drifted",
    )
    return helper


def validate_inputs(args: argparse.Namespace) -> dict[str, Any]:
    plan_path = Path(args.plan).resolve()
    plan_sha = ensure_sha256(args.plan_sha256, "plan SHA-256")
    require(sha256_file(plan_path) == plan_sha, "plan hash mismatch")
    plan = read_json(plan_path, "supervisor plan")
    require(
        plan.get("schemaVersion") == 1
        and plan.get("kind")
        == "nexus-v2-private-alpha-pre-reset-rollback-supervisor-plan",
        "supervisor plan kind mismatch",
    )
    operation_id = plan.get("operationId")
    release_id = plan.get("releaseId")
    site_release = plan.get("siteReleaseVersion")
    source_commit = plan.get("sourceCommit")
    require(isinstance(operation_id, str) and ID_RE.fullmatch(operation_id), "invalid operation ID")
    require(isinstance(release_id, str) and ID_RE.fullmatch(release_id), "invalid chain release ID")
    require(isinstance(site_release, str) and SITE_RELEASE_RE.fullmatch(site_release), "invalid site release version")
    require(isinstance(source_commit, str) and COMMIT_RE.fullmatch(source_commit), "invalid source commit")
    contract_path = Path(args.workflow_contract).resolve()
    contract_sha = ensure_sha256(
        args.workflow_contract_sha256, "workflow contract SHA-256"
    )
    require(sha256_file(contract_path) == contract_sha, "workflow contract hash mismatch")
    workflow = plan.get("workflow")
    require(isinstance(workflow, dict), "workflow plan is unavailable")
    require(
        workflow.get("contract")
        == {"path": str(contract_path), "sha256": contract_sha},
        "workflow contract is not plan-pinned",
    )
    contract = read_json(contract_path, "replacement workflow contract")
    require(set(contract) == CONTRACT_KEYS, "workflow contract schema mismatch")
    require(
        contract.get("schemaVersion") == 1
        and contract.get("kind")
        == "nexus-v2-private-alpha-replacement-workflow-contract",
        "workflow contract kind mismatch",
    )
    for field, expected in (
        ("operationId", operation_id),
        ("releaseId", release_id),
        ("siteReleaseVersion", site_release),
        ("sourceCommit", source_commit),
        ("frozenFinalizedBlock", plan.get("frozenFinalizedBlock")),
    ):
        require(contract.get(field) == expected, f"workflow contract {field} mismatch")
    require(contract.get("stageOrder") == list(STAGES), "workflow stage order mismatch")
    require(
        isinstance(contract.get("stageInputs"), dict)
        and set(contract["stageInputs"]) == set(STAGES)
        and all(isinstance(value, dict) for value in contract["stageInputs"].values()),
        "workflow stage input set mismatch",
    )
    fixture_only = plan.get("backend") == "fixture-nondeployable"
    require(contract.get("fixtureOnly") is fixture_only, "workflow fixture mode mismatch")
    acceptance = plan.get("acceptanceStartFence")
    require(isinstance(acceptance, dict), "acceptance-start fence plan is absent")
    require(
        contract.get("acceptanceStartFencePath") == acceptance.get("handoffPath"),
        "workflow acceptance-start fence path mismatch",
    )
    require(
        contract.get("bootstrapOrAcceptanceWritesAllowed") is False
        and contract.get("paidOrPublicActivationAllowed") is False,
        "workflow contract authorizes forbidden writes",
    )
    artifact_hashes = contract.get("artifactSha256")
    artifacts = plan.get("artifacts")
    require(isinstance(artifact_hashes, dict) and isinstance(artifacts, dict), "workflow artifact bindings are unavailable")
    require(
        artifact_hashes == {
            name: value.get("sha256") for name, value in sorted(artifacts.items())
        },
        "workflow artifact hashes do not match the supervisor plan",
    )
    sources = plan.get("sources")
    require(isinstance(sources, dict) and set(sources) == {"chain", "media", "site"}, "source set mismatch")
    roots = {source_id: immutable_root(source_id) for source_id in sources}
    helper_pins = workflow.get("helperPins")
    require(isinstance(helper_pins, dict) and set(helper_pins) == set(STAGES), "workflow helper set mismatch")
    helpers = {
        stage: resolve_helper(helper_pins[stage], roots, stage) for stage in STAGES
    }
    tool_pins = contract.get("toolPins")
    require(
        isinstance(tool_pins, dict) and set(tool_pins) == TOOL_ROLES,
        "workflow nested tool set mismatch",
    )
    tools = {
        role: resolve_helper(tool_pins[role], roots, f"nested-{role}")
        for role in TOOL_ROLES
    }
    arm_path = Path(args.automatic_restore_arm).resolve()
    arm_sha = ensure_sha256(args.automatic_restore_arm_sha256, "arm SHA-256")
    require(sha256_file(arm_path) == arm_sha, "automatic-restore arm hash mismatch")
    return {
        "planPath": plan_path,
        "planSha256": plan_sha,
        "contractPath": contract_path,
        "contractSha256": contract_sha,
        "armPath": arm_path,
        "armSha256": arm_sha,
        "operationId": operation_id,
        "releaseId": release_id,
        "siteReleaseVersion": site_release,
        "sourceCommit": source_commit,
        "fixtureOnly": fixture_only,
        "acceptancePath": Path(contract["acceptanceStartFencePath"]),
        "helpers": helpers,
        "tools": tools,
    }


def validate_stage_result(
    value: Mapping[str, Any], context: Mapping[str, Any], stage: str
) -> None:
    require(set(value) == STAGE_RESULT_KEYS, f"{stage} result schema mismatch")
    require(
        value.get("schemaVersion") == 1
        and value.get("kind")
        == "nexus-v2-private-alpha-replacement-workflow-stage-result",
        f"{stage} result kind mismatch",
    )
    for field, expected in (
        ("operationId", context["operationId"]),
        ("releaseId", context["releaseId"]),
        ("siteReleaseVersion", context["siteReleaseVersion"]),
        ("sourceCommit", context["sourceCommit"]),
        ("planSha256", context["planSha256"]),
        ("workflowContractSha256", context["contractSha256"]),
        ("stage", stage),
        ("fixtureOnly", context["fixtureOnly"]),
    ):
        require(value.get(field) == expected, f"{stage} result {field} mismatch")
    require(value.get("result") == "passed", f"{stage} did not pass")
    require(
        value.get("mutationPerformed") is (stage in MUTATING_STAGES),
        f"{stage} mutation flag mismatch",
    )
    require(
        value.get("acceptanceStartFenceWritten")
        is (stage == "createZeroAssetAcceptanceFence"),
        f"{stage} acceptance-start flag mismatch",
    )
    checks = value.get("checks")
    require(isinstance(checks, dict) and checks and all(item is True for item in checks.values()), f"{stage} has a failed check")
    completed = value.get("completedAtUtc")
    require(isinstance(completed, str) and completed.endswith("Z"), f"{stage} completion time is invalid")


def invoke_stage(
    context: Mapping[str, Any], root: Path, stage: str
) -> Path:
    stage_root = root / "stages" / stage
    require(not os.path.lexists(stage_root), f"{stage} state already exists")
    stage_root.mkdir(parents=True, mode=0o700)
    result = stage_root / "result.json"
    log = stage_root / "helper.log"
    command = [
        str(context["helpers"][stage]),
        "--plan",
        str(context["planPath"]),
        "--plan-sha256",
        context["planSha256"],
        "--workflow-contract",
        str(context["contractPath"]),
        "--workflow-contract-sha256",
        context["contractSha256"],
        "--automatic-restore-arm",
        str(context["armPath"]),
        "--automatic-restore-arm-sha256",
        context["armSha256"],
        "--stage",
        stage,
        "--workflow-state-root",
        str(root),
        "--stage-state-root",
        str(stage_root),
        "--result",
        str(result),
    ]
    with log.open("xb") as handle:
        os.chmod(log, 0o600)
        completed = subprocess.run(
            command,
            stdout=handle,
            stderr=subprocess.STDOUT,
            env=os.environ.copy(),
            check=False,
        )
    require(completed.returncode == 0, f"{stage} helper failed; see {log}")
    validate_stage_result(read_json(result, f"{stage} result"), context, stage)
    return result


def run(args: argparse.Namespace) -> None:
    output = Path(args.result).resolve()
    require(not os.path.lexists(output), "refusing to overwrite workflow result")
    root = output.parent
    require(root.is_dir() and not root.is_symlink(), "workflow state root is unavailable")
    context = validate_inputs(args)
    results: list[Path] = []
    for stage in STAGES:
        results.append(invoke_stage(context, root, stage))
    acceptance = context["acceptancePath"]
    require(
        acceptance.is_file() and not acceptance.is_symlink(),
        "zero-asset acceptance-start fence was not created",
    )
    # The supervisor independently invokes the canonical receipt verifier and
    # retires its lease.  This driver only proves the closed helper sequence.
    value = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-replacement-workflow-result",
        "operationId": context["operationId"],
        "releaseId": context["releaseId"],
        "siteReleaseVersion": context["siteReleaseVersion"],
        "planSha256": context["planSha256"],
        "result": "passed",
        "fixtureOnly": context["fixtureOnly"],
        "mutationPerformed": True,
        "acceptanceStartFenceWritten": True,
        "completedAtUtc": dt.datetime.now(dt.timezone.utc)
        .replace(microsecond=0)
        .strftime("%Y-%m-%dT%H:%M:%SZ"),
    }
    require(set(value) == WORKFLOW_RESULT_KEYS, "internal workflow result drifted")
    write_new(output, value)


def parser() -> argparse.ArgumentParser:
    value = argparse.ArgumentParser(description=__doc__)
    value.add_argument("--plan", required=True)
    value.add_argument("--plan-sha256", required=True)
    value.add_argument("--workflow-contract", required=True)
    value.add_argument("--workflow-contract-sha256", required=True)
    value.add_argument("--automatic-restore-arm", required=True)
    value.add_argument("--automatic-restore-arm-sha256", required=True)
    value.add_argument("--result", required=True)
    return value


def main(argv: Sequence[str] | None = None) -> int:
    try:
        run(parser().parse_args(argv))
    except (WorkflowError, OSError, subprocess.SubprocessError) as exc:
        print(f"pre_reset_replacement_workflow: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
