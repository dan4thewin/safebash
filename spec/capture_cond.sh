#!/bin/bash
# spec/capture_cond.sh — DD#88 capture-condition: `if x=$(cmd)` / `while x=$(cmd)`.
# A single external command substitution (or pipeline) as an assignment RHS in a
# boolean context is the CONDITION: the command's exit status is the truth value
# (no abort), and its stdout is captured into the target (tainted, attest before
# use). Positive behavior is native,c (janet's `$(cmd)` capture is a stub, filed
# janet-capture-condition). The rejects (SB4025 / SB5050 / SB5020) are
# validate/taint and backend-independent (all modes).
set -eu
. common.sh
setup capture_cond
set +e
plan=18
echo "1..$plan" >&3

# 88.1 non-negated success: the then-branch runs, stdout captured into x
if in_modes native,c; then
    run_sash 'if x=$(/bin/echo hi) { attest x [[ -n $x ]]; /bin/echo "ok [$x]" } else { /bin/echo no }'
    [[ $rc = 0 && $out = "ok [hi]" && -z $err ]]; chk 88.1
else skip_chk 88.1; fi

# 88.2 non-negated failure: else runs; `$?` in else is the REAL exit code (3),
# as the first action in the handling branch (DD#55 handling-position rule)
if in_modes native,c; then
    run_sash 'if x=$(/bin/sh -c "exit 3") { attest x [[ -n $x ]]; /bin/echo "[$x]" } else { /bin/echo "failed $?" }'
    [[ $rc = 0 && $out = "failed 3" && -z $err ]]; chk 88.2
else skip_chk 88.2; fi

# 88.3 negated `if ! x=$(cmd)`: the branch is taken on FAILURE; x is used after
if in_modes native,c; then
    run_sash 'if ! x=$(/bin/echo hi) { /bin/echo bang }
attest x [[ -n $x ]]; /bin/echo "got [$x]"'
    [[ $rc = 0 && $out = "got [hi]" && -z $err ]]; chk 88.3
else skip_chk 88.3; fi

# 88.4 `$?` as the FIRST action in the then-branch is 0 on success (x still must
# be read somewhere — here after the if)
if in_modes native,c; then
    run_sash 'if x=$(/bin/echo hi) { /bin/echo "status $?" }
attest x [[ -n $x ]]; /bin/echo "$x"'
    [[ $rc = 0 && $out = $'status 0\nhi' && -z $err ]]; chk 88.4
else skip_chk 88.4; fi

# 88.5 the captured command actually FAILS → else branch (no abort)
if in_modes native,c; then
    run_sash 'if x=$(/bin/false) { attest x [[ -n $x ]]; /bin/echo "[$x]" } else { /bin/echo caught }'
    [[ $rc = 0 && $out = "caught" && -z $err ]]; chk 88.5
else skip_chk 88.5; fi

# 88.6 pipeline RHS `$(a | b)`: stdout captured, condition = pipeline status
if in_modes native,c; then
    run_sash 'if x=$(/bin/echo hi | /bin/cat) { attest x [[ -n $x ]]; /bin/echo "ok [$x]" } else { /bin/echo no }'
    [[ $rc = 0 && $out = "ok [hi]" && -z $err ]]; chk 88.6
else skip_chk 88.6; fi

# 88.7 pipeline failure (pipefail) → else; `$?` is the aggregate status (1)
if in_modes native,c; then
    run_sash 'if x=$(/bin/false | /bin/cat) { attest x [[ -n $x ]]; /bin/echo ok } else { /bin/echo "failed $?" }'
    [[ $rc = 0 && $out = "failed 1" && -z $err ]]; chk 88.7
else skip_chk 88.7; fi

# 88.8 `while x=$(cmd)`: same capture-condition form in a loop
if in_modes native,c; then
    run_sash 'while x=$(/bin/echo one) { attest x [[ -n $x ]]; /bin/echo "[$x]"; break }'
    [[ $rc = 0 && $out = "[one]" && -z $err ]]; chk 88.8
else skip_chk 88.8; fi

# ── `!` vs `else`: `$?` magnitude under negation (uniform with a plain command
# condition — `!` = NegateTestResult collapses the stored status to 0/1; the
# capture does NOT change this). This is the DD#88 negation contrast. ──

# 88.15 negated `if ! x=$(cmd)` on a FAILING command: the bang-branch is taken,
# and `$?` there is the NEGATED boolean (0), NOT the command's real code — `!`
# collapses the magnitude, exactly like `if ! cmd`. (x is read after the if.)
if in_modes native,c; then
    run_sash 'if ! x=$(/bin/sh -c "/bin/echo o; exit 7") { /bin/echo "neg $?" }
attest x [[ -n $x ]]; /bin/echo "x=[$x]"'
    [[ $rc = 0 && $out = $'neg 0\nx=[o]' && -z $err ]]; chk 88.15
else skip_chk 88.15; fi

# 88.16 the SAME failing command via the non-negated `else` form: `$?` in the
# else branch is the command's REAL exit code (7). Use this spelling to read the
# numeric status in-branch.
if in_modes native,c; then
    run_sash 'if x=$(/bin/sh -c "/bin/echo o; exit 7") { attest x [[ -n $x ]]; /bin/echo "ok [$x]" } else { /bin/echo "else $?" }'
    [[ $rc = 0 && $out = "else 7" && -z $err ]]; chk 88.16
else skip_chk 88.16; fi

# 88.17 a BARE external command name (`printf`, not `/usr/bin/printf`) is a
# capture-condition too — the external test must not require a `/`. (Regression:
# `validate` runs before command resolution, so the head is still bare; the
# external test excludes intrinsics/builtins/functions, not "no slash".)
if in_modes native,c; then
    run_sash 'if out=$(printf foo) { attest out [[ -n $out ]]; /bin/echo "y $out" }'
    [[ $rc = 0 && $out = "y foo" && -z $err ]]; chk 88.17
else skip_chk 88.17; fi

# ── Taint + rejects (backend-independent: validate / taint) ──

# 88.9 the captured target is TAINTED (external output) — using it unattested is
# SB5020 (attest required)
run_sash 'if x=$(/bin/echo hi) { /bin/echo $x }'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 88.9

# 88.10 a truly-unused captured target is a dead variable (SB5050): if you do
# not need the stdout, use the non-capturing `if ! cmd` form
run_sash 'if x=$(/bin/echo hi) { /bin/echo unrelated }'
[[ $rc != 0 && -z $out && $err == *SB5050* ]]; chk 88.10

# 88.11 exclusion: a user-function capture `$(f)` is NOT a capture-condition
# (functions convey status via `return`) → SB4025. `validate` runs before
# resolution and can't see the function table, so this rejection comes from the
# transform (a bare `f` is external-looking until then); still SB4025, all modes.
run_sash 'fn f { return 0 }; if x=$(f) { /bin/echo $x }'
[[ $rc != 0 && -z $out && $err == *SB4025* ]]; chk 88.11

# 88.12 exclusion: a concatenation RHS (`foo$(cmd)`) has no single status → SB4025
# with the DD#88 Group-B message (a capture-condition needs a single external cmd)
run_sash 'if x=foo$(/bin/echo bar) { /bin/echo $x }'
[[ $rc != 0 && -z $out && $err == *SB4025* && $err == *"single external"* ]]; chk 88.12

# 88.13 exclusion: an intrinsic capture `$(getenv …)` is not an external command → SB4025
# (see task getenv-in-condition-presence for a possible future presence-test form)
run_sash 'if x=$(getenv HOME) { /bin/echo $x }'
[[ $rc != 0 && -z $out && $err == *SB4025* && $err == *"single external"* ]]; chk 88.13

# 88.14 a plain literal RHS in a condition is a value binding → SB4025 with the
# "sequence it" advice (Group A — no capture involved, so NOT the Group-B text)
run_sash 'if x=hi { /bin/echo $x }'
[[ $rc != 0 && -z $out && $err == *SB4025* && $err == *"sequence it"* ]]; chk 88.14

# 88.18 exclusion: a builtin capture `$(echo …)` (echo can't fail) → SB4025 with
# the Group-B message. Pins the DD#88 message: capture-conditions are external-only.
run_sash 'if x=$(echo hi) { /bin/echo $x }'
[[ $rc != 0 && -z $out && $err == *SB4025* && $err == *"single external"* ]]; chk 88.18
