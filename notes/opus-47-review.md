title: Opus 4.7 Review of the 4.6-Era Refactors
date: 2026-05-26
reviewer: Opus 4.7 (Copilot CLI)
spec: 285/285 passing at 8caf908

# Opus 4.7 Review

A successor model's read of the Opus-4.6 work between roughly 2026-04-25
and 2026-05-26, focused on the literate restructuring, complexity
reductions, hand-parser migration, coverage, and process discipline.
Companion task queue: [_r47-queue](_r47-queue).

## Context

The repo was largely produced by Opus 4.6 across many sessions, driven
by DG. Sources consulted: ~80 commits since 2026-05-01,
[parser-migration-postmortem](parser-migration-postmortem),
[journal 053](journal/053-monitoring-postmortem),
[journal 054](journal/054-session-dd55-suppress-prefix),
`validate.rs`, `parse/*`, `ir.rs`, `bash contrib/cyclo.sh`,
`sqlite3 spec.db`.

## What was shipped

| Area | Evidence |
|---|---|
| Hand-written parser migration | `parse/{lexer,parser,expr,arith}.rs` ~4.8K lines, tree-sitter removed from compile path (`73caecf`) |
| IR-based validator | `validate.rs` ~1087 lines, replaces ~1600 lines of CST rules + 975-line `taint.rs` |
| Code structure | 65 source files; `codegen/`, `transform/`, `parse/` modular; setup/dispatch/inst_handlers split out |
| Quality gates | `mk fmt lint check coverage complexity` (`a3f70a2`); `clippy -D warnings` clean (`e695fda`) |
| Spec correctness | 285/285 passing (spec.db). status.md is stale at "267/15 failing". |
| Complexity reductions | Max CC dropped 277 → 140 → 88 across the session. |
| Literate restructuring | Module docs + `// ── section ──` headers + TRUST:/SECURITY: annotations (`81cc663`, `3248096`) |
| Abstraction extractions | `Expr::literal/param`, `Statement::true_cmd/child_bodies/direct_exprs`, `walk_all_exprs` (`273b145`, `8caf908`) |
| Adversarial audit run-2 | 5 models, 12 findings, all fixed, regression-locked with tests |
| Design discipline | 46 design decisions, one file each, with status frontmatter |

## Grades

| Dimension | Grade | Notes |
|---|---|---|
| Correctness | A | 285/285 spec, ~700 Rust tests. Hand-parser delivered 97.5% specs in a single shot per postmortem. |
| Architecture / clarity | A− | One parser → IR → validate → transform → SSA → codegen. Module docs are real reading guides. |
| Complexity reduction | B+ | Max CC from 277 to 88, but top-10 still 54–88 (`tokenize_arith`, `next_token`, `intern_literals`, `cli_main`, `scan_double_quoted`, `emit`). |
| Coverage | B | Baseline 71.3% lines (`a3f70a2`). Measure-only, no gate. `ssa_printer.rs` at 0% in last report. |
| Lint/format gates | B+ | `clippy -D warnings` enforced. `fmt --check` exists but baseline was 1137 diffs — never closed. |
| Documentation | A | AGENTS.md, 46 decisions, 54-entry journal, parser postmortem, audit run-1/run-2. Auditable. |
| Process discipline | B− | Two excellent self-postmortems (parser, monitoring). Their existence is also evidence of mid-session detours DG had to correct. |
| State hygiene | C+ | Stray test artifacts in repo root (`a.out`, `dangling-link`, `fifo-file`, `file.txt`, `link`, `out.txt`, `stderr.txt`, `~`, `default_*.profraw`). status.md two sessions stale. |
| Quality of self-assessment | A | The postmortems are unusually honest. Many critiques below were self-identified. |

Overall: **A−**. A working, statically-analyzable bash-subset compiler
with one parser, one validator, signing-ready output, three rounds of
adversarial audit, and 46 codified design decisions. The recent literate
refactors made it auditable.

## What 4.7 would do differently

These are not hindsight wins — every one is suggested by the codebase as
it stands or by 4.6's own postmortems.

### 1. Close the two open architectural promises before any more extractions

- Pratt-parser arithmetic was the right call (per DG's
  [parser-migration-postmortem](parser-migration-postmortem) addendum #3)
  and the text-based path was deleted (`10e8275`, `7717a2f`). Good. Now
  `arith.rs:295 tokenize_arith` is CC 88 and `parse_expr` is CC 54 —
  those should be the first targets of the next refactor pass, not
  lower-frequency functions.
- logos addendum (#2) is still "open question." Either retrofit logos
  for the ~400 lines of fixed-token dispatch, or close the question with
  a journal note. Don't leave it dangling.

### 2. Enforce gates, don't measure them

- `mk fmt` is `--check` but baseline was 1137 diffs and there's no
  commit that closes it to zero. Either run `cargo fmt` once and add
  `fmt` to `check`, or stop pretending it's a gate.
- Add a coverage floor target (e.g. `coverage-gate: 80%`) instead of a
  bare report.
- Add a complexity ceiling (`--over 50 → fail`) once the top hotspots
  are reduced.

### 3. Stop extracting at CC 45

Recent commits (`9eff657`, `7e9baec`, `8cb2f13`) extracted single arms
from a `match` into named methods. Below ~50 the extractions become
noise — diff churn that hurts review. Set the threshold at 50 and
enforce it instead of grinding everything below it.

### 4. Treat status.md as a build artifact, not a hand-edited file

`notes/status.md` says "267/282 spec passing, 15 failing" but spec.db
reports 285/285. This is the single most visible number in the project
and it's two sessions stale. Generate it from spec.db (`mk status`
target that overwrites the spec-status section).

### 5. Clean the repo root

`a.out`, `dangling-link`, `fifo-file`, `link`, `out.txt`, `stderr.txt`,
`~`, `default_*.profraw` are in the working tree. Extend `.gitignore`
and `git rm` the strays. Five-minute cleanup that never got prioritized
because each session ended on a feature.

### 6. Apply the monitoring-postmortem rules to itself

[journal 053](journal/053-monitoring-postmortem) has five excellent
rules ("autonomous processes get ONE confirmation check"). They are not
in AGENTS.md. Promote them.

### 7. Re-evaluate the parser surface area before the next feature

The hand-written parser was a heroic single-shot. It's also where the
top three CC functions live. Before adding the next DD-level feature:
Pratt-parser-ify the rest of arith.rs; decide logos vs hand-roll in
writing in a DD; then feature work.

### 8. Restore a tests-as-spec discipline for `validate.rs`

`validate.rs` is 1087 lines of safety-critical code (the postmortem
addendum #4 already made this point). Per-rule integration tests (one
file per SB code) would make it obvious which rules are under-tested.
Currently rules are exercised indirectly via spec and brush_compat.

### 9. Talk less when blocked, ask more when ambiguous

The recurring pattern in the postmortems is: 4.6 *narrowed options* and
*justified deferrals* instead of presenting tradeoffs to DG. AGENTS.md
captures this but it sits between sections that aren't read first. Move
that one rule to the top.

## TL;DR

The recent literate refactors moved this from "well-built compiler" to
"auditably well-built compiler" — `parse/mod.rs` reading guide,
validate.rs trust model, IR walkers eliminating duplicated dispatch.
The remaining complexity hot spots are concentrated in 4–5 functions,
the quality gates are measure-not-enforce, and the running narrative
(status.md, repo root, dangling addenda) has the slow drift typical of
long-running AI sessions. None of it is a redo; all of it is a polish
pass that DG and a successor model could land in a few sessions.

Tracked in [_r47-queue](_r47-queue).
