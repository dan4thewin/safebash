#!/bin/bash
# spec/control_flow.sh — while / dispatch / for runtime behavior.
# Covers transform/control_flow.rs SSA lowering paths and the
# matching Cranelift codegen for loops, [[ ]] dispatch, and for-in.
set -eu
. common.sh
setup control_flow
plan=66
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

# --- dispatch statements ([[ ]] dispatch form, DD#66 — replaces case) ---

# cf.c1 dispatch with single matching arm
run_sash 'x=hello; [[ $x == hello { echo greeted } ]]'
[[ $rc = 0 && $out = "greeted" && -z $err ]]; chk cf.c1

# cf.c2 dispatch with default arm (||)
run_sash 'x=foo; [[ $x == bar { echo bar } || { echo other } ]]'
[[ $rc = 0 && $out = "other" && -z $err ]]; chk cf.c2

# cf.c3 dispatch with multiple arms, first matches
run_sash 'x=one; [[ $x == one { echo 1 } two { echo 2 } three { echo 3 } ]]'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk cf.c3

# cf.c4 dispatch with multiple arms, middle matches
run_sash 'x=two; [[ $x == one { echo 1 } two { echo 2 } three { echo 3 } ]]'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk cf.c4

# cf.c5 regex dispatch with alternation ^(a|b|c)$ (=~ replaces the a|b|c arm)
run_sash 'x=b; [[ $x =~ ^(a|b|c)$ { echo abc } || { echo other } ]]'
[[ $rc = 0 && $out = "abc" && -z $err ]]; chk cf.c5

# cf.c5b regex alternation with a NON-matching subject falls through to ||
run_sash 'x=q; [[ $x =~ ^(a|b|c)$ { echo abc } || { echo other } ]]'
[[ $rc = 0 && $out = "other" && -z $err ]]; chk cf.c5b

# cf.c6 regex prefix dispatch (the glob `h*` becomes the regex `^h`)
run_sash 'x=hello; [[ $x =~ ^h { echo h-prefix } || { echo other } ]]'
[[ $rc = 0 && $out = "h-prefix" && -z $err ]]; chk cf.c6

# cf.c7 regex `.` IS a single-char wildcard (unlike the *-only glob it
#       replaces): `^.b$` matches `ab`.
run_sash 'x=ab; [[ $x =~ ^.b$ { echo wildcard } || { echo star-only } ]]'
[[ $rc = 0 && $out = "wildcard" && -z $err ]]; chk cf.c7

# cf.c8 regex `[0-9]` IS a real character class (unlike the literal `[...]`
#       of the *-only glob it replaces): `^[0-9]$` matches `5`.
run_sash 'x=5; [[ $x =~ ^[0-9]$ { echo charclass } || { echo star-only } ]]'
[[ $rc = 0 && $out = "charclass" && -z $err ]]; chk cf.c8

# cf.c9 dispatch with no matching arm and no default is a no-op
run_sash 'x=z; [[ $x == a { echo a } b { echo b } ]]; echo done'
[[ $rc = 0 && $out = "done" && -z $err ]]; chk cf.c9

# cf.c10 dispatch arm body spanning multiple statements
run_sash 'x=a; [[ $x == a { echo one; echo two } || { echo other } ]]'
[[ $rc = 0 && $out = $'one\ntwo' && -z $err ]]; chk cf.c10

# cf.c11 dispatch with a quoted (interpolated) subject
run_sash 'x=hello; [[ "$x" == hello { echo hi } ]]'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk cf.c11

# cf.c12 dispatch with an empty arm body
run_sash 'x=skip; [[ $x == skip { } || { echo nope } ]]; echo done'
[[ $rc = 0 && $out = "done" && -z $err ]]; chk cf.c12

# cf.c13 the removed `case` keyword is a compile error (DD#66)
run_sash 'x=a; case $x in a) echo a ;; esac'
[[ $rc != 0 && -z $out && -n $err ]]; chk cf.c13

# cf.c14 `case` reject fires regardless of the fall-through terminators used
#        (`;&`/`;;&` are gone with case — DD#66)
run_sash 'case x in a) echo a ;& b) echo b ;;& *) echo c ;; esac'
[[ $rc != 0 && -z $out && -n $err ]]; chk cf.c14

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
run_sash 'for x in $(argv 1); do attest x [[ $x =~ . ]]; echo "$x"; done' alpha beta gamma
[[ $rc = 0 && $out = $'alpha\nbeta\ngamma' && -z $err ]]; chk cf.f4

# cf.f5 an accumulating loop is written with the ARITHMETIC header, whose
#       variable is a genuine int. This used to be `for x in 10 20 30` with
#       `(( sum += x ))`, which only worked because a word-list loop variable
#       had no type at all; a word list binds words, so that is now SB4030
#       (DD#86, spec 86.56). The arithmetic header is the form that means a
#       counting loop.
run_sash 'int sum=0; for (( i = 10; i < 40; i += 10 )); do (( sum += i )); done; echo $sum'
[[ $rc = 0 && $out = "60" && -z $err ]]; chk cf.f5

# cf.f6 nested for loops
run_sash 'for i in 1 2; do for j in a b; do echo "$i$j"; done; done'
[[ $rc = 0 && $out = $'1a\n1b\n2a\n2b' && -z $err ]]; chk cf.f6

# cf.f7 for over an empty user-arg slice runs zero iterations
run_sash 'for x in $(argv 1); do attest x [[ -n $x ]]; echo never; done; echo done'
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

# cf.f11 the args joined into a string via `$(join args)` (was `"$@"`,
# dropped by DD#75/DD#81 — `$@` looks scalar but splats).
run_sash 'fn greet { echo $(join args); }; greet alpha beta gamma'
[[ $rc = 0 && $out = "alpha beta gamma" && -z $err ]]; chk cf.f11

# cf.f12 for over @args iterates the function's own args (was `$@`)
run_sash 'fn each { for x in @args; do echo "$x"; done }; each a b c'
[[ $rc = 0 && $out = $'a\nb\nc' && -z $err ]]; chk cf.f12

# cf.f13 $# inside a function is the caller's arg count
run_sash 'fn n { echo "$#"; }; n a b'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk cf.f13

# cf.f14 $1 inside a function is the caller's first arg
run_sash 'fn one { echo "$1"; }; one alpha'
[[ $rc = 0 && $out = "alpha" && -z $err ]]; chk cf.f14

# --- DD#46 top-level positional rejection (directed to DD#60 builtins) ---

# cf.dd46.1 top-level $@ rejected (SB5045; directs to $(argv 1))
run_sash 'echo "$@"'
[[ $rc != 0 && -z $out && -n $err ]]; chk cf.dd46.1

# cf.dd46.2 top-level $1 rejected (use $(arg 1))
run_sash 'echo "$1"'
[[ $rc != 0 && -z $out && -n $err ]]; chk cf.dd46.2

# cf.dd46.3 top-level $* rejected (SB5045; directs to $(argv 1))
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

# ── `until` — the classic `until COND; do …; done` form ──────────────
# spec/brace.sh 57-63 covers the DD#96 brace form (`until (( … )) { … }`)
# only. The `do … done` spelling reaches the same lowering by a different
# parser path, and nothing exercised it outside the deleted Rust suite.
# cf.u1 until counts down — the body runs while the condition is false
run_sash 'int x=3
until (( x <= 0 )); do
    echo "$x"
    x=$(( x - 1 ))
done
echo done'
[[ $rc = 0 && $out = "3
2
1
done" && -z $err ]]; chk cf.u1

# cf.u2 a condition already true at entry runs the body zero times —
# `until` is the negation of `while`, not a do-while.
run_sash 'int x=0
until (( x <= 0 )); do
    echo body
    x=$(( x - 1 ))
done
echo after'
[[ $rc = 0 && $out = "after" && -z $err ]]; chk cf.u2

# cf.u3 `if` nested in `while`, guarded by a chain of `||`-joined [[ ]].
# Separate tests, not one test with `||` inside it — SB4053 rejects that.
run_sash 'int i=0
while (( i < 6 )); do
    if [[ $i == "0" ]] || [[ $i == "2" ]] || [[ $i == "4" ]]; then
        echo "even $i"
    fi
    (( i = i + 1 ))
done'
[[ $rc = 0 && $out = "even 0
even 2
even 4" && -z $err ]]; chk cf.u3

# --- nested @-splat loops (DD#100/DD#69) ----------------------------
# Every array-for loop used to share ONE synthesized index variable, so an
# inner splat loop destroyed the outer loop's index and the outer loop ran
# exactly once. Each loop now gets its own index, keyed on its header block.

# cf.n1 two different arrays: the outer loop must run to completion
run_sash 'array b=(1 2 3); array c=(p q); for x in @b; do for y in @c; do echo "$x$y"; done; done'
[[ $rc = 0 && $out = $'1p\n1q\n2p\n2q\n3p\n3q' && -z $err ]]; chk cf.n1

# cf.n2 the same array nested inside itself
run_sash 'array b=(1 2 3); for x in @b; do for y in @b; do echo "$x$y"; done; done'
[[ $rc = 0 && $out = $'11\n12\n13\n21\n22\n23\n31\n32\n33' && -z $err ]]; chk cf.n2

# cf.n3 three deep
run_sash 'array b=(1 2); array c=(p q); array e=(X Y); for x in @b; do for y in @c; do for z in @e; do echo "$x$y$z"; done; done; done'
[[ $rc = 0 && $out = $'1pX\n1pY\n1qX\n1qY\n2pX\n2pY\n2qX\n2qY' && -z $err ]]; chk cf.n3

# cf.n4 nested map splat
run_sash 'map m=([k]=1 [j]=2); map n=([p]=x [q]=y); for a in @m; do for b in @n; do echo "$a$b"; done; done'
[[ $rc = 0 && $out = $'1x\n1y\n2x\n2y' && -z $err ]]; chk cf.n4

# cf.n5 nested argv splat (a separate lowering, same shared-index defect)
run_sash 'fn f { for x in @args; do for y in @args; do echo "$x$y"; done; done; }; f a b'
[[ $rc = 0 && $out = $'aa\nab\nba\nbb' && -z $err ]]; chk cf.n5

# cf.n6 a counter and both loop variables survive the nest
run_sash 'array b=(1 2 3); array c=(p q); int n=0; for x in @b; do for y in @c; do (( n += 1 )); done; done; echo "$n $x $y"'
[[ $rc = 0 && $out = "6 3 q" && -z $err ]]; chk cf.n6

# cf.n7 outer splat, inner literal list (the literal form was always correct)
run_sash 'array b=(1 2); for x in @b; do for y in a b; do echo "$x$y"; done; done'
[[ $rc = 0 && $out = $'1a\n1b\n2a\n2b' && -z $err ]]; chk cf.n7

# --- break / continue inside @-splat loops --------------------------
# lower_for_array/lower_for_argv_slice never pushed state.loop_stack, so
# handle_break_stmt's `depth > 0` guard silently dropped both. The index
# increment now lives in a latch block that `continue` targets.

# cf.n8 break leaves an array-splat loop
run_sash 'array b=(1 2 3); for x in @b; do echo "$x"; break; done'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk cf.n8

# cf.n9 continue skips the rest of the body without hanging
run_sash 'array b=(1 2 3); for x in @b; do if [[ $x == 2 ]]; then continue; fi; echo "$x"; done'
[[ $rc = 0 && $out = $'1\n3' && -z $err ]]; chk cf.n9

# cf.n10 break in the inner loop leaves only the inner loop
run_sash 'array b=(1 2 3); array c=(p q); for x in @b; do for y in @c; do echo "$x$y"; break; done; done'
[[ $rc = 0 && $out = $'1p\n2p\n3p' && -z $err ]]; chk cf.n10

# cf.n11 break 2 leaves both loops
run_sash 'array b=(1 2 3); array c=(p q); for x in @b; do for y in @c; do echo "$x$y"; break 2; done; done'
[[ $rc = 0 && $out = "1p" && -z $err ]]; chk cf.n11

# cf.n12 break in an argv splat loop
run_sash 'fn f { for x in @args; do echo "$x"; break; done; }; f a b c'
[[ $rc = 0 && $out = "a" && -z $err ]]; chk cf.n12

# cf.n13 a value assigned before the break escapes the loop
run_sash 'array b=(1 2 3); int n=0; for x in @b; do (( n += 1 )); if [[ $x == 2 ]]; then break; fi; done; echo "$n"'
[[ $rc = 0 && $out = "2" && -z $err ]]; chk cf.n13

# --- SB5080: reserved names as a for variable (DD#100 Part B) -------
# DD#86 exempts a for variable from the shadowing gate ("a for variable is
# not a declaration"), and SB5052 only gates *writes*, so a loop variable
# could take a reserved name. Inside a function that gave one name two live
# meanings: $A read the loop variable, len A / ${A[0]} read the slot.

# cf.n14 a collection-slot letter cannot be a for variable
run_sash 'fn f { for A in @B; do echo "$A"; done; }; array a=(p q); array b=(x y); f =a =b'
[[ $rc != 0 && -z $out && $err == *SB5080* ]]; chk cf.n14

# cf.n15 nor can the immutable args array
run_sash 'fn f { for args in @A; do echo "$args"; done; }; array a=(1 2); f =a'
[[ $rc != 0 && -z $out && $err == *SB5080* ]]; chk cf.n15

# cf.n16 a lowercase name that merely looks like a slot is fine
run_sash 'array b=(1 2); for e in @b; do echo "$e"; done'
[[ $rc = 0 && $out = $'1\n2' && -z $err ]]; chk cf.n16

# --- SB5081: break/continue with no enclosing loop ------------------
# An empty loop_stack made both a silent no-op that compiled and did nothing.

# cf.n17 break at top level
run_sash 'echo hi; break'
[[ $rc != 0 && -z $out && $err == *SB5081* ]]; chk cf.n17

# cf.n18 a function body starts a fresh loop context
run_sash 'fn f { break; }; f'
[[ $rc != 0 && -z $out && $err == *SB5081* ]]; chk cf.n18


set -e
tally
