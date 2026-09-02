#!/bin/bash
# spec/scope.sh — DD#86 variable scope model. Phase 1: the `str` keyword
# (declares a string, meaningful-zero "", parallel to `int`), so a string
# global can be declared at top level without `=""`. Later phases (structural
# scope, no-shadowing, dead-variable rejection) extend this file.
set -eu
. common.sh
setup scope
set +e
plan=20
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

set -e
tally
