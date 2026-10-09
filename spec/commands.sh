#!/bin/bash
# spec/commands.sh — command resolution and pipeline tests (DD#14, DD#23, DD#54)
set -eu
. common.sh
setup commands
plan=172
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

# 14.2c attested, the external capture is usable.
run_sash 'x=$(printf hi); attest x [[ -n $x ]]; echo "$x"'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 14.2c

# 14.3 multi-statement / pipeline command substitution (DG 2026-08-19: implement
#      — run the body, capture stdout). The taint rejects are compile-time.
# 14.3a a clean multi-statement body captures its full stdout
run_sash 'x=$(echo a; echo b); echo "[$x]"'
[[ $rc = 0 && $out == $'[a\nb]' && -z $err ]]; chk 14.3a
# 14.3b a `$( )` body is an anonymous function, not a subshell (DD#86
#       addendum, DG 2026-09-28). sash has no subshells, so a write to an
#       existing global inside the body PERSISTS. Deliberate bash divergence
#       — bash prints `x=outer`. It also makes the inline body agree with
#       `y=$(f)`, where the write to a global has always survived.
run_sash 'x=outer; y=$(x=inner; echo $x); echo "y=$y x=$x"'
[[ $rc = 0 && $out = "y=inner x=inner" && -z $err ]]; chk 14.3b
# 14.3c a pipeline body with an external stage captures TAINTED output — using
#       it without a attest is SB5020 (compile-time, all modes). Closes the
#       silent `$(...)` placeholder bug.
run_sash 'echo $(echo hi | tr h H)'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 14.3c
# 14.3d attested, the pipeline capture is usable (native)
run_sash 'x=$(/bin/echo hi | /usr/bin/tr h H); attest x [[ -n $x ]]; echo $x'
[[ $rc = 0 && $out = "Hi" && -z $err ]]; chk 14.3d
# 14.3e the `$(- cmd)` suppression wrapper no longer hides an external from the
#       taint scan (a closed hole) — its capture is tainted, needs attest
run_sash 'x=$(- /bin/echo hi); echo $x'
[[ $rc != 0 && -z $out && $err == *SB5020* ]]; chk 14.3e

# 67.4 backticks are DROPPED, not sugar (DD#67, DG 2026-09-14). The mapping to
#      $(cmd) is not 1-to-1 — nesting needs \`, which DD#64 removes — so the
#      form fails DD#67's own keep-as-sugar test. sash-convert rewrites it.
run_sash 'x=`echo hi`; echo $x'
[[ $rc != 0 && -z $out && $err == *SB5076* ]]; chk 67.4

# 67.5 nested backticks take the same rejection — there is no longer a separate
#      "ambiguous nesting" case, because no backtick spelling is accepted.
run_sash 'x=`echo \`date\``; echo $x'
[[ $rc != 0 && -z $out && $err == *SB5076* ]]; chk 67.5

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
- wait'
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

# 23.20 a dynamic command head is SB5011 in STATEMENT position...
run_sash 'c=echo
$c hi'
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk 23.20

# 23.21 ...and equally inside a COMMAND SUBSTITUTION. It was not: the head
# check only ran on statements, so `$($c hi)` was accepted, then aborted at
# runtime in native ("bare command passed to sash_exec") and silently
# captured the empty string in c. DD#23 is a property of a command head,
# not of the position the command appears in.
run_sash 'c=echo
x=$($c hi)
attest x [[ -n $x ]]
echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB5011* ]]; chk 23.21

# 23.22 nested: the inner head is still caught, and reported once.
run_sash 'c=echo
x=$(/bin/echo "$($c hi)")
attest x [[ -n $x ]]
echo "$x"'
[[ $rc != 0 && -z $out && $(grep -c SB5011 <<< "$err") = 1 ]]; chk 23.22

# 23.23 a literal head inside a command substitution is still fine.
run_sash 'x=$(/bin/echo hi)
attest x [[ -n $x ]]
echo "$x"'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 23.23

# 23.24 a VALUE INTRINSIC inside `$( )` must NOT be dragged through the
# statement-position rules, where it is SB5073.
run_sash 'n=$(argc)
echo "$n"'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk 23.24

# ——— DD#111 a value intrinsic is exempt from SB5073 only when it IS the
# substitution. `transform::expr_handlers::nullary_magic_var` resolves a bare
# `$(status)` to the value it produces, and that requires the body to be
# exactly one statement with no arguments. One statement among several was not
# resolved, survived as a bare command no backend recognises, and aborted at
# runtime inside `sash_exec` — printing `sash: bug: bare command passed to
# sash_exec` into the program's OWN output, with a successful exit.
#
# (The parse-time `bareword_shortcut_expansion` that used to do this is gone:
# DD#111 reversed the direction, so `$?` desugars INTO `$(status)` rather than
# `$(status)` folding into `$?`.) ———

# 23.25 the shape that used to leak the internal abort message.
run_sash 'echo hi; echo "$(echo a; status)"'
[[ $rc != 0 && -z $out && $err == *SB5073* ]]; chk 23.25

# 23.26 order does not matter — first statement is just as unrewritten.
run_sash 'echo hi; echo "$(status; echo a)"'
[[ $rc != 0 && -z $out && $err == *SB5073* ]]; chk 23.26

# 23.27 the UNQUOTED spelling too. A sole `$(status)` is rewritten while the
# word is scanned (try_special_word) and never reaches the string-content
# path, so the two spellings take different routes to the same rule. Since
# DD#111 both routes build the identical `$(status)` node, which is why one
# rule can cover them.
run_sash 'echo hi; echo $(echo a; status)'
[[ $rc != 0 && -z $out && $err == *SB5073* ]]; chk 23.27

# 23.28 not just `status` — every value intrinsic.
run_sash 'echo hi; x=$(echo a; getcpid)'
[[ $rc != 0 && -z $out && $err == *SB5073* ]]; chk 23.28

# 23.29 a one-statement body is NOT enough on its own: the intrinsic must be
# the substitution's own statement. Nested inside an `if` arm the body length
# is 1 at every level, yet nothing rewrites it.
run_sash 'echo hi; x=$(if [[ 1 == 1 ]]; then status; fi)'
[[ $rc != 0 && -z $out && $err == *SB5073* ]]; chk 23.29

# 23.30 the exemption itself still holds — this is what 23.24 protects.
run_sash 'echo hi; echo "$(status)"'
[[ $rc = 0 && $out = "hi
0" && -z $err ]]; chk 23.30

# 23.31 and the spelling the diagnostic steers toward keeps working: `$?` is
# the punctuation that reads the same intrinsic, and it is legal mid-body.
run_sash 'echo hi; echo "$(echo a; echo $?)"'
[[ $rc = 0 && $out = "hi
a
0" && -z $err ]]; chk 23.31

# 23.32 a multi-statement body with no intrinsic is untouched.
run_sash 'echo hi; echo "$(echo a; echo b)"'
[[ $rc = 0 && $out = "hi
a
b" && -z $err ]]; chk 23.32

# --- DD#14 compound redirects ---

# 14.4 for...done > file → rejected (SB4021)
run_sash 'for x in a b c; do echo $x; done > /tmp/sash_test_redir'
[[ $rc != 0 && -z $out && -n $err ]]; chk 14.4

# 14.5 while...done | cmd → rejected (SB4022)
run_sash 'int i=0; while (( i < 3 )); do echo $i; (( i++ )); done | /usr/bin/wc -l'
[[ $rc != 0 && -z $out && -n $err ]]; chk 14.5

# 14.6 if...fi > file → rejected (SB4021)
run_sash 'if [[ "yes" == "yes" ]]; then echo hello; fi > /tmp/sash_test_redir2'
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
# spilling the lowered value to a temp before the redirect.

# 14.19 reject: command-substitution redirect target → SB4049 (all modes: parse)
run_sash 'echo x > $(echo f)'
[[ $rc != 0 && -z $out && $err == *SB4049* ]]; chk 14.19

# 14.20 reject: arithmetic redirect target → SB4049
run_sash 'echo x > $(( 1 + 1 ))'
[[ $rc != 0 && -z $out && $err == *SB4049* ]]; chk 14.20

# 14.21 support: `${x:-default}` target (x unset) opens the DEFAULT filename
run_sash 'echo d > ${miss:-r14def}; cat r14def'
[[ $rc = 0 && $out = "d" && -z $err ]]; chk 14.21

# 14.22 support: `${x:-default}` target (x set) opens the SET value, not default
run_sash 'f=r14set; echo s > ${f:-r14fallback}; cat r14set'
[[ $rc = 0 && $out = "s" && -z $err ]]; chk 14.22

# 14.23 support: `$a[i]` subscript redirect target opens the element's filename
run_sash 'array a=(r14zero r14one); echo sub > ${a[1]:?empty}; cat r14one'
[[ $rc = 0 && $out = "sub" && -z $err ]]; chk 14.23

# 14.24 support: append `>> $a[i]` accumulates to the element's filename
run_sash 'array a=(x r14ap); echo l1 > ${a[1]:?empty}; echo l2 >> ${a[1]:?empty}; cat r14ap'
[[ $rc = 0 && $out == *l1* && $out == *l2* && -z $err ]]; chk 14.24

# 14.25 reject: here-strings are dropped (DD#83) → SB4050 (all modes: parse).
# `<<<` is exactly a pipe in sash (no subshell, provenance-based taint, no
# word-splitting) — pure sugar, so it's removed. `cat <<< $x` → `echo $x | cat`.
run_sash 'cat <<< hi'
[[ $rc != 0 && -z $out && $err == *SB4050* ]]; chk 14.25

# --- more redirect-target forms (never-run coverage + a stage-redirect bug) ---

# 14.26 support: `&>file` (parse_ampgt_redirect) — stdout+stderr to a scalar
run_sash 'echo amp &> r14amp; cat r14amp'
[[ $rc = 0 && $out = "amp" && -z $err ]]; chk 14.26

# 14.27 support: single-quoted target `> '…'` (redirect_expr_literal)
run_sash "echo sq > 'r14sq'; cat r14sq"
[[ $rc = 0 && $out = "sq" && -z $err ]]; chk 14.27

# 14.28 support: double-quoted concat target `> "$x.txt"` (redirect_expr_concat)
run_sash 'x=r14dq; echo dq > "${x:?empty}.txt"; cat r14dq.txt'
[[ $rc = 0 && $out = "dq" && -z $err ]]; chk 14.28

# 14.29 BUG FIX: a pipeline-stage redirect with a `$var` target was silently
# DROPPED (the stage went to the pipe instead of the file) — the stage path had
# its own ad-hoc target lowering (ir_expr_to_ssa_value) that hardcoded SSA
# version 0. Now it reuses the top-level lower_ir_redirect_expr.
run_sash 'f=r14st; echo hi > ${f:?empty} | cat; cat r14st'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 14.29

# 14.30 BUG FIX: a pipeline-stage redirect with a `${x:-f}` target (spilled)
run_sash 'echo hi > ${miss:-r14sd} | cat; cat r14sd'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 14.30

# 14.31 BUG FIX: a pipeline-stage redirect with a `$a[i]` target (spilled)
run_sash 'array a=(x r14se); echo hi > ${a[1]:?empty} | cat; cat r14se'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 14.31


# ════════ DD#99 stage 4: `write` — echo without the trailing newline ════════
# DG: "I'd spell echo -n as write." A distinct construct rather than a flag,
# following DD#60/#75: `echo` terminates, `write` does not, neither takes a
# flag. Stage 7 removes the `echo -n` spelling.

# wr.1 writes with no trailing newline
run_sash 'write "a"
write "b"
echo "|"'
[[ $rc = 0 && $out = "ab|" && -z $err ]]; chk wr.1

# wr.2 several arguments are space-joined, as echo does
run_sash 'write "x" "y"
echo "|"'
[[ $rc = 0 && $out = "x y|" && -z $err ]]; chk wr.2

# wr.3 `echo` still terminates — the two differ only in the newline
run_sash 'echo "a"
echo "b"'
[[ $rc = 0 && $out = "a
b" && -z $err ]]; chk wr.3

# wr.4 interpolates like echo
run_sash 'v=hi
write "[$v]"
echo "|"'
[[ $rc = 0 && $out = "[hi]|" && -z $err ]]; chk wr.4

# wr.5 no arguments writes nothing at all
run_sash 'write
echo "|"'
[[ $rc = 0 && $out = "|" && -z $err ]]; chk wr.5

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
run_sash '/usr/bin/printf "x\ny\n" > rd1.txt; /bin/cat < rd1.txt | /bin/cat'
[[ $rc = 0 && $out = "x
y" && -z $err ]]; chk 14.32

# 14.33 …and a middle stage's, overriding what the pipe would have supplied.
# The producing stage is suppressed with `-`: because stage 2 redirects stdin
# from a file it never drains the pipe, so `echo` races SIGPIPE against stage
# 2's exit. DD#55 makes that abort deliberate ("SIGPIPE is the programmer's
# responsibility — use `- cmd` on the producing side"), so without the `-`
# this test failed about 2.5% of runs. Found 2026-09-12; it had already flaked
# once at fe71aa1.
run_sash '/usr/bin/printf "m\n" > rd2.txt; - echo ignored | /bin/cat < rd2.txt | /bin/cat'
[[ $rc = 0 && $out = "m" && -z $err ]]; chk 14.33

# 14.34 the DD#99 idiom: turn a separator into NUL upstream, then split
run_sash '/usr/bin/printf "a\nb\nc\n" > rd3.txt
/usr/bin/tr "\n" "\0" < rd3.txt | read arr
attest e in arr [[ $e == $e ]]
echo "$(len arr):${arr[1]}"'
[[ $rc = 0 && $out = "3:b" && -z $err ]]; chk 14.34

# 14.35 the OUTPUT direction still lands on fd 1, not fd 0
run_sash 'echo captured > rd4.txt | /bin/cat
/bin/cat rd4.txt'
[[ $rc = 0 && $out = "captured" && -z $err ]]; chk 14.35

# 14.36 a redirect on a NON-pipeline command is unaffected
run_sash '/usr/bin/printf "solo\n" > rd5.txt; /bin/cat < rd5.txt'
[[ $rc = 0 && $out = "solo" && -z $err ]]; chk 14.36


# ── DD#99 stage 7: an argv entry may not contain an interior NUL ──────────
# argv crosses execve(2) as C strings, so a NUL-bearing value is truncated
# there — silently, with a zero exit, and the callee receives a DIFFERENT,
# shorter argument than the program computed. sash scalars are byte-
# transparent and DD#99 makes NUL the field separator, so this went from
# exotic to a realistic slip (read a NUL-separated stream, forget to split,
# pass the whole thing). It aborts instead (DD#55).

# an.1 a scalar argument carrying a NUL aborts rather than truncating
run_sash 'nul=$'"'"'\x00'"'"'
s="abc${nul}HIDDEN"
/usr/bin/printf "[%s]" $s'
[[ $rc != 0 && -z $out && $rerr == *NUL* ]]; chk an.1

# an.2 …and so does a SPLAT ELEMENT. Guarding only scalars would be worse
# than no guard: it would imply argv NULs abort while `@a` still truncated.
run_sash 'nul=$'"'"'\x00'"'"'
s="abc${nul}HID"
array a=($s x)
/usr/bin/printf "[%s]" @{a:~}'
[[ $rc != 0 && $rerr == *NUL* ]]; chk an.2

# an.3 control: a clean scalar is unaffected
run_sash '/usr/bin/printf "[%s]" ok'
[[ $rc = 0 && $out = "[ok]" && -z $rerr ]]; chk an.3

# an.4 control: a clean splat is unaffected
run_sash 'array a=(p q)
/usr/bin/printf "[%s]" @{a:~}'
[[ $rc = 0 && $out = "[p][q]" && -z $rerr ]]; chk an.4

# an.5 the NUL is only rejected AT argv — it remains a legitimate value
# in-process, which is the whole basis of DD#99's split
run_sash 'nul=$'"'"'\x00'"'"'
s="a${nul}b"
read $s f
echo "$(strlen $s):$(len f):${f[1]}"'
[[ $rc = 0 && $out = "3:2:b" && -z $err ]]; chk an.5

# ── an auto-declared bulk-read target inside a `while` body ───────────────
# `lower_while` lowers the body twice: pass 1 discovers loop-carried vars,
# then its instructions are cleared and pass 2 re-lowers. Pass 1 had already
# registered the target in `state.collections`, so pass 2's
# `autodeclare_bulk_target` skipped the empty-collection Assign and the fill
# wrote into a slot holding the `""` initializer — a segfault, not a wrong
# answer. Only `read`'s two DD#99 forms reach it; `argv` was unaffected.

# 14.37 `read ARR` auto-declared inside a while body
run_sash '/usr/bin/printf "x y" > wl1.txt
int n=0
while (( n < 1 )) {
  (( n = n + 1 ))
  read ARR < wl1.txt
  echo "n=$(len ARR)"
}'
[[ $rc = 0 && $out = "n=1" && -z $rerr ]]; chk 14.37

# 14.38 `read $s ARR` (the split form) likewise — no I/O involved, so this
# isolates the missing allocation from anything the redirect does
run_sash 'int n=0
while (( n < 1 )) {
  (( n = n + 1 ))
  s="p q"
  read $s ARR
  echo "e=${ARR[0]}"
}'
[[ $rc = 0 && $out = "e=p q" && -z $rerr ]]; chk 14.38

# 14.39 the slot stays valid on every iteration, not just the first
run_sash '/usr/bin/printf "x y" > wl2.txt
int n=0
while (( n < 3 )) {
  (( n = n + 1 ))
  read ARR < wl2.txt
  echo "n=$(len ARR)"
}'
[[ $rc = 0 && $out = "n=1
n=1
n=1" && -z $rerr ]]; chk 14.39

# 14.40 control: an explicit declaration in the same position was never
# affected — it emits its own Assign, which pass 2 re-emits
run_sash '/usr/bin/printf "x y" > wl3.txt
int n=0
while (( n < 1 )) {
  (( n = n + 1 ))
  array ARR=()
  read ARR < wl3.txt
  echo "n=$(len ARR)"
}'
[[ $rc = 0 && $out = "n=1" && -z $rerr ]]; chk 14.40

# ── SB5069: a loop whose condition is a `read` (DD#99) ────────────────────
# `read` consumes the whole stream, so the loop has no second read to make.
# Before this was rejected it compiled clean and spun forever on an empty
# array. `readline v` is the iteration form and the message must name it.

# 14.41 `while read ARR` is rejected
run_sash 'array l=()
while read l {
  echo ITER
}'
[[ $rc != 0 && -z $out && $err == *SB5069* ]]; chk 14.41

# 14.42 the advice names `readline`, which is the whole point of the code
[[ $err == *readline* ]]; chk 14.42

# 14.43 `until read ARR` goes the same way — the defect is looping on a
# slurp, not the sense of the test
run_sash 'until read A {
  echo x
}'
[[ $rc != 0 && -z $out && $err == *SB5069* ]]; chk 14.43

# 14.44 a pipeline ending in `read` is still a read condition
run_sash '/usr/bin/printf "a\n" > wl4.txt
while /usr/bin/tr "\n" "\0" < wl4.txt | read ARR {
  echo "n=$(len ARR)"
}'
[[ $rc != 0 && -z $out && $err == *SB5069* ]]; chk 14.44

# 14.45 `if read ARR` is NOT rejected — testing whether a read produced
# anything is meaningful; only iterating on it is the category error
run_sash '/usr/bin/printf "x y" > wl5.txt
if read ARR < wl5.txt {
  echo "n=$(len ARR)"
}'
[[ $rc = 0 && $out = "n=1" && -z $rerr ]]; chk 14.45

# 14.46 control: `while readline v` is the replacement and still compiles.
# The redirect goes on the producing side — `while readline v < f` re-opens
# `f` every iteration and never advances, in sash exactly as in bash.
# Both backends, as of the __test_result fix (see wordsplit.sh rl.7).
run_sash '/usr/bin/printf "a\nb\n" | while readline v {
  attest v [[ $v =~ ^.*$ ]]
  echo "got $v"
}'
[[ $rc = 0 && $out = "got a
got b" && -z $rerr ]]; chk 14.46

# ── DD#99: the retired read/mapfile surfaces have no implementation ──
#
# The validator rejects them (SB5057 / SB5060); these assert that the
# rejection is the ONLY thing left — no flag is silently accepted, and no
# retired spelling has a working lowering behind it.

# rt99.1 `read` takes no flags at all
run_sash 'array a=()
read -a a'
[[ $rc != 0 && $err = *SB5057* ]]; chk rt99.1

# rt99.2 the flagless `readline NAME` is the surviving spelling
printf 'one\ntwo\n' > rt99.lines
run_sash 'readline x < rt99.lines
attest x [[ -n $x ]]
echo "$x"'
[[ $rc = 0 && $out = "one" && -z $err ]]; chk rt99.2

# rt99.3 `readline -r` is rejected, not accepted as a no-op
run_sash 'readline -r x'
[[ $rc != 0 && $err = *SB5057* ]]; chk rt99.3

# rt99.4 `-d` (record delimiter) is gone
run_sash 'readline -d: x'
[[ $rc != 0 && $err = *SB5057* ]]; chk rt99.4

# rt99.5 `-f` (field separator) is gone
run_sash 'readline -f: x y'
[[ $rc != 0 && $err = *SB5057* ]]; chk rt99.5

# rt99.6 the multi-target form is gone — no whitespace field splitting
run_sash 'readline x y'
[[ $rc != 0 && $err = *SB5057* ]]; chk rt99.6

# rt99.7 mapfile is retired
run_sash 'array l=()
mapfile -t l'
[[ $rc != 0 && $err = *SB5060* ]]; chk rt99.7

# rt99.8 readarray likewise
run_sash 'array l=()
readarray -t l'
[[ $rc != 0 && $err = *SB5060* ]]; chk rt99.8

# rt99.9 `mapfile -m` (the retired map mode) is retired with it
run_sash 'map m=()
mapfile -m m'
[[ $rc != 0 && $err = *SB5060* ]]; chk rt99.9

# rt99.10 `echo -n` is retired at the SOURCE level (SB5059)...
run_sash 'echo -n hi'
[[ $rc != 0 && $err = *SB5059* ]]; chk rt99.10

# rt99.11 ...but `write` still suppresses the newline — the `-n` lowering is
#         INTERNAL (rewrite_write_as_echo), which is why it is not removed.
run_sash 'write a
write b
echo'
[[ $rc = 0 && $out = "ab" && -z $err ]]; chk rt99.11

# rt99.12 DD#99 `read ARRAY` reads NUL-separated fields — the surviving form
printf 'p\0q\0' > rt99.nul
run_sash 'array a=()
read a < rt99.nul
attest x in a [[ -n $x ]]
echo "${a[0]}-${a[1]}"'
[[ $rc = 0 && $out = "p-q" && -z $err ]]; chk rt99.12

# rt99.13 ...and a trailing NUL does not add an empty field
run_sash 'array a=()
read a < rt99.nul
attest x in a [[ -n $x ]]
echo $(len a)'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk rt99.13

# rt99.14 a lastpipe `read ARRAY` fills an ARRAY, not a scalar.
run_sash 'array a=()
/usr/bin/printf "p\0q\0" | read a
attest x in a [[ -n $x ]]
echo "${a[0]}-${a[1]}"'
[[ $rc = 0 && $out = "p-q" && -z $err ]]; chk rt99.14

# ── DD#99: an empty line is a value, not EOF ──────────────────────────
#
# DD#99 fixes `readline` as byte-for-byte `IFS= read -r`, and that succeeds on
# an empty line, binding "". The runtime already draws the line: sash_read_line
# returns (NULL, 0) at true EOF but (non-NULL, 0) for an empty line. Both
# backends used to ignore the pointer — native tested `len > 0` and so treated
# an interior blank line as EOF, silently truncating the input there; c wrote
# the answer to the wrong variable entirely and never stopped. Bash is the
# oracle for all four.

# rt99.15 an interior empty line is bound and the loop continues
run_sash '/usr/bin/printf "a\n\nb\n" | while readline v {
  attest v [[ -z $v || -n $v ]]
  echo "[$v]"
}'
[[ $rc = 0 && $out = "[a]
[]
[b]" && -z $rerr ]]; chk rt99.15

# rt99.16 …including a leading one
run_sash '/usr/bin/printf "\nx\n" | while readline v {
  attest v [[ -z $v || -n $v ]]
  echo "[$v]"
}'
[[ $rc = 0 && $out = "[]
[x]" && -z $rerr ]]; chk rt99.16

# rt99.17 the loop still ends at true EOF — three lines, three iterations
run_sash 'int n=0
/usr/bin/printf "a\n\nb\n" | while readline v {
  attest v [[ -z $v || -n $v ]]
  (( n = n + 1 ))
}
echo "n=$n"'
[[ $rc = 0 && $out = "n=3" && -z $rerr ]]; chk rt99.17

# rt99.18 control: the one-shot `readline` never consulted the EOF predicate,
# so it bound "" correctly even before the fix. It pins that down.
printf '\nsecond\n' > rt99.blank
run_sash 'readline x < rt99.blank
attest x [[ -z $x || -n $x ]]
echo "[$x]"'
[[ $rc = 0 && $out = "[]" && -z $err ]]; chk rt99.18

# rt99.19 `read` with NO target. rt99.1-rt99.6 all pin a rejected *flag*; the
# arity half of the same message had no spec, so the two-arity shape ("a
# target name, optionally preceded by a source value") was never rendered.
run_sash 'read'
[[ $rc != 0 && $err = *SB5057* ]]; chk rt99.19

# rt99.20 …and the other side of the same bound: three words is one too many.
# DD#99 retired the multi-target split, so `read x y z` is an arity error, not
# a field-splitting request.
run_sash 'array a=()
read x y z'
[[ $rc != 0 && $err = *SB5057* ]]; chk rt99.20

# 14.47 a multi-argument `$(echo …)` capture joins with a single space.
# `echo_concat` builds the SSA Concat for this; the whole multi-arg path had
# no spec -- `for x in $(echo a b c)` (cov.61) takes the splat/pipeline route
# instead, so it never reached the value-capture code.
run_sash 'x=$(echo a b)
echo "[$x]"'
[[ $rc = 0 && $out = "[a b]" && -z $err ]]; chk 14.47

# 14.48 three arguments, to pin the separator between every adjacent pair
run_sash 'x=$(echo a b c)
echo "[$x]"'
[[ $rc = 0 && $out = "[a b c]" && -z $err ]]; chk 14.48

# 14.49 `$(echo)` with no arguments captures the empty string, not a newline
run_sash 'x=$(echo)
echo "[$x]"'
[[ $rc = 0 && $out = "[]" && -z $err ]]; chk 14.49

# 14.50 run-together spaces between arguments collapse: they are argument
# separators in the source, and the capture rejoins with exactly one
run_sash 'x=$(echo a    b)
echo "[$x]"'
[[ $rc = 0 && $out = "[a b]" && -z $err ]]; chk 14.50

# 14.51 spaces *inside* a quoted argument survive -- that is one argument,
# so no rejoining happens to it
run_sash 'x=$(echo "a  b")
echo "[$x]"'
[[ $rc = 0 && $out = "[a  b]" && -z $err ]]; chk 14.51

# 14.52 the arguments may be variables rather than literals
run_sash 'y=p
z=q
x=$(echo $y $z)
echo "[$x]"'
[[ $rc = 0 && $out = "[p q]" && -z $err ]]; chk 14.52

# 14.53 empty-string arguments are still arguments: two of them join to the
# one separator between them
run_sash 'x=$(echo "" "")
echo "[$x]"'
[[ $rc = 0 && $out = "[ ]" && -z $err ]]; chk 14.53

# ── a file-descriptor prefix must be ADJACENT to the operator ─────────
# bash binds `N` to a redirect only when the digits touch the operator:
# `2>f` redirects fd 2, but `2 >f` passes `2` as an argument and redirects
# stdout. The parser tested only the operator's token kind, so it ate the
# argument — `echo hi 2 >f` printed `hi` and left the file empty, where
# bash writes `hi 2` to the file. Silent wrong output, both backends.

# fd.1 a SPACED digit is an argument, and the `>` redirects stdout
rm -f fdout
run_sash 'echo hi 2 > fdout'
[[ $rc = 0 && -z $out && $(cat fdout) = "hi 2" ]]; chk fd.1

# fd.2 …including `1`, which would otherwise have aliased stdout silently
rm -f fdout
run_sash 'echo a 1 > fdout'
[[ $rc = 0 && -z $out && $(cat fdout) = "a 1" ]]; chk fd.2

# fd.3 …and a digit naming an fd the program never opened
rm -f fdout
run_sash 'echo a 3 > fdout'
[[ $rc = 0 && -z $out && $(cat fdout) = "a 3" ]]; chk fd.3

# fd.4 two digits are an argument too (the fd test accepts 1-2 digits)
rm -f fdout
run_sash 'echo a b 22 > fdout'
[[ $rc = 0 && -z $out && $(cat fdout) = "a b 22" ]]; chk fd.4

# fd.5 ADJACENT still binds: `2>` sends stderr, leaving stdout alone
rm -f fdout
run_sash 'echo hi 2>fdout'
[[ $rc = 0 && $out = "hi" && ! -s fdout ]]; chk fd.5

# fd.6 whitespace AFTER the operator is irrelevant — only the prefix side
rm -f fdout
run_sash 'echo hi 2> fdout'
[[ $rc = 0 && $out = "hi" && ! -s fdout ]]; chk fd.6

# fd.7 an adjacent `1>` redirects stdout, and consumes the digit
rm -f fdout
run_sash 'echo hi 1>fdout'
[[ $rc = 0 && -z $out && $(cat fdout) = "hi" ]]; chk fd.7

# fd.8 the append form binds the same way
rm -f fdout
run_sash 'echo a 1>fdout
echo b 1>>fdout'
[[ $rc = 0 && -z $out && $(cat fdout) = $'a\nb' ]]; chk fd.8

# fd.9 an adjacent fd prefix really does route stderr, not just swallow it
rm -f fdout
run_sash 'fn g { echo out; echo err >&2; }
g 2>fdout'
[[ $rc = 0 && $out = "out" && $(cat fdout) = "err" ]]; chk fd.9

# fd.10 trailing-redirect position: an ADJACENT fd prefix is still a
# redirect, so an assignment carrying one is SB4046 (not a stray command)
run_sash 'x=1 2>fdout
echo $x'
[[ $rc != 0 && $err == *SB4046* ]]; chk fd.10

# fd.11 …and a SPACED one is not a redirect at all, so `2` reads as the
# command it is — which is bash's reading too (`2: command not found`)
run_sash 'x=1 2 >fdout
echo $x'
[[ $rc != 0 && $err == *SB5011* ]]; chk fd.11

rm -f fdout

# ── csr.*: redirects on a command substitution ────────────────────────────
# `$(cmd > f)` used to SILENTLY DROP the redirect: the output that belonged in
# the file was captured instead, and the file was never created. Two separate
# defects — native's interner never walked a `CmdSubst`'s redirect targets (and
# `emit_open_redirect` skipped a redirect whose literal was missing rather than
# failing), while the C backend discarded the redirects outright and called the
# redirect-less `sash_capture_status`. A redirected capture now declines the
# single-command fast path and lowers through `CmdSubstBody`, the same path
# `$(a | b > f)` always took and the only one both backends implement.
# Every case below was differentially checked against bash.

# csr.1 stdout to a file: capture is EMPTY, file gets the output (builtin head)
run_sash 'str x=$(echo hi > csr1f)
attest x [[ -z $x || -n $x ]]
echo "cap=[$x]"
cat csr1f'
[[ $rc = 0 && $out = $'cap=[]\nhi' && -z $err ]]; chk csr.1

# csr.2 same with an EXTERNAL head (the fork/exec capture path)
run_sash 'str x=$(/bin/echo hi > csr2f)
attest x [[ -z $x || -n $x ]]
echo "cap=[$x]"
cat csr2f'
[[ $rc = 0 && $out = $'cap=[]\nhi' && -z $err ]]; chk csr.2

# csr.3 append inside a capture does not truncate
run_sash 'echo a > csr3f
str x=$(/bin/echo b >> csr3f)
attest x [[ -z $x || -n $x ]]
echo "cap=[$x]"
cat csr3f'
[[ $rc = 0 && $out = $'cap=[]\na\nb' && -z $err ]]; chk csr.3

# csr.4 stdin redirect feeds the captured command (capture was empty)
run_sash 'echo fromfile > csr4f
str x=$(/bin/cat < csr4f)
attest x [[ -n $x ]]
echo "cap=[$x]"'
[[ $rc = 0 && $out = "cap=[fromfile]" && -z $err ]]; chk csr.4

# csr.5 2>&1 folds stderr into the capture (this one always worked on native
#       and was always dropped by the C backend — a silent mode divergence)
run_sash 'str x=$(/bin/sh -c "echo e >&2" 2>&1)
attest x [[ -n $x ]]
echo "cap=[$x]"'
[[ $rc = 0 && $out = "cap=[e]" && -z $err ]]; chk csr.5

# csr.6 2>file diverts only stderr; stdout still reaches the capture
run_sash 'str x=$(/bin/sh -c "echo o; echo e >&2" 2>csr6f)
attest x [[ -n $x ]]
echo "cap=[$x]"
cat csr6f'
[[ $rc = 0 && $out = $'cap=[o]\ne' && -z $err ]]; chk csr.6

# csr.7 a NON-literal target (the `redirect_s` path, computed ptr+len)
run_sash 'str f=csr7f
str x=$(/bin/echo dyn > ${f:?empty})
attest x [[ -z $x || -n $x ]]
echo "cap=[$x]"
cat csr7f'
[[ $rc = 0 && $out = $'cap=[]\ndyn' && -z $err ]]; chk csr.7

# csr.8 >/dev/null discards rather than captures
run_sash 'str x=$(/bin/echo hi > /dev/null)
attest x [[ -z $x || -n $x ]]
echo "cap=[$x]"'
[[ $rc = 0 && $out = "cap=[]" && -z $err ]]; chk csr.8

# csr.9 DD#55: a redirected capture still aborts on a nonzero exit, exactly as
#       the unredirected form does — routing through the body path must not
#       have quietly dropped the check.
run_sash 'str x=$(/bin/false > csr9f)
attest x [[ -z $x || -n $x ]]
echo "after=[$x]"'
[[ $rc != 0 && $out != *after* && $rerr == *"/bin/false"* ]]; chk csr.9

rm -f csr1f csr2f csr3f csr4f csr6f csr7f csr9f

# ── DD#119: a redirect target must be provably non-empty ───────────────
# The old rule asked "is this a scalar?", a syntactic proxy that admitted an
# empty `$p` (which reached runtime as `: No such file or directory`). A
# disposition is the proof mechanism, so an undispositioned read is rejected.
run_sash 'str p=d119a
/bin/echo hi > ${p:?empty}
/bin/cat d119a'
[[ $rc = 0 && $out = hi && -z $err ]]; chk 14.54
# a bare string read proves nothing, even though it is perfectly non-empty here
run_sash 'str p=d119b
/bin/echo hi > $p'
[[ $rc != 0 && $err == *"may be empty"* ]]; chk 14.55
# `:-` with a non-empty constant default proves it; an EMPTY default does not.
# This is the hole a "did you write a disposition" rule would leave open.
run_sash 'str p=""
/bin/echo hi > ${p:-d119c}
/bin/cat d119c'
[[ $rc = 0 && $out = hi && -z $err ]]; chk 14.56
run_sash 'str p=""
/bin/echo hi > ${p:-}'
[[ $rc != 0 && $err == *"empty default"* ]]; chk 14.57
# an empty constant target is observably the bug
run_sash '/bin/echo hi > ""'
[[ $rc != 0 && $err == *"is empty"* ]]; chk 14.58
# an int is non-empty BY TYPE, so it needs no disposition — but it still needs
# quotes to be text (DG: "exempt but still needs quotes to make a string")
run_sash 'int n=119
/bin/echo hi > "$n".d119
/bin/cat 119.d119'
[[ $rc = 0 && $out = hi && -z $err ]]; chk 14.59
run_sash 'int n=119
/bin/echo hi > $n.d119'
[[ $rc != 0 && $err == *"not text"* ]]; chk 14.60
# strict concatenation: a literal neighbour does NOT discharge the obligation.
# Lenient would write to `.d119` here — a silent hidden file, its own surprise.
run_sash 'str p=d119d
/bin/echo hi > $p.d119'
[[ $rc != 0 && $err == *"may be empty"* ]]; chk 14.61
# a cell read behaves like a string variable, so it carries the same obligation
run_sash 'array a=(d119e)
/bin/echo hi > $a[0]'
[[ $rc != 0 && $err == *"cell read"* ]]; chk 14.62
# a command substitution target is still rejected at parse by SB4049 — DD#119
# does not weaken that rule, it explains it
run_sash '/bin/echo hi > $(/bin/echo d119f)'
[[ $rc != 0 && $err == *SB4049* ]]; chk 14.63

rm -f d119a d119c 119.d119

# --- DD#119's int exemption, dispositioned (bug: c backend only) ---
# DD#119 exempts ints from the non-empty proof ("exempt but still needs
# quotes to make a string") -- so `> "${n:?msg}"` is the spelling the rule
# directs authors to. It did not work: the c backend emitted `n_ptr`/`n_len`,
# which are the string slots of a VarSlot, and an int has neither. That is a
# hard COMPILE error in the generated C, not a wrong answer. Native was fine,
# so only the differential oracle could see it.
#
# `load_var_or_empty_c` built `{base}_ptr`/`{base}_len` without consulting the
# var's type -- the same fault as `latest_var_as_str` (fixed in 006fce2), in
# the second of the two functions that render a slot as text. They now share
# `var_slot_as_str_c` so they cannot disagree about an int again.
#
# These assert FILE CONTENTS, not just rc: the native failure mode for a
# broken redirect is to write to stdout, which a status-only check misses.

run_sash 'int n=5
/bin/echo hi > "${n:?no n}"
/bin/echo "wrote=$n"'
w=${out#wrote=}
[[ $rc == 0 && $w == 5 && -f $w && $(cat "$w") == hi ]]; chk 14.64
rm -f 5
# 14.65/14.66 are NOT proofs of this fix: neutering the type consultation
# leaves both green. `:-` on an int is a ternary (DD#62 meaningful zero), and
# an int cell goes through lower_value_as_str, so neither reaches
# load_var_or_empty_c. Kept as true assertions of the int exemption, labelled
# so they are not mistaken for coverage of the renderer. 14.64 and 14.67 are
# the proofs; 14.59 above fails under the same neuter, which is what confirms
# the two renderers really do share var_slot_as_str_c now.
# the same via `:-`, whose int arm is a ternary (DD#62 meaningful zero)
run_sash 'int n=6
/bin/echo hi > "${n:-9}"
/bin/echo "wrote=$n"'
w=${out#wrote=}
[[ $rc == 0 && $w == 6 && -f $w && $(cat "$w") == hi ]]; chk 14.65
rm -f 6
# an int cell carries the same obligation and the same rendering
run_sash 'int array a=(4 8)
/bin/echo hi > "${a[1]:?no cell}"
/bin/echo "wrote=${a[1]}"'
w=${out#wrote=}
[[ $rc == 0 && $w == 8 && -f $w && $(cat "$w") == hi ]]; chk 14.66
rm -f 8
# and the plain read path (emit_param_expansion), the other caller of the
# function that was broken -- not a redirect, so it pins the fix rather than
# the rule
run_sash 'int n=5
/bin/echo "v=${n:?no n}"'
[[ $rc == 0 && $out == "v=5" && -z $err ]]; chk 14.67

# --- DD#119: the paths the proof takes that 14.54-14.63 never exercised ---
# 14.55 rejects a BARE `$p`, which the type checker sees as a SimpleExpansion
# and never routes through `disposition_proof`. The braced spelling is a
# ParameterExpansion with no operator and takes a different arm; before this
# spec nothing in the suite reached it, so the two spellings were free to
# drift apart.
run_sash 'str p=d119g
/bin/echo hi > ${p}'
[[ $rc != 0 && $err == *"may be empty"* ]]; chk 14.68
# An operator the parser stores under an INTERNAL name. `${p:0:2}` is recorded
# as the operator "substring", and the diagnostic used to interpolate that
# straight into the subject, rendering `${psubstring…}` — syntax no author
# wrote and none could. Assert the absence, not just the rejection: a test
# that only checked `rc != 0` passed throughout the bug.
run_sash 'str p=d119hxx
/bin/echo hi > ${p:0:2}'
[[ $rc != 0 && $err != *'${psubstring'* && $err == *"substring"*"does not prove a filename"* ]]; chk 14.69
# Control: the same substring is perfectly legal away from a redirect target,
# so 14.69 pins the non-empty OBLIGATION, not a ban on the operator.
run_sash 'str p=d119hxx
/bin/echo "${p:0:2}"'
[[ $rc == 0 && $out == d1 && -z $err ]]; chk 14.70
# `:~` as a redirect target is SB5046 (an elide is legal only where a token may
# disappear), one pass before DD#119's proof ever sees it. That ordering is
# what makes the proof's `:~` arm a should-never-run ICE, so this spec pins the
# precondition: if SB5046 ever stops firing here, this fails rather than the
# compiler crashing on a user's program.
run_sash 'str p=d119i
/bin/echo hi > ${p:~}'
[[ $rc != 0 && $err == *SB5046* ]]; chk 14.71

# ── DD#126: `wait` is a non-aborting, pid-returning int builtin ──────────
# `wait` is wait(2); `wait PID` is waitpid(2). The value is the reaped pid,
# or -1 on ECHILD/error — it never aborts. The reaped child's exit lands in
# `$?` / `$(status)`, and on -1 the status is left untouched.

# 126.1 the pid is a value, so a bare `wait` statement is SB4016 — the same
#       rule that rejects a bare call to an int-returning function. `-` is
#       the discard, and DG's "`-` wait for I-don't-care" only has content
#       if the bare form is not also it.
run_sash 'echo bg &
wait'
[[ $rc != 0 && $err == *SB4016* ]]; chk 126.1

# 126.2 `- wait` is the discard
run_sash 'echo bg &
- wait
echo done'
[[ $rc = 0 && $out = "bg
done" && -z $err ]]; chk 126.2

# 126.3 captured, the value is the reaped pid
run_sash '- /bin/true &
int p=$(wait)
echo "pos=$(( p > 0 ))"'
[[ $rc = 0 && $out = "pos=1" && -z $err ]]; chk 126.3

# 126.4 no children: -1, and NOT an abort. This is the half the two backends
#       used to disagree about (notes/tasks/bug-wait-backends-diverge-no-children).
run_sash 'int p=$(wait)
echo "[$p]"'
[[ $rc = 0 && $out = "[-1]" && -z $err ]]; chk 126.4

# 126.5 the reaped child's exit is readable through `$(status)` even after
#       the `-` form — `-` discards the *pid*, which is a different thing
#       from the status. `wait` is the only command where those two differ.
run_sash '- /bin/sh -c "exit 3" &
- wait
int s=$(status)
echo "s=$s"'
[[ $rc = 0 && $out = "s=3" && -z $err ]]; chk 126.5

# 126.6 and after the capture form
run_sash '- /bin/sh -c "exit 5" &
int p=$(wait)
int s=$(status)
echo "pos=$(( p > 0 )) s=$s"'
[[ $rc = 0 && $out = "pos=1 s=5" && -z $err ]]; chk 126.6

# 126.7 a reap that found nothing says nothing about any child: the status
#       survives the failed wait. Both backends seed the out-param with the
#       current `$?` and write it back unconditionally, so this falls out of
#       the ABI with no branch on either side.
run_sash '- /bin/false
int p=$(wait)
int s=$(status)
echo "[$p] s=$s"'
[[ $rc = 0 && $out = "[-1] s=1" && -z $err ]]; chk 126.7

# 126.8 "wait for all" is a loop, not a flag (DD#126 declines to invent a
#       job-control surface). The extra parens are the call spelling.
run_sash '- /bin/true &
- /bin/true &
- /bin/true &
int n=0
while (( (wait) > 0 )) { n=$(( n + 1 )) }
echo "reaped=$n"'
[[ $rc = 0 && $out = "reaped=3" && -z $err ]]; chk 126.8

# 126.9 and the bare word is the same intrinsic (DD#125)
run_sash '- /bin/true &
- /bin/true &
int n=0
while (( wait > 0 )) { n=$(( n + 1 )) }
echo "reaped=$n"'
[[ $rc = 0 && $out = "reaped=2" && -z $err ]]; chk 126.9

# 126.10 the drain loop terminates immediately with no children
run_sash 'int n=0
while (( wait > 0 )) { n=$(( n + 1 )) }
echo "reaped=$n"'
[[ $rc = 0 && $out = "reaped=0" && -z $err ]]; chk 126.10

# 126.11 `wait PID` is waitpid(2) — it returns the pid it was given, and the
#        status is that child's. The argument reaches the backends as a
#        lowered int, so `$(getcpid)` and a variable both work.
run_sash '- /bin/sh -c "exit 7" &
int c=$(getcpid)
int r=$(wait $c)
int s=$(status)
echo "same=$(( r == c )) s=$s"'
[[ $rc = 0 && $out = "same=1 s=7" && -z $err ]]; chk 126.11

# 126.12 the sexpr spelling, with the argument as an arithmetic expression
#        rather than a word. `(len a)` names a collection, so the shared
#        reconstruction turns a sexpr operand into a literal; `(wait p)`
#        means the *value* of p, which is why it lowers on its own path.
run_sash '- /bin/sh -c "exit 9" &
int r=$(( (wait (getcpid)) ))
int s=$(status)
echo "pos=$(( r > 0 )) s=$s"'
[[ $rc = 0 && $out = "pos=1 s=9" && -z $err ]]; chk 126.12

# 126.13 waiting on a pid that is not our child is -1, not an abort
run_sash 'int r=$(wait 999999)
echo "[$r]"'
[[ $rc = 0 && $out = "[-1]" && -z $err ]]; chk 126.13

# 126.14-126.16 arity, in all three positions. The statement spelling is the
#        one the value-intrinsic arity checks cannot see; before DD#126 it
#        discarded the surplus in silence.
run_sash '- wait 1 2'
[[ $rc != 0 && $err == *SB4045* ]]; chk 126.14
run_sash 'int r=$(wait 1 2)
echo $r'
[[ $rc != 0 && $err == *SB4045* ]]; chk 126.15
run_sash 'int r=$(( (wait 1 2) ))
echo $r'
[[ $rc != 0 && $err == *SB4045* ]]; chk 126.16

# 126.17 the operand is an int sink, so a string is SB4030 in the `-` form
#        too (`wait $s` bare is isink.6)
run_sash 'str s=1
- wait $s'
[[ $rc != 0 && $err == *SB4030* ]]; chk 126.17

# 126.18 a boolean context consumes a status, and `wait`'s value is a pid, so
#        `if wait` would read "the reaped pid was 0". SB4016 has no
#        condition escape here, unlike the int-returning-function case.
run_sash '- /bin/true &
if wait { echo y }'
[[ $rc != 0 && $err == *SB4016* ]]; chk 126.18

# 126.19 the pid is clean — it is compiler-managed, like `$$` and `$!`, not
#        a value read out of the environment
run_sash '- /bin/true &
int p=$(wait)
if (( p > 0 )) { echo ok }'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk 126.19

# 126.20 `wait` is reserved (DD#125), so it cannot be shadowed by a name
run_sash 'int wait=1
echo $wait'
[[ $rc != 0 && $err == *SB5092* ]]; chk 126.20

set -e
tally
