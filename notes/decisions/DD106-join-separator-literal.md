title: DD#106 — `join`'s separator is a compile-time literal
status: accepted
depends: [DD75]

1. `join` takes a literal separator word; expansions are errors, never
   source text substituted as a separator.
2. An omitted separator means one space. An explicitly empty separator
   means concatenation with no intervening bytes.
3. Validation and lowering must share the same literal-word interpretation.

---

Settled with DG 2026-09-13.

`$(join arr SEP)` requires `SEP` to be a **literal word**. An expansion
there is **SB5074**.

```sh
array a=(x y)
sep=-
echo $(join a $sep)      # SB5074
echo $(join a "$sep")    # SB5074
echo $(join a -)         # x-y
echo $(join a ", ")      # x, y
echo $(join a "")        # xy     — explicitly empty, means concatenate
echo $(join a)           # x y    — no separator, means a space
```

## Why this is a decision and not just a restriction

It was found as a **bug**, and a bad one. `join_intrinsic` extracted the
separator with a helper whose fallback arm rendered the argument's
*source text*, so:

```sh
echo $(join a "$sep")    # printed  x"$sep"y
```

Not a crash — a wrong answer, with the quote characters included. The
separator lives in `SsaValue::ParamExpansion.operand`, a compile-time
`String`, so a dynamic separator had nothing to lower to and the helper
invented something plausible instead.

DG, on being shown the helper: *"does `strip_word_quotes()` have a
legitimate use? I would have expected a general string interpolation
call if there were double quotes."*

Right — and that reframed the fix. Merely *diagnosing* the bad case
would have left the source-text fallback in place for the next caller.
So the helper was replaced by `ir::literal_word_text`, which returns
**`Option<String>`** and has no fallback at all. A slot that requires a
compile-time constant can now say *"this is not one"* rather than
fabricate one. The bug class is gone, not reported.

The validator (`flag_join_separator`) rejects exactly the `None` cases,
so the transform's `None` arm is an `unreachable!` backstop guarded by a
stated invariant — the same shape as the existing `join` arity
invariant.

## Why reject rather than support

DG chose **reject now, support later** over implementing a dynamic
separator immediately. The measured position:

- Rejecting costs **nothing today**: every `join` in `spec/` uses a
  literal separator, and `sash-convert` never emits a dynamic one.
- Supporting it is a real feature — a new SSA shape carrying a value,
  touching `ssa.rs`, the transform, `codegen` (`intern_join_separator`),
  `emit_c` (`arrjoin_separator`) and `emit_ir`.
- The asymmetry with `sub`/`gsub`, which *do* accept dynamic operands in
  every slot, is genuine and is the argument for eventually doing it.

A silent wrong answer should not survive while a feature is designed.
Filed as `join-dynamic-separator`; if it lands, SB5074 is retired.

## The sibling bug this exposed

Pinning the empty separator with a spec immediately surfaced a
**backend divergence**: `$(join a "")` concatenated under native but
joined with spaces under `--emit-c`.

`emit_c::arrjoin_separator` keyed on *"is the operand empty"*, which
conflates two different things — `$(join a)`, which has no separator
and means a space, and `$(join a "")`, which has an explicitly empty one
and means straight concatenation. Native keyed on the *operator*
(`arrjoin` uses its operand; `arrvals`/`arrkeys` use a space), which is
correct. The c backend now does the same.

A sentinel value that cannot distinguish "absent" from "empty" is the
whole bug. `Option` at the front end, operator dispatch at the back.

## Consequences

- `ir::literal_word_text` — shared by validator and transform so the two
  cannot drift; `transform::strip_word_quotes` deleted.
- `validate::check_join_arity` renamed **`check_join_form`**: it now
  checks arity (SB5040) *and* separator (SB5074), so the old name lied.
- Specs `jsep.1`-`jsep.7` in `spec/errors.sh`.
