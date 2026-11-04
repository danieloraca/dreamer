#!/usr/bin/env python3
"""Install the on-demand Mac MCP tools and scoped Codex memory guidance.

No daemon, scheduled task, GitHub changes, model API or credential copying.
"""
import argparse
import json
import os
from pathlib import Path
import stat
import shutil
import subprocess
import sys
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
START = '<!-- dreamer-memory:start -->'
END = '<!-- dreamer-memory:end -->'
MARKER = '<!-- managed by Dreamer Mac -->'
CONFIG_START = '# BEGIN Dreamer Mac managed MCP'
CONFIG_END = '# END Dreamer Mac managed MCP'


def atomic(path, content, mode=0o600):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(prefix='.dreamer-', dir=path.parent)
    try:
        with os.fdopen(fd, 'wb') as f:
            f.write(content if isinstance(content, bytes) else content.encode())
        os.chmod(name, mode)
        os.replace(name, path)
    finally:
        if os.path.exists(name):
            os.unlink(name)


def replace_section(text, start, end, section):
    if start in text or end in text:
        if text.count(start) != 1 or text.count(end) != 1:
            raise RuntimeError('Ambiguous managed section; existing configuration preserved.')
        before, tail = text.split(start, 1)
        _, after = tail.split(end, 1)
        return before + section + after
    return text.rstrip() + '\n\n' + section + '\n'


def install(home, binary, token_file):
    home, binary, token_file = Path(home), Path(binary).resolve(), Path(token_file).resolve()
    metadata = token_file.stat()
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_mode & 0o077:
        raise RuntimeError('Existing writer credential file must be private (0600).')
    gh = shutil.which("gh")
    if not gh:
        raise RuntimeError("GitHub CLI must be installed and available on this Mac.")
    subprocess.run([str(binary), 'mac', '--help'], check=True, capture_output=True)
    shared = home / '.local/share/dreamer'
    config_root = home / '.config/dreamer'
    installed = shared / 'dreamer-mac'
    settings = config_root / 'mac.toml'
    skill = home / '.codex/skills/dreamer-memory/SKILL.md'
    agents = home / '.codex/AGENTS.md'
    codex = home / '.codex/config.toml'
    if any(path.is_symlink() for path in [codex, agents, skill, settings]):
        raise RuntimeError('Managed install targets must be real files, not symlinks.')
    existing = codex.read_text() if codex.exists() else ''
    parsed = tomllib.loads(existing)
    if 'dreamer_mac' in parsed.get('mcp_servers', {}) and CONFIG_START not in existing:
        raise RuntimeError('An unmanaged dreamer_mac MCP entry exists; nothing changed.')
    if skill.exists() and MARKER not in skill.read_text():
        raise RuntimeError('An unmanaged dreamer-memory skill exists; nothing changed.')
    settings_text = ('# Managed by Dreamer Mac. Credentials remain in their existing private file.\n'
        + 'repository = "geckolabs/Gecko-API"\n'
        + 'repositories = ["geckolabs/Gecko-API"]\n'
        + 'endpoint = "http://192.168.0.25:3445/mcp"\n'
        + 'writer_token_file = ' + json.dumps(str(token_file)) + '\n'
        + 'cache_dir = ' + json.dumps(str(shared / 'pr-cache')) + '\n')
    if settings.exists():
        current = tomllib.loads(settings.read_text())
        # Preserve the user's configured repository, endpoint and credential/cache paths.
        if not {'repository', 'endpoint', 'writer_token_file', 'cache_dir'} <= set(current) or set(current) - {'repository', 'repositories', 'endpoint', 'writer_token_file', 'cache_dir'}:
            raise RuntimeError('Existing Mac settings need inspection; nothing changed.')
        settings_text = settings.read_text()
        if 'repositories' not in current:
            settings_text = settings_text.rstrip() + '\nrepositories = ' + json.dumps([current['repository']]) + '\n'
    table = '\n'.join([CONFIG_START, '[mcp_servers.dreamer_mac]',
        'command = ' + json.dumps(str(installed)),
        'args = ' + json.dumps(['mac', '--settings', str(settings), 'mcp']),
        'startup_timeout_sec = 20', 'tool_timeout_sec = 180', '[mcp_servers.dreamer_mac.env]',
        'PATH = ' + json.dumps(str(Path(gh).parent) + os.pathsep + os.defpath), CONFIG_END])
    updated = replace_section(existing, CONFIG_START, CONFIG_END, table)
    tomllib.loads(updated)
    guidance = ('\n'.join([START, '## Dreamer knowledge', '',
        'For planning, implementing, debugging or reviewing a configured Dreamer repository (choose from the configured allowlist), use '
        + str(skill) + '. Read applicable repository instructions and required docs first, then recall knowledge before making code decisions, even when I do not ask. Project guidance governs over memory. '
        'After reading a relevant approved/merged PR for my task, automatically save supported reusable lessons. '
        'No individual lesson approval is required. Keep scope to the repositories and PRs involved in my work; '
        'do not crawl unrelated sources or modify GitHub as part of learning. If memory is unavailable, say so and continue with current source and tests.', END]))
    old_agents = agents.read_text() if agents.exists() else ''
    updated_agents = replace_section(old_agents, START, END, guidance)
    skill_text = (ROOT / 'skills/dreamer-memory/SKILL.md').read_text()
    if MARKER not in skill_text:
        raise RuntimeError('Packaged skill has no managed marker; nothing changed.')
    # All preflight checks are complete before making changes.
    for directory in [shared, config_root]:
        if not directory.exists():
            directory.mkdir(parents=True, mode=0o700)
        elif directory.is_symlink() or not directory.is_dir():
            raise RuntimeError('Mac install target must be a real directory.')
    atomic(installed, binary.read_bytes(), 0o700)
    atomic(settings, settings_text)
    atomic(skill, skill_text)
    atomic(agents, updated_agents)
    atomic(codex, updated)
    print('Installed Dreamer Mac MCP tools and repository-scoped Codex memory guidance.')
    print('Existing Pi MCP entry, other Codex settings and writer credential file preserved.')
    print('Start a new Codex task to load the tools and guidance; no background daemon installed.')
    return installed, settings


def main():
    if sys.platform != 'darwin':
        raise SystemExit('This installer is for the Mac; use make deploy for the Pi.')
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path, default=ROOT / 'target/release/dreamer')
    p.add_argument('--writer-token-file', type=Path, default=ROOT / '.env.writer')
    args = p.parse_args()
    try:
        install(Path.home(), args.binary, args.writer_token_file)
    except (OSError, RuntimeError, ValueError, subprocess.SubprocessError):
        raise SystemExit('Mac installation stopped; check the binary, private writer file and existing managed configuration. No credentials displayed.') from None


if __name__ == '__main__':
    main()
