---
title: "join: support a dynamic (non-literal) separator"
status: ready
priority: P3
kind: feature
depends: []
gates:
  - "`$(join a \"$sep\")` joins with the VALUE of `sep`, in native and c"
  - "`$(join a \"$(cmd)\")` likewise"
  - "`$(join a)` still means a space; `$(join a \"\")` still concatenates"
  - "SB5074 is retired and its `jsep.1`-`jsep.3` specs are rewritten as positive cases"
---

# `join` with a dynamic separator

DD#106 made `join`'s separator a compile-time literal (SB5074) because
the SSA operand is a `String` and a dynamic separator had nothing to
lower to — it was being rendered as *source text*, a silent wrong
answer. Rejecting it was the bug fix; this is the feature DG deferred.

## Why it is worth doing

`sub`/`gsub` accept a dynamic operand in **every** slot — subject,
pattern and replacement. `join` accepting one nowhere is an asymmetry a
user has no way to predict, and the workaround (write the separator
literally) is not always available: a separator chosen by a `case` or
passed into a function has to be a variable.

## Shape of the work

The separator currently rides in `SsaValue::ParamExpansion.operand: String`,
which is what forces the restriction. Options, not yet weighed with DG:

- a new `SsaValue` variant carrying the separator as a *value*, leaving
  the literal path alone (no regression risk, two code paths);
- or widening the existing operand to a value and letting the literal
  case fold (one path, but touches every `ParamExpansion` consumer).

Consumers to update either way: `codegen::intern_join_separator`,
`emit_c::arrjoin_separator` / `emit_arr_join_loop`, `emit_ir`, and the
transform's `join_intrinsic`.

## Watch out

- **Do not reintroduce the empty/absent conflation.** DD#106's sibling
  bug: `$(join a)` (no separator → space) and `$(join a "")` (explicitly
  empty → concatenate) are different, and `emit_c` got this wrong by
  keying on operand emptiness. Whatever representation replaces the
  `String`, "absent" and "empty" must stay distinguishable.
- `ir::literal_word_text` exists to let a constant-only slot say "not a
  constant" instead of inventing source text. If `join` stops needing
  it, check whether any new caller wants it before deleting it.
- Retiring SB5074 means `jsep.1`-`jsep.3` flip from rejection specs to
  behaviour specs; `jsep.4`-`jsep.7` stay as they are.
