#!/usr/bin/env python3
"""Install/update Dreamer on a systemd Linux Pi. Never prints credential values."""
import argparse
import grp
import json
import os
from pathlib import Path
import pwd
import secrets
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
BINARY = Path("/usr/local/bin/dreamer")
CONFIG = Path("/etc/dreamer/dreamer.toml")
ENV = Path("/etc/dreamer/dreamer.env")
UNIT = Path("/etc/systemd/system/dreamer.service")
STATE = Path("/var/lib/dreamer")
SERVICE = "dreamer.service"
BACKUP_UNIT = UNIT.with_name("dreamer-backup.service")
BACKUP_TIMER = UNIT.with_name("dreamer-backup.timer")
BACKUP_CONFIG = CONFIG.with_name("backup.json")
BACKUP_SCRIPT = Path("/usr/local/lib/dreamer/backup.py")


def run(*args, **kwargs):
    kwargs.setdefault("check", True)
    return subprocess.run([str(arg) for arg in args], **kwargs)


def atomic_copy(source, destination, mode):
    descriptor, name = tempfile.mkstemp(prefix=".dreamer-", dir=destination.parent)
    temporary = Path(name)
    try:
        with os.fdopen(descriptor, "wb") as output:
            output.write(source.read_bytes())
        temporary.chmod(mode)
        os.replace(temporary, destination)
    finally:
        temporary.unlink(missing_ok=True)


def read_credentials():
    values = {}
    for line in ENV.read_text().splitlines():
        if line.strip() and not line.lstrip().startswith("#"):
            key, separator, value = line.partition("=")
            if separator:
                values[key.strip()] = value.strip().strip("\"'")
    keys = ("DREAMER_MCP_TOKEN", "DREAMER_MCP_WRITE_TOKEN")
    for key in keys:
        value = values.get(key, "")
        if len(value) < 32 or any(c not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_" for c in value):
            raise RuntimeError(f"{ENV}: {key} must be a token of at least 32 ASCII letters/digits, '-' or '_'; value withheld")
    if values[keys[0]] == values[keys[1]]:
        raise RuntimeError("Read and write tokens must differ; values withheld")
    return values


def install_service():
    if sys.platform != "linux" or not Path("/run/systemd/system").is_dir():
        raise RuntimeError("Run this installer on the Pi's systemd Linux host")
    source = ROOT / "target/release/dreamer"
    if not source.is_file() or not os.access(source, os.X_OK):
        raise RuntimeError("Build first with make build")
    for command in ("systemctl", "systemd-analyze", "runuser", "getent", "useradd"):
        if not shutil.which(command):
            raise RuntimeError(f"Required command missing: {command}")
    if not Path("/usr/bin/python3").is_file():
        raise RuntimeError("The system Python /usr/bin/python3 is required for database backups")
    for unit in (UNIT, BACKUP_UNIT, BACKUP_TIMER):
        if unit.is_symlink() or (unit.exists() and not unit.read_text().startswith("# Managed by Dreamer.")):
            raise RuntimeError(f"{unit} is not managed by this installer; integrate it manually")
    for asset in ("scripts/backup.py", "deploy/backup.json", "deploy/dreamer-backup.service", "deploy/dreamer-backup.timer"):
        if not (ROOT / asset).is_file():
            raise RuntimeError("Missing backup deployment asset: " + asset)
    candidate_backup = BACKUP_CONFIG if BACKUP_CONFIG.exists() else ROOT / "deploy/backup.json"
    backup_settings = json.loads(candidate_backup.read_text())
    if (not isinstance(backup_settings, dict) or set(backup_settings) - {"directory", "keep", "timeout_seconds"}
        or not isinstance(backup_settings.get("directory"), str) or not Path(backup_settings["directory"]).is_absolute()
        or type(backup_settings.get("keep", 14)) is not int or not 1 <= backup_settings.get("keep", 14) <= 3650
        or type(backup_settings.get("timeout_seconds", 120)) is not int or not 1 <= backup_settings.get("timeout_seconds", 120) <= 120):
        raise RuntimeError("Invalid backup configuration; values withheld")
    # The shipped sandbox permits writes only under STATE. Custom destinations
    # require an operator-managed systemd drop-in and are not silently created.
    if not Path(backup_settings["directory"]).resolve().is_relative_to(STATE.resolve()) or Path(backup_settings["directory"]).resolve() == STATE.resolve():
        raise RuntimeError("Backup directory must be under /var/lib/dreamer for this installer")
    candidate_config = CONFIG if CONFIG.exists() else ROOT / "deploy/dreamer.toml"
    metadata = json.loads(run(source, "--config", candidate_config, "status", "--json", capture_output=True, text=True).stdout)
    if Path(metadata["data_dir"]).resolve() != STATE:
        raise RuntimeError("This service installer requires data_dir = /var/lib/dreamer; it will not move existing knowledge automatically")
    if ENV.exists():
        read_credentials()  # Validate before stopping an existing service.
    try:
        account = pwd.getpwnam("dreamer")
    except KeyError:
        group_exists = subprocess.run(["getent", "group", "dreamer"], stdout=subprocess.DEVNULL).returncode == 0
        group_args = ["--gid", "dreamer"] if group_exists else ["--user-group"]
        run("useradd", "--system", *group_args, "--home-dir", STATE, "--shell", "/usr/sbin/nologin", "dreamer")
        account = pwd.getpwnam("dreamer")
    if account.pw_uid == 0:
        raise RuntimeError("Refusing to run Dreamer as root")
    try:
        group = grp.getgrnam("dreamer")
    except KeyError:
        raise RuntimeError("The existing dreamer account needs a dreamer primary group") from None
    if group.gr_gid != account.pw_gid:
        raise RuntimeError("The existing dreamer account's primary group must be dreamer")
    CONFIG.parent.mkdir(mode=0o750, parents=True, exist_ok=True)
    os.chown(CONFIG.parent, 0, account.pw_gid)
    CONFIG.parent.chmod(0o750)
    STATE.mkdir(mode=0o700, parents=True, exist_ok=True)
    os.chown(STATE, account.pw_uid, account.pw_gid)
    STATE.chmod(0o700)
    backup_directory = Path(backup_settings["directory"])
    if backup_directory.is_symlink():
        raise RuntimeError("Backup directory must not be a symlink")
    backup_directory.mkdir(mode=0o700, parents=True, exist_ok=True)
    os.chown(backup_directory, account.pw_uid, account.pw_gid)
    backup_directory.chmod(0o700)
    if not BACKUP_CONFIG.exists():
        atomic_copy(candidate_backup, BACKUP_CONFIG, 0o640)
    os.chown(BACKUP_CONFIG, 0, account.pw_gid)
    BACKUP_CONFIG.chmod(0o640)
    if not CONFIG.exists():
        atomic_copy(candidate_config, CONFIG, 0o640)
    os.chown(CONFIG, 0, account.pw_gid)
    CONFIG.chmod(0o640)
    if not ENV.exists():
        # Generated only on the Pi, once. Reinstallation preserves both secrets.
        with ENV.open("x") as output:
            output.write(f"DREAMER_MCP_TOKEN={secrets.token_hex(32)}\n")
            output.write(f"DREAMER_MCP_WRITE_TOKEN={secrets.token_hex(32)}\n")
    os.chown(ENV, 0, 0)
    ENV.chmod(0o600)
    credentials = read_credentials()
    if run("systemctl", "is-active", "--quiet", SERVICE, check=False).returncode == 0:
        run("systemctl", "stop", SERVICE)
    # Avoid a scheduled copy racing the pre-migration backup and schema upgrade.
    run("systemctl", "stop", "dreamer-backup.timer", check=False)
    run("systemctl", "stop", "dreamer-backup.service", check=False)
    database = STATE / "knowledge.sqlite3"
    if database.exists():
        backups = Path("/var/backups/dreamer")
        backups.mkdir(mode=0o700, parents=True, exist_ok=True)
        os.chown(backups, 0, 0)
        backups.chmod(0o700)
        backup = backups / f"knowledge-{time.time_ns()}.sqlite3"
        staging_backup = STATE / f".backup-{time.time_ns()}.sqlite3"
        # Read as the database owner so any WAL sidecars retain correct ownership.
        backup_code = """import contextlib, pathlib, sqlite3, sys
with contextlib.closing(sqlite3.connect(pathlib.Path(sys.argv[1]).as_uri() + '?mode=ro', uri=True)) as source, contextlib.closing(sqlite3.connect(sys.argv[2])) as destination:
    source.backup(destination)
"""
        try:
            run("runuser", "-u", "dreamer", "--", "/usr/bin/python3", "-c", backup_code, database, staging_backup)
            shutil.move(str(staging_backup), str(backup))
        finally:
            staging_backup.unlink(missing_ok=True)
        os.chown(backup, 0, 0)
        backup.chmod(0o600)
        print(f"Database backup: {backup}")
    created_bin_directory = not BINARY.parent.exists()
    BINARY.parent.mkdir(mode=0o755, parents=True, exist_ok=True)
    if created_bin_directory:
        BINARY.parent.chmod(0o755)
    if BINARY.exists():
        atomic_copy(BINARY, BINARY.with_name("dreamer.previous"), 0o755)
    atomic_copy(source, BINARY, 0o755)
    run("runuser", "-u", "dreamer", "--", BINARY, "--config", CONFIG, "db", "init")
    atomic_copy(ROOT / "deploy/dreamer.service", UNIT, 0o644)
    BACKUP_SCRIPT.parent.mkdir(mode=0o755, parents=True, exist_ok=True)
    BACKUP_SCRIPT.parent.chmod(0o755)
    atomic_copy(ROOT / "scripts/backup.py", BACKUP_SCRIPT, 0o755)
    atomic_copy(ROOT / "deploy/dreamer-backup.service", BACKUP_UNIT, 0o644)
    atomic_copy(ROOT / "deploy/dreamer-backup.timer", BACKUP_TIMER, 0o644)
    run("systemd-analyze", "verify", UNIT, BACKUP_UNIT, BACKUP_TIMER)
    run("systemctl", "daemon-reload")
    run("systemctl", "enable", "--now", SERVICE)
    listen = metadata["http"]["listen"]
    host, port = listen.rsplit(":", 1)
    host = "127.0.0.1" if host == "0.0.0.0" else "[::1]" if host == "[::]" else host
    url = f"http://{host}:{port}/health"
    # Ignore proxy environment variables for the local deployment health check.
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    for _ in range(15):
        try:
            request = urllib.request.Request(url, headers={"Authorization": "Bearer " + credentials["DREAMER_MCP_TOKEN"]})
            with opener.open(request, timeout=1) as response:
                active = run("systemctl", "is-active", "--quiet", SERVICE, check=False).returncode == 0
                if active and response.status == 200 and json.load(response).get("status") == "ok":
                    run("systemctl", "enable", "--now", "dreamer-backup.timer")
                    print("Dreamer installed, enabled at boot, and healthy on port " + port)
                    print("Daily backup timer enabled; run make backup for an immediate checked snapshot")
                    print(f"Credentials retained privately in {ENV}; values not displayed")
                    return
        except (OSError, ValueError):
            pass
        time.sleep(1)
    raise RuntimeError("Service health check failed; inspect sudo journalctl -u dreamer.service. Earlier changes and backups remain available")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.parse_args()
    if os.geteuid() != 0:
        os.execvp("sudo", ["sudo", sys.executable, str(Path(__file__).resolve())])
    os.umask(0o077)
    try:
        install_service()
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"Install failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
