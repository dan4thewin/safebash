#!/bin/bash
# spec/glob.sh — DD#73: glob expansion yields normalized ABSOLUTE paths.
#
# A glob element is `<getcwd>/name`, not the bare directory-entry name —
# for referential stability across a chdir, and to close glob-sourced
# option-injection (a `-`-leading filename becomes `/abs/-name`, which no
# external command parses as an option). Lexical (no realpath); no relative
# segment survives.
#
# NOTE: `run_sash` (native) leaves `./test_bin` in the cwd during the run,
# so these tests use suffixed patterns (`*.www`, never bare `*`) to avoid
# matching the harness binary.
set -eu
. common.sh
setup glob
plan=17
echo "1..$plan" >&3

set +e

# g.1 a glob element is an absolute path `<cwd>/name`, not the bare name.
run_sash '/usr/bin/touch g1.www; for f in *.www; do echo "$f"; done'
cwd=$(pwd -P)
[[ $rc = 0 && $out = "$cwd/g1.www" && -z $err ]]; chk g.1

# g.2 option-injection closed: a `-`-leading filename becomes `<cwd>/-name`
#     (starts with `/`, so no external command reparses it as an option).
run_sash '/usr/bin/touch -- -rf.zz; for f in *.zz; do echo "$f"; done'
cwd=$(pwd -P)
[[ $rc = 0 && $out = "$cwd/-rf.zz" && $out = /* && -z $err ]]; chk g.2

# g.3 the array-literal form `array a=(*)` carries absolute elements too
#     (observed via for-in, which is disposition-exempt).
run_sash '/usr/bin/touch g3.rr; array a=(*.rr); for f in @a; do echo "$f"; done'
cwd=$(pwd -P)
[[ $rc = 0 && $out = "$cwd/g3.rr" && -z $err ]]; chk g.3

# g.4 referential stability: the absolute element still names the same file
#     after a chdir — the DD#73 correctness rationale. With a bare name,
#     `cat g4.tt` after `cd /` would fail.
run_sash 'echo hi > g4.tt; array a=(*.tt); cd /; /bin/cat @{a:?x}'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk g.4

# g.5 zero matches → empty (absolutization does not fabricate an entry).
run_sash 'array a=(*.nomatchQQ); /usr/bin/printf "[%s]" @{a:-none}'
[[ $rc = 0 && $out = "[none]" && -z $err ]]; chk g.5

# g.6 every element of a multi-match glob is absolute (order-independent).
run_sash '/usr/bin/touch g6a.pp g6b.pp; for f in *.pp; do echo "$f"; done'
cwd=$(pwd -P)
[[ $rc = 0 && $out == *"$cwd/g6a.pp"* && $out == *"$cwd/g6b.pp"* ]]; chk g.6

# g.7 a glob array's `len` agrees with its contents. The glob literal lowers
# to a `glob` value rather than an array literal, so it used to skip
# collection registration entirely — `join` saw every element while `len`
# read the table as a string and answered 0, with the `array` keyword present.
run_sash '/usr/bin/touch g7a.g7x g7b.g7x g7c.g7x; array a=(*.g7x); echo $(len a)'
[[ $rc = 0 && $out = "3" && -z $err ]]; chk g.7

# g.8 `len` and `join` agree about the same glob array
run_sash '/usr/bin/touch g8a.g8x g8b.g8x; array a=(*.g8x); echo $(len a); echo $(join a)'
[[ $rc = 0 && $out == "2"* && $out == *g8a.g8x* && $out == *g8b.g8x* && -z $err ]]; chk g.8

# g.9 a zero-match glob is a real EMPTY array — len 0, and still a collection
run_sash 'array a=(*.nomatchZZ); echo $(len a)'
[[ $rc = 0 && $out = "0" && -z $err ]]; chk g.9

# ── SB5075: the `arr=(GLOB =~ 'RE')` regex-filter shorthand is retired (DD#108)
# It was a list literal whose meaning depended on its own contents: `(* =~ re)`
# filtered, `(x =~ y)` was a plain 3-element array, and `(* =~ a b)` silently
# became a 4-element one. Quoting could not disambiguate it either — every
# quoting of the operator desugared identically — so making quotes the escape
# would have meant letting quotes change a list's arity. The loop replaces it
# and names what it matches.

# g.10 the shorthand is rejected rather than silently becoming a literal array
run_sash "array a=(* =~ '\\.ff\$')
echo \$(len a)"
[[ $rc != 0 && -z $out && $err == *SB5075* ]]; chk g.10

# g.11 …including the form that was the original bug: a non-bare glob used to
# fall through to a 3-element array of un-expanded literal text, no diagnostic
run_sash "array a=(*.ff =~ '\\.ff\$')
echo \$(len a)"
[[ $rc != 0 && -z $out && $err == *SB5075* ]]; chk g.11

# g.12 the rule is uniform — a bare `=~` anywhere in an array literal is out,
# not just when element 0 happens to hold a glob metacharacter
run_sash 'array a=(x =~ y)
echo $(len a)'
[[ $rc != 0 && -z $out && $err == *SB5075* ]]; chk g.12

# g.13 a QUOTED `=~` is an ordinary string element and stays legal — this is
# the escape the diagnostic names, and the reason the rule can be uniform
run_sash "array a=(x '=~' y)
echo \$(len a)"
[[ $rc = 0 && $out = "3" && -z $err ]]; chk g.13

# g.14 the replacement the message advises actually compiles and filters
run_sash "/usr/bin/touch g14a.ff g14b.ff g14c.gg
array a
for f in * { if [[ \$f =~ '\\.ff\$' ]] { a += (\$f) } }
echo \$(len a)"
[[ $rc = 0 && $out = "2" && -z $err ]]; chk g.14

# forglob.1-forglob.3: a `for` over a glob keeps its expansion in a compiler
# temporary that is reserved like every other (DD#125), so it can neither
# clobber a source name that happens to share its spelling nor be shared by a
# nested loop (batch-531 F13). It used to be the fixed name `__forglob`. The
# expansion's order is not asserted here.

# forglob.1 a source global named `__forglob` survives the loop (it was `[]`)
run_sash '/usr/bin/touch fga.fgx fgb.fgx
__forglob=keep
for f in *.fgx {
  echo "$f"
}
echo "[$__forglob]"'
[[ $rc = 0 && -z $err && $out == *'/fga.fgx'* && $out == *'/fgb.fgx'* \
   && $out == *$'\n[keep]' ]]; chk forglob.1

# forglob.2 a nested glob loop no longer overwrites the outer loop's list; the
# outer loop ran once
run_sash '/usr/bin/touch fgo1.fgo fgo2.fgo fgi.fgi
for a in *.fgo {
  for b in *.fgi {
    echo "$a $b"
  }
}'
[[ $rc = 0 && -z $err && $out == *'/fgo1.fgo '*'/fgi.fgi'* \
   && $out == *'/fgo2.fgo '*'/fgi.fgi'* \
   && $(printf '%s\n' "$out" | wc -l) = 2 ]]; chk forglob.2

# forglob.3 nor is a source const of that name a write target (a false SB4044)
run_sash '/usr/bin/touch fgc.fgc
const __forglob=keep
for f in *.fgc {
  echo "$f"
}
echo "[$__forglob]"'
[[ $rc = 0 && -z $err && $out == *'/fgc.fgc'$'\n[keep]' ]]; chk forglob.3

set -e
tally
