---
title: "for-in over a `$( )` capture was lowered as a pipeline — three miscompiles"
status: done
priority: P1
kind: bug
depends: []
gates:
  - "`for k in $(echo a; echo b)` does not silently print only `b`"
  - "`for k in $(echo a | cat)` does not silently run zero iterations"
  - "`fn g { echo a }` + `for k in $(g)` does not abort the compiled program in `sash_exec`"
  - "`for k in $(< file)` does not bind the whole file as one iteration"
  - "`for x in $(argv …)` and `for k in $(keys m)` keep working"
---

## Found

2026-10-02, mining branch coverage in `transform/control_flow.rs`. The lead was
`detect_array_iter`'s cold `DoubleQuoted` arm; chasing why it was cold turned up
`try_dispatch_cmdsubst` next door.

## Symptom

`try_dispatch_cmdsubst` rewrote `for x in $( … )` into `cmd | while readline x`,
building **one pipeline stage per statement** in the capture body. Three ways
that goes wrong, all silent:

| spelling | bash | sash | why |
|---|---|---|---|
| `for k in $(echo a; echo b)` | `a` `b` | **`b`** | `;` lowered as `\|` — `echo a \| echo b` |
| `for k in $(echo a \| cat)` | `a` | **nothing** | a `Pipeline` stmt is not `StmtKind::Command`, so `build_cmdsubst_stage` returned `None` and `build_pipeline_stages_from_cmdsubst` dropped it |
| `for k in $(g)`, `g` a user fn | `a` | **rc=127** | `expr_to_string(&c.name)` with no DD#23 resolution — the bare name reached `sash_exec` |

The first two compiled clean and produced a wrong answer. The third compiled
clean and aborted at runtime with `sash: bug: bare command passed to sash_exec`.

The same capture in **assignment** or **argument** position was correct
throughout (`str s=$(echo a; echo b)` → `a\nb`), so this was the for-in path
alone. The user-function case is the same class as
`bug-fn-capture-in-test-operand-ice` — a third position that never upheld the
DD#81 lifting invariant.

## Cause

The guard was the wrong polarity. `for_source_scalar_cmdsubst` **allowed by
default**: it rejected only `is_scalar_value_intrinsic` and let everything else
fall out of the bottom as a collection. Every external command, builtin,
pipeline, statement sequence and user function therefore arrived at a lowering
that treated the capture as an iterable "line stream".

There is no line stream in sash's value model. DD#28 eliminates word splitting,
so splitting a capture on newlines is the same thing wearing a hat — and it did
not even match bash, which splits `a b c` into three iterations where sash ran
one. `cov.61` pinned that single iteration and called it "a deliberate
divergence from bash".

## Resolution

2026-10-02. DG's call: a `$( )` capture is one value, exactly as `v=$( … )`
binds one, so this is a for-loop over a single value — which SB5009 already
rejects. See the DD#28 addendum.

- `resolve.rs`: `is_scalar_value_intrinsic` → **`is_collection_intrinsic`**,
  stated in the positive so the rule denies by default. The exception set is
  `argv` and `keys`.
- `validate.rs`: `for_source_scalar_cmdsubst` inverted; `ExprKind::SlurpFile`
  no longer exempt; SB5009's message extended to name the replacements
  (`cmd | while readline x { … }`, `read -a arr`).
- `control_flow.rs`: `try_dispatch_cmdsubst`,
  `build_pipeline_stages_from_cmdsubst`, `build_cmdsubst_stage` and
  `emit_cmdsubst_pipe_while_read` deleted (104 lines), with the `lower_for` arm.

An `$(argv …)` splat among several statements is left to **SB5025**, whose
complaint (the other statements would be silently discarded) is the specific
one and which DG settled 2026-09-29. `keys` has no sibling rule, so a
multi-statement `$(keys m; …)` takes the general SB5009 case.

Specs: `control_flow.sh` fs.8 flipped from pass to rejection, fs.10–fs.13 added
for the four shapes. `coverage.sh` cov.61 retired — it covered the three
deleted functions, and carried a c-backend EOF divergence that went with them.
