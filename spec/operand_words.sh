#!/bin/bash
# spec/operand_words.sh — DD#71 operand words. The `=~` RHS is an ordinary
# operand word: regex metacharacters are literal, quotes/`$` are honoured,
# and the word runs to the next unquoted whitespace / `]]`. Bare regexes
# work, quoted spaces work, an unquoted space is NOT a silent concat, and an
# interpolated (dynamic) pattern is built at runtime and handed to regcomp.
set -eu
. common.sh
setup operand_words
plan=56
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
run_sash 'x=q; y=b; [[ $x =~ a&&-n $y ]] || echo N'
[[ $rc = 0 && $out = "N" && -z $err ]]; chk ow.14
# ow.15 the same shape when the first half matches
run_sash 'x=a; y=a; [[ $x =~ a&&-n $y ]] && echo Y'
[[ $rc = 0 && $out = "Y" && -z $err ]]; chk ow.15
# ow.16 `||` is structural too, so the true right operand carries it
run_sash 'x=q; y=b; [[ $x =~ a||-n $y ]] && echo Y'
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

# ---- DD#118: `==` is the only equality, `-n` is explicit, no vacuous tests ----

# 118.1 a single `=` in a test operator slot is rejected (it always assigns)
run_sash 'str x=a
[[ $x = a ]] && echo y'
[[ $rc != 0 && -z $out && $err == *SB4067* ]]; chk 118.1

# 118.2 a bare `!` after an operand directs to `!=`
run_sash 'str x=a
[[ $x ! a ]] && echo y'
[[ $rc != 0 && -z $out && $err == *SB4067* ]]; chk 118.2

# 118.3 `==` is accepted (the only spelling)
run_sash 'str x=a
[[ $x == a ]] && echo y'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 118.3

# 118.4 DD#86 frictionless write is untouched — `=` still assigns in statement
#       position, which is precisely why it cannot also compare
run_sash 'msg = hi
echo $msg'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk 118.4

# 118.5 a bare word is no longer an implicit `-n`
run_sash 'str x=a
[[ $x ]] && echo y'
[[ $rc != 0 && -z $out && $err == *SB4068* ]]; chk 118.5

# 118.6 …nor is a negated one
run_sash 'str x=a
[[ ! $x ]] && echo y'
[[ $rc != 0 && -z $out && $err == *SB4068* ]]; chk 118.6

# 118.7 the explicit form is what to write
run_sash 'str x=a
[[ -n $x ]] && echo y'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 118.7

# 118.8 the glued shape that motivated all this: `[[ $x=y ]]` was silently and
#       unconditionally TRUE (one word, always containing "=y"). Now rejected.
run_sash 'str x=a
[[ $x=y ]] && echo y'
[[ $rc != 0 && -z $out && $err == *SB4068* ]]; chk 118.8

# 118.9 a word broken by some OTHER token is a missing `]]`, not a bare word —
#       `&` is the real fault, so SB4068 must not claim it
run_sash 'str x=a
[[ a&b == x ]] && echo y'
[[ $rc != 0 && -z $out && $err == *SB4014* ]]; chk 118.9

# 118.10 vacuous: `-n` on a literal can never be false
run_sash '[[ -n foo ]] && echo y'
[[ $rc != 0 && -z $out && $err == *SB5086* ]]; chk 118.10

# 118.11 vacuous: `-z` on an empty literal can never be true
run_sash '[[ -z "" ]] && echo y'
[[ $rc != 0 && -z $out && $err == *SB5086* ]]; chk 118.11

# 118.12 vacuous: two static operands compared
run_sash '[[ foo == foo ]] && echo y'
[[ $rc != 0 && -z $out && $err == *SB5086* ]]; chk 118.12

# 118.13 vacuous the other way (a constant FALSE is just as useless)
run_sash '[[ 1 == 2 ]] && echo y'
[[ $rc != 0 && -z $out && $err == *SB5086* ]]; chk 118.13

# 118.14 vacuous `=~` — a static subject against a static pattern
run_sash '[[ abc =~ b ]] && echo y'
[[ $rc != 0 && -z $out && $err == *SB5086* ]]; chk 118.14

# 118.15 `$(( ))` always renders at least one digit, so it is never empty
run_sash '[[ -n $((1+1)) ]] && echo y'
[[ $rc != 0 && -z $out && $err == *SB5086* ]]; chk 118.15

# 118.16 the rule is SYNTACTIC, not const-propagating: DG's escape rewrite has
#        to keep working, so one variable is enough to make a test honest
run_sash 'str s=foo
[[ -n $s ]] && echo y'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 118.16

# 118.17 …and the same for the comparison form
run_sash 'str s=foo
[[ $s == foo ]] && echo y'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 118.17

# 118.18 a concat is Unknown when any part is dynamic
run_sash 'str a=x
str b=y
[[ -n "$a$b" ]] && echo y'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 118.18

# 118.19 file tests are EXEMPT — a static operand, a runtime answer
run_sash '[[ -f /etc/passwd ]] && echo y'
[[ $rc = 0 && $out = "y" && -z $err ]]; chk 118.19

# 118.20 numeric operators are exempt too, so SB5086 cannot shadow SB5019's
#        more specific diagnosis of the very same source
run_sash '[[ 1 -eq 2 ]] && echo y'
[[ $rc != 0 && -z $out && $err == *SB5019* ]]; chk 118.20

# 118.21 DD#66 dispatch is a separate node and needs its own vacuity check:
#        a literal subject against literal patterns picks the arm at compile time
run_sash '[[ a == a { echo yes } || { echo no } ]]'
[[ $rc != 0 && -z $out && $err == *SB5086* ]]; chk 118.21

# 118.22 a dispatch with a real subject is fine (the `|| { }` arm carries no
#        patterns, which must not by itself count as static)
run_sash 'str x=a
[[ $x == a { echo yes } || { echo no } ]]'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk 118.22

# 118.23 SB5086 does not mask a later diagnostic on the same test
run_sash '[[ abc =~ ( ]] && echo y'
[[ $rc != 0 && -z $out && $err == *SB5086* && $err == *SB5023* ]]; chk 118.23

# ── DD#118 Part D: the glued operator needs the `$` sigil on the left ──
# `[[ ab==$x ]]` parses as a comparison in sash but as a bare word (implicit
# -n, always true) in bash. Part B made the bash reading an error, which left
# the glued spelling answering differently from bash with nothing said. Part D
# reserves the glued form for the `$`-initial template, so every other shape
# is rejected rather than quietly reinterpreted.

# 118.24 the template itself: a `$`-initial glued `==`
run_sash 'str x=a
if [[ $x==a ]]; then echo T; else echo F; fi'
[[ $rc = 0 && $out = "T" && -z $err ]]; chk 118.24

# 118.25 a literal-initial subject is SB4069, and because `==` commutes the
#        diagnostic offers the swap as well as the spacing
run_sash 'str x=ab
[[ ab==$x { echo hit } || { echo no } ]]'
[[ $rc != 0 && -z $out && $err == *SB4069* && $err == *swap* ]]; chk 118.25

# 118.26 `=~` does NOT commute — its subject must stay on the left — so the
#        same shape is rejected without the swap suggestion
run_sash 'str x=ab
[[ ab=~$x { echo hit } || { echo no } ]]'
[[ $rc != 0 && -z $out && $err == *SB4069* && $err == *"cannot be swapped"* ]]; chk 118.26

# 118.27 a MULTI-part literal-initial subject is rejected too: the guard is on
#        the first part, not on whether the word is wholly static
run_sash 'str x=b
[[ a$x==ab { echo hit } || { echo no } ]]'
[[ $rc != 0 && -z $out && $err == *SB4069* ]]; chk 118.27

# 118.28 the wholly-glued literal form: SB4069 now precedes the vacuity check,
#        since parsing runs before validation
run_sash 'if [[a==a]]; then echo T; else echo F; fi'
[[ $rc != 0 && -z $out && $err == *SB4069* ]]; chk 118.28

# 118.29 the spaced spelling is untouched — that is the fix the hint names
run_sash 'str x=ab
if [[ ab == $x ]]; then echo T; else echo F; fi'
[[ $rc = 0 && $out = "T" && -z $err ]]; chk 118.29

# 118.30 a braced/subscripted expansion carries the sigil just as `$x` does
run_sash 'map m=([k]=a)
str k=k
if [[ ${m[$k]}==a ]]; then echo T; else echo F; fi'
[[ $rc = 0 && $out = "T" && -z $err ]]; chk 118.30

# ── a missing test operand is its own SB4014 (parse_test_operand) ──
# Distinct from the "expected ']]'" SB4014 that DD#118's `_` arm emits: this
# one fires where an operand was REQUIRED and the token stream ran out.

# ow.24 a unary operator with nothing after it
run_sash 'if [[ -n ]]; then echo T; fi'
[[ $rc != 0 && -z $out && $err == *SB4014* && $err == *"expected operand"* ]]; chk ow.24

# ow.25 an empty test
run_sash 'if [[ ]]; then echo T; fi'
[[ $rc != 0 && -z $out && $err == *SB4014* && $err == *"expected operand"* ]]; chk ow.25

# ow.26 a binary operator with no right operand
run_sash 'str x=a
if [[ $x == ]]; then echo T; fi'
[[ $rc != 0 && -z $out && $err == *SB4014* && $err == *"expected operand"* ]]; chk ow.26

set -e
tally
