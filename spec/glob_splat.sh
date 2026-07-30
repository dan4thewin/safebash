#!/bin/bash
# spec/glob_splat.sh — DD#69 Part 2: external-splat empty-disposition.
# A bare `@a` splat into an EXTERNAL command is a compile error (SB5032);
# it must carry an empty-disposition `@{a:-WORD}` (empty → the single arg
# WORD; bare `@{a:-}` → zero args) or `@{a:?WORD}` (empty → runtime abort
# `<progname>: WORD`). Builtins (echo/…) and for-in are EXEMPT. Non-empty
# arrays splat per-element. Only `:-`/`:?` are valid after `@{a`.
set -eu
. common.sh
setup glob_splat
plan=19
echo "1..$plan" >&3

set +e

# gs.1 a bare `@a` into an EXTERNAL command is a compile error (SB5032),
#      directing to a disposition.
run_sash 'array a=(x y); /bin/echo @a'
[[ $rc != 0 && -z $out && $err == *SB5032* ]]; chk gs.1

# gs.2 `@{a:-WORD}` non-empty → the array splats per-element (WORD unused)
run_sash 'array a=(x y); /bin/echo @{a:-def}'
[[ $rc = 0 && $out = "x y" && -z $err ]]; chk gs.2

# gs.3 `@{a:-WORD}` empty → the single argument WORD
run_sash 'array a=(); /bin/echo @{a:-def}'
[[ $rc = 0 && $out = "def" && -z $err ]]; chk gs.3

# gs.4 bare `@{a:-}` empty → zero args (echo prints just a newline)
run_sash 'array a=(); /bin/echo @{a:-}'
[[ $rc = 0 && $out = "" && -z $err ]]; chk gs.4

# gs.5 `@{a:?WORD}` non-empty → the array splats per-element
run_sash 'array a=(x y); /bin/echo @{a:?oops}'
[[ $rc = 0 && $out = "x y" && -z $err ]]; chk gs.5

# gs.6 `@{a:?WORD}` empty → runtime abort, exit 1, stderr `<progname>: WORD`.
#      native-only: the c/janet spec harnesses discard runtime stderr, so
#      the abort diagnostic (rerr) can only be asserted natively. Assert the
#      message SUFFIX (after the program's own argv[0]).
if in_modes native; then
    run_sash 'array a=(); /bin/echo @{a:?no args!}'
    [[ $rc = 1 && -z $out && $rerr == *"no args!" ]]; chk gs.6
else
    skip_chk gs.6
fi

# gs.7 `@a` into the `echo` BUILTIN is EXEMPT from the disposition rule
#      (SB5032 is external-only) — it is still rejected by SB5030 (DD#22
#      flatten), NOT SB5032. This pins the external-only scoping.
run_sash 'array a=(x y); echo @a'
[[ $rc != 0 && $err == *SB5030* && $err != *SB5032* ]]; chk gs.7

# gs.8 a for-in over `@a` is EXEMPT (per-element iteration, its own empty
#      handling — zero matches → body never runs)
run_sash 'array a=(x y); for w in @a; do echo "$w"; done'
[[ $rc = 0 && $out = $'x\ny' && -z $err ]]; chk gs.8

# gs.9 only `:-` and `:?` are valid after `@{a` — `:+` is a compile error
run_sash 'array a=(x); /bin/echo @{a:+w}'
[[ $rc != 0 && -z $out && -n $err ]]; chk gs.9

# gs.10 `:=` is likewise rejected (DD#65 scalar prune parity)
run_sash 'array a=(x); /bin/echo @{a:=w}'
[[ $rc != 0 && -z $out && -n $err ]]; chk gs.10

# gs.11 WORD is a full interpolating word: `$p` expands
run_sash 'p=hi; array a=(); /bin/echo @{a:-$p}'
[[ $rc = 0 && $out = "hi" && -z $err ]]; chk gs.11

# gs.12 WORD substitutes as a SINGLE element (not re-split): a space in the
#       WORD stays one argument. WORD is a `${x:-WORD}`-style body (quotes
#       literal), so an unquoted space is the cleanest single-element probe.
run_sash 'array a=(); /usr/bin/printf "[%s]" @{a:-a b}'
[[ $rc = 0 && $out = "[a b]" && -z $err ]]; chk gs.12

# gs.13 a non-empty splat still passes each element as its own arg
run_sash 'array a=(one two); /usr/bin/printf "[%s]" @{a:?x}'
[[ $rc = 0 && $out = "[one][two]" && -z $err ]]; chk gs.13

# gs.14 DD#69 §3 + ICE fix: a DISPOSITIONED splat `@{a:?}` to `echo` is the
#       same flatten hazard as bare `@a`, so it is SB5030 — NOT an ICE.
#       (Regression: `echo @{a:?}` previously slipped past SB5030 because the
#       `arrvals_abort` operator was unrecognized, reaching a scalar value
#       lowering that panicked at codegen/values.rs.)
run_sash 'array a=(x y); echo @{a:?}'
[[ $rc != 0 && $err == *SB5030* ]]; chk gs.14

# gs.15 likewise the `@{a:-WORD}` disposition to `echo` → SB5030, not an ICE.
run_sash 'array a=(x y); echo @{a:-w}'
[[ $rc != 0 && $err == *SB5030* ]]; chk gs.15

# gs.16 DD#69 addendum: a splat is not a scalar — `x=@a` in scalar-assignment
#       context is SB5025 (use `$(join @a)` for a string, `${a[i]}` for one).
run_sash 'array a=(x y); x=@a; /bin/echo "$x"'
[[ $rc != 0 && $err == *SB5025* ]]; chk gs.16

# gs.17 likewise a DISPOSITIONED splat in scalar context → SB5025.
run_sash 'array a=(x y); x=@{a:?}; /bin/echo "$x"'
[[ $rc != 0 && $err == *SB5025* ]]; chk gs.17

# gs.18 DD#69 addendum: the `@{a:…}` empty-disposition is command-argument-
#       only — `for f in @{a:?}` is SB5034 (a for-in source is the bare `@a`).
run_sash 'array a=(x y); for f in @{a:?}; do /bin/echo "$f"; done'
[[ $rc != 0 && $err == *SB5034* ]]; chk gs.18

# gs.19 quotes are literal (bash-compatible), NOT a splat: `echo "@a"` prints
#       the literal `@a` (the native splat is the UNQUOTED `@a`). Not rejected.
run_sash 'array a=(x y); echo "@a"'
[[ $rc = 0 && $out = "@a" && -z $err ]]; chk gs.19

set -e
tally
