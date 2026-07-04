#!/bin/bash
# spec/brace.sh — DD#68 brace-block control flow (if/while/until/for { }).
# then/fi & do/done stay as DD#67 sugar; both forms build the same IR.
set -eu
. common.sh
setup brace
# DD#68 is a PARSER-ONLY feature: the brace and `then/do` forms build the
# identical If/While/For IR, so native (the IR reference) is authoritative.
# Run native only — the c/janet backends have pre-existing control-flow gaps
# (janet elif/continue, c until/`(( ))`) that the keyword forms hit
# identically; see cf-backend-gaps-brace-surfaced. (Same native-only pattern
# as convert.sh.)
[[ ${SASH_MODE:-native} != native ]] && exit 0
plan=25
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
run_sash 'if true && false { echo y } else { echo n }'
[[ $rc = 0 && $out = "n" && -z $err ]]; chk bc.a6b

# A7 list condition (status = last command)
run_sash 'if false; true { echo y } else { echo n }'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk bc.a7

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
# E1 $? catching: a failing body command is caught by the if (rc 0, no abort).
# (`echo $?` after an if is SB5012 in both forms — parser-only parity holds.)
run_sash 'if true { false }'
[[ $rc = 0 && -z $err ]]; chk bc.e1
# E2 else-arm catching
run_sash 'if false { true } else { false }'
[[ $rc = 0 && -z $err ]]; chk bc.e2
# E3 redirect inside a brace body (statements in the body parse normally)
run_sash 'if true { echo y > /dev/null }'
[[ $rc = 0 && -z $out && -z $err ]]; chk bc.e3
# E4 nested
run_sash 'if true { if true { echo y } }'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk bc.e4

set -e
tally
