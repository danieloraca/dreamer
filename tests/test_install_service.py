"""Deployment workflow tests in a temporary filesystem; no sudo/systemd changes."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import sqlite3
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("dreamer_installer", Path(__file__).resolve().parents[1] / "scripts/install_service.py")
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)
REAL_RUN = subprocess.run


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name).resolve()
        self.source = self.root / "checkout"
        self.binary = self.root / "bin/dreamer"
        self.config = self.root / "etc/dreamer/dreamer.toml"
        self.state = self.root / "state"
        self.unit = self.root / "system/dreamer.service"
        self.unit.parent.mkdir()
        (self.root / "systemd-running").mkdir()
        (self.source / "target/release").mkdir(parents=True)
        (self.source / "deploy").mkdir()
        (self.source / "target/release/dreamer").write_text("first-binary")
        (self.source / "target/release/dreamer").chmod(0o755)
        (self.source / "deploy/dreamer.toml").write_text("fixture config")
        (self.source / "deploy/dreamer.service").write_text("# Managed by Dreamer.\n[Service]\nType=exec\n")
        self.metadata = {"data_dir": str(self.state), "http": {"listen": "127.0.0.1:3445"}}
        self.calls = []
        self.active = False
        replacements = {"ROOT": self.source, "BINARY": self.binary, "CONFIG": self.config,
                        "ENV": self.config.with_name("dreamer.env"), "UNIT": self.unit, "STATE": self.state}
        for name, value in replacements.items():
            self.enter(patch.object(installer, name, value))
        self.enter(patch.object(installer.sys, "platform", "linux"))
        self.enter(patch.object(installer, "Path", self.mapped_path))
        self.enter(patch.object(installer.os, "chown"))
        self.enter(patch.object(installer.pwd, "getpwnam", return_value=SimpleNamespace(pw_uid=1001, pw_gid=1001)))
        self.enter(patch.object(installer.grp, "getgrnam", return_value=SimpleNamespace(gr_gid=1001)))
        self.enter(patch.object(installer.shutil, "which", side_effect=lambda command: "/usr/bin/" + command))
        self.enter(patch.object(installer.subprocess, "run", side_effect=self.command))
        self.enter(patch.object(installer.urllib.request, "build_opener", return_value=SimpleNamespace(open=self.health)))

    def enter(self, patcher):
        patcher.start()
        self.addCleanup(patcher.stop)

    def mapped_path(self, value):
        return {"/var/backups/dreamer": self.root / "backups", "/run/systemd/system": self.root / "systemd-running"}.get(str(value), Path(value))

    def command(self, command, **kwargs):
        self.calls.append(command)
        if "--json" in command:
            return subprocess.CompletedProcess(command, 0, json.dumps(self.metadata))
        if command[0] == "systemctl":
            if "is-active" in command:
                return subprocess.CompletedProcess(command, 0 if self.active else 3)
            if "stop" in command:
                self.active = False
            if "enable" in command:
                self.active = True
        if command[0] == "runuser":
            if "-c" in command:
                return REAL_RUN([sys.executable, *command[5:]], check=True, capture_output=True, text=True)
            with sqlite3.connect(self.state / "knowledge.sqlite3") as database:
                database.execute("CREATE TABLE IF NOT EXISTS observations (value TEXT)")
        return subprocess.CompletedProcess(command, 0)

    def health(self, request, timeout):
        self.assertEqual(request.full_url, "http://127.0.0.1:3445/health")
        self.assertTrue(request.get_header("Authorization").startswith("Bearer "))
        response = io.BytesIO(b'{"status":"ok"}')
        response.status = 200
        return response

    def test_install_and_update_preserve_credentials_config_and_back_up_knowledge(self):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            installer.install_service()
        credentials = installer.ENV.read_text()
        self.assertEqual(installer.ENV.stat().st_mode & 0o777, 0o600)
        self.assertEqual(self.config.parent.stat().st_mode & 0o777, 0o750)
        self.config.write_text("operator's existing config")
        with sqlite3.connect(self.state / "knowledge.sqlite3") as database:
            database.execute("INSERT INTO observations VALUES ('keep this knowledge')")
        (self.source / "target/release/dreamer").write_text("second-binary")
        with contextlib.redirect_stdout(output):
            installer.install_service()
        self.assertEqual(installer.ENV.read_text(), credentials)
        self.assertEqual(self.config.read_text(), "operator's existing config")
        self.assertEqual(self.binary.read_text(), "second-binary")
        self.assertEqual(self.binary.with_name("dreamer.previous").read_text(), "first-binary")
        backups = list((self.root / "backups").glob("*.sqlite3"))
        self.assertEqual(len(backups), 1)
        with sqlite3.connect(backups[0]) as database:
            self.assertEqual(database.execute("SELECT value FROM observations").fetchone()[0], "keep this knowledge")
        self.assertIn(["systemctl", "stop", "dreamer.service"], self.calls)
        self.assertIn(["systemctl", "enable", "--now", "dreamer.service"], self.calls)
        for line in credentials.splitlines():
            self.assertNotIn(line.split("=", 1)[1], output.getvalue())

    def test_an_unmanaged_unit_is_rejected_before_modifying_installation(self):
        self.unit.write_text("operator's custom unit")
        with self.assertRaisesRegex(RuntimeError, "not managed"):
            installer.install_service()
        self.assertEqual(self.unit.read_text(), "operator's custom unit")
        self.assertFalse(self.config.exists())
        self.assertFalse(self.binary.exists())

    def test_custom_data_directory_is_rejected_without_moving_knowledge(self):
        self.metadata["data_dir"] = str(self.root / "other-knowledge")
        with self.assertRaisesRegex(RuntimeError, "will not move"):
            installer.install_service()
        self.assertFalse(self.config.exists())
        self.assertFalse(self.state.exists())


if __name__ == "__main__":
    unittest.main()
