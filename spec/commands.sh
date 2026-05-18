#!/bin/bash
# spec/commands.sh — command resolution and pipeline tests (DD#14, DD#23, DD#54)
set -eu
. common.sh
setup commands
plan=16
echo "1..$plan" >&3

set +e

# 14.1 pipeline uses fork/exec/wait/pipe
run_sash 'echo hello | /usr/bin/cat'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 14.1

# 14.2 command substitution
run_sash 'x=$(echo hello); echo $x'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 14.2

# 14.3 background command
run_sash 'echo bg &
wait'
[[ $rc = 0 && -z $err ]]; chk 14.3

# 23.1 bare command resolved to absolute path
run_sash 'echo hello'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 23.1

# 23.2 unresolved external → compile error
run_sash 'nonexistent_command_xyz'
[[ $rc != 0 && -z $out && -n $err ]]; chk 23.2

# 23.3 command -v rejected
run_sash 'command -v echo'
[[ $rc != 0 && -z $out && -n $err ]]; chk 23.3

# 23.4 /usr/bin/env cmd accepted
run_sash '/usr/bin/env echo hello'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 23.4

# 23.5 sash_exec requires absolute path — test that absolute path works
run_sash '/bin/echo hello'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 23.5

# 54.1a source/dot rejected (dot form)
run_sash '. ./file.sh'
[[ $rc != 0 && -z $out && -n $err ]]; chk 54.1a

# 54.1b source/dot rejected (source form)
run_sash 'source ./file.sh'
[[ $rc != 0 && -z $out && -n $err ]]; chk 54.1b

# --- DD#23 compile-time path resolution ---

# 23.6 bare 'cat' resolved to absolute path
run_sash 'echo hello | cat'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 23.6

# 23.7 bare 'grep' resolved to absolute path
run_sash 'echo hello | grep hello'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 23.7

# 23.8 bare 'wc' resolved
run_sash 'echo hello | wc -c'
[[ $rc = 0 && -n $out && -z $err ]]; chk 23.8

# --- DD#14 compound redirects ---
# --- DD#14 compound redirects ---

# 14.4 for...done > file
run_sash 'for x in a b c; do echo $x; done > /tmp/sash_test_redir
/bin/cat /tmp/sash_test_redir
/bin/rm /tmp/sash_test_redir'
[[ $rc = 0 && $out == *"a"* && $out == *"b"* && $out == *"c"* && -z $err ]]; chk 14.4

# 14.5 while...done | cmd
run_sash 'int i=0; while (( i < 3 )); do echo $i; (( i++ )); done | /usr/bin/wc -l'
[[ $out == *"3"* && -z $err ]]; chk 14.5

# 14.6 if...fi > file
run_sash 'if [[ "yes" = "yes" ]]; then echo hello; fi > /tmp/sash_test_redir2
/bin/cat /tmp/sash_test_redir2
/bin/rm /tmp/sash_test_redir2'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 14.6

set -e
tally
