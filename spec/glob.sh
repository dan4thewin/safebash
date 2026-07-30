#!/bin/bash
# spec/glob.sh — DD#73: glob expansion yields normalized ABSOLUTE paths.
#
# A glob element is `<getcwd>/name`, not the bare directory-entry name —
# for referential stability across a chdir, and to close glob-sourced
# option-injection (a `-`-leading filename becomes `/abs/-name`, which no
# external command parses as an option). Lexical (no realpath); no relative
# segment survives. native+c only — Janet does not expand file globs.
#
# NOTE: `run_sash` (native) leaves `./test_bin` in the cwd during the run,
# so these tests use suffixed patterns (`*.www`, never bare `*`) to avoid
# matching the harness binary.
set -eu
. common.sh
setup glob
plan=6
echo "1..$plan" >&3

set +e

# g.1 a glob element is an absolute path `<cwd>/name`, not the bare name.
if in_modes native,c; then
    run_sash '/usr/bin/touch g1.www; for f in *.www; do echo "$f"; done'
    cwd=$(pwd -P)
    [[ $rc = 0 && $out = "$cwd/g1.www" && -z $err ]]; chk g.1
else
    skip_chk g.1
fi

# g.2 option-injection closed: a `-`-leading filename becomes `<cwd>/-name`
#     (starts with `/`, so no external command reparses it as an option).
if in_modes native,c; then
    run_sash '/usr/bin/touch -- -rf.zz; for f in *.zz; do echo "$f"; done'
    cwd=$(pwd -P)
    [[ $rc = 0 && $out = "$cwd/-rf.zz" && $out = /* && -z $err ]]; chk g.2
else
    skip_chk g.2
fi

# g.3 the array-literal form `array a=(*)` carries absolute elements too
#     (observed via for-in, which is disposition-exempt).
if in_modes native,c; then
    run_sash '/usr/bin/touch g3.rr; array a=(*.rr); for f in @a; do echo "$f"; done'
    cwd=$(pwd -P)
    [[ $rc = 0 && $out = "$cwd/g3.rr" && -z $err ]]; chk g.3
else
    skip_chk g.3
fi

# g.4 referential stability: the absolute element still names the same file
#     after a chdir — the DD#73 correctness rationale. With a bare name,
#     `cat g4.tt` after `cd /` would fail.
if in_modes native,c; then
    run_sash 'echo hi > g4.tt; array a=(*.tt); cd /; /bin/cat @{a:?x}'
    [[ $rc = 0 && $out = "hi" && -z $err ]]; chk g.4
else
    skip_chk g.4
fi

# g.5 zero matches → empty (absolutization does not fabricate an entry).
if in_modes native,c; then
    run_sash 'array a=(*.nomatchQQ); /usr/bin/printf "[%s]" @{a:-none}'
    [[ $rc = 0 && $out = "[none]" && -z $err ]]; chk g.5
else
    skip_chk g.5
fi

# g.6 every element of a multi-match glob is absolute (order-independent).
if in_modes native,c; then
    run_sash '/usr/bin/touch g6a.pp g6b.pp; for f in *.pp; do echo "$f"; done'
    cwd=$(pwd -P)
    [[ $rc = 0 && $out == *"$cwd/g6a.pp"* && $out == *"$cwd/g6b.pp"* ]]; chk g.6
else
    skip_chk g.6
fi

set -e
tally
