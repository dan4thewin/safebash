#!/bin/bash
# DD#56 — signals and END block
. common.sh
plan=77
setup signals

set +e

# --- END block ---

# 56.1 END block executes at exit
run_sash 'END {
  echo bye
}
echo hello'
[[ $rc = 0 && $out == *"hello"* && $out == *"bye"* && -z $err ]]; chk 56.1

# 56.2a reaching end of END terminates
run_sash 'END {
  echo done
}
echo main'
[[ $rc = 0 && $out == *"done"* && -z $err ]]; chk 56.2a

# 56.2b return in END terminates (no re-entry)
run_sash 'END {
  return
  echo unreachable
}
echo main'
[[ $rc = 0 && $out != *"unreachable"* && -z $err ]]; chk 56.2b

# 56.3 exit N in END overrides exit status
run_sash 'END {
  exit 42
}
/bin/false'
[[ $rc = 42 && -z $err ]]; chk 56.3

# 56.4 without exit N, preserves triggering status
run_sash 'END {
  echo cleanup
}
/bin/false'
[[ $rc != 0 && $out == *"cleanup"* && -z $err ]]; chk 56.4

# --- signal builtin ---

# 56.5 signal IGN masks signal
run_sash 'signal 2 IGN
/bin/kill -INT $$
echo survived'
[[ $rc = 0 && $out = "survived" && -z $err ]]; chk 56.5

# 56.6 signal DFL restores default
run_sash 'signal 2 IGN
signal 2 DFL
echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 56.6

# 56.7 signal FLAG records without check points
run_sash 'signal 10 FLAG
/bin/kill -USR1 $$
echo $(signum)'
[[ $rc = 0 && $out = "10" && -z $err ]]; chk 56.7

# 56.8 signal EXIT triggers END block
run_sash 'END {
  echo shutting_down
}
signal 15 EXIT
/bin/kill -TERM $$
echo should_not_reach'
[[ $rc != 0 && $out == *"shutting_down"* && $out != *"should_not_reach"* && -z $err ]]; chk 56.8

# 56.9 check points at external commands
run_sash 'END {
  echo caught
}
signal 2 EXIT
/bin/kill -INT $$
/bin/echo after_kill'
[[ $rc != 0 && $out == *"caught"* && $out != *"after_kill"* && -z $err ]]; chk 56.9

# 56.10 FLAG has zero overhead (no early exit)
run_sash 'signal 2 FLAG
/bin/kill -INT $$
echo still_here'
[[ $rc = 0 && $out = "still_here" && -z $err ]]; chk 56.10

# 56.11 $(signum) returns signal number
run_sash 'signal 12 FLAG
/bin/kill -USR2 $$
int s=$(signum)
echo $s'
[[ $rc = 0 && $out = "12" && -z $err ]]; chk 56.11

# 56.12a SIGHUP accepted
run_sash 'signal 1 IGN
echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 56.12a

# 56.12b SIGPIPE accepted
run_sash 'signal 13 IGN
echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 56.12b

# 56.13 signal takes numeric argument
run_sash 'signal 15 IGN
echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 56.13

# --- DD#56: `trap` is not a native form (SB4043) ---
# The native signal model is the `signal` builtin + `END { }`; `trap` is a bash
# spelling the converter rewrites (57.14 below). A hand-written `trap` in sash
# source is rejected, pointing at signal/END. (Validation reject — all modes.)

# 56.14 `trap … EXIT` (command handler) → SB4043
run_sash 'trap "echo cleanup" EXIT; echo hi'
[[ $rc != 0 && -z $out && $err == *SB4043* ]]; chk 56.14
# 56.15 `trap "" SIG` / `trap - SIG` (ignore/reset) → SB4043 too
run_sash 'trap "" SIGINT; echo hi'
[[ $rc != 0 && -z $out && $err == *SB4043* ]]; chk 56.15

# --- DD#56: `$?` in the END block is the TRIGGERING status (first action) ---
# The END block is entered because of a status — an abort's failing code, an
# `exit N`, or the last command's status on normal exit — so that status is
# available as its FIRST action, like a handling branch.

# 56.30 abort (failed command) → `$?` in END is the failing exit code
run_sash 'END { /bin/echo "s=$?" }
/bin/false'
[[ $rc != 0 && $out == *"s=1"* && -z $err ]]; chk 56.30

# 56.31 `exit N` → `$?` in END is N (the requested exit code). Both modes as of
# the c-backend fix: c's emit_exit went straight to the final `__exit` label and
# skipped END entirely; it now routes through the END block carrying the code,
# like native.
run_sash 'END { /bin/echo "s=$?" }
exit 5'
[[ $rc = 5 && $out == *"s=5"* && -z $err ]]; chk 56.31

# 56.34 DD#89: a bare `exit` is `exit 0`, NOT "keep the previous status". The c
# backend left __exit_status untouched, so a bare exit after a `-`-suppressed
# failure exited 1 there and 0 in native. END sees 0 too.
run_sash 'END { /bin/echo "s=$?" }
- /bin/false
exit'
[[ $rc = 0 && $out == *"s=0"* && -z $err ]]; chk 56.34

# 56.35 the same bare exit with no END block — the exit code itself is 0
run_sash '- /bin/false
exit'
[[ $rc = 0 && -z $out && -z $err ]]; chk 56.35

# 56.32 normal exit → `$?` in END is the last command's status (0 here)
run_sash 'END { /bin/echo "s=$?" }
/bin/echo hi'
[[ $rc = 0 && $out == *"s=0"* && -z $err ]]; chk 56.32

# 56.33 guard: the END-entry status does NOT leak into a regular function — a
# `$?` as the first action of an ordinary function is still SB5012 (all modes).
run_sash 'fn f { /bin/echo "$?" }
f'
[[ $rc != 0 && -z $out && $err == *SB5012* ]]; chk 56.33

# --- converter ---

# 57.14a trap "" SIG → signal SIG IGN
run_convert '#!/bin/bash
trap "" SIGINT'
[[ $rc = 0 && $out == *'signal 2 IGN'* && -z $err ]]; chk 57.14a

# 57.14b trap - SIG → signal N DFL
run_convert '#!/bin/bash
trap - SIGTERM'
[[ $rc = 0 && $out == *'signal 15 DFL'* && -z $err ]]; chk 57.14b

# --- END is a Program field, not a __sash_end__ function ---------------
# DD#56. `END { }` parses into `Program::end_block`; it used to be emitted as
# a function literally named `__sash_end__`, which leaked that name into user
# diagnostics, let a second END silently win, and left `:end-block nil` in the
# --emit-ir capture. Journal 309.

# 56.36 an END block is not a statement — it cannot be nested or made
# conditional. (It used to report SB5071 about `fn __sash_end__`.)
run_sash 'fn f {
END {
/bin/echo a
}
}
f'
[[ $rc != 0 && -z $out && $err == *SB4054* && $err != *__sash_end__* ]]; chk 56.36

# 56.37 a second END block is rejected. It used to be accepted, with the
# second silently winning and the first never running.
run_sash 'END {
/bin/echo a
}
END {
/bin/echo b
}'
[[ $rc != 0 && -z $out && $err == *SB4055* ]]; chk 56.37

# 56.88 an END body that never closes is a parse error, reported and recovered
# from rather than carried forward. `take_end_block`'s Err arm had never run:
# every END in the suite parses, so the one path that reports a malformed one
# was unexercised. It matters because the arm also decides what happens *next*
# — it recovers and keeps parsing rather than registering a half-built body, so
# a failure here would not be a missing message but a later pass walking an END
# block the parser never finished.
run_sash 'echo hi
END {
/bin/echo bye'
[[ $rc != 0 && -z $out && $err == *SB4014* ]]; chk 56.88

# 56.38 the END body is TOP-LEVEL scope, not a function body, so a positional
# splat is the top-level reject (SB5045) exactly as it is outside END. While
# END was encoded as a function, `$@` inside it was silently accepted as that
# function's arguments.
run_sash 'END {
/bin/echo $@
}'
[[ $rc != 0 && -z $out && $err == *SB5045* ]]; chk 56.38

# --- SB4059-SB4062: the shape of a `signal` declaration (DD#56) ---
# Every one of these used to compile clean and silently do nothing.
# `handle_signal` gated on `args.len() >= 2`, parsed the number with
# `unwrap_or(0)`, tested `signum > 0`, and matched the disposition with a bare
# `_ => {}`. `signal TERM IGN` -- the bash spelling, and the one the SB4043
# message itself all but invites -- was a no-op.

# 56.40 no arguments at all
run_sash 'signal'
[[ $rc != 0 && -z $out && $err == *SB4059* ]]; chk 56.40

# 56.41 a number but no disposition
run_sash 'signal 2'
[[ $rc != 0 && -z $out && $err == *SB4059* ]]; chk 56.41

# 56.42 a trailing argument is not quietly dropped
run_sash 'signal 2 IGN extra'
[[ $rc != 0 && -z $out && $err == *SB4059* ]]; chk 56.42

# 56.43 a signal NAME is not a signal number -- DD#56: the compiler does not
# know signal names, users reach for a GPP macro
run_sash 'signal TERM IGN'
[[ $rc != 0 && -z $out && $err == *SB4060* ]]; chk 56.43

# 56.44 the number must be a literal: the EXIT check-point set is resolved at
# compile time, so an expansion cannot be honoured
run_sash 'int n=2
signal $n EXIT'
[[ $rc != 0 && -z $out && $err == *SB4060* ]]; chk 56.44

# 56.45 an unknown disposition
run_sash 'signal 2 BOGUS'
[[ $rc != 0 && -z $out && $err == *SB4062* ]]; chk 56.45

# 56.46 the disposition must be a literal too
run_sash 'str d=IGN
signal 2 $d'
[[ $rc != 0 && -z $out && $err == *SB4062* ]]; chk 56.46

# 56.47 SIGKILL cannot be caught or ignored -- the kernel refuses, so the
# registration would be a promise the program cannot keep
run_sash 'signal 9 IGN'
[[ $rc != 0 && -z $out && $err == *SB4061* ]]; chk 56.47

# 56.48 SIGSTOP likewise
run_sash 'signal 19 IGN'
[[ $rc != 0 && -z $out && $err == *SB4061* ]]; chk 56.48

# 56.49 out of range
run_sash 'signal 99 IGN'
[[ $rc != 0 && -z $out && $err == *SB4061* ]]; chk 56.49

# 56.50 zero and negative are not signals
run_sash 'signal 0 IGN'
[[ $rc != 0 && -z $out && $err == *SB4061* ]]; chk 56.50

# 56.51 the four valid dispositions all still compile and run -- guards the
# rejection above against over-reach
run_sash 'signal 2 IGN
signal 2 DFL
signal 10 FLAG
signal 15 EXIT
echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 56.51

# --- 56.80+ a failure inside END must not route the abort THROUGH END ---
#
# DD#55 aborts route through the END block so cleanup runs. From *inside*
# END that is a jump to the block already executing: the program looped
# forever, emitting the diagnostic hundreds of thousands of times (rc=124
# under timeout). `run_sash` is time-limited, so a regression here shows up
# as a hang-class failure rather than a wrong-output one.
#
# Two separate holes fed it. The unchecked marking used a label test, but
# `install_end_block` lowers the trap body with `lower_stmts`, so only the
# *entry* block is named `__end__` and an `if`/`for` inside END produced
# ordinary labels (56.81/56.82). And the redirect open(2) abort keys on
# position, never on `checked`, so the marking could not reach it at all
# (56.80, 56.83, 56.84). DG chose: diagnose, restore, keep cleaning up.

# 56.80 a redirect open failure in END diagnoses and the cleanup continues
run_sash 'echo main
END {
  echo hi > /nonexistent_dir_56/f
  echo tail
}'
[[ $rc = 0 && $out = "main
tail" && $rerr == *"/nonexistent_dir_56/f: No such file or directory"* ]]; chk 56.80

# 56.81 a failing command inside an `if` inside END: the nested block gets
# an ordinary label, so the label test left it checked and it aborted
run_sash 'echo main
END {
  if /bin/true {
    /bin/false
    echo tail
  }
}'
[[ $rc = 0 && $out = "main
tail" ]]; chk 56.81

# 56.82 ... and inside a `for`, where the abort re-entered per iteration
run_sash 'echo main
END {
  for i in 1 2 {
    echo n$i
    /bin/false
  }
  echo tail
}'
[[ $rc = 0 && $out = "main
n1
n2
tail" ]]; chk 56.82

# 56.83 the open failure nested inside an `if` inside END — both holes at
# once (nested block AND the position-keyed redirect abort)
run_sash 'echo main
END {
  if /bin/true {
    echo hi > /nonexistent_dir_56/f
    echo tail
  }
}'
[[ $rc = 0 && $out = "main
tail" && $rerr == *"/nonexistent_dir_56/f: No such file or directory"* ]]; chk 56.83

# 56.84 a redirect *scope* on a function call in END behaves the same, and
# reports — silence is for a caught failure (55.25), not a cleanup one
run_sash 'fn f { echo hi }
echo main
END {
  f > /nonexistent_dir_56/f
  echo tail
}'
[[ $rc = 0 && $out = "main
tail" && $rerr == *"/nonexistent_dir_56/f: No such file or directory"* ]]; chk 56.84

# 56.85 `read` shares the guard, so it gets the same treatment
run_sash 'echo main
END {
  read a < /nonexistent_dir_56/f
  echo "n=$(len a)"
}'
[[ $rc = 0 && $out = "main
n=0" && $rerr == *"/nonexistent_dir_56/f: No such file or directory"* ]]; chk 56.85

# 56.86 OUTSIDE END the abort is unchanged — it still aborts, and still
# routes through END so the cleanup runs exactly once (guards the fix
# against over-reach: `in_end_block` must be the region, not the program)
run_sash 'echo main
END {
  echo cleanup
}
echo hi > /nonexistent_dir_56/f
echo unreachable'
[[ $rc != 0 && $out = "main
cleanup" && $rerr == *"/nonexistent_dir_56/f: No such file or directory"* ]]; chk 56.86

# 56.87 an explicit `exit N` inside END still wins over a failed open
run_sash 'echo main
END {
  echo hi > /nonexistent_dir_56/f
  exit 7
}'
[[ $rc = 7 && $out = "main" ]]; chk 56.87

# FLAG cannot become an EXIT merely because another signal is EXIT-armed.
run_sash 'signal 12 FLAG
signal 15 EXIT
END { echo $(signum) }
/bin/kill -12 $$
echo survived'
[[ $rc = 0 && $out = $'survived\n12' && -z $rerr ]]; chk sig518.1

run_sash 'signal 15 EXIT
/bin/kill -15 $$
echo bad'
[[ $rc = 143 && -z $out && -z $rerr ]]; chk sig518.2

run_sash 'signal 15 EXIT
signal 15 FLAG
/bin/kill -15 $$
echo $(signum)'
[[ $rc = 0 && $out = 15 && -z $rerr ]]; chk sig518.3

run_sash 'signal 15 FLAG
/bin/kill -15 $$
signal 15 EXIT
/bin/true
echo survived'
[[ $rc = 0 && $out = survived && -z $rerr ]]; chk sig518.4

run_sash 'int p=$$
signal 15 EXIT
/bin/kill -15 $p | /bin/cat
echo bad'
[[ $rc = 143 && -z $out && -z $rerr ]]; chk sig518.5

run_sash 'signal 15 EXIT
END {
  fn cleanup { /bin/echo end }
  cleanup
  for x in a b { /bin/echo $x }
}
/bin/kill -15 $$
echo bad'
[[ $rc = 143 && $out = $'end\na\nb' && -z $rerr ]]; chk sig518.6

# Delivery on the first authored write leaves a pending EXIT without an
# external-command checkpoint masking the function/loop boundary under test.
run_sash_under 'signal 15 EXIT
fn f { echo bad }
echo armed
f
echo bad' strace -qq -o signal.log -e inject=write:signal=SIGTERM:when=1
[[ $rc = 143 && $out = armed && -z $rerr ]]; chk sig518.7

run_sash_under 'signal 15 EXIT
fn f { echo tick; echo body }
f
echo bad' strace -qq -o signal.log -e inject=write:signal=SIGTERM:when=1
[[ $rc = 143 && $out = $'tick\nbody' && -z $rerr ]]; chk sig518.8

run_sash_under 'signal 15 EXIT
fn f -> int { echo tick; return 7 }
int n=$(( (f) ))
echo $n
echo bad' strace -qq -o signal.log -e inject=write:signal=SIGTERM:when=1
[[ $rc = 143 && $out = tick && -z $rerr ]]; chk sig518.9

run_sash_under 'signal 15 EXIT
int n=0
while (( n < 3 )) { (( n += 1 )); echo tick; echo body }
echo bad' strace -qq -o signal.log -e inject=write:signal=SIGTERM:when=1
[[ $rc = 143 && $out = $'tick\nbody' && -z $rerr ]]; chk sig518.10

run_sash_under 'signal 15 EXIT
for x in a b c { echo $x }
echo bad' strace -qq -o signal.log -e inject=write:signal=SIGTERM:when=1
[[ $rc = 143 && $out = a && -z $rerr ]]; chk sig518.11

run_sash_under 'signal 15 EXIT
array a=(a b c)
for x in @a { echo $x }
echo bad' strace -qq -o signal.log -e inject=write:signal=SIGTERM:when=1
[[ $rc = 143 && $out = a && -z $rerr ]]; chk sig518.12

run_sash_under 'signal 15 EXIT
fn f { for x in @args { echo $x } }
f a b c
echo bad' strace -qq -o signal.log -e inject=write:signal=SIGTERM:when=1
[[ $rc = 143 && $out = a && -z $rerr ]]; chk sig518.13

run_sash_under 'signal 15 EXIT
int n=0
until (( n >= 3 )) { (( n += 1 )); echo tick }
echo bad' strace -qq -o signal.log -e inject=write:signal=SIGTERM:when=1
[[ $rc = 143 && $out = tick && -z $rerr ]]; chk sig518.14

run_sash_under 'signal 15 EXIT
exec 3> marker
fn f { write tick >&3 }
s=$(f)
echo $s
echo bad' strace -qq -o signal.log -e inject=write:signal=SIGTERM:when=1
[[ $rc = 143 && -z $out && $(<marker) = tick && -z $rerr ]]; chk sig518.15

run_sash_under 'signal 15 FLAG
echo armed
echo $(signum)' strace -qq -o signal.log -e inject=write:signal=SIGTERM:when=1
[[ $rc = 0 && $out = $'armed\n15' && -z $rerr ]]; chk sig518.16

printf '%s\n' 'signal 15 FLAG; fn f { echo ok }; f' > flag.sash
out=$(timeout "$SPEC_TIMEOUT" "${SASH:-sash}" --emit-c flag.sash 2> flag.err)
rc=$?
[[ $rc = 0 && $out != *"= sash_exit_signum;"* && $out != *"__signal_checkpoint"* && ! -s flag.err ]]; chk sig518.17

# ---- B9 (batch-531 port, F2): END runs only in the main process (DD#56
# addendum 2026-10-28). A forked child — a background job, a function stage —
# leaves with its own status and never runs END.

# 56.89 `exit` in a background function: END runs once, in the parent
run_sash 'END { echo "end $?" }
fn f { echo hi; exit 3 }
f &
- wait
echo after'
[[ $rc = 0 && $out = $'hi\nafter\nend 0' && -z $rerr ]]; chk 56.89

# 56.91 `exit 0` in a stage function
run_sash 'END { echo "end $?" }
fn f { echo hi; exit 0 }
f | /bin/cat
echo after'
[[ $rc = 0 && $out = $'hi\nafter\nend 0' && -z $rerr ]]; chk 56.91

# 56.92 a failing final-stage function: the parent aborts, names `f`, and runs
# END exactly once
run_sash 'END { echo "end $?" }
fn f { /bin/cat; exit 3 }
/bin/echo a | f
echo after'
[[ $rc = 3 && $out = $'a\nend 3' && $rerr = "./test_bin: f: command failed (exit 3)" ]]; chk 56.92

# 56.93 a background job started inside END does not re-enter END (it used to,
# and a job in END forked without end; the marker file bounds this probe)
rm -f endk.ran
run_sash 'END {
  if [[ -e endk.ran ]] { echo "re-entered"; exit 9 }
  /bin/touch endk.ran
  /bin/echo e &
  - wait
  echo end
}
/bin/true &
- wait
echo main'
[[ $rc = 0 && $out = $'main\ne\nend' && -z $rerr ]]; chk 56.93
rm -f endk.ran

# --- END-defined functions are functions everywhere (batch 531 B8/B16) ---
# SB5071 admits a `fn` only at the top level or directly in `END { }`, but the
# transform and binding resolution registered top-level definitions alone, so
# an END function was unknown to both. Each spec below is the top-level
# behaviour, which END now shares; the parenthesis says what it did before.

# 56.94 `(f)` calls an END-defined int function (it read a variable f: 1)
run_sash 'END { fn f -> int { return 7; }; echo $(( (f) + 1 )); }'
[[ $rc = 0 && $out = 8 && -z $rerr ]]; chk 56.94

# 56.95 an END-defined function runs as a pipeline stage (the stage exec'd the
# bare name: "sash: bug: bare command passed to sash_exec")
run_sash 'END { fn up { echo hi; }; up | /bin/cat; }'
[[ $rc = 0 && $out = hi && -z $rerr ]]; chk 56.95

# 56.96 a variable and an END-defined function may not share a name, SB4019 as
# at the top level (it compiled and printed both)
run_sash 'f=1
echo "$f"
END { fn f { echo a; }; f; }'
[[ $rc != 0 && -z $out && $err == *":1:1: error [SB4019] variable 'f' collides with function 'f'"* ]]; chk 56.96

# 56.97 a call before the END definition is SB5041, as at the top level
# (DD#81 Part A; it read a variable f: 1)
run_sash 'END { echo $(( (f) + 1 )); fn f -> int { return 7; } }'
[[ $rc != 0 && -z $out && $err == *":1:28: error [SB5041] 'f' is used as a command before this definition"* ]]; chk 56.97

# 56.98 so is a top-level call, which runs before END defines f (it printed 1)
run_sash 'echo $(( (f) + 1 ))
END { fn f -> int { return 7; } }'
[[ $rc != 0 && -z $out && $err == *":2:7: error [SB5041] 'f' is used as a command before this definition"* ]]; chk 56.98

# 56.99 a collection argument between END-defined functions binds (binding
# resolution never renamed `h =a` there: a false SB5066)
run_sash 'END {
  fn h {
    echo "${A[0]}"
  }
  fn g {
    array a=(7 8)
    h =a
  }
  g
}
echo main'
[[ $rc = 0 && $out = $'main\n7' && -z $rerr ]]; chk 56.99

# 56.100 control: a bare int-function name after its END definition. It worked
# before only because the definition registered its `-> int` return type
# lazily; the up-front registration that replaced that must cover END too.
run_sash 'END { fn f -> int { return 7; }; echo $(( f + 1 )); }'
[[ $rc = 0 && $out = 8 && -z $rerr ]]; chk 56.100

set -e
tally
