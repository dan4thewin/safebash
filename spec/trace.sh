#!/bin/bash
# spec/trace.sh — DD#32 branch/loop runtime tracing.
#
# `--trace` (compile flag, injected via SASH_CFLAGS) makes native codegen emit
# the trace-write code — codegen/entry.rs `emit_branch_outcome_trace` +
# codegen/setup.rs `intern_trace_terminator`. At runtime, `SASH_TRACE=1` sets
# the global `sash_trace_flag`, so each branch/loop terminator writes
# `+ <kind> \u2192 <label>` to stderr (captured here as `$rerr`).
#
# Native-only: tracing is a native-backend feature; `--trace` is a no-op for
# the c emitter, so this suite exits there (like convert.sh) and its plan is
# counted in native alone.
set -eu
. common.sh
setup trace
[[ ${SASH_MODE:-native} != native ]] && exit 0
plan=5
echo "1..$plan" >&3

set +e

export SASH_CFLAGS=--trace
export SASH_TRACE=1

# tr.1 the taken (true) branch of an `if` traces "+ branch \u2192 if.then"
run_sash 'int x=5; if (( x > 0 )); then echo yes; fi'
[[ $rc = 0 && $out = yes && $rerr == *"if.then"* ]]; chk tr.1
# tr.2 the else (false) branch traces "+ branch \u2192 if.else" (the other
# select target in emit_branch_outcome_trace)
run_sash 'int x=-5; if (( x > 0 )); then echo yes; else echo no; fi'
[[ $rc = 0 && $out = no && $rerr == *"if.else"* ]]; chk tr.2
# tr.3 a `while` loop traces the body on each iteration and the exit when the
# condition fails — the `loop` kind, both terminator targets
run_sash 'int i=0; while (( i < 2 )); do echo $i; (( i++ )); done'
[[ $rc = 0 && $out = $'0\n1' && $rerr == *"while.body"* && $rerr == *"while.exit"* ]]; chk tr.3

# tr.4 a background job traces both sides of its fork, and the child's own
# branches (batch-531 port, F2: the child leaves by its own exit, not the
# parent's code)
run_sash 'fn g { if [[ $1 == x ]] { echo in } }
g x &
- wait
echo after'
[[ $rc = 0 && $out = $'in\nafter' && $rerr == *"bg.parent"* \
   && $rerr == *"bg.child"* && $rerr == *"if.then"* ]]; chk tr.4

# tr.5 a `[[ ]]` junction is a branch of its own (DD#95, journal 550): a
# decided left operand jumps to the junction's merge, an undecided one to its
# right operand
run_sash 'x=a
if [[ $x == a || $x == b ]] { echo one }
if [[ $x == b || $x == a ]] { echo two }
[[ $x == c && $x == d ]] || echo three'
[[ $rc = 0 && $out = $'one\ntwo\nthree' && $rerr == *"test.or.merge"* \
   && $rerr == *"test.or.right"* && $rerr == *"test.and.merge"* \
   && $rerr != *"test.and.right"* ]]; chk tr.5

unset SASH_CFLAGS SASH_TRACE

set -e
tally
