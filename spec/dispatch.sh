#!/bin/bash
# spec/dispatch.sh — DD#66 [[ ]] dispatch (cond) form. Multi-way dispatch
# statement alongside the boolean [[ ]] test. `==` exact, `=~` unanchored
# regex; `PATTERN { group }` arms; optional `|| { group }` default;
# first-match-wins; statement-only.
set -eu
. common.sh
setup dispatch
plan=22
echo "1..$plan" >&3

set +e

# ── A. exact dispatch (==) ───────────────────────────────────────────
run_sash 'x=foo; [[ $x == foo { echo a; } bar { echo b; } || { echo c; } ]]'
[[ $rc = 0 && $out = "a" && -z $err ]]; chk dp.a1
run_sash 'x=bar; [[ $x == foo { echo a; } bar { echo b; } || { echo c; } ]]'
[[ $rc = 0 && $out = "b" && -z $err ]]; chk dp.a2
run_sash 'x=zzz; [[ $x == foo { echo a; } bar { echo b; } || { echo c; } ]]'
[[ $rc = 0 && $out = "c" && -z $err ]]; chk dp.a3
# A4 no default, no match → no-op
run_sash 'x=zzz; [[ $x == a { echo a; } ]]; echo done'
[[ $rc = 0 && $out = "done" && -z $err ]]; chk dp.a4
# A5 single arm = if
run_sash 'x=a; [[ $x == a { echo y; } ]]'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk dp.a5
# A6 single arm + default = if/else
run_sash 'x=q; [[ $x == a { echo y; } || { echo n; } ]]'
[[ $rc = 0 && $out = "n" && -z $err ]]; chk dp.a6

# ── B. == is exact only (no alternation) ─────────────────────────────
run_sash "x='a|b'; [[ \$x == 'a|b' { echo lit; } || { echo no; } ]]"
[[ $rc = 0 && $out = "lit" && -z $err ]]; chk dp.b1
run_sash "x=a; [[ \$x == 'a|b' { echo hit; } || { echo miss; } ]]"
[[ $rc = 0 && $out = "miss" && -z $err ]]; chk dp.b2
# B3 boolean == still exact (regression)
run_sash 'x=a; if [[ $x == a ]]; then echo y; fi'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk dp.b3a
run_sash "x=a; if [[ \$x == 'a|b' ]]; then echo y; else echo n; fi"
[[ $rc = 0 && $out = "n" && -z $err ]]; chk dp.b3b

# ── C. regex dispatch (=~, unanchored) ───────────────────────────────
run_sash 'x=apple; [[ $x =~ ^a { echo A; } || { echo o; } ]]'
[[ $rc = 0 && $out = "A" && -z $err ]]; chk dp.c1
run_sash 'x=cab; [[ $x =~ b$ { echo B; } || { echo o; } ]]'
[[ $rc = 0 && $out = "B" && -z $err ]]; chk dp.c2
run_sash 'x=b; [[ $x =~ ^(a|b)$ { echo m; } || { echo n; } ]]'
[[ $rc = 0 && $out = "m" && -z $err ]]; chk dp.c3a
run_sash 'x=c; [[ $x =~ ^(a|b)$ { echo m; } || { echo n; } ]]'
[[ $rc = 0 && $out = "n" && -z $err ]]; chk dp.c3b

# ── D. || default marker ─────────────────────────────────────────────
run_sash "x='||'; [[ \$x == '||' { echo bar; } || { echo def; } ]]"
[[ $rc = 0 && $out = "bar" && -z $err ]]; chk dp.d1
run_sash 'x=hit; [[ $x == hit { echo h; } || { echo d; } ]]'
[[ $rc = 0 && $out = "h" && -z $err ]]; chk dp.d2

# ── G. disambiguation (boolean vs dispatch) ──────────────────────────
# G1 no { ⇒ boolean compound intact
run_sash 'x=a; if [[ $x == a && $x != b ]]; then echo y; fi'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk dp.g1
# G3 dispatch is statement-only: using it where a boolean is expected → reject
run_sash '[[ x == a { echo hit; } ]] || echo outer'
[[ $rc != 0 && -n $err ]]; chk dp.g3a
run_sash 'if [[ x == a { echo hit; } ]]; then echo t; fi'
[[ $rc != 0 && -n $err ]]; chk dp.g3b
run_sash '! [[ x == a { echo hit; } ]]'
[[ $rc != 0 && -n $err ]]; chk dp.g3c
# G4 boolean form unaffected (regression)
run_sash 'x=a; [[ $x == a ]] || echo o'
[[ $rc = 0 && -z $out && -z $err ]]; chk dp.g4a
run_sash 'x=b; if [[ $x == a ]]; then echo t; else echo f; fi'
[[ $rc = 0 && $out = "f" && -z $err ]]; chk dp.g4b

set -e
tally
