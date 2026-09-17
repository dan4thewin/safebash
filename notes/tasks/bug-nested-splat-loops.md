---
status: done
---

# Nested `@`-splat loops: the outer loop ran once

Found 2026-09-17 while probing cold branches in `type_check.rs::check_len_operand`
for the 100%-coverage push. Fixed the same day.

## Symptom

```sh
array b=(1 2 3)
array c=(p q)
for x in @b { for y in @c { echo "$x$y" } }
```

Wanted `1p 1q 2p 2q 3p 3q`. Got `1p 1q` — the outer loop ran exactly one
iteration, silently. Every splat form was affected: arrays, maps, and the
`@args` argv slice. Three-deep gave 2 lines instead of 8. Sequential
(non-nested) loops were always fine.

## Cause

`transform/pipelines.rs::lower_for_array` named its synthesized induction
variable with the **hardcoded string `"__ai"`**, and `lower_for_argv_slice`
did the same with `"__fi"`/`"__argc"`. One name per *compiler*, not per
*loop*. A nested loop therefore re-used the enclosing loop's index: the inner
preheader reset it to `0`, the inner body incremented it, and on inner exit
the shared counter — now equal to the inner array's length — was compared
against the *outer* array's bound. The outer index was destroyed.

`--emit-c` made it visible immediately: one `__ai_ptr`/`__ai_len` pair
declared for the whole function, written by both loops.

## Fix

Key the index name on the loop's own header block, which is already unique:

```rust
let idx_name = format!("__ai{}", header);
```

This follows an established in-tree convention — `transform/functions.rs`
already builds `__args{counter}` and `__bodycap_{blocks.len()}` the same way.
Nothing outside `pipelines.rs` referenced the old literals (only two
comments), so the rename was contained.

## What it exposed

Testing `break`/`continue` inside the now-working nested loops found a
**second, independent bug**: neither lowering pushed `state.loop_stack`, so
`handle_break_stmt`'s `depth > 0` guard silently dropped both. See
`bug-break-noop-in-splat-loops.md`.

## Why it survived

No spec anywhere nested two splat loops, and — separately — no spec in the
entire suite exercised a `break` statement at all. The 5404-test suite was
green throughout. The bug-finding value here came from the coverage push
asking "what reaches this cold branch?", not from any differential.
