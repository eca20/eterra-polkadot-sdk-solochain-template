#!/usr/bin/env python3

from __future__ import annotations

import dataclasses
import hashlib
import json
import os
import struct
import sys
import tempfile
import unittest
from pathlib import Path
from typing import Any


sys.path.insert(0, str(Path(__file__).resolve().parent))
import final_freeze_runtime_bundle as tool  # noqa: E402


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def elf64() -> bytes:
    value = bytearray(64)
    value[:7] = b"\x7fELF\x02\x01\x01"
    struct.pack_into("<H", value, 16, 3)
    struct.pack_into("<H", value, 18, 62)
    return bytes(value)


class FinalFreezeRuntimeBundleTests(unittest.TestCase):
    def setUp(self) -> None:
        self.context = tempfile.TemporaryDirectory(prefix="nexus-v2-freeze-runtime-test-")
        self.root = Path(self.context.name)

    def tearDown(self) -> None:
        self.context.cleanup()

    def make_bundle(self) -> tuple[Path, tool.ReleasePins]:
        root = self.root / "runtime"
        root.mkdir()
        files = {
            "solochain-eterra-node": elf64(),
            "runtime-spec-106.compact.compressed.wasm": b"linux-production",
            "runtime-spec-106.temporary-node-embedded-code.wasm": b"linux-production",
            "runtime-spec-106.try-runtime.wasm": b"try-runtime-wasm",
            "runtime-metadata.scale": b"meta\x0ffixture",
            "runtime-metadata.json": b'{"fixture":true}\n',
            "nexus-v2-migration-verifier.linux-amd64": b"linux-verifier",
            "nexus-v2-migration-verifier": b"host-verifier",
            "try-runtime": b"host-try-runtime",
        }
        for name, payload in files.items():
            path = root / name
            path.write_bytes(payload)
            if name in {"solochain-eterra-node", "nexus-v2-migration-verifier.linux-amd64", "nexus-v2-migration-verifier", "try-runtime"}:
                path.chmod(0o700)

        source = "1" * 40
        source_tree = "2" * 40
        prior_source = "3" * 40
        prior_manifest = "4" * 64
        old_wasm = "5" * 64
        assembly_commit = "6" * 40
        assembly_tree = "7" * 40
        genesis = "0x" + "8" * 64
        production = sha256(root / "runtime-spec-106.compact.compressed.wasm")
        metadata_scale = sha256(root / "runtime-metadata.scale")
        metadata_json = sha256(root / "runtime-metadata.json")

        raw_spec = {"genesis": {"raw": {"top": {"0x3a636f6465": "0x" + files["runtime-spec-106.compact.compressed.wasm"].hex()}}}}
        write_json(root / "runtime-spec-106.dev-chain-spec.raw.json", raw_spec)
        write_json(
            root / "metadata-compatibility.json",
            {
                "schemaVersion": 1,
                "kind": "nexus-v2-runtime-metadata-compatibility-proof",
                "baseline": {
                    "bundleManifestSha256": prior_manifest,
                    "sourceCommit": prior_source,
                    "metadataScaleSha256": metadata_scale,
                    "metadataJsonSha256": metadata_json,
                },
                "candidate": {
                    "sourceCommit": source,
                    "metadataScaleSha256": metadata_scale,
                    "metadataJsonSha256": metadata_json,
                    "metadataVersion": 15,
                },
                "exactScaleBytesEqual": True,
                "exactDecodedJsonBytesEqual": True,
                "sourceDeltaIsReleaseToolingOnly": True,
                "result": "compatible",
            },
        )
        write_json(
            root / "superseded-runtime-identity.json",
            {
                "schemaVersion": 1,
                "kind": "nexus-v2-superseded-runtime-identity",
                "sourceCommit": prior_source,
                "bundleManifestSha256": prior_manifest,
                "productionWasmSha256": old_wasm,
                "status": "superseded-not-a-release-target",
                "productionWasmCopiedIntoBundle": False,
                "authorizations": {"deploy": False, "release": False, "restoreTarget": False},
            },
        )
        write_json(
            root / "runtime-support-build-attestation.json",
            {
                "schemaVersion": 1,
                "kind": "nexus-v2-linux-runtime-support-build",
                "sourceCommit": source,
                "sourceTree": source_tree,
                "targetPlatform": tool.node_candidate.TARGET_PLATFORM,
                "buildEnvironment": {
                    "buildkitPlatform": "linux/amd64",
                    "containerImage": tool.node_candidate.PINNED_LINUX_AMD64_BUILD_IMAGE,
                    "cargoLocked": True,
                    "incremental": False,
                },
                "artifacts": {
                    "linuxMigrationVerifierSha256": sha256(root / "nexus-v2-migration-verifier.linux-amd64"),
                    "tryRuntimeWasmSha256": sha256(root / "runtime-spec-106.try-runtime.wasm"),
                },
                "authorizations": {"paidProduction": False, "publicDeploy": False, "publicRelease": False},
            },
        )
        write_json(
            root / "deployment-node-attestation.json",
            {
                "schemaVersion": 1,
                "kind": "nexus-v2-linux-amd64-deployment-node-build",
                "sourceCommit": source,
                "targetPlatform": tool.node_candidate.TARGET_PLATFORM,
                "buildEnvironment": {
                    "buildkitPlatform": "linux/amd64",
                    "containerImage": tool.node_candidate.PINNED_LINUX_AMD64_BUILD_IMAGE,
                    "cargoLocked": True,
                    "incremental": False,
                    "runtimeProductionFeature": True,
                },
                "artifacts": {
                    "nativeNodeSha256": sha256(root / "solochain-eterra-node"),
                    "productionWasmSha256": production,
                },
                "authorizations": {"paidProduction": False, "publicDeploy": False, "publicRelease": False},
            },
        )
        write_json(
            root / "linux-runtime-probe-result.json",
            {
                "schemaVersion": 1,
                "kind": "nexus-v2-linux-runtime-probe-result",
                "specVersion": 106,
                "metadataVersion": 15,
                "genesisHash": genesis,
                "embeddedCodeMatchesProductionWasm": True,
                "networkDisabledByRunner": True,
                "readOnlyRootFilesystemByRunner": True,
                "ephemeralWritableEvidenceWorkspace": True,
            },
        )
        artifact_hashes = {
            "nativeNodeSha256": sha256(root / "solochain-eterra-node"),
            "stagedProductionWasmSha256": production,
            "temporaryNodeEmbeddedWasmSha256": production,
            "metadataScaleSha256": metadata_scale,
            "metadataJsonSha256": metadata_json,
            "tryRuntimeWasmSha256": sha256(root / "runtime-spec-106.try-runtime.wasm"),
            "linuxMigrationVerifierSha256": sha256(root / "nexus-v2-migration-verifier.linux-amd64"),
            "migrationVerifierSha256": sha256(root / "nexus-v2-migration-verifier"),
            "tryRuntimeCliSha256": sha256(root / "try-runtime"),
            "runtimeSupportBuildAttestationSha256": sha256(root / "runtime-support-build-attestation.json"),
            "deploymentNodeAttestationSha256": sha256(root / "deployment-node-attestation.json"),
        }
        manifest = {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-runtime-bundle",
            "releaseId": f"nexus-v2-private-alpha-linux-{source[:12]}",
            "sourceCommit": source,
            "sourceTree": source_tree,
            "sourceStorageVersion": 14,
            "targetStorageVersion": 16,
            "targetSpecVersion": 106,
            "runtimeIdentity": {
                "authoritativeTargetPlatform": tool.node_candidate.TARGET_PLATFORM,
                "candidateRunnerSha256": tool.sha256_file(tool.node_candidate.LINUX_AMD64_RUNNER),
                "probeRunnerSha256": tool.sha256_file(tool.node_candidate.LINUX_RUNTIME_PROBE_RUNNER),
                "devChainSpecMatchesStagedProductionCode": True,
                "metadataScaleAndJsonExactlyMatchCompatibilityBaseline": True,
                "nativeHostExecutionAllowed": False,
                "probeEphemeralWorkspaceOnly": True,
                "probeReadOnlyRootFilesystem": True,
                "probeRunnerNetworkDisabled": True,
                "stagedProductionMatchesTemporaryNodeEmbeddedCode": True,
                "genesisHash": genesis,
            },
            "artifacts": artifact_hashes,
            "tools": {
                "assemblySourceCommit": assembly_commit,
                "assemblySourceTree": assembly_tree,
                "assemblyDeltaIsReleaseToolingOnly": True,
            },
            "authorizations": {
                "externalReviewsSelfApproved": False,
                "localBuildOnly": True,
                "paidProduction": False,
                "publicDeploy": False,
                "publicRelease": False,
            },
        }
        write_json(root / "runtime-bundle-manifest.json", manifest)
        checksummed = sorted(path for path in root.iterdir() if path.is_file())
        (root / "SHA256SUMS").write_text(
            "".join(f"{sha256(path)}  {path.name}\n" for path in checksummed),
            encoding="utf-8",
        )
        manifest_sha = sha256(root / "runtime-bundle-manifest.json")
        pins = tool.ReleasePins(
            manifest_sha256=manifest_sha,
            sha256_sums_sha256=sha256(root / "SHA256SUMS"),
            bundle_files=frozenset(path.name for path in root.iterdir()),
            source_commit=source,
            source_tree=source_tree,
            assembly_commit=assembly_commit,
            assembly_tree=assembly_tree,
            production_wasm_sha256=production,
            superseded_wasm_sha256=old_wasm,
            native_node_sha256=artifact_hashes["nativeNodeSha256"],
            metadata_scale_sha256=metadata_scale,
            metadata_json_sha256=metadata_json,
            try_runtime_wasm_sha256=artifact_hashes["tryRuntimeWasmSha256"],
            linux_migration_verifier_sha256=artifact_hashes["linuxMigrationVerifierSha256"],
            host_migration_verifier_sha256=artifact_hashes["migrationVerifierSha256"],
            try_runtime_cli_sha256=artifact_hashes["tryRuntimeCliSha256"],
            prior_source_commit=prior_source,
            prior_manifest_sha256=prior_manifest,
            genesis_hash=genesis,
        )
        return root, pins

    def rewrite_sums(self, root: Path) -> str:
        checksummed = sorted(path for path in root.iterdir() if path.is_file() and path.name != "SHA256SUMS")
        (root / "SHA256SUMS").write_text(
            "".join(f"{sha256(path)}  {path.name}\n" for path in checksummed),
            encoding="utf-8",
        )
        return sha256(root / "runtime-bundle-manifest.json")

    def test_accepts_exact_linux_fixture(self) -> None:
        root, pins = self.make_bundle()
        summary = tool.verify_final_freeze_runtime_bundle(root, pins.manifest_sha256, pins)
        self.assertEqual(summary["manifestSha256"], pins.manifest_sha256)
        self.assertFalse(summary["nativeHostExecutionAllowed"])
        self.assertFalse(summary["supersededProductionWasmCopied"])

    def test_rejects_superseded_retry1_macos_identity(self) -> None:
        root, pins = self.make_bundle()
        manifest_path = root / "runtime-bundle-manifest.json"
        manifest = json.loads(manifest_path.read_text())
        manifest["sourceCommit"] = tool.PRODUCTION_PINS.prior_source_commit
        manifest["artifacts"]["stagedProductionWasmSha256"] = tool.PRODUCTION_PINS.superseded_wasm_sha256
        write_json(manifest_path, manifest)
        old_manifest_sha = self.rewrite_sums(root)
        old_pins = dataclasses.replace(pins, manifest_sha256=old_manifest_sha)
        with self.assertRaisesRegex(tool.FreezeBundleError, "Wasm|source commit"):
            tool.verify_final_freeze_runtime_bundle(root, old_manifest_sha, old_pins)

    def test_production_contract_rejects_any_other_manifest(self) -> None:
        root, pins = self.make_bundle()
        with self.assertRaisesRegex(tool.FreezeBundleError, "frozen Linux release manifest"):
            tool.verify_final_freeze_runtime_bundle(root, pins.manifest_sha256)

    def test_rejects_native_host_fallback(self) -> None:
        root, pins = self.make_bundle()
        manifest_path = root / "runtime-bundle-manifest.json"
        manifest = json.loads(manifest_path.read_text())
        manifest["runtimeIdentity"]["nativeHostExecutionAllowed"] = True
        write_json(manifest_path, manifest)
        changed_sha = self.rewrite_sums(root)
        changed_pins = dataclasses.replace(pins, manifest_sha256=changed_sha)
        with self.assertRaisesRegex(tool.FreezeBundleError, "native host execution fallback|native-host fallback"):
            tool.verify_final_freeze_runtime_bundle(root, changed_sha, changed_pins)

    def test_rejects_reordered_sha256sums(self) -> None:
        root, pins = self.make_bundle()
        sums = root / "SHA256SUMS"
        sums.write_text("\n".join(reversed(sums.read_text().splitlines())) + "\n", encoding="utf-8")
        with self.assertRaisesRegex(tool.FreezeBundleError, "SHA256SUMS identity"):
            tool.verify_final_freeze_runtime_bundle(root, pins.manifest_sha256, pins)

    def test_rejects_unlisted_extra_file(self) -> None:
        root, pins = self.make_bundle()
        (root / "unexpected.txt").write_text("not checksummed\n", encoding="utf-8")
        with self.assertRaisesRegex(tool.FreezeBundleError, "closed file set"):
            tool.verify_final_freeze_runtime_bundle(root, pins.manifest_sha256, pins)


if __name__ == "__main__":
    unittest.main()
