#!/bin/bash
# spec/comments.sh — comments are lexical trivia (DG 2026-08-31). The Scanner
# drops `# … EOL` tokens entirely, so a comment never reaches the parser. This
# fixes trailing comments, which used to be SB4014 "expected expression" at
# every value-collecting loop (command args, declaration values, for-words, …)
# because the parser only recognized `#` at statement start. The source is
# embedded verbatim in `.sash_source` (cli.3), so `--extract` round-trip is
# unaffected. All modes (front-end lexing is backend-independent).
set -eu
. common.sh
setup comments
set +e
plan=26
echo "1..$plan" >&3

# cmt.1 trailing comment after a command
run_sash '/bin/echo hi # a trailing comment'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk cmt.1

# cmt.2 trailing comment after a pipeline
run_sash '/bin/echo hi | /bin/cat # comment'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk cmt.2

# cmt.3 trailing comment after a typed declaration
run_sash 'const foo=hello # a greeting
/bin/echo $foo'
[[ $rc = 0 && $out = "hello" && -z $err ]]; chk cmt.3

# cmt.4 trailing comment after an array declaration
run_sash 'array a=(one two three) # three words
/bin/echo ${a[1]}'
[[ $rc = 0 && $out = "two" && -z $err ]]; chk cmt.4

# cmt.5 trailing comment after a for-in header (a structural position)
run_sash 'for x in a b c # loop over three
{ /bin/echo $x }'
[[ $rc = 0 && $out = $'a\nb\nc' && -z $err ]]; chk cmt.5

# cmt.6 trailing comment after an `if` condition header
run_sash 'if /bin/true # always
{ /bin/echo yes }'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk cmt.6

# cmt.7 a full-line comment (the pre-existing case) still compiles + runs
run_sash '# a standalone comment
/bin/echo hi'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk cmt.7

# cmt.8 a shebang line is a comment — dropped, program runs normally
run_sash '#!/usr/bin/env sash
/bin/echo hi'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk cmt.8

# cmt.9 a comment runs to END OF LINE — a `;` inside it is NOT a separator,
# so the "second statement" hidden in the comment never executes
run_sash '/bin/echo one # ; /bin/echo two'
[[ $rc = 0 && $out = "one" && -z $err ]]; chk cmt.9

# cmt.10 a comment between two statements on their own line
run_sash '/bin/echo a
# middle comment
/bin/echo b'
[[ $rc = 0 && $out = $'a\nb' && -z $err ]]; chk cmt.10

# cmt.11 an inline comment inside a `$( )` body is dropped; the command runs.
run_sash 'fn g { /bin/echo hi }
v=$(g # inline comment
)
/bin/echo $v'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk cmt.11

# ---------------------------------------------------------------------------
# DD#110 — a `#` opens a comment only after a blank (or at start of input).
# Before this rule the scanner broke every unquoted word at an interior `#`
# and ate the rest of the line, so `echo a#b` printed `a`. Silently.
# ---------------------------------------------------------------------------

# cmt.12 an interior `#` is an ordinary word character
run_sash '/bin/echo a#b'
[[ $rc = 0 && $out = "a#b" && -z $err ]]; chk cmt.12

# cmt.13 the shape that named the bug: a bash base-prefix literal as a word.
# DD#72 rejects `16#ff` inside `$(( ))`; outside it, it is just text.
run_sash '/bin/echo 16#ff'
[[ $rc = 0 && $out = "16#ff" && -z $err ]]; chk cmt.13

# cmt.14 the truncation used to swallow every following word too
run_sash '/bin/echo ab#cd ef'
[[ $rc = 0 && $out = "ab#cd ef" && -z $err ]]; chk cmt.14

# cmt.15 a `#` after a simple expansion continues the word
run_sash 's="p"
/bin/echo $s#y'
[[ $rc = 0 && $out = "p#y" && -z $err ]]; chk cmt.15

# cmt.16 … and after a braced one. `}` is deliberately not an operator for
# this rule precisely so that this keeps working.
run_sash 's="p"
/bin/echo ${s}#y'
[[ $rc = 0 && $out = "p#y" && -z $err ]]; chk cmt.16

# cmt.17 … and after arithmetic
run_sash '/bin/echo $((1+1))#z'
[[ $rc = 0 && $out = "2#z" && -z $err ]]; chk cmt.17

# cmt.18 … and after a command substitution, which closes on `)`
run_sash 'fn p { echo p }
/bin/echo $(p)#z'
[[ $rc = 0 && $out = "p#z" && -z $err ]]; chk cmt.18

# cmt.19 a `#` right after a blank is still a comment, mid-line
run_sash '/bin/echo a #b'
[[ $rc = 0 && $out = "a" && -z $err ]]; chk cmt.19

# cmt.20 a `#` in column 1 is still a comment even with no space after it
run_sash '#nospace
/bin/echo ok'
[[ $rc = 0 && $out = "ok" && -z $err ]]; chk cmt.20

# cmt.21 a `#` after a tab is still a comment
run_sash $'/bin/echo a\t# c' 
[[ $rc = 0 && $out = "a" && -z $err ]]; chk cmt.21

# cmt.22 SB4058: bash opens a comment after an operator; sash does not, and
# says so rather than leaving `#` to surface as "command '#' not found".
run_sash '/bin/echo a;# c'
[[ $rc != 0 && -z $out && $err == *'SB4058'* ]]; chk cmt.22

# cmt.23 the same for a pipe — the whole operator set is `; & | < > (`
run_sash '/bin/echo a|# c'
[[ $rc != 0 && -z $out && $err == *'SB4058'* ]]; chk cmt.23

# cmt.24 … and for a redirect
run_sash '/bin/echo a >#c'
[[ $rc != 0 && -z $out && $err == *'SB4058'* ]]; chk cmt.24

# cmt.25 the spelled-out form of cmt.22 compiles and comments
run_sash '/bin/echo a ; # c'
[[ $rc = 0 && $out = "a" && -z $err ]]; chk cmt.25

# cmt.26 DD#72 still rejects the base-prefix literal inside arithmetic — the
# arith guard runs before this rule, so its SB4035 is unchanged.
run_sash 'int n=0
(( n = 16#ff ))'
[[ $rc != 0 && $err == *'SB4035'* ]]; chk cmt.26

set -e
tally
