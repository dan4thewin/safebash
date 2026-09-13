---
title: "Complexity gate: pure dispatchers with cc in (3×mean, 3×mean+15) cannot be exempted"
status: open
priority: P3
kind: decision
depends: []
gates:
  - "the gate's dispatcher exemption is reachable for a cc≈13 / cog=1 function, or the dead zone is accepted deliberately"
---

## The finding

The rubric in `notes/journal/108-cog-cc-rubric.md` says a **wide dispatcher**
— high cc, low cognitive — should be *accepted as-is, no refactor needed*.
The enforced gate implements that exemption as `delta ≤ -15`, where
`delta = cog - cc`.

Those two do not agree in a band just above the threshold.

A **pure** dispatcher has `cog = 1` (the match itself, no nesting). For such a
function `delta = 1 - cc`, so `delta ≤ -15` requires **`cc ≥ 16`**. The
rejection test fires at `cc > 3×mean_cc ≈ 12.1`. Everything in

```
12.1 < cc < 16   and   cog ≈ 1
```

is therefore a pure dispatcher by the rubric's own description, yet
**structurally un-exemptable by the gate** — the delta can never get low
enough, no matter how clean the function is.

It is also un-*refactorable* in the usual sense, because in this codebase the
width comes from an enum's variant count. An exhaustive match over 13
`ExprKind` variants costs `cc = 14` before a single line of logic. The only
ways down are a catch-all `_` (which is the hazard — see journal 284) or
splitting the match, which is what I ended up doing.

## It is not hypothetical

Nine functions sit in the band today, every one of them a dispatcher:

| function | file | cc | cog |
|---|---|---:|---:|
| `next_token` | `parse/lexer.rs` | 15 | 2 |
| `check_positionals_arith` | `validate.rs` | 13 | 3 |
| `arith_has_mutation` | `ir.rs` | 13 | 2 |
| `child_bodies_mut` | `ir.rs` | 13 | 1 |
| `child_bodies` | `ir.rs` | 13 | 1 |
| `absorb_text` | `parse/expr.rs` | 13 | 3 |
| `simple_escape` | `parse/lexer.rs` | 13 | 1 |
| `lift_int_func_calls` | `transform/functions.rs` | 13 | 3 |
| `assign_op_for` | `parse/arith.rs` | 13 | 1 |

Each will hit the same wall the next time anyone edits it. Given
grandfather-on-modify, that is exactly when the cost lands: adding one arm to
`ir.rs::child_bodies` will force its author to restructure a function nobody
thinks is complex.

## What happened on 2026-09-13

`$(<file)` added one `ExprKind` variant, which forced one new arm into three
dispatchers, all of which the gate then rejected:

| function | before | after fix |
|---|---|---|
| `expr_to_string` | cc 17, cog 5 | cc 9, cog 1 |
| `expr_to_value` | cc 13, cog 1 | cc 9, cog 1 |
| `resolve_expr` | cc 11, cog 7 | cc 10, cog 1 |

`resolve_expr` was a fair catch — cog 7 was real nesting (two `for` loops and
an `if let` inside arms), and extracting `resolve_each` genuinely improved it.
`expr_to_string` at cc 17 was a fair catch too.

`expr_to_value` was **not**. It was cc 13 / **cog 1** — thirteen arms, each a
single delegating call, zero nesting. There is nothing to simplify. I split
out `composite_to_value` purely to get the arm count down, and the result is
arguably worse: one more hop, and the helper needs a `_ => unreachable!()`
because Rust cannot express "the rest of the variants". The parent match stays
exhaustive so the safety property survives, but the indirection buys nothing
except a passing gate.

## Options

1. **Leave it.** The dead zone costs a split whenever one of nine functions is
   edited. Cheap per instance, but it pushes authors toward catch-alls, which
   is the opposite of what journal 284 argues for.
2. **Make the exemption relative:** `cog ≤ 2` (or `cog ≤ mean_cog`) exempts
   regardless of delta. A function with no nesting is a dispatcher whatever
   its width; delta is a proxy for that and the direct test is available.
3. **Scale the exemption with the threshold:** `delta ≤ -15` → `cog ≤ cc/4`,
   so the exemption tracks width instead of a fixed offset.
4. **Raise `3×mean_cc`** — no; that weakens the gate everywhere to fix a
   boundary artifact.

**Recommendation: option 2.** It states the rubric's own rule directly
("high cc, low cog → accept") instead of approximating it, and it is one
comparison. The risk is a genuinely-nested function with low cog escaping, but
cog is precisely the metric that measures nesting, so there isn't one.

Not acting unilaterally: the gate is DG's design and the formula is published
in journal 108.
