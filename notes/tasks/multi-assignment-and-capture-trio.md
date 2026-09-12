---
title: "multiple assignment via return; external capture as (stdout, status, stderr) trio"
status: closed
priority: P2
closed: 2026-09-12
kind: decision
depends: []
gates:
  - "DG decides the design of general multiple-value return + multi-target binding, and the external-capture trio (stdout/status/stderr) that reuses it — see the axes below. Blocks any implementation; capture the resolution of the SB4025/DD#55 conflict here too."
  - "PARTIAL (2026-09-02, DD#88): the external-command capture-CONDITION subset shipped — `if x=$(cmd)` / `while x=$(cmd)` (and `$(a|b)` pipeline) bind stdout + branch on the command's status. This resolves the SB4025/DD#55 conflict for the condition case. The GENERAL multi-value return + the (stdout, status, stderr) trio remain open (axes below)."
---

# CLOSED (DG, 2026-09-12)

**DD#88 solved the real problem; the residual capabilities have no driver.**

The "Motivating gap" table below is **obsolete** — every row in it was
re-tested on 2026-09-12 and all of them work:

| form | filed as | today |
|---|---|---|
| `if ! out = $(cmd) { … }` | SB4025 | **compiles, branches correctly** |
| `out = $(cmd) \|\| { … }` | SB4025 | **compiles, catches** |
| `out = $(cmd); <branch on status>` | aborted before the `if` | `st = $(status)` works |
| stderr | only via tempfiles | `2>&1` merge works |

So the **SB4025 ↔ DD#55 conflict listed under "Also resolve here" is already
resolved** — `||` on a capture is no longer rejected, and DD#55's documented
idiom works as written.

Of the proposed `(stdout, status, stderr)` trio, two thirds already exist as
ordinary language (`out = $(cmd)`, `st = $(status)`), and the third — stderr
captured *separately* from stdout — has no current driver; `2>&1` covers the
observed need. General multi-value `return a, b, c` likewise has no caller
waiting on it.

DG closed it rather than design six axes for a feature nothing needs. The
analysis below is kept for the record: if a genuine need for separate stderr
capture or N-ary return appears, this is the starting point, and the axes are
still the right ones. Nothing is blocked in the meantime.


> **Update 2026-09-02:** the narrow capture-**condition** case (`if ! out=$(cmd)`)
> shipped as **DD#88** (`notes/decisions/DD88-capture-condition.md`,
> `spec/capture_cond.sh`, journal 236): an external `$(cmd)`/`$(a|b)` RHS in a
> boolean context branches on the command's status and captures stdout. The
> general multiple-value `return` / multi-target binding / full trio below is
> still deferred to DG.

# Multiple assignment via `return`; capture a command's stdout/status/stderr

DG (2026-09-02): "support multiple assignment in general via return statement;
a capture of an external command could assign each of stdout, status, and
stderr." Filed as a decision; **no implementation until the axes below are
settled.** Directly resolves the `if ! out=$(cmd) { … }` gap (see
`## Motivating gap`).

## The idea

Multi-*target* binding `a, b, c = <producer>`, where a producer yields N
values. Two producer kinds:

- a **user function** via `return a, b, c` (an explicit value channel, separate
  from stdout — today a fn "returns" data only as stdout `$(f)` [clean, DD#24]
  or an `-> int` [Option C]);
- an **external capture**, which the compiler treats as the fixed trio
  `(stdout, status, stderr)` — externals have no `return`, so this is a
  built-in interpretation of a captured command.

Strictly more expressive than bash (stdout + `$?`, stderr only via `2>&1` /
tempfiles).

## Motivating gap (why this exists)

`if ! out=$(cmd) { echo bang }` — capture stdout AND branch on the command's
failure — has **no working sash form** today:

| form | result |
|------|--------|
| `if ! out=$(cmd) { … }` | SB4025 (rejected) |
| `out=$(cmd); if (( $? != 0 )) { … }` (SB4025's suggested "sequence it") | aborts on failure — never reaches the `if` |
| `out=$(cmd) \|\| { … }` | SB4025 — yet DD#55 documents this as the idiom (conflict, below) |
| `out=$(- cmd); …` | `-` DISCARDS status (`$?` after = compile error) — can't branch (and had an abort bug, fixed separately) |

Root cause: DD#71 says "an assignment has no meaningful exit status" — true for
`x=foo`, but **`x=$(cmd)` does** have one (the command's), which is exactly what
`if ! out=$(cmd)` relies on. That subcase wasn't separated out.

## The crux — a status target toggles catch vs. abort

Proposed rule: if you **bind the status**, you own the failure → **no abort**;
if you don't (`out = $(cmd)`), today's DD#55 abort stays. One rule:
- resolves the DD#55 ↔ DD#70/#71 conflict (replaces the dead `x=$(cmd) || …`);
- keeps "failures abort unless handled" — but "handling" now yields the *value*,
  not just `-`-style suppression.

## Design axes (each a fork for DG)

1. **Binding syntax.** sash has no multi-assign; DD#71 says `=` has no spaces and
   whitespace bounds a value, so a spaced `a b c = P` collides. Options: comma
   list `a, b, c = P` (commas otherwise free); a keyword (`let a, b, c = …`); a
   read-style verb (`cmd => out st err`, `P into a b c`). Lean: comma-list with
   an explicit lead so it can't be read as a value binding.
2. **Arity & typing.** Returns are already typed (`-> int`); extend to
   `-> (str, int, str)`. Bind exactly N with `_` to discard. Decide: allow a
   prefix (bind first K) or require all-or-`_`.
3. **Taint split.** stdout/stderr external → tainted (bless before use);
   **status clean** (`$?`-like compiler int, branch freely). Note: capturing
   stderr just to `echo` it on error still needs a bless — decide if fine.
4. **Trio order / partial bind.** `(stdout, status, stderr)` positional + `_`,
   or **named** (`{ out=stdout; st=status }`)? Named avoids an order convention
   and makes "just stdout+status" trivial.
5. **Runtime/backends.** Trio capture needs sash-rt to fork with **two** pipes
   (stdout+stderr), drain both (deadlock care), and return status; native+c
   share the runtime, janet separate. `SsaInst::ReadLineMulti` is the
   multi-target lowering precedent; `CaptureEnd { target, tainted }` is the
   single-target one to generalize.
6. **DD#74 reconciliation.** A fn becomes predicate (status) | value-fn (1+
   values) | void; multi-value is N-ary value return (not a predicate). `$(f)`
   on a multi-value fn — first value only, or illegal outside a multi-bind?

## Staging options

- **(i) mechanism-first:** general multi-return/multi-assign for user fns, then
  the external trio as a builtin producer. Clean layering; capture pain waits.
- **(ii) pain-first:** ship the external trio (`out, st, err = cmd`) as a narrow
  builtin now; generalize to user `return a, b, c` later.

## Also resolve here (do not fix piecemeal)

- **SB4025 ↔ DD#55 conflict.** DD#55 §"Command substitution" documents
  `x=$(cmd) || x=default` as "explicit handling," but
  `reject-nonstatus-in-boolean-context` (DD#70/#71 dual, SB4025) deliberately
  rejects `assignment || …`. The design decides whether `$(cmd)`-RHS
  assignments become catchable (making `||` / `if !` work) — if not, DD#55 must
  drop that idiom and SB4025's "sequence it" message must stop implying the
  capture+branch case is expressible.

## Related / benched

- `reject-nonstatus-in-boolean-context` (done) — the SB4025 origin.
- DD#55 (error model), DD#70 (taxonomy), DD#71 (value bindings), DD#74
  (predicate functions), DD#24 (function-capture clean).
- The `$(- cmd)` failing-capture **abort bug** is being fixed independently
  (it contradicts DD#55's *current* spec regardless of this decision).
