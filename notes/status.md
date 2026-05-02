---
created: 2026-05-01
modified: 2026-05-01
title: status
---

## Current state

- **590 tests** passing (268 codegen, 131 e2e, 37 janet, 59 oracle, 52 brush, 43 taint spec)
- End-to-end compilation working: bash source → static ELF binary
- Janet and --emit-source backends working

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

### New implementation tasks
- Subshell decomposition in lowerer (notes/tasks/subshell-decomposition.md)
- Sash-native subshell rejection (notes/tasks/subshell-native-rejection.md)
- if-bless conditional form (notes/tasks/if-bless-conditional.md)
- DD#31 mutation rejection in $(( )) (notes/tasks/dd31-reject-mutation-in-expr.md)
- DD#31 disjoint context enforcement + type fix (notes/tasks/dd31-enforce-disjoint-contexts.md)
- Janet parity quality gate (notes/tasks/janet-parity-gate.md)

### Remaining audit tasks
40 audit tasks at status: ready in notes/audit-tasks/
