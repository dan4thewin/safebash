# Journal 107 — validate_stmt_ctx extraction

## Goal

Ninth Phase 11 target. `validate_stmt_ctx`
(`validate.rs:69`, CC 43). The function dispatches on every
`StmtKind` variant and threads an `in_condition: bool` flag,
carrying SB5007 (mutation-in-condition), SB5009 (unquoted
for-loop var), DD#31 (comma-expression Group handling), and the
common `walk_exprs_in_stmt` post-pass.

The audit verdict: extract per-arm helpers like check_stmt, plus
collapse the trivial forwarding wrappers (Pipeline, Subshell,
Suppressed, Redirected, List) into a single default arm with a
context-forwarding predicate.

## Action

Two-tier collapse:

1. **Per-construct helpers** for arms with real logic:
   - `validate_assignment_stmt` (validate_assignment + SB5007
     mutation check)
   - `validate_if` (4 body sets, mixed in_condition)
   - `validate_while` (2 body sets, mixed in_condition)
   - `validate_for` (word check + SB5009 + body)
   - `validate_group` (DD#31 comma-expression special case)

2. **Default arm with `forwards_condition` predicate** for the
   forwarding wrappers:
   ```rust
   _ => {
       let inner = forwards_condition(stmt) && in_condition;
       for body in stmt.child_bodies() {
           for s in body {
               validate_stmt_ctx(s, diags, inner);
           }
       }
   }
   ```
   `forwards_condition` returns `true` for Pipeline / Subshell /
   Suppressed / Redirected / List, and false for Function / Case
   (whose bodies must reset to non-condition regardless).

This is the first refactor in this wave where the default arm
*also* threads state. The journal 056 risk doesn't apply here
the same way — `child_bodies()` returns the same slices each
explicit arm was walking, AND every wrapper kind here is in
forwards_condition (or in Function/Case which want false), so
the in_condition propagation matches the original behaviour
exactly.

## DD#31 in `validate_group`

The original Group arm had a tangle: an `if in_condition && last
is ArithTest` branch with its own enumerate loop, and an `else`
branch with the straightforward forward loop. Restructured with
early return:

```rust
let is_comma_pattern = in_condition && last_is_arith_test;
if !is_comma_pattern {
    // straightforward forward path
    for s in &g.body { validate_stmt_ctx(s, diags, in_condition); }
    return;
}
// DD#31 path: last stmt validates as condition, prior stmts as statements
let last = g.body.len() - 1;
for (i, s) in g.body.iter().enumerate() {
    validate_stmt_ctx(s, diags, i == last);
}
```

The `i == last` pattern collapses the explicit if/else inside
the loop. Same semantics.

## Verification

```
validate_stmt_ctx        CC 43 → 15
validate_if                    9   (new)
validate_for                   6   (new)
validate_group                 5   (new)
validate_assignment_stmt       3   (new)
validate_while                 3   (new)
forwards_condition             2   (new)
```

- `cargo build --release` — clean
- `cargo clippy --quiet -- -D warnings` — clean
- `cargo test --lib --quiet` — 12/12
- `bash spec/run.sh` — 291/291

`validate_stmt_ctx` drops out of the top-5. New top-5:

```
26 emit_test_expr      (deferred — emit_janet rewrite)
21 check_dq_stmt       (newly surfaced; in validate.rs)
19 infer_stmt          (type_check.rs)
17 scan_value_for_vars
16 parse_command_substitution
```

## Why this came out at audit estimate

Audit predicted CC ~15. Hit exactly 15. The DD#31 Group case
absorbed less than expected (CC 5 not 8) because of the
early-return restructure, but `validate_if` ate that back (4
distinct body sets all needing in_condition wiring).

## Phase 11 wave complete

Nine refactors plus one hook fix landed. All 13 active candidates
from the post-Phase-10 audit are either done (10) or explicitly
deferred (3 — emit_test_expr awaits emit_janet rewrite,
emit_command_kind is cyclo-allow'd, file_test_op_code is filed for
followup).

Live top-5 has shifted from:
```
49 check_stmt
43 validate_stmt_ctx
26 emit_test_expr
22 check_dq_stmt
21 scan_expr_positionals_*
```
to:
```
26 emit_test_expr      (deferred)
21 check_dq_stmt       (next wave candidate)
19 infer_stmt          (already on the radar)
17 scan_value_for_vars
16 parse_command_substitution
```

The maximum live CC dropped from 49 to 26, and 26 is in
emit_janet — the *real* live ceiling in the SSA pipeline is now
21.
