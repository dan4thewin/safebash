#!/bin/bash
# spec/taint.sh — taint tracking tests (DD#24, DD#50, DD#51, DD#53)
set -eu
. common.sh
setup taint
plan=31
echo "1..$plan" >&3

set +e

# 24.1 getenv produces tainted var
run_sash 'x=$(getenv HOME); echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.1

# 24.2 argv produces tainted var
run_sash 'x=$(argv 1); echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.2

# 24.3 read produces tainted var
run_sash 'read -r x; echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.3

# 24.4 mapfile produces tainted var
run_sash 'mapfile -t arr; echo ${arr[0]}'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.4

# 24.5 external cmd produces tainted var
run_sash 'x=$(/bin/date); echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.5

# 24.6 read from clean heredoc → clean
run_sash 'read x <<< "hello"; echo $x'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 24.6

# 24.7 user func capture → clean
run_sash 'fn greet { echo hello; }
x=$(greet)
echo $x'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 24.7

# 24.8 tainted var at use site → compile error
run_sash 'x=$(getenv PATH); /bin/ls $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.8

# 24.9 bless makes var clean
run_sash 'x=$(getenv HOME)
bless x [[ $x =~ ^/[a-zA-Z0-9/]+$ ]]
echo $x'
[[ $rc = 0 && -z $err ]]; chk 24.9

# 24.10 bless requires $VAR in test expression
run_sash 'x=$(getenv HOME)
bless x [[ 1 -eq 1 ]]
echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.10

# 24.11 bless requires $VAR on LHS
run_sash 'x=$(getenv HOME)
bless x [[ "foo" = $x ]]
echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.11

# 24.12 bless failure (unconditional) aborts at runtime
run_sash 'x=$(getenv HOME)
bless x [[ $x =~ ^NEVER_MATCH$ ]]
echo $x'
# bless fails at runtime → exits 1, no stdout
[[ $rc != 0 && -z $out ]]; chk 24.12

# 24.13 if bless failure → takes false branch, var still tainted
run_sash 'x=$(getenv HOME)
if bless x [[ $x =~ ^NEVER$ ]]; then
  echo $x
fi'
[[ $rc = 0 && -z $err ]]; chk 24.13

# 24.14 re-assignment from tainted source re-taints
run_sash 'x=$(getenv HOME)
bless x [[ $x =~ ^/[a-zA-Z0-9/]+$ ]]
x=$(getenv USER)
echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.14

# 24.15 literals and $#, $?, $$ are clean
run_sash 'x="hello"; echo $x'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 24.15

# 24.16 bless test SHALL NOT allow external commands or arithmetic
run_sash 'x=$(getenv HOME)
bless x [[ $(echo yes) = yes ]]
echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.16

# 24.17 compiler SHALL NOT auto-bless known external output
run_sash 'd=$(/bin/date +%Y-%m-%d); echo $d'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.17

# 24.18 known hostname — still tainted without explicit bless
run_sash 'h=$(/bin/hostname); echo $h'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.18

# 24.19 getenv without bless — tainted
run_sash 'x=$(getenv HOME); echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 24.19

# 50.1 tainted global at call site → error
run_sash 'x=$(getenv HOME)
fn f { echo $x; }
f'
[[ $rc != 0 && -z $out && -n $err ]]; chk 50.1

# 51.1 SSA backstop — tainted use caught before codegen
run_sash 'x=$(argv 1); echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 51.1

# 53.1 $HOME is an env var — must use $(getenv HOME)
run_sash 'echo $HOME'
[[ $rc != 0 && -z $out && -n $err ]]; chk 53.1

# 53.2 only way to read env is getenv
run_sash 'x=$(getenv HOME); bless x [[ $x =~ ^/ ]]; echo $x'
[[ $rc = 0 && -z $err ]]; chk 53.2

# 53.3 getenv result is tainted
run_sash 'x=$(getenv HOME); echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 53.3

# 53.4 no mechanism to push to env
run_sash 'export FOO=bar'
[[ $rc != 0 && -z $out && -n $err ]]; chk 53.4

# 53.5 export keyword compile error
run_sash 'export PATH=/usr/bin'
[[ $rc != 0 && -z $out && -n $err ]]; chk 53.5

# 53.6 per-command env vars use /usr/bin/env
run_sash '/usr/bin/env FOO=bar /bin/echo hello'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 53.6

# 53.10 $0 rejected — use $(argv 0)
run_sash 'echo $0'
[[ $rc != 0 && -z $out && -n $err ]]; chk 53.10

# 53.7 $RANDOM rejected
run_sash 'echo $RANDOM'
[[ $rc != 0 && -z $out && -n $err ]]; chk 53.7

# 53.8 $BASH_VERSION rejected
run_sash 'echo $BASH_VERSION'
[[ $rc != 0 && -z $out && -n $err ]]; chk 53.8

# 53.9 $PATH requires getenv
run_sash 'echo $PATH'
[[ $rc != 0 && -z $out && -n $err ]]; chk 53.9

set -e
tally
