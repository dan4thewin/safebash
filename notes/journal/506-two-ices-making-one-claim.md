# 506 — Two ICEs making one claim, and the question that collapsed them

2026-10-12, same day as 505. DG read one sentence of mine and found the
contradiction in it.

## The sentence

About `emit_attest`'s `unreachable!()`, which I had left cold and untested:

> The *line* earns its place — it guards a second data path. But a unit *test*
> of it would assert the same sentence against the same named function, which
> is a restatement, not a backstop.

DG:

> if a unit test can't warm the line, is it supposedly protecting against a
> regression in the self-same function?

## Why that is fatal to the position, not a quibble

I was holding both halves of an exclusive or:

- if the ICE guards a genuinely different path, a test of it is **not** a
  restatement — different function, different state, different regression;
- if a test of it would be a restatement, the ICE makes the **same** claim,
  and "it guards a second data path" was wishful.

I had taken whichever half suited the artefact in front of me. And the second
half does not even survive on its own terms: `load_var`'s test protects
`load_var` from reversion. It does nothing whatever for `emit_attest`. So the
one regression this whole exercise is about — someone restoring a tolerant
fallback — was unguarded at exactly the site I had declared safe by analogy.

## The resolution was not to write the second test

The heavier harness was never the real obstacle; the duplication was. Two
sites were making one claim, so the claim moved into one function:

```rust
pub(super) fn var_slot(vars: &HashMap<String, VarSlot>, key: &str) -> VarSlot
```

`load_var` and `emit_attest` both call it. There is now **one** ICE line, one
sentence, and one test — and the test protects both callers, because both
callers route through the line it warms.

The tell that this was right: the test no longer needs a `FunctionBuilder` at
all. It is a map lookup. `codegen/test_support.rs`, the `with_builder` harness
I had built yesterday specifically to reach the guard behind the builder
borrow, became unused and was deleted. Scaffolding that exists only to reach
duplicated logic is a sign the logic should not be duplicated. Eleven lines to
re-add when something genuinely needs a builder.

This also lands on the right side of the rule I already had written down:
never fork a shared walker privately in a pass. Three tolerant answers to one
question — `load_var`'s `(null, 0)`, `emit_attest`'s zeros in the fail block,
and its silently skipped copy in the pass block — were three forks of a
decision that belongs in one place.

## What the test does and does not claim

Unchanged from 505, because that part was right: it does **not** assert the
invariant. An eleventh defining `SsaInst` arriving unmapped leaves it green
and the ICE fires in my own sweep. It catches the **reversion**, which the
spec sweep is structurally blind to — a silent empty is silent, so all 8367
programs stay green while the compiler goes back to feeding a null pointer to
`sash_table_len`.

Proved by reversion rather than asserted: replacing the `unreachable!` with a
tolerant arm turns the test red and moves nothing else in either gate.

## Residual, stated plainly

Rewriting `load_var` or `emit_attest` to *bypass* `var_slot` would not be
caught. That was equally true of the per-site tests, and there is now one
obvious place for a reviewer to look.

## Gates

Sweep **8367** — 4068 c + 13 skipped + 4286 native. Zero flips, zero new ids.
`mk test` **124** + 27 + 20. The restructure changes the release binary, so
both gates were re-run rather than reasoned about.
