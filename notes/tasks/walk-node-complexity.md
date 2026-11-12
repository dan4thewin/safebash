title: "`walk_node` is cc 39 / cog 44, outside the gate by scope"
status: open
priority: P4

# `walk_node` is cc 39 / cog 44, outside the gate by scope

Noted 2026-09-27 (journal 427) while adding the generic `ERROR` warning.

`sash-convert/src/convert.rs::walk_node` measures **cc 39 / cog 44** against
a `sash/src` mean of roughly 3.8 / 1.9. The enforced complexity gate scopes
to `file LIKE 'sash/src/%'`, so `sash-convert` is exempt **by construction
rather than by merit** — nothing about the function argues it should be.

It is the single `match node.kind()` dispatcher for every conversion, so the
high cyclomatic count is the expected dispatcher shape (wide and shallow) and
would be exempted by the `cog <= 2` rule if it were shallow. **It is not** —
cog 44 says the arms carry real nesting, which is the "high both" row of the
rubric: a genuine refactor target.

## Not urgent

No correctness problem is attached to it, and every arm is individually
small. The cost of leaving it is that each new arm lands in a function
already past the point where the gate would reject it if it lived in
`sash/src`.

## If picked up

The natural seam is per-kind arm extraction, the way `sash/src/transform/`
was split into `stmt_handlers.rs` / `cmd_handlers.rs` / `expr_handlers.rs`.
That precedent exists and is the reason those transform functions now measure
in line with Mozilla exactly.

**Re-measure first** — the numbers above are a snapshot, and AGENTS.md is
explicit that a quoted complexity figure is wrong by the time it is read:

```sh
sqlite3 .indexnudge-rust.db "SELECT name, cyclomatic, cognitive FROM symbols
  WHERE file='sash-convert/src/convert.rs' AND kind='function'
  ORDER BY cognitive DESC LIMIT 10;"
```

## Open question for DG

Whether the gate *should* cover `sash-convert` at all. It is a real crate
with real contributors, and the current scope is not a stated decision so
much as where the gate happened to be pointed. Raising it would put this
function in violation immediately, but the gate is grandfather-on-modify, so
the practical effect is "fix it when you next touch it" rather than a
backlog.
