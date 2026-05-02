---
created: 2026-05-01
modified: 2026-05-02
title: status
---

## How to start a session

1. Read this file first
2. Run `iwe retrieve -k safebash -d 2` for full graph context
3. Read `notes/pinned.md` for constraints/blockers

## Current priority: DD audit task triage

40 audit tasks remain at `status: ready` in `notes/audit-tasks/`.
Pattern for each:

1. Read the next audit task file
2. **Discuss with DG** — even trivial ones get a sentence explaining why
3. Check the relevant DD note, journal, and implementation
4. If the DD note needs updating, update it (snapshot only, no narrative)
5. If implementation is misaligned, create or update tasks in `notes/tasks/`
6. Mark the audit task `status: resolved`

Do not batch-resolve without discussion. DG wants to be in the loop
for every finding.

## Current state

- **590 tests** passing (268 codegen, 131 e2e, 37 janet, 59 oracle, 52 brush, 43 taint spec)
- End-to-end compilation working: bash source → static ELF binary
- Janet and --emit-source backends working

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
