#!/bin/bash
# spec/commands.sh — command resolution and pipeline tests (DD#14, DD#23, DD#54)
set -eu
. common.sh
setup commands
plan=112
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
# 14.3b subshell isolation — an assignment inside the body does NOT leak
run_sash 'x=outer; y=$(x=inner; echo $x); echo "y=$y x=$x"'
[[ $rc = 0 && $out = "y=inner x=outer" && -z $err ]]; chk 14.3b
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

# --- DD#111 a value intrinsic is exempt from SB5073 only when it IS the
# substitution. `bareword_shortcut_expansion` rewrites `$(status)` into the
# value it produces, and that rewrite requires the body to be exactly one
# statement. One statement among several was not rewritten, survived as a bare
# command no backend recognises, and aborted at runtime inside `sash_exec` —
# printing `sash: bug: bare command passed to sash_exec` into the program's
# OWN output, with a successful exit. ---

# 23.25 the shape that used to leak the internal abort message.
run_sash 'echo hi; echo "$(echo a; status)"'
[[ $rc != 0 && -z $out && $err == *SB5073* ]]; chk 23.25

# 23.26 order does not matter — first statement is just as unrewritten.
run_sash 'echo hi; echo "$(status; echo a)"'
[[ $rc != 0 && -z $out && $err == *SB5073* ]]; chk 23.26

# 23.27 the UNQUOTED spelling too. A sole `$(status)` is rewritten while the
# word is scanned (try_special_word) and never reaches the string-content
# path, so the two spellings take different routes to the same rule.
run_sash 'echo hi; echo $(echo a; status)'
[[ $rc != 0 && -z $out && $err == *SB5073* ]]; chk 23.27

# 23.28 not just `status` — every value intrinsic.
run_sash 'echo hi; x=$(echo a; getcpid)'
[[ $rc != 0 && -z $out && $err == *SB5073* ]]; chk 23.28

# 23.29 a one-statement body is NOT enough on its own: the intrinsic must be
# the substitution's own statement. Nested inside an `if` arm the body length
# is 1 at every level, yet nothing rewrites it.
run_sash 'echo hi; x=$(if [[ 1 = 1 ]]; then status; fi)'
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
run_sash 'array a=(r14zero r14one); echo sub > $a[1]; cat r14one'
[[ $rc = 0 && $out = "sub" && -z $err ]]; chk 14.23

# 14.24 support: append `>> $a[i]` accumulates to the element's filename
run_sash 'array a=(x r14ap); echo l1 > $a[1]; echo l2 >> $a[1]; cat r14ap'
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
run_sash 'x=r14dq; echo dq > "$x.txt"; cat r14dq.txt'
[[ $rc = 0 && $out = "dq" && -z $err ]]; chk 14.28

# 14.29 BUG FIX: a pipeline-stage redirect with a `$var` target was silently
# DROPPED (the stage went to the pipe instead of the file) — the stage path had
# its own ad-hoc target lowering (ir_expr_to_ssa_value) that hardcoded SSA
# version 0. Now it reuses the top-level lower_ir_redirect_expr.
run_sash 'f=r14st; echo hi > $f | cat; cat r14st'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 14.29

# 14.30 BUG FIX: a pipeline-stage redirect with a `${x:-f}` target (spilled)
run_sash 'echo hi > ${miss:-r14sd} | cat; cat r14sd'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 14.30

# 14.31 BUG FIX: a pipeline-stage redirect with a `$a[i]` target (spilled)
run_sash 'array a=(x r14se); echo hi > $a[1] | cat; cat r14se'
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
attest e in arr [[ $e = $e ]]
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
run_sash 'nul=$'"'"'\0'"'"'
s="abc${nul}HIDDEN"
/usr/bin/printf "[%s]" $s'
[[ $rc != 0 && -z $out && $rerr == *NUL* ]]; chk an.1

# an.2 …and so does a SPLAT ELEMENT. Guarding only scalars would be worse
# than no guard: it would imply argv NULs abort while `@a` still truncated.
run_sash 'nul=$'"'"'\0'"'"'
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
run_sash 'nul=$'"'"'\0'"'"'
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

set -e
tally
