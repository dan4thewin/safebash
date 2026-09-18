#!/bin/bash
# spec/operand_words.sh — DD#71 operand words. The `=~` RHS is an ordinary
# operand word: regex metacharacters are literal, quotes/`$` are honoured,
# and the word runs to the next unquoted whitespace / `]]`. Bare regexes
# work, quoted spaces work, an unquoted space is NOT a silent concat, and an
# interpolated (dynamic) pattern is built at runtime and handed to regcomp.
set -eu
. common.sh
setup operand_words
plan=23
echo "1..$plan" >&3

set +e

# ── bare metacharacters (no quoting needed) ──────────────────────────
# ow.1 bare alternation group
run_sash 'x=b; if [[ $x =~ ^(a|b|c)$ ]] { echo Y } else { echo N }'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk ow.1
# ow.2 bare `.` is a real regex wildcard
run_sash 'x=aXbYc; if [[ $x =~ a.b.c ]] { echo Y } else { echo N }'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk ow.2
# ow.3 bare counted repetition `{n,m}` (the `{` is a regex char here, not a
#      dispatch arm — no whitespace before it)
run_sash 'x=aaa; if [[ $x =~ ^a{2,3}$ ]] { echo Y } else { echo N }'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk ow.3

# ── quoting (a literal space in the pattern) ─────────────────────────
# ow.4 single-quoted space matches a space
run_sash "x='a b'; if [[ \$x =~ 'a b' ]] { echo Y } else { echo N }"
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk ow.4
# ow.5 the quoted pattern is a real space — it does NOT match the space-free
#      string (regression gate for the old silent space-drop `a b`->`ab`)
run_sash "x='ab'; if [[ \$x =~ 'a b' ]] { echo Y } else { echo N }"
[[ $rc = 0 && $out = "N" && -z $err ]]; chk ow.5
# ow.6 double-quoted pattern preserves the space too
run_sash 'x="a b"; if [[ $x =~ "a b" ]] { echo Y } else { echo N }'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk ow.6

# ── an unquoted space is a word boundary, NOT a silent concat ─────────
# ow.7 `=~ foo bar` — `bar` is a dangling second word → clean compile error
#      (the old collector silently produced the regex `foobar`)
run_sash 'x=foobar; [[ $x =~ foo bar ]]'
[[ $rc != 0 && -z $out && -n $err ]]; chk ow.7

# ── dynamic (interpolated) patterns are supported ────────────────────
# ow.8 a `$var` pattern is built at runtime and handed to regcomp
run_sash 'pat=^a; x=abc; if [[ $x =~ $pat ]] { echo Y } else { echo N }'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk ow.8
# ow.9 dynamic non-match
run_sash 'pat=^z; x=abc; if [[ $x =~ $pat ]] { echo Y } else { echo N }'
[[ $rc = 0 && $out = "N" && -z $err ]]; chk ow.9
# ow.10 an interpolated group pattern
run_sash 'pat="^(cat|dog)$"; x=dog; if [[ $x =~ $pat ]] { echo Y } else { echo N }'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk ow.10
# ow.11 a spliced literal+variable operand (`run$suf`) concatenates in the
# `==` RHS: run+ing = runing (NOT running — a mis-stated bug repro; matches
# bash). See bug-test-operand-concat-splice (not-a-bug).
run_sash 'suf=ing; x=runing; if [[ $x == run$suf ]] { echo Y } else { echo N }'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk ow.11
# ow.12 the same splice in the `=~` RHS
run_sash 'suf=ing; x=runing; if [[ $x =~ run$suf ]] { echo Y } else { echo N }'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk ow.12
# ow.13 spliced operand negative
run_sash 'suf=xyz; x=running; if [[ $x == run$suf ]] { echo Y } else { echo N }'
[[ $rc = 0 && $out = "N" && -z $err ]]; chk ow.13

# ── command connectors: structural in an operand, literal in a value ──
# DD#71 scopes absorb-mode to "value positions that are not themselves a
# connectable command". The discriminator is whether the thing to the left
# can legally take a command connector. A `[[ ]]` operand cannot be
# connected, so `&& || &` stay STRUCTURAL there and terminate the word; an
# assignment is a value binding, so they are absorbed as LITERAL text.
#
# These only bite when the connector is ADJACENT (no space): parse_word_parts
# stops at the first non-adjacent token, so a spaced `&&` never reaches the
# absorb decision at all. Every pre-existing spec wrote `&&` with spaces,
# which is why this whole distinction was unexercised.

# ow.14 an adjacent `&&` in the `=~` RHS terminates the pattern, so this is
# `[[ $x =~ a ]] && [[ b ]]` -- not the pattern `a&&b`. Matches bash.
run_sash 'x=q; [[ $x =~ a&&b ]] || echo N'
[[ $rc = 0 && $out = "N" && -z $err ]]; chk ow.14
# ow.15 the same shape when the first half matches
run_sash 'x=a; [[ $x =~ a&&a ]] && echo Y'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk ow.15
# ow.16 `||` is structural too, so the true right operand carries it
run_sash 'x=q; [[ $x =~ a||b ]] && echo Y'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk ow.16
# ow.17 a lone `&` is neither absorbed nor a valid connector here
run_sash 'x=q; [[ $x =~ a&b ]] && echo Y'
[[ $rc != 0 && -z $out && $err == *SB4014* ]]; chk ow.17
# ow.18 a LEADING `!` is absorbed into the operand. `!` is only its own token
# in leading position -- `scan_word` swallows a mid-word one -- so this is the
# only spelling that reaches the absorb decision for it.
run_sash 'x=!f; [[ $x =~ !f ]] && echo Y'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk ow.18

# ow.19-23 assignment values absorb the connectors as literal text. This is a
# deliberate divergence from bash, which reads `a=foo&&bar` as `a=foo` then
# the command `bar`; DD#71 licenses it on the grounds that an assignment has
# no exit status and cannot head a pipe, a background job or a `&&` chain.
run_sash 'a=foo|cat; echo $a'
[[ $rc = 0 && $out = "foo|cat" && -z $err ]]; chk ow.19
run_sash 'a=foo&; echo $a'
[[ $rc = 0 && $out = "foo&" && -z $err ]]; chk ow.20
run_sash 'a=foo&&bar; echo $a'
[[ $rc = 0 && $out = "foo&&bar" && -z $err ]]; chk ow.21
# ow.22 re-assignment, not just the declaring one
run_sash 'a=x; a=foo&&bar; echo $a'
[[ $rc = 0 && $out = "foo&&bar" && -z $err ]]; chk ow.22
# ow.23 a leading `!` in a value (this one agrees with bash)
run_sash 'a=!foo; echo $a'
[[ $rc = 0 && $out = "!foo" && -z $err ]]; chk ow.23

set -e
tally
