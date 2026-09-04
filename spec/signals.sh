#!/bin/bash
# DD#56 — signals and END block
. common.sh
plan=23
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
# available as its FIRST action, like a handling branch. (janet's END block is
# a separate pre-existing gap — 56.1/56.4 already fail there — so native,c.)

# 56.30 abort (failed command) → `$?` in END is the failing exit code
if in_modes native,c; then
    run_sash 'END { /bin/echo "s=$?" }
/bin/false'
    [[ $rc != 0 && $out == *"s=1"* && -z $err ]]; chk 56.30
else skip_chk 56.30; fi

# 56.31 `exit N` → `$?` in END is N (the requested exit code). native only: the
# c backend's `exit N` in the main body skips the END block (a pre-existing DD#56
# gap — filed bug-emit-c-exit-skips-end); native routes it through END.
if in_modes native; then
    run_sash 'END { /bin/echo "s=$?" }
exit 5'
    [[ $rc = 5 && $out == *"s=5"* && -z $err ]]; chk 56.31
else skip_chk 56.31; fi

# 56.32 normal exit → `$?` in END is the last command's status (0 here)
if in_modes native,c; then
    run_sash 'END { /bin/echo "s=$?" }
/bin/echo hi'
    [[ $rc = 0 && $out == *"s=0"* && -z $err ]]; chk 56.32
else skip_chk 56.32; fi

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

set -e
tally
