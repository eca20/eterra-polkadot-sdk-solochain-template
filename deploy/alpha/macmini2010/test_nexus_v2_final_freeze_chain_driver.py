#!/usr/bin/env python3

from __future__ import annotations

import os
import re
import struct
import subprocess
import tempfile
import unittest
from pathlib import Path


DRIVER = Path(__file__).with_name("nexus-v2-final-freeze-chain-driver")


def executable(path: Path, payload: bytes) -> Path:
    path.write_bytes(payload)
    path.chmod(0o700)
    return path


class FinalFreezeChainDriverTests(unittest.TestCase):
    def test_empty_artifact_expansion_is_safe_in_native_bash(self) -> None:
        source = DRIVER.read_text(encoding="utf-8")
        self.assertIn('${artifact_specs[@]+"${artifact_specs[@]}"}', source)
        completed = subprocess.run(
            [
                "/bin/bash",
                "-c",
                "set -u; artifact_specs=(); "
                "python3 - fixed ${artifact_specs[@]+\"${artifact_specs[@]}\"} <<'PY'\n"
                "import sys\n"
                "assert sys.argv == ['-', 'fixed'], sys.argv\n"
                "PY\n",
            ],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stdout)

    def test_generated_chain_preflight_executes_through_elf_probe(self) -> None:
        source = DRIVER.read_text(encoding="utf-8")
        preflight = source.split('if [[ "${action}" == "preflight" ]]', 1)[1].split(
            'if [[ "${action}" == "freeze" ]]', 1
        )[0]
        match = re.search(r"\n\s*chain\)\n\s*remote_root_bash <<EOF\n(?P<script>.*?)\nEOF\n\s*;;", preflight, re.DOTALL)
        self.assertIsNotNone(match, "chain preflight remote script not found")
        # The local unquoted outer heredoc consumes backslashes before dollars.
        generated = match.group("script").replace("\\$", "$")
        generated = generated.replace(". /etc/os-release", 'ID="ubuntu"\nVERSION_ID="24.04"')
        generated = generated.replace('"${REMOTE_NODE_BIN}" --version >/dev/null', "true")

        with tempfile.TemporaryDirectory(prefix="nexus-v2-chain-preflight-test-") as temporary:
            root = Path(temporary)
            fake_bin = root / "bin"
            fake_bin.mkdir()
            executable(fake_bin / "systemctl", b"#!/bin/sh\nexit 0\n")
            executable(fake_bin / "ss", b"#!/bin/sh\nexit 0\n")
            executable(
                fake_bin / "uname",
                b"#!/bin/sh\n"
                b"case \"$*\" in\n"
                b"  -s) echo Linux ;;\n"
                b"  -m) echo x86_64 ;;\n"
                b"  -r) echo 6.8.0-fixture ;;\n"
                b"  -srm) echo 'Linux 6.8.0-fixture x86_64' ;;\n"
                b"  *) exit 2 ;;\n"
                b"esac\n",
            )
            node = root / "solochain-eterra-node"
            header = bytearray(64)
            header[:7] = b"\x7fELF\x02\x01\x01"
            struct.pack_into("<H", header, 16, 3)
            struct.pack_into("<H", header, 18, 62)
            executable(node, bytes(header))
            data = root / "data"
            deploy = root / "deploy"
            data.mkdir()
            deploy.mkdir()
            for name in ("chain.json", "node.env"):
                (root / name).write_text("fixture\n", encoding="utf-8")
            environment = dict(os.environ)
            environment.update(
                {
                    "PATH": f"{fake_bin}:{environment['PATH']}",
                    "REMOTE_NODE_SERVICE_NAME": "fixture-node",
                    "REMOTE_NODE_DATA_DIR": str(data),
                    "REMOTE_NODE_BIN": str(node),
                    "REMOTE_NODE_SPEC": str(root / "chain.json"),
                    "REMOTE_NODE_ENV_FILE": str(root / "node.env"),
                    "DEPLOY_ROOT": str(deploy),
                    "isolated_rpc_port": "19948",
                    "isolated_p2p_port": "31348",
                }
            )
            completed = subprocess.run(
                ["bash", "-c", generated],
                env=environment,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                check=False,
            )
        self.assertEqual(completed.returncode, 0, completed.stdout)


if __name__ == "__main__":
    unittest.main()
