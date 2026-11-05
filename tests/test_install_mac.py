import importlib.util
from pathlib import Path
import subprocess
import tempfile
import tomllib
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('dreamer_mac_installer', Path(__file__).resolve().parents[1] / 'scripts/install_mac.py')
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class MacInstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        patcher = patch.object(installer.shutil, 'which', return_value='/fixture/bin/gh')
        patcher.start()
        self.addCleanup(patcher.stop)
        self.root = Path(self.temp.name)
        self.home = self.root / 'home'
        (self.home / '.codex').mkdir(parents=True)
        self.binary = self.root / 'binary'
        self.binary.write_bytes(b'synthetic binary')
        self.writer = self.root / 'writer.env'
        self.writer.write_text('DREAMER_MCP_WRITE_TOKEN=' + 'w' * 64 + '\n')
        self.writer.chmod(0o600)
        self.codex = self.home / '.codex/config.toml'
        self.original = 'model = "existing-model"\n[mcp_servers.dreamer]\nurl = "http://127.0.0.1:3445/mcp"\n[mcp_servers.dreamer.http_headers]\nAuthorization = "Bearer existing-private-read-credential"\n'
        self.codex.write_text(self.original)
        self.agents = self.home / '.codex/AGENTS.md'
        self.agents.write_text('# My guidance\n\nKeep this personal instruction.\n')

    def test_install_is_repeatable_and_preserves_unrelated_settings_and_secrets(self):
        credential = self.writer.read_bytes()
        with patch.object(installer.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0)):
            binary, settings = installer.install(self.home, self.binary, self.writer)
            first = tomllib.loads(self.codex.read_text())
            self.assertEqual(first['mcp_servers']['dreamer'], tomllib.loads(self.original)['mcp_servers']['dreamer'])
            self.assertEqual(first['model'], 'existing-model')
            self.assertEqual(first['mcp_servers']['dreamer_mac']['args'], ['mac', '--settings', str(settings), 'mcp'])
            self.assertNotIn('w' * 64, self.codex.read_text())
            self.assertEqual(binary.read_bytes(), self.binary.read_bytes())
            self.assertEqual(self.codex.stat().st_mode & 0o777, 0o600)
            custom = settings.read_text().replace('http://192.168.0.25:3445/mcp', 'http://127.0.0.1:9998/mcp')
            custom = custom.replace('repositories = ["geckolabs/Gecko-API"]', 'repositories = ["geckolabs/Gecko-API", "example/second"]')
            settings.write_text(custom)
            installer.install(self.home, self.binary, self.writer)
        self.assertEqual(settings.read_text(), custom)
        self.assertEqual(self.writer.read_bytes(), credential)
        self.assertEqual(self.agents.read_text().count(installer.START), 1)
        self.assertIn('Keep this personal instruction.', self.agents.read_text())
        self.assertEqual(self.codex.read_text().count(installer.CONFIG_START), 1)

    def test_legacy_settings_gain_an_explicit_default_list_without_scope_expansion(self):
        settings = self.home / '.config/dreamer/mac.toml'
        settings.parent.mkdir(parents=True)
        legacy = 'repository = "example/first"\nendpoint = "http://127.0.0.1:9988/mcp"\nwriter_token_file = "private.env"\ncache_dir = "private-cache"\n'
        settings.write_text(legacy)
        with patch.object(installer.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0)):
            installer.install(self.home, self.binary, self.writer)
        result = tomllib.loads(settings.read_text())
        self.assertEqual(result['repositories'], ['example/first'])
        self.assertEqual(result['endpoint'], 'http://127.0.0.1:9988/mcp')
        self.assertEqual(result['writer_token_file'], 'private.env')

    def test_item_reader_can_find_codex_outside_the_github_cli_directory(self):
        with patch.object(installer.shutil, 'which', side_effect=lambda name: {
            'gh': '/fixture/github/bin/gh', 'codex': '/fixture/codex/bin/codex'
        }.get(name)), patch.object(installer.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0)):
            installer.install(self.home, self.binary, self.writer)
        configured = tomllib.loads(self.codex.read_text())['mcp_servers']['dreamer_mac']['env']['PATH']
        self.assertEqual(configured.split(installer.os.pathsep)[:2], ['/fixture/github/bin', '/fixture/codex/bin'])
        self.assertNotIn('DREAMER_MCP_WRITE_TOKEN', configured)

    def test_selected_chat_enablement_preserves_scope_and_stays_enabled(self):
        with patch.object(installer.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0)):
            _, settings = installer.install(self.home, self.binary, self.writer)
            self.assertNotIn('learn_conversations', tomllib.loads(settings.read_text()))
            before = tomllib.loads(settings.read_text())
            installer.install(self.home, self.binary, self.writer, enable_conversations=True)
            enabled = tomllib.loads(settings.read_text())
            self.assertTrue(enabled.pop('learn_conversations'))
            self.assertEqual(enabled, before)
            installer.install(self.home, self.binary, self.writer)
            self.assertTrue(tomllib.loads(settings.read_text())['learn_conversations'])
            settings.write_text(settings.read_text().replace('learn_conversations = true', 'learn_conversations = false # local'))
            installer.install(self.home, self.binary, self.writer, enable_conversations=True)
            self.assertTrue(tomllib.loads(settings.read_text())['learn_conversations'])

    def test_unmanaged_mcp_is_rejected_before_installation(self):
        self.codex.write_text(self.original + '\n[mcp_servers.dreamer_mac]\ncommand = "operator-command"\n')
        before = self.codex.read_bytes()
        with patch.object(installer.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0)):
            with self.assertRaisesRegex(RuntimeError, 'unmanaged'):
                installer.install(self.home, self.binary, self.writer)
        self.assertEqual(self.codex.read_bytes(), before)
        self.assertFalse((self.home / '.local/share/dreamer').exists())


if __name__ == '__main__':
    unittest.main()
