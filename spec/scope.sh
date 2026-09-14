#!/bin/bash
# spec/scope.sh — DD#86 variable scope model. Phase 1: the `str` keyword
# (declares a string, meaningful-zero "", parallel to `int`), so a string
# global can be declared at top level without `=""`. Later phases (structural
# scope, no-shadowing, dead-variable rejection) extend this file.
set -eu
. common.sh
setup scope
set +e
plan=53
echo "1..$plan" >&3

# 86.1 `str s` with no initializer declares a string, meaningful-zero ""
run_sash 'str s
/bin/echo "[$s]"'
[[ $rc = 0 && $out = "[]" && -z $err ]]; chk 86.1

# 86.2 `str s = v` initializes
run_sash 'str s = hello
/bin/echo "$s"'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 86.2

# 86.3 a `str` global is written frictionlessly from a function
run_sash 'str msg
fn set_msg { msg = hi }
set_msg
/bin/echo "$msg"'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 86.3

# 86.4 a `str` global declared-then-appended
run_sash 'str s = ab
s += cd
/bin/echo "$s"'
[[ $rc = 0 && $out = "abcd" && -z $err ]]; chk 86.4

# 86.5 `str` parallels `int` — both declare without assignment (0 / "")
run_sash 'int n
str s
/bin/echo "n=$n s=[$s]"'
[[ $rc = 0 && $out = "n=0 s=[]" && -z $err ]]; chk 86.5

# 86.6 a str global read in one function, written in another (method pattern)
run_sash 'str state = start
fn advance { state = done }
fn show { /bin/echo "$state" }
show
advance
show'
[[ $rc = 0 && $out = $'start\ndone' && -z $err ]]; chk 86.6

# ── Phase 3: no shadowing (SB5049) ───────────────────────────────
# A typed re-declaration inside a function of a top-level global is a
# redeclaration error — a bare assignment writes the global instead.

# 86.7 `int` re-declaration of a global inside a function → SB5049
run_sash 'int x = 1
fn f { int x = 9; /bin/echo $x }
f'
[[ $rc != 0 && -z $out && $err == *SB5049* ]]; chk 86.7

# 86.8 `str` re-declaration of a global inside a function → SB5049
run_sash 'str s = hi
fn f { str s = bye; /bin/echo $s }
f'
[[ $rc != 0 && -z $out && $err == *SB5049* ]]; chk 86.8

# 86.9 `const` re-declaration of a global inside a function → SB5049
run_sash 'int g = 5
fn f { const int g = 9; /bin/echo $g }
f'
[[ $rc != 0 && -z $out && $err == *SB5049* ]]; chk 86.9

# 86.10 a bare assignment to a global inside a function is NOT a shadow
run_sash 'int n = 0
fn bump { n = $((n + 1)) }
bump
bump
/bin/echo "n=$n"'
[[ $rc = 0 && $out = "n=2" && -z $err ]]; chk 86.10

# 86.11 a typed declaration of a NEW (non-global) name inside a function is a
# fresh function-local — not a shadow
run_sash 'fn f { int y = 9; /bin/echo "y=$y" }
f'
[[ $rc = 0 && $out = "y=9" && -z $err ]]; chk 86.11

# 86.12 shadowing also caught for a collection type (`array`)
run_sash 'array a = (1 2)
fn f { array a = (3 4); /bin/echo ${a[0]} }
f'
[[ $rc != 0 && -z $out && $err == *SB5049* ]]; chk 86.12

# ── Phase 4: dead-variable rejection (SB5050) ────────────────────
# The dual of SB5031: a variable assigned but never read is an error.

# 86.13 a global assigned but never read → SB5050
run_sash 'int n = 5
/bin/echo hi'
[[ $rc != 0 && -z $out && $err == *SB5050* ]]; chk 86.13

# 86.14 a misspelled write target is caught from the write side (the DD#86
# `coutn` example): the typo'd local is assigned but never read
run_sash 'str count = "0"
fn inc { coutn = count }
inc
/bin/echo "$count"'
[[ $rc != 0 && -z $out && $err == *SB5050* ]]; chk 86.14

# 86.15 a read makes the variable live (no SB5050)
run_sash 'int n = 5
/bin/echo "n=$n"'
[[ $rc = 0 && $out = "n=5" && -z $err ]]; chk 86.15

# 86.16 `: $v` acknowledges a deliberately-unused write (DD#81 idiom) — a read
run_sash 'int n = 5
: $n
/bin/echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 86.16

# 86.17 an unused `for` loop variable is dead (DG: for-vars are ordinary vars)
run_sash 'for i in 1 2 3; do /bin/echo hi; done'
[[ $rc != 0 && $err == *SB5050* ]]; chk 86.17

# 86.18 a read `for` loop variable is live
run_sash 'for i in 1 2 3; do /bin/echo "$i"; done'
[[ $rc = 0 && $out = $'1\n2\n3' && -z $err ]]; chk 86.18

# 86.19 a global written in one function and read in another is live (the
# method pattern — a program-wide read counts)
run_sash 'str state = ""
fn set_it { state = hi }
fn show { /bin/echo "$state" }
set_it
show'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 86.19

# 86.20 dead-var yields to a more-specific error: a tainted unread copy still
# reports the taint (SB5020), not SB5050 (DG: more-specific error wins)
run_sash 'y = $(getenv HOME)
z = $y'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 86.20


# ═══════════════ DD#98: `_` is the don't-care binder ═══════════════
# Discarding a value used to require satisfying two rules at once — DD#86's
# dead-variable gate (SB5050: every variable must be read) and DD#24 taint (a
# read target is tainted, so any use needs attest). The documented escape
# hatch `: $v` is itself a *use*, so on a standalone `read` it hit the SSA
# taint backstop; the only thing that compiled was `attest a p?` — asserting a
# property of a value you are explicitly throwing away.
# `_` is write-only: exempt from SB5050, unobservable (so taint-safe by
# construction), repeatable, and reading it in ANY form is SB5056.

# dc.1 a `_` read target needs no acknowledgment and no attest. (DD#99 stage 6
# removed multi-target read, so the binder list this used is gone; `_` is
# unchanged and still valid wherever a binder appears.)
run_sash '/usr/bin/printf "xx\n" | read _
echo "ok"'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk dc.1

# dc.2 `_` still BINDS — the array is filled and discarded, not skipped
run_sash 'echo "aa bb cc" | read _
echo "ok"'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk dc.2

# dc.3 `_` may repeat — two binders in one program, each independently bound
# and discarded. (It used to repeat within a single multi-target read binder
# list; DD#99 stage 6 removed that form, so repetition is shown across
# binders instead.)
run_sash 'array a=(p q)
for _ in @a { echo tick }
for _ in @a { echo tock }
echo "ok"'
[[ $rc = 0 && $out = $'tick\ntick\ntock\ntock\nok' && -z $err ]]; chk dc.3

# dc.4 reading `$_` is an error — this is what makes the taint exemption safe
# rather than a laundering hole (`read _ x; y=$_` must not work)
run_sash 'echo "aa bb" | read _
echo "$_"'
[[ $rc != 0 && -z $out && $err == *SB5056* ]]; chk dc.4

# dc.5 …in the braced form too
run_sash 'echo "aa bb" | read _
echo "${_}"'
[[ $rc != 0 && -z $out && $err == *SB5056* ]]; chk dc.5

# dc.6 …as a subscript cell
run_sash 'echo "aa" | readline _
echo "${_[0]}"'
[[ $rc != 0 && -z $out && $err == *SB5056* ]]; chk dc.6

# dc.7 …and as a splat
run_sash 'array _=(a b)
/bin/echo @_'
[[ $rc != 0 && -z $out && $err == *SB5056* ]]; chk dc.7

# dc.8 an unused `for` variable named `_` is not dead
run_sash 'for _ in a b { echo hi }'
[[ $rc = 0 && $out = "hi
hi" && -z $err ]]; chk dc.8

# dc.9 a plain assignment to `_` discards a tainted capture, no attest needed
run_sash '_=$(/usr/bin/printf "secret")
echo done'
[[ $rc = 0 && $out = "done" && -z $err ]]; chk dc.9

# dc.10 an `_` attest-all binder is exempt from "validate BOTH key and value" —
# discarding the key is the point
run_sash 'map m=([A]=1 [B]=2)
attest _ v in m [[ -n $v ]]
echo ${m[A]}'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk dc.10

# dc.11 an ordinary `_`-PREFIXED name is unaffected — still a real variable,
# still subject to SB5050
run_sash 'int _n = 5
/bin/echo ok'
[[ $rc != 0 && $err == *SB5050* ]]; chk dc.11

# dc.12 …and reads normally when it is read
run_sash 'int _n = 5
/bin/echo "n=$_n"'
[[ $rc = 0 && $out = "n=5" && -z $err ]]; chk dc.12

# dc.13 the SB5056 message names the binder and says why
run_sash 'echo "a" | readline _
echo "$_"'
[[ $rc != 0 && $err == *SB5056* && $err == *"cannot be read"* && $err == *"care binder"* ]]; chk dc.13

# dc.14 a BARE `_` in arithmetic is a read too. It is an `ArithExpr::Var`, not
# an expansion, so the expression walkers never surfaced it — it was caught
# only by the taint backstop, with no SB code and no explanation.
run_sash 'echo "1 2" | read _ x
attest x p?
(( y = _ ))
echo "$y $x"'
[[ $rc != 0 && -z $out && $err == *SB5056* ]]; chk dc.14

# dc.15 …and in a value-position `$(( … ))`
run_sash 'echo "1 2" | read _ x
attest x p?
y=$(( _ + 1 ))
echo "$y $x"'
[[ $rc != 0 && -z $out && $err == *SB5056* ]]; chk dc.15

# 86.21 `=`-leading word after a command is an argument, not an assignment.
# `=` binds as assignment only glued-both (`x=5`) or spaced-both (`msg = hi`);
# spaced-left/glued-right is a plain word, as in bash.
run_sash 'echo =a'
[[ $rc = 0 && $out = "=a" && -z $err ]]; chk 86.21

# 86.22 glued assignment still binds
run_sash 'int x=5
echo $x'
[[ $rc = 0 && $out = "5" && -z $err ]]; chk 86.22

# 86.23 DD#100 supersedes the old reading here: at a *user-function* call a
# `=`-leading word is a collection argument, not a plain word. `nums` is not a
# collection, so it is SB5066. (86.21's `echo =a` is a builtin and still a
# plain word; `f "=nums"` still passes the literal — see collparam.sh 100.21.)
run_sash 'fn f { echo ${args[0]} }
f =nums'
[[ $rc != 0 && -z $out && $err == *SB5066* ]]; chk 86.23

# 86.24 spaced-both at command position is still the DD#86 write (here the
# target is undeclared, so it reaches the dead-variable check, not `echo`)
run_sash 'echo = a'
[[ $rc != 0 && -z $out && $err == *SB5050* ]]; chk 86.24

# ── DD#86 (2026-09-11): `const` in a loop body, and `fn` placement ──────────

# 86.25 SB5072: DD#86 rejected loop-variable auto-scoping, so a loop body
# introduces no scope. A `const` there is one binding re-executed — a const
# reassigned. It was already rejected, but only by the spanless SSA backstop
# (reported at 1:1); this pins the deliberate, spanned rejection.
run_sash 'int n=0
while (( n < 2 )) {
  (( n = n + 1 ))
  const r="x"
  echo "$r"
}'
[[ $rc != 0 && -z $out && $err == *SB5072* ]]; chk 86.25

# 86.26 same for `for` — the static form unrolls, so the repeated binding is
# literal there
run_sash 'for i in 1 2 { const r="x"; echo "$r" }'
[[ $rc != 0 && -z $out && $err == *SB5072* ]]; chk 86.26

# 86.27 the diagnostic is spanned — not the 1:1 backstop it used to be
run_sash 'for i in 1 2 { const limit="x"; echo "$limit" }'
[[ $rc != 0 && $err != *:1:1:* && $err == *SB5072* ]]; chk 86.27

# 86.28 control: declaring the const BEFORE the loop is the fix, and works
run_sash 'const r="x"
for i in 1 2 { echo "$r$i" }'
[[ $rc = 0 && $out = "x1
x2" && -z $err ]]; chk 86.28

# 86.29 control: a non-const binding in a loop body is unaffected
run_sash 'for i in 1 2 { str r = "x"; echo "$r$i" }'
[[ $rc = 0 && $out = "x1
x2" && -z $err ]]; chk 86.29

# 86.30 control: a `const` inside a *function* body is still fine — a function
# body is entered once per call, not re-executed in place. (This is the case
# DD#86 phase 2 had to fix when `readonly_vars` leaked across the two-pass
# function lowering; it must stay fixed.)
run_sash 'fn f { const r="x"; echo "$r" }
f'
[[ $rc = 0 && $out = "x" && -z $err ]]; chk 86.30

# --- 86.31-86.38: SB5049 same-scope redeclaration -----------------------
# DD#86 gives one name one declaration per scope. SB5049 used to enforce only
# the CROSS-scope half (a typed declaration inside a function of a top-level
# global), so at top level a name could be declared twice, have its type
# changed, or silently acquire `const`. Journal 310.

# 86.31 the same declaration twice in one scope
run_sash 'str x="a"
str x="b"
echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB5049* ]]; chk 86.31

# 86.32 a redeclaration that CHANGES the type. This was the worst of the
#       accepted cases: the variable's contract changed with no diagnostic.
run_sash 'str x="a"
int x=1
echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB5049* ]]; chk 86.32

# 86.33 a redeclaration that ACQUIRES const — a mutable variable becoming
#       immutable partway down the file, silently.
run_sash 'str x="a"
const x="b"
echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB5049* ]]; chk 86.33

# 86.34 control: two SIBLING functions may each declare the same name. These
#       are genuinely different scopes; forbidding redeclaration "regardless
#       of scope" would have broken this.
run_sash 'fn f { str x="a"; echo "$x" }
fn g { str x="b"; echo "$x" }
f
g'
[[ $rc = 0 && $out = "a
b" && -z $err ]]; chk 86.34

# 86.35 control: a `for` variable is not a declaration, so repeating one is
#       untouched by the rule.
run_sash 'for x in 1 2 { echo "$x" }
for x in 3 4 { echo "$x" }'
[[ $rc = 0 && $out = "1
2
3
4" && -z $err ]]; chk 86.35

# 86.36 control: one declaration introducing two DISTINCT names is not a
#       redeclaration (the check is per name, not per statement).
run_sash 'str a="1" b="2"
echo "$a$b"'
[[ $rc = 0 && $out = "12" && -z $err ]]; chk 86.36

# 86.37 the top level and the DD#56 END body are ONE scope — END declarations
#       are globals — so a name declared in both is declared twice.
run_sash 'str x="a"
echo "$x"
END { str x="b" }'
[[ $rc != 0 && -z $out && $err == *SB5049* ]]; chk 86.37

# 86.38 control flow has no scope of its own (DD#86), so two branches each
#       declaring the same name are two declarations of one global, not two
#       locals. Declare once above the `if` and assign in the branches.
run_sash 'str c="y"
if [[ $c == y ]] { str x="a" } else { str x="b" }
echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB5049* ]]; chk 86.38

set -e
tally
