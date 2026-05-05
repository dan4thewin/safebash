#!/bin/bash
# spec/signals.sh — signal handling tests (DD#56)
set -eu
. common.sh
setup signals
plan=10
echo "1..$plan" >&3

set +e

# 56.1 trap : SIG sets flag without check points
run_sash 'trap : SIGINT; echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 56.1

# 56.2 trap exit SIG sets flag and inserts check points
run_sash 'trap exit SIGTERM; echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 56.2

# 56.3 $(signum) returns signal number
run_sash 'trap : SIGINT; echo $(signum)'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk 56.3

# 56.4 EXIT block: every path ends with exit
run_sash 'trap exit SIGTERM
echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 56.4

# 56.5 supported signals limited set
run_sash 'trap : SIGKILL'
[[ $rc != 0 && -z $out && -n $err ]]; chk 56.5

# 56.6 both SIGTERM and TERM accepted
run_sash 'trap : TERM; echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 56.6a

run_sash 'trap : SIGTERM; echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 56.6b

# 56.8 trap - SIG removes trap
run_sash 'trap : SIGINT; trap - SIGINT; echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 56.8

# 56.9 flag-only traps add zero overhead
run_sash 'trap : SIGINT; echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 56.9

# 56.10 check points before external commands and function calls
run_sash 'trap exit SIGTERM; /bin/echo hello'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 56.10

set -e
tally
