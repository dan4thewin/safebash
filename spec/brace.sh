#!/bin/bash
# spec/brace.sh — DD#68 brace-block control flow (if/while/until/for { }).
# then/fi & do/done stay as DD#67 sugar; both forms build the same IR.
set -eu
. common.sh
setup brace
# DD#68 is a PARSER-ONLY feature: the brace and `then/do` forms build the
# identical If/While/For IR. It was native-only on the belief that the c
# backend had a control-flow gap (until / `(( ))`) that these forms hit; all
# eight constructs were later measured byte-identical between the backends, so
# the guard was stale and is gone. Running both modes keeps it that way.
plan=42
echo "1..$plan" >&3

set +e

# ── A. if brace bodies ───────────────────────────────────────────────
# A1 then-arm
run_sash 'if true { echo y }'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk bc.a1

# A2 else
run_sash 'if false { echo y } else { echo e }'
[[ $rc = 0 && $out = "e" && -z $err ]]; chk bc.a2

# A3 elif
run_sash 'x=2; if [[ $x == 1 ]] { echo one } elif [[ $x == 2 ]] { echo two } else { echo z }'
[[ $rc = 0 && $out = "two" && -z $err ]]; chk bc.a3

# A4 else if (sugar for else + nested if)
run_sash 'x=3; if [[ $x == 1 ]] { echo a } else if [[ $x == 3 ]] { echo c } else { echo z }'
[[ $rc = 0 && $out = "c" && -z $err ]]; chk bc.a4

# A6 compound condition
run_sash 'if true && true { echo y }'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk bc.a6a
# bc.a6b `&&` RHS `false` → the compound condition CATCHES (DD#55; DG 2026-08-19)
# → the else branch prints n (the `&&` aggregate is the condition, its operands
# are caught, not aborted).
run_sash 'if true && false { echo y } else { echo n }'
[[ $rc = 0 && $out = "n" && -z $err ]]; chk bc.a6b

# A7 list condition
# bc.a7 `if a; b` is `a; if b` (DG 2026-08-19): the non-final `false` is a bare
# command whose status is discarded → it ABORTS (never reaches `if true`).
run_sash 'if false; true { echo y } else { echo n }'
[[ $rc != 0 && -z $out ]]; chk bc.a7

# bc.a8 `||` condition: both sides fail → condition false → else (the right
# operand's failure is caught, not aborted — DD#55; the fix that unmasked the
# `||` $?-reset). bc.a9 right side succeeds → then.
run_sash 'if false || false { echo y } else { echo n }'
[[ $rc = 0 && $out = "n" && -z $err ]]; chk bc.a8
run_sash 'if false || true { echo y } else { echo n }'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk bc.a9

# ── B. while / until / for ───────────────────────────────────────────
# B1 while
run_sash 'int i=0; while (( i < 2 )) { echo $i; (( i++ )) }'
[[ $rc = 0 && $out = $'0\n1' && -z $err ]]; chk bc.b1

# B2 until
run_sash 'int i=0; until (( i >= 2 )) { echo $i; (( i++ )) }'
[[ $rc = 0 && $out = $'0\n1' && -z $err ]]; chk bc.b2

# B3 for-in
run_sash 'for x in a b c { echo $x }'
[[ $rc = 0 && $out = $'a\nb\nc' && -z $err ]]; chk bc.b3

# B4 C-style for
run_sash 'for ((i=0;i<2;i++)) { echo $i }'
[[ $rc = 0 && $out = $'0\n1' && -z $err ]]; chk bc.b4

# B5 break/continue unchanged
run_sash 'for x in a b c { if [[ $x == b ]] { continue }; echo $x }'
[[ $rc = 0 && $out = $'a\nc' && -z $err ]]; chk bc.b5

# ── C. sugar still works (regression, DD#67) ─────────────────────────
run_sash 'if true; then echo y; fi'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk bc.c1
run_sash 'for x in a b; do echo $x; done'
[[ $rc = 0 && $out = $'a\nb' && -z $err ]]; chk bc.c2
run_sash 'int i=0; while (( i < 1 )); do echo $i; (( i++ )); done'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk bc.c3

# ── D. disambiguation + rejections ───────────────────────────────────
# D1 brace-group condition rejected
run_sash 'if { true; } { echo y }'
[[ $rc != 0 && -z $out && -n $err ]]; chk bc.d1
# D2 no mixing then<->brace
run_sash 'if true; then echo y }'
[[ $rc != 0 && -n $err ]]; chk bc.d2a
run_sash 'if true { echo y; fi'
[[ $rc != 0 && -n $err ]]; chk bc.d2b
# D3 missing body
run_sash 'if true'
[[ $rc != 0 && -n $err ]]; chk bc.d3
# D4 literal { in for-list via quoting
run_sash "for x in '{' a { echo \$x }"
[[ $rc = 0 && $out = $'{\na' && -z $err ]]; chk bc.d4
# D5 reserved { unaffected (echo a{ still rejects)
run_sash 'echo a{'
[[ $rc != 0 && -n $err ]]; chk bc.d5

# ── E. semantics unchanged (parser-only) ─────────────────────────────
# E1/E2 a failing FINAL command in an if/else BODY ABORTS — its status is
# discarded (DD#55 bare-command-uncaught; DG 2026-08-19), exactly like a
# top-level `false`. The old rc-0 expectation was a non-aborting-builtin
# artifact of the pre-externalization true/false.
run_sash 'if true { false }'
[[ $rc != 0 && -z $out ]]; chk bc.e1
run_sash 'if false { true } else { false }'
[[ $rc != 0 && -z $out ]]; chk bc.e2
# E3 redirect inside a brace body (statements in the body parse normally)
run_sash 'if true { echo y > /dev/null }'
[[ $rc = 0 && -z $out && -z $err ]]; chk bc.e3
# E4 nested
run_sash 'if true { if true { echo y } }'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk bc.e4

# --- bc.f1-f5: a declaration as the LAST statement in a brace block ---------
# `parse_declaration_assignments` stops at a newline, a `;`, EOF, or a
# redirect, but NOT at `}`. A declaration list is greedy because `str a b c`
# declares three names, so with no stop it consumed the block's closing brace
# as a candidate name and reported SB4014 "expected variable name in
# declaration" — pointing at a declaration that was already complete. It hit
# every brace form, and only when the declaration was last in the block: any
# following `;` or newline stopped the loop first, which is why every existing
# brace spec missed it. Journal 310.

# bc.f1 fn — the form the bug was found in
run_sash 'fn f { str x="a"; echo "$x" }
f'
[[ $rc = 0 && $out = "a" && -z $err ]]; chk bc.f1

# bc.f2 for
run_sash 'for i in 1 2 { str r="y"; echo "$r$i" }'
[[ $rc = 0 && $out = "y1
y2" && -z $err ]]; chk bc.f2

# bc.f3 if
run_sash 'str c="y"
if [[ $c == y ]] { str x="a"; echo "$x" }'
[[ $rc = 0 && $out = "a" && -z $err ]]; chk bc.f3

# bc.f4 a multi-name declaration last in a block still takes all its names and
#       stops at the brace — the fix is a stop token, not a one-name limit
run_sash 'fn f { str a="1" b="2"; echo "$a$b" }
f'
[[ $rc = 0 && $out = "12" && -z $err ]]; chk bc.f4

# bc.f5 control: the redirect stop is unaffected. `int x=5 > f` must still be
#       the clean SB4046 (a redirect cannot attach to a declaration) rather
#       than regressing to the misleading SB4014.
run_sash 'int x=5 > /dev/null
echo "$x"'
[[ $rc != 0 && -z $out && $err == *SB4046* ]]; chk bc.f5

# --- bc.g*: a `{` glued to a word is SB4071 --------------------------------
# Was a P1: `{` unconditionally ended a command's word list, so
# `echo a{echo z}` became TWO statements — it printed "a" then "z" where bash
# prints the literal `a{echo z}`. Adjacency is the discriminator: a brace
# glued to the previous word is the bug, a spaced one is a deliberate block.
# The `{ … }` group itself is kept — it is the only way to put more than one
# statement on the right of `||`/`&&` (see bc.g4).

# bc.g1 the bug itself.
run_sash 'echo a{echo z}'
[[ $rc != 0 && -z $out && $err == *SB4071* ]]; chk bc.g1

# bc.g2 a builtin body was the quietest case: this printed just "a".
run_sash 'echo a{:}'
[[ $rc != 0 && -z $out && $err == *SB4071* ]]; chk bc.g2

# bc.g3 the advice SB4071 gives must work, and must agree with bash:
#       quoting yields the literal word.
run_sash "echo 'a{echo z}'"
[[ $rc = 0 && $out = 'a{echo z}' && -z $err ]]; chk bc.g3

# bc.g4 the group survives where it does real work — a multi-statement `||`
#       right-hand side has no other spelling. Removing the surface to fix
#       bc.g1 would have cost this.
run_sash '/bin/false || { echo a; echo b; }'
[[ $rc = 0 && $out = $'a\nb' && -z $rerr ]]; chk bc.g4

# bc.g5 a bare statement-position group still parses and runs.
run_sash '{ echo z }'
[[ $rc = 0 && $out = "z" && -z $rerr ]]; chk bc.g5

# bc.g6 a SPACED brace after a command word is still a block, not an error.
#       This is the documented residual: bash reads `a { echo z }` as three
#       literal words, sash as a command plus a block. Pinned so the
#       divergence is a decision on record rather than an accident.
run_sash 'echo a { echo z }'
[[ $rc = 0 && $out = $'a\nz' && -z $rerr ]]; chk bc.g6

# bc.g7 adjacency must NOT fire inside a control-flow header, where `{` opens
#       the body (DD#68) — here `a` is an argument of the condition command
#       and the brace is glued to it. This is why the fix could not simply be
#       "a brace in an argument position is an error".
run_sash 'if /bin/true a{echo y}'
[[ $rc = 0 && $out = "y" && -z $rerr ]]; chk bc.g7

# bc.g8 ...and the same for a `while` header.
run_sash 'int i=0
while ((i < 2)){ echo w; ((i=i+1)) }'
[[ $rc = 0 && $out = $'w\nw' && -z $rerr ]]; chk bc.g8

# bc.g9 the sibling gate: a stray `}` was a bare "expected expression"
#       (SB4014), which said nothing about the brace. It closes nothing here,
#       and bash keeps it literal — SB4072 says so.
run_sash 'echo a}b'
[[ $rc != 0 && -z $out && $err == *SB4072* ]]; chk bc.g9

# bc.g10 and SB4072's advice works, agreeing with bash.
run_sash "echo 'a}b'"
[[ $rc = 0 && $out = 'a}b' && -z $err ]]; chk bc.g10

set -e
tally
