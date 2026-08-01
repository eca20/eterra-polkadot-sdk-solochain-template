from __future__ import annotations

import argparse
import base64
import datetime as dt
import hashlib
import importlib.util
import json
import os
import pathlib
import tempfile
import types
import unittest
from unittest import mock


HERE = pathlib.Path(__file__).resolve().parent
SCRIPT = HERE / "nexus-v2-post-acceptance-reopen.py"
SPEC = importlib.util.spec_from_file_location("nexus_v2_post_acceptance_reopen_tested", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
tool = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(tool)
from deployment_secret_environment import (  # noqa: E402
    DEPLOYMENT_SECRET_ENVIRONMENT_NAMES,
)
PINS_SCRIPT = HERE.parents[2] / "scripts/nexus-v2-private-alpha/capture_ssh_host_pins.py"
PINS_SPEC = importlib.util.spec_from_file_location(
    "capture_ssh_host_pins_for_reopen_test", PINS_SCRIPT
)
assert PINS_SPEC is not None and PINS_SPEC.loader is not None
pins_tool = importlib.util.module_from_spec(PINS_SPEC)
PINS_SPEC.loader.exec_module(pins_tool)


def canonical(value: dict) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def digest(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path: pathlib.Path, payload: bytes, mode: int = 0o600) -> pathlib.Path:
    path.write_bytes(payload)
    path.chmod(mode)
    return path


def ssh_string(value: bytes) -> bytes:
    return len(value).to_bytes(4, "big") + value


def encoded_ssh_key(key_type: str, seed: int) -> str:
    algorithm = ssh_string(key_type.encode("ascii"))
    if key_type == "ssh-ed25519":
        blob = algorithm + ssh_string(bytes([seed]) * 32)
    elif key_type == "ecdsa-sha2-nistp256":
        blob = algorithm + ssh_string(b"nistp256") + ssh_string(b"\x04" + bytes([seed]) * 64)
    elif key_type == "ssh-rsa":
        blob = algorithm + ssh_string(b"\x01\x00\x01") + ssh_string(
            b"\x00\x80" + bytes([seed]) * 255
        )
    else:
        raise AssertionError(key_type)
    return base64.b64encode(blob).decode("ascii")


def create_host_pin_artifacts(root: pathlib.Path) -> tuple[pathlib.Path, pathlib.Path]:
    source = root / "source-known-hosts"
    lines: list[str] = []
    seed = 1
    for host in pins_tool.TARGET_HOSTS:
        for key_type in pins_tool.EXPECTED_KEY_TYPES:
            lines.append(f"{host} {key_type} {encoded_ssh_key(key_type, seed)}")
            seed += 1
    write(source, ("\n".join(lines) + "\n").encode("ascii"), 0o600)
    known_hosts = root / "nexus-v2-alpha.known_hosts"
    manifest = root / "nexus-v2-alpha.known_hosts.json"
    pins_tool.capture(source.resolve(), known_hosts.resolve(), manifest.resolve())
    return known_hosts, manifest


def shell_function(source: str, name: str) -> str:
    marker = f"\n{name}() {{\n"
    start = source.index(marker) + 1
    end = source.index("\n}\n", start) + 2
    return source[start:end]


def embedded_python(source: str, function: str) -> str:
    body = shell_function(source, function)
    marker = "<<'PY'\n"
    start = body.index(marker) + len(marker)
    end = body.index("\nPY\n", start)
    return body[start:end]


def embedded_pythons(source: str, function: str) -> list[str]:
    body = shell_function(source, function)
    marker = "<<'PY'\n"
    blocks: list[str] = []
    cursor = 0
    while marker in body[cursor:]:
        start = body.index(marker, cursor) + len(marker)
        end = body.index("\nPY\n", start)
        blocks.append(body[start:end])
        cursor = end + len("\nPY\n")
    return blocks


class ReopenTests(unittest.TestCase):
    def test_cli_help_uses_one_required_command_group(self) -> None:
        parser = tool.build_parser()
        command_groups = [
            action
            for action in parser._actions
            if isinstance(action, argparse._SubParsersAction)
        ]
        self.assertEqual(len(command_groups), 1)
        self.assertTrue(command_groups[0].required)
        self.assertEqual(
            set(command_groups[0].choices),
            {
                "capture-plan",
                "validate",
                "validate-close",
                "validate-adoption-seal",
                "execute",
                "verify",
                "close",
            },
        )
        with self.assertRaises(SystemExit) as raised:
            parser.parse_args(["--help"])
        self.assertEqual(raised.exception.code, 0)

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="post-acceptance-reopen-")
        self.root = pathlib.Path(self.temporary.name)
        self.files: dict[str, pathlib.Path] = {}
        for name in (
            "release-lock",
            "replacement-lock",
            "acceptance",
            "seal",
            "prerequisite",
            "authority-manifest",
            "chain-env",
            "site-env",
            "unity-fps-env",
            "unity-fps-candidate",
            "full-loop-activation",
            "runtime-normalizer",
            "site-candidate-manifest",
            "site-phase1-identity",
            "normal-caddy",
            "phase1-caddy",
            "chain-library",
            "site-library",
            "unity-library",
        ):
            self.files[name] = write(self.root / name, f"{name}\n".encode())
        self.fake_chain_root = self.root / "pinned-chain"
        self.fake_web_root = self.root / "pinned-web"
        (self.fake_chain_root / "scripts/nexus-v2-private-alpha").mkdir(parents=True)
        (self.fake_web_root / "tcg/deploy/alpha/macmini2014").mkdir(parents=True)
        phase2_verifier = write(
            self.fake_chain_root
            / "scripts/nexus-v2-private-alpha/phase2_internal_transport.py",
            b"#!/usr/bin/env python3\nraise SystemExit(0)\n",
            0o700,
        )
        activation_verifier = write(
            self.fake_web_root
            / "tcg/deploy/alpha/macmini2014/nexus_v2_full_loop_activation_contract.py",
            b"#!/usr/bin/env python3\nraise SystemExit(0)\n",
            0o700,
        )
        self.files["release-lock"].write_bytes(
            canonical(
                {
                    "repositories": {
                        "chain": {"root": str(self.fake_chain_root)},
                        "web": {"root": str(self.fake_web_root)},
                        "unity": {"root": str(self.root / "pinned-unity"), "head": "d" * 40},
                    }
                }
            )
        )
        self.driver = write(self.root / "normal-driver", b"#!/bin/sh\nexit 0\n", 0o700)
        self.emergency_driver = write(self.root / "driver", b"#!/bin/sh\nexit 0\n", 0o700)
        self.chain_helper = write(self.root / "chain-helper", b"#!/bin/sh\nexit 0\n", 0o700)
        self.fps_helper = write(self.root / "fps-helper", b"#!/bin/sh\nexit 0\n", 0o700)
        self.site_helper = write(self.root / "site-helper", b"#!/bin/sh\nexit 0\n", 0o700)
        self.ssh_known_hosts, self.ssh_host_pin_manifest = create_host_pin_artifacts(
            self.root
        )
        self.ssh_host_pin_validator = write(
            self.root / "ssh-host-pin-validator",
            PINS_SCRIPT.read_bytes(),
            0o700,
        )
        now = dt.datetime.now(dt.timezone.utc).replace(microsecond=0)
        self.now = now
        self.files["full-loop-activation"].write_bytes(
            canonical(self.activation_receipt())
        )
        site_identity = self.site_deployment_identity(now)
        self.files["site-post-phase2-identity"] = write(
            self.root / "site-post-phase2-identity", canonical(site_identity)
        )
        phase2_handoff = self.phase2_handoff(now)
        self.files["phase2-handoff"] = write(
            self.root / "phase2-handoff", canonical(phase2_handoff)
        )
        fps_candidate_root = self.root / "fps-candidate"
        fps_candidate_root.mkdir()
        write(
            fps_candidate_root / "candidate-manifest.json",
            self.files["unity-fps-candidate"].read_bytes(),
            0o400,
        )
        fps_tools = {}
        for name in ("rollback", "candidate-verifier", "receipt-verifier", "pin-verifier"):
            fps_tools[name] = write(
                self.root / f"fps-{name}", b"#!/bin/sh\nexit 0\n", 0o700
            )
        self.plan = {
            "schemaVersion": 1,
            "kind": tool.PLAN_KIND,
            "operationId": "reopen-test-1",
            "releaseId": "nexus-v2-private-alpha-test",
            "siteReleaseVersion": "v0.1.0-alpha.1",
            "sourceCommit": "a" * 40,
            "siteSourceCommit": "b" * 40,
            "genesisHash": "0x" + "c" * 64,
            "createdAtUtc": now.strftime("%Y-%m-%dT%H:%M:%SZ"),
            "expiresAtUtc": (now + dt.timedelta(hours=1)).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "finalReleaseLock": self.pin("release-lock"),
            "replacementLock": self.pin("replacement-lock"),
            "acceptanceBoundaryReceipt": self.pin("acceptance"),
            "phase2FinalSeal": self.pin("seal"),
            "phase2BootstrapPrerequisite": self.pin("prerequisite"),
            "authorityManifest": self.pin("authority-manifest"),
            "selectedDeploymentEnvironment": self.pin("chain-env"),
            "selectedSiteDeploymentEnvironment": self.pin("site-env"),
            "fullLoopIndexerActivationReceipt": self.pin("full-loop-activation"),
            "caddyfiles": {
                "normal": self.pin("normal-caddy"),
                "phase1": self.pin("phase1-caddy"),
            },
            "drivers": {
                component: {
                    "path": str(self.driver),
                    "sha256": digest(self.driver),
                    "sourceCommit": "a" * 40,
                }
                for component in tool.COMPONENTS
            },
            "helpers": {
                "chain-transport": {
                    "path": str(self.chain_helper),
                    "sha256": digest(self.chain_helper),
                    "sourceCommit": "a" * 40,
                },
                "fps-server": {
                    "path": str(self.fps_helper),
                    "sha256": digest(self.fps_helper),
                    "sourceCommit": "d" * 40,
                },
                "site-ingress": {
                    "path": str(self.site_helper),
                    "sha256": digest(self.site_helper),
                    "sourceCommit": "a" * 40,
                },
            },
            "network": {
                "chainLanIp": "192.168.1.159",
                "siteLanIp": "192.168.1.218",
                "publicHostname": "pocket.eterra.online",
            },
            "ports": dict(tool.PORTS),
            "smoke": {
                "mediaPath": "/nft/alpha-smoke.json",
                "mediaSha256": "d" * 64,
                "ipfsPath": "/ipfs/bafy-alpha-smoke",
                "ipfsSha256": "e" * 64,
            },
            "runtimeAuthority": self.runtime_authority(),
            "indexerReadiness": self.indexer_readiness(),
            "siteDeploymentIdentity": site_identity,
            "sitePostPhase2DeploymentIdentity": self.pin("site-post-phase2-identity"),
            "siteDeploymentCandidateManifest": self.pin("site-candidate-manifest"),
            "sitePhase1PostDeployIdentity": self.pin("site-phase1-identity"),
            "siteRuntimeConfigNormalizer": self.pin("runtime-normalizer"),
            "unityFpsCandidateManifest": self.pin("unity-fps-candidate"),
            "unityFpsDeploymentEnvironment": self.pin("unity-fps-env"),
            "phase2InternalTransportHandoff": self.pin("phase2-handoff"),
            "phase2InternalTransport": phase2_handoff,
            "sshHostPins": {
                "knownHosts": self.pin_path(self.ssh_known_hosts),
                "manifest": self.pin_path(self.ssh_host_pin_manifest),
                "validator": self.pin_path(self.ssh_host_pin_validator),
            },
            "emergencyClosure": {
                "bundleRoot": str(self.root),
                "driver": self.pin_path(self.emergency_driver),
                "helpers": {
                    "chain-transport": self.pin_path(self.chain_helper),
                    "fps-server": self.pin_path(self.fps_helper),
                    "site-ingress": self.pin_path(self.site_helper),
                },
                "libraries": {
                    "chain": self.pin("chain-library"),
                    "site": self.pin("site-library"),
                    "unity": self.pin("unity-library"),
                },
                "unityFpsDeploymentEnvironment": self.pin("unity-fps-env"),
                "fps": {
                    "candidateRoot": str(fps_candidate_root),
                    "candidateManifestSha256": digest(
                        fps_candidate_root / "candidate-manifest.json"
                    ),
                    "snapshotPath": str(self.root / "fps-snapshot"),
                    "deploymentReceiptPath": str(self.root / "fps-deployment-receipt"),
                    "rollbackReceiptPath": str(self.root / "fps-rollback-receipt"),
                    "rollbackScript": self.pin_path(fps_tools["rollback"]),
                    "candidateVerifier": self.pin_path(fps_tools["candidate-verifier"]),
                    "receiptVerifier": self.pin_path(fps_tools["receipt-verifier"]),
                    "pinVerifier": self.pin_path(fps_tools["pin-verifier"]),
                },
                "caddyfiles": {
                    "normal": self.pin("normal-caddy"),
                    "phase1": self.pin("phase1-caddy"),
                },
                "sshHostPins": {
                    "knownHosts": self.pin_path(self.ssh_known_hosts),
                    "manifest": self.pin_path(self.ssh_host_pin_manifest),
                    "validator": self.pin_path(self.ssh_host_pin_validator),
                },
                "targets": {
                    "chainHost": "192.168.1.159",
                    "chainUser": "eterra2010",
                    "siteHost": "192.168.1.218",
                    "siteUser": "eterra2014",
                },
            },
            "policy": dict(tool.POLICY),
        }

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def pin(self, name: str) -> dict[str, str]:
        path = self.files[name]
        return {"path": str(path), "sha256": digest(path)}

    def pin_path(self, path: pathlib.Path) -> dict[str, str]:
        return {"path": str(path), "sha256": digest(path)}

    def runtime_authority(self) -> dict:
        modes = [
            (1005, 1, 1, "ability-deathmatch", "eterra-fps-authority"),
            (1005, 1, 2, "extraction", "eterra-fps-authority"),
            (1005, 1, 3, "extraction-battle-royale", "eterra-fps-authority"),
            (1006, 1, 1, "eterra-legends", "eterra-legends-authority"),
        ]
        return {
            "runtimeSpecVersion": 106,
            "runtimeCodeSha256": "1" * 64,
            "runtimeMetadataScaleSha256": "2" * 64,
            "alphaAccess": {
                "mode": "Enforced",
                "ownerAccountId": "0x" + "e" * 64,
                "sourceKind": "ManualAdmin",
                "sourceChainId": 0,
                "sourceContract": "0x" + "00" * 20,
                "sourceEventId": "0x" + "f" * 64,
                "expiresAtUnix": 2_000_000_000,
            },
            "authorityEpochs": [
                {
                    "gameId": game,
                    "gameVersion": version,
                    "modeId": mode,
                    "modeName": name,
                    "serviceId": service,
                    "authorityEpoch": 7,
                    "publicKey": "0x" + "a" * 64,
                    "authorityConfigHash": "0x" + ("b" if service == "eterra-fps-authority" else "c") * 64,
                    "activeFrom": 11,
                    "activeUntil": 1000,
                    "revoked": False,
                }
                for game, version, mode, name, service in modes
            ],
            "proofPolicy": {
                "key": [1005, 1, 1, 0xFFFFFFFE],
                "policyHash": "0x" + "d" * 64,
                "active": False,
                "everActivated": True,
                "economicRealm": "Training",
                "practiceOnly": True,
                "rewardBudget": {"xp_total": "0"},
            },
            "storageCounts": dict(tool.RUNTIME_STORAGE_COUNTS),
        }

    def indexer_readiness(self) -> dict:
        return {
            "releaseVersion": "v0.1.0-alpha.1",
            "sourceCommit": "b" * 40,
            "privateAlphaAccessKeySha256": "9" * 64,
            "projectionDirectory": "/var/lib/eterra/full-loop",
            "fullLoopAcceptanceTargetSha256": "a" * 64,
            "readinessProjectionSha256": "1" * 64,
            "healthReadySha256": "2" * 64,
            "acceptanceReadinessSha256": "1" * 64,
            "authorityVisibleBaseUrl": "https://pocket.eterra.online/nexus-api",
            "activationReceiptSha256": digest(self.files["full-loop-activation"]),
            "activationOverrideSha256": "3" * 64,
            "projectionManifestSha256": "4" * 64,
        }

    def activation_receipt(self) -> dict:
        return {
            "activationId": "a" * 64,
            "releaseVersion": "v0.1.0-alpha.1",
            "siteSourceCommit": "b" * 40,
            "privateAlphaAccessKeySha256": "9" * 64,
            "projection": {
                "hostPath": "/opt/eterra-alpha/full-loop",
                "containerPath": "/var/lib/eterra/full-loop",
                "manifestSha256": "4" * 64,
                "readinessProjectionSha256": "1" * 64,
                "targetSha256": "a" * 64,
                "readinessEvidenceSha256": "5" * 64,
                "economicEvidenceSha256": "6" * 64,
                "accessEvidenceSha256": "7" * 64,
                "driverSha256": "8" * 64,
                "appendOnlyRuns": True,
            },
            "verification": {
                "healthReady": True,
                "healthReadySha256": "2" * 64,
                "authenticatedReadinessExact": True,
                "acceptanceReadinessSha256": "1" * 64,
                "anonymousAccessDenied": True,
            },
            "readModel": {
                "siteLocalBaseUrl": "http://127.0.0.1:8787",
                "authorityVisibleBaseUrl": "https://pocket.eterra.online/nexus-api",
                "healthReadyPath": "/health/ready",
                "acceptanceReadinessPath": "/v2/acceptance/readiness",
            },
            "activationOverride": {
                "hostPath": "/opt/eterra-alpha/activation.env",
                "sha256": "3" * 64,
            },
        }

    def phase2_handoff(self, now: dt.datetime) -> dict:
        return {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-phase2-internal-transport-handoff",
            "releaseId": "nexus-v2-private-alpha-test",
            "siteReleaseVersion": "v0.1.0-alpha.1",
            "sourceCommit": "a" * 40,
            "siteSourceCommit": "b" * 40,
            "acceptanceBoundaryReceiptSha256": digest(self.files["acceptance"]),
            "replacementLockSha256": digest(self.files["replacement-lock"]),
            "sitePhase1PostDeployIdentitySha256": digest(
                self.files["site-phase1-identity"]
            ),
            "sitePostPhase2DeploymentIdentitySha256": digest(
                self.files["site-post-phase2-identity"]
            ),
            "network": {
                "chainLanIp": "192.168.1.159",
                "siteLanIp": "192.168.1.218",
                "allowedSourceIp": "192.168.1.218",
            },
            "ports": {
                "chainRpc": 9944,
                "authority": 8787,
                "media": 4000,
                "ipfsGateway": 8080,
                "forbidden": [30333, 5001],
            },
            "lease": {
                "operationId": "reopen-test-1",
                "planSha256": "5" * 64,
                "markerPath": "/opt/eterra-alpha/shared/phase2-internal-transport/reopen-test-1/open.json",
                "markerSha256": "6" * 64,
                "heartbeatPath": "/opt/eterra-alpha/shared/phase2-internal-transport/reopen-test-1/heartbeat.json",
                "heartbeatNonce": "7" * 64,
                "watchdogService": "nexus-v2-phase2-internal-transport-reopen-test-1.service",
                "watchdogTimer": "nexus-v2-phase2-internal-transport-reopen-test-1.timer",
                "watchdogUnitSha256": "8" * 64,
                "watchdogPayloadSha256": "a" * 64,
                "armed": True,
                "expiresAtUtc": (now + dt.timedelta(hours=2)).strftime(
                    "%Y-%m-%dT%H:%M:%SZ"
                ),
            },
            "phase2": {
                "publicIngressClosed": True,
                "siteIndexerSynchronized": True,
                "authorityReady": True,
                "fullLoopActivationReceiptSha256": digest(
                    self.files["full-loop-activation"]
                ),
            },
            "safety": {
                "chainStateMutationAuthorized": False,
                "paidOrPublicActivationAuthorized": False,
                "sourceRestricted": True,
                "loopbackBackendsPreserved": True,
                "forbiddenPortsClosed": True,
            },
            "capturedAtUtc": now.strftime("%Y-%m-%dT%H:%M:%SZ"),
        }

    def site_deployment_identity(self, now: dt.datetime) -> dict:
        return {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-site-deployment-identity",
            "releaseVersion": "v0.1.0-alpha.1",
            "siteSourceCommit": "b" * 40,
            "composeFileSha256": "3" * 64,
            "sourceContract": {
                "composeSha256": "3" * 64,
                "candidateManifestSha256": digest(
                    self.files["site-candidate-manifest"]
                ),
                "phase1PostDeployIdentitySha256": digest(
                    self.files["site-phase1-identity"]
                ),
                "runtimeNormalizerSha256": digest(self.files["runtime-normalizer"]),
                "fullLoopActivationReceiptSha256": digest(
                    self.files["full-loop-activation"]
                ),
                "fullLoopActivationOverrideSha256": "3" * 64,
                "fullLoopProjectionManifestSha256": "4" * 64,
                "fullLoopActivationVerifierSha256": digest(
                    self.fake_web_root
                    / "tcg/deploy/alpha/macmini2014/nexus_v2_full_loop_activation_contract.py"
                ),
            },
            "images": [
                {
                    "service": name,
                    "reference": f"eterra/{name}:test",
                    "imageId": "sha256:" + str(index) * 64,
                    "runtimeConfigSha256": f"{index:x}" * 64,
                    "resolvedComposeServiceSha256": f"{index + 4:x}" * 64,
                    "composeServiceConfigHash": f"{index + 8:x}" * 64,
                }
                for index, name in enumerate(("caddy", "indexer-api", "mongo", "site"), 4)
            ],
            "publications": {
                "site": ["127.0.0.1:3000:3000/tcp"],
                "indexer-api": ["127.0.0.1:8787:8787/tcp"],
                "mongo": [],
                "caddy": ["0.0.0.0:80:80/tcp", "0.0.0.0:443:443/tcp"],
            },
            "authorityStatus": {
                "fps": {
                    "sourceEndpoint": "http://127.0.0.1:8787/v1/fps/status",
                    "sourceDocumentSha256": "8" * 64,
                    "ok": True,
                    "signerAvailable": True,
                    "authorityStateAvailable": True,
                    "runtimeDerivesRewards": True,
                    "privateAlphaOnly": True,
                    "paidEntry": False,
                    "wagering": False,
                    "permanentAssetLoss": False,
                    "publicProduction": False,
                    "authorityConfigHash": "0x" + "b" * 64,
                },
                "legends": {
                    "sourceEndpoint": "http://127.0.0.1:8787/v1/eterra-legends/status",
                    "sourceDocumentSha256": "9" * 64,
                    "ok": True,
                    "gameId": 1006,
                    "gameVersion": 1,
                    "modeId": 1,
                    "signerAvailable": True,
                    "authorityStateAvailable": True,
                    "encounterCatalogAvailable": True,
                    "ownerAuthorizationAvailable": True,
                    "resultJournalAvailable": True,
                    "runtimeDerivesRewards": True,
                    "authorityConfigHash": "0x" + "c" * 64,
                },
            },
            "capturedAtUtc": now.strftime("%Y-%m-%dT%H:%M:%SZ"),
        }

    def test_source_root_resolves_repository_not_deploy_directory(self) -> None:
        expected = HERE.parents[2]
        self.assertEqual(tool.REPO_ROOT, expected)
        self.assertEqual(
            tool.RELEASE_LOCK_PATH,
            expected / "scripts/nexus-v2-private-alpha/release_lock.py",
        )
        self.assertTrue((tool.REPO_ROOT / "Cargo.toml").is_file())
        self.assertTrue(tool.RELEASE_LOCK_PATH.is_file())

    def test_plan_and_evidence_outputs_cannot_dirty_a_pinned_repository(self) -> None:
        lock = write(
            self.root / "inside-lock.json",
            canonical({"repositories": {"chain": {"root": str(self.root)}}}),
        )
        plan = json.loads(json.dumps(self.plan))
        plan["finalReleaseLock"] = {"path": str(lock), "sha256": digest(lock)}
        with self.assertRaisesRegex(tool.ReopenError, "outside final-lock-pinned"):
            tool.require_outside_pinned_repositories(
                self.root / "evidence",
                plan,
                "reopen evidence directory",
            )

    def test_plan_shape_locks_four_proxies_and_two_forbidden_ports(self) -> None:
        observed = tool.validate_plan_shape(self.plan, now=self.now)
        self.assertEqual(observed["ports"], tool.PORTS)
        self.assertEqual(observed["policy"]["exposedServices"], [
            "authority",
            "chainRpc",
            "ipfsGateway",
            "media",
        ])
        self.assertEqual(observed["policy"]["forbiddenExposedPorts"], [30333, 5001])
        self.assertFalse(observed["policy"]["chainStateMutationAuthorized"])
        self.assertFalse(observed["policy"]["chainStateRollbackAuthorized"])
        self.assertEqual(
            observed["sshHostPins"]["knownHosts"]["sha256"],
            digest(self.ssh_known_hosts),
        )

    def test_plan_rejects_host_pin_tampering_or_substitution(self) -> None:
        tampered = json.loads(json.dumps(self.plan))
        tampered["sshHostPins"]["knownHosts"]["sha256"] = "0" * 64
        with self.assertRaisesRegex(tool.ReopenError, "hash mismatch"):
            tool.validate_plan_shape(tampered, now=self.now)

        substituted = json.loads(json.dumps(self.plan))
        replacement = write(self.root / "replacement-known-hosts", b"not trusted\n", 0o600)
        substituted["sshHostPins"]["knownHosts"] = self.pin_path(replacement)
        with self.assertRaisesRegex(tool.ReopenError, "validation failed"):
            tool.validate_plan_shape(substituted, now=self.now)

    def test_chain_and_site_release_identities_cannot_be_conflated(self) -> None:
        conflated = json.loads(json.dumps(self.plan))
        conflated["siteReleaseVersion"] = conflated["releaseId"]
        with self.assertRaisesRegex(tool.ReopenError, "site release version"):
            tool.validate_plan_shape(conflated, now=self.now)
        driver = (HERE / "nexus-v2-post-acceptance-reopen-component-driver").read_text()
        self.assertIn('"${RELEASE_VERSION}" == "${site_release_version}"', driver)
        self.assertNotIn('"${RELEASE_VERSION}" == "${release_id}"', driver)

    def test_plan_rejects_extra_port_policy_expiry_and_unsafe_smoke_path(self) -> None:
        extra = json.loads(json.dumps(self.plan))
        extra["ports"]["debug"] = 9999
        with self.assertRaisesRegex(tool.ReopenError, "port contract"):
            tool.validate_plan_shape(extra, now=self.now)
        exposed_api = json.loads(json.dumps(self.plan))
        exposed_api["policy"]["forbiddenExposedPorts"] = [30333]
        with self.assertRaisesRegex(tool.ReopenError, "policy"):
            tool.validate_plan_shape(exposed_api, now=self.now)
        expired = json.loads(json.dumps(self.plan))
        expired["createdAtUtc"] = (self.now - dt.timedelta(hours=1)).strftime("%Y-%m-%dT%H:%M:%SZ")
        expired["expiresAtUtc"] = (self.now - dt.timedelta(seconds=1)).strftime("%Y-%m-%dT%H:%M:%SZ")
        expired["siteDeploymentIdentity"]["capturedAtUtc"] = expired["createdAtUtc"]
        expired_identity = write(
            self.root / "expired-site-deployment-identity",
            canonical(expired["siteDeploymentIdentity"]),
        )
        expired["sitePostPhase2DeploymentIdentity"] = self.pin_path(expired_identity)
        expired["phase2InternalTransport"][
            "sitePostPhase2DeploymentIdentitySha256"
        ] = digest(expired_identity)
        expired_handoff = write(
            self.root / "expired-phase2-handoff",
            canonical(expired["phase2InternalTransport"]),
        )
        expired["phase2InternalTransportHandoff"] = self.pin_path(expired_handoff)
        with self.assertRaisesRegex(tool.ReopenError, "expired|lifetime"):
            tool.validate_plan_shape(expired, now=self.now)
        # Expiry ends open/verify authority, never the ability to fail closed.
        tool.validate_plan_shape(expired, now=self.now, allow_expired=True)
        traversal = json.loads(json.dumps(self.plan))
        traversal["smoke"]["ipfsPath"] = "/ipfs/../secret"
        with self.assertRaisesRegex(tool.ReopenError, "IPFS smoke path"):
            tool.validate_plan_shape(traversal, now=self.now)

    def receipt(self) -> dict:
        return {
            "releaseId": self.plan["releaseId"],
            "sourceCommit": self.plan["sourceCommit"],
            "genesisHash": self.plan["genesisHash"],
            "runtimeCodeSha256": "1" * 64,
            "runtimeMetadataScaleSha256": "2" * 64,
        }

    def lock(self) -> dict:
        return {
            "artifacts": {
                "targetIdentity": {"path": "/target", "sha256": "3" * 64},
                "acceptanceBoundaryReceipt": {"path": "/receipt", "sha256": "4" * 64},
            }
        }

    def seal(self) -> dict:
        receipt = self.receipt()
        return {
            "schema": "eterra.nexus-v2-runtime-seeder-phase2-final-seal.v1",
            "generated_at_utc": self.now.strftime("%Y-%m-%dT%H:%M:%SZ"),
            "environment": "private_alpha",
            "status": "post_proof_finalized",
            "target": {
                "release_id": receipt["releaseId"],
                "source_commit": receipt["sourceCommit"],
                "genesis_hash": receipt["genesisHash"],
                "runtime_code_sha256": receipt["runtimeCodeSha256"],
                "runtime_metadata_scale_sha256": receipt["runtimeMetadataScaleSha256"],
                "runtime_spec_version": 106,
                "target_identity_sha256": "3" * 64,
                "acceptance_boundary_sha256": "4" * 64,
            },
            "source": {"sdk": {}, "unity": {}},
            "artifacts": {
                "bootstrap_prerequisite_sha256": "5" * 64,
                "bootstrap_finalized_evidence_sha256": "6" * 64,
                "bootstrap_journal_sha256": "7" * 64,
                "pre_deactivation_proof_sha256": "8" * 64,
                "proof_run_handoff_sha256": "9" * 64,
                "deactivation_evidence_sha256": "a" * 64,
                "fps_acceptance_proof_sha256": "b" * 64,
            },
            "authority_manifest": {
                "schema": "eterra.authority-registration-manifest.v1",
                "sha256": "c" * 64,
                "fixture_only": False,
                "registrations": 4,
            },
            "proof_baseline": {},
            "proof_policy": {
                "key": [1005, 1, 1, 0xFFFFFFFE],
                "policy_hash": "0x" + "d" * 64,
                "active": False,
                "extra_deactivated_policy_count": 1,
                "pre_deactivation_active": True,
            },
            "alpha_access": {
                "mode": "Enforced",
                "owner_account_id": "0x" + "e" * 64,
                "source_kind": "ManualAdmin",
                "source_event_id": "0x" + "f" * 64,
                "expires_at_unix": 2_000_000_000,
                "grant_count": 1,
            },
            "safety": {
                "alpha_access_mode": "Enforced",
                "bootstrap_only": False,
                "canonical_seed_eligible": True,
                "economically_valued_rewards": False,
                "marketplace": False,
                "paid_entry": False,
                "permanent_asset_loss": False,
                "private_alpha_only": True,
                "proof_policy_active": False,
                "public_production": False,
                "transfers": False,
                "wagering": False,
            },
        }

    def test_phase2_seal_requires_deactivated_proof_enforced_access_and_disabled_economy(self) -> None:
        tool.validate_phase2_seal_shape(self.seal(), self.receipt(), self.lock())
        active = self.seal()
        active["proof_policy"]["active"] = True
        with self.assertRaisesRegex(tool.ReopenError, "not deactivated"):
            tool.validate_phase2_seal_shape(active, self.receipt(), self.lock())
        public = self.seal()
        public["safety"]["public_production"] = True
        with self.assertRaisesRegex(tool.ReopenError, "safety"):
            tool.validate_phase2_seal_shape(public, self.receipt(), self.lock())
        open_access = self.seal()
        open_access["alpha_access"]["mode"] = "Open"
        with self.assertRaisesRegex(tool.ReopenError, "AlphaAccess"):
            tool.validate_phase2_seal_shape(open_access, self.receipt(), self.lock())

    def test_official_phase2_validator_uses_a_pinned_dynamic_module_import(self) -> None:
        chain_root = self.root / "chain"
        web_root = self.root / "web"
        driver = chain_root / "deploy/alpha/macmini2010/nexus-v2-post-acceptance-reopen-component-driver"
        chain_helper = chain_root / "deploy/alpha/macmini2010/nexus-v2-post-acceptance-reopen-host-action.sh"
        site_helper = chain_root / "deploy/alpha/macmini2010/nexus-v2-post-acceptance-reopen-site-action.sh"
        module = web_root / "tcg/apps/web/scripts/nexus-v2-runtime-seeder-phase2-lib.mjs"
        for path in (driver, chain_helper, site_helper, module):
            path.parent.mkdir(parents=True, exist_ok=True)
            path.touch()
        module.write_text(
            "export function loadPhase2FinalSeal(value) {\n"
            "  if (!value.sealPath || !/^[0-9a-f]{64}$/.test(value.sealSha256)) throw new Error('bad pin');\n"
            "}\n"
        )
        plan = json.loads(json.dumps(self.plan))
        for component in tool.COMPONENTS:
            plan["drivers"][component]["path"] = str(driver)
        plan["helpers"]["chain-transport"]["path"] = str(chain_helper)
        plan["helpers"]["site-ingress"]["path"] = str(site_helper)
        unity_root = self.root / "unity"
        fps_helper = (
            unity_root
            / "deploy/alpha/macmini2014/nexus-v2-fps-reopen-component.sh"
        )
        fps_helper.parent.mkdir(parents=True, exist_ok=True)
        fps_helper.touch()
        plan["helpers"]["fps-server"]["path"] = str(fps_helper)
        lock = {
            "repositories": {
                "chain": {"root": str(chain_root)},
                "web": {"root": str(web_root)},
                "unity": {"root": str(unity_root)},
            }
        }
        tool.run_official_phase2_validator(plan, lock)

    def make_result(self, component: str, action: str, mode: str) -> dict:
        component_receipt = None
        if component == "fps-server" and action in {"promote", "verify", "rollback"}:
            candidate_key = (
                "candidate" if action in {"promote", "verify"} else "rolledBackCandidate"
            )
            receipt = {
                "schema": (
                    "eterra.nexus-v2-fps-deployment-receipt.v1"
                    if action in {"promote", "verify"}
                    else "eterra.nexus-v2-fps-deployment-rollback-receipt.v1"
                ),
                candidate_key: {
                    "candidateManifestSha256": self.plan[
                        "unityFpsCandidateManifest"
                    ]["sha256"],
                    "chainReleaseId": self.plan["releaseId"],
                },
            }
            if action in {"promote", "verify"}:
                receipt.update(
                    {
                        "action": "promote",
                        "environment": "private_alpha",
                        "capturedAtUtc": self.now.strftime("%Y-%m-%dT%H:%M:%SZ"),
                        "selectedDeploymentEnvironmentSha256": self.plan[
                            "unityFpsDeploymentEnvironment"
                        ]["sha256"],
                        "safety": {
                            "privateAlphaOnly": True,
                            "chainRequired": True,
                            "gameResultsV2Required": True,
                            "paidEntry": False,
                            "wagering": False,
                            "permanentAssetLoss": False,
                            "marketplace": False,
                            "publicProduction": False,
                        },
                    }
                )
                receipt_path = self.root / "fps-deployment-receipt.json"
            else:
                receipt_path = self.root / "fps-rollback-receipt.json"
            receipt_payload = canonical(receipt)
            if receipt_path.exists():
                self.assertEqual(receipt_path.read_bytes(), receipt_payload)
            else:
                receipt_path.write_bytes(receipt_payload)
                receipt_path.chmod(0o400)
            component_receipt = self.pin_path(receipt_path)
        return {
            "schemaVersion": 1,
            "kind": "nexus-v2-private-alpha-post-acceptance-reopen-component-result",
            "operationId": self.plan["operationId"],
            "planSha256": "f" * 64,
            "releaseId": self.plan["releaseId"],
            "siteReleaseVersion": self.plan["siteReleaseVersion"],
            "sourceCommit": self.plan["sourceCommit"],
            "siteSourceCommit": self.plan["siteSourceCommit"],
            "componentId": component,
            "action": action,
            "mode": mode,
            "result": "passed",
            "mutationPerformed": action
            in {"adopt", "promote", "open", "prepare-commit", "commit", "rollback", "close"},
            "alreadyApplied": False,
            "finalReleaseLockSha256": self.plan["finalReleaseLock"]["sha256"],
            "acceptanceBoundaryReceiptSha256": self.plan["acceptanceBoundaryReceipt"]["sha256"],
            "phase2FinalSealSha256": self.plan["phase2FinalSeal"]["sha256"],
            "fpsAdoptionSealSha256": None,
            "driverSha256": (
                self.plan["emergencyClosure"]["driver"]["sha256"]
                if action in {"close", "rollback"}
                else self.plan["drivers"][component]["sha256"]
            ),
            "remoteMarkerSha256": None if mode == "dry-run" else "1" * 64,
            "componentReceipt": component_receipt,
            "checks": {name: True for name in tool.expected_checks(component, action, mode)},
            "completedAtUtc": self.now.strftime("%Y-%m-%dT%H:%M:%SZ"),
        }

    def test_component_results_are_closed_and_dry_run_cannot_claim_mutation(self) -> None:
        result = self.make_result("chain-transport", "preflight", "dry-run")
        path = write(self.root / "result.json", canonical(result))
        tool.validate_result(path, self.plan, "f" * 64, "chain-transport", "preflight", "dry-run")
        result["mutationPerformed"] = True
        path.write_bytes(canonical(result))
        with self.assertRaisesRegex(tool.ReopenError, "Preflight|preflight"):
            tool.validate_result(path, self.plan, "f" * 64, "chain-transport", "preflight", "dry-run")
        result["mutationPerformed"] = False
        result["unexpected"] = True
        path.write_bytes(canonical(result))
        with self.assertRaisesRegex(tool.ReopenError, "closed schema"):
            tool.validate_result(path, self.plan, "f" * 64, "chain-transport", "preflight", "dry-run")

    def test_emergency_close_validation_survives_normal_source_and_artifact_drift(self) -> None:
        plan_path = write(self.root / "immutable-close-plan.json", canonical(self.plan))
        expected = digest(plan_path)
        self.files["release-lock"].unlink()
        self.driver.unlink()
        loaded = tool.load_plan(
            plan_path,
            expected,
            allow_expired=True,
            closure_only=True,
        )
        self.assertEqual(loaded["operationId"], self.plan["operationId"])

    def test_runtime_authority_and_site_identity_reject_economic_or_public_drift(self) -> None:
        active = json.loads(json.dumps(self.plan["runtimeAuthority"]))
        active["proofPolicy"]["active"] = True
        with self.assertRaisesRegex(tool.ReopenError, "deactivated"):
            tool.validate_runtime_authority(active)
        public = json.loads(json.dumps(self.plan["siteDeploymentIdentity"]))
        public["authorityStatus"]["fps"]["publicProduction"] = True
        with self.assertRaisesRegex(tool.ReopenError, "FPS authority"):
            tool.validate_site_deployment_identity(public, self.plan)

    def operation_args(self, command: str, evidence_dir: pathlib.Path) -> types.SimpleNamespace:
        plan_path = write(self.root / f"{command}-plan.json", canonical(self.plan))
        return types.SimpleNamespace(
            command=command,
            plan=str(plan_path),
            expected_sha256=digest(plan_path),
            evidence_dir=str(evidence_dir),
        )

    def capture_adoption_fixture(self, plan_sha256: str = "f" * 64) -> dict[str, str]:
        promote = self.make_result("fps-server", "promote", "execute")
        promote["planSha256"] = plan_sha256
        promote_path = write(
            self.root / "adoption-promote.result.json", canonical(promote), 0o400
        )
        verify = self.make_result("fps-server", "verify", "execute")
        verify["planSha256"] = plan_sha256
        verify_path = write(
            self.root / "adoption-verify.result.json", canonical(verify), 0o400
        )
        evidence = self.root / "adoption-evidence"
        evidence.mkdir(exist_ok=True)
        return tool.capture_fps_adoption_seal(
            evidence,
            self.plan,
            plan_sha256,
            verify,
            self.pin_path(verify_path),
            promote,
            self.pin_path(promote_path),
        )

    def test_execute_orders_both_preflights_before_open_and_verifies_after(self) -> None:
        calls: list[tuple[str, str, str]] = []

        peer_pins: list[dict[str, str] | None] = []
        adoption_pins: list[tuple[str, str, dict[str, str] | None]] = []

        def fake_invoke(
            _path,
            _plan,
            _sha,
            _directory,
            component,
            action,
            mode,
            peer_commit_result=None,
            fps_adoption_seal=None,
        ):
            calls.append((component, action, mode))
            peer_pins.append(peer_commit_result)
            adoption_pins.append((component, action, fps_adoption_seal))
            if component == "fps-server" and action in {"promote", "verify"}:
                result = self.make_result(component, action, mode)
                result["planSha256"] = _sha
                result_path = write(
                    self.root / f"mock-{component}-{action}.json",
                    canonical(result),
                    0o400,
                )
                return result, self.pin_path(result_path)
            return {"alreadyApplied": False}, {"path": f"/{component}-{action}", "sha256": "1" * 64}

        evidence_dir = self.root / "execute-evidence"
        args = self.operation_args("execute", evidence_dir)
        with mock.patch.object(tool, "load_plan", return_value=self.plan), mock.patch.object(
            tool, "invoke_driver", side_effect=fake_invoke
        ), mock.patch.dict(
            os.environ,
            {"NEXUS_V2_POST_ACCEPTANCE_REOPEN_CONFIRMATION": "PRIVATE_ALPHA_RESTRICTED_REOPEN"},
            clear=False,
        ):
            tool.command_operate(args)
        self.assertEqual(
            calls,
            [
                ("chain-transport", "preflight", "dry-run"),
                ("fps-server", "preflight", "dry-run"),
                ("site-ingress", "preflight", "dry-run"),
                ("chain-transport", "preflight", "execute"),
                ("fps-server", "preflight", "execute"),
                ("site-ingress", "preflight", "execute"),
                ("chain-transport", "adopt", "execute"),
                ("fps-server", "promote", "execute"),
                ("fps-server", "verify", "execute"),
                ("site-ingress", "open", "execute"),
                ("chain-transport", "verify", "execute"),
                ("site-ingress", "verify", "execute"),
                ("site-ingress", "prepare-commit", "execute"),
                ("site-ingress", "commit", "execute"),
                ("chain-transport", "commit", "execute"),
            ],
        )
        self.assertIsNone(peer_pins[-3])
        self.assertEqual(
            peer_pins[-2],
            {"path": "/site-ingress-prepare-commit", "sha256": "1" * 64},
        )
        self.assertEqual(
            peer_pins[-1],
            {"path": "/site-ingress-commit", "sha256": "1" * 64},
        )
        guarded_adoption_pins = [
            pin
            for component, action, pin in adoption_pins
            if component == "site-ingress" and action in tool.SITE_ADOPTION_ACTIONS
        ]
        self.assertEqual(len(guarded_adoption_pins), 4)
        self.assertTrue(all(pin == guarded_adoption_pins[0] for pin in guarded_adoption_pins))
        self.assertTrue(all(pin is not None for pin in guarded_adoption_pins))
        evidence = json.loads((evidence_dir / "reopen-evidence.json").read_text())
        self.assertEqual(evidence["transport"]["exposedPorts"], [4000, 8080, 8787, 9944])
        self.assertEqual(evidence["transport"]["forbiddenPorts"], [30333, 5001])
        self.assertFalse(evidence["chainStateMutationPerformed"])
        self.assertFalse(evidence["chainStateRollbackPerformed"])
        adoption = tool.validate_pin(evidence["fpsAdoptionSeal"], "FPS adoption seal")
        adoption_value = json.loads(pathlib.Path(adoption["path"]).read_text())
        self.assertEqual(
            adoption_value["deploymentEnvironmentSha256"],
            self.plan["unityFpsDeploymentEnvironment"]["sha256"],
        )
        self.assertEqual(
            adoption_value["candidateManifestSha256"],
            self.plan["unityFpsCandidateManifest"]["sha256"],
        )
        self.assertEqual(
            adoption_value["deploymentReceipt"]["sha256"],
            digest(self.root / "fps-deployment-receipt.json"),
        )

    def test_adoption_seal_rejects_missing_mismatch_stale_and_tampered_authority(self) -> None:
        plan_sha = "f" * 64
        pin = self.capture_adoption_fixture(plan_sha)
        path = pathlib.Path(pin["path"])
        self.assertEqual(
            tool.validate_fps_adoption_seal(path, pin["sha256"], self.plan, plan_sha),
            pin,
        )
        with self.assertRaisesRegex(tool.ReopenError, "unavailable"):
            tool.validate_fps_adoption_seal(
                self.root / "missing-adoption.json",
                pin["sha256"],
                self.plan,
                plan_sha,
            )
        with self.assertRaisesRegex(tool.ReopenError, "hash mismatch"):
            tool.validate_fps_adoption_seal(
                path,
                "1" * 64,
                self.plan,
                plan_sha,
            )
        with self.assertRaisesRegex(tool.ReopenError, "stale"):
            tool.validate_fps_adoption_seal(
                path,
                pin["sha256"],
                self.plan,
                plan_sha,
                now=self.now + dt.timedelta(hours=2),
            )

        candidate_tamper = json.loads(path.read_text())
        candidate_tamper["candidateManifestSha256"] = "9" * 64
        candidate_tamper_path = write(
            self.root / "candidate-tampered-adoption.json",
            canonical(candidate_tamper),
            0o400,
        )
        with self.assertRaisesRegex(tool.ReopenError, "candidate|environment"):
            tool.validate_fps_adoption_seal(
                candidate_tamper_path,
                digest(candidate_tamper_path),
                self.plan,
                plan_sha,
            )

        receipt_path = pathlib.Path(candidate_tamper["deploymentReceipt"]["path"])
        receipt_path.chmod(0o600)
        receipt = json.loads(receipt_path.read_text())
        receipt["selectedDeploymentEnvironmentSha256"] = "8" * 64
        receipt_path.write_bytes(canonical(receipt))
        with self.assertRaisesRegex(tool.ReopenError, "receipt.*hash mismatch"):
            tool.validate_fps_adoption_seal(
                path,
                pin["sha256"],
                self.plan,
                plan_sha,
            )

    def test_protected_site_open_cannot_directly_bypass_adoption_seal(self) -> None:
        plan_path = write(self.root / "direct-open-plan.json", canonical(self.plan), 0o400)
        result_path = self.root / "direct-open-result.json"
        completed = tool.subprocess.run(
            [
                str(HERE / "nexus-v2-post-acceptance-reopen-component-driver"),
                "--component",
                "site-ingress",
                "--action",
                "open",
                "--mode",
                "execute",
                "--operation-id",
                self.plan["operationId"],
                "--plan",
                str(plan_path),
                "--plan-sha256",
                digest(plan_path),
                "--result",
                str(result_path),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertNotEqual(completed.returncode, 0)
        self.assertIn("requires an absolute regular FPS adoption seal", completed.stderr)
        self.assertFalse(result_path.exists())

    def test_remote_site_state_and_results_revalidate_exact_adoption_seal(self) -> None:
        site = (HERE / "nexus-v2-post-acceptance-reopen-site-action.sh").read_text()
        driver = (HERE / "nexus-v2-post-acceptance-reopen-component-driver").read_text()
        for required in (
            "validate_fps_adoption_seal_file",
            "require_fps_adoption_seal_exact",
            "retain_fps_adoption_seal",
            "FPS_ADOPTION_SEAL_FILE",
            "fpsAdoptionSealSha256",
            "fpsAdoptionSealPinned",
        ):
            self.assertIn(required, site)
        self.assertIn("--fps-adoption-seal", driver)
        self.assertIn("--fps-adoption-seal-sha256", driver)
        self.assertIn("validate-adoption-seal", driver)
        self.assertIn("fps_adoption_seal_base64", shell_function(driver, "run_remote_helper"))

    def test_missing_site_prepare_result_prevents_both_commits_and_fails_closed(self) -> None:
        calls: list[tuple[str, str, str]] = []

        def fake_invoke(
            _path,
            _plan,
            _sha,
            _directory,
            component,
            action,
            mode,
            peer_commit_result=None,
            fps_adoption_seal=None,
        ):
            calls.append((component, action, mode))
            if component == "fps-server" and action in {"promote", "verify"}:
                result = self.make_result(component, action, mode)
                result["planSha256"] = _sha
                result_path = write(
                    self.root / f"missing-prepare-{component}-{action}.json",
                    canonical(result),
                    0o400,
                )
                return result, self.pin_path(result_path)
            if component == "site-ingress" and action == "prepare-commit":
                raise tool.ReopenError("site prepare driver did not create a result")
            return {"alreadyApplied": False}, {
                "path": f"/{component}-{action}",
                "sha256": "1" * 64,
            }

        evidence_dir = self.root / "missing-site-prepare-evidence"
        args = self.operation_args("execute", evidence_dir)
        with mock.patch.object(tool, "load_plan", return_value=self.plan), mock.patch.object(
            tool, "invoke_driver", side_effect=fake_invoke
        ), mock.patch.dict(
            os.environ,
            {"NEXUS_V2_POST_ACCEPTANCE_REOPEN_CONFIRMATION": "PRIVATE_ALPHA_RESTRICTED_REOPEN"},
            clear=False,
        ):
            with self.assertRaisesRegex(tool.ReopenError, "Phase-1 transport was restored"):
                tool.command_operate(args)

        self.assertIn(("site-ingress", "prepare-commit", "execute"), calls)
        self.assertFalse(any(action == "commit" for _, action, _ in calls))
        self.assertEqual(
            calls[-3:],
            [
                ("chain-transport", "close", "execute"),
                ("fps-server", "rollback", "execute"),
                ("site-ingress", "close", "execute"),
            ],
        )
        failure = json.loads((evidence_dir / "reopen-failure.json").read_text())
        self.assertEqual(
            failure["failedReason"],
            "site prepare driver did not create a result",
        )

    def test_missing_final_site_commit_result_prevents_chain_commit_and_fails_closed(self) -> None:
        calls: list[tuple[str, str, str]] = []

        def fake_invoke(
            _path,
            _plan,
            _sha,
            _directory,
            component,
            action,
            mode,
            peer_commit_result=None,
            fps_adoption_seal=None,
        ):
            calls.append((component, action, mode))
            if component == "fps-server" and action in {"promote", "verify"}:
                result = self.make_result(component, action, mode)
                result["planSha256"] = _sha
                result_path = write(
                    self.root / f"missing-commit-{component}-{action}.json",
                    canonical(result),
                    0o400,
                )
                return result, self.pin_path(result_path)
            if component == "site-ingress" and action == "commit":
                raise tool.ReopenError("site commit driver did not create a result")
            return {"alreadyApplied": False}, {
                "path": f"/{component}-{action}",
                "sha256": "1" * 64,
            }

        evidence_dir = self.root / "missing-site-commit-evidence"
        args = self.operation_args("execute", evidence_dir)
        with mock.patch.object(tool, "load_plan", return_value=self.plan), mock.patch.object(
            tool, "invoke_driver", side_effect=fake_invoke
        ), mock.patch.dict(
            os.environ,
            {"NEXUS_V2_POST_ACCEPTANCE_REOPEN_CONFIRMATION": "PRIVATE_ALPHA_RESTRICTED_REOPEN"},
            clear=False,
        ):
            with self.assertRaisesRegex(tool.ReopenError, "Phase-1 transport was restored"):
                tool.command_operate(args)

        self.assertIn(("site-ingress", "prepare-commit", "execute"), calls)
        self.assertIn(("site-ingress", "commit", "execute"), calls)
        self.assertNotIn(("chain-transport", "commit", "execute"), calls)
        self.assertEqual(
            calls[-3:],
            [
                ("chain-transport", "close", "execute"),
                ("fps-server", "rollback", "execute"),
                ("site-ingress", "close", "execute"),
            ],
        )
        failure = json.loads((evidence_dir / "reopen-failure.json").read_text())
        self.assertEqual(
            failure["failedReason"],
            "site commit driver did not create a result",
        )

    def test_invoke_driver_forwards_a_hash_validated_peer_token(self) -> None:
        evidence_dir = self.root / "invoke-peer-evidence"
        evidence_dir.mkdir()
        peer = write(self.root / "peer.result.json", canonical({"peer": "token"}), 0o400)
        peer_pin = {"path": str(peer), "sha256": digest(peer)}
        observed: list[list[str]] = []

        class FakeProcess:
            pid = 999999
            returncode = 0

            def communicate(self, timeout=None):
                return "", ""

            def poll(self):
                return 0

        def fake_popen(command, **_kwargs):
            command = list(command)
            observed.append(command)
            result_path = pathlib.Path(command[command.index("--result") + 1])
            result = self.make_result("chain-transport", "commit", "execute")
            result["planSha256"] = "f" * 64
            write(result_path, canonical(result), 0o400)
            return FakeProcess()

        with mock.patch.object(tool.subprocess, "Popen", side_effect=fake_popen):
            _result, result_pin = tool.invoke_driver(
                self.root / "plan.json",
                self.plan,
                "f" * 64,
                evidence_dir,
                "chain-transport",
                "commit",
                "execute",
                peer_commit_result=peer_pin,
            )

        command = observed.pop()
        peer_index = command.index("--peer-commit-result")
        self.assertEqual(
            command[peer_index : peer_index + 4],
            [
                "--peer-commit-result",
                str(peer),
                "--peer-commit-result-sha256",
                digest(peer),
            ],
        )
        self.assertEqual(result_pin["sha256"], digest(pathlib.Path(result_pin["path"])))

        with mock.patch.object(tool.subprocess, "Popen") as runner:
            with self.assertRaisesRegex(tool.ReopenError, "peer commit result hash mismatch"):
                tool.invoke_driver(
                    self.root / "plan.json",
                    self.plan,
                    "f" * 64,
                    evidence_dir,
                    "chain-transport",
                    "commit",
                    "execute",
                    peer_commit_result={"path": str(peer), "sha256": "0" * 64},
                )
            runner.assert_not_called()

    def test_invoke_driver_rejects_peer_token_for_non_commit_actions(self) -> None:
        evidence_dir = self.root / "invoke-non-commit-peer-evidence"
        evidence_dir.mkdir()
        peer = write(self.root / "non-commit-peer.result.json", canonical({"peer": "token"}), 0o400)
        peer_pin = {"path": str(peer), "sha256": digest(peer)}
        with mock.patch.object(tool.subprocess, "Popen") as runner:
            with self.assertRaisesRegex(tool.ReopenError, "peer commit result.*commit"):
                tool.invoke_driver(
                    self.root / "plan.json",
                    self.plan,
                    "f" * 64,
                    evidence_dir,
                    "chain-transport",
                    "verify",
                    "execute",
                    peer_commit_result=peer_pin,
                )
            runner.assert_not_called()

    def test_invoke_driver_scrubs_all_deployment_secrets_from_child_environment(self) -> None:
        evidence_dir = self.root / "invoke-secret-boundary-evidence"
        evidence_dir.mkdir()
        observed_environment: dict[str, str] = {}

        class FakeProcess:
            pid = 999999
            returncode = 0

            def communicate(self, timeout=None):
                return "", ""

            def poll(self):
                return 0

        def fake_popen(command, **kwargs):
            observed_environment.update(kwargs["env"])
            result_path = pathlib.Path(command[command.index("--result") + 1])
            result = self.make_result("chain-transport", "preflight", "dry-run")
            result["planSha256"] = "f" * 64
            write(result_path, canonical(result), 0o400)
            return FakeProcess()

        sentinels = {
            name: f"COORDINATOR_{index:02d}_{name}_SENTINEL"
            for index, name in enumerate(
                sorted(DEPLOYMENT_SECRET_ENVIRONMENT_NAMES), start=1
            )
        }
        environment = {
            **sentinels,
            "NEXUS_TEST_NON_SECRET_MARKER": "preserved-non-secret-marker",
            "NEXUS_V2_POST_ACCEPTANCE_REOPEN_CONFIRMATION": "PRIVATE_ALPHA_RESTRICTED_REOPEN",
        }
        with mock.patch.dict(os.environ, environment, clear=False), mock.patch.object(
            tool.subprocess, "Popen", side_effect=fake_popen
        ):
            tool.invoke_driver(
                self.root / "plan.json",
                self.plan,
                "f" * 64,
                evidence_dir,
                "chain-transport",
                "preflight",
                "dry-run",
            )

        self.assertEqual(
            observed_environment["NEXUS_TEST_NON_SECRET_MARKER"],
            "preserved-non-secret-marker",
        )
        self.assertEqual(
            observed_environment["NEXUS_V2_POST_ACCEPTANCE_REOPEN_CONFIRMATION"],
            "PRIVATE_ALPHA_RESTRICTED_REOPEN",
        )
        for name, sentinel in sentinels.items():
            self.assertNotIn(name, observed_environment)
            self.assertFalse(
                any(sentinel in value for value in observed_environment.values()),
                name,
            )

    def test_invoke_driver_terminates_the_entire_process_group_on_timeout(self) -> None:
        evidence_dir = self.root / "invoke-timeout-evidence"
        evidence_dir.mkdir()
        observed_kwargs: dict = {}

        class TimedOutProcess:
            pid = 424242
            returncode = None

            def communicate(self, timeout=None):
                raise tool.subprocess.TimeoutExpired(["driver"], timeout)

            def poll(self):
                return self.returncode

            def wait(self, timeout=None):
                if timeout is not None:
                    raise tool.subprocess.TimeoutExpired(["driver"], timeout)
                self.returncode = -tool.signal.SIGKILL
                return self.returncode

        def fake_popen(_command, **kwargs):
            observed_kwargs.update(kwargs)
            return TimedOutProcess()

        with mock.patch.object(tool.subprocess, "Popen", side_effect=fake_popen), mock.patch.object(
            tool.os, "killpg"
        ) as kill_group, mock.patch.object(
            tool.signal, "signal", return_value=tool.signal.SIG_DFL
        ) as install_handler:
            with self.assertRaisesRegex(tool.ReopenError, "driver timed out"):
                tool.invoke_driver(
                    self.root / "plan.json",
                    self.plan,
                    "f" * 64,
                    evidence_dir,
                    "chain-transport",
                    "preflight",
                    "execute",
                )

        self.assertIs(observed_kwargs["start_new_session"], True)
        self.assertEqual(
            kill_group.call_args_list,
            [
                mock.call(TimedOutProcess.pid, tool.signal.SIGTERM),
                mock.call(TimedOutProcess.pid, tool.signal.SIGKILL),
            ],
        )
        installed_signals = [call.args[0] for call in install_handler.call_args_list[:3]]
        self.assertEqual(
            installed_signals,
            [tool.signal.SIGHUP, tool.signal.SIGINT, tool.signal.SIGTERM],
        )

    def test_partial_open_failure_closes_chain_before_site_and_never_restores_state(self) -> None:
        calls: list[tuple[str, str, str]] = []
        failed = False

        def fake_invoke(
            _path,
            _plan,
            _sha,
            _directory,
            component,
            action,
            mode,
            peer_commit_result=None,
            fps_adoption_seal=None,
        ):
            nonlocal failed
            calls.append((component, action, mode))
            if component == "fps-server" and action in {"promote", "verify"}:
                result = self.make_result(component, action, mode)
                result["planSha256"] = _sha
                result_path = write(
                    self.root / f"partial-open-{component}-{action}.json",
                    canonical(result),
                    0o400,
                )
                return result, self.pin_path(result_path)
            if component == "site-ingress" and action == "open" and not failed:
                failed = True
                raise tool.ReopenError("simulated partial site open")
            return {"alreadyApplied": False}, {"path": f"/{component}-{action}", "sha256": "1" * 64}

        evidence_dir = self.root / "failure-evidence"
        args = self.operation_args("execute", evidence_dir)
        with mock.patch.object(tool, "load_plan", return_value=self.plan), mock.patch.object(
            tool, "invoke_driver", side_effect=fake_invoke
        ), mock.patch.dict(
            os.environ,
            {"NEXUS_V2_POST_ACCEPTANCE_REOPEN_CONFIRMATION": "PRIVATE_ALPHA_RESTRICTED_REOPEN"},
            clear=False,
        ):
            with self.assertRaisesRegex(tool.ReopenError, "Phase-1 transport was restored"):
                tool.command_operate(args)
        self.assertEqual(
            calls[-3:],
            [
                ("chain-transport", "close", "execute"),
                ("fps-server", "rollback", "execute"),
                ("site-ingress", "close", "execute"),
            ],
        )
        failure = json.loads((evidence_dir / "reopen-failure.json").read_text())
        self.assertTrue(failure["transportCloseCompleted"])
        self.assertFalse(failure["chainStateMutationPerformed"])
        self.assertFalse(failure["chainStateRollbackPerformed"])

    def test_lost_promotion_output_after_remote_commit_still_rolls_fps_back(self) -> None:
        calls: list[tuple[str, str, str]] = []
        remote_commit_receipt = self.root / "lost-output-remote-fps-receipt.json"

        def fake_invoke(
            _path,
            _plan,
            _sha,
            _directory,
            component,
            action,
            mode,
            peer_commit_result=None,
            fps_adoption_seal=None,
        ):
            calls.append((component, action, mode))
            if component == "fps-server" and action == "promote":
                write(
                    remote_commit_receipt,
                    canonical({"remotePromotionCommitted": True}),
                    0o400,
                )
                raise tool.ReopenError(
                    "promotion transport lost after remote deployment commit"
                )
            return {"alreadyApplied": False}, {
                "path": f"/{component}-{action}",
                "sha256": "1" * 64,
            }

        evidence_dir = self.root / "lost-promotion-output-evidence"
        args = self.operation_args("execute", evidence_dir)
        with mock.patch.object(tool, "load_plan", return_value=self.plan), mock.patch.object(
            tool, "invoke_driver", side_effect=fake_invoke
        ), mock.patch.dict(
            os.environ,
            {"NEXUS_V2_POST_ACCEPTANCE_REOPEN_CONFIRMATION": "PRIVATE_ALPHA_RESTRICTED_REOPEN"},
            clear=False,
        ):
            with self.assertRaisesRegex(tool.ReopenError, "Phase-1 transport was restored"):
                tool.command_operate(args)
        self.assertTrue(remote_commit_receipt.is_file())
        self.assertIn(("fps-server", "promote", "execute"), calls)
        self.assertEqual(
            calls[-3:],
            [
                ("chain-transport", "close", "execute"),
                ("fps-server", "rollback", "execute"),
                ("site-ingress", "close", "execute"),
            ],
        )
        failure = json.loads((evidence_dir / "reopen-failure.json").read_text())
        self.assertEqual(
            failure["failedReason"],
            "promotion transport lost after remote deployment commit",
        )

    def test_active_verify_failure_uses_emergency_close_sequence(self) -> None:
        calls: list[tuple[str, str, str]] = []
        failed = False

        def fake_invoke(
            _path,
            _plan,
            _sha,
            _directory,
            component,
            action,
            mode,
            peer_commit_result=None,
            fps_adoption_seal=None,
        ):
            nonlocal failed
            calls.append((component, action, mode))
            if component == "fps-server" and action == "verify":
                result = self.make_result(component, action, mode)
                result["planSha256"] = _sha
                result_path = write(
                    self.root / "active-verify-fps.json",
                    canonical(result),
                    0o400,
                )
                return result, self.pin_path(result_path)
            if component == "site-ingress" and action == "verify" and not failed:
                failed = True
                raise tool.ReopenError("simulated live drift")
            return {"alreadyApplied": False}, {"path": f"/{component}-{action}", "sha256": "1" * 64}

        evidence_dir = self.root / "verify-failure-evidence"
        args = self.operation_args("verify", evidence_dir)
        adoption_pin = self.capture_adoption_fixture(args.expected_sha256)
        args.fps_adoption_seal = adoption_pin["path"]
        args.fps_adoption_seal_sha256 = adoption_pin["sha256"]
        with mock.patch.object(tool, "load_plan", return_value=self.plan), mock.patch.object(
            tool, "invoke_driver", side_effect=fake_invoke
        ), mock.patch.dict(
            os.environ,
            {"NEXUS_V2_POST_ACCEPTANCE_REOPEN_CONFIRMATION": "PRIVATE_ALPHA_RESTRICTED_REOPEN"},
            clear=False,
        ):
            with self.assertRaisesRegex(tool.ReopenError, "Phase-1 transport was restored"):
                tool.command_operate(args)
        self.assertEqual(
            calls[-3:],
            [
                ("chain-transport", "close", "execute"),
                ("fps-server", "rollback", "execute"),
                ("site-ingress", "close", "execute"),
            ],
        )

    def test_active_verify_reuses_exact_adoption_seal_and_fresh_receipt(self) -> None:
        calls: list[tuple[str, str, str, dict[str, str] | None]] = []
        evidence_dir = self.root / "verify-success-evidence"
        args = self.operation_args("verify", evidence_dir)
        adoption_pin = self.capture_adoption_fixture(args.expected_sha256)
        args.fps_adoption_seal = adoption_pin["path"]
        args.fps_adoption_seal_sha256 = adoption_pin["sha256"]

        def fake_invoke(
            _path,
            _plan,
            _sha,
            _directory,
            component,
            action,
            mode,
            peer_commit_result=None,
            fps_adoption_seal=None,
        ):
            calls.append((component, action, mode, fps_adoption_seal))
            if component == "fps-server" and action == "verify":
                result = self.make_result(component, action, mode)
                result["planSha256"] = _sha
                result_path = write(
                    self.root / "successful-active-fps-verify.json",
                    canonical(result),
                    0o400,
                )
                return result, self.pin_path(result_path)
            return {"alreadyApplied": False}, {
                "path": f"/{component}-{action}",
                "sha256": "1" * 64,
            }

        with mock.patch.object(tool, "load_plan", return_value=self.plan), mock.patch.object(
            tool, "invoke_driver", side_effect=fake_invoke
        ), mock.patch.dict(
            os.environ,
            {"NEXUS_V2_POST_ACCEPTANCE_REOPEN_CONFIRMATION": "PRIVATE_ALPHA_RESTRICTED_REOPEN"},
            clear=False,
        ):
            tool.command_operate(args)

        site_verify = next(
            call for call in calls if call[:3] == ("site-ingress", "verify", "execute")
        )
        self.assertEqual(site_verify[3], adoption_pin)
        evidence = json.loads((evidence_dir / "reopen-evidence.json").read_text())
        self.assertEqual(evidence["fpsAdoptionSeal"], adoption_pin)
        self.assertFalse((evidence_dir / "fps-adoption-seal.json").exists())

    def test_explicit_and_failure_close_sequence_is_chain_first(self) -> None:
        self.assertEqual(
            tool.close_sequence(),
            [
                ("chain-transport", "close", "execute"),
                ("fps-server", "rollback", "execute"),
                ("site-ingress", "close", "execute"),
            ],
        )
        coordinator = SCRIPT.read_text()
        close_loop = coordinator[
            coordinator.index("def close_sequence()") : coordinator.index(
                "def command_operate", coordinator.index("def close_sequence()")
            )
        ]
        self.assertLess(
            close_loop.index('("chain-transport", "close", "execute")'),
            close_loop.index('("fps-server", "rollback", "execute")'),
        )
        self.assertLess(
            close_loop.index('("fps-server", "rollback", "execute")'),
            close_loop.index('("site-ingress", "close", "execute")'),
        )

    def test_emergency_driver_neither_sources_env_nor_honors_mutable_ssh_opts(self) -> None:
        emergency = (HERE / "nexus-v2-post-acceptance-emergency-close-driver").read_text()
        active_lines = [
            line
            for line in emergency.splitlines()
            if line.strip() and not line.lstrip().startswith("#")
        ]
        active = "\n".join(active_lines)
        self.assertNotRegex(active, r"(?m)^\s*(?:source|\.)\s+")
        self.assertNotIn("extra_ssh_opts", active)
        self.assertNotRegex(active, r"SSH_CMD.*SSH_OPTS|SSH_OPTS.*SSH_CMD")
        self.assertNotRegex(active, r"\beval\b")
        self.assertIn("selected credential environment", emergency)

    def test_emergency_credentials_never_enter_argv_remote_command_or_output(self) -> None:
        sudo_secret = "SENTINEL_REMOTE_SUDO_PASSWORD_b3043f91"
        bundle = self.root / "credential-secrecy-bundle"
        bundle.mkdir()
        driver = write(
            bundle / "nexus-v2-post-acceptance-emergency-close-driver",
            (HERE / "nexus-v2-post-acceptance-emergency-close-driver").read_bytes(),
            0o700,
        )
        helper = write(bundle / "close-helper", b"#!/bin/sh\nexit 0\n", 0o700)
        normal = write(bundle / "normal.Caddyfile", b"normal\n", 0o400)
        phase1 = write(bundle / "phase1.Caddyfile", b"phase1\n", 0o400)
        identity = write(bundle / "identity", b"test identity\n", 0o600)
        emergency_known_hosts = write(
            bundle / "ssh-known-hosts", self.ssh_known_hosts.read_bytes(), 0o600
        )
        emergency_host_pin_manifest = write(
            bundle / "ssh-host-pin-manifest",
            self.ssh_host_pin_manifest.read_bytes(),
            0o600,
        )
        emergency_host_pin_validator = write(
            bundle / "ssh-host-pin-validator", PINS_SCRIPT.read_bytes(), 0o700
        )
        sudo_secret_file = write(
            bundle / "sudo-secret", (sudo_secret + "\n").encode(), 0o600
        )
        credentials = write(
            bundle / "credentials.env",
            (
                "DEPLOY_HOST=192.168.1.159\n"
                "DEPLOY_USER=eterra2010\n"
                "SSH_PORT=22\n"
                f"SSH_IDENTITY_FILE={identity}\n"
                "SSH_OPTS=\n"
                f"REMOTE_SUDO_PASSWORD=@{sudo_secret_file.resolve()}\n"
            ).encode(),
            0o600,
        )
        plan = json.loads(json.dumps(self.plan))
        plan["selectedDeploymentEnvironment"] = {
            "path": str(credentials),
            "sha256": digest(credentials),
        }
        plan["selectedSiteDeploymentEnvironment"] = {
            "path": str(credentials),
            "sha256": digest(credentials),
        }
        plan["emergencyClosure"] = {
            "bundleRoot": str(bundle),
            "driver": {"path": str(driver), "sha256": digest(driver)},
            "helpers": {
                "chain-transport": {"path": str(helper), "sha256": digest(helper)},
                "fps-server": {"path": str(helper), "sha256": digest(helper)},
                "site-ingress": {"path": str(helper), "sha256": digest(helper)},
            },
            "libraries": self.plan["emergencyClosure"]["libraries"],
            "unityFpsDeploymentEnvironment": self.plan["emergencyClosure"][
                "unityFpsDeploymentEnvironment"
            ],
            "fps": self.plan["emergencyClosure"]["fps"],
            "caddyfiles": {
                "normal": {"path": str(normal), "sha256": digest(normal)},
                "phase1": {"path": str(phase1), "sha256": digest(phase1)},
            },
            "sshHostPins": {
                "knownHosts": {
                    "path": str(emergency_known_hosts),
                    "sha256": digest(emergency_known_hosts),
                },
                "manifest": {
                    "path": str(emergency_host_pin_manifest),
                    "sha256": digest(emergency_host_pin_manifest),
                },
                "validator": {
                    "path": str(emergency_host_pin_validator),
                    "sha256": digest(emergency_host_pin_validator),
                },
            },
            "targets": {
                "chainHost": "192.168.1.159",
                "chainUser": "eterra2010",
                "siteHost": "192.168.1.218",
                "siteUser": "eterra2014",
            },
        }
        plan_path = write(bundle / "plan.json", canonical(plan), 0o400)
        result_path = bundle / "result.json"
        capture_path = bundle / "ssh-capture.json"
        fake_bin = bundle / "fake-bin"
        fake_bin.mkdir()
        write(
            fake_bin / "ssh",
            f"""#!/usr/bin/env python3
import base64
import json
import os
import pathlib
import sys

sudo_secret = {sudo_secret!r}
stdin = sys.stdin.buffer.read()
credential, separator, payload = stdin.partition(b"\\n")
record = {{
    "readToEof": True,
    "credentialMatches": separator == b"\\n" and credential.decode() == sudo_secret,
    "payloadNonempty": bool(payload),
    "argvSecretFree": not any(sudo_secret in value for value in sys.argv),
    "remoteCommandSecretFree": sudo_secret not in sys.argv[-1],
    "environmentSecretFree": not any(
        sudo_secret in value for value in os.environ.values()
    ),
    "credentialVariablesAbsent": not any(
        name in os.environ for name in ("DEPLOY_PASSWORD", "REMOTE_SUDO_PASSWORD")
    ),
    "argv": sys.argv[1:],
}}
pathlib.Path(os.environ["NEXUS_TEST_SSH_CAPTURE"]).write_text(
    json.dumps(record, sort_keys=True), encoding="utf-8"
)
result = base64.b64encode(b"{{}}\\n").decode()
print("NEXUS_V2_REOPEN_RESULT:" + result)
""".encode(),
            0o700,
        )
        environment = os.environ.copy()
        environment["PATH"] = f"{fake_bin}:{environment['PATH']}"
        environment["NEXUS_TEST_SSH_CAPTURE"] = str(capture_path)
        environment["DEPLOY_PASSWORD"] = "AMBIENT_DEPLOY_PASSWORD_must_not_escape"
        environment["REMOTE_SUDO_PASSWORD"] = sudo_secret
        completed = tool.subprocess.run(
            [
                str(driver),
                "--component",
                "chain-transport",
                "--action",
                "close",
                "--mode",
                "execute",
                "--operation-id",
                plan["operationId"],
                "--plan",
                str(plan_path),
                "--plan-sha256",
                digest(plan_path),
                "--result",
                str(result_path),
            ],
            env=environment,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertNotIn(sudo_secret, completed.stdout)
        self.assertNotIn(sudo_secret, completed.stderr)
        self.assertNotIn(sudo_secret, result_path.read_text())
        record = json.loads(capture_path.read_text())
        self.assertTrue(record["argvSecretFree"])
        self.assertTrue(record["remoteCommandSecretFree"])
        self.assertTrue(record["environmentSecretFree"])
        self.assertTrue(record["credentialVariablesAbsent"])
        self.assertTrue(record["readToEof"])
        self.assertTrue(record["credentialMatches"])
        self.assertTrue(record["payloadNonempty"])
        spawn = record["argv"]
        self.assertEqual(spawn[:2], ["-F", "/dev/null"])
        for option in (
            "Hostname=192.168.1.159",
            "HostKeyAlias=192.168.1.159",
            f"UserKnownHostsFile={emergency_known_hosts}",
            "GlobalKnownHostsFile=/dev/null",
            "StrictHostKeyChecking=yes",
            "UpdateHostKeys=no",
            "KnownHostsCommand=none",
            "VerifyHostKeyDNS=no",
            "CheckHostIP=yes",
            "CanonicalizeHostname=no",
            "ProxyCommand=none",
            "ProxyJump=none",
            "IdentitiesOnly=yes",
            "IdentityAgent=none",
            "BatchMode=yes",
            "PasswordAuthentication=no",
            "KbdInteractiveAuthentication=no",
            "PreferredAuthentications=publickey",
            "NumberOfPasswordPrompts=0",
            "RequestTTY=no",
        ):
            self.assertEqual(spawn.count(option), 1)
        self.assertNotIn("StrictHostKeyChecking=accept-new", spawn)
        emergency = driver.read_text(encoding="utf-8")
        self.assertNotIn("expect -f", emergency)
        self.assertNotIn("SUDO_ASKPASS", emergency)
        self.assertNotIn("sudo -A", emergency)
        self.assertIn("DEPLOY_PASSWORD is forbidden", emergency)
        self.assertIn("@/absolute/path owner-only", emergency)

    def test_component_helpers_use_global_exclusive_flock_race_tokens(self) -> None:
        fixtures = (
            (
                "nexus-v2-post-acceptance-reopen-host-action.sh",
                "/run/lock/nexus-v2-post-acceptance-reopen-chain-transport.lock",
            ),
            (
                "nexus-v2-post-acceptance-reopen-site-action.sh",
                "/run/lock/nexus-v2-post-acceptance-reopen-site-ingress.lock",
            ),
        )
        for filename, lock_path in fixtures:
            with self.subTest(filename=filename):
                source = (HERE / filename).read_text()
                self.assertIn(lock_path, source)
                lock_assignment = source.index(f'LOCK_FILE="{lock_path}"')
                descriptor = source.index('exec 9>"${LOCK_FILE}"', lock_assignment)
                acquisition = source.index("flock -x -w 180 9", descriptor)
                action_dispatch = source.index('case "${action}" in', acquisition)
                self.assertLess(lock_assignment, descriptor)
                self.assertLess(descriptor, acquisition)
                self.assertLess(acquisition, action_dispatch)
                self.assertNotIn("operation_id", source[lock_assignment:descriptor])
                self.assertNotIn('rm -f "${LOCK_FILE}"', source)

    def test_alternate_port_translation_targets_protected_ports_are_rejected(self) -> None:
        host = (HERE / "nexus-v2-post-acceptance-reopen-host-action.sh").read_text()
        validator = embedded_python(host, "require_no_protected_port_translation")

        destination_match = {
            "match": {
                "op": "==",
                "left": {"payload": {"protocol": "tcp", "field": "dport"}},
                "right": 18080,
            }
        }
        fixtures = {
            "ipv4-dnat": ("ip", {"dnat": {"addr": "127.0.0.1", "port": 9944}}),
            "ipv6-dnat": ("ip6", {"dnat": {"addr": "::1", "port": 9944}}),
            "redirect": ("inet", {"redirect": {"port": 9944}}),
            "tproxy": ("inet", {"tproxy": {"addr": "127.0.0.1", "port": 9944}}),
        }
        for name, (family, translation) in fixtures.items():
            with self.subTest(name=name):
                payload = {
                    "nftables": [
                        {
                            "rule": {
                                "family": family,
                                "table": "nat",
                                "chain": "prerouting",
                                "expr": [destination_match, translation],
                            }
                        }
                    ]
                }
                fixture = write(
                    self.root / f"{name}-ruleset.json",
                    json.dumps(payload).encode(),
                )
                completed = tool.subprocess.run(
                    ["python3", "-", str(fixture)],
                    input=validator,
                    capture_output=True,
                    text=True,
                    check=False,
                )
                self.assertNotEqual(
                    completed.returncode,
                    0,
                    f"{name} alternate-port target reached a protected port",
                )

        safe = write(
            self.root / "safe-ruleset.json",
            json.dumps(
                {
                    "nftables": [
                        {
                            "rule": {
                                "family": "ip",
                                "table": "nat",
                                "chain": "prerouting",
                                "expr": [
                                    destination_match,
                                    {"dnat": {"addr": "127.0.0.1", "port": 1234}},
                                ],
                            }
                        }
                    ]
                }
            ).encode(),
        )
        completed = tool.subprocess.run(
            ["python3", "-", str(safe)],
            input=validator,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)

    def test_legacy_iptables_nat_translation_is_part_of_the_closed_check(self) -> None:
        host = (HERE / "nexus-v2-post-acceptance-reopen-host-action.sh").read_text()
        translation = shell_function(host, "require_no_protected_port_translation")
        self.assertIn("iptables-save", translation)
        self.assertIn("ip6tables-save", translation)
        validators = embedded_pythons(host, "require_no_protected_port_translation")
        self.assertEqual(len(validators), 2)
        validator = validators[1]

        fixtures = {
            "ipv4-dnat": (
                "*nat\n-A PREROUTING -p tcp --dport 18080 -j DNAT "
                "--to-destination 127.0.0.1:9944\nCOMMIT\n",
                "*nat\nCOMMIT\n",
            ),
            "ipv6-dnat": (
                "*nat\nCOMMIT\n",
                "*nat\n-A PREROUTING -p tcp --dport 18080 -j DNAT "
                "--to-destination [::1]:9944\nCOMMIT\n",
            ),
            "redirect": (
                "*nat\n-A PREROUTING -p tcp --dport 18080 -j REDIRECT "
                "--to-ports 9944\nCOMMIT\n",
                "*nat\nCOMMIT\n",
            ),
            "tproxy": (
                "*mangle\n-A PREROUTING -p tcp --dport 18080 -j TPROXY "
                "--on-port 9944\nCOMMIT\n",
                "*nat\nCOMMIT\n",
            ),
        }
        for name, (ipv4, ipv6) in fixtures.items():
            with self.subTest(name=name):
                ipv4_path = write(self.root / f"{name}-iptables4", ipv4.encode())
                ipv6_path = write(self.root / f"{name}-iptables6", ipv6.encode())
                completed = tool.subprocess.run(
                    ["python3", "-", str(ipv4_path), str(ipv6_path)],
                    input=validator,
                    capture_output=True,
                    text=True,
                    check=False,
                )
                self.assertNotEqual(
                    completed.returncode,
                    0,
                    f"legacy {name} alternate-port target reached a protected port",
                )

        safe_ipv4 = write(
            self.root / "safe-iptables4",
            (
                "*nat\n-A PREROUTING -p tcp --dport 18080 -j DNAT "
                "--to-destination 127.0.0.1:1234\nCOMMIT\n"
            ).encode(),
        )
        safe_ipv6 = write(self.root / "safe-iptables6", b"*nat\nCOMMIT\n")
        completed = tool.subprocess.run(
            ["python3", "-", str(safe_ipv4), str(safe_ipv6)],
            input=validator,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)

    def test_watchdog_guard_hashes_every_payload_and_exact_unit(self) -> None:
        payloads = (
            "WATCHDOG_SCRIPT",
            "WATCHDOG_HELPER",
            "WATCHDOG_PLAN",
            "WATCHDOG_NORMAL_CADDY",
            "WATCHDOG_PHASE1_CADDY",
            "WATCHDOG_SERVICE",
            "WATCHDOG_TIMER",
        )
        for filename in (
            "nexus-v2-post-acceptance-reopen-host-action.sh",
            "nexus-v2-post-acceptance-reopen-site-action.sh",
        ):
            with self.subTest(filename=filename):
                source = (HERE / filename).read_text()
                guard = shell_function(source, "require_guard_state")
                for payload in payloads:
                    matching = [line.lower() for line in guard.splitlines() if payload in line]
                    self.assertTrue(matching, f"{payload} is not verified by {filename}")
                    self.assertTrue(
                        any("sha256" in line or "hash" in line for line in matching),
                        f"{payload} bytes are not hash verified by {filename}",
                    )

    def test_watchdog_absence_checks_files_activity_enablement_and_load_state(self) -> None:
        payloads = (
            "WATCHDOG_SCRIPT",
            "WATCHDOG_HELPER",
            "WATCHDOG_PLAN",
            "WATCHDOG_NORMAL_CADDY",
            "WATCHDOG_PHASE1_CADDY",
        )
        for filename in (
            "nexus-v2-post-acceptance-reopen-host-action.sh",
            "nexus-v2-post-acceptance-reopen-site-action.sh",
        ):
            with self.subTest(filename=filename):
                source = (HERE / filename).read_text()
                absent = shell_function(source, "require_watchdog_absent")
                for payload in payloads:
                    self.assertIn(payload, absent)
                self.assertIn("WATCHDOG_SERVICE", absent)
                self.assertIn("WATCHDOG_TIMER", absent)
                self.assertIn("is-active", absent)
                self.assertIn("is-enabled", absent)
                self.assertIn("LoadState", absent)
                self.assertIn("not-found", absent)

    def test_chain_proxy_sockets_are_runtime_only_and_never_enabled_for_reboot(self) -> None:
        host = (HERE / "nexus-v2-post-acceptance-reopen-host-action.sh").read_text()
        units = shell_function(host, "write_units")
        opened = shell_function(host, "require_units_open")
        self.assertNotIn("WantedBy=sockets.target", units)
        self.assertNotIn('systemctl enable --now "$(unit_name', host)
        self.assertIn('systemctl start "$(unit_name', host)
        self.assertIn("is-enabled", opened)
        self.assertRegex(opened, r"!\s+systemctl is-enabled|is-enabled[^\n]+(?:disabled|not-found)")

    def test_site_watchdog_boots_fail_closed_before_docker_can_publish_caddy(self) -> None:
        site = (HERE / "nexus-v2-post-acceptance-reopen-site-action.sh").read_text()
        watchdog = shell_function(site, "arm_watchdog")
        exact = shell_function(site, "require_boot_guard_exact")
        absent = shell_function(site, "require_boot_guard_absent")
        self.assertIn("DefaultDependencies=no", watchdog)
        self.assertIn("Before=docker.service", watchdog)
        self.assertIn("RequiredBy=docker.service", watchdog)
        self.assertIn('systemctl enable "${BOOT_GUARD_SERVICE}"', watchdog)
        self.assertLess(
            watchdog.index("sha256sum '${BOOT_GUARD_PHASE1}'"),
            watchdog.index("cp '${BOOT_GUARD_PHASE1}' '${REMOTE_CADDYFILE}'"),
        )
        self.assertGreater(
            watchdog.rindex("sha256sum '${REMOTE_CADDYFILE}'"),
            watchdog.index("cp '${BOOT_GUARD_PHASE1}' '${REMOTE_CADDYFILE}'"),
        )
        for payload in (
            "BOOT_GUARD_SCRIPT",
            "BOOT_GUARD_PHASE1",
            "BOOT_GUARD_SERVICE",
        ):
            matching = [line.lower() for line in exact.splitlines() if payload in line]
            self.assertTrue(matching, f"{payload} is not verified by the boot guard")
            self.assertTrue(
                any("sha256" in line or "hash" in line for line in matching),
                f"{payload} is not hash verified by the boot guard",
            )
        self.assertIn("phase1_caddy_sha256", exact)
        self.assertIn("is-enabled", exact)
        self.assertIn("FragmentPath", exact)
        self.assertIn("DropInPaths", exact)
        for payload in (
            "BOOT_GUARD_SCRIPT",
            "BOOT_GUARD_PHASE1",
            "BOOT_GUARD_MANIFEST",
            "BOOT_GUARD_SERVICE",
        ):
            self.assertIn(payload, absent)
        self.assertIn("is-active", absent)
        self.assertIn("is-enabled", absent)
        self.assertIn("LoadState", absent)
        self.assertIn("not-found", absent)
        open_branch = site[site.index("\n\topen)\n") : site.index("\n\tverify)\n")]
        self.assertLess(open_branch.index("arm_watchdog"), open_branch.index("install_caddy"))

    def test_helper_action_and_argument_envelopes_are_closed(self) -> None:
        host = (HERE / "nexus-v2-post-acceptance-reopen-host-action.sh").read_text()
        site = (HERE / "nexus-v2-post-acceptance-reopen-site-action.sh").read_text()
        driver = (HERE / "nexus-v2-post-acceptance-reopen-component-driver").read_text()

        self.assertIn(
            '[[ "${action}" =~ ^(preflight|open|adopt|verify|commit|close)$ ]]',
            host,
        )
        self.assertNotIn("prepare-commit|commit|close)$", host)
        self.assertIn(
            '[[ "${action}" =~ ^(preflight|open|verify|prepare-commit|commit|close)$ ]]',
            site,
        )

        host_envelope = host[: host.index("\n\nfor command in ")]
        self.assertIn('[[ $# -eq 8 ]]', host_envelope)
        self.assertIn('[[ $# -eq 7 ]]', host_envelope)
        self.assertIn('if [[ "$1" == commit ]]', host_envelope)
        site_envelope = site[: site.index("\n\nfor command in ")]
        self.assertIn('[[ $# -eq 10 ]]', site_envelope)
        self.assertIn('[[ $# -eq 9 ]]', site_envelope)
        self.assertIn('[[ $# -eq 7 ]]', site_envelope)
        self.assertIn('open|verify|prepare-commit)', site_envelope)

        peer_gate_start = driver.index('if [[ "${action}" == "commit" ]]')
        peer_gate_end = driver.index("\n\nfor command in ", peer_gate_start)
        peer_gate = driver[peer_gate_start:peer_gate_end]
        self.assertIn("final commit requires an absolute regular peer commit result", peer_gate)
        self.assertIn("peer commit result hash mismatch", peer_gate)
        self.assertIn("peer commit result is valid only for a final component commit", peer_gate)

        remote = shell_function(driver, "run_remote_helper")
        self.assertIn('if [[ -n "${fps_adoption_seal_base64}" && -n "${peer_commit_base64}" ]]', remote)
        self.assertIn('"${phase1_caddy_base64}" "${peer_commit_base64}"', remote)
        self.assertIn('"${fps_adoption_seal_base64}" "${fps_adoption_seal_sha256}"', remote)
        self.assertIn('"${normal_caddy_base64}" "${phase1_caddy_base64}"', remote)

    def test_peer_commit_helpers_validate_closed_action_specific_tokens(self) -> None:
        host = (HERE / "nexus-v2-post-acceptance-reopen-host-action.sh").read_text()
        site = (HERE / "nexus-v2-post-acceptance-reopen-site-action.sh").read_text()
        chain_validator = shell_function(host, "require_site_commit_token")
        site_validator = shell_function(site, "require_site_prepare_token")

        for expected in (
            'keys == ["acceptanceBoundaryReceiptSha256","action","alreadyApplied","checks",',
            '.componentId == "site-ingress" and .action == "commit" and .mode == "execute"',
            '.result == "passed" and .mutationPerformed == true',
            '.driverSha256 == $driverSha256',
            '.fpsAdoptionSealSha256 | test("^[0-9a-f]{64}$")',
            'fpsAdoptionSealPinned:true',
            'siteIngressPrepareTokenVerified:true',
            'final site-ingress commit token contract mismatch',
        ):
            self.assertIn(expected, chain_validator)
        for expected in (
            'keys == ["acceptanceBoundaryReceiptSha256","action","alreadyApplied","checks",',
            '.componentId == "site-ingress" and .action == "prepare-commit" and .mode == "execute"',
            '.result == "passed" and .mutationPerformed == true',
            '.driverSha256 == $driverSha256',
            '.fpsAdoptionSealSha256 == $fpsAdoptionSealSha256',
            'fpsAdoptionSealPinned:true',
            'coordinatorWatchdogArmed:true',
            'site-ingress prepare token contract mismatch',
        ):
            self.assertIn(expected, site_validator)

    def test_close_keeps_watchdog_armed_until_exposure_is_removed(self) -> None:
        host = (HERE / "nexus-v2-post-acceptance-reopen-host-action.sh").read_text()
        site = (HERE / "nexus-v2-post-acceptance-reopen-site-action.sh").read_text()
        chain_close = shell_function(host, "close_transport")
        site_close = shell_function(site, "close_site")

        chain_watchdog = chain_close.index("remove_watchdog")
        for removal in (
            "remove_units",
            "remove_permit_rules",
            "remove_nft_guard",
            'require_closed_or_absent_listener 8787 "authority"',
        ):
            self.assertLess(chain_close.index(removal), chain_watchdog)

        site_watchdog = site_close.index("remove_watchdog")
        for removal in (
            'install_caddy "${RETAINED_PHASE1}"',
            "require_site_firewall",
            "require_loopback_or_absent 3000 site",
            "require_loopback_or_absent 8787 indexer",
        ):
            self.assertLess(site_close.index(removal), site_watchdog)

    def test_watchdog_close_retries_and_uses_the_seven_argument_close_envelope(self) -> None:
        for filename in (
            "nexus-v2-post-acceptance-reopen-host-action.sh",
            "nexus-v2-post-acceptance-reopen-site-action.sh",
        ):
            with self.subTest(filename=filename):
                source = (HERE / filename).read_text()
                watchdog = shell_function(source, "arm_watchdog")
                self.assertIn("StartLimitIntervalSec=0", watchdog)
                self.assertIn("Restart=on-failure", watchdog)
                self.assertIn("RestartSec=5s", watchdog)
                close_line = next(
                    line for line in watchdog.splitlines() if "WATCHDOG_HELPER" in line and " close " in line
                )
                self.assertIn('close "\\${plan_base64}"', close_line)
                self.assertIn('"\\${normal_base64}" "\\${phase1_base64}"', close_line)
                self.assertNotIn("prepare_result", close_line)
                self.assertNotIn("commit_result", close_line)

    def test_all_helper_result_action_envelopes_have_exact_check_sets(self) -> None:
        for component, action in sorted(tool.CHECKS):
            with self.subTest(component=component, action=action):
                result = self.make_result(component, action, "execute")
                adoption_pin = None
                if component == "site-ingress" and action in tool.SITE_ADOPTION_ACTIONS:
                    adoption_path = write(
                        self.root / f"{component}-{action}.adoption.json",
                        canonical({"action": action}),
                        0o400,
                    )
                    adoption_pin = self.pin_path(adoption_path)
                    result["fpsAdoptionSealSha256"] = adoption_pin["sha256"]
                path = write(
                    self.root / f"{component}-{action}.result.json",
                    canonical(result),
                    0o400,
                )
                tool.validate_result(
                    path,
                    self.plan,
                    "f" * 64,
                    component,
                    action,
                    "execute",
                    fps_adoption_seal=adoption_pin,
                )
                result["checks"].pop(next(iter(result["checks"])))
                path.chmod(0o600)
                path.write_bytes(canonical(result))
                with self.assertRaisesRegex(tool.ReopenError, "closed schema"):
                    tool.validate_result(
                        path,
                        self.plan,
                        "f" * 64,
                        component,
                        action,
                        "execute",
                        fps_adoption_seal=adoption_pin,
                    )

    def test_remote_helpers_encode_narrow_transport_and_caddy_fail_closed(self) -> None:
        host = (HERE / "nexus-v2-post-acceptance-reopen-host-action.sh").read_text()
        site = (HERE / "nexus-v2-post-acceptance-reopen-site-action.sh").read_text()
        driver = (HERE / "nexus-v2-post-acceptance-reopen-component-driver").read_text()
        emergency = (HERE / "nexus-v2-post-acceptance-emergency-close-driver").read_text()
        self.assertIn("systemd-socket-proxyd", host)
        self.assertIn('ufw allow proto tcp from "${site_ip}" to "${chain_ip}"', host)
        self.assertIn("install_nft_guard", host)
        self.assertIn("dedicated nft guard semantic contract mismatch", host)
        self.assertIn("require_no_protected_port_translation", host)
        self.assertIn("final site-ingress commit token", host)
        self.assertIn("require_current_runtime_identity", host)
        self.assertIn("coordinator watchdog", host)
        self.assertIn("archive_marker_anomalies", host)
        self.assertIn("FORBIDDEN_PORTS=(30333 5001)", host)
        self.assertNotRegex(host, r"\[(?:chain-p2p|ipfs-api)\]=")
        self.assertIn("proxy socket ListenStream mismatch", host)
        self.assertIn("proxy service target mismatch", host)
        self.assertIn("proxy unit bytes drifted after reopen", host)
        self.assertIn("acceptanceBoundaryReceiptSha256", host)
        self.assertIn('install_caddy "${RETAINED_PHASE1}"', site)
        self.assertIn("require_phase1_routes", site)
        self.assertIn("require_public_reads", site)
        self.assertIn("require_current_runtime_authority", site)
        self.assertIn("require_deployment_identity", site)
        self.assertIn("require_authority_statuses", site)
        self.assertIn("coordinator watchdog", site)
        self.assertIn('"PUBLIC_MEDIA_UPLOAD_ENABLED": "false"', site)
        self.assertIn('"PUBLIC_AVATAR_UPLOAD_ENABLED": "false"', site)
        self.assertIn('"NEXUS_V2_SESSION_AUTHORIZATION_PRODUCTION_ENABLED": "false"', site)
        self.assertIn('"CHAIN_RPC_PORT": "9944"', site)
        self.assertIn('"IPFS_GATEWAY_PORT": "8080"', site)
        self.assertIn("normalCaddyfileSha256", site)
        self.assertIn("PRIVATE_ALPHA_RESTRICTED_REOPEN", driver)
        self.assertIn("--peer-commit-result", driver)
        self.assertIn("remote_root_bash", driver)
        self.assertIn("json.dumps(value, indent=2, sort_keys=True)", driver)
        self.assertIn("nexus-v2-pinned-host-v1|nexus-v2-pinned-key-only-v2", driver)
        for key_only_option in (
            "IdentityAgent=none",
            "BatchMode=yes",
            "PasswordAuthentication=no",
            "KbdInteractiveAuthentication=no",
            "PreferredAuthentications=publickey",
        ):
            self.assertIn(key_only_option, driver)
        fps_context = shell_function(driver, "load_fps_context")
        self.assertIn(
            'require_loaded_ssh_transport "${site_lan_ip}" "${DEPLOY_USER}"',
            fps_context,
        )
        self.assertIn("emergency authority permits execute mode only", emergency)
        self.assertNotIn("load_env", emergency)


if __name__ == "__main__":
    unittest.main()
