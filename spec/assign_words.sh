#!/bin/bash
# spec/assign_words.sh — DD#71 assignment value words. An assignment is a
# value binding, not a connectable command, so the command connectors
# `& && || |` (and operand puncts) are absorbed as literal characters: the
# value runs to the next unquoted whitespace / `;` / newline. A literal space
# is quoted; array literals `=(…)` are unaffected.
set -eu
. common.sh
setup assign_words
plan=28
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

# ── aw.21-aw.25 SB5011 names the cause the line actually has ──
#
# A malformed assignment (`a-b=1`) and a runtime-chosen command (`$x foo`)
# both reach `check_cmd_in_stmt` as a non-`Literal` head, and both used to
# get the dynamic-head message — telling the author of a line containing no
# variable and no expansion that "a variable or expansion can't be resolved
# at compile time". aw.11-aw.15 and aw.20 above pin the CODE; these pin that
# the two arrivals are now told apart.

# aw.21 a malformed target is named, and the message does not claim there is
#       an expansion in a line that has none
run_sash 'a-b=1'
[[ $rc != 0 && -z $out && $err == *'SB5011'* && $err == *'a-b'* \
   && $err == *'not a valid variable name'* && $err != *'expansion'* ]]; chk aw.21

# aw.22 the leading-digit target likewise
run_sash '9x=1
echo "${9x}"'
[[ $rc != 0 && -z $out && $err == *'9x'* && $err == *'not a valid variable name'* ]]; chk aw.22

# aw.23 and the bracket-soup target
run_sash 'a]b[=1'
[[ $rc != 0 && -z $out && $err == *'a]b['* && $err == *'not a valid variable name'* ]]; chk aw.23

# aw.24 the genuine DD#23 dynamic head KEEPS the original wording — the fix
#       distinguishes the arrivals rather than widening the message
run_sash 'x=/bin/echo
$x foo'
[[ $rc != 0 && -z $out && $err == *'SB5011'* && $err == *'literal path'* \
   && $err == *'expansion'* ]]; chk aw.24

# aw.25 an expansion LEFT of the `=` is a word that really does contain one,
#       so it keeps the dynamic-head wording too
run_sash 'array a=()
9x[$i]=1'
[[ $rc != 0 && -z $out && $err == *'SB5011'* && $err == *'literal path'* ]]; chk aw.25

# aw.26 an `=` in an ARGUMENT is not an assignment attempt and still runs
run_sash 'echo a=b'
[[ $rc = 0 && $out = "a=b" && -z $err ]]; chk aw.26

# aw.27-aw.28 an UNCLOSED subscript. `is_identifier_or_subscript` splits on the
#   first `[` and then requires the word to end in `]`; the "does not end in
#   `]`" arm was reachable but had no test, so nothing would have caught its
#   removal. `a]b[=1` (aw.20) reaches the same arm from the other direction.
run_sash 'a[1=2'
[[ $rc != 0 && -z $out && $err == *'a[1'* && $err == *'not a valid variable name'* ]]; chk aw.27

# aw.28 ...and trailing junk after a well-formed subscript is not a target
run_sash 'a[1]x=2'
[[ $rc != 0 && -z $out && $err == *'a[1]x'* && $err == *'not a valid variable name'* ]]; chk aw.28

set -e
tally
