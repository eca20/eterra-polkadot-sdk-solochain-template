from __future__ import annotations

import base64
import importlib.util
import json
import hashlib
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock


SCRIPT = Path(__file__).with_name("release_lock.py")
SPEC = importlib.util.spec_from_file_location("release_lock_tested", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
tool = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = tool
SPEC.loader.exec_module(tool)


def write_json(path: Path, value: object) -> None:
    path.write_bytes(tool.canonical_bytes(value))


def ssh_string(value: bytes) -> bytes:
    return len(value).to_bytes(4, "big") + value


def encoded_ssh_key(key_type: str, seed: int) -> str:
    algorithm = ssh_string(key_type.encode("ascii"))
    if key_type == "ssh-ed25519":
        blob = algorithm + ssh_string(bytes([seed]) * 32)
    elif key_type == "ecdsa-sha2-nistp256":
        blob = algorithm + ssh_string(b"nistp256") + ssh_string(b"\x04" + bytes([seed]) * 64)
    elif key_type == "ssh-rsa":
        blob = (
            algorithm
            + ssh_string(b"\x01\x00\x01")
            + ssh_string(b"\x00\x80" + bytes([seed]) * 255)
        )
    else:
        raise AssertionError(key_type)
    return base64.b64encode(blob).decode("ascii")


def create_host_pin_artifacts(root: Path) -> tuple[Path, Path]:
    source = root / "source-known-hosts"
    lines: list[str] = []
    seed = 1
    for host in tool.ssh_host_pins.TARGET_HOSTS:
        for key_type in tool.ssh_host_pins.EXPECTED_KEY_TYPES:
            lines.append(f"{host} {key_type} {encoded_ssh_key(key_type, seed)}")
            seed += 1
    source.write_text("\n".join(lines) + "\n", encoding="ascii")
    source.chmod(0o600)
    known_hosts = root / "nexus-v2-alpha.known_hosts"
    manifest = root / "nexus-v2-alpha.known_hosts.json"
    tool.ssh_host_pins.capture(
        source.resolve(), known_hosts.resolve(), manifest.resolve()
    )
    return known_hosts, manifest


class ReleaseLockTests(unittest.TestCase):
    def create_repo(self, root: Path, name: str) -> dict[str, str]:
        root.mkdir()
        (root / "README.md").write_text(f"{name}\n", encoding="utf-8")
        if name == "chain":
            unit = root / "deploy/alpha/macmini2010/eterra-arcade-authority.service"
            unit.parent.mkdir(parents=True)
            unit.write_text("[Service]\nExecStart=/immutable/authority\n", encoding="utf-8")
        if name == "unity":
            verifier = root / "scripts/release/fps-server-candidate.py"
            verifier.parent.mkdir(parents=True)
            verifier.write_text(
                """#!/usr/bin/env python3
import json
import pathlib
import sys
if len(sys.argv) != 3 or sys.argv[1] != "verify":
    raise SystemExit(2)
path = pathlib.Path(sys.argv[2]) / "candidate-manifest.json"
value = json.loads(path.read_text(encoding="utf-8"))
if value.get("schema") != "eterra.nexus-v2-fps-dedicated-server-candidate.v2":
    raise SystemExit(1)
print('{"verified":true}')
""",
                encoding="utf-8",
            )
            verifier.chmod(0o755)
        subprocess.run(["git", "init", "-q", str(root)], check=True)
        subprocess.run(["git", "-C", str(root), "config", "user.name", "Lock Test"], check=True)
        subprocess.run(
            ["git", "-C", str(root), "config", "user.email", "lock@example.invalid"],
            check=True,
        )
        subprocess.run(["git", "-C", str(root), "add", "."], check=True)
        subprocess.run(["git", "-C", str(root), "commit", "-qm", "fixture"], check=True)
        return tool.repository_pin(str(root.resolve()), name)

    def test_critical_inputs_reject_leaf_and_ancestor_symlinks(self) -> None:
        with tempfile.TemporaryDirectory(prefix="release-lock-symlinks-") as temporary:
            root = Path(temporary).resolve()
            real = root / "real"
            real.mkdir()
            artifact = real / "artifact.json"
            write_json(artifact, {"schemaVersion": 1})
            leaf_alias = root / "leaf.json"
            leaf_alias.symlink_to(artifact)
            directory_alias = root / "alias"
            directory_alias.symlink_to(real, target_is_directory=True)
            with self.assertRaisesRegex(tool.ReleaseLockError, "canonical|symlink"):
                tool.file_pin(str(leaf_alias), "leaf alias", canonical_json=True)
            with self.assertRaisesRegex(tool.ReleaseLockError, "canonical|symlink"):
                tool.file_pin(
                    str(directory_alias / artifact.name),
                    "ancestor alias",
                    canonical_json=True,
                )

    def test_repository_root_rejects_symlink_ancestry(self) -> None:
        with tempfile.TemporaryDirectory(prefix="release-lock-repo-alias-") as temporary:
            root = Path(temporary).resolve()
            repository = root / "real" / "chain"
            repository.parent.mkdir()
            self.create_repo(repository, "chain")
            alias = root / "alias"
            alias.symlink_to(repository.parent, target_is_directory=True)
            with self.assertRaisesRegex(tool.ReleaseLockError, "canonical|symlink"):
                tool.repository_pin(str(alias / "chain"), "aliased repository")

    def test_output_rejects_symlink_parent_without_touching_target(self) -> None:
        with tempfile.TemporaryDirectory(prefix="release-lock-output-alias-") as temporary:
            root = Path(temporary).resolve()
            target_parent = root / "target"
            target_parent.mkdir()
            alias_parent = root / "alias"
            alias_parent.symlink_to(target_parent, target_is_directory=True)
            output = alias_parent / "release-lock.json"
            with self.assertRaisesRegex(tool.ReleaseLockError, "unsafe"):
                tool.write_new(output, {"schemaVersion": 1})
            self.assertFalse((target_parent / output.name).exists())

    def test_capture_and_verify_all_component_and_artifact_pins(self) -> None:
        with tempfile.TemporaryDirectory(prefix="release-lock-") as temporary:
            root = Path(temporary).resolve()
            repositories = {
                identifier: self.create_repo(root / identifier, identifier)
                for identifier in sorted(tool.REPOSITORY_IDS)
            }
            release_id = "nexus-v2-lock-test"
            runtime = root / "runtime.json"
            target = root / "target.json"
            node = root / "node.json"
            media = root / "media.json"
            receipt = root / "acceptance-receipt.json"
            read_model = root / "read-model.json"
            snapshot = root / "snapshot.json"
            ssh_known_hosts, ssh_host_pin_manifest = create_host_pin_artifacts(root)
            write_json(runtime, {"kind": "runtime", "schemaVersion": 1})
            runtime_code_sha256 = "8" * 64
            runtime_code_hash = "0x" + ("c" * 64)
            metadata_scale_sha256 = "9" * 64
            genesis_hash = "0x" + ("a" * 64)
            write_json(
                target,
                {
                    "deploymentSourceCommit": repositories["chain"]["head"],
                    "genesisHash": genesis_hash,
                    "kind": "target",
                    "releaseId": release_id,
                    "runtimeCodeHash": runtime_code_hash,
                    "runtimeMetadata": {"scaleSha256": metadata_scale_sha256},
                    "schemaVersion": 1,
                },
            )
            write_json(
                node,
                {
                    "deploymentSourceCommit": repositories["chain"]["head"],
                    "kind": "node",
                    "releaseId": release_id,
                    "runtimeBundle": {
                        "manifestSha256": tool.sha256_file(runtime),
                        "metadataScaleSha256": metadata_scale_sha256,
                        "productionWasmSha256": runtime_code_sha256,
                    },
                    "schemaVersion": 1,
                },
            )
            write_json(
                media,
                {
                    "chainDeployCommit": repositories["chain"]["head"],
                    "mediaSourceCommit": repositories["media"]["head"],
                    "schemaVersion": 1,
                },
            )
            authority_api = root / "authority-api"
            authority_operator = root / "authority-operator"
            (authority_api / "catalog").mkdir(parents=True)
            authority_operator.mkdir()
            catalog_bytes = b'{"fixture":"authority-catalog"}\n'
            tool.authority_candidate.CATALOG_SHA256 = hashlib.sha256(catalog_bytes).hexdigest()
            (authority_api / "catalog/eterra-legends.encounters.private-alpha.v1.json").write_bytes(catalog_bytes)
            (authority_api / "Eterra.Arcade.Authority.Api").write_bytes(b"api")
            (authority_api / "Eterra.Arcade.Authority.Api").chmod(0o755)
            (authority_operator / "Eterra.Arcade.Authority.Operator").write_bytes(b"operator")
            (authority_operator / "Eterra.Arcade.Authority.Operator").chmod(0o755)
            authority_release_manifest = root / "authority-release-manifest.json"
            authority_release_value = {
                "schema": tool.authority_candidate.SDK_RELEASE_SCHEMA,
                "files": tool.authority_candidate.scan_publish_trees(authority_api, authority_operator),
            }
            authority_release_manifest.write_bytes(
                tool.authority_candidate.sdk_manifest_bytes(authority_release_value)
            )
            tool.authority_candidate.SDK_RELEASE_MANIFEST_SHA256 = tool.sha256_file(
                authority_release_manifest
            )
            authority_signer = root / "authority-signer.public.json"
            authority_signer.write_text(
                json.dumps(
                    {
                        "publicKey": "0x" + ("12" * 32),
                        "scheme": "sr25519",
                        "ss58Address": "5CUQ61VmsjapsAVSAUuTZyGiLDPmi85ZGPCeCefm92CqFA9f",
                    },
                    sort_keys=True,
                    separators=(",", ":"),
                )
                + "\n",
                encoding="utf-8",
            )
            authority_candidate_root = root / "authority-candidate"
            tool.authority_candidate.assemble(
                SimpleNamespace(
                    api_tree=str(authority_api),
                    operator_tree=str(authority_operator),
                    release_manifest=str(authority_release_manifest),
                    public_signer=str(authority_signer),
                    service_unit=str(
                        Path(repositories["chain"]["root"])
                        / "deploy/alpha/macmini2010/eterra-arcade-authority.service"
                    ),
                    release_id=release_id,
                    chain_repository=repositories["chain"]["root"],
                    chain_commit=repositories["chain"]["head"],
                    sdkgen_repository=repositories["sdkgen"]["root"],
                    sdkgen_commit=repositories["sdkgen"]["head"],
                    genesis_hash=genesis_hash,
                    runtime_code_hash=runtime_code_hash,
                    runtime_code_sha256=runtime_code_sha256,
                    runtime_metadata_sha256=metadata_scale_sha256,
                    read_model_adapter_version="nexus-v2-game-results-runtime-106-storage-fixture-v2",
                    authority_epoch=7,
                    created_at="2026-07-31T12:00:00Z",
                    output=str(authority_candidate_root),
                )
            )
            authority_candidate = authority_candidate_root / tool.authority_candidate.CANDIDATE_NAME
            observed_block = {"number": 101, "hash": "0x" + ("b" * 64)}
            receipt_value = {
                "schemaVersion": 1,
                "kind": "nexus-v2-private-alpha-acceptance-boundary-receipt",
                "releaseId": release_id,
                "sourceCommit": repositories["chain"]["head"],
                "genesisHash": genesis_hash,
                "runtimeCodeSha256": runtime_code_sha256,
                "runtimeMetadataScaleSha256": metadata_scale_sha256,
                "observedAtFinalizedBlock": observed_block,
                "acceptanceBoundaryCaptureSha256": "1" * 64,
                "economicGatesSha256": "2" * 64,
                "acceptanceInventorySha256": "3" * 64,
                "postCutoverObservationSha256": "4" * 64,
                "coordinatorExecuteEvidenceSha256": "5" * 64,
                "coordinatorDecision": "keep-v2",
                "ingressClosedEvidenceSha256": "6" * 64,
                "ingressMode": "AllExternalWriteIngressClosed",
                "phase1SmokePassed": True,
                "automaticRestorePermanentlyDisabled": True,
                "operatorV2WriteScope": tool.acceptance_boundary.OPERATOR_SCOPE,
                "createdAtUtc": "2026-07-31T12:00:00Z",
            }
            write_json(receipt, receipt_value)
            receipt_sha256 = tool.sha256_file(receipt)
            unity_fps_candidate_root = root / "unity-fps-candidate"
            (unity_fps_candidate_root / "evidence").mkdir(parents=True)
            frozen_unity_sdk_commit = "d" * 40
            unity_sdk_manifest = unity_fps_candidate_root / "evidence/unity-sdk-manifest.json"
            write_json(
                unity_sdk_manifest,
                {"sdkSource": {"commit": frozen_unity_sdk_commit}},
            )
            unity_fps_candidate = unity_fps_candidate_root / "candidate-manifest.json"
            unity_fps_value = {
                "schema": "eterra.nexus-v2-fps-dedicated-server-candidate.v2",
                "environment": "private_alpha",
                "candidate_id": "nexus-v2-fps-fixture",
                "source": {
                    "repository": "Eterra-Arcade-Unity",
                    "commit": repositories["unity"]["head"],
                    "tree": repositories["unity"]["tree"],
                },
                "sdk": {
                    "commit": frozen_unity_sdk_commit,
                    "manifest_sha256": tool.sha256_file(unity_sdk_manifest),
                    "metadata_json_sha256": "ef" * 32,
                },
                "runtime": {
                    "spec_version": 106,
                    "chain_release_id": release_id,
                    "deployment_source_commit": repositories["chain"]["head"],
                    "genesis_hash": genesis_hash,
                    "runtime_code_sha256": runtime_code_sha256,
                    "runtime_metadata_scale_sha256": metadata_scale_sha256,
                },
                "game_results_acceptance": {
                    "acceptance_boundary_sha256": receipt_sha256,
                    "proof_policy_deactivated": True,
                },
                "safety": {
                    "economic_realm": "Training",
                    "normalized_legacy_rejects_persistent_power": True,
                    "paid_entry": False,
                    "wagering": False,
                    "permanent_asset_loss": False,
                    "marketplace": False,
                    "public_production": False,
                },
            }
            unity_fps_candidate.write_text(
                json.dumps(unity_fps_value, sort_keys=True, separators=(",", ":")) + "\n",
                encoding="utf-8",
            )
            write_json(
                read_model,
                {
                    "acceptanceBoundary": {
                        "automaticRestorePermanentlyDisabled": True,
                        "chainSourceCommit": repositories["chain"]["head"],
                        "coordinatorDecision": "keep-v2",
                        "genesisHash": genesis_hash,
                        "observedAtFinalizedBlock": observed_block,
                        "receiptKind": "nexus-v2-private-alpha-acceptance-boundary-receipt",
                        "receiptSha256": receipt_sha256,
                        "releaseId": release_id,
                        "runtimeCodeSha256": runtime_code_sha256,
                        "runtimeMetadataScaleSha256": metadata_scale_sha256,
                    },
                    "kind": "nexus-v2-private-alpha-exact-block-read-model-candidate",
                    "releaseSafety": {
                        "actionSubmissionEnabled": False,
                        "economicActivationEnabled": False,
                        "paidAcquisitionEnabled": False,
                        "publicReleaseEnabled": False,
                        "readModelOnly": True,
                    },
                    "runtimePins": {
                        "genesisHash": genesis_hash,
                        "metadataSha256": metadata_scale_sha256,
                        "specVersion": 106,
                    },
                    "schemaVersion": 1,
                    "source": {
                        "commit": repositories["ai"]["head"],
                        "tree": repositories["ai"]["tree"],
                    },
                },
            )
            write_json(snapshot, {"kind": "snapshot", "schemaVersion": 1})
            edit = root / "edit.xml"
            play = root / "play.xml"
            edit.write_text('<test-run result="Passed" total="590" passed="590" failed="0"/>\n', encoding="utf-8")
            play.write_text('<test-run result="Passed" total="25" passed="25" failed="0"/>\n', encoding="utf-8")
            environment = root / "release.env"
            environment.write_text(
                "\n".join(
                    (
                        f'ETERRA_RELEASE_VERSION="{release_id}"',
                        f'ETERRA_EXPECTED_CHAIN_COMMIT="{repositories["chain"]["head"]}"',
                        f'ETERRA_EXPECTED_MEDIA_COMMIT="{repositories["media"]["head"]}"',
                        f'ETERRA_EXPECTED_SDKGEN_COMMIT="{repositories["sdkgen"]["head"]}"',
                        f'NEXUS_V2_NODE_CANDIDATE_SHA256="{tool.sha256_file(node)}"',
                        f'NEXUS_V2_TARGET_IDENTITY_SHA256="{tool.sha256_file(target)}"',
                        f'NEXUS_V2_ALPHA_GENESIS_HASH="{genesis_hash}"',
                        f'RUNTIME_CODE_HASH="{runtime_code_hash}"',
                        'RUNTIME_SPEC_VERSION="106"',
                        'AUTHORITY_SUBMITTER_MODE="in_memory"',
                        'ETERRA_LEGENDS_READ_MODEL_URL="https://eterra.example.invalid/nexus-api"',
                        'ETERRA_LEGENDS_READ_MODEL_ADAPTER_VERSION="nexus-v2-game-results-runtime-106-storage-fixture-v2"',
                        'ETERRA_LEGENDS_AUTHORITY_EPOCH="7"',
                        'ETERRA_LEGENDS_SIGNER_MNEMONIC="@/secure/authority.mnemonic"',
                        'ETERRA_LEGENDS_PRIVATE_ALPHA_ACCESS_KEY="@/secure/read-model.key"',
                        f'NEXUS_V2_AUTHORITY_CANDIDATE_PATH="{authority_candidate}"',
                        f'NEXUS_V2_AUTHORITY_CANDIDATE_SHA256="{tool.sha256_file(authority_candidate)}"',
                        f'NEXUS_V2_SSH_KNOWN_HOSTS_FILE="{ssh_known_hosts}"',
                        f'NEXUS_V2_SSH_KNOWN_HOSTS_SHA256="{tool.sha256_file(ssh_known_hosts)}"',
                        f'NEXUS_V2_SSH_HOST_PIN_MANIFEST="{ssh_host_pin_manifest}"',
                        f'NEXUS_V2_SSH_HOST_PIN_MANIFEST_SHA256="{tool.sha256_file(ssh_host_pin_manifest)}"',
                    )
                )
                + "\n",
                encoding="utf-8",
            )
            site_environment = root / "site-release.env"
            site_environment.write_text(
                "\n".join(
                    (
                        f'EXPECTED_SOURCE_COMMIT="{repositories["web"]["head"]}"',
                        "PUBLIC_MEDIA_UPLOAD_ENABLED=false",
                        "PUBLIC_AVATAR_UPLOAD_ENABLED=false",
                        "NEXUS_V2_SESSION_AUTHORIZATION_PRODUCTION_ENABLED=false",
                        f'NEXUS_V2_SSH_KNOWN_HOSTS_FILE="{ssh_known_hosts}"',
                        f'NEXUS_V2_SSH_KNOWN_HOSTS_SHA256="{tool.sha256_file(ssh_known_hosts)}"',
                        f'NEXUS_V2_SSH_HOST_PIN_MANIFEST="{ssh_host_pin_manifest}"',
                        f'NEXUS_V2_SSH_HOST_PIN_MANIFEST_SHA256="{tool.sha256_file(ssh_host_pin_manifest)}"',
                    )
                )
                + "\n",
                encoding="utf-8",
            )
            stale = root / "alpha-release-final.env"
            stale.write_text("ETERRA_RELEASE_VERSION=stale\n", encoding="utf-8")
            unity_fps_environment = root / "unity-fps-release.env"
            unity_fps_environment.write_text("FPS_ALPHA_PUBLIC_PRODUCTION_ENABLED=0\n", encoding="utf-8")
            site_candidate = root / "site-candidate.json"
            site_phase1 = root / "site-phase1.json"
            full_loop = root / "full-loop.json"
            site_phase2 = root / "site-phase2.json"
            phase2_handoff = root / "phase2-handoff.json"
            for path, kind in (
                (site_candidate, "site-candidate"),
                (site_phase1, "site-phase1"),
                (full_loop, "full-loop"),
                (site_phase2, "site-phase2"),
                (phase2_handoff, "phase2-handoff"),
            ):
                write_json(path, {"kind": kind, "schemaVersion": 1})

            common_args = dict(
                release_id=release_id,
                repository=[
                    f"{identifier}={repositories[identifier]['root']}"
                    for identifier in sorted(tool.REPOSITORY_IDS)
                ],
                deployment_environment=str(environment),
                site_deployment_environment=str(site_environment),
                unity_fps_deployment_environment=str(unity_fps_environment),
                ssh_known_hosts=str(ssh_known_hosts),
                ssh_host_pin_manifest=str(ssh_host_pin_manifest),
                forbidden_deployment_environment=[str(stale)],
                runtime_bundle_manifest=str(runtime),
                target_identity=str(target),
                node_candidate_manifest=str(node),
                media_candidate_manifest=str(media),
                authority_candidate_manifest=str(authority_candidate),
                site_deployment_candidate_manifest=str(site_candidate),
                snapshot_manifest=str(snapshot),
                unity_editmode_results=str(edit),
                unity_playmode_results=str(play),
                created_at="2026-07-31T12:00:00Z",
            )
            replacement_output = root / "replacement-lock.json"
            replacement_args = SimpleNamespace(
                **common_args,
                output=str(replacement_output),
            )
            output = root / "release-lock.json"
            args = SimpleNamespace(
                **common_args,
                replacement_lock=str(replacement_output),
                acceptance_boundary_receipt=str(receipt),
                read_model_manifest=str(read_model),
                unity_fps_candidate_manifest=str(unity_fps_candidate),
                site_phase1_post_deploy_identity=str(site_phase1),
                full_loop_indexer_activation_receipt=str(full_loop),
                site_post_phase2_deployment_identity=str(site_phase2),
                phase2_internal_transport_handoff=str(phase2_handoff),
                output=str(output),
            )

            with mock.patch.object(tool, "validate_semantic_pins"):
                tool.command_capture_replacement(replacement_args)
                replacement_digest = tool.sha256_file(replacement_output)
                replacement = tool.validate_replacement_lock(
                    replacement_output,
                    replacement_digest,
                    str(environment),
                    str(site_environment),
                )
                self.assertEqual(replacement["kind"], tool.REPLACEMENT_LOCK_KIND)
                self.assertNotIn("readModelManifest", replacement["artifacts"])
                self.assertNotIn("acceptanceBoundaryReceipt", replacement["artifacts"])
                self.assertNotIn("unityFpsCandidateManifest", replacement["artifacts"])
                with self.assertRaises(tool.ReleaseLockError):
                    tool.validate_lock(
                        replacement_output,
                        replacement_digest,
                        str(environment),
                        str(site_environment),
                    )

                tool.command_capture(args)
                digest = tool.sha256_file(output)
                value = tool.validate_lock(
                    output, digest, str(environment), str(site_environment)
                )
                self.assertEqual(set(value["repositories"]), tool.REPOSITORY_IDS)
                self.assertEqual(
                    value["artifacts"]["unityTestResults"]["editMode"]["total"], 590
                )
                self.assertIn("unityFpsCandidateManifest", value["artifacts"])
                self.assertEqual(
                    value["artifacts"]["sshKnownHosts"]["sha256"],
                    tool.sha256_file(ssh_known_hosts),
                )
                with self.assertRaises(tool.ReleaseLockError):
                    tool.validate_lock(output, digest, str(stale), str(site_environment))
                selected_alias = root / "selected-release.env"
                selected_alias.symlink_to(environment)
                with self.assertRaisesRegex(tool.ReleaseLockError, "canonical|symlink"):
                    tool.validate_lock(
                        output,
                        digest,
                        str(selected_alias),
                        str(site_environment),
                    )

            environment.write_text("ETERRA_RELEASE_VERSION=stale\n", encoding="utf-8")
            with mock.patch.object(tool, "validate_semantic_pins"):
                with self.assertRaises(tool.ReleaseLockError):
                    tool.validate_lock(output, digest, str(environment), str(site_environment))


if __name__ == "__main__":
    unittest.main()
