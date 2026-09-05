---
title: "the args array is immutable and always clean (DD#92 addendum; SB5052)"
date: 2026-09-05
---

# `args` is immutable + always clean (SB5052)

DG: "could I ever pass a tainted arg to a function — I don't think so; passing the
arg counts as a read of the value."

## Confirming the model

Right, and verified in every form: a value passed to a function is read at the
call site, so a tainted one is **SB5020** unless blessed first:
- `f $x` / `f $(getenv HOME)` (tainted) → SB5020, **even when `f` never reads
  `$1`** (the passing *is* the read).
- a tainted **default** inside the body (`${1:-$(getenv HOME)}`) → SB5020 too.
- `f $x` after `bless x …` → compiles.

So the args a function **receives** are unconditionally clean. The taint system
already depends on this: `ssa_taint.rs::seed_clean_specials` seeds `$1..$9` /
`$@` / `$*` as `Clean` "by DD#24". DG's reasoning is exactly why that is sound.

## The one nuance → the decision

Args can be *locally re-filled* with tainted data inside the body — `argv 1 args`
(the tainted program command line) or a tainted `read -a args`. That is a local
write, not a passed arg, and was tracked normally — but it meant "args is always
clean" held only *as received*, not for its whole lifetime.

DG chose (A) to close that: make `args` **immutable**. Every write is now
**SB5052**:
- `args=…`, `args[i]=…`
- `read -a args`, `argv … args`, `mapfile args`
- `array args=…`
- `bless … in args` — a clean rejection now (was the `bug-bless-all-args-array-ice`
  codegen ICE), and meaningless: `args` can't be tainted.

Reading `args` is unaffected (`@args`, `${args[i]}`, `$1`, `$#`, `len args`, …).
Use a differently-named local array to `read`/`argv`/`mapfile` into.

## Implementation

`validate.rs` `check_args_immutable_stmt` — a recursive walk wired into
`validate()`, keyed on the write target: an `Assignment`/`Declaration` named
`args`, or a `read`/`argv`/`mapfile`/`bless` command whose array target is `args`
(`command_writes_args` + `argv_target_name` / `mapfile_target_name` /
`bless_array_target`, mirroring the transform's target extraction). SB5052 is a
new code.

## Verification

- `optional_args.sh` 84.25–84.30: each write form → SB5052; a differently-named
  local array still works (84.30, native,c — janet read-array gap). plan 24→30.
- Full 3-mode sweep: **0 regressions**, 18 new. fmt/clippy clean.

DD#92 gained the addendum; `bug-bless-all-args-array-ice` resolved by rejection.
