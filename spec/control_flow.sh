#!/bin/bash
# spec/control_flow.sh — while / case / for runtime behavior.
# Covers transform/control_flow.rs SSA lowering paths and the
# matching Cranelift codegen for loops, case dispatch, and for-in.
set -eu
. common.sh
setup control_flow
plan=43
echo "1..$plan" >&3

set +e

# --- while loops ---

# cf.w1 while with arithmetic condition
run_sash 'int i=0; while (( i < 3 )); do echo $i; (( i++ )); done'
[[ $rc = 0 && $out = $'0\n1\n2' && -z $err ]]; chk cf.w1

# cf.w2 while with command condition
run_sash 'int i=3; while [[ $i != 0 ]]; do echo $i; (( i-- )); done'
[[ $rc = 0 && $out = $'3\n2\n1' && -z $err ]]; chk cf.w2

# cf.w3 while body modifies variable read by condition
run_sash 'int n=5; int sum=0; while (( n > 0 )); do (( sum += n )); (( n-- )); done; echo $sum'
[[ $rc = 0 && $out = "15" && -z $err ]]; chk cf.w3

# cf.w4 while reads multiple bindings
run_sash 'int i=0; int j=10; while (( i < j )); do (( i++ )); (( j-- )); done; echo "$i $j"'
[[ $rc = 0 && $out = "5 5" && -z $err ]]; chk cf.w4

# cf.w5 while-true with explicit break is not allowed in sash; use until
# This tests the loop terminator wiring.
run_sash 'int i=0; while (( i < 1 )); do echo once; (( i++ )); done'
[[ $rc = 0 && $out = "once" && -z $err ]]; chk cf.w5

# cf.w6 nested while loops
run_sash 'int i=0; while (( i < 2 )); do int j=0; while (( j < 2 )); do echo "$i $j"; (( j++ )); done; (( i++ )); done'
[[ $rc = 0 && $out = $'0 0\n0 1\n1 0\n1 1' && -z $err ]]; chk cf.w6

# cf.w7 while with no iterations (condition false on entry)
run_sash 'int i=10; while (( i < 0 )); do echo never; (( i-- )); done; echo done'
[[ $rc = 0 && $out = "done" && -z $err ]]; chk cf.w7

# --- case statements ---

# cf.c1 case with single matching arm
run_sash 'x=hello; case $x in hello) echo greeted ;; esac'
[[ $rc = 0 && $out = "greeted" && -z $err ]]; chk cf.c1

# cf.c2 case with default arm (*)
run_sash 'x=foo; case $x in bar) echo bar ;; *) echo other ;; esac'
[[ $rc = 0 && $out = "other" && -z $err ]]; chk cf.c2

# cf.c3 case with multiple arms, first matches
run_sash 'x=one; case $x in one) echo 1 ;; two) echo 2 ;; three) echo 3 ;; esac'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk cf.c3

# cf.c4 case with multiple arms, middle matches
run_sash 'x=two; case $x in one) echo 1 ;; two) echo 2 ;; three) echo 3 ;; esac'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk cf.c4

# cf.c5 case with multi-pattern arm (a|b|c)
run_sash 'x=b; case $x in a|b|c) echo abc ;; *) echo other ;; esac'
[[ $rc = 0 && $out = "abc" && -z $err ]]; chk cf.c5

# cf.c5b multi-pattern arm with a NON-matching subject must fall through to a
# later arm (regression gate for the janet a|b -> `_` catch-all bug).
run_sash 'x=q; case $x in a|b|c) echo abc ;; *) echo other ;; esac'
[[ $rc = 0 && $out = "other" && -z $err ]]; chk cf.c5b

# cf.c6 case with glob pattern (* suffix)
run_sash 'x=hello; case $x in h*) echo h-prefix ;; *) echo other ;; esac'
[[ $rc = 0 && $out = "h-prefix" && -z $err ]]; chk cf.c6

# cf.c7 `?` is a literal in case patterns, NOT a single-char wildcard
#       (DD#39 star-only glob): `?b` does not match `ab`, so the `*` arm wins.
run_sash 'x=ab; case $x in ?b) echo wildcard ;; *) echo star-only ;; esac'
[[ $rc = 0 && $out = "star-only" && -z $err ]]; chk cf.c7

# cf.c8 `[class]` is literal, NOT a character class (DD#39 star-only glob):
#       `[0-9]` does not match `5`, so the `*` arm wins.
run_sash 'x=5; case $x in [0-9]) echo charclass ;; *) echo star-only ;; esac'
[[ $rc = 0 && $out = "star-only" && -z $err ]]; chk cf.c8

# cf.c9 case with no matching arm and no default
run_sash 'x=z; case $x in a) echo a ;; b) echo b ;; esac; echo done'
[[ $rc = 0 && $out = "done" && -z $err ]]; chk cf.c9

# cf.c10 case where arm bodies span multiple statements
run_sash 'x=a; case $x in a) echo one; echo two ;; *) echo other ;; esac'
[[ $rc = 0 && $out = $'one\ntwo' && -z $err ]]; chk cf.c10

# cf.c11 case with variable expansion in scrutinee
run_sash 'x=hello; case "$x" in hello) echo hi ;; esac'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk cf.c11

# cf.c12 case with empty arm body (just ;;)
run_sash 'x=skip; case $x in skip) ;; *) echo nope ;; esac; echo done'
[[ $rc = 0 && $out = "done" && -z $err ]]; chk cf.c12

# --- for loops ---

# cf.f1 for with literal word list
run_sash 'for x in a b c; do echo $x; done'
[[ $rc = 0 && $out = $'a\nb\nc' && -z $err ]]; chk cf.f1

# cf.f2 for with single-item list
run_sash 'for x in solo; do echo $x; done'
[[ $rc = 0 && $out = "solo" && -z $err ]]; chk cf.f2

# cf.f3 for over array expansion (sash-native bare splat — DD#41)
run_sash 'array a=(one two three); for x in @a; do echo $x; done'
[[ $rc = 0 && $out = $'one\ntwo\nthree' && -z $err ]]; chk cf.f3

# cf.f4 for over the user-arg slice via $(argv 1) (was top-level "$@", which ICEd)
run_sash 'for x in $(argv 1); do bless x [[ $x =~ . ]]; echo "$x"; done' alpha beta gamma
[[ $rc = 0 && $out = $'alpha\nbeta\ngamma' && -z $err ]]; chk cf.f4

# cf.f5 for body references loop variable in arithmetic
run_sash 'int sum=0; for x in 10 20 30; do (( sum += x )); done; echo $sum'
[[ $rc = 0 && $out = "60" && -z $err ]]; chk cf.f5

# cf.f6 nested for loops
run_sash 'for i in 1 2; do for j in a b; do echo "$i$j"; done; done'
[[ $rc = 0 && $out = $'1a\n1b\n2a\n2b' && -z $err ]]; chk cf.f6

# cf.f7 for over an empty user-arg slice runs zero iterations
run_sash 'for x in $(argv 1); do echo never; done; echo done'
[[ $rc = 0 && $out = "done" && -z $err ]]; chk cf.f7

# cf.f8 C-style for in arithmetic context
run_sash 'for (( i=0; i < 3; i++ )); do echo $i; done'
[[ $rc = 0 && $out = $'0\n1\n2' && -z $err ]]; chk cf.f8

# cf.f9 C-style for with multiple init/update
run_sash 'for (( i=0, j=10; i < 3; i++, j-- )); do echo "$i $j"; done'
[[ $rc = 0 && $out = $'0 10\n1 9\n2 8' && -z $err ]]; chk cf.f9

# cf.f10 for-loop variable persists after loop ends (sash semantics)
run_sash 'for x in a b c; do :; done; echo $x'
[[ $rc = 0 && $out = "c" && -z $err ]]; chk cf.f10

# --- inside-function positionals stay clean (DD#46) ---

# cf.f11 $@ inside a function is the caller's args (clean)
run_sash 'fn greet { echo "$@"; }; greet alpha beta gamma'
[[ $rc = 0 && $out = "alpha beta gamma" && -z $err ]]; chk cf.f11

# cf.f12 for over $@ inside a function iterates the caller's args
run_sash 'fn each { for x in $@; do echo "$x"; done }; each a b c'
[[ $rc = 0 && $out = $'a\nb\nc' && -z $err ]]; chk cf.f12

# cf.f13 $# inside a function is the caller's arg count
run_sash 'fn n { echo "$#"; }; n a b'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk cf.f13

# cf.f14 $1 inside a function is the caller's first arg
run_sash 'fn one { echo "$1"; }; one alpha'
[[ $rc = 0 && $out = "alpha" && -z $err ]]; chk cf.f14

# --- DD#46 top-level positional rejection (directed to DD#60 builtins) ---

# cf.dd46.1 top-level $@ rejected (use $(argv))
run_sash 'echo "$@"'
[[ $rc != 0 && -z $out && -n $err ]]; chk cf.dd46.1

# cf.dd46.2 top-level $1 rejected (use $(arg 1))
run_sash 'echo "$1"'
[[ $rc != 0 && -z $out && -n $err ]]; chk cf.dd46.2

# cf.dd46.3 top-level $* rejected (use $(argv))
run_sash 'echo "$*"'
[[ $rc != 0 && -z $out && -n $err ]]; chk cf.dd46.3

# cf.dd46.4 top-level $# rejected (use $(argc))
run_sash 'echo "$#"'
[[ $rc != 0 && -z $out && -n $err ]]; chk cf.dd46.4

# cf.dd46.5 top-level $0 rejected (use $(arg 0))
run_sash 'echo "$0"'
[[ $rc != 0 && -z $out && -n $err ]]; chk cf.dd46.5

# cf.dd46.6 top-level for-in over "$@" rejected (was an ICE — bug closure)
run_sash 'for x in "$@"; do echo "$x"; done'
[[ $rc != 0 && -z $out && -n $err ]]; chk cf.dd46.6

# --- if/elif chains (covers flatten_elif) ---

# cf.i1 if/elif/else chain — first arm matches
run_sash 'int x=1; if (( x == 1 )); then echo one; elif (( x == 2 )); then echo two; else echo other; fi'
[[ $rc = 0 && $out = "one" && -z $err ]]; chk cf.i1

# cf.i2 if/elif/else chain — middle arm matches
run_sash 'int x=2; if (( x == 1 )); then echo one; elif (( x == 2 )); then echo two; else echo other; fi'
[[ $rc = 0 && $out = "two" && -z $err ]]; chk cf.i2

# cf.i3 if/elif/else chain — else arm runs
run_sash 'int x=99; if (( x == 1 )); then echo one; elif (( x == 2 )); then echo two; else echo other; fi'
[[ $rc = 0 && $out = "other" && -z $err ]]; chk cf.i3

set -e
tally
