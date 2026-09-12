#!/bin/bash
# spec/filetest.sh — [[ -OP FILE ]] unary file tests. Bash-compatible
# semantics for the whole operator set. Section A pins -N, which compares
# WHOLE SECONDS with `>=` and ignores nanoseconds entirely (bash test.c);
# sash previously used a strict `>` plus a nanosecond tiebreak and answered
# "no" to the two cases -N exists to report. Janet lowers only -d/-e/-f, so
# everything else is guarded to native,c.
set -eu
. common.sh
setup filetest
plan=28
echo "1..$plan" >&3

set +e

# ── A. -N: modified since last read ──────────────────────────────────
# Bash: mtime >= atime on whole seconds. Nanoseconds are NOT consulted.
if in_modes native,c; then
    # A1 freshly written, never read — mtime == atime to the nanosecond.
    # The case -N exists to report. A strict `>` answered "no".
    printf 'x\n' > n1
    run_sash 'if [[ -N n1 ]]; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.1

    # A2 same, with both stamps pinned exactly equal
    printf 'x\n' > n2; touch -d '@1700000000.000000000' n2
    run_sash 'if [[ -N n2 ]]; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.2

    # A3 mtime strictly newer than atime
    printf 'x\n' > n3
    touch -a -d '@1700000000' n3; touch -m -d '@1700000900' n3
    run_sash 'if [[ -N n3 ]]; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.3

    # A4 mtime older than atime — read since last write
    printf 'x\n' > n4
    touch -m -d '@1700000000' n4; touch -a -d '@1700000900' n4
    run_sash 'if [[ -N n4 ]]; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.4

    # A5 same second, atime nanoseconds GREATER than mtime's. Bash still
    # says yes because it truncates to seconds. The old nsec tiebreak
    # made this "no".
    printf 'x\n' > n5
    touch -m -d '@1700000000.000000100' n5; touch -a -d '@1700000000.000000900' n5
    run_sash 'if [[ -N n5 ]]; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.5

    # A6 nonexistent file is never "modified since read"
    run_sash 'if [[ -N nosuch ]]; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.6
else
    skip_chk ft.1; skip_chk ft.2; skip_chk ft.3
    skip_chk ft.4; skip_chk ft.5; skip_chk ft.6
fi

# ── B. portable core: -e / -f / -d (all three backends) ──────────────
printf 'x\n' > plain; mkdir -p adir

run_sash 'if [[ -e plain ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.7
run_sash 'if [[ -e nosuch ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.8
run_sash 'if [[ -f plain ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.9
# -f is regular-file, not merely existing: a directory is "no"
run_sash 'if [[ -f adir ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.10
run_sash 'if [[ -d adir ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.11
run_sash 'if [[ -d plain ]]; then echo yes; else echo no; fi'
[[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.12

# ── C. the remaining operators (native,c) ────────────────────────────
if in_modes native,c; then
    : > empty; ln -sf plain slink; mkfifo afifo 2>/dev/null
    printf '#!/bin/sh\n' > execf; chmod +x execf

    # -s: exists AND has nonzero size
    run_sash 'if [[ -s plain ]]; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.13
    run_sash 'if [[ -s empty ]]; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.14

    # -r / -w: access permission
    run_sash 'if [[ -r plain ]]; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.15
    run_sash 'if [[ -w plain ]]; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.16
    run_sash 'if [[ -r nosuch ]]; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.17

    # -x: execute permission, not mere existence
    run_sash 'if [[ -x execf ]]; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.18
    run_sash 'if [[ -x plain ]]; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.19

    # -L / -h: symlink. Both test the link itself, not its target.
    run_sash 'if [[ -L slink ]]; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.20
    run_sash 'if [[ -L plain ]]; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.21
    run_sash 'if [[ -h slink ]]; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.22
    # -f follows the link, so a symlink to a regular file is "yes"
    run_sash 'if [[ -f slink ]]; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.23

    # -c / -b: character vs block special. /dev/null is character.
    run_sash 'if [[ -c /dev/null ]]; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.24
    run_sash 'if [[ -b /dev/null ]]; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.25

    # -p: named pipe.  -S: socket (a plain file is not one)
    run_sash 'if [[ -p afifo ]]; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.26
    run_sash 'if [[ -S plain ]]; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "no" && -z $err ]]; chk ft.27

    # -O: owned by the effective uid.  -a: historical alias of -e.
    run_sash 'if [[ -O plain ]]; then echo yes; else echo no; fi'
    [[ $rc = 0 && $out = "yes" && -z $err ]]; chk ft.28
else
    skip_chk ft.13; skip_chk ft.14; skip_chk ft.15; skip_chk ft.16
    skip_chk ft.17; skip_chk ft.18; skip_chk ft.19; skip_chk ft.20
    skip_chk ft.21; skip_chk ft.22; skip_chk ft.23; skip_chk ft.24
    skip_chk ft.25; skip_chk ft.26; skip_chk ft.27; skip_chk ft.28
fi

tally
