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
# the c and janet emitters, so this suite exits for those modes (like
# convert.sh) and its plan is counted in native alone.
set -eu
. common.sh
setup trace
[[ ${SASH_MODE:-native} != native ]] && exit 0
plan=3
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

unset SASH_CFLAGS SASH_TRACE

set -e
tally
