#!/bin/bash
# spec/functions.sh — function and positional parameter tests (DD#17, DD#18, DD#42, DD#46, DD#48)
set -eu
. common.sh
setup functions
plan=17
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

# 42.3 $(argv N) reads program arg
run_sash 'x=$(argv 1)
bless x [[ $x =~ ^[a-z]+$ ]]
echo $x' hello
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 42.3

# 42.4 $(argv N) result is tainted (at top level)
run_sash 'x=$(argv 1); echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 42.4

# 42.5 $(argc) returns count of program args
run_sash 'echo $(argc)' a b c
[[ $rc = 0 && $out = "3" && -z $err ]]; chk 42.5

# 42.6 out-of-range $(argv N) aborts with error
run_sash 'echo $(argv 99)' hello
[[ $rc != 0 && -z $out ]]; chk 42.6

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

# 48.2 compiler tracks cumulative shift count
run_sash 'fn f { shift; echo $1; }
f a b'
[[ $rc = 0 && $out = "b" && -z $err ]]; chk 48.2

set -e
tally
