#!/bin/bash
# spec/scope.sh — DD#86 variable scope model. Phase 1: the `str` keyword
# (declares a string, meaningful-zero "", parallel to `int`), so a string
# global can be declared at top level without `=""`. Later phases (structural
# scope, no-shadowing, dead-variable rejection) extend this file.
set -eu
. common.sh
setup scope
set +e
plan=6
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

set -e
tally
