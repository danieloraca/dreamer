"""Measure one ephemeral, read-only Codex review; retain no raw event transcript.

The prompt and source identities belong in the trial's frozen manifest. Results
are usage observations, not an automatic quality score or a claim of savings.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time
import tomllib


def save(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    path.write_text(json.dumps(value, indent=2) + '\n')
    path.chmod(0o600)


def usage_counts(usage):
    """Keep unavailable counters unknown and reject invalid subset counts."""
    if not isinstance(usage, dict):
        return None
    values = {}
    for key in ['input_tokens', 'cached_input_tokens', 'output_tokens']:
        n = usage.get(key)
        if isinstance(n, bool) or not isinstance(n, int) or n < 0:
            return None
        values[key] = n
    if values['cached_input_tokens'] > values['input_tokens']:
        return None
    values['uncached_input_tokens'] = values['input_tokens'] - values['cached_input_tokens']
    values['input_plus_output_tokens'] = values['input_tokens'] + values['output_tokens']
    # Preserve additional counters without double-counting reasoning or cache.
    values['reported'] = usage
    return values


def summarize_events(lines, returncode, elapsed, timed_out=False):
    completions, failures, commands, outputs, answer = [], 0, 0, 0, None
    for line in lines:
        try:
            event = json.loads(line)
        except (json.JSONDecodeError, TypeError):
            continue
        kind = event.get('type')
        if kind == 'turn.completed':
            completions.append(event.get('usage'))
        elif kind in ['turn.failed', 'error']:
            failures += 1
        elif kind == 'item.completed':
            item = event.get('item', {})
            if item.get('type') == 'command_execution':
                commands += 1
                outputs += len(item.get('aggregated_output', '').encode())
            elif item.get('type') == 'agent_message':
                answer = item.get('text')
    usage = usage_counts(completions[0]) if len(completions) == 1 else None
    complete = returncode == 0 and not timed_out and not failures and len(completions) == 1 and bool(answer)
    return {
        'review_complete': complete,
        'usage_available': usage is not None,
        'comparable': complete and usage is not None,
        'returncode': returncode,
        'timed_out': timed_out,
        'wall_seconds': elapsed,
        'turn_completed_events': len(completions),
        'failure_events': failures,
        'usage': usage,
        'command_count': commands,
        'command_output_bytes': outputs,
        'answer': answer,
    }


def run(prompt_file, directory, timeout=240, schema=None):
    prompt_file, directory = Path(prompt_file).resolve(), Path(directory).resolve()
    if timeout <= 0:
        raise ValueError('timeout must be positive')
    directory.mkdir(parents=True, mode=0o700, exist_ok=False)
    config = tomllib.loads((Path.home() / '.codex/config.toml').read_text())
    model, effort = config['model'], config['model_reasoning_effort']
    prompt = prompt_file.read_text()
    args = ['codex', 'exec', '--ephemeral', '--ignore-user-config', '--strict-config',
            '--sandbox', 'read-only', '--skip-git-repo-check', '--json',
            '-c', 'project_doc_max_bytes=0', '-c', 'features.memories=false',
            '-c', 'model=' + json.dumps(model),
            '-c', 'model_reasoning_effort=' + json.dumps(effort)]
    if schema:
        args.extend(['--output-schema', str(Path(schema).resolve())])
    args.append('-')
    started = time.time()
    begin = time.monotonic()
    timed_out = False
    # Temporary stdout is unlinked immediately and never copied to reports. Raw
    # reasoning, command arguments, and stderr are deliberately not retained.
    with tempfile.TemporaryFile(mode='w+t') as output, tempfile.TemporaryFile(mode='w+t') as diagnostic:
        process = subprocess.Popen(args, cwd=prompt_file.parent, stdin=subprocess.PIPE,
                                   stdout=output, stderr=diagnostic, text=True,
                                   start_new_session=True)
        try:
            process.communicate(prompt, timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
        elapsed = time.monotonic() - begin
        output.seek(0)
        result = summarize_events(output, process.returncode, elapsed, timed_out)
    answer = result.pop('answer')
    if answer:
        (directory / 'answer.txt').write_text(answer)
        (directory / 'answer.txt').chmod(0o600)
        result['answer_sha256'] = hashlib.sha256(answer.encode()).hexdigest()
        try:
            save(directory / 'answer.json', json.loads(answer))
        except json.JSONDecodeError:
            result['structured_answer_valid'] = False
            result['comparable'] = False
        else:
            result['structured_answer_valid'] = True
    result.update({'started_at_unix': started, 'model': model, 'reasoning_effort': effort,
                   'prompt_sha256': hashlib.sha256(prompt.encode()).hexdigest(),
                   'prompt_bytes': len(prompt.encode()), 'timeout_seconds': timeout,
                   'raw_transcript_retained': False, 'runner': 'codex exec --ephemeral --sandbox read-only'})
    save(directory / 'metrics.json', result)
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prompt', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--timeout', type=int, default=240)
    parser.add_argument('--schema', type=Path, required=True)
    options = parser.parse_args()
    metrics = run(options.prompt, options.output, options.timeout, options.schema)
    print(json.dumps(metrics))
    raise SystemExit(0 if metrics['comparable'] else 1)
