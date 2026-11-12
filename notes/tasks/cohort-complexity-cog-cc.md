title: Ongoing complexity reduction via cog/cc rubric
status: active
priority: P2
kind: work
depends: []
gates:
  - top non-grandfathered function clears the rubric on touch
  - new violators never land (gate blocks them)
  - plan to tighten threshold 3× → 2× → 1.5× as cohort clears

# Ongoing complexity reduction via cog/cc rubric

The named-hotspot work in
[r47-reduce-parser-complexity-hotspots](r47-reduce-parser-complexity-hotspots)
closed 2026-06-05 with all 6 targeted parser hotspots refactored.
Phases 11–12 then introduced the cognitive-vs-cyclomatic rubric
([journal 108](../journal/108-cog-cc-rubric.md)) and the
grandfather-on-modify complexity gate (commit `323eeb4`).

This task captures the **standing cohort work** the gate is built
around. It is the home for ongoing refactor waves, the
threshold-tightening plan, and the rolling list of violators.

## Progress (2026-06-07 wave)

Twenty-eight violators cleared in one session. Live cog ceiling dropped
from 59 (`emit_stmt`) to 16. As mean cog fell from 3.82 to 2.94, the
gate's 3× threshold ratcheted automatically from 11.46 to 8.83 — gate
tightened twice without any policy change.

| commit  | function                          | cc        | cog        | notes |
|---------|-----------------------------------|-----------|------------|-------|
| `af2bc7c` | `emit_stmt`                     | 39→17     | 59→1       | pure dispatcher |
| `07d2440` | `emit_expr_inline`              | 31→13     | 27→1       | pure dispatcher |
| `8f0e0c5` | `lower_ir_redirect_expr`        | 16→5      | 27→1       | pure dispatcher + DRY |
| `1c56fc2` | `scan_brace_expansion`          | 18→11     | 26→8       | DRY (reused skip_balanced_*) |
| `9bd8447` | `handle_pipe_while_or_mapfile`  | 8→2       | 25→2       | extracted nested loop |
| `4cb05e4` | `handle_function_call_or_command` | 16→2    | 23→1       | 9 helpers, all ≤ cog 3 |
| `f00260c` | `lower_redirected_stmt`         | 15→7      | 23→9       | predicate + 3 helpers |
| `99ad915` | `emit_manifest`                 | 9→2       | 23→1       | extracted builtin set |
| `f7af4fb` | `emit_redirected`               | 17→2      | 22→2       | 11 helpers |
| `f2ca27f` | `lower_declaration_stmt`        | 13→4      | 22→1       | one-decl orchestrator |
| `c8f89d5` | `tokenize_arith`                | 12→4      | 22→5       | next-token + op-cascade |
| `b169ef0` | `lower_expr_value_as_int`       | 10→4      | 22→1       | atoi_call helper |
| `9ffcaca` | `parse_command_substitution`    | 16→12     | 21→9       | CmdSubStep enum |
| `0d76469` | `taint_command`                 | 13→5      | 21→4       | read/bless extracted |
| `111cbc6` | `check_subscript_types`         | 13→6      | 21→3       | key-kind extracted |
| `586c2b4` | `handle_cd`                     | 10→1      | 20→0       | **bug fix** — pre-DD#53 cruft (journal 109) |
| `8dce63e` | `emit_array_join_expansion`     | 15→2      | 20→2       | 14 helpers (Cranelift emitter) |
| `4efe8ad` | `emit_exec`                     | 10→3      | 20→3       | three-path dispatcher; finalize_exec dedup |
| `a28c37c` | `collect_arith_content`         | 14→3      | 20→3       | step_*_token helpers |
| `5ed3216` | `emit_pipeline`                 | 10→3      | 19→2       | stage + wait helpers |
| `893c610` | `expr_type`                     | 15→6      | 18→1       | per-kind helpers |
| `7d539df` | `collect_returns_inner`         | 13→7      | 18→6       | maybe_collect_return |
| `c7e5d58` | `check_rejected`                | 9→2       | 18→0       | per-rejected-shape helpers |
| `7533865` | `parse_function`                | 14→7      | 17→2       | optional parens + return type |
| `b0a6026` | `emit_test_expr`                | 26→7      | 17→1       | inner-test wrap + per-op |
| `f3a3371` | `emit_while`                    | 12→2      | 17→2       | while-read vs generic |
| `aa3ccbf` | `check_cmd_in_stmt`             | 11→5      | 17→4       | is_regex_mutation + let-else |
| `1b6cd37` | `intern_inst`                   | 17→11     | 17→3       | intern_command_parts dedup |

## The rubric (verbatim from journal 108)

```
reject if (cog - cc > -15)                     -- not a pure dispatcher
       AND (cc > 3×mean OR cog > 3×mean)       -- AND high on either axis
```

Means are recalculated per `indexnudge --freshen`. Current (2026-06-07,
after the 28-commit wave):

```
mean cc = 4.85, threshold = 14.55
mean cog = 2.94, threshold =  8.83
```

Pre-existing violators are grandfathered. The gate triggers only
when a commit's staged hunks intersect a violator's line range —
the function must be brought into compliance before the touching
commit lands.

## Current cohort (post-wave, 2026-06-07)

96 remaining violators. The number rose from 15 (pre-wave) as the
threshold tightened twice (cog 11.46 → 9.84 → 8.83), pulling in
moderate functions that were previously below the cutoff. Top
cognitive complexity in the live cohort is 16. Refresh with the
query below; the live list keeps moving as the threshold tightens.

Refresh:

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

1. **3× mean (now).** Cohort cleared from 15 → 15 (ratchet kept
   surfacing new entries as means dropped). Each touch in
   `sash/src/` triggers refactor.
2. **2× mean.** Once cohort clears to ~5 or fewer violators.
3. **1.5× mean.** Stretch goal.

Tightening doesn't re-grandfather; pre-existing functions above the
new threshold become new cohort members.

## Strategy notes

- **Cog drives the refactor priority, not cc.** A cog=22 dispatcher
  with cc=10 is harder to read than a cc=22 function with cog=8.
  Sort cohort by cog/mean_cog ratio.
- **Touch-driven, not campaign-driven.** Don't preemptively refactor
  a grandfathered function. Wait for a real reason to edit it; the
  gate forces the refactor at that point. (The 2026-06-07 wave was
  one such reason: a deliberate batch session to drain the cohort.)
- **Pure dispatchers stay.** The delta filter exists exactly to
  preserve them.
- **One refactor per commit.** Same convention as
  r47-reduce-parser-complexity-hotspots — each violator clears in its
  own commit with all gates green.
- **Watch for buried bugs.** The handle_cd refactor uncovered ~140
  lines of pre-DD#53 dead code in a single function. Any refactor
  where the function is significantly more complex than the DD
  surface area requires is a candidate for "this might be cruft" —
  ask DG before just decomposing.

## References

- [journal 108](../journal/108-cog-cc-rubric.md) — rubric derivation
- [journal 109](../journal/109-pre-dd53-cruft-in-handle-cd.md) — handle_cd cruft
- Commit `323eeb4` — gate enforcement
- [r47-reduce-parser-complexity-hotspots](r47-reduce-parser-complexity-hotspots)
  — closed predecessor (named hotspots, parser-side)
- [r47-set-extraction-floor-cc50](r47-set-extraction-floor-cc50) —
  superseded predecessor (numeric floor approach)
- `.githooks/pre-commit` — gate implementation
- `extract-instctx-setup` — specific cohort target also tracked separately
- `tilde-vs-dd53` — companion bug, similar pre-DD#53 cruft
