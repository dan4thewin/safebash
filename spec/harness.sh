#!/bin/bash
# spec/harness.sh — the spec harness's own hang guard.
#
# This file tests common.sh, not the compiler. It earns its place because a
# hang is the one failure mode the suite cannot report: `bash run.sh` stops
# producing output entirely and every later spec goes unrun. That is not
# hypothetical — the DD#14 pipeline-stage redirect bug (spec 14.32) made
# `cat < f | cat` read the runner's own stdin, and the suite sat there
# forever with no indication of which spec was responsible.
#
# So every child the harness spawns gets `</dev/null` and a `timeout`. If
# either guard is ever dropped these three checks fail instead of hanging.
set -eu
. common.sh
setup harness
plan=3
echo "1..$plan" >&3
set +e

# ht.1 a program that reads stdin sees EOF, not the runner's stdin. Without
# the guard this blocks whenever run.sh is invoked from a terminal.
if in_modes native,c; then
    run_sash 'readline x
attest x [[ $x = "" ]]
echo "got=[$x]"'
    [[ $rc = 0 && $out = "got=[]" ]]; chk ht.1
else
    skip_chk ht.1
fi

# ht.2 a program that never exits is killed at SPEC_TIMEOUT and reported as
# such. `timeout` exits 124; the message names the guard so the failure is
# not mistaken for an ordinary bad exit status.
if in_modes native,c; then
    SPEC_TIMEOUT=2 run_sash '/bin/sleep 30'
    [[ $rc = 124 && $rerr == *"hang guard"* ]]; chk ht.2
else
    skip_chk ht.2
fi

# ht.3 …and the suite carries on afterwards, which is the whole point: one
# runaway spec must cost its timeout, not the rest of the run.
if in_modes native,c; then
    run_sash 'echo alive'
    [[ $rc = 0 && $out = "alive" ]]; chk ht.3
else
    skip_chk ht.3
fi

set -e
tally
