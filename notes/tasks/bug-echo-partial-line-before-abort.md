---
title: "Bug: `echo` and `write` print part of their line before an argument aborts"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "each program in the table writes nothing, to stdout or to a redirect target, and still aborts with the same message and status, in both backends"
  - "interior NUL bytes in a word still print raw (the per-part contract `echo_write_one_arg`'s doc states)"
  - "a spec pins each shape in both backends; each fails on the pre-fix binary"
---

## What

Measured on the base binary (1309385) and after F14, with the same result
in both backends:

| Program | stdout | abort message, status |
|---|---|---|
| `str x=""` / `echo "[${x:?m}]"` | `[` | `m`, 1 |
| `str x=""` / `echo a "${x:?m}" b` | `a ` | `m`, 1 |
| `int z=0` / `echo "[$(( 1 / z ))]"` | `[` | `division by zero`, 2 |
| `str x=""` / `write "[${x:?m}]"` | `[` | `m`, 1 |
| `str x=""` / `echo "[${x:?m}]" > out.txt` | (none; `out.txt` holds the one byte `[`) | `m`, 1 |

The command aborts while its arguments are being evaluated, but whatever
came before the failing part has already been written. An external echo
does not do this: `/bin/echo "[${x:?m}]"` prints nothing and aborts with
`m`, because every argument is evaluated before the exec.

No DD, journal or task discusses this (searched for partial or
half-written output), and no spec pins it.

## Cause

Both backends write a word one part at a time, lowering each part just
before its write: `echo_write_one_arg` (`sash/src/codegen/commands.rs`) and
`emit_echo_write_arg` (`sash/src/emit_c.rs`). A `:?` abort or an arithmetic
trap inside a later part, or in a later argument, therefore runs after the
earlier writes. `write` shares the path, through `rewrite_write_as_echo`
(`sash/src/transform/cmd_handlers.rs`).

## Fix (proposed, not yet ruled)

Evaluate every part of every argument before the first write, then write.
One thing to check first: a part's pointer must survive the evaluation of
the parts after it. If two parts can share a scratch buffer, such as one
used to format an integer, writing as you go is what keeps them correct
today, and evaluating everything first would print the last value twice.

Found during the batch-531 port (step F14, while re-measuring the base
reproducers that the DD#65 and DD#72 addenda cite). It is unrelated to the
port. Filed under DG's rule for the port that questions and uncovered cases
become backlog tasks.
