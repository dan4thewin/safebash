#!/bin/bash
# spec/assign_words.sh — DD#71 assignment value words. An assignment is a
# value binding, not a connectable command, so the command connectors
# `& && || |` (and operand puncts) are absorbed as literal characters: the
# value runs to the next unquoted whitespace / `;` / newline. A literal space
# is quoted; array literals `=(…)` are unaffected.
set -eu
. common.sh
setup assign_words
plan=10
echo "1..$plan" >&3

set +e

# aw.1 a glued pipe is a literal, not a pipeline
run_sash 'a=foo|cat; echo "[$a]"'
[[ $rc = 0 && $out = "[foo|cat]" && -z $err ]]; chk aw.1
# aw.2 glued && is a literal, not a boolean
run_sash 'a=foo&&bar; echo "[$a]"'
[[ $rc = 0 && $out = "[foo&&bar]" && -z $err ]]; chk aw.2
# aw.3 glued & is a literal, not a background fork
run_sash 'a=foo&; echo "[$a]"'
[[ $rc = 0 && $out = "[foo&]" && -z $err ]]; chk aw.3
# aw.4 glued || is a literal
run_sash 'a=x||y; echo "[$a]"'
[[ $rc = 0 && $out = "[x||y]" && -z $err ]]; chk aw.4
# aw.5 parens/braces are literal in a value
run_sash 'a=x(y){z}; echo "[$a]"'
[[ $rc = 0 && $out = "[x(y){z}]" && -z $err ]]; chk aw.5

# ── terminators still sequence / bound ───────────────────────────────
# aw.6 `;` ends the assignment (statement separator)
run_sash 'a=foo;echo "[$a]"'
[[ $rc = 0 && $out = "[foo]" && -z $err ]]; chk aw.6
# aw.7 whitespace bounds the value (a literal space is quoted)
run_sash 'a="one two"; echo "[$a]"'
[[ $rc = 0 && $out = "[one two]" && -z $err ]]; chk aw.7
# aw.8 an expansion in the value still interpolates
run_sash 'a=$(echo hi)|x; echo "[$a]"'
[[ $rc = 0 && $out = "[hi|x]" && -z $err ]]; chk aw.8

# ── array literal `=(…)` is unaffected ───────────────────────────────
# aw.9 the `(` right after `=` is still an array literal, not absorbed
run_sash 'array f=(one two); for x in @f; do echo $x; done'
[[ $rc = 0 && $out = $'one\ntwo' && -z $err ]]; chk aw.9

# ── interaction with the boolean-context reject ──────────────────────
# aw.10 a *spaced* `&&` after an assignment is an operator (assignment in a
#       boolean context) → SB4025, NOT absorbed
run_sash 'a=foo && echo x'
[[ $rc != 0 && -z $out && $err == *'SB4025'* ]]; chk aw.10

set -e
tally
