---
title: "exit accepts any int value/variable/return (DD#89)"
date: 2026-09-03
---

# `exit` accepts any int (DD#89)

DG: "I want to bless this in the sense that any int value/variable/return should
work as argument to exit, and a DD/specs to ensure the functionality survives
future changes; but I don't intend to dictate that other programmers should use
this vs. a global — it's up to them."

Follows the DD#56 `$?`-in-END work and DG's out-of-range-exit-as-tag idea:
`exit`'s argument is now an ordinary int sink — a literal, a variable, an
arithmetic result, or an int-returning function — with no compile-time range
restriction. The value is a full i64 internally (readable in END via `$?`) and
truncated to 8 bits only at the process boundary (POSIX `exit_group`).

## Two gaps closed

1. **Native codegen only handled a literal.** `emit_exit` parsed
   `SsaValue::Literal` and defaulted every non-literal to `0` — so `exit $code`
   / `exit $((…))` silently exited 0 in native (c already used
   `lower_value_as_int`). Now native evaluates any int-valued argument the same
   way.
2. **type_check rejected `exit $intvar`** ("value may exceed 0-255; use
   `exit $((v<256?v:1))`"). Removed — clamping is the programmer's choice.

## Behavior

```
int code=7;  exit $code        # exit 7
exit $((3+4))                  # exit 7
fn f -> int { return 9 }; exit $(f)   # exit 9
exit 300                       # process exits 44 (300 & 0xFF)
exit -1                        # process exits 255
int code=300
END { int c=$?; /bin/echo "$c" }
exit $code                     # END sees 300; process exits 44
```

The out-of-range value is faithful *inside* the program and to END; only the
final `exit_group(status as i32)` truncates to the low byte for the parent's
`wait()`, exactly like bash.

## Not prescribed

DD#89 blesses the *mechanism* (any int works; the value is faithful to END), not
an idiom. Using an out-of-range code as an exit-site tag (dispatch in END, then
re-exit a real 0-255 code) is one option; a plain global read in END is an
equally valid one (keeps the exit code clean, but only covers explicit exits).
DG: "it's up to them."

## Complexity

`emit_exit` was refactored while touched — the `end_cl_block`/`program.end_block`
`ssa_id < end_id` triple-nest flattened to one `Option::zip(...).filter(...)`,
dropping cognitive 8→2.

## Files / tests

- `sash/src/codegen/dispatch.rs` — `emit_exit` evaluates any int arg;
  routing flattened.
- `sash/src/type_check.rs` — `check_exit_arg` reject removed.
- `notes/decisions/DD89-exit-any-int.md`.
- `spec/exit_arg.sh` 89.1–89.8 (var/arith/int-return; literal + var + negative
  truncation; no-arg; full-i64-to-END). native,c; 89.8 native (the c
  exit-skips-END gap, bug-emit-c-exit-skips-end). 3-mode sweep: 0 regressions.
