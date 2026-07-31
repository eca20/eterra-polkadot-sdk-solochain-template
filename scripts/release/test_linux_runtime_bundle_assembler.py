#!/usr/bin/env python3

from __future__ import annotations

import argparse
import importlib.util
import json
import struct
import subprocess
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("assemble-nexus-v2-linux-runtime-bundle.py")
SPEC = importlib.util.spec_from_file_location("linux_runtime_bundle_assembler", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
tool = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(tool)


def write_json(path: Path, value: dict) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def elf64(machine: int = 62) -> bytes:
    header = bytearray(64)
    header[:7] = b"\x7fELF\x02\x01\x01"
    struct.pack_into("<H", header, 16, 3)
    struct.pack_into("<H", header, 18, machine)
    return bytes(header) + b"fixture"


class LinuxRuntimeBundleAssemblerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.context = tempfile.TemporaryDirectory(prefix="nexus-v2-linux-bundle-test-")
        self.root = Path(self.context.name)
        self.source_commit = subprocess.check_output(
            ["git", "-C", str(tool.REPO_ROOT), "rev-parse", "HEAD"], text=True
        ).strip()
        self.source_epoch = int(
            subprocess.check_output(
                ["git", "-C", str(tool.REPO_ROOT), "show", "-s", "--format=%ct", self.source_commit],
                text=True,
            ).strip()
        )

    def tearDown(self) -> None:
        self.context.cleanup()

    def make_linux_build(self, *, platform: dict | None = None) -> tuple[Path, str]:
        build = self.root / "linux-build"
        build.mkdir()
        node = build / "solochain-eterra-node"
        node.write_bytes(elf64())
        node.chmod(0o700)
        wasm = build / "runtime-spec-106.compact.compressed.wasm"
        wasm.write_bytes(b"linux-production-wasm")
        (build / "buildkit-metadata.json").write_text("{}\n", encoding="utf-8")
        value = {
            "schemaVersion": 1,
            "kind": "nexus-v2-linux-amd64-deployment-node-build",
            "sourceCommit": self.source_commit,
            "sourceDateEpoch": self.source_epoch,
            "targetPlatform": platform or dict(tool.TARGET_PLATFORM),
            "buildEnvironment": {
                "buildkitPlatform": "linux/amd64",
                "cargoLocked": True,
                "containerImage": tool.PINNED_IMAGE,
                "dockerfileSha256": tool.committed_file_sha(
                    self.source_commit, "scripts/release/Dockerfile.node-linux-amd64"
                ),
                "incremental": False,
                "runtimeProductionFeature": True,
                "rustc": tool.RUSTC_VERSION,
            },
            "artifacts": {
                "buildkitMetadataSha256": tool.sha256_file(build / "buildkit-metadata.json"),
                "nativeNodeSha256": tool.sha256_file(node),
                "productionWasmSha256": tool.sha256_file(wasm),
            },
            "authorizations": {"paidProduction": False, "publicDeploy": False, "publicRelease": False},
        }
        write_json(build / "deployment-node-attestation.json", value)
        names = (
            "solochain-eterra-node",
            "runtime-spec-106.compact.compressed.wasm",
            "buildkit-metadata.json",
            "deployment-node-attestation.json",
        )
        (build / "SHA256SUMS").write_text(
            "".join(f"{tool.sha256_file(build / name)}  {name}\n" for name in names),
            encoding="utf-8",
        )
        return build, tool.sha256_file(wasm)

    def test_accepts_closed_attested_linux_build(self) -> None:
        build, wasm_sha = self.make_linux_build()
        result = tool.verify_linux_build(build, self.source_commit, wasm_sha)
        self.assertEqual(result["wasmSha256"], wasm_sha)

    def test_rejects_wrong_target_platform(self) -> None:
        wrong = dict(tool.TARGET_PLATFORM)
        wrong["architecture"] = "aarch64"
        build, wasm_sha = self.make_linux_build(platform=wrong)
        with self.assertRaisesRegex(tool.AssemblyError, "target platform mismatch"):
            tool.verify_linux_build(build, self.source_commit, wasm_sha)

    def test_rejects_swapped_node_after_attestation(self) -> None:
        build, wasm_sha = self.make_linux_build()
        node = build / "solochain-eterra-node"
        node.write_bytes(elf64() + b"swapped")
        lines = []
        for name in (
            "solochain-eterra-node",
            "runtime-spec-106.compact.compressed.wasm",
            "buildkit-metadata.json",
            "deployment-node-attestation.json",
        ):
            lines.append(f"{tool.sha256_file(build / name)}  {name}\n")
        (build / "SHA256SUMS").write_text("".join(lines), encoding="utf-8")
        with self.assertRaisesRegex(tool.AssemblyError, "attestation hash mismatch"):
            tool.verify_linux_build(build, self.source_commit, wasm_sha)

    def test_rejects_swapped_runner_and_node_roles(self) -> None:
        build, _ = self.make_linux_build()
        with self.assertRaisesRegex(tool.AssemblyError, "runner was swapped"):
            tool.validate_probe_boundary(
                build / "solochain-eterra-node",
                tool.PROBE,
                build / "solochain-eterra-node",
            )

    def test_probe_command_has_no_native_node_fallback(self) -> None:
        command = tool.runtime_probe_command(self.root)
        self.assertEqual(Path(command[0]).resolve(), tool.PROBE_RUNNER.resolve())
        self.assertNotEqual(Path(command[0]).name, "solochain-eterra-node")
        self.assertIn("/work/solochain-eterra-node", command)
        runner = tool.PROBE_RUNNER.read_text(encoding="utf-8")
        self.assertIn("--network none", runner)
        self.assertNotIn("uname -m", runner)

    def test_rejects_old_macos_wasm_as_new_target_before_io(self) -> None:
        old = "f" * 64
        args = argparse.Namespace(
            source_commit=self.source_commit,
            expected_production_wasm_sha256=old,
            expected_superseded_wasm_sha256=old,
            expected_metadata_scale_sha256="1" * 64,
            expected_metadata_json_sha256="2" * 64,
            subxt_sha256="3" * 64,
            try_runtime_revision="a" * 40,
            output=str(self.root / "output"),
            linux_build_root=str(self.root / "missing-linux"),
            prior_runtime_bundle=str(self.root / "missing-prior"),
            subxt_bin=str(self.root / "missing-subxt"),
        )
        with self.assertRaisesRegex(tool.AssemblyError, "old macOS production Wasm"):
            tool.assemble(args)


if __name__ == "__main__":
    unittest.main()
