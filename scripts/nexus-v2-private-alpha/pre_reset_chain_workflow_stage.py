#!/usr/bin/env python3
"""Create the pre-reset closure or deploy chain/media/authority in Phase 1."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Any, Mapping, Sequence

import deployment_secret_environment  # noqa: F401


STAGES = {"createPreResetClosure", "deployChainMediaAuthority"}
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
SITE_RELEASE_RE = re.compile(
    r"^v(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
    r"(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$"
)
DEPLOY_INPUT_KEYS = {
    "nodeCandidatePath",
    "nodeCandidateSha256",
    "nodeTargetIdentityPath",
    "nodeTargetIdentitySha256",
    "mediaCandidatePath",
    "mediaCandidateSha256",
}
RESULT_KEYS = {
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


class StageError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise StageError(message)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def ensure_sha256(value: Any, label: str) -> str:
    require(isinstance(value, str) and SHA256_RE.fullmatch(value), f"invalid {label}")
    return value


def read_json(path: Path, label: str) -> dict[str, Any]:
    require(path.is_file() and not path.is_symlink(), f"{label} is unavailable")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise StageError(f"invalid {label}") from exc
    require(isinstance(value, dict), f"{label} must be an object")
    return value


def pinned_file(path_value: Any, digest_value: Any, label: str) -> Path:
    require(isinstance(path_value, str), f"invalid {label} path")
    path = Path(path_value).resolve()
    require(path.is_file() and not path.is_symlink(), f"{label} is unavailable")
    digest = ensure_sha256(digest_value, f"{label} SHA-256")
    require(sha256_file(path) == digest, f"{label} hash drifted")
    return path


def resolve_tool(pin: Any, label: str) -> Path:
    require(
        isinstance(pin, dict) and set(pin) == {"sourceId", "path", "sha256"},
        f"{label} pin mismatch",
    )
    source_id = pin.get("sourceId")
    require(source_id in {"chain", "media", "site"}, f"invalid {label} source")
    root_value = os.environ.get(
        f"NEXUS_V2_PRE_RESET_IMMUTABLE_{source_id.upper()}_ROOT", ""
    )
    root = Path(root_value)
    require(root.is_absolute() and root.is_dir() and not root.is_symlink(), f"{label} immutable root is unavailable")
    root = root.resolve()
    relative = pin.get("path")
    require(
        isinstance(relative, str)
        and relative
        and not relative.startswith("/")
        and ".." not in Path(relative).parts,
        f"invalid {label} path",
    )
    path = (root / relative).resolve()
    require(root in path.parents and path.is_file() and not path.is_symlink(), f"{label} is unavailable")
    require(sha256_file(path) == ensure_sha256(pin.get("sha256"), f"{label} SHA-256"), f"{label} hash drifted")
    return path


def validate(args: argparse.Namespace) -> dict[str, Any]:
    require(args.stage in STAGES, "unsupported chain workflow stage")
    plan_path = Path(args.plan).resolve()
    plan_sha = ensure_sha256(args.plan_sha256, "plan SHA-256")
    require(sha256_file(plan_path) == plan_sha, "plan hash mismatch")
    plan = read_json(plan_path, "supervisor plan")
    contract_path = Path(args.workflow_contract).resolve()
    contract_sha = ensure_sha256(
        args.workflow_contract_sha256, "workflow contract SHA-256"
    )
    require(sha256_file(contract_path) == contract_sha, "workflow contract hash mismatch")
    contract = read_json(contract_path, "workflow contract")
    operation_id = plan.get("operationId")
    release_id = plan.get("releaseId")
    site_release = plan.get("siteReleaseVersion")
    source_commit = plan.get("sourceCommit")
    require(contract.get("operationId") == operation_id, "workflow operation mismatch")
    require(contract.get("releaseId") == release_id, "workflow chain release mismatch")
    require(contract.get("siteReleaseVersion") == site_release, "workflow site release mismatch")
    require(contract.get("sourceCommit") == source_commit, "workflow source mismatch")
    require(isinstance(source_commit, str) and COMMIT_RE.fullmatch(source_commit), "invalid source commit")
    require(isinstance(site_release, str) and SITE_RELEASE_RE.fullmatch(site_release), "invalid site release version")
    require(
        os.environ.get("NEXUS_V2_PRE_RESET_CHAIN_RELEASE_ID") == release_id
        and os.environ.get("NEXUS_V2_PRE_RESET_SITE_RELEASE_VERSION") == site_release
        and os.environ.get("NEXUS_V2_PRE_RESET_SOURCE_COMMIT") == source_commit,
        "stage environment identity mismatch",
    )
    arm = pinned_file(
        args.automatic_restore_arm,
        args.automatic_restore_arm_sha256,
        "automatic-restore arm",
    )
    raw_arm = read_json(arm, "automatic-restore arm")
    require(
        raw_arm.get("releaseId") == release_id
        and raw_arm.get("siteReleaseVersion") == site_release
        and raw_arm.get("sourceCommit") == source_commit
        and raw_arm.get("planSha256") == plan_sha
        and raw_arm.get("automaticRestoreArmed") is True,
        "automatic-restore arm identity mismatch",
    )
    state_root = Path(args.workflow_state_root).resolve()
    stage_root = Path(args.stage_state_root).resolve()
    require(
        state_root.is_dir()
        and stage_root.is_dir()
        and not state_root.is_symlink()
        and not stage_root.is_symlink()
        and state_root in stage_root.parents,
        "workflow stage roots are invalid",
    )
    tool_pins = contract.get("toolPins")
    require(isinstance(tool_pins, dict), "workflow tool pins are unavailable")
    tool_role = (
        "preResetClosure"
        if args.stage == "createPreResetClosure"
        else "chainDeployAll"
    )
    tool = resolve_tool(tool_pins.get(tool_role), tool_role)
    inputs = contract.get("stageInputs", {}).get(args.stage)
    require(isinstance(inputs, dict), "stage inputs are unavailable")
    return {
        "plan": plan,
        "planPath": plan_path,
        "planSha256": plan_sha,
        "contractSha256": contract_sha,
        "operationId": operation_id,
        "releaseId": release_id,
        "siteReleaseVersion": site_release,
        "sourceCommit": source_commit,
        "fixtureOnly": contract.get("fixtureOnly"),
        "tool": tool,
        "inputs": inputs,
        "stageRoot": stage_root,
        "workflowRoot": state_root,
        "armPath": arm,
        "armSha256": args.automatic_restore_arm_sha256,
    }


def run_command(
    command: list[str],
    log: Path,
    *,
    environment: Mapping[str, str] | None = None,
) -> None:
    require(not os.path.lexists(log), f"refusing to overwrite {log}")
    child_environment = os.environ.copy()
    if environment is not None:
        child_environment.update(environment)
    with log.open("xb") as handle:
        os.chmod(log, 0o600)
        completed = subprocess.run(
            command,
            env=child_environment,
            stdout=handle,
            stderr=subprocess.STDOUT,
            check=False,
        )
    require(completed.returncode == 0, f"stage command failed; see {log}")


def closure_path(context: Mapping[str, Any]) -> Path:
    return context["workflowRoot"] / "stages" / "createPreResetClosure" / "pre-reset-closure.json"


def create_closure(context: Mapping[str, Any]) -> dict[str, bool]:
    require(context["inputs"] == {}, "closure stage accepts no extra inputs")
    plan = context["plan"]
    artifacts = plan["artifacts"]
    output = context["stageRoot"] / "pre-reset-closure.json"
    command = [
        str(context["tool"]),
        "create",
        "--plan",
        artifacts["finalFreezePlan"]["path"],
        "--expected-plan-sha256",
        artifacts["finalFreezePlan"]["sha256"],
        "--bundle-root",
        plan["bundleRoot"],
        "--state-root",
        str(context["stageRoot"] / "closure-state"),
    ]
    options = (
        ("replacement-lock", "replacementLock"),
        ("reset-readiness", "resetReadiness"),
        ("final-freeze-evidence", "finalFreezeEvidence"),
        ("backup-manifest", "backupManifest"),
        ("restore-evidence", "restoreEvidence"),
        ("migration-evidence", "migrationEvidence"),
    )
    for flag, artifact in options:
        command.extend(
            [
                f"--{flag}",
                artifacts[artifact]["path"],
                f"--expected-{flag}-sha256",
                artifacts[artifact]["sha256"],
            ]
        )
    command.extend(
        [
            "--automatic-restore-arm",
            str(context["armPath"]),
            "--expected-automatic-restore-arm-sha256",
            context["armSha256"],
            "--selected-deployment-environment",
            plan["selectedDeploymentEnvironment"],
            "--selected-site-deployment-environment",
            plan["selectedSiteDeploymentEnvironment"],
            "--output",
            str(output),
        ]
    )
    run_command(command, context["stageRoot"] / "pre-reset-closure.log")
    require(output.is_file() and not output.is_symlink(), "pre-reset closure was not created")
    return {
        "armFreshAndLive": True,
        "artifactsBound": True,
        "finalFreezeReverified": True,
        "noMutation": True,
        "preResetClosureCreated": True,
    }


def validate_deploy_inputs(value: Mapping[str, Any]) -> dict[str, Path]:
    require(set(value) == DEPLOY_INPUT_KEYS, "chain deploy input schema mismatch")
    resolved: dict[str, Path] = {}
    for prefix in ("nodeCandidate", "nodeTargetIdentity", "mediaCandidate"):
        resolved[prefix] = pinned_file(
            value[f"{prefix}Path"], value[f"{prefix}Sha256"], prefix
        )
    return resolved


def deploy_chain(context: Mapping[str, Any]) -> dict[str, bool]:
    inputs = validate_deploy_inputs(context["inputs"])
    closure = closure_path(context)
    require(closure.is_file() and not closure.is_symlink(), "pre-reset closure stage output is absent")
    closure_sha = sha256_file(closure)
    stage = context["stageRoot"]
    base = [
        str(context["tool"]),
        "--fresh",
        "--phase1-closed",
        "--fresh-reset-readiness",
        context["plan"]["artifacts"]["resetReadiness"]["path"],
        "--pre-reset-closure-handoff",
        str(closure),
        "--pre-reset-closure-handoff-sha256",
        closure_sha,
        "--promote-node-candidate",
        str(inputs["nodeCandidate"]),
        "--node-target-identity",
        str(inputs["nodeTargetIdentity"]),
        "--node-evidence",
        str(stage / "node-evidence.json"),
        "--promote-media-candidate",
        str(inputs["mediaCandidate"]),
        "--media-evidence",
        str(stage / "media-evidence.json"),
    ]
    if context["fixtureOnly"]:
        marker = stage / "NONDEPLOYABLE.fixture"
        marker.write_text("no protected host contacted\n", encoding="utf-8")
        os.chmod(marker, 0o400)
    else:
        require(
            os.environ.get("NEXUS_V2_PRE_RESET_PRODUCTION_CONFIRMATION")
            == "PRIVATE_ALPHA_ROLLBACK_ONLY",
            "chain Phase1 deploy lacks PRIVATE_ALPHA_ROLLBACK_ONLY confirmation",
        )
        deployment_environment = context["plan"].get(
            "selectedDeploymentEnvironment"
        )
        require(
            isinstance(deployment_environment, str)
            and Path(deployment_environment).is_absolute(),
            "selected chain deployment environment is invalid",
        )
        child_environment = {
            "ALPHA_MACMINI2010_ENV_FILE": deployment_environment,
        }
        run_command(
            base + ["--dry-run"],
            stage / "deploy-preflight.log",
            environment=child_environment,
        )
        run_command(
            base,
            stage / "deploy-execute.log",
            environment=child_environment,
        )
        for evidence_name in ("node-evidence.json", "media-evidence.json"):
            evidence = stage / evidence_name
            require(
                evidence.is_file() and not evidence.is_symlink(),
                f"chain Phase1 deploy omitted {evidence_name}",
            )
    return {
        "chainReleaseIdValidated": True,
        "closedIngressLaunchRequested": True,
        "deploymentCandidatesPinned": True,
        "paidOrPublicActivationDisabled": True,
        "preResetClosureConsumed": True,
    }


def write_result(path: Path, value: Mapping[str, Any]) -> None:
    require(not os.path.lexists(path), "refusing to overwrite stage result")
    require(set(value) == RESULT_KEYS, "internal stage result schema mismatch")
    payload = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o400)
    with os.fdopen(descriptor, "wb") as handle:
        handle.write(payload)
        handle.flush()
        os.fsync(handle.fileno())


def run(args: argparse.Namespace) -> None:
    output = Path(args.result).resolve()
    require(not os.path.lexists(output), "refusing to overwrite stage result")
    context = validate(args)
    checks = (
        create_closure(context)
        if args.stage == "createPreResetClosure"
        else deploy_chain(context)
    )
    value = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-replacement-workflow-stage-result",
        "operationId": context["operationId"],
        "releaseId": context["releaseId"],
        "siteReleaseVersion": context["siteReleaseVersion"],
        "sourceCommit": context["sourceCommit"],
        "planSha256": context["planSha256"],
        "workflowContractSha256": context["contractSha256"],
        "stage": args.stage,
        "result": "passed",
        "fixtureOnly": context["fixtureOnly"],
        "mutationPerformed": args.stage == "deployChainMediaAuthority",
        "acceptanceStartFenceWritten": False,
        "checks": checks,
        "completedAtUtc": dt.datetime.now(dt.timezone.utc)
        .replace(microsecond=0)
        .strftime("%Y-%m-%dT%H:%M:%SZ"),
    }
    write_result(output, value)


def parser() -> argparse.ArgumentParser:
    value = argparse.ArgumentParser(description=__doc__)
    for name in (
        "plan",
        "plan-sha256",
        "workflow-contract",
        "workflow-contract-sha256",
        "automatic-restore-arm",
        "automatic-restore-arm-sha256",
        "stage",
        "workflow-state-root",
        "stage-state-root",
        "result",
    ):
        value.add_argument(f"--{name}", required=True)
    return value


def main(argv: Sequence[str] | None = None) -> int:
    try:
        run(parser().parse_args(argv))
    except (StageError, OSError, subprocess.SubprocessError) as exc:
        print(f"pre_reset_chain_workflow_stage: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
