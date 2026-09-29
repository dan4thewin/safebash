---
title: "`walk_scoped_stmts` now enters `$( )` bodies — thirteen callers unaudited"
status: open
priority: P3
kind: audit
depends: []
gates:
  - "each of the thirteen callers is classified as correct-by-design or pinned"
---

# What

Journal 444 moved `$( … )` bodies onto the statement side of the IR, so
`child_bodies()` yields them. `walk_scoped_stmts` takes `child_bodies()` and
therefore now descends into bodies. It has **thirteen callers** — nine in
`validate.rs`, two in `type_check.rs`.

The full spec sweep is green at 6961 with the widening in place, and the
widening is the *intent* of DD#86 (a body is a function body, so a `fn`
declared inside one really is nested). But each caller was not individually
reasoned about, and journal 444's own experience is that a green sweep is
evidence about what it covers, not about what changed — a silently-accepted
`$RANDOM` in a body survived 6913 green rows.

# What to do

For each caller, decide and record: does entering a body give the right answer,
or does the pass need `structural_bodies()`? The classification rule is in the
DD#86 addendum of 2026-09-29.

Pay particular attention to any caller that mutates scope state — that is the
one category the addendum pins unconditionally.

# Where

- `sash/src/ir.rs` — `walk_scoped_stmts`, `child_bodies`, `structural_bodies`
- 13 call sites; find them with
  `sqlite3 .indexnudge-rust.db "SELECT caller_file, caller_symbol, line_num FROM calls WHERE callee='walk_scoped_stmts';"`

Filed 2026-09-29, journal 444.
