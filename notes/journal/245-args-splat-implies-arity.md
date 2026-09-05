---
title: "a bare @args splat to an external command implies min arity ≥1 (DD#92)"
date: 2026-09-05
---

# `@args` external splat ⟹ min arity ≥1 (DD#92)

DG: "we have arity checking for functions. we should be able to forego the SB5032
or `@args:?` modifier for args in a command within a function when the arity
check knows there's always at least one argument. … treat `fn f { tool @args }`
as if it were `fn f { : $1; tool @args }`."

## The idea

DD#69 makes a bare `@a` splat into an external command a compile error (SB5032) —
an empty array would run the command with too few args — unless it carries a
disposition (`@{a:-WORD}`/`@{a:?WORD}`). For the function-args array that is
heavy: `tool @args` almost always means "my args, of which there's ≥1".

DG's key insight (which is also what makes it *sound*): don't just waive the
check — **raise the arity**. A bare `@args` external splat records a **required
index 0** (as if the body read `$1`), so the function requires ≥1 arg. Then:
- the splat is provably non-empty → SB5032 waived;
- a 0-arg call is rejected at compile time (SB5042, existing arity enforcement);
- soundness comes from the call-site check, not a runtime abort.

Merely waiving SB5032 without the floor would be *unsound* — `fn f { tool @args }`
would be 0-arg-callable and run the external with nothing (the very bug SB5032
guards). The floor is essential.

## Rule + scope

A bare `@args` (operator `arrvals`, name `args`) in an **external** command's args
(post-DD#23 absolute-path head):
- validate `check_external_splat` skips SB5032 (via `bare_arrvals_splat_name`).
- transform `analyze_arity` (`sig_scan_stmt` + `is_external_head` /
  `is_bare_args_splat`) does `sig.required.insert(0)` → variadic, `floor ≥ 1`.

Both key on the *same* predicate (bare `@args` in an external command, both
seeing the resolved head), so they can't drift.

Scope (all verified):
- **`@args` only** — a general `@a` keeps SB5032 (DD#69's no-static-exemption
  stands; only `args` has an arity guarantee).
- **External only** — for-in `@args` (0 iterations on empty) and builtin `@args`
  impose no floor; a for-in-only function stays 0-arg-callable.
- **Top level unaffected** — `@args` is SB5024 there (function-local), so the
  rule only applies in functions.
- **Dispositions still mean "accept 0 args"** — `@{args:-}` (empty → zero args)
  and `@{args:?}` (runtime abort on empty) impose no floor; use them when the
  function should accept 0 args, bare `@args` when it shouldn't.

## Verification

- `glob_splat.sh` gs.31 (`tool @args` splats), gs.32 (0-arg call → SB5042), gs.33
  (general `@a` still SB5032), gs.34 (for-in `@args` 0-arg ok), gs.35 (`@{args:-}`
  0-arg ok). gs.31/34/35 native,c — **janet's `@args` splat is unimplemented**
  ("unknown symbol args", a PRE-EXISTING gap; `@{args:?}` fails there too). plan
  30→35.
- Full 3-mode sweep: **0 regressions**, 15 new (12 passing, 3 janet skips). Rust
  e2e 47 / codegen ~81 = baseline. fmt/clippy clean.

## Found alongside (separate, filed)

`bless x in args` (bless-all over the function args array) ICEs at
`codegen/types.rs:188` — pre-existing (reproduces with this change stashed and
without any splat), filed `bug-bless-all-args-array-ice`.

DD#92 written. No new SB code (reuses SB5042/SB5032/SB5024).
