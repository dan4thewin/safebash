---
status: open
priority: P4
depends: none
gates: none
---

# `resolve.rs` guards made unreachable by `run_structural_checks`

Found during the branch-coverage campaign. Two zero-hit arms in
`sash/src/resolve.rs`, both with heavy sibling traffic:

| line | arm | hits | sibling |
|------|-----|-----:|--------:|
| 105  | `name.is_empty()` in `is_resolvable_external` | 0 | 8180 |
| 207  | `let ExprKind::Literal(name) = &c.name.kind else { return }` | 0 | 8188 |

## Why they are unreachable

`lib.rs:165` runs `run_structural_checks` **before** `resolve::resolve_commands`
(`lib.rs:170`), and a bad command head is fatal there — the process exits.
Verified by instrumenting `resolve_command` with an `eprintln!` and rebuilding:

```
echo hi                      -> resolve_command called (1)
str c = "/bin/echo"; $c hi   -> NOT called (0) — SB5011 from validate, exits first
"" hi                        -> NOT called (0) — same
'' hi                        -> same
```

So by the time `resolve_command` runs, the head is guaranteed to be a
non-empty `ExprKind::Literal`.

## Why this was NOT converted to a panic

Unlike the lexer arms settled in journal 312, this invariant is **cross-pass**,
not local. It holds only because one caller happens to sequence the structural
pass first. The `let-else` is also a totality requirement — the pattern is
refutable, so *some* arm must exist; the only question is `return` vs `panic!`.

Turning it into a panic would couple `resolve` to `run_structural_checks` and
make any future entry point that resolves without structural checking abort
instead of no-op. The current behaviour (skip what you cannot resolve) is the
safer default.

## What to do

Nothing urgent. If the pass pipeline is ever restructured so resolution can be
invoked standalone, revisit. Recorded so the coverage ranking does not
re-surface these two lines as candidates.
