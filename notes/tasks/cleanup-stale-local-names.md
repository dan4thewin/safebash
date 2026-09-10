---
title: "cleanup: `local`-era names and doc comments outlived DD#86"
status: ready
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
