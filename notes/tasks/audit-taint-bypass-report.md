---
title: "audit: a single report file listing every bless / taint bypass site"
status: ready
priority: P2
kind: hardening
depends: []
gates:
  - "one generated report enumerates every taint-relevant construct: bless / bless-all / map-bless, the `:!` taint-off operator (DD#91), --no-taint / --no-taint-cst usage, and (for context) every external-capture taint source"
  - "each entry carries file:line and the variable/subject, so a security reviewer can audit every bypass without grepping"
---

## Why (DG 2026-09-04)

Filed while adding the `:!` operator (DD#91) — a sanctioned per-use taint bypass
(`$foo:!` = `:~` elide + taint guard off, the middle ground between `bless` and
`--no-taint`). DG: prefer, over recording `:!` sites in `--manifest`, "a new
audit file with all bless/taint related hits" — a broader audit surface than
just `:!`.

The adversarial audit's highest-priority target is validator bypass (AGENTS.md).
A single generated report of every place taint is deliberately cleared or
suppressed makes that review mechanical:

- `bless v [[…]]` / `bless x in a [[…]]` / `bless k v in m [[…]]` (predicate blesses)
- **`:!`** taint-off expansions (DD#91) — the predicate-less bless
- `--no-taint` / `--no-taint-cst` invocations (whole-program bypass, DD#82)
- external capture taint SOURCES (`$(getenv)`, `$(argv)`, `$(external)`, read
  from fd) for context — where tainted data enters

## Shape (to design with DG)

- A compiler mode (e.g. `--audit-taint`) or a standalone pass over the IR that
  emits the report (text / json), OR a section appended to `--manifest`.
- Columns: kind (bless/bless-all/map-bless/taint-off/no-taint/source), file:line,
  subject variable/name, and the predicate text for blesses.
- Possibly a repo-committed snapshot the audit diffs against (like spec.db) so a
  new bypass shows up in review.

## Found

2026-09-04, DD#91 (`:!` operator), journal 243.
