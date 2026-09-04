---
title: "$? readable in the END block (DD#56); found a c exit-skips-END bug"
date: 2026-09-03
---

# `$?` as the first action of the END block

DG: "If I try to read `$?` as the first action of the END block, I get SB5012.
Given an abort due to a failed cmd jumps to the END block, I'd think I should
be able to" — and "likewise, I'd think I could read the value for an exit N".

DD#56 already designed for this: "In END block: the status that triggered END"
and "Without explicit `exit N`, the process exits with the status that triggered
END." The runtime already carries the triggering status into END (at `__end__`
entry `__end_status = __exit_status`, and `__exit_status` is not reset), so `$?`
there reads the right value — the only blocker was the validator's SB5012 `$?`
state machine, which validated the END block in a fresh `Undefined` state.

## The fix (validator only)

The END block is parsed as the `__sash_end__` function
(`parse_end_block` → `StmtKind::Function{name:"__sash_end__"}`;
`program.end_block` is never populated — that IR field is vestigial, distinct
from the transform's `ssa.end_block`). So the `$?` machine reached it via
`check_dq_function`, which starts every function `Undefined`.

`check_dq_function` now starts the `__sash_end__` body in `StatusState::Available`
— the END block is ENTERED because of a triggering status, so that status is the
first-action value, exactly like an `if`/`while` handling branch. Every other
function still starts `Undefined` (no leak — guarded by spec 56.33).

## Behavior

```
END { /bin/echo "s=$?" }
/bin/false          # s=1  (abort's failing code), exit 1
```
```
END { /bin/echo "s=$?" }
exit 5              # s=5, exit 5
```
```
END { /bin/echo "s=$?" }
/bin/echo hi        # s=0  (last command's status), exit 0
```

`$?` is a full i64 — usable in `(( ))` (`if (( $? > 255 ))`) and assignable
(`int code=$?`). The first read consumes it (handling-position rule); capture it
once (`int code=$?`) to use the value repeatedly. This also makes DG's
"out-of-range exit as a tag for which exit preceded END" idiom work in native:
`exit 300` → `int code=$?` in END → `code == 300` (the process exit truncates to
`300 & 0xFF = 44` only at the final `exit_group`).

## Found: c backend `exit N` skips END (filed bug-emit-c-exit-skips-end)

Adding the specs exposed a pre-existing c-oracle divergence: `emit_c::emit_exit`
always `goto __exit` (bypassing `__end__`), so a main-body `exit N` does not run
END in c (native routes through it). 56.30 (abort) / 56.32 (normal) pass in c;
56.31 (`exit N`) is gated `in_modes native` and the c fix is filed.

## Tests / verification

`spec/signals.sh` 56.30–56.33 (plan 19→23): abort / `exit N` / normal → `$?` in
END; 56.33 guards that a regular function still can't read `$?` first (SB5012).
janet's END block is a separate pre-existing gap (56.1/56.4 already fail), so the
positive specs are `native,c` (56.31 native). 3-mode sweep vs HEAD: native 0
regressions, c 0, janet 0. DD#56 amended.

## Files

- `sash/src/validate.rs` — `check_dq_function` starts `__sash_end__` `Available`.
- `spec/signals.sh` — 56.30–56.33.
- `notes/decisions/DD56-signal-trap.md` — `$?`-in-END amendment.
- `notes/tasks/bug-emit-c-exit-skips-end.md` — the c divergence.
