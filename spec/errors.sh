#!/bin/bash
# spec/errors.sh — error model tests (DD#21, DD#25, DD#55)
set -eu
. common.sh
setup errors
plan=725
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

# 0.5q the bare `return` spelling of SB4017. 55.14 below covers `return 0`;
# the rule's test is `s.is_empty() || s == "0"`, so the empty return is the
# other half of one arm and nothing reached it.
run_sash 'fn f -> int {
    return
}
int x=$((f))
echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB4017* ]]; chk 0.5q

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

# --- validator walk: each diagnostic exactly once ---------------------
#
# `validate_stmt_ctx` recurses into every nested statement AND used to run a
# *deep* expression walk over the whole statement, so an expression nested N
# levels deep was validated N+1 times and its diagnostic printed N+1 times.
# The per-statement walk is now `walk_own_exprs` (direct exprs only); the
# statement recursion supplies the rest. Journal 308.

# 0.7 baseline: a rejected env var at top level reports once.
run_sash 'echo $HOME'
n=$(grep -c SB5018 <<< "$err")
[[ $rc != 0 && -z $out && $n = 1 ]]; chk 0.7

# 0.8 one level of nesting (a function body) still reports once, not twice.
run_sash 'fn f { echo $HOME }
f'
n=$(grep -c SB5018 <<< "$err")
[[ $rc != 0 && -z $out && $n = 1 ]]; chk 0.8

# 0.9 two levels (function + if) still reports once, not three times.
run_sash 'fn f { if /bin/true; then echo $HOME; fi }
f'
n=$(grep -c SB5018 <<< "$err")
[[ $rc != 0 && -z $out && $n = 1 ]]; chk 0.9

# 0.10 the walk must not LOSE a diagnostic: a rejected var reachable only
# through a construct the statement recursion reaches via `child_bodies`
# (a `case` pattern, a `for` word list, an `if` condition) is still caught.
run_sash 'for x in $HOME; do /bin/echo "$x"; done'
n=$(grep -c SB5018 <<< "$err")
[[ $rc != 0 && -z $out && $n = 1 ]]; chk 0.10

# 0.11 the subject and an arm pattern of a `[[ ]]` dispatch (DD#66) are both
# direct exprs of the same statement; each is validated exactly once, so two
# distinct rejected vars give exactly two diagnostics.
run_sash '[[ $HOME == $USER { /bin/echo a } ]]'
n=$(grep -c SB5018 <<< "$err")
[[ $rc != 0 && -z $out && $n = 2 ]]; chk 0.11

# 21.1 - cmd suppresses exit status
run_sash '- /bin/false; echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 21.1

# 21.2 per-segment pipeline suppression
run_sash '- /bin/false | echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 21.2

# 21.3 unchecked pipeline segment flagged (pipefail always on)
run_sash '/bin/false | echo ok'
[[ $rc != 0 && -z $err ]]; chk 21.3

# 25.1 integer overflow traps at runtime.
run_sash 'int x=9223372036854775807; (( x += 1 )); echo $x'
[[ $rc != 0 && -z $err ]]; chk 25.1

# 55.1 external cmd nonzero aborts (+ DD#55 diagnostic to stderr in native)
run_sash '/bin/false
echo should_not_reach'
[[ $rc != 0 && $out != *"should_not_reach"* && -z $err ]] \
  && [[ $rerr == *"/bin/false: command failed (exit 1)"* ]]; chk 55.1

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
  && [[ $rerr == *"/bin/false: command failed (exit 1)"* ]]; chk 55.8

# 55.9 x=$(- cmd) captures output (discards exit). The output is external →
# tainted, so it must be attested before use. (Was a pre-existing taint hole:
# the `-` suppression wrapper hid the external from the first-stmt-only taint
# scan — closed 2026-08-19 by the multi-statement-cmdsub taint fix.)
run_sash 'x=$(- /bin/echo hello); attest x [[ -n $x ]]; echo $x'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 55.9

# 55.9b x=$(- cmd) does NOT abort when the captured command FAILS (55.9 only
# tested a succeeding /bin/echo). DD#55: `-` discards the exit status, so a
# failed `$(- /bin/false)` yields empty output and continues — mirroring the
# statement-position `- /bin/false` (55.4).
run_sash 'x=$(- /bin/false); attest x [[ -z $x ]]; echo "survived[$x]"'
[[ $rc = 0 && $out = "survived[]" && -z $err ]]; chk 55.9b

# 55.10 pipefail always on
run_sash '/bin/false | /bin/true'
[[ $rc != 0 && -z $err ]] \
  && [[ $rerr == *"/bin/false: command failed (exit 1)"* ]]; chk 55.10

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
  && [[ $rerr == *"/bin/false: command failed (exit 1)"* ]]; chk 55.35

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

# 55.13 bare call of non-void function → compile error (SB4016). Named the
# code 2026-10-14: `-n $err` alone is satisfied by ANY diagnostic, so this
# passed whether or not the rule it was filed for still existed.
run_sash 'fn f -> int { return 42; }
f'
[[ $rc != 0 && -z $out && $err == *SB4016* ]]; chk 55.13

# 55.14 return 0 as only return path → compile error (SB4017), likewise named.
# This was SB4017's ONLY live assertion in the suite — 0.5 near the top of the
# file is a negative (the rule must not fire spuriously) and would pass if the
# rule were deleted outright.
run_sash 'fn f -> int { return 0; }'
[[ $rc != 0 && -z $out && $err == *SB4017* && $err == *'only returns 0'* ]]; chk 55.14

# 55.15 - cmd; return $? → compile error ($? unavailable after -)
run_sash 'fn f -> int { - /bin/false; return $?; }'
[[ $rc != 0 && -z $out && -n $err ]]; chk 55.15

# 55.16 background exec failure — the CHILD reports the exec errno at the
#       point of failure and exits 127; both messages are the child's.
#       The parent exits 0, and that is DD#126 §2: `- wait` discards the
#       *pid*, and the reaped status now goes to the `$(waitstatus)` latch
#       rather than to `$?`. A parent that never reads the latch has said
#       "I do not care" twice, so it exits on its own status. 126.36 is the
#       spelling that propagates the failure.
#       The command is staged, compiled against, then deleted before the
#       binary runs (SB5095 rejects a path that was never there).
run_sash_vanish 'SPEC_VANISH_CMD &
- wait'
[[ $rc = 0 && -z $err ]] \
  && [[ $rerr == *": No such file or directory"* ]] \
  && [[ $rerr == *"command failed (exit 127)"* ]]; chk 55.16

# 55.30 fg UNCAUGHT exec failure → child errno detail + parent uncaught
#       abort (both lines). DD#55: child owns exec errors; parent's
#       command-failed fires because it is uncaught.
run_sash_vanish 'SPEC_VANISH_CMD
echo unreached'
[[ $rc != 0 && -z $out \
   && $rerr == *": No such file or directory"* \
   && $rerr == *": command failed (exit 127)"* ]]; chk 55.30

# 55.31 CAUGHT exec failure (`|| …`) → only the child's errno detail; the
#       parent does NOT abort, so no command-failed line.
run_sash_vanish 'SPEC_VANISH_CMD || echo caught'
[[ $rc = 0 && $out = "caught" && -z $err \
   && $rerr == *": No such file or directory"* \
   && $rerr != *"command failed"* ]]; chk 55.31

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
run_sash '/bin/sleep 0 & p=$!; - wait $p; echo done'
[[ $rc = 0 && $out = "done" && -z $err ]]; chk 55.19

# ── DD#55 syscall-abort: internal builtins must thread real status ──
# Internal syscall wrappers (cd→chdir, redirect→open, echo→write) must
# (a) set their real exit status, visible to catching positions, and
# (b) abort on failure unless caught (if/||) — or, for a command's own
# verdict (cd / echo's write), suppressed by `-`. A redirect open(2) is
# the implicit `exec N>file` that runs FIRST, so `-` (which binds to the
# command) does NOT suppress it; and a failed open must never silently
# misroute output to the original fd.
# The c backend now implements the DD#55 cd + echo-write(2) syscall aborts
# with descriptive diagnostics (2026-08-20), and the harness captures c's
# runtime stderr into $rerr. Redirect open(2)-failure abort is still c-TODO
# (nested guard below).
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

# --- DD#53: `cd` arity (SB4066). The environment is a separate namespace, so
# there is no $HOME for a bare `cd` to fall back on. Both backends used to
# answer the misuse silently: a bare `cd` set $? = 0 without moving (bash goes
# to $HOME) and a second argument was dropped without a word.

# cd.1 bare `cd` is rejected — it has nowhere to go
run_sash 'cd
echo after'
[[ $rc != 0 && -z $out && $err == *SB4066* ]]; chk cd.1

# cd.2 a second argument is rejected, not silently dropped
run_sash 'cd /tmp /var
echo after'
[[ $rc != 0 && -z $out && $err == *SB4066* ]]; chk cd.2

# cd.3 unlike `close`'s fd (SB4052), the directory need not be a literal —
# a path is not a runtime-chosen descriptor
run_sash 'str d=/tmp
cd $d
echo after'
[[ $rc = 0 && $out = "after" && -z $err && -z $rerr ]]; chk cd.3

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
# Nested c backend: redirect open(2)-failure abort is now implemented
# (emit_redirects_guarded + emit_command skip-body + in_condition, so
# `- echo > /bad` still aborts while `if`/`||` catch).
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

# --- DD#70: multiple redirects are performed left to right and processing
# STOPS at the first failed open(2). Both backends used to open every redirect
# eagerly and then OR-fold the failures, which cost two bash divergences: a
# later redirect still created/truncated its file, and the diagnostic named
# the FIRST open candidate rather than the one that actually failed.

# rd.1 the diagnostic names the redirect that actually failed, not the first
# one attempted (was: "rd1_ok.txt", a file that opened fine)
run_sash 'echo hi > rd1_ok.txt 2> /nonexistent_dir_55/f
echo after'
[[ $rc != 0 && -z $out && -z $err \
   && $rerr == *"test_bin: /nonexistent_dir_55/f: No such file or directory"* ]]; chk rd.1

# rd.2 a redirect AFTER a failed open does not run, so its file is never
# created (bash stops at the first failure; sash used to clobber it)
rm -f rd2_made.txt
run_sash 'echo hi > /nonexistent_dir_55/f 2> rd2_made.txt
echo after'
[[ $rc != 0 && ! -f rd2_made.txt \
   && $rerr == *"test_bin: /nonexistent_dir_55/f: No such file or directory"* ]]; chk rd.2

# rd.3 when the FIRST redirect is the one that fails, it is still the one
# reported — the ordering guard cuts both ways
run_sash 'echo hi > /nonexistent_dir_55/f 2> rd3_ok.txt
echo after'
[[ $rc != 0 && -z $out \
   && $rerr == *"test_bin: /nonexistent_dir_55/f: No such file or directory"* ]]; chk rd.3

# rd.4 two good redirects still both apply (the guard must not suppress a
# later open when nothing has failed)
rm -f rd4_a.txt rd4_b.txt
run_sash '/bin/sh -c "echo out; echo err 1>&2" > rd4_a.txt 2> rd4_b.txt
echo after'
[[ $rc = 0 && $out = "after" && -z $err \
   && "$(cat rd4_a.txt)" = "out" && "$(cat rd4_b.txt)" = "err" ]]; chk rd.4

# --- DD#120: the setup-failure diagnostic is the INVOKER's, so a redirect
# written on the command never captures it. A redirect establishes a channel
# for *that command's* output, and a command whose redirect could not be set
# up produced none.
#
# This is a deliberate divergence from bash, dash and busybox alike, all of
# which write the message through the redirects that already succeeded.
#
# Read the assertions carefully. `$rerr` holding the message is NOT on its own
# a discriminator: the pre-DD#120 code ALSO left `$rerr` empty only in the
# cases where the capture succeeded, so a half-right implementation can pass
# one half. Each check below pins BOTH sides — the message on the invoker's
# stderr AND the capture file present but EMPTY. Present-but-empty is the
# sharp edge: it proves the `2>` really was applied (so the message could have
# landed there) and that the diagnostic still did not go into it.

# rd.5 a succeeding `2>` before the failing open does NOT capture the
# diagnostic — it goes to the invoker's stderr
rm -f rd5_cap.txt
run_sash '/bin/echo hi 2> rd5_cap.txt > /nonexistent_dir_55/f
echo after'
[[ $rc != 0 && -z $out && -z $err    && $rerr == *"/nonexistent_dir_55/f: No such file or directory"*    && -f rd5_cap.txt && ! -s rd5_cap.txt ]]
chk rd.5

# rd.6 the control for rd.5: with the failing open FIRST the `2>` never runs
# (rd.2's ordering guard), so the message goes to the real stderr and the
# capture file is never created. Without this pair rd.5 could be satisfied by
# a compiler that simply always wrote to the last-named file.
rm -f rd6_cap.txt
run_sash '/bin/echo hi > /nonexistent_dir_55/f 2> rd6_cap.txt
echo after'
[[ $rc != 0 && ! -f rd6_cap.txt    && $rerr == *"/nonexistent_dir_55/f: No such file or directory"* ]]; chk rd.6

# rd.7 the same rule for a function redirect SCOPE, which is a different
# emitter in both backends (native emit_redirect_push / c emit_redirect_push).
rm -f rd7_cap.txt
run_sash 'f() { echo hi; }
f 2> rd7_cap.txt > /nonexistent_dir_55/f
echo after'
[[ $rc != 0 && -z $out && -z $err    && $rerr == *"/nonexistent_dir_55/f: No such file or directory"*    && -f rd7_cap.txt && ! -s rd7_cap.txt ]]
chk rd.7

# rd.8 a redirect that DUPS rather than opens is equally powerless to capture
# it: `2>&1 1> /bad` points fd 2 at the original stdout, which the harness
# reads as $out. Pins that the rule is about the invoker's channel, not about
# files specifically — $out must stay clean and the message must be on $rerr.
run_sash '/bin/echo hi 2>&1 1> /nonexistent_dir_55/f
echo after'
[[ $rc != 0 && -z $err && -z $out    && $rerr == *"/nonexistent_dir_55/f: No such file or directory"* ]]; chk rd.8

# rd.9 a dup AFTER a successful open — the everyday `> f 2>&1`. The open points
# fd 1 at the file, then `2>&1` points fd 2 at the same description, so both
# streams land in it. Every `2>&1` in the suite until now was the harness's own
# bash, never a compiled program, and the dup is emitted under the preceding
# open's failure guard (bash abandons a redirect list at the first failed open)
# — so this ordering was the one guarded-dup path nothing reached.
rm -f rd9.txt
run_sash '/bin/sh -c "echo out; echo err 1>&2" > rd9.txt 2>&1
echo after'
[[ $rc = 0 && $out = "after" && -z $err \
   && "$(cat rd9.txt)" = "out
err" ]]; chk rd.9

# rd.10 the ordering control for rd.9: with the dup FIRST, `2>&1` copies fd 1
# while it is still the invoker's stdout, so stderr goes there and only stdout
# is redirected into the file. Without this pair rd.9 is also satisfied by an
# implementation that merges the two streams whatever the order.
rm -f rd10.txt
run_sash '/bin/sh -c "echo out; echo err 1>&2" 2>&1 > rd10.txt
echo after'
[[ $rc = 0 && $out = "err
after" && -z $err && "$(cat rd10.txt)" = "out" ]]; chk rd.10

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

# ── 71.1-71.9: a typed declaration is a value binding too (DD#71) ─────────
# DD#71: "an assignment (and typed declaration) is a value binding, not a
# command", so it cannot be a `&&`/`||` operand, a pipeline stage or a
# background job. A declaration took every word to the end of the statement as
# an assignment, so `int n=1 && …` read `&&` as a name and said SB4014
# "expected variable name in declaration" on 71.1-71.4. It now stops at a list
# or pipe operator, as an untyped assignment does, and gets the same verdicts
# (batch-531 B28, journal 537).
# 71.1 a typed declaration as a `&&` operand gets e.br6's verdict
run_sash 'int n=1 && /bin/echo ok
echo "$n"'
[[ $rc != 0 && -z $out \
   && $err == *':1:1: error [SB4025] a declaration is a value binding, not a boolean or exit status'* ]]; chk 71.1
# 71.2 ... and as a `||` operand
run_sash 'int n=1 || /bin/echo ok
echo "$n"'
[[ $rc != 0 && -z $out \
   && $err == *':1:1: error [SB4025] a declaration is a value binding, not a boolean or exit status'* ]]; chk 71.2
# 71.3 as a pipeline stage it is SB4022: the binding would land in the
#      stage's child
run_sash 'int n=1 | /bin/cat
echo "$n"'
[[ $rc != 0 && -z $out \
   && $err == *':1:1: error [SB4022] a variable declaration isn'\''t allowed as a pipeline stage'* ]]; chk 71.3
# 71.4 under `&` it is SB4024, for the same reason
run_sash 'int n=1 &
echo "$n"'
[[ $rc != 0 && -z $out \
   && $err == *':1:1: error [SB4024] a variable declaration isn'\''t allowed in a background job'* ]]; chk 71.4
# 71.5 (positive) a `|` with no blank before it is still part of the value
#      word; only an operator token ends the declaration
run_sash 'str s=a|b
echo "$s"'
[[ $rc = 0 && $out = "a|b" && -z $rerr ]]; chk 71.5
# 71.6-71.9 (journal 537): in a control-flow header the `{` that opens the
# body also ends a declaration, as it ends a command's words there (DD#68).
# The declaration took the `{` as its next name and said SB4014 "expected
# variable name in declaration"; DD#88 makes a typed declaration in a
# condition SB4025, which the bash-style `if int n=$(cmd); then` already was.
# 71.6 an `if` declaration with a capture
run_sash 'if int n=$(/bin/echo 1) { echo "$n" }'
[[ $rc != 0 && -z $out \
   && $err == *':1:4: error [SB4025] a declaration captures a value, but a capture-condition branches on an exit status'* ]]; chk 71.6
# 71.7 a `while` declaration without one
run_sash 'while int n=1 { break }'
[[ $rc != 0 && -z $out \
   && $err == *':1:7: error [SB4025] a declaration is a value binding, not a boolean or exit status'* ]]; chk 71.7
# 71.8 an `elif` header
run_sash 'if /bin/false { echo a } elif str s=$(/bin/echo 1) { echo "$s" }'
[[ $rc != 0 && -z $out \
   && $err == *':1:31: error [SB4025] a declaration captures a value, but a capture-condition branches on an exit status'* ]]; chk 71.8
# 71.9 (control) outside a header the `{` does not end a declaration, so this
#      is still SB4014. Whether it should split into a declaration and a
#      block, as `n=1 { … }` splits into an assignment and a block and
#      `echo a { … }` into a command and a block (bc.g6), is
#      q-statement-form-residuals; this pins only that 71.6-71.8 left the
#      statement level alone.
run_sash 'int n=1 { echo hi }
echo "$n"'
[[ $rc != 0 && -z $out \
   && $err == *':1:1: error [SB4014] expected variable name in declaration'* ]]; chk 71.9

# ── DD#70 (A)/(B) error-model contract ───────────────────────────────
# (B) must-consume booleans: `match` (regex.sh 63.1c-2f) and a bare
# `[[ … ]]`. A bare boolean is a compile error; if/&&/|| consume it;
# `|| :` and `-` both discard it (DD#70 addendum 2026-09-09 — `-` is an
# explicit discard, so it carries the same weight as `|| :`).
# SB4018/SB4019 are transform diagnostics — native-only (not surfaced on
# --emit-c yet).

# e.b70.tbare a bare `[[ … ]]` result is unused → SB4018
# e.b70.tdash `-` on a `[[ … ]]` discards the result — legal
run_sash 'v=a; [[ $v == a ]]'
[[ $rc != 0 && -z $out && $err == *SB4018* ]]; chk e.b70.tbare

run_sash 'v=a
- [[ $v == a ]]
echo after'
[[ $rc = 0 && $out = "after" && -z $err ]]; chk e.b70.tdash

# DD#70 (B) applies to `!` too, via a DIFFERENT function
# (`check_negation_must_consume`) than the `[[ ]]`/match cases above
# (`check_boolean_must_consume`). Every SB4018 spec covered the latter.

# e.b70.nbare a bare `! X` result is unused → SB4018
run_sash '! true
echo after'
[[ $rc != 0 && -z $out && $err == *SB4018* ]]; chk e.b70.nbare

# e.b70.nif a consuming position takes it
run_sash 'if ! true; then echo T; else echo F; fi'
[[ $rc = 0 && $out = "F" && -z $err ]]; chk e.b70.nif

# e.b70.ndash `- !` is SB4019: `-` and `!` don't combine on one stage, in
# either order (DD#70 2026-10-26 reverses the 2026-09-09 addendum that made
# it legal). SB4019 has several sites, so the message pins this one.
run_sash '- ! true
echo after'
[[ $rc != 0 && -z $out && $err == *SB4019* \
   && $err == *'`-` cannot suppress a `!`-negated command'* ]]; chk e.b70.ndash

# e.b70.tdisc `[[ … ]] || :` explicitly discards the boolean → ok
run_sash 'v=a; [[ $v == a ]] || :
echo done'
[[ $rc = 0 && $out = "done" && -z $err ]]; chk e.b70.tdisc

# e.b70.tand a `[[ … ]] && action` consumes it (runs on true)
run_sash 'v=a; [[ $v == a ]] && echo hit'
[[ $rc = 0 && $out = "hit" && -z $err ]]; chk e.b70.tand

# e.b70.tor a `[[ … ]] || action` consumes it (runs on false)
run_sash 'v=a; [[ $v == b ]] || echo miss'
[[ $rc = 0 && $out = "miss" && -z $err ]]; chk e.b70.tor

# A guard whose test is FALSE skips the action and leaves the test's own
# status behind, so a program ending in an unfired guard exits non-zero.
# `[[ ]]` publishes `$?` like every other status producer; the three forms
# below must agree. Provisional (DG 2026-09-15) — a case can be made for
# reserving non-zero exits for `exit` and aborts, which would flip all three
# together.
# e.b70.tandf an unfired `[[ … ]] &&` guard exits 1, not 0
run_sash 'v=a; [[ $v == b ]] && echo hit'
[[ $rc = 1 && -z $out && -z $err ]]; chk e.b70.tandf
# e.b70.tandfa `(( … )) &&` agrees
run_sash '(( 1 == 2 )) && echo hit'
[[ $rc = 1 && -z $out && -z $err ]]; chk e.b70.tandfa
# e.b70.tandfx an external command's guard agrees
run_sash 'grep -q zzz /dev/null && echo hit'
[[ $rc = 1 && -z $out && -z $err ]]; chk e.b70.tandfx
# e.b70.tandfi the same false test under `if` still exits 0 (no else)
run_sash 'v=a; if [[ $v == b ]]; then echo hit; fi'
[[ $rc = 0 && -z $out && -z $err ]]; chk e.b70.tandfi

# ── `!` precedence + pipeline + suppress (DG 2026-08-19). `!` binds its
# immediate unit (bash-consistent `(!X) && Y`); it cannot negate a pipeline
# (reviewability, SB4040). The parse-time rejects are all modes. ──
# bng.1 `! X && Y` reassociates to `(!X) && Y` (was wrongly SB4018)
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
# bng.4 `!` on a pipeline is rejected — reviewer can't see `!`'s scope (SB4040)
run_sash 'if ! /bin/true | /bin/cat; then echo x; fi'
[[ $rc != 0 && -z $out && $err == *SB4040* ]]; chk bng.4
# bng.5 `- !` is SB4019, the mirror of bng.11's `! -` (DD#70 2026-10-26).
# `!` inverts a status and `-` discards it, so asking for both is
# contradictory; a plain `- cmd` says what was meant.
run_sash '- ! /bin/false
echo after'
[[ $rc != 0 && -z $out && $err == *SB4019* \
   && $err == *'`-` cannot suppress a `!`-negated command'* ]]; chk bng.5

# ── DD#70 terminal-segment negation (DG 2026-08-31): `!` may negate the FINAL
# pipeline stage (`a | ! b`), which supplies the pipeline's result — the same
# `!` a standalone simple command allows. Its status is inverted (0↔nonzero)
# before the DD#55 worst-of. A `!` on a non-terminal stage is SB4040 (a
# plumbing status a reviewer can't trace). Both backends implement it. ──
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
# bng.11 `! -` is rejected: `-` has already discarded the result, so `!` has
# nothing left to invert. A composition error, not a must-consume one; its
# mirror `- !` is SB4019 too (bng.5).
run_sash '/bin/echo abc | ! - /bin/grep -q z'
[[ $rc != 0 && -z $out && $err == *SB4019* ]]; chk bng.11
# bng.12 `return ! X | Y` — a non-terminal `!` in a return producer → SB4040
#        (was the confusing bare-`!` SB4014)
run_sash 'f() { return ! /bin/echo abc | /bin/grep -q z }
if f; then echo T; fi'
[[ $rc != 0 && -z $out && $err == *SB4040* ]]; chk bng.12
# bng.13-bng.25 (journal 537; DD#70 2026-10-26): `-` and `!` on one stage are
# SB4019 wherever they meet, an unconsumed terminal `!` is SB4018, and a `!` on
# a `{ }` group in a condition is still a group condition.
# bng.13 `! - a | b` is bng.11's `! -` at the head of a pipeline; it was
#        SB4018 (batch-531 n1)
run_sash '! - /bin/echo a | /bin/cat
echo after'
[[ $rc != 0 && -z $out \
   && $err == *':1:1: error [SB4019] `!` cannot negate a `-`-suppressed command'* ]]; chk bng.13
# bng.14 ... and in a condition, where it compiled (batch-531 n5)
run_sash 'if ! - /bin/echo a | /bin/false { echo y } else { echo n }
echo after'
[[ $rc != 0 && -z $out \
   && $err == *':1:4: error [SB4019] `!` cannot negate a `-`-suppressed command'* ]]; chk bng.14
# bng.15 `a | - ! b` is bng.5's `- !` on the final stage; it was the
#        compound-stage SB4022 (batch-531 n2)
run_sash '/bin/echo a | - ! /bin/cat
echo after'
[[ $rc != 0 && -z $out \
   && $err == *':1:15: error [SB4019] `-` cannot suppress a `!`-negated command'* ]]; chk bng.15
# bng.16 `- !` on a `{ }` group, which compiled
run_sash '- ! { /bin/true }
echo after'
[[ $rc != 0 && -z $out \
   && $err == *':1:1: error [SB4019] `-` cannot suppress a `!`-negated command'* ]]; chk bng.16
# bng.17 a terminal `!` that nothing consumes is SB4018, the verdict a
#        standalone `! cmd` gets (DD#70 2026-08-31: "the same `!` a
#        standalone simple command takes"). It aborted at run time with
#        "/bin/true: command failed (exit 1)". The advice no longer offers
#        `-` or `|| :`, which would discard the status `!` inverted
run_sash '/bin/true | ! /bin/true
echo after'
[[ $rc != 0 && -z $out \
   && $err == *':1:13: error [SB4018] `!` result is unused — consume it with if/`&&`/`||`, or write `- cmd` without the `!`'* ]]; chk bng.17
# bng.18 ... under `&` (was the compound-stage SB4022)
run_sash '/bin/true | ! /bin/true &
- wait
echo after'
[[ $rc != 0 && -z $out \
   && $err == *':1:13: error [SB4018] `!` result is unused — consume it with if/`&&`/`||`, or write `- cmd` without the `!`'* ]]; chk bng.18
# bng.19 ... as a capture's last statement, whose status is unchecked
#        (it compiled)
run_sash 'x=$(/bin/echo a | ! /bin/false)
echo "[${x:!}]"'
[[ $rc != 0 && -z $out \
   && $err == *':1:19: error [SB4018] `!` result is unused — consume it with if/`&&`/`||`, or write `- cmd` without the `!`'* ]]; chk bng.19
# bng.20 (positive) a return producer consumes a terminal `!`
run_sash 'fn p { return /bin/echo abc | ! /bin/grep -q z }
if p { echo t } else { echo f }'
[[ $rc = 0 && $out = "t" && -z $rerr ]]; chk bng.20
# bng.21 `if ! { … }` is a `{ }` group condition, which DD#68 bans; the
#        `!` hid it (it printed `n`)
run_sash 'if ! { /bin/true } { echo y } else { echo n }'
[[ $rc != 0 && -z $out \
   && $err == *':1:6: error [SB4027] a `{ }` group cannot be a condition'* ]]; chk bng.21
# bng.22 (positive) `! { … }` as a `&&` operand at statement level
run_sash '! { /bin/false } && echo y'
[[ $rc = 0 && $out = "y" && -z $rerr ]]; chk bng.22
# bng.23 (positive) `! { … }` as a return producer
run_sash 'fn f { return ! { /bin/true } }
if f { echo y } else { echo n }'
[[ $rc = 0 && $out = "n" && -z $rerr ]]; chk bng.23
# bng.24 (positive) a negated loop is not a group: still a legal condition
#        operand
run_sash 'if ! for x in a b { echo $x } || /bin/true { echo y }'
[[ $rc = 0 && $out = $'a\nb\ny' && -z $rerr ]]; chk bng.24
# bng.25 (positive) `- until` parses like `- while`; it was SB4014
#        "expected expression" at its condition
run_sash 'int n=0
- until (( n >= 2 )) { n+=1 }
echo "$n"'
[[ $rc = 0 && $out = "2" && -z $rerr ]]; chk bng.25

# e.b70.fndisc `f || :` does NOT discard a `-> int` function's return.
# DD#70 offered `|| :` as the discard for an `-> int` function, but by DD#74
# that is `f` as a `||` operand — a boolean context, and an int function's
# `return N` is a value, not a status. DG (2026-09-24) took the rejection:
# one consistent rule, and `- f` (DD#70 addendum) is both available and the
# spelling DG preferred anyway. `- f` is pinned by 74.48.
run_sash 'fn f -> int { return 3 }
f || :
echo done'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk e.b70.fndisc

# (A) abort channel: a CAUGHT failure prints NOTHING to stderr (the
# `<argv0>: …` diagnostic is only for an *uncaught* abort).
# e.a70.silent `if /bin/false` → else branch, no runtime diagnostic
run_sash 'if /bin/false; then echo y; else echo n; fi'
[[ $rc = 0 && $out = "n" && -z $rerr ]]; chk e.a70.silent

# (A) dynamic (interpolated) bad regex → runtime regcomp abort with the
# `regexp error: …` diagnostic (compile-time validation only covers a
# static pattern). Runtime diagnostics are captured into $rerr.
# e.a70.regcomp
run_sash 'p="[bad"
if match hello "$p"; then echo y; else echo n; fi'
[[ $rc != 0 && $rerr == *"regexp error"* ]]; chk e.a70.regcomp

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

# cmdpath.* DD#23/SB5095: an ABSOLUTE command head skips the PATH search, but
# the compiler still knows whether the program is there and whether it can be
# run. Until SB5095 that was the one spelling of "this program is not here"
# nobody checked — a bare name is SB5011 and a relative one SB5051, both at
# compile time, while the path the author was most explicit about compiled
# clean and became a runtime 127.
#
# "Not there" is only one of the faults. A directory, a device node and a
# regular file with no execute bit are all PRESENT, and none of them can be
# `exec`ed; the diagnostic names which, because "command not found" for a
# mode-0644 file sends the reader looking for the wrong thing.
run_sash '/nonexistent_cp991'
[[ $rc != 0 && -z $out && $err == *SB5095* && $err == *"no such file"* ]]; chk cmdpath.1
run_sash '/tmp'
[[ $rc != 0 && -z $out && $err == *SB5095* && $err == *"is a directory"* ]]; chk cmdpath.2
run_sash '/dev/null'
[[ $rc != 0 && -z $out && $err == *SB5095* && $err == *"not a regular file"* ]]; chk cmdpath.3
rm -f cp991_noexec; printf 'not a program\n' > cp991_noexec; chmod 444 cp991_noexec
run_sash "$PWD/cp991_noexec"
[[ $rc != 0 && -z $out && $err == *SB5095* && $err == *"0444"* \
   && $err == *"no execute permission"* ]]; chk cmdpath.4

# Uniform across positions, exactly as SB5051 is: the rule is a property of a
# command head, not of the statement it sits in. `-` and `&` are the two that
# matter most — both are ways of saying "I am not handling this", and neither
# is a licence to name a program that cannot run.
run_sash '/bin/echo hi | /nonexistent_cp991'
[[ $rc != 0 && $err == *SB5095* ]]; chk cmdpath.5
run_sash 'x=$(/nonexistent_cp991)
echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB5095* ]]; chk cmdpath.6
run_sash '- /nonexistent_cp991'
[[ $rc != 0 && $err == *SB5095* ]]; chk cmdpath.7
run_sash '/nonexistent_cp991 &
- wait'
[[ $rc != 0 && $err == *SB5095* ]]; chk cmdpath.8

# `exec` replaces the process image, so a target that cannot run is worse
# there than anywhere else — there is no process left to report it.
run_sash 'exec /nonexistent_cp991'
[[ $rc != 0 && $err == *SB5095* && $err == *"exec"* ]]; chk cmdpath.9
run_sash 'exec /tmp'
[[ $rc != 0 && $err == *SB5095* && $err == *"is a directory"* ]]; chk cmdpath.10

# The controls. Without these, a compiler that rejected every absolute head
# would pass cmdpath.1-10.
run_sash '/bin/echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk cmdpath.11
run_sash 'exec /bin/echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk cmdpath.12
# A symlink to a program IS a program: `metadata`, not `symlink_metadata`.
rm -f cp991_link; ln -s /bin/echo cp991_link
run_sash "$PWD/cp991_link ok"
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk cmdpath.13
# DD#23 names `[[ -x PATH ]]` as the RUNTIME existence test that replaces
# `command -v`. It is a test, not a command head, so SB5095 must not touch it
# — rejecting this would delete the escape the DD prescribes.
run_sash 'if [[ -x /nonexistent_cp991 ]]; then echo y; else echo n; fi'
[[ $rc = 0 && $out = "n" && -z $err ]]; chk cmdpath.14
# The three diagnostics stay distinct: bare is SB5011, relative is SB5051.
run_sash 'nosuchcmd992 x'
[[ $rc != 0 && $err == *SB5011* && $err != *SB5095* ]]; chk cmdpath.15
run_sash './nosuchcmd992 x'
[[ $rc != 0 && $err == *SB5051* && $err != *SB5095* ]]; chk cmdpath.16

# cmdpath.17 the same rule applied to the PATH search itself: a candidate
# that is present but not executable does not win it. Before SB5095 the
# search tested `is_file()`, so an unreadable-as-a-program file early in PATH
# resolved, shadowing the real tool, and the failure arrived at `exec` time
# with the shadowing path baked into the binary.
rm -rf cp991_d1 cp991_d2; mkdir -p cp991_d1 cp991_d2
printf 'not a program\n' > cp991_d1/cptool991; chmod 444 cp991_d1/cptool991
printf '#!/bin/sh\necho second\n' > cp991_d2/cptool991; chmod 755 cp991_d2/cptool991
_cp991_path=$PATH
PATH="$PWD/cp991_d1:$PWD/cp991_d2:$PATH"
run_sash 'cptool991'
PATH=$_cp991_path
[[ $rc = 0 && $out = "second" && -z $err ]]; chk cmdpath.17
rm -rf cp991_d1 cp991_d2 cp991_noexec cp991_link

# ── Compound-condition catch (DD#55; DG 2026-08-19). `&&`/`||` operands in a
# condition are caught (not aborted); a `;` list's non-final member and an
# if-body's final command are bare → they abort.
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

# ── cnd.1-cnd.24: what may stand as a condition (journal 537) ─────────────
# An empty condition compiled (cnd.1-4). A `{ }` group, which
# DD#68 bans as a condition, got in behind `&&`, `elif` or `-` (cnd.5-7), and
# a dispatch form, which SB4028 bans, got in the same ways (cnd.8-10, cnd.19)
# and in the other boolean positions: a `return` producer and an `&&`/`||`
# operand (cnd.14-17, cnd.20, cnd.21). Behind `! -` it drew `-`'s SB4019
# instead (cnd.13). A group that was already rejected is now reported at its
# `{` (cnd.22-24).
# cnd.1 `if then`
run_sash 'if then echo y; fi
echo after'
[[ $rc != 0 && -z $out \
   && $err == *':1:4: error [SB4027] this header has no condition'* ]]; chk cnd.1
# cnd.2 `while do`
run_sash 'while do echo y; break; done
echo after'
[[ $rc != 0 && -z $out \
   && $err == *':1:7: error [SB4027] this header has no condition'* ]]; chk cnd.2
# cnd.3 `until do`
run_sash 'until do echo y; done
echo after'
[[ $rc != 0 && -z $out \
   && $err == *':1:7: error [SB4027] this header has no condition'* ]]; chk cnd.3
# cnd.4 `elif then`
run_sash 'if /bin/false; then echo n; elif then echo y; fi
echo after'
[[ $rc != 0 && -z $out \
   && $err == *':1:34: error [SB4027] this header has no condition'* ]]; chk cnd.4
# cnd.5 a `{ }` group as a condition's `&&` operand is still a group
#       condition (DD#68); it printed `y`
run_sash 'if /bin/true && { /bin/true } { echo y }'
[[ $rc != 0 && -z $out \
   && $err == *':1:17: error [SB4027] a `{ }` group cannot be a condition'* ]]; chk cnd.5
# cnd.6 ... as an `elif` condition
run_sash 'if /bin/false; then echo n; elif { /bin/true; }; then echo y; fi'
[[ $rc != 0 && -z $out \
   && $err == *':1:34: error [SB4027] a `{ }` group cannot be a condition'* ]]; chk cnd.6
# cnd.7 ... behind a `-`; it printed `in clean` and `done` (batch-531
#       B112). The SB4072 that follows it is a known parse-error cascade
run_sash 'int n=0
while - { x=clean; (( n < 1 )) } { n+=1; echo "in $x" }
echo done'
[[ $rc != 0 && -z $out \
   && $err == *':2:9: error [SB4027] a `{ }` group cannot be a condition'* ]]; chk cnd.7
# cnd.8 the dispatch form composes only as a statement (DD#66), never as a
#       condition. Behind `-` it printed `d` and `y`, as cnd.9 and cnd.10 did
run_sash 'str x=a
if - [[ $x == a { echo d } ]] { echo y }'
[[ $rc != 0 && -z $out \
   && $err == *':2:6: error [SB4028] the `[[ ]]` dispatch form is a statement, not a boolean'* ]]; chk cnd.8
# cnd.9 ... as an `elif` condition
run_sash 'str x=a
if /bin/false { echo n } elif [[ $x == a { echo d } ]] { echo y }'
[[ $rc != 0 && -z $out \
   && $err == *':2:31: error [SB4028] the `[[ ]]` dispatch form is a statement, not a boolean'* ]]; chk cnd.9
# cnd.10 ... as a condition's `&&` operand
run_sash 'str x=a
if /bin/true && [[ $x == a { echo d } ]] { echo y }'
[[ $rc != 0 && -z $out \
   && $err == *':2:17: error [SB4028] the `[[ ]]` dispatch form is a statement, not a boolean'* ]]; chk cnd.10
# cnd.11 (positive) a comma list splits into several statements (DD#31),
#        which the parser holds as a group; that group is not a `{ }`
#        condition
run_sash 'int a=0
if (( a++, a > 0 )) { echo "y$a" } else { echo "n$a" }'
[[ $rc = 0 && $out = "y1" && -z $rerr ]]; chk cnd.11
# cnd.12 (positive) ... nor in a `while`
run_sash 'int a=0
while (( a++, a < 3 )) { echo "w$a" }'
[[ $rc = 0 && $out = $'w1\nw2' && -z $rerr ]]; chk cnd.12
# cnd.13 the dispatch is named behind `! -` too, not the `-`
run_sash 'str x=a
! - [[ $x == a { echo d } ]]'
[[ $rc != 0 && -z $out \
   && $err == *':2:1: error [SB4028] the `[[ ]]` dispatch form is a statement, not a boolean'* ]]; chk cnd.13
# cnd.14 a `return` producer supplies a status, so it is a boolean
#        position; it printed `d` and `y`
run_sash 'fn f { return [[ $1 == a { echo d } ]] }
if f a { echo y }'
[[ $rc != 0 && -z $out \
   && $err == *':1:15: error [SB4028] the `[[ ]]` dispatch form is a statement, not a boolean'* ]]; chk cnd.14
# cnd.15 ... and so is an `&&`/`||` operand at statement level; it printed `d`
run_sash 'str x=a
/bin/true && [[ $x == a { echo d } ]]'
[[ $rc != 0 && -z $out \
   && $err == *':2:14: error [SB4028] the `[[ ]]` dispatch form is a statement, not a boolean'* ]]; chk cnd.15
# cnd.16 ... behind a `-` (likewise)
run_sash 'str x=a
/bin/true && - [[ $x == a { echo d } ]]'
[[ $rc != 0 && -z $out \
   && $err == *':2:16: error [SB4028] the `[[ ]]` dispatch form is a statement, not a boolean'* ]]; chk cnd.16
# cnd.17 ... and inside a `return` producer's list; it printed `d` and `y`
run_sash 'fn f { return /bin/true && [[ $1 == a { echo d } ]] }
if f a { echo y }'
[[ $rc != 0 && -z $out \
   && $err == *':1:28: error [SB4028] the `[[ ]]` dispatch form is a statement, not a boolean'* ]]; chk cnd.17
# cnd.18 (positive) a dispatch as a capture's whole body is a statement in a
#        body (DD#86), not a boolean, so it stays legal
run_sash 'str x=a
y=$([[ $x == a { echo d } ]])
echo "[$y]"'
[[ $rc = 0 && $out = "[d]" && -z $rerr ]]; chk cnd.18
# cnd.19 a dispatch in the keyword spelling of `elif` (cnd.9 is the brace
#        one); it printed `d` and `y`
run_sash 'str x=a
if /bin/false; then echo n; elif [[ $x == a { echo d } ]]; then echo y; fi'
[[ $rc != 0 && -z $out \
   && $err == *':2:34: error [SB4028] the `[[ ]]` dispatch form is a statement, not a boolean'* ]]; chk cnd.19
# cnd.20 ... as an `||` operand at statement level (cnd.15 is `&&`); it
#        printed `d`
run_sash 'str x=a
/bin/false || [[ $x == a { echo d } ]]'
[[ $rc != 0 && -z $out \
   && $err == *':2:15: error [SB4028] the `[[ ]]` dispatch form is a statement, not a boolean'* ]]; chk cnd.20
# cnd.21 ... and inside a `while` condition (cnd.10 is an `if`); it printed
#        `d` and `y`. The SB4072 that follows it is a known parse-error
#        cascade
run_sash 'str x=a
while /bin/false || [[ $x == a { echo d } ]] { echo y; break }'
[[ $rc != 0 && -z $out \
   && $err == *':2:21: error [SB4028] the `[[ ]]` dispatch form is a statement, not a boolean'* ]]; chk cnd.21
# cnd.22 a group condition is reported at its `{`, so a brace `elif`'s is
#        on the elif's line; it was reported at the `if` (1:1). The SB4072
#        that follows it is a known parse-error cascade
run_sash 'if /bin/true {
  echo a
} elif {
  echo b
}'
[[ $rc != 0 && -z $out \
   && $err == *':3:8: error [SB4027] a `{ }` group cannot be a condition'* ]]; chk cnd.22
# cnd.23 ... and an `if`'s, which was reported at the `if` (2:1)
run_sash 'echo x
if { /bin/true; }; then echo y; fi'
[[ $rc != 0 && -z $out \
   && $err == *':2:4: error [SB4027] a `{ }` group cannot be a condition'* ]]; chk cnd.23
# cnd.24 a header with no condition before its `{` is reported at that `{`;
#        it was reported at the `if` (2:1)
run_sash 'echo x
if {
  echo b
}'
[[ $rc != 0 && -z $out \
   && $err == *':2:4: error [SB4027] a `{ }` group cannot be a condition'* ]]; chk cnd.24

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
# match takes 2 (string, regex) — one arg is as wrong as three
run_sash 'x=$(match 2); echo $x'
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
run_sash 'x=$(getenv HOME)
attest x [[ -n $x ]]
echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk via.10
run_sash 'p=$(getpid); if (( p > 0 )); then echo ok; fi'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk via.11
# via.12 `argv` is NOT value-only — DD#60's out-param statement form stays legal
run_sash 'array a
argv a
n=$(len a)
echo "n=$n"'
[[ $rc = 0 && $out == n=* && -z $err ]]; chk via.12

# via.13-18 a value intrinsic in statement position is SB5073 **regardless of
# what is installed on the build host**. Before 2026-09-13 these names were in
# neither BUILTINS nor VALUE_ONLY_INTRINSICS, so statement position fell
# through to PATH resolution: join/basename/dirname have coreutils twins and
# were silently exec'd (a real external program, with the real taint
# consequences), while len/toupper/random "passed" only because no binary of
# that name happened to exist. The safe set was host-dependent.
#
# via.13-15 are the three names with a real PATH twin on this host.
run_sash 'basename /a/b'
[[ $rc != 0 && -z $out && $err == *SB5073* && $err != *"sash: bug"* ]]; chk via.13
run_sash 'dirname /a/b'
[[ $rc != 0 && -z $out && $err == *SB5073* && $err != *"sash: bug"* ]]; chk via.14
run_sash 'join a ","'
[[ $rc != 0 && -z $out && $err == *SB5073* && $err != *"sash: bug"* ]]; chk via.15
# via.16-18 are names with no twin — the control. They must reject for the same
# reason (being value-only), not because PATH lookup happened to miss.
run_sash 'random'
[[ $rc != 0 && -z $out && $err == *SB5073* && $err != *"sash: bug"* ]]; chk via.16
run_sash 'len m'
[[ $rc != 0 && -z $out && $err == *SB5073* && $err != *"sash: bug"* ]]; chk via.17
run_sash 'toupper x'
[[ $rc != 0 && -z $out && $err == *SB5073* && $err != *"sash: bug"* ]]; chk via.18

# via.19-20 the value-producing forms are untouched
run_sash 'str b=$(basename /a/b)
echo $b'
[[ $rc = 0 && $out = b && -z $err ]]; chk via.19
run_sash 'str d=$(dirname /a/b)
echo $d'
[[ $rc = 0 && $out = /a && -z $err ]]; chk via.20

# ── via.21-26: `match` is the one value intrinsic that is ALSO a statement ──
# The table above says "none is a statement". `match` is the exception:
# `$(match S P)` captures the matched text, while `match S P ARR` fills ARR and
# yields a status. They are one token apart and their results are INVERTED —
# via.23 gives `[b][]`, via.26 gives `[][b]`.
#
# SB4045's generic "exact arity" sentence denied the second form existed, and
# the repair a reader infers from it — consume the status with `|| :` — is
# precisely the spelling that captures nothing (via.26). `match` is the only
# name needing this: every other name in both tables (`sub`, `gsub`, `len`,
# `strlen`, `keys`) is value-only and draws SB5073 as a statement (via.6-9).
# Front-end — all modes.

# via.21 the bare 3-arg form stays rejected, but names the statement form
run_sash 'array m=()
s=abc
r=$(match "$s" b m)
echo "$r"
echo "${m[0]}"'
[[ $rc != 0 && -z $out && $err == *SB4045* && $err == *"statement form"* ]]; chk via.21
# via.22 ...and drops the sentence that denied that form exists, warning about
# the empty capture instead of inviting it
[[ $err != *"exact arity"* && $err == *"capture the empty string"* ]]; chk via.22

# via.23 the first repair it offers: drop ARR and capture the matched text
run_sash 'array m=()
s=abc
r=$(match "$s" b)
echo "[$r][${m[0]}]"'
[[ $rc = 0 && $out = "[b][]" && -z $err ]]; chk via.23

# via.24 the second: use the statement form outside `$( )`
run_sash 'array m=()
s=abc
if match "$s" b m; then echo "hit=${m[0]}"; fi'
[[ $rc = 0 && $out = "hit=b" && -z $err ]]; chk via.24

# via.25 any OTHER wrong arity for `match` keeps the generic sentence
run_sash 'r=$(match a b c d)
echo "$r"'
[[ $rc != 0 && -z $out && $err == *SB4045* && $err == *"exact arity"* ]]; chk via.25

# via.26 the `|| :` spelling the old text invited: it COMPILES, and captures
# the empty string while filling ARR — via.23 inverted. This is the footgun
# the new wording steers away from, so pin it: if it ever becomes an error,
# that is a deliberate change and this spec must be updated with it.
run_sash 'array m=()
s=abc
r=$(match "$s" b m || :)
echo "[$r][${m[0]}]"'
[[ $rc = 0 && $out = "[][b]" && -z $err ]]; chk via.26

# ── via.27-30: SB5073 must not contradict SB5025 ──────────────────────────
# `keys` is the ONLY collection-valued name in VALUE_ONLY_INTRINSICS (`argv` is
# deliberately absent — it has the DD#60 out-param statement form, via.12). It
# inherited the generic value-intrinsic wording, which advises capturing to a
# scalar: "Capture it first (`x=$(keys m)`) and use `$x` here". That form is
# itself rejected, by SB5025, whose message says in as many words that
# `$(keys m)` "cannot be joined into a scalar". An author following SB5073
# landed on a rule that contradicted it.
#
# `$(keys m)` has exactly one legal position: the sole source of a for-in loop.
# Both SB5073 wordings must name that form. Front-end — all modes.
MAPH='map m
m[a]=1
m[b]=2
echo ${m[a]} ${m[b]}'

# via.27 statement position names the for-in form, not a scalar capture
run_sash "$MAPH
keys m"
[[ $rc != 0 && -z $out && $err == *SB5073* ]]; chk via.27
# via.28 ...and does not advise the spelling SB5025 forbids
[[ $err == *'for k in $(keys m)'* && $err != *'x=$(keys'* ]]; chk via.28

# via.29 one statement among several: same correction
run_sash "$MAPH
for k in \$(keys m; /bin/echo e) { echo \"\$k\" }"
[[ $rc != 0 && $err == *SB5073* && $err == *'for k in $(keys m)'* && $err != *'x=$(keys'* ]]; chk via.29

# via.30 the operand is named, so the suggestion is copy-pasteable
run_sash 'map tbl
tbl[a]=1
echo ${tbl[a]}
keys tbl'
[[ $rc != 0 && -z $out && $err == *'for k in $(keys tbl)'* ]]; chk via.30



# ── `join`'s separator is fixed at compile time (SB5074) ──
# The SSA operand is a compile-time String, so a non-literal separator had
# nothing to lower to — the transform rendered the argument's *source text*
# instead, and `$(join a "$sep")` silently joined with the four characters
# `"$sep"`. A wrong answer, not a crash. Front-end — all modes.

run_sash 'array a=(x y)
sep=-
echo $(join a $sep)'
[[ $rc != 0 && -z $out && $err == *SB5074* ]]; chk jsep.1
run_sash 'array a=(x y)
sep=-
echo $(join a "$sep")'
[[ $rc != 0 && -z $out && $err == *SB5074* ]]; chk jsep.2
run_sash 'array a=(x y)
echo $(join a "$(echo -)")'
[[ $rc != 0 && -z $out && $err == *SB5074* ]]; chk jsep.3

# jsep.4-7 every literal spelling still works, including the empty separator
run_sash 'array a=(x y)
echo $(join a -)'
[[ $rc = 0 && $out = "x-y" && -z $err ]]; chk jsep.4
run_sash 'array a=(x y)
echo $(join a ", ")'
[[ $rc = 0 && $out = "x, y" && -z $err ]]; chk jsep.5
run_sash 'array a=(x y)
echo $(join a "")'
[[ $rc = 0 && $out = "xy" && -z $err ]]; chk jsep.6
run_sash 'array a=(x y)
echo $(join a)'
[[ $rc = 0 && $out = "x y" && -z $err ]]; chk jsep.7

run_sash 'array a=(x y)
str sep=-
echo $(join a "${sep}tail")'
[[ $rc != 0 && -z $out && $err == *SB5074* \
   && $err == *'separator is fixed at compile time'* \
   && $err == *'an expansion cannot be used here'* ]]; chk jsep.8

# ── a string intrinsic on an int operand is a type error (SB4030) ──
# `$(toupper $n)` on an `int n=42` used to yield "42" — the implicit coercion
# the type system exists to prevent — and `$(strlen $n)` answered a question
# about a decimal rendering the source never asked for. `join`/`len` already
# rejected a mistyped operand; these did not. The test is the operand's TYPE,
# not its spelling. Front-end — all modes.

run_sash 'int n=42
echo $(toupper $n)'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk sint.1
run_sash 'int n=42
echo $(tolower $n)'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk sint.2
run_sash 'int n=42
echo $(strlen $n)'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk sint.3
run_sash 'int n=42
echo $(basename $n)'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk sint.4
run_sash 'int n=42
echo $(dirname $n)'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk sint.5

# sint.6-9 every string slot is checked, not just the subject
run_sash 'int n=42
echo $(match $n "4")'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk sint.6
run_sash 's=abc
int n=4
echo $(match $s $n)'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk sint.7
run_sash 'int n=42
echo $(sub $n "4" "9")'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk sint.8
run_sash 's=abc
int n=9
echo $(gsub $s "b" $n)'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk sint.9

# sint.10-12 int-ness is a type, not a spelling: arithmetic, an int-returning
# intrinsic and an `int array` element are all caught
run_sash 'echo $(strlen $(( 1 + 41 )))'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk sint.10
run_sash 'echo $(toupper $(getpid))'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk sint.11
run_sash 'int array a=(1 2 3)
echo $(strlen ${a[0]})'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk sint.12

# sint.13-16 strings are untouched, and double-quoting stays the sanctioned
# conversion (a bare numeric literal is a string in bash, and stays one here)
run_sash 'echo $(toupper abc)'
[[ $rc = 0 && $out = "ABC" && -z $err ]]; chk sint.13
run_sash 'echo $(strlen 42)'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk sint.14
run_sash 'int n=42
echo $(strlen "$n")'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk sint.15
run_sash 'array a=(abc d)
echo $(toupper ${a[0]})'
[[ $rc = 0 && $out = "ABC" && -z $err ]]; chk sint.16

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
run_sash '/bin/sleep 0 &
p="$(getcpid)"
[[ -n $p ]] || :
echo ok'
[[ $rc = 0 && $out = "ok" && -z $rerr ]]; chk 55.55

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
run_sash '/usr/bin/timeout --preserve-status -s TERM 0.1 /bin/sleep 5'
[[ $rc = 143 ]] \
  && [[ $rerr == *"(exit 143)"* ]]; chk 55.57

# 55.58 SIGPIPE -> 141. DD#55 makes the abort itself deliberate ("SIGPIPE is
# the programmer's responsibility"); this pins the STATUS it aborts with.
run_sash '/usr/bin/timeout --preserve-status -s PIPE 0.1 /bin/sleep 5'
[[ $rc = 141 ]] \
  && [[ $rerr == *"(exit 141)"* ]]; chk 55.58

# 55.59 a normal exit code is unaffected by the 128+signum path
run_sash '/bin/sh -c "exit 42"'
[[ $rc = 42 ]] \
  && [[ $rerr == *"(exit 42)"* ]]; chk 55.59

# --- DD#55 + DD#99: the read family runs the same open(2) guard ---
# `read ARR < f` / `readline v < f` carry their own redirects (SsaInst::
# ReadArray/ReadLine), so a failed open aborts instead of silently yielding
# an empty array / empty line — a mistyped path used to be indistinguishable
# from an empty file. Context governs the abort exactly as for `echo > f`
# (55.24-55.26, 55.50).
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

# 55.65/55.66 the guard does not disturb a redirect that opens fine.
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

# --- DD#55 condition position: `while`/`until` and the operand of `!` ---
# DD#55 tabulates `cmd || action` / `if cmd` / `while cmd` together as
# explicit handling, and defines condition position as "if / while / until
# test, operand of `!`". A failed redirect open there is CAUGHT, silently
# (caught failures print no diagnostic — see abort-diagnostics-remaining).
# Both backends previously got two of these wrong: native treated only
# Branch-terminated blocks as conditions, so `while` aborted; and both took
# the DD#70 `__negate_test_result` sigil for the condition, so `!` aborted.
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

  # 55.73-55.75 DD#55 suppression written with no space after the `-`.
  # The lexer emits `Tok::Dash` only when the byte after `-` is a terminator
  # (space, tab, newline, `;`, `|`, `&`, `)`, `}`, `#`). Any other word-break
  # byte sends it to `scan_word`, which yields `Tok::Word("-")` instead --
  # a separate arm in `parse_statement` that read 0 hits against 13,485 in
  # branch coverage (journal 312). It is reachable, just never written.

  # 55.73 bare `[[ … ]]` is SB4018 -- its result is unused
  run_sash '[[ -f /nonexistent_dir_55/f ]]
echo done'
  [[ $rc != 0 && -z $out && $err == *SB4018* ]]; chk 55.73

  # 55.74 ... and `-[[ … ]]`, no space, suppresses it: this only parses if
  # the `Tok::Word("-")` arm routes to `parse_suppressed`
  run_sash '-[[ -f /nonexistent_dir_55/f ]]
echo done'
  [[ $rc = 0 && $out = "done" && -z $err && -z $rerr ]]; chk 55.74

  # 55.75 the same arm on a true test -- suppression does not invert
  run_sash '-[[ -f /etc/passwd ]]
echo done'
  [[ $rc = 0 && $out = "done" && -z $err && -z $rerr ]]; chk 55.75

# --- 55.76-55.87: a body is not in condition position ---------------------
# DD#55's set -e comparison lists "functions called from `if` lose errexit
# for their entire body" among bash's blind spots and says sash has none of
# them; validate's own rule is that the condition context does not cross a
# function-body boundary. The transform disagreed: `in_condition` stayed
# raised while an inlined body was lowered, so every command in the body was
# lowered unchecked and its failure was silently swallowed (`after`, then
# `yes`). The same leak reached `-> int` bodies in an arithmetic condition,
# a captured function, and an anonymous `$( … )` body in a condition
# command's arguments. Each of these printed the swallowed output before the
# fix, in both backends. Journal 542.
  # 55.76 a function called as an `if` condition: its body is checked
  run_sash 'fn h { /bin/false | /bin/cat; echo after; return 0 }
if h { echo yes }
echo done'
  [[ $rc = 1 && -z $out && -z $err \
 && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk 55.76

  # 55.77 ... as the operand of `!`
  run_sash 'fn h { /bin/false | /bin/cat; echo after; return 0 }
! h || echo neg
echo done'
  [[ $rc = 1 && -z $out && -z $err \
 && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk 55.77

  # 55.78 ... where the failure is the tail of a list inside the body
  run_sash 'fn h { : && /bin/false; echo after; return 0 }
if h { echo yes }
echo done'
  [[ $rc = 1 && -z $out && -z $err \
 && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk 55.78

  # 55.79 ... as the left operand of `||`
  run_sash 'fn h { /bin/false | /bin/cat; echo after; return 0 }
h || echo caught
echo done'
  [[ $rc = 1 && -z $out && -z $err \
 && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk 55.79

  # 55.80 ... as a `while` condition
  run_sash 'fn h { /bin/false | /bin/cat; echo after; return 0 }
while h { echo body; break }
echo done'
  [[ $rc = 1 && -z $out && -z $err \
 && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk 55.80

  # 55.81 ... as the final stage of a condition pipeline. The stage child
  # aborts with the diagnostic, so the pipeline fails and the condition is
  # false; the parent carries on. (`h` drains its input so `/bin/echo` can
  # never take a SIGPIPE.)
  run_sash 'fn h { /bin/cat >/dev/null; /bin/false | /bin/cat; echo after; return 0 }
if /bin/echo y | h { echo yes }
echo done'
  [[ $rc = 0 && $out = done && -z $err \
 && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk 55.81

  # 55.82 a `-> int` body called from an arithmetic condition
  run_sash 'fn g -> int { /bin/false | /bin/cat; return 2 }
if (( g > 1 )) { echo yes }
echo done'
  [[ $rc = 1 && -z $out && -z $err \
 && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk 55.82

  # 55.83 a capture assignment inside a condition function's body
  SASH_CFLAGS=--no-taint run_sash 'fn h { x=$(/bin/false); echo "after[$x]"; return 0 }
if h { echo yes }
echo done'
  [[ $rc = 1 && -z $out && -z $err \
 && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk 55.83

  # 55.84 an anonymous `$( … )` body in an `if` command's argument: the
  # capture is an argument, not the condition
  SASH_CFLAGS=--no-taint run_sash 'if /bin/echo "[$(/bin/false | /bin/cat)]" >/dev/null { echo yes }
echo done'
  [[ $rc = 1 && -z $out && -z $err \
 && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk 55.84

  # 55.85 ... in a `while` command's argument
  SASH_CFLAGS=--no-taint run_sash 'while /bin/echo "[$(/bin/false | /bin/cat)]" >/dev/null { echo body; break }
echo done'
  [[ $rc = 1 && -z $out && -z $err \
 && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk 55.85

  # 55.86 ... in the argument of a negated condition command
  SASH_CFLAGS=--no-taint run_sash 'if ! /bin/echo "[$(/bin/false | /bin/cat)]" >/dev/null { echo neg }
echo done'
  [[ $rc = 1 && -z $out && -z $err \
 && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk 55.86

  # 55.87 a captured function in a `[[ ]]` condition operand
  SASH_CFLAGS=--no-taint run_sash 'fn h { /bin/false | /bin/cat; echo hi }
if [[ $(h) == hi ]] { echo yes }
echo done'
  [[ $rc = 1 && -z $out && -z $err \
 && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk 55.87

# --- 55.89-55.106: the condition is the LAST status-setting instruction -----
# DD#55 catches the status of the condition operand and nothing else: a
# command before it keeps its abort, and so does a capture inside a test
# operand, which is an argument rather than the condition. The transform's
# uncheck scan and the backends' redirect-guard scan each listed the
# instructions they recognised and passed over the rest, so a `[[ … ]]`, a
# `readline`, a `match`, an `attest … in` or a `return [[ … ]]` in condition
# position was skipped and the scan settled on the command before it. That
# command then failed silently, its redirect failure was "caught", and
# `while [[ $(cmd) … ]]` looped on a failing capture. Every shape from 55.89
# to 55.104 ran on to `done` before the fix, in both backends. Journal 536.
  # 55.89 a command before an `if [[ … ]]`
  run_sash 'x=a
/bin/sh -c "exit 3"
if [[ $x == a ]] { echo yes }
echo done'
  [[ $rc = 3 && -z $out && -z $err \
 && $rerr == *'/bin/sh: command failed (exit 3)'* ]]; chk 55.89

  # 55.90 a failed capture in the `[[ … ]]` operand itself
  SASH_CFLAGS=--no-taint run_sash 'if [[ $(/bin/false) == "" ]] { echo yes }
echo done'
  [[ $rc = 1 && -z $out && -z $err \
 && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk 55.90

  # 55.91 ... in a `while` condition, which looped on the failure
  SASH_CFLAGS=--no-taint run_sash 'while [[ $(/bin/false) == "" ]] { echo body; break }
echo done'
  [[ $rc = 1 && -z $out && -z $err \
 && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk 55.91

  # 55.92 a command before the `[[ … ]]` left operand of `&&`
  run_sash 'x=a
/bin/sh -c "exit 3"
[[ $x == a ]] && echo yes
echo done'
  [[ $rc = 3 && -z $out && -z $err \
 && $rerr == *'/bin/sh: command failed (exit 3)'* ]]; chk 55.92

  # 55.93 ... before `if ! [[ … ]]` (DD#70: the sigil follows its operand)
  run_sash 'x=a
/bin/sh -c "exit 3"
if ! [[ $x == b ]] { echo yes }
echo done'
  [[ $rc = 3 && -z $out && -z $err \
 && $rerr == *'/bin/sh: command failed (exit 3)'* ]]; chk 55.93

  # 55.94 ... before the dispatch form (DD#66)
  run_sash 'x=a
/bin/sh -c "exit 3"
[[ $x == a { echo a } || { echo other } ]]
echo done'
  [[ $rc = 3 && -z $out && -z $err \
 && $rerr == *'/bin/sh: command failed (exit 3)'* ]]; chk 55.94

  # 55.95 ... before `if readline`
  SASH_CFLAGS=--no-taint run_sash '/bin/sh -c "exit 3"
if readline x < /dev/null { echo "got $x" } else { echo eof }
echo done'
  [[ $rc = 3 && -z $out && -z $err \
 && $rerr == *'/bin/sh: command failed (exit 3)'* ]]; chk 55.95

  # 55.96 ... before `if match`
  run_sash 'x=ab
/bin/sh -c "exit 3"
if match $x "a(b)" m { echo "${m[1]}" }
echo done'
  [[ $rc = 3 && -z $out && -z $err \
 && $rerr == *'/bin/sh: command failed (exit 3)'* ]]; chk 55.96

  # 55.97 ... before `if attest … in` (DD#121)
  run_sash 'array a=(x y)
/bin/sh -c "exit 3"
if attest v in a [[ -n $v ]] { echo all } else { echo some }
echo done'
  [[ $rc = 3 && -z $out && -z $err \
 && $rerr == *'/bin/sh: command failed (exit 3)'* ]]; chk 55.97

  # 55.98 ... before `if read`, DD#99's array read
  SASH_CFLAGS=--no-taint run_sash '/bin/sh -c "exit 3"
if read a < /dev/null { echo "got ${a[0]:-none}" } else { echo eof }
echo done'
  [[ $rc = 3 && -z $out && -z $err \
 && $rerr == *'/bin/sh: command failed (exit 3)'* ]]; chk 55.98

  # 55.99 a failed capture whose target the next condition attests: the
  # capture is a statement of its own, not the condition
  run_sash 'y=$(/bin/sh -c "echo b; exit 3")
if attest y [[ -n $y ]] { echo "yes $y" } else { y=z }
echo "done $y"'
  [[ $rc = 3 && -z $out && -z $err \
 && $rerr == *'/bin/sh: command failed (exit 3)'* ]]; chk 55.99

  # 55.100 a command before `return [[ … ]]` in a condition function
  run_sash 'fn p { /bin/sh -c "exit 3"; return [[ $1 == a ]] }
if p a { echo yes } else { echo no }
echo done'
  [[ $rc = 3 && -z $out && -z $err \
 && $rerr == *'/bin/sh: command failed (exit 3)'* ]]; chk 55.100

  # 55.101 a failed capture in the operand of `return [[ … ]]`
  SASH_CFLAGS=--no-taint run_sash 'fn p { return [[ $(/bin/sh -c "echo a; exit 3") == a ]] }
if p { echo yes } else { echo no }
echo done'
  [[ $rc = 3 && -z $out && -z $err \
 && $rerr == *'/bin/sh: command failed (exit 3)'* ]]; chk 55.101

  # 55.102 ... in the `[[ … ]]` right operand of a caught `||`
  SASH_CFLAGS=--no-taint run_sash 'if /bin/false || [[ $(/bin/sh -c "echo a; exit 3") == a ]] { echo yes } else { echo no }
echo done'
  [[ $rc = 3 && -z $out && -z $err \
 && $rerr == *'/bin/sh: command failed (exit 3)'* ]]; chk 55.102

  # 55.103 a redirect failure before an `if [[ … ]]` is not the condition's
  run_sash 'x=a
/bin/echo hi > /nonexistent_dir_55/f
if [[ $x == a ]] { echo yes }
echo done'
  [[ $rc = 1 && -z $out && -z $err \
 && $rerr == *'/nonexistent_dir_55/f: No such file or directory'* ]]; chk 55.103

  # 55.104 ... as the non-final member of an `if` list. DD#55: `if a; b` is
  # `a; if b`, so `a` is a bare command and keeps its abort. bc.a7 pins that
  # for `if false; true`; with a `[[ … ]]` as the final member the scan
  # skipped the test, settled on `a` and unchecked it
  run_sash 'x=a
if /bin/sh -c "exit 3"; [[ $x == a ]] { echo yes }
echo done'
  [[ $rc = 3 && -z $out && -z $err \
 && $rerr == *'/bin/sh: command failed (exit 3)'* ]]; chk 55.104

  # 55.105-55.106 aborted before the fix too: the old scan unchecked a
  # pipeline and stopped at a redirect. No shape above reaches those two
  # arms of the shared classifier (ssa::sets_condition_status), so these
  # pin them.
  # 55.105 a command before an `if` whose condition is a pipeline
  run_sash '/bin/sh -c "exit 3"
if /bin/true | /bin/true { echo yes }
echo done'
  [[ $rc = 3 && -z $out && -z $err \
 && $rerr == *'/bin/sh: command failed (exit 3)'* ]]; chk 55.105

  # 55.106 a command before a redirected function call. The inlined call
  # ends the caller's block in a branch on the redirect, so the scan runs
  # there too, and the redirect is that branch's condition
  run_sash 'fn f { echo hi }
/bin/sh -c "exit 3"
f > /dev/null
echo done'
  [[ $rc = 3 && -z $out && -z $err \
 && $rerr == *'/bin/sh: command failed (exit 3)'* ]]; chk 55.106

# --- 55.107-55.144: what `-` can suppress, and what can run under `&` ------
# `-` suppresses a command's failure, so on anything that is not a command it is
# SB4014 (55.107-55.115). A background job runs in a forked child, so a
# control word, an assignment or a builtin that acts on this process would act
# on the child and be lost: SB4024 (DD#55 2026-10-26). A job has no status
# until it is reaped, so it cannot be a condition or a return producer either.
# A statement-level `-` takes its statement up to its `&`, and what follows the
# `&` is a new statement, as without the `-` (55.137-55.144). Every negative
# here compiled except 55.107-55.115, 55.135 and 55.136, which got the wrong
# diagnostic, and 55.139 and 55.140, which reached an ICE. Journal 537.
  # 55.107 `- attest` (was SB4014 "expected expression", at its predicate)
  run_sash 'str s=abc
- attest s [[ -n $s ]]
echo "[$s]"'
  [[ $rc != 0 && -z $out \
     && $err == *':2:1: error [SB4014] `-` cannot suppress an `attest`'* ]]; chk 55.107

  # 55.108 `- str` (was SB5011 "command '-' not found"); the advice points
  #        at `$(- cmd)`
  run_sash '- str y=$(/bin/false)
echo "[$y]"'
  [[ $rc != 0 && -z $out \
     && $err == *':1:1: error [SB4014] `-` suppresses a command, and a declaration is not one'* ]]; chk 55.108

  # 55.109 `- int` (was SB5011)
  run_sash '- int n=1
echo "$n"'
  [[ $rc != 0 && -z $out \
     && $err == *':1:1: error [SB4014] `-` suppresses a command, and a declaration is not one'* ]]; chk 55.109

  # 55.110 `- const` (was SB5011)
  run_sash '- const c=1
echo "$c"'
  [[ $rc != 0 && -z $out \
     && $err == *':1:1: error [SB4014] `-` suppresses a command, and a declaration is not one'* ]]; chk 55.110

  # 55.111 `- array` (was SB4014 "expected expression", at the value)
  run_sash '- array a=(x)
echo "${a[0]}"'
  [[ $rc != 0 && -z $out \
     && $err == *':1:1: error [SB4014] `-` suppresses a command, and a declaration is not one'* ]]; chk 55.111

  # 55.112 `- map` (likewise)
  run_sash '- map m=([k]=v)
echo "${m[k]}"'
  [[ $rc != 0 && -z $out \
     && $err == *':1:1: error [SB4014] `-` suppresses a command, and a declaration is not one'* ]]; chk 55.112

  # 55.113 `- fn` (was SB5011)
  run_sash '- fn f { echo hi }
f'
  [[ $rc != 0 && -z $out \
     && $err == *':1:1: error [SB4014] `-` suppresses a command, and a function definition is not one'* ]]; chk 55.113

  # 55.114 `- function` (was SB5011)
  run_sash '- function f { echo hi }
f'
  [[ $rc != 0 && -z $out \
     && $err == *':1:1: error [SB4014] `-` suppresses a command, and a function definition is not one'* ]]; chk 55.114

  # 55.115 `- END { }` (was SB5011)
  run_sash 'echo hi
- END { echo bye }'
  [[ $rc != 0 && -z $out \
     && $err == *':2:1: error [SB4014] `-` suppresses a command, and an `END { }` block is not one'* ]]; chk 55.115

  # 55.116 `return N &`: the child returned into the caller's code, so
  #        `done` printed twice (batch-531 B11)
  run_sash 'fn g { return 3 & }
g
- wait
echo done'
  [[ $rc != 0 && -z $out \
     && $err == *':1:8: error [SB4024] `return` isn'\''t allowed in a background job'* ]]; chk 55.116

  # 55.117 `break &`: each child broke out of the loop and ran the rest of
  #        the program, so `after` printed three times (B11)
  run_sash 'for x in 1 2 { echo "$x"; break & }
echo after'
  [[ $rc != 0 && -z $out \
     && $err == *':1:27: error [SB4024] `break` isn'\''t allowed in a background job'* ]]; chk 55.117

  # 55.118 `continue &`: each child carried on with the loop, so `2` printed
  #        twice and `after` four times (B11)
  run_sash 'for x in 1 2 { echo "$x"; continue & }
echo after'
  [[ $rc != 0 && -z $out \
     && $err == *':1:27: error [SB4024] `continue` isn'\''t allowed in a background job'* ]]; chk 55.118

  # 55.119 a background job as a condition: it took the branch and then
  #        reported "/bin/false: command failed" (batch-531 B26)
  run_sash 'if /bin/false & { echo y } else { echo n }'
  [[ $rc != 0 && -z $out \
     && $err == *':1:4: error [SB4024] condition: a background job (`… &`) is not a status producer'* ]]; chk 55.119

  # 55.120 ... as a return producer; it printed `y` (batch-531 n4)
  run_sash 'fn f { return /bin/true & }
if f { echo y } else { echo n }'
  [[ $rc != 0 && -z $out \
     && $err == *':1:8: error [SB4024] return: a background job (`… &`) is not a status producer'* ]]; chk 55.120

  # 55.121 an assignment under `&` was lost: it printed `[a]`
  run_sash 'x=a; x=b &
- wait
echo "[$x]"'
  [[ $rc != 0 && -z $out \
     && $err == *':1:6: error [SB4024] a variable assignment isn'\''t allowed in a background job'* ]]; chk 55.121

  # 55.122 ... and an arithmetic one: it printed `0`
  run_sash 'int n=0; (( n = 1 )) &
- wait; echo $n'
  [[ $rc != 0 && -z $out \
     && $err == *':1:10: error [SB4024] a variable assignment isn'\''t allowed in a background job'* ]]; chk 55.122

  # 55.123 ... inside a backgrounded condition: it printed `x0`
  run_sash 'int a=0; if ! (( a++, a > 3 )) & { echo "x$a" }'
  [[ $rc != 0 && -z $out \
     && $err == *':1:15: error [SB4024] a variable assignment isn'\''t allowed in a background job'* ]]; chk 55.123

  # 55.124 `cd &` changed only the child's directory
  run_sash 'cd /tmp &
- wait
echo after'
  [[ $rc != 0 && -z $out \
     && $err == *':1:1: error [SB4024] `cd` isn'\''t allowed in a background job'* ]]; chk 55.124

  # 55.125 `exec >f &` rewired only the child's descriptors
  run_sash 'exec >/dev/null &
- wait
echo after'
  [[ $rc != 0 && -z $out \
     && $err == *':1:1: error [SB4024] `exec` without a command isn'\''t allowed in a background job'* ]]; chk 55.125

  # 55.126 `attest … &` gated only the child's copy of x, yet the parent's
  #        read compiled as attested: C printed the unattested value, native
  #        an empty one
  run_sash 'x=$(/bin/echo hi)
attest x [[ -n $x ]] &
- wait
echo "x=$x"'
  [[ $rc != 0 && -z $out \
     && $err == *':2:1: error [SB4024] `attest` isn'\''t allowed in a background job'* ]]; chk 55.126

  # 55.127 `argv ar &`: the native binary segfaulted, the C one printed a
  #        garbage length
  run_sash 'argv ar &
- wait
echo "n=$(len ar)"'
  [[ $rc != 0 && -z $out \
     && $err == *':1:1: error [SB4024] `argv` isn'\''t allowed in a background job'* ]]; chk 55.127

  # 55.128 (positive) `exec cmd &` stays legal: the child becomes the command
  #        (unlike `cmd &`, whose child runs it as a second process; DD#55)
  run_sash 'exec /bin/true &
- wait
echo after'
  [[ $rc = 0 && $out = "after" && -z $rerr ]]; chk 55.128

  # 55.129 a `-` operand ended the parse of its list, so the `|| c` after it
  #        escaped SB4053 and the program printed `x` (batch-531 B24, DD#95)
  run_sash '/bin/true && - /bin/false || /bin/echo x
echo after'
  [[ $rc != 0 && -z $out \
     && $err == *':1:1: error [SB4053] `&&` and `||` in one list are ambiguous'* ]]; chk 55.129

  # 55.130 (positive) `a && - b &` stays legal
  run_sash '/bin/true && - /bin/false &
- wait
echo after'
  [[ $rc = 0 && $out = "after" && -z $rerr ]]; chk 55.130

  # 55.131 `a && - b &` backgrounds the whole list, as `a && b &` does. The
  #        `-` took `b &` alone, so the `cd` ran in this process and `pwd`
  #        printed /tmp; now the `cd` is in the job (55.124)
  run_sash 'cd /tmp && - /bin/true &
- wait
/bin/pwd'
  [[ $rc != 0 && -z $out \
     && $err == *':1:1: error [SB4024] `cd` isn'\''t allowed in a background job'* ]]; chk 55.131

  # 55.132 the check looks through `-`: the job changed only its own
  #        directory, and `pwd` printed the old one
  run_sash '/bin/true && - cd /tmp &
- wait
/bin/pwd'
  [[ $rc != 0 && -z $out \
     && $err == *':1:16: error [SB4024] `cd` isn'\''t allowed in a background job'* ]]; chk 55.132

  # 55.133 ... an assignment behind `-`: it printed `[a]`
  run_sash 'x=a
/bin/true && - x=$(/bin/echo b) &
- wait
echo "[${x:!}]"'
  [[ $rc != 0 && -z $out \
     && $err == *':2:16: error [SB4024] a variable assignment isn'\''t allowed in a background job'* ]]; chk 55.133

  # 55.134 ... an arithmetic one: it printed `0`
  run_sash 'int n=0
/bin/false || - (( n = 1 )) &
- wait
echo "$n"'
  [[ $rc != 0 && -z $out \
     && $err == *':2:17: error [SB4024] a variable assignment isn'\''t allowed in a background job'* ]]; chk 55.134

  # 55.135 ... a suppressed final reader (DD#122). It was SB4022, which
  #        called the stage a compound command. Were the check not to look
  #        through `-`, the job would compile and lose `y`
  run_sash '/bin/echo hi | - readline y &
- wait
echo "[${y:!}]"'
  [[ $rc != 0 && -z $out \
     && $err == *':1:18: error [SB4024] `readline` isn'\''t allowed in a background job'* ]]; chk 55.135

  # 55.136 ... `read` (likewise; that job would segfault natively and print
  #        a garbage length in C)
  run_sash '/bin/printf "a\0b\0" | - read ar &
- wait
echo "n=$(len ar)"'
  [[ $rc != 0 && -z $out \
     && $err == *':1:26: error [SB4024] `read` isn'\''t allowed in a background job'* ]]; chk 55.136

  # 55.137 a statement-level `-` takes its statement up to its `&`, and what
  #        follows the `&` is a new statement, as without the `-` (55.143).
  #        The parser folded the statement a second time, which joined
  #        `&& b` to the job: this ran `b`
  run_sash '- /bin/echo a & && /bin/echo b
- wait
echo done'
  [[ $rc != 0 && -z $out \
     && $err == *':1:17: error [SB4014] expected expression'* ]]; chk 55.137

  # 55.138 ... `|| b`: `b` never ran
  run_sash '- /bin/echo a & || /bin/echo b
- wait
echo done'
  [[ $rc != 0 && -z $out \
     && $err == *':1:17: error [SB4014] expected expression'* ]]; chk 55.138

  # 55.139 ... `| b`: the job became a pipeline stage (ICE)
  run_sash '- /bin/echo a & | /bin/cat
- wait
echo done'
  [[ $rc != 0 && -z $out \
     && $err == *':1:17: error [SB4014] expected expression'* ]]; chk 55.139

  # 55.140 ... a redirect after the `&` (ICE)
  run_sash '- /bin/echo a & >/dev/null
- wait
echo done'
  [[ $rc != 0 && -z $out \
     && $err == *':1:17: error [SB4014] expected expression'* ]]; chk 55.140

  # 55.141 ... a second `&`: a job that backgrounded the first job
  run_sash '- /bin/echo a & &
- wait
echo done'
  [[ $rc != 0 && -z $out \
     && $err == *':1:17: error [SB4014] expected expression'* ]]; chk 55.141

  # 55.142 ... in a block body as well: `b` never ran
  run_sash 'if /bin/true { - /bin/echo a & || /bin/echo b }
- wait
echo done'
  [[ $rc != 0 && -z $out \
     && $err == *':1:32: error [SB4014] expected expression'* ]]; chk 55.142

  # 55.143 (control) the same statement without the `-`, unchanged
  run_sash '/bin/echo a & && /bin/echo b
- wait
echo done'
  [[ $rc != 0 && -z $out \
     && $err == *':1:15: error [SB4014] expected expression'* ]]; chk 55.143

  # 55.144 (positive) a statement after the `&` on the same line runs
  run_sash '- /bin/true & echo b
- wait
echo done'
  [[ $rc = 0 && $out = $'b\ndone' && -z $rerr ]]; chk 55.144

# --- 95.13-95.20: SB4056, a list/pipeline operator with no right operand ----
# `parse_trailing_list_op` did `parse_list_operand(...).unwrap_or(true_cmd)`,
# so a missing right-hand side became an implicit `true`: `echo a &&` compiled
# and printed `a`. The pipeline loop had the same hole by a different route --
# with no stage after the `|` the `while` simply fell out and the pipeline was
# returned as its left side alone. bash rejects all three outright. Found by
# branch coverage: `parse_statement`'s at_eof guard had 0 hits against 19915,
# and the only caller that could reach it was the trailing-pipe path.
# Journal 311.

# 95.13 trailing `&&`
run_sash 'echo a &&'
[[ $rc != 0 && -z $out && $err == *SB4056* ]]; chk 95.13

# 95.14 trailing `||`
run_sash 'echo a ||'
[[ $rc != 0 && -z $out && $err == *SB4056* ]]; chk 95.14

# 95.15 trailing `|`
run_sash 'echo a |'
[[ $rc != 0 && -z $out && $err == *SB4056* ]]; chk 95.15

# 95.16 a trailing newline does not supply the operand — `skip_terminators`
#       runs before the operand is parsed, so this is the same error
run_sash 'echo a &&
'
[[ $rc != 0 && -z $out && $err == *SB4056* ]]; chk 95.16

# 95.17 control: a trailing `&` is NOT this error. Background genuinely has no
#       right operand, and `background_list_op` still synthesises the `true`.
#       bash accepts it too.
run_sash 'echo a &'
[[ $rc = 0 && $out = "a" && -z $err ]]; chk 95.17

# 95.18 control: a complete `&&` chain still folds left (DD#95)
run_sash 'true && echo x && echo y'
[[ $rc = 0 && $out = "x
y" && -z $err ]]; chk 95.18

# 95.19 control: a complete `||` still runs its right side
run_sash 'false || echo c'
[[ $rc = 0 && $out = "c" && -z $err ]]; chk 95.19

# 95.20 control: a multi-stage pipeline is unaffected
run_sash 'echo a | cat | cat'
[[ $rc = 0 && $out = "a" && -z $err ]]; chk 95.20

# --- 57u.1-57u.12: SB4057, a construct whose closer never arrives ----------
# Every scan-to-closer helper in the lexer ended with the same idiom:
#
#     if self.pos < self.source.len() { self.pos += 1; }   // skip closing X
#
# The guard *is* the run-off-the-end detection, and it silently did nothing.
# So `/bin/echo "hello` and `echo ${x` compiled clean and ran. Found from the
# branch-coverage asymmetry query via `scan_by_brace_depth`'s length guard
# (journal 313). bash rejects all of these with "unexpected EOF".
run_sash '/bin/echo "hello'
[[ $rc != 0 && -z $out && $err == *SB4057* ]]; chk 57u.1
run_sash "/bin/echo 'hello"
[[ $rc != 0 && -z $out && $err == *SB4057* ]]; chk 57u.2
run_sash 'str ux = "a"
/bin/echo ${ux'
[[ $rc != 0 && -z $out && $err == *SB4057* ]]; chk 57u.3
run_sash 'str ux = "a"
/bin/echo "${ux"'
[[ $rc != 0 && -z $out && $err == *SB4057* ]]; chk 57u.4
run_sash 'str ux = "a"
/bin/echo ${ux:-z'
[[ $rc != 0 && -z $out && $err == *SB4057* ]]; chk 57u.5
run_sash "/bin/echo \$'abc"
[[ $rc != 0 && -z $out && $err == *SB4057* ]]; chk 57u.6
# the diagnostic points at the opener, not at end-of-source
run_sash '/bin/echo "hello'
[[ $err == *:1:11:* ]]; chk 57u.7

# controls: the terminated spelling of each still compiles and runs
run_sash '/bin/echo "hello"'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 57u.8
run_sash "/bin/echo 'hello'"
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 57u.9
run_sash 'str ux = "a"
/bin/echo ${ux}'
[[ $rc = 0 && $out = "a" && -z $err ]]; chk 57u.10
run_sash 'str ux = ""
/bin/echo ${ux:-z}'
[[ $rc = 0 && $out = "z" && -z $err ]]; chk 57u.11
# an empty string is terminated, not unterminated
run_sash '/bin/echo "" | cat'
[[ $rc = 0 && -z $err ]]; chk 57u.12

# --- 57u.13-57u.18: the same bug one layer up, in the parser ---------------
# 57u.1-57u.12 fixed every scan-to-closer helper in the *lexer*.
# `parse_command_substitution` had the identical hole and was missed:
# its loop did `if scanner.at_eof() { break; }` and then returned a
# well-formed `CommandSubstitution`, so an unclosed `$(` was silently
# discarded. `echo $(echo hi` compiled clean and printed `hi`; worse,
# `echo $(status` emitted a *bare* command name and the binary died at
# runtime with "sash: bug: bare command passed to sash_exec". The quoted
# spelling `"$(status` had always been SB4057 — only the unquoted path
# was blind. Found from the branch-coverage asymmetry in
# `parse_status_word`'s argument loop (journal 338).
run_sash 'echo $(echo hi'
[[ $rc != 0 && -z $out && $err == *SB4057* ]]; chk 57u.13
run_sash 'echo $(echo a
echo b'
[[ $rc != 0 && -z $out && $err == *SB4057* ]]; chk 57u.14
# the two that reached the runtime assertion rather than merely misparsing
run_sash 'echo $(status'
[[ $rc != 0 && -z $out && $err == *SB4057* && $err != *"sash: bug"* ]]; chk 57u.15
run_sash 'echo $(getcpid'
[[ $rc != 0 && -z $out && $err == *SB4057* ]]; chk 57u.16
# the diagnostic points at the `$(` opener, not at end-of-source
run_sash 'echo $(echo hi'
[[ $err == *:1:6:* ]]; chk 57u.17
# control: the terminated spelling still compiles and runs
run_sash 'echo $(echo hi)'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 57u.18

# ── SB4046: a redirect on something that produces no output ──────────
# DG 2026-09-14: no meaningless redirects. bash creates and truncates the
# file and writes nothing into it; sash used to parse the redirect and then
# discard it in the transform, diverging silently (the file was never made).
# rdr.1 a bare arithmetic test
run_sash '(( 1 + 1 )) > /tmp/sash_rdr_should_not_exist'
[[ $rc != 0 && $err == *SB4046* && $err == *"arithmetic test"* ]]; chk rdr.1
# rdr.2 suppressed with `-` — the suppression wraps the redirect, so the
# redirect still lands on the arithmetic test
run_sash '- (( 1 + 1 )) > /tmp/sash_rdr_should_not_exist'
[[ $rc != 0 && $err == *SB4046* ]]; chk rdr.2
# rdr.3 as a return producer inside a function
run_sash 'fn g { return (( 1 + 1 )) > /tmp/sash_rdr_should_not_exist
}
- g'
[[ $rc != 0 && $err == *SB4046* ]]; chk rdr.3
# rdr.4 as the right operand of `&&` — the redirect binds to the operand
run_sash 'str s = "y"
[[ $s == x ]] && (( 1 + 1 )) > /tmp/sash_rdr_should_not_exist'
[[ $rc != 0 && $err == *SB4046* ]]; chk rdr.4
# rdr.5 a `[[ … ]]` test as a return producer
run_sash 'fn g { return [[ -f /etc/passwd ]] > /tmp/sash_rdr_should_not_exist
}
- g'
[[ $rc != 0 && $err == *SB4046* && $err == *"test"* ]]; chk rdr.5
# rdr.6 a bare `[[ … ]]` with a redirect reports the redirect (SB4046), not
# the unused result (SB4018) — the redirect is caught at parse time
run_sash '[[ -f /etc/passwd ]] > /tmp/sash_rdr_should_not_exist'
[[ $rc != 0 && $err == *SB4046* ]]; chk rdr.6
# rdr.7 without the redirect the same test is still SB4018 — SB4046 did not
# swallow the unused-result rule
run_sash '[[ -f /etc/passwd ]]'
[[ $rc != 0 && $err == *SB4018* ]]; chk rdr.7
# rdr.8 the file is never created, because the program never compiles
[[ ! -e /tmp/sash_rdr_should_not_exist ]]; chk rdr.8
# rdr.9 control: a redirect on a real command still compiles and writes
run_sash '/bin/echo hi > /tmp/sash_rdr_ok'
[[ $rc = 0 && -z $err ]]; chk rdr.9
# rdr.10 control: a redirect inside an if body is untouched
run_sash 'if [[ -f /etc/passwd ]] { /bin/echo y > /tmp/sash_rdr_ok }'
[[ $rc = 0 && -z $err ]]; chk rdr.10
rm -f /tmp/sash_rdr_ok

# ── SB4046 part 2: the no-output *builtins* ──────────────────────────
# DG 2026-10-11, extending the rule above from statement kinds to command
# names. Every one of these used to compile clean and SILENTLY DISCARD the
# redirect — the file was never opened and the filename never reached the
# binary. The drop had two independent causes, a `_ => {}` in
# `attach_redirects_to_last_inst` and codegen arms that took no `redirects`
# parameter at all, which is why the gate is a closed allow-list at parse
# (`BuiltinCmd::may_take_redirect`) rather than a patch at either site.
#
# The c backend already honoured several of these, so native and c disagreed
# on shapes no spec exercised — the differential oracle could not report a
# divergence nobody asked it about.
#
# rdr.11 `attest` is the one that did not merely drop the redirect: it
# PANICKED the compiler. `parse_attest` builds its command by hand and never
# routes through `parse_simple_command`, so the redirect survived to
# `redirect_reject_noun` — an ICE whose own doc said no source text could
# reach it. All three operators crashed.
run_sash 'str s = "x"
attest s [[ -n $s ]] > /tmp/sash_rdr_should_not_exist
echo "$s"'
[[ $rc != 0 && -z $out && $err == *SB4046* && $err == *attest* \
   && $err != *ICE* && $err != *panic* ]]; chk rdr.11
run_sash 'str s = "x"
attest s [[ -n $s ]] 2> /tmp/sash_rdr_should_not_exist
echo "$s"'
[[ $rc != 0 && $err == *SB4046* && $err != *panic* ]]; chk rdr.12
run_sash 'str s = "x"
attest s [[ -n $s ]] < /tmp/sash_rdr_should_not_exist
echo "$s"'
[[ $rc != 0 && $err == *SB4046* && $err != *panic* ]]; chk rdr.13
# rdr.14 the DD#93 Part B shorthand reaches the same gate
run_sash 'str s = "x"
attest s -n > /tmp/sash_rdr_should_not_exist
echo "$s"'
[[ $rc != 0 && $err == *SB4046* && $err != *panic* ]]; chk rdr.14
# rdr.15 `cd` — bash creates the file; sash rejects rather than copy the wart
run_sash 'cd /tmp > /tmp/sash_rdr_should_not_exist
echo done'
[[ $rc != 0 && -z $out && $err == *SB4046* && $err == *cd* ]]; chk rdr.15
# rdr.16 `exit`
run_sash 'exit 0 > /tmp/sash_rdr_should_not_exist'
[[ $rc != 0 && $err == *SB4046* && $err == *exit* ]]; chk rdr.16
# rdr.17 `close`
run_sash 'close 2 > /tmp/sash_rdr_should_not_exist'
[[ $rc != 0 && $err == *SB4046* && $err == *close* ]]; chk rdr.17
# rdr.18 `unset`
run_sash 'map m
m[q]=1
echo "$(len m)"
unset m[q] > /tmp/sash_rdr_should_not_exist'
[[ $rc != 0 && $err == *SB4046* && $err == *unset* ]]; chk rdr.18
# rdr.19 `return` inside a function
run_sash 'fn g { return 0 > /tmp/sash_rdr_should_not_exist
}
- g'
[[ $rc != 0 && $err == *SB4046* && $err == *return* ]]; chk rdr.19
# rdr.20 `match` where its result IS used, so SB4018 cannot be what fires
run_sash 'str s = "ab"
if match "$s" /a/ > /tmp/sash_rdr_should_not_exist { echo y }'
[[ $rc != 0 && -z $out && $err == *SB4046* && $err == *match* ]]; chk rdr.20
# rdr.21 a value-only intrinsic
run_sash 'str s = "ab"
strlen "$s" > /tmp/sash_rdr_should_not_exist'
[[ $rc != 0 && $err == *SB4046* ]]; chk rdr.21
# rdr.22 none of the above ever created its file
[[ ! -e /tmp/sash_rdr_should_not_exist ]]; chk rdr.22

# ── the allow-list: these must keep working ──────────────────────────
# rdr.23 `:` is DG's deliberate exception. It produces no output like the
# rest, but `: >f` is the universal truncate idiom, so it HONOURS its
# redirect instead of being rejected. It used to drop it silently.
rm -f /tmp/sash_rdr_trunc
run_sash 'echo before
: > /tmp/sash_rdr_trunc
echo after'
[[ $rc = 0 && $out = $'before\nafter' && -z $err && -e /tmp/sash_rdr_trunc ]]; chk rdr.23
# rdr.24 and it truncates an existing file to empty, which is the whole point
printf 'CONTENT\n' > /tmp/sash_rdr_trunc
run_sash ': > /tmp/sash_rdr_trunc'
[[ $rc = 0 && -z $err && -e /tmp/sash_rdr_trunc && ! -s /tmp/sash_rdr_trunc ]]; chk rdr.24
rm -f /tmp/sash_rdr_trunc
# rdr.25 `exec CMD >f` was the worst of the set: it produced WRONG OUTPUT
# rather than a missing side effect, printing to the terminal while the file
# was never made. `exec` replaces the process with one that does produce
# output, so it honours the redirect — bash's semantics.
rm -f /tmp/sash_rdr_exec
run_sash 'exec /bin/echo hi > /tmp/sash_rdr_exec'
[[ $rc = 0 && -z $out && -z $err && $(cat /tmp/sash_rdr_exec) = "hi" ]]; chk rdr.25
rm -f /tmp/sash_rdr_exec
# rdr.26 the no-argument `exec` fd-manipulation form is untouched
rm -f /tmp/sash_rdr_exec
run_sash 'exec > /tmp/sash_rdr_exec
echo hi'
[[ $rc = 0 && -z $out && $(cat /tmp/sash_rdr_exec) = "hi" ]]; chk rdr.26
rm -f /tmp/sash_rdr_exec
# rdr.27 `echo`/`write` and an external command are unaffected
rm -f /tmp/sash_rdr_ok
run_sash 'echo hi > /tmp/sash_rdr_ok'
[[ $rc = 0 && -z $err && $(cat /tmp/sash_rdr_ok) = "hi" ]]; chk rdr.27
run_sash 'write hi > /tmp/sash_rdr_ok'
[[ $rc = 0 && -z $err && $(cat /tmp/sash_rdr_ok) = "hi" ]]; chk rdr.28
# rdr.29 `readline` consumes input, so it takes a redirect too
printf 'hello\n' > /tmp/sash_rdr_in
run_sash 'str s = ""
readline s < /tmp/sash_rdr_in
attest s [[ -n $s ]]
echo "got:$s"'
[[ $rc = 0 && $out = "got:hello" && -z $err ]]; chk rdr.29
rm -f /tmp/sash_rdr_in /tmp/sash_rdr_ok
# rdr.30 a user function is not a builtin, so it keeps its redirect
rm -f /tmp/sash_rdr_ok
run_sash 'fn g { echo fromfn
}
g > /tmp/sash_rdr_ok'
[[ $rc = 0 && -z $out && $(cat /tmp/sash_rdr_ok) = "fromfn" ]]; chk rdr.30
rm -f /tmp/sash_rdr_ok
# rdr.31 an expansion head is not judged here — its value is unknown at parse
# and it is an external command anyway, so it is left to SB5011
run_sash 'str c = "/bin/echo"
$c hi > /tmp/sash_rdr_ok'
[[ $err != *SB4046* ]]; chk rdr.31
rm -f /tmp/sash_rdr_ok
# rdr.32 EVERY value-producing intrinsic rejects a redirect, not just the
# `BuiltinCmd` ones. `BuiltinCmd::from_name` maps all twelve `resolve`
# intrinsics to `Other`, and `may_take_redirect` read `Other` as
# "external or user function — allow", so `$(getcwd 2>/dev/null)` escaped
# SB4046 and drew a false SB5073 while `$(len a 2>/dev/null)` was caught.
# The gate now asks `resolve::head_may_take_redirect`, which joins both
# tables. Looped so the whole set is pinned, not a sample of it.
_bad=""
for _n in getenv arg argc epochseconds random getpid getppid waitstatus getcwd signum; do
  run_sash "str s = \$($_n x 2>/dev/null)
echo \$s"
  [[ $err == *SB4046* ]] || _bad="$_bad $_n"
done
[[ -z $_bad ]]; chk rdr.32
# rdr.33 `status` and `getcpid` reached the same fault down a DIFFERENT road:
# `parse_status_word`/`parse_getcpid_word` claimed the word before
# `parse_command` ran, so the parser's redirect gate never saw them at all.
# `parse_getcpid_word` discarded the rest of the command without consuming
# it, so `2>/dev/null` became a second statement and SB5073's "not one
# statement among several" was SELF-FULFILLING; `parse_status_word` fed `2>`
# to `parse_expr` and got SB4014 "expected expression". Both now fall
# through to the ordinary parse unless the next token closes the capture.
run_sash '/bin/true
int x = $(status 2>/dev/null)
echo $x'
[[ $err == *SB4046* ]]; chk rdr.33
run_sash '/bin/sleep 0 &
int x = $(getcpid 2>/dev/null)
echo $x'
[[ $err == *SB4046* ]]; chk rdr.34
# rdr.35 the bare forms still parse — the shortcut claims exactly `$(status)`
# and `$(getcpid)` by looking ahead for the closing paren. Read in an `else`,
# where DD#21 makes a non-zero status available to be handled.
run_sash 'if /bin/sh -c "exit 7" {
echo no
} else {
int x = $(status)
echo "st=$x"
}'
[[ $rc = 0 && $out = "st=7" && -z $err ]]; chk rdr.35
# rdr.36 `$(getcpid x)` was the one name missing from `value_intrinsic_arity`,
# and nothing could see the gap while the parser was throwing the argument
# away. Routing it through the ordinary parse exposed it as an ICE in the
# int-context lowering. It is an arity error, like every sibling.
run_sash '/bin/sleep 0 &
int x = $(getcpid x)
echo $x'
[[ $err == *SB4045* && $err != *panic* ]]; chk rdr.36
# rdr.37 a SUPPRESSED capture honours its redirect. The parser nests this as
# `Suppressed(Redirected(Command))`, and `single_command_subst`'s guard
# tested the OUTERMOST kind for `Redirected` — which is `Suppressed` — so it
# missed, took the fast path, and built a `CmdSubst` carrying no redirects
# at all. The redirect was dropped in silence, in both backends. The guard
# was non-transitive while the peel beneath it was transitive.
rm -f /tmp/sash_rdr_sup
run_sash 'str s = $(- /bin/sh -c "echo E >&2; echo OUT" 2>/tmp/sash_rdr_sup)
attest s [[ -n $s ]]
echo "got:$s"'
[[ $rc = 0 && $out = "got:OUT" && -z $err && $(cat /tmp/sash_rdr_sup) = "E" ]]; chk rdr.37
rm -f /tmp/sash_rdr_sup
# rdr.38 and the same spelling aimed at /dev/null really does suppress — the
# tell for the dropped redirect was `E` arriving on the terminal
run_sash 'str s = $(- /bin/sh -c "echo E >&2; echo OUT" 2>/dev/null)
attest s [[ -n $s ]]
echo "got:$s"'
[[ $rc = 0 && $out = "got:OUT" && $rerr != *E* ]]; chk rdr.38
# rdr.39 `return CMD` derives its status from the command, so it is a command
# statement and takes a redirect (DD#74). `return N` names the status
# outright and has no output to redirect — DG: "return N >f is nonsense".
run_sash 'fn f {
return /bin/sh -c "echo E >&2; exit 3" 2>/dev/null
}
f || echo "rc=$?"'
[[ $rc = 0 && $out = "rc=3" && $rerr != *E* ]]; chk rdr.39
run_sash 'fn f {
return 3 > /tmp/sash_rdr_rn
}
f'
[[ $err == *SB4046* && ! -e /tmp/sash_rdr_rn ]]; chk rdr.40
# rdr.41 an APPENDING redirect on a pipeline stage accumulates across
# statements rather than truncating each time
rm -f /tmp/sash_rdr_ap
run_sash '/bin/echo one >> /tmp/sash_rdr_ap | /bin/cat
/bin/echo two >> /tmp/sash_rdr_ap | /bin/cat
/bin/cat /tmp/sash_rdr_ap'
[[ $rc = 0 && $out = $'one\ntwo' && -z $err ]]; chk rdr.41
rm -f /tmp/sash_rdr_ap
# rdr.42 a redirect does not erase the exit status. SB5012's
# `leaf_status_after` asked `is_command_like`, which lists `Command` /
# `Pipeline` / `List` / `ArithTest` but not `Redirected`, so a redirected
# command left the status `Undefined` and the next `$?` was told there was
# "no preceding command" — naming the one thing that had just happened. Its
# two sibling predicates (`stmt_backgrounds_job`, `stmt_reaps_child`) both
# peel the wrapper already, and the first of those carries a doc recording
# this exact fault being fixed for `$!`.
rm -f /tmp/sash_rdr_st
run_sash '/bin/echo hi > /tmp/sash_rdr_st
int x = $(status)
echo "st=$x"'
[[ $rc = 0 && $out = "st=0" && -z $err ]]; chk rdr.42
run_sash '/bin/echo hi > /tmp/sash_rdr_st
echo $?'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk rdr.43
# rdr.44 the status is still readable when the `$?` is itself inside the
# redirected statement
run_sash '/bin/true
/bin/echo $? > /tmp/sash_rdr_st
/bin/cat /tmp/sash_rdr_st'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk rdr.44
# rdr.45 suppression is NOT peeled, and must not be. DD#55: "`$?` is not
# available after `- cmd` — reading `$?` after `-` is a compile error. You
# said you don't care; you don't get to inspect it." 55.15 pins the
# `- cmd; return $?` spelling the DD names; these two are the top-level bare
# read and the redirected combination, and they are the control that keeps
# the fix above from becoming "peel every wrapper".
run_sash '- /bin/sh -c "exit 7"
echo $?'
[[ $err == *SB5012* ]]; chk rdr.45
run_sash '- /bin/echo hi > /tmp/sash_rdr_st
echo $?'
[[ $err == *SB5012* ]]; chk rdr.46
rm -f /tmp/sash_rdr_st
# rdr.47 a trailing redirect on a pipeline, which DD#55 names as legal
# alongside `cmd && a | b` and `a | b &`. It had no end-to-end spec at all —
# the only matches in the suite were sash-convert *rendering* assertions.
#
# Note what the parser does with it, because it is not what the surface
# suggests: `a | b > f` becomes `Pipeline[Command, Redirected(Command)]`, not
# `Redirected(Pipeline)`. The redirect binds to the FINAL STAGE (verified with
# --emit-ir), which is bash's semantics. Every spelling that would wrap a
# pipeline as a unit is rejected at parse — `{ a | b } > f` is SB4021, a
# backgrounded one SB4024 (NOT SB4022, as this comment said until 2026-10-14;
# SB4022 is the *pipe* member of the family) — so `Redirected(Pipeline)` is
# never constructed.
# These specs therefore do NOT reach `attach_redirects_to_last_inst`'s
# `SsaInst::Pipeline` arm; that arm is unreachable by construction. Measured,
# not assumed: the arm was replaced with a panic and this file still passed.
rm -f /tmp/sash_rdr_pl
run_sash '/bin/echo hi | /bin/cat > /tmp/sash_rdr_pl'
[[ $rc = 0 && -z $out && -z $err && $(cat /tmp/sash_rdr_pl) = "hi" ]]; chk rdr.47
# rdr.48 appending, and accumulating across two pipelines rather than
# truncating — the stage-level spelling is rdr.41
rm -f /tmp/sash_rdr_pl
run_sash '/bin/echo one | /bin/cat >> /tmp/sash_rdr_pl
/bin/echo two | /bin/cat >> /tmp/sash_rdr_pl
/bin/cat /tmp/sash_rdr_pl'
[[ $rc = 0 && $out = $'one\ntwo' && -z $err ]]; chk rdr.48
# rdr.49 an input redirect binds to the final stage, so it reads the file and
# not the pipe — bash's behaviour
printf 'in1\nin2\n' > /tmp/sash_rdr_in
run_sash '/bin/cat | /bin/cat < /tmp/sash_rdr_in'
[[ $rc = 0 && $out = $'in1\nin2' && -z $err ]]; chk rdr.49
rm -f /tmp/sash_rdr_pl /tmp/sash_rdr_in

# rdr.50-rdr.57 (journal 537): a redirect after a `!`-negated test, assignment,
# declaration or attest is SB4046, as it is without the `!`: none of them
# produces output. The `!` hid the operand from the check, and every one of
# these crashed the compiler (ICE).
# rdr.50 `if ! [[ … ]] >f`
run_sash 'if ! [[ -n x ]] >/dev/null { echo y } else { echo n }'
[[ $rc != 0 && -z $out \
   && $err == *':1:6: error [SB4046] a redirect cannot attach to a `[[ … ]]` test'* ]]; chk rdr.50
# rdr.51 `if ! (( … )) >f`
run_sash 'if ! (( 1 )) >/dev/null { echo y } else { echo n }'
[[ $rc != 0 && -z $out \
   && $err == *':1:6: error [SB4046] a redirect cannot attach to an arithmetic test'* ]]; chk rdr.51
# rdr.52 `if ! x=b >f`
run_sash 'x=a; if ! x=b >/dev/null { echo y } else { echo n }; echo "$x"'
[[ $rc != 0 && -z $out \
   && $err == *':1:11: error [SB4046] a redirect cannot attach to a variable assignment'* ]]; chk rdr.52
# rdr.53 `if ! str s=a >f`
run_sash 'if ! str s=a >/dev/null { echo "$s" }'
[[ $rc != 0 && -z $out \
   && $err == *':1:6: error [SB4046] a redirect cannot attach to a variable declaration'* ]]; chk rdr.53
# rdr.54 `return ! [[ … ]] >f`
run_sash 'fn f { return ! [[ -n x ]] >/dev/null }
if f { echo y } else { echo n }'
[[ $rc != 0 && -z $out \
   && $err == *':1:17: error [SB4046] a redirect cannot attach to a `[[ … ]]` test'* ]]; chk rdr.54
# rdr.55 `if ! attest … >f`
run_sash 'str s=$(/bin/echo a)
if ! attest s [[ -n $s ]] >/dev/null { echo y }'
[[ $rc != 0 && -z $out \
   && $err == *':2:6: error [SB4046] a redirect cannot attach to an `attest`'* ]]; chk rdr.55
# rdr.56 `a | ! [[ … ]] >f`, a negated final stage
run_sash 'if /bin/echo a | ! [[ -n x ]] >/dev/null { echo y }'
[[ $rc != 0 && -z $out \
   && $err == *':1:20: error [SB4046] a redirect cannot attach to a `[[ … ]]` test'* ]]; chk rdr.56
# rdr.57 `! [[ … ]] 2>f` as a statement
run_sash '! [[ -n x ]] 2>/dev/null
echo after'
[[ $rc != 0 && -z $out \
   && $err == *':1:3: error [SB4046] a redirect cannot attach to a `[[ … ]]` test'* ]]; chk rdr.57


# ── Suffixes on a compound command (SB4021 / 4022 / 4023 / 4024) ────
#
# One family, four codes, one message shape: "wrap the compound in a function".
# Only SB4022 (the pipe) had a live assertion — pf.16/pf.17. The other three
# were raised by the parser, named in comments here and in validate.rs, and
# asserted by NOTHING. The comment three blocks up even named the wrong code
# for the backgrounded case. Assert each separately: they are four distinct
# arms of `compound_suffix_fault` and a shared message would not tell them
# apart.

# csfx.1 a redirect on a compound
run_sash '{ /bin/echo a | /bin/cat; } >/tmp/sash_csfx'
[[ $rc != 0 && -z $out && $err == *SB4021* && $err == *'wrap the compound'* \
   && ! -e /tmp/sash_csfx ]]; chk csfx.1

# csfx.2 `&&` / `||` on a compound
run_sash 'for x in a b; do /bin/echo "$x"; done && /bin/echo y'
[[ $rc != 0 && -z $out && $err == *SB4023* ]]; chk csfx.2

# csfx.3 `&` on a compound — the case the rdr.47 comment misnamed as SB4022
run_sash 'while false; do :; done &'
[[ $rc != 0 && -z $out && $err == *SB4024* && $err != *SB4022* ]]; chk csfx.3
rm -f /tmp/sash_csfx

# csfx.4-csfx.11 (journal 537): the same suffixes after a `!`-negated group,
# and on a comma list in a return producer. Either way the group was hidden
# from the suffix check: `&` ran it or drew another diagnostic, and a pipe or a
# redirect crashed the compiler (ICE).
# csfx.4 `! { … } && b &` (it ran)
run_sash '! { /bin/false } && /bin/true &
- wait
echo after'
[[ $rc != 0 && -z $out \
   && $err == *':1:3: error [SB4024] `&` on a compound command'* ]]; chk csfx.4

# csfx.5 `a && ! { … } &` (was SB4018)
run_sash '/bin/true && ! { /bin/false } &
- wait
echo after'
[[ $rc != 0 && -z $out \
   && $err == *':1:16: error [SB4024] `&` on a compound command'* ]]; chk csfx.5

# csfx.6 `return ! { … } &` (it ran)
run_sash 'fn f { return ! { /bin/true } & }
if f { echo y } else { echo n }'
[[ $rc != 0 && -z $out \
   && $err == *':1:17: error [SB4024] `&` on a compound command'* ]]; chk csfx.6

# csfx.7 `$(! { … } &)` (was an unrelated SB5020)
run_sash 'x=$(! { /bin/true } &)
echo "[$x]"'
[[ $rc != 0 && -z $out \
   && $err == *':1:7: error [SB4024] `&` on a compound command'* ]]; chk csfx.7

# csfx.8 `if ! { … } & {` (it ran)
run_sash 'if ! { /bin/true } & { echo y }'
[[ $rc != 0 && -z $out \
   && $err == *':1:6: error [SB4024] `&` on a compound command'* ]]; chk csfx.8

# csfx.9 a comma list with a write is one arithmetic command (DD#31), held
#        as a group only because a part writes, so a pipe on one in a return
#        producer is the assignment's SB4022 (an ICE before journal 537)
run_sash 'int n=0
fn f { return (( n = 1, n > 0 )) | /bin/cat }
if f { echo "y$n" } else { echo "n$n" }'
[[ $rc != 0 && -z $out \
   && $err == *':2:15: error [SB4022] a variable assignment isn'\''t allowed as a pipeline stage'* ]]; chk csfx.9

# csfx.10 `! { … } 2>f` (ICE)
run_sash '! { /bin/true } 2>/dev/null
echo after'
[[ $rc != 0 && -z $out \
   && $err == *':1:3: error [SB4021] a redirect on a compound command'* ]]; chk csfx.10

# csfx.11 a redirect on a comma list with a write in a return producer is
#         the assignment's SB4046 (an ICE before journal 537)
run_sash 'int n=0
fn f { return (( n = 1, n > 0 )) >/dev/null }
if f { echo "y$n" } else { echo "n$n" }'
[[ $rc != 0 && -z $out \
   && $err == *':2:15: error [SB4046] a redirect cannot attach to a variable assignment'* ]]; chk csfx.11

# csfx.12-csfx.19 (journal 537): an fd-tagged redirect (`2>f`) on a compound
# gets the bare `>f`'s verdict, code and position, wherever the compound
# stands. The suffix check saw only operator tokens, so the `2` began a new
# statement: SB5011 "command '2' not found", SB4022 on a last stage, and a
# parser ICE after a `-`.
# csfx.12 at statement level
run_sash '{ /bin/echo hi } 2>/dev/null'
[[ $rc != 0 && -z $out \
   && $err == *':1:1: error [SB4021] a redirect on a compound command'* ]]; chk csfx.12

# csfx.13 on an `if`
run_sash 'if /bin/true { /bin/echo hi } 2>/dev/null'
[[ $rc != 0 && -z $out \
   && $err == *':1:1: error [SB4021] a redirect on a compound command'* ]]; chk csfx.13

# csfx.14 as a `&&` operand
run_sash '/bin/true && { /bin/echo a } 2>/dev/null'
[[ $rc != 0 && -z $out \
   && $err == *':1:14: error [SB4021] a redirect on a compound command'* ]]; chk csfx.14

# csfx.15 as a pipeline's first stage
run_sash '{ /bin/echo a } 2>/dev/null | /bin/cat'
[[ $rc != 0 && -z $out \
   && $err == *':1:1: error [SB4021] a redirect on a compound command'* ]]; chk csfx.15

# csfx.16 as its last stage (was SB4022)
run_sash '/bin/echo a | { /bin/cat } 2>/dev/null'
[[ $rc != 0 && -z $out \
   && $err == *':1:15: error [SB4021] a redirect on a compound command'* ]]; chk csfx.16

# csfx.17 after a `-` (ICE)
run_sash '- { /bin/echo a } 2>/dev/null'
[[ $rc != 0 && -z $out \
   && $err == *':1:3: error [SB4021] a redirect on a compound command'* ]]; chk csfx.17

# csfx.18 in a return producer
run_sash 'fn f { return { /bin/echo a } 2>/dev/null }
if f { echo y }'
[[ $rc != 0 && -z $out \
   && $err == *':1:15: error [SB4021] a redirect on a compound command'* ]]; chk csfx.18

# csfx.19 (control) a digit word with a blank before the operator is not an
#         fd tag, so it is not the compound's redirect. What a word after
#         `}` starts is q-statement-form-residuals; this pins only that the
#         first diagnostic is at the `2`, not on the compound.
run_sash '{ /bin/echo hi } 2 >/dev/null'
[[ $rc != 0 && -z $out && $err != *SB4021* \
   && $err == *':1:18: error ['* ]]; chk csfx.19

# csfx.20 csfx.10 without the fd tag, `! { … } >f`, was the same parser ICE
#         before journal 537: the `!` wrapper hid the compound from the
#         redirect check
run_sash '! { /bin/true } >/dev/null
echo after'
[[ $rc != 0 && -z $out \
   && $err == *':1:3: error [SB4021] a redirect on a compound command'* ]]; chk csfx.20

# acm.1 a comma list with a write is one arithmetic command, not a compound
#       (DD#31): it is held as a group only because a part writes, so a pipe
#       on one is the assignment's SB4022, not the compound's advice to wrap
#       it in a function (that advice, before journal 537)
run_sash 'int a=0
(( a++, a<3 )) | /bin/cat
echo "a=$a"'
[[ $rc != 0 && -z $out \
   && $err == *':2:1: error [SB4022] a variable assignment isn'\''t allowed as a pipeline stage'* ]]; chk acm.1
# acm.2 ... as the last stage
run_sash 'int a=0
/bin/echo x | (( a++, a<3 ))
echo "a=$a"'
[[ $rc != 0 && -z $out \
   && $err == *':2:15: error [SB4022] a variable assignment isn'\''t allowed as a pipeline stage'* ]]; chk acm.2
# acm.3 ... as a middle stage
run_sash 'int a=0
/bin/echo x | (( a++, a<3 )) | /bin/cat
echo "a=$a"'
[[ $rc != 0 && -z $out \
   && $err == *':2:15: error [SB4022] a variable assignment isn'\''t allowed as a pipeline stage'* ]]; chk acm.3
# acm.4 ... and the write need not be the first part
run_sash 'int a=0
(( a<3, a++ )) | /bin/cat
echo "a=$a"'
[[ $rc != 0 && -z $out \
   && $err == *':2:1: error [SB4022] a variable assignment isn'\''t allowed as a pipeline stage'* ]]; chk acm.4
# acm.5 a redirect on one is the assignment's SB4046 (the compound's SB4021
#       before journal 537)
run_sash 'int a=0
(( a++, a<3 )) >/dev/null
echo "a=$a"'
[[ $rc != 0 && -z $out \
   && $err == *':2:1: error [SB4046] a redirect cannot attach to a variable assignment'* ]]; chk acm.5
# acm.6 ... before a list operator too
run_sash 'int a=0
(( a++, a<3 )) >/dev/null && echo y
echo "a=$a"'
[[ $rc != 0 && -z $out \
   && $err == *':2:1: error [SB4046] a redirect cannot attach to a variable assignment'* ]]; chk acm.6
# acm.7 ... and under `!`, whose operand reaches the redirect check apart
#       from a statement's
run_sash 'int a=0
! (( a++, a<3 )) >/dev/null
echo "a=$a"'
[[ $rc != 0 && -z $out \
   && $err == *':2:3: error [SB4046] a redirect cannot attach to a variable assignment'* ]]; chk acm.7
# acm.8 `&` on one is the assignment's SB4024 (the compound's before
#       journal 537)
run_sash 'int a=0
(( a++, a<3 )) &
echo "a=$a"'
[[ $rc != 0 && -z $out \
   && $err == *':2:1: error [SB4024] a variable assignment isn'\''t allowed in a background job'* ]]; chk acm.8
# acm.9 a `-` is peeled: a suppressed one is still an assignment as a stage
run_sash 'int a=0
- (( a++, a<3 )) | /bin/cat
echo "a=$a"'
[[ $rc != 0 && -z $out \
   && $err == *':2:3: error [SB4022] a variable assignment isn'\''t allowed as a pipeline stage'* ]]; chk acm.9
# acm.10 ... and under `&`
run_sash 'int a=0
- (( a++, a<3 )) &
echo "a=$a"'
[[ $rc != 0 && -z $out \
   && $err == *':2:3: error [SB4024] a variable assignment isn'\''t allowed in a background job'* ]]; chk acm.10
# acm.11 `&&` / `||` after one keeps the compound's SB4023 for now: whether
#        that operand is a condition is an open question (DD#31 addendum
#        2026-10-27)
run_sash 'int a=0
(( a++, a<3 )) && echo y
echo "a=$a"'
[[ $rc != 0 && -z $out \
   && $err == *':2:1: error [SB4023] `&&` / `||` on a compound command'* ]]; chk acm.11
# acm.12 ... and as a list's operand
run_sash 'int a=0
/bin/true && (( a++, a<3 )) && echo y
echo "a=$a"'
[[ $rc != 0 && -z $out \
   && $err == *':2:14: error [SB4023] `&&` / `||` on a compound command'* ]]; chk acm.12


# dspan.1 every "expected expression" pointed at 1:1 — `parts_to_expr` built
#         its diagnostic with a HARDCODED `Span { line: 1, col: 1 }`, so the
#         reader was sent to the first byte of the file no matter which line
#         the offending token was on. A stray `}` on line 2 must say line 2.
#         The code is now SB4072 (a stray `}` is diagnosed directly rather
#         than falling through to a bare "expected expression"); these three
#         are about the SPAN, which is what must not regress.
run_sash '/bin/echo hi
}'
[[ $rc != 0 && $err == *SB4072* && $err == *:2:1:* ]]; chk dspan.1

# dspan.2 the column matters too: `}` inside a command argument is at the
#         brace, not at the start of the line
run_sash '/bin/echo hi
/bin/echo a}b'
[[ $rc != 0 && $err == *SB4072* && $err == *:2:12:* ]]; chk dspan.2

# dspan.3 the form that exposed it: a trailing `}` after a splat disposition
run_sash 'array a=(p q)
/bin/echo @{a:-x}}'
[[ $rc != 0 && $err == *SB4072* && $err == *:2:18:* ]]; chk dspan.3

# dspan.4 keeps `parts_to_expr` itself guarded. dspan.1-3 used a stray `}`,
#         which SB4072 now intercepts before that code is reached — so
#         without this the hardcoded-1:1 regression would be unguarded. A
#         stray `)` still lands there, and must also name its own column.
run_sash '/bin/echo hi
/bin/echo a)b'
[[ $rc != 0 && $err == *SB4014* && $err == *:2:12:* ]]; chk dspan.4

# ── skw.1-skw.34: a block keyword or `}` that belongs to no block (journal 537)
# A keyword that can only close or continue a block (then, do, in, esac,
# else, elif, fi, done) fell to the ordinary-word path wherever a command
# could begin, so a stray `fi` was SB5011 "command 'fi' not found", whose
# advice to define it nobody can take (SB5092 forbids `fn fi`). Each is now
# SB4072, like a stray `}`, at a statement head and in every position a
# command may stand (skw.1-skw.17). SB4072's `}` text now says when a `}`
# does close a block (skw.18, skw.19), and the `else` and `elif` texts say
# that after a brace body the clause must share the `}`'s line (skw.20). A
# block that runs into a closer it cannot take is reported at that closer,
# as bash does, where it was SB4014 or SB4027 at the `if` or `while`
# (skw.21-skw.25). An `elif` or `else` clause's
# errors name that clause, not its `if` (skw.26-skw.28). After a parse error,
# a keyword left over by the recovery is not reported again (skw.29). A
# stray inside a `$(` that is never closed is reported as that `$(`
# (skw.30-skw.33). A keyword that is an argument is still a plain word
# (skw.34).
skw_then='a `then` here continues nothing — a `then` may only follow a complete `if` or `elif` condition'
skw_do='a `do` here opens nothing — a `do` may only follow a complete `while`, `until` or `for` header'
skw_in='an `in` here continues nothing — an `in` may only follow the loop variable of a `for` (`for NAME in …`)'
skw_esac='an `esac` here closes nothing — sash has no `case`; use the `[[ ]]` dispatch form: `[[ $x == pat { … } other { … } || { … } ]]` (exact match) or `[[ $x =~ regex { … } || { … } ]]` (regex match)'
skw_fi='a `fi` here closes nothing — a `fi` ends the innermost open block only if that block is an `if … then` body (or its `elif … then` or `else` part), and only as a statement of its own'
skw_done='a `done` here closes nothing — a `done` ends the innermost open block only if that block is the `do` body of a `while`, `until` or `for`, and only as a statement of its own'
skw_else=$'an `else` here continues nothing — an `else` continues the innermost open block only if that block is an `if … then` or `elif … then` body, and only as a statement of its own; after a brace-form body it must directly follow that body\'s closing `}`, on the same line (`} else {`)'
skw_elif=$'an `elif` here continues nothing — an `elif` continues the innermost open block only if that block is an `if … then` or `elif … then` body, and only as a statement of its own; after a brace-form body it must directly follow that body\'s closing `}`, on the same line (`} elif … {`)'
skw_rb=$'a `}` here closes nothing — a `}` ends the innermost open block only if that block is a `{ … }` one, and never right after a `|`, `&&`, `||` or `!` that still needs a command. For a literal brace, quote it (`\'a}b\'`)'
skw_sub='unterminated `$(` — it is never closed before the end of the source, so everything after it was read as part of it'
# skw.1-skw.8 each keyword at a statement head; skw.7 and skw.8 close one
#             block too many
run_sash 'then /bin/echo a'
[[ $rc != 0 && -z $out && $err == *":1:1: error [SB4072] $skw_then"* ]]; chk skw.1
run_sash 'do /bin/echo a'
[[ $rc != 0 && -z $out && $err == *":1:1: error [SB4072] $skw_do"* ]]; chk skw.2
run_sash 'in a b'
[[ $rc != 0 && -z $out && $err == *":1:1: error [SB4072] $skw_in"* ]]; chk skw.3
run_sash 'esac'
[[ $rc != 0 && -z $out && $err == *":1:1: error [SB4072] $skw_esac"* ]]; chk skw.4
run_sash 'else /bin/echo a'
[[ $rc != 0 && -z $out && $err == *":1:1: error [SB4072] $skw_else"* ]]; chk skw.5
run_sash 'elif /bin/true; then /bin/echo a; fi'
[[ $rc != 0 && -z $out && $err == *":1:1: error [SB4072] $skw_elif"* ]]; chk skw.6
run_sash 'for x in a; do /bin/echo $x; done; done'
[[ $rc != 0 && -z $out && $err == *":1:36: error [SB4072] $skw_done"* ]]; chk skw.7
run_sash 'if /bin/true; then /bin/echo a; fi; fi'
[[ $rc != 0 && -z $out && $err == *":1:37: error [SB4072] $skw_fi"* ]]; chk skw.8
# skw.9-skw.15 ... after `|`, `&&` and `&`, behind `!`, `-` and `&& -`, and
#              as a `return` producer
run_sash '/bin/echo a | then'
[[ $rc != 0 && -z $out && $err == *":1:15: error [SB4072] $skw_then"* ]]; chk skw.9
run_sash '/bin/true && fi'
[[ $rc != 0 && -z $out && $err == *":1:14: error [SB4072] $skw_fi"* ]]; chk skw.10
run_sash '/bin/echo a & fi'
[[ $rc != 0 && -z $out && $err == *":1:15: error [SB4072] $skw_fi"* ]]; chk skw.11
run_sash '! fi'
[[ $rc != 0 && -z $out && $err == *":1:3: error [SB4072] $skw_fi"* ]]; chk skw.12
run_sash '- fi'
[[ $rc != 0 && -z $out && $err == *":1:3: error [SB4072] $skw_fi"* ]]; chk skw.13
run_sash '/bin/echo a && - fi'
[[ $rc != 0 && -z $out && $err == *":1:18: error [SB4072] $skw_fi"* ]]; chk skw.14
run_sash 'fn f { return ! fi }
f'
[[ $rc != 0 && -z $out && $err == *":1:17: error [SB4072] $skw_fi"* ]]; chk skw.15
# skw.16-skw.17 ... at the head of a capture body, unquoted and inside
#               "…" (where the position comes through the re-lex). The
#               capture is closed, so the keyword keeps SB4072
run_sash 'x=$(fi)
/bin/echo "$x"'
[[ $rc != 0 && -z $out && $err == *":1:5: error [SB4072] $skw_fi"* ]]; chk skw.16
run_sash 'x="$(fi)"
/bin/echo "$x"'
[[ $rc != 0 && -z $out && $err == *":1:6: error [SB4072] $skw_fi"* ]]; chk skw.17
# skw.18-skw.19 a `}` after a `|` or a `-` that still needs a command
run_sash '{ /bin/echo a | }'
[[ $rc != 0 && -z $out && $err == *":1:17: error [SB4072] $skw_rb"* ]]; chk skw.18
run_sash '- }'
[[ $rc != 0 && -z $out && $err == *":1:3: error [SB4072] $skw_rb"* ]]; chk skw.19
# skw.20 an `else` on the line after a brace body's `}` continues nothing.
#        The `}` that follows it is a known parse-error cascade, unasserted
#        (notes/tasks/bug-parse-error-in-block-cascades-sb4072.md)
run_sash 'if /bin/true {
  /bin/echo a
}
else {
  /bin/echo b
}'
[[ $rc != 0 && -z $out && $err == *":4:1: error [SB4072] $skw_else"* ]]; chk skw.20
# skw.21-skw.25 a header with no `then` or `do` runs into a closer: an `if`
#               and a `while` header, an `elif` header, and an inner `if`
#               body cut short by its loop's `done`. skw.25 is a `fi` that
#               does not stand as a statement of its own
run_sash 'if /bin/true
  /bin/echo a
fi'
[[ $rc != 0 && -z $out && $err == *":3:1: error [SB4072] $skw_fi"* ]]; chk skw.21
run_sash 'while /bin/false
  /bin/echo a
done'
[[ $rc != 0 && -z $out && $err == *":3:1: error [SB4072] $skw_done"* ]]; chk skw.22
run_sash 'if /bin/false; then
  echo a
elif /bin/true
  echo b
fi'
[[ $rc != 0 && -z $out && $err == *":5:1: error [SB4072] $skw_fi"* ]]; chk skw.23
run_sash 'while /bin/false; do if /bin/true; then /bin/echo a; done'
[[ $rc != 0 && -z $out && $err == *":1:54: error [SB4072] $skw_done"* ]]; chk skw.24
run_sash 'if /bin/true; then
  /bin/echo a | fi'
[[ $rc != 0 && -z $out && $err == *":2:17: error [SB4072] $skw_fi"* ]]; chk skw.25
# skw.26-skw.28 with no closer to run into, the clause names itself
run_sash 'if /bin/false; then /bin/echo a; elif /bin/true'
[[ $rc != 0 && -z $out \
   && $err == *":1:34: error [SB4014] expected 'then' after elif"* ]]; chk skw.26
run_sash 'if /bin/false {
  echo a
} elif /bin/true
echo b'
[[ $rc != 0 && -z $out \
   && $err == *":3:3: error [SB4014] expected '{' block body"* ]]; chk skw.27
run_sash 'if /bin/false {
  echo a
} else {
  echo b'
[[ $rc != 0 && -z $out \
   && $err == *":3:3: error [SB4014] expected '}' to close the block body"* ]]; chk skw.28
# skw.29 the recovery from an error in a header skips to the `;`, so the
#        `then` and the `fi` that follow are left over; reporting them would
#        report the header's error two more times
run_sash 'if (( 1 + )); then
  /bin/echo a
fi'
[[ $rc != 0 && -z $out && $err == *':1:4: error [SB4020] '* \
   && $err != *SB4072* ]]; chk skw.29
# skw.30-skw.33 the `}` or `fi` was written to close the `if`, and is stray
#               only because the open `$(` took it in. A later `$( … )` or
#               `$(( … ))` does not close the open one (skw.32, skw.33)
run_sash 'if /bin/true {
  x=$(/bin/echo a
}
/bin/echo "$x"'
[[ $rc != 0 && -z $out && $err == *":2:5: error [SB4057] $skw_sub"* \
   && $err != *SB4072* ]]; chk skw.30
run_sash 'if /bin/true; then
  x=$(/bin/echo a
fi
/bin/echo "$x"'
[[ $rc != 0 && -z $out && $err == *":2:5: error [SB4057] $skw_sub"* \
   && $err != *SB4072* ]]; chk skw.31
run_sash 'if /bin/true {
  x=$(/bin/echo a
}
y=$(/bin/echo b)
/bin/echo "$x$y"'
[[ $rc != 0 && -z $out && $err == *":2:5: error [SB4057] $skw_sub"* \
   && $err != *SB4072* ]]; chk skw.32
run_sash 'if /bin/true {
  x=$(/bin/echo a
}
n=$(( 1 + 2 ))
/bin/echo "$x$n"'
[[ $rc != 0 && -z $out && $err == *":2:5: error [SB4057] $skw_sub"* \
   && $err != *SB4072* ]]; chk skw.33
# skw.34 (positive) as an argument, each keyword is an ordinary word
run_sash 'echo do done esac in then elif else fi'
[[ $rc = 0 && $out = 'do done esac in then elif else fi' && -z $rerr ]]; chk skw.34

# --- the operand checks reach every context, not just `echo $(…)` ---
# The string-intrinsic check ran from `check_stmt`, which only ever sees a
# statement-level command. A command substitution is an EXPRESSION, so
# `$(toupper $n)` was checked in `echo $(toupper $n)` and silently skipped on
# an assignment RHS — the only spelling that does anything with the result.
# Every sint.N spec above used the `echo` form, so the cohort was green over a
# check that was vacuous in its useful case. Front-end — all modes.

run_sash 'int n=42
str t=$(toupper $n)
echo $t'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk sint.20
# sint.21 is NOT a proof of this pass: neutering the pass leaves it green,
# because `strlen` on an int is also caught by check_int_string_ops. Kept as a
# true assertion, labelled so it is not mistaken for coverage of the traversal.
run_sash 'int n=42
str t=$(strlen $n)
echo $t'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk sint.21
run_sash 'int n=42
str t=$(basename $n)
echo $t'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk sint.22
# nested one level deeper: inside a double-quoted assignment RHS
run_sash 'int n=42
str t="$(tolower $n)"
echo $t'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk sint.23
# and inside a function body, where the enclosing statement is not top level
run_sash 'fn f { int n=42
str t=$(toupper $n)
echo $t }
f'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk sint.24
# the legitimate string operand still compiles in the same spelling — the
# widened reach must not reject what the `echo` form always accepted
run_sash 'str s=hi
str t=$(toupper $s)
echo $t'
[[ $rc = 0 && $out = "HI" && -z $err ]]; chk sint.25
# `$(join args)` / `$(keys M)` are scope-sensitive, not type-sensitive: they
# ask whether the operand is a known collection, and the implicit `args` is
# registered as check_stmt descends. Re-running them over the finished
# top-level env rejected these four. They stay on their own path.
run_sash 'fn f { echo $(join args ,) }
f a b c'
[[ $rc = 0 && $out = "a,b,c" && -z $err ]]; chk sint.26

# --- exit / wait / arg take an int, not a string (DG 2026-09-26) ---
# All three silently coerced a string operand through the runtime's atoi.
# `arg` was the worst: `$(arg $s)` on a string yielded argv[0] -- a wrong
# answer rather than a diagnostic. `exit $s` reported SUCCESS for a script
# that meant to fail. Front-end -- all modes.

run_sash 'exit 0'
[[ $rc = 0 && -z $err ]]; chk isink.1
# a signed decimal literal stays legal -- bash truncates to 255
run_sash 'exit -1'
[[ $rc = 255 && -z $err ]]; chk isink.2
# the empty operand is rejected (DG): it read as 0, i.e. success
run_sash "exit ''"
[[ $rc != 0 && $err == *SB4030* ]]; chk isink.3
# a QUOTED number is a string -- quoting is the int->string conversion
# everywhere else in sash, so it must not be the int spelling here
run_sash 'exit "1"'
[[ $rc != 0 && $err == *SB4030* ]]; chk isink.4
run_sash 'str s=0
exit $s'
[[ $rc != 0 && $err == *SB4030* ]]; chk isink.5
run_sash 'str s=1
wait $s'
[[ $rc != 0 && $err == *SB4030* ]]; chk isink.6
# `$(( ))` is the accountable conversion: a non-numeric value aborts
# rather than reading as 0
run_sash 'str s=3
exit $(( ${s:?bad} ))'
[[ $rc = 3 && -z $err ]]; chk isink.7
# `arg` -- the silent argv[0] case. This is only reached because the
# operand checks now walk expressions; on the old statement-only
# traversal an assignment RHS was never looked at.
run_sash 'str s=x
str t=$(arg $s)
attest t [[ -n $t ]]
echo $t'
[[ $rc != 0 && $err == *SB4030* ]]; chk isink.8
run_sash 'int i=1
str t=$(arg $i)
attest t [[ -n $t ]]
echo $t' alpha
[[ $rc = 0 && $out = "alpha" && -z $err ]]; chk isink.9
# slot 1 of `arg` is a string default and is NOT an int sink
run_sash 'str t=$(arg 9 fallback)
attest t [[ -n $t ]]
echo $t'
[[ $rc = 0 && $out = "fallback" && -z $err ]]; chk isink.10
# $! is sugar for $(getcpid), which is int-returning -- the punctuation
# form must type the same as the intrinsic, or `wait $!` fails a rule
# that `wait $(getcpid)` passes
run_sash '/bin/sleep 0 &
p=$!
- wait $p
echo done'
[[ $rc = 0 && $out = "done" && -z $err ]]; chk isink.11
run_sash 'int n=7
exit $n'
[[ $rc = 7 && -z $err ]]; chk isink.12

# DD#111: `$?` is sugar for `$(status)`, which is int-returning. These pin the
# int-ness at the sinks. Nothing caught it before: neutering `status` out of
# `is_int_returning_intrinsic` broke `int x=$?` / `exit $?` / `wait $?` with a
# FULLY GREEN sweep. `(( 0 ))` is the non-aborting way to set a failing status.
run_sash '(( 0 ))
int n=$?
echo "[$n]"'
[[ $rc = 0 && $out = "[1]" && -z $err ]]; chk isink.13

run_sash '(( 0 ))
exit $?'
[[ $rc = 1 && -z $err ]]; chk isink.14

# ...and the intrinsic spelling must type identically to the punctuation. This
# is the DD#111 invariant itself: after the desugar they are the same node, so
# a divergence here means a construction site was not routed through
# `magic_sugar_expansion`.
run_sash '(( 0 ))
int n=$(status)
echo "[$n]"'
[[ $rc = 0 && $out = "[1]" && -z $err ]]; chk isink.15

# --- DD#120: the invoker's channel -------------------------------------
# Every diagnostic sash emits is the INVOKER speaking about its own failure,
# so none of them can be captured by a redirect written on the command. The
# rd.5-rd.8 block above covers setup failure; these cover the other three
# emitters, the lockstep rule that makes `exec 2>` different from `cmd 2>`,
# and the END-block consequence.
#
# The controls matter more than the checks here. "The file is empty" is
# satisfied by a compiler that never writes to redirects at all, so iv.3
# pins that a RUNNING command's own stderr still lands there, and iv.9 pins
# that a persistent `exec >` is still honoured by END.

# iv.1 exec failure — emitted by the forked CHILD after execve fails, which
# is the case that only works because the scope fd is F_DUPFD_CLOEXEC: still
# open when execve fails, gone when it succeeds.
rm -f iv1_cap.txt
run_sash_vanish 'SPEC_VANISH_CMD 2> iv1_cap.txt
echo after'
[[ $rc = 127 && -z $out    && $rerr == *": No such file or directory"*    && -f iv1_cap.txt && ! -s iv1_cap.txt ]]; chk iv.1

# iv.2 the DD#55 implicit-check abort, emitted by the parent after wait
rm -f iv2_cap.txt
run_sash '/bin/false 2> iv2_cap.txt
echo after'
[[ $rc = 1 && -z $out    && $rerr == *"command failed (exit 1)"*    && -f iv2_cap.txt && ! -s iv2_cap.txt ]]; chk iv.2

# iv.3 CONTROL for iv.1/iv.2: a running command's own stderr is not a
# diagnostic and must still go through the redirect. Without this, a
# compiler that simply stopped writing to `2>` files would pass both above.
rm -f iv3_cap.txt
run_sash '/bin/sh -c "echo real 1>&2" 2> iv3_cap.txt'
[[ $rc = 0 && -z $out && -z $rerr && "$(cat iv3_cap.txt)" = "real" ]]; chk iv.3

# iv.4 lockstep: `exec 2>` rebinds the INVOKER's own stderr, so the channel
# follows it and the diagnostic DOES land in the file.
rm -f iv4_log.txt
run_sash 'exec 2> iv4_log.txt
/bin/false'
[[ $rc = 1 && -z $out && -z $rerr    && "$(cat iv4_log.txt)" == *"command failed (exit 1)"* ]]; chk iv.4

# iv.5 the paired control: the same diagnostic, the same fd, the same file —
# only the redirect's scope differs. iv.4 without iv.5 would pass a compiler
# that always wrote diagnostics to fd 2.
rm -f iv5_log.txt
run_sash '/bin/false 2> iv5_log.txt'
[[ $rc = 1 && -z $out && $rerr == *"command failed (exit 1)"*    && -f iv5_log.txt && ! -s iv5_log.txt ]]; chk iv.5

# iv.6 END is the invoker's code, so it runs on the invoker's fds. The abort
# terminates the block before the command's restore can run, so without
# sash_scope_restore() at END entry `cleanup` is written INTO iv6_log.txt.
# Both backends had this wrong together, so the oracle could not see it.
rm -f iv6_log.txt
run_sash 'END { echo cleanup }
/bin/false > iv6_log.txt'
[[ $rc = 1 && $out = "cleanup" && -f iv6_log.txt && ! -s iv6_log.txt ]]; chk iv.6

# iv.7 the same leak by the exec-failure route, which aborts from a
# different place
rm -f iv7_log.txt
run_sash_vanish 'END { echo cleanup }
SPEC_VANISH_CMD > iv7_log.txt'
[[ $rc = 127 && $out = "cleanup" && -f iv7_log.txt && ! -s iv7_log.txt ]]; chk iv.7

# iv.8 and by the echo write-failure route — a third abort site, which is
# why the fix belongs at END entry rather than at each of them
run_sash 'END { echo cleanup }
echo hi > /dev/full'
[[ $rc = 1 && $out = "cleanup" && $rerr == *"write error"* ]]; chk iv.8

# iv.9 CONTROL for iv.6-iv.8: restoring at END entry must restore the
# invoker's CURRENT fds, not the ones it started with. A persistent
# `exec >` is the invoker rebinding its own stdout, so END follows it —
# exactly as bash's EXIT trap does. A `sash_scope_restore` that reset to
# the startup fds would put "cleanup" on the terminal and fail here.
rm -f iv9_log.txt
run_sash 'END { echo cleanup }
exec > iv9_log.txt
echo body'
[[ $rc = 0 && -z $out && "$(cat iv9_log.txt)" = "body
cleanup" ]]; chk iv.9


# ── csc.1-csc.10: a `$( )` closer beats DD#71 absorb ────────────────
# `$(int x=1)` was reported as SB4057 "unterminated `$(`" with the `)`
# plainly there. DD#71 makes `)` an absorbable literal in an assignment
# value, so the value word ate the substitution's own terminator and the
# body ran to end of input. DD#71 scopes absorb "to whitespace / closer",
# and `]]` already beats absorb for a `[[ ]]` operand (csc.9) — `)` inside
# a `$( )` body is the same rule. Scanner now carries `cmd_sub_depth`,
# mirroring `arith_depth`.
run_sash 'echo $(int x=1)'
[[ $rc = 0 && -z $out && -z $err ]]; chk csc.1
# the spaced spelling took `)` as the next declarator name and reported
# SB4014 "expected variable name" on a declaration that was complete
run_sash 'echo $(int x=1 )'
[[ $rc = 0 && -z $out && -z $err ]]; chk csc.2
run_sash 'echo $(str s=a)'
[[ $rc = 0 && -z $out && -z $err ]]; chk csc.3
# the quoted spelling had the same fault (journal 422's convergence)
run_sash 'echo "$(int x=1)"'
[[ $rc = 0 && -z $out && -z $err ]]; chk csc.4
# a declaration that is NOT last always worked — the `;` stopped the
# declaration loop before the `)` was reached
run_sash 'echo $(int x=1; echo $x)'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk csc.5
# an array literal inside a body still finds its own `)`
run_sash 'echo $(array a=(1 2); echo ${a[1]})'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk csc.6
# the body is an anonymous function, not a subshell — a write to the
# existing global `x` persists (DD#86 addendum, DG 2026-09-28). Bash prints 1.
run_sash 'int x=1
echo $(x=2)
echo $x'
[[ $rc = 0 && $out = $'\n2' && -z $err ]]; chk csc.7
# DD#71 absorb is UNCHANGED outside a body — `)` is still a literal there
run_sash 'str s=a)b
echo "[$s]"'
[[ $rc = 0 && $out = "[a)b]" && -z $err ]]; chk csc.8
# …and a `[[ ]]` operand still absorbs its parens, which is the precedent
# the cmd-sub rule is modelled on
run_sash 'str x=ab
if [[ $x =~ ^(a|b)+$ ]] { echo yes }'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk csc.9
# a genuinely unterminated `$(` is still SB4057 — the fix must not make
# the real diagnostic unreachable
run_sash 'echo $(echo hi'
[[ $rc != 0 && -z $out && $err == *SB4057* ]]; chk csc.10


# --- DD#86: a `$( … )` body is an anonymous function body ------------
#
# Two bugs, one shape. A statement builtin as the sole body resolved to
# nothing, so the backend built an exec argv whose head was a bare builtin
# name and the program printed `sash: bug: bare command passed to sash_exec`
# into its own output with a successful exit. And the statement passes never
# entered a body at all — 42 of the 45 walkers in validate.rs recursed
# `child_bodies()`, which did not yield one — so what should have been a
# diagnostic was a silent acceptance, or a compiler panic where `transform`
# had been promised a validated argument.

# csc.11 `:` as the sole body captures nothing and does not ICE
run_sash 'str s=$(:)
attest s [[ -z $s ]]
echo "[$s]"'
[[ $rc = 0 && $out = "[]" && -z $err ]]; chk csc.11

# csc.12 `write` as the sole body — it has no value lowering, so it is the
#        body path that makes it work at all
run_sash 'str s=$(write hi)
attest s [[ -n $s ]]
echo "[$s]"'
[[ $rc = 0 && $out = "[hi]" && -z $err ]]; chk csc.12

# csc.13 `cd` as the sole body
run_sash 'str s=$(cd /tmp)
attest s [[ -z $s ]]
echo "[$s]"'
[[ $rc = 0 && $out = "[]" && -z $err ]]; chk csc.13

# csc.14 `wait` is no longer a capture of a statement's stdout. DD#126 makes
#        it an int intrinsic yielding the reaped pid, so a `str` target is a
#        type error. This spec used to assert the empty capture.
run_sash 'str s=$(wait)
echo "[$s]"'
[[ $rc != 0 && $err == *SB4030* ]]; chk csc.14

# csc.15 `attest` as the sole body — and its declassification persists,
#        because a body is a function body, not a subshell (DD#86)
run_sash 'str v=abc
str s=$(attest v [[ -n $v ]])
attest s [[ -z $s ]]
echo "[$s][$v]"'
[[ $rc = 0 && $out = "[][abc]" && -z $err ]]; chk csc.15

# csc.16 DD#60's out-param fill is a statement, and a statement is legal in a
#        body — `argv N a` fills, `$(argv N)` splats, and one argument
#        discriminates them
run_sash 'str s=$(argv 1 b; attest x in b [[ -n $x ]]; /bin/echo @{b:-none})
attest s [[ -n $s ]]
echo "[$s]"' a b
[[ $rc = 0 && $out = "[a b]" && -z $err ]]; chk csc.16

# csc.17 `unset` of an element as the sole body
run_sash 'array q=(1 2)
str s=$(unset q[0])
attest s [[ -z $s ]]
echo "[$s]$(len q)"'
[[ $rc = 0 && $out = "[]1" && -z $err ]]; chk csc.17

# csc.18 a `[[ … ]]` body is a clean SB4018 reject, not the `sash: bug:` ICE.
#        The operand is a variable on purpose: with a literal, SB5086 (constant
#        test) fires first — which is itself the descent working.
run_sash 'str v=x
str s=$([[ -n $v ]])
attest s [[ -z $s ]]
echo "[$s]"'
[[ $rc != 0 && -z $out && $err == *SB4018* ]]; chk csc.18

# csc.19 SB4060: `transform` is promised a validated signal number. Before the
#        body joined the statement recursion this panicked the compiler.
run_sash 'str s=$(signal INT ignore)
attest s [[ -z $s ]]
echo "[$s]"'
[[ $rc != 0 && -z $out && $err == *SB4060* ]]; chk csc.19

# csc.20 the same, nested inside an `if` in the body — the descent is
#        recursive, not just one level
run_sash 'str s=$(if [[ -n a ]]; then signal INT ignore; fi)
attest s [[ -z $s ]]
echo "[$s]"'
[[ $rc != 0 && -z $out && $err == *SB4060* ]]; chk csc.20

# csc.21 SB5060: DD#99 retired `mapfile`. In a body it was a runtime ICE.
run_sash 'str s=$(mapfile -m q)
attest s [[ -z $s ]]
echo "[$s]"'
[[ $rc != 0 && -z $out && $err == *SB5060* ]]; chk csc.21

# csc.22 SB4066: `cd` arity, silently accepted in a body
run_sash 'str s=$(cd /tmp /var)
attest s [[ -z $s ]]
echo "[$s]"'
[[ $rc != 0 && -z $out && $err == *SB4066* ]]; chk csc.22

# csc.23 SB4052: `close` wants a file descriptor
run_sash 'str s=$(close abc)
attest s [[ -z $s ]]
echo "[$s]"'
[[ $rc != 0 && -z $out && $err == *SB4052* ]]; chk csc.23

# csc.24 SB5081: `break` with no enclosing loop. A body is a function body, so
#        an enclosing loop OUTSIDE the substitution does not supply one either.
run_sash 'str s=$(break)
attest s [[ -z $s ]]
echo "[$s]"'
[[ $rc != 0 && -z $out && $err == *SB5081* ]]; chk csc.24

# csc.25 SB5057: a quoted `read` target names nothing
run_sash 'str s=$(read -a "q")
attest s [[ -z $s ]]
echo "[$s]"'
[[ $rc != 0 && -z $out && $err == *SB5057* ]]; chk csc.25

# csc.26 SB5080: `args` is reserved as a loop variable in a body too
run_sash 'str s=$(for args in a b; do /bin/echo $args; done)
attest s [[ -z $s ]]
echo "[$s]"'
[[ $rc != 0 && -z $out && $err == *SB5080* ]]; chk csc.26

# csc.27 SB5011: DD#23 command resolution reaches a body
run_sash 'str s=$(nosuchcmd)
attest s [[ -z $s ]]
echo "[$s]"'
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk csc.27

# csc.28 a body reached through a `[[ … ]]` operand. The walker this replaced
#        had arms for Concat/DoubleQuoted/Array/Subscript and nothing else, so
#        a substitution inside a test operand was never visited at all.
run_sash 'if [[ -n $(cd /tmp /var) ]]; then echo y; fi'
[[ $rc != 0 && -z $out && $err == *SB4066* ]]; chk csc.28

# csc.29 control: a value intrinsic IS the right thing when it is the whole
#        substitution — the descent must not reject what it now walks into
run_sash '/bin/true
int n=$(status)
echo "[$n]"'
[[ $rc = 0 && $out = "[0]" && -z $err ]]; chk csc.29

# csc.30 control: one statement among several is still SB5073, and the wording
#        is the body one — the statement-position sentence ("only valid inside
#        a command substitution") would be absurd here
run_sash 'str s=$(getenv HOME; /bin/true)
attest s [[ -z $s ]]
echo "[$s]"'
[[ $rc != 0 && -z $out && $err == *"not one statement among several"* ]]; chk csc.30


# csc.31 `validate_expr` reaches a body. Pre-(c) it did, by accident: the
#        expression walker descended THROUGH the substitution node into the
#        body's statements. Moving bodies to the statement side took that away
#        and nothing in the suite noticed — `$RANDOM` inside a body compiled
#        clean for one build.
run_sash 'str s=$(/bin/echo $RANDOM)
attest s [[ -n $s ]]
echo "[$s]"'
[[ $rc != 0 && -z $out && $err == *SB5017* ]]; chk csc.31

# csc.32 the same for an environment variable (SB5018 → use `$(getenv NAME)`)
run_sash 'str s=$(/bin/echo $HOME)
attest s [[ -n $s ]]
echo "[$s]"'
[[ $rc != 0 && -z $out && $err == *SB5018* ]]; chk csc.32

# csc.33 and for `**` (SB5006), which is a Literal arm — a different shape
#        again, so the descent is not special-casing expansions
run_sash 'str s=$(/bin/echo "a**b")
attest s [[ -n $s ]]
echo "[$s]"'
[[ $rc != 0 && -z $out && $err == *SB5006* ]]; chk csc.33

# csc.34 the duplication guard. Bodies belong to exactly one side of the
#        recursion now; if a wrapper statement took them BOTH from
#        `validate_nested` and from the unconditional descent, this prints
#        SB5017 twice. Journal 308 is the precedent — that failure mode
#        multiplies with nesting depth, so an `if` inside a `while` is the
#        cheapest shape that catches it.
run_sash 'while [[ -n $(getenv HOME) ]]; do
    if [[ -n a ]]; then
        str s=$(/bin/echo $RANDOM)
        echo "[$s]"
    fi
    break
done'
[[ $rc != 0 && -z $out && $(grep -c SB5017 <<< "$err") = 1 ]]; chk csc.34

# csc.35-csc.40 the TYPE side of the same descent. `validate` reaching a body
#        (csc.31-csc.34 above) says nothing about `type_check`, which is a
#        separate pass over the same statements — and its `check_stmt` had the
#        identical fault: the arms that carry a substitution (`Command`,
#        `Declaration`, `Assignment`) are leaves that never recursed, so a
#        capture's body was never type-checked at all. DD#30's `unset`-on-a-
#        scalar is the witness: rejected everywhere else, silently accepted
#        inside `$( )`.

# csc.35 the base case
run_sash 'str s=$(unset zz)
attest s [[ -z $s ]]
echo "[$s]"'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk csc.35

# csc.36 through a wrapper statement inside the body — the body's own
#        statements recurse like any others once the descent is in
run_sash 'str v=a
str s=$(if [[ $v == a ]]; then unset zz; fi)
attest s [[ -z $s ]]
echo "[$s]"'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk csc.36

# csc.37 a body nested inside another body reaches it too
run_sash 'str s=$(/bin/echo "$(unset zz)")
attest s [[ -n $s ]] || s=x
echo "[$s]"'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk csc.37

# csc.38 the duplication guard for the type pass, matching csc.34. `check_stmt`
#        descends unconditionally while its `_` arm still walks
#        `structural_bodies()`; take the body twice and this prints SB4030
#        once per path.
run_sash 'str v=a
int i=0
str s=$(while (( i < 1 )); do if [[ $v == a ]]; then unset zz; fi; i=$(( i + 1 )); done)
attest s [[ -z $s ]]
echo "[$s]"'
[[ $rc != 0 && -z $out && $(grep -c SB4030 <<< "$err") = 1 ]]; chk csc.38

# csc.39 DD#30's carve-out survives the descent: the subscript form is still
#        allowed inside a body, exactly as it is outside one
run_sash 'array q=(1 2)
str s=$(unset q[0])
attest s [[ -z $s ]]
echo "[$s]($(len q))"'
[[ $rc = 0 && $out == "[](1)" ]]; chk csc.39

# csc.40 and the control: a scalar `unset` at top level was always rejected.
#        If this ever passes, the diagnostic moved rather than the descent
#        working.
run_sash 'unset zz
echo done'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk csc.40

# qhead.* A single-quoted command head denotes a literal command name, so a
# path containing a space can be run at all. Before this there was no spelling
# for one: bare broke the word, and a double-quoted head is SB5011 because
# interpolation makes it genuinely non-literal.
#
# The quoting is NOT a second spelling of an ordinary name (DG, 2026-10-10 —
# "should only be used when there is no unquoted way to say the command
# name"), so the rule is two-sided and both sides are pinned here. The test is
# shared with the lexer's own `word_ends_here`, cross-checked against the real
# Scanner by a unit test, so these specs pin the POLICY rather than restating
# the byte set.
rm -rf qh_dir; mkdir qh_dir; printf '#!/bin/sh\necho spaced\n' > 'qh_dir/qh tool'
chmod +x 'qh_dir/qh tool'

# qhead.1-3 superfluous quotes are rejected. A bare name, an absolute path and
# a name containing `*` all have bare spellings — `*` is not a word break, and
# per DD#39 not a glob in this position either.
run_sash "'echo' hi"
[[ $rc != 0 && -z $out && $err == *SB5096* && $err == *"without quotes"* ]]; chk qhead.1
run_sash "'/bin/echo' hi"
[[ $rc != 0 && -z $out && $err == *SB5096* ]]; chk qhead.2
run_sash "'ech*'"
[[ $rc != 0 && -z $out && $err == *SB5096* ]]; chk qhead.3

# qhead.4 the case the rule exists for: a path with a space runs.
run_sash "'$PWD/qh_dir/qh tool'"
[[ $rc = 0 && $out == spaced ]]; chk qhead.4

# qhead.5-6 double quotes are NOT the escape hatch. Both are rejected, but by
# whichever rule is actually true of them — that distinction is qhead.13-21.
# `"echo"` is a constant that needed no quotes at all (SB5096); the second has
# a real `$PWD` in it, so it is the SB5011 the message describes.
run_sash '"echo" hi'
[[ $rc != 0 && -z $out && $err == *SB5096* ]]; chk qhead.5
run_sash "\"\$PWD/qh_dir/qh tool\""
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk qhead.6

# qhead.7-9 `*` in command position is an ordinary filename byte. The head
# used to be exempt from SB5095 on the assumption that a glob expands at
# runtime; DD#39 makes the glob star-only and DG scoped it to array context,
# so `ech*` names a program that does not exist. The exemption re-opened
# exactly the compile-time/runtime asymmetry SB5095 was built to close, and
# the bare arm silently fell through to the PATH search for `echo`.
run_sash '/bin/ech* hi'
[[ $rc != 0 && -z $out && $err == *SB5095* && $err == *"no such file"* ]]; chk qhead.7
run_sash 'ech* hi'
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk qhead.8
# ...and the `exec` position reports it too, from the same authority.
run_sash 'exec /bin/ech*'
[[ $rc != 0 && -z $out && $err == *SB5095* ]]; chk qhead.9

# qhead.10-12 `''` is the degenerate case of that normalisation, and DG ruled
# it a *syntax* error (2026-10-11): "an empty name can neither name an
# intrinsic nor an external". So it is SB4074 at parse, not a resolution
# diagnostic. Quoting exists to say a name with no bare spelling; it is not a
# way to say the absence of a name.
#
# Rejecting it at parse is what keeps the case out of every later pass. It
# used to reach `validate` and, in the capture-condition position alone,
# produce a *pair* of diagnostics (SB4025 + SB5011) where every other
# unrunnable head gets exactly one — `$(/bin/nosuchcmd)` is SB5095 by itself.
# SB4025's text named a reason that was not true of `''` (an intrinsic, or a
# concatenation). All three spellings below now report SB4074 and nothing
# else; the three positions are kept so that none can smuggle an empty name
# past the gate.
run_sash "''"
[[ $rc != 0 && -z $out && $err == *SB4074* && $err == *"empty command name"* ]]; chk qhead.10

# qhead.11 the capture-condition position (DD#88) — the one that used to
# report two diagnostics.
run_sash "if y=\$('') { /bin/echo hi }"
[[ $rc != 0 && -z $out && $err == *SB4074* && $err != *SB4025* ]]; chk qhead.11

# qhead.12 ...and inside an ordinary capture.
run_sash "str z = \$('')
echo \"\$z\""
[[ $rc != 0 && -z $out && $err == *SB4074* && $err == *"empty command name"* ]]; chk qhead.12

# qhead.13-21 DG, 2026-10-11: judge every *quoted constant* head here, not
# only a wholly single-quoted one.
#
# Six shapes used to fall through to `validate`'s general SB5011, whose text
# explains the rejection as "a variable or expansion can't be resolved at
# compile time" — a cause none of them has. `bad_command_head_diag`'s own doc
# comment names that exact fault and special-cases it for `a-b=1`; these were
# the same fault, unfixed. The verdicts never changed (all were already
# rejected); what changed is that the reason given is now true of the input.
#
# qhead.13-14 `""` is an empty name, the same as `''` — the quote character
# was never the point. It used to get SB5011 purely because it took a
# different path through the parser.
run_sash '"" hi'
[[ $rc != 0 && -z $out && $err == *SB4074* && $err != *SB5011* ]]; chk qhead.13
run_sash 'if y=$("") { /bin/echo hi }'
[[ $rc != 0 && -z $out && $err == *SB4074* && $err != *SB5011* ]]; chk qhead.14

# qhead.15-18 a constant assembled from quoted pieces, or quoted with the
# wrong character, is still a constant with a bare spelling. All four name
# `echo` or `/bin/echo` and all four get SB5096's "write it bare". qhead.17 is
# the one that shows this was never only a double-quote problem: a *single*
# quoted piece inside a concatenation fell through too, because the old check
# matched a head that was wholly `SingleQuoted`.
run_sash '"ec"ho hi'
[[ $rc != 0 && -z $out && $err == *SB5096* && $err == *"without quotes"* ]]; chk qhead.15
run_sash 'ec"ho" hi'
[[ $rc != 0 && -z $out && $err == *SB5096* ]]; chk qhead.16
run_sash "'ec'ho hi"
[[ $rc != 0 && -z $out && $err == *SB5096* ]]; chk qhead.17
run_sash '"/bin/echo" hi'
[[ $rc != 0 && -z $out && $err == *SB5096* ]]; chk qhead.18

# qhead.19 a constant that genuinely needs quoting, said the wrong way. Same
# SB5096 — one rule ("one bare word, or one single-quoted word when it has no
# bare spelling") deserves one code — but the advice is the opposite one.
run_sash '"/qh dir/qh tool" hi'
[[ $rc != 0 && -z $out && $err == *SB5096* && $err == *"single-quoted word"* ]]; chk qhead.19

# qhead.20 the load-bearing non-capture. `a-b=1` is a `Concat` of *bare*
# literals, so a constancy test alone would capture it and replace its good
# diagnostic with a quoting one. The gate is the consumed source spelling: this
# family is about quoting, so an unquoted head is not its business.
run_sash 'a-b=1'
[[ $rc != 0 && -z $out && $err == *"is not a valid variable name"* && $err != *SB5096* ]]; chk qhead.20

# qhead.21 ...and the other side of it: a head that really does interpolate
# still gets SB5011, so that message's stated reason is true of every input
# that now receives it.
run_sash 'str x="echo"
"$x" hi'
[[ $rc != 0 && -z $out && $err == *SB5011* && $err == *"variable or expansion"* ]]; chk qhead.21

# Compiler-owned spellings are not external names needing quotes.
run_sash "if '[[' { echo accepted }"
[[ $rc != 0 && -z $out && $err == *SB5096* ]]; chk qhead.22
run_sash 'if "[[" { echo accepted }'
[[ $rc != 0 && -z $out && $err == *SB5096* ]]; chk qhead.23
run_sash "if '[''[' { echo accepted }"
[[ $rc != 0 && -z $out && $err == *SB5096* ]]; chk qhead.24
run_sash "- '[['"
[[ $rc != 0 && -z $out && $err == *SB5096* ]]; chk qhead.25
run_sash "str x=\$('[['); echo \$x"
[[ $rc != 0 && -z $out && $err == *SB5096* ]]; chk qhead.26
run_sash "f() { if '[[' { echo accepted } }; f"
[[ $rc != 0 && -z $out && $err == *SB5096* ]]; chk qhead.27
run_sash "if \$'[[' { echo accepted }"
[[ $rc != 0 && -z $out && $err == *SB5096* ]]; chk qhead.28

# ANSI-C decoding must not erase quote provenance at the head.
run_sash "\$'\\x65cho' hi"
[[ $rc != 0 && -z $out && $err == *SB5096* ]]; chk qhead.29
run_sash "\$''"
[[ $rc != 0 && -z $out && $err == *SB4074* && $err == *"empty command name"* ]]; chk qhead.30
run_sash "ec\$'ho' hi"
[[ $rc != 0 && -z $out && $err == *SB5096* ]]; chk qhead.31
run_sash "\$'$PWD/qh_dir/qh tool'"
[[ $rc = 0 && $out = spaced && -z $rerr ]]; chk qhead.32

# Lookahead may scan this comment, but its quotes do not belong to the head.
run_sash 'echo ok # "not a quoted head"'
[[ $rc = 0 && $out = ok && -z $rerr ]]; chk qhead.33

# These parser fallthroughs are reachable, not invariant failures.
run_sash '-'
[[ $rc != 0 && -z $out && $err == *SB5011* \
   && $err == *"command '-' not found"* ]]; chk pcold.1

run_sash 'for x'
[[ $rc != 0 && -z $out && $err == *SB4014* \
   && $err == *'expected a `{ }` body or `do`'* ]]; chk pcold.2

# A quoted segment in a brace-less default still owes its own closer.
run_sash "echo \$x:-\$(echo 'broken)"
[[ $rc != 0 && -z $out && $err == *':1:17: error [SB4057]'* \
   && $err == *"unterminated \`'\` string"* ]]; chk lexcold.2

# Multiple bad C-escapes preserve the first escape and its source position.
run_sash "echo \$'\\101\\cA'"
[[ $rc != 0 && -z $out && $err == *':1:8: error [SB4063]'* \
   && $err == *'octal escapes are not supported'* ]]; chk lexcold.3

# Outer missing closers must not replace the inner arithmetic opener.
run_sash 'echo "$(echo "$((1"'
[[ $rc != 0 && -z $out && $err == *':1:15: error [SB4057]'* \
   && $err == *'unterminated `$((`'* ]]; chk lexcold.4

run_sash "echo \"\$(echo \$'\\101\\cA')\""
[[ $rc != 0 && -z $out && $err == *':1:16: error [SB4063]'* \
   && $err == *'octal escapes are not supported'* ]]; chk lexcold.5

# supval.1-supval.22 were written when `-` reached into a call's body. On a
# user function `-` now discards only the value the function returns (DD#55
# 2026-10-29), so on a void function, or on a capture that binds an `-> int`
# function's int, it has nothing to discard: SB4076. The ids that spelled one
# of those now pin the rejection; the rest keep their programs.
run_sash 'fn speak { if (( 1 )) { /bin/false }; echo hello }
- speak
echo after'
[[ $rc != 0 && -z $out && $err == *':2:1: error [SB4076]'* \
   && $err == *"function 'speak' returns nothing, so \`-\` has nothing to discard"* ]]; chk supval.1

SASH_CFLAGS=--no-taint run_sash 'fn speak { echo hello; /bin/false }
str x=$(- speak)
echo "[$x]"'
[[ $rc != 0 && -z $out && $err == *':2:7: error [SB4076]'* \
   && $err == *"function 'speak' returns nothing"* ]]; chk supval.2

SASH_CFLAGS=--no-taint run_sash 'fn speak { echo hello; /bin/false }
echo "[$(- speak)]"'
[[ $rc != 0 && -z $out && $err == *':2:6: error [SB4076]'* \
   && $err == *"function 'speak' returns nothing"* ]]; chk supval.3

run_sash 'fn count -> int { /bin/false; return 7 }
int n=$(- count)
echo "$n"'
[[ $rc != 0 && -z $out && $err == *':2:7: error [SB4076]'* \
   && $err == *"\`\$(count)\` binds the int that function 'count' returns"* \
   && $err == *'capture it without `-`'* ]]; chk supval.4

run_sash 'fn count -> int { /bin/false; return 7 }
int n=$(- count)
echo $((n + 1))'
[[ $rc != 0 && -z $out && $err == *':2:7: error [SB4076]'* \
   && $err == *"binds the int that function 'count' returns"* ]]; chk supval.5

SASH_CFLAGS=--no-taint run_sash 'fn fail { echo token; /bin/false }
fn use { echo "$1" }
str x=$(- use $(fail))
echo "[$x]"'
[[ $rc != 0 && -z $out && $err == *':3:7: error [SB4076]'* \
   && $err == *"function 'use' returns nothing"* ]]; chk supval.6

# `-` discards only its own command's status, never that of a capture lifted
# from the command's operands (DD#55 2026-10-30), so fail's /bin/false aborts.
# supval.7, .8 and .18-.21 printed fail's output before that.
# Bypass taint only to reach these status/lowering checks (DD#127).
SASH_CFLAGS=--no-taint run_sash 'fn fail { echo token; /bin/false }
str x=$(- toupper $(fail))
echo "[$x]"'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk supval.7

SASH_CFLAGS=--no-taint run_sash 'fn fail { echo token; /bin/false }
str x=$(- echo $(fail))
echo "[$x]"'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk supval.8

run_sash 'fn descend -> int {
int n=$(($1))
if (( n == 0 )) { /bin/false; return 7 }
int next=$((n - 1))
return $(- descend $next)
}
echo $(descend 2)'
[[ $rc != 0 && -z $out && $err == *':5:8: error [SB4076]'* \
   && $err == *"\`\$(descend …)\` binds the int that function 'descend' returns"* ]]; chk supval.9

# These four pinned the old suppressed self-tail loop, which no program can
# reach now: each spelling of it is SB4076.
run_sash 'fn descend -> int {
int n=$(($1))
/bin/false
if (( n == 0 )) { return 7 }
int next=$((n - 1))
return $(- descend $next)
}
echo $(descend 2)'
[[ $rc != 0 && -z $out && $err == *':6:8: error [SB4076]'* \
   && $err == *"binds the int that function 'descend' returns"* ]]; chk supval.10

run_sash 'fn descend -> int {
int n=$(($1))
if (( n == 0 )) { /bin/false; return 7 }
int next=$((n - 1))
return $(- descend $next)
}
echo $(- descend 10000)'
[[ $rc != 0 && -z $out && $err == *':5:8: error [SB4076]'* \
   && $err == *':7:6: error [SB4076]'* ]]; chk supval.11

run_sash 'fn descend -> int {
int n=$(($1))
if (( n == 1 )) { /bin/false }
if (( n == 0 )) { return 7 }
int next=$((n - 1))
return $(- descend $next)
}
echo $(descend 2)'
[[ $rc != 0 && -z $out && $err == *':6:8: error [SB4076]'* \
   && $err == *"binds the int that function 'descend' returns"* ]]; chk supval.12

run_sash 'fn descend -> int {
int n=$(($1))
if (( n == 0 )) { /bin/false; return 7 }
int next=$((n - 1))
return $(descend $next)
}
echo $(descend 2)'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'command failed (exit 1)'* ]]; chk supval.13

SASH_CFLAGS=--no-taint run_sash 'fn zero { echo 0; /bin/false }
fn descend -> int {
int n=$(($1))
if (( n == 0 )) { return 7 }
return $(- descend $(zero))
}
echo $(descend 1)'
[[ $rc != 0 && -z $out && $err == *':5:8: error [SB4076]'* \
   && $err == *"binds the int that function 'descend' returns"* ]]; chk supval.14

# A later prefix must not uncheck blocks emitted by an earlier capture. `safe`
# returns a status, so `- safe` is legal and discards it.
run_sash 'fn count -> int { if (( 1 )) { /bin/false }; return 7 }
fn safe { echo hello; return 1 }
int n=$(count)
- safe
echo "$n"'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'command failed (exit 1)'* ]]; chk supval.15

SASH_CFLAGS=--no-taint run_sash 'fn speak { if (( 1 )) { /bin/false }; echo token }
fn safe { echo hello; return 1 }
str x=$(speak)
- safe
echo "$x"'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'command failed (exit 1)'* ]]; chk supval.16

run_sash 'fn speak { echo hello; /bin/false }
- speak
/bin/false
echo after'
[[ $rc != 0 && -z $out && $err == *':2:1: error [SB4076]'* \
   && $err == *"function 'speak' returns nothing"* ]]; chk supval.17

SASH_CFLAGS=--no-taint run_sash 'fn fail { echo token; /bin/false }
str x=$(- basename $(fail))
echo "$x"'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk supval.18

SASH_CFLAGS=--no-taint run_sash 'fn fail { echo token; /bin/false }
int n=$(- strlen $(fail))
echo "$n"'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk supval.19

SASH_CFLAGS=--no-taint run_sash 'fn fail { echo token; /bin/false }
str x=$(- /bin/echo $(fail))
echo "[$x]"'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk supval.20

SASH_CFLAGS=--no-taint run_sash 'fn fail { if (( 1 )) { /bin/false }; echo token }
- /bin/echo $(fail)
echo after'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk supval.21

SASH_CFLAGS=--no-taint run_sash 'fn speak { if (( 1 )) { /bin/false }; echo token }
str x=$(- speak 2>&1)
echo "[$x]"'
[[ $rc != 0 && -z $out && $err == *':2:9: error [SB4076]'* \
   && $err == *"function 'speak' returns nothing"* ]]; chk supval.22

# Prefix peeling must retain the sole-value position and structural gates.
run_sash 'echo $(- strlen)'
[[ $rc != 0 && -z $out && $err == *SB4045* && $err == *'expects 1 argument but got 0'* ]]; chk supval.23

run_sash '- toupper token'
[[ $rc != 0 && -z $out && $err == *SB5073* && $err == *'A bare statement would discard the value'* ]]; chk supval.24

run_sash 'str x=$(- toupper token; echo trailing); echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB5073* && $err == *'not one statement among several'* ]]; chk supval.25

run_sash 'fn speak { echo token }
if x=$(- speak) { echo "$x" }'
[[ $rc != 0 && -z $out && $err == *SB4025* && $err == *"capturing a function"* ]]; chk supval.26

run_sash 'fn count -> int { if (( 1 )) { return 0 }; return 7 }
int n=$(count; echo token)
echo "$n"'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *'string'* ]]; chk supval.27

run_sash 'fn safe { echo hello; return 1 }
int choose=1
if (( choose )) {
if (( choose )) { /bin/false }
} else { - safe }
echo after'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'command failed (exit 1)'* ]]; chk supval.28

SASH_CFLAGS=--no-taint run_sash 'echo $(- getpid extra)'
[[ $rc != 0 && -z $out && $err == *SB4045* && $err == *'expects 0 arguments but got 1'* ]]; chk supval.29

SASH_CFLAGS=--no-taint run_sash 'echo $(- wait 1 2)'
[[ $rc != 0 && -z $out && $err == *SB4045* && $err == *'got 2'* ]]; chk supval.30

run_sash 'echo $(- status)'
[[ $rc != 0 && -z $out && $err == *SB5012* ]]; chk supval.31

run_sash 'if /bin/false { : } else { echo $(- status) }'
[[ $rc = 0 && $out = 1 && -z $err && -z $rerr ]]; chk supval.32

run_sash '- /bin/false; echo $(- status)'
[[ $rc != 0 && -z $out && $err == *SB5012* ]]; chk supval.33

SASH_SPEC_SUPVAL_PRESENT=token run_sash 'if x=$(- getenv SASH_SPEC_SUPVAL_PRESENT) {
attest x [[ -n $x ]]
echo "$x"
} else { echo missing }'
[[ $rc = 0 && $out = token && -z $err && -z $rerr ]]; chk supval.34

unset SASH_SPEC_SUPVAL_ABSENT
run_sash 'if x=$(- getenv SASH_SPEC_SUPVAL_ABSENT) {
attest x [[ -n $x ]]
echo "$x"
} else { echo missing }'
[[ $rc = 0 && $out = missing && -z $err && -z $rerr ]]; chk supval.35

# nonvoid.1-nonvoid.24: a non-void function's answer is handled or discarded
# (batch-531 B15; DD#55 2026-10-29 and 2026-10-30). A status function's call
# is handled by a condition or `||`, or discarded with `-`; as a bare
# statement, a list tail, a background job or a capture it is SB4016,
# wherever it appears. A function whose only exits are `return 0`, a bare
# `return`, falling off the end or a self-tail call is void, so `-` on it is
# SB4076. `return g` takes g's class, and the last definition of a name
# decides it, in any textual order.
run_sash 'fn f { return 3 }
f
echo after'
[[ $rc != 0 && -z $out && $err == *':2:1: error [SB4016]'* \
   && $err == *"function 'f' returns a status — handle it with"* ]]; chk nonvoid.1

# the tail of a list is not handled: its status is the statement's
run_sash 'fn f { return 3 }
/bin/true && f
echo after'
[[ $rc != 0 && -z $out && $err == *':2:14: error [SB4016]'* ]]; chk nonvoid.2

# reading `$?` afterwards does not handle it
run_sash 'fn f { return 3 }
f
echo "st $?"'
[[ $rc != 0 && -z $out && $err == *':2:1: error [SB4016]'* ]]; chk nonvoid.3

# inside a function body (g itself falls off the end, so it is void)
run_sash 'fn f { return 3 }
fn g { f }
g
echo after'
[[ $rc != 0 && -z $out && $err == *':2:8: error [SB4016]'* \
   && $err != *':3:1:'* ]]; chk nonvoid.4

run_sash 'fn s { return 3 }
END { s }
echo after'
[[ $rc != 0 && -z $out && $err == *':2:7: error [SB4016]'* ]]; chk nonvoid.5

run_sash 'fn f { return 3 }
f &
- wait
echo after'
[[ $rc != 0 && -z $out && $err == *':2:1: error [SB4016]'* ]]; chk nonvoid.6

# a tail call is status-transparent: b returns a's status
run_sash 'fn a { return 3 }
fn b { return a }
b || echo caught
b
echo after'
[[ $rc != 0 && -z $out && $err == *':4:1: error [SB4016]'* \
   && $err == *"function 'b' returns a status"* && $err != *':3:1:'* ]]; chk nonvoid.7

# the definition in force decides the class
run_sash 'fn f { echo one }
fn f { return 3 }
f
echo after'
[[ $rc != 0 && -z $out && $err == *':3:1: error [SB4016]'* ]]; chk nonvoid.8

# b is defined after the body that calls it
run_sash 'fn a { b }
fn b -> int { return 7 }
echo after'
[[ $rc != 0 && -z $out && $err == *':1:8: error [SB4016]'* \
   && $err == *"function 'b' returns an int"* ]]; chk nonvoid.9

# a capture binds the output, so the status has nowhere to go
run_sash 'fn f { echo out; return 3 }
x=$(f)
echo "[$x]"'
[[ $rc != 0 && -z $out && $err == *':2:3: error [SB4016]'* \
   && $err == *"which a capture cannot handle — discard it with \`\$(- f)\`"* ]]; chk nonvoid.10

run_sash 'fn f { echo "$1 $2"; return 3 }
x=$(f one two)
echo "[$x]"'
[[ $rc != 0 && -z $out && $err == *':2:3: error [SB4016]'* \
   && $err == *"discard it with \`\$(- f …)\`"* ]]; chk nonvoid.11

# a statement's `-` does not reach a capture among its arguments
run_sash 'fn f { echo out; return 3 }
- /bin/echo $(f)
echo after'
[[ $rc != 0 && -z $out && $err == *':2:13: error [SB4016]'* ]]; chk nonvoid.12

run_sash 'fn f { echo out; return 3 }
if [[ $(f) == out ]] { echo eq }
echo after'
[[ $rc != 0 && -z $out && $err == *':2:7: error [SB4016]'* ]]; chk nonvoid.13

# a redirected capture body is a statement, and gets the statement's error
SASH_CFLAGS=--no-taint run_sash 'fn s { echo x; return 3 }
str y=$(s 2>&1)
echo "[$y]"'
[[ $rc != 0 && -z $out && $err == *':2:9: error [SB4016]'* ]]; chk nonvoid.14

# a function capture as a condition is SB4025, which supersedes SB4016
run_sash 'fn f { echo out; return 3 }
if x=$(f) { echo "yes [$x]" } else { echo "no [$x]" }'
[[ $rc != 0 && -z $out && $err == *':2:6: error [SB4025]'* \
   && $err != *SB4016* ]]; chk nonvoid.15

# `return g` with a void g returns only 0
run_sash 'fn g { echo g }
fn f -> int { return g }
int n=$(( f ))
echo "[$n]"'
[[ $rc != 0 && -z $out && $err == *':2:1: error [SB4017]'* \
   && $err == *"function 'f' only returns 0"* ]]; chk nonvoid.16

# ... and makes a status function void
run_sash 'fn d { echo d }
fn c { return d }
- c
echo after'
[[ $rc != 0 && -z $out && $err == *':3:1: error [SB4076]'* \
   && $err == *"function 'c' returns nothing"* ]]; chk nonvoid.17

SASH_CFLAGS=--no-taint run_sash 'fn v { echo x }
str y=$(- v 2>&1)
echo "[$y]"'
[[ $rc != 0 && -z $out && $err == *':2:9: error [SB4076]'* \
   && $err == *"function 'v' returns nothing"* ]]; chk nonvoid.18

# nonvoid.19-nonvoid.24 are accepted.
run_sash 'fn f { return 3 }
fn f { echo two }
f
echo after'
[[ $rc = 0 && $out = $'two\nafter' && -z $err && -z $rerr ]]; chk nonvoid.19

# `return 0` on one path and an int call on the other is an int function
run_sash 'str v=y
fn g -> int { return 7 }
fn f -> int {
    if [[ $v == x ]] { return 0 }
    return g
}
int n=$(( f ))
echo "after $n"'
[[ $rc = 0 && $out = 'after 7' && -z $err && -z $rerr ]]; chk nonvoid.20

run_sash 'fn f { return 3 }
f && echo yes
f || echo caught
echo after'
[[ $rc = 0 && $out = $'caught\nafter' && -z $err && -z $rerr ]]; chk nonvoid.21

run_sash 'fn f { echo out; return 3 }
x=$(- f)
echo "[$x]"'
[[ $rc = 0 && $out = '[out]' && -z $err && -z $rerr ]]; chk nonvoid.22

run_sash 'fn s { echo x; return 3 }
str y=$(- s 2>&1)
echo "[${y:!}]"'
[[ $rc = 0 && $out = '[x]' && -z $err && -z $rerr ]]; chk nonvoid.23

# a stage or a job is a process, whose own status `-` discards, void or not
run_sash 'fn v { echo v }
fn s { echo s; return 3 }
- v &
- wait
- s &
- wait
- v | /bin/cat
- s | /bin/cat
echo after'
[[ $rc = 0 && $out = $'v\ns\nv\ns\nafter' && -z $err && -z $rerr ]]; chk nonvoid.24

# nonvoid.25-nonvoid.29: a call is checked against the definition it runs,
# the one in force where it stands, not a later redefinition.
run_sash 'fn f -> int { return 3 }
f
fn f { echo two }
echo after'
[[ $rc != 0 && -z $out && $err == *':2:1: error [SB4016]'* \
   && $err == *"function 'f' returns an int"* ]]; chk nonvoid.25

# g's body runs the int f, so its bare call is the error
run_sash 'fn f -> int { return 3 }
fn g { f }
g
fn f { echo two }
echo after'
[[ $rc != 0 && -z $out && $err == *':2:8: error [SB4016]'* \
   && $err == *"function 'f' returns an int"* ]]; chk nonvoid.26

run_sash 'fn f { return 3 }
f
fn f { echo two }
echo after'
[[ $rc != 0 && -z $out && $err == *':2:1: error [SB4016]'* \
   && $err == *"function 'f' returns a status"* ]]; chk nonvoid.27

run_sash 'fn f { echo one }
f
fn f -> int { return 3 }
echo after'
[[ $rc = 0 && $out = $'one\nafter' && -z $err && -z $rerr ]]; chk nonvoid.28

run_sash 'fn f { echo one }
f
fn f { return 3 }
echo after'
[[ $rc = 0 && $out = $'one\nafter' && -z $err && -z $rerr ]]; chk nonvoid.29

# noreach.1-noreach.17: `-` discards only the status of its own command or
# call (DD#55 2026-10-30). It does not reach into a user function's body,
# which is checked like any other body, or into a capture or call lifted from
# the command's operands, so each /bin/false below aborts; every one of them
# but noreach.17 printed and carried on before that. noreach.2 is the
# control: the status that f returns is still discarded. An external's output
# is tainted, so the ids that capture one directly compile with --no-taint.

# noreach.1 the called function's body
run_sash 'fn f { /bin/false; echo x; return 1 }
- f
echo done'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk noreach.1

# noreach.2 control: the returned status is what `-` discards
run_sash 'fn f { echo x; return 1 }
- f
echo done'
[[ $rc = 0 && $out = $'x\ndone' && -z $err && -z $rerr ]]; chk noreach.2

# noreach.3 an int function's body
run_sash 'fn f -> int { /bin/false; return 7 }
- f
echo done'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk noreach.3

# noreach.4 a background job's body: the child aborts, and waitstatus says so
run_sash 'fn f { echo x; /bin/false; echo y; return 1 }
- f &
- wait
int s=$(waitstatus)
echo "s=$s"'
[[ $rc = 0 && $out = $'x\ns=1' && -z $err && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk noreach.4

# noreach.5 a suppressed capture of a status function
run_sash 'fn f { /bin/false; echo tok; return 1 }
str x=$(- f)
echo "[${x:!}]"'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk noreach.5

# noreach.6 a function stage of a capture condition: the stage fails, so the
# condition is false
SASH_CFLAGS=--no-taint run_sash 'fn f { /bin/false; echo inf }
if x=$(f | /bin/cat) { echo "yes [$x]" } else { echo no }
echo done'
[[ $rc = 0 && $out = $'no\ndone' && -z $err && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk noreach.6

# noreach.7 an external command's operand capture
SASH_CFLAGS=--no-taint run_sash '- /bin/echo "$(/bin/false)"
echo done'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk noreach.7

# noreach.8 a builtin's operand capture
SASH_CFLAGS=--no-taint run_sash '- echo "$(/bin/false)"
echo done'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk noreach.8

# noreach.9 an operand capture of a suppressed capture's command
SASH_CFLAGS=--no-taint run_sash 'str x=$(- /bin/echo "$(/bin/false)")
echo "[$x] done"'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk noreach.9

# noreach.10 a function captured as an operand
SASH_CFLAGS=--no-taint run_sash 'fn g { /bin/false; echo x }
- echo $(g)
echo done'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk noreach.10

# noreach.11 a `[[ ]]` operand capture
SASH_CFLAGS=--no-taint run_sash '- [[ $(/bin/false) == x ]]
echo after'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk noreach.11

# noreach.12 a value intrinsic's argument capture
SASH_CFLAGS=--no-taint run_sash 'int n=$(- strlen $(/bin/false))
echo "$n"'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk noreach.12

# noreach.13 a stage's operand capture in a capture condition
SASH_CFLAGS=--no-taint run_sash 'if x=$(/bin/echo $(/bin/false) | /bin/cat) { echo "yes [$x]" } else { echo no }
echo done'
[[ $rc = 0 && $out = $'no\ndone' && -z $err && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk noreach.13

# noreach.14 a capture in an arithmetic test's operand
SASH_CFLAGS=--no-taint run_sash 'map m
m[k]=1
- (( ${m[$(/bin/false)]:-0} ))
echo after'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk noreach.14

# noreach.15 an int call assigned inside an arithmetic test
run_sash 'fn h -> int { /bin/false; return 3 }
int n=0
- (( n = (h) ))
echo "after $n"'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk noreach.15

# noreach.16 an int call as an arithmetic test's operand
run_sash 'fn h -> int { /bin/false; return 3 }
- (( (h) ))
echo after'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk noreach.16

# noreach.17 a capture condition's operand capture, lifted out of a single
# command (batch-531 B76's shape): the condition consumes /bin/echo's status
# only. Unlike noreach.13's stage, the capture runs in the main process. A
# control: the port's base (1309385) aborted here too, while the batch-531
# branch's hunks H234/H235 (dropped) printed "yes [F]" and carried on.
SASH_CFLAGS=--no-taint run_sash 'fn f { /bin/false; echo F }
if x=$(/bin/echo $(f)) { echo "yes [$x]" } else { echo no }
echo done'
[[ $rc = 1 && -z $out && -z $err && $rerr == *'/bin/false: command failed (exit 1)'* ]]; chk noreach.17

# 55.145-55.147: a `-` over a `||` list keeps the right operand's status
# (batch-531 B84). The right operand runs unchecked under `-`, and `||`'s
# top-level reset to 0 assumed a completed right side had passed its check, so
# 55.145 and 55.146 exited 0. Suppression consumes the status the way a
# condition does, so the reset no longer applies: the program exits with the
# right side's status, as a lone `- /bin/false` does. 55.147 is the control.
#
# 55.145 an external right side that fails
run_sash '- /bin/false || /bin/false'
[[ $rc = 1 && -z $out && -z $err && -z $rerr ]]; chk 55.145

# 55.146 a status function's returned 1 as the right side
run_sash 'fn f { /bin/echo in; return 1 }
- /bin/false || f'
[[ $rc = 1 && $out = in && -z $err && -z $rerr ]]; chk 55.146

# 55.147 a right side that succeeds still exits 0
run_sash '- /bin/false || /bin/true'
[[ $rc = 0 && -z $out && -z $err && -z $rerr ]]; chk 55.147

# relex.1-relex.11: text re-parsed out of a quoted capture, a subscript index
# or a sed field reports its real source position (batch-531 B3: each re-parse
# used to number from the start of its own substring), and a fatal its
# sub-scanner raises is reported rather than dropped (B61). The lexer's
# `Origin` carries the substring's position into the sub-scanner; see the
# DD#114 and DD#115 addenda. Each faulty construct sits below line 1, where a
# substring-relative position cannot coincide with the real one.

# relex.1 a quoted capture: `$HOME` is at 3:18, not the substring's 1:6
run_sash 'echo start
echo start
   echo "[$(echo $HOME)]"'
[[ $rc != 0 && -z $out && $err == *':3:18: error [SB5018]'* ]]; chk relex.1

# relex.2 the capture spans a line break: its second line is source line 3
run_sash 'echo start
echo "[$(echo a
  echo $HOME)]"'
[[ $rc != 0 && -z $out && $err == *':3:8: error [SB5018]'* ]]; chk relex.2

# relex.3 a capture inside a subscript index, which is re-parsed on its own
run_sash 'map m=([k]=v)
echo "${m[k]}"
echo "${m[$(nosuchcmd)]}"'
[[ $rc != 0 && -z $out && $err == *':3:13: error [SB5011]'* ]]; chk relex.3

# relex.4 the same, with the index's capture spanning a line break
run_sash 'map m=([k]=v)
echo "${m[$(echo k
  nosuchcmd)]}"'
[[ $rc != 0 && -z $out && $err == *':3:3: error [SB5011]'* ]]; chk relex.4

# relex.5 arithmetic one level inside an index's capture reports its own `$((`
# (the index expression itself keeps DD#114's re-anchor to its `${`)
run_sash 'map m=([k]=v)
echo "${m[k]}"
echo "${m[$(echo $((j+1)))]}"'
[[ $rc != 0 && -z $out && $err == *':3:18: error [SB5031]'* ]]; chk relex.5

# relex.6 a capture inside a sed field of an expansion
run_sash 'str x=abc
echo start
   echo "${x:s/$(echo $HOME)/b/}"'
[[ $rc != 0 && -z $out && $err == *':3:23: error [SB5018]'* ]]; chk relex.6

# relex.7 an unsupported `$'…'` escape in a sed field is SB4063 (DD#115); the
# dropped fatal used to compile it, printing `a101c`
run_sash "str x=abc
echo \"\${x:s/b/\$'\\101'/}\""
[[ $rc != 0 && -z $out && $err == *':2:17: error [SB4063]'* \
   && $err == *'octal escapes are not supported'* ]]; chk relex.7

# relex.8 an unterminated `$(` in a sed field; it used to print `babc`
run_sash 'x=abc
echo "${x:s/$(echo/b/}"'
[[ $rc != 0 && -z $out && $err == *':2:13: error [SB4057]'* \
   && $err == *'unterminated `$(`'* ]]; chk relex.8

# relex.9 a `#` glued to an operator inside a quoted capture is SB4058, not
# the SB5011 its swallowed rest of line used to produce
run_sash 'echo start
echo "$(echo a;# c)"'
[[ $rc != 0 && -z $out && $err == *':2:16: error [SB4058]'* \
   && $err != *SB5011* ]]; chk relex.9

# relex.10 an unsupported escape in a subscript index (it was SB4030)
run_sash "map m=([A]=x [101]=y)
echo \"\${m[\$'\\101']}\""
[[ $rc != 0 && -z $out && $err == *':2:13: error [SB4063]'* \
   && $err != *SB4030* ]]; chk relex.10

# relex.11 the statement form of a sed edit re-parses its fields the same way
run_sash "str x=abc
x =~ s/b/\$'\\101'/
echo \$x"
[[ $rc != 0 && -z $out && $err == *':2:12: error [SB4063]'* ]]; chk relex.11

# relex.12-relex.21: each spec pins one more place that hands a re-parsed
# substring its position, so that reverting any one of them turns a spec red.
# The ones above already pin the rest.

# relex.12 a brace-less cell read inside a string: its index starts after `$m`
run_sash 'map m=([k]=v)
echo "$m[k]"
echo "$m[$(nosuchcmd)]"'
[[ $rc != 0 && -z $out && $err == *':3:12: error [SB5011]'* ]]; chk relex.12

# relex.13 an unquoted `${…}`: its content starts two bytes past the `$`
run_sash 'map m=([k]=v)
echo "${m[k]}"
echo ${m[$(nosuchcmd)]}'
[[ $rc != 0 && -z $out && $err == *':3:12: error [SB5011]'* ]]; chk relex.13

# relex.14 an unquoted brace-less cell read: its content starts one byte past
run_sash 'map m=([k]=v)
echo "${m[k]}"
echo $m[$(nosuchcmd)]'
[[ $rc != 0 && -z $out && $err == *':3:11: error [SB5011]'* ]]; chk relex.14

# relex.15 a subscript on the positional splat: its index's own fatal is
# reported where it is, ahead of SB5031 for the subscript itself
run_sash "echo start
echo \"\${@[\$'\\101']}\""
[[ $rc != 0 && -z $out && $err == *':2:13: error [SB4063]'* ]]; chk relex.15

# relex.16 a sed edit on a cell: the RE starts after `]:s/`
run_sash 'map m=([k]=v)
echo start
echo "${m[k]:s/$(echo $HOME)/b/}"'
[[ $rc != 0 && -z $out && $err == *':3:23: error [SB5018]'* ]]; chk relex.16

# relex.17 blanks before an index are skipped, not counted out of it
run_sash 'map m=([k]=v)
echo "${m[k]}"
echo "${m[ $(nosuchcmd)]}"'
[[ $rc != 0 && -z $out && $err == *':3:14: error [SB5011]'* ]]; chk relex.17

# relex.18 the second edit of a chain starts after the first edit and its `,`
run_sash 'str x=abc
echo start
echo "${x:s/a/b/,s/$(echo $HOME)/c/}"'
[[ $rc != 0 && -z $out && $err == *':3:27: error [SB5018]'* ]]; chk relex.18

# relex.19 an expansion inside `$(( ))` reports the arithmetic's own position:
# the arithmetic lexer rebuilds the expansion's text, so nothing inside it
# maps back to a column (the `Within` origin)
run_sash 'str x=5
echo start
int n=$(( ${x:s/5/$(nosuchcmd)/} + 1 ))
echo $n'
[[ $rc != 0 && -z $out && $err == *':3:7: error [SB5011]'* ]]; chk relex.19

# relex.20 a dynamic index on an assignment's left-hand side
run_sash 'array a=(1 2)
echo start
a[$(nosuchcmd)]=x
echo "${a[0]}"'
[[ $rc != 0 && -z $out && $err == *':3:5: error [SB5011]'* ]]; chk relex.20

# relex.21 the statement form of a chain continued after its `,` onto the next
# line (DD#96): the second edit is on source line 3
run_sash 'str x=abc
x =~ s/a/b/,
  s/$(echo $HOME)/c/
echo $x'
[[ $rc != 0 && -z $out && $err == *':3:12: error [SB5018]'* ]]; chk relex.21

# tpos.1-tpos.5: a diagnostic raised while lowering a statement (call arity,
# recursion, a splat into a fixed-arity function) reports that statement's
# position, not 1:1 (batch-531 B54).

# tpos.1 an arity error inside a function body
run_sash 'fn f { echo hi }
fn g { f a }
g
g'
[[ $rc != 0 && -z $out && $err == *':2:8: error [SB5043]'* \
   && $err == *"function 'f' takes exactly 0 arguments but was called with 1"* ]]; chk tpos.1

# tpos.2 recursion that is not a self tail call
run_sash 'fn f {
  echo hi
  f
}
f'
[[ $rc != 0 && -z $out && $err == *':3:3: error [SB5037]'* \
   && $err == *"recursive call to 'f'"* ]]; chk tpos.2

# tpos.3 too few arguments to a captured call
run_sash 'fn f { echo $1 }
echo start
str x=$(f)
echo "$x"'
[[ $rc != 0 && -z $out && $err == *':3:1: error [SB5042]'* \
   && $err == *"function 'f' expects 1 argument but was called with 0"* ]]; chk tpos.3

# tpos.4 a splat into a function of fixed arity
run_sash 'fn f {
  echo "$1"
}
array a=(x y)
echo start
f @a'
[[ $rc != 0 && -z $out && $err == *':6:1: error [SB5087]'* ]]; chk tpos.4

# tpos.5 a call in an `elif` condition
run_sash 'fn f { return /bin/true }
int n=2
if (( n == 1 )) {
  echo a
} elif f x {
  echo b
}
echo done'
[[ $rc != 0 && -z $out && $err == *':5:8: error [SB5043]'* ]]; chk tpos.5

set -e
tally
