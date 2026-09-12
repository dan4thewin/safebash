#!/bin/bash
# spec/pipe_functions.sh — user functions as pipeline stages (bug-function-in-
# pipeline). A `Command` stage that names a known function must run as a forked
# compound child (like a `{ }` group), not be exec'd as a bare command.
#
# NATIVE-ONLY: compound pipeline stages (DD#14) are native-only infra; the
# c backend has no compound-pipeline-stage support at all (a group stage
# already fails there).
# Same native-only pattern as brace.sh / convert.sh.
set -eu
. common.sh
setup pipe_functions
[[ ${SASH_MODE:-native} != native ]] && exit 0
plan=11
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

set -e
tally
