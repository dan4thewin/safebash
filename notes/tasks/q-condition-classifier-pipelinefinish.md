---
title: "Question: the condition classifier's `PipelineFinish` arm is unreachable — ICE, or answer by `preserve_status`?"
status: open
priority: P4
kind: question
depends: []
gates:
  - "DG rules between the options below"
  - "if (a): the arm is `unreachable!()` with a message that names reader cleanup's bridging, and a `#[should_panic]` unit test pins it"
  - "if (b): a unit test asserts both answers (`preserve_status` true → false, false → true)"
  - "either way, `uncheck_last_condition_inst`'s `PipelineFinish` arm follows the same ruling"
---

## What

F7 (batch-531 finding B85, journal 536) made `ssa::sets_condition_status`
total over `SsaInst`. It ported the reference branch's answer for a reader's
`PipelineFinish` as-is: `true`, a finish sets the status a following
condition reads (`ssa.rs:1484`).
`transform/entry.rs::uncheck_last_condition_inst` clears `checked` on it
(`entry.rs:178`). DD#55's addendum of 2026-10-26 records that the arm cannot
be reached. This task asks what the arm should say.

## Why it cannot be reached

- The one producer is `transform/reader_cleanup.rs::finishes` (line 58),
  called by `install`. On an edge out of a block that ends in a `Branch` or
  `Loop` it puts the finishes in a new `pipe.finish` block ending in a `Goto`
  (`install_edge`, lines 67–88). Otherwise it appends them to a block that
  ends in a `Goto` or `Return`. So no finish sits in a block that ends in a
  `Branch` or `Loop`.
- `install` runs at `transform/entry.rs:32`, before
  `mark_unchecked_in_conditions` at line 40. Nothing after it moves an
  instruction between blocks. `install_signal_checkpoints` (line 36) only
  inserts at index 0, and the two uncheck passes only clear `checked`.
  `cli_main` (`lib.rs:437–460`) then sets the source-name table and hands the
  program to the backends.
- Every caller of the scan sees only such blocks, or runs before any finish
  exists:
  - `ssa::condition_inst_index` returns `None` unless the terminator is a
    `Branch` or `Loop`. Native (`codegen/entry.rs:319`) and C
    (`emit_c.rs:759`) call it.
  - `mark_unchecked_in_conditions` (`transform/entry.rs:159`) filters on
    the same two terminators.
  - `lower_return_producer` (`statements.rs:97`) and `lower_short_circuit`
    (`statements.rs:992`) run during lowering, before `install`.

## Why `true` may be the wrong answer anyway

A finish with `preserve_status` restores the status and the test result it
found, in both backends: `codegen/inst_handlers.rs:585`
(`handle_pipeline_finish`) and `emit_c.rs:3952` (`emit_reader_finish`).
It sets nothing a following condition reads. If a finish ever reached the
scan, `true` would select it over the instruction that really set the
status. Without `preserve_status` a finish does write the test result: the
producers' status, ANDed with the one it found unless the reader is a loop's
consumer (`inst_handlers.rs:588`, `emit_c.rs:3958`).

## Options

- **(a) `unreachable!()`**, citing reader cleanup's bridging. AGENTS.md's
  "Cold arms: ICE or answer?" rule (unreachable by construction → ICE)
  points here. The check is orthogonal (an SSA placement invariant that no
  gate restates), and a `#[should_panic]` test is the claim.
- **(b) `!preserve_status`**, the precise answer. It stays an unexercised
  answer that nothing can reach.
- **(c) keep `true`.** This is the port's behaviour, and a cold line in the
  coverage queue.

Recommended: (a).

## Related, unmeasured: the `Command` arm counts every internal sigil

`sets_condition_status` answers `true` for every `Command` except
`__negate_test_result`, including the compiler-internal ones
(`BuiltinCmd::may_take_redirect`'s sigil list, `ssa.rs:307–317`). Its doc
says `true` is "for exactly the instructions both backends lower to a write
of the test result", which some of those may not be. HEAD's scan matched
every `Command` too, so F7 changed nothing here.

- `SignalCheckpoint` is harmless. `install_signal_checkpoints` puts it at
  index 0 only, so the scan selects it only when nothing else in its block
  sets the status. Selecting it then is the same as selecting nothing: both
  backends ignore its `checked` (`codegen/dispatch.rs:289`,
  `emit_c.rs:2115`), it takes no redirect, and it writes the status only on
  the path that exits (`inst_handlers.rs:935`, `emit_c.rs:2359`).
- For the other sigils it is not yet known whether each writes the test
  result, or whether any can follow a condition operand in its block. If one
  can, that operand keeps its abort and the sigil gets the redirect guard.
  Measure this when the task is taken up.
