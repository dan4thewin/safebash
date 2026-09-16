---
title: Trace the `arrvals` path through the C backend's join
status: open
priority: P2
kind: investigate
depends: []
gates:
  - "either a spec drives `arrjoin_separator` with a non-`arrjoin` operator, or the `arrvals` alternative is removed from `emit_c.rs`'s join dispatch"
  - "the answer is a trace through the passes, not an inference from a green sweep"
---

# Is `arrvals` a second ghost in the C backend's join?

## The observation

Branch coverage at 2026-09-16 (journal 335):

```
emit_c.rs
4036|0|0|33     ← if operator == "arrjoin"
4036|0|1|0      ← else  { " " }
```

`arrjoin_separator` is called 33 times and the operator is `"arrjoin"` every
single time. But its one live caller is reached from

```rust
"arrvals" | "arrjoin" => emit_arr_values_joined(e, operator, &s_ptr, operand),
```

so on paper `"arrvals"` should arrive too. It never does.

## Why this is worth a trace and not a guess

This is the same shape as the `arrkeys` ghost closed in journal 335: a match arm
naming an operator that nothing delivers. But the conclusion there took two
attempts — the first static argument for deleting the sibling `DoubleQuoted`
arms was **wrong**, and only tracing to the parse-time SB4038 made it sound.

So the question here is not "can I delete the `arrvals` alternative" but **what
happens to `@a` in the C backend instead**. Plausible answers, none verified:

- a bare `@a` splat is lowered to multiple *arguments* rather than a joined
  string, so it never becomes a `ParamExpansion` in value position;
- it is rewritten to `arrjoin` upstream in the transform;
- it reaches a different `emit_c` path entirely.

The first is most likely — splat-into-command is argument expansion, and
`$(join a)` is the joined-string spelling, which is `arrjoin`. If so, `"arrvals"`
in that match is dead for a *structural* reason worth writing down, and the same
question should be asked of `codegen/value_handlers.rs`'s native join dispatch,
which lists `arrvals` alongside `arrjoin` in exactly the same way.

## Do not conclude from the sweep

`$(join a)` and `@a` are both well specced and the sweep is green, which is
precisely why this needs a trace: green says the live paths work, not that the
dead ones are dead. See journal 334.

## Found

2026-09-16, while purging the `arrkeys` ghost. Reported rather than acted on —
the session had already spent its careful-verification budget on `arrkeys`, and
guessing at the end of a session is how the first `DoubleQuoted` argument went
wrong.
