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

## Material retained from the former man page

On 2026-11-11 DG approved replacing the mixed `sash(1)` manual with a
compiler-command reference, while preserving its useful language material
for this effort. The complete former page remains in Git at
[`7e8150b5:docs/sash.1`](https://github.com/dan4thewin/safebash/blob/7e8150b5e038c2c969d253cc66b5354aa7e404f1/docs/sash.1).
Retrieve it with:

```sh
git show 7e8150b5e038c2c969d253cc66b5354aa7e404f1:docs/sash.1
```

This is a source-material map, not another specification or a certification
of the old prose. Line ranges refer to that frozen revision, not the current
command manual.

| Former section | Lines | Reuse boundary |
|---|---:|---|
| Redirection | 349-444 | Retain the syntax inventory; reconcile failure handling with current decisions and delivery. |
| Test operators | 445-586 | Retain the operator explanations; check current typing, consumption and regex contracts. |
| String escapes | 587-695 | Useful lexical examples; verify each context rather than importing Bash escape rules. |
| EXIT/ERR handlers | 696-773 | Recent material agrees with shipped DD128 cause-based routing, including `exit N` and `fail 0`. |
| Special values, status and process latches | 774-847 | Separate current behavior from accepted-but-pending capture-entry changes. |
| Functions and tail recursion | 848-888 | Integrate with the current void/status/integer return-kind rules. |
| Regex dialect and quoting | 889-1134 | Useful material with the corrections and limitations below. |

Known exclusions and corrections from the assessment:

- Do not carry forward the `read` flags, `mapfile` or `echo -n` survey
  (220-348), its old BUGS catalogue (1160-1253), or the retired diagnostic
  entries. DD99's shipped interface uses flagless `read`/`readline` and
  `echo`/`write`; provisional scratch/accumulation policy is not implemented.
- The regex additions preamble wrongly says Bash does not accept counted
  repetition. A Bash control accepts `^a{2}$`; do not reproduce that claim.
- The whole-subject anchor promise contradicts the still-open
  [anchor bug](bug-regex-line-anchors.md), including global-match resumption.
  Do not turn the faulty implementation into an intentional dialect rule.
- The regex interior-NUL restriction (1066-1099) is instead a
  [deliberately documented limitation](limitation-regex-stops-at-interior-nul.md):
  matching stops at the first NUL although scalar storage retains the suffix.
- DD132's runtime-pattern proof model is provisional and unimplemented.
  DD39's literal-regex precompilation/cache delivery is also pending;
  neither may be inferred from older prose.

Use safebash for the language, sash for the compiler, and `.sb` for source
examples. Journal 623 records the manual refresh. This preservation does not
select the specification's organization, IDs, authoring model or authority,
and the effort remains deferred.

## Related

- `spec-db-clause-text` — the requirement text, and the `spec/clauses.sql`
  fragility (231 clause texts exist only inside the binary `spec.db`).
- `decollide-spec-chk-ids` (done) — why the current id namespace is flat.
- `dd70-error-model-contract-spec` — an error-model contract already headed in
  this direction.
- `docs/GUIDE.md` — the public architecture overview; not normative.
