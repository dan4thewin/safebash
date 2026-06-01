# 066 — extracting lower_case transform phases

## Background

`lower_case` in `sash/src/transform/control_flow.rs` was the
last A-verdict function at CC ≥ 40 (CC 40, 220 lines). The
audit (line 194) named the phases explicitly:

> Multi-phase: build bodies, wire fall-through, build dispatch,
> build no-match. Split phases.

## Stop-the-world: dead code found

Investigating before extracting, found:

- Lines 473-481 (loop A): set each `case_exit` terminator to
  fall-through OR merge.
- Lines 490-501 (loop B, ten lines later): set each `case_exit`
  terminator to fall-through OR resume OR merge.

Loop B writes the same blocks loop A just wrote. Loop A's writes
are unconditionally overwritten. Verified case-by-case:

- `resume=false`: loop B reduces to `fallthrough ? next : merge`,
  identical to loop A.
- `resume=true`: loop B overwrites loop A's `merge` with the
  resume target.

Pure dead code — no behavior change. The first loop was apparently
left over from an earlier iteration before the `;;&` (resume)
terminator was added; the comment "Pre-create check blocks for
dispatch chain (needed for ;;& resume wiring)" on line 483
matches that history.

Not a bug per the zero-tolerance policy (no behavior was wrong),
but cruft worth surfacing. Removed in the same commit.

## What changed

Seven focused phase-helpers plus three small utilities:

- `build_case_bodies(c, ssa, state, pre_state) -> (Vec<BlockId>,
  Vec<BlockId>, Vec<BranchSnapshot>)` — lower each arm body
  from `pre_state`, return (case_blocks, case_exits, snapshots).
- `build_check_blocks(ssa, n_items, entry) -> Vec<BlockId>` —
  entry + n-1 fresh `case.check.N` blocks.
- `wire_case_exits(c, ssa, …)` — single combined terminator loop
  (the dead first loop, removed).
- `build_dispatch_chain(c, ssa, …)` — wildcard / single-pattern
  / multi-pattern dispatch wiring per arm. Delegates the inner
  emission to:
  - `is_wildcard_arm(item)` — does any pattern `=` `"*"`
  - `emit_single_pattern_branch(ssa, …)` — one Test + Branch
  - `emit_multi_pattern_chain(ssa, arm_idx, …)` — Test chain
    across pattern sub-blocks
- `wire_no_match_path(c, ssa, state, …) -> Option<BranchSnapshot>`
  — when no arm is wildcard, build a `case.nomatch` block that
  defines `?` = 0 (POSIX) and redirect dispatch false-branches
  to it via:
  - `redirect_merge_false_branches(ssa, check_blocks, …)`
- `insert_case_phis(ssa, merge, branch_states, no_match, state)`
  — phi insertion across all incoming paths (≥2) or
  state-adoption (==1).

Also introduced a type alias `type BranchSnapshot = (BlockId,
HashMap<String, u32>)` to dedupe the noisy tuple type.

Parent `lower_case` is now a 27-line orchestrator: empty guard,
snapshot, six helper calls.

## Results

| function                        | before | after |
| ------------------------------- | -----: | ----: |
| `lower_case`                    |     40 |     2 |
| `redirect_merge_false_branches` |      — |    10 |
| `insert_case_phis`              |      — |    10 |
| `wire_case_exits`               |      — |     6 |
| `build_dispatch_chain`          |      — |     5 |
| `emit_multi_pattern_chain`      |      — |     4 |
| `build_case_bodies`             |      — |     2 |
| `build_check_blocks`            |      — |     2 |
| `is_wildcard_arm`               |      — |     2 |
| `wire_no_match_path`            |      — |     2 |
| `emit_single_pattern_branch`    |      — |     1 |

Parent CC **40 → 2**. Max helper CC 10. The parent is now
documentation-grade — six named phases in the order they execute.

Verification: build clean, clippy clean, sash lib 12/12, spec
291/291.

## Top hotspot list after this commit

| CC | function | file |
| --: | --- | --- |
| 49 | `check_stmt` | `sash/src/type_check.rs:243` (C) |
| 48 | `print_arith_expr` | `sash/src/printer.rs:446` (B/intrinsic) |
| 48 | `to_tok` | `sash/src/parse/logos_tokens.rs:137` (B/intrinsic) |
| 44 | `arith_expr_to_janet` | `sash/src/emit_janet.rs:1219` (B/intrinsic) |
| 43 | `print_stmt` | `sash/src/printer.rs:27` (C) |
| 43 | `validate_stmt_ctx` | `sash/src/validate.rs:69` (C, audit) |
| 40 | `value_is_tainted` | `sash/src/ssa_taint.rs` (B/whitelist) |
| 39 | `emit_stmt` | `sash/src/emit_janet.rs` |
| 37 | `from_name` | `sash/src/ssa.rs` |
| 36 | `verify_ssa_taint` | `sash/src/ssa_taint.rs` |
| 36 | `allocate_variables` | `sash/src/codegen/setup.rs` |
| 35 | `classify_word_token` | `sash/src/parse/lexer.rs` (intrinsic) |
| 35 | `lower_for` | `sash/src/transform/control_flow.rs` |

The CC 40+ tier is now entirely C-verdict or B/intrinsic. Next
A-verdict targets (per audit): `emit_stmt` (39), `lower_for`
(35), `lower_test_unary` (45 → check live), `emit_command`
(39), `allocate_variables` (36).

## Aside: bug-policy interpretation

The dead loop was cleanly factored cruft, not a behavioral bug —
no test would catch it, no output would change. Per AGENTS.md
zero-tolerance bug policy ("never work around it") the right
action was to investigate, confirm semantically equivalent, and
remove inline in the refactor. Surfacing it here so DG knows the
extraction was not purely mechanical.
