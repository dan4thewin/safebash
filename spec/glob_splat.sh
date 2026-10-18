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
plan=81
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
# gs.11 WORD is a literal, so an expansion in it is SB4070
run_sash 'p=hi; array a=(); /bin/echo @{a:-$p}'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk gs.11

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
run_sash 'array a=(x y); if [[ @a == x ]] { echo T } else { echo F }'
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

# gs.38 a DD#69 `@{…}` disposition with NO colon at all → SB5033. cov.17 pins
#       `@{a:x}` (a colon with a bad operator), which takes a different branch;
#       the missing-colon arm had no spec.
run_sash 'array a=(1)
echo @{a}'
[[ $rc != 0 && -z $out && $err == *SB5033* ]]; chk gs.38

# gs.39 an UNCLOSED `@{` named the wrong sigil: "unterminated `${`", pointing
#       the reader at something they had not written. The brace scan is shared
#       with `${…}`, and it hardcoded the `$` spelling.
run_sash 'array a=(1)
echo @{a:-W'
[[ $rc != 0 && -z $out && $err == *SB4057* && $err == *'`@{`'* ]]; chk gs.39


# gs.40 a splat JOINED to adjacent text was silently collapsed into ONE
#       argument: `@{a:-x}y` on a=(p q) emitted `p qy`, where bash emits TWO
#       arguments — `p` and `qy` (the suffix attaches to the last element).
#       The argv/keys splats were already SB5025 ("it cannot be joined into a
#       scalar"); the named-array family was the one form still silent. DG
#       ruled the form out rather than matching bash's attach-to-last shape.
run_sash 'array a=(p q); /usr/bin/printf "[%s]\n" @{a:-x}y'
[[ $rc != 0 && -z $out && $err == *SB5083* ]]; chk gs.40

# gs.41 the elide disposition joins no differently
run_sash 'array a=(p q); /usr/bin/printf "[%s]\n" @{a:~}y'
[[ $rc != 0 && -z $out && $err == *SB5083* ]]; chk gs.41

# gs.42 …nor the abort disposition
run_sash 'array a=(p q); /usr/bin/printf "[%s]\n" @{a:?o}y'
[[ $rc != 0 && -z $out && $err == *SB5083* ]]; chk gs.42

# gs.43 the adjacent text need not be a literal — an expansion glued to the
#       splat is the same collapse (`p qS`, one argument)
run_sash 'array a=(p q)
str s = "S"
/usr/bin/printf "[%s]\n" @{a:-x}$s'
[[ $rc != 0 && -z $out && $err == *SB5083* ]]; chk gs.43

# gs.44 CONTROL: the splat alone still splats per-element. The rejection must
#       key on adjacency, not on the disposition itself.
run_sash 'array a=(p q); /usr/bin/printf "[%s]\n" @{a:-x}'
[[ $rc = 0 && $out = $'[p]\n[q]' && -z $err ]]; chk gs.44
# gs.45 a non-empty array ignores the WORD entirely (literal now, but the
# point is that it is not used at all)
run_sash 'array a=(p q)
/usr/bin/printf "[%s]\n" @{a:-S}'
[[ $rc = 0 && $out = $'[p]\n[q]' && -z $err ]]; chk gs.45

# ── prefix adjacency (DD#69) ──────────────────────────────────────────────
# `y@{a:-x}` used to scan `y@` as a word and let the already-breaking `{` open
# a brace GROUP, so the splat was reported as `SB5011 command 'a:-x' not
# found`. The prefix spelling collapses exactly as the suffix one does and now
# reaches the same check.

# gs.46 prefix text glued to a splat is the same collapse as a suffix
run_sash 'array a=(p q); /usr/bin/printf "[%s]\n" y@{a:-x}'
[[ $rc != 0 && -z $out && $err == *SB5083* ]]; chk gs.46

# gs.47 the elide disposition too — adjacency is what is rejected, not the op
run_sash 'array a=(p q); /usr/bin/printf "[%s]\n" y@{a:~}'
[[ $rc != 0 && -z $out && $err == *SB5083* ]]; chk gs.47

# gs.48 text on BOTH sides
run_sash 'array a=(p q); /usr/bin/printf "[%s]\n" p@{a:-x}s'
[[ $rc != 0 && -z $out && $err == *SB5083* ]]; chk gs.48

# gs.49 a prefixed `@{a}` reports the missing disposition (SB5033), exactly as
#       the bare `@{a}` does — not "command 'a' not found"
run_sash 'array a=(p q); /usr/bin/printf "[%s]\n" y@{a}'
[[ $rc != 0 && -z $out && $err == *SB5033* ]]; chk gs.49

# gs.50 CONTROL: `@` is a word break only before `{`, so an address is still
#       one ordinary word. This is the regression guard for the new break.
run_sash '/usr/bin/printf "[%s]\n" user@example.com'
[[ $rc = 0 && $out = '[user@example.com]' && -z $err ]]; chk gs.50

# gs.51 CONTROL: a bare `@` mid-word is untouched (no brace follows)
run_sash '/usr/bin/printf "[%s]\n" a@b'
[[ $rc = 0 && $out = '[a@b]' && -z $err ]]; chk gs.51

# gs.52-55 DD#90 addendum (DG 2026-09-20): a BRACE-LESS `@` opens a splat only
# at the START of a word. Mid-word it is an ordinary byte, as in bash. This is
# deliberately asymmetric with the braced form (gs.46-51: `y@{a:-x}` is
# SB5083) — braces are an explicit request for splat machinery, a bare `@` is
# not. The deciding case is `user@host:~`, which matches the brace-less
# trigger (`@name` + `:~`) exactly and is the scp/rsync/ssh remote-path
# spelling; a prefix rule would try to splat an array named `host`.
# gs.52 a prefixed brace-less disposition is LITERAL text, matching bash
run_sash 'array a=(p q)
echo $(join a)
/usr/bin/printf "[%s]\n" y@a:-z'
[[ $rc = 0 && $out = "p q
[y@a:-z]" && -z $err ]]; chk gs.52
# gs.53 the scp/rsync target spelling survives intact
run_sash '/usr/bin/printf "[%s]\n" user@host:~/dir me@box:~'
[[ $rc = 0 && $out = "[user@host:~/dir]
[me@box:~]" && -z $err ]]; chk gs.53
# gs.54 CONTRAST: word-leading, the same brace-less disposition DOES splat
run_sash 'array a=(p q); /usr/bin/printf "[%s]\n" @a:-z'
[[ $rc = 0 && $out = "[p]
[q]" && -z $err ]]; chk gs.54
# gs.55 a prefixed brace-less `@name` with no disposition is literal too
run_sash 'array a=(p q)
echo $(join a)
/usr/bin/printf "[%s]\n" y@a'
[[ $rc = 0 && $out = "p q
[y@a]" && -z $err ]]; chk gs.55

# gs.56-gs.58 use `/bin/echo`, not the `echo` builtin: a splat into `echo` is
# SB5030 whatever the message says, so the builtin would let these pass for the
# wrong reason.
# DD#69's `@{a:?WORD}` message obeys the same rule as the scalar
# `${x:?MESSAGE}` (SB4070). It did NOT: `optional_splat_word` called
# `parse_default_word`, so the array form was a full interpolating word — and
# therefore a live user-function call site — for as long as the scalar form was
# a literal. `@{a:?$(g)}` ran `g` and used its return value as the message.
# gs.56 a `$name` in a splat abort message is SB4070
run_sash 'array a=()
/bin/echo @{a:?$m}'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk gs.56
# gs.57 and `$( )` — the call site that made this worth fixing. `g` is
#       undefined, so a word parse could not compile at all.
run_sash 'array a=()
/bin/echo @{a:?$(g)}'
[[ $rc != 0 && -z $out && $err == *SB4070* ]]; chk gs.57
# gs.58 the single-quote escape works here too, quotes stripped
run_sash 'array a=()
/bin/echo @{a:?'"'"'costs $5'"'"'}'
[[ $rc != 0 && -z $out ]] \
  && { ! in_modes native || [[ $rerr == *": costs \$5" ]]; }; chk gs.58

# ── A DOUBLE-QUOTED `@a` is literal text, not a splat: inside `"…"` the `@` is
#    not an expansion sigil, so the parser yields a plain literal and no splat
#    node ever exists. Every other way of quoting a splat was already caught
#    (`"${a[@]}"` SB4038, `"$(keys m)"`/`"$(argv)"` SB5025/SB5026) — this
#    spelling was the silent hole, and the most natural one to write. Rejected
#    only in a for-in SOURCE, where a one-iteration loop over a constant string
#    is obviously not what was meant (DG, 2026-09-30). As a command argument it
#    stays legal literal text, matching bash.
# gs.59 a quoted splat as a for-in source is rejected
run_sash 'array g=(a b)
echo $(len g)
for v in "@g" { echo "[$v]" }'
[[ $rc != 0 && -z $out && $err == *SB5026* ]]; chk gs.59
# gs.60 the message names BOTH remedies — the bare splat and the literal form
run_sash 'array g=(a b)
echo $(len g)
for v in "@g" { echo "[$v]" }'
[[ $rc != 0 && $err == *'`@g`'* && $err == *"'@g'"* ]]; chk gs.60
# gs.61 the advice must itself be LEGAL here: a for-in source rejects the
#       empty-disposition (SB5034), so `"@{g:-x}"` is advised back as `@g`,
#       never as `@{g:-x}`
run_sash 'array g=(a b)
echo $(len g)
for v in "@{g:-x}" { echo "[$v]" }'
[[ $rc != 0 && $err == *'`@g`'* && $err != *'`@{g:-x}`'* ]]; chk gs.61
# gs.62 `:!` is a MODIFIER, legal in this position (DD#91), so it is carried
#       over into the advice instead of being stripped
run_sash 'array g=(a b)
echo $(len g)
for v in "@{g:!}" { echo "[$v]" }'
[[ $rc != 0 && $err == *'`@{g:!}`'* ]]; chk gs.62
# gs.63 single quotes are the documented escape — one literal iteration
run_sash 'array g=(a b)
echo $(len g)
for v in '"'"'@g'"'"' { echo "[$v]" }'
[[ $rc = 0 && $out == "2
[@g]" && -z $rerr ]]; chk gs.63
# gs.64 a quoted `@a` as a command ARGUMENT is untouched literal text (bash
#       prints `@g` too) — the rejection is scoped to the for-in source
run_sash 'array g=(a b)
echo $(len g)
echo "[@g]"'
[[ $rc = 0 && $out == "2
[@g]" && -z $rerr ]]; chk gs.64
# gs.65 words that merely contain `@` are unaffected
run_sash 'for v in "a@b" "@" "@{}" { echo "[$v]" }'
[[ $rc = 0 && $out == "[a@b]
[@]
[@{}]" && -z $rerr ]]; chk gs.65

# Joins are strings, not fixed-size scratch buffers.
join_payload() {
    printf -v join_text '%*s' "$1" ''
    join_text=${join_text// /x}
}

join_payload 4095
run_sash "array a=('$join_text'); echo \$(join a)"
[[ $rc = 0 && $out = "$join_text" && -z $err && -z $rerr ]]; chk joinbuf.1

join_payload 4096
run_sash "array a=('$join_text'); echo \$(join a)"
[[ $rc = 0 && $out = "$join_text" && -z $err && -z $rerr ]]; chk joinbuf.2

join_payload 4097
run_sash "array a=('$join_text'); echo \$(join a)"
[[ $rc = 0 && $out = "$join_text" && -z $err && -z $rerr ]]; chk joinbuf.3

join_payload 8192
run_sash "array a=('$join_text'); echo \$(join a)"
[[ $rc = 0 && $out = "$join_text" && -z $err && -z $rerr ]]; chk joinbuf.4

run_sash "array a=(a b); echo \$(join a '$join_text')"
[[ $rc = 0 && $out = "a${join_text}b" && -z $err && -z $rerr ]]; chk joinbuf.5

# A later join cannot overwrite an earlier result's backing storage.
run_sash "array a=('$join_text'); array b=(kept value)
str x=\$(join a)
str y=\$(join b ,)
echo \"\$x|\$y\""
[[ $rc = 0 && $out = "$join_text|kept,value" && -z $err && -z $rerr ]]; chk joinbuf.6

run_sash 'int map m=([low]=-9223372036854775808 [drop]=17 [high]=9223372036854775807 [zero]=0)
unset m[drop]
echo $(join m ,)'
[[ $rc = 0 && $out = '-9223372036854775808,9223372036854775807,0' \
   && -z $err && -z $rerr ]]; chk joinbuf.7

run_sash 'array a=($'"'"'a\x00b'"'"' c)
str s=$(join a $'"'"'\x00:'"'"')
echo "$s" > joined-bytes'
bytes=$(od -An -tx1 joined-bytes | tr -d ' \n')
[[ $rc = 0 && $bytes = 610062003a630a && -z $err && -z $rerr ]]; chk joinbuf.8

# A joined string also obeys DD#87's trailing-NUL invariant at an argv boundary.
run_sash "array a=('$join_text' tail)
/usr/bin/printf '%s\n' \"\$(join a ,)\""
[[ $rc = 0 && $out = "$join_text,tail" && -z $err && -z $rerr ]]; chk joinbuf.9

run_sash 'array a=(); echo "<$(join a)>"'
[[ $rc = 0 && $out = '<>' && -z $err && -z $rerr ]]; chk joinbuf.10

run_sash 'array a=("" q ""); echo "<$(join a)>"'
[[ $rc = 0 && $out = '< q >' && -z $err && -z $rerr ]]; chk joinbuf.11

run_sash 'array a=("" q ""); echo "<$(join a "")>"'
[[ $rc = 0 && $out = '<q>' && -z $err && -z $rerr ]]; chk joinbuf.12

join_payload 4096
SASH_CFLAGS="${SASH_CFLAGS:-} --heap-size=8K" run_sash "array a=('$join_text' '$join_text' '$join_text' '$join_text' '$join_text' '$join_text' '$join_text' '$join_text' '$join_text' '$join_text')
echo \$(join a)"
[[ $rc = 1 && -z $out && -z $err \
   && $rerr == *'out of memory (requested 40970 bytes)'* ]]; chk joinbuf.13

set -e
tally
