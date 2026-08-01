from __future__ import annotations

import argparse
import base64
import datetime as dt
import hashlib
import importlib.util
import json
import os
import pathlib
import stat
import tempfile
import types
import unittest
from unittest import mock


HERE = pathlib.Path(__file__).resolve().parent
SCRIPT = HERE / "phase2_internal_transport.py"
SPEC = importlib.util.spec_from_file_location("phase2_internal_transport_tested", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
tool = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(tool)


def canonical(value: dict) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + "\n").encode()


def digest(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


class Phase2InternalTransportTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="phase2-transport-")
        self.root = pathlib.Path(self.temporary.name).resolve()
        self.now = dt.datetime.now(dt.timezone.utc).replace(microsecond=0)
        self.plan = {
            "operationId": "phase2-test-1",
            "releaseId": "nexus-v2-private-alpha-test",
            "sourceCommit": "a" * 40,
            "remote": {
                "host": "192.168.1.159",
                "user": "eterra2010",
                "helper": {"path": "/helper", "sha256": "b" * 64, "sourceCommit": "a" * 40},
            },
            "replacementLock": {"path": "/replacement", "sha256": "c" * 64},
            "selectedDeploymentEnvironment": {"path": "/chain.env", "sha256": "d" * 64},
            "selectedSiteDeploymentEnvironment": {"path": "/site.env", "sha256": "e" * 64},
            "sshHostPins": {
                "knownHosts": {"path": "/known-hosts", "sha256": "1" * 64},
                "manifest": {"path": "/host-pins", "sha256": "2" * 64},
                "validator": {"path": "/pin-validator", "sha256": "3" * 64},
            },
        }

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def result(self, action: str = "verify") -> dict:
        operation = self.plan["operationId"]
        expires = (self.now + dt.timedelta(minutes=10)).strftime("%Y-%m-%dT%H:%M:%SZ")
        return {
            "schemaVersion": 1,
            "kind": tool.RESULT_KIND,
            "operationId": operation,
            "planSha256": "4" * 64,
            "releaseId": self.plan["releaseId"],
            "sourceCommit": self.plan["sourceCommit"],
            "action": action,
            "state": "open",
            "mutationPerformed": action == "renew",
            "alreadyApplied": action != "renew",
            "helperSha256": self.plan["remote"]["helper"]["sha256"],
            "marker": {
                "path": f"/opt/eterra-alpha/shared/phase2-internal-transport/{operation}/open.json",
                "sha256": "5" * 64,
            },
            "heartbeat": {
                "path": f"/opt/eterra-alpha/shared/phase2-internal-transport/{operation}/heartbeat.json",
                "nonce": "6" * 64,
                "expiresAtUtc": expires,
            },
            "watchdog": {
                "service": f"nexus-v2-phase2-internal-transport-{operation}.service",
                "timer": f"nexus-v2-phase2-internal-transport-{operation}.timer",
                "unitSha256": "7" * 64,
                "payloadSha256": "8" * 64,
                "armed": True,
            },
            "transport": {"network": tool.NETWORK, "ports": tool.PORTS},
            "safety": tool.POLICY,
            "completedAtUtc": self.now.strftime("%Y-%m-%dT%H:%M:%SZ"),
        }

    def handoff(self) -> dict:
        operation = self.plan["operationId"]
        return {
            "schemaVersion": 1,
            "kind": tool.HANDOFF_KIND,
            "releaseId": self.plan["releaseId"],
            "siteReleaseVersion": "v0.1.0-alpha.1",
            "sourceCommit": self.plan["sourceCommit"],
            "siteSourceCommit": "8" * 40,
            "acceptanceBoundaryReceiptSha256": "9" * 64,
            "replacementLockSha256": "a" * 64,
            "sitePhase1PostDeployIdentitySha256": "b" * 64,
            "sitePostPhase2DeploymentIdentitySha256": "c" * 64,
            "network": tool.NETWORK,
            "ports": tool.PORTS,
            "lease": {
                "operationId": operation,
                "planSha256": "d" * 64,
                "markerPath": f"/opt/eterra-alpha/shared/phase2-internal-transport/{operation}/open.json",
                "markerSha256": "e" * 64,
                "heartbeatPath": f"/opt/eterra-alpha/shared/phase2-internal-transport/{operation}/heartbeat.json",
                "heartbeatNonce": "f" * 64,
                "watchdogService": f"nexus-v2-phase2-internal-transport-{operation}.service",
                "watchdogTimer": f"nexus-v2-phase2-internal-transport-{operation}.timer",
                "watchdogUnitSha256": "1" * 64,
                "watchdogPayloadSha256": "3" * 64,
                "armed": True,
                "expiresAtUtc": (self.now + dt.timedelta(minutes=10)).strftime("%Y-%m-%dT%H:%M:%SZ"),
            },
            "phase2": {
                "publicIngressClosed": True,
                "siteIndexerSynchronized": True,
                "authorityReady": True,
                "fullLoopActivationReceiptSha256": "2" * 64,
            },
            "safety": {
                "chainStateMutationAuthorized": False,
                "paidOrPublicActivationAuthorized": False,
                "sourceRestricted": True,
                "loopbackBackendsPreserved": True,
                "forbiddenPortsClosed": True,
            },
            "capturedAtUtc": self.now.strftime("%Y-%m-%dT%H:%M:%SZ"),
        }

    def test_cli_exposes_only_the_seven_guarded_commands(self) -> None:
        parser = tool.build_parser()
        groups = [item for item in parser._actions if isinstance(item, argparse._SubParsersAction)]
        self.assertEqual(len(groups), 1)
        self.assertEqual(
            set(groups[0].choices),
            {"capture-plan", "validate", "execute", "renew", "verify", "capture-handoff", "verify-handoff", "close"},
        )

    def test_result_paths_watchdog_lease_and_idempotence_are_exact(self) -> None:
        tool.validate_result(self.result(), self.plan, "4" * 64, "verify")
        for mutation in (
            ("marker", "path", "/tmp/open.json"),
            ("heartbeat", "path", "/tmp/heartbeat.json"),
            ("watchdog", "service", "nexus-v2-phase2-internal-transport-other.service"),
        ):
            bad = json.loads(json.dumps(self.result()))
            bad[mutation[0]][mutation[1]] = mutation[2]
            with self.subTest(field=mutation[:2]), self.assertRaises(tool.TransportError):
                tool.validate_result(bad, self.plan, "4" * 64, "verify")
        stale = self.result()
        stale["heartbeat"]["expiresAtUtc"] = (self.now + dt.timedelta(minutes=4)).strftime("%Y-%m-%dT%H:%M:%SZ")
        with self.assertRaisesRegex(tool.TransportError, "five minutes"):
            tool.validate_result(stale, self.plan, "4" * 64, "verify")

    def test_handoff_rejects_artifact_substitution_and_path_prefix_confusion(self) -> None:
        replacement = {
            "releaseId": self.plan["releaseId"],
            "repositories": {
                "chain": {"head": self.plan["sourceCommit"]},
                "web": {"head": "8" * 40},
            },
            "artifacts": {"siteDeploymentCandidateManifest": {"path": "/candidate"}},
        }
        pins = {
            "replacement_pin": {"path": "/replacement", "sha256": "a" * 64},
            "acceptance_pin": {"path": "/acceptance", "sha256": "9" * 64},
            "phase1_pin": {"path": "/phase1", "sha256": "b" * 64},
            "activation_pin": {"path": "/activation", "sha256": "2" * 64},
            "identity_pin": {"path": "/identity", "sha256": "c" * 64},
            "chain_environment": {},
            "site_environment": {},
        }
        with mock.patch.object(tool, "validate_replacement", return_value=replacement), mock.patch.object(
            tool, "validate_acceptance"
        ), mock.patch.object(tool, "validate_site_phase2"), mock.patch.object(
            tool.release_lock,
            "validate_site_candidate",
            return_value={"releaseVersion": "v0.1.0-alpha.1"},
        ):
            tool.validate_handoff(self.handoff(), **pins)
            substituted = self.handoff()
            substituted["sitePostPhase2DeploymentIdentitySha256"] = "3" * 64
            with self.assertRaisesRegex(tool.TransportError, "artifact binding"):
                tool.validate_handoff(substituted, **pins)
            escaped = self.handoff()
            escaped["lease"]["markerPath"] += ".attacker"
            with self.assertRaisesRegex(tool.TransportError, "lease paths"):
                tool.validate_handoff(escaped, **pins)

    def test_mutating_commands_refuse_existing_output_before_remote_contact(self) -> None:
        output = self.root / "existing.json"
        output.write_text("occupied", encoding="utf-8")
        args = types.SimpleNamespace(
            command="execute",
            plan=str(self.root / "plan.json"),
            expected_plan_sha256="4" * 64,
            result=str(output),
        )
        with mock.patch.object(tool, "load_plan", return_value=self.plan), mock.patch.object(
            tool, "invoke_remote"
        ) as invoke, self.assertRaisesRegex(tool.TransportError, "overwrite"):
            tool.command_remote(args)
        invoke.assert_not_called()

    def test_json_pins_and_plans_are_parsed_from_the_single_hashed_read(self) -> None:
        pinned = self.root / "pinned.json"
        raw = canonical({"value": 1})
        pinned.write_bytes(raw)
        original_read = tool.read_stable_regular_file
        reads: list[str] = []

        def observed_read(path, label):
            reads.append(label)
            return original_read(path, label)

        with mock.patch.object(tool, "read_stable_regular_file", side_effect=observed_read):
            self.assertEqual(
                tool.file_pin(str(pinned), "pinned input", canonical_json=True)["sha256"],
                hashlib.sha256(raw).hexdigest(),
            )
        self.assertEqual(reads, ["pinned input"])

        reads.clear()
        with mock.patch.object(tool, "read_stable_regular_file", side_effect=observed_read), mock.patch.object(
            tool, "validate_plan", side_effect=lambda value, **_: value
        ):
            self.assertEqual(
                tool.load_plan(pinned, hashlib.sha256(raw).hexdigest()),
                {"value": 1},
            )
        self.assertEqual(reads, ["Phase-2 transport plan"])

    def test_inputs_and_outputs_reject_symlinked_ancestors(self) -> None:
        real = self.root / "real"
        real.mkdir()
        source = real / "source.json"
        source.write_bytes(canonical({"value": 1}))
        alias = self.root / "alias"
        alias.symlink_to(real, target_is_directory=True)

        with self.assertRaisesRegex(tool.TransportError, "symlink|canonical"):
            tool.file_pin(str(alias / "source.json"), "aliased input", canonical_json=True)
        with self.assertRaisesRegex(tool.TransportError, "unsafe"):
            tool.write_new(alias / "output.json", {"value": 2})
        self.assertFalse((real / "output.json").exists())

    def test_new_output_uses_nofollow_exclusive_creation_and_exact_mode(self) -> None:
        output = self.root / "new" / "nested" / "result.json"
        tool.write_new(output, {"value": 1})
        self.assertEqual(output.read_bytes(), canonical({"value": 1}))
        self.assertEqual(stat.S_IMODE(output.stat().st_mode), 0o400)
        with self.assertRaisesRegex(tool.TransportError, "overwrite"):
            tool.write_new(output, {"value": 2})

    def test_environment_parser_rejects_duplicates_and_control_characters(self) -> None:
        duplicate = self.root / "duplicate.env"
        duplicate.write_text("DEPLOY_USER=eterra2010\nDEPLOY_USER=other\n", encoding="utf-8")
        with self.assertRaisesRegex(tool.TransportError, "duplicate key"):
            tool.parse_environment(duplicate)
        controlled = self.root / "controlled.env"
        controlled.write_bytes(b"DEPLOY_USER=eterra2010\r\n")
        with self.assertRaisesRegex(tool.TransportError, "control characters"):
            tool.parse_environment(controlled)

    def test_remote_execution_uses_the_pinned_deployment_library_transport(self) -> None:
        chain_root = self.root / "chain"
        library = chain_root / "deploy/alpha/macmini2010/lib.sh"
        library.parent.mkdir(parents=True)
        library.write_text("# pinned test library\n", encoding="utf-8")
        replacement = self.root / "replacement.json"
        replacement.write_bytes(canonical({"repositories": {"chain": {"root": str(chain_root)}}}))
        helper = self.root / "helper"
        helper.write_text("#!/bin/sh\n", encoding="utf-8")
        identity = self.root / "identity"
        identity.write_text("test identity\n", encoding="utf-8")
        sudo_secret = self.root / "sudo-secret"
        sudo_secret.write_text("phase2-test-secret\n", encoding="utf-8")
        sudo_secret.chmod(0o600)
        chain_environment = self.root / "chain.env"
        chain_environment.write_text(
            f"SSH_IDENTITY_FILE={identity}\n"
            f"DEPLOY_PASSWORD=\n"
            f"REMOTE_SUDO_PASSWORD=@{sudo_secret}\n",
            encoding="utf-8",
        )
        plan_path = self.root / "plan.json"
        plan_path.write_bytes(b"{}\n")
        self.plan["replacementLock"] = {"path": str(replacement), "sha256": digest(replacement)}
        self.plan["remote"]["helper"] = {
            "path": str(helper),
            "sha256": digest(helper),
            "sourceCommit": self.plan["sourceCommit"],
        }
        self.plan["selectedDeploymentEnvironment"]["path"] = str(chain_environment)
        self.plan["sshHostPins"]["knownHosts"]["path"] = str(self.root / "known-hosts")
        self.plan["sshHostPins"]["manifest"]["path"] = str(self.root / "host-pins")
        observed: dict = {}
        payload = base64.b64encode(json.dumps(self.result()).encode()).decode()

        def fake_run(command, **kwargs):
            observed["command"] = command
            observed["input"] = kwargs["input"]
            observed["env"] = kwargs["env"]
            return types.SimpleNamespace(
                returncode=0,
                stdout=f"NEXUS_V2_PHASE2_TRANSPORT_RESULT:{payload}\n",
                stderr="",
            )

        with mock.patch.object(tool.subprocess, "run", side_effect=fake_run), mock.patch.dict(
            os.environ,
            {
                "NEXUS_V2_PHASE2_INTERNAL_TRANSPORT_CONFIRMATION":
                    "PRIVATE_ALPHA_PHASE2_INTERNAL_TRANSPORT",
                **{
                    name: f"ambient-{index}-must-not-escape"
                    for index, name in enumerate(
                        tool.deployment_secret_environment.DEPLOYMENT_SECRET_ENVIRONMENT_NAMES
                    )
                },
            },
            clear=False,
        ):
            tool.invoke_remote(plan_path, self.plan, "4" * 64, "verify")
        self.assertEqual(observed["command"], ["/bin/bash", "-s", "--"])
        self.assertIn(f"source {library}", observed["input"])
        self.assertIn("read_protected_sudo_secret_value", observed["input"])
        self.assertNotIn("phase2-test-secret", observed["input"])
        self.assertIn("remote_root_bash", observed["input"])
        self.assertNotIn("ProxyCommand", observed["input"])
        self.assertTrue(
            tool.deployment_secret_environment.DEPLOYMENT_SECRET_ENVIRONMENT_NAMES.isdisjoint(
                observed["env"]
            )
        )
        self.assertEqual(
            observed["env"]["NEXUS_V2_SSH_KNOWN_HOSTS_SHA256"],
            self.plan["sshHostPins"]["knownHosts"]["sha256"],
        )

    def test_remote_execution_rejects_literal_or_password_credentials_before_child(self) -> None:
        secret = self.root / "sudo-secret"
        secret.write_text("fixture-secret\n", encoding="utf-8")
        secret.chmod(0o600)
        valid = {"DEPLOY_PASSWORD": "", "REMOTE_SUDO_PASSWORD": f"@{secret}"}
        self.assertEqual(
            tool.validate_phase2_transport_credential_reference(valid),
            f"@{secret}",
        )
        for invalid, message in (
            ({**valid, "DEPLOY_PASSWORD": "literal"}, "DEPLOY_PASSWORD is forbidden"),
            ({**valid, "REMOTE_SUDO_PASSWORD": "literal"}, "file reference"),
            ({**valid, "REMOTE_SUDO_PASSWORD": ""}, "file reference"),
        ):
            with self.subTest(invalid=invalid), self.assertRaisesRegex(
                tool.TransportError, message
            ):
                tool.validate_phase2_transport_credential_reference(invalid)

    def test_host_helper_is_narrow_fail_closed_and_has_no_site_or_chain_writer(self) -> None:
        source = (
            HERE.parents[1]
            / "deploy/alpha/macmini2010/nexus-v2-phase2-internal-transport-host-action.sh"
        ).read_text(encoding="utf-8")
        self.assertIn("SERVICES=(authority chain-rpc ipfs-gateway media)", source)
        self.assertIn("FORBIDDEN_PORTS=(30333 5001)", source)
        self.assertIn("OnUnitActiveSec=30", source)
        self.assertIn("phase1PublicCaddyMustRemainUnchanged:true", source)
        self.assertNotIn("Caddyfile", source)
        self.assertNotIn("author_submit", source)
        self.assertNotIn("sudo_unchecked_weight", source)
        self.assertNotIn("state_call", source)

    def test_host_helper_pins_state_units_dropins_and_translation_paths(self) -> None:
        source = (
            HERE.parents[1]
            / "deploy/alpha/macmini2010/nexus-v2-phase2-internal-transport-host-action.sh"
        ).read_text(encoding="utf-8")
        for expected in (
            'require_root_owned_directory "${ancestor}"',
            'require_root_owned_directory "${STATE_BASE}" 700',
            'require_root_owned_directory "${STATE_ROOT}" 700',
            'set -C\n\texec 8>"${path}"',
            'existing Phase-2 operation root lacks the exact open marker',
            'cmp -s "${expected_socket}" "${socket_path}"',
            'cmp -s "${expected_service}" "${service_path}"',
            '"$(stat -c \'%U:%G:%a\' "${socket_path}")" == root:root:644',
            'FragmentPath --value',
            'DropInPaths --value',
            'require_no_protected_port_translation',
            'nft -j list ruleset',
            'iptables-save >"${legacy4}"',
            'ip6tables-save >"${legacy6}"',
        ):
            self.assertIn(expected, source)
        self.assertNotIn('awk -F= \'$1 == "ListenStream"', source)

        bootstrap = (
            HERE.parents[1] / "deploy/alpha/macmini2010/bootstrap.sh"
        ).read_text(encoding="utf-8")
        self.assertIn(
            'chown root:root "${DEPLOY_ROOT}" "${DEPLOY_ROOT}/shared"', bootstrap
        )

    def test_protected_root_execution_hashes_and_executes_one_root_owned_fd(self) -> None:
        library = (
            HERE.parents[1] / "deploy/alpha/macmini2010/lib.sh"
        ).read_text(encoding="utf-8")
        protected = library.split("protected_remote_root_stream() {", 1)[1].split(
            "\n}\n\nssh_to_remote()", 1
        )[0]
        self.assertIn("mktemp -d /run/nexus-v2-root-exec.XXXXXX", protected)
        self.assertIn('chmod 0700 "${stage}"', protected)
        self.assertIn("python3 -I -S -c", protected)
        self.assertIn("os.O_EXCL", protected)
        self.assertIn("O_NOFOLLOW", protected)
        self.assertIn("os.fchmod(handle.fileno(), 0o500)", protected)
        self.assertIn('exec 9<"${payload}"', protected)
        self.assertIn("sha256sum /proc/self/fd/9", protected)
        self.assertIn("/bin/bash /proc/self/fd/9", protected)
        self.assertIn("/usr/bin/sudo -S -k -p '' -- /bin/bash -c", protected)
        self.assertIn("cat \"${script_path}\"", protected)
        self.assertIn('} | "${SSH_CMD[@]}" "${remote_command}"', protected)
        self.assertNotIn("expect -f", protected)
        self.assertNotIn("base64", protected)
        self.assertNotIn("SUDO_ASKPASS", protected)
        self.assertNotIn("sudo -n bash -s", protected)


if __name__ == "__main__":
    unittest.main()
