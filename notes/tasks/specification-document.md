---
title: "The specification document (built from the DDs + spec work)"
status: deferred
priority: P3
kind: effort
depends: []
gates:
  - "DECIDE: the logical ordering and id scheme (the current DD numbering is chronological, not logical)"
  - "DECIDE: generated/derived vs authored — and what mechanically checks it against the DDs and specs"
  - "Every normative statement traces to a DD and to at least one spec test; the inverse query (clauses with no test) is answerable"
  - "SUBSET.md's failure mode cannot recur: no claim survives in the document that the specs contradict"
---

# The specification document

DG (2026-09-07): *"there will ultimately be a specification document, presumably
with a logical ordering and id scheme, built from the DD and spec work."*

Recorded now — not to start it, but so nearby work stops optimising for the
wrong shape. It is the missing consumer that several loose ends point at.

## Why this changes decisions already in flight

- **`spec-db-clause-text`** gates on "nothing reads the `clauses` table, so is
  the text worth writing?" This is the answer: the clause text is an *input*
  to the specification document. But the **keying is provisional** — clause ids
  today mirror spec test ids, which are DD-numbered by *chronology*, not by
  logic. So author text freely; do not build tooling that assumes
  `clause_id == test id` forever, and expect a re-keying step.
- **spec `chk` ids** are a flat global namespace that has already needed a
  de-collision pass (`decollide-spec-chk-ids`). A logical id scheme in the
  document does not have to replace them — a mapping is enough — but the
  document's ids should not inherit the chronological accident.

## The precedent that must not repeat

`SUBSET.md` was exactly this document's ancestor, hand-maintained in parallel
prose. It drifted: it claimed non-colon coercion sash does not do and listed
dropped `:=`/`:+` forms as supported. DG's verdict was **"worse than useless"**,
and it was deleted 2026-08-29, with `notes/decisions/` repointed as the subset
authority (AGENTS.md still says so).

The lesson is not "don't write a spec document" — it is that a normative
document which is not **mechanically tied** to the DDs and the specs will drift,
and a drifted specification is worse than none, because people trust it.

So the load-bearing gate is the second one: whatever the authoring model,
something must fail loudly when the document and the implementation disagree.
The spec suite is already that oracle for behavior (4034 passing across three
backends); the document needs to hang off it rather than beside it.

## Open questions (all DG's)

1. **Ordering.** By language surface (lexical → expansion → commands → control
   flow → functions → types → taint), by DD dependency order, or by the
   existing spec-file split? The DDs are chronological, so some re-ordering is
   unavoidable.
2. **Id scheme.** Hierarchical section ids (`§4.2.1`), stable opaque ids, or
   keep DD-derived ids with a logical index over them? Stability matters most
   if specs cite them.
3. **Generated or authored?** Generated from DD frontmatter + clause text is
   drift-proof but reads like a database dump; authored prose reads better and
   drifts. A middle path — authored prose with generated normative clauses and
   a checked test↔clause table — is likely, but it is a real decision.
4. **What is normative?** The DDs decide behavior; the specs demonstrate it.
   If the document is normative, the DDs become its rationale; if it is
   descriptive, the DDs stay authoritative. This determines what a conflict
   *means*.
5. **Scope.** Language reference only, or also the taint model, error model
   (DD#70) and the runtime/ABI surface?

## Related

- `spec-db-clause-text` — the requirement text, and the `spec/clauses.sql`
  fragility (231 clause texts exist only inside the binary `spec.db`).
- `decollide-spec-chk-ids` (done) — why the current id namespace is flat.
- `dd70-error-model-contract-spec` — an error-model contract already headed in
  this direction.
- `docs/GUIDE.md` — the public architecture overview; not normative.
