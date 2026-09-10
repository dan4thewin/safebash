---
title: "the :! taint-off operator + $(…) brace-less operands (DD#91)"
date: 2026-09-04
---

# `:!` taint-off operator, and `$( )` brace-less operands (DD#91)

DG, two requests: "yes, `$( )` too [in brace-less disposition operands]; and I'd
like a new operand like `$foo:!` which is equivalent to `:~` but also disables
the taint guard — I see it as a compromise between full bless use and the
`--no-taint` full abnegation."

## `$( )` brace-less operands (the small half)

The DD#90 brace-less operand scanner previously fell back to requiring braces for
a `$( )` / `$(( ))` / backtick operand. Now it scans them by balanced depth
(`skip_paren_to_close` skips quoted regions so a `)` inside `'…'`/`"…"` doesn't
miscount; `skip_backtick_to_close` for backticks). So `$1:-$(getpid)`,
`$x:-$((2+3))` work brace-less, matching the braced semantics exactly (an
external `$(…)` capture is still tainted → SB5020, e.g. `$1:-$(/bin/echo Z)`).

## `:!` — a per-use bless-without-predicate (the security half)

Design settled with DG first (security-sensitive — a sanctioned validator
bypass). Via ask_user: scope **both scalar and array, braced + brace-less**;
`:!` produces a **clean, propagating** value, **elidable-position-only like
`:~`**; audit — not `--manifest`, but **file a task for a broader bless/taint
audit report** (`audit-taint-bypass-report`).

`:!` = `:~` (elide) + taint guard off. `${x:!}` uses a tainted `x` without a
`bless` predicate; the value is clean and doesn't re-taint downstream. Wordless
(SB4051/SB5033), elidable-position-only (SB5046 in a scalar assignment, same as
`:~`).

### The dual-pass subtlety

`:~` and `:!` both lower to elide, so the SSA must distinguish them or the SSA
backstop (which re-derives taint) would ICE on `:!`. I added:
- scalar: `SsaValue::ElideEmpty { source, taint_off }` (was a tuple) — `taint_off`
  set only for `:!`.
- array: `ArraySplatDisposition.taint_off` (operator `arrvals_trust` for `@a:!`).

Both taint passes honor it: the CST pass (`validate.rs`, authoritative) skips the
use-site check and treats the value clean (`check_param_expansion_taint`,
`expr_uses_tainted`, `check_elide_pexp`); the SSA backstop (`ssa_taint.rs`)
classifies the trusting elide/splat Clean. The backstop doesn't yet check these
arg positions (`ssa-taint-full-backstop`), so its flag is forward-compatible; the
CST gate enforces the bypass today. Verified with `--no-taint-cst` that `:!` does
not ICE the backstop.

### Behavior (verified all 3 backends)

| form | result |
|------|--------|
| `x=$(getenv HOME); echo ${x:!}` | runs (bypass) — `${x:~}` would SB5020 |
| `echo $x:!` (brace-less) | same bypass |
| `array a=(${x:!} lit); echo @a:?e` | `a` clean, splat runs (propagation) |
| `argv 1 a; printf %s @a:!` | tainted array splat bypassed |
| `${x:!foo}` / `@{a:!x}` | SB4051 / SB5033 (no word) |
| `z=${x:!}` | SB5046 (elidable-only, like `:~`) |
| `echo @{a:!}` / `for f in @{a:!}` | SB5030 / SB5034 (as any disposition) |

## Backends

native/c see the SSA `ElideEmpty`/`ArraySplat` (taint_off ignored at codegen —
the runtime splat is identical to a wordless `arrvals_default`). janet's
`janet_splat_splice` gained an `arrvals_trust` arm. Scalar `${x:!}` already flows
through janet's elide path.

## Verification

- `taint.sh` 91.1–91.11 (bypass, controls that `:~`/`@a:?` still SB5020, no-word,
  elidable-only, clean-var no-op). getenv-executing tests (91.1/91.3/91.4) gated
  native,c — janet lacks `getenv` (pre-existing). plan 118→129.
- `var_modifiers.sh` vm.bl13–15 (`$( )`/`$(( ))` operands; external-capture still
  SB5020). vm.bl13 gated native,c (janet positional-default bug). plan 69→72.
- Full 3-mode sweep vs HEAD (`05dda23`): **0 regressions**, 42 new (38 passing, 4
  janet-gated skips). Rust taint_spec 25 / e2e 47 = baseline (0 new failures).
  fmt/clippy clean.

DD#91 written. Tasks filed: `audit-taint-bypass-report`. No new SB code.
