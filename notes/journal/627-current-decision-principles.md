# 627 - Put current decision rules first

DG requested a concise numbered list at the beginning of every decision's
body: current enforceable principles, constraints and specification only.
Overturned rules must disappear from those lists, even when the overturning
decision is elsewhere. Historical detail stays below, with obsolete claims
visibly struck rather than another addendum contradicting them.

The AI applied that structure to all 121 decision documents. The original
corpus contained 165,983 words; this pass adds a short reading layer rather
than attempting a wholesale rewrite of that history.

## Current rules, retained history

Each body now starts with one ordered list, followed by a separator and the
original material. Necessary sublists are ordered too. A record with no
surviving requirement says **"No current requirements."** Retired features
are absent from the openings, not restated as historical prohibitions.
The openings total 11,264 words; 86 documents have struck body passages.

Metadata alone does not determine what survives. DD42 retains its environment
lookup contract after its argument interface was superseded. DD15 retains
the independent semantic-checking purpose through DD104's data seam, not the
removed in-tree backend. Conversely, DD78's default-interpolation discipline
has no current requirement after DD65's literal-only ruling.

The detailed bodies retain their original wording and headings. Inline
strikethrough marks obsolete prose; blank-line-separated `<del>` wrappers
mark obsolete code blocks, tables and longer passages without breaking their
Markdown rendering. AGENTS records this as the continuing DD convention.

Three bounded agents handled disjoint cohorts, with six central contracts
and final cross-document reconciliation retained by the parent. The opening
lists were reviewed together, not accepted merely because each document's
local summary looked plausible.

## Reconciliation is more than shortening

The pass applies the later result-kind, no-reach, termination, lexical
identity, collection-signature and taint-ownership decisions to older
prescriptions. Clean writes are not taint resolution; kernel reads do not
become clean because their writer is internal; early reader exit closes and
waits rather than requiring the remaining stream to be drained.

Several precision corrections mattered:

- DD64's November 10 claim that defaults "still follow double-quoted-body
  interpolation" was an inaccurate continuity claim, not a selected reversal
  of DD65. The dollar/converter ruling did not change literal-only defaults.
- DD84 still explicitly defers integer-zero elision. Scalar/cell `:~` keeps
  integer zero rendered as `0`; DD123's meaningful-zero `:-` rule does not
  silently extend elision.
- DD92's explicit managed-`args` attest prohibition remains. DD127's
  clean-name rule removes an adjacency requirement, not subject-eligibility
  restrictions; DD86's const allowance is for an otherwise legal attest.
- DD122's old permission to prefix a final loop with `-` is superseded by
  the resultless-control-flow contract. Final-reader command results,
  loop-body checks and the pipeline aggregate remain distinct.

DD131 and DD132 remain explicitly provisional. Their trial accumulation,
fresh immutable scratch and runtime-regex proof rules do not silently replace
ordinary collection-fill or regex contracts. Accepted-but-unimplemented
requirements are not dropped merely because the compiler does not yet
enforce them.

## The capture-pipeline conflict needed DG

DD88 expressly limited capture-condition pipelines to external stages.
DD55 later described a failing function stage making the capture condition
false. The bounded provenance trace found the original restriction in
`fac082f0`, explicitly DG-chosen, and the later statement in `825fea9f` as
AI decide-and-report. Later anonymous-return and discarded-status rulings
did not themselves reconcile producer eligibility.

After reading the two passages, DG ruled:

> i see no reason to disallow a user function behaving as a filter in a
> pipeline regardless of the result being captured

Capture-condition pipelines now explicitly use ordinary pipeline-stage
eligibility, including otherwise-legal user-function filters. DD88's
external-only pipeline restriction is struck; DD55's function-stage account
is retained. The direct `if text=$(f)` exclusion remains. This introduces
no filter annotation, purity requirement, integer-to-status coercion or
relaxation of writer, taint, no-reach or fatal-failure rules.

This was the unresolved policy point referred to DG, not a compiler change
or a new claim of execution evidence.

## Preservation and verification

An immutable pre-edit snapshot supplies the original headers and bodies.
The session checker verifies all 121 opening lists, exact bare headers,
original body-token order, retained rendered headings, new local-link
targets, retired-term omissions, provisional labels, rendered strike
structure and whitespace. All 973 original rendered headings remain.
Body-token preservation is corroboration against
accidental loss, not an automatic proof of semantic reconciliation.

The first core preservation check caught a dropped full stop inside a struck
sentence. It was restored rather than exempted. Rendering controls exercised
both inline strikes and wrappers around fenced code and tables.
`cmark-gfm` was missing and was installed through Linux Homebrew; the
installation log also records Homebrew's automatic old-keg/cache cleanup.
No repository dependency was added.

Decision metadata and task dispositions remain unchanged. The snapshotted
lockfiles, compiler/helper binaries, runtime archive, iwe configuration,
historical hunk ledger and generated status block retain their baseline
identities. Spec history remains at row 6902131. No source, spec or build
rule changed, and no spec sweep or coverage run was performed.
