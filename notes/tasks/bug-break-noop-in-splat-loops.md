title: "`break` and `continue` were silent no-ops in every `@`-splat loop"
status: done

# `break` and `continue` were silent no-ops in every `@`-splat loop

Found 2026-09-17 immediately after fixing `bug-nested-splat-loops.md`, while
checking whether `break`/`continue` behaved correctly inside a nested splat
loop. They did not behave correctly anywhere.

## Symptom

```sh
array b=(1 2 3)
for x in @b { echo "$x"; break }
```

Printed `1 2 3`. The `break` compiled and did nothing. `continue` in the
middle of a body was equally inert — it did not skip the rest of the body.
Both were no-ops in **every** `@`-splat loop, nested or not, over arrays,
maps, or `@args`. A literal-list loop (`for x in a b c`) was always correct.

## Cause

Two independent lowerings build loops:

- `transform/control_flow.rs::lower_for_static` (literal list) pushes
  `state.loop_stack` — correct.
- `transform/pipelines.rs::lower_for_array` / `lower_for_argv_slice` (splat)
  **never pushed it**.

`transform/stmt_handlers.rs::handle_break_stmt` guards on
`n > 0 && depth > 0` and, when the stack is empty, falls through and returns
the block unchanged. There is no diagnostic — the statement is discarded.

## Fix

Give each splat loop a **latch block** holding the index increment, and
bracket the body with the loop context:

```rust
state.loop_stack.push((latch, exit_block));
let body_exit = lower_stmts(&f.body, ssa, state, body_block);
state.loop_stack.pop();
ssa.block_mut(body_exit).terminator = Terminator::Goto(latch);
```

The latch is why the increment had to move. `continue` jumps to the
continue-target; with the increment sitting at the end of the *body*, a
`continue` would have skipped it and spun forever. The latch is now the
back-edge predecessor for phi insertion, and pass 2 clears it alongside the
header and body.

`break 2` works because `handle_break_stmt` already indexes
`loop_stack[depth - level]`; it simply never had entries to index.

## The related gap, fixed with it

`break`/`continue` **outside any loop** hit the same empty-stack path and were
likewise accepted and discarded. DG's call: reject. That is now **SB5081**,
which treats a function body as a fresh context — a `break` lexically inside a
function is not inside a loop even when the call site is.

## Why it survived

`grep '^\s*break\b' spec/*.sh` matched **nothing**. The construct was entirely
unspecced, in a 5404-test suite. Covered now by `cf.n8`–`cf.n13`, `cf.n17`,
`cf.n18`.
