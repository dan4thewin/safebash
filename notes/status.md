---
created: 2026-05-01
modified: 2026-05-02
title: status
---

## How to start a session

1. Read this file first
2. Run `iwe retrieve -k safebash -d 2` for full graph context
3. Read `notes/decisions/` for design constraints

## Current priority: Acceptance tests

Write spec tests against clauses in spec.db. Prerequisite: nail
down SB error codes for each diagnostic so tests can assert on them.

Infrastructure: spec/common.sh harness, `sash -c` implemented,
spec.db has 192 clauses across 45 DDs. Test scripts go in `spec/`,
one file per clause, output TSV for database loading.

## Previous priorities (done)

All audit tasks resolved. ~15 implementation tasks created.
See `notes/tasks/` for the backlog. Key priorities when we shift
to implementation:

- **BUG: EXIT trap wrong scope** (bug-exit-trap-wrong-scope)
- **DD#55 error model** (4 tasks: implicit checks, `-` prefix,
  `$?` scoping, non-void functions)
- **DD#56 signal model (4 tasks: signum keyword, signal FLAG,
  signal EXIT, END block)
- **DD#31 arithmetic refinement** (disjoint contexts, mutation
  rejection, operand type checking)
- **Taint clean list review** (review-taint-clean-list)

## Current state

- **590 tests** passing (268 codegen, 131 e2e, 37 janet, 59 oracle, 52 brush, 43 taint spec)
- End-to-end compilation working: bash source → static ELF binary
- Janet and --emit-source backends working
- All audit tasks resolved (0 remaining)

## Session 2026-05-02 (continued)

DD audit triage complete. All audit tasks resolved.

### New design decisions
- **DD#55**: Error handling model — implicit exit checks, `-` suppression,
  `$?` scoping, void/non-void functions, background process rules
- **DD#56**: Signal/trap model — flag-setting traps, `signum` keyword,
  `signal builtin (IGN/DFL/FLAG/EXIT), END block, signum keyword

### DD amendments
- **DD#31**: Postfix ++/-- only (Go rule), comma operator desugaring,
  bare boolean at statement level → error
- **DD#24**: Strengthened bless anti-tautology ($VAR LHS only)
- **DD#30**: `x=5` is string, no ambiguity. Documented.
- **DD#39**: Plan 9 regex engine limitations, compile-time validation,
  runtime precompilation of literal patterns

### Bug found
- END block: current impl inlines before every Return (fires on function returns,
  misses `exit` builtin). Filed bug-exit-trap-wrong-scope. DD#56 redesign fixes this.

### Docs
- README first section rewritten (compiler as specification, facts
  without implying perfection)
- GUIDE.md: removed stale counts, fixed taint source descriptions
- SUBSET.md: arithmetic semantics, regex engine limits, glob
  "restricted"→"rejected", coercion language fix, unset refinement
- Deleted pinned.md (stale, contradicted decisions)

### New tasks (~15)
- dd55-implicit-exit-checks, dd55-suppress-prefix,
  dd55-dollar-question-scoping, dd55-nonvoid-functions
- dd56-signum-keyword, dd56-flag-traps, dd56-exiting-traps, dd56-exit-block-rework,
  dd56-exit-block-rework
- bug-exit-trap-wrong-scope
- dd25-overflow-recovery-model
- dd32-runtime-trace
- dd39-compile-time-regex-validation, dd39-precompile-literal-regex
- review-taint-clean-list, refine-unset-semantics
- dd46-reject-toplevel-positionals (existed, confirmed)
- dd49-antitautology (strengthened)

### Audit tasks resolved (~25)
All remaining audit tasks from the 3-model adversarial review.
See journal/050-session-error-model.md for full list.

### Policy decisions
- Magic variables: `$$`/`$!`/`$?` for punctuation specials. Keywords
  for syscall-backed: epochseconds, getpid, getppid, getcwd, signum,
  argv, getenv. RANDOM dropped (bad LCG, use /dev/urandom).
- `[[ -v ]]` confirmed removed from design (was stale in pinned.md)
- No DD needed for every accepted construct (exec, builtins, etc.)

## Session 2026-05-02

DD audit triage + major design simplifications.

### Design decisions added/rewritten
- **DD#53**: Env var isolation — sash vars and env vars are separate namespaces.
  Only `$(getenv NAME)` + bless reads env. Per-command env delegates to `/usr/bin/env`.
- **DD#54**: Source pipeline — source/. rejected. Replacement: `sash-expand | mapfile -m`.
- **DD#23**: Rewritten — compile-time path resolution, no runtime PATH walk. `env` is
  the escape hatch. `command -v` coerced to `[[ -x ]]`, no command builtin in sash-native.
- **DD#17**: Clarified two call forms (capture `$(func)` vs bare `func`).
- **DD#20**: Added glob option amendments (extglob/globstar rejected, failglob on).
- **DD#21**: Added pipeline coercion amendment.

### Audit tasks resolved (11)
auto-bless-visibility, dd01-09-export-crossref, dd45-export-amendment,
dd01-09-namerefs-stale, dd17-universal-call-overstated, dd18-depends-dd38,
dd20-glob-amendments, dd21-pipeline-amendment, dd23-no-path-validator,
dd23-path-guide-contradiction, no-dd-env-mutation, no-dd-source-loader

### Implementation tasks created (13)
- remove-implicit-env-fallback (security)
- remove-export-ssa-dead-code (cleanup)
- mapfile-map-mode, source-rejection
- dd21-grammar-dash-prefix, dd21-implement-dash-prefix
- dd23-remove-path-walk, dd23-remove-command-v
- dd23-compile-time-resolve, dd23-command-v-coercion
- reject-extglob, reject-globstar, implement-failglob
- per-command-env-vars (rewritten: delegate to /usr/bin/env)

### Remaining audit tasks
~29 at status: ready in notes/audit-tasks/

## Session 2026-05-01

Major session: iwe migration + 3-model DD audit + design clarifications.

### Done
- Migrated all project docs to iwe knowledge graph (notes/)
- Removed HANDOFF.md, docs/DECISIONS.md, docs/JOURNAL.md, docs/TODO.md
- 3-model adversarial audit of 52 design decisions (Opus 4.6, GPT 5.4, Gemini 2.5 Pro)
- 49 audit tasks created, 9 resolved during triage
- DD#24 consolidated as canonical taint/bless document (absorbed DD#43, 44, 47, 49)
- DD#15 oracle rationale expanded from journal — no content lost
- DD#31 rewritten: reject-or-coerce, no silent semantic change. SUBSET.md updated.
- DD#33 corrected: both key types implemented (raw i64 for indexed, string for maps)
- DD#38 subshell coercion documented (decompose to sash primitives)
- DD#42 status corrected to implemented
- DD#26 allocator description fixed (first-fit free-list, not bump)
- System prompt: added "Questions are not imperatives" section
- Pinned notes updated for DD#31 and subshell design/implementation gap

### Implementation tasks from 2026-05-01
- Subshell decomposition in lowerer
- Sash-native subshell rejection
- if-bless conditional form
- DD#31 mutation rejection in $(( ))
- DD#31 disjoint context enforcement + type fix
- Janet parity quality gate
