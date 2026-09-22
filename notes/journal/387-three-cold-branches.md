---
title: "Three cold branches that were never dead"
date: 2026-09-21
---

# 387 — Three cold branches that were never dead

`build_expr_from_parts` (`parse/parser.rs`) had three branches. The lcov said
one ran 48 times and the other two ran zero. Two zero-count branches in a
15-line function is the shape of dead code, and the call-site reading agreed:
`extract_from_concat` slices a `Concat` around a middle index `i` drawn from
`1..len-1`, so neither side can be empty — the `is_empty()` branch looked
structurally impossible, and the `Concat` branch looked like the same kind of
defensive leftover.

Journal 386 was one commit earlier. Its whole lesson was that reading a call
site is not a measurement. So: instrument, and ask what input *reaches* this.

Four `panic!`s in one build — the two cold branches here, plus
`extract_from_concat`'s non-`Literal` early return (:1480) and
`match_operator`'s `else → None` (:1496), both also at zero.

Three of the four fired on the first probe sheet:

```
[[ $x$y==ab { … } ]]   → SITE-NONLIT   (skip a non-Literal middle part)
[[ ab==$x$y { … } ]]   → SITE-MULTI    (rejoin a multi-part pattern)
[[ $x:$y==a:b { … } ]] → SITE-NOTOP    (skip a middle Literal that is not ==)
```

Nothing dead about any of them. A compact dispatch operator whose subject or
pattern spans more than one part — `$x$y==ab`, `ab==a$x`, `$x-$y==a-b` — is
ordinary sash, and the suite simply had never written one. Every existing
compact-operator spec (`dp.co1`-`dp.co8`, `dp.i1`-`dp.i8`) put a single part
on each side.

So the next question was whether the untested paths were also *correct*. Twelve
shapes against bash's non-compact spelling (`[[ "$x$y" == ab ]]`): match and
miss on each side, both sides multi-part, `:` and `-` as middle literals, the
`=~` side, and boolean-`[[ ]]` parity. All twelve agree. Working code that
nobody had ever run — a coverage gap, not a bug. They are now `dp.mp1`-`dp.mp12`.

## The fourth

`SITE-EMPTY` never fired, and here the structural argument is closed rather
than suggestive: `build_expr_from_parts` has exactly two call sites, both in
`extract_from_concat`, both slicing around a middle `i ∈ 1..len-1`, so
`parts[..i]` holds at least `parts[0]` and `parts[i+1..]` at least
`parts[len-1]`.

DD#116 decides what happens to it. Not a rival — nothing else in the tree
answers "what `Expr` does an empty parts slice build?". And it has none of the
three backstop properties. In particular it is the wrong side of property 1:
returning `Literal("")` would manufacture an empty subject or pattern out of a
program that has neither, which is a plausible wrong answer, not a loud
failure. Deleting the *silence* is the point.

It is now a slice `match` whose `[]` arm is `unreachable!`, with the index
bound written down. Same move as the `is_fd_number` parse in 385.

## Ledger

- 12 new specs, `spec/dispatch.sh` plan 58 → 70.
- Sweep **5945** (predicted 5945: 5921 + 2×12, dual-mode), 0 failing, 0 flips
  vs `ae26dc4`. Units 51/27/20.
- `build_expr_from_parts` cc 4 / cog 1.

## What to carry forward

Journal 386 found live arms behind a "proved dead" claim. This one found three
live branches behind two zero counts and a tidy call-site argument. Same shape,
one commit apart, and the second time the *reasoning* was mine and fresh rather
than inherited from an old filing.

A zero count means "no input in the suite reached this". Converting that into
"no input can" takes either an execution probe that fails to reach it or a
closed argument over *all* call sites — and when the closed argument does hold,
DD#116, not the absence of coverage, decides whether the code stays.
