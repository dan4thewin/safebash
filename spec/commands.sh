#!/bin/bash
# spec/commands.sh — command resolution and pipeline tests (DD#14, DD#23, DD#54)
set -eu
. common.sh
setup commands
plan=33
echo "1..$plan" >&3

set +e

# 14.1 pipeline uses fork/exec/wait/pipe
run_sash 'echo hello | /usr/bin/cat'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 14.1

# 14.2 command substitution
run_sash 'x=$(echo hello); echo $x'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk 14.2

# 14.2b command substitution of an EXTERNAL command (printf resolves to
#        /usr/bin/printf via DD#23) has TAINTED capture — execve output is
#        tainted, so using it needs a bless (SB5020). Compile-time, all modes.
run_sash 'x=$(printf hi); echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 14.2b

# 14.2c blessed, the external capture is usable. native only: the C cmdsub
#        exec is a Phase-4 TODO and the Janet os/spawn capture pipe is racy
#        (janet-pipeline-eagain-flaky).
if in_modes native,c; then
    run_sash 'x=$(printf hi); bless x [[ -n $x ]]; echo "$x"'
    [[ $rc = 0 && $out = "hi" && -z $err ]]; chk 14.2c
else
    skip_chk 14.2c
fi

# 14.3 multi-statement / pipeline command substitution (DG 2026-08-19: implement
#      — run the body, capture stdout). Capture-into-var is native-only (the C
#      CaptureStart/End capture and the Janet cmdsub are gaps, like $(f)); the
#      taint rejects are compile-time (all modes).
# 14.3a a clean multi-statement body captures its full stdout
if in_modes native,c; then
    run_sash 'x=$(echo a; echo b); echo "[$x]"'
    [[ $rc = 0 && $out == $'[a\nb]' && -z $err ]]; chk 14.3a
else
    skip_chk 14.3a
fi
# 14.3b subshell isolation — an assignment inside the body does NOT leak
if in_modes native,c; then
    run_sash 'x=outer; y=$(x=inner; echo $x); echo "y=$y x=$x"'
    [[ $rc = 0 && $out = "y=inner x=outer" && -z $err ]]; chk 14.3b
else
    skip_chk 14.3b
fi
# 14.3c a pipeline body with an external stage captures TAINTED output — using
#       it without a bless is SB5020 (compile-time, all modes). Closes the
#       silent `$(...)` placeholder bug.
run_sash 'echo $(echo hi | tr h H)'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 14.3c
# 14.3d blessed, the pipeline capture is usable (native)
if in_modes native,c; then
    run_sash 'x=$(/bin/echo hi | /usr/bin/tr h H); bless x [[ -n $x ]]; echo $x'
    [[ $rc = 0 && $out = "Hi" && -z $err ]]; chk 14.3d
else
    skip_chk 14.3d
fi
# 14.3e the `$(- cmd)` suppression wrapper no longer hides an external from the
#       taint scan (a closed hole) — its capture is tainted, needs bless
run_sash 'x=$(- /bin/echo hi); echo $x'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 14.3e

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

# 14.17 a BARE (unquoted) nested command substitution as a command argument.
# `$(f $(g))` — the two adjacent `))` are two cmdsub closes, not the arith
# `))` terminator (bug-bare-nested-cmdsub-arg). This still guards the lexer
# split of DblParenClose outside arith mode: the SB5020 below can only fire if
# the inner `$( )` was recognized as a cmdsub. But it is a TAINT ERROR — the
# inner `$(/bin/echo hi)` is external (tainted) and its result is *read* to
# become an argument to the outer command, and `bless x` covers only the outer
# assignment, not that inner read. So a tainted nested cmdsub is rejected.
# (Ids are 14.17+ to avoid the pre-existing collision with wordsplit.sh, which
# owns 14.4-14.9; see bug-commands-wordsplit-14x-chk-collision.)
run_sash 'x=$(/bin/echo $(/bin/echo hi)); bless x [[ -n $x ]]; echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 14.17
# 14.18 triple-nested bare cmdsub (`)))` splits into three closes) — parses, and
# the tainted inner reads are likewise rejected (SB5020).
run_sash 'x=$(/bin/echo $(/bin/echo $(/bin/echo deep))); bless x [[ -n $x ]]; echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 14.18

# --- bug #4: a redirect may only attach to a command (SB4046) ---
# A redirect gluing onto a function definition, a declaration, or an assignment
# is rejected with one clear spanned error (was: misleading SB5011 for a
# function, misleading SB4014 for a declaration, and SILENTLY DROPPED for a bare
# assignment — `x=5 > f` truncated f and set x with no diagnostic). Ids are
# 14.13+ (commands.sh shares the 14.x block with wordsplit.sh, which owns
# 14.4–14.9).

# 14.13 redirect on a function definition → SB4046 (was SB5011 on the call)
run_sash 'f() { echo hi } > /tmp/sash_test_redir3; f'
[[ $rc != 0 && -z $out && $err == *SB4046* ]]; chk 14.13

# 14.14 redirect on a declaration → SB4046 (was SB4014 "expected variable name")
run_sash 'int x=5 > /tmp/sash_test_redir3; echo $x'
[[ $rc != 0 && -z $out && $err == *SB4046* ]]; chk 14.14

# 14.15 redirect on a bare assignment → SB4046 (was silently accepted+dropped)
run_sash 'x=5 > /tmp/sash_test_redir3; echo $x'
[[ $rc != 0 && -z $out && $err == *SB4046* ]]; chk 14.15

# 14.16 control: a plain function definition + call still works (no redirect)
run_sash 'f() { echo hi }; f'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 14.16

set -e
tally
