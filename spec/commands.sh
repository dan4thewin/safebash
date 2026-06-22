#!/bin/bash
# spec/commands.sh — command resolution and pipeline tests (DD#14, DD#23, DD#54)
set -eu
. common.sh
setup commands
plan=21
echo "1..$plan" >&3

set +e

# 14.1 pipeline uses fork/exec/wait/pipe
run_sash 'echo hello | /usr/bin/cat'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 14.1

# 14.2 command substitution
run_sash 'x=$(echo hello); echo $x'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 14.2

# 14.2b command substitution resolves an EXTERNAL command head (DD#23) —
#        bare `printf`/`cat` used to reach sash_exec unresolved. native
#        only: the C cmdsub exec is a Phase-4 TODO and the Janet os/spawn
#        capture pipe is racy (janet-pipeline-eagain-flaky).
if in_modes native; then
    run_sash 'x=$(printf hi); echo "$x"'
    [[ $rc = 0 && $out = "hi" && -z $err ]]; chk 14.2b
else
    skip_chk 14.2b
fi

# 67.4 flat backtick `cmd` is sugar for $(cmd) (DD#67)
run_sash 'x=`echo hi`; echo $x'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 67.4

# 67.5 nested backticks (\`-escaping) are rejected — use $()
run_sash 'x=`echo \`date\``; echo $x'
[[ $rc != 0 && -z $out && -n $err ]]; chk 67.5

# 67.6 single-bracket `[` is an ordinary literal (DD#67), no longer a
#      test construct. `[[ … ]]` is the only test form; `arr[i]` subscript
#      is unaffected. 67.6a: `[`/`]` are plain words in argument position.
run_sash 'echo [ ]'
[[ $rc = 0 && $out = "[ ]" && -z $err ]]; chk 67.6a

# 67.6b `[ … ]` resolves and runs as the external `[` command (exit 0
#       here, so the DD#55 implicit check passes and execution continues).
run_sash '[ a = a ]
echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 67.6b

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

# 14.4 for...done > file → rejected (SB4021)
run_sash 'for x in a b c; do echo $x; done > /tmp/sash_test_redir'
[[ $rc != 0 && -z $out && -n $err ]]; chk 14.4

# 14.5 while...done | cmd → rejected (SB4022)
run_sash 'int i=0; while (( i < 3 )); do echo $i; (( i++ )); done | /usr/bin/wc -l'
[[ $rc != 0 && -z $out && -n $err ]]; chk 14.5

# 14.6 if...fi > file → rejected (SB4021)
run_sash 'if [[ "yes" = "yes" ]]; then echo hello; fi > /tmp/sash_test_redir2'
[[ $rc != 0 && -z $out && -n $err ]]; chk 14.6

set -e
tally
