title: "bug: c backend — `exit N` in the main body skips the END block"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "In the c backend, `END { … }` followed by a main-body `exit N` runs the END block (DD#56: END executes on the exit builtin), matching native. Then ungate spec/signals.sh 56.31 back to `in_modes native,c`."

# c backend: `exit N` in main skips END

Found while adding the DD#56 `$?`-in-END specs (56.30–56.33). Native runs the
END block for a main-body `exit N` and `$?` there is N; the **c backend skips
END entirely** for `exit N`.

## Symptom

```
END { /bin/echo "s=$?" }
exit 5
```

- native: prints `s=5`, process exits 5. ✓
- c: END does not run (no `s=5`); wrong exit. ✗  (spec 56.31, gated to
  `in_modes native` for now)

`56.30` (abort → END) and `56.32` (normal → END) PASS in c — only the `exit N`
entry path is broken. `56.3` (`END { exit 42 }; /bin/false`) passes because its
main body is an abort, and the `exit` is *inside* END.

## Root cause

`emit_c.rs::emit_exit` always emits `goto __exit;` (the final exit label),
which bypasses the `__end__` block:

```rust
fn emit_exit(e, args, types) {
    if let Some(arg) = args.first() { __exit_status = <int>; }
    __end_status = __exit_status;   // set, but END never runs
    goto __exit;
}
```

Native's `codegen/dispatch.rs::emit_exit` instead routes to the END block when
one exists and the exit is *outside* it: `if end_cl exists && ssa_id < end_id {
def exit_status = code; jump(end_cl) }`. The c backend needs the same
main-vs-inside-END distinction — `goto _blk_<end_id>` for a main-body `exit N`
(carrying the code in `__exit_status`), and the direct `goto __exit` only when
already inside END (or when there is no END block).

## Fix sketch

In `emit_exit` (c), mirror native: if an END block exists and the current block
is not part of it, set `__exit_status = <code>` and `goto _blk_<end_id>` instead
of `goto __exit`. The c emitter has `e.end_block = ssa.end_block` (emit_c.rs:363)
and the current block id (`e.cur_block`) — use them for the same `cur < end`
guard native uses.

Then ungate `spec/signals.sh` 56.31 to `in_modes native,c`.

## Note

This is a c-oracle divergence (native is the regression gate). It was latent —
no c test exercised a main-body `exit N` with an END block until 56.31, because
`$?`-in-END was blocked by the validator (SB5012) until DD#56's `$?`-availability
fix (journal 237).


## Done (2026-09-13, journal 279)

Fixed in `emit_c.rs::emit_exit`, exactly as the fix sketch describes: route to
`_blk_<end_id>` when an END block exists and `e.cur_block < eb`, otherwise the
direct `goto __exit`. `__end_status` is now set **only** on the direct path —
on the routing path the END block's own prologue captures the code, and
setting it twice would have been harmless but misleading. `spec/signals.sh`
56.31 un-gated; it passes in both modes.

**A second bug was in the same function.** DD#89 line 18 says "No argument →
`exit 0`", but the c emitter left `__exit_status` untouched for a bare `exit`
(its doc comment even asserted the wrong rule: "Bare `exit` keeps the previous
status"). So `- /bin/false` followed by `exit` exited **1** in c and **0** in
native. New specs 56.34 / 56.35 pin it in both modes.

Not fixed here, and separately filed: with an END block or without, the c
backend still emits no DD#55 `<argv0>: <cmd>: command failed (exit N)`
diagnostic for an external-command abort (it does emit the `cd:` errno one).
That is `dd55-abort-channel-remaining`, and the errors.sh specs already guard
those stderr assertions `native`-only on purpose.
