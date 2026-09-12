---
title: "cleanup: `local`-era names and doc comments outlived DD#86"
status: done
priority: P4
kind: cleanup
depends: []
gates:
  - "no reference to `parse_local` remains (it does not exist)"
  - "`save_locals`/`restore_locals` are named for what they do"
---

## What

DD#86 phases 2-5 are complete and the `local` keyword is gone, but two
cosmetic traces remain.

**1. `parse_local` in doc comments only.** `sash/src/parse/parser.rs:14` lists
it among the declaration parsers, and `:648` says "`local int array` already
stacks via parse_local". The function does not exist; both comments describe a
parser that was removed.

**2. `save_locals` / `restore_locals` are misnamed.**
(`transform/stmt_handlers.rs:1427` and `:1409`.) They are live and correct,
but they have nothing to do with `local` — they snapshot the variables a
**body capture** assigns and restore them afterwards, which is `$(f)` subshell
isolation. Something like `snapshot_assigned` / `restore_snapshot` would say
what they do. The call site already passes the tag `"bodycap"`.

Neither affects behaviour. Filed so the next reader of that code is not sent
looking for a scope feature that was deleted.

## Verified 2026-09-12

Both traces still present. Line numbers drifted from the filing:

- `parse_local` in comments only — `sash/src/parse/parser.rs:14` and `:648`.
  The function does not exist.
- `restore_locals` @ `transform/stmt_handlers.rs:1417`, `save_locals` @ `:1435`
  (filing said 1409 / 1427).

Cosmetic, no behaviour change. Confirmed still worth doing exactly as filed.

## DONE 2026-09-12

Both traces removed, exactly as filed.

1. The two `parse_local` mentions in `sash/src/parse/parser.rs` (module doc
   list and the `declaration_axes` comment) are gone. The function had not
   existed since DD#86 phase 2.
2. `save_locals` -> **`snapshot_assigned`**, `restore_locals` ->
   **`restore_snapshot`**, with doc comments that say what they do (snapshot
   the variables a body capture assigns, so a body capture's writes can be
   undone for subshell isolation) and an explicit note that they have nothing
   to do with the removed `local` keyword.

Rename + comments only. 3-mode sweep at 5305 passing / 41 failing / 559
skipped with **zero flips**, which is the evidence the change is behaviour-
neutral.
