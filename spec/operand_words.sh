#!/bin/bash
# spec/operand_words.sh — DD#71 operand words. The `=~` RHS is an ordinary
# operand word: regex metacharacters are literal, quotes/`$` are honoured,
# and the word runs to the next unquoted whitespace / `]]`. Bare regexes
# work, quoted spaces work, an unquoted space is NOT a silent concat, and an
# interpolated (dynamic) pattern is built at runtime and handed to regcomp.
set -eu
. common.sh
setup operand_words
plan=10
echo "1..$plan" >&3

set +e

# ── bare metacharacters (no quoting needed) ──────────────────────────
# ow.1 bare alternation group
run_sash 'x=b; [[ $x =~ ^(a|b|c)$ ]] && echo Y || echo N'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk ow.1
# ow.2 bare `.` is a real regex wildcard
run_sash 'x=aXbYc; [[ $x =~ a.b.c ]] && echo Y || echo N'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk ow.2
# ow.3 bare counted repetition `{n,m}` (the `{` is a regex char here, not a
#      dispatch arm — no whitespace before it)
run_sash 'x=aaa; [[ $x =~ ^a{2,3}$ ]] && echo Y || echo N'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk ow.3

# ── quoting (a literal space in the pattern) ─────────────────────────
# ow.4 single-quoted space matches a space
run_sash "x='a b'; [[ \$x =~ 'a b' ]] && echo Y || echo N"
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk ow.4
# ow.5 the quoted pattern is a real space — it does NOT match the space-free
#      string (regression gate for the old silent space-drop `a b`->`ab`)
run_sash "x='ab'; [[ \$x =~ 'a b' ]] && echo Y || echo N"
[[ $rc = 0 && $out = "N" && -z $err ]]; chk ow.5
# ow.6 double-quoted pattern preserves the space too
run_sash 'x="a b"; [[ $x =~ "a b" ]] && echo Y || echo N'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk ow.6

# ── an unquoted space is a word boundary, NOT a silent concat ─────────
# ow.7 `=~ foo bar` — `bar` is a dangling second word → clean compile error
#      (the old collector silently produced the regex `foobar`)
run_sash 'x=foobar; [[ $x =~ foo bar ]]'
[[ $rc != 0 && -z $out && -n $err ]]; chk ow.7

# ── dynamic (interpolated) patterns are supported ────────────────────
# ow.8 a `$var` pattern is built at runtime and handed to regcomp
run_sash 'pat=^a; x=abc; [[ $x =~ $pat ]] && echo Y || echo N'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk ow.8
# ow.9 dynamic non-match
run_sash 'pat=^z; x=abc; [[ $x =~ $pat ]] && echo Y || echo N'
[[ $rc = 0 && $out = "N" && -z $err ]]; chk ow.9
# ow.10 an interpolated group pattern
run_sash 'pat="^(cat|dog)$"; x=dog; [[ $x =~ $pat ]] && echo Y || echo N'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk ow.10
# ow.11 a spliced literal+variable operand (`run$suf`) concatenates in the
# `==` RHS: run+ing = runing (NOT running — a mis-stated bug repro; matches
# bash). See bug-test-operand-concat-splice (not-a-bug).
run_sash 'suf=ing; x=runing; [[ $x == run$suf ]] && echo Y || echo N'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk ow.11
# ow.12 the same splice in the `=~` RHS
run_sash 'suf=ing; x=runing; [[ $x =~ run$suf ]] && echo Y || echo N'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk ow.12
# ow.13 spliced operand negative
run_sash 'suf=xyz; x=running; [[ $x == run$suf ]] && echo Y || echo N'
[[ $rc = 0 && $out = "N" && -z $err ]]; chk ow.13

set -e
tally
