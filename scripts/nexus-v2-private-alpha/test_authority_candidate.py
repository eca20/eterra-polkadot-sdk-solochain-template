#!/usr/bin/env python3

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace


sys.path.insert(0, str(Path(__file__).resolve().parent))
import authority_candidate as tool  # noqa: E402


def write_canonical(path: Path, value: dict[str, object]) -> None:
    path.write_bytes(tool.canonical_bytes(value))


class AuthorityCandidateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="authority-candidate-")
        self.root = Path(self.temporary.name).resolve()
        self.api = self.root / "published-api"
        self.operator = self.root / "published-operator"
        (self.api / "catalog").mkdir(parents=True)
        self.operator.mkdir()
        self.catalog_bytes = b'{"catalog":"fixture"}\n'
        tool.CATALOG_SHA256 = hashlib.sha256(self.catalog_bytes).hexdigest()
        (self.api / "catalog" / "eterra-legends.encounters.private-alpha.v1.json").write_bytes(
            self.catalog_bytes
        )
        (self.api / "Eterra.Arcade.Authority.Api").write_bytes(b"api-binary")
        (self.api / "Eterra.Arcade.Authority.Api").chmod(0o755)
        (self.api / "Microsoft.Extensions.Configuration.UserSecrets.dll").write_bytes(
            b"reviewed-runtime-assembly"
        )
        (self.operator / "Eterra.Arcade.Authority.Operator").write_bytes(b"operator-binary")
        (self.operator / "Eterra.Arcade.Authority.Operator").chmod(0o755)
        self.release_manifest = self.root / "release-manifest.json"
        manifest = {
            "schema": tool.SDK_RELEASE_SCHEMA,
            "files": tool.scan_publish_trees(self.api, self.operator),
        }
        self.release_manifest.write_bytes(tool.sdk_manifest_bytes(manifest))
        tool.SDK_RELEASE_MANIFEST_SHA256 = tool.sha256_file(self.release_manifest)
        self.signer = self.root / "signer.json"
        self.public_key = "0x" + ("12" * 32)
        self.signer.write_text(
            json.dumps(
                {
                    "publicKey": self.public_key,
                    "scheme": "sr25519",
                    "ss58Address": "5CUQ61VmsjapsAVSAUuTZyGiLDPmi85ZGPCeCefm92CqFA9f",
                },
                sort_keys=True,
                separators=(",", ":"),
            )
            + "\n",
            encoding="utf-8",
        )
        self.service_unit = self.root / "eterra-arcade-authority.service"
        self.service_unit.write_text("[Service]\nExecStart=/immutable/api\n", encoding="utf-8")
        self.chain = self.create_repo("chain")
        self.sdkgen = self.create_repo("sdkgen")
        self.candidate_root = self.root / "candidate"
        self.assemble()
        self.candidate_path = self.candidate_root / tool.CANDIDATE_NAME
        self.candidate_sha = tool.sha256_file(self.candidate_path)

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def create_repo(self, name: str) -> Path:
        root = self.root / name
        root.mkdir()
        (root / "README.md").write_text(name + "\n", encoding="utf-8")
        subprocess.run(["git", "init", "-q", str(root)], check=True)
        subprocess.run(["git", "-C", str(root), "config", "user.name", "Fixture"], check=True)
        subprocess.run(["git", "-C", str(root), "config", "user.email", "fixture@example.invalid"], check=True)
        subprocess.run(["git", "-C", str(root), "add", "."], check=True)
        subprocess.run(["git", "-C", str(root), "commit", "-qm", "fixture"], check=True)
        return root

    def commit(self, root: Path) -> str:
        return subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True).strip()

    def assemble(self, output: Path | None = None) -> None:
        tool.assemble(
            SimpleNamespace(
                api_tree=str(self.api),
                operator_tree=str(self.operator),
                release_manifest=str(self.release_manifest),
                public_signer=str(self.signer),
                service_unit=str(self.service_unit),
                release_id="nexus-v2-authority-test",
                chain_repository=str(self.chain),
                chain_commit=self.commit(self.chain),
                sdkgen_repository=str(self.sdkgen),
                sdkgen_commit=self.commit(self.sdkgen),
                genesis_hash="0x" + ("34" * 32),
                runtime_code_hash="0x" + ("56" * 32),
                runtime_code_sha256="78" * 32,
                runtime_metadata_sha256="9a" * 32,
                read_model_adapter_version="nexus-v2-game-results-runtime-106-storage-fixture-v2",
                authority_epoch=7,
                created_at="2026-08-01T00:00:00Z",
                output=str(output or self.candidate_root),
            )
        )

    def test_assemble_and_verify_closed_relocatable_candidate(self) -> None:
        candidate = tool.validate_candidate(
            self.candidate_path,
            self.candidate_sha,
            expected_release_id="nexus-v2-authority-test",
            expected_chain_commit=self.commit(self.chain),
            expected_sdkgen_commit=self.commit(self.sdkgen),
        )
        self.assertEqual(candidate["artifacts"]["catalog"]["sha256"], tool.CATALOG_SHA256)
        self.assertEqual(set(candidate["services"]), {"legendsAuthority"})
        self.assertFalse(candidate["safety"]["fpsReleaseIncluded"])
        self.assertTrue(all(not value["path"].startswith("/") for value in candidate["artifacts"].values()))

    def test_candidate_is_create_once(self) -> None:
        with self.assertRaisesRegex(tool.CandidateError, "overwrite"):
            self.assemble()

    def test_extra_missing_and_mode_drift_fail(self) -> None:
        extra = self.candidate_root / "api" / "extra.dll"
        extra.write_bytes(b"extra")
        with self.assertRaisesRegex(tool.CandidateError, "complete publish trees"):
            tool.validate_candidate(self.candidate_path, self.candidate_sha)
        extra.unlink()
        executable = self.candidate_root / "api" / "Eterra.Arcade.Authority.Api"
        executable.chmod(0o644)
        with self.assertRaisesRegex(tool.CandidateError, "complete publish trees"):
            tool.validate_candidate(self.candidate_path, self.candidate_sha)

    def test_nested_symlink_is_rejected(self) -> None:
        link = self.candidate_root / "api" / "linked"
        link.symlink_to(self.candidate_root / "operator", target_is_directory=True)
        with self.assertRaisesRegex(tool.CandidateError, "symlink"):
            tool.validate_candidate(self.candidate_path, self.candidate_sha)

    def test_noncanonical_and_secret_shaped_release_paths_fail(self) -> None:
        self.release_manifest.write_bytes(self.release_manifest.read_bytes() + b"\n")
        output = self.root / "noncanonical"
        with self.assertRaisesRegex(tool.CandidateError, "canonical SDK JSON"):
            self.assemble(output)
        self.release_manifest.write_bytes(
            tool.sdk_manifest_bytes(
                {
                    "schema": tool.SDK_RELEASE_SCHEMA,
                    "files": [
                        {
                            "path": "api/operator-access-key.pem",
                            "sha256": "ab" * 32,
                            "size": 1,
                            "executable": False,
                        },
                        {
                            "path": "operator/tool",
                            "sha256": "cd" * 32,
                            "size": 1,
                            "executable": False,
                        },
                    ],
                }
            )
        )
        with self.assertRaisesRegex(tool.CandidateError, "secret-shaped"):
            self.assemble(self.root / "secret-path")

    def test_dirty_source_repository_is_rejected(self) -> None:
        (self.chain / "dirty.txt").write_text("dirty\n", encoding="utf-8")
        with self.assertRaisesRegex(tool.CandidateError, "must be clean"):
            self.assemble(self.root / "dirty-source")

    def test_public_signer_address_must_match_public_key(self) -> None:
        value = json.loads(self.signer.read_text(encoding="utf-8"))
        value["ss58Address"] = "5Ef8v9xNMgZ8UPN4QEc5SbRVNgmS7kZmVPBMqgUa8A6tXPjS"
        self.signer.write_text(
            json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n",
            encoding="utf-8",
        )
        with self.assertRaisesRegex(tool.CandidateError, "does not encode"):
            self.assemble(self.root / "wrong-ss58")

    def valid_observation(self) -> dict[str, object]:
        candidate = tool.validate_candidate(self.candidate_path, self.candidate_sha)
        deployment_root = f"/opt/eterra-alpha/arcade-authority/releases/{self.candidate_sha}"
        nonce = bytes(range(32))
        nonce_hex = "0x" + nonce.hex()
        manifest = json.loads((self.candidate_root / tool.RELEASE_MANIFEST_NAME).read_text())
        by_path = {item["path"]: item for item in manifest["files"]}
        return {
            "schemaVersion": 1,
            "kind": tool.OBSERVATION_KIND,
            "releaseId": candidate["releaseId"],
            "candidateSha256": self.candidate_sha,
            "releaseManifestSha256": candidate["artifacts"]["releaseManifest"]["sha256"],
            "chainSourceCommit": candidate["sources"]["chain"]["commit"],
            "sdkgenSourceCommit": candidate["sources"]["sdkgen"]["commit"],
            "deploymentRoot": deployment_root,
            "serviceUnit": {
                "path": "/etc/systemd/system/eterra-arcade-authority.service",
                "sha256": candidate["deployment"]["serviceUnitSha256"],
                "mode": "0644",
                "owner": "root:root",
            },
            "environment": {
                "path": "/opt/eterra-alpha/shared/env/arcade-authority.env",
                "sha256": "ab" * 32,
                "mode": "0640",
                "owner": "root:eterra2010",
            },
            "secrets": {
                "signerMnemonic": {
                    "path": "/opt/eterra-alpha/shared/secrets/nexus-v2-legends-authority.mnemonic",
                    "sha256": "ac" * 32,
                    "mode": "0640",
                    "owner": "root:eterra2010",
                },
                "privateAlphaAccessKey": {
                    "path": "/opt/eterra-alpha/shared/secrets/nexus-v2-legends-authority.access-key",
                    "sha256": "ad" * 32,
                    "mode": "0640",
                    "owner": "root:eterra2010",
                },
                "signerDerivationPassword": None,
            },
            "process": {
                "serviceActive": True,
                "mainPid": 1234,
                "user": "eterra2010",
                "executablePath": deployment_root + "/api/Eterra.Arcade.Authority.Api",
                "procExecutableSha256": by_path["api/Eterra.Arcade.Authority.Api"]["sha256"],
                "listenerHost": "127.0.0.1",
                "listenerPort": 8787,
                "environmentMatched": True,
            },
            "catalog": {
                "path": deployment_root + "/" + tool.CATALOG_PATH,
                "sha256": tool.CATALOG_SHA256,
                "mode": "0644",
                "owner": "root:root",
            },
            "manifestVerification": {
                "operatorCliPath": deployment_root + "/operator/Eterra.Arcade.Authority.Operator",
                "operatorCliSha256": by_path["operator/Eterra.Arcade.Authority.Operator"]["sha256"],
                "stdoutSha256": "cd" * 32,
                "ok": True,
            },
            "journal": {
                "path": "/var/lib/eterra/legends-authority-journal",
                "mode": "0700",
                "owner": "eterra2010:eterra2010",
                "nonSymlinkDirectory": True,
            },
            "liveness": {
                "httpStatus": 200,
                "requestNonceHex": nonce_hex,
                "response": {
                    "schema": tool.LIVENESS_SCHEMA,
                    "ok": True,
                    "algorithm": "sr25519",
                    "nonceHex": nonce_hex,
                    "payloadHashHex": "0x" + hashlib.sha256(tool.LIVENESS_DOMAIN + nonce).hexdigest(),
                    "publicKeyHex": self.public_key,
                    "signatureHex": "0x" + ("01" * 64),
                    "error": "",
                },
            },
            "observedAtUtc": "2026-08-01T00:05:00Z",
        }

    def test_receipt_measures_deployment_and_defers_only_crypto_to_reopen(self) -> None:
        observation = self.root / "observation.json"
        write_canonical(observation, self.valid_observation())
        receipt = self.root / "receipt.json"
        tool.create_receipt(
            SimpleNamespace(
                candidate=str(self.candidate_path),
                expected_candidate_sha256=self.candidate_sha,
                observation=str(observation),
                output=str(receipt),
            )
        )
        value = json.loads(receipt.read_text())
        self.assertTrue(value["liveness"]["signerMatchesCandidate"])
        self.assertFalse(value["liveness"]["signatureCryptographicallyVerified"])
        self.assertTrue(value["liveness"]["cryptographicVerificationRequiredAtRestrictedReopen"])

    def test_receipt_rejects_wrong_liveness_signer_or_process(self) -> None:
        value = self.valid_observation()
        value["liveness"]["response"]["publicKeyHex"] = "0x" + ("34" * 32)
        observation = self.root / "bad-observation.json"
        write_canonical(observation, value)
        with self.assertRaisesRegex(tool.CandidateError, "signer"):
            tool.create_receipt(
                SimpleNamespace(
                    candidate=str(self.candidate_path),
                    expected_candidate_sha256=self.candidate_sha,
                    observation=str(observation),
                    output=str(self.root / "bad-receipt.json"),
                )
            )

    def test_assemble_rejects_symlinked_sources_and_service_unit(self) -> None:
        api_alias = self.root / "api-alias"
        api_alias.symlink_to(self.api, target_is_directory=True)
        original_api = self.api
        self.api = api_alias
        try:
            with self.assertRaisesRegex(tool.CandidateError, "symlink"):
                self.assemble(self.root / "symlink-api")
        finally:
            self.api = original_api

        unit_alias = self.root / "unit-alias.service"
        unit_alias.symlink_to(self.service_unit)
        original_unit = self.service_unit
        self.service_unit = unit_alias
        try:
            with self.assertRaisesRegex(tool.CandidateError, "symlink"):
                self.assemble(self.root / "symlink-unit")
        finally:
            self.service_unit = original_unit

    def test_receipt_rejects_symlinked_observation(self) -> None:
        observation = self.root / "observation-real.json"
        write_canonical(observation, self.valid_observation())
        alias = self.root / "observation-alias.json"
        alias.symlink_to(observation)
        with self.assertRaisesRegex(tool.CandidateError, "symlink"):
            tool.create_receipt(
                SimpleNamespace(
                    candidate=str(self.candidate_path),
                    expected_candidate_sha256=self.candidate_sha,
                    observation=str(alias),
                    output=str(self.root / "symlink-observation-receipt.json"),
                )
            )

        value = self.valid_observation()
        value["process"]["executablePath"] = "/tmp/unpinned-authority"
        write_canonical(observation, value)
        with self.assertRaisesRegex(tool.CandidateError, "/proc executable path"):
            tool.create_receipt(
                SimpleNamespace(
                    candidate=str(self.candidate_path),
                    expected_candidate_sha256=self.candidate_sha,
                    observation=str(observation),
                    output=str(self.root / "bad-process-receipt.json"),
                )
            )

    def test_receipt_rejects_environment_secret_and_live_process_tamper(self) -> None:
        mutations = (
            ("environment mode", lambda value: value["environment"].__setitem__("mode", "0644")),
            (
                "secret owner",
                lambda value: value["secrets"]["signerMnemonic"].__setitem__(
                    "owner", "eterra2010:eterra2010"
                ),
            ),
            (
                "live process",
                lambda value: value["process"].__setitem__("environmentMatched", False),
            ),
        )
        for label, mutate in mutations:
            with self.subTest(label=label):
                value = self.valid_observation()
                mutate(value)
                observation = self.root / f"tampered-{label.replace(' ', '-')}.json"
                write_canonical(observation, value)
                with self.assertRaises(tool.CandidateError):
                    tool.create_receipt(
                        SimpleNamespace(
                            candidate=str(self.candidate_path),
                            expected_candidate_sha256=self.candidate_sha,
                            observation=str(observation),
                            output=str(self.root / f"receipt-{label.replace(' ', '-')}.json"),
                        )
                    )


if __name__ == "__main__":
    unittest.main()
