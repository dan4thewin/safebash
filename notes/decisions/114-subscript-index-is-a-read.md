---
status: accepted
---

# DD#114 — a subscript index is a read, in every position

DD#24 already said this. Its use-site list names `arr[$x]=v` — "index
expression" — and its 2026-08-26 amendment says the list is
**position-uniform**: a read of tainted data is rejected wherever it appears,
naming the assignment RHS and the `[[ … ]]` operand explicitly.

`validate` did not implement it. All three taint walkers matched the container
and discarded the index:

```rust
ExprKind::Subscript { name, .. } => push_taint_use(tainted, name, …)
```

`..` is the whole bug.

## What that cost

| position | before | after |
|---|---|---|
| `/bin/echo "${a[$i]}"` | SSA backstop | SB5020 |
| `echo "${a[$i]}"` | SSA backstop | SB5020 |
| `printf '%s' "${a[$i]}"` | SSA backstop | SB5020 |
| `a[$i]=v` | SSA backstop | SB5020 |
| **`s="${a[$i]}"`** | **accepted** | SB5020 |
| **`[[ -n ${a[$i]} ]]`** | **accepted** | SB5020 |

The last two are a laundering bypass: an attacker-chosen index selected a cell
and the result was treated as clean, with no diagnostic anywhere.

The other four were *rejected*, but only by the SSA taint backstop — which
`notes/tasks/ssa-taint-full-backstop.md` describes as defense-in-depth behind
an authoritative CST gate. Its message is

```
c.sh: tainted variable i_1 used as argument to command 'echo' without attest (SSA taint backstop)
```

— no SB code, no line:col, and an SSA-internal name. Reaching it at all is the
symptom of a `validate` gap.

## Decision

The index is checked as a read by the same walker that checks the container,
in all three places (`check_expr_taint`, `reject_scalar_rhs_read`,
`expr_uses_tainted`), plus `taint_assignment` for the write form. It is
reported as an ordinary SB5020 read of the index variable.

The diagnostic names the **index** variable, not the container. In
`${m[$k]}` with a clean `m` and a tainted `k`, blaming `m` would send the
author to an `attest` that cannot help.

Note what is *not* being decided here: whether a tainted index makes the
retrieved value tainted. It does not arise — DD#24 has no propagation, and the
read of the index is rejected before any question about the result.

## Span

The fix immediately exposed a second defect. `parse_subscript_index` re-parses
the captured index text with a **fresh `Scanner`**, which numbers lines and
columns from the start of that substring, so every index expression carried a
1:1-relative span. The new diagnostic reported line 1 for an error on line 3.

`Span` is `{line, col}`, not a byte offset, so the sub-scanner cannot simply be
given a base. The index expression is re-anchored to the span of the enclosing
`${name[…]}` — which is where the read is. Exprs nested deeper inside an index
still carry relative spans; filed as
`notes/tasks/subscript-index-nested-span.md`.

## Why it was open

Zero existing specs used a tainted subscript index — the full sweep was
unchanged at 5071 by the fix. The surface was not covered, so nothing
disagreed with the code.
