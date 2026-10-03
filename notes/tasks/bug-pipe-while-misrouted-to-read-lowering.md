---
status: next
priority: P1
depends:
gates:
---

# `cmd | while COND` is misrouted into the `while read` lowering

Found 2026-10-03 while running DD#116's "prove it before acting" clause
against the arms journal 466 classified as validate-shadowed. The probe for
`lower_one_pipeline_stage`'s compound-stage branch hit, which meant the
classification was wrong; chasing *why* `| while` did **not** hit it found
this.

## The bug

`transform/stmt_handlers.rs:637`:

```rust
let is_while_read = matches!(&last.kind, StmtKind::While(_));
```

The name says `is_while_read`. The test is **structural only** — it never
checks that the condition actually reads anything. So every pipeline whose
last stage is a `while` or `until` is routed to
`handle_pipe_while_or_array_read` → `lower_pipe_while_read`, which lowers the
loop **in the parent process** and then calls `repoint_readlines_to_pipe` to
aim its `ReadLine`s at the pipe fd.

When the condition contains no `readline`, there is nothing to repoint. The
loop runs in the parent, the producer's output is written to a pipe nobody
drains, and the body sees the parent's stdin instead.

```
int n=0
/bin/printf 'a\n' | while (( n < 1 )); do (( n = n + 1 )); /bin/cat; done
```

| | output |
|---|---|
| bash (equivalent) | `a` |
| sash | *(nothing)* |

`until` is affected identically — `StmtKind::While { until: true }` matches
the same pattern. The loop body *does* run (`… do echo hi; done` prints
`hi`), so this is not a dropped stage; it is a stage connected to the wrong
file descriptors. **Silently wrong output, no diagnostic, both backends.**

## What works, for contrast

- `cmd | while readline v; do … done` — correct, agrees with bash. This is
  what `lower_pipe_while_read` exists for, and `repoint_readlines_to_pipe`
  does its job.
- `cmd | { … }`, `cmd | if …; fi`, `cmd | for …; done` — correct, agree with
  bash. These take the regular path and become a forked compound child
  (`lower_one_pipeline_stage`'s else branch).
- `{ … } | cmd` and a compound in a *middle* stage — SB4022. The rule only
  rejects a **non-final** compound stage.

So `while`/`until` is the one terminal compound form that is neither handled
correctly nor rejected.

## The shape

This is the fourth time a predicate keyed on **syntax** rather than on the
thing it claims to detect has been the bug (journals 430, 432, 440, 456).
`is_while_read` is a name that asserts a property the code never checks.

## Fix — DG's call

**(a) Make the predicate honest.** `is_while_read` becomes "last stage is a
`while`/`until` whose condition contains a `readline`". A non-reading
`while`/`until` then falls through to the regular multi-stage path and
becomes a forked compound child — exactly like `| { … }` / `| if` / `| for`,
which are already correct. Bash-compatible, consistent, and it warms the
compound-stage branch for a fourth spelling.

**(b) Reject it.** A new diagnostic: a `while`/`until` pipeline stage must
read. Conservative and smaller, but singles out `while` when `if`, `for` and
`{ }` are all allowed in that position — an arbitrary hole in the language.

Recommended: **(a)**.

## Specs needed either way

`spec/pipe_functions.sh` has no coverage of a terminal compound stage at all
— the cold-line report is what surfaced that. Needs: `| { … }`, `| if`,
`| for`, `| while readline` (the working form), and whichever of (a)/(b) is
chosen for `| while COND`.
