#!/usr/bin/env python3
"""Guard the temporary Phase-2 site-to-chain transport and its final handoff.

The producer is intentionally separate from the post-acceptance reopen.  It can
open only the four read/authority paths required to finish Phase 2, renews a
remote fail-closed lease, and emits an immutable handoff only after the locked
site activation and deployment identity pass their official verifiers.  It
never changes chain state and never opens public site ingress.
"""

from __future__ import annotations

import argparse
import base64
import datetime as dt
import hashlib
import ipaddress
import json
import os
import re
import shlex
import stat
import subprocess
import sys
from pathlib import Path
from typing import Any, Mapping, Sequence


SCRIPT_PATH = Path(__file__).resolve()
TOOL_DIR = SCRIPT_PATH.parent
REPO_ROOT = SCRIPT_PATH.parents[2]
sys.path.insert(0, str(TOOL_DIR))
import deployment_secret_environment  # noqa: E402,F401
import acceptance_boundary  # noqa: E402
import release_lock  # noqa: E402


SHA_RE = re.compile(r"^[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
ID_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
UTC_RE = re.compile(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$")
PLAN_KIND = "nexus-v2-private-alpha-phase2-internal-transport-plan"
RESULT_KIND = "nexus-v2-private-alpha-phase2-internal-transport-result"
HANDOFF_KIND = "nexus-v2-private-alpha-phase2-internal-transport-handoff"
PLAN_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "releaseId",
    "siteReleaseVersion",
    "sourceCommit",
    "siteSourceCommit",
    "createdAtUtc",
    "expiresAtUtc",
    "leaseDurationSeconds",
    "replacementLock",
    "acceptanceBoundaryReceipt",
    "selectedDeploymentEnvironment",
    "selectedSiteDeploymentEnvironment",
    "sshHostPins",
    "remote",
    "network",
    "ports",
    "policy",
}
PIN_KEYS = {"path", "sha256"}
EXECUTABLE_PIN_KEYS = {"path", "sha256", "sourceCommit"}
SSH_PIN_KEYS = {"knownHosts", "manifest", "validator"}
REMOTE_KEYS = {"host", "user", "helper"}
NETWORK = {
    "chainLanIp": "192.168.1.159",
    "siteLanIp": "192.168.1.218",
    "allowedSourceIp": "192.168.1.218",
}
PORTS = {
    "chainRpc": 9944,
    "authority": 8787,
    "media": 4000,
    "ipfsGateway": 8080,
    "forbidden": [30333, 5001],
}
POLICY = {
    "privateAlphaOnly": True,
    "publicIngressMutationAuthorized": False,
    "chainStateMutationAuthorized": False,
    "paidOrPublicActivationAuthorized": False,
    "phase1PublicCaddyMustRemainUnchanged": True,
    "underlyingBackendsRemainLoopbackOnly": True,
    "sourceRestrictedToSiteHost": True,
}
RESULT_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "planSha256",
    "releaseId",
    "sourceCommit",
    "action",
    "state",
    "mutationPerformed",
    "alreadyApplied",
    "helperSha256",
    "marker",
    "heartbeat",
    "watchdog",
    "transport",
    "safety",
    "completedAtUtc",
}
HANDOFF_KEYS = {
    "schemaVersion",
    "kind",
    "releaseId",
    "siteReleaseVersion",
    "sourceCommit",
    "siteSourceCommit",
    "acceptanceBoundaryReceiptSha256",
    "replacementLockSha256",
    "sitePhase1PostDeployIdentitySha256",
    "sitePostPhase2DeploymentIdentitySha256",
    "network",
    "ports",
    "lease",
    "phase2",
    "safety",
    "capturedAtUtc",
}
LEASE_KEYS = {
    "operationId",
    "planSha256",
    "markerPath",
    "markerSha256",
    "heartbeatPath",
    "heartbeatNonce",
    "watchdogService",
    "watchdogTimer",
    "watchdogUnitSha256",
    "watchdogPayloadSha256",
    "armed",
    "expiresAtUtc",
}


class TransportError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise TransportError(message)


def canonical_bytes(value: Any) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")


def duplicate_rejecting_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        require(key not in value, f"duplicate JSON field: {key}")
        value[key] = item
    return value


def decode_json_object(raw: bytes, label: str) -> dict[str, Any]:
    try:
        value = json.loads(
            raw.decode("utf-8"),
            object_pairs_hook=duplicate_rejecting_object,
        )
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise TransportError(f"cannot read {label}: {exc}") from exc
    require(isinstance(value, dict), f"{label} must be a JSON object")
    return value


def read_json(path: Path, label: str) -> dict[str, Any]:
    return decode_json_object(read_stable_regular_file(path, label), label)


def exact_keys(value: Any, keys: set[str], label: str) -> Mapping[str, Any]:
    require(isinstance(value, dict) and set(value) == keys, f"{label} closed schema mismatch")
    return value


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_regular_path(path: Path, label: str) -> Path:
    require(path.is_absolute(), f"{label} path must be absolute")
    require(".." not in path.parts, f"{label} path may not contain parent traversal")
    try:
        resolved = path.resolve(strict=True)
    except OSError as exc:
        raise TransportError(f"{label} is unavailable") from exc
    require(path == resolved, f"{label} path is not canonical or traverses a symlink")
    cursor = path
    while cursor != cursor.parent:
        require(not cursor.is_symlink(), f"{label} path traverses a symlink")
        cursor = cursor.parent
    require(path.is_file() and not path.is_symlink(), f"{label} is unavailable")
    return path


def read_stable_regular_file(path: Path, label: str) -> bytes:
    path = canonical_regular_path(path, label)
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(path, flags)
    except OSError as exc:
        raise TransportError(f"cannot open {label}") from exc
    try:
        before = os.fstat(descriptor)
        require(stat.S_ISREG(before.st_mode), f"{label} is not a regular file")
        chunks: list[bytes] = []
        while True:
            chunk = os.read(descriptor, 1024 * 1024)
            if not chunk:
                break
            chunks.append(chunk)
        after = os.fstat(descriptor)
    finally:
        os.close(descriptor)
    observed = os.lstat(path)
    identity = lambda item: (
        item.st_dev,
        item.st_ino,
        item.st_size,
        item.st_mtime_ns,
        item.st_ctime_ns,
    )
    require(
        identity(before) == identity(after) == identity(observed),
        f"{label} changed while it was read",
    )
    return b"".join(chunks)


def stable_sha256(path: Path, label: str) -> str:
    return hashlib.sha256(read_stable_regular_file(path, label)).hexdigest()


def ensure_sha(value: Any, label: str, *, nonzero: bool = True) -> str:
    require(isinstance(value, str) and SHA_RE.fullmatch(value) is not None, f"invalid {label}")
    if nonzero:
        require(value != "0" * 64, f"zero {label} is forbidden")
    return value


def ensure_id(value: Any, label: str) -> str:
    require(isinstance(value, str) and ID_RE.fullmatch(value) is not None, f"invalid {label}")
    return value


def parse_utc(value: Any, label: str) -> dt.datetime:
    require(isinstance(value, str) and UTC_RE.fullmatch(value) is not None, f"invalid {label}")
    return dt.datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=dt.timezone.utc)


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).replace(microsecond=0).strftime("%Y-%m-%dT%H:%M:%SZ")


def parse_environment(path: Path, label: str = "deployment environment") -> dict[str, str]:
    raw = read_stable_regular_file(path, label)
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise TransportError(f"{label} is not UTF-8") from exc
    require("\r" not in text and "\x00" not in text, f"{label} contains control characters")
    result: dict[str, str] = {}
    for line_number, raw_line in enumerate(text.splitlines(), 1):
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        require("=" in line, f"{label} line {line_number} is not an assignment")
        key, value = line.split("=", 1)
        key = key.strip()
        require(
            re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", key) is not None,
            f"{label} line {line_number} has an invalid key",
        )
        require(key not in result, f"{label} has duplicate key: {key}")
        value = value.strip()
        if value[:1] in {"'", '"'} or value[-1:] in {"'", '"'}:
            require(
                len(value) >= 2 and value[0] == value[-1],
                f"{label} line {line_number} has unmatched quotes",
            )
            value = value[1:-1]
        require(
            all(ord(character) >= 0x20 and character != "\x7f" for character in value),
            f"{label} line {line_number} contains control characters",
        )
        result[key] = value
    return result


def validate_phase2_transport_credential_reference(
    environment: Mapping[str, str],
) -> str:
    """Validate the one credential reference the Phase-2 transport may consume.

    The coordinator must never place credential-bearing names or plaintext in a
    child environment.  The protected Bash boundary receives only this pinned
    owner-only file reference as shell-local data and resolves it with the
    deployment library's stable descriptor reader.
    """

    require(
        environment.get("DEPLOY_PASSWORD", "") == "",
        "Phase-2 transport requires key-only SSH; DEPLOY_PASSWORD is forbidden",
    )
    reference = environment.get("REMOTE_SUDO_PASSWORD", "")
    require(
        reference.startswith("@/"),
        "Phase-2 REMOTE_SUDO_PASSWORD must be an @/absolute/path file reference",
    )
    path = Path(reference[1:])
    path = canonical_regular_path(path, "Phase-2 sudo credential file")
    observed = os.lstat(path)
    require(
        observed.st_uid == os.getuid()
        and observed.st_nlink == 1
        and stat.S_IMODE(observed.st_mode) in {0o400, 0o600},
        "Phase-2 sudo credential file must be current-owner, single-link, and mode 0400 or 0600",
    )
    return reference


def file_pin(path_value: str, label: str, *, canonical_json: bool = False) -> dict[str, str]:
    path = Path(path_value)
    path = canonical_regular_path(path, label)
    raw = read_stable_regular_file(path, label)
    if canonical_json:
        value = decode_json_object(raw, label)
        require(raw == canonical_bytes(value), f"{label} is not canonical JSON")
    return {"path": str(path), "sha256": hashlib.sha256(raw).hexdigest()}


def validate_pin(value: Any, label: str, *, executable: bool = False) -> dict[str, str]:
    keys = EXECUTABLE_PIN_KEYS if executable else PIN_KEYS
    pin = dict(exact_keys(value, keys, label))
    path = canonical_regular_path(Path(pin["path"]), label)
    ensure_sha(pin["sha256"], f"{label} SHA-256", nonzero=False)
    require(stable_sha256(path, label) == pin["sha256"], f"{label} hash mismatch")
    if executable:
        require(os.access(path, os.X_OK), f"{label} is not executable")
        require(COMMIT_RE.fullmatch(str(pin["sourceCommit"])) is not None, f"{label} source commit is invalid")
    return pin


def write_new(path: Path, value: Mapping[str, Any]) -> None:
    require_new_output(path)
    parent_fd = open_output_parent(path.parent)
    descriptor: int | None = None
    created = False
    try:
        flags = (
            os.O_WRONLY
            | os.O_CREAT
            | os.O_EXCL
            | getattr(os, "O_CLOEXEC", 0)
            | getattr(os, "O_NOFOLLOW", 0)
        )
        descriptor = os.open(path.name, flags, 0o400, dir_fd=parent_fd)
        created = True
        os.fchmod(descriptor, 0o400)
        payload = canonical_bytes(value)
        offset = 0
        while offset < len(payload):
            written = os.write(descriptor, payload[offset:])
            require(written > 0, "output write made no progress")
            offset += written
        os.fsync(descriptor)
        opened = os.fstat(descriptor)
        observed = os.stat(path.name, dir_fd=parent_fd, follow_symlinks=False)
        require(
            stat.S_ISREG(opened.st_mode)
            and (opened.st_dev, opened.st_ino) == (observed.st_dev, observed.st_ino),
            "output target changed while it was written",
        )
    except OSError as exc:
        if created:
            try:
                os.unlink(path.name, dir_fd=parent_fd)
            except OSError:
                pass
        raise TransportError(f"cannot create output: {path}") from exc
    except Exception:
        if created:
            try:
                os.unlink(path.name, dir_fd=parent_fd)
            except OSError:
                pass
        raise
    finally:
        if descriptor is not None:
            os.close(descriptor)
        os.close(parent_fd)


def open_output_parent(path: Path) -> int:
    require(path.is_absolute(), "output parent must be absolute")
    require(".." not in path.parts, "output parent may not contain parent traversal")
    flags = os.O_RDONLY | os.O_DIRECTORY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open("/", flags)
    try:
        for component in path.parts[1:]:
            try:
                next_descriptor = os.open(component, flags, dir_fd=descriptor)
            except FileNotFoundError:
                try:
                    os.mkdir(component, 0o700, dir_fd=descriptor)
                except FileExistsError:
                    pass
                next_descriptor = os.open(component, flags, dir_fd=descriptor)
            os.close(descriptor)
            descriptor = next_descriptor
        return descriptor
    except OSError as exc:
        os.close(descriptor)
        raise TransportError(f"output parent is unsafe: {path}") from exc


def require_new_output(path: Path) -> None:
    require(path.is_absolute(), "output must be absolute")
    require(".." not in path.parts and path.name not in {"", ".", ".."}, "output path is invalid")
    cursor = Path("/")
    for component in path.parts[1:-1]:
        cursor /= component
        try:
            observed = os.lstat(cursor)
        except FileNotFoundError:
            break
        require(stat.S_ISDIR(observed.st_mode), f"output parent is unsafe: {cursor}")
    try:
        os.lstat(path)
    except FileNotFoundError:
        return
    raise TransportError(f"refusing to overwrite output: {path}")


def validate_acceptance(
    replacement: Mapping[str, Any], receipt_pin: Mapping[str, str]
) -> dict[str, Any]:
    artifacts = replacement["artifacts"]
    target = read_json(Path(artifacts["targetIdentity"]["path"]), "target identity")
    node = read_json(Path(artifacts["nodeCandidateManifest"]["path"]), "node candidate")
    runtime = node.get("runtimeBundle")
    require(isinstance(runtime, dict), "replacement node runtime bundle is invalid")
    try:
        receipt = acceptance_boundary.validate_receipt(
            Path(receipt_pin["path"]),
            receipt_pin["sha256"],
            release_id=replacement["releaseId"],
            source_commit=replacement["repositories"]["chain"]["head"],
            genesis_hash=target.get("genesisHash"),
            runtime_code_sha256=runtime.get("productionWasmSha256"),
            runtime_metadata_scale_sha256=runtime.get("metadataScaleSha256"),
        )
    except acceptance_boundary.BoundaryError as exc:
        raise TransportError(f"acceptance-boundary receipt is invalid: {exc}") from exc
    require(
        receipt.get("coordinatorDecision") == "keep-v2"
        and receipt.get("phase1SmokePassed") is True
        and receipt.get("automaticRestorePermanentlyDisabled") is True,
        "acceptance boundary has not retired automatic restore for the kept V2 state",
    )
    require(
        receipt.get("operatorV2WriteScope", {}).get("paidOrPublicActivation") is False,
        "acceptance boundary permits paid/public activation",
    )
    return receipt


def validate_replacement(
    replacement_pin: Mapping[str, str],
    chain_environment: Mapping[str, str],
    site_environment: Mapping[str, str],
) -> dict[str, Any]:
    try:
        return release_lock.validate_replacement_lock(
            Path(replacement_pin["path"]),
            replacement_pin["sha256"],
            chain_environment["path"],
            site_environment["path"],
        )
    except release_lock.ReleaseLockError as exc:
        raise TransportError(f"replacement-lock validation failed: {exc}") from exc


def validate_ssh_pins(value: Any) -> dict[str, dict[str, str]]:
    pins = exact_keys(value, SSH_PIN_KEYS, "SSH host pins")
    result = {name: validate_pin(pins[name], f"SSH {name}") for name in SSH_PIN_KEYS}
    validator = Path(result["validator"]["path"])
    require(os.access(validator, os.X_OK), "SSH host-pin validator is not executable")
    completed = subprocess.run(
        [
            sys.executable,
            str(validator),
            "verify",
            "--known-hosts",
            result["knownHosts"]["path"],
            "--manifest",
            result["manifest"]["path"],
        ],
        capture_output=True,
        text=True,
        check=False,
        timeout=30,
        env=deployment_secret_environment.child_environment(),
    )
    require(completed.returncode == 0, "SSH host-pin verification failed")
    return result


def validate_plan(value: Mapping[str, Any], *, allow_expired: bool = False) -> dict[str, Any]:
    plan = dict(exact_keys(value, PLAN_KEYS, "Phase-2 transport plan"))
    require(plan["schemaVersion"] == 1 and plan["kind"] == PLAN_KIND, "Phase-2 transport plan identity mismatch")
    ensure_id(plan["operationId"], "operation ID")
    ensure_id(plan["releaseId"], "release ID")
    require(COMMIT_RE.fullmatch(str(plan["sourceCommit"])) is not None, "invalid chain source commit")
    require(COMMIT_RE.fullmatch(str(plan["siteSourceCommit"])) is not None, "invalid site source commit")
    created = parse_utc(plan["createdAtUtc"], "plan creation time")
    expires = parse_utc(plan["expiresAtUtc"], "plan expiry")
    now = dt.datetime.now(dt.timezone.utc)
    require(created < expires and expires - created <= dt.timedelta(hours=24), "plan lifetime must be in (0,24h]")
    require(created <= now + dt.timedelta(seconds=30), "Phase-2 transport plan is from the future")
    if not allow_expired:
        require(expires > now, "Phase-2 transport plan expired")
    require(plan["leaseDurationSeconds"] == 900, "Phase-2 transport lease duration must be 900 seconds")
    replacement_pin = validate_pin(plan["replacementLock"], "replacement lock")
    receipt_pin = validate_pin(plan["acceptanceBoundaryReceipt"], "acceptance receipt")
    chain_env_pin = validate_pin(plan["selectedDeploymentEnvironment"], "chain environment")
    site_env_pin = validate_pin(plan["selectedSiteDeploymentEnvironment"], "site environment")
    chain_environment = parse_environment(Path(chain_env_pin["path"]), "chain environment")
    site_environment = parse_environment(Path(site_env_pin["path"]), "site environment")
    replacement = validate_replacement(replacement_pin, chain_env_pin, site_env_pin)
    validate_acceptance(replacement, receipt_pin)
    require(replacement["releaseId"] == plan["releaseId"], "plan/replacement release mismatch")
    require(
        replacement["repositories"]["chain"]["head"] == plan["sourceCommit"]
        and replacement["repositories"]["web"]["head"] == plan["siteSourceCommit"],
        "plan/replacement source mismatch",
    )
    candidate = release_lock.validate_site_candidate(
        Path(replacement["artifacts"]["siteDeploymentCandidateManifest"]["path"]),
        replacement["repositories"],
    )
    require(candidate["releaseVersion"] == plan["siteReleaseVersion"], "site release version mismatch")
    require(plan["network"] == NETWORK and plan["ports"] == PORTS, "Phase-2 transport network/port contract mismatch")
    for value_ip in NETWORK.values():
        require(ipaddress.ip_address(value_ip).is_private, "Phase-2 transport address is not private")
    require(plan["policy"] == POLICY, "Phase-2 transport policy mismatch")
    require(
        chain_environment.get("DEPLOY_HOST") == NETWORK["chainLanIp"]
        and chain_environment.get("DEPLOY_USER", "eterra2010") == "eterra2010"
        and chain_environment.get("SSH_PORT", "22") == "22"
        and chain_environment.get("SSH_OPTS", "") == ""
        and chain_environment.get("MINI_LAN_IP") == NETWORK["chainLanIp"]
        and chain_environment.get("SITE_PROXY_LAN_IP") == NETWORK["siteLanIp"]
        and chain_environment.get("CHAIN_RPC_PORT", "9944") == "9944"
        and chain_environment.get("CHAIN_P2P_PORT", "30333") == "30333"
        and chain_environment.get("MEDIA_PORT", "4000") == "4000"
        and chain_environment.get("IPFS_API_PORT", "5001") == "5001"
        and chain_environment.get("IPFS_GATEWAY_PORT", "8080") == "8080"
        and chain_environment.get("AUTHORITY_PORT", "8787") == "8787"
        and chain_environment.get("ETERRA_RELEASE_VERSION") == plan["releaseId"]
        and chain_environment.get("ETERRA_EXPECTED_CHAIN_COMMIT")
        == plan["sourceCommit"],
        "selected chain environment differs from the protected Phase-2 transport contract",
    )
    identity_path = Path(chain_environment.get("SSH_IDENTITY_FILE", ""))
    canonical_regular_path(identity_path, "chain SSH identity")
    require(
        site_environment.get("DEPLOY_HOST") == NETWORK["siteLanIp"]
        and site_environment.get("SITE_LAN_IP") == NETWORK["siteLanIp"]
        and site_environment.get("CHAIN_UPSTREAM_HOST") == NETWORK["chainLanIp"]
        and site_environment.get("MEDIA_UPSTREAM_HOST") == NETWORK["chainLanIp"]
        and site_environment.get("IPFS_UPSTREAM_HOST") == NETWORK["chainLanIp"]
        and site_environment.get("AUTHORITY_UPSTREAM_HOST") == NETWORK["chainLanIp"]
        and site_environment.get("INDEXER_CHAIN_WS_URL")
        == f"ws://{NETWORK['chainLanIp']}:9944"
        and site_environment.get("NEXUS_V2_FULL_LOOP_ACCEPTANCE_READS_ENABLED", "").lower()
        == "false"
        and site_environment.get("NEXUS_V2_FULL_LOOP_ACCEPTANCE_TARGET_JSON", "")
        == "",
        "selected site environment differs from the closed Phase-2 internal transport contract",
    )
    validate_phase2_transport_credential_reference(chain_environment)
    validate_ssh_pins(plan["sshHostPins"])
    remote = exact_keys(plan["remote"], REMOTE_KEYS, "remote chain host")
    require(
        remote["host"] == NETWORK["chainLanIp"] and remote["user"] == "eterra2010",
        "remote chain target must be exact",
    )
    helper = validate_pin(remote["helper"], "Phase-2 transport helper", executable=True)
    require(helper["sourceCommit"] == plan["sourceCommit"], "Phase-2 helper source mismatch")
    return plan


def load_plan(path: Path, expected_sha256: str, *, allow_expired: bool = False) -> dict[str, Any]:
    ensure_sha(expected_sha256, "plan SHA-256")
    raw = read_stable_regular_file(path, "Phase-2 transport plan")
    require(hashlib.sha256(raw).hexdigest() == expected_sha256, "Phase-2 transport plan hash mismatch")
    value = decode_json_object(raw, "Phase-2 transport plan")
    require(raw == canonical_bytes(value), "Phase-2 transport plan is not canonical JSON")
    return validate_plan(value, allow_expired=allow_expired)


def validate_result(value: Mapping[str, Any], plan: Mapping[str, Any], plan_sha256: str, action: str) -> dict[str, Any]:
    result = dict(exact_keys(value, RESULT_KEYS, "Phase-2 transport result"))
    require(result["schemaVersion"] == 1 and result["kind"] == RESULT_KIND, "Phase-2 result identity mismatch")
    require(
        result["operationId"] == plan["operationId"]
        and result["planSha256"] == plan_sha256
        and result["releaseId"] == plan["releaseId"]
        and result["sourceCommit"] == plan["sourceCommit"]
        and result["action"] == action,
        "Phase-2 result authority mismatch",
    )
    require(result["helperSha256"] == plan["remote"]["helper"]["sha256"], "Phase-2 result helper mismatch")
    require(isinstance(result["mutationPerformed"], bool) and isinstance(result["alreadyApplied"], bool), "Phase-2 result flags are invalid")
    require(result["transport"] == {"network": NETWORK, "ports": PORTS}, "Phase-2 result transport mismatch")
    require(result["safety"] == POLICY, "Phase-2 result safety mismatch")
    completed = parse_utc(result["completedAtUtc"], "Phase-2 result completion")
    now = dt.datetime.now(dt.timezone.utc)
    require(completed <= now + dt.timedelta(seconds=30), "Phase-2 result completion is in the future")
    expected_root = (
        f"/opt/eterra-alpha/shared/phase2-internal-transport/"
        f"{plan['operationId']}"
    )
    if action == "close":
        require(result["state"] == "closed", "Phase-2 close did not close")
        require(result["heartbeat"] is None and result["watchdog"] == {"armed": False}, "Phase-2 close left a lease")
        marker = exact_keys(result["marker"], {"path", "sha256"}, "Phase-2 close marker")
        require(marker["path"] == f"{expected_root}/closed.json", "Phase-2 close marker path mismatch")
        ensure_sha(marker["sha256"], "Phase-2 close marker SHA-256")
        require(
            (result["mutationPerformed"], result["alreadyApplied"])
            in {(True, False), (False, True)},
            "Phase-2 close idempotence flags are invalid",
        )
        return result
    require(result["state"] == "open", "Phase-2 transport is not open")
    marker = exact_keys(result["marker"], {"path", "sha256"}, "Phase-2 marker")
    require(marker["path"] == f"{expected_root}/open.json", "Phase-2 marker path mismatch")
    ensure_sha(marker["sha256"], "Phase-2 marker SHA-256")
    heartbeat = exact_keys(result["heartbeat"], {"path", "nonce", "expiresAtUtc"}, "Phase-2 heartbeat")
    require(heartbeat["path"] == f"{expected_root}/heartbeat.json", "Phase-2 heartbeat path mismatch")
    require(re.fullmatch(r"[0-9a-f]{64}", str(heartbeat["nonce"])) is not None, "Phase-2 heartbeat nonce is invalid")
    heartbeat_expiry = parse_utc(heartbeat["expiresAtUtc"], "Phase-2 heartbeat expiry")
    require(
        heartbeat_expiry >= now + dt.timedelta(minutes=5),
        "Phase-2 heartbeat has less than five minutes remaining",
    )
    watchdog = exact_keys(
        result["watchdog"],
        {"service", "timer", "unitSha256", "payloadSha256", "armed"},
        "Phase-2 watchdog",
    )
    require(watchdog["armed"] is True, "Phase-2 watchdog is not armed")
    ensure_sha(watchdog["unitSha256"], "Phase-2 watchdog unit SHA-256")
    ensure_sha(watchdog["payloadSha256"], "Phase-2 watchdog payload SHA-256")
    require(
        watchdog["service"]
        == f"nexus-v2-phase2-internal-transport-{plan['operationId']}.service"
        and watchdog["timer"]
        == f"nexus-v2-phase2-internal-transport-{plan['operationId']}.timer",
        "Phase-2 watchdog identity mismatch",
    )
    expected_flags = {
        "execute": {(True, False), (False, True)},
        "renew": {(True, False)},
        "verify": {(False, True)},
    }
    require(
        (result["mutationPerformed"], result["alreadyApplied"])
        in expected_flags[action],
        f"Phase-2 {action} idempotence flags are invalid",
    )
    return result


def invoke_remote(plan_path: Path, plan: Mapping[str, Any], plan_sha256: str, action: str) -> dict[str, Any]:
    require(action in {"execute", "renew", "verify", "close"}, "invalid Phase-2 remote action")
    require(os.environ.get("NEXUS_V2_PHASE2_INTERNAL_TRANSPORT_CONFIRMATION") == "PRIVATE_ALPHA_PHASE2_INTERNAL_TRANSPORT", "protected Phase-2 transport confirmation is missing")
    helper_path = Path(plan["remote"]["helper"]["path"])
    helper_base64 = base64.b64encode(
        read_stable_regular_file(helper_path, "Phase-2 transport helper")
    ).decode("ascii")
    plan_base64 = base64.b64encode(
        read_stable_regular_file(plan_path, "Phase-2 transport plan")
    ).decode("ascii")
    wrapper = "\n".join(
        (
            "set -euo pipefail",
            'helper="$(mktemp /tmp/nexus-v2-phase2-transport-helper.XXXXXX)"',
            'trap \'rm -f "${helper}"\' EXIT',
            f"printf %s {shlex.quote(helper_base64)} | base64 -d >\"${{helper}}\"",
            f"test \"$(sha256sum \"${{helper}}\" | awk '{{print $1}}')\" = {shlex.quote(plan['remote']['helper']['sha256'])}",
            'chmod 0700 "${helper}"',
            f'"${{helper}}" {shlex.quote(action)} {shlex.quote(plan_base64)} {shlex.quote(plan_sha256)} {shlex.quote(plan["remote"]["helper"]["sha256"])}',
            "",
        )
    )
    chain_root = Path(
        read_json(Path(plan["replacementLock"]["path"]), "replacement lock")[
            "repositories"
        ]["chain"]["root"]
    )
    library = canonical_regular_path(
        chain_root / "deploy/alpha/macmini2010/lib.sh",
        "pinned chain deployment library",
    )
    pins = plan["sshHostPins"]
    chain_values = parse_environment(
        Path(plan["selectedDeploymentEnvironment"]["path"]),
        "chain environment",
    )
    sudo_reference = validate_phase2_transport_credential_reference(chain_values)
    environment = deployment_secret_environment.child_environment(
        {
            "DEPLOY_HOST": NETWORK["chainLanIp"],
            "DEPLOY_USER": "eterra2010",
            "SSH_PORT": "22",
            "SSH_OPTS": "",
            "SSH_IDENTITY_FILE": chain_values["SSH_IDENTITY_FILE"],
            "SSH_TARGET": f"eterra2010@{NETWORK['chainLanIp']}",
            "REMOTE_SCRIPT_DIR": chain_values.get(
                "REMOTE_SCRIPT_DIR", "/tmp/alpha-macmini2010-eterra2010"
            ),
            "ETERRA_RELEASE_VERSION": plan["releaseId"],
            "NEXUS_V2_SSH_KNOWN_HOSTS_FILE": pins["knownHosts"]["path"],
            "NEXUS_V2_SSH_KNOWN_HOSTS_SHA256": pins["knownHosts"]["sha256"],
            "NEXUS_V2_SSH_HOST_PIN_MANIFEST": pins["manifest"]["path"],
            "NEXUS_V2_SSH_HOST_PIN_MANIFEST_SHA256": pins["manifest"]["sha256"],
        }
    )
    launcher = "\n".join(
        (
            "set -euo pipefail",
            f"source {shlex.quote(str(library))}",
            "DEPLOY_PASSWORD=''",
            f"REMOTE_SUDO_PASSWORD={shlex.quote(sudo_reference)}",
            'REMOTE_SUDO_PASSWORD="$(read_protected_sudo_secret_value "${REMOTE_SUDO_PASSWORD}")"',
            "clear_transport_secret_exports",
            "verify_nexus_v2_ssh_host_pins",
            "build_nexus_v2_pinned_ssh_transport",
            '[[ "${NEXUS_V2_SSH_TRANSPORT_CONTRACT_VERSION}" == nexus-v2-pinned-host-v1 ]]',
            "remote_root_bash <<'NEXUS_V2_PHASE2_REMOTE'",
            wrapper.rstrip("\n"),
            "NEXUS_V2_PHASE2_REMOTE",
            "",
        )
    )
    completed = subprocess.run(
        ["/bin/bash", "-s", "--"],
        input=launcher,
        env=environment,
        capture_output=True,
        text=True,
        check=False,
        timeout=180,
    )
    require(completed.returncode == 0, f"Phase-2 remote {action} failed: {completed.stderr.strip()}")
    lines = [line.removeprefix("NEXUS_V2_PHASE2_TRANSPORT_RESULT:") for line in completed.stdout.splitlines() if line.startswith("NEXUS_V2_PHASE2_TRANSPORT_RESULT:")]
    require(len(lines) == 1, "Phase-2 helper returned an invalid result envelope")
    try:
        value = json.loads(base64.b64decode(lines[0], validate=True))
    except (ValueError, json.JSONDecodeError) as exc:
        raise TransportError("Phase-2 helper returned invalid result JSON") from exc
    require(isinstance(value, dict), "Phase-2 helper result is not an object")
    return validate_result(value, plan, plan_sha256, action)


def validate_site_phase2(
    replacement: Mapping[str, Any],
    phase1_pin: Mapping[str, str],
    activation_pin: Mapping[str, str],
    identity_pin: Mapping[str, str],
) -> None:
    synthetic = {
        "releaseId": replacement["releaseId"],
        "repositories": replacement["repositories"],
        "artifacts": {
            **replacement["artifacts"],
            "sitePhase1PostDeployIdentity": phase1_pin,
            "fullLoopIndexerActivationReceipt": activation_pin,
            "sitePostPhase2DeploymentIdentity": identity_pin,
        },
    }
    candidate = release_lock.validate_site_candidate(
        Path(replacement["artifacts"]["siteDeploymentCandidateManifest"]["path"]),
        replacement["repositories"],
    )
    release_lock.validate_site_final_artifacts(synthetic, candidate)


def validate_handoff(
    value: Mapping[str, Any],
    *,
    replacement_pin: Mapping[str, str],
    acceptance_pin: Mapping[str, str],
    phase1_pin: Mapping[str, str],
    activation_pin: Mapping[str, str],
    identity_pin: Mapping[str, str],
    chain_environment: Mapping[str, str],
    site_environment: Mapping[str, str],
) -> dict[str, Any]:
    handoff = dict(exact_keys(value, HANDOFF_KEYS, "Phase-2 transport handoff"))
    require(handoff["schemaVersion"] == 1 and handoff["kind"] == HANDOFF_KIND, "Phase-2 handoff identity mismatch")
    replacement = validate_replacement(replacement_pin, chain_environment, site_environment)
    validate_acceptance(replacement, acceptance_pin)
    validate_site_phase2(replacement, phase1_pin, activation_pin, identity_pin)
    candidate = release_lock.validate_site_candidate(
        Path(replacement["artifacts"]["siteDeploymentCandidateManifest"]["path"]),
        replacement["repositories"],
    )
    require(
        handoff["releaseId"] == replacement["releaseId"]
        and handoff["siteReleaseVersion"] == candidate["releaseVersion"]
        and handoff["sourceCommit"] == replacement["repositories"]["chain"]["head"]
        and handoff["siteSourceCommit"] == replacement["repositories"]["web"]["head"],
        "Phase-2 handoff release/source mismatch",
    )
    require(
        handoff["replacementLockSha256"] == replacement_pin["sha256"]
        and handoff["acceptanceBoundaryReceiptSha256"] == acceptance_pin["sha256"]
        and handoff["sitePhase1PostDeployIdentitySha256"] == phase1_pin["sha256"]
        and handoff["sitePostPhase2DeploymentIdentitySha256"] == identity_pin["sha256"],
        "Phase-2 handoff artifact binding mismatch",
    )
    require(handoff["network"] == NETWORK and handoff["ports"] == PORTS, "Phase-2 handoff transport mismatch")
    lease = exact_keys(handoff["lease"], LEASE_KEYS, "Phase-2 handoff lease")
    ensure_id(lease["operationId"], "lease operation ID")
    for field in (
        "planSha256",
        "markerSha256",
        "watchdogUnitSha256",
        "watchdogPayloadSha256",
    ):
        ensure_sha(lease[field], f"lease {field}")
    protected_root = f"/opt/eterra-alpha/shared/phase2-internal-transport/{lease['operationId']}"
    require(
        lease["markerPath"] == f"{protected_root}/open.json"
        and lease["heartbeatPath"] == f"{protected_root}/heartbeat.json",
        "Phase-2 lease paths do not match the protected operation root",
    )
    require(re.fullmatch(r"[0-9a-f]{64}", str(lease["heartbeatNonce"])) is not None, "Phase-2 lease nonce is invalid")
    require(
        lease["watchdogService"]
        == f"nexus-v2-phase2-internal-transport-{lease['operationId']}.service"
        and lease["watchdogTimer"]
        == f"nexus-v2-phase2-internal-transport-{lease['operationId']}.timer",
        "Phase-2 handoff watchdog identity mismatch",
    )
    require(lease["armed"] is True, "Phase-2 handoff lease is not armed")
    lease_expiry = parse_utc(lease["expiresAtUtc"], "Phase-2 handoff lease expiry")
    require(
        handoff["phase2"]
        == {
            "publicIngressClosed": True,
            "siteIndexerSynchronized": True,
            "authorityReady": True,
            "fullLoopActivationReceiptSha256": activation_pin["sha256"],
        },
        "Phase-2 readiness proof mismatch",
    )
    require(
        handoff["safety"]
        == {
            "chainStateMutationAuthorized": False,
            "paidOrPublicActivationAuthorized": False,
            "sourceRestricted": True,
            "loopbackBackendsPreserved": True,
            "forbiddenPortsClosed": True,
        },
        "Phase-2 handoff safety mismatch",
    )
    captured = parse_utc(handoff["capturedAtUtc"], "Phase-2 handoff capture time")
    require(
        captured <= dt.datetime.now(dt.timezone.utc) + dt.timedelta(seconds=30)
        and lease_expiry >= captured + dt.timedelta(minutes=5),
        "Phase-2 handoff was not captured with at least five minutes of lease",
    )
    return handoff


def command_capture_plan(args: argparse.Namespace) -> None:
    replacement_pin = file_pin(args.replacement_lock, "replacement lock", canonical_json=True)
    require(replacement_pin["sha256"] == args.expected_replacement_lock_sha256, "replacement-lock hash mismatch")
    acceptance_pin = file_pin(args.acceptance_boundary_receipt, "acceptance receipt", canonical_json=True)
    require(acceptance_pin["sha256"] == args.expected_acceptance_boundary_receipt_sha256, "acceptance-receipt hash mismatch")
    chain_env_pin = file_pin(args.selected_deployment_environment, "chain environment")
    site_env_pin = file_pin(args.selected_site_deployment_environment, "site environment")
    replacement = validate_replacement(replacement_pin, chain_env_pin, site_env_pin)
    validate_acceptance(replacement, acceptance_pin)
    artifacts = replacement["artifacts"]
    require(chain_env_pin == artifacts["deploymentEnvironment"] and site_env_pin == artifacts["siteDeploymentEnvironment"], "selected environments differ from replacement lock")
    site_candidate = release_lock.validate_site_candidate(
        Path(artifacts["siteDeploymentCandidateManifest"]["path"]),
        replacement["repositories"],
    )
    chain_env = parse_environment(Path(chain_env_pin["path"]), "chain environment")
    helper = Path(replacement["repositories"]["chain"]["root"]) / "deploy/alpha/macmini2010/nexus-v2-phase2-internal-transport-host-action.sh"
    ssh_validator = Path(replacement["repositories"]["chain"]["root"]) / "scripts/nexus-v2-private-alpha/capture_ssh_host_pins.py"
    created = args.created_at or utc_now()
    created_time = parse_utc(created, "plan creation time")
    expires = args.expires_at or (created_time + dt.timedelta(hours=4)).strftime("%Y-%m-%dT%H:%M:%SZ")
    plan = {
        "schemaVersion": 1,
        "kind": PLAN_KIND,
        "operationId": args.operation_id,
        "releaseId": replacement["releaseId"],
        "siteReleaseVersion": site_candidate["releaseVersion"],
        "sourceCommit": replacement["repositories"]["chain"]["head"],
        "siteSourceCommit": replacement["repositories"]["web"]["head"],
        "createdAtUtc": created,
        "expiresAtUtc": expires,
        "leaseDurationSeconds": 900,
        "replacementLock": replacement_pin,
        "acceptanceBoundaryReceipt": acceptance_pin,
        "selectedDeploymentEnvironment": chain_env_pin,
        "selectedSiteDeploymentEnvironment": site_env_pin,
        "sshHostPins": {
            "knownHosts": artifacts["sshKnownHosts"],
            "manifest": artifacts["sshHostPinManifest"],
            "validator": file_pin(str(ssh_validator), "SSH host-pin validator"),
        },
        "remote": {
            "host": NETWORK["chainLanIp"],
            "user": chain_env.get("DEPLOY_USER", "eterra2010"),
            "helper": {
                **file_pin(str(helper), "Phase-2 transport helper"),
                "sourceCommit": replacement["repositories"]["chain"]["head"],
            },
        },
        "network": NETWORK,
        "ports": PORTS,
        "policy": POLICY,
    }
    validate_plan(plan)
    output = Path(args.output)
    write_new(output, plan)
    print(f"Phase-2 internal transport plan captured: {output} sha256={sha256_file(output)}")


def command_validate(args: argparse.Namespace) -> None:
    load_plan(Path(args.plan), args.expected_plan_sha256)
    print(f"Phase-2 internal transport plan verified: sha256={args.expected_plan_sha256}")


def command_remote(args: argparse.Namespace) -> None:
    plan_path = Path(args.plan)
    plan = load_plan(plan_path, args.expected_plan_sha256, allow_expired=args.command == "close")
    output = Path(args.result)
    require_new_output(output)
    result = invoke_remote(plan_path, plan, args.expected_plan_sha256, args.command)
    write_new(output, result)
    print(f"Phase-2 internal transport {args.command}: {output} sha256={sha256_file(output)}")


def checked_pin(path_value: str, expected_sha: str, label: str) -> dict[str, str]:
    pin = file_pin(path_value, label, canonical_json=True)
    require(pin["sha256"] == expected_sha, f"{label} hash mismatch")
    return pin


def command_capture_handoff(args: argparse.Namespace) -> None:
    plan_path = Path(args.plan)
    plan = load_plan(plan_path, args.expected_plan_sha256)
    verification_output = Path(args.verification_result)
    handoff_output = Path(args.output)
    require_new_output(verification_output)
    require_new_output(handoff_output)
    require(
        verification_output != handoff_output,
        "verification result and handoff outputs must differ",
    )
    # Renew immediately before sealing the handoff so final-lock capture and the
    # reopen adopter inherit a full lease window rather than a nearly-expired one.
    verification = invoke_remote(plan_path, plan, args.expected_plan_sha256, "renew")
    write_new(verification_output, verification)
    phase1_pin = checked_pin(args.site_phase1_post_deploy_identity, args.expected_site_phase1_post_deploy_identity_sha256, "site Phase-1 identity")
    activation_pin = checked_pin(args.full_loop_indexer_activation_receipt, args.expected_full_loop_indexer_activation_receipt_sha256, "full-loop activation receipt")
    identity_pin = checked_pin(args.site_post_phase2_deployment_identity, args.expected_site_post_phase2_deployment_identity_sha256, "site post-Phase2 identity")
    replacement = validate_replacement(plan["replacementLock"], plan["selectedDeploymentEnvironment"], plan["selectedSiteDeploymentEnvironment"])
    validate_site_phase2(replacement, phase1_pin, activation_pin, identity_pin)
    heartbeat_expiry = parse_utc(verification["heartbeat"]["expiresAtUtc"], "live heartbeat expiry")
    require(heartbeat_expiry >= dt.datetime.now(dt.timezone.utc) + dt.timedelta(minutes=5), "live Phase-2 heartbeat has less than five minutes remaining")
    handoff = {
        "schemaVersion": 1,
        "kind": HANDOFF_KIND,
        "releaseId": plan["releaseId"],
        "siteReleaseVersion": plan["siteReleaseVersion"],
        "sourceCommit": plan["sourceCommit"],
        "siteSourceCommit": plan["siteSourceCommit"],
        "acceptanceBoundaryReceiptSha256": plan["acceptanceBoundaryReceipt"]["sha256"],
        "replacementLockSha256": plan["replacementLock"]["sha256"],
        "sitePhase1PostDeployIdentitySha256": phase1_pin["sha256"],
        "sitePostPhase2DeploymentIdentitySha256": identity_pin["sha256"],
        "network": NETWORK,
        "ports": PORTS,
        "lease": {
            "operationId": plan["operationId"],
            "planSha256": args.expected_plan_sha256,
            "markerPath": verification["marker"]["path"],
            "markerSha256": verification["marker"]["sha256"],
            "heartbeatPath": verification["heartbeat"]["path"],
            "heartbeatNonce": verification["heartbeat"]["nonce"],
            "watchdogService": verification["watchdog"]["service"],
            "watchdogTimer": verification["watchdog"]["timer"],
            "watchdogUnitSha256": verification["watchdog"]["unitSha256"],
            "watchdogPayloadSha256": verification["watchdog"]["payloadSha256"],
            "armed": True,
            "expiresAtUtc": verification["heartbeat"]["expiresAtUtc"],
        },
        "phase2": {
            "publicIngressClosed": True,
            "siteIndexerSynchronized": True,
            "authorityReady": True,
            "fullLoopActivationReceiptSha256": activation_pin["sha256"],
        },
        "safety": {
            "chainStateMutationAuthorized": False,
            "paidOrPublicActivationAuthorized": False,
            "sourceRestricted": True,
            "loopbackBackendsPreserved": True,
            "forbiddenPortsClosed": True,
        },
        "capturedAtUtc": utc_now(),
    }
    validate_handoff(
        handoff,
        replacement_pin=plan["replacementLock"],
        acceptance_pin=plan["acceptanceBoundaryReceipt"],
        phase1_pin=phase1_pin,
        activation_pin=activation_pin,
        identity_pin=identity_pin,
        chain_environment=plan["selectedDeploymentEnvironment"],
        site_environment=plan["selectedSiteDeploymentEnvironment"],
    )
    write_new(handoff_output, handoff)
    print(f"Phase-2 internal transport handoff captured: {handoff_output} sha256={sha256_file(handoff_output)}")


def command_verify_handoff(args: argparse.Namespace) -> None:
    handoff_pin = checked_pin(args.handoff, args.expected_handoff_sha256, "Phase-2 transport handoff")
    replacement_pin = checked_pin(args.replacement_lock, args.expected_replacement_lock_sha256, "replacement lock")
    acceptance_pin = checked_pin(args.acceptance_boundary_receipt, args.expected_acceptance_boundary_receipt_sha256, "acceptance receipt")
    phase1_pin = checked_pin(args.site_phase1_post_deploy_identity, args.expected_site_phase1_post_deploy_identity_sha256, "site Phase-1 identity")
    activation_pin = checked_pin(args.full_loop_indexer_activation_receipt, args.expected_full_loop_indexer_activation_receipt_sha256, "full-loop activation receipt")
    identity_pin = checked_pin(args.site_post_phase2_deployment_identity, args.expected_site_post_phase2_deployment_identity_sha256, "site post-Phase2 identity")
    chain_environment = file_pin(args.selected_deployment_environment, "chain environment")
    site_environment = file_pin(args.selected_site_deployment_environment, "site environment")
    value = read_json(Path(handoff_pin["path"]), "Phase-2 transport handoff")
    validate_handoff(
        value,
        replacement_pin=replacement_pin,
        acceptance_pin=acceptance_pin,
        phase1_pin=phase1_pin,
        activation_pin=activation_pin,
        identity_pin=identity_pin,
        chain_environment=chain_environment,
        site_environment=site_environment,
    )
    print(json.dumps({"handoffSha256": handoff_pin["sha256"], "replacementLockSha256": replacement_pin["sha256"], "verified": True}, sort_keys=True, separators=(",", ":")))


def add_plan_args(command: argparse.ArgumentParser) -> None:
    command.add_argument("--plan", required=True)
    command.add_argument("--expected-plan-sha256", required=True)


def add_late_artifact_args(command: argparse.ArgumentParser) -> None:
    command.add_argument("--site-phase1-post-deploy-identity", required=True)
    command.add_argument("--expected-site-phase1-post-deploy-identity-sha256", required=True)
    command.add_argument("--full-loop-indexer-activation-receipt", required=True)
    command.add_argument("--expected-full-loop-indexer-activation-receipt-sha256", required=True)
    command.add_argument("--site-post-phase2-deployment-identity", required=True)
    command.add_argument("--expected-site-post-phase2-deployment-identity-sha256", required=True)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    capture = commands.add_parser("capture-plan")
    capture.add_argument("--operation-id", required=True)
    capture.add_argument("--replacement-lock", required=True)
    capture.add_argument("--expected-replacement-lock-sha256", required=True)
    capture.add_argument("--acceptance-boundary-receipt", required=True)
    capture.add_argument("--expected-acceptance-boundary-receipt-sha256", required=True)
    capture.add_argument("--selected-deployment-environment", required=True)
    capture.add_argument("--selected-site-deployment-environment", required=True)
    capture.add_argument("--created-at")
    capture.add_argument("--expires-at")
    capture.add_argument("--output", required=True)
    capture.set_defaults(func=command_capture_plan)
    validate = commands.add_parser("validate")
    add_plan_args(validate)
    validate.set_defaults(func=command_validate)
    for name in ("execute", "renew", "verify", "close"):
        command = commands.add_parser(name)
        add_plan_args(command)
        command.add_argument("--result", required=True)
        command.set_defaults(func=command_remote)
    handoff = commands.add_parser("capture-handoff")
    add_plan_args(handoff)
    add_late_artifact_args(handoff)
    handoff.add_argument("--verification-result", required=True)
    handoff.add_argument("--output", required=True)
    handoff.set_defaults(func=command_capture_handoff)
    verify_handoff = commands.add_parser("verify-handoff")
    verify_handoff.add_argument("--handoff", required=True)
    verify_handoff.add_argument("--expected-handoff-sha256", required=True)
    verify_handoff.add_argument("--replacement-lock", required=True)
    verify_handoff.add_argument("--expected-replacement-lock-sha256", required=True)
    verify_handoff.add_argument("--acceptance-boundary-receipt", required=True)
    verify_handoff.add_argument("--expected-acceptance-boundary-receipt-sha256", required=True)
    add_late_artifact_args(verify_handoff)
    verify_handoff.add_argument("--selected-deployment-environment", required=True)
    verify_handoff.add_argument("--selected-site-deployment-environment", required=True)
    verify_handoff.set_defaults(func=command_verify_handoff)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        args.func(args)
    except (TransportError, OSError, subprocess.SubprocessError) as exc:
        print(f"phase2-internal-transport: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
