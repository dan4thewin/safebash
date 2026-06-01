# 056 — check_stmt missed type-checking inside grouping statements

## Setup

While returning to the Phase 8 "pattern #5 rollout" task
(`p2-child-bodies-rollout`), I audited each of the six walkers the
audit had recommended for `ir::child_bodies()` migration. The
recommendation had been blanket; per-walker semantics differed
materially. One finding looked like an outright bug.

## The bug

`type_check::check_stmt` had explicit arms for `Assignment`,
`Declaration`, `Command`, `If`, `For`, `While`, `Function`, `Case` —
and a bare `_ => {}` fall-through. That fall-through silently
absorbed `Pipeline`, `Group`, `Subshell`, `List`, `Redirected`,
`Suppressed`. Wrapping any statement in `{ ... }`, `( ... )`,
`a && b`, `cmd | cmd`, or `cmd > file` made the type checker stop
descending, so type errors that fire at top level were silently
missed when wrapped.

Verified via a tiny driver (`sash/examples/check_descent.rs`,
deleted after use). With `int x=5` declared:

| form                       | top-level | wrapped         |
|----------------------------|:---------:|:---------------:|
| `x=hello`                  | ✗ caught  | n/a             |
| `{ x=hello; }`             | n/a       | ✗ MISSED        |
| `x=hello && true`          | n/a       | ✗ MISSED        |
| `true && x=hello`          | n/a       | ✗ MISSED        |
| `x=hello \| cat`            | n/a       | ✗ MISSED        |
| `case $1 in a) x=hello;;`  | n/a       | ✓ caught        |

## The fix

Replaced the bare `_ => {}` with default `child_bodies()` descent.
`child_bodies()` returns `vec![]` for the leaf StmtKinds
(Assignment/Declaration/Command/Comment/ArithTest), which each have
explicit arms above, so the change can't cause double-visits or
descent into leaves.

```rust
_ => {
    for body in stmt.child_bodies() {
        for s in body {
            check_stmt(s, env, errors);
        }
    }
}
```

## Tests

Four new oracle_tests, one per surviving wrapper form (Group,
List-left, List-right, Pipeline). All four fail without the fix,
pass with it. Subshell and compound-Redirected don't appear because
the parser rejects `( x=hello )` (SB4014) and `{ ... } > file`
(SB4021); the type checker fix still covers them when the parser
eventually accepts them.

Regression: 285/285 spec gates green; 12/12 lib tests; 41 → 45
oracle_tests passing (the +4 are the new tests). 18 pre-existing
oracle_tests / 72 codegen / 27 brush_compat / 11 janet / 28 taint
failures are all stale-syntax `declare`-style — unchanged before
vs. after the fix, separate clean-up task.

## CC

`check_stmt` went 47 → 49 (new arm adds two nested loops). The
audit's original recommendation also suggested collapsing the
explicit If/While/For/Function/Case arms now that a default helper
exists; each of those arms still carries custom pre-work
(`check_condition_type` etc.) before the recursion, so the collapse
is a separate refactor with its own design.

## Per-walker audit (recorded for the deferred rollout)

| walker                      | CC | status                                              |
|-----------------------------|:--:|-----------------------------------------------------|
| `taint_stmt`                | 28 | already uses `child_bodies()` in `_ =>` arm         |
| `check_dq_stmt`             | 21 | not applicable — `_ =>` operates on whole stmt      |
| `scan_expr_positionals_stmt`| 37 | clean rollout candidate                             |
| `collect_returns_inner`     | 21 | careful — must skip Subshell/Function explicitly    |
| `validate_stmt_ctx`         | 43 | needs state-aware helper (`in_condition` flag)      |
| `check_stmt`                | 47 | **bug-fixed this session; collapse deferred**        |

`p2-child-bodies-rollout` remains blocked pending a design call on
whether to introduce a state-aware `child_bodies_with_ctx(T)`
helper, or to do the safe individual rollouts and leave
`validate_stmt_ctx` alone.
