---
title: "bug: native and c disagree on a redirect open failure in a while condition"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "`while echo hi > /nonexistent_dir/f { … }` behaves identically in native and c"
  - "a spec pins whichever behaviour DG rules correct, in both modes"
---

## What

DD#55 says a redirect `open(2)` failure aborts unless an enclosing condition
catches it. `if` and `||` catch it in both backends (spec 55.25, 55.50). A
`while` condition does not agree:

```sh
int i=0
while echo hi > /nonexistent_dir_55/f { i=$(( i + 1 )); if (( i > 2 )) { break } }
echo "after i=$i"
```

| mode   | result                                                     |
|--------|------------------------------------------------------------|
| native | rc=1, `<argv0>: /nonexistent_dir_55/f: No such file or directory` |
| c      | rc=0, `after i=0` — caught, loop never runs                |

(janet aborts on nothing at all — separate, filed as
`c-janet-dd55-abort-channel`.)

## Root

The two backends compute `in_condition` differently:

- native `codegen/entry.rs::emit_block_insts` — `Terminator::Branch` only,
  and the *last Command/Pipeline/read* of the block.
- c `emit_c.rs::emit_block` — `Terminator::Branch | Terminator::Loop`, and
  the *last inst* of the block.

So a loop-condition block is a catching context in c and not in native.

## Not caused by the read-family guard

Found while fixing `read-family-redirect-open-not-guarded` (the read family
had no guard at all, so the divergence was unreachable through a read). The
`echo` reproducer above predates that work — verified at `c1e9807`.

## Decision needed

Which is right? A `while` condition reads like a catching context by analogy
with `if` (the loop simply does not run), but `while` is also the one
construct where an uncatchable setup failure would otherwise spin. DG rules;
then make both backends agree and pin it with a spec in `native,c`.
