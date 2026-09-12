#!/bin/bash
# spec/errors.sh — error model tests (DD#21, DD#25, DD#55)
set -eu
. common.sh
setup errors
plan=150
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

# 55.1 external cmd nonzero aborts (+ DD#55 diagnostic to stderr in native)
run_sash '/bin/false
echo should_not_reach'
[[ $rc != 0 && $out != *"should_not_reach"* && -z $err ]] \
  && { ! in_modes native || [[ $rerr == *"/bin/false: command failed (exit 1)"* ]]; }; chk 55.1

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

# 55.7 x=$(cmd) checks exit status: a failing cmdsub aborts before the read
# (DD#86: `local` removed; x is read via attest so it isn't a dead variable)
run_sash 'fn f { x=$(/bin/false); attest x [[ -n $x ]]; echo $x }; f'
[[ $rc != 0 && -z $err ]]; chk 55.7

# 55.8 x=$(cmd) where cmd fails aborts (taint orthogonal — don't reference x)
run_sash 'x=$(/bin/false); attest x [[ -n $x ]]'
[[ $rc != 0 && -z $err ]] \
  && { ! in_modes native || [[ $rerr == *"/bin/false: command failed (exit 1)"* ]]; }; chk 55.8

# 55.9 x=$(- cmd) captures output (discards exit). The output is external →
# tainted, so it must be attested before use. (Was a pre-existing taint hole:
# the `-` suppression wrapper hid the external from the first-stmt-only taint
# scan — closed 2026-08-19 by the multi-statement-cmdsub taint fix.)
run_sash 'x=$(- /bin/echo hello); attest x [[ -n $x ]]; echo $x'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 55.9

# 55.9b x=$(- cmd) does NOT abort when the captured command FAILS (55.9 only
# tested a succeeding /bin/echo). DD#55: `-` discards the exit status, so a
# failed `$(- /bin/false)` yields empty output and continues — mirroring the
# statement-position `- /bin/false` (55.4). native,c: janet's `$(- cmd)`
# capture is a separate TODO.
if in_modes native,c; then
    run_sash 'x=$(- /bin/false); attest x [[ -z $x ]]; echo "survived[$x]"'
    [[ $rc = 0 && $out = "survived[]" && -z $err ]]; chk 55.9b
else skip_chk 55.9b; fi

# 55.10 pipefail always on
run_sash '/bin/false | /bin/true'
[[ $rc != 0 && -z $err ]] \
  && { ! in_modes native || [[ $rerr == *"/bin/false: command failed (exit 1)"* ]]; }; chk 55.10

# 55.11 - cmd1 | cmd2 suppresses cmd1 but checks cmd2
run_sash '- /bin/false | /bin/true'
[[ $rc = 0 && -z $err ]]; chk 55.11

# --- DD#55 pipeline aggregate is context-independent (2026-07-24) ---

# 55.32 pipefail in a condition: a FIRST-segment failure counts (else branch).
# (Corrects the old stage-0 skip: `if false|true` used to take THEN.)
run_sash 'if /bin/false | /bin/true; then echo T; else echo E; fi'
[[ $rc = 0 && $out = "E" && -z $err ]]; chk 55.32

# 55.33 pipefail in a condition: last-segment failure also counts (else).
run_sash 'if /bin/true | /bin/false; then echo T; else echo E; fi'
[[ $rc = 0 && $out = "E" && -z $err ]]; chk 55.33

# 55.34 pipefail in a condition: all-succeed takes then.
run_sash 'if /bin/true | /bin/true; then echo T; else echo E; fi'
[[ $rc = 0 && $out = "T" && -z $err ]]; chk 55.34

# 55.35 leading `-` suppresses only the FIRST segment: a later-stage failure
# still aborts (AFTER must not run). Was a bug: `-` unchecked the whole pipe.
run_sash '- /bin/true | /bin/false
echo AFTER'
[[ $rc != 0 && $out != *"AFTER"* && -z $err ]] \
  && { ! in_modes native || [[ $rerr == *"/bin/false: command failed (exit 1)"* ]]; }; chk 55.35

# 55.36 leading `-` on a first-segment failure does NOT abort; AFTER runs.
run_sash '- /bin/false | /bin/true
echo AFTER'
[[ $rc = 0 && $out = "AFTER" && -z $err ]]; chk 55.36

# 55.37 a non-first `-` segment still RUNS (only its status is suppressed):
# grep filters the input, so output is empty — the stage is not dropped.
run_sash 'echo hello | - /bin/grep nomatch
echo done'
[[ $rc = 0 && $out = "done" && -z $err ]]; chk 55.37

# 55.38 `pipeline && cmd` is rejected (SB4023) — ambiguous postfix; use `if`.
run_sash '/bin/false | /bin/true && echo foo'
[[ $rc != 0 && -z $out && -n $err ]]; chk 55.38

# 55.39 `pipeline || cmd` is likewise rejected (SB4023).
run_sash '/bin/false | /bin/true || echo foo'
[[ $rc != 0 && -z $out && -n $err ]]; chk 55.39

# 55.40 a pipeline as the CONSEQUENT of && stays legal (not a postfix).
run_sash '/bin/true && /bin/echo ok | /bin/cat'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 55.40


# 55.12 Option C: `echo $(f)` on a -> int function prints its RETURN value
#       (`$(f)` ≡ `$((f))`). (Was a "cannot capture" error before the
#       $()/$(()) unification.)
run_sash 'fn f -> int { return 42; }
echo $(f)'
[[ $rc = 0 && $out = "42" && -z $err ]]; chk 55.12

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

# 55.16 background exec failure — the CHILD reports the exec errno at the
#       point of failure; the parent's uncaught abort adds command-failed.
run_sash '/nonexistent_binary_xyz &
wait'
[[ $rc != 0 && -z $err ]] \
  && { ! in_modes native || [[ $rerr == *"/nonexistent_binary_xyz: No such file or directory"* ]]; }; chk 55.16

# 55.30 fg UNCAUGHT exec failure → child errno detail + parent uncaught
#       abort (both lines). DD#55: child owns exec errors; parent's
#       command-failed fires because it is uncaught. native (msg format).
if in_modes native; then
    run_sash '/nonexistent_binary_xyz
echo unreached'
    [[ $rc != 0 && -z $out \
       && $rerr == *"/nonexistent_binary_xyz: No such file or directory"* \
       && $rerr == *"/nonexistent_binary_xyz: command failed (exit 127)"* ]]; chk 55.30
else
    skip_chk 55.30
fi

# 55.31 CAUGHT exec failure (`|| …`) → only the child's errno detail; the
#       parent does NOT abort, so no command-failed line.
if in_modes native; then
    run_sash '/nonexistent_binary_xyz || echo caught'
    [[ $rc = 0 && $out = "caught" && -z $err \
       && $rerr == *"/nonexistent_binary_xyz: No such file or directory"* \
       && $rerr != *"command failed"* ]]; chk 55.31
else
    skip_chk 55.31
fi

# 55.17 $(status) only valid as first expression in handling branch
run_sash 'cmd || { s=$(status); echo $s; }'
[[ $rc != 0 && -z $out && -n $err ]]; chk 55.17a

# first-expression rule: $(status) after another command → error
run_sash '/bin/true || { /bin/echo x; s=$(status); }'
[[ $rc != 0 && -z $out && -n $err ]]; chk 55.17b

# $(status) as first expression → OK
run_sash '/bin/false || s=$(status); : $s'
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
# (b) abort on failure unless caught (if/||) — or, for a command's own
# verdict (cd / echo's write), suppressed by `-`. A redirect open(2) is
# the implicit `exec N>file` that runs FIRST, so `-` (which binds to the
# command) does NOT suppress it; and a failed open must never silently
# misroute output to the original fd.
# native+c: c now implements the DD#55 cd + echo-write(2) syscall aborts
# with descriptive diagnostics (2026-08-20), and the harness captures c's
# runtime stderr into $rerr. Redirect open(2)-failure abort is still c-TODO
# (nested guard below). janet deferred.
if in_modes native,c; then
    # Abort cases emit a descriptive diagnostic to stderr (captured as
    # $rerr), prefixed with the program name (argv[0], here ./test_bin):
    #   <argv0>: cd: <path>: <reason>
    #   <argv0>: <path>: <reason>              (redirect open)
    #   <argv0>: echo: write error: <reason>   (echo write)
    # where <reason> is the OS strerror text. Caught (if/||) and `-`
    # suppressed failures do NOT abort, so they emit NO diagnostic.

    # 55.20 bare cd to a bad dir aborts (failed chdir is a failed syscall)
    run_sash 'echo a
cd /nonexistent_xyz_55
echo b'
    [[ $rc != 0 && $out = "a" && -z $err \
       && $rerr == *"test_bin: cd: /nonexistent_xyz_55: No such file or directory"* ]]; chk 55.20

    # 55.21 cd SUCCESS is visible to a condition (regression: was always "no")
    run_sash 'if cd /tmp; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "yes" && -z $err && -z $rerr ]]; chk 55.21

    # 55.22 cd failure in a condition is caught — no abort, no diagnostic
    run_sash 'if cd /nonexistent_xyz_55; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "no" && -z $err && -z $rerr ]]; chk 55.22

    # 55.23 - cd /bad suppresses the abort (and the diagnostic)
    run_sash '- cd /nonexistent_xyz_55
echo after'
    [[ $rc = 0 && $out = "after" && -z $err && -z $rerr ]]; chk 55.23

    # 55.27 bare echo write(2) failure aborts (/dev/full → ENOSPC) + diagnostic
    run_sash 'echo hi > /dev/full
echo after'
    [[ $rc != 0 && -z $out && -z $err \
       && $rerr == *"test_bin: echo: write error: No space left on device"* ]]; chk 55.27

    # 55.28 echo write failure is catchable with || — no abort, no diagnostic
    run_sash 'echo hi > /dev/full || echo caught'
    [[ $rc = 0 && $out = "caught" && -z $err && -z $rerr ]]; chk 55.28

    # 55.29 - echo write failure suppressed — no diagnostic
    run_sash '- echo hi > /dev/full
echo after'
    [[ $rc = 0 && $out = "after" && -z $err && -z $rerr ]]; chk 55.29

    # ── DD#55 redirect-open(2): the open is a checked syscall that runs
    # FIRST (the implicit `exec N>file`). On failure the command body is
    # skipped (no misroute), and the abort is governed by CONTEXT — bare
    # and `-` both abort (the `-` binds to the command's write verdict,
    # not the prior open); an enclosing condition (if/||) catches it via
    # the failed final-command result.
    # NESTED native+c: redirect open(2)-failure abort is now implemented in c
    # (emit_redirects_guarded + emit_command skip-body + in_condition, so
    # `- echo > /bad` still aborts while `if`/`||` catch). janet deferred.
    if in_modes native,c; then
    # 55.24 bare redirect open failure aborts — no misroute + diagnostic
    run_sash 'echo hi > /nonexistent_dir_55/f
echo after'
    [[ $rc != 0 && -z $out && -z $err \
       && $rerr == *"test_bin: /nonexistent_dir_55/f: No such file or directory"* ]]; chk 55.24

    # 55.25 open failure in an if-condition is caught: the failed final
    # command makes the condition false → else branch (no misroute, no diag)
    run_sash 'if echo hi > /nonexistent_dir_55/f; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "no" && -z $err && -z $rerr ]]; chk 55.25

    # 55.26 `- echo > /bad` still ABORTS: `-` suppresses echo's write
    # verdict, not the prior `exec N>file` open, which runs first (+ diagnostic)
    run_sash '- echo hi > /nonexistent_dir_55/f
echo after'
    [[ $rc != 0 && -z $out && -z $err \
       && $rerr == *"test_bin: /nonexistent_dir_55/f: No such file or directory"* ]]; chk 55.26

    # 55.50 open failure is catchable with || too (symmetry with if), no diag
    run_sash 'echo hi > /nonexistent_dir_55/f || echo caught'
    [[ $rc = 0 && $out = "caught" && -z $err && -z $rerr ]]; chk 55.50
    else
        skip_chk 55.24; skip_chk 55.25; skip_chk 55.26; skip_chk 55.50
    fi
else
    skip_chk 55.20; skip_chk 55.21; skip_chk 55.22; skip_chk 55.23
    skip_chk 55.27; skip_chk 55.28; skip_chk 55.29
    skip_chk 55.24; skip_chk 55.25; skip_chk 55.26; skip_chk 55.50
fi

# --- DD#46 top-level positional rejection: directed error text (DD#60) ---

# e.dd46.t1 top-level $@ → SB5045 (sash has no $@/$*), directs to $(argv 1)
# (the user-arg slice; $(argv) would include the program name at index 0)
run_sash 'echo "$@"'
[[ $rc != 0 && -z $out && $err == *SB5045* && $err == *'argv 1'* ]]; chk e.dd46.t1

# e.dd46.t2 top-level $1 → directs to $(arg 1)
run_sash 'echo "$1"'
[[ $rc != 0 && -z $out && $err == *'use $(arg 1)'* ]]; chk e.dd46.t2

# e.dd46.t3 top-level $* → SB5045 (sash has no $@/$*), directs to $(argv 1)
run_sash 'echo "$*"'
[[ $rc != 0 && -z $out && $err == *SB5045* && $err == *'argv 1'* ]]; chk e.dd46.t3

# e.dd46.t4 top-level $# → directs to $(argc)
run_sash 'echo "$#"'
[[ $rc != 0 && -z $out && $err == *'use $(argc)'* ]]; chk e.dd46.t4

# e.dd46.t5 top-level $0 → directs to $(arg 0)
run_sash 'echo "$0"'
[[ $rc != 0 && -z $out && $err == *'use $(arg 0)'* ]]; chk e.dd46.t5

# e.dd46.t6 top-level braced ${@} is the same splat → SB5045 (was slipping)
run_sash 'echo "${@}"'
[[ $rc != 0 && -z $out && $err == *SB5045* ]]; chk e.dd46.t6

# e.dd46.t7 top-level $@ inside $(( )) → SB5045 (arith path)
run_sash 'echo $(( $@ ))'
[[ $rc != 0 && -z $out && $err == *SB5045* ]]; chk e.dd46.t7

# ── DD#70/#71 dual: a value binding used in a boolean context is rejected ──
# A boolean context (if/while/until cond, left of &&/||, operand of !) must
# consume a boolean or exit status; an assignment/declaration (a value
# binding, DD#71) there is a compile error (SB4025).
run_sash 'x=hi && echo $x'
[[ $rc != 0 && -z $out && $err == *'SB4025'* ]]; chk e.br1
run_sash 'if x=foo; then echo t; fi'
[[ $rc != 0 && -z $out && $err == *'SB4025'* ]]; chk e.br2
run_sash 'int n=0; while x=foo; do echo t; done'
[[ $rc != 0 && -z $out && $err == *'SB4025'* ]]; chk e.br3
run_sash 'x=foo || echo y'
[[ $rc != 0 && -z $out && $err == *'SB4025'* ]]; chk e.br4
run_sash '! x=foo'
[[ $rc != 0 && -z $out && $err == *'SB4025'* ]]; chk e.br5
# br6 a typed declaration is also a value binding
run_sash 'if int x=5; then echo t; fi'
[[ $rc != 0 && -z $out && $err == *'SB4025'* ]]; chk e.br6
# br7 (positive) a real status producer is still a valid condition
run_sash 'if true; then echo t; fi'
[[ $rc = 0 && $out = "t" && -z $err ]]; chk e.br7
# br8 (positive) a `[[ ]]` boolean test condition
run_sash 'x=a; if [[ $x == a ]]; then echo t; fi'
[[ $rc = 0 && $out = "t" && -z $err ]]; chk e.br8
# br9 (positive) an assignment as a plain statement (not a condition) is fine
run_sash 'x=foo; echo $x'
[[ $rc = 0 && $out = "foo" && -z $err ]]; chk e.br9
# br10 (positive) an assignment on the RIGHT of || (the consequent) is fine
run_sash 'false || x=set; : $x; echo done'
[[ $rc = 0 && $out = "done" && -z $err ]]; chk e.br10

# ── DD#70 (A)/(B) error-model contract ───────────────────────────────
# (B) must-consume booleans: `match` (regex.sh 63.1c-2f) and a bare
# `[[ … ]]`. A bare boolean is a compile error; if/&&/|| consume it;
# `|| :` and `-` both discard it (DD#70 addendum 2026-09-09 — `-` is an
# explicit discard, so it carries the same weight as `|| :`).
# SB4018/SB4019 are transform diagnostics — native-only (not surfaced on
# --emit-c / --janet yet; see c-janet-dd55-abort-channel).

# e.b70.tbare a bare `[[ … ]]` result is unused → SB4018
# e.b70.tdash `-` on a `[[ … ]]` discards the result — legal
if in_modes native,c; then
    run_sash '[[ a = a ]]'
    [[ $rc != 0 && -z $out && $err == *SB4018* ]]; chk e.b70.tbare

    run_sash '- [[ a = a ]]
echo after'
    [[ $rc = 0 && $out = "after" && -z $err ]]; chk e.b70.tdash
else
    skip_chk e.b70.tbare; skip_chk e.b70.tdash
fi

# e.b70.tdisc `[[ … ]] || :` explicitly discards the boolean → ok
run_sash '[[ a = a ]] || :
echo done'
[[ $rc = 0 && $out = "done" && -z $err ]]; chk e.b70.tdisc

# e.b70.tand a `[[ … ]] && action` consumes it (runs on true)
run_sash '[[ a = a ]] && echo hit'
[[ $rc = 0 && $out = "hit" && -z $err ]]; chk e.b70.tand

# e.b70.tor a `[[ … ]] || action` consumes it (runs on false)
run_sash '[[ a = b ]] || echo miss'
[[ $rc = 0 && $out = "miss" && -z $err ]]; chk e.b70.tor

# ── `!` precedence + pipeline + suppress (DG 2026-08-19). `!` binds its
# immediate unit (bash-consistent `(!X) && Y`); it cannot negate a pipeline
# (reviewability, SB4040). Runtime negation is native-only
# (c/janet `!` backend gap); the parse-time rejects are all modes. ──
# bng.1 `! X && Y` reassociates to `(!X) && Y` (was wrongly SB4018)
if in_modes native,c; then
    run_sash 'if ! /bin/false && /bin/true; then echo Y; else echo N; fi'
    [[ $rc = 0 && $out = "Y" && -z $rerr ]]; chk bng.1
    # bng.2 `! X && action` runs the action when !X is true
    run_sash '! /bin/false && /bin/echo ran'
    [[ $rc = 0 && $out = "ran" && -z $rerr ]]; chk bng.2
    # bng.3 `! X || action` runs the action when !X is false
    run_sash '! /bin/true || /bin/echo ran'
    [[ $rc = 0 && $out = "ran" && -z $rerr ]]; chk bng.3
    # bng.6 escape hatch: a pipeline wrapped in a predicate function negates
    run_sash 'f() { if /bin/false | /bin/cat; then return 0; fi; return 1 }
if ! f; then echo neg; fi'
    [[ $rc = 0 && $out = "neg" && -z $rerr ]]; chk bng.6
else
    skip_chk bng.1; skip_chk bng.2; skip_chk bng.3; skip_chk bng.6
fi
# bng.4 `!` on a pipeline is rejected — reviewer can't see `!`'s scope (SB4040)
run_sash 'if ! /bin/true | /bin/cat; then echo x; fi'
[[ $rc != 0 && -z $out && $err == *SB4040* ]]; chk bng.4
# bng.5 `- !` discards the negated result (DD#70 addendum). `!` yields a
# boolean and `-` discards results, so the composition is legal — redundant
# with a plain `- cmd`, but explicitly so.
run_sash '- ! /bin/false
echo after'
[[ $rc = 0 && $out = "after" && -z $err ]]; chk bng.5

# ── DD#70 terminal-segment negation (DG 2026-08-31): `!` may negate the FINAL
# pipeline stage (`a | ! b`), which supplies the pipeline's result — the same
# `!` a standalone simple command allows. Its status is inverted (0↔nonzero)
# before the DD#55 worst-of. A `!` on a non-terminal stage is SB4040 (a
# plumbing status a reviewer can't trace). All three backends implement it. ──
# bng.7 `a | ! b` — b fails (no match) → `!` true → the branch is taken
run_sash 'if /bin/echo abc | ! /bin/grep -q z; then echo T; else echo F; fi'
[[ $rc = 0 && $out = "T" && -z $rerr ]]; chk bng.7
# bng.8 `a | ! b` — b matches → `!` false → the else branch
run_sash 'if /bin/echo xyz | ! /bin/grep -q y; then echo T; else echo F; fi'
[[ $rc = 0 && $out = "F" && -z $rerr ]]; chk bng.8
# bng.9 negation is on the TERMINAL stage of a 3-stage pipeline
run_sash 'if /bin/echo abc | /bin/cat | ! /bin/grep -q z; then echo T; else echo F; fi'
[[ $rc = 0 && $out = "T" && -z $rerr ]]; chk bng.9
# bng.10 `!` on a NON-terminal stage (`a | ! b | c`) is rejected — SB4040
run_sash '/bin/echo abc | ! /bin/grep abc | /bin/cat'
[[ $rc != 0 && -z $out && $err == *SB4040* ]]; chk bng.10
# bng.11 `! -` stays rejected, and NOT as the `- !` mirror: `-` has already
# discarded the result, so `!` has nothing left to invert. That is a
# composition error, not a must-consume one — the addendum does not touch it.
run_sash '/bin/echo abc | ! - /bin/grep -q z'
[[ $rc != 0 && -z $out && $err == *SB4019* ]]; chk bng.11
# bng.12 `return ! X | Y` — a non-terminal `!` in a return producer → SB4040
#        (was the confusing bare-`!` SB4014)
run_sash 'f() { return ! /bin/echo abc | /bin/grep -q z }
if f; then echo T; fi'
[[ $rc != 0 && -z $out && $err == *SB4040* ]]; chk bng.12

# e.b70.fndisc `f || :` discards a `-> int` function's return → ok.
# native+c: janet doesn't yet compile a `-> int` function used in `|| :`
# position (see c-janet-fn-int-discard). janet deferred.
if in_modes native,c; then
    run_sash 'fn f -> int { return 3 }
f || :
echo done'
    [[ $rc = 0 && $out = "done" && -z $err ]]; chk e.b70.fndisc
else
    skip_chk e.b70.fndisc
fi

# (A) abort channel: a CAUGHT failure prints NOTHING to stderr (the
# `<argv0>: …` diagnostic is only for an *uncaught* abort).
# e.a70.silent `if /bin/false` → else branch, no runtime diagnostic
if in_modes native,c; then
    run_sash 'if /bin/false; then echo y; else echo n; fi'
    [[ $rc = 0 && $out = "n" && -z $rerr ]]; chk e.a70.silent
else
    skip_chk e.a70.silent
fi

# (A) dynamic (interpolated) bad regex → runtime regcomp abort with the
# `regexp error: …` diagnostic (compile-time validation only covers a
# static pattern). native+c: c emits the runtime diagnostic and the harness
# now captures its stderr into $rerr. janet deferred.
# e.a70.regcomp
if in_modes native,c; then
    run_sash 'p="[bad"
if match hello "$p"; then echo y; else echo n; fi'
    [[ $rc != 0 && $rerr == *"regexp error"* ]]; chk e.a70.regcomp
else
    skip_chk e.a70.regcomp
fi

# 55.41 an external command in a `while` condition is CAUGHT (a nonzero exit
#       ends the loop), like `if` — it must not abort. bash: `done`, rc 0.
#       Regression: the while-condition command was not marked unchecked
#       (only `if`/Branch conditions were), so it aborted on the terminating
#       false evaluation.
run_sash 'while /bin/false; do echo x; done; echo done'
[[ $rc = 0 && $out = "done" && -z $err && -z $rerr ]]; chk 55.41

# cmdvar.1 a variable used as a command name is a COMPILE error (a command
#          can't be resolved at runtime — DD#23). Was a runtime crash
#          ("bug: bare command passed to sash_exec"). const is no exception
#          yet (const-fold is a future task).
run_sash 'x=/usr/bin/echo; $x hi'
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk cmdvar.1
run_sash 'const x=/usr/bin/echo; $x hi'
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk cmdvar.2

# relpath.* DD#23: a relative external path (`./x`, `foo/bar`) is a COMPILE
# error (SB5051) — a resolved external head must be absolute so it can't bind
# to the runtime cwd ("no PATH injection, no search ambiguity"). Uniform across
# positions: a top-level head, inside `$( )`, and a pipeline stage. An absolute
# path still compiles+runs; a bare unknown name is still "not found" (SB5011),
# NOT a relative-path error (the two are distinct).
run_sash 'foo/bar baz'
[[ $rc != 0 && -z $out && $err == *SB5051* ]]; chk relpath.1
run_sash './tool -a'
[[ $rc != 0 && -z $out && $err == *SB5051* ]]; chk relpath.2
run_sash 'x=$(./tool -a); echo $x'
[[ $rc != 0 && -z $out && $err == *SB5051* ]]; chk relpath.3
run_sash '/bin/echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk relpath.4
run_sash 'nosuchcmd991 x'
[[ $rc != 0 && $err == *SB5011* && $err != *SB5051* ]]; chk relpath.5
run_sash './tool a | /bin/cat'
[[ $rc != 0 && -z $out && $err == *SB5051* ]]; chk relpath.6

# ── Compound-condition catch (DD#55; DG 2026-08-19). `&&`/`||` operands in a
# condition are caught (not aborted); a `;` list's non-final member and an
# if-body's final command are bare → they abort. native,c (janet emits
# conditions from the AST — pre-existing control-flow gap, brace.sh native-only).
if in_modes native,c; then
    # cc.1 `&&` RHS failure is caught → else branch
    run_sash 'if /bin/true && /bin/false; then echo Y; else echo N; fi'
    [[ $rc = 0 && $out = "N" && -z $rerr ]]; chk cc.1
    # cc.2 `||` both sides fail → condition false → else (the `||` $?-reset fix)
    run_sash 'if /bin/false || /bin/false; then echo Y; else echo N; fi'
    [[ $rc = 0 && $out = "N" && -z $rerr ]]; chk cc.2
    # cc.3 `if a; b` is `a; if b` — the non-final `a` aborts (bare, discarded)
    run_sash 'if /bin/false; /bin/true; then echo Y; else echo N; fi'
    [[ $rc != 0 && -z $out ]]; chk cc.3
    # cc.4 a failing if-body final command aborts (bare-command-uncaught)
    run_sash 'if /bin/true; then /bin/false; fi'
    [[ $rc != 0 && -z $out ]]; chk cc.4
else
    skip_chk cc.1; skip_chk cc.2; skip_chk cc.3; skip_chk cc.4
fi

# --- SB4045: value-intrinsic wrong arity (was: fall-through to a bare command
# and a runtime "bug: bare command passed to sash_exec"). Front-end — all modes.
# 55.42 sub needs 3 args
run_sash 's=x; echo $(sub a)'
[[ $rc != 0 && -z $out && $err == *SB4045* ]]; chk 55.42
# 55.43 toupper needs 1 arg (0 given)
run_sash 'echo $(toupper)'
[[ $rc != 0 && -z $out && $err == *SB4045* ]]; chk 55.43
# 55.44 len is now strict too (was lenient — silently ignored extra args)
run_sash 'a=x; echo $(len a b)'
[[ $rc != 0 && -z $out && $err == *SB4045* ]]; chk 55.44
# 55.45 a correct-arity call still compiles + runs
run_sash 's=hello; echo $(strlen $s)'
[[ $rc = 0 && $out = 5 && -z $err ]]; chk 55.45

# ── via.*: every value intrinsic has an arity, and none is a statement ──
# The SB4045 table above covered only len/sub/toupper/…; the *other* value
# intrinsics (getenv, status, getpid, argc, dtoi, …) had no arity entry and no
# context check at all. Extra arguments were silently discarded, and a bare
# statement fell through to `sash_exec`, so an ACCEPTED program printed
# `sash: bug: bare command passed to sash_exec` at runtime. Front-end — all modes.

# via.1-5 wrong arity in `$( )` is SB4045, not a silent ignore
run_sash 'x=$(getenv); echo $x'
[[ $rc != 0 && -z $out && $err == *SB4045* ]]; chk via.1
run_sash 'x=$(getenv HOME EXTRA); echo $x'
[[ $rc != 0 && -z $out && $err == *SB4045* ]]; chk via.2
run_sash 'x=$(getpid 7); echo $x'
[[ $rc != 0 && -z $out && $err == *SB4045* ]]; chk via.3
run_sash 'x=$(argc 7); echo $x'
[[ $rc != 0 && -z $out && $err == *SB4045* ]]; chk via.4
# dtoi takes 2 (digits, value) — one arg is as wrong as three
run_sash 'x=$(dtoi 2); echo $x'
[[ $rc != 0 && -z $out && $err == *SB4045* ]]; chk via.5

# via.6-9 a value intrinsic in STATEMENT position is SB5073, never an ICE
run_sash 'getenv HOME'
[[ $rc != 0 && -z $out && $err == *SB5073* && $err != *"sash: bug"* ]]; chk via.6
run_sash 'status'
[[ $rc != 0 && -z $out && $err == *SB5073* && $err != *"sash: bug"* ]]; chk via.7
run_sash 'getpid'
[[ $rc != 0 && -z $out && $err == *SB5073* && $err != *"sash: bug"* ]]; chk via.8
run_sash 'epochseconds'
[[ $rc != 0 && -z $out && $err == *SB5073* && $err != *"sash: bug"* ]]; chk via.9

# via.10-12 the legitimate forms are untouched
# native,c only: janet exec's the getpid-class instead of implementing it
# (filed, deferred: janet-getpid-class-intrinsics).
if in_modes native,c; then
    run_sash 'x=$(getenv HOME)
attest x [[ -n $x ]]
echo ok'
    [[ $rc = 0 && $out = "ok" && -z $err ]]; chk via.10
    run_sash 'p=$(getpid); if (( p > 0 )); then echo ok; fi'
    [[ $rc = 0 && $out = "ok" && -z $err ]]; chk via.11
else
    skip_chk via.10; skip_chk via.11
fi
# via.12 `argv` is NOT value-only — DD#60's out-param statement form stays legal
run_sash 'array a
argv a
n=$(len a)
echo "n=$n"'
[[ $rc = 0 && $out == n=* && -z $err ]]; chk via.12

# ── DD#95: mixing `&&` and `||` in one list is ambiguous ──
# The two contexts grouped the same text differently — inside `[[ ]]` `&&` binds
# tighter (as in C/Go/Rust/Python), outside they are equal-precedence and
# left-associative (as in POSIX shell). Rather than pick a winner, sash rejects
# the mixture, so no text means two things depending on where it sits. Pure
# chains are unaffected (they group the same either way). Ada does the same;
# C's own -Wall warns on the identical shape.
# 95.1 inside `[[ ]]`: mixed is rejected, and the advice is parentheses
run_sash 'a=x; b=; c=
if [[ -n $a || -n $b && -n $c ]]; then echo T; else echo F; fi'
[[ $rc != 0 && -z $out && $err == *SB4053* && $err == *parenthes* ]]; chk 95.1
# 95.2 …explicit grouping resolves it, both ways
run_sash 'a=x; b=; c=
if [[ ( -n $a || -n $b ) && -n $c ]]; then echo T; else echo F; fi'
[[ $rc = 0 && $out = "F" && -z $err ]]; chk 95.2
run_sash 'a=x; b=; c=
if [[ -n $a || ( -n $b && -n $c ) ]]; then echo T; else echo F; fi'
[[ $rc = 0 && $out = "T" && -z $err ]]; chk 95.3
# 95.4 outside `[[ ]]`: mixed is rejected, and the advice is `if` / dispatch
run_sash 'a=x; b=; c=
[[ -n $a ]] || [[ -n $b ]] && [[ -n $c ]]'
[[ $rc != 0 && -z $out && $err == *SB4053* && $err == *if* ]]; chk 95.4
# 95.5 …including in an `if` condition, which is still outside the brackets
run_sash 'a=x; b=; c=
if [[ -n $a ]] || [[ -n $b ]] && [[ -n $c ]]; then echo T; fi'
[[ $rc != 0 && -z $out && $err == *SB4053* ]]; chk 95.5
# 95.6 the classic ternary idiom is the motivating case
run_sash 'x=b
[[ $x =~ ^b$ ]] && echo Y || echo N'
[[ $rc != 0 && -z $out && $err == *SB4053* ]]; chk 95.6
# 95.7 …and both replacements work
run_sash 'x=b
if [[ $x =~ ^b$ ]] { echo Y } else { echo N }'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk 95.7
run_sash 'x=q
[[ $x =~ ^b$ { echo Y; } || { echo N; } ]]'
[[ $rc = 0 && $out = "N" && -z $err ]]; chk 95.8
# 95.9 PURE chains stay legal in both contexts — they group the same either way
run_sash 'a=x; b=x
if [[ -n $a && -n $b ]]; then echo T; fi
[[ -n $a ]] && [[ -n $b ]] && echo chain'
[[ $rc = 0 && $out = "T
chain" && -z $err ]]; chk 95.9
run_sash 'a=; b=
[[ -n $a ]] || [[ -n $b ]] || echo none'
[[ $rc = 0 && $out = "none" && -z $err ]]; chk 95.10
# 95.11 the error-handling idiom is untouched
run_sash 'true || echo unreached
false || echo handled'
[[ $rc = 0 && $out = "handled" && -z $err ]]; chk 95.11
# 95.12 `!` is unaffected — it is unary and binds tightest in both contexts
run_sash 'a=; b=x
if [[ ! -n $a && -n $b ]]; then echo T; else echo F; fi'
[[ $rc = 0 && $out = "T" && -z $err ]]; chk 95.12


# ── `$?` is sugar for `$(status)`, so they must agree EVERYWHERE ──────────
# `$(status)` -> `$?` is a parse-time rewrite that lived only on the unquoted
# `$( )` path. Inside a double-quoted string the substitution survived as a
# `Command { name: "status" }`, which no backend recognises as a builtin, so
# it was dispatched as an external — and the runtime's complaint was
# substituted INTO THE OUTPUT while the program exited 0. A silent wrong
# answer, and every compile-time check passed because `status` IS a builtin.

# 55.51 `$(status)` inside "…" reports the status
run_sash '/bin/false || { echo "s=$(status)"; }'
[[ $rc = 0 && $out = "s=1" && -z $rerr ]]; chk 55.51

# 55.52 …and agrees with the `$?` spelling, which always worked
run_sash '/bin/false || { echo "s=$?"; }'
[[ $rc = 0 && $out = "s=1" && -z $rerr ]]; chk 55.52

# 55.53 the unquoted spelling is unaffected
run_sash '/bin/false || { echo $(status); }'
[[ $rc = 0 && $out = "1" && -z $rerr ]]; chk 55.53

# 55.54 concatenated with other text in the same quoted word
run_sash '/bin/false || { echo "[$(status)][$(status)]"; }'
[[ $rc = 0 && $out = "[1][1]" && -z $rerr ]]; chk 55.54

# 55.55 `$(getcpid)` (sugar for `$!`) had the identical bug
if in_modes native,c; then
    run_sash '/bin/sleep 0 &
p="$(getcpid)"
[[ -n $p ]] || :
echo ok'
    [[ $rc = 0 && $out = "ok" && -z $rerr ]]; chk 55.55
else
    skip_chk 55.55
fi

# 55.56 no spelling reaches sash_exec as a bare command name — the shape of
# the failure, pinned directly so a regression cannot be silent again
run_sash '/bin/false || { echo "s=$(status)"; }'
[[ $out = "s=1" && $out != *"bare command"* && $out != *"sash: bug"* ]]; chk 55.56

# ── a signal death is 128+signum, not a bare 128 (2026-09-12) ──
# `sash_wait` returned `128` for ANY signal, dropping the number, so every
# signal death reported the same status. DD#55 defines the pipeline aggregate
# as the worst exit status (pipefail), and bash's pipefail gives 128+signum;
# `emit_signal_exit_check` already used that convention for the program's own
# signal deaths, so the runtime was the odd one out. Found while tracking a
# 2.5% flake in commands.sh 14.33.
#
# `- cmd` would discard the status (DD#70), and `$(status)` is unavailable
# after it (SB5012), so these pin the program's own exit code -- the
# observable that actually diverged from bash. The diagnostic TEXT is asserted
# in native only, per the convention at 55.1/55.8/55.10: the c backend exits
# with the right status but emits no abort diagnostic.

# 55.57 SIGTERM -> 143, matching bash
if in_modes native,c; then
    run_sash '/usr/bin/timeout --preserve-status -s TERM 0.1 /bin/sleep 5'
    [[ $rc = 143 ]] \
      && { ! in_modes native || [[ $rerr == *"(exit 143)"* ]]; }; chk 55.57
else
    skip_chk 55.57
fi

# 55.58 SIGPIPE -> 141. DD#55 makes the abort itself deliberate ("SIGPIPE is
# the programmer's responsibility"); this pins the STATUS it aborts with.
if in_modes native,c; then
    run_sash '/usr/bin/timeout --preserve-status -s PIPE 0.1 /bin/sleep 5'
    [[ $rc = 141 ]] \
      && { ! in_modes native || [[ $rerr == *"(exit 141)"* ]]; }; chk 55.58
else
    skip_chk 55.58
fi

# 55.59 a normal exit code is unaffected by the 128+signum path
if in_modes native,c; then
    run_sash '/bin/sh -c "exit 42"'
    [[ $rc = 42 ]] \
      && { ! in_modes native || [[ $rerr == *"(exit 42)"* ]]; }; chk 55.59
else
    skip_chk 55.59
fi

# --- DD#55 + DD#99: the read family runs the same open(2) guard ---
# `read ARR < f` / `readline v < f` carry their own redirects (SsaInst::
# ReadArray/ReadLine), so a failed open aborts instead of silently yielding
# an empty array / empty line — a mistyped path used to be indistinguishable
# from an empty file. Context governs the abort exactly as for `echo > f`
# (55.24-55.26, 55.50). native,c: janet has no DD#55 abort channel at all
# (task `c-janet-dd55-abort-channel`).
if in_modes native,c; then
    # 55.60 bare `read ARR < nosuch` aborts — no empty array, + diagnostic
    run_sash 'read a < /nonexistent_dir_55/f
echo "n=$(len a)"'
    [[ $rc != 0 && -z $out && -z $err \
       && $rerr == *"/nonexistent_dir_55/f: No such file or directory"* ]]; chk 55.60

    # 55.61 bare `readline v < nosuch` aborts — no empty line, + diagnostic
    run_sash 'readline v < /nonexistent_dir_55/f
attest v [[ -n $v ]]
echo "v=$v"'
    [[ $rc != 0 && -z $out && -z $err \
       && $rerr == *"/nonexistent_dir_55/f: No such file or directory"* ]]; chk 55.61

    # 55.62 `- read ARR < nosuch` still ABORTS: `-` suppresses the read's own
    # verdict, not the prior open, which runs first (mirrors 55.26)
    run_sash '- read a < /nonexistent_dir_55/f
echo "n=$(len a)"'
    [[ $rc != 0 && -z $out && -z $err \
       && $rerr == *"/nonexistent_dir_55/f: No such file or directory"* ]]; chk 55.62

    # 55.63/55.64 are NOT fail-first gates — they passed before the guard
    # existed too, because the unguarded read fell through to the harness's
    # `</dev/null` stdin and returned empty. They pin the other half of the
    # fix: now that the guard DOES fire, `in_condition` must catch it rather
    # than abort (getting that wrong turns these red).
    # 55.63 the failure is caught in an if-condition → else branch, no diag
    run_sash 'if read a < /nonexistent_dir_55/f { echo yes } else { echo no }
echo "n=$(len a)"'
    [[ $rc = 0 && $out = "no
n=0" && -z $err && -z $rerr ]]; chk 55.63

    # 55.64 ... and with `||` too (symmetry with if, mirrors 55.50)
    run_sash 'read a < /nonexistent_dir_55/f || echo caught
echo "n=$(len a)"'
    [[ $rc = 0 && $out = "caught
n=0" && -z $err && -z $rerr ]]; chk 55.64
else
    skip_chk 55.60; skip_chk 55.61; skip_chk 55.62
    skip_chk 55.63; skip_chk 55.64
fi

# 55.65/55.66 the guard does not disturb a redirect that opens fine.
# native,c only: janet drops the redirect and reads stdin instead —
# janet-read-family-ignores-input-redirect (pre-existing, not this guard).
if in_modes native,c; then
  # DD#99 `read ARR` splits the whole stream on NUL, so two NUL-terminated
  # fields are two elements.
  printf 'alpha\0beta\0' > /tmp/sash_spec_55_65.txt
  run_sash 'read a < /tmp/sash_spec_55_65.txt
echo "n=$(len a)"'
  [[ $rc = 0 && $out = "n=2" && -z $err ]]; chk 55.65

  # 55.66 likewise for the scalar read (newline-delimited, so its own file)
  printf 'alpha beta\n' > /tmp/sash_spec_55_66.txt
  run_sash 'readline v < /tmp/sash_spec_55_66.txt
attest v [[ -n $v ]]
echo "v=$v"'
  [[ $rc = 0 && $out = "v=alpha beta" && -z $err ]]; chk 55.66
else
  skip_chk 55.65; skip_chk 55.66
fi

# --- DD#55 condition position: `while`/`until` and the operand of `!` ---
# DD#55 tabulates `cmd || action` / `if cmd` / `while cmd` together as
# explicit handling, and defines condition position as "if / while / until
# test, operand of `!`". A failed redirect open there is CAUGHT, silently
# (caught failures print no diagnostic — see abort-diagnostics-remaining).
# Both backends previously got two of these wrong: native treated only
# Branch-terminated blocks as conditions, so `while` aborted; and both took
# the DD#70 `__negate_test_result` sigil for the condition, so `!` aborted.
if in_modes native,c; then
  # 55.67 failed open in a `while` condition ends the loop, body never runs
  run_sash 'int n = 0
while echo hi > /nonexistent_dir_55/f { n = $(( n + 1 )); if (( n > 2 )) { break } }
echo "done n=$n"'
  [[ $rc = 0 && $out = "done n=0" && -z $err && -z $rerr ]]; chk 55.67

  # 55.68 same for a read in a `while` condition (carries its own redirect).
  # `while read` is rejected outright (SB5069 — it consumes the whole stream),
  # so the loop form of the read family is `while readline`.
  run_sash 'int n = 0
while readline v < /nonexistent_dir_55/f { attest v [[ -n $v ]]; n = $(( n + 1 )); echo "got $v" }
echo "done n=$n"'
  [[ $rc = 0 && $out = "done n=0" && -z $err && -z $rerr ]]; chk 55.68

  # 55.69 `until` is condition position too, and terminates in a Loop just as
  # `while` does — the failed open is caught, so the condition is false and
  # the body DOES run (once, then break)
  run_sash 'int n = 0
until echo hi > /nonexistent_dir_55/f { n = $(( n + 1 )); break }
echo "done n=$n"'
  [[ $rc = 0 && $out = "done n=1" && -z $err && -z $rerr ]]; chk 55.69

  # 55.70 operand of `!`: the open fails, `!` negates the false condition,
  # so the then-branch runs. The sigil sits AFTER the command it negates
  # (DD#70), so a naive "last instruction" rule marks the wrong one.
  run_sash 'if ! echo hi > /nonexistent_dir_55/f { echo caught }
echo done'
  [[ $rc = 0 && $out = "caught
done" && -z $err && -z $rerr ]]; chk 55.70

  # 55.71 ... and with a read as the negated operand
  run_sash 'if ! read a < /nonexistent_dir_55/f { echo caught }
echo "n=$(len a)"'
  [[ $rc = 0 && $out = "caught
n=0" && -z $err && -z $rerr ]]; chk 55.71

  # 55.72 REGRESSION GUARD: widening condition position must not swallow an
  # uncaught failure. A bare redirect still aborts with its diagnostic.
  run_sash 'echo hi > /nonexistent_dir_55/f
echo done'
  [[ $rc != 0 && -z $out && -z $err \
     && $rerr == *"/nonexistent_dir_55/f: No such file or directory"* ]]; chk 55.72
else
  skip_chk 55.67; skip_chk 55.68; skip_chk 55.69
  skip_chk 55.70; skip_chk 55.71; skip_chk 55.72
fi

set -e
tally
