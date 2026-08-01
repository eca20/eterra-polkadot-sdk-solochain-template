from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace


SCRIPT = Path(__file__).with_name("release_lock.py")
SPEC = importlib.util.spec_from_file_location("release_lock_tested", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
tool = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = tool
SPEC.loader.exec_module(tool)


def write_json(path: Path, value: object) -> None:
    path.write_bytes(tool.canonical_bytes(value))


class ReleaseLockTests(unittest.TestCase):
    def create_repo(self, root: Path, name: str) -> dict[str, str]:
        root.mkdir()
        (root / "README.md").write_text(f"{name}\n", encoding="utf-8")
        subprocess.run(["git", "init", "-q", str(root)], check=True)
        subprocess.run(["git", "-C", str(root), "config", "user.name", "Lock Test"], check=True)
        subprocess.run(
            ["git", "-C", str(root), "config", "user.email", "lock@example.invalid"],
            check=True,
        )
        subprocess.run(["git", "-C", str(root), "add", "."], check=True)
        subprocess.run(["git", "-C", str(root), "commit", "-qm", "fixture"], check=True)
        return tool.repository_pin(str(root.resolve()), name)

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
            write_json(runtime, {"kind": "runtime", "schemaVersion": 1})
            runtime_code_sha256 = "8" * 64
            metadata_scale_sha256 = "9" * 64
            genesis_hash = "0x" + ("a" * 64)
            write_json(
                target,
                {
                    "deploymentSourceCommit": repositories["chain"]["head"],
                    "genesisHash": genesis_hash,
                    "kind": "target",
                    "releaseId": release_id,
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
                    )
                )
                + "\n",
                encoding="utf-8",
            )
            stale = root / "alpha-release-final.env"
            stale.write_text("ETERRA_RELEASE_VERSION=stale\n", encoding="utf-8")
            output = root / "release-lock.json"
            args = SimpleNamespace(
                release_id=release_id,
                repository=[
                    f"{identifier}={repositories[identifier]['root']}"
                    for identifier in sorted(tool.REPOSITORY_IDS)
                ],
                deployment_environment=str(environment),
                site_deployment_environment=str(site_environment),
                forbidden_deployment_environment=[str(stale)],
                runtime_bundle_manifest=str(runtime),
                target_identity=str(target),
                node_candidate_manifest=str(node),
                media_candidate_manifest=str(media),
                acceptance_boundary_receipt=str(receipt),
                read_model_manifest=str(read_model),
                snapshot_manifest=str(snapshot),
                unity_editmode_results=str(edit),
                unity_playmode_results=str(play),
                created_at="2026-07-31T12:00:00Z",
                output=str(output),
            )
            tool.command_capture(args)
            digest = tool.sha256_file(output)
            value = tool.validate_lock(output, digest, str(environment), str(site_environment))
            self.assertEqual(set(value["repositories"]), tool.REPOSITORY_IDS)
            self.assertEqual(value["artifacts"]["unityTestResults"]["editMode"]["total"], 590)
            with self.assertRaises(tool.ReleaseLockError):
                tool.validate_lock(output, digest, str(stale), str(site_environment))

            replacement_output = root / "replacement-lock.json"
            replacement_args = SimpleNamespace(
                **{
                    **vars(args),
                    "output": str(replacement_output),
                }
            )
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
            with self.assertRaises(tool.ReleaseLockError):
                tool.validate_lock(
                    replacement_output,
                    replacement_digest,
                    str(environment),
                    str(site_environment),
                )

            invalid_read_model = root / "read-model-invalid.json"
            invalid_value = json.loads(read_model.read_text(encoding="utf-8"))
            invalid_value["acceptanceBoundary"]["receiptSha256"] = "0" * 64
            write_json(invalid_read_model, invalid_value)
            invalid_args = SimpleNamespace(
                **{
                    **vars(args),
                    "read_model_manifest": str(invalid_read_model),
                    "output": str(root / "invalid-final-lock.json"),
                }
            )
            with self.assertRaisesRegex(tool.ReleaseLockError, "acceptance binding"):
                tool.command_capture(invalid_args)

            environment.write_text("ETERRA_RELEASE_VERSION=stale\n", encoding="utf-8")
            with self.assertRaises(tool.ReleaseLockError):
                tool.validate_lock(output, digest, str(environment), str(site_environment))


if __name__ == "__main__":
    unittest.main()
