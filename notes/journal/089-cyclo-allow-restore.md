# Journal 089 — restore cyclo-allow as load-bearing signal

## Goal

The pre-commit hook's top-5 CC report had become noise: 4 of the
top 5 hits were intrinsics (`print_arith_expr`, `to_tok`,
`arith_expr_to_janet`, `print_stmt`) — all annotated with
`// cyclo-allow:` but the annotation was dead-letter.

Wanted to restore the original 2026-05-31 intent (commit `1dc651e`):
intrinsics filtered out, only "real" CC hotspots surface.

## Background

Three artifacts coexisted briefly in late May 2026:

1. **`contrib/cyclo.sh`** — top-N report tool, filtered by
   `.cyclo-allow`. **Retired in `967af30`** when indexnudge v9 landed.
2. **`.cyclo-allow`** (repo root, 119 lines) — `path::fn # rationale`
   whitelist consumed by cyclo.sh. **Deleted** alongside cyclo.sh.
3. **`// cyclo-allow: <reason>`** comments above each whitelisted fn —
   human-visible markers, kept in sync with `.cyclo-allow` by
   `contrib/cyclo_insert_allow_comments.py`.

After (1) and (2) went away, the comments survived as **inert
documentation**: nothing read them, the new pre-commit report (a
plain `SELECT cyclomatic, name FROM symbols ORDER BY cyclomatic
DESC LIMIT 5`) ignored them entirely.

## Approach

DG asked the natural question: "the top-n is just a sql query — couldn't
the query be modified to do the exclusion?"

Yes. `lines` table in `.indexnudge-rust.db` has full source content
per `(file, line_num)`, so we can `LEFT JOIN` on `start_line - 1`
and filter on the line content. Single-query change. No script needed.

## Result

### Pre-commit SQL

```sql
SELECT s.cyclomatic, s.name, s.file||':'||s.start_line
FROM symbols s
LEFT JOIN lines l
  ON l.file = s.file AND l.line_num = s.start_line - 1
WHERE s.kind IN ('function','method')
  AND s.cyclomatic IS NOT NULL
  AND s.file LIKE 'sash/src/%'
  AND (l.content IS NULL OR TRIM(l.content) NOT LIKE '// cyclo-allow:%')
ORDER BY s.cyclomatic DESC LIMIT 5;
```

### Before/after top-5

Before (contaminated):
```
49 check_stmt
48 print_arith_expr     ← annotated intrinsic
48 to_tok               ← annotated intrinsic
44 arith_expr_to_janet  ← annotated intrinsic
43 print_stmt           ← annotated intrinsic
```

After (filtered):
```
49 check_stmt           ← actual C-verdict, journal 056 doc
43 validate_stmt_ctx
29 scan_value_for_vars  ← already in Phase 10 backlog
29 print_ssa
28 emit_command_kind    ← Phase 10 candidate
```

Every line of the new top-5 is actionable — either a known C-verdict
deferred for a reason or a Phase 10 candidate.

### Surfacing of one missing annotation

The filtered query immediately surfaced `classify_word_token` at
CC 35 — the lexer's keyword table, declared intrinsic in journal 063
but never annotated. Added the comment (citing the journal). Now
filtered out as intended.

### Retirement

`contrib/cyclo_insert_allow_comments.py` is gone. The annotation is
now hand-added when an author declares a fn intrinsic. Single source
of truth: the comment in source. No script-driven sync. No
`.cyclo-allow` file to maintain.

## Why this matters

This restores a **load-bearing tooling signal** at low cost:

- **The annotation has a consumer again.** Authors writing a new
  intrinsic (e.g., a future case-handler table) have a reason to
  add the comment beyond "it's the convention" — it actually
  controls the report.
- **Threshold enforcement is now a one-line change.** When DG
  decides to wire up a CC ceiling, adding `WHERE s.cyclomatic > N`
  to the same query (and `exit 1` if any rows return) drops in
  without further plumbing.
- **The top-5 report regains review value.** Currently every commit
  prints the same top-5; with the filter, the report changes only
  when a refactor actually moves a C-verdict in/out of the danger
  band, which is what you want to see.

## Files touched

- `.githooks/pre-commit` — SQL block replaced with LEFT JOIN
  variant. Comment block clarifies what the filter does.
- `sash/src/parse/lexer.rs:1147` — added missing `// cyclo-allow:`
  above `classify_word_token`.
- `contrib/cyclo_insert_allow_comments.py` — **deleted**.
- Plan refreshed (Phase 9 results table through `e403807`; Phase 11
  documents this housekeeping).

## Gates

- `cargo build --release` (no source change beyond the comment)
- `cargo clippy -D warnings` (clean)
- `cargo test --lib` (12/12)
- `spec/run.sh` (291/291)
- Hook live-test: pre-commit fired during commit, filtered top-5
  matches the expected listing above.
