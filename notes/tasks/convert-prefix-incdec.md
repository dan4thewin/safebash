---
title: "sash-convert emits prefix ++/-- verbatim, which DD#31 forbids"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "`for (( i=0; i < 3; ++i ))` converts to something `sash` accepts"
  - "`(( ++i ))` and `$(( ++i ))` likewise"
  - "converter spec coverage for all three contexts"
---

DD#31 is categorical: *"Only postfix `x++` and `x--`. No prefix forms."*
`sash-convert` does not implement it — it passes prefix through verbatim
in **every** context:

| input | converter output | `sash` verdict |
|---|---|---|
| `$(( ++i ))` | unchanged | SB4020 |
| `(( ++i ))` | unchanged | SB4020 |
| `for (( i=0; i < 3; ++i ))` | unchanged | SB4020 (as of the `lower_c_style_for` fix) |

So the converter produces sash that does not compile.

**This is pre-existing and general, not a regression.** The first two rows
have always been SB4020. The third only became an error when
`lower_c_style_for` stopped discarding parse errors — before that, the
converter's invalid output happened to be accepted by a compiler bug, which
is why the gap went unnoticed.

## Why this is not just a rewrite

`(( ++i ))` → `(( i++ ))` is a safe mechanical rewrite: in statement
position the two are equivalent, and DD#31 calls both "syntactic sugar for
`x = x + 1`".

`$(( ++i ))` is **not**. Prefix in a value context means "increment, then
yield the new value", and sash forbids mutation inside `$(( … ))` outright —
there is no postfix spelling with the same meaning, because `$(( i++ ))`
would be rejected too. The honest conversions are a `CHECK:` comment or a
hard abort, not a rewrite.

**That choice is converter policy and belongs to DG** — it is the same
question as DD#57 and DG's "abort with message disposition for converter".
Do not pick one unilaterally. Present: (a) rewrite where sound + `CHECK:`
where not, (b) `CHECK:` everywhere, (c) abort with a message everywhere.

## Found by

Branch-coverage work, 2026-09-15 — chasing `parse/arith.rs` cold branches
`295|0|1`, `304|0|1`, `314|0|1`, which turned out to be the three discarded
`Err` arms in `lower_c_style_for`. See journal 323.
