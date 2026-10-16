#!/bin/bash
# spec/functions.sh — function and positional parameter tests (DD#17, DD#18, DD#42, DD#46, DD#48)
set -eu
. common.sh
setup functions
plan=190
echo "1..$plan" >&3

set +e

# 17.1 $(func args) captures return value
run_sash 'fn greet { echo hello; }
x=$(greet)
echo $x'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 17.1

# 17.2 bare func args for side effects, discards return
run_sash 'fn greet { echo hello; }
greet'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 17.2

# 17.3 inline `$(f)` stdout capture in ARGUMENT position (was an ICE panic;
# string-func-capture-in-arg-ice, DG: lift-and-print). Prints greet's stdout,
# same as the two-step `s=$(greet); echo $s`.
run_sash 'fn greet { echo hi; }
echo $(greet)'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 17.3
# 17.4 inline capture concatenated with a literal argument
run_sash 'fn greet { echo world; }
echo "hello-$(greet)"'
[[ $rc = 0 && $out = "hello-world" && -z $err ]]; chk 17.4

# 18.1 DD#86: `local` keyword removed — a bare `local` is now an unknown command
run_sash 'fn f { local x=5; echo $x }
f'
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk 18.1

# 18.2 DD#86: `local fn` (DD#18 block-scoped functions) removed — `local` before
# `fn` is an unknown command
run_sash 'fn outer {
  local fn inner { echo inside }
}
outer'
[[ $rc != 0 && $err == *SB5011* ]]; chk 18.2

# 18.3 DD#86 (2026-09-11): a `fn` must be defined at the TOP LEVEL. A nested
# definition used to be accepted and hoisted to a global — which is what
# DD#86's no-block-scope model implies, but it reads as though the function
# were scoped to its parent. Rejected rather than documented.
run_sash 'fn outer { fn inner { echo in } }
outer
inner'
[[ $rc != 0 && -z $out && $err == *SB5071* ]]; chk 18.3

# 18.4 same rejection when the nested definition is used only inside its parent
run_sash 'fn outer {
  fn inner { echo inside }
  inner
}
outer'
[[ $rc != 0 && $err == *SB5071* ]]; chk 18.4

# 18.5 a `fn` inside control flow is rejected. This is the case that made the
# rule necessary: the branch is FALSE, never runs, and sash still defined `f`
# and printed A — where bash reports `f: command not found`.
run_sash 's=y; if [[ $s == n ]] { fn f { echo A } }
f'
[[ $rc != 0 && -z $out && $err == *SB5071* ]]; chk 18.5

# 18.6 the same divergence via `&&` — the left operand fails, so bash never
# defines the function; sash hoisted it.
run_sash 'false && fn f { echo A }
f'
[[ $rc != 0 && $err == *SB5071* ]]; chk 18.6

# 18.7 ... and as a pipeline stage, where bash runs the stage in a subshell so
# the definition cannot survive into the parent at all.
run_sash 'fn f { echo A } | cat
f'
[[ $rc != 0 && $err == *SB5071* ]]; chk 18.7

# 18.8 a `{ … }` group is not the top level either
run_sash '{ fn f { echo A } }
f'
[[ $rc != 0 && $err == *SB5071* ]]; chk 18.8

# 18.9 the taint bypass this rule closes by construction: `check_taint` defers
# the functions it finds in the top-level statement list, so one hidden inside
# control flow kept the definition-point walk and could read a global tainted
# afterwards. It compiled and leaked raw argv before SB5071.
run_sash 's=y; if [[ $s == y ]] { fn f { for v in @g { echo "$v" } } }
argv g
f'
[[ $rc != 0 && -z $out && $err == *SB5071* ]]; chk 18.9

# 18.10 control: the top-level spelling of 18.9 is still taint-checked (SB5020),
# so the rejection above is not hiding a lost check.
run_sash 'fn f { for v in @g { echo "$v" } }
argv g
f'
[[ $rc != 0 && $err == *SB5020* ]]; chk 18.10

# 18.11 control: an ordinary top-level `fn` calling another is unaffected
run_sash 'fn a { echo A }
fn b { a }
b'
[[ $rc = 0 && $out = "A" && -z $err ]]; chk 18.11

# 18.12 control: `;`-separated top-level statements are still top level
run_sash 'g=hi; fn f { echo $g }; f'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 18.12

# 42.1 $(getenv VAR) reads env var
run_sash 'x=$(getenv HOME)
attest x [[ $x =~ ^/ ]]
echo $x'
[[ $rc = 0 && -n $out && -z $err ]]; chk 42.1

# 42.2 $(getenv VAR) result is tainted
run_sash 'x=$(getenv HOME); echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 42.2

# 42.3 $(arg N) reads program arg
run_sash 'x=$(arg 1)
attest x [[ $x =~ ^[a-z]+$ ]]
echo $x' hello
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 42.3

# 42.4 $(arg N) result is tainted (rejected without attest)
run_sash 'x=$(arg 1); echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 42.4

# 42.5 $(argc) returns C argc (1 + user-arg count)
run_sash 'echo $(argc)' a b c
[[ $rc = 0 && $out = "4" && -z $err ]]; chk 42.5

# 42.6 out-of-range $(arg N) aborts at runtime
run_sash 'x=$(arg 99); attest x [[ -n $x ]]; echo $x' hello
[[ $rc != 0 && -z $out ]]; chk 42.6

# 42.7 $(getpid) — in-process intrinsic, positive int, CLEAN capture (usable
#      without a attest).
run_sash 'x=$(getpid); if [[ $x =~ ^[0-9]+$ ]] { echo Y } else { echo N }'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk 42.7
# 42.8 $(random) — non-negative i64 (0..i64::MAX), clean capture
run_sash 'a=$(random); if [[ $a =~ ^[0-9]+$ ]] { echo Y } else { echo N }'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk 42.8
# 42.9 two $(random) draws differ (distinct entropy)
run_sash 'a=$(random); b=$(random); if [[ $a != $b ]] { echo Y } else { echo N }'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk 42.9

# 46.1 top-level $1..$N → compile error
run_sash 'echo $1'
[[ $rc != 0 && -z $out && -n $err ]]; chk 46.1

# 46.2 $1..$N inside function body accepted
run_sash 'fn f { echo $1; }
f hello'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 46.2

# 46.4 reading $K where K > argc → compile error
run_sash 'fn f { echo $3; }
f a'
[[ $rc != 0 && -z $out && -n $err ]]; chk 46.4

# 48.1 function reads $N but call provides fewer → error
run_sash 'fn f { echo $2; }
f one'
[[ $rc != 0 && -z $out && -n $err ]]; chk 48.1

# 48.2 removed (DD#75 §5, 2026-08-21): `shift` is dropped — it was a toy slice.
# Iteration uses `for x in @args`; a future array-slice op (post-beta research)
# subsumes the skip-N-keep-rest use.

# DD#27 — functions are inlined (no runtime call stack), so recursion can't be
# inlined: it is rejected (SB5037), EXCEPT a direct self tail-call (`return F …`)
# which lowers to a loop. Added 2026-08-21.
# 27.1 non-tail self-recursion → compile error
run_sash 'fn f { echo hi; f; }
f'
[[ $rc != 0 && -z $out && -n $err ]]; chk 27.1
# 27.2 mutual recursion → compile error
run_sash 'fn a { b; }
fn b { a; }
a'
[[ $rc != 0 && -z $out && -n $err ]]; chk 27.2
# 27.3 int tail recursion (accumulator) lowers to a loop and runs
run_sash 'fn fact -> int {
  int n=$(($1)); int acc=$(($2))
  if (( n <= 1 )); then return $(( acc )); fi
  int n1=$(( n - 1 )); int a1=$(( n * acc ))
  return $(( (fact n1 a1) ))
}
int r=$(( (fact 5 1) )); echo "$r"'
[[ $rc = 0 && $out = "120" && -z $err ]]; chk 27.3
# 27.4 void tail recursion (`return F $x`) loops, running the body each iteration
run_sash 'fn f {
  if (( $1 <= 0 )); then echo done; return 0; fi
  echo "T$1"
  int m=$(( $1 - 1 ))
  return f $m
}
f 3'
[[ $rc = 0 && $out = $'T3\nT2\nT1\ndone' && -z $err ]]; chk 27.4
# 27.5 a tail self-call is the ONLY allowed recursion — a non-tail call in the
# same function (before the tail return) still rejects
run_sash 'fn f -> int {
  if (( $1 <= 0 )); then return 0; fi
  int junk=$(( (f 0) ))
  return $(( (f 0) ))
}
int x=$(( (f 2) )); echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 27.5

# 67.1 bash `name() { }` function sugar ≡ fn name { } (DD#67)
run_sash 'greet() { echo hi; }
greet'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 67.1

# 67.2 `name()` sugar passes positionals like fn. DD#81: a fixed-arity function
# must read `args` contiguously — `: $1` acknowledges the unused first param.
run_sash 'pick() { : $1; echo $2; }
pick a b'
[[ $rc = 0 && $out = "b" && -z $err ]]; chk 67.2

# 67.3 `function name { }` keyword form also accepted
run_sash 'function g { echo G; }
g'
[[ $rc = 0 && $out = "G" && -z $err ]]; chk 67.3

# --- DD#74: `return N` drives the enclosing condition (predicate functions) ---

# 74.1 `return 0` makes `if f` take the then-branch (was silently "no")
run_sash 'is_ok() { return 0; }
if is_ok; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 74.1

# 74.2 `return 1` → else-branch
run_sash 'is_ok() { return 1; }
if is_ok; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk 74.2

# 74.3 `! f` negates the return value.
run_sash 'is_ok() { return 0; }
if ! is_ok; then echo neg; else echo pos; fi'
[[ $rc = 0 && $out = "pos" && -z $err ]]; chk 74.3

# 74.4 the return value drives the branch even when a prior [[ ]] disagrees —
#      [[ false ]] but the function falls through to `return 0` → then
run_sash 'f() { [[ -e /nonexistent_dd74 ]] && return 1
return 0 }
if f; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 74.4

# 74.5 meaningful codes beyond 1: `return 2` is false for the branch
run_sash 'f() { return 2; }
if f; then echo t; else echo f; fi'
[[ $rc = 0 && $out = "f" && -z $err ]]; chk 74.5

# 74.6 a return-based predicate in a while condition
run_sash 'int i=0
lt3() { if (( i < 3 )); then return 0; else return 1; fi }
while lt3; do echo $i; (( i++ )); done'
[[ $rc = 0 && $out = $'0\n1\n2' && -z $err ]]; chk 74.6

# --- DD#74 phase 2: `return <producer>` (command / [[ ]] / (( )) ) ---

# 74.7 `return [[ … ]]` returns the test's boolean
run_sash 'has() { v=ok; return [[ $v == ok ]] }
if has; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 74.7

# 74.8 `return [[ … ]]` — false test → else
run_sash 'has() { v=ok; return [[ $v == no ]] }
if has; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk 74.8

# 74.9 `return (( … ))` returns the arith truth
run_sash 'int x=5; int lo=1; int hi=9
in_range() { return (( x >= lo && x <= hi )) }
if in_range; then echo in; else echo out; fi'
[[ $rc = 0 && $out = "in" && -z $err ]]; chk 74.9

# 74.10 `return (( … ))` — false → else
run_sash 'int x=50
small() { return (( x < 10 )) }
if small; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk 74.10

# 74.11 `return CMD` returns the command's exit status; the command is
#       consumed by the return (never aborts)
run_sash 'has_root() { return /bin/grep -q root /etc/passwd }
if has_root; then echo found; else echo none; fi'
[[ $rc = 0 && $out = "found" && -z $err ]]; chk 74.11

# 74.12 `return CMD` with a failing command → false, no abort
run_sash 'never() { return /bin/false }
if never; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk 74.12

# --- DD#74 phase 3: a boolean-context function must definite-return ---

# 74.13 a function used in `if` must `return` on all paths — a bare trailing
#       command is no longer an implicit predicate → compile error
run_sash 'maybe() { echo hi }
if maybe; then echo yes; fi'
[[ $rc != 0 && -z $out && -n $err ]]; chk 74.13

# 74.14 the explicit-return form is accepted
run_sash 'ok() { return 0 }
if ok; then echo y; fi'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 74.14

# 74.15 a fall-through path (if without else) in a predicate → compile error
run_sash 'p() { v=ok; if [[ $v == ok ]]; then return 0; fi }
if p; then echo y; fi'
[[ $rc != 0 && -z $out && -n $err ]]; chk 74.15

# 74.16 if/else where both branches return → accepted
run_sash 'p() { v=ok; if [[ $v == ok ]]; then return 0; else return 1; fi }
if p; then echo y; fi'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 74.16

# 74.17 a void function (no return) called bare is still fine — only a
#       boolean-context use requires definite-return
run_sash 'greet() { echo hi }
greet'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 74.17

# 74.18 `! f` also requires f to be a predicate
run_sash 'maybe() { echo hi }
if ! maybe; then echo y; fi'
[[ $rc != 0 && -z $out && -n $err ]]; chk 74.18

# --- DD#74: `return` takes a COMPOSITE producer (pipeline / && / ||) ---
# The producer parse grabbed only the first command, so `return A | B` folded
# to `Pipeline([return A, B])` — the return was trapped in stage 0, the rest ran
# uncaught (SB4037 in a predicate; a runtime abort as a statement). `return` now
# absorbs the full pipeline / list / redirect composite (DG 2026-08-31).

# 74.19 `return <pipeline>` — the pipeline's status drives the condition (match)
run_sash 'has() { return /bin/echo abc | /bin/grep -q a }
if has; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 74.19

# 74.20 `return <pipeline>` — no match → false (and no abort — the return
#       consumes the pipeline)
run_sash 'has() { return /bin/echo abc | /bin/grep -q z }
if has; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk 74.20

# 74.21 `return A && B` — an and-list producer (both true → true).
run_sash 'both() { a=y; b=y; return [[ $a == y ]] && [[ $b == y ]] }
if both; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 74.21

# 74.22 `return A && B` — second false → false
run_sash 'both() { a=y; b=y; return [[ $a == y ]] && [[ $b == n ]] }
if both; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk 74.22

# 74.23 `return A || B` — an or-list producer (first false, second true → true)
run_sash 'either() { a=y; b=y; return [[ $a == n ]] || [[ $b == y ]] }
if either; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 74.23

# ── DD#81 function-call discipline: one name, one meaning ──────────────
# 81.1 dynamic shadowing rejected: a name used as a command, then defined as
# a function, is a collision (SB5041) — no silent position-based switch.
run_sash 'cat /etc/hostname
fn cat { echo W; }
cat /etc/hostname'
[[ $rc != 0 && -z $out && $err == *SB5041* ]]; chk 81.1

# 81.2 forward `$(f)` (function defined after the call) → SB5041 (was a
# codegen ICE; the $( ) gate now matches the plain-call gate).
run_sash 'echo $(f)
fn f { echo hi; }'
[[ $rc != 0 && -z $out && $err == *SB5041* ]]; chk 81.2

# 81.3 forward plain call → SB5041 (was a runtime `bug:` / 127).
run_sash 'f x
fn f { echo hi; }'
[[ $rc != 0 && -z $out && $err == *SB5041* ]]; chk 81.3

# 81.4 KEPT: a helper defined below its caller — `b` is only ever a function
# (called when `a` runs, after `b` is defined), never an external.
run_sash 'fn a { b; }
fn b { echo hi; }
a'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 81.4

# 81.5 KEPT: static override — define `fn cat` first, then use it; the name is
# the function throughout (call the real one via an absolute path).
run_sash 'fn cat { echo W; }
cat'
[[ $rc = 0 && $out = "W" && -z $err ]]; chk 81.5

# 81.6 shadowing through `$( )` is rejected too (same gate).
run_sash 'x=$(cat /etc/hostname)
fn cat { echo W; }
y=$(cat /etc/hostname)'
[[ $rc != 0 && -z $out && $err == *SB5041* ]]; chk 81.6

# 81.7 DD#81: `$(f arg)` passes positional args to the captured function
# (was a codegen ICE — the capture dropped the args and the body's `$1`/
# `${args[0]}` hit an undefined args array).
run_sash 'fn greet { echo "hi $1"; }
s=$(greet World)
echo $s'
[[ $rc = 0 && $out = "hi World" && -z $err ]]; chk 81.7

# 81.8 multi-arg capture, read via the `args` array
run_sash 'fn pair { echo "${args[0]}-${args[1]}"; }
s=$(pair a b)
echo $s'
[[ $rc = 0 && $out = "a-b" && -z $err ]]; chk 81.8

# 81.9 DD#81 Part B: too few args → SB5042
run_sash 'fn f { echo "$1$2"; }
f a'
[[ $rc != 0 && -z $out && $err == *SB5042* ]]; chk 81.9

# 81.10 too many args (fixed arity) → SB5043
run_sash 'fn f { echo "$1"; }
f a b'
[[ $rc != 0 && -z $out && $err == *SB5043* ]]; chk 81.10

# 81.11 a non-contiguous read (hole, rule a) → SB5044
run_sash 'fn f { echo "$1$3"; }
f a b c'
[[ $rc != 0 && -z $out && $err == *SB5044* ]]; chk 81.11

# 81.12 @args makes the function variadic — any count is accepted
run_sash 'fn f { for x in @args; do echo "$x"; done; }
f a b c'
[[ $rc = 0 && $out = $'a\nb\nc' && -z $err ]]; chk 81.12

# 81.13 arity is checked on the $() capture path too → SB5042
run_sash 'fn f { echo "$1"; }
s=$(f)
echo $s'
[[ $rc != 0 && -z $out && $err == *SB5042* ]]; chk 81.13

# 81.14 DD#75/DD#81: `$@` is dropped inside a function ($-sigil that splats) →
# SB5045; the splat is `@args`, a joined string is `$(join args)`.
run_sash 'fn f { echo "$@"; }
f a b'
[[ $rc != 0 && -z $out && $err == *SB5045* ]]; chk 81.14

# 81.15 DD#75/DD#81: `$*` inside a function is dropped too (was silently
# compiling to empty) → SB5045; universally rejected like `$@`.
run_sash 'fn f { echo "$*"; }
f a b'
[[ $rc != 0 && -z $out && $err == *SB5045* ]]; chk 81.15

# 81.16 the braced spelling `${*}` (like `${@}`) is the same splat → SB5045.
run_sash 'fn f { echo "${*}"; }
f a b'
[[ $rc != 0 && -z $out && $err == *SB5045* ]]; chk 81.16

# DD#81 addendum (2026-09-28, DG): counting the arguments is not accessing
# them. `$(len args)` / `$#` read the argument *count*, never an argument, so
# they contribute NOTHING to the inferred signature. Previously either one put
# the function in the whole-args set and silently erased its arity ceiling.

# 81.17 a body that only counts accesses no argument → Fixed{0,0}. SB5043, and
# the message already names the escape hatch.
run_sash 'fn n { echo "$#" }
n a b'
[[ $rc != 0 && -z $out && $err == *SB5043* && $err == *"@args"* ]]; chk 81.17

# 81.18 the `$(len args)` spelling is the same rule
run_sash 'fn n { echo "n=$(len args)" }
n a b'
[[ $rc != 0 && -z $out && $err == *SB5043* ]]; chk 81.18

# 81.19 ... and so is `$#` inside `$(( ))` (sig_scan_arith reaches it)
run_sash 'fn n { echo $(( $# * 2 )) }
n a b'
[[ $rc != 0 && -z $out && $err == *SB5043* ]]; chk 81.19

# 81.20 branching on the count is still not an access
run_sash 'fn n {
  if (( $# < 2 )); then echo few; else echo many; fi
}
n a'
[[ $rc != 0 && -z $out && $err == *SB5043* ]]; chk 81.20

# 81.21 zero arguments is a legal call to such a function — it really is
# Fixed{0,0}, not an error in itself, and the count it reports is 0.
run_sash 'fn n { echo "$#" }
n'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk 81.21

# 81.22 DD#81's no-op idiom declares the variadic intent and restores the count
run_sash 'fn n { : @args; echo "$#" }
n a b'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk 81.22

# 81.23 the same for `$(len args)`
run_sash 'fn n { : @args; echo "n=$(len args)" }
n a b c'
[[ $rc = 0 && $out = "n=3" && -z $err ]]; chk 81.23

# 81.24 and for the branch form
run_sash 'fn n {
  : @args
  if (( $# < 2 )); then echo few; else echo many; fi
}
n a
n a b c'
[[ $rc = 0 && $out = $'few\nmany' && -z $err ]]; chk 81.24

# 81.25 THE REGRESSION THIS RULE FIXES: a counted fixed-arity function keeps
# its ceiling. Before the addendum this compiled and silently discarded two
# arguments, printing "4 [p][d]".
run_sash 'fn f {
  str a=${args[0]}
  str b=${args[1]}
  echo "$(len args) [$a][$b]"
}
f p d x y'
[[ $rc != 0 && -z $out && $err == *SB5043* ]]; chk 81.25

# 81.26 the same body called correctly still counts at runtime — `len` works,
# it just does not decide the signature
run_sash 'fn f {
  str a=${args[0]}
  str b=${args[1]}
  echo "$(len args) [$a][$b]"
}
f p d'
[[ $rc = 0 && $out = "2 [p][d]" && -z $err ]]; chk 81.26

# 81.27 the canonical variadic loop is unaffected: `len` is the loop BOUND, and
# the dynamic index `${args[$i]}` inside the loop is what marks it variadic.
run_sash 'fn f {
  int n=$(len args)
  int i=0
  while (( i < n )); do
    echo "[${args[$i]}]"
    i=$(( i + 1 ))
  done
}
f a b c'
[[ $rc = 0 && $out = $'[a]\n[b]\n[c]' && -z $err ]]; chk 81.27

# 81.28 a `@args` splat marks variadic on its own, with no `len` anywhere
run_sash 'fn f { for x in @args; do echo "[$x]"; done }
f a b c'
[[ $rc = 0 && $out = $'[a]\n[b]\n[c]' && -z $err ]]; chk 81.28

# 81.29 `$(join args)` is a whole-args FOLD — it reads every element, so unlike
# `len` it does still imply variadic
run_sash 'fn f { echo "j=$(join args ,)" }
f a b c'
[[ $rc = 0 && $out = "j=a,b,c" && -z $err ]]; chk 81.29

# ── DD#74 addendum (DG 2026-09-07): the TAIL of an `&&`/`||` statement is not
# a predicate ──
# A function is a predicate when something branches on its status. In a bare
# `A || B` statement the FINAL clause's status is just the statement's status —
# nothing branches on it — so it is no more a predicate than a bare `f` is. The
# earlier clauses still are (they decide whether the next one runs), and inside
# an `if`/`while` CONDITION the whole list is a predicate, last clause included.
# 74.30 the reported case: a user function as the tail of `[[ ]] ||`
run_sash 'fn f { echo fallback; }
x=
[[ -n $x ]] || f'
[[ $rc = 0 && $out = "fallback" && -z $err ]]; chk 74.30
# 74.31 …and as the tail of `[[ ]] &&`
run_sash 'fn f { echo ran; }
x=y
[[ -n $x ]] && f'
[[ $rc = 0 && $out = "ran" && -z $err ]]; chk 74.31
# 74.32 …and after an ordinary command
run_sash 'fn f { echo fallback; }
false || f'
[[ $rc = 0 && $out = "fallback" && -z $err ]]; chk 74.32
# 74.33 the LEFT operand still is a predicate — its status decides whether the
# right side runs.
run_sash 'fn f { echo hi; }
f && echo yes'
[[ $rc != 0 && -z $out && $err == *SB4037* ]]; chk 74.33
# 74.34 …including a middle clause, which decides the one after it
run_sash 'fn f { echo hi; }
true && f && echo x'
[[ $rc != 0 && -z $out && $err == *SB4037* ]]; chk 74.34
# 74.35 inside an `if` CONDITION the whole list is a predicate — the last clause
# included (DG 2026-09-07).
run_sash 'fn f { echo hi; }
if true && f; then echo yes; fi'
[[ $rc != 0 && -z $out && $err == *SB4037* ]]; chk 74.35
# 74.36 …same for a `while` condition
run_sash 'fn f { echo hi; }
while true && f; do echo x; done'
[[ $rc != 0 && -z $out && $err == *SB4037* ]]; chk 74.36
# 74.37 the strongest form of the tail case: a BRANCHING function with no
# `return` on any path — exactly what SB4037 exists to reject in a predicate,
# and exactly what is fine as a tail.
run_sash 'fn f { if true; then echo a; else echo b; fi; }
x=
[[ -n $x ]] || f'
[[ $rc = 0 && $out = "a" && -z $err ]]; chk 74.37

# 74.65 / 74.66 — the two statement shapes `stmt_always_returns` reaches that
# no spec did. 74.33-74.37 all end a body in a Command or an If, so the
# REDIRECTED arm (which must peel the redirect and judge the statement under
# it) and the catch-all for every other statement kind were both cold. Both
# must answer "does not return", and a wrong answer is silent: SB4037 would
# simply not fire on a predicate that cannot produce a status.

# 74.65 a redirected tail is still not a `return`
run_sash 'fn f { echo hi >/dev/null; }
if f; then echo yes; fi'
[[ $rc != 0 && -z $out && $err == *SB4037* ]]; chk 74.65

# 74.66 a loop tail is not a `return` either (the catch-all arm)
run_sash 'fn f { while true; do :; done }
if f; then echo yes; fi'
[[ $rc != 0 && -z $out && $err == *SB4037* ]]; chk 74.66

# ------------------------------------------------- capture in a [[ ]] operand
# A `$(f)` capture used as a test operand reached codegen as a bare FuncCall
# and PANICKED ("ICE: FuncCall should have been lifted during transform").
# The assignment path lifts captures into a temp; the test path never did.
# DG: make it work, consistent with `v=$(f)`.

# ice.1 a string function's capture as a test operand
run_sash 'fn f() { echo x }
if [[ $(f) == x ]] { echo T } else { echo F }'
[[ $rc = 0 && $out = "T" && -z $err ]]; chk ice.1

# ice.2 …and it really compares the captured value, not a constant
run_sash 'fn f() { echo x }
if [[ $(f) == q ]] { echo T } else { echo F }'
[[ $rc = 0 && $out = "F" && -z $err ]]; chk ice.2

# ice.3 an `-> int` function's capture (the values.rs:84 sibling arm)
run_sash 'fn n() -> int { return 7 }
if [[ $(n) == 7 ]] { echo T } else { echo F }'
[[ $rc = 0 && $out = "T" && -z $err ]]; chk ice.3

# ice.4 a multi-statement `$(…)` body as an operand (the values.rs:90 arm);
# `v=$(echo a; echo b)` already worked, so the test position must agree
run_sash 'if [[ $(echo a; echo b) == "a
b" ]] { echo T } else { echo F }'
[[ $rc = 0 && $out = "T" && -z $err ]]; chk ice.4

# ice.5 a capture in a unary operand, and nested inside `&&`
run_sash 'fn f() { echo x }
if [[ -n $(f) && $(f) == x ]] { echo T } else { echo F }'
[[ $rc = 0 && $out = "T" && -z $err ]]; chk ice.5

# ice.6 the same in a `while` condition (a different lowering site), and the
# capture is re-evaluated each iteration because it sits in the condition block
run_sash 'fn f() { echo x }
int i=0
while [[ $(f) == x ]] { echo L; (( i = i + 1 )); if (( i > 1 )) { break } }'
[[ $rc = 0 && $out = "L
L" && -z $err ]]; chk ice.6


# ── a splat argument must SPLAT, not join ────────────────────────────────
# `@a` gave an external command three arguments but a user function ONE
# joined argument, and an array literal `(@a q)` two elements instead of
# four. Both go through `IndexedArray`, whose element loop had no splat arm,
# so a splat was lowered as a single value that stringifies as the joined
# array. Silently lossy: `("a b" "c")` and `(a b c)` both became "a b c", so
# an element containing a space was indistinguishable from two elements.

# sp.1 a function receives one argument per ELEMENT
run_sash 'array a=(x y z)
fn f { : @args; echo "n=$(len args)" }
f @a'
[[ $rc = 0 && $out = "n=3" && -z $err ]]; chk sp.1

# sp.2 …and they are the elements, in order
run_sash 'array a=(x y z)
fn f { for v in @args { echo "<$v>" } }
f @a'
[[ $rc = 0 && $out = $'<x>\n<y>\n<z>' && -z $err ]]; chk sp.2

# sp.3 element boundaries survive: two DIFFERENT arrays must not collapse to
# the same thing (this is the lossy half of the bug).
# DD#84 (2026-09-28): a bare `@two` can be empty, so an index read must be
# optional (`:-`) for the callee's floor to admit it — see 84.53-84.66.
run_sash 'array two=("a b" "c")
fn f { : @args; echo "n=$(len args) first=[${args[0]:-}]" }
f @two'
[[ $rc = 0 && $out = "n=2 first=[a b]" && -z $err ]]; chk sp.3

# sp.4 a splat inside an ARRAY LITERAL splices too
run_sash 'array a=(x y z)
array b=(@a q)
echo "$(len b):${b[0]}:${b[3]}"'
[[ $rc = 0 && $out = "4:x:q" && -z $err ]]; chk sp.4

# sp.5 a splat BEFORE and AFTER other elements keeps its position
run_sash 'array a=(m n)
array b=(p @a q)
echo "$(len b):${b[0]}:${b[1]}:${b[2]}:${b[3]}"'
[[ $rc = 0 && $out = "4:p:m:n:q" && -z $err ]]; chk sp.5

# sp.6 an EMPTY array splats to nothing (it must not contribute an element)
run_sash 'array e=()
array b=(p @{e:~} q)
echo "$(len b):${b[0]}:${b[1]}"'
[[ $rc = 0 && $out = "2:p:q" && -z $err ]]; chk sp.6

# sp.7 the external path is unchanged (control — it was always correct)
run_sash 'array a=(x y z)
/usr/bin/printf "<%s>" @{a:~}'
[[ $rc = 0 && $out = "<x><y><z>" && -z $err ]]; chk sp.7

# sp.8 an explicit argument list is unchanged (control)
run_sash 'fn f { : @args; echo "n=$(len args)" }
f x y z'
[[ $rc = 0 && $out = "n=3" && -z $err ]]; chk sp.8

# ── The same call in a PIPELINE (2026-09-28) ────────────────────────
# The 2026-09-10 fix moved `inline_function_call` and `lower_array_literal` to
# `arg_to_value`; the pipeline-stage path kept `expr_to_value`, so `f @a | g`
# still joined. Same bug, third position — these gate it.

# sp.9 a pipelined function receives one argument per ELEMENT
run_sash 'array a=(x y z)
fn f { : @args; echo "n=$(len args)" }
f @a | /bin/cat'
[[ $rc = 0 && $out = "n=3" && -z $err ]]; chk sp.9

# sp.10 …and element boundaries survive there too (the lossy half)
run_sash 'array two=("a b" "c")
fn f { : @args; echo "n=$(len args) first=[${args[0]:-}]" }
f @two | /bin/cat'
[[ $rc = 0 && $out = "n=2 first=[a b]" && -z $err ]]; chk sp.10

# sp.11 the elements arrive in order through a pipeline
run_sash 'array a=(x y z)
fn f { for v in @args; do echo "<$v>"; done }
f @a | /bin/cat'
[[ $rc = 0 && $out = $'<x>\n<y>\n<z>' && -z $err ]]; chk sp.11

# ── DD#27: recursion, and the one shape that is allowed ──
# These pin the examples in docs/GUIDE.md ("Functions, inlining, and
# recursion") and docs/sash.1 (FUNCTIONS AND RECURSION). Documented examples
# that nothing compiles are how a doc drifts into being wrong — GUIDE.md said
# flatly "SB5037 rejects recursion" while the accumulator form below had been
# compiling to a loop all along.

# rec.1 the documented accumulator factorial compiles and runs
run_sash 'fn fact -> int {
  int n=$(($1)); int acc=$(($2))
  if (( n <= 1 )); then return $(( acc )); fi
  int n1=$(( n - 1 )); int a1=$(( n * acc ))
  return $(( (fact n1 a1) ))
}
int r=$(( (fact 5 1) ))
echo $r'
[[ $rc = 0 && $out = 120 && -z $err ]]; chk rec.1

# rec.2 …and it is a loop, not a stack: a depth that would blow any inliner
run_sash 'fn count -> int {
  int n=$(($1)); int acc=$(($2))
  if (( n <= 0 )); then return $(( acc )); fi
  int n1=$(( n - 1 )); int a1=$(( acc + 1 ))
  return $(( (count n1 a1) ))
}
int r=$(( (count 100000 0) ))
echo $r'
[[ $rc = 0 && $out = 100000 && -z $err ]]; chk rec.2

# rec.3 a NON-tail self-call is SB5037 — the multiply happens after the call
# returns, so there is nothing to turn into a loop. This is what a direct
# translation of the textbook factorial produces.
run_sash 'fn f -> int {
  int n=$(($1))
  if (( n <= 1 )); then return 1; fi
  int m=$(( n - 1 ))
  return $(( n * (f m) ))
}
int r=$(( (f 5) ))
echo $r'
[[ $rc != 0 && $err == *SB5037* ]]; chk rec.3

# rec.4 mutual recursion is SB5037 too
run_sash 'fn a -> int {
  int n=$(($1))
  if (( n <= 0 )); then return 0; fi
  int m=$(( n - 1 ))
  return $(( (b m) ))
}
fn b -> int { int n=$(($1)); return $(( (a n) )) }
int r=$(( (a 3) ))
echo $r'
[[ $rc != 0 && $err == *SB5037* ]]; chk rec.4

# --- fr.* redirect on a user-function call (`f > out.txt`) ---
#
# SB4021 rejects a redirect on a compound command and tells the user to
# "wrap the compound in a function" — so `f > file` is the *sanctioned*
# spelling of a compound redirect, and it has to work. The body is a
# compound, so the scope opens once around the whole call and restores
# once: `>` must truncate a single time no matter how many commands the
# body runs (fr.2 is that differential).

frf=/tmp/sash_spec_fr_$$.txt
frin=/tmp/sash_spec_fr_in_$$.txt

# fr.1 the redirect reaches the body — stdout is empty, the file has it
run_sash "fn f { echo a }
f > $frf
echo done"
[[ $rc = 0 && $out = "done" && -z $err && $(cat $frf) = "a" ]]; chk fr.1

# fr.2 a multi-command body opens the scope ONCE: both lines land, and
# `>` truncates a single time (per-command attachment would keep only "b")
run_sash "fn f { echo a; echo b }
f > $frf"
[[ $rc = 0 && -z $out && -z $err && $(cat $frf) = "a
b" ]]; chk fr.2

# fr.3 `>>` appends across two separate scopes
rm -f $frf
run_sash "fn f { echo x }
f >> $frf
f >> $frf"
[[ $rc = 0 && -z $out && $(cat $frf) = "x
x" ]]; chk fr.3

# fr.4 the scope restores the fd — output after the call goes to stdout
run_sash "fn f { echo inside }
f > $frf
echo outside"
[[ $rc = 0 && $out = "outside" && $(cat $frf) = "inside" ]]; chk fr.4

# fr.5 `<` feeds the body's read from the file
printf 'alpha\0' > $frin
run_sash "fn f { read a; echo \"n=\$(len a)\" }
f < $frin"
[[ $rc = 0 && $out = "n=1" && -z $err ]]; chk fr.5

# fr.6 `2>` captures only stderr; stdout is untouched
run_sash "fn f { /bin/echo oops >&2; echo fine }
f 2> $frf"
[[ $rc = 0 && $out = "fine" && $(cat $frf) = "oops" ]]; chk fr.6

# fr.7 args reach the body through the scope
run_sash "fn f { echo \$1-\$2 }
f one two > $frf"
[[ $rc = 0 && -z $out && $(cat $frf) = "one-two" ]]; chk fr.7

# fr.8 scopes nest: the inner scope restores to the OUTER redirect, not
# to the caller's stdout (per-site slots — one shared slot would clobber)
run_sash "fn g { echo inner }
fn f { echo before; g > $frin; echo after }
f > $frf"
[[ $rc = 0 && -z $out && $(cat $frf) = "before
after" && $(cat $frin) = "inner" ]]; chk fr.8

# fr.9 a redirected call inside a loop opens and restores per iteration
run_sash "fn f { echo tick }
rm -f $frf
for i in 1 2 3 { f >> $frf; echo n\$i }"
[[ $rc = 0 && $out = "n1
n2
n3" && $(cat $frf) = "tick
tick
tick" ]]; chk fr.9

# fr.10 DD#55: a bare open failure aborts, body never runs (mirrors 55.24)
run_sash 'fn f { echo hi }
f > /nonexistent_dir_55/f
echo after'
[[ $rc != 0 && -z $out \
   && $rerr == *"/nonexistent_dir_55/f: No such file or directory"* ]]; chk fr.10

# fr.11 ... and an enclosing if-condition catches it → else branch, no
# diagnostic and no misroute (mirrors 55.25)
run_sash 'fn f { echo hi; return 0 }
if f > /nonexistent_dir_55/f { echo yes } else { echo no }'
[[ $rc = 0 && $out = "no" && -z $err && -z $rerr ]]; chk fr.11

# fr.12 `||` catches it too (mirrors 55.50)
run_sash 'fn f { echo hi; return 0 }
f > /nonexistent_dir_55/f || echo caught'
[[ $rc = 0 && $out = "caught" && -z $err && -z $rerr ]]; chk fr.12

# ---------------------------------- DD#74 boolean contexts for an `-> int` fn
# `check_condition_type` had exactly ONE caller — the direct `if` condition —
# so five of the six contexts DD#74 names accepted an int function silently.
# The hazard is not loud: `return 7` sets status 7, which is FALSY, so
# `while f` never looped and `if ! f` took the true branch.

# 74.38 `if f` — the context that was always checked
run_sash 'fn f -> int { return 7 }
if f; then echo a; fi'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 74.38

# 74.39 `elif` — a second condition on the same statement
run_sash 'fn f -> int { return 7 }
s=y
if [[ $s == n ]]; then echo a; elif f; then echo b; fi'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 74.39

# 74.40 `while f`
run_sash 'fn f -> int { return 7 }
while f; do break; done'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 74.40

# 74.41 `until f`
run_sash 'fn f -> int { return 7 }
until f; do break; done'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 74.41

# 74.42 `! f`
run_sash 'fn f -> int { return 7 }
if ! f; then echo a; fi'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 74.42

# 74.43 `f` as the LEFT operand of a bare `&&` — its status decides whether
# the right side runs, so it is a predicate (DD#74 addendum).
run_sash 'fn f -> int { return 7 }
f && echo a'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 74.43

# 74.44 a MIDDLE clause. `&&`/`||` are left-associative, so `a && f && c` is
# `(a && f) && c` — `f`'s status decides whether `c` runs. Reaching it needs
# the traversal to recurse into the left operand's own list.
run_sash 'fn f -> int { return 7 }
s=y
[[ $s == y ]] && f && echo c'
[[ $rc != 0 && -z $out && $err == *SB4030* ]]; chk 74.44

# 74.45 the TAIL of a bare `||` is NOT a boolean context (DD#74 addendum):
# nothing branches on it, it is the statement's own status. It must not draw
# SB4030 — the bare-call rule (SB4016) is what applies there.
run_sash 'fn f -> int { return 7 }
s=y
[[ $s == n ]] || f'
[[ $rc != 0 && -z $out && $err == *SB4016* && $err != *SB4030* ]]; chk 74.45

# --------------------------------------------- SB4016 advises what compiles
# SB4016 used to say "handle with if/||", and following that advice landed on
# SB4030. `function_non_void` is populated only by `analyze_function_returns`,
# which returns early for anything not `-> int`, so every name it names is
# int-typed and `if`/`||` was unconditionally wrong there.

# 74.46 the advice no longer names the forms that SB4030 rejects
run_sash 'fn f -> int { return 7 }
f'
[[ $rc != 0 && $err == *SB4016* && $err != *"if/||"* ]]; chk 74.46

# 74.47 ...and names `$((f))`, which compiles and yields the value
run_sash 'fn f -> int { return 7 }
int n=$(( f ))
echo "$n"'
[[ $rc = 0 && $out = "7" && -z $err ]]; chk 74.47

# 74.48 ...and `-`, which discards it
run_sash 'fn f -> int { return 7 }
- f
echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 74.48


rm -f $frf $frin

# ── DD#74: `return` must be given a producer or a status VALUE ──────────
# A reserved word, a bare `-`, or `=` is neither. They used to fall through
# to the simple-command path, which absorbed them as an *argument* of a
# `return` command and silently dropped them: the function returned 0 where
# bash reports "numeric argument required" (rc 2). Found via coverage —
# the SB4014 below was unreachable, which is why nothing caught it.
#
# Each check asserts the MESSAGE, not just SB4014: error recovery emits a
# follow-on `SB4014 expected expression` on the orphaned `}`, so asserting
# the code alone would pass without the guard ever firing.

# 74.49 a reserved word after `return`
run_sash 'f() {
return if
}
f'
[[ $rc != 0 && $err == *"return: expected a status producer"* ]]; chk 74.49

# 74.50 ...including one that is also a type keyword
run_sash 'f() {
return str
}
f'
[[ $rc != 0 && $err == *"return: expected a status producer"* ]]; chk 74.50

# 74.51 a bare `-` — `-` discards a status, `return` consumes one (cf. SB4019
# for the same clash between `!` and `-`)
run_sash 'f() {
return -
}
f'
[[ $rc != 0 && $err == *"return: expected a status producer"* ]]; chk 74.51

# 74.52 `return - CMD` additionally threw the command away WITHOUT running it
run_sash 'f() {
return - /bin/echo hi
}
f'
[[ $rc != 0 && -z $out && $err == *"return: expected a status producer"* ]]; chk 74.52

# 74.53 a bare `=`
run_sash 'f() {
return =
}
f'
[[ $rc != 0 && $err == *"return: expected a status producer"* ]]; chk 74.53

# 74.54 the guard must not over-reach: a non-keyword word is still a COMMAND
# producer, so it reports the command, not the producer error
run_sash 'f() {
return abc
}
f'
[[ $rc != 0 && $err == *SB5011* && $err != *"status producer"* ]]; chk 74.54

# 74.55 a literal status still works
run_sash 'f() {
return 7
}
f
echo "rc=$?"'
[[ $rc = 0 && $out = "rc=7" && -z $err ]]; chk 74.55

# 74.56 ...and so do the quoted / arithmetic status VALUE forms
run_sash 'f() {
return $(( 1 + 1 ))
}
f
echo "a=$?"
g() {
return "5"
}
g
echo "b=$?"'
[[ $rc = 0 && $out = "a=2
b=5" && -z $err ]]; chk 74.56

# 74.57 DD#77: `return`'s status is an INT sink, so a string operand's
# disposition uses the arith trigger ("not a valid integer"), not the word
# trigger (unset-or-empty, DD#65). s is set and non-empty but not numeric, so
# `:-` must supply the default. Native lowered the value as a *string* and then
# leniently atoi'd it, silently yielding 0 while the c backend yielded 7.
run_sash 'f() {
str s=abc
return ${s:-7}
}
f
echo "rc=$?"'
[[ $rc = 0 && $out = "rc=7" && -z $err ]]; chk 74.57

# 74.58 ...and `:?` must ABORT rather than silently return 0. This is the
# safety-relevant half: the author wrote an explicit "not an integer" guard.
run_sash 'f() {
str s=abc
return ${s:?not an int}
}
f
echo "rc=$?"'
[[ $rc = 1 && $rerr == *"not an int"* && $out != *rc=* ]]; chk 74.58

# 74.59 a VALID integer string converts to its own value, not to the default
run_sash 'f() {
str s=5
return ${s:-7}
}
f
echo "rc=$?"'
[[ $rc = 0 && $out = "rc=5" && -z $err ]]; chk 74.59

# 74.60 ...and passes the `:?` gate without aborting
run_sash 'f() {
str s=5
return ${s:?not an int}
}
f
echo "rc=$?"'
[[ $rc = 0 && $out = "rc=5" && -z $err ]]; chk 74.60

# 74.61 the disposition reaches the caller's control flow, not just `$?`
# (DD#74 keeps test_result in sync). The default is 1, not 0, so the two
# answers differ: converted -> 1 -> "no"; the old lenient atoi("abc") -> 0 ->
# "yes". A `:-0` here would have been vacuous.
run_sash 'f() {
str s=abc
return ${s:-1}
}
if f; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk 74.61

# 74.62 a predicate whose body is a `{ }` GROUP ending in `return`. The
# "does every path return?" walk has to descend into the group, or the group
# reads as a non-returning statement and the predicate is rejected (74.15).
run_sash 'p() {
{ return 0; }
}
if p; then echo y; else echo n; fi'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 74.62

# 74.63 …and through a REDIRECT on the `return` itself. Same walk, the other
# wrapper: `return /bin/true 2>/dev/null` is a `Redirected` whose body returns.
#
# This used to be spelled `return 0 2>/dev/null`. DG ruled 2026-10-11 that
# the literal-code form is nonsense — `return N` states the status outright,
# where a producer *derives* it, so there is nothing for the redirect to
# attach to (74.64). DD#74's "a trailing redirect, exactly as a normal
# statement would" is about the producer composite, which is what this spec
# needs anyway: the point is that the predicate walk sees through a
# `Redirected` to the `return` inside it.
run_sash 'p() {
return /bin/true 2>/dev/null
}
if p; then echo y; else echo n; fi'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 74.63

# 74.64 `return N` with a redirect is SB4046. `return N` writes nothing to any
# descriptor — it names the status directly — so the redirect could only
# create and truncate the file as a side effect.
#
# This spec used to assert the opposite, that `return 3 2>/dev/null` compiled
# and carried its value out to `$?`. It did, by SILENTLY DISCARDING the
# redirect: its own comment said "the attach step has to leave it alone",
# which is the drop written down and accepted rather than reported. The file
# was never created. That is one of the cases that made the wider rule.
run_sash 'f() {
return 3 2>/dev/null
}
f
echo "rc=$?"'
[[ $rc != 0 && -z $out && $err == *SB4046* && $err == *return* ]]; chk 74.64

# 74.64b the value still reaches `$?` without the redirect — SB4046 did not
# disturb what the old spelling was really testing.
run_sash 'f() {
return 3
}
f
echo "rc=$?"'
[[ $rc = 0 && $out = "rc=3" && -z $err ]]; chk 74.64b

# ── lift: a capture nested in a value must be materialized ──────────────
# `lift_int_func_calls` walks a value and captures every `$(f)` into a temp.
# It used to end in `other => other`, so it never descended into most of
# `SsaValue` — an unlifted FuncCall/IntFuncCall then reached codegen and
# panicked ("ICE: FuncCall should have been lifted during transform"). The
# walk is now total over `SsaValue`. Every program below is valid bash and
# crashed the compiler before that; the expected values match bash exactly.

# lift.1 a capture as an array element
run_sash 'f() { echo hi; }
array a=( $(f) )
echo "[${a[0]}]"'
[[ $rc = 0 && $out = "[hi]" && -z $err ]]; chk lift.1

# lift.2 a capture as a map value in the initializer
run_sash 'f() { echo hi; }
map m=([k]=$(f))
echo "[${m[k]}]"'
[[ $rc = 0 && $out = "[hi]" && -z $err ]]; chk lift.2

# lift.3 a capture is NOT a subscript index (DG, 2026-10-03: "I don't think
# arr[$(f)] is legal, but arr[$((f))] where f is an int returning fn seems
# fine"). This spec used to assert the opposite, and passed only because an
# assignment's VALUE was never walked by the subscript rules — the identical
# `echo "${arr[$(f)]}"` was SB4030 throughout. DD#80 requires the index to
# yield a number; a capture yields a string that happens to parse as one, at
# runtime, which is the guarantee DD#80 exists to make statically.
run_sash 'f() { echo 1; }
array arr=(p q)
str z=${arr[$(f)]}
echo "[$z]"'
[[ $rc != 0 && -z $out && $err == *SB4030* && $err == *"capture as an index"* ]]; chk lift.3
# lift.3b the legal form DG named. The advice in lift.3's diagnostic has to be
# a form that actually works: `${arr[$(( $(f) ))]}` is SB4041 (a capture is not
# an arithmetic operand either) and `int i=$(f)` is SB4030, so an int-returning
# function is the only route and the message says so.
run_sash 'fn f -> int { return 1 }
array arr=(p q)
str z=${arr[$(( (f) ))]}
echo "[$z]"'
[[ $rc = 0 && $out = "[q]" && -z $err ]]; chk lift.3b
# lift.3c the same index in an argument — the point of the fix is that the
# three positions now agree.
run_sash 'fn f -> int { return 1 }
array arr=(p q)
echo "[${arr[$(( (f) ))]}]"'
[[ $rc = 0 && $out = "[q]" && -z $err ]]; chk lift.3c

# lift.11 a capture assigned into a map cell
run_sash 'f() { echo hi; }
map m=([k]=old)
m[k]=$(f)
echo "[${m[k]}]"'
[[ $rc = 0 && $out = "[hi]" && -z $err ]]; chk lift.11

# lift.12 a capture appended to an array
run_sash 'f() { echo hi; }
array a=(p)
a+=( $(f) )
echo "[${a[1]}]"'
[[ $rc = 0 && $out = "[hi]" && -z $err ]]; chk lift.12

# 81.30 SB5043 names the whole accepted range, not its floor. `f` reads
# `${args[0]}` bare and `${args[1]:-d}` through a disposition, so DD#84 gives
# it Fixed{min=1,max=2} — one argument or two. The message used to be built
# from `min`, so a 3-argument call was told "takes 1 argument", which denies
# that `f a b` is legal. A diagnostic that states a false number is worse than
# one that states none.
run_sash 'fn f {
    /bin/echo ${args[0]} ${args[1]:-d}
}
f a b c'
[[ $rc != 0 && -z $out && $err == *SB5043* && $err == *"takes 1 to 2 arguments"* ]]; chk 81.30

# 81.31 the min == max case still reads as one number, via the same
# `arity_range` the SB5087 dynamic-count message already used
run_sash 'fn g {
    /bin/echo ${args[0]}
}
g a b'
[[ $rc != 0 && -z $out && $err == *SB5043* && $err == *"takes exactly 1 argument"* ]]; chk 81.31


# 81.32 a `$( … )` body shares the ENCLOSING function's arguments. This is the
# fact that makes `walk_scoped_stmts` right to carry the enclosing function's
# name into a body: `${args[0]}` inside the substitution really is `g`'s first
# argument, so a slot or an argument attributed to `g` is attributed truly. It
# is only the "am I at the top level" test that a body invalidates.
run_sash 'fn g {
    str s=$(/bin/echo ${args[0]})
    attest s [[ -n $s ]]
    echo "[$s]"
}
g hello'
[[ $rc = 0 && $out = "[hello]" && -z $err ]]; chk 81.32


# 81.33-81.36 a positional read inside a COMPOUND test still counts toward the
# signature. `sig_scan_test` recurses through `!`, `&&` and `||`; only the
# `Unary`/`Binary` leaves had ever been exercised, so the three recursive arms
# were carrying the whole of `fn f { [[ $1 == a && $2 == b ]] }` on an
# unverified claim. An arm that failed to recurse would not be loud — it would
# silently under-count the arity, and a call that omits the argument would
# compile and read an argument that was never passed.

# 81.33 `&&` records BOTH operands' positionals
run_sash 'fn f {
    [[ ${args[0]} == x && ${args[1]} == y ]] && echo both
}
f x'
[[ $rc != 0 && -z $out && $err == *SB5042* && $err == *"expects 2 arguments"* ]]; chk 81.33

# 81.34 the control: the same body with both arguments compiles and reads both.
# 81.33 alone is satisfied by a signature that over-counts, so the pair is what
# pins the index set rather than just its size.
run_sash 'fn f {
    [[ ${args[0]} == x && ${args[1]} == y ]] && echo both
}
f x y'
[[ $rc = 0 && $out = "both" && -z $err ]]; chk 81.34

# 81.35 `!` recurses into the negated test — the operand is still a read
run_sash 'fn g {
    [[ ! ${args[0]} == x ]] && echo notx
}
g'
[[ $rc != 0 && -z $out && $err == *SB5042* && $err == *"expects 1 argument"* ]]; chk 81.35

# 81.36 `||` with a nested `!` — both recursive arms on one test
run_sash 'fn h {
    [[ ! ${args[0]} == x || ${args[1]} == y ]] && echo hit
}
h a'
[[ $rc != 0 && -z $out && $err == *SB5042* && $err == *"expects 2 arguments"* ]]; chk 81.36

# ── 81.37-81.46: DD#81 Part A forward calls, in the three positions that
# resolve a bareword to an external. The plain-command path recorded the
# external use (so the later `fn` raised SB5041) but the PIPELINE-STAGE and
# ARITH-SEXPR paths did not, each failing in its own way:
#   - a pipeline stage compiled to a bare `exec("up")` and died at RUNTIME with
#     the runtime's own "bare command passed to sash_exec", status 127;
#   - `$(( (f) ))` returned a literal 0, silently, with a clean exit.
# SB5011 cannot catch either, because *validate's* function table is
# whole-program while the transform's `function_bodies` is execution-order —
# the two passes disagreed about whether `up` was a function.

# 81.37 forward call as the FIRST pipeline stage
run_sash 'echo hi | up
fn up { tr a-z A-Z }'
[[ $rc != 0 && -z $out && $err == *SB5041* && $err == *"'up'"* ]]; chk 81.37

# 81.38 forward call as a MIDDLE stage (a different stage index)
run_sash 'echo hi | up | cat
fn up { tr a-z A-Z }'
[[ $rc != 0 && -z $out && $err == *SB5041* ]]; chk 81.38

# 81.39 forward call in an ARITH SEXPR — was a silent 0 with status 0
run_sash 'int n=$(( (up) ))
echo $n
fn up -> int { return 7 }'
[[ $rc != 0 && -z $out && $err == *SB5041* ]]; chk 81.39

# 81.40 a BARE EXTERNAL as a stage, then defined as a function: DD#81's
#       "dynamic shadowing" row. The stage path recorded nothing, so this
#       was accepted too.
run_sash 'echo hi | tr a-z A-Z
fn tr { echo no }
tr'
[[ $rc != 0 && $err == *SB5041* && $err == *"'tr'"* ]]; chk 81.40

# 81.41 BACKWARD stage call still works (the fix must not reject the normal form)
run_sash 'fn up { tr a-z A-Z }
echo hi | up'
[[ $rc = 0 && $out = "HI" && -z $err ]]; chk 81.41

# 81.42 BACKWARD arith sexpr still returns the real value, not 0
run_sash 'fn up -> int { return 7 }
int n=$(( (up) ))
echo $n'
[[ $rc = 0 && $out = "7" && -z $err ]]; chk 81.42

# 81.43 DD#81 KEPT: "helper below caller" as a pipeline stage. `b` is only ever
#       *called* — when `a` runs, `b` is defined — so the record made while
#       lowering `a`'s dead `fn_` template must not count as real execution
#       order. This is what the `lowering_fn_template` guard protects.
run_sash 'fn a { echo hi | b }
fn b { tr a-z A-Z }
a'
[[ $rc = 0 && $out = "HI" && -z $err ]]; chk 81.43

# 81.44 same, for the arith-sexpr path
run_sash 'fn a -> int { int t=$(( (b) ))
return $t }
fn b -> int { return 4 }
int z=$(( (a) ))
echo $z'
[[ $rc = 0 && $out = "4" && -z $err ]]; chk 81.44

# 81.45 DD#81 KEPT: an EXPLICIT external stage is always the real command, so a
#       later `fn` of that name is not a collision
run_sash 'echo hi | /bin/cat
fn cat { echo no }
cat'
[[ $rc = 0 && $out = $'hi\nno' && -z $err ]]; chk 81.45

# 81.46 a stage callee that is never defined anywhere is still SB5011, not
#       SB5041 — the forward-call record must not swallow the missing-command
#       diagnostic
run_sash 'fn a { echo hi | b }
a'
[[ $rc != 0 && -z $out && $err == *SB5011* && $err == *"'b'"* ]]; chk 81.46

# A present return-type arrow requires DD#16's `int`, in every definition
# spelling. Missing types used to silently select the default string return.
run_sash 'fn f -> { echo ok; }; f'
[[ $rc != 0 && -z $out && $err == *SB4014* \
   && $err == *"expected 'int' after '->'"* ]]; chk rty.1

run_sash 'fn f() -> { echo ok; }; f'
[[ $rc != 0 && -z $out && $err == *SB4014* \
   && $err == *"expected 'int' after '->'"* ]]; chk rty.2

run_sash 'function f() -> { echo ok; }; f'
[[ $rc != 0 && -z $out && $err == *SB4014* \
   && $err == *"expected 'int' after '->'"* ]]; chk rty.3

run_sash 'f() -> { echo ok; }; f'
[[ $rc != 0 && -z $out && $err == *SB4014* \
   && $err == *"expected 'int' after '->'"* ]]; chk rty.4

run_sash 'fn f -> str { echo ok; }; f'
[[ $rc != 0 && -z $out && $err == *SB4014* \
   && $err == *"expected 'int' after '->'"* ]]; chk rty.5

run_sash 'fn f ->'
[[ $rc != 0 && -z $out && $err == *SB4014* \
   && $err == *"expected 'int' after '->'"* ]]; chk rty.6


set -e
tally
