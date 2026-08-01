#!/usr/bin/env python3
"""Collect and verify the Nexus V2 Phase-1 acceptance boundary.

The collector is read-only.  It pins one finalized block, the exact frozen
spec-106 Linux runtime bytes/metadata, and every storage query used to derive
the disabled-economic gates and acceptance inventory.  The receipt command is
the one-way Phase-1 -> Phase-2 hand-off: it accepts only a successful
post-cutover coordinator execute decision and a separately hash-pinned closed
ingress observation, then permanently retires automatic archive restoration.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import re
import subprocess
import sys
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Mapping, Sequence


TOOL_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOL_DIR))
import alpha_v2_release as release  # noqa: E402
import final_freeze_runtime_bundle as runtime_bundle  # noqa: E402


SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
HASH256_RE = re.compile(r"^0x[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
RELEASE_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
UTC_RE = re.compile(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$")
HEX_BYTES_RE = re.compile(r"^0x(?:[0-9a-f]{2})*$")
EXPECTED_SPEC_VERSION = 106
MAX_PREFIX_KEYS = 100_000
PAGE_SIZE = 256
CODE_STORAGE_KEY = "0x3a636f6465"

CAPTURE_KEYS = {
    "schemaVersion",
    "kind",
    "releaseId",
    "sourceCommit",
    "observedAtUtc",
    "observedAtFinalizedBlock",
    "genesisHash",
    "runtime",
    "storage",
}
RUNTIME_KEYS = {
    "specVersion",
    "transactionVersion",
    "stateVersion",
    "runtimeCodeHex",
    "runtimeCodeSha256",
    "runtimeMetadataScaleHex",
    "runtimeMetadataScaleSha256",
    "runtimeMetadataJsonSha256",
    "runtimeBundleManifestSha256",
}
STORAGE_KEYS = {"plain", "exactMaps", "prefixes"}
QUERY_KEYS = {"pallet", "storage", "key", "value"}
PREFIX_CAPTURE_KEYS = {"pallet", "storage", "prefix", "keys", "values"}

INGRESS_KEYS = {
    "schemaVersion",
    "kind",
    "releaseId",
    "sourceCommit",
    "genesisHash",
    "observedAtFinalizedBlock",
    "observedAtUtc",
    "mode",
    "components",
    "blockProductionContinues",
    "paidOrPublicActivationAuthorized",
}
INGRESS_COMPONENT_KEYS = {"chain-media", "site-indexer"}
CHAIN_INGRESS_KEYS = {
    "publicRpcWriteIngressClosed",
    "authorityOperatorIngressClosed",
    "gameplaySessionIngressClosed",
    "componentEvidenceSha256",
}
SITE_INGRESS_KEYS = {
    "webMutationIngressClosed",
    "indexerMutationIngressClosed",
    "componentEvidenceSha256",
}

RECEIPT_KEYS = {
    "schemaVersion",
    "kind",
    "releaseId",
    "sourceCommit",
    "genesisHash",
    "runtimeCodeSha256",
    "runtimeMetadataScaleSha256",
    "observedAtFinalizedBlock",
    "acceptanceBoundaryCaptureSha256",
    "economicGatesSha256",
    "acceptanceInventorySha256",
    "postCutoverObservationSha256",
    "coordinatorExecuteEvidenceSha256",
    "coordinatorDecision",
    "ingressClosedEvidenceSha256",
    "ingressMode",
    "phase1SmokePassed",
    "automaticRestorePermanentlyDisabled",
    "operatorV2WriteScope",
    "createdAtUtc",
}
COORDINATOR_EVIDENCE_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "planSha256",
    "releaseId",
    "sourceCommit",
    "genesisHash",
    "runtimeCodeSha256",
    "runtimeMetadataScaleSha256",
    "componentSourceCommits",
    "decision",
    "postCutoverSmokePassed",
    "automaticRestorePerformed",
    "postAcceptanceContainmentPerformed",
    "finalBackupManifestSha256",
    "restoreEvidenceSha256",
    "postCutoverObservationSha256",
    "acceptanceBoundaryCaptureSha256",
    "ingressClosedEvidenceSha256",
    "economicGatesSha256",
    "acceptanceInventorySha256",
    "observedAtFinalizedBlock",
    "nonzeroAcceptanceAssets",
    "componentMarkerSha256",
    "completedAtUtc",
}
OBSERVATION_KEYS = {
    "schemaVersion",
    "kind",
    "releaseId",
    "sourceCommit",
    "componentSourceCommits",
    "observedAtFinalizedBlock",
    "observedAtUtc",
    "writeBarrier",
    "acceptanceBoundaryCaptureSha256",
    "ingressClosedEvidenceSha256",
    "economicGatesSha256",
    "acceptanceInventorySha256",
}
WRITE_BARRIER_KEYS = {
    "mode",
    "chainWritesPaused",
    "authorityResultsPaused",
    "webMutationsPaused",
    "gameplaySessionIngressPaused",
    "inventoryObservedAfterPause",
    "pausedAtUtc",
    "stabilityWindowSeconds",
    "evidenceSha256",
}
PHASE1_EXECUTE_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "releaseId",
    "sourceCommit",
    "siteSourceCommit",
    "siteReleaseVersion",
    "siteCandidateUsableForExecute",
    "genesisHash",
    "driverSha256",
    "inputsSha256",
    "executeTokenSha256",
    "observedAtFinalizedBlock",
    "acceptanceBoundaryCaptureSha256",
    "economicGatesSha256",
    "acceptanceInventorySha256",
    "chainMediaComponentEvidenceSha256",
    "siteIndexerComponentEvidenceSha256",
    "ingressClosedEvidenceSha256",
    "stabilityWindowSeconds",
    "stabilityWindowElapsedMilliseconds",
    "allExternalWriteIngressClosed",
    "blockProductionContinues",
    "authorityLocalServicePreserved",
    "readOnlySiteStackPreserved",
    "automaticReopenAuthorized",
    "paidOrPublicActivationAuthorized",
    "completedAtUtc",
}
PHASE1_CHAIN_COMPONENT_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "releaseId",
    "sourceCommit",
    "genesisHash",
    "observedAtUtc",
    "observedAtFinalizedBlock",
    "driverSha256",
    "inputsSha256",
    "executeTokenSha256",
    "acceptanceBoundaryCaptureSha256",
    "closureObservationSha256",
    "postWindowObservationSha256",
    "remoteMarkerSha256",
    "firewallStatusSha256",
    "stabilityWindowSeconds",
    "stabilityWindowElapsedMilliseconds",
    "services",
    "trustedObservation",
    "checks",
    "automaticReopenAuthorized",
    "paidOrPublicActivationAuthorized",
}
PHASE1_SITE_COMPONENT_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "releaseId",
    "sourceCommit",
    "siteSourceCommit",
    "genesisHash",
    "observedAtUtc",
    "observedAtFinalizedBlock",
    "driverSha256",
    "inputsSha256",
    "executeTokenSha256",
    "acceptanceBoundaryCaptureSha256",
    "closureObservationSha256",
    "postWindowObservationSha256",
    "remoteMarkerSha256",
    "firewallStatusSha256",
    "listenersSha256",
    "readOnlyCaddyfileSha256",
    "originalCaddyfileSha256",
    "services",
    "localReadiness",
    "routeStatus",
    "checks",
    "automaticReopenAuthorized",
    "paidOrPublicActivationAuthorized",
}
COORDINATOR_PLAN_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "releaseId",
    "sourceCommit",
    "genesisHash",
    "runtimeCodeSha256",
    "runtimeMetadataScaleSha256",
    "runtimeBundleManifestSha256",
    "freshResetReadinessSha256",
    "finalBackupManifestSha256",
    "restoreEvidenceSha256",
    "postCutoverObservationSha256",
    "acceptanceBoundaryCaptureSha256",
    "ingressClosedEvidenceSha256",
    "coordinatorSha256",
    "maxObservationAgeSeconds",
    "automaticRestoreApproved",
    "paidOrPublicActivationAuthorized",
    "createdAtUtc",
    "expiresAtUtc",
    "components",
}
FINAL_MARKER_KEYS = {
    "schemaVersion",
    "kind",
    "operationId",
    "planSha256",
    "evidencePath",
    "evidenceSha256",
    "completedAtUtc",
}
OPERATOR_SCOPE = {
    "boundedManualAdminAlphaAccessGrant": True,
    "alphaAccessModeMustRemainEnforced": True,
    "alphaAccessModeOpen": False,
    "authorityRegistration": True,
    "practiceRewardPolicy": True,
    "proofOnlyAbilityDeathmatchTrainingPolicy": True,
    "proofPolicyDeactivationRequired": True,
    "canonicalPolicySeedingBeforeProof": False,
    "v2SessionAuthorization": True,
    "v2ResultSettlement": True,
    "economicallyValuedRewards": False,
    "paidOrPublicActivation": False,
}

# Storage values with simple SCALE codecs.
PLAIN_QUERIES = {
    "tcgNextCardIdV2": ("EterraTCG", "NextCardIdV2"),
    "tcgNextPackCreditIdV2": ("EterraTCG", "NextPackCreditIdV2"),
    "tcgLegacyCreationSealed": ("EterraTCG", "LegacyCreationSealedV16"),
    "randomnessCurrentMode": ("EterraRandomness", "CurrentMode"),
    "randomnessCryptographyReviewApproved": (
        "EterraRandomness",
        "CryptographyReviewApproved",
    ),
    "creaturesNextEntityId": ("EterraCreatures", "NextEntityId"),
    "magicNextPrismSpellId": ("EterraMagic", "NextPrismSpellId"),
    "gameResultsNextSessionId": ("EterraGameResults", "NextSessionId"),
    "gameAuthorityNextGameId": ("EterraGameAuthority", "NextGameId"),
}

ENUM_MAP_QUERIES = {
    **{
        f"tcgFeature.{name}": ("EterraTCG", "V2FeatureEnabled", name)
        for name in ("Packs", "Conversion", "Ranked", "MythicalAscension")
    },
    **{
        f"economyPaused.{name}": ("EterraEconomy", "PausedDomains", name)
        for name in (
            "TicketEarning",
            "TicketTransfers",
            "TicketRedemption",
            "RandomVending",
            "FeaturedVending",
            "PackCreditRedemptionV2",
        )
    },
}

# Every prefix is enumerated and every extant value is captured at the same
# block.  Counting keys is intentionally conservative for ValueQuery maps: a
# retained zero/default record still closes automatic restore.
PREFIX_QUERIES = {
    "tcgCardsV2": ("EterraTCG", "CardsV2"),
    "tcgPackCreditsV2": ("EterraTCG", "PackCreditsV2"),
    "tcgPendingPackOpeningsV2": ("EterraTCG", "PendingPackOpeningsV2"),
    "tcgPackOpeningReceiptsV2": ("EterraTCG", "PackOpeningRequestReceiptsV2"),
    "tcgConversionTombstones": ("EterraTCG", "CardConversionTombstones"),
    "tcgCompetitiveTeamsV2": ("EterraTCG", "CompetitiveTeamsV2"),
    "creaturesEntities": ("EterraCreatures", "Entities"),
    "magicEssenceBalances": ("EterraMagic", "EssenceBalances"),
    "magicSpellChargeBalances": ("EterraMagic", "SpellChargeBalances"),
    "magicPrismSpells": ("EterraMagic", "PrismSpells"),
    "magicProcessedResults": ("EterraMagic", "ProcessedMagicResults"),
    "gameResultsRewardPolicyActivation": (
        "EterraGameResults",
        "RewardPolicyActivation",
    ),
    "gameResultsSessions": ("EterraGameResults", "Sessions"),
    "gameResultsProcessedResults": ("EterraGameResults", "ProcessedResults"),
    "gameResultsSettledSessions": ("EterraGameResults", "SettledSessions"),
    "gameResultsSealedEpochs": ("EterraGameResults", "SealedResultEpochs"),
    "gamerPlayerAdvancement": ("EterraGamer", "V2PlayerAdvancementXp"),
    "gamerPackProgress": ("EterraGamer", "V2PackProgress"),
    "gamerLifetimeXp": ("EterraGamer", "V2LifetimePlayerXp"),
    "gameAuthorityGames": ("EterraGameAuthority", "Games"),
    "gameAuthorityActivePlayers": ("EterraGameAuthority", "ActiveGameByPlayer"),
    "gameAuthorityEliminations": ("EterraGameAuthority", "Eliminations"),
    "gameAuthorityEndCommands": ("EterraGameAuthority", "ProcessedEndCommands"),
    "gameAuthorityEliminationEvents": (
        "EterraGameAuthority",
        "ProcessedEliminationEvents",
    ),
}


class BoundaryError(RuntimeError):
    """The acceptance-boundary contract failed closed."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise BoundaryError(message)


def duplicate_rejecting_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        require(key not in result, f"duplicate JSON field: {key}")
        result[key] = value
    return result


def read_json(path: Path, label: str) -> dict[str, Any]:
    require(path.is_file() and not path.is_symlink(), f"{label} must be a regular file")
    try:
        value = json.loads(
            path.read_text(encoding="utf-8"),
            object_pairs_hook=duplicate_rejecting_object,
        )
    except (OSError, json.JSONDecodeError) as exc:
        raise BoundaryError(f"invalid {label}: {path}") from exc
    require(isinstance(value, dict), f"{label} root must be an object")
    return value


def exact_keys(value: Any, expected: set[str], label: str) -> Mapping[str, Any]:
    require(isinstance(value, dict), f"{label} must be an object")
    require(set(value) == expected, f"{label} fields do not match the closed schema")
    return value


def canonical_bytes(value: Mapping[str, Any]) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")


def write_new_json(path: Path, value: Mapping[str, Any], mode: int = 0o440) -> None:
    require(not path.exists(), f"refusing to overwrite immutable output: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, mode)
    with os.fdopen(descriptor, "wb") as handle:
        handle.write(canonical_bytes(value))


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def ensure_sha256(value: Any, label: str) -> str:
    require(isinstance(value, str) and bool(SHA256_RE.fullmatch(value)), f"invalid {label}")
    return value


def ensure_hash256(value: Any, label: str) -> str:
    require(isinstance(value, str) and bool(HASH256_RE.fullmatch(value)), f"invalid {label}")
    return value


def ensure_commit(value: Any) -> str:
    require(isinstance(value, str) and bool(COMMIT_RE.fullmatch(value)), "invalid source commit")
    return value


def ensure_release(value: Any) -> str:
    require(isinstance(value, str) and bool(RELEASE_RE.fullmatch(value)), "invalid release ID")
    return value


def parse_utc(value: Any, label: str) -> dt.datetime:
    require(
        isinstance(value, str) and bool(UTC_RE.fullmatch(value)),
        f"{label} must be canonical YYYY-MM-DDTHH:MM:SSZ",
    )
    try:
        parsed = dt.datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ").replace(
            tzinfo=dt.timezone.utc
        )
    except ValueError as exc:
        raise BoundaryError(f"invalid {label}") from exc
    return parsed


def utc_now() -> str:
    return (
        dt.datetime.now(dt.timezone.utc)
        .replace(microsecond=0)
        .isoformat()
        .replace("+00:00", "Z")
    )


def hex_bytes(value: Any, label: str) -> bytes:
    require(
        isinstance(value, str) and bool(HEX_BYTES_RE.fullmatch(value)),
        f"invalid/noncanonical {label}",
    )
    try:
        raw = bytes.fromhex(value[2:])
    except ValueError as exc:
        raise BoundaryError(f"invalid {label}") from exc
    return raw


def rotate_left(value: int, count: int) -> int:
    return ((value << count) | (value >> (64 - count))) & 0xFFFFFFFFFFFFFFFF


def xxh64(value: bytes, seed: int) -> int:
    """Dependency-free XXH64 used by FRAME's Twox128 storage prefixes."""

    prime1 = 11400714785074694791
    prime2 = 14029467366897019727
    prime3 = 1609587929392839161
    prime4 = 9650029242287828579
    prime5 = 2870177450012600261
    mask = 0xFFFFFFFFFFFFFFFF

    def round64(accumulator: int, lane: int) -> int:
        accumulator = (accumulator + lane * prime2) & mask
        accumulator = rotate_left(accumulator, 31)
        return (accumulator * prime1) & mask

    length = len(value)
    offset = 0
    if length >= 32:
        v1 = (seed + prime1 + prime2) & mask
        v2 = (seed + prime2) & mask
        v3 = seed & mask
        v4 = (seed - prime1) & mask
        while offset <= length - 32:
            v1 = round64(v1, int.from_bytes(value[offset : offset + 8], "little"))
            v2 = round64(v2, int.from_bytes(value[offset + 8 : offset + 16], "little"))
            v3 = round64(v3, int.from_bytes(value[offset + 16 : offset + 24], "little"))
            v4 = round64(v4, int.from_bytes(value[offset + 24 : offset + 32], "little"))
            offset += 32
        result = (
            rotate_left(v1, 1)
            + rotate_left(v2, 7)
            + rotate_left(v3, 12)
            + rotate_left(v4, 18)
        ) & mask
        for lane in (v1, v2, v3, v4):
            result ^= round64(0, lane)
            result = (result * prime1 + prime4) & mask
    else:
        result = (seed + prime5) & mask
    result = (result + length) & mask
    while offset <= length - 8:
        lane = round64(0, int.from_bytes(value[offset : offset + 8], "little"))
        result ^= lane
        result = (rotate_left(result, 27) * prime1 + prime4) & mask
        offset += 8
    if offset <= length - 4:
        result ^= (int.from_bytes(value[offset : offset + 4], "little") * prime1) & mask
        result &= mask
        result = (rotate_left(result, 23) * prime2 + prime3) & mask
        offset += 4
    while offset < length:
        result ^= (value[offset] * prime5) & mask
        result &= mask
        result = (rotate_left(result, 11) * prime1) & mask
        offset += 1
    result ^= result >> 33
    result = (result * prime2) & mask
    result ^= result >> 29
    result = (result * prime3) & mask
    result ^= result >> 32
    return result & mask


def twox128(value: str) -> bytes:
    encoded = value.encode("utf-8")
    return xxh64(encoded, 0).to_bytes(8, "little") + xxh64(encoded, 1).to_bytes(8, "little")


def storage_prefix(pallet_prefix: str, storage_name: str) -> bytes:
    return twox128(pallet_prefix) + twox128(storage_name)


def blake2_128_concat(value: bytes) -> bytes:
    return hashlib.blake2b(value, digest_size=16).digest() + value


@dataclass(frozen=True)
class Metadata:
    value: Mapping[str, Any]
    pallets: Mapping[str, Mapping[str, Any]]
    types: Mapping[int, Mapping[str, Any]]

    @classmethod
    def from_bytes(cls, payload: bytes) -> "Metadata":
        try:
            raw = json.loads(payload)
            value = raw[1]["V15"]
            pallets = {item["name"]: item for item in value["pallets"]}
            types = {item["id"]: item["type"] for item in value["types"]["types"]}
        except (KeyError, TypeError, ValueError, json.JSONDecodeError) as exc:
            raise BoundaryError("decoded runtime metadata is not the expected V15 JSON") from exc
        return cls(value=value, pallets=pallets, types=types)

    def entry(self, pallet_name: str, storage_name: str) -> Mapping[str, Any]:
        require(pallet_name in self.pallets, f"metadata pallet missing: {pallet_name}")
        pallet = self.pallets[pallet_name]
        storage = pallet.get("storage")
        require(isinstance(storage, dict), f"metadata pallet has no storage: {pallet_name}")
        matches = [item for item in storage.get("entries", []) if item.get("name") == storage_name]
        require(len(matches) == 1, f"metadata storage missing/ambiguous: {pallet_name}.{storage_name}")
        return matches[0]

    def prefix(self, pallet_name: str, storage_name: str) -> bytes:
        pallet = self.pallets[pallet_name]
        storage = pallet.get("storage")
        require(isinstance(storage, dict), f"metadata pallet has no storage: {pallet_name}")
        self.entry(pallet_name, storage_name)
        return storage_prefix(str(storage["prefix"]), storage_name)

    def enum_variant(self, type_id: int, name: str) -> int:
        try:
            variants = self.types[type_id]["def"]["variant"]["variants"]
        except (KeyError, TypeError) as exc:
            raise BoundaryError(f"metadata type {type_id} is not an enum") from exc
        matches = [item for item in variants if item.get("name") == name]
        require(len(matches) == 1, f"metadata enum variant missing/ambiguous: {name}")
        require(not matches[0].get("fields"), f"enum variant has unsupported fields: {name}")
        index = matches[0].get("index")
        require(isinstance(index, int) and 0 <= index <= 255, f"invalid enum index: {name}")
        return index

    def exact_enum_map_key(self, pallet_name: str, storage_name: str, variant: str) -> bytes:
        entry = self.entry(pallet_name, storage_name)
        map_type = entry.get("ty", {}).get("Map")
        require(isinstance(map_type, dict), f"storage is not a map: {pallet_name}.{storage_name}")
        require(map_type.get("hashers") == ["Blake2_128Concat"], "enum map must use Blake2_128Concat")
        encoded = bytes([self.enum_variant(int(map_type["key"]), variant)])
        return self.prefix(pallet_name, storage_name) + blake2_128_concat(encoded)

    def default(self, pallet_name: str, storage_name: str) -> bytes:
        value = self.entry(pallet_name, storage_name).get("default")
        require(isinstance(value, list) and all(isinstance(item, int) for item in value), "invalid storage default")
        return bytes(value)


@dataclass(frozen=True)
class RuntimeArtifacts:
    metadata: Metadata
    metadata_scale: bytes
    metadata_json: bytes
    bundle_manifest_sha256: str


def load_runtime_artifacts(root: Path, expected_manifest_sha256: str) -> RuntimeArtifacts:
    try:
        runtime_bundle.verify_final_freeze_runtime_bundle(root, expected_manifest_sha256)
    except runtime_bundle.FreezeBundleError as exc:
        raise BoundaryError(str(exc)) from exc
    scale_path = root / "runtime-metadata.scale"
    json_path = root / "runtime-metadata.json"
    metadata_scale = scale_path.read_bytes()
    metadata_json = json_path.read_bytes()
    pins = runtime_bundle.PRODUCTION_PINS
    require(sha256_bytes(metadata_scale) == pins.metadata_scale_sha256, "runtime metadata SCALE pin mismatch")
    require(sha256_bytes(metadata_json) == pins.metadata_json_sha256, "runtime metadata JSON pin mismatch")
    return RuntimeArtifacts(
        metadata=Metadata.from_bytes(metadata_json),
        metadata_scale=metadata_scale,
        metadata_json=metadata_json,
        bundle_manifest_sha256=expected_manifest_sha256,
    )


class Rpc:
    def __init__(self, url: str, timeout: int = 30) -> None:
        require(url.startswith("http://") or url.startswith("https://"), "RPC URL must use HTTP(S)")
        self.url = url
        self.timeout = timeout
        self.request_id = 0

    def call(self, method: str, params: list[Any]) -> Any:
        self.request_id += 1
        body = json.dumps(
            {"id": self.request_id, "jsonrpc": "2.0", "method": method, "params": params},
            separators=(",", ":"),
        ).encode("utf-8")
        request = urllib.request.Request(
            self.url,
            data=body,
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                payload = response.read()
        except (OSError, urllib.error.URLError) as exc:
            raise BoundaryError(f"RPC request failed: {method}") from exc
        try:
            value = json.loads(payload, object_pairs_hook=duplicate_rejecting_object)
        except json.JSONDecodeError as exc:
            raise BoundaryError(f"RPC returned invalid JSON: {method}") from exc
        exact_keys(value, {"id", "jsonrpc", "result"}, f"RPC {method} response")
        require(value["jsonrpc"] == "2.0" and value["id"] == self.request_id, f"RPC identity mismatch: {method}")
        return value["result"]

    def keys(self, prefix: str, block_hash: str) -> list[str]:
        result: list[str] = []
        start: str | None = None
        while True:
            params: list[Any] = [prefix, PAGE_SIZE, start, block_hash]
            page = self.call("state_getKeysPaged", params)
            require(isinstance(page, list), "state_getKeysPaged result must be an array")
            require(all(isinstance(key, str) for key in page), "state_getKeysPaged returned a non-string key")
            require(len(page) <= PAGE_SIZE, "state_getKeysPaged exceeded the requested page size")
            if not page:
                break
            require(page == sorted(page) and len(set(page)) == len(page), "storage-key page is not strictly unique/sorted")
            require(all(key.startswith(prefix) for key in page), "storage-key page escaped its prefix")
            if result:
                require(page[0] > result[-1], "storage-key pagination did not advance")
            result.extend(page)
            require(len(result) <= MAX_PREFIX_KEYS, "storage prefix exceeds the acceptance collector bound")
            if len(page) < PAGE_SIZE:
                break
            start = page[-1]
        return result


def normalize_storage_value(value: Any, label: str, *, optional: bool = True) -> str | None:
    if value is None:
        require(optional, f"missing required storage value: {label}")
        return None
    raw = hex_bytes(value, label)
    return "0x" + raw.hex()


def collect_capture(
    rpc: Rpc,
    artifacts: RuntimeArtifacts,
    release_id: str,
    source_commit: str,
    expected_genesis_hash: str,
    observed_at: str,
) -> dict[str, Any]:
    ensure_release(release_id)
    ensure_commit(source_commit)
    ensure_hash256(expected_genesis_hash, "expected genesis hash")
    parse_utc(observed_at, "observedAtUtc")
    head = ensure_hash256(rpc.call("chain_getFinalizedHead", []), "finalized head")
    header = rpc.call("chain_getHeader", [head])
    require(isinstance(header, dict) and isinstance(header.get("number"), str), "invalid finalized header")
    try:
        block_number = int(header["number"], 16)
    except ValueError as exc:
        raise BoundaryError("invalid finalized block number") from exc
    require(
        ensure_hash256(rpc.call("chain_getBlockHash", [block_number]), "block hash at number") == head,
        "finalized block hash/number round trip mismatch",
    )
    genesis_hash = ensure_hash256(rpc.call("chain_getBlockHash", [0]), "genesis hash")
    require(genesis_hash == expected_genesis_hash, "RPC genesis hash does not match the release target")
    version = rpc.call("state_getRuntimeVersion", [head])
    require(isinstance(version, dict), "invalid runtime version")
    spec_version = version.get("specVersion")
    transaction_version = version.get("transactionVersion")
    state_version = version.get("stateVersion")
    for name, value in (
        ("specVersion", spec_version),
        ("transactionVersion", transaction_version),
        ("stateVersion", state_version),
    ):
        require(isinstance(value, int) and not isinstance(value, bool) and value >= 0, f"invalid runtime {name}")
    require(spec_version == EXPECTED_SPEC_VERSION, "RPC runtime is not spec 106")
    code_hex = normalize_storage_value(
        rpc.call("state_getStorage", [CODE_STORAGE_KEY, head]),
        "runtime :code",
        optional=False,
    )
    assert code_hex is not None
    code_hash = sha256_bytes(hex_bytes(code_hex, "runtime :code"))
    require(code_hash == runtime_bundle.PRODUCTION_PINS.production_wasm_sha256, "deployed runtime :code is not the frozen production Wasm")
    metadata_hex = normalize_storage_value(
        rpc.call("state_getMetadata", [head]),
        "runtime metadata",
        optional=False,
    )
    assert metadata_hex is not None
    metadata_bytes = hex_bytes(metadata_hex, "runtime metadata")
    require(metadata_bytes == artifacts.metadata_scale, "RPC metadata is not byte-identical to the frozen Linux bundle")

    plain: dict[str, Any] = {}
    for alias, (pallet, storage) in sorted(PLAIN_QUERIES.items()):
        key = "0x" + artifacts.metadata.prefix(pallet, storage).hex()
        plain[alias] = {
            "pallet": pallet,
            "storage": storage,
            "key": key,
            "value": normalize_storage_value(rpc.call("state_getStorage", [key, head]), alias),
        }

    exact_maps: dict[str, Any] = {}
    for alias, (pallet, storage, variant) in sorted(ENUM_MAP_QUERIES.items()):
        key = "0x" + artifacts.metadata.exact_enum_map_key(pallet, storage, variant).hex()
        exact_maps[alias] = {
            "pallet": pallet,
            "storage": storage,
            "key": key,
            "value": normalize_storage_value(rpc.call("state_getStorage", [key, head]), alias),
        }

    prefixes: dict[str, Any] = {}
    for alias, (pallet, storage) in sorted(PREFIX_QUERIES.items()):
        prefix = "0x" + artifacts.metadata.prefix(pallet, storage).hex()
        keys = rpc.keys(prefix, head)
        values: dict[str, str] = {}
        for key in keys:
            value = normalize_storage_value(
                rpc.call("state_getStorage", [key, head]),
                f"{alias}:{key}",
                optional=False,
            )
            assert value is not None
            values[key] = value
        prefixes[alias] = {
            "pallet": pallet,
            "storage": storage,
            "prefix": prefix,
            "keys": keys,
            "values": values,
        }

    return {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-acceptance-boundary-rpc-capture",
        "releaseId": release_id,
        "sourceCommit": source_commit,
        "observedAtUtc": observed_at,
        "observedAtFinalizedBlock": {"number": block_number, "hash": head},
        "genesisHash": genesis_hash,
        "runtime": {
            "specVersion": spec_version,
            "transactionVersion": transaction_version,
            "stateVersion": state_version,
            "runtimeCodeHex": code_hex,
            "runtimeCodeSha256": code_hash,
            "runtimeMetadataScaleHex": metadata_hex,
            "runtimeMetadataScaleSha256": sha256_bytes(metadata_bytes),
            "runtimeMetadataJsonSha256": sha256_bytes(artifacts.metadata_json),
            "runtimeBundleManifestSha256": artifacts.bundle_manifest_sha256,
        },
        "storage": {"plain": plain, "exactMaps": exact_maps, "prefixes": prefixes},
    }


def captured_value(
    item: Mapping[str, Any],
    metadata: Metadata,
    *,
    default: tuple[str, str],
) -> bytes:
    value = item["value"]
    return metadata.default(*default) if value is None else hex_bytes(value, "captured storage value")


def decode_bool(value: bytes, label: str) -> bool:
    require(value in {b"\x00", b"\x01"}, f"{label} is not a SCALE bool")
    return value == b"\x01"


def decode_u64(value: bytes, label: str) -> int:
    require(len(value) == 8, f"{label} is not a SCALE u64")
    return int.from_bytes(value, "little")


def validate_capture(value: Mapping[str, Any], artifacts: RuntimeArtifacts) -> dict[str, Any]:
    exact_keys(value, CAPTURE_KEYS, "acceptance-boundary capture")
    require(value["schemaVersion"] == 1, "acceptance-boundary capture schema mismatch")
    require(value["kind"] == "nexus-v2-private-alpha-acceptance-boundary-rpc-capture", "acceptance-boundary capture kind mismatch")
    release_id = ensure_release(value["releaseId"])
    source_commit = ensure_commit(value["sourceCommit"])
    parse_utc(value["observedAtUtc"], "capture observedAtUtc")
    block_number, block_hash = release.finalized_block(value["observedAtFinalizedBlock"], "capture")
    require(block_number > 0, "acceptance-boundary capture may not use genesis block zero")
    genesis_hash = ensure_hash256(value["genesisHash"], "capture genesis hash")
    runtime = exact_keys(value["runtime"], RUNTIME_KEYS, "capture runtime")
    require(runtime["specVersion"] == EXPECTED_SPEC_VERSION, "capture runtime spec mismatch")
    for name in ("transactionVersion", "stateVersion"):
        require(isinstance(runtime[name], int) and not isinstance(runtime[name], bool) and runtime[name] >= 0, f"invalid capture runtime {name}")
    code = hex_bytes(runtime["runtimeCodeHex"], "capture runtime code")
    require(sha256_bytes(code) == runtime["runtimeCodeSha256"], "capture runtime code hash mismatch")
    require(runtime["runtimeCodeSha256"] == runtime_bundle.PRODUCTION_PINS.production_wasm_sha256, "capture runtime code is not frozen production")
    metadata_scale = hex_bytes(runtime["runtimeMetadataScaleHex"], "capture runtime metadata")
    require(metadata_scale == artifacts.metadata_scale, "capture metadata SCALE differs from frozen bundle")
    require(runtime["runtimeMetadataScaleSha256"] == runtime_bundle.PRODUCTION_PINS.metadata_scale_sha256, "capture metadata SCALE pin mismatch")
    require(runtime["runtimeMetadataJsonSha256"] == runtime_bundle.PRODUCTION_PINS.metadata_json_sha256, "capture metadata JSON pin mismatch")
    require(runtime["runtimeBundleManifestSha256"] == artifacts.bundle_manifest_sha256, "capture runtime bundle pin mismatch")

    storage = exact_keys(value["storage"], STORAGE_KEYS, "capture storage")
    plain = exact_keys(storage["plain"], set(PLAIN_QUERIES), "capture plain queries")
    for alias, (pallet, storage_name) in PLAIN_QUERIES.items():
        item = exact_keys(plain[alias], QUERY_KEYS, f"capture query {alias}")
        require((item["pallet"], item["storage"]) == (pallet, storage_name), f"capture query identity mismatch: {alias}")
        require(item["key"] == "0x" + artifacts.metadata.prefix(pallet, storage_name).hex(), f"capture query key mismatch: {alias}")
        if item["value"] is not None:
            hex_bytes(item["value"], f"capture query {alias}")

    exact_maps = exact_keys(storage["exactMaps"], set(ENUM_MAP_QUERIES), "capture exact-map queries")
    for alias, (pallet, storage_name, variant) in ENUM_MAP_QUERIES.items():
        item = exact_keys(exact_maps[alias], QUERY_KEYS, f"capture query {alias}")
        require((item["pallet"], item["storage"]) == (pallet, storage_name), f"capture query identity mismatch: {alias}")
        expected_key = "0x" + artifacts.metadata.exact_enum_map_key(pallet, storage_name, variant).hex()
        require(item["key"] == expected_key, f"capture enum-map key mismatch: {alias}")
        if item["value"] is not None:
            hex_bytes(item["value"], f"capture query {alias}")

    prefixes = exact_keys(storage["prefixes"], set(PREFIX_QUERIES), "capture prefix queries")
    for alias, (pallet, storage_name) in PREFIX_QUERIES.items():
        item = exact_keys(prefixes[alias], PREFIX_CAPTURE_KEYS, f"capture prefix {alias}")
        require((item["pallet"], item["storage"]) == (pallet, storage_name), f"capture prefix identity mismatch: {alias}")
        expected_prefix = "0x" + artifacts.metadata.prefix(pallet, storage_name).hex()
        require(item["prefix"] == expected_prefix, f"capture prefix mismatch: {alias}")
        keys = item["keys"]
        values = item["values"]
        require(isinstance(keys, list) and keys == sorted(keys) and len(keys) == len(set(keys)), f"capture prefix keys invalid: {alias}")
        require(len(keys) <= MAX_PREFIX_KEYS, f"capture prefix exceeds bound: {alias}")
        require(isinstance(values, dict) and set(values) == set(keys), f"capture prefix value set mismatch: {alias}")
        for key in keys:
            require(isinstance(key, str) and key.startswith(expected_prefix), f"capture key escaped prefix: {alias}")
            hex_bytes(values[key], f"capture prefix value {alias}")

    return {
        "releaseId": release_id,
        "sourceCommit": source_commit,
        "observedAtUtc": value["observedAtUtc"],
        "blockNumber": block_number,
        "blockHash": block_hash,
        "genesisHash": genesis_hash,
        "runtimeCodeSha256": runtime["runtimeCodeSha256"],
        "runtimeMetadataScaleSha256": runtime["runtimeMetadataScaleSha256"],
    }


def prefix_count(capture: Mapping[str, Any], alias: str) -> int:
    return len(capture["storage"]["prefixes"][alias]["keys"])


def plain_u64(capture: Mapping[str, Any], metadata: Metadata, alias: str) -> int:
    pallet, storage = PLAIN_QUERIES[alias]
    return decode_u64(
        captured_value(capture["storage"]["plain"][alias], metadata, default=(pallet, storage)),
        alias,
    )


def plain_bool(capture: Mapping[str, Any], metadata: Metadata, alias: str) -> bool:
    pallet, storage = PLAIN_QUERIES[alias]
    return decode_bool(
        captured_value(capture["storage"]["plain"][alias], metadata, default=(pallet, storage)),
        alias,
    )


def enum_map_bool(capture: Mapping[str, Any], metadata: Metadata, alias: str) -> bool:
    pallet, storage, _ = ENUM_MAP_QUERIES[alias]
    return decode_bool(
        captured_value(capture["storage"]["exactMaps"][alias], metadata, default=(pallet, storage)),
        alias,
    )


def disabled_gates(capture: Mapping[str, Any], metadata: Metadata) -> dict[str, Any]:
    mode_bytes = captured_value(
        capture["storage"]["plain"]["randomnessCurrentMode"],
        metadata,
        default=PLAIN_QUERIES["randomnessCurrentMode"],
    )
    require(len(mode_bytes) == 1, "randomness mode is not a fieldless SCALE enum")
    mode_names = {0: "Disabled", 1: "DeterministicPrivateAlpha", 2: "DrandQuicknet"}
    require(mode_bytes[0] in mode_names, "unknown randomness mode")
    mode = mode_names[mode_bytes[0]]
    # The recoverable boundary is stricter than later valueless alpha play: no
    # deterministic seed or active reward policy may exist yet.
    require(mode == "Disabled", "Phase-1 boundary requires disabled randomness")
    require(
        not plain_bool(capture, metadata, "randomnessCryptographyReviewApproved"),
        "cryptography review is unexpectedly activated",
    )
    activation = capture["storage"]["prefixes"]["gameResultsRewardPolicyActivation"]
    require(
        all(not decode_bool(hex_bytes(value, "reward policy activation"), "reward policy activation") for value in activation["values"].values()),
        "Phase-1 boundary has an active reward policy",
    )
    features = {
        name: enum_map_bool(capture, metadata, f"tcgFeature.{name}")
        for name in ("Packs", "Conversion", "Ranked", "MythicalAscension")
    }
    require(not any(features.values()), "Phase-1 boundary has an enabled V2 TCG feature")
    require(plain_bool(capture, metadata, "tcgLegacyCreationSealed"), "legacy TCG creation is not sealed")
    pause_names = (
        "TicketEarning",
        "TicketTransfers",
        "TicketRedemption",
        "RandomVending",
        "FeaturedVending",
        "PackCreditRedemptionV2",
    )
    paused = {name: enum_map_bool(capture, metadata, f"economyPaused.{name}") for name in pause_names}
    require(all(paused.values()), "an EterraEconomy domain is not paused")
    identity = capture["observedAtFinalizedBlock"]
    return {
        "schemaVersion": 1,
        "kind": release.POST_V16_ECONOMIC_GATE_KIND,
        "releaseId": capture["releaseId"],
        "sourceCommit": capture["sourceCommit"],
        "observedAtFinalizedBlock": identity,
        "tcg": {"features": features, "legacyCreationSealed": True},
        "randomness": {
            "mode": mode,
            "privateAlphaSeedRecorded": False,
            "cryptographyReviewApproved": False,
            "drandQuicknetEnabled": False,
            "productionEconomicUseAllowed": False,
        },
        "gameResults": {
            "activeProductionPolicyCount": 0,
            "allAlphaPoliciesPracticeOnlyOrValuelessTraining": True,
        },
        "issuance": {
            "trainingPackCreditRejectsProduction": True,
            "paidV2IssuanceCallAvailable": False,
        },
        "reforge": {"dispatchableAvailable": False},
        "magic": {"seedTrainingOnly": True, "productionTransferEnabled": False},
        "legacyEconomy": {
            "marketplaceEnabled": False,
            "purchaseEnabled": False,
            "faucetEnabled": False,
            "economicWritesEnabled": False,
        },
        "arcadeTickets": {
            "earningEnabled": not paused["TicketEarning"],
            "transferEnabled": not paused["TicketTransfers"],
            "redemptionEnabled": not paused["TicketRedemption"],
            "randomVendingEnabled": not paused["RandomVending"],
            "featuredVendingEnabled": not paused["FeaturedVending"],
        },
        "additionalEconomicFlags": {
            "baseCallFilterPaidSurfaces": False,
            "legacyPackMint": False,
            "productionRewardPolicies": False,
            "publicAssetTransfers": False,
        },
    }


def sealed_terminal_sessions(capture: Mapping[str, Any]) -> int:
    total = 0
    values = capture["storage"]["prefixes"]["gameResultsSealedEpochs"]["values"]
    for raw_hex in values.values():
        raw = hex_bytes(raw_hex, "sealed result epoch")
        # SealedResultEpoch { u64 epoch, [u8;32] root, u32 session_count }.
        require(len(raw) == 44, "sealed result epoch SCALE shape drifted")
        total += int.from_bytes(raw[40:44], "little")
    return total


def acceptance_inventory(capture: Mapping[str, Any], metadata: Metadata) -> dict[str, Any]:
    next_card = plain_u64(capture, metadata, "tcgNextCardIdV2")
    next_credit = plain_u64(capture, metadata, "tcgNextPackCreditIdV2")
    next_entity = plain_u64(capture, metadata, "creaturesNextEntityId")
    next_prism = plain_u64(capture, metadata, "magicNextPrismSpellId")
    next_session = plain_u64(capture, metadata, "gameResultsNextSessionId")
    next_game = plain_u64(capture, metadata, "gameAuthorityNextGameId")
    cards = prefix_count(capture, "tcgCardsV2")
    credits = prefix_count(capture, "tcgPackCreditsV2")
    entities = prefix_count(capture, "creaturesEntities")
    essence = prefix_count(capture, "magicEssenceBalances")
    charges = prefix_count(capture, "magicSpellChargeBalances")
    prisms = prefix_count(capture, "magicPrismSpells")
    sessions = prefix_count(capture, "gameResultsSessions")
    processed_results = prefix_count(capture, "gameResultsProcessedResults")
    settled_sessions = prefix_count(capture, "gameResultsSettledSessions")
    sealed_terminal = sealed_terminal_sessions(capture)
    teams = prefix_count(capture, "tcgCompetitiveTeamsV2")
    advancement = prefix_count(capture, "gamerPlayerAdvancement")
    pack_progress = prefix_count(capture, "gamerPackProgress")
    lifetime_xp = prefix_count(capture, "gamerLifetimeXp")
    legacy_games = prefix_count(capture, "gameAuthorityGames")
    legacy_active = prefix_count(capture, "gameAuthorityActivePlayers")
    legacy_eliminations = prefix_count(capture, "gameAuthorityEliminations")
    legacy_end = prefix_count(capture, "gameAuthorityEndCommands")
    legacy_events = prefix_count(capture, "gameAuthorityEliminationEvents")
    conversion_tombstones = prefix_count(capture, "tcgConversionTombstones")
    opening_receipts = prefix_count(capture, "tcgPackOpeningReceiptsV2")
    processed_magic = prefix_count(capture, "magicProcessedResults")
    counts = {name: 0 for name in release.ACCEPTANCE_COUNT_FIELDS}
    counts.update(
        {
            "cardsV2": cards,
            "entitiesV2": entities,
            "trainingPackCredits": credits,
            "productionPackCredits": 0,
            "pendingPackOpenings": prefix_count(capture, "tcgPendingPackOpeningsV2"),
            "conversionCommitments": conversion_tombstones,
            "reforgeCommitments": 0,
            "productionMagicBalances": 0,
            "trainingMagicBalances": essence + charges + prisms,
            "essenceBalances": essence,
            "spellChargeBalances": charges,
            "prismSpells": prisms,
            "activeV2Sessions": sessions,
            "acceptedProductionResults": 0,
            "acceptedTrainingResults": processed_results,
            "founderEntitlements": 0,
            "rankedTeams": teams,
            "playerAdvancementRecords": advancement,
            "packProgressRecords": pack_progress,
            "lifetimeCardsV2Created": 0 if next_card == 0 else next_card - 1,
            "lifetimeEntitiesV2Created": next_entity,
            "lifetimePackCreditsIssued": 0 if next_credit == 0 else next_credit - 1,
            "lifetimePackOpeningsRequested": opening_receipts,
            "lifetimeConversionsCommitted": conversion_tombstones,
            "lifetimeReforgesCommitted": 0,
            # No exact all-time essence/Charge issuance counter exists.  This
            # conservative signal combines monotonic Prism IDs, extant records,
            # and durable processed-result receipts.  Any uncertainty can only
            # increase the restore-blocking value.
            "lifetimeMagicAssetsCreated": max(
                next_prism,
                essence + charges + prisms,
                processed_magic,
            ),
            "lifetimeV2SessionsAuthorized": next_session,
            # Processed results are pruned when an epoch seals.  Sealed epochs
            # expose terminal session_count, which includes expiry/abort and is
            # therefore conservative rather than an exact result count.
            "lifetimeV2ResultsAccepted": max(
                processed_results,
                settled_sessions,
                sealed_terminal,
            ),
            "lifetimeFounderEntitlementsIssued": 0,
            "lifetimeRankedTeamsCreated": teams,
            "lifetimeProgressionRecordsCreated": advancement + pack_progress + lifetime_xp,
            "currentLegacyAuthorityGames": legacy_games,
            "currentLegacyAuthorityActivePlayerLocks": legacy_active,
            "currentLegacyAuthorityEliminationRecords": legacy_eliminations,
            "lifetimeLegacyAuthorityGamesCreated": next_game,
            "lifetimeLegacyAuthorityEndCommandsProcessed": legacy_end,
            "lifetimeLegacyAuthorityEliminationEventsProcessed": legacy_events,
            "lifetimeLegacyAuthorityAcceptanceWritesLowerBound": next_game + legacy_end + legacy_events,
            "currentV2GameResultSessions": sessions,
            "currentV2ProcessedResults": processed_results,
            "currentV2SettledSessions": settled_sessions,
            "lifetimeV2SessionIdsAllocated": next_session,
            "conservativeSealedV2TerminalSessions": sealed_terminal,
        }
    )
    require(set(counts) == release.ACCEPTANCE_COUNT_FIELDS, "derived acceptance count set drifted")
    return {
        "schemaVersion": 1,
        "kind": "nexus-v2-acceptance-inventory",
        "releaseId": capture["releaseId"],
        "sourceCommit": capture["sourceCommit"],
        "observedAtFinalizedBlock": capture["observedAtFinalizedBlock"],
        "counts": counts,
    }


def derive_and_validate_artifacts(
    capture_path: Path,
    gates_path: Path,
    inventory_path: Path,
    artifacts: RuntimeArtifacts,
) -> dict[str, Any]:
    capture = read_json(capture_path, "acceptance-boundary capture")
    require(
        capture_path.read_bytes() == canonical_bytes(capture),
        "acceptance-boundary capture is not canonical JSON",
    )
    identity = validate_capture(capture, artifacts)
    expected_gates = disabled_gates(capture, artifacts.metadata)
    expected_inventory = acceptance_inventory(capture, artifacts.metadata)
    require(gates_path.is_file() and not gates_path.is_symlink(), "economic gates must be a regular file")
    require(
        inventory_path.is_file() and not inventory_path.is_symlink(),
        "acceptance inventory must be a regular file",
    )
    require(gates_path.read_bytes() == canonical_bytes(expected_gates), "economic gates were not deterministically derived from the RPC capture")
    require(inventory_path.read_bytes() == canonical_bytes(expected_inventory), "acceptance inventory was not deterministically derived from the RPC capture")
    gates = release.validate_economic_gates(gates_path, identity["releaseId"], identity["sourceCommit"])
    inventory = release.validate_acceptance_inventory(inventory_path, identity["releaseId"], identity["sourceCommit"])
    require(
        (gates["blockNumber"], gates["blockHash"])
        == (identity["blockNumber"], identity["blockHash"])
        == (inventory["blockNumber"], inventory["blockHash"]),
        "capture, gates, and inventory do not use one finalized block",
    )
    return {
        **identity,
        "captureSha256": sha256_file(capture_path),
        "gatesSha256": sha256_file(gates_path),
        "inventorySha256": sha256_file(inventory_path),
        "nonzero": inventory["nonzero"],
    }


def validate_ingress_evidence(
    path: Path,
    expected_sha256: str,
    identity: Mapping[str, Any],
) -> dict[str, Any]:
    ensure_sha256(expected_sha256, "ingress evidence SHA-256")
    require(sha256_file(path) == expected_sha256, "ingress evidence hash mismatch")
    value = read_json(path, "ingress-closed evidence")
    require(path.read_bytes() == canonical_bytes(value), "ingress evidence is not canonical JSON")
    exact_keys(value, INGRESS_KEYS, "ingress-closed evidence")
    require(value["schemaVersion"] == 1, "ingress evidence schema mismatch")
    require(value["kind"] == "nexus-v2-private-alpha-ingress-closed-evidence", "ingress evidence kind mismatch")
    require(value["releaseId"] == identity["releaseId"], "ingress evidence release mismatch")
    require(value["sourceCommit"] == identity["sourceCommit"], "ingress evidence source mismatch")
    require(value["genesisHash"] == identity["genesisHash"], "ingress evidence genesis mismatch")
    require(
        value["observedAtFinalizedBlock"]
        == {"number": identity["blockNumber"], "hash": identity["blockHash"]},
        "ingress evidence finalized block mismatch",
    )
    parse_utc(value["observedAtUtc"], "ingress evidence observedAtUtc")
    require(value["mode"] == "AllExternalWriteIngressClosed", "ingress mode mismatch")
    components = exact_keys(value["components"], INGRESS_COMPONENT_KEYS, "ingress components")
    chain = exact_keys(components["chain-media"], CHAIN_INGRESS_KEYS, "chain-media ingress")
    site = exact_keys(components["site-indexer"], SITE_INGRESS_KEYS, "site-indexer ingress")
    for name in (
        "publicRpcWriteIngressClosed",
        "authorityOperatorIngressClosed",
        "gameplaySessionIngressClosed",
    ):
        require(chain[name] is True, f"ingress evidence must set {name}=true")
    for name in ("webMutationIngressClosed", "indexerMutationIngressClosed"):
        require(site[name] is True, f"ingress evidence must set {name}=true")
    ensure_sha256(chain["componentEvidenceSha256"], "chain-media ingress component hash")
    ensure_sha256(site["componentEvidenceSha256"], "site-indexer ingress component hash")
    require(value["blockProductionContinues"] is True, "ingress evidence did not retain finalized observation capability")
    require(value["paidOrPublicActivationAuthorized"] is False, "ingress evidence authorizes paid/public activation")
    return value


def validate_receipt(
    path: Path,
    expected_sha256: str,
    *,
    release_id: str,
    source_commit: str,
    genesis_hash: str,
    runtime_code_sha256: str,
    runtime_metadata_scale_sha256: str,
) -> dict[str, Any]:
    ensure_sha256(expected_sha256, "acceptance-boundary receipt SHA-256")
    require(path.is_file() and not path.is_symlink(), "acceptance-boundary receipt must be a regular file")
    require(sha256_file(path) == expected_sha256, "acceptance-boundary receipt hash mismatch")
    value = read_json(path, "acceptance-boundary receipt")
    exact_keys(value, RECEIPT_KEYS, "acceptance-boundary receipt")
    require(path.read_bytes() == canonical_bytes(value), "acceptance-boundary receipt is not canonical JSON")
    require(value["schemaVersion"] == 1, "acceptance-boundary receipt schema mismatch")
    require(value["kind"] == "nexus-v2-private-alpha-acceptance-boundary-receipt", "acceptance-boundary receipt kind mismatch")
    require(value["releaseId"] == ensure_release(release_id), "acceptance-boundary receipt release mismatch")
    require(value["sourceCommit"] == ensure_commit(source_commit), "acceptance-boundary receipt source mismatch")
    require(value["genesisHash"] == ensure_hash256(genesis_hash, "expected genesis hash"), "acceptance-boundary receipt genesis mismatch")
    require(value["runtimeCodeSha256"] == ensure_sha256(runtime_code_sha256, "expected runtime code SHA-256"), "acceptance-boundary receipt runtime code mismatch")
    require(value["runtimeMetadataScaleSha256"] == ensure_sha256(runtime_metadata_scale_sha256, "expected metadata SHA-256"), "acceptance-boundary receipt metadata mismatch")
    block_number, _ = release.finalized_block(
        value["observedAtFinalizedBlock"], "acceptance-boundary receipt"
    )
    require(block_number > 0, "acceptance-boundary receipt may not use genesis block zero")
    for field in (
        "acceptanceBoundaryCaptureSha256",
        "economicGatesSha256",
        "acceptanceInventorySha256",
        "postCutoverObservationSha256",
        "coordinatorExecuteEvidenceSha256",
        "ingressClosedEvidenceSha256",
    ):
        ensure_sha256(value[field], f"receipt {field}")
    require(value["coordinatorDecision"] == "keep-v2", "receipt coordinator decision mismatch")
    require(value["ingressMode"] == "AllExternalWriteIngressClosed", "receipt ingress mode mismatch")
    require(value["phase1SmokePassed"] is True, "receipt Phase-1 smoke did not pass")
    require(value["automaticRestorePermanentlyDisabled"] is True, "receipt did not retire automatic restore")
    require(value["operatorV2WriteScope"] == OPERATOR_SCOPE, "receipt operator scope mismatch")
    created_at = parse_utc(value["createdAtUtc"], "acceptance-boundary receipt createdAtUtc")
    require(
        created_at <= dt.datetime.now(dt.timezone.utc) + dt.timedelta(seconds=30),
        "acceptance-boundary receipt creation time is in the future",
    )
    return value


def command_collect(args: argparse.Namespace) -> None:
    artifacts = load_runtime_artifacts(Path(args.runtime_bundle_root), args.runtime_bundle_manifest_sha256)
    capture = collect_capture(
        Rpc(args.rpc_url, args.rpc_timeout_seconds),
        artifacts,
        args.release_id,
        args.source_commit,
        args.genesis_hash,
        args.observed_at or utc_now(),
    )
    gates = disabled_gates(capture, artifacts.metadata)
    inventory = acceptance_inventory(capture, artifacts.metadata)
    capture_path = Path(args.capture)
    gates_path = Path(args.economic_gates)
    inventory_path = Path(args.acceptance_inventory)
    write_new_json(capture_path, capture)
    write_new_json(gates_path, gates)
    write_new_json(inventory_path, inventory)
    derived = derive_and_validate_artifacts(capture_path, gates_path, inventory_path, artifacts)
    print(
        "acceptance boundary collected from one finalized block: "
        f"block={derived['blockNumber']} captureSha256={derived['captureSha256']}"
    )


def canonical_input(path: Path, label: str) -> dict[str, Any]:
    require(path.is_absolute(), f"{label} path must be absolute")
    require(path.is_file() and not path.is_symlink(), f"{label} must be a regular file")
    value = read_json(path, label)
    require(path.read_bytes() == canonical_bytes(value), f"{label} is not canonical JSON")
    return value


def phase1_path(root: Path, relative: str, label: str) -> Path:
    path = (root / relative).resolve()
    try:
        path.relative_to(root)
    except ValueError as exc:
        raise BoundaryError(f"{label} escapes the Phase-1 output root") from exc
    return path


def validate_phase1_output(root_value: str, expected_execute_sha256: str) -> dict[str, Any]:
    root = Path(root_value)
    require(root.is_absolute(), "Phase-1 output root must be absolute")
    require(root.is_dir() and not root.is_symlink(), "Phase-1 output root is unavailable")
    root = root.resolve()
    expected_execute_sha256 = ensure_sha256(
        expected_execute_sha256, "Phase-1 execute evidence SHA-256"
    )

    execute_path = phase1_path(root, "execute-evidence.json", "Phase-1 execute evidence")
    execute = canonical_input(execute_path, "Phase-1 execute evidence")
    require(sha256_file(execute_path) == expected_execute_sha256, "Phase-1 execute evidence hash mismatch")
    exact_keys(execute, PHASE1_EXECUTE_KEYS, "Phase-1 execute evidence")
    require(execute["schemaVersion"] == 1, "Phase-1 execute evidence schema mismatch")
    require(
        execute["kind"] == "nexus-v2-private-alpha-phase1-ingress-closure-execute-evidence",
        "Phase-1 execute evidence kind mismatch",
    )
    release_id = ensure_release(execute["releaseId"])
    source_commit = ensure_commit(execute["sourceCommit"])
    site_commit = ensure_commit(execute["siteSourceCommit"])
    genesis_hash = ensure_hash256(execute["genesisHash"], "Phase-1 genesis hash")
    for field in (
        "siteCandidateUsableForExecute",
        "allExternalWriteIngressClosed",
        "blockProductionContinues",
        "authorityLocalServicePreserved",
        "readOnlySiteStackPreserved",
    ):
        require(execute[field] is True, f"Phase-1 execute evidence must set {field}=true")
    for field in ("automaticReopenAuthorized", "paidOrPublicActivationAuthorized"):
        require(execute[field] is False, f"Phase-1 execute evidence must set {field}=false")
    stability = execute["stabilityWindowSeconds"]
    elapsed = execute["stabilityWindowElapsedMilliseconds"]
    require(
        isinstance(stability, int)
        and not isinstance(stability, bool)
        and 30 <= stability <= 900,
        "Phase-1 stability window must be in 30..900",
    )
    require(
        isinstance(elapsed, int) and not isinstance(elapsed, bool) and elapsed >= stability * 1000,
        "Phase-1 stability window did not elapse",
    )
    parse_utc(execute["completedAtUtc"], "Phase-1 completion time")
    block_number, block_hash = release.finalized_block(
        execute["observedAtFinalizedBlock"], "Phase-1 execute evidence"
    )

    files = {
        "capture": ("acceptance-boundary-rpc-capture.json", "acceptanceBoundaryCaptureSha256"),
        "gates": ("post-v16-economic-gates.json", "economicGatesSha256"),
        "inventory": ("post-v16-acceptance-inventory.json", "acceptanceInventorySha256"),
        "ingress": ("ingress-closed-evidence.json", "ingressClosedEvidenceSha256"),
        "chainComponent": (
            "chain-media-ingress-component-evidence.json",
            "chainMediaComponentEvidenceSha256",
        ),
        "siteComponent": (
            "site-indexer-ingress-component-evidence.json",
            "siteIndexerComponentEvidenceSha256",
        ),
    }
    loaded: dict[str, dict[str, Any]] = {}
    paths: dict[str, Path] = {}
    for name, (relative, hash_field) in files.items():
        path = phase1_path(root, relative, f"Phase-1 {name}")
        value = canonical_input(path, f"Phase-1 {name}")
        require(
            sha256_file(path) == ensure_sha256(execute[hash_field], f"Phase-1 {name} SHA-256"),
            f"Phase-1 {name} hash mismatch",
        )
        loaded[name] = value
        paths[name] = path

    capture = loaded["capture"]
    exact_keys(capture, CAPTURE_KEYS, "Phase-1 acceptance capture")
    require(capture["schemaVersion"] == 1, "Phase-1 acceptance capture schema mismatch")
    require(
        capture["kind"] == "nexus-v2-private-alpha-acceptance-boundary-rpc-capture",
        "Phase-1 acceptance capture kind mismatch",
    )
    require(capture["releaseId"] == release_id, "Phase-1 capture release mismatch")
    require(capture["sourceCommit"] == source_commit, "Phase-1 capture source mismatch")
    require(capture["genesisHash"] == genesis_hash, "Phase-1 capture genesis mismatch")
    require(
        capture["observedAtFinalizedBlock"] == {"number": block_number, "hash": block_hash},
        "Phase-1 capture block mismatch",
    )
    observed_at = parse_utc(capture["observedAtUtc"], "Phase-1 capture observedAtUtc")
    runtime = exact_keys(capture["runtime"], RUNTIME_KEYS, "Phase-1 capture runtime")
    require(runtime["specVersion"] == EXPECTED_SPEC_VERSION, "Phase-1 runtime spec mismatch")
    ensure_sha256(runtime["runtimeCodeSha256"], "Phase-1 runtime code SHA-256")
    ensure_sha256(runtime["runtimeMetadataScaleSha256"], "Phase-1 runtime metadata SHA-256")

    ingress = loaded["ingress"]
    validate_ingress_evidence(
        paths["ingress"],
        execute["ingressClosedEvidenceSha256"],
        {
            "releaseId": release_id,
            "sourceCommit": source_commit,
            "genesisHash": genesis_hash,
            "blockNumber": block_number,
            "blockHash": block_hash,
        },
    )
    require(ingress["observedAtUtc"] == capture["observedAtUtc"], "Phase-1 ingress time mismatch")
    require(
        ingress["components"]["chain-media"]["componentEvidenceSha256"]
        == execute["chainMediaComponentEvidenceSha256"],
        "Phase-1 ingress chain component hash mismatch",
    )
    require(
        ingress["components"]["site-indexer"]["componentEvidenceSha256"]
        == execute["siteIndexerComponentEvidenceSha256"],
        "Phase-1 ingress site component hash mismatch",
    )

    chain_component = loaded["chainComponent"]
    site_component = loaded["siteComponent"]
    exact_keys(chain_component, PHASE1_CHAIN_COMPONENT_KEYS, "Phase-1 chain component")
    exact_keys(site_component, PHASE1_SITE_COMPONENT_KEYS, "Phase-1 site component")
    for component, kind in (
        (chain_component, "nexus-v2-private-alpha-phase1-chain-media-ingress-component-evidence"),
        (site_component, "nexus-v2-private-alpha-phase1-site-indexer-ingress-component-evidence"),
    ):
        require(component["schemaVersion"] == 1, "Phase-1 component schema mismatch")
        require(component["kind"] == kind, "Phase-1 component kind mismatch")
        require(component["releaseId"] == release_id, "Phase-1 component release mismatch")
        require(component["sourceCommit"] == source_commit, "Phase-1 component source mismatch")
        require(component["genesisHash"] == genesis_hash, "Phase-1 component genesis mismatch")
        require(
            component["observedAtFinalizedBlock"] == capture["observedAtFinalizedBlock"],
            "Phase-1 component block mismatch",
        )
        require(component["automaticReopenAuthorized"] is False, "Phase-1 component may not reopen")
        require(
            component["paidOrPublicActivationAuthorized"] is False,
            "Phase-1 component may not authorize paid/public activation",
        )
    require(site_component["siteSourceCommit"] == site_commit, "Phase-1 site source mismatch")

    closure_times: list[dt.datetime] = []
    for name, component in (("chain-close", chain_component), ("site-close", site_component)):
        path = phase1_path(root, f"component-observations/{name}.json", f"Phase-1 {name}")
        value = canonical_input(path, f"Phase-1 {name}")
        require(
            sha256_file(path) == component["closureObservationSha256"],
            f"Phase-1 {name} observation hash mismatch",
        )
        require(value.get("schemaVersion") == 1, f"Phase-1 {name} schema mismatch")
        expected_kind = (
            "nexus-v2-private-alpha-phase1-chain-ingress-observation"
            if name == "chain-close"
            else "nexus-v2-private-alpha-phase1-site-ingress-observation"
        )
        require(value.get("kind") == expected_kind, f"Phase-1 {name} kind mismatch")
        require(value.get("action") == "close", f"Phase-1 {name} action mismatch")
        closure_times.append(parse_utc(value.get("observedAtUtc"), f"Phase-1 {name} time"))
    paused_at = max(closure_times)
    require(paused_at <= observed_at, "Phase-1 acceptance capture predates closure")
    require(
        observed_at - paused_at >= dt.timedelta(seconds=stability),
        "Phase-1 closure was not stable before acceptance capture",
    )

    return {
        "root": root,
        "execute": execute,
        "executePath": execute_path,
        "executeSha256": expected_execute_sha256,
        "releaseId": release_id,
        "sourceCommit": source_commit,
        "siteSourceCommit": site_commit,
        "genesisHash": genesis_hash,
        "block": {"number": block_number, "hash": block_hash},
        "observedAtUtc": capture["observedAtUtc"],
        "pausedAtUtc": paused_at.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "stabilityWindowSeconds": stability,
        "capture": capture,
        "paths": paths,
    }


def compose_observation_value(phase1: Mapping[str, Any], media_commit: str) -> dict[str, Any]:
    media_commit = ensure_commit(media_commit)
    execute = phase1["execute"]
    return {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-post-cutover-rollback-observation",
        "releaseId": phase1["releaseId"],
        "sourceCommit": phase1["sourceCommit"],
        "componentSourceCommits": {
            "chain-media": {"chain": phase1["sourceCommit"], "media": media_commit},
            "site-indexer": {
                "chain": phase1["sourceCommit"],
                "site": phase1["siteSourceCommit"],
            },
        },
        "observedAtFinalizedBlock": phase1["block"],
        "observedAtUtc": phase1["observedAtUtc"],
        "writeBarrier": {
            "mode": "AllV2WritesPaused",
            "chainWritesPaused": True,
            "authorityResultsPaused": True,
            "webMutationsPaused": True,
            "gameplaySessionIngressPaused": True,
            "inventoryObservedAfterPause": True,
            "pausedAtUtc": phase1["pausedAtUtc"],
            "stabilityWindowSeconds": phase1["stabilityWindowSeconds"],
            "evidenceSha256": execute["ingressClosedEvidenceSha256"],
        },
        "acceptanceBoundaryCaptureSha256": execute["acceptanceBoundaryCaptureSha256"],
        "ingressClosedEvidenceSha256": execute["ingressClosedEvidenceSha256"],
        "economicGatesSha256": execute["economicGatesSha256"],
        "acceptanceInventorySha256": execute["acceptanceInventorySha256"],
    }


def command_compose_observation(args: argparse.Namespace) -> None:
    phase1 = validate_phase1_output(
        args.phase1_output_root, args.phase1_execute_evidence_sha256
    )
    observation = compose_observation_value(phase1, args.media_source_commit)
    exact_keys(observation, OBSERVATION_KEYS, "composed post-cutover observation")
    output = Path(args.output)
    write_new_json(output, observation)
    print(f"post-cutover observation composed: {output} sha256={sha256_file(output)}")


def git_output(root: Path, *arguments: str) -> str:
    completed = subprocess.run(
        ["git", "-C", str(root), *arguments], capture_output=True, text=True, check=False
    )
    require(completed.returncode == 0, f"cannot inspect source root: {root}")
    return completed.stdout.strip()


def validate_clean_root(root_value: str, expected_commit: str, label: str) -> Path:
    root = Path(root_value)
    require(root.is_absolute() and root.is_dir() and not root.is_symlink(), f"{label} root is invalid")
    root = root.resolve()
    require(
        Path(git_output(root, "rev-parse", "--show-toplevel")).resolve() == root,
        f"{label} root must be a Git worktree root",
    )
    require(git_output(root, "rev-parse", "HEAD") == expected_commit, f"{label} HEAD mismatch")
    require(
        git_output(root, "status", "--porcelain", "--untracked-files=all") == "",
        f"{label} worktree is dirty",
    )
    return root


def script_pin(root: Path, relative_value: str, label: str) -> dict[str, str]:
    relative = Path(relative_value)
    require(not relative.is_absolute() and ".." not in relative.parts, f"{label} path is invalid")
    path = (root / relative).resolve()
    try:
        path.relative_to(root)
    except ValueError as exc:
        raise BoundaryError(f"{label} escapes its source root") from exc
    require(path.is_file() and not path.is_symlink(), f"{label} is unavailable")
    require(bool(path.stat().st_mode & 0o100), f"{label} is not executable")
    git_output(root, "ls-files", "--error-unmatch", relative.as_posix())
    return {"path": relative.as_posix(), "sha256": sha256_file(path)}


def command_compose_coordinator_plan(args: argparse.Namespace) -> None:
    phase1 = validate_phase1_output(
        args.phase1_output_root, args.phase1_execute_evidence_sha256
    )
    observation_path = Path(args.observation)
    observation = canonical_input(observation_path, "post-cutover observation")
    observation_sha = ensure_sha256(args.observation_sha256, "post-cutover observation SHA-256")
    require(sha256_file(observation_path) == observation_sha, "post-cutover observation hash mismatch")
    exact_keys(observation, OBSERVATION_KEYS, "post-cutover observation")
    expected_observation = compose_observation_value(phase1, args.media_source_commit)
    require(observation == expected_observation, "post-cutover observation does not match Phase-1 output")

    component_commits = observation["componentSourceCommits"]
    chain_commit = ensure_commit(component_commits["chain-media"]["chain"])
    media_commit = ensure_commit(component_commits["chain-media"]["media"])
    site_commit = ensure_commit(component_commits["site-indexer"]["site"])
    chain_root = validate_clean_root(args.chain_root, chain_commit, "chain")
    media_root = validate_clean_root(args.media_root, media_commit, "media")
    site_root = validate_clean_root(args.site_root, site_commit, "site")

    runtime_artifacts = load_runtime_artifacts(
        Path(args.runtime_bundle_root), args.runtime_bundle_manifest_sha256
    )
    derived = derive_and_validate_artifacts(
        phase1["paths"]["capture"],
        phase1["paths"]["gates"],
        phase1["paths"]["inventory"],
        runtime_artifacts,
    )
    require(derived["captureSha256"] == observation["acceptanceBoundaryCaptureSha256"], "capture hash mismatch")

    readiness_path = Path(args.fresh_reset_readiness)
    canonical_input(readiness_path, "fresh-reset readiness")
    readiness_sha = ensure_sha256(args.fresh_reset_readiness_sha256, "fresh-reset readiness SHA-256")
    require(sha256_file(readiness_path) == readiness_sha, "fresh-reset readiness hash mismatch")
    backup_path = Path(args.final_backup_manifest)
    canonical_input(backup_path, "final backup manifest")
    restore_path = Path(args.restore_evidence)
    canonical_input(restore_path, "restore evidence")

    coordinator_relative = "deploy/alpha/macmini2010/nexus-v2-post-cutover-coordinator.py"
    chain_driver_relative = "deploy/alpha/macmini2010/nexus-v2-rollback-component-driver"
    chain_scripts = {
        "restoreState": "deploy/alpha/macmini2010/restore-alpha-state.sh",
        "deployNode": "deploy/alpha/macmini2010/deploy-node.sh",
        "deployMedia": "deploy/alpha/macmini2010/deploy-media.sh",
        "status": "deploy/alpha/macmini2010/status.sh",
    }
    site_scripts = {
        "restoreState": args.site_restore_path,
        "deploySite": args.site_deploy_path,
        "status": args.site_status_path,
    }
    coordinator_pin = script_pin(chain_root, coordinator_relative, "post-cutover coordinator")
    chain_driver = script_pin(chain_root, chain_driver_relative, "chain rollback driver")
    site_driver = script_pin(site_root, args.site_driver_path, "site rollback driver")
    chain_script_pins = {
        role: {"sourceId": "chain", **script_pin(chain_root, path, f"chain {role}")}
        for role, path in chain_scripts.items()
    }
    site_script_pins = {
        role: {"sourceId": "site", **script_pin(site_root, path, f"site {role}")}
        for role, path in site_scripts.items()
    }

    created = parse_utc(args.created_at or utc_now(), "coordinator plan createdAtUtc")
    expires = (
        parse_utc(args.expires_at, "coordinator plan expiresAtUtc")
        if args.expires_at
        else created + dt.timedelta(minutes=15)
    )
    require(created < expires and expires - created <= dt.timedelta(hours=1), "invalid coordinator plan window")
    max_age = args.max_observation_age_seconds
    require(30 <= max_age <= 900, "max observation age must be in 30..900")
    archive_base = args.reset_archive_root.rstrip("/") + "/" + readiness_sha

    plan = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-post-cutover-coordinator-plan",
        "operationId": ensure_release(args.operation_id),
        "releaseId": phase1["releaseId"],
        "sourceCommit": chain_commit,
        "genesisHash": phase1["genesisHash"],
        "runtimeCodeSha256": derived["runtimeCodeSha256"],
        "runtimeMetadataScaleSha256": derived["runtimeMetadataScaleSha256"],
        "runtimeBundleManifestSha256": runtime_artifacts.bundle_manifest_sha256,
        "freshResetReadinessSha256": readiness_sha,
        "finalBackupManifestSha256": sha256_file(backup_path),
        "restoreEvidenceSha256": sha256_file(restore_path),
        "postCutoverObservationSha256": observation_sha,
        "acceptanceBoundaryCaptureSha256": derived["captureSha256"],
        "ingressClosedEvidenceSha256": phase1["execute"]["ingressClosedEvidenceSha256"],
        "coordinatorSha256": coordinator_pin["sha256"],
        "maxObservationAgeSeconds": max_age,
        "automaticRestoreApproved": True,
        "paidOrPublicActivationAuthorized": False,
        "createdAtUtc": created.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "expiresAtUtc": expires.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "components": [
            {
                "id": "chain-media",
                "sourcePins": [
                    {"id": "chain", "root": str(chain_root), "expectedCommit": chain_commit},
                    {"id": "media", "root": str(media_root), "expectedCommit": media_commit},
                ],
                "driverSourceId": "chain",
                "driverPath": chain_driver["path"],
                "driverSha256": chain_driver["sha256"],
                "requiredResetArchives": {
                    "node": f"{archive_base}/node",
                    "media": f"{archive_base}/media",
                },
                "scriptPins": chain_script_pins,
            },
            {
                "id": "site-indexer",
                "sourcePins": [
                    {"id": "chain", "root": str(chain_root), "expectedCommit": chain_commit},
                    {"id": "site", "root": str(site_root), "expectedCommit": site_commit},
                ],
                "driverSourceId": "site",
                "driverPath": site_driver["path"],
                "driverSha256": site_driver["sha256"],
                "requiredResetArchives": {"site": f"{archive_base}/site"},
                "scriptPins": site_script_pins,
            },
        ],
    }
    exact_keys(plan, COORDINATOR_PLAN_KEYS, "composed coordinator plan")
    output = Path(args.output)
    write_new_json(output, plan)
    print(f"post-cutover coordinator plan composed: {output} sha256={sha256_file(output)}")


def command_validate_capture(args: argparse.Namespace) -> None:
    artifacts = load_runtime_artifacts(Path(args.runtime_bundle_root), args.runtime_bundle_manifest_sha256)
    result = derive_and_validate_artifacts(
        Path(args.capture),
        Path(args.economic_gates),
        Path(args.acceptance_inventory),
        artifacts,
    )
    require(result["releaseId"] == ensure_release(args.release_id), "capture release mismatch")
    require(result["sourceCommit"] == ensure_commit(args.source_commit), "capture source mismatch")
    require(result["genesisHash"] == ensure_hash256(args.genesis_hash, "expected genesis hash"), "capture genesis mismatch")
    print(f"acceptance boundary capture verified: sha256={result['captureSha256']}")


def command_verify_receipt(args: argparse.Namespace) -> None:
    validate_receipt(
        Path(args.receipt),
        args.expected_sha256,
        release_id=args.release_id,
        source_commit=args.source_commit,
        genesis_hash=args.genesis_hash,
        runtime_code_sha256=args.runtime_code_sha256,
        runtime_metadata_scale_sha256=args.runtime_metadata_scale_sha256,
    )
    print(f"acceptance-boundary receipt verified: sha256={args.expected_sha256}")


def command_create_receipt(args: argparse.Namespace) -> None:
    artifacts = load_runtime_artifacts(
        Path(args.runtime_bundle_root),
        args.runtime_bundle_manifest_sha256,
    )
    derived = derive_and_validate_artifacts(
        Path(args.capture),
        Path(args.economic_gates),
        Path(args.acceptance_inventory),
        artifacts,
    )
    require(not derived["nonzero"], "acceptance receipt requires zero current/lifetime V2 and legacy writes")
    require(derived["releaseId"] == ensure_release(args.release_id), "receipt input release mismatch")
    require(derived["sourceCommit"] == ensure_commit(args.source_commit), "receipt input source mismatch")
    require(derived["genesisHash"] == ensure_hash256(args.genesis_hash, "expected genesis hash"), "receipt input genesis mismatch")

    observation_path = Path(args.observation)
    observation = read_json(observation_path, "post-cutover observation")
    require(
        observation_path.read_bytes() == canonical_bytes(observation),
        "post-cutover observation is not canonical JSON",
    )
    exact_keys(observation, OBSERVATION_KEYS, "post-cutover observation")
    require(observation["schemaVersion"] == 1, "post-cutover observation schema mismatch")
    require(
        observation["kind"]
        == "nexus-v2-private-alpha-post-cutover-rollback-observation",
        "post-cutover observation kind mismatch",
    )
    require(observation["releaseId"] == derived["releaseId"], "observation release mismatch")
    require(observation["sourceCommit"] == derived["sourceCommit"], "observation source mismatch")
    expected_block = {"number": derived["blockNumber"], "hash": derived["blockHash"]}
    require(observation["observedAtFinalizedBlock"] == expected_block, "observation block mismatch")
    require(
        observation["observedAtUtc"] == derived["observedAtUtc"],
        "observation timestamp differs from the RPC capture",
    )
    require(observation["acceptanceBoundaryCaptureSha256"] == derived["captureSha256"], "observation capture mismatch")
    require(observation["economicGatesSha256"] == derived["gatesSha256"], "observation gates mismatch")
    require(observation["acceptanceInventorySha256"] == derived["inventorySha256"], "observation inventory mismatch")

    ingress_path = Path(args.ingress_closed_evidence)
    ingress_hash = ensure_sha256(args.ingress_closed_evidence_sha256, "expected ingress evidence SHA-256")
    ingress = validate_ingress_evidence(ingress_path, ingress_hash, derived)
    require(observation["ingressClosedEvidenceSha256"] == ingress_hash, "observation ingress mismatch")
    barrier = exact_keys(
        observation["writeBarrier"],
        WRITE_BARRIER_KEYS,
        "observation write barrier",
    )
    require(barrier["mode"] == "AllV2WritesPaused", "write barrier mode mismatch")
    require(barrier["evidenceSha256"] == ingress_hash, "write barrier does not bind ingress evidence")
    for name in (
        "chainWritesPaused",
        "authorityResultsPaused",
        "webMutationsPaused",
        "gameplaySessionIngressPaused",
        "inventoryObservedAfterPause",
    ):
        require(barrier[name] is True, f"write barrier must set {name}=true")
    stability_window = barrier["stabilityWindowSeconds"]
    require(
        isinstance(stability_window, int)
        and not isinstance(stability_window, bool)
        and 30 <= stability_window <= 900,
        "write-barrier stability window must be in 30..900",
    )
    paused_at = parse_utc(barrier["pausedAtUtc"], "write-barrier pausedAtUtc")
    observed_at = parse_utc(observation["observedAtUtc"], "observation observedAtUtc")
    require(paused_at <= observed_at, "observation predates the write barrier")
    require(
        observed_at - paused_at >= dt.timedelta(seconds=stability_window),
        "write barrier did not remain stable for its declared window",
    )

    component_commits = exact_keys(
        observation["componentSourceCommits"],
        {"chain-media", "site-indexer"},
        "observation component source commits",
    )
    chain_media = exact_keys(
        component_commits["chain-media"],
        {"chain", "media"},
        "chain-media source commits",
    )
    site_indexer = exact_keys(
        component_commits["site-indexer"],
        {"chain", "site"},
        "site-indexer source commits",
    )
    for component, commits in (("chain-media", chain_media), ("site-indexer", site_indexer)):
        for name, commit in commits.items():
            require(
                commit == ensure_commit(commit),
                f"invalid {component} {name} source commit",
            )
    require(
        chain_media["chain"] == site_indexer["chain"] == derived["sourceCommit"],
        "observation chain source commits do not match the deployment source",
    )

    coordinator_path = Path(args.coordinator_evidence)
    coordinator_hash = ensure_sha256(
        args.coordinator_evidence_sha256,
        "expected coordinator execute evidence SHA-256",
    )
    require(sha256_file(coordinator_path) == coordinator_hash, "coordinator execute evidence hash mismatch")
    coordinator = read_json(coordinator_path, "coordinator execute evidence")
    require(
        coordinator_path.read_bytes() == canonical_bytes(coordinator),
        "coordinator execute evidence is not canonical JSON",
    )
    exact_keys(coordinator, COORDINATOR_EVIDENCE_KEYS, "coordinator execute evidence")
    require(coordinator["schemaVersion"] == 1, "coordinator evidence schema mismatch")
    require(
        coordinator["kind"]
        == "nexus-v2-private-alpha-post-cutover-coordinator-evidence",
        "coordinator evidence kind mismatch",
    )
    require(coordinator["releaseId"] == derived["releaseId"], "coordinator release mismatch")
    require(coordinator["sourceCommit"] == derived["sourceCommit"], "coordinator source mismatch")
    require(coordinator["genesisHash"] == derived["genesisHash"], "coordinator genesis mismatch")
    require(coordinator["runtimeCodeSha256"] == derived["runtimeCodeSha256"], "coordinator runtime code mismatch")
    require(
        coordinator["runtimeMetadataScaleSha256"]
        == derived["runtimeMetadataScaleSha256"],
        "coordinator runtime metadata mismatch",
    )
    require(coordinator["decision"] == "keep-v2", "coordinator did not execute a keep-v2 decision")
    require(coordinator["postCutoverSmokePassed"] is True, "coordinator Phase-1 smoke did not pass")
    require(coordinator["automaticRestorePerformed"] is False, "coordinator already restored the archive")
    require(coordinator["postAcceptanceContainmentPerformed"] is False, "coordinator entered containment")
    require(coordinator["nonzeroAcceptanceAssets"] == {}, "coordinator evidence is not pre-acceptance")
    require(coordinator["observedAtFinalizedBlock"] == expected_block, "coordinator block mismatch")
    require(coordinator["postCutoverObservationSha256"] == sha256_file(observation_path), "coordinator observation mismatch")
    require(coordinator["acceptanceBoundaryCaptureSha256"] == derived["captureSha256"], "coordinator capture mismatch")
    require(coordinator["ingressClosedEvidenceSha256"] == ingress_hash, "coordinator ingress mismatch")
    require(coordinator["economicGatesSha256"] == derived["gatesSha256"], "coordinator gates mismatch")
    require(coordinator["acceptanceInventorySha256"] == derived["inventorySha256"], "coordinator inventory mismatch")
    for field in (
        "planSha256",
        "finalBackupManifestSha256",
        "restoreEvidenceSha256",
    ):
        ensure_sha256(coordinator[field], f"coordinator {field}")
    require(
        coordinator["componentSourceCommits"] == component_commits,
        "coordinator component source commits mismatch",
    )
    marker_hashes = coordinator["componentMarkerSha256"]
    require(isinstance(marker_hashes, dict), "coordinator component marker hashes must be an object")
    required_execute_markers = {
        "chain-media.post-cutover-smoke.execute.json",
        "site-indexer.post-cutover-smoke.execute.json",
    }
    require(
        required_execute_markers.issubset(marker_hashes),
        "coordinator keep-v2 evidence lacks executed smoke markers",
    )
    for name, digest in marker_hashes.items():
        require(isinstance(name, str) and name.endswith(".json"), "invalid coordinator marker name")
        ensure_sha256(digest, f"coordinator marker {name}")
    coordinator_completed_at = parse_utc(
        coordinator["completedAtUtc"], "coordinator evidence completedAtUtc"
    )

    final_marker_path = Path(args.coordinator_final_marker)
    final_marker_hash = ensure_sha256(
        args.coordinator_final_marker_sha256,
        "expected coordinator final marker SHA-256",
    )
    require(
        sha256_file(final_marker_path) == final_marker_hash,
        "coordinator final marker hash mismatch",
    )
    final_marker = read_json(final_marker_path, "coordinator final marker")
    require(
        final_marker_path.read_bytes() == canonical_bytes(final_marker),
        "coordinator final marker is not canonical JSON",
    )
    exact_keys(final_marker, FINAL_MARKER_KEYS, "coordinator final marker")
    require(final_marker["schemaVersion"] == 1, "coordinator final marker schema mismatch")
    require(
        final_marker["kind"] == "nexus-v2-private-alpha-post-cutover-final-marker",
        "coordinator final marker kind mismatch",
    )
    require(final_marker["operationId"] == coordinator["operationId"], "coordinator marker operation mismatch")
    require(final_marker["planSha256"] == coordinator["planSha256"], "coordinator marker plan mismatch")
    require(
        Path(final_marker["evidencePath"]).resolve() == coordinator_path.resolve(),
        "coordinator marker evidence path mismatch",
    )
    require(final_marker["evidenceSha256"] == coordinator_hash, "coordinator marker evidence hash mismatch")
    marker_completed_at = parse_utc(
        final_marker["completedAtUtc"], "coordinator final marker completedAtUtc"
    )
    require(
        marker_completed_at >= coordinator_completed_at,
        "coordinator final marker predates execute evidence",
    )

    created_at = args.created_at or utc_now()
    created_at_value = parse_utc(created_at, "receipt createdAtUtc")
    require(
        created_at_value >= marker_completed_at,
        "receipt creation time predates coordinator completion",
    )
    require(
        created_at_value <= dt.datetime.now(dt.timezone.utc) + dt.timedelta(seconds=30),
        "receipt creation time is in the future",
    )
    receipt = {
        "schemaVersion": 1,
        "kind": "nexus-v2-private-alpha-acceptance-boundary-receipt",
        "releaseId": derived["releaseId"],
        "sourceCommit": derived["sourceCommit"],
        "genesisHash": derived["genesisHash"],
        "runtimeCodeSha256": derived["runtimeCodeSha256"],
        "runtimeMetadataScaleSha256": derived["runtimeMetadataScaleSha256"],
        "observedAtFinalizedBlock": expected_block,
        "acceptanceBoundaryCaptureSha256": derived["captureSha256"],
        "economicGatesSha256": derived["gatesSha256"],
        "acceptanceInventorySha256": derived["inventorySha256"],
        "postCutoverObservationSha256": sha256_file(observation_path),
        "coordinatorExecuteEvidenceSha256": coordinator_hash,
        "coordinatorDecision": "keep-v2",
        "ingressClosedEvidenceSha256": ingress_hash,
        "ingressMode": ingress["mode"],
        "phase1SmokePassed": True,
        # Conservatively close restoration at receipt issuance, before the
        # first bootstrap write.  A failed/no-op write never reopens it.
        "automaticRestorePermanentlyDisabled": True,
        "operatorV2WriteScope": OPERATOR_SCOPE,
        "createdAtUtc": created_at,
    }
    output = Path(args.output)
    write_new_json(output, receipt)
    digest = sha256_file(output)
    validate_receipt(
        output,
        digest,
        release_id=derived["releaseId"],
        source_commit=derived["sourceCommit"],
        genesis_hash=derived["genesisHash"],
        runtime_code_sha256=derived["runtimeCodeSha256"],
        runtime_metadata_scale_sha256=derived["runtimeMetadataScaleSha256"],
    )
    print(f"acceptance boundary crossed; automatic restore retired: {output} sha256={digest}")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    observation = subparsers.add_parser(
        "compose-observation",
        help="compose the closed post-cutover observation from a pinned Phase-1 output",
    )
    observation.add_argument("--phase1-output-root", required=True)
    observation.add_argument("--phase1-execute-evidence-sha256", required=True)
    observation.add_argument("--media-source-commit", required=True)
    observation.add_argument("--output", required=True)
    observation.set_defaults(func=command_compose_observation)

    plan = subparsers.add_parser(
        "compose-coordinator-plan",
        help="compose a hash-pinned plan accepted by the post-cutover coordinator",
    )
    plan.add_argument("--phase1-output-root", required=True)
    plan.add_argument("--phase1-execute-evidence-sha256", required=True)
    plan.add_argument("--observation", required=True)
    plan.add_argument("--observation-sha256", required=True)
    plan.add_argument("--media-source-commit", required=True)
    plan.add_argument("--operation-id", required=True)
    plan.add_argument("--chain-root", required=True)
    plan.add_argument("--media-root", required=True)
    plan.add_argument("--site-root", required=True)
    plan.add_argument("--runtime-bundle-root", required=True)
    plan.add_argument("--runtime-bundle-manifest-sha256", required=True)
    plan.add_argument("--fresh-reset-readiness", required=True)
    plan.add_argument("--fresh-reset-readiness-sha256", required=True)
    plan.add_argument("--final-backup-manifest", required=True)
    plan.add_argument("--restore-evidence", required=True)
    plan.add_argument(
        "--site-driver-path",
        default="tcg/deploy/alpha/macmini2014/nexus-v2-rollback-component-driver",
    )
    plan.add_argument(
        "--site-restore-path",
        default="tcg/deploy/alpha/macmini2014/restore-alpha-state.sh",
    )
    plan.add_argument(
        "--site-deploy-path", default="tcg/deploy/alpha/macmini2014/deploy-site.sh"
    )
    plan.add_argument(
        "--site-status-path", default="tcg/deploy/alpha/macmini2014/status.sh"
    )
    plan.add_argument(
        "--reset-archive-root", default="/opt/eterra-alpha/archive/nexus-v2-fresh-reset"
    )
    plan.add_argument("--max-observation-age-seconds", type=int, default=600)
    plan.add_argument("--created-at")
    plan.add_argument("--expires-at")
    plan.add_argument("--output", required=True)
    plan.set_defaults(func=command_compose_coordinator_plan)

    collect = subparsers.add_parser("collect", help="read one finalized block and derive gates/inventory")
    collect.add_argument("--rpc-url", required=True)
    collect.add_argument("--rpc-timeout-seconds", type=int, default=30)
    collect.add_argument("--runtime-bundle-root", required=True)
    collect.add_argument("--runtime-bundle-manifest-sha256", required=True)
    collect.add_argument("--release-id", required=True)
    collect.add_argument("--source-commit", required=True)
    collect.add_argument("--genesis-hash", required=True)
    collect.add_argument("--observed-at")
    collect.add_argument("--capture", required=True)
    collect.add_argument("--economic-gates", required=True)
    collect.add_argument("--acceptance-inventory", required=True)
    collect.set_defaults(func=command_collect)

    validate = subparsers.add_parser("validate-capture", help="rederive gates/inventory from an immutable capture")
    validate.add_argument("--runtime-bundle-root", required=True)
    validate.add_argument("--runtime-bundle-manifest-sha256", required=True)
    validate.add_argument("--release-id", required=True)
    validate.add_argument("--source-commit", required=True)
    validate.add_argument("--genesis-hash", required=True)
    validate.add_argument("--capture", required=True)
    validate.add_argument("--economic-gates", required=True)
    validate.add_argument("--acceptance-inventory", required=True)
    validate.set_defaults(func=command_validate_capture)

    create = subparsers.add_parser("create-receipt", help="irreversibly close automatic restore and authorize narrow Phase-2 writes")
    create.add_argument("--runtime-bundle-root", required=True)
    create.add_argument("--runtime-bundle-manifest-sha256", required=True)
    create.add_argument("--release-id", required=True)
    create.add_argument("--source-commit", required=True)
    create.add_argument("--genesis-hash", required=True)
    create.add_argument("--capture", required=True)
    create.add_argument("--economic-gates", required=True)
    create.add_argument("--acceptance-inventory", required=True)
    create.add_argument("--observation", required=True)
    create.add_argument("--ingress-closed-evidence", required=True)
    create.add_argument("--ingress-closed-evidence-sha256", required=True)
    create.add_argument("--coordinator-evidence", required=True)
    create.add_argument("--coordinator-evidence-sha256", required=True)
    create.add_argument("--coordinator-final-marker", required=True)
    create.add_argument("--coordinator-final-marker-sha256", required=True)
    create.add_argument("--created-at")
    create.add_argument("--output", required=True)
    create.set_defaults(func=command_create_receipt)

    verify = subparsers.add_parser("verify-receipt", help="verify the exact hash-pinned Phase-2 receipt")
    verify.add_argument("--receipt", required=True)
    verify.add_argument("--expected-sha256", required=True)
    verify.add_argument("--release-id", required=True)
    verify.add_argument("--source-commit", required=True)
    verify.add_argument("--genesis-hash", required=True)
    verify.add_argument("--runtime-code-sha256", required=True)
    verify.add_argument("--runtime-metadata-scale-sha256", required=True)
    verify.set_defaults(func=command_verify_receipt)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        require(
            not hasattr(args, "rpc_timeout_seconds")
            or 1 <= args.rpc_timeout_seconds <= 120,
            "RPC timeout must be in 1..120 seconds",
        )
        args.func(args)
    except (BoundaryError, release.SafetyError, OSError) as exc:
        print(f"acceptance_boundary: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
