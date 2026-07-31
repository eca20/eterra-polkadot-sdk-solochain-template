#!/usr/bin/env python3
"""Fail-closed cross-host final-freeze and backup coordinator.

Remote and service-specific work is delegated to SHA-256-pinned executables.
This coordinator never embeds credentials, SSH commands, service commands, or
database commands.  It supplies a closed action protocol, validates every
receipt, keeps a partial failure frozen, and assembles the exact backup manifest
consumed by ``alpha_v2_release.py``.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path
from typing import Any, Mapping, Sequence


sys.path.insert(0, str(Path(__file__).resolve().parent))
import alpha_v2_release as release  # noqa: E402


SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
TRANSACTION_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
ROLES = (
    "site-ingress",
    "site-indexer-mongo",
    "authority",
    "chain",
    "media-ipfs",
)
FREEZE_ORDER = ROLES
ACTIONS = ("preflight", "freeze", "verify-frozen", "snapshot", "verify-snapshot")
SOURCE_COMPONENTS = {"chain", "media", "sdkgen", "web"}
COMPONENT_ARTIFACTS: dict[str, set[tuple[str, str]]] = {
    "site-ingress": {
        ("ingress", "caddy-config"),
        ("ingress", "caddy-state"),
        ("service", "caddy-service"),
    },
    "site-indexer-mongo": {
        ("config", "indexer-env"),
        ("config", "site-env"),
        ("indexer", "indexer-checkpoint"),
        ("indexer", "indexer-state"),
        ("indexer", "mongo-state"),
        ("service", "indexer-service"),
        ("service", "site-service"),
        ("site", "site-image-lock"),
        ("site", "site-state"),
    },
    "authority": {
        ("authority", "authority-state"),
        ("config", "authority-env"),
        ("service", "authority-service"),
    },
    "chain": {
        ("config", "chain-spec"),
        ("config", "node-env"),
        ("node", "node-binary"),
        ("node", "node-data"),
        ("node", "runtime-v14-wasm"),
        ("node", "runtime-v14-metadata"),
        ("node", "runtime-v16-production-wasm"),
        ("node", "runtime-v16-try-runtime-wasm"),
        ("node", "tcg-storage-version-observation"),
        ("node", "try-runtime-snapshot"),
        ("service", "node-service"),
    },
    "media-ipfs": {
        ("config", "media-env"),
        ("ipfs", "ipfs-data"),
        ("ipfs", "ipfs-staging"),
        ("media", "media-image-lock"),
        ("media", "media-state"),
        ("service", "media-service"),
    },
}
COORDINATOR_ARTIFACTS = {
    ("config", "acceptance-inventory"),
    ("config", "deployment-fingerprints"),
    ("config", "economic-gates"),
    ("config", "release-identifiers"),
    ("config", "write-barrier-evidence"),
}
PREFLIGHT_CHECKS = {
    "credentialsAvailable",
    "driverPinned",
    "restoreInputsIdentified",
    "snapshotDestinationWritable",
    "targetResolved",
}
FREEZE_CHECKS = {
    "site-ingress": {
        "caddyStopped",
        "publicHttpIngressStopped",
        "publicRpcWriteIngressStopped",
    },
    "site-indexer-mongo": {"indexerStopped", "mongoWritesQuiescent", "siteStopped"},
    "authority": {"authorityStopped", "resultSubmissionStopped"},
    "chain": {
        "blockProductionStopped",
        "finalizedHeadCaptured",
        "nodeP2pStopped",
        "nodeRpcStopped",
        "nodeStopped",
    },
    "media-ipfs": {"ipfsStopped", "mediaStopped", "uploadIngressStopped"},
}
SNAPSHOT_CHECKS = {
    "artifactHashesComputed",
    "artifactRolesComplete",
    "consistentSnapshotCaptured",
    "privateBundlePermissionsRestricted",
}
VERIFY_SNAPSHOT_CHECKS = {
    "archivesReadable",
    "artifactHashesVerified",
    "noServiceResumed",
    "restoreContractReady",
}
STANDARD_FLAGS = {
    "--action",
    "--artifact",
    "--bundle-root",
    "--dry-run",
    "--frozen-block-hash",
    "--frozen-block-number",
    "--release-id",
    "--result",
    "--role",
    "--source-commit",
    "--target",
    "--transaction-id",
}
PLAN_KEYS = {
    "authorizations",
    "componentSourceCommits",
    "components",
    "kind",
    "releaseId",
    "preV16SourceRuntime",
    "schemaVersion",
    "sourceCommit",
    "stabilityWindowSeconds",
    "transactionId",
}
PRE_V16_SOURCE_KEYS = {
    "deployedSourceCommit",
    "flowPalletIndex",
    "metadataVersion",
    "specVersion",
    "tcgPalletIndex",
    "tcgStorageVersion",
}
COMPONENT_KEYS = {"arguments", "driver", "driverSha256", "target"}
RESULT_KEYS = {
    "action",
    "artifacts",
    "checks",
    "dryRun",
    "frozenAtUtc",
    "frozenFinalizedBlock",
    "kind",
    "liveMutationPerformed",
    "planned",
    "releaseId",
    "role",
    "schemaVersion",
    "sourceCommit",
    "target",
    "transactionId",
}


class FreezeError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise FreezeError(message)


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def parse_utc(value: Any, label: str) -> dt.datetime:
    require(isinstance(value, str) and value, f"{label} must be an ISO-8601 string")
    try:
        parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise FreezeError(f"invalid {label}") from exc
    require(parsed.tzinfo is not None, f"{label} must include a timezone")
    return parsed.astimezone(dt.timezone.utc)


def read_json(path: Path, label: str) -> dict[str, Any]:
    require(path.is_file() and not path.is_symlink(), f"{label} must be a regular file: {path}")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise FreezeError(f"invalid {label}: {path}") from exc
    require(isinstance(value, dict), f"{label} root must be an object")
    return value


def write_new_json(path: Path, value: Mapping[str, Any], mode: int = 0o600) -> None:
    require(not path.exists(), f"refusing to overwrite immutable output: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    encoded = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, mode)
    with os.fdopen(descriptor, "wb") as handle:
        handle.write(encoded)


def ensure_sha(value: Any, label: str) -> str:
    require(isinstance(value, str) and bool(SHA256_RE.fullmatch(value)), f"invalid {label}")
    return value


def ensure_commit(value: Any, label: str) -> str:
    require(isinstance(value, str) and bool(COMMIT_RE.fullmatch(value)), f"invalid {label}")
    return value


def sha256_file(path: Path) -> str:
    return release.sha256_file(path)


def closed_true_checks(value: Any, expected: set[str], label: str) -> None:
    require(isinstance(value, dict) and set(value) == expected, f"{label} checks do not match the closed set")
    for name, result in value.items():
        require(result is True, f"{label} check failed: {name}")


def contains_sensitive_key(value: Any) -> bool:
    if isinstance(value, dict):
        for key, nested in value.items():
            lowered = str(key).lower()
            if any(marker in lowered for marker in ("mnemonic", "password", "secret", "suri", "privatekey", "private_key", "token")):
                return True
            if contains_sensitive_key(nested):
                return True
    elif isinstance(value, list):
        return any(contains_sensitive_key(item) for item in value)
    return False


def regular_executable(path: Path, label: str) -> Path:
    require(path.is_absolute(), f"{label} path must be absolute")
    require(path.is_file() and not path.is_symlink(), f"{label} must be a regular file")
    require(bool(path.stat().st_mode & 0o100), f"{label} must be executable")
    return path.resolve()


def validate_plan(path: Path, expected_sha256: str) -> dict[str, Any]:
    path = path.resolve()
    expected_sha256 = ensure_sha(expected_sha256, "expected plan SHA-256")
    require(sha256_file(path) == expected_sha256, "final-freeze plan SHA-256 mismatch")
    value = read_json(path, "final-freeze plan")
    require(set(value) == PLAN_KEYS, "final-freeze plan does not match the closed schema")
    require(value.get("schemaVersion") == 1, "unsupported final-freeze plan schema")
    require(value.get("kind") == "nexus-v2-private-alpha-final-freeze-plan", "final-freeze plan kind mismatch")
    require(not contains_sensitive_key(value), "final-freeze plan may not contain credentials or secret fields")
    release_id = release.ensure_release_id(str(value.get("releaseId", "")))
    source_commit = ensure_commit(value.get("sourceCommit"), "plan source commit")
    transaction_id = value.get("transactionId")
    require(isinstance(transaction_id, str) and bool(TRANSACTION_RE.fullmatch(transaction_id)), "invalid final-freeze transaction ID")
    window = value.get("stabilityWindowSeconds")
    require(isinstance(window, int) and not isinstance(window, bool) and 30 <= window <= 300, "stability window must be in 30..300 seconds")
    source_commits = value.get("componentSourceCommits")
    require(isinstance(source_commits, dict) and set(source_commits) == SOURCE_COMPONENTS, "component source commits do not match the closed set")
    for component, commit in source_commits.items():
        ensure_commit(commit, f"{component} source commit")
    require(source_commits["chain"] == source_commit, "plan chain source commit mismatch")
    pre_v16 = value.get("preV16SourceRuntime")
    require(isinstance(pre_v16, dict) and set(pre_v16) == PRE_V16_SOURCE_KEYS, "pre-V16 source runtime contract mismatch")
    ensure_commit(pre_v16.get("deployedSourceCommit"), "deployed pre-V16 source commit")
    require(
        pre_v16
        == {
            "deployedSourceCommit": pre_v16["deployedSourceCommit"],
            "specVersion": 1,
            "metadataVersion": 14,
            "tcgPalletIndex": 9,
            "tcgStorageVersion": 14,
            "flowPalletIndex": 29,
        },
        "pre-V16 source runtime identity must be spec 1 / metadata 14 / TCG V14",
    )
    authorizations = value.get("authorizations")
    require(
        authorizations
        == {
            "automaticResumeOnFailure": False,
            "finalFreezeAndBackup": True,
            "freshReset": False,
            "paidOrPublicActivation": False,
            "privateAlphaOnly": True,
        },
        "final-freeze authorization block mismatch",
    )
    components = value.get("components")
    require(isinstance(components, dict) and set(components) == set(ROLES), "components must match the canonical closed role set")
    normalized: dict[str, Any] = {}
    for role in ROLES:
        component = components[role]
        require(isinstance(component, dict) and set(component) == COMPONENT_KEYS, f"{role} component contract mismatch")
        driver = regular_executable(Path(str(component.get("driver", ""))), f"{role} driver")
        driver_hash = ensure_sha(component.get("driverSha256"), f"{role} driver SHA-256")
        require(sha256_file(driver) == driver_hash, f"{role} driver SHA-256 mismatch")
        target = component.get("target")
        require(isinstance(target, str) and bool(TRANSACTION_RE.fullmatch(target)), f"invalid {role} target ID")
        arguments = component.get("arguments")
        require(isinstance(arguments, list), f"{role} arguments must be an array")
        for argument in arguments:
            require(isinstance(argument, str) and argument and "\x00" not in argument, f"invalid {role} driver argument")
            lowered_argument = argument.lower()
            require(
                not any(
                    marker in lowered_argument
                    for marker in (
                        "mnemonic",
                        "password",
                        "private-key",
                        "private_key",
                        "secret",
                        "seed-phrase",
                        "seed_phrase",
                        "suri",
                    )
                ),
                f"{role} driver arguments may not carry secret material",
            )
            require(
                not any(argument == flag or argument.startswith(f"{flag}=") for flag in STANDARD_FLAGS),
                f"{role} arguments may not override standardized flag {argument}",
            )
        normalized[role] = {
            "driver": driver,
            "driverSha256": driver_hash,
            "target": target,
            "arguments": arguments,
        }
    expected_roles = {
        (group, name)
        for group, names in release.REQUIRED_ARTIFACTS.items()
        for name in names
    }
    mapped_roles = set().union(*COMPONENT_ARTIFACTS.values(), COORDINATOR_ARTIFACTS)
    require(mapped_roles == expected_roles, "final-freeze artifact ownership does not cover the backup closed set")
    return {
        "path": path,
        "sha256": expected_sha256,
        "value": value,
        "releaseId": release_id,
        "sourceCommit": source_commit,
        "transactionId": transaction_id,
        "stabilityWindowSeconds": window,
        "components": normalized,
        "preV16SourceRuntime": pre_v16,
    }


def expected_checks(role: str, action: str) -> set[str]:
    if action == "preflight":
        return PREFLIGHT_CHECKS
    if action == "freeze":
        return FREEZE_CHECKS[role]
    if action == "verify-frozen":
        return FREEZE_CHECKS[role] | {"remainsStopped"}
    if action == "snapshot":
        return SNAPSHOT_CHECKS
    require(action == "verify-snapshot", f"unknown component action: {action}")
    return VERIFY_SNAPSHOT_CHECKS


def finalized_block(value: Any, label: str) -> dict[str, Any]:
    require(isinstance(value, dict) and set(value) == {"hash", "number"}, f"{label} must be a finalized block")
    number = value.get("number")
    block_hash = value.get("hash")
    require(isinstance(number, int) and not isinstance(number, bool) and number >= 0, f"invalid {label} number")
    require(isinstance(block_hash, str) and bool(release.HEX_256_RE.fullmatch(block_hash)), f"invalid {label} hash")
    return {"number": number, "hash": block_hash.lower()}


def validate_artifacts(value: Any, role: str, bundle_root: Path, required: bool) -> list[dict[str, Any]]:
    require(isinstance(value, list), f"{role} artifacts must be an array")
    if not required:
        require(not value, f"{role} dry-run/non-snapshot result may not report artifacts")
        return []
    mapped: dict[tuple[str, str], dict[str, Any]] = {}
    for entry in value:
        require(isinstance(entry, dict) and set(entry) == {"bytes", "group", "name", "path", "sha256"}, f"{role} artifact receipt schema mismatch")
        group = entry.get("group")
        name = entry.get("name")
        require(isinstance(group, str) and isinstance(name, str), f"{role} artifact role is invalid")
        key = (group, name)
        require(key not in mapped, f"duplicate {role} artifact role: {group}:{name}")
        relative = entry.get("path")
        require(isinstance(relative, str) and relative, f"{role} artifact path is missing")
        artifact = release.resolve_bundle_file(bundle_root, relative, f"{role} {group}:{name}")
        size = entry.get("bytes")
        require(isinstance(size, int) and not isinstance(size, bool) and size >= 0, f"invalid {role} artifact byte count")
        require(artifact.stat().st_size == size, f"{role} artifact byte count mismatch: {group}:{name}")
        digest = ensure_sha(entry.get("sha256"), f"{role} artifact SHA-256")
        require(sha256_file(artifact) == digest, f"{role} artifact SHA-256 mismatch: {group}:{name}")
        mapped[key] = dict(entry)
    require(set(mapped) == COMPONENT_ARTIFACTS[role], f"{role} artifact roles do not match the closed component set")
    return [mapped[key] for key in sorted(mapped)]


def validate_result(
    value: Mapping[str, Any],
    plan: Mapping[str, Any],
    role: str,
    action: str,
    dry_run: bool,
    bundle_root: Path,
    frozen: Mapping[str, Any] | None,
) -> dict[str, Any]:
    require(set(value) == RESULT_KEYS, f"{role} {action} result does not match the closed schema")
    require(value.get("schemaVersion") == 1, f"{role} result schema mismatch")
    require(value.get("kind") == "nexus-v2-private-alpha-final-freeze-component-result", f"{role} result kind mismatch")
    require(value.get("transactionId") == plan["transactionId"], f"{role} transaction mismatch")
    require(value.get("releaseId") == plan["releaseId"], f"{role} release mismatch")
    require(value.get("sourceCommit") == plan["sourceCommit"], f"{role} source commit mismatch")
    require(value.get("role") == role and value.get("action") == action, f"{role} action identity mismatch")
    require(value.get("target") == plan["components"][role]["target"], f"{role} target mismatch")
    require(value.get("dryRun") is dry_run, f"{role} dry-run mode mismatch")
    require(value.get("planned") is dry_run, f"{role} planned flag mismatch")
    if dry_run:
        require(value.get("liveMutationPerformed") is False, f"{role} dry-run reported a live mutation")
        require(value.get("frozenAtUtc") is None, f"{role} dry-run reported a freeze time")
        require(value.get("frozenFinalizedBlock") is None, f"{role} dry-run reported a frozen block")
    else:
        require(isinstance(value.get("liveMutationPerformed"), bool), f"{role} mutation flag must be boolean")
        if action == "preflight":
            require(value.get("liveMutationPerformed") is False, f"{role} preflight performed a live mutation")
            require(value.get("frozenAtUtc") is None, f"{role} preflight reported a freeze time")
            require(value.get("frozenFinalizedBlock") is None, f"{role} preflight reported a frozen block")
        else:
            parse_utc(value.get("frozenAtUtc"), f"{role} frozenAtUtc")
            if action == "freeze" and role != "chain":
                require(value.get("frozenFinalizedBlock") is None, f"{role} freeze guessed the chain finalized block")
            else:
                result_block = finalized_block(value.get("frozenFinalizedBlock"), f"{role} frozen block")
                if frozen is not None:
                    require(result_block == frozen, f"{role} frozen block differs from the stopped chain")
    closed_true_checks(value.get("checks"), expected_checks(role, action), f"{role} {action}")
    artifacts = validate_artifacts(
        value.get("artifacts"),
        role,
        bundle_root,
        required=not dry_run and action in {"snapshot", "verify-snapshot"},
    )
    result = dict(value)
    result["artifacts"] = artifacts
    return result


def invoke_driver(
    plan: Mapping[str, Any],
    role: str,
    action: str,
    dry_run: bool,
    bundle_root: Path,
    state_root: Path,
    frozen: Mapping[str, Any] | None,
) -> tuple[dict[str, Any], Path, Path]:
    component = plan["components"][role]
    driver: Path = component["driver"]
    require(sha256_file(driver) == component["driverSha256"], f"{role} driver changed before {action}")
    result_path = state_root / role / f"{action}.json"
    log_path = state_root / role / f"{action}.log"
    require(not result_path.exists() and not log_path.exists(), f"{role} {action} receipt already exists")
    result_path.parent.mkdir(parents=True, exist_ok=True)
    command = [
        str(driver),
        *component["arguments"],
        "--action",
        action,
        "--transaction-id",
        plan["transactionId"],
        "--release-id",
        plan["releaseId"],
        "--source-commit",
        plan["sourceCommit"],
        "--role",
        role,
        "--target",
        component["target"],
        "--bundle-root",
        str(bundle_root),
        "--result",
        str(result_path),
    ]
    for group, name in sorted(COMPONENT_ARTIFACTS[role]):
        command.extend(["--artifact", f"{group}:{name}"])
    if frozen is not None:
        command.extend(["--frozen-block-number", str(frozen["number"])])
        command.extend(["--frozen-block-hash", str(frozen["hash"])])
    if dry_run:
        command.append("--dry-run")
    completed = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
    log_path.write_bytes(completed.stdout)
    require(completed.returncode == 0, f"{role} {action} driver failed; keep every stopped component frozen; see {log_path}")
    result = read_json(result_path, f"{role} {action} result")
    return validate_result(result, plan, role, action, dry_run, bundle_root, frozen), result_path, log_path


def coordinator_artifacts(
    plan: Mapping[str, Any],
    bundle_root: Path,
    receipts: Mapping[str, Mapping[str, Mapping[str, Any]]],
    receipt_paths: Mapping[str, Mapping[str, Path]],
    frozen: Mapping[str, Any],
) -> list[dict[str, Any]]:
    config_root = bundle_root / "artifacts/config"
    config_root.mkdir(parents=True, exist_ok=True)
    fingerprints_path = config_root / "deployment-fingerprints.json"
    identifiers_path = config_root / "release-identifiers.json"
    barrier_path = config_root / "write-barrier-evidence.json"
    gates_path = config_root / "economic-gates.json"
    inventory_path = config_root / "acceptance-inventory.json"
    fingerprints = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-deployment-fingerprints",
        "transactionId": plan["transactionId"],
        "planSha256": plan["sha256"],
        "componentSourceCommits": plan["value"]["componentSourceCommits"],
        "drivers": {
            role: {
                "sha256": plan["components"][role]["driverSha256"],
                "target": plan["components"][role]["target"],
            }
            for role in ROLES
        },
        "receipts": {
            role: {
                action: sha256_file(receipt_paths[role][action])
                for action in ACTIONS
            }
            for role in ROLES
        },
    }
    identifiers = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-release-identifiers",
        "transactionId": plan["transactionId"],
        "releaseId": plan["releaseId"],
        "sourceCommit": plan["sourceCommit"],
        "componentSourceCommits": plan["value"]["componentSourceCommits"],
        "frozenFinalizedBlock": frozen,
        "paidOrPublicActivationAllowed": False,
    }
    stopped_at = max(
        parse_utc(receipts[role]["freeze"]["frozenAtUtc"], f"{role} freeze time")
        for role in ROLES
    ).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    barrier = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-all-ingress-write-barrier",
        "transactionId": plan["transactionId"],
        "releaseId": plan["releaseId"],
        "sourceCommit": plan["sourceCommit"],
        "frozenFinalizedBlock": frozen,
        "stoppedAtUtc": stopped_at,
        "stabilityWindowSeconds": plan["stabilityWindowSeconds"],
        "roles": {
            role: {
                "freezeReceiptSha256": sha256_file(receipt_paths[role]["freeze"]),
                "verifyFrozenReceiptSha256": sha256_file(receipt_paths[role]["verify-frozen"]),
                "checks": receipts[role]["verify-frozen"]["checks"],
            }
            for role in ROLES
        },
        "allIngressAndMutatingServicesStopped": True,
        "automaticResumeAttempted": False,
    }
    write_new_json(fingerprints_path, fingerprints, mode=0o600)
    write_new_json(identifiers_path, identifiers, mode=0o600)
    write_new_json(barrier_path, barrier, mode=0o600)
    chain_artifacts = {
        (entry["group"], entry["name"]): entry
        for entry in receipts["chain"]["verify-snapshot"]["artifacts"]
    }
    runtime_v14_hash = chain_artifacts[("node", "runtime-v14-wasm")]["sha256"]
    metadata_v14_hash = chain_artifacts[("node", "runtime-v14-metadata")]["sha256"]
    tcg_observation_hash = chain_artifacts[("node", "tcg-storage-version-observation")]["sha256"]
    gates = {
        "schemaVersion": 1,
        "kind": release.PRE_V16_FRESH_RESET_GATE_KIND,
        "releaseId": plan["releaseId"],
        "sourceCommit": plan["sourceCommit"],
        "observedAtFinalizedBlock": frozen,
        "operationScope": {
            "freshGenesisReplacementOnly": True,
            "inPlaceRuntimeUpgradeAllowed": False,
            "v2ActivationAllowed": False,
            "paidOrPublicActivationAllowed": False,
        },
        "sourceRuntime": {
            **plan["preV16SourceRuntime"],
            "runtimeV14WasmSha256": runtime_v14_hash,
            "runtimeMetadataScaleSha256": metadata_v14_hash,
            "tcgStorageVersionObservationSha256": tcg_observation_hash,
        },
        "v2StructuralAbsence": {
            "absentPallets": release.PRE_V16_ABSENT_V2_PALLETS,
            "absentPalletIndices": release.PRE_V16_ABSENT_V2_PALLET_INDICES,
            "tcgV2StoragePresent": False,
            "tcgV2DispatchablesPresent": False,
            "v2EventsPresent": False,
        },
        "knownLegacyEconomicSurfaces": {
            "tcgPaidMintDispatchablesPresent": True,
            "tcgMarketplaceDispatchablesPresent": True,
            "faucetDispatchablePresent": True,
            "economyDispatchablesPresent": True,
            "arcadePayContinueDispatchablePresent": True,
            "reachableThroughWriteIngress": False,
        },
        "legacyWriteBarrier": {
            "mode": "AllIngressStopped",
            "nodeServiceStopped": True,
            "authorityServiceStopped": True,
            "publicRpcWriteIngressStopped": True,
            "p2pIngressStopped": True,
            "blockProductionStopped": True,
            "offlineFinalizedHeadMatchesGateBlock": True,
            "inventoryCapturedAfterWriteStop": True,
            "stoppedAtUtc": stopped_at,
            "stabilityWindowSeconds": plan["stabilityWindowSeconds"],
            "writeBarrierEvidenceSha256": sha256_file(barrier_path),
        },
        "externalReviewFlags": {
            "cryptographyApproved": False,
            "paidFeaturesApproved": False,
            "publicProductionApproved": False,
        },
        "additionalEconomicFlags": {
            "legacyFaucetIngressReachable": False,
            "legacyStorefrontIngressReachable": False,
        },
    }
    inventory = {
        "schemaVersion": 1,
        "kind": "nexus-v2-acceptance-inventory",
        "releaseId": plan["releaseId"],
        "sourceCommit": plan["sourceCommit"],
        "observedAtFinalizedBlock": frozen,
        "counts": {name: 0 for name in sorted(release.ACCEPTANCE_COUNT_FIELDS)},
    }
    write_new_json(gates_path, gates, mode=0o600)
    write_new_json(inventory_path, inventory, mode=0o600)
    release.validate_pre_v16_fresh_reset_gates(
        gates_path,
        plan["releaseId"],
        plan["sourceCommit"],
    )
    release.validate_acceptance_inventory(
        inventory_path,
        plan["releaseId"],
        plan["sourceCommit"],
    )
    output = []
    for group, name, path in (
        ("config", "acceptance-inventory", inventory_path),
        ("config", "deployment-fingerprints", fingerprints_path),
        ("config", "economic-gates", gates_path),
        ("config", "release-identifiers", identifiers_path),
        ("config", "write-barrier-evidence", barrier_path),
    ):
        output.append(
            {
                "group": group,
                "name": name,
                "path": path.relative_to(bundle_root).as_posix(),
                "bytes": path.stat().st_size,
                "sha256": sha256_file(path),
            }
        )
    return output


def prepare_roots(bundle_root: Path, state_root: Path) -> None:
    for root, label in ((bundle_root, "bundle root"), (state_root, "state root")):
        require(not root.exists(), f"refusing to reuse final-freeze {label}: {root}")
        root.mkdir(parents=True, mode=0o700)


def run_plan(args: argparse.Namespace, dry_run: bool) -> None:
    plan = validate_plan(Path(args.plan), args.expected_plan_sha256)
    bundle_root = Path(args.bundle_root).resolve()
    state_root = Path(args.state_root).resolve()
    evidence = Path(args.evidence).resolve()
    require(not evidence.exists(), f"refusing to overwrite final-freeze evidence: {evidence}")
    prepare_roots(bundle_root, state_root)
    receipts: dict[str, dict[str, dict[str, Any]]] = {role: {} for role in ROLES}
    receipt_paths: dict[str, dict[str, Path]] = {role: {} for role in ROLES}
    log_paths: dict[str, dict[str, Path]] = {role: {} for role in ROLES}
    frozen: dict[str, Any] | None = None
    frozen_times: list[dt.datetime] = []

    try:
        for action in ACTIONS:
            for role in FREEZE_ORDER:
                if not dry_run and action == "verify-frozen" and role == FREEZE_ORDER[0]:
                    require(frozen is not None, "chain freeze did not establish a finalized block")
                    stable_at = max(frozen_times) + dt.timedelta(seconds=plan["stabilityWindowSeconds"])
                    remaining = (stable_at - dt.datetime.now(dt.timezone.utc)).total_seconds()
                    if remaining > 0:
                        time.sleep(remaining)
                supplied_block = frozen if action in {"verify-frozen", "snapshot", "verify-snapshot"} else None
                result, result_path, log_path = invoke_driver(
                    plan,
                    role,
                    action,
                    dry_run,
                    bundle_root,
                    state_root,
                    supplied_block,
                )
                receipts[role][action] = result
                receipt_paths[role][action] = result_path
                log_paths[role][action] = log_path
                if not dry_run and action == "verify-snapshot":
                    require(
                        result["artifacts"] == receipts[role]["snapshot"]["artifacts"],
                        f"{role} verified snapshot receipt differs from captured snapshot",
                    )
                if not dry_run and action == "freeze":
                    frozen_times.append(parse_utc(result["frozenAtUtc"], f"{role} frozenAtUtc"))
                    if role == "chain":
                        frozen = finalized_block(result["frozenFinalizedBlock"], "chain frozen block")

        if dry_run:
            dry_evidence = {
                "schemaVersion": 1,
                "kind": "nexus-v2-private-alpha-final-freeze-dry-run",
                "transactionId": plan["transactionId"],
                "releaseId": plan["releaseId"],
                "sourceCommit": plan["sourceCommit"],
                "planSha256": plan["sha256"],
                "driverActionsValidated": {role: list(ACTIONS) for role in ROLES},
                "liveMutationPerformed": False,
                "backupManifestCreated": False,
                "completedAtUtc": utc_now(),
            }
            write_new_json(evidence, dry_evidence)
            print(f"final-freeze dry-run passed: {evidence}")
            return

        assert frozen is not None
        coordinator_entries = coordinator_artifacts(plan, bundle_root, receipts, receipt_paths, frozen)
        snapshot_entries = [
            entry
            for role in ROLES
            for entry in receipts[role]["verify-snapshot"]["artifacts"]
        ] + coordinator_entries
        mapped = {(entry["group"], entry["name"]): entry for entry in snapshot_entries}
        expected = {
            (group, name)
            for group, names in release.REQUIRED_ARTIFACTS.items()
            for name in names
        }
        require(set(mapped) == expected and len(mapped) == len(snapshot_entries), "final backup artifact roles are incomplete or duplicated")
        manifest_path = bundle_root / "backup-manifest.json"
        artifact_args = [
            item
            for key in sorted(mapped)
            for item in ("--artifact", f"{key[0]}:{key[1]}:{mapped[key]['path']}")
        ]
        result = release.main(
            [
                "backup-manifest",
                "--bundle-root",
                str(bundle_root),
                "--release-id",
                plan["releaseId"],
                "--source-commit",
                plan["sourceCommit"],
                *artifact_args,
                "--output",
                str(manifest_path),
            ]
        )
        require(result == 0, "final backup manifest creation failed")
        verified = release.verify_backup_manifest(manifest_path, bundle_root)
        final_evidence = {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-final-freeze-evidence",
            "transactionId": plan["transactionId"],
            "releaseId": plan["releaseId"],
            "sourceCommit": plan["sourceCommit"],
            "componentSourceCommits": plan["value"]["componentSourceCommits"],
            "planSha256": plan["sha256"],
            "frozenFinalizedBlock": frozen,
            "stabilityWindowSeconds": plan["stabilityWindowSeconds"],
            "allIngressAndMutatingServicesStopped": True,
            "automaticResumeAttempted": False,
            "backupManifestSha256": verified["sha256"],
            "artifactGroups": sorted(release.REQUIRED_ARTIFACTS),
            "driverSha256": {
                role: plan["components"][role]["driverSha256"] for role in ROLES
            },
            "completedAtUtc": utc_now(),
            "paidOrPublicActivationAllowed": False,
        }
        write_new_json(evidence, final_evidence)
        print(f"final freeze and backup passed: {evidence}")
    except Exception as exc:
        if not evidence.exists():
            failure = {
                "schemaVersion": 1,
                "kind": "nexus-v2-private-alpha-final-freeze-failure",
                "transactionId": plan["transactionId"],
                "releaseId": plan["releaseId"],
                "sourceCommit": plan["sourceCommit"],
                "planSha256": plan["sha256"],
                "error": str(exc),
                "writeBarrierMayBePartial": not dry_run,
                "automaticResumeAttempted": False,
                "requiredResponse": "keep-all-stopped-components-frozen-and-investigate",
                "failedAtUtc": utc_now(),
            }
            write_new_json(evidence, failure)
        raise


def command_validate(args: argparse.Namespace) -> None:
    plan = validate_plan(Path(args.plan), args.expected_plan_sha256)
    print(
        json.dumps(
            {
                "schemaVersion": 1,
                "kind": "nexus-v2-private-alpha-final-freeze-plan-validation",
                "transactionId": plan["transactionId"],
                "releaseId": plan["releaseId"],
                "sourceCommit": plan["sourceCommit"],
                "planSha256": plan["sha256"],
                "roles": list(ROLES),
                "valid": True,
            },
            sort_keys=True,
        )
    )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Coordinate the final private-alpha freeze and complete backup")
    subparsers = parser.add_subparsers(dest="command", required=True)
    validate = subparsers.add_parser("validate")
    validate.add_argument("--plan", required=True)
    validate.add_argument("--expected-plan-sha256", required=True)
    validate.set_defaults(handler=command_validate)
    for name, dry_run in (("dry-run", True), ("execute", False)):
        command = subparsers.add_parser(name)
        command.add_argument("--plan", required=True)
        command.add_argument("--expected-plan-sha256", required=True)
        command.add_argument("--bundle-root", required=True)
        command.add_argument("--state-root", required=True)
        command.add_argument("--evidence", required=True)
        command.set_defaults(handler=lambda args, selected=dry_run: run_plan(args, selected))
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        args.handler(args)
    except (FreezeError, OSError, subprocess.SubprocessError) as exc:
        print(f"nexus-v2-final-freeze: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
