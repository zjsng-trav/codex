"""Exercise the generated macOS launcher with local command fixtures."""

import errno
import json
import os
from pathlib import Path
import pty
import subprocess
import sys
import tempfile
import unittest


@unittest.skipUnless(sys.platform == "darwin", "the launcher supports macOS")
class LauncherTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="codex-litellm-launcher-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.launcher = self.root / "launchers" / "codex-litellm-test"
        self.launcher.parent.mkdir()
        binary = self.root / "libexec" / "codex-litellm" / "codex"
        binary.parent.mkdir(parents=True)
        binary.write_text(
            "#!/usr/bin/env python3\n"
            "import json, os, pathlib, sys\n"
            "pathlib.Path(os.environ['FIXTURE_BINARY_LOG']).write_text(json.dumps(sys.argv[1:]))\n"
            "print('codex-cli 0.166.0')\n"
        )
        binary.chmod(0o755)
        installer = Path(__file__).resolve().parents[1] / "install-codex-litellm.sh"
        body = (
            installer.read_text()
            .split("cat <<'LAUNCHER'\n", 1)[1]
            .split("\nLAUNCHER\n", 1)[0]
        )
        self.launcher.write_text(
            "#!/bin/sh\nset -eu\nprofile=codex-litellm-test\ncurrent_version=0.166.0\n"
            + body
            + "\n"
        )
        self.launcher.chmod(0o755)
        commands = self.root / "commands"
        commands.mkdir()
        curl = commands / "curl"
        curl.write_text(
            "#!/usr/bin/env python3\n"
            "import json, os, pathlib, sys\n"
            "with pathlib.Path(os.environ['FIXTURE_CURL_LOG']).open('a') as log:\n"
            "    log.write(json.dumps(sys.argv[1:]) + '\\n')\n"
            "if os.environ.get('FIXTURE_OFFLINE') == '1': sys.exit(28)\n"
            "print(os.environ['FIXTURE_RELEASE_URL'], end='')\n"
        )
        curl.chmod(0o755)
        security = commands / "security"
        security.write_text("#!/bin/sh\nexit 1\n")
        security.chmod(0o755)
        self.binary_log = self.root / "binary.json"
        self.curl_log = self.root / "curl.jsonl"
        self.env = dict(
            os.environ,
            PATH=str(commands) + os.pathsep + os.environ["PATH"],
            CODEX_LITELLM_API_KEY="fixture-key-not-a-real-secret",
            FIXTURE_BINARY_LOG=str(self.binary_log),
            FIXTURE_CURL_LOG=str(self.curl_log),
            FIXTURE_RELEASE_URL="https://github.com/zjsng-trav/codex/releases/tag/codex-litellm-v0.167.0",
        )

    def run_launcher(self, *args, interactive=False):
        if not interactive:
            return subprocess.run(
                [str(self.launcher), *args],
                env=self.env,
                capture_output=True,
                text=True,
            )
        master, slave = pty.openpty()
        try:
            process = subprocess.Popen(
                [str(self.launcher), *args],
                env=self.env,
                stdin=slave,
                stdout=subprocess.DEVNULL,
                stderr=slave,
            )
        finally:
            os.close(slave)
        chunks = []
        try:
            while True:
                try:
                    chunk = os.read(master, 8192)
                except OSError as error:
                    if error.errno == errno.EIO:
                        break
                    raise
                if not chunk:
                    break
                chunks.append(chunk)
        finally:
            os.close(master)
        return subprocess.CompletedProcess(
            process.args,
            process.wait(timeout=10),
            stdout="",
            stderr=b"".join(chunks).decode().replace("\r\n", "\n"),
        )

    def requests(self):
        if not self.curl_log.exists():
            return []
        return [json.loads(line) for line in self.curl_log.read_text().splitlines()]

    def test_normal_launch_disables_upstream_check_and_preserves_arguments(self):
        result = self.run_launcher("exec", "explain this code")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            json.loads(self.binary_log.read_text()),
            [
                "--no-daemon",
                "-p",
                "codex-litellm-test",
                "-c",
                "check_for_update_on_startup=false",
                "exec",
                "explain this code",
            ],
        )
        self.assertEqual(self.requests(), [])

    def test_updater_commands_never_execute_the_binary(self):
        for args in [
            ("update",),
            ("--profile", "other", "update"),
            ("-c", 'label="update"', "update"),
            ("app-server", "daemon", "update"),
        ]:
            with self.subTest(args=args):
                result = self.run_launcher(*args)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("0.166.0 -> 0.167.0", result.stderr)
                self.assertIn("Review and update manually:", result.stderr)
                self.assertFalse(self.binary_log.exists())
        for request in self.requests():
            self.assertEqual(
                request[-1], "https://github.com/zjsng-trav/codex/releases/latest"
            )
            self.assertIn("--max-time", request)

    def test_option_values_and_prompt_text_are_not_updater_commands(self):
        for args in [("-c", 'label="update"', "exec", "update"), ("--", "update")]:
            with self.subTest(args=args):
                result = self.run_launcher(*args)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(
                    json.loads(self.binary_log.read_text())[-len(args) :], list(args)
                )
        self.assertEqual(self.requests(), [])

    def test_same_or_older_releases_do_not_offer_an_update(self):
        for version in ["0.166.0", "0.165.9", "0.99.0"]:
            with self.subTest(version=version):
                self.env["FIXTURE_RELEASE_URL"] = (
                    "https://github.com/zjsng-trav/codex/releases/tag/codex-litellm-v"
                    + version
                )
                result = self.run_launcher("update")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("is current", result.stdout)
                self.assertNotIn("update available", result.stderr)
        self.assertFalse(self.binary_log.exists())

    def test_upstream_or_malformed_release_never_becomes_an_update(self):
        for url in [
            "https://github.com/openai/codex/releases/tag/rust-v9.0.0",
            "https://github.com/zjsng-trav/codex/releases/tag/rust-v9.0.0",
            "https://github.com/zjsng-trav/codex/releases/tag/codex-litellm-vgarbage",
        ]:
            with self.subTest(url=url):
                self.env["FIXTURE_RELEASE_URL"] = url
                result = self.run_launcher("update")
                self.assertEqual(result.returncode, 1)
                self.assertIn(
                    "Could not identify a stable custom release", result.stderr
                )
                self.assertNotIn("openai/codex", result.stderr)
        self.assertFalse(self.binary_log.exists())

    def test_interactive_notice_uses_cache_and_does_not_block_launch(self):
        for _ in range(2):
            result = self.run_launcher(interactive=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("0.166.0 -> 0.167.0", result.stderr)
        self.assertEqual(len(self.requests()), 1)
        result = self.run_launcher("update")
        self.assertEqual(result.returncode, 0)
        self.assertEqual(len(self.requests()), 2)

    def test_offline_startup_continues_and_manual_check_reports_failure(self):
        self.env["FIXTURE_OFFLINE"] = "1"
        for _ in range(2):
            result = self.run_launcher(interactive=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stderr, "")
        self.assertEqual(len(self.requests()), 1)
        result = self.run_launcher("update")
        self.assertEqual(result.returncode, 1)
        self.assertIn("Could not check custom releases", result.stderr)

    def test_version_does_not_require_a_key_or_check_for_updates(self):
        self.env.pop("CODEX_LITELLM_API_KEY")
        result = self.run_launcher("--version")
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "codex-cli 0.166.0\n")
        self.assertEqual(self.requests(), [])


if __name__ == "__main__":
    unittest.main()
