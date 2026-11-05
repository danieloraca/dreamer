#!/usr/bin/python3
"""Managed Dreamer SQLite snapshots. No credentials, network or service changes."""
import argparse
import contextlib
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import sqlite3
import stat
import sys
import tempfile
import time
import uuid

MANAGER = 'dreamer-backup-v1'
NAME = re.compile(r'^scheduled-[0-9]{8}T[0-9]{12}Z-[0-9a-f]{12}\.sqlite3$')


def utc():
    return datetime.now(timezone.utc).isoformat().replace('+00:00', 'Z')


def private_directory(path):
    path = Path(path)
    path.mkdir(mode=0o700, parents=True, exist_ok=True)
    metadata = path.lstat()
    if not stat.S_ISDIR(metadata.st_mode) or metadata.st_mode & 0o077 or metadata.st_uid != os.geteuid():
        raise ValueError('backup destination must be a private directory owned by this account')
    return path


def regular_file(path):
    path = Path(path)
    if not stat.S_ISREG(path.lstat().st_mode):
        raise ValueError('expected a regular file, not a symlink')
    return path


def atomic_json(path, value):
    fd, name = tempfile.mkstemp(prefix='.dreamer-json-', dir=path.parent)
    try:
        with os.fdopen(fd, 'w') as destination:
            json.dump(value, destination, sort_keys=True)
            destination.write('\n')
            destination.flush()
            os.fsync(destination.fileno())
        os.replace(name, path)
    finally:
        if os.path.exists(name):
            os.unlink(name)


def sha256(path):
    digest = hashlib.sha256()
    with path.open('rb') as source:
        for block in iter(lambda: source.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()


def inspect(path):
    regular_file(path)
    with contextlib.closing(sqlite3.connect(path.resolve().as_uri() + '?mode=ro', uri=True)) as database:
        database.execute('PRAGMA query_only=ON')
        if database.execute('PRAGMA quick_check').fetchall() != [('ok',)]:
            raise ValueError('snapshot integrity check failed')
        if database.execute('PRAGMA foreign_key_check').fetchone() is not None:
            raise ValueError('snapshot foreign-key check failed')
        rows = database.execute('SELECT version FROM schema_migrations ORDER BY version').fetchall()
        if not rows or [row[0] for row in rows] != list(range(1, len(rows) + 1)):
            raise ValueError('snapshot migration history is incomplete')
        database.execute('SELECT COUNT(*) FROM lessons').fetchone()
    return {'schema': len(rows), 'bytes': path.stat().st_size, 'sha256': sha256(path)}


def copy_snapshot(source, destination, timeout_seconds):
    deadline = time.monotonic() + timeout_seconds
    def progress(_status, _remaining, _total):
        if time.monotonic() >= deadline:
            raise TimeoutError('snapshot deadline exceeded')
    regular_file(source)
    with contextlib.closing(sqlite3.connect(source.resolve().as_uri() + '?mode=ro', uri=True)) as original, contextlib.closing(sqlite3.connect(destination)) as copy:
        original.execute('PRAGMA query_only=ON')
        original.backup(copy, pages=128, progress=progress, sleep=0.1)
        copy.execute('PRAGMA journal_mode=DELETE')
    with destination.open('rb') as stored:
        os.fsync(stored.fileno())


def verify(snapshot):
    snapshot = Path(snapshot)
    manifest_path = regular_file(snapshot.with_suffix('.json'))
    manifest = json.loads(manifest_path.read_text())
    if manifest.get('manager') != MANAGER or manifest.get('filename') != snapshot.name:
        raise ValueError('unrecognized snapshot manifest')
    observed = inspect(snapshot)
    if any(manifest.get(key) != observed[key] for key in ('schema', 'bytes', 'sha256')):
        raise ValueError('snapshot does not match its manifest')
    return manifest


def managed_snapshots(directory):
    pairs = []
    for path in directory.glob('scheduled-*.sqlite3'):
        if not NAME.fullmatch(path.name) or path.is_symlink():
            continue
        manifest_path = path.with_suffix('.json')
        try:
            regular_file(manifest_path)
            manifest = json.loads(manifest_path.read_text())
            if manifest.get('manager') == MANAGER and manifest.get('filename') == path.name:
                pairs.append(path)
        except (OSError, ValueError):
            continue
    return sorted(pairs, key=lambda path: path.name, reverse=True)


def create(database, directory, keep=14, timeout_seconds=120):
    database = Path(database)
    status_path = database.parent / 'backup-status.json'
    last_success = None
    try:
        if status_path.exists() and not status_path.is_symlink():
            last_success = json.loads(status_path.read_text()).get('last_success')
    except (OSError, ValueError):
        pass
    attempt = utc()
    temporary = None
    snapshot = None
    try:
        if type(keep) is not int or not 1 <= keep <= 3650 or type(timeout_seconds) is not int or not 1 <= timeout_seconds <= 1800:
            raise ValueError('invalid backup retention/deadline')
        directory = private_directory(directory)
        fd, temporary = tempfile.mkstemp(prefix='.dreamer-snapshot-', dir=directory)
        os.close(fd)
        temporary = Path(temporary)
        copy_snapshot(database, temporary, timeout_seconds)
        observed = inspect(temporary)
        filename = 'scheduled-' + datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%S%fZ') + '-' + uuid.uuid4().hex[:12] + '.sqlite3'
        snapshot = directory / filename
        os.replace(temporary, snapshot)
        manifest = {'manager': MANAGER, 'filename': filename, 'completed_at': utc(), **observed}
        atomic_json(snapshot.with_suffix('.json'), manifest)
        verify(snapshot)
        last_success = {**manifest, 'retained_count': len(managed_snapshots(directory))}
        # Prune only our recognized pairs, only after a checked new snapshot exists.
        for expired in managed_snapshots(directory)[keep:]:
            expired.unlink()
            expired.with_suffix('.json').unlink()
        success = {**manifest, 'retained_count': len(managed_snapshots(directory))}
        atomic_json(status_path, {'manager': MANAGER, 'state': 'ok', 'last_attempt_at': attempt, 'last_success': success, 'error_code': None})
        return success
    except Exception:
        # No exception text/source content is copied into dashboard metadata.
        atomic_json(status_path, {'manager': MANAGER, 'state': 'failed', 'last_attempt_at': attempt, 'last_success': last_success, 'error_code': 'backup_failed'})
        raise
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def restore(snapshot, output, timeout_seconds=120):
    """Create a checked NEW file. No live database replacement or service control."""
    snapshot, output = Path(snapshot), Path(output)
    manifest = verify(snapshot)
    private_directory(output.parent)
    # O_EXCL also refuses dangling symlinks and racing existing destinations.
    fd = os.open(output, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    os.close(fd)
    try:
        copy_snapshot(snapshot, output, timeout_seconds)
        observed = inspect(output)
        if observed != {key: manifest[key] for key in ('schema', 'bytes', 'sha256')}:
            # SQLite backup may rewrite equivalent file-header counters; validate
            # logical content separately below instead of requiring byte identity.
            if observed['schema'] != manifest['schema']:
                raise ValueError('restored schema differs')
        with contextlib.closing(sqlite3.connect(output.resolve().as_uri() + '?mode=ro', uri=True)) as destination, contextlib.closing(sqlite3.connect(snapshot.resolve().as_uri() + '?mode=ro', uri=True)) as original:
            def dump_digest(database):
                digest = hashlib.sha256()
                for line in database.iterdump():
                    digest.update(line.encode())
                    digest.update(b'\n')
                return digest.digest()
            if dump_digest(destination) != dump_digest(original):
                raise ValueError('restored logical content differs')
        return {'restored': True, 'schema': observed['schema'], 'output': str(output)}
    except Exception:
        output.unlink(missing_ok=True)
        raise


def settings(path):
    value = json.loads(Path(path).read_text())
    if not isinstance(value, dict) or not isinstance(value.get('directory'), str):
        raise ValueError('invalid backup configuration')
    if set(value) - {'directory', 'keep', 'timeout_seconds'} or not Path(value['directory']).is_absolute():
        raise ValueError('invalid backup configuration')
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    backup = commands.add_parser('create')
    backup.add_argument('--database', type=Path, required=True)
    backup.add_argument('--config', type=Path, required=True)
    for name in ('verify', 'restore'):
        command = commands.add_parser(name)
        command.add_argument('--snapshot', type=Path, required=True)
        if name == 'restore':
            command.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    os.umask(0o077)
    try:
        if args.command == 'create':
            try:
                configuration = settings(args.config)
            except (OSError, ValueError):
                status_path = args.database.parent / 'backup-status.json'
                previous = {}
                try:
                    if not status_path.is_symlink():
                        previous = json.loads(status_path.read_text())
                except (OSError, ValueError):
                    pass
                atomic_json(status_path, {'manager':MANAGER, 'state':'failed', 'last_attempt_at':utc(), 'last_success':previous.get('last_success'), 'error_code':'backup_failed'})
                raise
            result = create(args.database, **configuration)
        elif args.command == 'verify':
            result = verify(args.snapshot)
        else:
            result = restore(args.snapshot, args.output)
        print(json.dumps(result, sort_keys=True))
        return 0
    except (OSError, ValueError, sqlite3.Error, TimeoutError):
        print('Backup operation failed; inspect paths, permissions, manifest and database integrity locally.', file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
