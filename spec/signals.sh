#!/bin/bash
# DD#56 / DD#128 — signals and terminal handlers
. common.sh
plan=73
setup signals

set +e

# --- Terminal handlers ---

# EXIT executes on orderly completion.
run_sash 'EXIT {
  echo bye
}
echo hello'
[[ $rc = 0 && $out == *"hello"* && $out == *"bye"* && -z $err ]]; chk h128.56.1

# Reaching the end of EXIT terminates.
run_sash 'EXIT {
  echo done
}
echo main'
[[ $rc = 0 && $out == *"done"* && -z $err ]]; chk h128.56.2a

# Return in EXIT terminates without re-entry.
run_sash 'EXIT {
  return
  echo unreachable
}
echo main'
[[ $rc = 0 && $out != *"unreachable"* && -z $err ]]; chk h128.56.2b

# An explicit exit in ERR overrides the triggering status.
run_sash 'ERR {
  exit 42
}
/bin/false'
[[ $rc = 42 && -z $err ]]; chk h128.56.3

# 56.4 without exit N, preserves triggering status
run_sash 'ERR {
  - echo cleanup
}
/bin/false'
[[ $rc != 0 && $out == *"cleanup"* && -z $err ]]; chk h128.56.4

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

# A configured signal selects EXIT, despite its nonzero status.
run_sash 'EXIT {
  echo shutting_down
}
signal 15 EXIT
/bin/kill -TERM $$
echo should_not_reach'
[[ $rc != 0 && $out == *"shutting_down"* && $out != *"should_not_reach"* && -z $err ]]; chk h128.56.8

# 56.9 check points at external commands
run_sash 'EXIT {
  echo caught
}
signal 2 EXIT
/bin/kill -INT $$
/bin/echo after_kill'
[[ $rc != 0 && $out == *"caught"* && $out != *"after_kill"* && -z $err ]]; chk h128.56.9

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
# The native signal model is the `signal` builtin + `EXIT { }`; `trap` is a bash
# spelling the converter rewrites (57.14 below). A hand-written `trap` in sash
# source is rejected, pointing at signal/EXIT. (Validation reject — all modes.)

# 56.14 `trap … EXIT` (command handler) → SB4043
run_sash 'trap "echo cleanup" EXIT; echo hi'
[[ $rc != 0 && -z $out && $err == *SB4043* ]]; chk 56.14
# 56.15 `trap "" SIG` / `trap - SIG` (ignore/reset) → SB4043 too
run_sash 'trap "" SIGINT; echo hi'
[[ $rc != 0 && -z $out && $err == *SB4043* ]]; chk 56.15

# The handler's triggering status is available as its first action.

run_sash 'ERR { - /bin/echo "s=$?" }
/bin/false'
[[ $rc != 0 && $out == *"s=1"* && -z $err ]]; chk h128.56.30

run_sash 'ERR { - /bin/echo "s=$?" }
fail 5'
[[ $rc = 5 && $out == *"s=5"* && -z $err ]]; chk migratea.55

# 56.34 DD#89: a bare `exit` is `exit 0`, NOT "keep the previous status". The c
# backend left __exit_status untouched, so a bare exit after a `-`-suppressed
# failure exited 1 there and 0 in native. EXIT sees 0 too.
run_sash 'EXIT { /bin/echo "s=$?" }
- /bin/false
exit'
[[ $rc = 0 && $out == *"s=0"* && -z $err ]]; chk h128.56.34

# 56.35 the same bare exit with no handler — the exit code itself is 0
run_sash '- /bin/false
exit'
[[ $rc = 0 && -z $out && -z $err ]]; chk 56.35

# Normal completion preserves the last status.
run_sash 'EXIT { /bin/echo "s=$?" }
/bin/echo hi'
[[ $rc = 0 && $out == *"s=0"* && -z $err ]]; chk h128.56.32

# 56.33 guard: handler-entry status does NOT leak into a regular function — a
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

# Handlers are separate Program fields, never nested statements.
run_sash 'fn f {
EXIT {
/bin/echo a
}
}
f'
[[ $rc != 0 && -z $out && $err == *SB4054* && $err != *__sash_end__* ]]; chk h128.56.36

# A second declaration of the same handler is rejected.
run_sash 'EXIT {
/bin/echo a
}
EXIT {
/bin/echo b
}'
[[ $rc != 0 && -z $out && $err == *SB4055* ]]; chk h128.56.37

# A malformed handler must not reach downstream passes as a partial body.
run_sash 'echo hi
EXIT {
/bin/echo bye'
[[ $rc != 0 && -z $out && $err == *SB4014* ]]; chk h128.56.88

# The handler's scope is global, not a function's positional-argument scope.
run_sash 'EXIT {
/bin/echo $@
}'
[[ $rc != 0 && -z $out && $err == *SB5045* ]]; chk h128.56.38

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

# EXIT is checked, including nested blocks and redirect scopes.
# Failures enter ERR once rather than restarting or silently continuing EXIT.
run_sash 'echo main
EXIT {
  echo hi > /nonexistent_dir_56/f
  echo tail
}
ERR { - echo failed }'
[[ $rc = 1 && $out = $'main\nfailed' &&
   $rerr == *"/nonexistent_dir_56/f: No such file or directory"* ]]; chk h128.56.80

run_sash 'echo main
EXIT {
  if /bin/true {
    /bin/false
    echo tail
  }
}
ERR { - echo failed }'
[[ $rc = 1 && $out = $'main\nfailed' ]]; chk h128.56.81

run_sash 'echo main
EXIT {
  for i in 1 2 {
    echo n$i
    /bin/false
  }
  echo tail
}
ERR { - echo failed }'
[[ $rc = 1 && $out = $'main\nn1\nfailed' ]]; chk h128.56.82

run_sash 'echo main
EXIT {
  if /bin/true {
    echo hi > /nonexistent_dir_56/f
    echo tail
  }
}
ERR { - echo failed }'
[[ $rc = 1 && $out = $'main\nfailed' &&
   $rerr == *"/nonexistent_dir_56/f: No such file or directory"* ]]; chk h128.56.83

run_sash 'fn f { echo hi }
echo main
EXIT {
  f > /nonexistent_dir_56/f
  echo tail
}
ERR { - echo failed }'
[[ $rc = 1 && $out = $'main\nfailed' &&
   $rerr == *"/nonexistent_dir_56/f: No such file or directory"* ]]; chk h128.56.84

run_sash 'echo main
EXIT {
  read a < /nonexistent_dir_56/f
  echo "n=$(len a)"
}
ERR { - echo "n=$(len a)" }'
[[ $rc = 1 && $out = $'main\nn=0' &&
   $rerr == *"/nonexistent_dir_56/f: No such file or directory"* ]]; chk h128.56.85

# Main failures select ERR directly.
run_sash 'echo main
ERR {
  - echo cleanup
}
echo hi > /nonexistent_dir_56/f
echo unreachable'
[[ $rc != 0 && $out = "main
cleanup" && $rerr == *"/nonexistent_dir_56/f: No such file or directory"* ]]; chk h128.56.86

# A caught setup failure can deliberately terminate with a chosen status.
run_sash 'echo main
EXIT {
  if echo hi > /nonexistent_dir_56/f { : }
  exit 7
}'
[[ $rc = 7 && $out = "main" ]]; chk h128.56.87

# FLAG cannot become an EXIT merely because another signal is EXIT-armed.
run_sash 'signal 12 FLAG
signal 15 EXIT
EXIT { echo $(signum) }
/bin/kill -12 $$
echo survived'
[[ $rc = 0 && $out = $'survived\n12' && -z $rerr ]]; chk h128.sig518.1

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
fn cleanup { /bin/echo end }
EXIT {
  cleanup
  for x in a b { /bin/echo $x }
}
/bin/kill -15 $$
echo bad'
[[ $rc = 143 && $out = $'end\na\nb' && -z $rerr ]]; chk h128.sig518.18

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
echo ${s:!}
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

# Forked children leave with their own status and never run either handler.

run_sash 'EXIT { echo "end $?" }
ERR { - echo unexpected }
fn f { echo hi; exit 3 }
f &
- wait
echo after'
[[ $rc = 0 && $out = $'hi\nafter\nend 0' && -z $rerr ]]; chk h128.56.89

# 56.91 `exit 0` in a stage function
run_sash 'EXIT { echo "end $?" }
ERR { - echo unexpected }
fn f { echo hi; exit 0 }
f | /bin/cat
echo after'
[[ $rc = 0 && $out = $'hi\nafter\nend 0' && -z $rerr ]]; chk h128.56.91

# 56.92 a failing final-stage function: the parent aborts, names `f`, and runs
# ERR exactly once
run_sash 'ERR { - echo "end $?" }
fn f { /bin/cat; exit 3 }
/bin/echo a | f
echo after'
[[ $rc = 3 && $out = $'a\nend 3' && $rerr = "./test_bin: f: command failed (exit 3)" ]]; chk h128.56.92

# A job started inside EXIT does not re-enter it; the marker bounds regressions.
rm -f endk.ran
run_sash 'EXIT {
  if [[ -e endk.ran ]] { echo "re-entered"; exit 9 }
  /bin/touch endk.ran
  /bin/echo e &
  - wait
  echo end
}
/bin/true &
- wait
echo main'
[[ $rc = 0 && $out = $'main\ne\nend' && -z $rerr ]]; chk h128.56.93
rm -f endk.ran

# A cleanup body shares global variables, not the program's definition root.
# DD86's SB5071 applies here too; DD128 retains it for both EXIT and ERR.
run_sash 'EXIT {
fn cleanup { echo done; }
cleanup
}'
[[ $rc != 0 && -z $out && $err == *':2:1: error [SB5071]'* && $err == *'Move `fn cleanup` to the top level.'* ]]; chk h128.fnroot.1

# Actual completed writes, not the normal-path version after a skipped attest.
run_sash 'x=safe
x=$(/bin/sh -c "echo evil; exit 3")
attest x [[ $x == allowed ]]
ERR { - echo "end[${x:!}]" }'
[[ $rc = 3 && -z $err && $out = 'end[safe]' && $rerr == *'command failed (exit 3)'* ]]; chk h128.gstate.1

run_sash 'x=before
/bin/sh -c "echo changed; exit 3" | readline x
attest x [[ $x == allowed ]]
ERR { - echo "read[${x:!}]" }'
[[ $rc = 3 && -z $err && $out = 'read[changed]' && $rerr == *'command failed (exit 3)'* ]]; chk h128.gstate.2

set -e
tally
