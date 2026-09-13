# 279 — Two bugs in one function, and a guard that was never true

Continuing DG's "known bugs, worst first" list. Journal 278 took `exec`; this
is the next two items, plus the stale gate behind them.

## `exit N` skipped the END block

Filed as `bug-emit-c-exit-skips-end`, and for once the filing was exactly
right — it even contained the correct fix sketch. C's `emit_exit` always
emitted `goto __exit`, the final label, bypassing the END block entirely:

```
END { /bin/echo "s=$?" }
exit 5
```

native printed `s=5`; c printed nothing. `56.30` (abort → END) and `56.32`
(normal → END) both passed in c, so only the `exit` entry path was broken.

Native routes through the END block carrying the code, and the structure of
the emitted C already supports that — `__exit:` is `sash_exit(__end_status)`,
and the END block's prologue captures `__end_status = __exit_status` on entry.
So the fix is a `goto _blk_<end>` when an END block exists and `cur_block <
end_block`, the same guard `emit_signal_exit_check` was already using.

One detail worth stating: `__end_status` is now set **only** on the direct
path. On the routing path END's own prologue captures it; setting it in both
places would have been harmless but would have implied the two paths were
doing the same thing, which they are not. Inside END, `exit N` *must* set it,
because there it overrides the triggering status.

## The second bug was in the same six lines

While reading `emit_exit` I checked what it does with no argument, because the
native version uses `iconst(0)` and the C version's doc comment said something
different:

> Bare `exit` keeps the previous status.

DD#89, line 18: **"No argument → `exit 0`."**

So the comment asserted a rule the project had already decided against, and
the code implemented the comment. `- /bin/false` followed by a bare `exit`
exited **1** in c and **0** in native.

This is the kind of thing that only turns up by reading the neighbouring
lines of a fix. The filed bug was about END routing; nothing pointed at the
argument handling. Both are now pinned — `56.34` (with END, so the END block
sees 0 too) and `56.35` (without).

## A gate that was never true

`spec/brace.sh` carried:

```
# Run native only — the c backend has a pre-existing control-flow gap
# (until / `(( ))`) that the keyword forms hit identically
[[ ${SASH_MODE:-native} != native ]] && exit 0
```

All eight constructs had already been measured byte-identical between the
backends during the grooming pass. Removing the guard: **27/27 in c**, first
try. The gap the comment describes did not exist when the comment was written.

That is the second time in two sessions that a *guard* rather than a *bug* was
the thing standing between the C backend and its coverage — `exec.sh` was the
first. A native-only guard written as a workaround is a claim about the
backend, and claims decay. Nothing re-checks them, and because the guard
suppresses the evidence, nothing can.

## What is still open in the same area

The probe that confirmed the END fix also showed c emitting **no** DD#55
`<argv0>: <cmd>: command failed (exit N)` diagnostic for an external-command
abort — with or without an END block. It does emit the `cd:` errno one, which
is why this reads at first like an END-related regression. It is not: it is
the known `dd55-abort-channel-remaining` parity gap, and `spec/errors.sh`
already guards those stderr assertions `native`-only on purpose
(`{ ! in_modes native || [[ $rerr == … ]] }`). Measured, matched against the
record, left alone.

Next on DG's list is `c-compound-pipeline-stage`, which is a feature rather
than a one-line divergence: a `fn` or `{ }` group used as a pipeline stage
reaches `sash_exec("__compound_5", …)` in c and dies with `sash: bug: bare
command passed to sash_exec`. Native routes it through the DD#14
`__compound_<id>` mechanism; the C emitter has no handling for it at all.

## Result

Sweep **4170 passing, 28 skipped, 0 failing**. One flip, and it is the
intended one: `c:56.31 skipped → passing`. +31 gained — 27 from un-gating
`brace.sh` in c, 4 from the two new bare-exit specs across both modes.

Both fixes fail-first verified: reverting `emit_c.rs` fails 56.31, 56.34 and
56.35 in c and leaves native untouched, which is the right signature for a
c-only divergence.
