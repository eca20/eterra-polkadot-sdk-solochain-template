#!/usr/bin/env python3
"""Verify the one exact Linux runtime bundle approved for final freeze."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Sequence

import node_candidate


@dataclass(frozen=True)
class ReleasePins:
    manifest_sha256: str
    sha256_sums_sha256: str
    bundle_files: frozenset[str]
    source_commit: str
    source_tree: str
    assembly_commit: str
    assembly_tree: str
    production_wasm_sha256: str
    superseded_wasm_sha256: str
    native_node_sha256: str
    metadata_scale_sha256: str
    metadata_json_sha256: str
    try_runtime_wasm_sha256: str
    linux_migration_verifier_sha256: str
    host_migration_verifier_sha256: str
    try_runtime_cli_sha256: str
    prior_source_commit: str
    prior_manifest_sha256: str
    genesis_hash: str


PRODUCTION_PINS = ReleasePins(
    manifest_sha256="79359a961d065bd189f9b585cd57d339b6f58d8855b4d1d156c03b3e0b3feb5c",
    sha256_sums_sha256="e983dc09310e737c0e9e5cc3ba067a336da877c72db42c335e9b39316be2aace",
    bundle_files=frozenset(
        {
            "SHA256SUMS",
            "deployment-node-SHA256SUMS",
            "deployment-node-attestation.json",
            "deployment-node-buildkit-metadata.json",
            "external-reviews.pending.json",
            "genesis-hash.rpc.json",
            "linux-runtime-probe-result.json",
            "metadata-compatibility.json",
            "metadata-v15.rpc-proof.json",
            "nexus-v2-migration-verifier",
            "nexus-v2-migration-verifier.linux-amd64",
            "runtime-bundle-manifest.json",
            "runtime-metadata.json",
            "runtime-metadata.scale",
            "runtime-spec-106.compact.compressed.wasm",
            "runtime-spec-106.dev-chain-spec.raw.json",
            "runtime-spec-106.temporary-node-embedded-code.wasm",
            "runtime-spec-106.try-runtime.wasm",
            "runtime-spec-live-v14.recovery.wasm",
            "runtime-support-build-attestation.json",
            "runtime-support-buildkit-metadata.json",
            "runtime-version.rpc.json",
            "solochain-eterra-node",
            "superseded-runtime-identity.json",
            "tcg-storage-version-observation.json",
            "temporary-node-embedded-code.rpc.json",
            "try-runtime",
        }
    ),
    source_commit="7338beff0c99ef72db43a6908f7bee07a181b50b",
    source_tree="ebd3627937c8b095ab7ff7679c8babcdba13e51c",
    assembly_commit="b296cda3b8f2bbdb46822b90772d8b2e4607daa1",
    assembly_tree="545a0651032ffad55c42fde7534761651955ae66",
    production_wasm_sha256="0b0c7c52b38ea880fa626784846164752aa256b9f30d83ed0b45d25277f38243",
    superseded_wasm_sha256="fc9a48dc7f27946221510f5f5ef7b616b121928c8366b4903afcc0ffeaf58b9d",
    native_node_sha256="2dd95dcdb7f752de82599a1562361e6564a66ea4a257ee5f6361160e23476b4e",
    metadata_scale_sha256="26ed50d186a0cb134cb8ef6b9f619cd04195b52cf4d06fb3f2c31050b103ee68",
    metadata_json_sha256="0ee7ef62ebdffff64ff521ded92046bc903fa5d8a8399e76150979e9666e32c7",
    try_runtime_wasm_sha256="7674327bb88b3a1986abfac86e41ec43aa072929fda526647abd26bfca5131d0",
    linux_migration_verifier_sha256="032be60fc5c193bd89a348e3f4da31ebf14257f366bd40d6d924846562dcdeea",
    host_migration_verifier_sha256="149d4019f529c775c210938d54cc8ecf22d03e371917b6420fd6801174ab757a",
    try_runtime_cli_sha256="7fa7a9b9f85bbdf416d8b6e446c4bceeda5b528bffd9b6c2e871320fa17085d9",
    prior_source_commit="94ac1f46e7ec3e802c12de86ba7baec168583723",
    prior_manifest_sha256="5710327ccb0d39f461ab358d9cf1a8fb0bc602a16f8013a27de67b8306774adb",
    genesis_hash="0x0bcde48aef1384cf2646991e8472962c0fc09f24736c7236e3a7d0ae449c9a66",
)


class FreezeBundleError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise FreezeBundleError(message)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def read_json(path: Path, label: str) -> dict[str, Any]:
    require(path.is_file() and not path.is_symlink(), f"{label} must be a regular file")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise FreezeBundleError(f"invalid {label}: {path}") from exc
    require(isinstance(value, dict), f"{label} root must be an object")
    return value


def checked_artifact(root: Path, manifest: dict[str, Any], key: str, name: str, expected: str) -> Path:
    artifacts = manifest.get("artifacts")
    require(isinstance(artifacts, dict) and artifacts.get(key) == expected, f"runtime bundle {key} pin mismatch")
    path = root / name
    require(path.is_file() and not path.is_symlink(), f"runtime bundle artifact is missing: {name}")
    require(sha256_file(path) == expected, f"runtime bundle artifact hash mismatch: {name}")
    return path


def verify_final_freeze_runtime_bundle(
    root: Path,
    expected_manifest_sha256: str,
    pins: ReleasePins = PRODUCTION_PINS,
) -> dict[str, Any]:
    root = root.resolve()
    require(expected_manifest_sha256 == pins.manifest_sha256, "runtime bundle manifest is not the frozen Linux release manifest")
    require(
        {item.name for item in root.iterdir()} == pins.bundle_files
        and all(item.is_file() and not item.is_symlink() for item in root.iterdir()),
        "runtime bundle root does not match the frozen closed file set",
    )
    try:
        runtime = node_candidate.verify_runtime_bundle(root)
    except node_candidate.CandidateError as exc:
        raise FreezeBundleError(str(exc)) from exc
    manifest = runtime["manifest"]
    require(runtime["manifestSha256"] == pins.manifest_sha256, "runtime bundle manifest SHA-256 mismatch")
    require(runtime["sumsSha256"] == pins.sha256_sums_sha256, "runtime bundle SHA256SUMS identity mismatch")
    require(runtime["sourceCommit"] == pins.source_commit, "runtime bundle source commit mismatch")
    require(manifest.get("sourceTree") == pins.source_tree, "runtime bundle source tree mismatch")
    require(manifest.get("releaseId") == f"nexus-v2-private-alpha-linux-{pins.source_commit[:12]}", "runtime bundle release ID mismatch")
    require(manifest.get("sourceStorageVersion") == 14, "runtime bundle source storage version mismatch")
    require(runtime["wasmSha256"] == pins.production_wasm_sha256, "runtime production Wasm is not the frozen Linux build")
    require(runtime["nodeSha256"] == pins.native_node_sha256, "runtime native node is not the frozen Linux build")
    require(runtime["metadataSha256"] == pins.metadata_scale_sha256, "runtime metadata SCALE mismatch")

    identity = manifest.get("runtimeIdentity")
    require(isinstance(identity, dict), "runtime identity is missing")
    require(identity.get("authoritativeTargetPlatform") == node_candidate.TARGET_PLATFORM, "runtime target platform mismatch")
    for field in (
        "devChainSpecMatchesStagedProductionCode",
        "metadataScaleAndJsonExactlyMatchCompatibilityBaseline",
        "probeEphemeralWorkspaceOnly",
        "probeReadOnlyRootFilesystem",
        "probeRunnerNetworkDisabled",
        "stagedProductionMatchesTemporaryNodeEmbeddedCode",
    ):
        require(identity.get(field) is True, f"runtime identity proof is missing: {field}")
    require(identity.get("nativeHostExecutionAllowed") is False, "runtime bundle permits native-host fallback")
    require(identity.get("genesisHash") == pins.genesis_hash, "runtime genesis hash mismatch")

    checked_artifact(root, manifest, "stagedProductionWasmSha256", "runtime-spec-106.compact.compressed.wasm", pins.production_wasm_sha256)
    checked_artifact(root, manifest, "temporaryNodeEmbeddedWasmSha256", "runtime-spec-106.temporary-node-embedded-code.wasm", pins.production_wasm_sha256)
    checked_artifact(root, manifest, "metadataScaleSha256", "runtime-metadata.scale", pins.metadata_scale_sha256)
    checked_artifact(root, manifest, "metadataJsonSha256", "runtime-metadata.json", pins.metadata_json_sha256)
    checked_artifact(root, manifest, "tryRuntimeWasmSha256", "runtime-spec-106.try-runtime.wasm", pins.try_runtime_wasm_sha256)
    checked_artifact(root, manifest, "linuxMigrationVerifierSha256", "nexus-v2-migration-verifier.linux-amd64", pins.linux_migration_verifier_sha256)
    checked_artifact(root, manifest, "migrationVerifierSha256", "nexus-v2-migration-verifier", pins.host_migration_verifier_sha256)
    checked_artifact(root, manifest, "tryRuntimeCliSha256", "try-runtime", pins.try_runtime_cli_sha256)

    raw_spec = read_json(root / "runtime-spec-106.dev-chain-spec.raw.json", "runtime dev raw spec")
    try:
        embedded_hex = raw_spec["genesis"]["raw"]["top"]["0x3a636f6465"]
        embedded = bytes.fromhex(embedded_hex.removeprefix("0x"))
    except (KeyError, TypeError, ValueError) as exc:
        raise FreezeBundleError("runtime dev raw spec has invalid embedded :code") from exc
    require(hashlib.sha256(embedded).hexdigest() == pins.production_wasm_sha256, "raw spec embedded :code differs from production Wasm")

    compatibility = read_json(root / "metadata-compatibility.json", "metadata compatibility proof")
    require(compatibility.get("kind") == "nexus-v2-runtime-metadata-compatibility-proof", "metadata compatibility kind mismatch")
    require(compatibility.get("result") == "compatible", "metadata compatibility did not pass")
    require(compatibility.get("exactScaleBytesEqual") is True, "metadata SCALE was not byte-identical")
    require(compatibility.get("exactDecodedJsonBytesEqual") is True, "metadata JSON was not byte-identical")
    require(compatibility.get("sourceDeltaIsReleaseToolingOnly") is True, "runtime source delta is not release-tooling-only")
    baseline = compatibility.get("baseline")
    candidate = compatibility.get("candidate")
    require(
        isinstance(baseline, dict)
        and baseline.get("sourceCommit") == pins.prior_source_commit
        and baseline.get("bundleManifestSha256") == pins.prior_manifest_sha256
        and baseline.get("metadataScaleSha256") == pins.metadata_scale_sha256
        and baseline.get("metadataJsonSha256") == pins.metadata_json_sha256,
        "metadata compatibility baseline mismatch",
    )
    require(
        isinstance(candidate, dict)
        and candidate.get("sourceCommit") == pins.source_commit
        and candidate.get("metadataVersion") == 15
        and candidate.get("metadataScaleSha256") == pins.metadata_scale_sha256
        and candidate.get("metadataJsonSha256") == pins.metadata_json_sha256,
        "metadata compatibility candidate mismatch",
    )

    superseded = read_json(root / "superseded-runtime-identity.json", "superseded runtime identity")
    require(superseded.get("kind") == "nexus-v2-superseded-runtime-identity", "superseded runtime identity kind mismatch")
    require(superseded.get("sourceCommit") == pins.prior_source_commit, "superseded runtime source mismatch")
    require(superseded.get("bundleManifestSha256") == pins.prior_manifest_sha256, "superseded bundle identity mismatch")
    require(superseded.get("productionWasmSha256") == pins.superseded_wasm_sha256, "superseded Wasm proof mismatch")
    require(superseded.get("status") == "superseded-not-a-release-target", "superseded runtime status mismatch")
    require(superseded.get("productionWasmCopiedIntoBundle") is False, "superseded production Wasm was copied into the bundle")
    require(
        superseded.get("authorizations") == {"deploy": False, "release": False, "restoreTarget": False},
        "superseded runtime authorizations are unsafe",
    )
    for wasm in root.glob("*.wasm"):
        require(sha256_file(wasm) != pins.superseded_wasm_sha256, "superseded macOS production Wasm is present in the release bundle")

    support = read_json(root / "runtime-support-build-attestation.json", "runtime support build attestation")
    require(
        manifest["artifacts"].get("runtimeSupportBuildAttestationSha256")
        == sha256_file(root / "runtime-support-build-attestation.json"),
        "runtime support attestation artifact pin mismatch",
    )
    require(support.get("kind") == "nexus-v2-linux-runtime-support-build", "runtime support attestation kind mismatch")
    require(support.get("sourceCommit") == pins.source_commit and support.get("sourceTree") == pins.source_tree, "runtime support source mismatch")
    require(support.get("targetPlatform") == node_candidate.TARGET_PLATFORM, "runtime support target platform mismatch")
    support_environment = support.get("buildEnvironment")
    require(
        isinstance(support_environment, dict)
        and support_environment.get("buildkitPlatform") == "linux/amd64"
        and support_environment.get("containerImage") == node_candidate.PINNED_LINUX_AMD64_BUILD_IMAGE
        and support_environment.get("cargoLocked") is True
        and support_environment.get("incremental") is False,
        "runtime support build environment mismatch",
    )
    require(support.get("authorizations") == {"paidProduction": False, "publicDeploy": False, "publicRelease": False}, "runtime support authorizations are unsafe")
    support_artifacts = support.get("artifacts")
    require(
        isinstance(support_artifacts, dict)
        and support_artifacts.get("linuxMigrationVerifierSha256") == pins.linux_migration_verifier_sha256
        and support_artifacts.get("tryRuntimeWasmSha256") == pins.try_runtime_wasm_sha256,
        "runtime support artifact pins mismatch",
    )

    bundle_node_attestation = read_json(root / "deployment-node-attestation.json", "bundle spec-builder node attestation")
    require(
        manifest["artifacts"].get("deploymentNodeAttestationSha256")
        == sha256_file(root / "deployment-node-attestation.json"),
        "bundle spec-builder node attestation pin mismatch",
    )
    require(bundle_node_attestation.get("kind") == "nexus-v2-linux-amd64-deployment-node-build", "bundle node attestation kind mismatch")
    require(bundle_node_attestation.get("sourceCommit") == pins.source_commit, "bundle node attestation source mismatch")
    require(bundle_node_attestation.get("targetPlatform") == node_candidate.TARGET_PLATFORM, "bundle node attestation platform mismatch")
    bundle_node_environment = bundle_node_attestation.get("buildEnvironment")
    require(
        isinstance(bundle_node_environment, dict)
        and bundle_node_environment.get("buildkitPlatform") == "linux/amd64"
        and bundle_node_environment.get("containerImage") == node_candidate.PINNED_LINUX_AMD64_BUILD_IMAGE
        and bundle_node_environment.get("cargoLocked") is True
        and bundle_node_environment.get("incremental") is False
        and bundle_node_environment.get("runtimeProductionFeature") is True,
        "bundle node attestation build environment mismatch",
    )
    bundle_node_artifacts = bundle_node_attestation.get("artifacts")
    require(
        isinstance(bundle_node_artifacts, dict)
        and bundle_node_artifacts.get("nativeNodeSha256") == pins.native_node_sha256
        and bundle_node_artifacts.get("productionWasmSha256") == pins.production_wasm_sha256,
        "bundle node attestation artifact pins mismatch",
    )
    require(
        bundle_node_attestation.get("authorizations")
        == {"paidProduction": False, "publicDeploy": False, "publicRelease": False},
        "bundle node attestation authorizations are unsafe",
    )

    probe = read_json(root / "linux-runtime-probe-result.json", "Linux runtime probe result")
    require(
        probe.get("kind") == "nexus-v2-linux-runtime-probe-result"
        and probe.get("specVersion") == 106
        and probe.get("metadataVersion") == 15
        and probe.get("genesisHash") == pins.genesis_hash
        and probe.get("embeddedCodeMatchesProductionWasm") is True
        and probe.get("networkDisabledByRunner") is True
        and probe.get("readOnlyRootFilesystemByRunner") is True
        and probe.get("ephemeralWritableEvidenceWorkspace") is True,
        "Linux runtime probe result mismatch",
    )

    tools = manifest.get("tools")
    require(isinstance(tools, dict), "runtime bundle tool provenance is missing")
    require(tools.get("assemblySourceCommit") == pins.assembly_commit, "runtime assembler commit mismatch")
    require(tools.get("assemblySourceTree") == pins.assembly_tree, "runtime assembler tree mismatch")
    require(tools.get("assemblyDeltaIsReleaseToolingOnly") is True, "runtime assembly delta is not tooling-only")
    authorizations = manifest.get("authorizations")
    require(
        authorizations
        == {
            "externalReviewsSelfApproved": False,
            "localBuildOnly": True,
            "paidProduction": False,
            "publicDeploy": False,
            "publicRelease": False,
        },
        "runtime bundle authorizations do not match the private-alpha freeze contract",
    )
    return {
        "manifestSha256": runtime["manifestSha256"],
        "sha256SumsSha256": runtime["sumsSha256"],
        "sourceCommit": runtime["sourceCommit"],
        "sourceTree": pins.source_tree,
        "nativeNodeSha256": runtime["nodeSha256"],
        "productionWasmSha256": runtime["wasmSha256"],
        "metadataScaleSha256": runtime["metadataSha256"],
        "metadataVersion": runtime["metadataVersion"],
        "genesisHash": pins.genesis_hash,
        "targetPlatform": dict(node_candidate.TARGET_PLATFORM),
        "nativeHostExecutionAllowed": False,
        "supersededProductionWasmCopied": False,
    }


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-bundle", required=True)
    parser.add_argument("--expected-manifest-sha256", required=True)
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        summary = verify_final_freeze_runtime_bundle(
            Path(args.runtime_bundle),
            args.expected_manifest_sha256,
        )
    except (FreezeBundleError, OSError) as exc:
        print(f"final-freeze runtime bundle verification failed: {exc}", file=sys.stderr)
        return 2
    print(json.dumps(summary, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
