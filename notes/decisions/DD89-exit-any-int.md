---
title: DD#89 — `exit` accepts any int value/variable/return; full i64 to END, 8-bit at the process boundary
status: accepted
---

Settled with DG (2026-09-03). `exit`'s argument is an ordinary integer sink:
**any int — a literal, a variable, an arithmetic result, or an int-returning
function — is a valid `exit` argument**, with no compile-time range restriction.

## The rule

- `exit 5`, `exit 300`, `exit -1` (literals, including out-of-range).
- `exit $code` where `int code = …` (an int variable).
- `exit $(( expr ))` (an arithmetic result).
- `exit $(f)` where `f` is an `-> int` function (an int return).
- `exit $?` (the current status — subject to the usual `$?` handling-position
  rule).
- No argument → `exit 0`.
- A string argument coerces to int (atoi), as in every other int context; `exit`
  imposes no extra type restriction.

The prior type-check reject of `exit $intvar` ("value may exceed 0-255; use
`exit $((v<256?v:1))`") is **removed** — clamping is the programmer's choice,
not a compiler mandate.

## Value semantics

- **Internally the value is a full i64.** `exit N` stores the untruncated value
  in the exit-status channel, so an END block reads the exact value via `$?`
  (e.g. `int code=$?`) — `exit 300` → END sees `300`, `exit -1` → END sees `-1`.
- **At the process boundary it is 8-bit (POSIX).** The final `exit_group(status)`
  passes only the low byte to the parent's `wait()`: `exit 300` → parent sees
  `300 & 0xFF = 44`; `exit -1` → `255`; `exit 256` → `0`. Same as bash.

So an out-of-range value is faithful *inside* the program (and to END) and
truncated only when it actually leaves the process.

## What this enables (but does NOT mandate)

Because the full value reaches END, a program *may* use an out-of-range exit
code as a **tag** for "which `exit` preceded END", dispatch on it in END
(`int code=$?; if (( code > 255 )) { … }`), and re-exit with a real 0-255 code.
DG (2026-09-03): this is **not** prescribed over just using a global variable —
"it's up to them." The DD blesses the *mechanism* (any int works, value is
faithful to END), not a particular idiom. A plain global read in END is an
equally valid way to convey an exit-site marker (it keeps the exit code a clean
0-255 but only covers explicit exits, not aborts).

## Implementation

- **Native codegen** (`codegen/dispatch.rs::emit_exit`): evaluate the argument
  with `lower_value_as_int` (was literal-only, defaulting any non-literal to 0).
- **c backend** (`emit_c.rs::emit_exit`): already used `lower_value_as_int` — no
  change.
- **type_check**: `check_exit_arg` (the `exit $intvar` reject) removed.
- **Process exit**: `sash_rt::sash_exit(i64)` → `exit_group(status as i32)`; the
  kernel truncates to 8 bits for the parent. (No change — this is the POSIX
  boundary that makes >255/<0 "internal only".)

## SB codes

None. This REMOVES a type-check diagnostic (the `exit $intvar` reject); no new
codes.

## Related / open

- DD#56 — `$?` in the END block is the triggering status (the channel that
  carries an out-of-range tag into END). journal 237.
- `bug-emit-c-exit-skips-end` — the c backend's main-body `exit N` skips END
  (native routes through it); orthogonal to this DD but on the same path.
- Whether to add a dedicated "exit-site" channel (like `$(signum)`) is left
  open; a plain global already suffices, so no new surface is planned.
