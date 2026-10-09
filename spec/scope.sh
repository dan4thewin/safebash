#!/bin/bash
# spec/scope.sh — DD#86 variable scope model. Phase 1: the `str` keyword
# (declares a string, meaningful-zero "", parallel to `int`), so a string
# global can be declared at top level without `=""`. Later phases (structural
# scope, no-shadowing, dead-variable rejection) extend this file.
set -eu
. common.sh
setup scope
set +e
plan=100
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

# 86.24 spaced-both at command position is still the DD#86 write -- which is
# what the diagnostic proves: the target is `echo`, and DD#125 reserves it, so
# this is SB5092 rather than the SB5011 an ordinary command would get
run_sash 'echo = a'
[[ $rc != 0 && -z $out && $err == *SB5092* && $err == *'`echo`'* ]]; chk 86.24

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

# ── DD#86: a binding fixes a variable's type ──────────────────────────────
# A `for` header BINDS its variable exactly as an assignment does, and a
# variable does not change type. Both rules were unenforced: `for` was never
# type-checked at all, and a declaration was collected program-wide so it
# reached BACKWARDS and retyped reads above it.

# 86.39 the bug: a loop over string words into an `int` variable was accepted
#       and silently bound 0 each iteration, while the equivalent bare
#       assignment `x=hello` was SB4030.
run_sash 'int x=5
for x in a b {
  echo "$x"
}'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 86.39

# 86.40 same through a splat: the items come from a string array.
run_sash 'int x=5
array a=(p q)
for x in @a {
  echo "$x"
}'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 86.40

# 86.41 and inside a function, where the declaration is a local.
run_sash 'fn f {
  int n=5
  for n in a b {
    echo "$n"
  }
}
f'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 86.41

# 86.42 DG: "if I declare a variable and then use that in the for loop, it
#       should work using the existing variable." Types agree, so it does —
#       and the binding survives the loop.
run_sash 'str t="z"
for t in a b {
  echo "$t"
}
echo "after=$t"'
[[ $rc = 0 && $out == "a
b
after=b" && -z $err ]]; chk 86.42

# 86.43 an `int` loop variable over numeric literals is a type match.
run_sash 'int x=5
for x in 1 2 {
  echo "$x"
}'
[[ $rc = 0 && $out == "1
2" && -z $err ]]; chk 86.43

# 86.44 the common case — an undeclared loop variable — is introduced by the
#       header as a string, because a word list binds words.
run_sash 'for i in a b {
  echo "$i"
}'
[[ $rc = 0 && $out == "a
b" && -z $err ]]; chk 86.44

# 86.45 a `str` loop variable takes numeric words: storing "1" in a string is
#       not a type change, exactly as `s=42` on a `str` is not.
run_sash 'str s="q"
for s in 1 a {
  echo "$s"
}'
[[ $rc = 0 && $out == "1
a" && -z $err ]]; chk 86.45

# 86.46 DG: "if another loop uses a variable from the prior loop, it should
#       work." Untyped loop variables are strings, so differing item shapes
#       are not a type change.
run_sash 'for y in 1 2 {
  echo "$y"
}
for y in a b {
  echo "$y"
}'
[[ $rc = 0 && $out == "1
2
a
b" && -z $err ]]; chk 86.46

# 86.47 the retroactive-retype bug (SB5082). This printed `0` then `7`: the
#       declaration below reached back and retyped the read above it, so the
#       program reported a value it never wrote.
run_sash 'x=hello
echo "$x"
int x=7
echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB5082* ]]; chk 86.47

# 86.48 the same with a numerically-valid value, which is why it hid: this
#       printed `5` then `7` and looked correct.
run_sash 'x=5
echo "$x"
int x=7
echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB5082* ]]; chk 86.48

# 86.49 `(( x = 7 ))` introduces an int by assignment, so a later `str`
#       declaration is the same retype. This printed `7` then `0`.
run_sash '(( x = 7 ))
echo "$x"
str x="a"
echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB5082* ]]; chk 86.49

# 86.50 a `for` header binds, so it introduces the name too.
run_sash 'for z in 1 2 {
  echo "$z"
}
int z=7
echo "$z"'
[[ $rc != 0 && -z $out && $err == *SB5082* ]]; chk 86.50

# 86.51 the legal order is unaffected: declare, then assign.
run_sash 'int x=5
x=7
echo "$x"'
[[ $rc = 0 && $out = "7" && -z $err ]]; chk 86.51

# 86.52 a name introduced by assignment and never declared stays legal — a
#       variable may still be instantiated by assignment.
run_sash 'x=hello
echo "$x"'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 86.52

# 86.53 `(( x = 7 ))` yields a genuine int, usable in arithmetic. This is the
#       way to introduce an int without a declaration.
run_sash '(( x = 7 ))
echo "$(( x + 1 ))"'
[[ $rc = 0 && $out = "8" && -z $err ]]; chk 86.53

# 86.54 a bare assignment instantiates a STRING, which is why a later `int`
#       declaration is a type change: arithmetic on it is SB4030.
run_sash 'x=5
echo "$(( x + 1 ))"'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 86.54

# 86.55 scope still bounds the rule — an assignment in one function does not
#       stop another function declaring that name.
run_sash 'fn f {
  y=1
  echo "$y"
}
fn g {
  int y=2
  echo "$y"
}
f
g'
[[ $rc = 0 && $out == "1
2" && -z $err ]]; chk 86.55

# 86.56 a word-list loop variable is a STRING, so using it bare in arithmetic
#       is SB4030 exactly as `x=10; (( sum += x ))` is. This was accepted
#       before only because such a variable had no type at all — cf.f5 relied
#       on it, and was rewritten to the arithmetic header (DG, 2026-09-17).
run_sash 'int sum=0
for x in 10 20 30 {
  (( sum += x ))
}
echo "$sum"'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 86.56

# 86.57 the escape hatch SB4030 names works here like anywhere else: a
#       string→int conversion carrying a disposition is legal.
run_sash 'int sum=0
for x in 10 20 30 {
  (( sum += ${x:-0} ))
}
echo "$sum"'
[[ $rc = 0 && $out = "60" && -z $err ]]; chk 86.57

# 86.58 and the arithmetic header binds a genuine int, needing no disposition.
#       Nothing is lost: the word-list header is for words, this one is for
#       numbers.
run_sash 'int sum=0
for (( i = 10; i < 40; i += 10 )) {
  (( sum += i ))
}
echo "$sum"'
[[ $rc = 0 && $out = "60" && -z $err ]]; chk 86.58

# ── A name does not change type, and the bulk binders are bound by that ──
# DD#86: "a name's type is fixed at its first introduction, whether that is a
# declaration or a binding." `read`/`argv`/3-arg `match`|`matchall` introduce
# a collection, so pointing one at a name already bound as a scalar is a
# rebinding and must be refused. It used to be accepted, which is how a
# `SashTable*` ended up in a scalar slot (see types.sh 75.101); over an `int`
# target it was a compiler ICE.

# 86.59 a declared `str` cannot be refilled as a collection
run_sash 'str x=hi
/bin/echo $x
read x'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 86.59

# 86.60 an `int` target — this one ICEd ("expected string var, got int")
run_sash 'int x=1
/bin/echo $x
read x'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err != *ICE* ]]; chk 86.60

# 86.61 a bare assignment binds the type just as a declaration does
run_sash 'x=hi
/bin/echo $x
argv x'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 86.61

# 86.62 so does a `for` header (the third non-declaring binder)
run_sash 'for x in a b { /bin/echo $x }
read x'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 86.62

# 86.63 the 3-argument `match` capture target is bound by the same rule
run_sash 'str x=hi
/bin/echo $x
match "abc" "b" x'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 86.63

# 86.64 control: filling a name already declared a collection is not a
# rebinding — the type does not change
run_sash 'array x=(a)
argv x
attest e in x [[ -n $e ]]
echo "${x[1]}"' p q
[[ $rc = 0 && $out = "p" && -z $err ]]; chk 86.64

# 86.65 control: two bulk binders on one name agree on the type
run_sash 'argv x
argv x
attest e in x [[ -n $e ]]
echo "${x[1]}"' p q
[[ $rc = 0 && $out = "p" && -z $err ]]; chk 86.65

# ── A `$( )` body is an anonymous function, not a subshell ──
# DG, 2026-09-28: "sash doesn't have subshells - so changes to globals, for
# example, survive unless a fork is required […] a `$( )` capture could be
# modeled as an anonymous function; all new variables would be local."
# The compiler used to synthesise a save/restore around a capture body. That
# contradicted DD#86 (a global is frictionlessly writable from anywhere), it
# contradicted *itself* (`$(f)` where `f` writes a global always let the write
# survive), and the save set was not total, so a subscript write escaped it
# anyway. These are deliberate bash divergences.

# 86.66 a write to an existing global inside the body persists
run_sash 'str x=one
str s=$(x=two; echo hi)
echo "$s $x"'
[[ $rc = 0 && $out = "hi two" && -z $err ]]; chk 86.66

# 86.67 the inline body and the function call now agree — this was the
# self-contradiction: same effect, two spellings, two answers
run_sash 'str x=one
fn f { x=fn; echo A }
str p=$(f)
str q=$(x=inline; echo B)
echo "$p$q $x"'
[[ $rc = 0 && $out = "AB inline" && -z $err ]]; chk 86.67

# 86.68 a subscript write persists too — the old save set only ever collected
# whole-variable assignments and declarations, so this escaped regardless
run_sash 'array a=(one)
str s=$(a[0]=two; echo hi)
echo "$s ${a[0]}"'
[[ $rc = 0 && $out = "hi two" && -z $err ]]; chk 86.68

# 86.69 a name first introduced inside the body is LOCAL to it — that is the
# other half of the anonymous-function model
run_sash 'str s=$(int y=5; echo hi)
echo "$s $y"'
[[ $rc != 0 && -z $out && $err == *SB5031* ]]; chk 86.69

# 86.70 a write nested inside a block in the body persists as well.
# The condition compares a variable, not `1 == 1`: SB5086 rejects a comparison
# the source text already decides, and once `$( … )` bodies joined the
# statement recursion that rule reaches inside one. The tautology here only
# ever compiled because nothing walked in.
run_sash 'str x=one
str g=1
str s=$(if [[ $g == 1 ]] { x=two }; echo hi)
echo "$s $x"'
[[ $rc = 0 && $out = "hi two" && -z $err ]]; chk 86.70

# ---- DD#125: a name the compiler already understands is not the user's ----
# The reserved set is keywords + BuiltinCmd names + INTRINSICS. Three groups,
# three spellings of the same error, so each group gets both a variable and a
# non-variable binding form.

# 125.1 a keyword as a variable. `in` is contextual -- it lexes as an ordinary
# word outside a `for`, so this used to compile.
run_sash 'int in=5; echo $(( in ))'
[[ $rc != 0 && -z $out && $err == *SB5092* && $err == *'`in`'* ]]; chk 125.1

# 125.2 the other contextual keyword
run_sash 'int do=5; echo $(( do ))'
[[ $rc != 0 && -z $out && $err == *SB5092* ]]; chk 125.2

# 125.3 a statement builtin as a variable
run_sash 'int cd=5; echo $(( cd ))'
[[ $rc != 0 && -z $out && $err == *SB5092* && $err == *'`cd`'* ]]; chk 125.3

# 125.4 a value intrinsic as a variable -- the case that motivated the rule:
# `$(( (status) ))` gave the intrinsic while `$status` gave the variable
run_sash 'int status=5; echo $(( status ))'
[[ $rc != 0 && -z $out && $err == *SB5092* ]]; chk 125.4

# 125.5 a value intrinsic as a function name. Before DD#125 this program had
# two answers for `getpid` depending on which call syntax was used.
run_sash 'fn getpid -> int { return 42 }
int n=$(getpid)
echo $n'
[[ $rc != 0 && -z $out && $err == *SB5092* && $err == *'`getpid`'* ]]; chk 125.5

# 125.6 a value builtin as a function name
run_sash 'fn sub { : }
echo hi'
[[ $rc != 0 && -z $out && $err == *SB5092* ]]; chk 125.6

# 125.7 a `for` variable
run_sash 'array a=(1 2)
for len in @a { echo $len }'
[[ $rc != 0 && -z $out && $err == *SB5092* && $err == *'`len`'* ]]; chk 125.7

# 125.8 a read target binds a name, so it is covered too
run_sash 'readline echo
echo $echo'
[[ $rc != 0 && -z $out && $err == *SB5092* && $err == *'`echo`'* ]]; chk 125.8

# 125.9 a collection declaration
run_sash 'array keys=(1 2); echo $(len keys)'
[[ $rc != 0 && -z $out && $err == *SB5092* && $err == *'`keys`'* ]]; chk 125.9

# 125.10 a map declaration
run_sash 'map join=([k]=v); echo ${join[k]}'
[[ $rc != 0 && -z $out && $err == *SB5092* ]]; chk 125.10

# 125.11 a bare assignment, not just a typed declaration
run_sash 'random=5
echo $random'
[[ $rc != 0 && -z $out && $err == *SB5092* ]]; chk 125.11

# 125.12 the reservation reaches inside a function body, not just the top level
run_sash 'fn f { int argc=3; echo $(( argc )) }
f'
[[ $rc != 0 && -z $out && $err == *SB5092* && $err == *'`argc`'* ]]; chk 125.12

# 125.13 SB5092 must not MASK a better diagnostic. `read -a arr -d :` is a
# four-argument `read` (DD#99 gave the family no flags), so SB5057 owns it;
# the target extractor hands SB5092 the `-d` flag's argument `:`. Reported
# from the earlier batch, SB5092 suppressed the arity error and asked the
# user to rename a delimiter.
run_sash 'array arr=(); read -a arr -d :'
[[ $rc != 0 && -z $out && $err == *SB5057* ]]; chk 125.13

# 125.14 an ordinary name that merely CONTAINS a reserved one is unaffected --
# the check is whole-name, not substring
run_sash 'int status_code=5; int cdx=2; echo $(( status_code + cdx ))'
[[ $rc = 0 && $out = "7" && -z $err ]]; chk 125.14

# 125.15 and a reserved name is still usable as a plain word argument -- the
# rule is about BINDING a name, not mentioning one
run_sash 'echo status len wait cd'
[[ $rc = 0 && $out = "status len wait cd" && -z $err ]]; chk 125.15


set -e
tally
