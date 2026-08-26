#!/bin/bash
# spec/taint_switch.sh — DD#82 taint bypass switches: --no-taint (full) and
# --no-taint-cst (CST off, SSA backstop on, native only). Compile flags are
# injected via SASH_CFLAGS (common.sh).
set -eu
. common.sh
setup taint_switch
plan=7
echo "1..$plan" >&3

set +e

# sw.1 control: with both layers on, a tainted read is rejected (SB5020)
run_sash 'echo $(getenv HOME)'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk sw.1

# ── --no-taint: full bypass (CST + SSA) ──
export SASH_CFLAGS=--no-taint
# sw.6 the switch disables TAINT only — a language rule ($@, SB5045) still fires
run_sash 'echo "$@"'
[[ $rc != 0 && -z $out && $err == *SB5045* ]]; chk sw.6
# sw.7 a clean program is unaffected by the switch
run_sash 'echo hello'
[[ $rc = 0 && $out = hello && -z $err ]]; chk sw.7
if in_modes native,c; then
    # sw.2 a tainted scalar compiles & runs (leaks) under full bypass
    run_sash 'x=$(arg 1); echo $x' hello
    [[ $rc = 0 && $out = hello && -z $err ]]; chk sw.2
    # sw.3 a tainted join compiles & runs under full bypass
    run_sash 'argv 1 a; echo $(join a)' one two
    [[ $rc = 0 && $out = "one two" && -z $err ]]; chk sw.3
else
    skip_chk sw.2
    skip_chk sw.3
fi
unset SASH_CFLAGS

# ── --no-taint-cst: CST off, SSA backstop remains (native only) ──
export SASH_CFLAGS=--no-taint-cst
if in_modes native; then
    # sw.4 the SSA backstop still rejects an assigned tainted source
    run_sash 'x=$(getenv HOME); echo $x'
    [[ $rc != 0 && -z $out ]]; chk sw.4
    # sw.5 the SSA backstop catches a value propagated through echo
    run_sash 'y=$(getenv HOME); x=$(echo $y); echo $x'
    [[ $rc != 0 && -z $out ]]; chk sw.5
else
    skip_chk sw.4
    skip_chk sw.5
fi
unset SASH_CFLAGS

set -e
tally
