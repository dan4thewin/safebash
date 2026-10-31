#!/bin/bash
# spec/control_flow.sh — while / dispatch / for runtime behavior.
# Covers transform/control_flow.rs SSA lowering paths and the
# matching Cranelift codegen for loops, [[ ]] dispatch, and for-in.
set -eu
. common.sh
setup control_flow
plan=125
echo "1..$plan" >&3

set +e

# --- while loops ---

# cf.w1 while with arithmetic condition
run_sash 'int i=0; while (( i < 3 )); do echo $i; (( i++ )); done'
[[ $rc = 0 && $out = $'0\n1\n2' && -z $err ]]; chk cf.w1

# cf.w2 while with command condition
run_sash 'int i=3; while [[ "$i" != 0 ]]; do echo $i; (( i-- )); done'
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
run_sash 'fn n { : @args; echo "$#"; }; n a b'
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
    if [[ "$i" == "0" ]] || [[ "$i" == "2" ]] || [[ "$i" == "4" ]]; then
        echo "even $i"
    fi
    (( i = i + 1 ))
done'
[[ $rc = 0 && $out = "even 0
even 2
even 4" && -z $err ]]; chk cf.u3

# --- `until` with a condition that does not finish in the header ----
# cf.u1/u2 only ever used `(( ))`, where the condition is evaluated and
# branched on in the SAME block. `until` was desugared by appending a
# NegateTestResult sigil to the loop *header*, which is only the right
# block for that shape. A user-function condition is inlined into fresh
# blocks, so the header ended in a `Goto` into the callee and the negate
# ran on the PREVIOUS iteration's value, which the callee then overwrote:
# `until f` silently behaved exactly like `while f`.

# cf.u4 a predicate that succeeds ends an `until` loop before it starts.
run_sash 'fn p { return 0 }
int n=0
until p; do
    (( n = n + 1 ))
    if (( n > 2 )); then break; fi
done
echo "n=$n"'
[[ $rc = 0 && $out = "n=0" && -z $err ]]; chk cf.u4

# cf.u5 ...and one that fails keeps it running. Together with cf.u4 this
# pins both rows: swapping them is exactly the old inverted behaviour.
run_sash 'fn p { return 1 }
int n=0
until p; do
    (( n = n + 1 ))
    if (( n > 2 )); then break; fi
done
echo "n=$n"'
[[ $rc = 0 && $out = "n=3" && -z $err ]]; chk cf.u5

# cf.u6 the same bug by a second route: a piped `until` reaches lowering
# through lower_pipe_while_read, which never applied the header patch at
# all, so the negation was not merely misplaced but absent. A successful
# read must end the loop immediately.
run_sash '/bin/printf "a\nb\n" | until readline v; do
    attest v [[ -n $v ]]
    echo "got-$v"
done
echo done'
[[ $rc = 0 && $out = "done" && -z $err ]]; chk cf.u6

# cf.u7 the external-command condition, which was always correct — a
# regression shield for the forms the fix moved the sigil away from.
run_sash 'int k=0
until /bin/true; do
    (( k = k + 1 ))
    if (( k > 5 )); then break; fi
done
int j=0
until /bin/false; do
    (( j = j + 1 ))
    if (( j > 2 )); then break; fi
done
echo "$k $j"'
[[ $rc = 0 && $out = "0 3" && -z $err ]]; chk cf.u7

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

# cf.n19-cf.n21 (journal 542): a `$( )` body is a function body (DD#86), so a
# loop outside the capture does not enclose a `break`/`continue` inside it.
# Both used to branch out of the open capture and lose all later output (B60).

# cf.n19 break in a capture inside a for
run_sash 'for x in a b { str s=$(echo $x; break); echo "[$s]" }
echo end'
[[ $rc != 0 && -z $out && $err == *':1:33: error [SB5081]'* ]]; chk cf.n19

# cf.n20 continue in a capture inside a while
run_sash 'int n=0
while (( n < 3 )) { n+=1; str s=$(echo y$n; continue); echo "[$s]" }
echo end'
[[ $rc != 0 && -z $out && $err == *':2:45: error [SB5081]'* ]]; chk cf.n20

# cf.n21 control: a loop wholly inside the capture still encloses its break
run_sash 'for x in a b { str s=$(for y in 1 2 { echo $x$y; break }); echo "[$s]" }
echo end'
[[ $rc = 0 && $out = $'[a1]\n[b1]\nend' && -z $err ]]; chk cf.n21

# cf.n22-cf.n23 (journal 542): a `while`/`until` condition belongs to its own
# loop, as SB5081 already judged, so a `break` there leaves that loop. It used
# to leave the enclosing `for` instead (B101).

# cf.n22 while
run_sash 'for i in 1 2 {
  while break {
    echo "in $i"
  }
  echo "after while $i"
}
echo done'
[[ $rc = 0 && $out = $'after while 1\nafter while 2\ndone' && -z $err ]]; chk cf.n22

# cf.n23 until
run_sash 'for i in 1 2 {
  until break {
    echo "in $i"
  }
  echo "after until $i"
}
echo done'
[[ $rc = 0 && $out = $'after until 1\nafter until 2\ndone' && -z $err ]]; chk cf.n23

# --- SB5097: the loop COUNT on break/continue ------------------------
# SB5081 rejects a `break`/`continue` that would be a silent no-op for want of
# an enclosing loop. The optional count reached the same no-op by a second
# route and nothing stopped it: the transform guarded on `n > 0 && depth > 0`,
# so `continue 0` fell straight through and did nothing, where bash fails the
# builtin ("loop count out of range") and abandons the loop. Two neighbouring
# spellings were worse than a no-op — `continue $k` crashed the compiler in
# `literal_name_of`, and `continue abc` became `continue 1` via `unwrap_or(1)`.
#
# `continue N` was not specified at all before this block (only `break 2` was),
# which is how all of it survived in twin copies of the same handler.

# cf.lc1 `continue 2` in nested `while` — byte-identical to bash
run_sash 'int i=0
while (( i < 3 )) {
  i=$(( i + 1 )); int j=0
  while (( j < 3 )) { j=$(( j + 1 )); if (( j == 2 )) { continue 2 }; echo "i=$i j=$j" }
}'
[[ $rc = 0 && $out = $'i=1 j=1\ni=2 j=1\ni=3 j=1' && -z $err ]]; chk cf.lc1

# cf.lc2 `break 2` in nested `while`
run_sash 'int i=0
while (( i < 3 )) {
  i=$(( i + 1 )); int j=0
  while (( j < 3 )) { j=$(( j + 1 )); if (( j == 2 )) { break 2 }; echo "i=$i j=$j" }
}
echo done'
[[ $rc = 0 && $out = $'i=1 j=1\ndone' && -z $err ]]; chk cf.lc2

# cf.lc3 `continue 2` in nested `for` — `for` pushes its loop context before
#        its single lowering pass, unlike `while`
run_sash 'for a in 1 2 3 {
  for b in x y z { if [[ $b == y ]] { continue 2 }; echo "$a$b" }
}'
[[ $rc = 0 && $out = $'1x\n2x\n3x' && -z $err ]]; chk cf.lc3

# cf.lc4 a count deeper than the nesting CLAMPS to the outermost loop, as bash
#        does — this is not an error
run_sash 'int i=0
while (( i < 3 )) { i=$(( i + 1 )); if (( i == 2 )) { continue 9 }; echo $i }'
[[ $rc = 0 && $out = $'1\n3' && -z $err ]]; chk cf.lc4

# cf.lc5 a bare `continue` inside a `while`. `lower_while` lowers the body
#        twice and pushes the loop context only after pass 1, so this reaches
#        the handler once with an empty stack; emitting nothing there is
#        correct, because pass 1's instructions are discarded.
run_sash 'int i=0
while (( i < 3 )) { i=$(( i + 1 )); if (( i == 2 )) { continue }; echo $i }'
[[ $rc = 0 && $out = $'1\n3' && -z $err ]]; chk cf.lc5

# cf.lc6 `continue 0` is the silent no-op SB5081 exists to prevent
run_sash 'int i=0
while (( i < 3 )) { i=$(( i + 1 )); if (( i == 2 )) { continue 0 }; echo $i }'
[[ $rc != 0 && -z $out && $err == *SB5097* && $err == *"silent no-op"* ]]; chk cf.lc6

# cf.lc7 `break 0` likewise — the twins share one rule
run_sash 'int i=0
while (( i < 3 )) { i=$(( i + 1 )); if (( i == 2 )) { break 0 }; echo $i }'
[[ $rc != 0 && -z $out && $err == *SB5097* && $err == *"silent no-op"* ]]; chk cf.lc7

# cf.lc8 `00` parses to zero too — the check is on the value, not the spelling
run_sash 'int i=0
while (( i < 3 )) { i=$(( i + 1 )); if (( i == 2 )) { continue 00 }; echo $i }'
[[ $rc != 0 && -z $out && $err == *SB5097* ]]; chk cf.lc8

# cf.lc9 a non-numeric count silently MEANT `continue 1` before this rule
run_sash 'int i=0
while (( i < 3 )) { i=$(( i + 1 )); if (( i == 2 )) { continue abc }; echo $i }'
[[ $rc != 0 && -z $out && $err == *SB5097* && $err == *"not a number"* ]]; chk cf.lc9

# cf.lc10 a non-literal count ICEd the compiler in `literal_name_of`
run_sash 'int i=0
int k=2
while (( i < 3 )) { i=$(( i + 1 )); if (( i == 2 )) { continue $k }; echo $i }'
[[ $rc != 0 && -z $out && $err == *SB5097* && $err != *"internal error"* ]]; chk cf.lc10

# cf.lc11 extra arguments were silently ignored; bash fails with "too many"
run_sash 'int i=0
while (( i < 3 )) { i=$(( i + 1 )); if (( i == 2 )) { continue 1 2 }; echo $i }'
[[ $rc != 0 && -z $out && $err == *SB5097* && $err == *"at most one"* ]]; chk cf.lc11

# cf.lc12 a quoted count is not a literal — write the number
run_sash 'int i=0
while (( i < 3 )) { i=$(( i + 1 )); if (( i == 2 )) { continue "2" }; echo $i }'
[[ $rc != 0 && -z $out && $err == *SB5097* ]]; chk cf.lc12

# --- SB5009: a for-in source must denote a COLLECTION ----------------
# DD#28 eliminates word splitting, so a source holding ONE scalar could only
# ever run a single iteration. The rule used to be keyed on the bare `$v`
# shape alone; every other spelling of a scalar fell through it, and a value
# intrinsic fell all the way past lowering. `resolve` exempts an intrinsic
# name from the DD#23 path check, so `for v in $(getcwd)` COMPILED CLEAN and
# then aborted the program at runtime — "bare command passed to sash_exec",
# exit 127 — which is verbatim the failure DD#111 fixed in the other
# direction. fs.1 is that bug's guard.

# fs.1 a nullary value intrinsic as a for-in source
run_sash 'for v in $(getcwd) { echo "$v" }'
[[ $rc != 0 && -z $out && $err == *SB5009* ]]; chk fs.1

# fs.2 …and it is the whole family, not one name
run_sash 'for v in $(getpid) { echo "$v" }'
[[ $rc != 0 && -z $out && $err == *SB5009* ]]; chk fs.2

# fs.3 quoting does not make a scalar a collection — the reverse of the
# quoted-SPLAT rule (SB5026), which denies what quoting here used to grant.
run_sash 'for v in "$(getcwd)" { echo "$v" }'
[[ $rc != 0 && -z $out && $err == *SB5009* ]]; chk fs.3

# fs.4 an intrinsic taking arguments is still a scalar. This one used to be
# SB5020 (getenv is tainted); the source-shape rule is the more specific
# complaint and now fires first.
run_sash 'for v in $(getenv HOME) { echo "$v" }'
[[ $rc != 0 && -z $out && $err == *SB5009* ]]; chk fs.4

# fs.5 `$(join a)` returns ONE joined string, so it is a scalar even though
# its argument is a collection.
run_sash 'array a=(x y)
for v in $(join a) { echo "$v" }'
[[ $rc != 0 && -z $out && $err == *SB5009* ]]; chk fs.5

# fs.6 one element of an array is a scalar
run_sash 'array a=(x y)
for v in ${a[0]} { echo "$v" }'
[[ $rc != 0 && -z $out && $err == *SB5009* ]]; chk fs.6

# fs.7 an arithmetic expansion is a number
run_sash 'for v in $(( 1 + 1 )) { echo "$v" }'
[[ $rc != 0 && -z $out && $err == *SB5009* ]]; chk fs.7

# fs.8 a `$( )` capture is ONE value however many lines it holds, so a command
# substitution is a scalar source like any other. This used to be accepted and
# lowered to a pipe-and-read loop that split the capture on newlines — word
# splitting by another name, which DD#28 eliminates. It never matched bash
# either: bash word-splits `a b c` into three iterations, sash ran exactly one
# binding the whole string (the old spec pinned `out = "[a b c]"` and called the
# divergence deliberate).
run_sash 'for x in $(echo a b c) { attest x [[ -n $x ]]; echo "[$x]" }'
[[ $rc != 0 && -z $out && $err == *SB5009* ]]; chk fs.8

# fs.9 …and a one-element LITERAL list is still a list, not a scalar
run_sash 'for v in solo { echo "$v" }'
[[ $rc = 0 && $out = "solo" && -z $err ]]; chk fs.9

# ── fs.10–fs.13: the four shapes the old line-stream lowering miscompiled ──
# Each compiled clean and produced a wrong answer; all are now SB5009. The
# rule is deny-by-default, so these are rejected by the general case rather
# than by four special cases. (`cmd | while readline v` — spec 14.46 in
# commands.sh — is the streaming replacement the diagnostic names.)

# fs.10 a statement SEQUENCE was lowered as a PIPELINE: `$(echo a; echo b)`
# became `echo a | echo b`, so only `b` survived. bash prints both.
run_sash 'for k in $(echo a; echo b) { attest k [[ -n $k ]]; echo "[$k]" }'
[[ $rc != 0 && -z $out && $err == *SB5009* ]]; chk fs.10

# fs.11 a real PIPELINE inside the capture is not a `StmtKind::Command`, so the
# stage builder returned None and the loop silently dropped it — zero
# iterations, no diagnostic. bash prints `[a]`.
run_sash 'for k in $(echo a | cat) { attest k [[ -n $k ]]; echo "[$k]" }'
[[ $rc != 0 && -z $out && $err == *SB5009* ]]; chk fs.11

# fs.12 a USER FUNCTION capture took the raw name with no DD#23 resolution and
# reached `sash_exec` bare: the compiled program aborted at runtime with
# "sash: bug: bare command passed to sash_exec". Same class as the test-operand
# ICE (bug-fn-capture-in-test-operand-ice), a third position missing the lift.
run_sash 'fn g { echo a }
for k in $(g) { attest k [[ -n $k ]]; echo "[$k]" }'
[[ $rc != 0 && -z $out && $err == *SB5009* ]]; chk fs.12

# fs.13 `$(< file)` was exempted as "a stream of lines, like an external
# command" — the same mistake. It already ran exactly one iteration binding the
# entire file, embedded newlines and all.
run_sash 'for k in $(< /etc/hostname) { attest k [[ -n $k ]]; echo "[$k]" }'
[[ $rc != 0 && -z $out && $err == *SB5009* ]]; chk fs.13

# fs.14 the two collection captures keep their OWN diagnostics when combined
# with another command — there is no sane way to write that, and each already
# says so in the author's terms. `keys` is SB5073 ("recognised only when it is
# the whole substitution"); the `argv` splat is SB5025 (argv.sh av.c1–av.c3).
# The source-shape rule must stay quiet for both, or it answers "iterate an
# array instead", which is not the problem. Pins the deferral, including that
# SB5009 does NOT also fire.
run_sash 'map m
m[k]=1
echo ${m[k]}
for k in $(keys m; /bin/echo e) { echo "[$k]" }'
[[ $rc != 0 && $err == *SB5073* && $err != *SB5009* ]]; chk fs.14

# fs.15 a REDIRECT on the captured command does not change what the capture
# is. The source-shape rule has to see through `StmtKind::Redirected` to the
# command underneath, or `$(cmd 2>/dev/null)` reads as "not a command" and
# falls out of the rule entirely — the same silent single-iteration bind that
# fs.11-fs.13 each pinned for a different wrapper.
run_sash 'for k in $(/bin/echo a 2>/dev/null) { attest k [[ -n $k ]]; echo "[$k]" }'
[[ $rc != 0 && -z $out && $err == *SB5009* ]]; chk fs.15

# Loop-control integers must not allocate decimal strings on each iteration.
# The small heap leaves ample room for the table, but not 20000 counter strings.
SASH_CFLAGS="${SASH_CFLAGS:-} --heap-size=64K" run_sash 'array a=(red blue)
int n=0
while (( n<10000 )) { for x in @a { : $x }; (( n++ )) }
echo $n'
[[ $rc = 0 && $out = 10000 && -z $err && -z $rerr ]]; chk loopint.1

SASH_CFLAGS="${SASH_CFLAGS:-} --heap-size=64K" run_sash 'map m=([k]=red [j]=blue)
int n=0
while (( n<10000 )) { for x in @m { : $x }; (( n++ )) }
echo $n'
[[ $rc = 0 && $out = 10000 && -z $err && -z $rerr ]]; chk loopint.2

SASH_CFLAGS="${SASH_CFLAGS:-} --heap-size=64K" run_sash 'map m=([k]=red [j]=blue)
int n=0
while (( n<10000 )) { for k in $(keys m) { : $k }; (( n++ )) }
echo $n'
[[ $rc = 0 && $out = 10000 && -z $err && -z $rerr ]]; chk loopint.3

SASH_CFLAGS="${SASH_CFLAGS:-} --heap-size=64K" run_sash 'int n=0
while (( n<10000 )) {
    for x in $(argv 1) { attest x [[ -n $x ]]; : $x }
    (( n++ ))
}
echo $n' red blue
[[ $rc = 0 && $out = 10000 && -z $err && -z $rerr ]]; chk loopint.4

# Even an empty collection rechecks its bound once per enclosing iteration.
SASH_CFLAGS="${SASH_CFLAGS:-} --heap-size=64K" run_sash 'array a
int n=0
while (( n<10000 )) { for x in @a { : $x }; (( n++ )) }
echo $n'
[[ $rc = 0 && $out = 10000 && -z $err && -z $rerr ]]; chk loopint.5

# Compiler counters must not overwrite user variables, including later ones.
run_sash 'str __ai1=first
str __ai1_=second
array a=(x y)
for x in @a { echo $x }
str __ai1__=later
echo "$__ai1 $__ai1_ $__ai1__"'
[[ $rc = 0 && $out = $'x\ny\nfirst second later' && -z $err && -z $rerr ]]; chk loopint.10

run_sash 'str __fi1=start
str __argc1=bound
for x in $(argv 1) { attest x [[ -n $x ]]; echo $x }
str __fi1_=future_i
str __argc1_=future_b
echo "$__fi1 $__argc1 $__fi1_ $__argc1_"' red blue
[[ $rc = 0 && $out = $'red\nblue\nstart bound future_i future_b' && -z $err && -z $rerr ]]; chk loopint.11

run_sash 'array a=(red blue)
for __ai1 in @a { echo $__ai1 }'
[[ $rc = 0 && $out = $'red\nblue' && -z $err && -z $rerr ]]; chk loopint.12

run_sash 'for __fi1 in $(argv 1) {
    attest __fi1 [[ -n $__fi1 ]]
    echo $__fi1
}' red blue
[[ $rc = 0 && $out = $'red\nblue' && -z $err && -z $rerr ]]; chk loopint.13

run_sash 'for __argc1 in $(argv 1) {
    attest __argc1 [[ -n $__argc1 ]]
    echo $__argc1
}' red blue
[[ $rc = 0 && $out = $'red\nblue' && -z $err && -z $rerr ]]; chk loopint.14

run_sash 'array a=(red blue)
for x in @a { echo $x }
for __ai1 in tail { echo $__ai1 }'
[[ $rc = 0 && $out = $'red\nblue\ntail' && -z $err && -z $rerr ]]; chk loopint.15

run_sash 'str n=7
array a=(red blue)
for x in @a {
    attest n (( __ai1 = n ))
    echo "$x $__ai1"
}'
[[ $rc = 0 && $out = $'red 7\nblue 7' && -z $err && -z $rerr ]]; chk loopint.16

# ── DD#95: `&&` / `||` inside `[[ ]]` short-circuit ──
# The right operand runs only when the left one has not decided the result:
# a capture there never runs and an expansion that can abort is never
# evaluated. Before 2026-10-31 (journal 550) the whole test was lifted and
# evaluated before the branch, so each negative below aborted or ran the
# capture.

# lazy.1 a decided `||` never evaluates a right-hand `${y:?}`
run_sash 'x=a
str y=""
if [[ $x == a || ${y:?unset} == b ]] { echo yes }
echo done'
[[ $rc = 0 && $out = $'yes\ndone' && -z $err && -z $rerr ]]; chk lazy.1

# lazy.2 ... nor a zero divisor (quoted: a whole-int operand is SB4030, DD#30)
run_sash 'x=a
int z=0
if [[ $x == a || "$(( 10 / z ))" == 1 ]] { echo yes }
echo done'
[[ $rc = 0 && $out = $'yes\ndone' && -z $err && -z $rerr ]]; chk lazy.2

# lazy.3 a decided `&&` skips its right operand the same way
run_sash 'x=b
str y=""
if [[ $x == a && ${y:?unset} == b ]] { echo yes } else { echo no }
echo done'
[[ $rc = 0 && $out = $'no\ndone' && -z $err && -z $rerr ]]; chk lazy.3

# lazy.4 a skipped capture never runs: no output, no global write
run_sash 'g=old
fn f { g=new; echo tried >&2; echo b }
x=a
if [[ $x == a || $(f) == b ]] { echo "yes $g" }
echo "after $g"'
[[ $rc = 0 && $out = $'yes old\nafter old' && -z $err && -z $rerr ]]; chk lazy.4

# lazy.5 control: an undecided left operand runs the capture, and its write
# survives the merge
run_sash 'g=old
fn f { g=new; echo b }
x=b
if [[ $x == a || $(f) == b ]] { echo "yes $g" }
echo "after $g"'
[[ $rc = 0 && $out = $'yes new\nafter new' && -z $err && -z $rerr ]]; chk lazy.5

# lazy.6 a negated junction is still lazy, and `!` flips its result
run_sash 'fn g { echo tried >&2; echo b }
x=a
if [[ ! ( $x == a || $(g) == b ) ]] { echo yes } else { echo no }
echo done'
[[ $rc = 0 && $out = $'no\ndone' && -z $err && -z $rerr ]]; chk lazy.6

# lazy.7 the statement form is lazy too
run_sash 'x=a
str y=""
[[ $x == a || ${y:?unset} == b ]] && echo ok
echo done'
[[ $rc = 0 && $out = $'ok\ndone' && -z $err && -z $rerr ]]; chk lazy.7

# lazy.8 control: the operand that ran last sets the result and `$?`
run_sash 'x=a
y=c
[[ $x == a && $y == b ]] || echo "alt $?"
[[ $x == b || $y == c ]] && echo "and $?"
if [[ $x == a && ( $y == b || $y == c ) ]] { echo nest1 }
if [[ ! ( $x == a && $y == b ) ]] { echo neg1 $? }
if [[ ! ( $x == a && $y == c ) ]] { echo bad } else { echo neg2 }
echo end'
[[ $rc = 0 && $out = $'alt 1\nand 0\nnest1\nneg1 0\nneg2\nend' \
   && -z $err && -z $rerr ]]; chk lazy.8

# lazy.9 control: a loop condition re-runs its right operand on every
# iteration it is reached, and sees the left operand's current value (the
# counters are quoted: a whole-int operand is SB4030, DD#30)
run_sash 'int i=0
fn f { i+=1; echo $i }
while [[ "$i" != 9 && $(f) != 3 ]] { echo "body $i" }
echo "out $i"
int j=0
fn h { j+=1; echo $j }
until [[ "$j" == 9 || $(h) == 3 ]] { echo "ubody $j" }
echo "uout $j"'
[[ $rc = 0 && $out = $'body 1\nbody 2\nout 3\nubody 1\nubody 2\nuout 3' \
   && -z $err && -z $rerr ]]; chk lazy.9

# lazy.10 control: a predicate function and a captured condition
run_sash 'fn both { return [[ $1 == a && $2 == b ]] }
fn zed { echo z }
if both a b { echo t1 }
if both a c { echo bad } else { echo f1 }
y=q
x=$(if [[ $y == b || $(zed) == z ]] { echo cap })
echo "[$x]"'
[[ $rc = 0 && $out = $'t1\nf1\n[cap]' && -z $err && -z $rerr ]]; chk lazy.10

# lazy.11 a skipped intrinsic that would abort (argv bounds) is not run
SASH_CFLAGS="${SASH_CFLAGS:-} --no-taint" run_sash 'x=a
if [[ $x == a || $(arg 5) == b ]] { echo yes }
echo done'
[[ $rc = 0 && $out = $'yes\ndone' && -z $err && -z $rerr ]]; chk lazy.11


set -e
tally
