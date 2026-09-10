#!/bin/bash
# spec/cli.sh — CLI-switch behaviors that the -c/--janet/--emit-c spec runner
# never reaches: --manifest (emit_manifest), --extract (dump_sash_source), and
# --sysroot= (set_sysroot + sysroot_file_exists). These are front-end/CLI and
# mode-independent, so each check runs identically under every SASH_MODE; the
# point is to exercise the switch handlers directly (they were never-run in the
# coverage grind — only Rust e2e touched them). Invokes ${SASH} with the flags
# rather than run_sash so the instrumented binary (under coverage) is exercised.
set -eu
. common.sh
setup cli
set +e
plan=7
echo "1..$plan" >&3

SASH=$(command -v "${SASH:-sash}")

# cli.1 --manifest lists an external command dependency (emit_manifest → the
# allowlist collected at the resolution funnel: a resolved name containing '/').
printf '/bin/echo hi\n' > m.sash
out=$($SASH --manifest m.sash 2>/dev/null); rc=$?
[[ $rc = 0 && $out == *"/bin/echo"* ]]; chk cli.1

# cli.2 --manifest of an all-builtin program has no external deps → empty.
printf 'echo hi\n' > m2.sash
out=$($SASH --manifest m2.sash 2>/dev/null); rc=$?
[[ $rc = 0 && -z $out ]]; chk cli.2

# cli.3 --extract dumps the `.sash_source` section every native binary embeds
# (dump_sash_source; DD#45).
$SASH -c 'echo hello' -o mybin 2>/dev/null
out=$($SASH --extract mybin 2>/dev/null); rc=$?
[[ $rc = 0 && $out == *"echo hello"* ]]; chk cli.3

# cli.4 --extract on a file with no `.sash_source` section errors cleanly
# (not a silent success).
: > empty.bin
$SASH --extract empty.bin >/dev/null 2>err.txt; rc=$?
[[ $rc != 0 ]]; chk cli.4

# cli.5 control: a command that is not on PATH is unresolvable → SB5011
# (no --sysroot; the resolution funnel can't find it).
mkdir -p sr/bin; : > sr/bin/mytool; chmod +x sr/bin/mytool
PATH=/bin $SASH -c 'mytool' -o /dev/null 2>ns.err; rc=$?
[[ $rc != 0 ]] && grep -q SB5011 ns.err; chk cli.5

# cli.6 --sysroot= makes the same command resolve: set_sysroot records the
# root and sysroot_file_exists (dir branch) finds <sysroot>/<PATH-dir>/mytool.
PATH=/bin $SASH --sysroot="$PWD/sr" -c 'mytool' -o sysbin 2>ws.err; rc=$?
[[ $rc = 0 ]]; chk cli.6

# cli.7 the -o output name is passed as its own argument, not glued. `ld` uses
# getopt_long_only, so a glued `-ou` matched the single-dash long option
# `--out-implib` by unambiguous prefix and swallowed the object file as its
# argument — linking failed for any binary named `u` or `ut`, with the
# object silently unlinked ("undefined reference to `main'").
$SASH -c 'echo hi' -o ut >/dev/null 2>&1; rc=$?
out=$([[ -x ut ]] && ./ut 2>/dev/null)
[[ $rc = 0 && $out = "hi" ]]; chk cli.7

set -e
tally
