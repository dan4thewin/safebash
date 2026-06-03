# Journal 093 — taint_stmt StmtKind dispatcher floor

## Goal

Third Phase 10 target. `taint_stmt` (`validate.rs`, CC 28) —
C-verdict from `r47-abstraction-audit.md:303`. Recipe: "StmtKind
dispatcher, Pipeline/Redirected have distinct logic. Extract
`taint_pipeline`, `taint_redirected_read`."

## Pattern

8 StmtKind arms, mostly with multi-line inline bodies:

- Assignment: predicate (2-OR) + insert
- Declaration: for + same predicate + insert
- Command: 1-line delegate (already clean — `taint_command`)
- Function: clone tainted, strip $1-$9, recurse on body
- Pipeline: detect first-stage-external + conditional per stage
- Redirected: herestring-read exception + recurse
- Case: discriminant taint + nested recursion
- _ wildcard: child_bodies recursion

Assignment and Declaration repeated the same `is_taint_source ||
expr_uses_tainted → insert` shape with different access patterns
(single vs iterated).

## Refactor

Five helpers extracted, all named after their semantic role:

| helper | what it owns |
| --- | --- |
| `taint_assignment_value(name, value, tainted)` | shared 2-cond predicate + insert |
| `taint_function_body(body, tainted, diags)` | $1-$9 clean-arg shadowing + recurse |
| `taint_pipeline(p, tainted, diags)` | first-stage-external rule |
| `taint_redirected(r, tainted, diags)` | herestring-read exception + recurse |
| `taint_case_arms(c, tainted, diags)` | discriminant + arm recursion |

Wildcard arm's 2-line `child_bodies` loop left inline — already
concise and is the catch-all for the variants without distinct
semantics.

`taint_assignment_value` is the cross-arm dedup: Assignment uses
`&a.name` directly, Declaration iterates `&d.assignments`. Both
call the same helper now.

## Result

| function | before | after |
| --- | --: | --: |
| `taint_stmt` | 28 | 12 |
| `taint_assignment_value` (new) | — | 3 |
| `taint_function_body` (new) | — | 3 |
| `taint_pipeline` (new) | — | 4 |
| `taint_redirected` (new) | — | 6 |
| `taint_case_arms` (new) | — | 3 |

Net: −16 CC in the parent, +19 CC distributed across 5 helpers
that each name one rule of the taint propagation algorithm. No new
hotspot — all helpers under the audit band.

## Insight

This kind of StmtKind dispatcher in a static analyzer is the prime
case for "name every rule." The taint algorithm has rules:
"assignment from a source taints the LHS," "function call shadows
positional args," "pipeline propagates from external first stage,"
"herestring read is clean." Pre-refactor, those rules were embedded
in a 70-line match. Post-refactor, each rule is a named function
with its preconditions in the signature and a one-line summary in
the doc comment. The CC drop (28 → 12) is a side effect of
restoring the abstraction layer the audit wanted.

This generalizes the Phase 9 pattern (Wave 1 statement walker
dispatchers — j081 `lower_one`, j082 `lower_stmt`, j085 `walk_stmt`)
to one more walker. validate.rs's other big walker `check_stmt` (CC 49)
is the obvious next candidate but is marked C-wrapper in the audit.

## Gates

- `cargo build --release` clean
- `cargo clippy -D warnings` clean
- `cargo test --lib` 12/12
- `spec/run.sh` 291/291

## Files touched

- `sash/src/validate.rs` — extracted 5 helpers; `taint_stmt` arms
  collapsed to single-line calls.
- `notes/tasks/r47-abstraction-audit.md:303` — annotated.
