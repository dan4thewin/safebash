---
title: "bug: c backend — `exit N` in the main body skips the END block"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "In the c backend, `END { … }` followed by a main-body `exit N` runs the END block (DD#56: END executes on the exit builtin), matching native. Then ungate spec/signals.sh 56.31 back to `in_modes native,c`."
---

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
