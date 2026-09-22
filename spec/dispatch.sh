#!/bin/bash
# spec/dispatch.sh — DD#66 [[ ]] dispatch (cond) form. Multi-way dispatch
# statement alongside the boolean [[ ]] test. `==` exact, `=~` unanchored
# regex; `PATTERN { group }` arms; optional `|| { group }` default;
# first-match-wins; statement-only.
set -eu
. common.sh
setup dispatch
plan=70
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

# ── E. literal `*` — dispatch has no wildcards; `||` is the only default ──
# (bug-dispatch-literal-star-wildcard: `*` is an ordinary exact pattern)
run_sash 'x=hi; [[ $x == * { echo star; } || { echo default; } ]]'
[[ $rc = 0 && $out = "default" && -z $err ]]; chk dp.e1
run_sash "x='*'; [[ \$x == * { echo star; } || { echo default; } ]]"
[[ $rc = 0 && $out = "star" && -z $err ]]; chk dp.e2
# E3 no default, no match (literal `*` != `hi`) → no-op
run_sash 'x=hi; [[ $x == * { echo star; } ]]; echo done'
[[ $rc = 0 && $out = "done" && -z $err ]]; chk dp.e3
# E4 `||` default still fires when no arm matches (regression: default arm
#    no longer relies on a `*` sentinel)
run_sash 'x=zzz; [[ $x == a { echo A; } b { echo B; } || { echo def; } ]]'
[[ $rc = 0 && $out = "def" && -z $err ]]; chk dp.e4

# ── F. multi-exact arms (DD#71 — `== a b c` is a b OR c, first-match) ──
run_sash 'x=b; [[ $x == a b c { echo hit; } || { echo no; } ]]'
[[ $rc = 0 && $out = "hit" && -z $err ]]; chk dp.f1
run_sash 'x=z; [[ $x == a b c { echo hit; } || { echo no; } ]]'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk dp.f2
# F3 a multi-pattern arm after a single-pattern arm; middle group matches
run_sash 'x=three; [[ $x == one { echo 1; } two three { echo 23; } || { echo d; } ]]'
[[ $rc = 0 && $out = "23" && -z $err ]]; chk dp.f3
# F4 two multi-pattern arms, neither matches → default
run_sash 'x=q; [[ $x == a b { echo ab; } c d { echo cd; } || { echo def; } ]]'
[[ $rc = 0 && $out = "def" && -z $err ]]; chk dp.f4
# F5 a quoted (spaces) pattern within a multi-pattern arm
run_sash 'x="a b"; [[ $x == "a b" c { echo hit; } || { echo no; } ]]'
[[ $rc = 0 && $out = "hit" && -z $err ]]; chk dp.f5
# F6 `=~` arms stay single-regex: a bare second word is NOT a second pattern
#    (no explicit `{`), so it is a parse error
run_sash 'x=a; [[ $x =~ a b { echo hit; } ]]'
[[ $rc != 0 && -z $out && -n $err ]]; chk dp.f6

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

# ── H. whitespace-optional [[ ]] — symmetric with (( )) ──────────────
# `[[` / `]]` tokenize even when glued to adjacent words or their contents
# (a single `[`/`]` stays a subscript char). See DD#59.
# H1 boolean test, compact interior
run_sash 'if [[a==a]]; then echo y; fi'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk dp.ws1
# H2 no space after `if` before `[[`
run_sash 'x=a; if[[ $x == a ]]; then echo y; fi'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk dp.ws2
# H3 fully compact `if[[$x==a]]`
run_sash 'x=a; if[[$x==a]]; then echo y; fi'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk dp.ws3
# H4 `while[[` adjacency (loop body skipped)
run_sash 'x=b; while[[ $x == a ]]; do echo loop; done; echo done'
[[ $rc = 0 && $out = "done" && -z $err ]]; chk dp.ws4
# H5 negation `![[` adjacency
run_sash 'x=b; if ![[ $x == a ]]; then echo neg; fi'
[[ $rc = 0 && $out = "neg" && -z $err ]]; chk dp.ws5
# H6 dispatch form, `[[`/`]]` glued to subject/close
run_sash 'x=a; [[$x == a { echo hit } || { echo no }]]'
[[ $rc = 0 && $out = "hit" && -z $err ]]; chk dp.ws6
# H7 DD#74 `return[[ … ]]` predicate, compact
run_sash 'fn f{return[[a==a]]}; if f; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk dp.ws7
# H8 regression: a single `[` is still a subscript char
run_sash 'array a=(5 6 7); a[0]=9; echo ${a[0]}'
[[ $rc = 0 && $out = "9" && -z $err ]]; chk dp.ws8
# H9 regression: `(( ))` still whitespace-optional
run_sash 'if((1==1)); then echo p; fi'
[[ $rc = 0 && $out = "p" && -z $err ]]; chk dp.ws9

# I1–I8: dispatch compact operator parity (symmetric with boolean test)
# I1 dispatch form with compact `==`
run_sash '[[ a==a { echo yes } || { echo no } ]]'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk dp.co1
# I2 dispatch form with compact `==`, non-matching
run_sash '[[ a==b { echo no } || { echo yes } ]]'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk dp.co2
# I3 dispatch form with compact `=~`
run_sash '[[ a=~a { echo yes } || { echo no } ]]'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk dp.co3
# I4 dispatch form with compact operator on variable
run_sash 'x=a; [[ $x==a { echo yes } || { echo no } ]]'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk dp.co4
# I5 dispatch form with compact operator, multiple patterns
run_sash '[[ a==a b c { echo a } || { echo no } ]]'
[[ $rc = 0 && $out = "a" && -z $err ]]; chk dp.co5
# I6 dispatch compact as a statement inside an if body (dispatch is a
# statement, not a boolean condition — SB4028 guards the condition slot)
run_sash 'x=b; if [[ -n $x ]]; then [[ $x==a { echo yes } || { echo no } ]]; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk dp.co6
# I7 dispatch compact `=~` with variable
run_sash 'x=abc; [[ $x=~b { echo yes } || { echo no } ]]'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk dp.co7
# I8 nested dispatch (compact) inside loop
run_sash 'for x in a b c; do [[ $x==b { echo "$x"; break } || { :; } ]]; done'
[[ $rc = 0 && $out = "b" && -z $err ]]; chk dp.co8

# ── DD#94: line breaks and comments inside `[[ ]]` ──
# Package A (DG 2026-09-07): newlines and comments are trivia inside the
# brackets, so a multi-arm dispatch can be laid out one arm per line instead of
# one long line. Arm BODIES are ordinary statement lists, where newlines still
# separate statements.
# 94.20 arms on separate lines
run_sash 'x=bar
[[ $x == foo { echo a; }
   bar { echo b; }
   || { echo c; } ]]'
[[ $rc = 0 && $out = "b" && -z $err ]]; chk 94.20
# 94.21 comments between arms
run_sash 'x=zzz
[[ $x == foo { echo a; }   # the foo case
   # anything else falls through to the default
   || { echo c; } ]]'
[[ $rc = 0 && $out = "c" && -z $err ]]; chk 94.21
# 94.22 a newline may also sit between the pattern and its `{`
run_sash 'x=foo
[[ $x == foo
   { echo a; } ]]'
[[ $rc = 0 && $out = "a" && -z $err ]]; chk 94.22
# 94.23 …and after the `==`, and before the closing `]]`
run_sash 'x=foo
[[ $x ==
   foo { echo a; }
]]'
[[ $rc = 0 && $out = "a" && -z $err ]]; chk 94.23
# 94.24 a newline ENDS one arm's pattern list, so the next word starts a new
# arm rather than being another pattern of the previous one. That is what keeps
# a multi-line arm list unambiguous.
run_sash 'x=bar
[[ $x == foo { echo a; }
   bar baz { echo b; } ]]'
[[ $rc = 0 && $out = "b" && -z $err ]]; chk 94.24
# 94.25 arm bodies still treat newlines as statement separators
run_sash 'x=foo
[[ $x == foo {
     echo one
     echo two
   } ]]'
[[ $rc = 0 && $out = "one
two" && -z $err ]]; chk 94.25
# 94.26 the `[[ ]]` TEST form spans lines too (`&&` at end of line)
run_sash 'x=a; y=b
if [[ -n $x &&
      -n $y ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 94.26
# 94.27 …and with the operator leading the continued line
run_sash 'x=a; y=
if [[ -n $x
      && -n $y ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk 94.27
# 94.28 a comment inside a spanned test
run_sash 'x=a; y=b
if [[ -n $x &&   # host set
      -n $y ]]; then echo yes; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 94.28

# ── MP. compact operator with a MULTI-PART subject or pattern ────────
# `$x$y==ab` and `ab==$x$y` reach the rejoin path in
# `parse/parser.rs::build_expr_from_parts` (and the non-literal /
# not-an-operator middle-part skips in `extract_from_concat`). Every case
# below was verified to agree with bash's non-compact spelling
# (`[[ "$x$y" == ab ]]`). All were uncovered before 2026-09-21.

# MP1-MP3 multi-part SUBJECT (skips a non-Literal middle part first)
run_sash 'x=a; y=b; [[ $x$y==ab { echo hit } || { echo no } ]]'
[[ $rc = 0 && $out = "hit" && -z $err ]]; chk dp.mp1
run_sash 'x=a; y=z; [[ $x$y==ab { echo hit } || { echo no } ]]'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk dp.mp2
run_sash 'x=b; [[ a$x==ab { echo hit } || { echo no } ]]'
[[ $rc = 0 && $out = "hit" && -z $err ]]; chk dp.mp3

# MP4-MP6 multi-part PATTERN (the Concat rejoin itself)
run_sash 'x=a; y=b; [[ ab==$x$y { echo hit } || { echo no } ]]'
[[ $rc = 0 && $out = "hit" && -z $err ]]; chk dp.mp4
run_sash 'x=a; y=z; [[ ab==$x$y { echo hit } || { echo no } ]]'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk dp.mp5
run_sash 'x=b; [[ ab==a$x { echo hit } || { echo no } ]]'
[[ $rc = 0 && $out = "hit" && -z $err ]]; chk dp.mp6

# MP7 both sides multi-part
run_sash 'a=x; b=y; c=x; d=y; [[ $a$b==$c$d { echo hit } || { echo no } ]]'
[[ $rc = 0 && $out = "hit" && -z $err ]]; chk dp.mp7

# MP8-MP9 a middle Literal that is NOT an operator is skipped, and the real
# operator further right is still found
run_sash 'x=a; y=b; [[ $x:$y==a:b { echo hit } || { echo no } ]]'
[[ $rc = 0 && $out = "hit" && -z $err ]]; chk dp.mp8
run_sash 'x=a; y=b; [[ $x-$y==a-b { echo hit } || { echo no } ]]'
[[ $rc = 0 && $out = "hit" && -z $err ]]; chk dp.mp9

# MP10 regex side, multi-part pattern
run_sash 'x=a; y=b; [[ ab=~$x$y { echo hit } || { echo no } ]]'
[[ $rc = 0 && $out = "hit" && -z $err ]]; chk dp.mp10

# MP11-MP12 boolean [[ ]] parity for both sides
run_sash 'x=a; y=b; if [[ $x$y==ab ]]; then echo y; else echo n; fi'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk dp.mp11
run_sash 'x=a; y=b; if [[ ab==$x$y ]]; then echo y; else echo n; fi'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk dp.mp12

set -e
tally
