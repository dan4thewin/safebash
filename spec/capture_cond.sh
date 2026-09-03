#!/bin/bash
# spec/capture_cond.sh — DD#88 capture-condition: `if x=$(cmd)` / `while x=$(cmd)`.
# A single external command substitution (or pipeline) as an assignment RHS in a
# boolean context is the CONDITION: the command's exit status is the truth value
# (no abort), and its stdout is captured into the target (tainted, bless before
# use). Positive behavior is native,c (janet's `$(cmd)` capture is a stub, filed
# janet-capture-condition). The rejects (SB4025 / SB5050 / SB5020) are
# validate/taint and backend-independent (all modes).
set -eu
. common.sh
setup capture_cond
set +e
plan=14
echo "1..$plan" >&3

# 88.1 non-negated success: the then-branch runs, stdout captured into x
if in_modes native,c; then
    run_sash 'if x=$(/bin/echo hi) { bless x [[ -n $x ]]; /bin/echo "ok [$x]" } else { /bin/echo no }'
    [[ $rc = 0 && $out = "ok [hi]" && -z $err ]]; chk 88.1
else skip_chk 88.1; fi

# 88.2 non-negated failure: else runs; `$?` in else is the REAL exit code (3),
# as the first action in the handling branch (DD#55 handling-position rule)
if in_modes native,c; then
    run_sash 'if x=$(/bin/sh -c "exit 3") { bless x [[ -n $x ]]; /bin/echo "[$x]" } else { /bin/echo "failed $?" }'
    [[ $rc = 0 && $out = "failed 3" && -z $err ]]; chk 88.2
else skip_chk 88.2; fi

# 88.3 negated `if ! x=$(cmd)`: the branch is taken on FAILURE; x is used after
if in_modes native,c; then
    run_sash 'if ! x=$(/bin/echo hi) { /bin/echo bang }
bless x [[ -n $x ]]; /bin/echo "got [$x]"'
    [[ $rc = 0 && $out = "got [hi]" && -z $err ]]; chk 88.3
else skip_chk 88.3; fi

# 88.4 `$?` as the FIRST action in the then-branch is 0 on success (x still must
# be read somewhere — here after the if)
if in_modes native,c; then
    run_sash 'if x=$(/bin/echo hi) { /bin/echo "status $?" }
bless x [[ -n $x ]]; /bin/echo "$x"'
    [[ $rc = 0 && $out = $'status 0\nhi' && -z $err ]]; chk 88.4
else skip_chk 88.4; fi

# 88.5 the captured command actually FAILS → else branch (no abort)
if in_modes native,c; then
    run_sash 'if x=$(/bin/false) { bless x [[ -n $x ]]; /bin/echo "[$x]" } else { /bin/echo caught }'
    [[ $rc = 0 && $out = "caught" && -z $err ]]; chk 88.5
else skip_chk 88.5; fi

# 88.6 pipeline RHS `$(a | b)`: stdout captured, condition = pipeline status
if in_modes native,c; then
    run_sash 'if x=$(/bin/echo hi | /bin/cat) { bless x [[ -n $x ]]; /bin/echo "ok [$x]" } else { /bin/echo no }'
    [[ $rc = 0 && $out = "ok [hi]" && -z $err ]]; chk 88.6
else skip_chk 88.6; fi

# 88.7 pipeline failure (pipefail) → else; `$?` is the aggregate status (1)
if in_modes native,c; then
    run_sash 'if x=$(/bin/false | /bin/cat) { bless x [[ -n $x ]]; /bin/echo ok } else { /bin/echo "failed $?" }'
    [[ $rc = 0 && $out = "failed 1" && -z $err ]]; chk 88.7
else skip_chk 88.7; fi

# 88.8 `while x=$(cmd)`: same capture-condition form in a loop
if in_modes native,c; then
    run_sash 'while x=$(/bin/echo one) { bless x [[ -n $x ]]; /bin/echo "[$x]"; break }'
    [[ $rc = 0 && $out = "[one]" && -z $err ]]; chk 88.8
else skip_chk 88.8; fi

# ── Taint + rejects (backend-independent: validate / taint) ──

# 88.9 the captured target is TAINTED (external output) — using it unblessed is
# SB5020 (bless required)
run_sash 'if x=$(/bin/echo hi) { /bin/echo $x }'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 88.9

# 88.10 a truly-unused captured target is a dead variable (SB5050): if you do
# not need the stdout, use the non-capturing `if ! cmd` form
run_sash 'if x=$(/bin/echo hi) { /bin/echo unrelated }'
[[ $rc != 0 && -z $out && $err == *SB5050* ]]; chk 88.10

# 88.11 exclusion: a user-function capture `$(f)` is NOT a capture-condition
# (functions convey status via `return`) → stays SB4025
run_sash 'fn f { return 0 }; if x=$(f) { /bin/echo $x }'
[[ $rc != 0 && -z $out && $err == *SB4025* ]]; chk 88.11

# 88.12 exclusion: a concatenation RHS (`foo$(cmd)`) has no single status → SB4025
run_sash 'if x=foo$(/bin/echo bar) { /bin/echo $x }'
[[ $rc != 0 && -z $out && $err == *SB4025* ]]; chk 88.12

# 88.13 exclusion: an intrinsic capture `$(getenv …)` is not an external command → SB4025
run_sash 'if x=$(getenv HOME) { /bin/echo $x }'
[[ $rc != 0 && -z $out && $err == *SB4025* ]]; chk 88.13

# 88.14 a plain literal RHS in a condition is still a value binding → SB4025
run_sash 'if x=hi { /bin/echo $x }'
[[ $rc != 0 && -z $out && $err == *SB4025* ]]; chk 88.14
