# 368 — Two holes under one loop

**Sweep 5645, 0 failing** (5629 + 16, predicted exactly). Units green.

Journal 367 landed the redirect scope and filed a P1 found while probing
it: a redirect-open failure inside `END { }` never terminates. DG chose
**A1** — diagnose, restore, let the rest of the cleanup run. Fixing it
turned up a second, wider hole feeding the same loop.

## The loop

DD#55 aborts route *through* the END block so cleanup still runs. From
inside END that is a jump to the block already executing. Under
`timeout 3`: rc=124 and 380,232 identical diagnostic lines.

`mark_unchecked_in_end_blocks` exists to prevent exactly this. It could
not reach the redirect path, because it clears `checked` while the
redirect-open abort keys on positional `in_condition` — two signals that
never meet.

## The second hole was in the predicate

Fixing only that would have left the bigger problem. The marking pass
selected its blocks by label:

```rust
if block.label.contains("__end__") {
```

`install_end_block` lowers the trap body with `lower_stmts`, so **only
the entry block is named `__end__`**. An `if` or `for` inside END
produces ordinary labels. Measured:

```sh
END { if /bin/true { /bin/false; echo tail } }   # rc=124, 2,732 lines
END { for i in 1 2 { echo n$i; /bin/false } }    # rc=124, 10,740 lines
```

So *any* failing command in a nested block inside END looped — not a
redirect problem at all. The flat case worked, which is why it survived:
`END { /bin/false; echo tail }` is the shape anyone writes first, and it
is the one shape the label test covers.

The predicate wanted reachability, not a name. `SsaProgram::end_region()`
is `reachable_from(end_block)`, and both the transform's marking and
codegen's abort routing now share it, the way `condition_inst_index` is
already shared between backends. `reachable()` became a thin call on the
generalised `reachable_from`.

Worth noting there is a *third* spelling of "inside END" still in the
tree: the signal check-point compares block ids (`ssa_id >= end_block`,
and `e.cur_block >= eb` in emit_c). That one happens to be correct —
END's blocks are allocated after it — but it is a different predicate
for the same question, and it is correct by an accident of allocation
order rather than by construction.

## A near-miss on the way

The open-failure path disagreed with the *exec*-failure path — the
sibling setup failure, where the command equally never runs — in three
of four positions:

| position | exec failure | open failure |
|---|---|---|
| top, unsuppressed | msg + abort | msg + abort |
| top, `-` suppressed | msg + **continue** | msg + **abort** |
| `if` condition | **msg** + else | **no msg** + else |
| inside `END` | msg + **continue** | **infinite loop** |

Three rows wrong out of four looks like one inconsistency with one fix,
and the table is persuasive precisely because the two failures really
are the same *kind* of event. They are not treated as one here, and
`errors.sh` says so explicitly:

> 55.26 `- echo > /bad` still ABORTS: `-` suppresses echo's write
> verdict, not the prior `exec N>file` open, which runs first

with 55.25/55.50/55.63/55.64 pinning "no misroute, **no diag**" in
condition position. Three of the four rows are recorded design with a
stated rationale — the open runs *before* the command, so suppression
scoped to the command does not reach it. Only the END row was a bug.

The generalisation is not "check `notes/decisions/` first", which
AGENTS.md already says. It is that a consistency argument carries
exactly as much weight as the claim that the two things compared are
the same kind of thing — and that claim is what the spec comment
denies. The table was evidence *for the analogy*, not evidence the
analogy held.

## What A1 looks like

`emit_open_failure_abort` now returns whether it left the block
unterminated. Inside END it reports the diagnostic, sets a failing
status, and returns true; the caller jumps to the merge and cleanup
continues. Everywhere else it aborts as before. Extracting that helper
in journal 367 for the complexity gate turned out to give the fix
exactly one place to land in the native backend, and
`emit_open_failure_goto` mirrors it in C.

One asymmetry fell out and had to be closed: the redirect *scope* is
marked unchecked inside END, so its failure path was silent there. But
silence belongs to a *caught* failure (55.25), not a cleanup one, and
the plain-command path in END reports. Both now use
`checked || in_end_block`.

## Scope check

The other abort sites in END are fine and were measured, not assumed:
command failure, exec failure, `cd` failure and the pipeline abort are
all `checked`-governed (so the corrected marking now covers them),
divide-by-zero exits cleanly at rc=2, and `exit N` in END still wins.
No safety net was added to `abort_target` — with the region fixed there
is no reachable abort left inside END to catch, and untestable defensive
code is its own problem.

## Spec ids

`56.80`–`56.87` in `signals.sh` (plan 40 → 48, +16). `56.81`/`56.82`
are the nested-block cases, `56.86` guards the fix against over-reach
(outside END the abort must still abort, and route through END once),
and `56.87` pins `exit N` in END beating a failed open. `run_sash` is
time-limited, so a regression fails as a hang rather than a wrong
answer.
