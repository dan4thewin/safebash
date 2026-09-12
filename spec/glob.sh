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
plan=9
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

set -e
tally
