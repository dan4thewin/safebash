#!/bin/bash
# spec/commands.sh — command resolution and pipeline tests (DD#14, DD#23, DD#54)
set -eu
. common.sh
setup commands
plan=59
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
#        tainted, so using it needs a attest (SB5020). Compile-time, all modes.
run_sash 'x=$(printf hi); echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 14.2b

# 14.2c attested, the external capture is usable. native only: the C cmdsub
#        exec is a Phase-4 TODO and the Janet os/spawn capture pipe is racy
#        (janet-pipeline-eagain-flaky).
if in_modes native,c; then
    run_sash 'x=$(printf hi); attest x [[ -n $x ]]; echo "$x"'
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
#       it without a attest is SB5020 (compile-time, all modes). Closes the
#       silent `$(...)` placeholder bug.
run_sash 'echo $(echo hi | tr h H)'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 14.3c
# 14.3d attested, the pipeline capture is usable (native)
if in_modes native,c; then
    run_sash 'x=$(/bin/echo hi | /usr/bin/tr h H); attest x [[ -n $x ]]; echo $x'
    [[ $rc = 0 && $out = "Hi" && -z $err ]]; chk 14.3d
else
    skip_chk 14.3d
fi
# 14.3e the `$(- cmd)` suppression wrapper no longer hides an external from the
#       taint scan (a closed hole) — its capture is tainted, needs attest
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
# become an argument to the outer command, and `attest x` covers only the outer
# assignment, not that inner read. So a tainted nested cmdsub is rejected.
# (Ids are 14.17+ to avoid the pre-existing collision with wordsplit.sh, which
# owns 14.4-14.9; see bug-commands-wordsplit-14x-chk-collision.)
run_sash 'x=$(/bin/echo $(/bin/echo hi)); attest x [[ -n $x ]]; echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 14.17
# 14.18 triple-nested bare cmdsub (`)))` splits into three closes) — parses, and
# the tainted inner reads are likewise rejected (SB5020).
run_sash 'x=$(/bin/echo $(/bin/echo $(/bin/echo deep))); attest x [[ -n $x ]]; echo "$x"'
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

# --- redirect target must be a scalar (SB4049; DG 2026-08-30) ---
# A file-redirect target (`>`/`>>`/`<`/`<>`/`&>`) must be a scalar the compiler
# can evaluate to a filename: a literal/quoted path, `$x`, `${x:-default}`, or
# `$a[i]`. A command substitution `$(…)` or arithmetic `$((…))` is rejected —
# the compiler can't know the resulting filename, and the old fallthrough
# silently redirected to an *empty* target (bug-redirect-nonsimple-target-empty).
# Scalar reads beyond a bare `$x` (a `:-` default, a subscript) are supported by
# spilling the lowered value to a temp before the redirect. janet has no file
# redirects (it emits a no-op comment), so the positives are native,c only.

# 14.19 reject: command-substitution redirect target → SB4049 (all modes: parse)
run_sash 'echo x > $(echo f)'
[[ $rc != 0 && -z $out && $err == *SB4049* ]]; chk 14.19

# 14.20 reject: arithmetic redirect target → SB4049
run_sash 'echo x > $(( 1 + 1 ))'
[[ $rc != 0 && -z $out && $err == *SB4049* ]]; chk 14.20

# 14.21 support: `${x:-default}` target (x unset) opens the DEFAULT filename
if in_modes native,c; then
    run_sash 'echo d > ${miss:-r14def}; cat r14def'
    [[ $rc = 0 && $out = "d" && -z $err ]]; chk 14.21
else
    skip_chk 14.21
fi

# 14.22 support: `${x:-default}` target (x set) opens the SET value, not default
if in_modes native,c; then
    run_sash 'f=r14set; echo s > ${f:-r14fallback}; cat r14set'
    [[ $rc = 0 && $out = "s" && -z $err ]]; chk 14.22
else
    skip_chk 14.22
fi

# 14.23 support: `$a[i]` subscript redirect target opens the element's filename
if in_modes native,c; then
    run_sash 'array a=(r14zero r14one); echo sub > $a[1]; cat r14one'
    [[ $rc = 0 && $out = "sub" && -z $err ]]; chk 14.23
else
    skip_chk 14.23
fi

# 14.24 support: append `>> $a[i]` accumulates to the element's filename
if in_modes native,c; then
    run_sash 'array a=(x r14ap); echo l1 > $a[1]; echo l2 >> $a[1]; cat r14ap'
    [[ $rc = 0 && $out == *l1* && $out == *l2* && -z $err ]]; chk 14.24
else
    skip_chk 14.24
fi

# 14.25 reject: here-strings are dropped (DD#83) → SB4050 (all modes: parse).
# `<<<` is exactly a pipe in sash (no subshell, provenance-based taint, no
# word-splitting) — pure sugar, so it's removed. `cat <<< $x` → `echo $x | cat`.
run_sash 'cat <<< hi'
[[ $rc != 0 && -z $out && $err == *SB4050* ]]; chk 14.25

# --- more redirect-target forms (never-run coverage + a stage-redirect bug) ---

# 14.26 support: `&>file` (parse_ampgt_redirect) — stdout+stderr to a scalar
if in_modes native,c; then
    run_sash 'echo amp &> r14amp; cat r14amp'
    [[ $rc = 0 && $out = "amp" && -z $err ]]; chk 14.26
else
    skip_chk 14.26
fi

# 14.27 support: single-quoted target `> '…'` (redirect_expr_literal)
if in_modes native,c; then
    run_sash "echo sq > 'r14sq'; cat r14sq"
    [[ $rc = 0 && $out = "sq" && -z $err ]]; chk 14.27
else
    skip_chk 14.27
fi

# 14.28 support: double-quoted concat target `> "$x.txt"` (redirect_expr_concat)
if in_modes native,c; then
    run_sash 'x=r14dq; echo dq > "$x.txt"; cat r14dq.txt'
    [[ $rc = 0 && $out = "dq" && -z $err ]]; chk 14.28
else
    skip_chk 14.28
fi

# 14.29 BUG FIX: a pipeline-stage redirect with a `$var` target was silently
# DROPPED (the stage went to the pipe instead of the file) — the stage path had
# its own ad-hoc target lowering (ir_expr_to_ssa_value) that hardcoded SSA
# version 0. Now it reuses the top-level lower_ir_redirect_expr.
if in_modes native,c; then
    run_sash 'f=r14st; echo hi > $f | cat; cat r14st'
    [[ $rc = 0 && $out = "hi" && -z $err ]]; chk 14.29
else
    skip_chk 14.29
fi

# 14.30 BUG FIX: a pipeline-stage redirect with a `${x:-f}` target (spilled)
if in_modes native,c; then
    run_sash 'echo hi > ${miss:-r14sd} | cat; cat r14sd'
    [[ $rc = 0 && $out = "hi" && -z $err ]]; chk 14.30
else
    skip_chk 14.30
fi

# 14.31 BUG FIX: a pipeline-stage redirect with a `$a[i]` target (spilled)
if in_modes native,c; then
    run_sash 'array a=(x r14se); echo hi > $a[1] | cat; cat r14se'
    [[ $rc = 0 && $out = "hi" && -z $err ]]; chk 14.31
else
    skip_chk 14.31
fi


# ════════ DD#99 stage 4: `write` — echo without the trailing newline ════════
# DG: "I'd spell echo -n as write." A distinct construct rather than a flag,
# following DD#60/#75: `echo` terminates, `write` does not, neither takes a
# flag. Stage 7 removes the `echo -n` spelling.

# wr.1 writes with no trailing newline
if in_modes native,c; then
    run_sash 'write "a"
write "b"
echo "|"'
    [[ $rc = 0 && $out = "ab|" && -z $err ]]; chk wr.1
else
    skip_chk wr.1
fi

# wr.2 several arguments are space-joined, as echo does
if in_modes native,c; then
    run_sash 'write "x" "y"
echo "|"'
    [[ $rc = 0 && $out = "x y|" && -z $err ]]; chk wr.2
else
    skip_chk wr.2
fi

# wr.3 `echo` still terminates — the two differ only in the newline
run_sash 'echo "a"
echo "b"'
[[ $rc = 0 && $out = "a
b" && -z $err ]]; chk wr.3

# wr.4 interpolates like echo
if in_modes native,c; then
    run_sash 'v=hi
write "[$v]"
echo "|"'
    [[ $rc = 0 && $out = "[hi]|" && -z $err ]]; chk wr.4
else
    skip_chk wr.4
fi

# wr.5 no arguments writes nothing at all
if in_modes native,c; then
    run_sash 'write
echo "|"'
    [[ $rc = 0 && $out = "|" && -z $err ]]; chk wr.5
else
    skip_chk wr.5
fi

# wr.6 a tainted value still needs attest (DD#24 is unaffected)
run_sash 'v=$(getenv HOME)
write $v'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk wr.6

# wr.7 the DD#69 §3 echo-splat rule applies to `write` too — an array splat
# needs an explicit join, since it is the same output construct
run_sash 'array a=(x y)
write @a'
[[ $rc != 0 && -z $out && -n $err ]]; chk wr.7


# wr.8 `echo -n` is retired — the spelling is `write` (DD#99). `echo` ends its
# output with a newline and `write` does not; neither takes a flag.
run_sash 'echo -n "a"'
[[ $rc != 0 && -z $out && $err == *SB5059* ]]; chk wr.8


# ── DD#14: a redirect on a PIPELINE STAGE ─────────────────────────────────
# `cmd < f | consumer` silently gave the stage nothing: the stage path
# defaulted every redirect's implicit fd to 1, so an INPUT redirect was
# applied to the stage's stdout. With stdin on a terminal the program hung
# waiting on it; with </dev/null it read empty. The non-pipeline path used
# the direction to pick the fd and so always worked. This is DD#99's
# canonical idiom (`tr SEP '\0' < f | read arr`).

# 14.32 the first stage's input redirect feeds it
if in_modes native,c; then
    run_sash '/usr/bin/printf "x\ny\n" > rd1.txt; /bin/cat < rd1.txt | /bin/cat'
    [[ $rc = 0 && $out = "x
y" && -z $err ]]; chk 14.32
else
    skip_chk 14.32
fi

# 14.33 …and a middle stage's, overriding what the pipe would have supplied
if in_modes native,c; then
    run_sash '/usr/bin/printf "m\n" > rd2.txt; echo ignored | /bin/cat < rd2.txt | /bin/cat'
    [[ $rc = 0 && $out = "m" && -z $err ]]; chk 14.33
else
    skip_chk 14.33
fi

# 14.34 the DD#99 idiom: turn a separator into NUL upstream, then split
if in_modes native,c; then
    run_sash '/usr/bin/printf "a\nb\nc\n" > rd3.txt
/usr/bin/tr "\n" "\0" < rd3.txt | read arr
attest e in arr [[ $e = $e ]]
echo "$(len arr):${arr[1]}"'
    [[ $rc = 0 && $out = "3:b" && -z $err ]]; chk 14.34
else
    skip_chk 14.34
fi

# 14.35 the OUTPUT direction still lands on fd 1, not fd 0
if in_modes native,c; then
    run_sash 'echo captured > rd4.txt | /bin/cat
/bin/cat rd4.txt'
    [[ $rc = 0 && $out = "captured" && -z $err ]]; chk 14.35
else
    skip_chk 14.35
fi

# 14.36 a redirect on a NON-pipeline command is unaffected
if in_modes native,c; then
    run_sash '/usr/bin/printf "solo\n" > rd5.txt; /bin/cat < rd5.txt'
    [[ $rc = 0 && $out = "solo" && -z $err ]]; chk 14.36
else
    skip_chk 14.36
fi

set -e
tally
