---
title: "@{a:~} — the elide disposition for external array splats (DD#69/#84)"
date: 2026-09-04
---

# `@{a:~}` array-splat elide

DG: "for array splat to command, we allow `:-` and `:?`. I think `:~` should
work too."

## Context

DD#69 requires a bare `@a` splat into an external command to carry an
empty-disposition: `@{a:-WORD}` (empty → single arg WORD; bare `@{a:-}` → zero
args) or `@{a:?WORD}` (empty → runtime abort). DD#84 added `:~` as the scalar
**elide** operator (`${x:~}` → empty disappears) — explicitly "the scalar analog
of the array-splat elision (`@a` / `@{a:-}` on an empty array → zero args)". But
the array-splat parser rejected `@{a:~}` (SB5033), so the elide operator worked
for scalars and not for the array form it was modeled on. DG wants the spelling
accepted for uniformity.

## Semantics

`@{a:~}` **elides**: empty → zero args, non-empty → splat per-element — behaviorally
identical to a bare `@{a:-}` / `@a`. Like the scalar `${x:~}` it takes **no
word**, so `@{a:~WORD}` is a compile error (SB5033).

## Implementation

One parser change: `parse/expr.rs::parse_at_brace_disposition` gains a `:~`
branch that rejects a trailing word and encodes as a **wordless
`arrvals_default`** — the exact encoding of bare `@{a:-}`. So `@{a:~}` flows
through validate, transform, and all three backends with zero new wiring (it *is*
an `arrvals_default` with no operand). The SB5033 "malformed" message and the
function doc now list `:~`.

Chose to reuse `arrvals_default` rather than mint an `arrvals_elide` operator:
the two are behaviorally identical, and a distinct operator would need arms in
validate/transform/codegen/emit_c/emit_janet for no observable difference. (A
reversible internal choice — reported for the record.)

## Behavior (all contexts match `@{a:-}`)

| input                                   | result           |
|-----------------------------------------|------------------|
| `array a=(); /bin/echo s @{a:~} e`      | `s e` (0 args)   |
| `array a=(x y); /bin/echo @{a:~}`       | `x y` (splat)    |
| `@{a:~WORD}`                            | **SB5033** (no word) |
| `echo @{a:~}` (builtin flatten)         | **SB5030**       |
| `for f in @{a:~}` (for-in source)       | **SB5034**       |

## Verification

- `glob_splat.sh` gs.20–24 (all 3 modes: `@{a:~}` works in native/c/janet since
  it is an `arrvals_default`). plan 19→24.
- Full 3-mode sweep, latest-run diff vs HEAD (`c7e0f62`): **0 regressions**, 15
  new passing (gs.20–24 ×3), 0 gone.
- `cargo fmt`/`clippy` clean.

DD#69 disposition list + DD#84 gained addenda. No new SB code (reuses SB5033).
