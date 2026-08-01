from __future__ import annotations

import ast
import os
import json
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


HERE = Path(__file__).resolve().parent
DEPLOYMENT_SECRET_NAMES = (
    "DEPLOY_PASSWORD",
    "REMOTE_SUDO_PASSWORD",
    "AURA_SURI",
    "GRAN_SURI",
    "MEDIA_SIGNER_SEED",
    "MEDIA_ADMIN_API_KEY",
    "AUTHORITY_RELAY_MNEMONIC",
    "AUTHORITY_RELAY_DERIVATION_PASSWORD",
    "ETERRA_LEGENDS_SIGNER_MNEMONIC",
    "ETERRA_LEGENDS_SIGNER_DERIVATION_PASSWORD",
    "ETERRA_LEGENDS_PRIVATE_ALPHA_ACCESS_KEY",
    "ETERRA_ALPHA_SUDO_MNEMONIC",
    "ETERRA_ALPHA_SUDO_DERIVATION_PASSWORD",
    "ADMIN_SESSION_SECRET",
    "ALPHA_ACCESS_SESSION_SECRET",
    "DISCORD_CLIENT_SECRET",
    "DISCORD_BOT_TOKEN",
    "MONGODB_URI",
    "ETERRA_LEGENDS_PLAYER_ACCESS_TOKEN",
    "NEXUS_V2_PRIVATE_ALPHA_ACCESS_KEY",
    "NEXUS_V2_SESSION_AUTHORIZATION_PROFILES_JSON",
    "ADMIN_API_KEY",
    "ETERRA_FPS_V2_OWNER_SECRET_PATH",
    "ETERRA_FPS_V2_PLAYER_GATEWAY_ACCESS_TOKEN",
    "ETERRA_FPS_V2_ROOT_SECRET_PATH",
    "ETERRA_FPS_V2_SUDO_SECRET_PATH",
)


class Phase1ClosedDeployTests(unittest.TestCase):
    def run_launcher(self, closed: bool) -> list[str]:
        with tempfile.TemporaryDirectory(prefix="phase1-launcher-") as temporary:
            root = Path(temporary)
            node = root / "node"
            node.write_text("#!/bin/sh\nprintf '%s\\n' \"$@\"\n", encoding="utf-8")
            node.chmod(0o755)
            spec = root / "alpha-raw.json"
            spec.write_text("{}\n", encoding="utf-8")
            base = root / "state"
            (base / "network").mkdir(parents=True)
            (base / "network" / "secret_ed25519").write_text("key\n", encoding="utf-8")
            (base / ".alpha-keys-inserted").write_text("ok\n", encoding="utf-8")
            environment = os.environ.copy()
            environment.update(
                {
                    "NODE_BIN": str(node),
                    "RAW_SPEC": str(spec),
                    "BASE_PATH": str(base),
                    "AURA_SURI": "test-aura",
                    "GRAN_SURI": "test-grandpa",
                    "MINI_LAN_IP": "10.81.13.19",
                    "NEXUS_V2_PHASE1_CLOSED": "1" if closed else "0",
                    "RPC_BIND_HOST": "127.0.0.1" if closed else "0.0.0.0",
                }
            )
            completed = subprocess.run(
                [str(HERE / "start-alpha-node.sh")],
                env=environment,
                capture_output=True,
                text=True,
                check=True,
            )
            return completed.stdout.splitlines()

    def test_closed_launcher_omits_every_external_rpc_or_p2p_advertisement(self) -> None:
        output = self.run_launcher(True)
        self.assertNotIn("--unsafe-rpc-external", output)
        self.assertNotIn("--rpc-external", output)
        self.assertNotIn("--public-addr", output)
        self.assertIn("--listen-addr", output)
        self.assertIn("/ip4/127.0.0.1/tcp/30333", output)
        self.assertTrue(any("phase1_closed=true rpc_bind=127.0.0.1" in line for line in output))

    def test_normal_launcher_preserves_existing_external_behavior(self) -> None:
        output = self.run_launcher(False)
        self.assertIn("--unsafe-rpc-external", output)
        self.assertIn("--public-addr", output)
        self.assertIn("/ip4/10.81.13.19/tcp/30333", output)

    def test_closed_launcher_rejects_non_loopback_contract(self) -> None:
        with tempfile.TemporaryDirectory(prefix="phase1-launcher-reject-") as temporary:
            root = Path(temporary)
            node = root / "node"
            node.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            node.chmod(0o755)
            spec = root / "spec.json"
            spec.write_text("{}\n", encoding="utf-8")
            environment = os.environ.copy()
            environment.update(
                {
                    "NODE_BIN": str(node),
                    "RAW_SPEC": str(spec),
                    "BASE_PATH": str(root / "state"),
                    "AURA_SURI": "test-aura",
                    "GRAN_SURI": "test-grandpa",
                    "NEXUS_V2_PHASE1_CLOSED": "1",
                    "RPC_BIND_HOST": "0.0.0.0",
                }
            )
            completed = subprocess.run(
                [str(HERE / "start-alpha-node.sh")],
                env=environment,
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("requires RPC_BIND_HOST=127.0.0.1", completed.stderr)

    def test_node_and_authority_preclose_before_restart(self) -> None:
        node = (HERE / "deploy-node.sh").read_text(encoding="utf-8")
        authority = (HERE / "deploy-arcade-authority.sh").read_text(encoding="utf-8")
        node_remote = node.index('"${remote_tmp_dir}/nexus-v2-phase1-closed-ingress.sh" preclose')
        node_restart = node.index('systemctl restart "${REMOTE_NODE_SERVICE_NAME}.service"')
        candidate_start = authority.index('if [[ -n "${promotion_manifest}" ]]; then', authority.index("# In Phase-1"))
        authority_remote = authority.index('"\\${guard}" preclose', candidate_start)
        authority_restart = authority.index('systemctl restart "${AUTHORITY_SERVICE_NAME}.service"', candidate_start)
        self.assertLess(node_remote, node_restart)
        self.assertLess(authority_remote, authority_restart)

    def test_authority_release_dry_run_is_candidate_only_and_local(self) -> None:
        authority = (HERE / "deploy-arcade-authority.sh").read_text(encoding="utf-8")
        dry_run = authority.index('if [[ "${dry_run}" -eq 1 ]]; then')
        dry_exit = authority.index("\texit 0", dry_run)
        first_remote = authority.index("remote_root_bash", dry_exit)
        dotnet = authority.index('DOTNET_BIN="${DOTNET_BIN:-/opt/homebrew/bin/dotnet}"')
        candidate_verify = authority.index('python3 "${AUTHORITY_CANDIDATE_TOOL}" verify')
        self.assertLess(candidate_verify, dry_run)
        self.assertLess(dry_exit, first_remote)
        self.assertLess(dry_exit, dotnet)
        self.assertNotIn("dotnet publish", authority[:dry_exit].lower())
        self.assertNotIn("read_secret_value", authority[:dry_exit])
        self.assertIn('AUTHORITY_SUBMITTER_MODE}" == "in_memory"', authority[:dry_exit])
        for fixed_contract in (
            'DEPLOY_ROOT}" == "/opt/eterra-alpha"',
            'DEPLOY_USER}" == "eterra2010"',
            'AUTHORITY_SERVICE_NAME}" == "eterra-arcade-authority"',
            'AUTHORITY_PORT}" == "8787"',
        ):
            self.assertIn(fixed_contract, authority[:dry_exit])

    def test_authority_candidate_promotion_rehashes_with_deployed_operator(self) -> None:
        authority = (HERE / "deploy-arcade-authority.sh").read_text(encoding="utf-8")
        candidate_start = authority.index('if [[ -n "${promotion_manifest}" ]]; then', authority.index("# In Phase-1"))
        candidate_end = authority.index("\texit 0", candidate_start)
        candidate = authority[candidate_start:candidate_end]
        self.assertNotIn('"${DOTNET_BIN}" publish', candidate)
        self.assertIn('verify-release-manifest', candidate)
        self.assertIn('/proc/\\${pid}/exe', candidate)
        self.assertIn('ETERRA_LEGENDS_RESULT_JOURNAL_PATH', (HERE / "lib.sh").read_text(encoding="utf-8"))
        self.assertIn('chown -R root:root "\\${candidate_stage}"', candidate)
        self.assertIn('mv -T "\\${candidate_stage}" "\\${release_root}"', candidate)
        self.assertIn('install -d -m 0750 -o root -g "${DEPLOY_USER}" "${REMOTE_SHARED_SECRET_DIR}"', candidate)
        self.assertIn('seal_service_file()', candidate)
        self.assertIn('"${legends_mnemonic_sha256}" 640 "${DEPLOY_USER}"', candidate)
        self.assertIn('"${authority_env_sha256}" 640 "${DEPLOY_USER}"', candidate)
        self.assertIn('test ! -L "${REMOTE_LEGENDS_SIGNER_DERIVATION_PASSWORD_FILE}"', candidate)
        self.assertIn('test ! -e "\\${retired_derivation_password}"', candidate)
        self.assertIn('test ! -L "\\${retired_derivation_password}"', candidate)
        self.assertIn('mv -- "${REMOTE_LEGENDS_SIGNER_DERIVATION_PASSWORD_FILE}"', candidate)
        self.assertNotIn("--authorize", candidate)
        self.assertNotIn("--seed-config", candidate)

    def test_release_env_and_authority_observation_are_privilege_sealed(self) -> None:
        media = (HERE / "deploy-media.sh").read_text(encoding="utf-8")
        authority = (HERE / "deploy-arcade-authority.sh").read_text(encoding="utf-8")
        library = (HERE / "lib.sh").read_text(encoding="utf-8")
        self.assertIn(
            'install -m 0600 -o root -g root "${remote_tmp_dir}/media.env" "${REMOTE_MEDIA_ENV_FILE}"',
            media,
        )
        self.assertIn('"600 root:root"', media)
        self.assertIn('live media environment drifted', media)
        self.assertIn('mediaEnvironment', media)
        self.assertIn('remote_observation_root="/run/nexus-v2-authority-observation-', authority)
        self.assertIn('NEXUS_V2_AUTHORITY_OBSERVATION:', authority)
        self.assertIn('"400 root:root"', authority)
        self.assertNotIn(
            'chown "${DEPLOY_USER}:${DEPLOY_USER}" "${remote_observation}"', authority
        )
        self.assertNotIn('rsync_from_remote_no_delete "${remote_observation}"', authority)
        self.assertIn("write_exact_env_file()", library)
        self.assertIn("unsafe metacharacters", library)

    def test_exact_env_renderer_is_owner_only_and_rejects_injection(self) -> None:
        with tempfile.TemporaryDirectory(prefix="exact-env-") as temporary:
            root = Path(temporary).resolve()
            output = root / "good.env"
            completed = subprocess.run(
                ["bash", "-c", 'source "$1"; write_exact_env_file "$2" "ALPHA=one" "BETA=two words"', "bash", str(HERE / "lib.sh"), str(output)],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertEqual(output.read_text(encoding="utf-8"), "ALPHA=one\nBETA=two words\n")
            self.assertEqual(output.stat().st_mode & 0o777, 0o600)

            bad = root / "bad.env"
            completed = subprocess.run(
                ["bash", "-c", 'source "$1"; write_exact_env_file "$2" "$3"', "bash", str(HERE / "lib.sh"), str(bad), "ALPHA=ok\nINJECTED=yes"],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("line break", completed.stderr)

    def test_phase1_orchestrator_forbids_write_bootstrap(self) -> None:
        deploy_all = (HERE / "deploy-all.sh").read_text(encoding="utf-8")
        self.assertIn("--phase1-closed forbids authority authorization and config seeding", deploy_all)
        self.assertIn('authority_args+=("--phase1-closed")', deploy_all)
        self.assertIn('media_args+=("--phase1-closed")', deploy_all)
        self.assertIn('authority_args+=("--dry-run")', deploy_all)

    def test_media_candidate_build_exits_before_any_node_or_authority_deploy(self) -> None:
        deploy_all = (HERE / "deploy-all.sh").read_text(encoding="utf-8")
        candidate_dispatch = deploy_all.index(
            '"${SCRIPT_DIR}/deploy-media.sh" "${media_args[@]}"',
            deploy_all.index('if [[ "${build_media_candidate}" -eq 1 ]]'),
        )
        candidate_exit = deploy_all.index("\texit 0", candidate_dispatch)
        node_dispatch = deploy_all.rindex('"${SCRIPT_DIR}/deploy-node.sh"')
        authority_dispatch = deploy_all.rindex('"${SCRIPT_DIR}/deploy-arcade-authority.sh"')
        self.assertLess(candidate_dispatch, candidate_exit)
        self.assertLess(candidate_exit, node_dispatch)
        self.assertLess(candidate_exit, authority_dispatch)
        self.assertIn(
            "--build-media-candidate must be the only candidate or deployment action",
            deploy_all,
        )

    def test_media_candidate_and_phase1_validation_stay_off_public_ingress(self) -> None:
        media = (HERE / "deploy-media.sh").read_text(encoding="utf-8")
        isolated = media.index('if [[ -n "$candidate_output" ]]; then', media.index("if $dry_run; then"))
        active_root = media.index('mkdir -p "${REMOTE_MEDIA_DIR}"', isolated)
        self.assertIn('"${MEDIA_REPO_DIR}/" "${SSH_TARGET}:${candidate_source}/"', media[isolated:active_root])
        self.assertNotIn('"${SSH_TARGET}:${REMOTE_MEDIA_DIR}/"', media[isolated:active_root])
        self.assertIn('validation_transport="ssh_loopback"', media)
        self.assertIn('health_url="http://127.0.0.1:${MEDIA_PORT}/health/ready"', media)
        self.assertIn('"phase1Closed": phase1_closed == "true"', media)

    def test_phase1_media_uses_sealed_overlay_and_resolved_config_gate(self) -> None:
        library = (HERE / "lib.sh").read_text(encoding="utf-8")
        media = (HERE / "deploy-media.sh").read_text(encoding="utf-8")
        guard = (HERE / "nexus-v2-phase1-closed-ingress.sh").read_text(encoding="utf-8")

        self.assertIn('REMOTE_MEDIA_COMPOSE_PHASE1="${REMOTE_MEDIA_DIR}/docker-compose.phase1-closed.yaml"', library)
        self.assertIn("docker-compose.phase1-closed.yaml)", library)
        self.assertIn('REMOTE_DOCKER_COMPOSE_CMD="${REMOTE_DOCKER_COMPOSE_PHASE1_CMD}"', media)
        self.assertIn("Docker Compose >= 2.24.4", media)
        self.assertIn('config --format json | python3 -c', media)
        self.assertIn('media.get("network_mode") != "host"', media)
        self.assertIn('Phase1 resolved IPFS port bindings mismatch', media)
        self.assertIn('MEDIA_BIND_HOST=127.0.0.1', media)
        self.assertIn('NEXUS_V2_PHASE1_CLOSED=1', media)
        for port_name in (
            "CHAIN_RPC_PORT",
            "CHAIN_P2P_PORT",
            "AUTHORITY_PORT",
            "MEDIA_PORT",
            "IPFS_API_PORT",
            "IPFS_GATEWAY_PORT",
        ):
            self.assertIn(f'"${{{port_name}}}"', guard)
        self.assertIn('verify_loopback_listener "${CHAIN_P2P_PORT}" "chain P2P"', guard)

    def test_deploy_transport_never_trusts_a_host_on_first_contact(self) -> None:
        library = (HERE / "lib.sh").read_text(encoding="utf-8")
        self.assertNotIn("StrictHostKeyChecking=accept-new", library)
        self.assertNotIn("StrictHostKeyChecking=no", library)
        self.assertIn("protected Nexus V2 transport rejects SSH_OPTS", library)
        self.assertIn("capture_ssh_host_pins.py", library)
        self.assertIn('SSH_CMD=("${rsync_ssh_command[@]}" "${SSH_TARGET}")', library)
        self.assertIn('build_nexus_v2_rsync_rsh "${rsync_ssh_command[@]}"', library)
        self.assertNotIn("printf -v RSYNC_RSH '%q '", library)
        for token in (
            "-F /dev/null",
            "Hostname=${DEPLOY_HOST}",
            "HostKeyAlias=${DEPLOY_HOST}",
            "UserKnownHostsFile=${NEXUS_V2_SSH_KNOWN_HOSTS_FILE}",
            "GlobalKnownHostsFile=/dev/null",
            "StrictHostKeyChecking=yes",
            "UpdateHostKeys=no",
            "KnownHostsCommand=none",
            "VerifyHostKeyDNS=no",
            "CheckHostIP=yes",
            "CanonicalizeHostname=no",
            "ProxyCommand=none",
            "ProxyJump=none",
            "HostKeyAlgorithms=",
            "PubkeyAcceptedAlgorithms=",
            "KexAlgorithms=",
            "Ciphers=",
            "MACs=",
            "IdentitiesOnly=yes",
            "IdentityAgent=none",
            "BatchMode=yes",
            "PasswordAuthentication=no",
            "KbdInteractiveAuthentication=no",
            "PreferredAuthentications=publickey",
            "NumberOfPasswordPrompts=0",
            "RequestTTY=no",
        ):
            self.assertIn(token, library)
        self.assertNotIn("expect -f", library)
        self.assertNotIn("SUDO_ASKPASS", library)
        self.assertNotIn("sudo -A", library)
        self.assertNotIn('send -- "yes\\r"', library)

    def test_protected_rsync_transport_preserves_exact_algorithm_tokens(self) -> None:
        rsync = shutil.which("rsync")
        self.assertIsNotNone(rsync, "native rsync is required for deployment transport coverage")
        host_key_algorithms = (
            "ssh-ed25519,ecdsa-sha2-nistp256,rsa-sha2-512,rsa-sha2-256"
        )
        kex_algorithms = (
            "curve25519-sha256,curve25519-sha256@libssh.org,"
            "diffie-hellman-group16-sha512,diffie-hellman-group14-sha256"
        )
        ciphers = (
            "chacha20-poly1305@openssh.com,aes256-gcm@openssh.com,"
            "aes128-gcm@openssh.com,aes256-ctr,aes192-ctr,aes128-ctr"
        )
        macs = (
            "hmac-sha2-512-etm@openssh.com,hmac-sha2-256-etm@openssh.com,"
            "hmac-sha2-512,hmac-sha2-256"
        )
        with tempfile.TemporaryDirectory(prefix="nexus-v2-rsync-rsh-") as temporary:
            root = Path(temporary)
            identity = root / "identity"
            identity.write_text("fixture\n", encoding="utf-8")
            identity.chmod(0o600)
            known_hosts = root / "known_hosts"
            known_hosts.write_text("fixture\n", encoding="utf-8")
            manifest = root / "manifest.json"
            manifest.write_text("{}\n", encoding="utf-8")
            bash = subprocess.run(
                [
                    "/bin/bash",
                    "-c",
                    r'''
source "$1"
DEPLOY_HOST=192.0.2.10
DEPLOY_USER=eterra2010
SSH_PORT=22
SSH_TARGET=eterra2010@192.0.2.10
SSH_IDENTITY_FILE="$2"
NEXUS_V2_SSH_KNOWN_HOSTS_FILE="$3"
NEXUS_V2_SSH_HOST_PIN_MANIFEST="$4"
build_nexus_v2_pinned_ssh_transport
printf '%s' "${RSYNC_RSH}"
''',
                    "nexus-v2-rsync-rsh",
                    str(HERE / "lib.sh"),
                    str(identity),
                    str(known_hosts),
                    str(manifest),
                ],
                capture_output=True,
                text=True,
                check=True,
            )
            serialized = bash.stdout
            self.assertNotIn("\\,", serialized)
            expected_options = (
                f"HostKeyAlgorithms={host_key_algorithms}",
                f"PubkeyAcceptedAlgorithms={host_key_algorithms}",
                f"KexAlgorithms={kex_algorithms}",
                f"Ciphers={ciphers}",
                f"MACs={macs}",
            )
            for option in expected_options:
                self.assertIn(f"-o {option}", serialized)

            fake_bin = root / "bin"
            fake_bin.mkdir()
            capture = root / "ssh-argv.bin"
            fake_ssh = fake_bin / "ssh"
            fake_ssh.write_text(
                "#!/bin/bash\nprintf '%s\\0' \"$@\" >\"$NEXUS_TEST_SSH_ARGV\"\nexit 1\n",
                encoding="utf-8",
            )
            fake_ssh.chmod(0o755)
            environment = os.environ.copy()
            environment["PATH"] = f"{fake_bin}:{environment['PATH']}"
            environment["NEXUS_TEST_SSH_ARGV"] = str(capture)
            completed = subprocess.run(
                [
                    str(rsync),
                    "--list-only",
                    "-e",
                    serialized,
                    "nexus-fixture:/",
                    str(root / "destination"),
                ],
                env=environment,
                capture_output=True,
                check=False,
            )
            self.assertNotEqual(completed.returncode, 0)
            self.assertTrue(capture.is_file(), "rsync did not invoke the native remote shell")
            argv = capture.read_bytes().split(b"\0")
            if argv[-1:] == [b""]:
                argv.pop()
            decoded = [item.decode("utf-8") for item in argv]
            for option in expected_options:
                index = decoded.index(option)
                self.assertEqual(decoded[index - 1], "-o")
            self.assertFalse(any("\\," in item for item in decoded))
            self.assertIn("nexus-fixture", decoded)

    def test_all_live_deploy_scripts_have_no_expect_askpass_or_pty_transport(self) -> None:
        audited: list[Path] = []
        for path in HERE.iterdir():
            if not path.is_file() or path.name.startswith("test_") or path.suffix == ".md":
                continue
            source = path.read_text(encoding="utf-8", errors="ignore")
            if not source.startswith("#!"):
                continue
            audited.append(path)
            with self.subTest(path=path.name):
                self.assertIsNone(re.search(r"\bexpect\b|\bsshpass\b", source))
                self.assertNotIn("SUDO_ASKPASS", source)
                self.assertNotIn("sudo -A", source)
                self.assertNotIn("RequestTTY=yes", source)
                self.assertNotIn("NumberOfPasswordPrompts=1", source)
                self.assertIsNone(re.search(r"(?:^|\s)ssh(?:\s+[^\n]*)?\s-[tT](?:\s|$)", source))
        self.assertGreaterEqual(len(audited), 20)

    def test_all_deployment_secrets_are_absent_from_early_children_and_ssh(self) -> None:
        secret_names = DEPLOYMENT_SECRET_NAMES
        with tempfile.TemporaryDirectory(prefix="deployment-secret-environment-") as temporary:
            root = Path(temporary).resolve()
            def fixture(path: Path, payload: str, mode: int) -> Path:
                path.write_text(payload, encoding="utf-8")
                path.chmod(mode)
                return path

            fake_bin = root / "bin"
            fake_bin.mkdir()
            identity = fixture(root / "identity", "private-key-fixture\n", 0o600)
            public_identity = fixture(root / "identity.pub", "public-key-fixture\n", 0o600)
            overrides = fixture(root / "alpha-overrides.json", "{}\n", 0o600)
            media_root = root / "media"
            media_root.mkdir()
            loaded_values = {
                name: f"LOADED_{index:02d}_{name}_sentinel"
                for index, name in enumerate(secret_names, start=1)
            }
            loaded_values["DEPLOY_PASSWORD"] = ""
            loaded_values["REMOTE_SUDO_PASSWORD"] = ""
            ambient_values = {
                name: f"AMBIENT_{index:02d}_{name}_sentinel"
                for index, name in enumerate(secret_names, start=1)
            }
            env_file = root / "deploy.env"
            public_values = {
                "DEPLOY_HOST": "192.0.2.10",
                "DEPLOY_USER": "eterra2010",
                "SSH_IDENTITY_FILE": str(identity),
                "SSH_PUBLIC_KEY_FILE": str(public_identity),
                "MINI_LAN_IP": "192.0.2.10",
                "SITE_PROXY_LAN_IP": "192.0.2.11",
                "LAN_CIDR": "192.0.2.0/24",
                "SITE_PUBLIC_ORIGIN": "https://alpha.invalid",
                "ALPHA_OVERRIDES_FILE": str(overrides),
                "MEDIA_REPO_DIR": str(media_root),
                "ETERRA_RELEASE_VERSION": "dev",
                "NEXUS_V2_LOCAL_ONLY_RELEASE": "0",
                "NEXUS_V2_PHASE1_CLOSED": "0",
            }
            env_file.write_text(
                "".join(
                    f"{name}={value}\n"
                    for name, value in {**public_values, **loaded_values}.items()
                ),
                encoding="utf-8",
            )
            secret_manifest = root / "secret-values.json"
            secret_manifest.write_text(
                json.dumps(
                    {
                        "names": list(secret_names),
                        "values": [
                            *ambient_values.values(),
                            *(value for value in loaded_values.values() if value),
                        ],
                    }
                ),
                encoding="utf-8",
            )
            early_capture = root / "early-child.json"
            child_capture = root / "child.json"
            ssh_capture = root / "ssh.json"
            fake_dirname = fake_bin / "dirname"
            fake_dirname.write_text(
                """#!/usr/bin/env python3
import json
import os
import pathlib
import sys

contract = json.loads(pathlib.Path(os.environ["NEXUS_TEST_SECRET_MANIFEST"]).read_text())
environment = dict(os.environ)
record = {
    "namesPresent": sorted(name for name in contract["names"] if name in environment),
    "valuesPresent": sorted(value for value in contract["values"] if any(value in item for item in environment.values())),
}
pathlib.Path(os.environ["NEXUS_TEST_EARLY_CAPTURE"]).write_text(json.dumps(record, sort_keys=True))
os.execv("/usr/bin/dirname", ["dirname", *sys.argv[1:]])
""",
                encoding="utf-8",
            )
            fake_dirname.chmod(0o700)
            fake_probe = fake_bin / "environment-probe"
            fake_probe.write_text(
                """#!/usr/bin/env python3
import json
import os
import pathlib
import sys

capture, manifest, label = map(pathlib.Path, sys.argv[1:4])
contract = json.loads(manifest.read_text())
environment = dict(os.environ)
record = {
    "label": str(label),
    "namesPresent": sorted(name for name in contract["names"] if name in environment),
    "valuesPresent": sorted(value for value in contract["values"] if any(value in item for item in environment.values())),
    "valuesInArgv": sorted(value for value in contract["values"] if any(value in item for item in sys.argv)),
}
capture.write_text(json.dumps(record, sort_keys=True))
""",
                encoding="utf-8",
            )
            fake_probe.chmod(0o700)
            environment = os.environ.copy()
            environment.update(ambient_values)
            environment.update(
                {
                    "PATH": f"{fake_bin}:{environment['PATH']}",
                    "NEXUS_TEST_EARLY_CAPTURE": str(early_capture),
                    "NEXUS_TEST_SECRET_MANIFEST": str(secret_manifest),
                }
            )
            environment.pop("NEXUS_V2_POST_ACCEPTANCE_REOPEN_BACKEND", None)
            completed = subprocess.run(
                [
                    "bash",
                    "-c",
                    'source "$1"; export ALPHA_MACMINI2010_ENV_FILE="$2"; load_env; '
                    '"$3" "$4" "$5" child; '
                    'SSH_CMD=("$3" "$6" "$5" ssh); ssh_to_remote harmless-command',
                    "bash",
                    str(HERE / "lib.sh"),
                    str(env_file),
                    str(fake_probe),
                    str(child_capture),
                    str(secret_manifest),
                    str(ssh_capture),
                ],
                env=environment,
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            for capture in (early_capture, child_capture, ssh_capture):
                record = json.loads(capture.read_text(encoding="utf-8"))
                self.assertEqual(record["namesPresent"], [], capture.name)
                self.assertEqual(record["valuesPresent"], [], capture.name)
                if "valuesInArgv" in record:
                    self.assertEqual(record["valuesInArgv"], [], capture.name)
            library = (HERE / "lib.sh").read_text(encoding="utf-8")
            for name in secret_names:
                self.assertIn(f"\t{name}\n", library)
            self.assertLess(
                library.index('export -n "${NEXUS_V2_DEPLOYMENT_SECRET_VARIABLES[@]}"'),
                library.index('DEPLOY_LIB_DIR="$(cd'),
            )

    def test_actual_shell_entrypoints_scrub_before_hostile_path_children(self) -> None:
        safety_tool_dir = HERE.parents[2] / "scripts/nexus-v2-private-alpha"
        sys.path.insert(0, str(safety_tool_dir))
        try:
            from deployment_secret_environment import (  # noqa: PLC0415
                DEPLOYMENT_SECRET_ENVIRONMENT_NAMES,
            )
        finally:
            sys.path.pop(0)
        self.assertEqual(
            set(DEPLOYMENT_SECRET_NAMES),
            set(DEPLOYMENT_SECRET_ENVIRONMENT_NAMES),
            "the shell and coordinator closed secret-name contracts differ",
        )
        for driver_name in (
            "nexus-v2-rollback-component-driver",
            "nexus-v2-pre-reset-chain-media-component-driver",
        ):
            tree = ast.parse((HERE / driver_name).read_text(encoding="utf-8"))
            assignments = [
                node
                for node in tree.body
                if isinstance(node, ast.Assign)
                and any(
                    isinstance(target, ast.Name)
                    and target.id == "DEPLOYMENT_SECRET_ENVIRONMENT_NAMES"
                    for target in node.targets
                )
            ]
            self.assertEqual(len(assignments), 1, driver_name)
            value = assignments[0].value
            self.assertIsInstance(value, ast.Call, driver_name)
            assert isinstance(value, ast.Call)
            self.assertEqual(len(value.args), 1, driver_name)
            self.assertEqual(
                set(ast.literal_eval(value.args[0])),
                set(DEPLOYMENT_SECRET_NAMES),
                f"{driver_name} closed secret-name contract differs",
            )
        shell_entrypoints = sorted(
            path
            for path in HERE.iterdir()
            if path.is_file() and path.read_bytes().startswith(b"#!/bin/bash\n")
        )
        self.assertGreaterEqual(len(shell_entrypoints), 20)
        for entrypoint in shell_entrypoints:
            source = entrypoint.read_text(encoding="utf-8")
            with self.subTest(entrypoint=entrypoint.name, contract="static"):
                self.assertEqual(source.splitlines()[0], "#!/bin/bash")
                scrub_end = source.index("2>/dev/null || true")
                match = re.search(
                    r"export -n (?P<names>.*?) 2>/dev/null \|\| true",
                    source[: scrub_end + len("2>/dev/null || true")],
                    flags=re.DOTALL,
                )
                self.assertIsNotNone(match)
                assert match is not None
                scrub_names = set(match.group("names").replace("\\", "").split())
                self.assertEqual(scrub_names, set(DEPLOYMENT_SECRET_NAMES))
                first_child = source.find("$(")
                if first_child >= 0:
                    self.assertLess(scrub_end, first_child)

        dirname_entrypoints = [
            path
            for path in shell_entrypoints
            if "dirname" in "\n".join(path.read_text(encoding="utf-8").splitlines()[:40])
        ]
        self.assertGreaterEqual(len(dirname_entrypoints), 15)
        with tempfile.TemporaryDirectory(prefix="actual-entrypoint-secret-boundary-") as temporary:
            root = Path(temporary).resolve()
            fake_bin = root / "bin"
            fake_bin.mkdir()
            capture = root / "dirname-captures.jsonl"
            mediation_capture = root / "interpreter-mediation.txt"
            secret_values = {
                name: f"ACTUAL_WRAPPER_{index:02d}_{name}_SENTINEL"
                for index, name in enumerate(DEPLOYMENT_SECRET_NAMES, start=1)
            }
            fake_dirname = fake_bin / "dirname"
            fake_dirname.write_text(
                """#!/usr/bin/python3
import json
import os
import pathlib

names = os.environ["NEXUS_TEST_SECRET_NAMES"].split(",")
sentinels = os.environ["NEXUS_TEST_SECRET_SENTINELS"].split(",")
environment = dict(os.environ)
visible_environment = {
    key: value for key, value in environment.items()
    if not key.startswith("NEXUS_TEST_")
}
record = {
    "entrypoint": environment["NEXUS_TEST_ENTRYPOINT"],
    "namesPresent": sorted(name for name in names if name in environment),
    "valuesPresent": sorted(
        sentinel
        for sentinel in sentinels
        if any(sentinel in value for value in visible_environment.values())
    ),
}
with pathlib.Path(environment["NEXUS_TEST_DIRNAME_CAPTURE"]).open("a", encoding="utf-8") as handle:
    handle.write(json.dumps(record, sort_keys=True) + "\\n")
print("/definitely/missing/nexus-v2-secret-boundary")
""",
                encoding="utf-8",
            )
            fake_dirname.chmod(0o700)
            for command in ("bash", "env"):
                probe = fake_bin / command
                probe.write_text(
                    "#!/bin/sh\n"
                    'printf "%s\\n" "$0" >>"$NEXUS_TEST_INTERPRETER_CAPTURE"\n'
                    "exit 97\n",
                    encoding="utf-8",
                )
                probe.chmod(0o700)

            base_environment = os.environ.copy()
            base_environment.update(secret_values)
            base_environment.update(
                {
                    "PATH": f"{fake_bin}:/usr/bin:/bin",
                    "NEXUS_TEST_SECRET_NAMES": ",".join(DEPLOYMENT_SECRET_NAMES),
                    "NEXUS_TEST_SECRET_SENTINELS": ",".join(secret_values.values()),
                    "NEXUS_TEST_DIRNAME_CAPTURE": str(capture),
                    "NEXUS_TEST_INTERPRETER_CAPTURE": str(mediation_capture),
                }
            )
            for entrypoint in dirname_entrypoints:
                environment = dict(base_environment)
                environment["NEXUS_TEST_ENTRYPOINT"] = entrypoint.name
                completed = subprocess.run(
                    [str(entrypoint), "--help"],
                    env=environment,
                    capture_output=True,
                    text=True,
                    check=False,
                )
                combined = completed.stdout + completed.stderr
                for sentinel in secret_values.values():
                    self.assertNotIn(sentinel, combined, entrypoint.name)

            self.assertFalse(
                mediation_capture.exists(),
                "an entrypoint used ambient PATH to select bash or env before its scrub",
            )
            records = [json.loads(line) for line in capture.read_text().splitlines()]
            self.assertEqual(
                {record["entrypoint"] for record in records},
                {path.name for path in dirname_entrypoints},
            )
            for record in records:
                self.assertEqual(record["namesPresent"], [], record["entrypoint"])
                self.assertEqual(record["valuesPresent"], [], record["entrypoint"])

    def test_protected_root_transport_is_key_only_eof_framed_and_never_echoes(self) -> None:
        with tempfile.TemporaryDirectory(prefix="protected-root-stream-") as temporary:
            root = Path(temporary).resolve()
            capture = root / "capture.json"
            payload = root / "action.sh"
            payload.write_text(
                "#!/usr/bin/env bash\nROOT_PAYLOAD_SENTINEL=must-not-be-echoed\n",
                encoding="utf-8",
            )
            payload.chmod(0o600)
            secret = "SUDO_SENTINEL_must_not_be_echoed"
            expected_secret = root / "expected-secret"
            expected_secret.write_text(secret, encoding="utf-8")
            expected_secret.chmod(0o600)
            fake_bin = root / "bin"
            fake_bin.mkdir()
            fake_ssh = fake_bin / "ssh"
            fake_ssh.write_text(
                """#!/usr/bin/env python3
import hashlib
import json
import os
import pathlib
import sys

data = sys.stdin.buffer.read()
password, separator, action = data.partition(b"\\n")
expected = pathlib.Path(os.environ["NEXUS_TEST_PAYLOAD"]).read_bytes()
secret = pathlib.Path(os.environ["NEXUS_TEST_SECRET_FILE"]).read_bytes()
record = {
    "eofReached": True,
    "framingMatched": separator == b"\\n" and password == secret and action == expected,
    "payloadSha256": hashlib.sha256(action).hexdigest(),
    "expectedSha256": hashlib.sha256(expected).hexdigest(),
    "secretInArgv": any(secret.decode() in value for value in sys.argv),
    "payloadInArgv": any(expected.decode() in value for value in sys.argv),
    "secretInEnvironment": any(secret.decode() in value for value in os.environ.values()),
    "credentialVariablesPresent": any(
        name in os.environ for name in ("DEPLOY_PASSWORD", "REMOTE_SUDO_PASSWORD")
    ),
    "argv": sys.argv[1:],
}
pathlib.Path(os.environ["NEXUS_TEST_CAPTURE"]).write_text(
    json.dumps(record, sort_keys=True), encoding="utf-8"
)
print("TRANSPORT_OK")
""",
                encoding="utf-8",
            )
            fake_ssh.chmod(0o700)
            fake_shasum = fake_bin / "shasum"
            fake_shasum.write_text(
                """#!/usr/bin/env python3
import json
import os
import pathlib
import sys

secret = pathlib.Path(os.environ["NEXUS_TEST_SECRET_FILE"]).read_text()
record = {
    "secretInEnvironment": any(secret in value for value in os.environ.values()),
    "credentialVariablesPresent": any(
        name in os.environ for name in ("DEPLOY_PASSWORD", "REMOTE_SUDO_PASSWORD")
    ),
}
pathlib.Path(os.environ["NEXUS_TEST_SHASUM_CAPTURE"]).write_text(
    json.dumps(record, sort_keys=True), encoding="utf-8"
)
os.execv("/usr/bin/shasum", ["shasum", *sys.argv[1:]])
""",
                encoding="utf-8",
            )
            fake_shasum.chmod(0o700)
            shasum_capture = root / "shasum-capture.json"
            environment = os.environ.copy()
            environment.update(
                {
                    "PATH": f"{fake_bin}:{environment['PATH']}",
                    "NEXUS_TEST_CAPTURE": str(capture),
                    "NEXUS_TEST_PAYLOAD": str(payload),
                    "NEXUS_TEST_SECRET_FILE": str(expected_secret),
                    "NEXUS_TEST_SHASUM_CAPTURE": str(shasum_capture),
                }
            )
            completed = subprocess.run(
                [
                    "bash",
                    "-c",
                    'source "$1"; export DEPLOY_PASSWORD="" REMOTE_SUDO_PASSWORD="$2"; clear_transport_secret_exports; SSH_CMD=("$3" "eterra2010@192.0.2.1"); protected_remote_root_stream "$4"',
                    "bash",
                    str(HERE / "lib.sh"),
                    secret,
                    str(fake_ssh),
                    str(payload),
                ],
                env=environment,
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertEqual(completed.stdout, "TRANSPORT_OK\n")
            self.assertNotIn(secret, completed.stdout + completed.stderr)
            self.assertNotIn("ROOT_PAYLOAD_SENTINEL", completed.stdout + completed.stderr)
            record = json.loads(capture.read_text(encoding="utf-8"))
            self.assertTrue(record["eofReached"])
            self.assertTrue(record["framingMatched"])
            self.assertEqual(record["payloadSha256"], record["expectedSha256"])
            self.assertFalse(record["secretInArgv"])
            self.assertFalse(record["payloadInArgv"])
            self.assertFalse(record["secretInEnvironment"])
            self.assertFalse(record["credentialVariablesPresent"])
            shasum_record = json.loads(shasum_capture.read_text(encoding="utf-8"))
            self.assertFalse(shasum_record["secretInEnvironment"])
            self.assertFalse(shasum_record["credentialVariablesPresent"])
            self.assertIn("/usr/bin/sudo -S -k -p ''", record["argv"][-1])
            library = (HERE / "lib.sh").read_text(encoding="utf-8")
            protected = library[
                library.index("protected_remote_root_stream()") : library.index(
                    "ssh_to_remote()", library.index("protected_remote_root_stream()")
                )
            ]
            self.assertNotIn("expect -f", protected)
            self.assertNotIn("base64", protected)
            self.assertNotIn("SUDO_ASKPASS", protected)

    def test_protected_sudo_credential_requires_owner_only_file_reference(self) -> None:
        with tempfile.TemporaryDirectory(prefix="protected-sudo-secret-") as temporary:
            root = Path(temporary).resolve()
            secret = root / "sudo-secret"
            secret.write_text("fixture-secret\n", encoding="utf-8")
            secret.chmod(0o600)
            command = [
                "bash",
                "-c",
                'source "$1"; read_protected_sudo_secret_value "$2"',
                "bash",
                str(HERE / "lib.sh"),
            ]
            accepted = subprocess.run(
                [*command, f"@{secret}"], capture_output=True, text=True, check=False
            )
            self.assertEqual(accepted.returncode, 0, accepted.stderr)
            self.assertEqual(accepted.stdout, "fixture-secret")
            raw = subprocess.run(
                [*command, "literal-secret"], capture_output=True, text=True, check=False
            )
            self.assertNotEqual(raw.returncode, 0)
            self.assertIn("@/absolute/path", raw.stderr)
            secret.chmod(0o644)
            permissive = subprocess.run(
                [*command, f"@{secret}"], capture_output=True, text=True, check=False
            )
            self.assertNotEqual(permissive.returncode, 0)
            self.assertIn("0600 or 0400", permissive.stderr)
            secret.chmod(0o600)

            leaf_alias = root / "sudo-secret-alias"
            leaf_alias.symlink_to(secret)
            leaf_rejected = subprocess.run(
                [*command, f"@{leaf_alias}"], capture_output=True, text=True, check=False
            )
            self.assertNotEqual(leaf_rejected.returncode, 0)
            self.assertRegex(leaf_rejected.stderr, "symlink|invalid")

            real_parent = root / "real-parent"
            real_parent.mkdir()
            nested_secret = real_parent / "sudo-secret"
            nested_secret.write_text("nested-secret\n", encoding="utf-8")
            nested_secret.chmod(0o600)
            swapped_ancestor = root / "current-parent"
            swapped_ancestor.symlink_to(real_parent, target_is_directory=True)
            ancestor_rejected = subprocess.run(
                [*command, f"@{swapped_ancestor / nested_secret.name}"],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertNotEqual(ancestor_rejected.returncode, 0)
            self.assertRegex(ancestor_rejected.stderr, "symlink|invalid")

            hardlink = root / "sudo-secret-hardlink"
            os.link(secret, hardlink)
            hardlink_rejected = subprocess.run(
                [*command, f"@{secret}"], capture_output=True, text=True, check=False
            )
            self.assertNotEqual(hardlink_rejected.returncode, 0)
            self.assertIn("exactly one hard link", hardlink_rejected.stderr)
            hardlink.unlink()

            secret.write_text("first-line\nsecond-line\n", encoding="utf-8")
            multiline = subprocess.run(
                [*command, f"@{secret}"], capture_output=True, text=True, check=False
            )
            self.assertNotEqual(multiline.returncode, 0)
            self.assertIn("one bounded nonempty line", multiline.stderr)
            library = (HERE / "lib.sh").read_text(encoding="utf-8")
            reader = library[
                library.index("read_protected_sudo_secret_value()") : library.index(
                    "clear_transport_secret_exports()",
                    library.index("read_protected_sudo_secret_value()"),
                )
            ]
            self.assertIn('getattr(os, "O_NOFOLLOW", 0)', reader)
            self.assertIn("before.st_nlink != 1", reader)
            self.assertIn("dir_fd=descriptor", reader)
            self.assertIn("identity(before) == identity(after)", reader)


if __name__ == "__main__":
    unittest.main()
