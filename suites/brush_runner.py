#!/usr/bin/env python3
"""Run brush test cases through sash, compare to bash. Output structured results."""

import yaml
import subprocess
import tempfile
import os
import sys
import json
import glob
import time

SASH = os.path.expanduser("~/src/safebash/sash/target/release/sash")
RT = os.path.expanduser("~/src/safebash/sash-rt/target/release/libsash_rt.a")
CASES_DIR = os.path.expanduser("~/src/brush/brush-shell/tests/cases")
TIMEOUT = 10

# Features sash intentionally doesn't support (mark NA)
NA_KEYWORDS = [
    'eval ', 'alias ', 'unalias ', 'compgen', 'complete ', 'compopt',
    'getopts', 'set -', 'set +', 'shopt ', 'enable ', 'hash ',
    'help ', 'jobs ', 'kill ', 'times', 'type ', 'typeset ',
    'disown', 'suspend', 'ulimit', 'umask',
    'select ', 'coproc ',
    'source ', '. /',
    'BASH_SOURCE', 'BASH_LINENO', 'BASH_REMATCH', 'FUNCNAME',
    'BASH_VERSINFO', 'BASH_VERSION', 'BASH_SUBSHELL',
    'declare -p', 'declare -f', 'declare -F',
    'local -', 'export -',
    'history', 'fc ',
    'bind ', 'builtin ',
    'PS1', 'PS2', 'PS3', 'PS4', 'PROMPT_COMMAND',
    'PIPESTATUS', 'trap -p', 'command -V',
    'exec -a', 'exec -c', 'printf -v',
    'LINENO', 'SHELLOPTS', 'GROUPS', 'SRANDOM',
]

# Files that are entirely NA (interactive, completion, etc.)
NA_FILES = {
    'interactive.yaml', 'prompt.yaml', 'options.yaml',
    'builtins/alias.yaml', 'builtins/compgen.yaml', 'builtins/complete.yaml',
    'builtins/eval.yaml', 'builtins/getopts.yaml', 'builtins/hash.yaml',
    'builtins/help.yaml', 'builtins/jobs.yaml', 'builtins/kill.yaml',
    'builtins/set.yaml', 'builtins/shopt.yaml', 'builtins/enable.yaml',
    'builtins/times.yaml', 'builtins/type.yaml', 'builtins/unalias.yaml',
    'builtins/dot.yaml', 'builtins/pushd_popd_dirs.yaml',
    'builtins/ps4.yaml',
    'patterns/filename_expansion.yaml',  # globbing is complex
}

def is_na(stdin_text):
    """Check if a test uses features sash intentionally doesn't support."""
    if not stdin_text:
        return True
    for kw in NA_KEYWORDS:
        if kw in stdin_text:
            return True
    return False

def run_bash(script, env=None):
    """Run script in bash, return (stdout, stderr, exit_code)."""
    try:
        r = subprocess.run(['bash', '-c', script], capture_output=True, timeout=TIMEOUT,
                          env={**(env or os.environ)})
        return r.stdout.decode('utf-8', errors='replace'), r.stderr.decode('utf-8', errors='replace'), r.returncode
    except subprocess.TimeoutExpired:
        return '', 'TIMEOUT', -1

def run_sash(script):
    """Compile with sash, run the binary. Return (stdout, stderr, exit_code, compile_err)."""
    with tempfile.NamedTemporaryFile(mode='w', suffix='.sh', delete=False) as f:
        f.write(script)
        f.flush()
        src = f.name

    out_bin = src + '.bin'
    try:
        # Compile
        r = subprocess.run([SASH, src, '--nolint', '-o', out_bin],
                          capture_output=True, timeout=30)
        if r.returncode != 0:
            compile_err = r.stderr.decode('utf-8', errors='replace')
            return '', '', r.returncode, compile_err

        # Run
        try:
            r = subprocess.run([out_bin], capture_output=True, timeout=TIMEOUT)
            return (r.stdout.decode('utf-8', errors='replace'),
                    r.stderr.decode('utf-8', errors='replace'),
                    r.returncode, '')
        except subprocess.TimeoutExpired:
            return '', 'TIMEOUT', -1, ''
    finally:
        for p in [src, out_bin, src.replace('.sh', '.o')]:
            try: os.unlink(p)
            except: pass

def collect_test_files():
    """Find all yaml test files."""
    files = []
    for root, dirs, fnames in os.walk(CASES_DIR):
        for fn in sorted(fnames):
            if fn.endswith('.yaml'):
                full = os.path.join(root, fn)
                rel = os.path.relpath(full, CASES_DIR)
                files.append((rel, full))
    return sorted(files)

def main():
    results = {}
    totals = {'PASS': 0, 'FAIL_OUTPUT': 0, 'FAIL_COMPILE': 0, 'FAIL_EXIT': 0,
              'NA': 0, 'SKIP': 0, 'TIMEOUT': 0}

    test_files = collect_test_files()
    total_cases = 0

    for rel, full in test_files:
        if rel in NA_FILES:
            with open(full) as f:
                data = yaml.safe_load(f)
            n = len(data.get('cases', []))
            totals['NA'] += n
            total_cases += n
            results[rel] = {'status': 'NA_FILE', 'count': n}
            print(f"NA_FILE  {rel} ({n} cases)", flush=True)
            continue

        with open(full) as f:
            data = yaml.safe_load(f)

        cases = data.get('cases', [])
        file_results = []

        for i, case in enumerate(cases):
            total_cases += 1
            name = case.get('name', f'case_{i}')
            stdin_text = case.get('stdin', '')
            args = case.get('args', [])
            test_files_list = case.get('test_files', [])
            expected_exit = case.get('expected_exit_code')

            # Skip cases requiring test_files or args (need file setup)
            if test_files_list or args:
                totals['SKIP'] += 1
                file_results.append({'name': name, 'status': 'SKIP', 'reason': 'test_files/args'})
                continue

            if not stdin_text or not stdin_text.strip():
                totals['SKIP'] += 1
                file_results.append({'name': name, 'status': 'SKIP', 'reason': 'no stdin'})
                continue

            if is_na(stdin_text):
                totals['NA'] += 1
                file_results.append({'name': name, 'status': 'NA'})
                continue

            # Run bash
            bash_out, bash_err, bash_exit = run_bash(stdin_text)
            if bash_exit == -1 and bash_err == 'TIMEOUT':
                totals['TIMEOUT'] += 1
                file_results.append({'name': name, 'status': 'TIMEOUT', 'phase': 'bash'})
                continue

            # Run sash
            sash_out, sash_err, sash_exit, compile_err = run_sash(stdin_text)

            if compile_err:
                # Categorize compile error
                reason = 'unknown'
                if 'taint' in compile_err.lower() or 'SB4020' in compile_err:
                    reason = 'taint'
                elif 'SB4003' in compile_err or 'indirect' in compile_err.lower():
                    reason = 'indirect'
                elif 'SB4002' in compile_err or 'source' in compile_err.lower():
                    reason = 'source'
                elif 'SB4004' in compile_err or 'deferred' in compile_err.lower() or 'dynamic' in compile_err.lower():
                    reason = 'deferred'
                elif 'panic' in compile_err.lower() or 'thread' in compile_err.lower():
                    reason = 'panic'
                elif 'timeout' in compile_err.lower():
                    reason = 'timeout'
                totals['FAIL_COMPILE'] += 1
                file_results.append({
                    'name': name, 'status': 'FAIL_COMPILE', 'reason': reason,
                    'error': compile_err[:200]
                })
                continue

            if sash_exit == -1 and sash_err == 'TIMEOUT':
                totals['TIMEOUT'] += 1
                file_results.append({'name': name, 'status': 'TIMEOUT', 'phase': 'sash_run'})
                continue

            if sash_out == bash_out:
                if expected_exit is not None and sash_exit != expected_exit:
                    totals['FAIL_EXIT'] += 1
                    file_results.append({
                        'name': name, 'status': 'FAIL_EXIT',
                        'expected': expected_exit, 'got': sash_exit
                    })
                else:
                    totals['PASS'] += 1
                    file_results.append({'name': name, 'status': 'PASS'})
            else:
                totals['FAIL_OUTPUT'] += 1
                file_results.append({
                    'name': name, 'status': 'FAIL_OUTPUT',
                    'bash': bash_out[:300], 'sash': sash_out[:300],
                    'script': stdin_text[:200]
                })

        results[rel] = file_results

        # Print summary per file
        statuses = [r['status'] for r in file_results]
        pass_n = statuses.count('PASS')
        fail_n = statuses.count('FAIL_OUTPUT') + statuses.count('FAIL_EXIT')
        comp_n = statuses.count('FAIL_COMPILE')
        na_n = statuses.count('NA') + statuses.count('SKIP')
        print(f"{rel:50s}  P:{pass_n} F:{fail_n} C:{comp_n} NA:{na_n}", flush=True)

    print(f"\n{'='*60}")
    print(f"TOTALS: {total_cases} cases")
    for k, v in sorted(totals.items()):
        print(f"  {k:15s}: {v}")

    # Save detailed results
    with open('/tmp/brush_results.json', 'w') as f:
        json.dump(results, f, indent=2)

    # Save failures for analysis
    with open('/tmp/brush_failures.txt', 'w') as f:
        for rel, file_res in sorted(results.items()):
            if isinstance(file_res, dict) and file_res.get('status') == 'NA_FILE':
                continue
            for r in file_res:
                if r['status'] in ('FAIL_OUTPUT', 'FAIL_EXIT', 'FAIL_COMPILE'):
                    f.write(f"\n{'='*60}\n")
                    f.write(f"FILE: {rel}\n")
                    f.write(f"TEST: {r['name']}\n")
                    f.write(f"STATUS: {r['status']}\n")
                    if 'reason' in r:
                        f.write(f"REASON: {r['reason']}\n")
                    if 'error' in r:
                        f.write(f"ERROR: {r['error']}\n")
                    if 'bash' in r:
                        f.write(f"BASH: {r['bash']}\n")
                    if 'sash' in r:
                        f.write(f"SASH: {r['sash']}\n")
                    if 'script' in r:
                        f.write(f"SCRIPT: {r['script']}\n")

if __name__ == '__main__':
    main()
