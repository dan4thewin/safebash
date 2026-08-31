---
title: "janet: optional args (`:-`/`:~`) and the `:~` elide operator (DD#84)"
status: ready
priority: P3
kind: backend-parity
depends: []
gates:
  - "janet `${args[i]:-D}` / `${args[i]:~}` optional trailing args work (arity [min,max])"
  - "janet `:~` elides an empty token from an arg list / array literal (native/c parity)"
---

## What

DD#84 (optional trailing args + the scalar elide operator `:~`) shipped native +
c. **janet is not covered** — two independent janet gaps:

1. **No `args`-array model.** `${args[i]}` on janet → `compile error: unknown
   symbol args` (janet uses its own `_args`; the args-as-a-named-array model of
   DD#75 §5 was never ported). So optional-arity via `${args[i]:-}`/`:~` can't
   work until janet has the args array. The arity *checks* (SB5042/5043/5047/
   5048) also live in the SSA transform (native/c); janet emits from the AST and
   does no arity checking.
2. **No `:~` elision.** janet lowers `${x:~}` to the plain value (the operator is
   ignored), so `x=; echo a ${x:~} b` emits `a  b` (empty arg) instead of `a b`.
   janet's argv/array construction needs the 0-or-1 conditional the native argv
   builder / `lower_indexed_array_value` and the c `emit_argv_splat` /
   `emit_indexed_array` got (store only when the value is non-empty).

## Do

- Port the `args` named-array model to the janet backend (prerequisite for the
  args half; a larger task in its own right — may split out).
- Handle `SsaValue::ElideEmpty` in janet's arg-list and array-literal emit:
  emit the value only when non-empty (`(when (not (empty? v)) …)`), compacting
  array indices.
- Un-gate the `native,c`-only checks in `spec/optional_args.sh` (84.1–12, 84.17,
  84.18) for janet once it works.

## Pointers

- Native reference: `codegen/commands.rs` `store_argv_entry_if_nonempty`,
  `codegen/values.rs` `lower_indexed_array_value`; c: `emit_c.rs`
  `emit_argv_splat` / `emit_indexed_array`.
- Specs: `spec/optional_args.sh` (DD#84, 84.x) — janet currently skips the
  positive checks.
- DD: `notes/decisions/DD84-optional-args-elision.md`.

## Found

2026-08-31, implementing DD#84 (native + c). Journal ~218.
