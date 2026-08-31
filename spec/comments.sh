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
plan=11
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
# Gated native,c: janet doesn't inline a user-function `$(g)` capture (spawns it
# as an external — a pre-existing backend gap, unrelated to comments).
if in_modes native,c; then
    run_sash 'fn g { /bin/echo hi }
v=$(g # inline comment
)
/bin/echo $v'
    [[ $rc = 0 && $out = "hi" && -z $err ]]; chk cmt.11
else
    skip_chk cmt.11
fi

set -e
tally
