# 057 — elif walker bugs and child_bodies rollout (#1 and #2)

## Background

Journal 056 fixed `check_stmt`'s elif-and-grouping bug. The
per-walker audit recorded in that journal flagged two more walkers
with the same shape — they each had explicit recursion arms for
`StmtKind::If` that omitted `elif_clauses`. Quick repros confirmed:

```text
$ sash -c 'fn f -> int { if false; then return 0; elif true; then return 5; fi; }
           int x=$((f)); echo "$x"'
sash: error [SB4017] function 'f' only returns 0 ...    # spurious!

$ sash -c 'fn f { if (($#==0)); then :;
                  elif true; then echo $5; fi; }
           f a b c'
(no error)                                              # silent accept of out-of-range $5
```

Both bugs surface in real user-visible behavior — one as a spurious
rejection, the other as a silent acceptance.

## Spec coverage first

DG proposed adding regression tests under an implicit "DD#0 —
support bash syntax where not otherwise specified". elif is plain
bash; sash should apply every compile-time analysis to it
uniformly. New spec series `0.N`:

| id | file | scenario |
|----|------|----------|
| 0.1 | types.sh | type-check descends into `{ ... }` |
| 0.2 | types.sh | type-check descends into left of `&&` |
| 0.3 | types.sh | type-check descends into right of `&&` |
| 0.4 | types.sh | type-check descends into pipeline stages |
| 0.5 | errors.sh | collect_returns walks elif (no spurious SB4017) |
| 0.6 | errors.sh | scan_expr_positionals walks elif (catches out-of-range $N) |

0.1–0.4 covered the check_stmt fix already shipped. 0.5–0.6 were
red until the walker fixes below.

## The two fixes

Both follow the same shape as the check_stmt fix in journal 056:
keep the explicit arms that do real per-StmtKind work, collapse
the mechanical recursion arms into a default arm that delegates
to `ir::Statement::child_bodies()`.

### 1. `scan_expr_positionals_stmt` (helpers.rs)

Kept explicit:

- `Command` (detects `shift` and scans name+args)
- `Assignment`, `Declaration`, `For`, `Case` (scan direct exprs)
- `Function` (skip — nested fns have their own scan)

Collapsed: `Pipeline`, `If`, `While`, `Redirected`, `List`,
`Subshell`, `Group`, `Suppressed`, `Comment`, `ArithTest` →
default `_ => for body in stmt.child_bodies() { ... }`.

CC: 37 → 21.

### 2. `collect_returns_inner` (transform/functions.rs)

Kept explicit:

- `Command` (detects `return` and records its argument)
- `Subshell` (no-op — `return` inside `(...)` exits the subshell only)
- `Function` (no-op — nested fns own their returns)

Collapsed: `If`, `While`, `For`, `Case`, `Group`, `Pipeline`,
`List`, `Redirected` → default `_ => for body in stmt.child_bodies()`.

CC: 21 → 13.

### Side-benefit bug fix

The original `collect_returns_inner` `_ => {}` arm dropped
`Suppressed` along with its no-op arms. With the new default,
`Suppressed` correctly descends. The user-visible impact is
microscopic (a `- return X` wrapper is exotic), but it's the right
shape.

## Verification

- 291/291 spec gates green (was 285 before; +6 from the 0.N
  series).
- `cargo test --lib`: 12/12.
- `cargo test --test oracle_tests`: 45 passed / 18 failed
  (baseline — the 18 are pre-existing stale-`declare`-syntax tests,
  unchanged before vs. after).
- `cargo clippy -D warnings` on `--lib` and `--test oracle_tests`:
  clean.

## Remaining pattern-#5 walkers

- `validate_stmt_ctx` (CC 43): not really a child_bodies()
  candidate. Read closer this session; the arms each express real
  per-StmtKind semantics threading an `in_condition` flag. Only the
  five pure-passthrough arms (Pipeline / Redirected / List /
  Subshell / Suppressed) are mechanical. A `child_bodies_with_ctx`
  helper would save maybe 4–6 CC at the cost of hiding the
  passthrough decisions. Leave as-is.
- `check_dq_stmt` (CC 21): not applicable. The `_ =>` arm IS the
  work (state-machine for `$?` availability). It has a separate
  open question — whether For / Case / Subshell / Pipeline /
  Redirected / Suppressed are correctly handled by the leaf-stmt
  logic. That's a flow-analysis correctness review, not a
  child_bodies rollout.
- `taint_stmt` (CC 28): already uses `child_bodies()`.

`p2-child-bodies-rollout` can now be closed — every walker is
either fixed, audited as not-applicable, or left alone for stated
reasons.

## DD#0 note

The 0.N series sits in spec/{types,errors}.sh under an explicit
comment naming "DD#0 (implicit)". If DG wants this formalised as
an actual DD file, it would document: "bash control-flow
constructs (if/elif/else, while, for, case, grouping, pipelines,
lists) are part of the safe subset and every compile-time analysis
must apply uniformly across them. Walker regressions are spec'd in
the 0.N series."
