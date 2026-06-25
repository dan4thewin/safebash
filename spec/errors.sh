#!/bin/bash
# spec/errors.sh — error model tests (DD#21, DD#25, DD#55)
set -eu
. common.sh
setup errors
plan=42
echo "1..$plan" >&3

set +e

# --- DD#0 (implicit): bash control-flow constructs (elif, etc.)
# must apply compile-time analyses uniformly. Walker bugs that
# silently skip elif clauses caused either spurious diagnostics or
# silent acceptance of invalid code. Regressions for journal 057.

# 0.5 collect_returns_inner must walk elif branches when deciding if
# a -> int function is non-void. Without this, only `return 0` in
# the `then` branch is seen and SB4017 fires spuriously even though
# the elif returns a meaningful status.
run_sash 'fn f -> int {
    if /bin/false; then
        return 0
    elif /bin/true; then
        return 5
    fi
}
int x=$((f))
echo "$x"'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk 0.5

# 0.6 scan_expr_positionals_stmt must walk elif branches when
# computing the highest positional read inside a function body.
# Without this, `$5` in an elif branch is invisible and the
# caller-argc check (SB4017-like) is bypassed.
run_sash 'fn f {
    if (($# == 0)); then
        :
    elif /bin/true; then
        /bin/echo $5
    fi
}
f a b c'
[[ $rc != 0 && -z $out && -n $err ]]; chk 0.6

# 21.1 - cmd suppresses exit status
run_sash '- /bin/false; echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 21.1

# 21.2 per-segment pipeline suppression
run_sash '- /bin/false | echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 21.2

# 21.3 unchecked pipeline segment flagged (pipefail always on)
run_sash '/bin/false | echo ok'
[[ $rc != 0 && -z $err ]]; chk 21.3

# 25.1 integer overflow traps at runtime. The janet backend uses float/bignum
# arithmetic with no i64 overflow trap (it previously "passed" only because an
# unrelated codegen crash aborted the program); native/c are the reference.
# See bug-janet-int-overflow.
if in_modes native,c; then
    run_sash 'int x=9223372036854775807; (( x += 1 )); echo $x'
    [[ $rc != 0 && -z $err ]]; chk 25.1
else
    skip_chk 25.1
fi

# 55.1 external cmd nonzero aborts
run_sash '/bin/false
echo should_not_reach'
[[ $rc != 0 && $out != *"should_not_reach"* && -z $err ]]; chk 55.1

# 55.2 variable assignment no exit check
run_sash 'x=hello; echo $x'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 55.2

# 55.3 arithmetic no exit check
run_sash 'int x=5; (( x += 0 )); echo $x'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk 55.3

# 55.4 - cmd suppresses exit check
run_sash '- /bin/false; echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 55.4

# 55.5 $? after - cmd → compile error
run_sash '- /bin/false; echo $?'
[[ $rc != 0 && -z $out && -n $err ]]; chk 55.5

# 55.6 $? only in handling position
run_sash 'x=$?'
[[ $rc != 0 && -z $out && -n $err ]]; chk 55.6

# 55.7 local x=$(cmd) checks exit status (taint orthogonal — don't reference x)
run_sash 'fn f { local x=$(/bin/false); }; f'
[[ $rc != 0 && -z $err ]]; chk 55.7

# 55.8 x=$(cmd) where cmd fails aborts (taint orthogonal — don't reference x)
run_sash 'x=$(/bin/false)'
[[ $rc != 0 && -z $err ]]; chk 55.8

# 55.9 x=$(- cmd) captures output, discards exit
run_sash 'x=$(- /bin/echo hello); echo $x'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 55.9

# 55.10 pipefail always on
run_sash '/bin/false | /bin/true'
[[ $rc != 0 && -z $err ]]; chk 55.10

# 55.11 - cmd1 | cmd2 suppresses cmd1 but checks cmd2
run_sash '- /bin/false | /bin/true'
[[ $rc = 0 && -z $err ]]; chk 55.11

# 55.12 $(f) for -> int function is type error (use $((f)) instead)
run_sash 'fn f -> int { return 42; }
echo $(f)'
[[ $rc != 0 && -z $out && -n $err ]]; chk 55.12

# 55.13 bare call of non-void function → compile error
run_sash 'fn f -> int { return 42; }
f'
[[ $rc != 0 && -z $out && -n $err ]]; chk 55.13

# 55.14 return 0 as only return path → compile error
run_sash 'fn f -> int { return 0; }'
[[ $rc != 0 && -z $out && -n $err ]]; chk 55.14

# 55.15 - cmd; return $? → compile error ($? unavailable after -)
run_sash 'fn f -> int { - /bin/false; return $?; }'
[[ $rc != 0 && -z $out && -n $err ]]; chk 55.15

# 55.16 background exec failure aborts parent
run_sash '/nonexistent_binary_xyz &
wait'
[[ $rc != 0 && -z $err ]]; chk 55.16

# 55.17 $(status) only valid as first expression in handling branch
run_sash 'cmd || { s=$(status); echo $s; }'
[[ $rc != 0 && -z $out && -n $err ]]; chk 55.17a

# first-expression rule: $(status) after another command → error
run_sash '/bin/true || { /bin/echo x; s=$(status); }'
[[ $rc != 0 && -z $out && -n $err ]]; chk 55.17b

# $(status) as first expression → OK
run_sash '/bin/false || s=$(status)'
[[ $rc = 0 && -z $err ]]; chk 55.17c

# 55.18 $? is sugar for $(status)
run_sash '/bin/false || echo $?'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk 55.18

# 55.19 $! is sugar for $(getcpid)
run_sash '/bin/sleep 0 & p=$!; wait $p; echo done'
[[ $rc = 0 && $out = "done" && -z $err ]]; chk 55.19

# ── DD#55 syscall-abort: internal builtins must thread real status ──
# Internal syscall wrappers (cd→chdir, redirect→open, echo→write) must
# (a) set their real exit status, visible to catching positions, and
# (b) abort on failure unless caught (if/||) or suppressed (-). A failed
# redirect open must NOT silently misroute output to the original fd.
# NATIVE-ONLY for now: the C and janet oracles don't yet implement the
# DD#55 (A)-channel abort at all (bare `/bin/false` doesn't abort there);
# bringing them to parity is a separate task. See notes/tasks/.
if in_modes native; then
    # 55.20 bare cd to a bad dir aborts (failed chdir is a failed syscall)
    run_sash 'echo a
cd /nonexistent_xyz_55
echo b'
    [[ $rc != 0 && $out = "a" && -z $err ]]; chk 55.20

    # 55.21 cd SUCCESS is visible to a condition (regression: was always "no")
    run_sash 'if cd /tmp; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "yes" && -z $err ]]; chk 55.21

    # 55.22 cd failure in a condition is caught — no abort
    run_sash 'if cd /nonexistent_xyz_55; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "no" && -z $err ]]; chk 55.22

    # 55.23 - cd /bad suppresses the abort
    run_sash '- cd /nonexistent_xyz_55
echo after'
    [[ $rc = 0 && $out = "after" && -z $err ]]; chk 55.23

    # 55.27 bare echo write(2) failure aborts (/dev/full → ENOSPC)
    run_sash 'echo hi > /dev/full
echo after'
    [[ $rc != 0 && -z $out && -z $err ]]; chk 55.27

    # 55.28 echo write failure is catchable with ||
    run_sash 'echo hi > /dev/full || echo caught'
    [[ $rc = 0 && $out = "caught" && -z $err ]]; chk 55.28

    # 55.29 - echo write failure suppressed
    run_sash '- echo hi > /dev/full
echo after'
    [[ $rc = 0 && $out = "after" && -z $err ]]; chk 55.29
else
    skip_chk 55.20; skip_chk 55.21; skip_chk 55.22; skip_chk 55.23
    skip_chk 55.27; skip_chk 55.28; skip_chk 55.29
fi

# 55.24-26 redirect open(2) setup-failure (abort / catch / M1-no-suppress)
# are pending the W3 redirect increment (redirect-path short-circuit +
# the `suppressed` IR bit). Skipped on all modes until implemented.
skip_chk 55.24; skip_chk 55.25; skip_chk 55.26

# --- DD#46 top-level positional rejection: directed error text (DD#60) ---

# e.dd46.t1 top-level $@ → directs to $(argv)
run_sash 'echo "$@"'
[[ $rc != 0 && -z $out && $err == *'use $(argv)'* ]]; chk e.dd46.t1

# e.dd46.t2 top-level $1 → directs to $(arg 1)
run_sash 'echo "$1"'
[[ $rc != 0 && -z $out && $err == *'use $(arg 1)'* ]]; chk e.dd46.t2

# e.dd46.t3 top-level $* → directs to $(argv)
run_sash 'echo "$*"'
[[ $rc != 0 && -z $out && $err == *'use $(argv)'* ]]; chk e.dd46.t3

# e.dd46.t4 top-level $# → directs to $(argc)
run_sash 'echo "$#"'
[[ $rc != 0 && -z $out && $err == *'use $(argc)'* ]]; chk e.dd46.t4

# e.dd46.t5 top-level $0 → directs to $(arg 0)
run_sash 'echo "$0"'
[[ $rc != 0 && -z $out && $err == *'use $(arg 0)'* ]]; chk e.dd46.t5

set -e
tally
