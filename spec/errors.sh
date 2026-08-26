#!/bin/bash
# spec/errors.sh — error model tests (DD#21, DD#25, DD#55)
set -eu
. common.sh
setup errors
plan=87
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

# 55.7 local x=$(cmd) checks exit status (taint orthogonal — don't reference x)
run_sash 'fn f { local x=$(/bin/false); }; f'
[[ $rc != 0 && -z $err ]]; chk 55.7

# 55.8 x=$(cmd) where cmd fails aborts (taint orthogonal — don't reference x)
run_sash 'x=$(/bin/false)'
[[ $rc != 0 && -z $err ]] \
  && { ! in_modes native || [[ $rerr == *"/bin/false: command failed (exit 1)"* ]]; }; chk 55.8

# 55.9 x=$(- cmd) captures output (discards exit). The output is external →
# tainted, so it must be blessed before use. (Was a pre-existing taint hole:
# the `-` suppression wrapper hid the external from the first-stmt-only taint
# scan — closed 2026-08-19 by the multi-statement-cmdsub taint fix.)
run_sash 'x=$(- /bin/echo hello); bless x [[ -n $x ]]; echo $x'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 55.9

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
fi

# 55.31 CAUGHT exec failure (`|| …`) → only the child's errno detail; the
#       parent does NOT abort, so no command-failed line.
if in_modes native; then
    run_sash '/nonexistent_binary_xyz || echo caught'
    [[ $rc = 0 && $out = "caught" && -z $err \
       && $rerr == *"/nonexistent_binary_xyz: No such file or directory"* \
       && $rerr != *"command failed"* ]]; chk 55.31
fi

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

    # 55.30 open failure is catchable with || too (symmetry with if), no diag
    run_sash 'echo hi > /nonexistent_dir_55/f || echo caught'
    [[ $rc = 0 && $out = "caught" && -z $err && -z $rerr ]]; chk 55.30
    else
        skip_chk 55.24; skip_chk 55.25; skip_chk 55.26; skip_chk 55.30
    fi
else
    skip_chk 55.20; skip_chk 55.21; skip_chk 55.22; skip_chk 55.23
    skip_chk 55.27; skip_chk 55.28; skip_chk 55.29
    skip_chk 55.24; skip_chk 55.25; skip_chk 55.26; skip_chk 55.30
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
run_sash 'false || x=set; echo done'
[[ $rc = 0 && $out = "done" && -z $err ]]; chk e.br10

# ── DD#70 (A)/(B) error-model contract ───────────────────────────────
# (B) must-consume booleans: `match` (regex.sh 63.1c-2f) and a bare
# `[[ … ]]`. A bare boolean is a compile error; if/&&/|| consume it;
# `|| :` discards it; `-` on a (B) construct is a compile error.
# SB4018/SB4019 are transform diagnostics — native-only (not surfaced on
# --emit-c / --janet yet; see c-janet-dd55-abort-channel).

# e.b70.tbare a bare `[[ … ]]` result is unused → SB4018
# e.b70.tdash `-` on a `[[ … ]]` (a must-consume boolean) → SB4019
if in_modes native,c; then
    run_sash '[[ a = a ]]'
    [[ $rc != 0 && -z $out && $err == *SB4018* ]]; chk e.b70.tbare

    run_sash '- [[ a = a ]]'
    [[ $rc != 0 && -z $out && $err == *SB4019* ]]; chk e.b70.tdash
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
# (reviewability, SB4040); `- !` is SB4019. Runtime negation is native-only
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
# bng.5 `- !` cannot suppress a boolean → SB4019 (was the garbage SB4014)
run_sash '- ! /bin/false'
[[ $rc != 0 && -z $out && $err == *SB4019* ]]; chk bng.5

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

set -e
tally
