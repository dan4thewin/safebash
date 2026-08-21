#!/bin/bash
# spec/functions.sh — function and positional parameter tests (DD#17, DD#18, DD#42, DD#46, DD#48)
set -eu
. common.sh
setup functions
plan=47
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
# same as the two-step `s=$(greet); echo $s`. native+c (c implements
# CaptureStart/End via sash_capture_start/end, 2026-08-20). janet deferred.
if in_modes native,c; then
    run_sash 'fn greet { echo hi; }
echo $(greet)'
    [[ $rc = 0 && $out = "hi" && -z $err ]]; chk 17.3
    # 17.4 inline capture concatenated with a literal argument
    run_sash 'fn greet { echo world; }
echo "hello-$(greet)"'
    [[ $rc = 0 && $out = "hello-world" && -z $err ]]; chk 17.4
else
    skip_chk 17.3; skip_chk 17.4
fi

# 18.1 local fn visible only within block
run_sash 'fn outer {
  local fn inner { echo inside; }
  inner
}
outer'
[[ $rc = 0 && $out = "inside" && -z $err ]]; chk 18.1

# 18.2 local fn overrides global within block
run_sash 'fn greet { echo global; }
fn outer {
  local fn greet { echo local; }
  greet
}
outer'
[[ $rc = 0 && $out = "local" && -z $err ]]; chk 18.2

# 18.3 global restored after block exits
run_sash 'fn greet { echo global; }
fn outer {
  local fn greet { echo local; }
  greet
}
outer
greet'
[[ $rc = 0 && $out == *"local"*"global"* && -z $err ]]; chk 18.3

# 18.4 local fn not callable from outside
run_sash 'fn outer {
  local fn inner { echo inside; }
}
outer
inner'
[[ $rc != 0 && -z $err ]]; chk 18.4

# 42.1 $(getenv VAR) reads env var
run_sash 'x=$(getenv HOME)
bless x [[ $x =~ ^/ ]]
echo $x'
[[ $rc = 0 && -n $out && -z $err ]]; chk 42.1

# 42.2 $(getenv VAR) result is tainted
run_sash 'x=$(getenv HOME); echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 42.2

# 42.3 $(arg N) reads program arg
run_sash 'x=$(arg 1)
bless x [[ $x =~ ^[a-z]+$ ]]
echo $x' hello
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 42.3

# 42.4 $(arg N) result is tainted (rejected without bless)
run_sash 'x=$(arg 1); echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 42.4

# 42.5 $(argc) returns C argc (1 + user-arg count)
run_sash 'echo $(argc)' a b c
[[ $rc = 0 && $out = "4" && -z $err ]]; chk 42.5

# 42.6 out-of-range $(arg N) aborts at runtime
run_sash 'x=$(arg 99); bless x [[ -n $x ]]; echo $x' hello
[[ $rc != 0 && -z $out ]]; chk 42.6

# 42.7 $(getpid) — in-process intrinsic, positive int, CLEAN capture (usable
#      without a bless). native,c only: janet lacks the getpid-class.
if in_modes native,c; then
    run_sash 'x=$(getpid); [[ $x =~ ^[0-9]+$ ]] && echo Y || echo N'
    [[ $rc = 0 && $out = "Y" && -z $err ]]; chk 42.7
else
    skip_chk 42.7
fi
# 42.8 $(random) — non-negative i64 (0..i64::MAX), clean capture
if in_modes native,c; then
    run_sash 'a=$(random); [[ $a =~ ^[0-9]+$ ]] && echo Y || echo N'
    [[ $rc = 0 && $out = "Y" && -z $err ]]; chk 42.8
else
    skip_chk 42.8
fi
# 42.9 two $(random) draws differ (distinct entropy)
if in_modes native,c; then
    run_sash 'a=$(random); b=$(random); [[ $a != $b ]] && echo Y || echo N'
    [[ $rc = 0 && $out = "Y" && -z $err ]]; chk 42.9
else
    skip_chk 42.9
fi

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

# 67.2 `name()` sugar passes positionals like fn
run_sash 'pick() { echo $2; }
pick a b c'
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

# 74.3 `! f` negates the return value. (janet: `!` in a condition is a
#      separate pre-existing gap — it lowers to `true`; see the janet-predicate
#      task — so guard the runtime to native,c.)
if in_modes native,c; then
    run_sash 'is_ok() { return 0; }
if ! is_ok; then echo neg; else echo pos; fi'
    [[ $rc = 0 && $out = "pos" && -z $err ]]; chk 74.3
else
    skip_chk 74.3
fi

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
run_sash 'has() { return [[ ok == ok ]] }
if has; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 74.7

# 74.8 `return [[ … ]]` — false test → else
run_sash 'has() { return [[ ok == no ]] }
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
run_sash 'p() { if [[ ok == ok ]]; then return 0; fi }
if p; then echo y; fi'
[[ $rc != 0 && -z $out && -n $err ]]; chk 74.15

# 74.16 if/else where both branches return → accepted
run_sash 'p() { if [[ ok == ok ]]; then return 0; else return 1; fi }
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

set -e
tally
