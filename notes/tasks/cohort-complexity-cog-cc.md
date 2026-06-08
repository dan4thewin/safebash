---
title: Ongoing complexity reduction via cog/cc rubric
status: active
priority: P2
kind: work
depends: []
gates:
  - top non-grandfathered function clears the rubric on touch
  - new violators never land (gate blocks them)
  - plan to tighten threshold 3× → 2× → 1.5× as cohort clears
---

# Ongoing complexity reduction via cog/cc rubric

The named-hotspot work in
[r47-reduce-parser-complexity-hotspots](r47-reduce-parser-complexity-hotspots)
closed 2026-06-05 with all 6 targeted parser hotspots refactored.
Phases 11–12 then introduced the cognitive-vs-cyclomatic rubric
([journal 108](../journal/108-cog-cc-rubric.md)) and the
grandfather-on-modify complexity gate (commit `c51e296`).

This task captures the **standing cohort work** the gate is built
around. It is the home for ongoing refactor waves, the
threshold-tightening plan, and the rolling list of violators.

## The rubric (verbatim from journal 108)

```
reject if (cog - cc > -15)                     -- not a pure dispatcher
       AND (cc > 3×mean OR cog > 3×mean)       -- AND high on either axis
```

Means are recalculated per `indexnudge --freshen`. Current (2026-06-07):

```
mean cc = 5.50, threshold = 16.49
mean cog = 3.82, threshold = 11.46
```

Pre-existing violators are grandfathered. The gate triggers only
when a commit's staged hunks intersect a violator's line range —
the function must be brought into compliance before the touching
commit lands.

## Current cohort (2026-06-07)

15 grandfathered violators across the codebase. Top by axis ratio:

| function                            | file                                    | cc | cog | delta |
|-------------------------------------|-----------------------------------------|----|-----|-------|
| `emit_stmt`                         | `sash/src/emit_janet.rs`                | 39 |  59 |   20  |
| `emit_expr_inline`                  | `sash/src/emit_janet.rs`                | 31 |  27 |   -4  |
| `lower_ir_redirect_expr`            | `sash/src/codegen/redirects.rs`         | 16 |  27 |   11  |
| `scan_brace_expansion`              | `sash/src/parse/lexer.rs`               | 18 |  26 |    8  |
| `handle_pipe_while_or_mapfile`      | `sash/src/transform/stmt_handlers.rs`   |  8 |  25 |   17  |
| `handle_function_call_or_command`   | `sash/src/transform/cmd_handlers.rs`    | 16 |  23 |    7  |
| `lower_redirected_stmt`             | `sash/src/transform/statements.rs`      | 15 |  23 |    8  |
| `emit_manifest`                     | `sash/src/lib.rs`                       |  9 |  23 |   14  |
| `emit_redirected`                   | `sash/src/emit_janet.rs`                | 17 |  22 |    5  |
| `lower_declaration_stmt`            | `sash/src/transform/statements.rs`      | 13 |  22 |    9  |
| `tokenize_arith`                    | `sash/src/parse/arith.rs`               | 12 |  22 |   10  |
| `lower_expr_value_as_int`           | `sash/src/codegen/values.rs`            | 10 |  22 |   12  |
| `parse_command_substitution`        | `sash/src/parse/expr.rs`                | 16 |  21 |    5  |
| `taint_command`                     | `sash/src/validate.rs`                  | 13 |  21 |    8  |
| `check_subscript_types`             | `sash/src/type_check.rs`                | 13 |  21 |    8  |

Refresh this list periodically:

```bash
sqlite3 .indexnudge-rust.db "
WITH stats AS (
  SELECT AVG(cyclomatic) AS mcc, AVG(cognitive) AS mcog
  FROM symbols
  WHERE file LIKE 'sash/src/%' AND kind IN ('function','method')
    AND cyclomatic IS NOT NULL
)
SELECT s.name, s.file, s.cyclomatic AS cc, s.cognitive AS cog,
       (s.cognitive - s.cyclomatic) AS delta
FROM symbols s, stats
WHERE s.file LIKE 'sash/src/%' AND s.kind IN ('function','method')
  AND (s.cognitive - s.cyclomatic) > -15
  AND (s.cyclomatic > stats.mcc*3 OR s.cognitive > stats.mcog*3)
ORDER BY MAX(s.cyclomatic / stats.mcc, s.cognitive / stats.mcog) DESC;
"
```

## Pure dispatchers (excluded by rubric)

Functions with `delta = cog - cc ≤ -15` are excluded: high CC but
flat structure (one arm per variant, no nesting). They show in the
top-5 cc report but never trigger the gate:

- `to_tok` (parse/logos_tokens.rs) — cc=48, cog=1
- `from_name` (ssa.rs) — cc=37, cog=1
- `classify_word_token` (parse/lexer.rs) — cc=35, cog=1

These should stay flat. If any of them ever sprouts nesting they'd
fail on the delta check (cog goes up but cc stays flat → delta
shrinks → eligible for the rubric).

## Threshold-tightening plan

The gate is currently 3× mean. As the grandfathered cohort clears,
tighten:

1. **3× mean (now).** ~15 violators grandfathered. Each touch in
   `sash/src/` triggers refactor.
2. **2× mean.** Once cohort clears to ~5 or fewer violators.
   Currently 2× would be cc > 11, cog > 7.6 — would re-catch
   dozens of moderate functions.
3. **1.5× mean.** Stretch goal. Currently cc > 8.25, cog > 5.7 —
   essentially the population mean + halo.

Tightening doesn't re-grandfather; pre-existing functions above the
new threshold become new cohort members.

## Strategy notes

- **Cog drives the refactor priority, not cc.** A cog=22 dispatcher
  with cc=10 is harder to read than a cc=22 function with cog=8.
  Sort cohort by cog/mean_cog ratio.
- **Touch-driven, not campaign-driven.** Don't preemptively refactor
  a grandfathered function. Wait for a real reason to edit it; the
  gate forces the refactor at that point.
- **Pure dispatchers stay.** The delta filter exists exactly to
  preserve them.
- **One refactor per commit.** Same convention as
  r47-reduce-parser-complexity-hotspots — each violator clears in its
  own commit with all gates green.

## References

- [journal 108](../journal/108-cog-cc-rubric.md) — rubric derivation
- Commit `c51e296` — gate enforcement
- [r47-reduce-parser-complexity-hotspots](r47-reduce-parser-complexity-hotspots)
  — closed predecessor (named hotspots, parser-side)
- [r47-set-extraction-floor-cc50](r47-set-extraction-floor-cc50) —
  superseded predecessor (numeric floor approach)
- `.githooks/pre-commit` — gate implementation
- `extract-instctx-setup` — specific cohort target also tracked separately
