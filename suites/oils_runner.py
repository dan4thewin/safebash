#!/usr/bin/env python3
"""Run oils spec tests through sash, compare to expected stdout."""

import os, subprocess, sys, json, re
from collections import defaultdict

SASH = os.path.expanduser("~/src/safebash/sash/target/release/sash")
SPEC_DIR = os.path.expanduser("~/src/safebash/suites/oils/spec")

# Files to skip entirely (features outside safe subset)
NA_FILES = {
    "alias", "background", "errexit", "errexit-oil", "errexit-osh",
    "eval-trap", "globignore", "history", "interactive",
    "jobs", "nix-idioms", "nocasematch", "process-sub",
    "read-compat", "signal", "sigwinch", "tilde", "trap",
    "trap-err", "type-compat", "ysh-",  # all ysh files
}

# Keywords that make individual tests NA
NA_KEYWORDS = [
    "eval ", "alias ", "shopt ", "set -o", "set +o",
    "BASH_SOURCE", "BASH_VERSION", "BASH_LINENO",
    "BASH_REMATCH", "PIPESTATUS", "FUNCNAME",
    "trap ", "PS1", "PS2", "PS4",
    "declare -p", "declare -f", "declare -F",
    "compgen", "complete", "getopts",
    "printf -v", "LINENO", "SHELLOPTS",
    "source ", ". /", "mapfile", "readarray",
    "coproc", "select ",
]

def is_na_file(basename):
    name = basename.replace(".test.sh", "")
    for naf in NA_FILES:
        if name.startswith(naf) or name == naf:
            return True
    return False

def is_na_test(script):
    for kw in NA_KEYWORDS:
        if kw in script:
            return True
    return False

def parse_tests(filepath):
    """Parse oils spec test format: #### name, code, ## stdout: expected"""
    tests = []
    with open(filepath) as f:
        lines = f.readlines()
    
    i = 0
    while i < len(lines):
        if lines[i].startswith("#### "):
            name = lines[i][5:].strip()
            i += 1
            script_lines = []
            expected = None
            status = None
            skip = False
            
            while i < len(lines) and not lines[i].startswith("#### "):
                line = lines[i]
                if line.startswith("## stdout:"):
                    expected = line[len("## stdout:"):].strip()
                    # Handle multi-line stdout with ## prefix
                    i += 1
                    while i < len(lines) and not lines[i].startswith("#### ") and not lines[i].startswith("## "):
                        expected += "\n" + lines[i].rstrip()
                        i += 1
                    continue
                elif line.startswith("## stdout-json:"):
                    raw = line[len("## stdout-json:"):].strip()
                    try:
                        expected = json.loads(raw)
                    except:
                        expected = raw.strip('"').strip("'")
                    i += 1
                    continue
                elif line.startswith("## BUG") or line.startswith("## OK") or line.startswith("## N-I"):
                    # Skip alternate expectations
                    i += 1
                    continue
                elif line.startswith("## status:"):
                    status = int(line.split(":")[1].strip())
                    i += 1
                    continue
                elif line.startswith("## "):
                    i += 1
                    continue
                else:
                    script_lines.append(line)
                    i += 1
            
            script = "".join(script_lines).strip()
            if script and expected is not None:
                tests.append({"name": name, "script": script, "expected": expected, "status": status})
        else:
            i += 1
    return tests

def run_test(test, timeout=10):
    script = test["script"]
    expected = test["expected"]
    
    if is_na_test(script):
        return "NA", "", ""
    
    # Compile with sash
    tmp_sh = "/tmp/oils_test.sh"
    tmp_bin = "/tmp/oils_test_bin"
    with open(tmp_sh, "w") as f:
        f.write(script + "\n")
    
    try:
        comp = subprocess.run(
            [SASH, tmp_sh, "--nolint", "-o", tmp_bin],
            capture_output=True, text=True, timeout=timeout
        )
    except subprocess.TimeoutExpired:
        return "TIMEOUT", "", ""
    
    if comp.returncode != 0:
        stderr = comp.stderr
        if "taint" in stderr.lower():
            return "FAIL_COMPILE_TAINT", "", stderr
        elif "indirect" in stderr.lower():
            return "FAIL_COMPILE_INDIRECT", "", stderr
        else:
            return "FAIL_COMPILE", "", stderr
    
    # Run compiled binary
    try:
        run = subprocess.run(
            [tmp_bin], capture_output=True, text=True, timeout=timeout
        )
    except subprocess.TimeoutExpired:
        return "TIMEOUT", "", ""
    
    got = run.stdout.rstrip("\n")
    exp = expected.rstrip("\n")
    
    if got == exp:
        return "PASS", got, exp
    else:
        return "FAIL_OUTPUT", got, exp

def main():
    totals = defaultdict(int)
    file_results = {}
    
    for fn in sorted(os.listdir(SPEC_DIR)):
        if not fn.endswith(".test.sh"):
            continue
        if is_na_file(fn):
            tests_in_file = parse_tests(os.path.join(SPEC_DIR, fn))
            totals["NA"] += max(len(tests_in_file), 1)
            print(f"NA_FILE  {fn}")
            continue
        
        tests = parse_tests(os.path.join(SPEC_DIR, fn))
        if not tests:
            continue
        
        counts = defaultdict(int)
        for t in tests:
            status, got, exp = run_test(t)
            counts[status] += 1
            totals[status] += 1
        
        p, f, c, n = counts["PASS"], counts["FAIL_OUTPUT"], counts.get("FAIL_COMPILE",0)+counts.get("FAIL_COMPILE_TAINT",0)+counts.get("FAIL_COMPILE_INDIRECT",0), counts["NA"]
        if p + f + c > 0:
            print(f"{fn:50s} P:{p:3d} F:{f:3d} C:{c:3d} NA:{n:3d}")
        file_results[fn] = counts
    
    total = sum(totals.values())
    print(f"\n{'='*60}")
    print(f"TOTALS: {total} cases")
    for k in sorted(totals.keys()):
        print(f"  {k:20s}: {totals[k]}")

if __name__ == "__main__":
    main()
