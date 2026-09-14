#!/bin/bash
# spec/glob_splat.sh — DD#69 Part 2: external-splat empty-disposition.
# A bare `@a` splat into an EXTERNAL command is a compile error (SB5032);
# it must carry an empty-disposition `@{a:-WORD}` (empty → the single arg
# WORD; bare `@{a:-}` → one EMPTY arg, bash `"${a[@]:-}"` parity), `@{a:?WORD}`
# (empty → runtime abort `<progname>: WORD`), or `@{a:~}` (elide: empty → ZERO
# args, no word — DD#84; the sole drop-if-empty form). Builtins (echo/…) and
# for-in are EXEMPT. Non-empty arrays splat per-element. Only `:-`/`:?`/`:~`
# are valid after `@{a`.
set -eu
. common.sh
setup glob_splat
plan=40
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

# gs.4 bare `@{a:-}` empty → ONE empty element (bash `"${a[@]:-}"` parity, and
#       consistent with the scalar `${x:-}`; DD#69/#84 2026-09-04). Probe with a
#       leading fixed arg so zero-args vs one-empty-arg is distinguishable:
#       `[PRE]` alone = zero args; `[PRE]` then `[]` = one empty element.
run_sash 'array a=(); /usr/bin/printf "[%s]\n" PRE @{a:-}'
[[ $rc = 0 && $out = $'[PRE]\n[]' && -z $err ]]; chk gs.4

# gs.5 `@{a:?WORD}` non-empty → the array splats per-element
run_sash 'array a=(x y); /bin/echo @{a:?oops}'
[[ $rc = 0 && $out = "x y" && -z $err ]]; chk gs.5

# gs.6 `@{a:?WORD}` empty → runtime abort, exit 1, stderr `<progname>: WORD`.
#      Assert the message suffix after the program's own argv[0].
run_sash 'array a=(); /bin/echo @{a:?no args!}'
[[ $rc = 1 && -z $out && $rerr == *"no args!" ]]; chk gs.6

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
#       context is SB5025 (use `$(join a)` for a string, `${a[i]}` for one).
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
run_sash 'array a=(x y); : @a; echo "@a"'
[[ $rc = 0 && $out = "@a" && -z $err ]]; chk gs.19

# gs.20 DD#84: `@{a:~}` (elide) empty → ZERO args — the sole zero-args
#       disposition (contrast gs.4: `@{a:-}` empty → one EMPTY arg). Same
#       leading-`PRE` probe: `[PRE]` alone confirms nothing followed.
run_sash 'array a=(); /usr/bin/printf "[%s]\n" PRE @{a:~}'
[[ $rc = 0 && $out = "[PRE]" && -z $err ]]; chk gs.20

# gs.21 `@{a:~}` non-empty → the array splats per-element
run_sash 'array a=(x y); /bin/echo @{a:~}'
[[ $rc = 0 && $out = "x y" && -z $err ]]; chk gs.21

# gs.22 `:~` (elide) takes NO word, like the scalar `${x:~}` — `@{a:~WORD}` is
#       a compile error (SB5033).
run_sash 'array a=(); /bin/echo @{a:~foo}'
[[ $rc != 0 && -z $out && $err == *SB5033* ]]; chk gs.22

# gs.23 `@{a:~}` to the `echo` BUILTIN is the same flatten hazard as `@{a:-}` →
#       SB5030 (external-only disposition; builtin splat is rejected).
run_sash 'array a=(x y); echo @{a:~}'
[[ $rc != 0 && $err == *SB5030* ]]; chk gs.23

# gs.24 the `@{a:~}` disposition is command-argument-only — a for-in source is
#       the bare `@a`, so `for f in @{a:~}` → SB5034, like `@{a:?}` (gs.18).
run_sash 'array a=(x y); for f in @{a:~}; do /bin/echo "$f"; done'
[[ $rc != 0 && $err == *SB5034* ]]; chk gs.24

# gs.25-30 brace-less array-splat dispositions (DG 2026-09-04): `@a:-` / `@a:?`
# / `@a:~` desugar to `@{a:…}`, exactly like the scalar `$x:…` sugar. Only
# `:-`/`:?`/`:~` after `@name` trigger; behaviour is identical to the braced
# form (gs.2–gs.24).
# gs.25 `@a:-WORD` empty → the single arg WORD
run_sash 'array a=(); /bin/echo @a:-none'
[[ $rc = 0 && $out = "none" && -z $err ]]; chk gs.25
# gs.26 `@a:-` (bare) empty → one EMPTY arg (parity with `@{a:-}` / gs.4)
run_sash 'array a=(); /usr/bin/printf "[%s]\n" PRE @a:-'
[[ $rc = 0 && $out = $'[PRE]\n[]' && -z $err ]]; chk gs.26
# gs.27 `@a:~` empty → ZERO args (the elide; parity with gs.20)
run_sash 'array a=(); /usr/bin/printf "[%s]\n" PRE @a:~'
[[ $rc = 0 && $out = "[PRE]" && -z $err ]]; chk gs.27
# gs.28 `@a:~` non-empty → the array splats per-element
run_sash 'array a=(x y); /bin/echo @a:~'
[[ $rc = 0 && $out = "x y" && -z $err ]]; chk gs.28
# gs.29 `@a:?` non-empty → splat (no abort)
run_sash 'array a=(one two); /usr/bin/printf "[%s]" @a:?e'
[[ $rc = 0 && $out = "[one][two]" && -z $err ]]; chk gs.29
# gs.30 brace-less ≡ braced: `@a:-none` and `@{a:-none}` agree
run_sash 'array a=(); /bin/echo @a:-none @{a:-none}'
[[ $rc = 0 && $out = "none none" && -z $err ]]; chk gs.30

# gs.31-35 DD#92: a bare `@args` splat to an EXTERNAL command is valid without a
# disposition — it implies the function takes ≥1 arg (the arity records a
# required index 0, so `tool @args` ≈ `: $1; tool @args`). A 0-arg call is
# SB5042; a general `@a` still needs a disposition (SB5032); for-in / `@{args:-}`
# keep their empty-safe behaviour.
# gs.32 a 0-arg call to such a function is too few args → SB5042 (compile-time,
# all backends). The arity floor makes the splat provably non-empty (no SB5032).
run_sash 'fn f { /usr/bin/printf "[%s]" @args }; f'
[[ $rc != 0 && -z $out && $err == *SB5042* ]]; chk gs.32
# gs.33 the exemption is `@args`-only: a general `@a` still requires a
# disposition (SB5032) — DD#69 has no static-non-empty exemption. (compile-time)
run_sash 'fn f { array a=(x); /usr/bin/printf "[%s]" @a }; f a'
[[ $rc != 0 && -z $out && $err == *SB5032* ]]; chk gs.33
# gs.31 bare `@args` to external, called with args → splats them (no disposition)
run_sash 'fn f { /usr/bin/printf "[%s]" @args }; f a b'
[[ $rc = 0 && $out = "[a][b]" && -z $err ]]; chk gs.31
# gs.34 for-in over `@args` imposes NO floor (0 iterations on empty) — a
# for-in-only function is still 0-arg callable.
run_sash 'fn f { for w in @args; do /bin/echo x$w; done }; f'
[[ $rc = 0 && -z $out && -z $err ]]; chk gs.34
# gs.35 `@{args:-}` still accepts 0 args (empty → zero args), unlike the bare
# `@args` floor — the disposition is the opt-in to 0-arg calls.
run_sash 'fn f { /usr/bin/printf "%s" @{args:-} }; f'
[[ $rc = 0 && -z $out && -z $err ]]; chk gs.35


# --------------------------------- an array splat in a scalar position (DD#69)
# `v=@a` is SB5025, but a test operand and a redirect target were never
# checked. They did not panic — they silently did the wrong thing, which is
# worse: `[[ @a = x ]]` was ALWAYS false, and `> @a` wrote to neither file.

# ice.14 a splat as a test operand is SB5025, not a silent false
run_sash 'array a=(x y); if [[ @a = x ]] { echo T } else { echo F }'
[[ $rc != 0 && -z $out && $err == *SB5025* ]]; chk ice.14

# ice.15 …including in a unary test
run_sash 'array a=(x); if [[ -n @a ]] { echo T }'
[[ $rc != 0 && -z $out && $err == *SB5025* ]]; chk ice.15

# ice.16 a splat as a redirect target is SB5025, not a silent misdirection
run_sash 'array a=(o1 o2); echo hi > @a'
[[ $rc != 0 && $err == *SB5025* ]]; chk ice.16

# gs.36 SB5030's own advice must be a form the compiler accepts. It used to
#       read `echo $(join @a)`, which is SB5062 — a diagnostic recommending a
#       rejected spelling (DD#100 line 119 flagged it; the cleanup was filed
#       into a task that closed without it). `join` takes the array by NAME.
run_sash 'array a=(x y); echo @a'
[[ $rc != 0 && $err == *SB5030* && $err == *'$(join a)'* && $err != *'join @a'* ]]; chk gs.36

# gs.37 …and the recommended form actually compiles and runs
run_sash 'array a=(x y); echo $(join a)'
[[ $rc = 0 && $out = "x y" && -z $err ]]; chk gs.37

set -e
tally
