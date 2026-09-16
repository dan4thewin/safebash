#!/bin/bash
# spec/cli.sh — CLI-switch behaviors that the -c/--emit-c spec runner
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
plan=39
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

# ── Usage, --help, and unknown flags (cli-usage-and-unknown-flags) ──
# There used to be three different usage strings, no --help at all, and any
# unknown `--` token was collected and never looked at, so a typo silently
# produced a different artifact than the one requested.

printf 'echo hi\n' > u.sash

# cli.8 --help prints on STDOUT and exits 0. It used to be collected as an
# unknown flag, leave no positional input, and fall into the "no input"
# branch: usage on stderr, exit 1.
out=$($SASH --help 2>/dev/null); rc=$?
[[ $rc = 0 && $out == Usage:* ]]; chk cli.8

# cli.9 ...and nothing goes to stderr
err=$($SASH --help 2>&1 >/dev/null); rc=$?
[[ $rc = 0 && -z $err ]]; chk cli.9

# cli.10 -h is the same thing
out=$($SASH -h 2>/dev/null); rc=$?
[[ $rc = 0 && $out == Usage:* ]]; chk cli.10

# cli.11 one usage text: the no-argument error and --help agree verbatim.
# Previously these were two different strings advertising different flags.
help_text=$($SASH --help 2>/dev/null)
noarg_text=$($SASH 2>&1 >/dev/null)
[[ $help_text = "$noarg_text" ]]; chk cli.11

# cli.12 ...and the --extract misuse path prints it too (it had a third,
# one-line string of its own)
extract_text=$($SASH --extract 2>&1 >/dev/null)
[[ $extract_text = "$noarg_text" ]]; chk cli.12

# cli.13 the usage text covers every advertised flag
missing=""
for f in -c -o --emit-ir --emit-c --manifest --extract --no-taint --trace --sysroot= --heap-size=; do
    [[ $help_text == *"$f"* ]] || missing="$missing $f"
done
[[ -z $missing ]]; chk cli.13

# cli.14 --no-taint-cst is a testing aid (DG, 2026-09-12) and stays OUT of
# the usage text, while remaining accepted
[[ $help_text != *"--no-taint-cst"* ]]; chk cli.14

# cli.15 ...still accepted
$SASH --no-taint-cst u.sash -o u1 >/dev/null 2>&1; rc=$?
[[ $rc = 0 ]]; chk cli.15

# cli.16 an unknown long flag is rejected. `--emit-C` (capital C) used to
# compile a native ELF and exit 0 — the user asked for C source.
err=$($SASH --emit-C u.sash 2>&1 >/dev/null); rc=$?
[[ $rc != 0 && $err == *"unrecognized option"* && $err == *"--emit-C"* ]]; chk cli.16

# cli.17 ...and no artifact is produced
[[ ! -e u.sash.out ]]; chk cli.17

# cli.18 a missing -o operand is an error. It used to be ignored, so the
# binary landed at the default path and sash exited 0.
err=$($SASH u.sash -o 2>&1 >/dev/null); rc=$?
[[ $rc != 0 && $err == *"needs an operand"* ]]; chk cli.18

# cli.19 the DD#57 converter flags keep their own directing diagnostic
# rather than degrading to a plain "unrecognized option"
err=$($SASH --annotate u.sash 2>&1 >/dev/null); rc=$?
[[ $rc != 0 && $err == *"sash-convert"* ]]; chk cli.19

# ── DD#107 --heap-size ────────────────────────────────────────────────
# The heap size is compiled in, never read from the environment: safebash
# goes out of its way to keep $PATH and friends from steering a running
# binary, and a $SASH_HEAP would be a way back in. So the *flag* is the
# whole surface, and a bad operand has to be caught here, at compile time.

# cli.20 a non-numeric size is rejected rather than parsed as 0 and
# silently producing a binary that cannot allocate anything
# The assertion names the size diagnostic specifically: every one of these
# operands is also rejected by the unknown-flag path when --heap-size is not
# recognised at all, so a bare "exited nonzero" would pass without the flag
# ever existing.
err=$($SASH --heap-size=abc u.sash -o h1 2>&1 >/dev/null); rc=$?
[[ $rc != 0 && $err == *"expected a positive size"* && ! -e h1 ]]; chk cli.20

# cli.21 ...as is a zero or overflowing size. Clamping would hand back a
# heap that differs from the one asked for, and the mismatch would only
# surface later as an unexplained out-of-memory.
err=$($SASH --heap-size=0 u.sash -o h2 2>&1 >/dev/null); rc=$?
[[ $rc != 0 && $err == *"expected a positive size"* && ! -e h2 ]]; chk cli.21
err=$($SASH --heap-size=99999999999999999999 u.sash -o h3 2>&1 >/dev/null); rc=$?
[[ $rc != 0 && $err == *"expected a positive size"* && ! -e h3 ]]; chk cli.22

# cli.23 a valid size compiles: the flag carries its value in the same
# token, so it must be prefix-matched and not read as an unknown flag
$SASH --heap-size=8M u.sash -o h4 >/dev/null 2>&1; rc=$?
[[ $rc = 0 && -e h4 ]]; chk cli.23

# cli.24 --manifest reports externals called from a user function's body,
# and does not leak the function's own name as a dependency.
printf 'fn g() { date +%%s; }\ng\n' > m3.sash
out=$($SASH --manifest m3.sash 2>/dev/null); rc=$?
[[ $rc = 0 && $out == *"/date"* && $out != *"g"$'\n'* ]]; chk cli.24

# cli.25 every stage of a pipeline is a dependency, not just the head.
printf 'date | cat\n' > m4.sash
out=$($SASH --manifest m4.sash 2>/dev/null); rc=$?
[[ $rc = 0 && $out == *"/date"* && $out == *"/cat"* ]]; chk cli.25

# cli.26 DD#55 externalized `true`, so a program of `echo`/`:`/`true` is no
# longer dependency-free: `true` resolves to a real binary and is listed.
# (cli.2 still holds — `echo` alone stays empty.)
printf 'echo hi\n:\ntrue\n' > m5.sash
out=$($SASH --manifest m5.sash 2>/dev/null); rc=$?
[[ $rc = 0 && $out == *"/true"* ]]; chk cli.26

# ── DD#109: every native compile embeds its IR and manifest ───────────────
# `--emit-ir` and `--manifest` were terminal stdout modes only, so the
# exhaustive IR emitter ran only when someone passed the flag. Embedding on
# every compile makes "a new IR variant does not render" a failure of every
# build instead of a latent gap in a rarely-taken path.

# 109.1 a native binary carries its IR capture, retrievable by section name.
$SASH -c 'int n=3
if (( n > 2 )) { echo big }' -o irbin 2>/dev/null
out=$($SASH --extract irbin ir 2>/dev/null); rc=$?
[[ $rc = 0 && $out == *":ir-version 1"* && $out == *":kind :if"* ]]; chk 109.1

# 109.2 the embedded IR is byte-identical to what --emit-ir prints for the
# same source. The section and the flag cannot drift apart.
printf 'int n=3\nif (( n > 2 )) { echo big }\n' > ir.sash
$SASH ir.sash -o irbin2 >/dev/null 2>&1
flag_ir=$($SASH --emit-ir ir.sash 2>/dev/null)
sect_ir=$($SASH --extract irbin2 ir 2>/dev/null)
[[ $flag_ir = "$sect_ir" ]]; chk 109.2

# 109.3 likewise the manifest: the embedded section equals --manifest.
printf 'date | cat\n' > mf.sash
$SASH mf.sash -o mfbin >/dev/null 2>&1
flag_mf=$($SASH --manifest mf.sash 2>/dev/null)
sect_mf=$($SASH --extract mfbin manifest 2>/dev/null)
[[ $flag_mf = "$sect_mf" && $sect_mf == *"/date"* ]]; chk 109.3

# 109.4 a program with no external dependencies has an EMPTY manifest, not a
# missing one. A zero-length section does not survive into the linked ELF, so
# the absent section must read back as empty rather than as an error — this
# is the common case (an all-builtin program), and reporting it as a missing
# artifact would make it look broken.
$SASH -c 'echo hi' -o nodeps 2>/dev/null
out=$($SASH --extract nodeps manifest 2>/dev/null); rc=$?
[[ $rc = 0 && -z $out ]]; chk 109.4

# 109.5 the DD#45 spelling is unchanged: no section argument still means
# source, so --extract <binary> keeps its original meaning.
$SASH -c 'echo hello' -o srcbin 2>/dev/null
out=$($SASH --extract srcbin 2>/dev/null); rc=$?
[[ $rc = 0 && $out == *"echo hello"* ]]; chk 109.5

# 109.6 an unknown section name is rejected rather than silently treated as
# the default.
err=$($SASH --extract srcbin bogus 2>&1 >/dev/null); rc=$?
[[ $rc != 0 && $err == *"bogus"* ]]; chk 109.6

# 109.7 the empty-manifest fallback is scoped to sash binaries: a file with
# no .sash_source still errors rather than reporting an empty manifest.
: > notsash.bin
$SASH --extract notsash.bin manifest >/dev/null 2>&1; rc=$?
[[ $rc != 0 ]]; chk 109.7

# cli.27-29 `native_output_path`: with no `-o`, the binary is named from the
# input. The spec runner always passes `-o`, so all three branches here were
# never-run. `.sh` and `.bash` are stripped; anything else gains `.out`.
printf 'echo hi\n' > op1.sh
$SASH op1.sh >/dev/null 2>op.err; rc=$?
out=$([[ -x op1 ]] && ./op1)
[[ $rc = 0 && $out = "hi" && ! -e op1.sh.out ]]; chk cli.27

printf 'echo bee\n' > op2.bash
$SASH op2.bash >/dev/null 2>op.err; rc=$?
out=$([[ -x op2 ]] && ./op2)
[[ $rc = 0 && $out = "bee" ]]; chk cli.28

# no recognised suffix → `<input>.out`, so the input is never overwritten
printf 'echo enn\n' > op3
$SASH op3 >/dev/null 2>op.err; rc=$?
out=$([[ -x op3.out ]] && ./op3.out)
[[ $rc = 0 && $out = "enn" ]]; chk cli.29

# cli.30 the inline form with no `-o` is the `is_inline` branch → `a.out`
rm -f a.out
$SASH -c 'echo inline-out' >/dev/null 2>op.err; rc=$?
out=$([[ -x a.out ]] && ./a.out)
[[ $rc = 0 && $out = "inline-out" ]]; chk cli.30

# cli.31-32 `sysroot_file_exists` squashfs branch. cli.6 covers the plain
# directory sysroot; a `.squashfs` / `.sqsh` root instead shells out to
# `unsquashfs -l` once and caches the listing. A root that is not a readable
# image yields an empty listing, so the command simply does not resolve —
# the point is that the switch handler runs rather than what it finds.
# Two externals in one compile exercise the second lookup, which reuses the
# populated cache rather than repopulating it.
PATH=/bin $SASH --sysroot="$PWD/nosuch.squashfs" -c 'toolA
toolB' -o sq1 >/dev/null 2>sq.err; rc=$?
[[ $rc != 0 && $(grep -c SB5011 sq.err) = 2 ]]; chk cli.31

PATH=/bin $SASH --sysroot="$PWD/nosuch.sqsh" -c 'toolA' -o sq2 >/dev/null 2>sq2.err; rc=$?
[[ $rc != 0 && $(grep -c SB5011 sq2.err) = 1 ]]; chk cli.32

set -e
tally
