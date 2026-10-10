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
plan=64
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

# cli.31-32 an unreadable `--sysroot` image is a USAGE error, not a language
# one, and compilation does not start (DG, 2026-10-10). These used to assert
# one SB5011 per command head: an absent image yielded an empty listing, so
# every head in the program reported "not found" and the single real fault —
# a bad compiler argument — was buried under a diagnostic per line of the
# user's source, each blaming the wrong thing.
#
# These two prove nothing about the listing PARSER; they never did. The sq.*
# block below resolves against a real image, which is what covers it — and
# which now also exercises the cache priming this check performs.
PATH=/bin $SASH --sysroot="$PWD/nosuch.squashfs" -c 'toolA
toolB' -o sq1 >/dev/null 2>sq.err; rc=$?
sr_err=$(cat sq.err)
[[ $rc != 0 && $(grep -c SB sq.err) = 0 && $sr_err == "sash: --sysroot="* && $sr_err == *nosuch.squashfs* ]]; chk cli.31

# A directory sysroot that is not there has the identical failure shape, so
# it reports the same way rather than SB5011 per head.
PATH=/bin $SASH --sysroot="$PWD/nosuchdir" -c 'toolA' -o sq2 >/dev/null 2>sq2.err; rc=$?
sr_err=$(cat sq2.err)
[[ $rc != 0 && $(grep -c SB sq2.err) = 0 && $sr_err == "sash: --sysroot="* && $sr_err == *nosuchdir* ]]; chk cli.32

# cli.40 a sysroot that EXISTS but is not a directory. Distinct from cli.32:
# `metadata` succeeds, so the fault is the kind rather than the lookup.
: > sr_plain_file
$SASH --sysroot="$PWD/sr_plain_file" -c 'toolA' -o sq3 >/dev/null 2>sq3.err; rc=$?
sr_err=$(cat sq3.err)
[[ $rc != 0 && $(grep -c SB sq3.err) = 0 && $sr_err == *"not a directory"* ]]; chk cli.40

# cli.41 the image reader needs `unsquashfs`, and its absence is reported as
# the missing tool rather than as an unreadable image — the two faults send a
# reader to different places. `Command::new` returns NotFound before the image
# is ever opened, so this does not depend on the path existing.
PATH=/nonexistent $SASH --sysroot="$PWD/absent.squashfs" -c 'toolA' -o sq4 >/dev/null 2>sq4.err; rc=$?
sr_err=$(cat sq4.err)
[[ $rc != 0 && $(grep -c SB sq4.err) = 0 && $sr_err == *"unsquashfs is not installed"* ]]; chk cli.41

# cli.42 the hazard case, and the reason this is a usage error at all: an
# image that is PRESENT but unreadable. An absent one is obvious; a corrupt
# one used to yield an empty listing, which answered every path lookup with a
# confident "not there" and buried the real fault under one SB5095 per
# command head. `unsquashfs` exits nonzero and its own words are reported.
: > corrupt.squashfs
$SASH --sysroot="$PWD/corrupt.squashfs" -c 'toolA' -o sq5 >/dev/null 2>sq5.err; rc=$?
sr_err=$(cat sq5.err)
[[ $rc != 0 && $(grep -c SB sq5.err) = 0 && $sr_err == "sash: --sysroot="*corrupt.squashfs* ]]; chk cli.42

# sq.1-15 the squashfs sysroot resolved against a REAL image.
#
# Every line of the `unsquashfs -ll` parser and the symlink follower was cold
# until this block existed — cli.31-32 exercise the switch, never the parse.
# Several checks below are the only record in the tree of a fact about
# `unsquashfs -ll` output format, so each one names which fact it holds.
#
# Do NOT add `PATH=/bin` to these: the compiler resolves `unsquashfs` through
# PATH, and restricting it makes every check pass for the wrong reason (an
# unreadable image, not a resolved one). Absolute heads do not consult PATH,
# so leaving it alone costs nothing.
#
# The fixture needs no root — mksquashfs's pseudo-file syntax supplies the
# device node, and chmod supplies every mode case.
mkdir -p sqtree/bin sqtree/usr/bin sqtree/usr/lib
cp /bin/true sqtree/bin/tool
cp /bin/true sqtree/usr/lib/libtool
ln -s bin/tool sqtree/rel
ln -s /bin/tool sqtree/abs
ln -s ../lib/libtool sqtree/usr/bin/updown
ln -s ../nowhere/gone sqtree/usr/bin/dangle
ln -s loop2 sqtree/loop1
ln -s loop1 sqtree/loop2
cp /bin/true sqtree/noexec && chmod 444 sqtree/noexec
cp /bin/true sqtree/setuid && chmod 4644 sqtree/setuid
cp /bin/true sqtree/setuid_exec && chmod 4755 sqtree/setuid_exec
cp /bin/true sqtree/sticky_exec && chmod 1755 sqtree/sticky_exec
printf 'dev d 755 0 0\ndev/zero c 666 0 0 1 5\n' > sq.pf
mksquashfs sqtree img.squashfs -no-progress -quiet -pf sq.pf >/dev/null 2>&1
SQ="$PWD/img.squashfs"

# sq.1 a plain regular executable inside the image resolves. This is the
#      check that proves the listing was parsed at all.
$SASH --sysroot="$SQ" -c '/bin/tool' -o sqa >/dev/null 2>sqa.err; rc=$?
[[ $rc = 0 && ! -s sqa.err ]]; chk sq.1

# sq.2 a relative symlink is followed, read against the link's own directory.
$SASH --sysroot="$SQ" -c '/rel' -o sqb >/dev/null 2>sqb.err; rc=$?
[[ $rc = 0 && ! -s sqb.err ]]; chk sq.2

# sq.3 an absolute symlink target is read from the image root, not the host's.
$SASH --sysroot="$SQ" -c '/abs' -o sqc >/dev/null 2>sqc.err; rc=$?
[[ $rc = 0 && ! -s sqc.err ]]; chk sq.3

# sq.4 `..` in a target is resolved before lookup. A squashfs listing holds
#      no `..` in any key, so an unnormalised join misses and reports the
#      program absent — a FALSE REJECT, and `../lib/x` is how real images
#      spell most cross-directory links. This check is that bug's gravestone.
$SASH --sysroot="$SQ" -c '/usr/bin/updown' -o sqd >/dev/null 2>sqd.err; rc=$?
[[ $rc = 0 && ! -s sqd.err ]]; chk sq.4

# sq.5 ...and normalising is not the same as accepting: a `..` target that
#      genuinely leads nowhere is still rejected. Without this, sq.4 would
#      also pass if the follower simply stopped checking.
$SASH --sysroot="$SQ" -c '/usr/bin/dangle' -o sqe >/dev/null 2>sqe.err; rc=$?
[[ $rc != 0 && $(<sqe.err) == *"SB5095"*"no such file"* ]]; chk sq.5

# sq.6 a symlink cycle is named as one. The hop limit is what keeps it from
#      hanging the compiler, and `PathMiss::LinkLoop` is why it does not
#      report "no such file" — the end of a loop is not an empty directory.
$SASH --sysroot="$SQ" -c '/loop1' -o sqf >/dev/null 2>sqf.err; rc=$?
[[ $rc != 0 && $(<sqf.err) == *"SB5095"*"symlink chain does not terminate"* ]]; chk sq.6

# sq.7 a directory is named as one.
$SASH --sysroot="$SQ" -c '/bin' -o sqg >/dev/null 2>sqg.err; rc=$?
[[ $rc != 0 && $(<sqg.err) == *"SB5095"*"it is a directory"* ]]; chk sq.7

# sq.8 a device node is rejected as not-a-regular-file — and its listing line
#      is the format fact that breaks naive parsing: the size column reads
#      `1,  5`, two whitespace-separated tokens where every other line has
#      one. A column-counting parser gets the path wrong for this line.
$SASH --sysroot="$SQ" -c '/dev/zero' -o sqh >/dev/null 2>sqh.err; rc=$?
[[ $rc != 0 && $(<sqh.err) == *"SB5095"*"it is not a regular file"* ]]; chk sq.8

# sq.9 a regular file with no execute bit is rejected, and the message quotes
#      the mode so the reader can see why.
$SASH --sysroot="$SQ" -c '/noexec' -o sqi >/dev/null 2>sqi.err; rc=$?
[[ $rc != 0 && $(<sqi.err) == *"SB5095"*"mode 0444 grants no execute"* ]]; chk sq.9

# sq.10 `-rwSr--r--`: an UPPERCASE S is setuid WITHOUT execute. The mode must
#       come out 0644, not 0744 — reading the character as an execute bit
#       here would wave through a file nothing can run.
$SASH --sysroot="$SQ" -c '/setuid' -o sqj >/dev/null 2>sqj.err; rc=$?
[[ $rc != 0 && $(<sqj.err) == *"SB5095"*"mode 0644 grants no execute"* ]]; chk sq.10

# sq.11 `-rwsr-xr-x`: a LOWERCASE s is setuid WITH execute, so this resolves.
#       The pair sq.10/sq.11 is the whole reason `triple_bits` reads the
#       character instead of testing it for `x`; testing for `x` would reject
#       every setuid binary in a real image.
$SASH --sysroot="$SQ" -c '/setuid_exec' -o sqk >/dev/null 2>sqk.err; rc=$?
[[ $rc = 0 && ! -s sqk.err ]]; chk sq.11

# sq.12 `-rwxr-xr-t`: a lowercase t is sticky WITH execute. Same rule, other
#       character, so a fix that special-cased only `s` would fail here.
$SASH --sysroot="$SQ" -c '/sticky_exec' -o sql >/dev/null 2>sql.err; rc=$?
[[ $rc = 0 && ! -s sql.err ]]; chk sq.12

# sq.13 a path simply not in the image.
$SASH --sysroot="$SQ" -c '/nosuch' -o sqm >/dev/null 2>sqm.err; rc=$?
[[ $rc != 0 && $(<sqm.err) == *"SB5095"*"no such file"* ]]; chk sq.13

# sq.14 two in-image commands in one compile. cli.31 makes this claim against
#       an EMPTY listing, where a broken cache would be invisible; here the
#       second lookup has to find something.
$SASH --sysroot="$SQ" -c '/bin/tool
/rel' -o sqn >/dev/null 2>sqn.err; rc=$?
[[ $rc = 0 && ! -s sqn.err ]]; chk sq.14

# sq.15 the manifest reports in-image paths. SB5095 exists so the manifest
#       cannot assert a dependency the compiler knows is unmeetable; this is
#       the other half — what it does assert comes from the image.
printf '/bin/tool\n/usr/bin/updown\n' > sqman.sash
out=$($SASH --sysroot="$SQ" --manifest sqman.sash 2>/dev/null); rc=$?
[[ $rc = 0 && $out == *"/bin/tool"* && $out == *"/usr/bin/updown"* ]]; chk sq.15

# cli.33-36 the `K` and `G` multipliers. cli.23 covers `M`, which is the only
# suffix any spec had ever spelled. Accepting the suffix is not the claim
# worth making — that a stuck multiplier of 1 would also satisfy — so each
# size is paired with an operand that overflows `usize` *only after* scaling:
# the bare number compiles, the suffixed one is rejected. That is the
# multiplication itself, observed.
$SASH --heap-size=4K u.sash -o h5 >/dev/null 2>&1; rc=$?
[[ $rc = 0 && -e h5 ]]; chk cli.33
$SASH --heap-size=99999999999999999 u.sash -o h6 >/dev/null 2>&1; rc=$?
err=$($SASH --heap-size=99999999999999999K u.sash -o h7 2>&1 >/dev/null); rc2=$?
[[ $rc = 0 && $rc2 != 0 && $err == *"expected a positive size"* && ! -e h7 ]]; chk cli.34

$SASH --heap-size=1G u.sash -o h8 >/dev/null 2>&1; rc=$?
[[ $rc = 0 && -e h8 ]]; chk cli.35
$SASH --heap-size=99999999999 u.sash -o h9 >/dev/null 2>&1; rc=$?
err=$($SASH --heap-size=99999999999G u.sash -o h10 2>&1 >/dev/null); rc2=$?
[[ $rc = 0 && $rc2 != 0 && $err == *"expected a positive size"* && ! -e h10 ]]; chk cli.36

# cli.37 --extract of a path that cannot be read at all. cli.4 covers a file
# that exists but carries no section; this is the earlier failure, where the
# read itself fails, and it has to name the path and the OS reason rather
# than report the file as sectionless.
err=$($SASH --extract ./no-such-binary source 2>&1 >/dev/null); rc=$?
[[ $rc != 0 && $err == *"./no-such-binary"* && $err == *"No such file"* ]]; chk cli.37

# cli.38 a flag with neither a script operand nor `-c` is a usage error.
# cli.11-12 cover the no-argument and --extract-misuse spellings; this is the
# third caller of USAGE, reached only when flags parsed cleanly and left
# nothing to compile.
err=$($SASH --no-taint 2>&1 >/dev/null); rc=$?
[[ $rc != 0 && $err == *"Usage: sash"* ]]; chk cli.38

# cli.39 a named script that cannot be read. The diagnostic is the compiler's
# own, not a panic: an unreadable input is an ordinary user mistake and has to
# name the path and the reason.
err=$($SASH ./no-such-script.sash -o ns.out 2>&1 >/dev/null); rc=$?
[[ $rc != 0 && $err == *"./no-such-script.sash"* && $err == *"No such file"* && ! -e ns.out ]]; chk cli.39

set -e
tally
