#!/bin/bash
# spec/assign_words.sh — DD#71 assignment value words. An assignment is a
# value binding, not a connectable command, so the command connectors
# `& && || |` (and operand puncts) are absorbed as literal characters: the
# value runs to the next unquoted whitespace / `;` / newline. A literal space
# is quoted; array literals `=(…)` are unaffected.
set -eu
. common.sh
setup assign_words
plan=20
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

# ── leading-digit identifiers are not assignment targets ─────────────
# `parser.rs::is_identifier` never checked the FIRST character, so `9x`, `0`,
# and `123` all passed as names and `9x=1` parsed as an assignment to a name
# bash will not accept (bash: "9x=1: command not found"). A word that is not a
# valid identifier falls through to command position and errors there, the
# same as any other unknown word.

# aw.11 a leading-digit name is not an assignment target
run_sash '9x=1
echo "${9x}"'
[[ $rc != 0 && -z $out && $err == *'SB5011'* ]]; chk aw.11

# aw.12 an all-digit name likewise
run_sash '0=1'
[[ $rc != 0 && -z $out && $err == *'SB5011'* ]]; chk aw.12

# aw.13 ...of any length
run_sash '123=5
echo hi'
[[ $rc != 0 && -z $out && $err == *'SB5011'* ]]; chk aw.13

# aw.14 the dynamic-subscript probe has the same gate
run_sash 'array a=()
9x[$i]=1'
[[ $rc != 0 && -z $out && $err == *'SB5011'* ]]; chk aw.14

# aw.15 `var =~ s/pat/rep/` needs a real identifier too. This one COMPILED
#       CLEANLY before the fix — a working binary for input bash rejects.
run_sash '9x =~ s/a/b/'
[[ $rc != 0 && -z $out && $err == *'SB5011'* ]]; chk aw.15

# aw.16 a digit anywhere but first is still fine
run_sash 'x9=1
echo "$x9"'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk aw.16

# aw.17 a leading underscore is an identifier
run_sash '_x=1
echo "$_x"'
[[ $rc = 0 && $out = "1" && -z $err ]]; chk aw.17

# aw.18 literal and dynamic subscript assignment both still parse
run_sash 'array a=(p q)
int i=1
a[1]=v
echo "${a[1]}"
a[$i]=w
echo "${a[1]}"'
[[ $rc = 0 && $out = "v
w" && -z $err ]]; chk aw.18

# aw.19 a real regex mutation is unaffected
run_sash 'x=aaa
x =~ s/a/b/
echo "$x"'
[[ $rc = 0 && $out = "baa" && -z $err ]]; chk aw.19

# aw.20 the subscript half is no longer a flat character-set test: `][` and
#       `a]b[` used to pass as assignment targets.
run_sash 'a]b[=1'
[[ $rc != 0 && -z $out && $err == *'SB5011'* ]]; chk aw.20

set -e
tally
