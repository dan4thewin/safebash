#!/bin/bash
# spec/pipe_functions.sh — user functions as pipeline stages (bug-function-in-
# pipeline). A `Command` stage that names a known function must run as a forked
# compound child (like a `{ }` group), not be exec'd as a bare command.
#
# Both backends. This was native-only while the c backend had no
# compound-pipeline-stage support: a `__compound_<id>` stage name fell through
# to `sash_exec("__compound_2", …)` and died with `sash: bug: bare command
# passed to sash_exec`. The c emitter now jumps to the compound block's
# `_blk_<id>:` label in the forked child, mirroring native's jump.
set -eu
. common.sh
setup pipe_functions
plan=179
echo "1..$plan" >&3

set +e

# pf.1 function as pipeline PRODUCER
run_sash 'fn f { echo hi }
f | cat'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk pf.1

# pf.2 function as pipeline CONSUMER
run_sash 'fn f { cat }
echo hi | f'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk pf.2

# pf.3 function as a MIDDLE stage
run_sash 'fn m { cat }
echo hi | m | cat'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk pf.3

# pf.4 producer function with one arg ($1 binds)
run_sash 'fn f { echo $1 }
f x | cat'
[[ $rc = 0 && $out = "x" && -z $err ]]; chk pf.4

# pf.5 producer function with two args
run_sash 'fn f { echo $1-$2 }
f a b | cat'
[[ $rc = 0 && $out = "a-b" && -z $err ]]; chk pf.5

# pf.6 the SB4022 escape hatch: a multi-statement compound wrapped in a
# function AND piped runs (this is the idiom the compiler recommends)
run_sash 'fn g { echo one; echo two }
g | cat'
[[ $rc = 0 && $out = $'one\ntwo' && -z $err ]]; chk pf.6

# pf.7 a global variable is readable inside the forked function stage
run_sash 'g=hey
fn f { echo $g }
f | cat'
[[ $rc = 0 && $out = "hey" && -z $err ]]; chk pf.7

# pf.8 both producer and consumer are functions
run_sash 'fn p { echo hi }
fn c { cat }
p | c'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk pf.8

# pf.9 function stage assignments do NOT leak to the parent (subshell)
run_sash 'x=parent
fn f { x=child; echo in-fn }
f | cat
echo $x'
[[ $rc = 0 && $out = $'in-fn\nparent' && -z $err ]]; chk pf.9

# pf.10 a function producing multiple lines feeds a real consumer
run_sash 'fn f { echo a; echo b; echo c }
f | grep b'
[[ $rc = 0 && $out = "b" && -z $err ]]; chk pf.10

# pf.11 function producer feeding a while-read consumer (build_producer_stages)
run_sash 'fn f { echo a; echo b }
f | while readline x; do attest x [[ -n $x ]]; echo "got $x"; done'
[[ $rc = 0 && $out = $'got a\ngot b' && -z $err ]]; chk pf.11

# ── compound stages in both backends ──────────────────────────────────
#
# A compound stage is lowered to its own SSA block and named `__compound_<id>`.
# Native jumps to that block in the forked child; the c emitter now emits the
# matching `goto _blk_<id>`. Before that it fell through to the external-exec
# path and died with `sash: bug: bare command passed to sash_exec`.

# pf.12 a function stage's exit status becomes the pipeline status.
# The DD#55 `command failed` diagnostic is native-only for now
# (c-dd55-abort-channel), so only native asserts on stderr.
run_sash 'fn bad { exit 3 }
bad | /bin/cat'
[[ $rc = 3 && -z $out ]] &&
  { ! in_modes native || [[ $rerr == *"command failed (exit 3)"* ]]; }; chk pf.12

# pf.13 a loop inside the function stage
run_sash 'fn gen {
  int i=0
  while (( i < 3 )) {
    echo "n$i"
    (( i = i + 1 ))
  }
}
gen | /bin/cat'
[[ $rc = 0 && $out = $'n0\nn1\nn2' && -z $rerr ]]; chk pf.13

# pf.14 two compound stages back to back
run_sash 'fn a { echo hi }
fn b { /bin/cat }
a | b'
[[ $rc = 0 && $out = "hi" && -z $rerr ]]; chk pf.14

# pf.15 a compound stage in the middle of a three-stage pipeline
run_sash 'fn mid { /bin/cat }
/bin/echo m | mid | /bin/cat'
[[ $rc = 0 && $out = "m" && -z $rerr ]]; chk pf.15

# --- statement forms are not pipeline stages (SB4022) -----------------------
# A stage runs in a forked child, so a test's status is discarded and an
# arithmetic assignment is lost. `[[` and `attest` were worse than useless:
# they reached codegen as a command *named* `[[`, compiled clean, and died at
# run time on the runtime's own `bare command passed to sash_exec` assertion —
# the identical failure this file's header records for functions.

# pf.16 `[[ … ]]` as the pipeline producer
run_sash 'str x=a
[[ -n $x ]] | /bin/cat'
[[ $rc != 0 && -z $out && $err == *SB4022* ]]; chk pf.16

# pf.17 `[[ … ]]` as the pipeline consumer
run_sash 'str x=a
/bin/echo z | [[ -n $x ]]'
[[ $rc != 0 && -z $out && $err == *SB4022* ]]; chk pf.17

# pf.18 `(( … ))` assignment as the producer — silently lost the assignment
run_sash 'int n=1
(( n = 2 )) | /bin/cat
echo $n'
[[ $rc != 0 && -z $out && $err == *SB4022* ]]; chk pf.18

# pf.19 `(( … ))` as the consumer — aborted the producer with SIGPIPE
run_sash 'int n=1
/bin/echo z | (( n = 2 ))
echo $n'
[[ $rc != 0 && -z $out && $err == *SB4022* ]]; chk pf.19

# pf.20 `attest` as a pipeline stage
run_sash 'str x=a
attest x [[ -n $x ]] | /bin/cat'
[[ $rc != 0 && -z $out && $err == *SB4022* ]]; chk pf.20

# pf.21 the `-` suppression wrapper does not smuggle a test into a stage
run_sash 'str x=a
- [[ -n $x ]] | /bin/cat'
[[ $rc != 0 && -z $out && $err == *SB4022* ]]; chk pf.21

# pf.22 a test in the MIDDLE of a three-stage pipeline
run_sash 'str x=a
/bin/echo z | [[ -n $x ]] | /bin/cat'
[[ $rc != 0 && -z $out && $err == *SB4022* ]]; chk pf.22

# pf.23 guard against over-rejection: `&&`/`||` on a test is still legal —
# only the pipe discards the result, so only the pipe is rejected.
run_sash 'str x=a
[[ -n $x ]] && echo hit'
[[ $rc = 0 && $out = "hit" && -z $rerr ]]; chk pf.23

# --- DD#122: a pipe binds to the READER, not to fd 0 ------------------------
# `readline`/`read` with no leading pipe reads fd 0; `cmd | while readline v`
# points that `readline` at the pipe; a command inside the loop body still
# reads fd 0, because the loop runs in the PARENT (which is the whole point —
# see pf.26). The pipe therefore attaches by rewriting a reader's fd to the
# -1 sentinel, and the two rewrites used to walk EVERY block lowered so far.

# pf.24 a `readline` with no pipe is not captured by a later pipeline.
# The first pipeline's consumer reads one line and leaves `LEFT` in the pipe,
# so a mis-aimed `readline mid` succeeds and reads it; correctly aimed at fd 0
# (the harness runs binaries on /dev/null) it hits EOF. The bug needed BOTH
# pipelines: delete the third line and the identical statement read fd 0.
run_sash '/bin/printf "x\nLEFT\n" | readline a
attest a [[ -n $a ]]
if readline mid; then echo "MID-READ"; else echo "MID-EOF"; fi
attest mid [[ -z $mid ]]
echo "MID=[$mid]"
/bin/printf "y\n" | while readline b; do attest b [[ -n $b ]]; echo "B=$b"; done'
[[ $rc = 0 && $out = "MID-EOF
MID=[]
B=y" && -z $rerr ]]; chk pf.24

# pf.25 the same for the ARRAY half of the read family — `rewrite_lastpipe_
# consumer_fds` had the identical whole-program walk.
run_sash '/bin/printf "x\nLEFT\n" | readline a
attest a [[ -n $a ]]
read arr
echo "N=$(len arr)"
/bin/printf "y\0" | read brr
echo "M=$(len brr)"'
[[ $rc = 0 && $out = "N=0
M=1" && -z $rerr ]]; chk pf.25

# pf.26 the property the whole design protects: the loop runs in the PARENT,
# so assignments survive it. bash prints 0 here — this is its most notorious
# pipeline gotcha, and safebash deliberately does not reproduce it.
run_sash 'int n=0
/bin/printf "a\nb\nc\n" | while readline v; do attest v [[ -n $v ]]; (( n = n + 1 )); done
echo "n=$n"'
[[ $rc = 0 && $out == *"n=3" && -z $rerr ]]; chk pf.26

# pf.27 the reader may be in the loop BODY rather than the condition — what
# binds the pipe is the reader, not the `while` header.
run_sash 'int n=0
/bin/printf "a\nb\n" | while (( n < 1 )); do readline v; attest v [[ -n $v ]]; echo "v=$v"; (( n = n + 1 )); done'
[[ $rc = 0 && $out = "v=a" && -z $rerr ]]; chk pf.27

# --- the final stage must consume the pipe (SB4022 / SB4073) ----------------
# A compound final stage is FORKED, so it reads the pipe and then throws away
# everything it assigned — the bundling DD#38 rejects `( … )` for, arriving
# through a different syntactic door. `cmd | { readline g; }` ran, read a
# line, and left `g` unset with no diagnostic.

# pf.28 a `{ }` group as the final stage
run_sash '/bin/printf "a\n" | { readline g; attest g [[ -n $g ]]; echo "g=$g" }'
[[ $rc != 0 && -z $out && $err == *SB4022* ]]; chk pf.28

# pf.29 an `if` as the final stage — took the right branch, still lost `g`
run_sash '/bin/printf "a\n" | if readline g; then echo br; fi'
[[ $rc != 0 && -z $out && $err == *SB4022* ]]; chk pf.29

# pf.30 a `for` as the final stage
run_sash '/bin/printf "a\n" | for x in 1 2; do echo $x; done'
[[ $rc != 0 && -z $out && $err == *SB4022* ]]; chk pf.30

# pf.31 a `while` that never reads the pipe — the loop runs in the parent and
# nothing binds the pipe, so the producer's output went nowhere, silently.
run_sash 'int n=0
/bin/printf "a\n" | while (( n < 1 )); do /bin/cat; (( n = n + 1 )); done'
[[ $rc != 0 && -z $out && $err == *SB4073* ]]; chk pf.31

# pf.32 a reader nested in an inner PIPELINE does not count as the outer
# stage's reader — it binds to its own pipe. SB4073, not a silent discard.
run_sash 'int n=0
/bin/printf "a\n" | while (( n < 1 )); do /bin/echo z | readline w; (( n = n + 1 )); done'
[[ $rc != 0 && -z $out && $err == *SB4073* ]]; chk pf.32

# pf.33 guard against over-rejection: the SB4022 advice is "wrap it in a
# function", so the function form must stay legal. It is also exactly what
# bash does with the `{ }` — both print only `a`, `head` having consumed the
# buffer.
run_sash 'fn f { /bin/head -n1; /bin/cat }
/bin/printf "a\nb\nc\n" | f'
[[ $rc = 0 && $out = "a" && -z $rerr ]]; chk pf.33

# pf.34 guard against over-rejection: the two DD#99 lastpipe readers are
# ordinary commands and stay legal as the final stage.
run_sash '/bin/printf "a\0b\0" | read arr
echo "n=$(len arr)"'
[[ $rc = 0 && $out = "n=2" && -z $rerr ]]; chk pf.34

# pf.35-pf.66 (journal 537; batch-531 B12): a stage runs in a forked child, so
# a builtin that acts on this process does nothing useful there. Only the last
# stage's reader runs in this process (DD#122), and it may carry `-`, or `!`
# where a condition consumes it. A `-` discards the reader's status, so a
# condition's verdict is the earlier stages' alone (pf.61-64). Unless noted,
# each negative compiled and died with "bug: bare command passed to sash_exec".
# pf.35 `return` as the last stage
run_sash 'fn g { /bin/echo x | return 3 }
g
echo "st=$?"'
[[ $rc != 0 && -z $out \
   && $err == *':1:22: error [SB4022] `return` isn'\''t allowed as a pipeline stage — a stage runs in a forked child, so `return` would act on the child'* ]]; chk pf.35
# pf.36 `return` as an earlier stage. The SB4072 that follows is a known
#       parse-error cascade, as in pf.37
run_sash 'fn g {
  return 3 | /bin/cat
  echo in-g
}
g
echo done'
[[ $rc != 0 && -z $out \
   && $err == *':2:3: error [SB4022] `return` isn'\''t allowed as a pipeline stage — a stage runs in a forked child, so `return` would act on the child'* ]]; chk pf.36
# pf.37 `break`
run_sash 'for i in 1 2 {
  break | /bin/cat
  echo $i
}
echo after'
[[ $rc != 0 && -z $out \
   && $err == *':2:3: error [SB4022] `break` isn'\''t allowed as a pipeline stage — a stage runs in a forked child, so `break` would act on the child'* ]]; chk pf.37
# pf.38 `exit`
run_sash 'exit 3 | /bin/cat'
[[ $rc != 0 && -z $out \
   && $err == *':1:1: error [SB4022] `exit` isn'\''t allowed as a pipeline stage — a stage runs in a forked child, so `exit` would act on the child'* ]]; chk pf.38
# pf.39 `readline` as an earlier stage (was SB5020)
run_sash 'readline x | /bin/cat
echo "[$x]"'
[[ $rc != 0 && -z $out \
   && $err == *':1:1: error [SB4022] `readline` runs in this process only as a pipeline'\''s last stage'* ]]; chk pf.39
# pf.40 `cd`
run_sash 'cd /tmp | /bin/cat
echo after'
[[ $rc != 0 && -z $out \
   && $err == *':1:1: error [SB4022] `cd` isn'\''t allowed as a pipeline stage — a stage runs in a forked child, so `cd` would act on the child'* ]]; chk pf.40
# pf.41 an assignment stage is named as one; the message called it
#       "`(( … ))`" (batch-531 B19)
run_sash 'x=safe | /bin/cat
echo "[$x]"'
[[ $rc != 0 && -z $out \
   && $err == *':1:1: error [SB4022] a variable assignment isn'\''t allowed as a pipeline stage'* ]]; chk pf.41
# pf.42 `exec` as a stage
run_sash '/bin/echo a | exec /bin/cat
echo after'
[[ $rc != 0 && -z $out \
   && $err == *':1:15: error [SB4022] `exec` isn'\''t allowed as a pipeline stage — a stage already runs in a forked child'* ]]; chk pf.42
# pf.43 the final reader reads the pipe in this process, so a redirect on
#       it has nothing to attach to. It compiled; native then died of the
#       producer's SIGPIPE, and gcc rejected the C output
run_sash '/bin/echo hi | readline x 2>/dev/null
echo "x=[${x:!}]"'
[[ $rc != 0 && -z $out \
   && $err == *':1:16: error [SB4022] a redirect on `readline` isn'\''t allowed when it is a pipeline'\''s last stage'* ]]; chk pf.43
# pf.44 ... on `read`: a native ICE, and in C the producer died of SIGPIPE
run_sash '/bin/printf "a\0b\0" | read ar 2>/dev/null
echo "n=$(len ar)"'
[[ $rc != 0 && -z $out \
   && $err == *':1:24: error [SB4022] a redirect on `read` isn'\''t allowed when it is a pipeline'\''s last stage'* ]]; chk pf.44
# pf.45 a negated reader loop as the last stage (ICE)
run_sash '/bin/echo a | ! while readline x { echo "got ${x:!}" }
echo after'
[[ $rc != 0 && -z $out \
   && $err == *':1:17: error [SB4022] a negated `while` isn'\''t allowed as a pipeline'\''s last stage'* ]]; chk pf.45
# pf.46 `read` as an earlier stage (was SB5069; cov.182 now pins SB5069
#       with `&&`)
run_sash 'while read A | read B; do echo x; done'
[[ $rc != 0 && -z $out \
   && $err == *':1:7: error [SB4022] `read` runs in this process only as a pipeline'\''s last stage'* ]]; chk pf.46
# pf.47 (positive) a suppressed reader loop as the last stage (ICE)
run_sash '/bin/echo a | - while readline x { echo "got ${x:!}" }
echo after'
[[ $rc = 0 && $out = $'got a\nafter' && -z $rerr ]]; chk pf.47
# pf.48 (positive) ... in a condition (ICE)
run_sash 'if /bin/printf "a\nb\n" | - while readline x { echo "got ${x:!}" } {
  echo t
} else {
  echo f
}'
[[ $rc = 0 && $out = $'got a\ngot b\nt' && -z $rerr ]]; chk pf.48
# pf.49 (positive) `a | - readline y` in a capture. Native died with the
#       bare-command bug, and gcc rejected the C output
run_sash 'x=$(/bin/echo hi | - readline y; echo "${y:!}")
echo "[${x:!}]"'
[[ $rc = 0 && $out = "[hi]" && -z $rerr ]]; chk pf.49
# pf.50 (positive) a negated final `read` in a condition: a native ICE,
#       and C printed a garbage length
run_sash 'if /bin/true | ! read ar {
  echo "empty n=$(len ar)"
} else {
  echo "got n=$(len ar)"
}'
[[ $rc = 0 && $out = "empty n=0" && -z $rerr ]]; chk pf.50
# pf.51 (positive) ... fed by `:` (likewise)
run_sash 'if : | ! read ar { echo "t[$(len ar)]" } else { echo "f[$(len ar)]" }'
[[ $rc = 0 && $out = "t[0]" && -z $rerr ]]; chk pf.51
# pf.52 (positive) `a | b &` is legal (DD#55); it was the compound-stage
#       SB4022 (batch-531 B23)
run_sash '/bin/echo a | /bin/cat &
- wait
echo after'
[[ $rc = 0 && $out = $'a\nafter' && -z $rerr ]]; chk pf.52
# pf.53 `wait`
run_sash '/bin/true &
wait | /bin/cat
echo after'
[[ $rc != 0 && -z $out \
   && $err == *':2:1: error [SB4022] `wait` isn'\''t allowed as a pipeline stage — a stage runs in a forked child, so `wait` would act on the child'* ]]; chk pf.53
# pf.54 `close`
run_sash 'exec 9>fd9
close 9 | /bin/cat
echo after'
[[ $rc != 0 && -z $out \
   && $err == *':2:1: error [SB4022] `close` isn'\''t allowed as a pipeline stage — a stage runs in a forked child, so `close` would act on the child'* ]]; chk pf.54
# pf.55 `unset`
run_sash 'array items=(one two)
unset items[0] | /bin/cat
echo "n=$(len items)"'
[[ $rc != 0 && -z $out \
   && $err == *':2:1: error [SB4022] `unset` isn'\''t allowed as a pipeline stage — a stage runs in a forked child, so `unset` would act on the child'* ]]; chk pf.55
# pf.56 `signal`
run_sash 'signal 2 IGN | /bin/cat
echo after'
[[ $rc != 0 && -z $out \
   && $err == *':1:1: error [SB4022] `signal` isn'\''t allowed as a pipeline stage — a stage runs in a forked child, so `signal` would act on the child'* ]]; chk pf.56
# pf.57 `argv` (a native ICE)
run_sash 'argv ar | /bin/cat
echo "n=$(len ar)"'
[[ $rc != 0 && -z $out \
   && $err == *':1:1: error [SB4022] `argv` isn'\''t allowed as a pipeline stage — a stage runs in a forked child, so `argv` would act on the child'* ]]; chk pf.57
# pf.58 `keys` (was SB5050)
run_sash 'map m=([a]=1)
keys m ks | /bin/cat
echo "n=$(len ks)"'
[[ $rc != 0 && -z $out \
   && $err == *':2:1: error [SB4022] `keys` isn'\''t allowed as a pipeline stage — a stage runs in a forked child, so `keys` would act on the child'* ]]; chk pf.58
# pf.59 `match` (a native ICE)
run_sash 'match abc "(a)" ar | /bin/cat
echo "n=$(len ar)"'
[[ $rc != 0 && -z $out \
   && $err == *':1:1: error [SB4022] `match` isn'\''t allowed as a pipeline stage — a stage runs in a forked child, so `match` would act on the child'* ]]; chk pf.59
# pf.60 `matchall` (a native ICE)
run_sash 'matchall abc "[a-c]" ar | /bin/cat
echo "n=$(len ar)"'
[[ $rc != 0 && -z $out \
   && $err == *':1:1: error [SB4022] `matchall` isn'\''t allowed as a pipeline stage — a stage runs in a forked child, so `matchall` would act on the child'* ]]; chk pf.60
# pf.61 (positive) `readline` fails at end of input, yet under `-` the
#       condition holds: the verdict is the earlier stages'. Native reported
#       the bare-command bug, and gcc rejected the C output
run_sash 'if /bin/true | - readline y { echo t } else { echo f }
echo "[${y:!}]"'
[[ $rc = 0 && $out = $'t\n[]' && -z $rerr ]]; chk pf.61
# pf.62 (positive) ... so a failing earlier stage still fails it (likewise)
run_sash 'if /bin/false | - readline y { echo t } else { echo f }
echo "[${y:!}]"'
[[ $rc = 0 && $out = $'f\n[]' && -z $rerr ]]; chk pf.62
# pf.63 (positive) ... with `read`: a native ICE, and C printed a garbage
#       length
run_sash 'if /bin/true | - read ar { echo "t$(len ar)" } else { echo "f$(len ar)" }'
[[ $rc = 0 && $out = "t0" && -z $rerr ]]; chk pf.63
# pf.64 (positive) ... in a `while` condition (as pf.61)
run_sash 'while /bin/true | - readline y { echo "w[${y:!}]"; break }
echo after'
[[ $rc = 0 && $out = $'w[]\nafter' && -z $rerr ]]; chk pf.64
# pf.65 a negated final reader that nothing consumes is SB4018, as any
#       unconsumed terminal `!` is (DD#70 2026-10-26). Native reported the
#       bare-command bug, and gcc rejected the C output
run_sash '/bin/echo hi | ! readline x
echo "after[${x:!}]"'
[[ $rc != 0 && -z $out \
   && $err == *':1:16: error [SB4018] `!` result is unused — consume it with if/`&&`/`||`, or write `- cmd` without the `!`'* ]]; chk pf.65
# pf.66 ... with `read`: a native ICE, and C reported the bare-command bug
run_sash '/bin/printf "a\0" | ! read ar
echo "after[$(len ar)]"'
[[ $rc != 0 && -z $out \
   && $err == *':1:21: error [SB4018] `!` result is unused — consume it with if/`&&`/`||`, or write `- cmd` without the `!`'* ]]; chk pf.66
# pf.67 SB4073 names the loop the user wrote: an `until` is not a `while`
run_sash '/bin/echo a | until /bin/false { break }
echo after'
[[ $rc != 0 && -z $out \
   && $err == *':1:15: error [SB4073] this `until` is a pipeline'\''s last stage but never reads it'* ]]; chk pf.67
# pf.68 ... and so does pf.45's SB4022 for a negated reader loop
run_sash '/bin/echo a | ! until readline x { break }
echo after'
[[ $rc != 0 && -z $out \
   && $err == *':1:17: error [SB4022] a negated `until` isn'\''t allowed as a pipeline'\''s last stage'* ]]; chk pf.68

# Stage setup uses the same checked redirects as ordinary commands.
printf 'from-fd\n' > input
run_sash 'exec 9< input; /bin/cat <&9 | /bin/cat; close 9'
[[ $rc = 0 && $out = from-fd && -z $rerr ]]; chk p518.1

run_sash '/bin/sh -c "printf written >&3" 3<> created | /bin/cat'
[[ $rc = 0 && $(<created) = written && -z $rerr ]]; chk p518.2

printf OLD-DATA > existing
run_sash '/bin/sh -c "printf new >&3" 3<> existing | /bin/cat'
[[ $rc = 0 && $(<existing) = new-DATA && -z $rerr ]]; chk p518.3

run_sash 'if /bin/sh -c "echo bad; touch marker" < missing | /bin/cat { echo bad } else { echo $(status) }'
[[ $rc = 0 && $out = 1 && ! -e marker && -z $rerr ]]; chk p518.4

run_sash '- close 9
if /bin/echo bad 1>&9 | /bin/cat { echo bad } else { echo $(status) }'
[[ $rc = 0 && $out = 1 && -z $rerr ]]; chk p518.5

run_sash 'if echo data > /dev/full | /bin/cat { echo bad } else { echo $(status) }'
[[ $rc = 0 && $out = 1 && -z $rerr ]]; chk p518.6

run_sash 'END { echo $(status) }
/bin/echo bad < missing | /bin/cat
echo bad'
[[ $rc = 1 && $out = 1 && $rerr == *"missing:"* ]]; chk p518.7

run_sash '- /bin/sh -c "touch marker" < missing | /bin/cat
echo continued'
[[ $rc = 0 && $out = continued && ! -e marker && -z $rerr ]]; chk p518.8

run_sash ': | - /bin/echo bad < missing | /bin/cat
echo continued'
[[ $rc = 0 && $out = continued && -z $rerr ]]; chk p518.9

run_sash 'exec 9< input
: | /bin/cat <&9 | /bin/cat
close 9'
[[ $rc = 0 && $out = from-fd && -z $rerr ]]; chk p518.10

run_sash 'exec 9> redirected
echo data | /bin/cat >&9
close 9'
[[ $rc = 0 && -z $out && $(<redirected) = data && -z $rerr ]]; chk p518.11

run_sash 'fn f { /bin/touch marker }
if f < missing | /bin/cat { echo bad } else { echo $(status) }'
[[ $rc = 0 && $out = 1 && ! -e marker && -z $rerr ]]; chk p518.12

run_sash ': | /bin/cat'
[[ $rc = 0 && -z $out && -z $rerr ]]; chk p518.13

printf keep > keep
run_sash 'if echo bad > missing-dir/file > keep | /bin/cat { echo bad } else { echo $(status) }'
[[ $rc = 0 && $out = 1 && $(<keep) = keep && -z $rerr ]]; chk p518.14

# The compiler/linker are not resource-limited or traced. Only the resulting
# binary is wrapped, and strace does not follow the already-started children.
run_sash_under 'if echo data | /bin/cat { echo bad } else { echo $(status) }' prlimit --nofile=4:4
[[ $rc = 0 && $out = 1 && -z $rerr ]]; chk p518.15

run_sash_under 'END { echo $(status) }
echo data | /bin/cat
echo bad' strace -qq -o fault.log -e inject=pipe2:error=EIO:when=1
[[ $rc = 1 && $out = 1 && $rerr == *": pipe: "* ]]; chk p518.16

run_sash_under 'if /bin/touch marker | /bin/cat | /bin/cat { echo bad } else { echo $(status) }' \
    strace -qq -o fault.log -e inject=pipe2:error=EMFILE:when=2
[[ $rc = 0 && $out = 1 && ! -e marker && -z $rerr ]]; chk p518.17

run_sash_under 'if /bin/sh -c "sleep 0.05; echo done > finished" | /bin/cat { echo bad } else { /bin/cat finished }' \
    strace -qq -o fault.log -e inject=clone:error=EAGAIN:when=2
[[ $rc = 0 && $out = done && -z $rerr ]]; chk p518.18

run_sash_under 'END { echo $(status) }
echo data | /bin/cat
echo bad' strace -qq -o fault.log -e inject=clone:error=EAGAIN:when=1
[[ $rc = 1 && $out = 1 && $rerr == *": fork: "* ]]; chk p518.19

rm -f finished
run_sash_under 'if /bin/sh -c "sleep 0.05; echo done > finished" | /bin/cat { echo bad } else { /bin/cat finished }' \
    strace -qq -o fault.log -e inject=clone:error=EAGAIN:when=2 -e inject=wait4:error=EINTR:when=1
[[ $rc = 0 && $out = done && -z $rerr ]]; chk p518.20

run_sash_under 'if echo data | readline x { echo ${x:!} } else { echo $(status) }' \
    strace -qq -o fault.log -e inject=pipe2:error=EMFILE:when=1
[[ $rc = 0 && $out = 1 && -z $rerr ]]; chk p518.21

run_sash_under 'array a=(old)
- { echo data | read a }
echo ${a[0]:!}' strace -qq -o fault.log -e inject=clone:error=EAGAIN:when=1
[[ $rc = 0 && $out = old && -z $rerr ]]; chk p518.22

run_sash_under '- { echo data | while readline x { attest x [[ -n $x ]]; echo $x } }
echo continued' strace -qq -o fault.log -e inject=pipe2:error=EMFILE:when=1
[[ $rc = 0 && $out = continued && -z $rerr ]]; chk p518.23

rm -f finished
run_sash_under 'if /bin/sh -c "sleep 0.05; echo done > finished" | /bin/cat | readline x { attest x [[ -n $x ]]; echo $x } else { /bin/cat finished }' \
    strace -qq -o fault.log -e inject=clone:error=EAGAIN:when=2
[[ $rc = 0 && $out = done && -z $rerr ]]; chk p518.24

run_sash 'array a=(old)
- echo data | read a
echo ${a[0]:!}'
[[ $rc = 0 && $out = data && -z $rerr ]]; chk p518.25

run_sash 'array a=(old)
- echo data < missing | read a
echo $(len a)'
[[ $rc = 0 && $out = 0 && -z $rerr ]]; chk p518.26

run_sash 'array a=(a b c)
s=new
read $s a
echo "$(len a):${a[0]}"'
[[ $rc = 0 && $out = 1:new && -z $rerr ]]; chk p518.27

# pipe2 may succeed while relocating an endpoint away from an authored fd
# fails. This is still construction failure, before either output is consumed.
# common.sh keeps fd 3 open, so the successful pair occupies fds 4 and 5.
run_sash_under '- close 4
END { echo $(status) }
: | /bin/cat
echo bad' prlimit --nofile=6:6
[[ $rc = 1 && $out = 1 && $rerr == *": pipe: "* ]]; chk p518.28

run_sash_under '- close 5
END { echo $(status) }
: | /bin/cat
echo bad' prlimit --nofile=6:6
[[ $rc = 1 && $out = 1 && $rerr == *": pipe: "* ]]; chk p518.29

# Each nested reader retains its own pipe across outer-loop iterations.
run_sash '/usr/bin/printf "one\ntwo\n" | while readline a {
  attest a [[ -n $a ]]
  /usr/bin/printf "inner\n" | while readline b {
    attest b [[ $b == inner ]]
    echo $a $b
  }
}'
[[ $rc = 0 && $out = $'one inner\ntwo inner' && -z $rerr ]]; chk pipeown.1

run_sash '/usr/bin/printf "one\ntwo\n" | while readline a {
  attest a [[ -n $a ]]
  /usr/bin/printf "x\0y\0" | read b
  echo "$a:$(len b)"
}'
[[ $rc = 0 && $out = $'one:2\ntwo:2' && -z $rerr ]]; chk pipeown.2

run_sash 'fn inner {
  echo child | readline b
  attest b [[ $b == child ]]
  echo $1 $b
}
/usr/bin/printf "one\ntwo\n" | while readline a {
  attest a [[ -n $a ]]
  inner $a
}'
[[ $rc = 0 && $out = $'one child\ntwo child' && -z $rerr ]]; chk pipeown.3

# The shared writer uses real pipes; this checks reader scope, not attest.
run_sash 'fn pair { echo a; echo b }
pair | while readline a {
  pair | while readline b {
    pair | while readline c { echo "${a:!}${b:!}${c:!}" }
  }
}'
[[ $rc = 0 && $out = $'aaa\naab\naba\nabb\nbaa\nbab\nbba\nbbb' && -z $rerr ]]; chk pipeown.4

# More than a pipe buffer must stream while the parent consumes it.
run_sash 'int n=0
/usr/bin/seq 1 20000 | while readline v {
  attest v [[ -n $v ]]
  (( n++ ))
}
echo $n'
[[ $rc = 0 && $out = 20000 && -z $rerr ]]; chk pipeown.5

run_sash '/usr/bin/head -c 1048576 /dev/zero | /usr/bin/tr '\''\0'\'' x | read a
attest e in a [[ -n $e ]]
echo "$(len a):$(strlen ${a[0]})"'
[[ $rc = 0 && $out = 1:1048576 && -z $rerr ]]; chk pipeown.6

run_sash 'END { echo "end:$(status)" }
/bin/sh -c "echo record; exit 7" | while readline v {
  attest v [[ -n $v ]]
  echo $v
}
echo bad'
[[ $rc = 7 && $out = $'record\nend:7' && $rerr == *"command failed (exit 7)"* ]]; chk pipeown.7

run_sash 'if /bin/sh -c "echo record; exit 7" | while readline v {
  attest v [[ -n $v ]]
  echo $v
} { echo bad } else { echo $(status) }'
[[ $rc = 0 && $out = $'record\n7' && -z $rerr ]]; chk pipeown.8

run_sash '- /bin/sh -c "echo record; exit 7" | while readline v {
  attest v [[ -n $v ]]
  echo $v
}
echo after'
[[ $rc = 0 && $out = $'record\nafter' && -z $rerr ]]; chk pipeown.9

run_sash 'echo record | - /bin/sh -c "cat; exit 9" | while readline v {
  attest v [[ -n $v ]]
  echo $v
}
echo after'
[[ $rc = 0 && $out = $'record\nafter' && -z $rerr ]]; chk pipeown.10

run_sash 'if /bin/sh -c "echo record; exit 4" | /bin/sh -c "cat; exit 9" | while readline v {
  attest v [[ -n $v ]]
  echo $v
} { echo bad } else { echo $(status) }'
[[ $rc = 0 && $out = $'record\n4' && -z $rerr ]]; chk pipeown.11

# Early completion closes the reader before waiting, allowing producer SIGPIPE.
run_sash '- /usr/bin/seq 1 200000 | readline v
attest v [[ $v == 1 ]]
echo $v
int p=$(( (wait) ))
echo $p'
[[ $rc = 0 && $out = $'1\n-1' && -z $rerr ]]; chk pipeown.12

run_sash 'END { echo $(status) }
/usr/bin/seq 1 200000 | readline v
attest v [[ -n $v ]]
echo $v'
[[ $rc = 141 && $out = 141 && $rerr == *"command failed (exit 141)"* ]]; chk pipeown.13

run_sash 'int n=0
- /usr/bin/seq 1 200000 | while readline v {
  attest v [[ -n $v ]]
  (( n++ ))
  break
}
int p=$(( (wait) ))
echo $n $p'
[[ $rc = 0 && $out = "1 -1" && -z $rerr ]]; chk pipeown.14

run_sash '/usr/bin/seq 1 3 | while readline v {
  attest v [[ -n $v ]]
  if [[ $v == 2 ]] { continue }
  echo $v
}
int p=$(( (wait) ))
echo $p'
[[ $rc = 0 && $out = $'1\n3\n-1' && -z $rerr ]]; chk pipeown.15

run_sash 'for label in left right {
  - /usr/bin/seq 1 200000 | while readline v {
    attest v [[ -n $v ]]
    echo $label
    break 2
  }
  echo bad
}
int p=$(( (wait) ))
echo done $p'
[[ $rc = 0 && $out = $'left\ndone -1' && -z $rerr ]]; chk pipeown.16

run_sash 'int n=0
while (( n < 2 )) {
  (( n++ ))
  - /usr/bin/seq 1 200000 | while readline v {
    attest v [[ -n $v ]]
    continue 2
  }
  echo bad
}
int p=$(( (wait) ))
echo $n $p'
[[ $rc = 0 && $out = "2 -1" && -z $rerr ]]; chk pipeown.17

run_sash '/usr/bin/printf "one\ntwo\n" | while readline a {
  attest a [[ -n $a ]]
  - /usr/bin/seq 1 200000 | while readline b {
    attest b [[ -n $b ]]
    echo $a $b
    break
  }
}
int p=$(( (wait) ))
echo $p'
[[ $rc = 0 && $out = $'one 1\ntwo 1\n-1' && -z $rerr ]]; chk pipeown.18

run_sash 'fn inner -> int {
  - /usr/bin/seq 1 200000 | while readline v {
    attest v [[ -n $v ]]
    return 7
  }
  return 9
}
/usr/bin/printf "one\ntwo\n" | while readline a {
  attest a [[ -n $a ]]
  int result=$(( (inner) ))
  echo "$result:$a"
}
int p=$(( (wait) ))
echo $p'
[[ $rc = 0 && $out = $'7:one\n7:two\n-1' && -z $rerr ]]; chk pipeown.19

run_sash 'fn count {
  int n=$(($1))
  if (( n > 0 )) {
    - /usr/bin/seq 1 200000 | while readline v {
      attest v [[ -n $v ]]
      int m=$(( n - 1 ))
      return count $m
    }
  }
  echo done
}
count 3
int p=$(( (wait) ))
echo $p'
[[ $rc = 0 && $out = $'done\n-1' && -z $rerr ]]; chk pipeown.20

run_sash 'fn count -> int {
  int n=$(($1))
  if (( n > 0 )) {
    - /usr/bin/seq 1 200000 | while readline v {
      attest v [[ -n $v ]]
      int m=$(( n - 1 ))
      return $(( (count m) ))
    }
  }
  return 42
}
int answer=$(( (count 3) ))
int p=$(( (wait) ))
echo $answer $p'
[[ $rc = 0 && $out = "42 -1" && -z $rerr ]]; chk pipeown.21

rm -f finished
run_sash 'END { echo "end:$(status)"; /bin/cat finished }
/bin/sh -c "echo record; sleep 0.05; echo done > finished" | while readline v {
  attest v [[ -n $v ]]
  /bin/false
}
echo bad'
[[ $rc = 1 && $out = $'end:1\ndone' && $rerr == *"command failed (exit 1)"* ]]; chk pipeown.22

rm -f finished
run_sash 'END { echo "end:$(status)"; /bin/cat finished }
/bin/sh -c "echo record; sleep 0.05; echo done > finished" | while readline v {
  attest v [[ -n $v ]]
  exit 23
}'
[[ $rc = 23 && $out = $'end:23\ndone' && -z $rerr ]]; chk pipeown.23

rm -f finished
run_sash_under '/bin/sh -c "echo record; sleep 0.05; echo done > finished" | while readline v {
  attest v [[ $v == impossible ]]
  echo $v
}' strace -qq -o fault.log -e trace=close,wait4
[[ $rc = 1 && $(<finished) = done && $rerr == *"failed the gate"* ]] &&
  grep -q 'wait4(' fault.log; chk pipeown.24

run_sash_under 'int n=0
while (( n < 100 )) {
  echo record | readline v
  attest v [[ $v == record ]]
  (( n++ ))
}
int p=$(( (wait) ))
echo $n $p' prlimit --nofile=12:12
[[ $rc = 0 && $out = "100 -1" && -z $rerr ]]; chk pipeown.25

run_sash 'END {
  echo "end:$(status)"
  /bin/sh -c "echo cleanup; exit 9" | while readline x {
    attest x [[ -n $x ]]
    echo $x
  }
  echo complete
}
echo record | while readline v {
  attest v [[ -n $v ]]
  /bin/false
}'
[[ $rc = 1 && $out = $'end:1\ncleanup\ncomplete' && $rerr == *"command failed (exit 1)"* ]]; chk pipeown.26

run_sash_under '/usr/bin/printf "one\ntwo\n" | while readline a {
  attest a [[ -n $a ]]
  if echo inner | readline b {
    attest b [[ -n $b ]]
    echo "$a:$b"
  } else { echo "$a:failed" }
}' strace -qq -o fault.log -e inject=pipe2:error=EMFILE:when=2
[[ $rc = 0 && $out = $'one:failed\ntwo:inner' && -z $rerr ]]; chk pipeown.27

rm -f finished
run_sash_under '/usr/bin/printf "one\ntwo\n" | while readline a {
  attest a [[ -n $a ]]
  if /bin/sh -c "sleep 0.05; echo done > finished" | /bin/cat | readline b {
    attest b [[ -n $b ]]
    echo bad $b
  } else { /bin/cat finished; echo $a }
}' strace -qq -o fault.log -e inject=clone:error=EAGAIN:when=3
[[ $rc = 0 && $out = $'done\none\ndone\ntwo' && -z $rerr ]]; chk pipeown.28

run_sash_under 'echo record | readline v
attest v [[ -n $v ]]
int p=$(( (wait) ))
echo $v $p' strace -qq -o fault.log -e inject=wait4:error=EINTR:when=1
[[ $rc = 0 && $out = "record -1" && -z $rerr ]]; chk pipeown.29

# An authored wait may reap a producer; finishing still uses its real status.
run_sash 'if /bin/sh -c "echo record; exit 7" | while readline v {
  attest v [[ -n $v ]]
  int p=$(( (wait) ))
  if (( p > 0 )) { echo reaped }
  echo $v
} { echo bad } else { echo $(status) }'
[[ $rc = 0 && $out = $'reaped\nrecord\n7' && -z $rerr ]]; chk pipeown.30

run_sash 'if /bin/true | read a { echo bad } else { echo "$(len a):empty" }
if /bin/echo record | read b { echo "$(len b):full" } else { echo bad }'
[[ $rc = 0 && $out = $'0:empty\n1:full' && -z $rerr ]]; chk pipeown.31

run_sash 'if : | readline a { echo bad } else { echo empty }
if /bin/sh -c "echo record; exit 7" | readline b {
  attest b [[ -n $b ]]
  echo bad $b
} else { echo $(status) }
echo "[${a:!}]"'
[[ $rc = 0 && $out = $'empty\n7\n[]' && -z $rerr ]]; chk pipeown.32

printf 'stdin\n' > reader-stdin
run_sash 'exec 0< reader-stdin
/usr/bin/printf "one\ntwo\n" | while readline a {
  attest a [[ -n $a ]]
  /bin/cat
  echo $a
}'
[[ $rc = 0 && $out = $'stdin\none\ntwo' && -z $rerr ]]; chk pipeown.33

run_sash 'if /bin/sh -c "echo outer; exit 7" | while readline a {
  attest a [[ -n $a ]]
  echo inner | while readline b {
    attest b [[ -n $b ]]
    int p=$(( (wait) ))
    int q=$(( (wait) ))
    if (( p > 0 && q > 0 )) { echo reaped }
    echo $a $b
  }
} { echo bad } else { echo $(status) }'
[[ $rc = 0 && $out = $'reaped\nouter inner\n7' && -z $rerr ]]; chk pipeown.34

run_sash 'int n=0
echo pipe | while (( n < 1 )) {
  read a
  attest e in a [[ -n $e ]]
  echo "value:$(join a)"
  (( n++ ))
}
echo $n'
[[ $rc = 0 && $out = $'value:pipe\n\n1' && -z $rerr ]]; chk pipeown.35

run_sash '/usr/bin/printf "one\ntwo\n" | while readline a {
  attest a [[ -n $a ]]
  int n=0
  echo inner | while (( n < 1 )) {
    read b
    attest e in b [[ -n $e ]]
    echo "$a:$(len b):$(strlen ${b[0]})"
    (( n++ ))
  }
}'
[[ $rc = 0 && $out = $'one:1:6\ntwo:1:6' && -z $rerr ]]; chk pipeown.36

printf 'from-file\n' > reader-line-input
run_sash '/usr/bin/printf "one\ntwo\n" | while readline a {
  attest a [[ -n $a ]]
  readline b < reader-line-input
  attest b [[ -n $b ]]
  echo "$a:$b"
}'
[[ $rc = 0 && $out = $'one:from-file\ntwo:from-file' && -z $rerr ]]; chk pipeown.37

printf 'from-file\0' > reader-array-input
run_sash '/usr/bin/printf "one\ntwo\n" | while readline a {
  attest a [[ -n $a ]]
  read b < reader-array-input
  attest e in b [[ -n $e ]]
  echo "$a:$(join b)"
}'
[[ $rc = 0 && $out = $'one:from-file\ntwo:from-file' && -z $rerr ]]; chk pipeown.38

printf 'stdin-one\nstdin-two\n' > reader-stdin
run_sash 'fn f {
  readline v
  attest v [[ -n $v ]]
  echo $v
}
exec 0< reader-stdin
/usr/bin/printf "one\ntwo\n" | while readline a {
  attest a [[ -n $a ]]
  echo $a
  f
}'
[[ $rc = 0 && $out = $'one\nstdin-one\ntwo\nstdin-two' && -z $rerr ]]; chk pipeown.39

run_sash 'fn f {
  readline v
  attest v [[ -n $v ]]
  echo "read:$v"
}
/usr/bin/printf "one\ntwo\n" | while readline a {
  attest a [[ -n $a ]]
  echo $a
  f < reader-line-input
}'
[[ $rc = 0 && $out = $'one\nread:from-file\ntwo\nread:from-file' && -z $rerr ]]; chk pipeown.40

run_sash 'exec 0< reader-stdin
/usr/bin/printf "one\ntwo\n" | while readline a {
  attest a [[ -n $a ]]
  str b=$(readline q; attest q [[ -n $q ]]; echo $q)
  attest b [[ -n $b ]]
  echo "$a:$b"
}'
[[ $rc = 0 && $out = $'one:stdin-one\ntwo:stdin-two' && -z $rerr ]]; chk pipeown.41

# ---- Forked stages (batch-531 port, F2). A stage is a forked child: it
# evaluates its own arguments, leaves with its own status, and the parent
# reports a failed stage by what the source wrote.

# B120: a function stage's arguments are evaluated in the caller's frame. The
# stage took the caller's positional array before it lowered the stage's own
# arguments, so any argument that reads the caller's positionals found no
# array: an ICE in both backends ("undefined array" / "no emitted SSA slot"),
# and for `$#` in C a silently wrong number.

# stgarg.1 `$1` of the caller, passed to a function stage
run_sash 'fn f { echo "a=$1" }
fn h { f "$1" | /bin/cat }
h x'
[[ $rc = 0 && $out = a=x && -z $rerr ]]; chk stgarg.1

# stgarg.2 arithmetic over the caller's `$1`
run_sash 'fn f { echo "f$1" }
fn g { f $(( $1 - 1 )) | /bin/cat }
g 2'
[[ $rc = 0 && $out = f1 && -z $rerr ]]; chk stgarg.2

# stgarg.3 a `:-` default on an unset positional, then the caller reads `$1`
run_sash 'fn f { echo "a=$1" }
fn h { f "${2:-dflt}" | /bin/cat; echo "h=$1" }
h y'
[[ $rc = 0 && $out = $'a=dflt\nh=y' && -z $rerr ]]; chk stgarg.3

# stgarg.4 the caller's `args` array by index
run_sash 'fn f { echo "a=$1" }
fn h { f "${args[0]}" | /bin/cat }
h y'
[[ $rc = 0 && $out = a=y && -z $rerr ]]; chk stgarg.4

# stgarg.5 the caller's `$#` (C printed an address-like number here)
run_sash 'fn f { echo "n=$1" }
fn h { f "$#" | /bin/cat; echo "h=$1" }
h y'
[[ $rc = 0 && $out = $'n=1\nh=y' && -z $rerr ]]; chk stgarg.5

# stgarg.6 a global beside the positional, in the argument and in the stage
run_sash 'str s=outer
fn f { echo "f $1 $s" }
fn h { f "$1-$s" | /bin/cat }
h x'
[[ $rc = 0 && $out = 'f x-outer outer' && -z $rerr ]]; chk stgarg.6

# stgarg.7 the stage inside a background job
run_sash 'fn f { echo "f $1" }
fn h { f "$1" | /bin/cat; echo "h $1" }
h x &
- wait
echo after'
[[ $rc = 0 && $out = $'f x\nh x\nafter' && -z $rerr ]]; chk stgarg.7

# stgarg.8 the caller is itself a stage
run_sash 'fn f { echo "f $1" }
fn h { f "$1" | /bin/cat }
h x | /bin/cat
echo after'
[[ $rc = 0 && $out = $'f x\nafter' && -z $rerr ]]; chk stgarg.8

# stgarg.9 the function is the final stage
run_sash 'fn f { /bin/cat; echo "a=$1" }
fn g { /bin/echo in | f "$1" }
g y'
[[ $rc = 0 && $out = $'in\na=y' && -z $rerr ]]; chk stgarg.9

# B10: a failing function stage is reported by the function's name, not by the
# compiler's `__compound_N` label for the stage.

# pf.69 `exit` in the stage function
run_sash 'fn f { echo x; exit 3 }
f | /bin/cat
echo done'
[[ $rc = 3 && $out = x && $rerr = "./test_bin: f: command failed (exit 3)" ]]; chk pf.69

# pf.70 `return` in the stage function
run_sash 'fn f {
  echo x
  return 3
}
f | /bin/cat
echo after'
[[ $rc = 3 && $out = x && $rerr = "./test_bin: f: command failed (exit 3)" ]]; chk pf.70

# pf.71 the status comes from a call inside the stage: still named `f`. (The
# call is a tail `return g`: a bare `g` would discard g's status, SB4016.)
run_sash 'fn g { return 3 }
fn f { echo x; return g }
f | /bin/cat
echo after'
[[ $rc = 3 && $out = x && $rerr = "./test_bin: f: command failed (exit 3)" ]]; chk pf.71

# A stage's body is checked as any body is: its first uncaught failure aborts
# the child, which reports the command, and the parent reports the function.

# pf.82 a function stage
run_sash 'fn f { /bin/false; echo "after $?" }
f | /bin/cat
echo done'
[[ $rc = 1 && -z $out \
   && $rerr = $'./test_bin: /bin/false: command failed (exit 1)\n./test_bin: f: command failed (exit 1)' ]]; chk pf.82

# pf.83 a self-tail function as a stage
run_sash 'fn f { if (( $1 > 0 )) { /bin/false; echo "n $1"; return f $(( $1 - 1 )) } }
f 2 | /bin/cat
echo done'
[[ $rc = 1 && -z $out \
   && $rerr = $'./test_bin: /bin/false: command failed (exit 1)\n./test_bin: f: command failed (exit 1)' ]]; chk pf.83

# B17: a background job inside a reader loop. The child left through the
# reader's finish, which belongs to the parent, and panicked "reader scopes
# must finish in nesting order" once per job.

# pipeown.42 a function's background job, waited by pid, inside the reader
run_sash 'fn bg {
  echo "got $1" &
  int p=$!
  int r=$(wait $p)
  int s=$(waitstatus)
  echo "s=$s r=$(( r == p ))"
}
/bin/printf "1\n2\n" | while readline x {
  attest x [[ -n $x ]]
  bg "$x"
}
echo done'
[[ $rc = 0 && $out = $'got 1\ns=0 r=1\ngot 2\ns=0 r=1\ndone' && -z $rerr ]]; chk pipeown.42

# pipeown.43 a command background job directly in the reader body
run_sash '/bin/printf "1\n2\n" | while readline x {
  attest x [[ -n $x ]]
  /bin/true &
  echo "got $x"
}
- wait
echo done'
[[ $rc = 0 && $out = $'got 1\ngot 2\ndone' && -z $rerr ]]; chk pipeown.43

# B81: a command stage's arguments are evaluated in the stage's child. A
# capture in a command stage's arguments (a function's, an external command's,
# an anonymous body's, or an int function called in its arithmetic) ICE'd at
# compile time in both backends, and a `:` stage dropped its captures unrun.
# The child now evaluates them as a function stage's child does: after the
# stage's redirects, before the exec.

# stgarg.11 the final stage
run_sash 'fn g { echo hi }
/bin/echo hi | /bin/grep "$(g)"
echo done'
[[ $rc = 0 && $out = $'hi\ndone' && -z $rerr ]]; chk stgarg.11

# stgarg.12 a middle stage
run_sash 'fn g { echo hi }
/bin/echo hi | /bin/grep "$(g)" | /bin/cat
echo done'
[[ $rc = 0 && $out = $'hi\ndone' && -z $rerr ]]; chk stgarg.12

# stgarg.13 inside a function
run_sash 'fn g { echo hi }
fn h { /bin/echo "h:$(g)" | /bin/cat }
h
echo done'
[[ $rc = 0 && $out = $'h:hi\ndone' && -z $rerr ]]; chk stgarg.13

# stgarg.14 inside END
run_sash 'fn g { echo hi }
END { /bin/echo "end:$(g)" | /bin/cat }
echo main'
[[ $rc = 0 && $out = $'main\nend:hi' && -z $rerr ]]; chk stgarg.14

# stgarg.15 inside a capture
run_sash 'fn g { echo hi }
x=$(/bin/echo "$(g)" | /bin/cat)
echo "x=${x:!}"'
[[ $rc = 0 && $out = x=hi && -z $rerr ]]; chk stgarg.15

# stgarg.16 a condition
run_sash 'fn g { echo hi }
if /bin/echo "$(g)" | /bin/grep -q hi { echo yes } else { echo no }'
[[ $rc = 0 && $out = yes && -z $rerr ]]; chk stgarg.16

# stgarg.17 a condition whose final stage is negated
run_sash 'fn g { echo hi }
if /bin/echo "$(g)" | ! /bin/grep -q zz { echo neg }'
[[ $rc = 0 && $out = neg && -z $rerr ]]; chk stgarg.17

# stgarg.18 feeding a reader loop
run_sash 'fn g { echo hi }
/bin/echo "$(g)" | while readline x { echo "got ${x:!}" }
echo done'
[[ $rc = 0 && $out = $'got hi\ndone' && -z $rerr ]]; chk stgarg.18

# stgarg.19 a `write` stage
run_sash 'fn g { echo hi }
write "w:$(g)" | /bin/cat
echo
echo done'
[[ $rc = 0 && $out = $'w:hi\ndone' && -z $rerr ]]; chk stgarg.19

# stgarg.20 an int function in the argument's arithmetic
run_sash 'fn sq -> int { return $(( $1 * $1 )) }
/bin/echo "n=$(( (sq 3) + 1 ))" | /bin/cat
echo done'
[[ $rc = 0 && $out = $'n=10\ndone' && -z $rerr ]]; chk stgarg.20

# stgarg.22 a background pipeline
run_sash 'fn g { echo hi }
/bin/echo "bg:$(g)" | /bin/cat &
- wait
echo done'
[[ $rc = 0 && $out = $'bg:hi\ndone' && -z $rerr ]]; chk stgarg.22

# stgarg.23 with a signal trap installed
run_sash 'fn g { echo hi }
signal 15 EXIT
/bin/echo "s:$(g)" | /bin/cat
echo done'
[[ $rc = 0 && $out = $'s:hi\ndone' && -z $rerr ]]; chk stgarg.23

# stgarg.24 a capture as a map key
run_sash 'fn g { echo k }
map m=([k]=v)
/bin/echo "${m[$(g)]:-d}" | /bin/cat
echo done'
[[ $rc = 0 && $out = $'v\ndone' && -z $rerr ]]; chk stgarg.24

# stgarg.25 an external command's capture (--no-taint: it is not attested)
SASH_CFLAGS=--no-taint run_sash '/bin/echo "a=$(/bin/echo x)" | /bin/cat
echo done'
[[ $rc = 0 && $out = $'a=x\ndone' && -z $rerr ]]; chk stgarg.25

# stgarg.26 an anonymous capture body
run_sash '/bin/echo "a=$(echo x; echo y)" | /bin/cat
echo done'
[[ $rc = 0 && $out = $'a=x\ny\ndone' && -z $rerr ]]; chk stgarg.26

# stgarg.27 the capture reads the stage's stdin, which is the pipe
SASH_CFLAGS=--no-taint run_sash 'echo hi | /bin/echo "got=$(/bin/cat)"
echo done'
[[ $rc = 0 && $out = $'got=hi\ndone' && -z $rerr ]]; chk stgarg.27

# A `:` stage runs its argument captures, as the statement `:` does. Their
# output is discarded but their stderr is not, and an uncaught failure inside
# one aborts the stage: the child reports the failure, the parent the stage
# (DD#55's two lines, as for any failed stage).

# stgarg.28 the capture runs: its stderr is seen
SASH_CFLAGS=--no-taint run_sash 'fn g { /bin/echo side >&2; echo x }
: "$(g)" | /bin/cat
echo done'
[[ $rc = 0 && $out = done && $rerr = side ]]; chk stgarg.28

# stgarg.29 an uncaught failure inside it aborts the stage
SASH_CFLAGS=--no-taint run_sash 'fn g { /bin/false; echo x }
: "$(g)" | /bin/cat
echo done'
[[ $rc = 1 && -z $out \
   && $rerr = $'./test_bin: /bin/false: command failed (exit 1)\n./test_bin: :: command failed (exit 1)' ]]; chk stgarg.29

# stgarg.30 the same failure in an external stage's argument: the child aborts
# before its exec, and the parent names the stage's command
SASH_CFLAGS=--no-taint run_sash 'fn g { /bin/false; echo x }
/bin/echo "a=$(g)" | /bin/cat
echo done'
[[ $rc = 1 && -z $out \
   && $rerr = $'./test_bin: /bin/false: command failed (exit 1)\n./test_bin: /bin/echo: command failed (exit 1)' ]]; chk stgarg.30

# stgarg.31 an `echo` stage's own redirect: echo's write error, then the stage
run_sash 'fn g { echo hi }
echo "$(g)" >/dev/full | /bin/cat
echo done'
[[ $rc = 1 && -z $out \
   && $rerr = $'./test_bin: echo: write error: No space left on device\n./test_bin: echo: command failed (exit 1)' ]]; chk stgarg.31

# stgarg.32 the stage's exec fails after its arguments ran: the child reports
# the errno and the parent the uncaught failure (errors.sh 55.30's two lines)
run_sash_vanish 'fn g { echo hi }
SPEC_VANISH_CMD "$(g)" | /bin/cat
echo unreached'
[[ $rc != 0 && -z $out \
   && $rerr == *": No such file or directory"* \
   && $rerr == *": command failed (exit 127)"* ]]; chk stgarg.32

# Order. A stage applies its redirects and then evaluates its arguments, as a
# function stage already did; a statement evaluates its arguments first. Which
# order a stage should use is an open question (q-stage-capture-redirect-order);
# these pin today's.
printf 'abc\n' > ro.in

# stgarg.33 a command stage's capture reads the stage's `<` file
SASH_CFLAGS=--no-taint run_sash ': | /bin/echo "got=$(/bin/cat)" < ro.in
echo done'
[[ $rc = 0 && $out = $'got=abc\ndone' && -z $rerr ]]; chk stgarg.33

# stgarg.34 a function stage's capture does too (unchanged)
SASH_CFLAGS=--no-taint run_sash 'fn f { echo "f:$1" }
: | f "$(/bin/cat)" < ro.in
echo done'
[[ $rc = 0 && $out = $'f:abc\ndone' && -z $rerr ]]; chk stgarg.34
rm -f ro.in

# stgarg.35 a command stage's `2>` covers its capture's stderr
SASH_CFLAGS=--no-taint run_sash 'fn g { - /bin/ls /nonexistent-stgarg35; echo x }
/bin/echo "a=$(g)" 2>/dev/null | /bin/cat
echo done'
[[ $rc = 0 && $out = $'a=x\ndone' && -z $rerr ]]; chk stgarg.35

# stgarg.36 a statement's `2>` does not: its capture ran first (unchanged)
SASH_CFLAGS=--no-taint run_sash 'fn g { - /bin/ls /nonexistent-stgarg36; echo x }
/bin/echo "a=$(g)" 2>/dev/null
echo done'
[[ $rc = 0 && $out = $'a=x\ndone' && $rerr == *nonexistent-stgarg36* ]]; chk stgarg.36

set -e
tally
