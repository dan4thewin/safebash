# 630 - Draft language-specification outline

DG requested a terse, uncommitted specification outline from the DDs,
treating provisional and not-yet-implemented rules as accepted and complete
within the draft. The audience knows POSIX-like shells and Linux, not Bash
specifics; implementation mechanisms do not belong unless essential to a
language rule.

The AI read all 121 current-rule openings and consulted the interacting
bodies, then wrote [docs/SPEC-outline.md](../../docs/SPEC-outline.md) as one
numbered list with nested constraints. Its eighteen topics follow language
concepts rather than decision chronology.

The draft consistently uses DD131/DD132's accumulating array fills, fresh
immutable scratch, runtime regex proof and text-plus-boolean substitutions.
It also includes the accepted pending result, capture, evaluation-order,
failure and getenv rules. Historical alternatives and implementation
machinery are excluded; undecided scalar scratch notation is not invented.

This does not change any DD's disposition, claim compiler delivery, or close
the broader [specification task](../tasks/specification-document.md).
The initial outline and bookkeeping were left uncommitted for DG's review.
No compiler, runtime, spec, build or dependency changes; no new behavior
measurement, spec sweep or coverage run.

## Snapshot before reordering - 2026-11-13

DG requested a branch snapshot, then a more pedagogic order: simpler
concepts first, taint before `:!`, and a capture definition before its
enclosing-binding rules. This commit preserves the original outline
unchanged on `spec-outline`; the ordering revision follows.
