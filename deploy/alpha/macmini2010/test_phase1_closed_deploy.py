from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path


HERE = Path(__file__).resolve().parent


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
        authority_remote = authority.index('"${remote_tmp_dir}/nexus-v2-phase1-closed-ingress.sh" preclose')
        authority_restart = authority.index('systemctl restart "${AUTHORITY_SERVICE_NAME}.service"')
        self.assertLess(node_remote, node_restart)
        self.assertLess(authority_remote, authority_restart)

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


if __name__ == "__main__":
    unittest.main()
