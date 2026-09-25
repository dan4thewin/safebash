#!/bin/bash
# spec/taint_switch.sh — DD#82 taint bypass switches: --no-taint (full) and
# --no-taint-cst (CST off, SSA backstop on, native only). Compile flags are
# injected via SASH_CFLAGS (common.sh).
set -eu
. common.sh
setup taint_switch
plan=11
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
# sw.2 a tainted scalar compiles & runs (leaks) under full bypass
run_sash 'x=$(arg 1); echo $x' hello
[[ $rc = 0 && $out = hello && -z $err ]]; chk sw.2
# sw.3 a tainted join compiles & runs under full bypass
run_sash 'argv 1 a; echo $(join a)' one two
[[ $rc = 0 && $out = "one two" && -z $err ]]; chk sw.3
unset SASH_CFLAGS

# ── --no-taint-cst: CST off, SSA backstop remains (native only) ──
export SASH_CFLAGS=--no-taint-cst
# sw.4 the SSA backstop still rejects an assigned tainted source
run_sash 'x=$(getenv HOME); echo $x'
[[ $rc != 0 && -z $out ]]; chk sw.4
# sw.5 the SSA backstop catches a value propagated through echo
run_sash 'y=$(getenv HOME); x=$(echo $y); echo $x'
[[ $rc != 0 && -z $out ]]; chk sw.5
# sw.8 the SSA backstop catches an operand propagated through a transform
# builtin ($(toupper …)) — cmd_subst_taint propagates operand taint (DD#82)
run_sash 'y=$(getenv HOME); x=$(toupper $y); echo $x'
[[ $rc != 0 && -z $out ]]; chk sw.8
# sw.9 the SSA backstop rejects a pipeline laundered through a leading
# builtin (`echo ig | /bin/cat f | read x`). It could not: `clean_pipe` was
# hardcoded `true` for every lastpipe read, so the backstop was inert for
# this shape and the front-end pass was the only layer — and it missed it.
run_sash 'echo ig | /bin/cat /etc/hostname | readline x
echo "$x"'
[[ $rc != 0 && -z $out ]]; chk sw.9
# sw.10 the SSA backstop rejects a tainted ARRAY at a subscript read. This is
# the one tainted-array origin the backstop classifies: the array literal is
# an `Assign` whose value carries the cmdsub's taint. (The bulk-read origins
# DD#61 names — `read`/`mapfile`/`argv` fills — define no SSA VarRef and stay
# CST-only; see notes/tasks/ssa-taint-full-backstop.md, deferred by DG
# 2026-09-12.) Assert the message so this cannot pass on the scalar backstop.
run_sash 'array a=($(getenv HOME))
echo ${a[0]}'
[[ $rc != 0 && -z $out && $err == *"tainted array a[...]"* ]]; chk sw.10
# sw.11 control: a CLEAN array subscript is not false-positived by the
# backstop with the CST pass off — the reject above is about taint, not about
# arrays being unclassifiable at SSA.
run_sash 'array a=(x y)
echo ${a[0]}'
[[ $rc = 0 && $out = x && -z $err ]]; chk sw.11
unset SASH_CFLAGS

set -e
tally
