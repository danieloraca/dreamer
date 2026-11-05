"""Snapshot/restore tests on disposable WAL databases; never operates on the Pi."""
import contextlib
import importlib.util
import json
import os
from pathlib import Path
import sqlite3
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('dreamer_backup', Path(__file__).resolve().parents[1] / 'scripts/backup.py')
backup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(backup)


class BackupTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.database = self.root / 'knowledge.sqlite3'
        self.writer = sqlite3.connect(self.database)
        self.addCleanup(self.writer.close)
        self.writer.executescript("PRAGMA journal_mode=WAL; CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY); INSERT INTO schema_migrations VALUES (1); CREATE TABLE lessons(id TEXT PRIMARY KEY, summary TEXT); INSERT INTO lessons VALUES ('a','retain this evidence');")
        self.writer.commit()
        self.directory = self.root / 'backups'

    def test_online_snapshot_contains_committed_wal_and_restores_logical_state(self):
        self.writer.execute("INSERT INTO lessons VALUES ('b','committed WAL row')")
        self.writer.commit()
        result = backup.create(self.database, self.directory)
        snapshot = self.directory / result['filename']
        self.assertEqual(backup.verify(snapshot)['sha256'], result['sha256'])
        self.assertEqual(snapshot.stat().st_mode & 0o777, 0o600)
        self.writer.execute('DELETE FROM lessons')
        self.writer.commit()
        restored = self.root / 'restored/knowledge.sqlite3'
        backup.restore(snapshot, restored)
        with contextlib.closing(sqlite3.connect(restored)) as database:
            self.assertEqual(database.execute('SELECT id FROM lessons ORDER BY id').fetchall(), [('a',), ('b',)])
        self.assertEqual(self.writer.execute('SELECT COUNT(*) FROM lessons').fetchone()[0], 0)
        with self.assertRaises(FileExistsError):
            backup.restore(snapshot, restored)

    def test_retention_only_prunes_managed_pairs_after_checked_success(self):
        unknown = None
        for _ in range(4):
            backup.create(self.database, self.directory, keep=2)
            if unknown is None:
                unknown = self.directory / 'my-manual-copy.sqlite3'
                unknown.write_text('preserve operator file')
        self.assertEqual(len(backup.managed_snapshots(self.directory)), 2)
        self.assertEqual(unknown.read_text(), 'preserve operator file')
        current = [path.name for path in backup.managed_snapshots(self.directory)]
        old_success = json.loads((self.root / 'backup-status.json').read_text())['last_success']
        with patch.object(backup, 'copy_snapshot', side_effect=OSError('synthetic disk failure')):
            with self.assertRaises(OSError):
                backup.create(self.database, self.directory, keep=1)
        self.assertEqual([path.name for path in backup.managed_snapshots(self.directory)], current)
        status = json.loads((self.root / 'backup-status.json').read_text())
        self.assertEqual(status['state'], 'failed')
        self.assertEqual(status['last_success'], old_success)
        self.assertNotIn('synthetic', json.dumps(status))
        self.assertFalse(list(self.directory.glob('.dreamer-snapshot-*')))

    def test_manifest_tamper_invalid_policy_and_symlink_targets_are_rejected(self):
        result = backup.create(self.database, self.directory)
        snapshot = self.directory / result['filename']
        manifest = snapshot.with_suffix('.json')
        value = json.loads(manifest.read_text())
        value['sha256'] = '0' * 64
        manifest.write_text(json.dumps(value))
        with self.assertRaises(ValueError):
            backup.verify(snapshot)
        with self.assertRaises(ValueError):
            backup.restore(snapshot, self.root / 'restored/knowledge.sqlite3')
        with self.assertRaises(ValueError):
            backup.create(self.database, self.directory, keep=0)
        manifest.unlink()
        result = backup.create(self.database, self.directory)
        target = self.root / 'existing.sqlite3'
        target.symlink_to(self.database)
        with self.assertRaises(FileExistsError):
            backup.restore(self.directory / result['filename'], target)
        linked = self.root / 'linked-backups'
        linked.symlink_to(self.directory, target_is_directory=True)
        with self.assertRaises(ValueError):
            backup.create(self.database, linked)


if __name__ == '__main__':
    unittest.main()
