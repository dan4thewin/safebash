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
plan=34
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

set -e
tally
