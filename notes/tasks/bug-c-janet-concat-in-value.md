---
title: "BUG — c/janet backends don't lower a Concat value in non-echo position (assignments, subscript-assign)"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "c and janet lower SsaValue::Concat wherever it appears (assignment RHS, subscript-assign value), not only as an echo/command arg"
  - "`b=world; a=hello$b; echo $a` -> `helloworld` in c and janet"
  - "drop the in_modes native guard on spec types.sh 33.11-33.13 (subscript append)"
---

# BUG — c/janet Concat-in-value not lowered

## Symptom

In the c backend, `b=world; a=hello$b; echo $a` prints empty; janet
mis-handles it too. `echo hello$b` (Concat as a command arg) works — the
gap is Concat in *value* position.

## Root cause (c)

`emit_c.rs::lower_value_as_str` has no `SsaValue::Concat` arm — it falls
to the `_ => /* TODO */` branch and returns `("", 0)`. The Concat
command-arg path (emit_c.rs ~909) is the only place c concatenates. So
any value-position Concat (assignment RHS, `SubscriptAssign` value)
becomes empty. janet has an analogous gap (returns a wrong value).

## Where it bites

`subscript-append-broken`'s native fix lowers `a[i] += v` to
`SubscriptAssign { value: Concat([${a[i]}, v]) }`. native handles the
Concat; c/janet don't, so spec types.sh 33.11–33.13 are guarded
`in_modes native`. Fixing this here unblocks them.

## Fix sketch

- **c:** add a `SsaValue::Concat(parts)` arm to `lower_value_as_str` that
  builds the concatenation at runtime (allocate, copy each part's
  ptr/len), mirroring the command-arg concat or codegen's
  `lower_concat_value`. Likely a small runtime helper (`sash_concat`) or
  a temp-buffer emit.
- **janet:** ensure the janet value emitter concatenates Concat parts as
  strings in value position.

## Discovery

2026-06-20, fixing subscript append (read-modify-write produces a Concat
value). Pre-existing; broader than subscript-assign.
