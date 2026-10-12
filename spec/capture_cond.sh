#!/bin/bash
# spec/capture_cond.sh — DD#88 capture-condition: `if x=$(cmd)` / `while x=$(cmd)`.
# A single external command substitution (or pipeline) as an assignment RHS in a
# boolean context is the CONDITION: the command's exit status is the truth value
# (no abort), and its stdout is captured into the target (tainted, attest before
# use). The rejects (SB4025 / SB5050 / SB5020) are validate/taint and
# backend-independent.
set -eu
. common.sh
setup capture_cond
set +e
plan=48
echo "1..$plan" >&3

# 88.1 non-negated success: the then-branch runs, stdout captured into x
run_sash 'if x=$(/bin/echo hi) { attest x [[ -n $x ]]; /bin/echo "ok [$x]" } else { /bin/echo no }'
[[ $rc = 0 && $out = "ok [hi]" && -z $err ]]; chk 88.1

# 88.2 non-negated failure: else runs; `$?` in else is the REAL exit code (3),
# as the first action in the handling branch (DD#55 handling-position rule)
run_sash 'if x=$(/bin/sh -c "exit 3") { attest x [[ -n $x ]]; /bin/echo "[$x]" } else { /bin/echo "failed $?" }'
[[ $rc = 0 && $out = "failed 3" && -z $err ]]; chk 88.2

# 88.3 negated `if ! x=$(cmd)`: the branch is taken on FAILURE; x is used after
run_sash 'if ! x=$(/bin/echo hi) { /bin/echo bang }
attest x [[ -n $x ]]; /bin/echo "got [$x]"'
[[ $rc = 0 && $out = "got [hi]" && -z $err ]]; chk 88.3

# 88.4 `$?` as the FIRST action in the then-branch is 0 on success (x still must
# be read somewhere — here after the if)
run_sash 'if x=$(/bin/echo hi) { /bin/echo "status $?" }
attest x [[ -n $x ]]; /bin/echo "$x"'
[[ $rc = 0 && $out = $'status 0\nhi' && -z $err ]]; chk 88.4

# 88.5 the captured command actually FAILS → else branch (no abort)
run_sash 'if x=$(/bin/false) { attest x [[ -n $x ]]; /bin/echo "[$x]" } else { /bin/echo caught }'
[[ $rc = 0 && $out = "caught" && -z $err ]]; chk 88.5

# 88.6 pipeline RHS `$(a | b)`: stdout captured, condition = pipeline status
run_sash 'if x=$(/bin/echo hi | /bin/cat) { attest x [[ -n $x ]]; /bin/echo "ok [$x]" } else { /bin/echo no }'
[[ $rc = 0 && $out = "ok [hi]" && -z $err ]]; chk 88.6

# 88.7 pipeline failure (pipefail) → else; `$?` is the aggregate status (1)
run_sash 'if x=$(/bin/false | /bin/cat) { attest x [[ -n $x ]]; /bin/echo ok } else { /bin/echo "failed $?" }'
[[ $rc = 0 && $out = "failed 1" && -z $err ]]; chk 88.7

# 88.8 `while x=$(cmd)`: same capture-condition form in a loop
run_sash 'while x=$(/bin/echo one) { attest x [[ -n $x ]]; /bin/echo "[$x]"; break }'
[[ $rc = 0 && $out = "[one]" && -z $err ]]; chk 88.8

# ── `!` vs `else`: `$?` magnitude under negation (uniform with a plain command
# condition — `!` = NegateTestResult collapses the stored status to 0/1; the
# capture does NOT change this). This is the DD#88 negation contrast. ──

# 88.15 negated `if ! x=$(cmd)` on a FAILING command: the bang-branch is taken,
# and `$?` there is the NEGATED boolean (0), NOT the command's real code — `!`
# collapses the magnitude, exactly like `if ! cmd`. (x is read after the if.)
run_sash 'if ! x=$(/bin/sh -c "/bin/echo o; exit 7") { /bin/echo "neg $?" }
attest x [[ -n $x ]]; /bin/echo "x=[$x]"'
[[ $rc = 0 && $out = $'neg 0\nx=[o]' && -z $err ]]; chk 88.15

# 88.16 the SAME failing command via the non-negated `else` form: `$?` in the
# else branch is the command's REAL exit code (7). Use this spelling to read the
# numeric status in-branch.
run_sash 'if x=$(/bin/sh -c "/bin/echo o; exit 7") { attest x [[ -n $x ]]; /bin/echo "ok [$x]" } else { /bin/echo "else $?" }'
[[ $rc = 0 && $out = "else 7" && -z $err ]]; chk 88.16

# 88.17 a BARE external command name (`printf`, not `/usr/bin/printf`) is a
# capture-condition too — the external test must not require a `/`. (Regression:
# `validate` runs before command resolution, so the head is still bare; the
# external test excludes intrinsics/builtins/functions, not "no slash".)
run_sash 'if out=$(printf foo) { attest out [[ -n $out ]]; /bin/echo "y $out" }'
[[ $rc = 0 && $out = "y foo" && -z $err ]]; chk 88.17

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

# 88.13 DD#101 carve-out: `$(getenv NAME)` IS a capture-condition. This spec
# previously pinned the opposite (SB4025) — DD#101 overturned it, because the
# getenv lookup is the one intrinsic result that can legitimately fail.
run_sash 'if x=$(getenv HOME) { attest x [[ -n $x ]]; /bin/echo "got [$x]" }'
[[ $rc = 0 && $out == "got ["*"]" && -z $err ]]; chk 88.13

# 88.14 a plain literal RHS in a condition is a value binding → SB4025 with the
# "sequence it" advice (Group A — no capture involved, so NOT the Group-B text)
run_sash 'if x=hi { /bin/echo $x }'
[[ $rc != 0 && -z $out && $err == *SB4025* && $err == *"sequence it"* ]]; chk 88.14

# 88.18 exclusion: a builtin capture `$(echo …)` (echo can't fail) → SB4025 with
# the Group-B message. Pins the DD#88 message: capture-conditions are external-only.
run_sash 'if x=$(echo hi) { /bin/echo $x }'
[[ $rc != 0 && -z $out && $err == *SB4025* && $err == *"single external"* ]]; chk 88.18
# ── DD#101: $(getenv NAME) reports a lookup status ──────────────────
# Unset is a failed lookup; set — INCLUDING set to the empty string — is a
# successful one. Present-but-empty succeeding is the entire point: without
# it, unset and set-to-empty stay indistinguishable, which no amount of
# validation can recover.

export SASH_SPEC_SET=/spec/value
export SASH_SPEC_EMPTY=""
unset SASH_SPEC_UNSET 2>/dev/null || true

# dd101.1 the shape DG asked for: `h=$(getenv NAME) || h=DEFAULT`. A present
# name succeeds, so the fallback does not run.
run_sash 'str h=""
h=$(getenv SASH_SPEC_SET) || h=/fallback
attest h [[ -n $h ]]; /bin/echo "[$h]"'
[[ $rc = 0 && $out = "[/spec/value]" && -z $err ]]; chk dd101.1

# dd101.2 an unset name is a FAILED lookup → the fallback runs
run_sash 'str h=""
h=$(getenv SASH_SPEC_UNSET) || h=/fallback
attest h [[ -n $h ]]; /bin/echo "[$h]"'
[[ $rc = 0 && $out = "[/fallback]" && -z $err ]]; chk dd101.2

# dd101.3 set-to-EMPTY is a SUCCESSFUL lookup (POSIX getenv: the variable
# exists). The fallback must NOT fire, and h must be the empty string. This is
# the one check that pins unset-vs-empty apart; the runtime distinguishes them
# by the returned POINTER, since the length is 0 either way.
run_sash 'str h="seed"
h=$(getenv SASH_SPEC_EMPTY) || h=/fallback
attest h [[ $h != /fallback ]]; /bin/echo "[$h]"'
[[ $rc = 0 && $out = "[]" && -z $err ]]; chk dd101.3

# dd101.4 the DD#88 negated capture-condition form over getenv
run_sash 'str h="seed"
if ! h=$(getenv SASH_SPEC_UNSET) { /bin/echo absent }
attest h [[ -z $h ]]; /bin/echo "[$h]"'
[[ $rc = 0 && $out = $'absent\n[]' && -z $err ]]; chk dd101.4

# dd101.5 non-negated: a present name takes the then-branch and captures
run_sash 'str h="seed"
if h=$(getenv SASH_SPEC_SET) { attest h [[ -n $h ]]; /bin/echo "hit [$h]" } else { /bin/echo miss }'
[[ $rc = 0 && $out = "hit [/spec/value]" && -z $err ]]; chk dd101.5

# dd101.6 ...and an unset name takes the else-branch, where `$?` is the real
# nonzero lookup status (DD#55 handling-position rule)
run_sash 'str h="seed"
if h=$(getenv SASH_SPEC_UNSET) { attest h [[ -n $h ]]; /bin/echo "hit [$h]" } else { /bin/echo "miss $?" }'
[[ $rc = 0 && $out = "miss 1" && -z $err ]]; chk dd101.6

# dd101.7 a failed lookup does NOT abort — DD#101 rules abort out by
# construction, since `||` branches on a status and an abort would tear the
# program down before the fallback could run
run_sash 'str h="seed"
h=$(getenv SASH_SPEC_UNSET) || h=/fallback
attest h [[ -n $h ]]; /bin/echo done'
[[ $rc = 0 && $out = "done" && -z $rerr ]]; chk dd101.7

# dd101.8 taint survives the join: the `||` fallback binds a clean literal,
# but the conservative join keeps h tainted, so a use without attest is still
# SB5020. (validate — all modes.)
run_sash 'str h=""
h=$(getenv SASH_SPEC_SET) || h=/fallback
/bin/echo "$h"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk dd101.8

# dd101.9 the carve-out is `getenv` specifically, not intrinsics generally —
# `$(len a)` is a pure function of its argument and stays SB4025
run_sash 'array a=(x y)
if n=$(len a) { /bin/echo "$n" }'
[[ $rc != 0 && -z $out && $err == *SB4025* ]]; chk dd101.9

# dd101.10 ...and neither is `$(status)`, which genuinely cannot fail
run_sash 'if s=$(status) { /bin/echo "$s" }'
[[ $rc != 0 && -z $out && $err == *SB4025* ]]; chk dd101.10

# dd101.11 the carve-out does not weaken arity: `$(getenv)` in condition
# position is still SB4045, not a silently-admitted condition
run_sash 'str h=""
if h=$(getenv) { /bin/echo "$h" }'
[[ $rc != 0 && -z $out && $err == *SB4045* ]]; chk dd101.11

# dd101.12 ...same for a two-argument call (no DEFAULT arg — DD#101 rejected
# the DD#60 `$(arg N DEFAULT)` shape for getenv)
run_sash 'str h=""
if h=$(getenv HOME X) { /bin/echo "$h" }'
[[ $rc != 0 && -z $out && $err == *SB4045* ]]; chk dd101.12

# ── Redirects in a capture (DD#88 addendum, DG 2026-10-12) ─────────────
# A redirect on a captured command is fine SO LONG AS the capture pipe still
# has a writer. `ir::redirects_preserve_capture` simulates the list left to
# right; SB4075 rejects the cases that abandon the pipe. The rule is scoped to
# every `$( … )`, not just a condition — see cap4075.* below.

# 88.19 a redirect that leaves fd 1 alone: the capture still reads stdout
run_sash 'if x=$(/bin/sh -c "echo OUT; echo ERR >&2" 2>/dev/null) { attest x [[ -n $x ]]; /bin/echo "ok [$x]" }'
[[ $rc = 0 && $out = "ok [OUT]" && -z $err && $rerr != *ERR* ]]; chk 88.19

# 88.20 the `2>&1 >file` idiom — fd 2 takes the pipe, then fd 1 goes to the
# file, so the capture reads STDERR and the file gets stdout. This is the
# spelling DG named when settling the rule. The real exit code still reaches
# `$?` as the first action of the handling branch.
rm -f /tmp/sash_cc_sw
run_sash 'if x=$(/bin/sh -c "echo OUT; echo ERR >&2; exit 3" 2>&1 >/tmp/sash_cc_sw) {
  /bin/echo no
} else {
  /bin/echo "st=$?"
  /bin/echo "cap=[${x:!}]"
}'
[[ $rc = 0 && $out = $'st=3\ncap=[ERR]' && -z $err ]]; chk 88.20
[[ $(cat /tmp/sash_cc_sw) = OUT ]]; chk 88.21
rm -f /tmp/sash_cc_sw

# 88.22 a redirect on fd 0 cannot touch the capture
rm -f /tmp/sash_cc_in
printf 'from-stdin\n' > /tmp/sash_cc_in
run_sash 'if x=$(/bin/cat </tmp/sash_cc_in) { attest x [[ -n $x ]]; /bin/echo "ok [$x]" }'
[[ $rc = 0 && $out = "ok [from-stdin]" && -z $err ]]; chk 88.22
rm -f /tmp/sash_cc_in

# 88.23 a dup onto an unrelated fd keeps the pipe reachable, so it is allowed
run_sash 'if x=$(/bin/echo hi 3>&1) { attest x [[ -n $x ]]; /bin/echo "ok [$x]" }'
[[ $rc = 0 && $out = "ok [hi]" && -z $err ]]; chk 88.23

# 88.24 `- cmd` suppression composes with a surviving redirect
run_sash 'if x=$(- /bin/sh -c "echo OUT; echo ERR >&2" 2>/dev/null) { attest x [[ -n $x ]]; /bin/echo "ok [$x]" }'
[[ $rc = 0 && $out = "ok [OUT]" && -z $err ]]; chk 88.24

# 88.25 `while` takes the same form
run_sash 'int n = 0
while x=$(/bin/sh -c "echo t; exit $n" 2>/dev/null) {
  attest x [[ -n $x ]]
  /bin/echo "[$x]"
  n = 1
}'
[[ $rc = 0 && $out = "[t]" && -z $err ]]; chk 88.25

# 88.26 a pipeline stage may carry a redirect that spares the capture. Before
# this, `stmt_is_external_command` peeled `-`/`!` but NOT `Redirected`, so it
# disagreed with its own caller (which peels one on a bare command) and this
# drew a false SB4025.
run_sash 'if x=$(/bin/echo hi | /bin/sh -c "cat; echo E >&2" 2>/dev/null) { attest x [[ -n $x ]]; /bin/echo "ok [$x]" }'
[[ $rc = 0 && $out = "ok [hi]" && -z $err ]]; chk 88.26

# 88.27 an EARLIER stage's `>file` steals its own stdout from the next pipe,
# not from the capture — the last stage still feeds it
rm -f /tmp/sash_cc_st
run_sash 'if x=$(/bin/echo a >/tmp/sash_cc_st | /bin/echo b) { attest x [[ -n $x ]]; /bin/echo "ok [$x]" }'
[[ $rc = 0 && $out = "ok [b]" && -z $err && $(cat /tmp/sash_cc_st) = a ]]; chk 88.27
rm -f /tmp/sash_cc_st

# 88.28 a user function is still DD#88's reject, redirect or no — the function
# table check moved with the relaxation rather than being left behind. The
# message names the function, which is the one shape it is TRUE of.
run_sash 'fn f {
  /bin/echo hi
}
if x=$(f 2>/dev/null) { /bin/echo "$x" }'
[[ $rc != 0 && -z $out && $err == *SB4025* && $err == *"function's output"* ]]; chk 88.28

# ── SB4075: an abandoned capture, anywhere ─────────────────────────────
# Scoped to every `$( … )`, because `a=$(cmd >f)` is the same mistake as
# `if a=$(cmd >f)`: the capture can only ever produce "".

# cap4075.1 the condition case that started it. It used to draw the DD#88
# message — "capturing a function's output is not a condition" — with no
# function anywhere in the program, because `try_capture_condition` fell to
# `_ => None` and the caller had only one explanation for None.
rm -f /tmp/sash_cc_x
run_sash 'if x=$(/bin/echo hi >/tmp/sash_cc_x) { /bin/echo "$x" }'
[[ $rc != 0 && -z $out && $err == *SB4075* && $err != *"function's output"* ]]; chk cap4075.1

# cap4075.2 ...and a PLAIN assignment is the same fault
run_sash 'str a = $(/bin/echo hi >/tmp/sash_cc_x)
/bin/echo "${a:!}"'
[[ $rc != 0 && -z $out && $err == *SB4075* ]]; chk cap4075.2

# cap4075.3 ...as is a capture in argument position, with no assignment at all
run_sash '/bin/echo "[$(/bin/echo hi >/tmp/sash_cc_x)]"'
[[ $rc != 0 && -z $out && $err == *SB4075* ]]; chk cap4075.3

# cap4075.4 the OTHER order of the pair: `>file 2>&1` sends fd 2 to the file,
# not to the pipe, so unlike 88.20 nothing is left writing the capture
run_sash 'str a = $(/bin/echo hi >/tmp/sash_cc_x 2>&1)
/bin/echo "${a:!}"'
[[ $rc != 0 && -z $out && $err == *SB4075* ]]; chk cap4075.4

# cap4075.5 `&>file` desugars to a plain `>file` (the parser builds one Write
# on fd 1), so it is caught by the same simulation
run_sash 'str a = $(/bin/echo hi &>/tmp/sash_cc_x)
/bin/echo "${a:!}"'
[[ $rc != 0 && -z $out && $err == *SB4075* ]]; chk cap4075.5

# cap4075.6 a dup can abandon the pipe too — `1>&2` hands fd 1 to stderr
run_sash 'str a = $(/bin/echo hi 1>&2)
/bin/echo "${a:!}"'
[[ $rc != 0 && -z $out && $err == *SB4075* ]]; chk cap4075.6

# cap4075.7 a pipeline whose LAST stage abandons the pipe
run_sash 'str a = $(/bin/echo hi | /bin/cat >/tmp/sash_cc_x)
/bin/echo "${a:!}"'
[[ $rc != 0 && -z $out && $err == *SB4075* ]]; chk cap4075.7

# cap4075.8 the rule is about the capture having SOME writer, not about every
# statement having one: `b` still feeds it
rm -f /tmp/sash_cc_x
run_sash 'str a = $(/bin/echo a >/tmp/sash_cc_x; /bin/echo b)
/bin/echo "[${a:!}]"'
[[ $rc = 0 && $out = "[b]" && -z $err && $(cat /tmp/sash_cc_x) = a ]]; chk cap4075.8
rm -f /tmp/sash_cc_x

set -e
tally
