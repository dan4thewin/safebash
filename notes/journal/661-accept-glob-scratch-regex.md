title: Accepting the glob, scratch and regex package on main
date: 2026-11-18

# Accepting the glob, scratch and regex package on main

DG directed:

> move the provisional work status to accepted, bring it to main

That is the adoption decision required by DD131/DD132, not another request
for an experiment. DG made the decision; the AI reconciled the records and
landed the already implemented package. No new language behavior or
implementation strategy is selected in this closeout.

## What is accepted

DD131/DD132 now have `status: accepted`. Their companion current rules
describe ordinary atomic accumulating fillers, fresh immutable scratch,
bounded `glob`, regex-based `rglob`, ordered pattern-list expansion, quiet
`r?`, exact immutable runtime-pattern proofs, forced-pattern refusal,
caught-scratch read scopes, and regex text plus matched/no-match truth.
Substitution sugar explicitly projects text; assignment remains resultless.

The in-place reconciliation removes contradictory replacement/clear-on-entry
claims and the old dynamic-consumer-error framing. DD61's former single-origin
array rationale cannot survive accumulating batches: whole-binding taint and
ownership remain, not a claim that every element shares one acquisition.
Ordinary const validation, guarded redirect indices and the independently
accepted failure/scope/alias rules retain their authority and dispositions.
DD129's quiet-predicate delivery pointer is updated without claiming the
separate cross-target jump implementation.

The integrated owner records DG's acceptance for every package row and closes,
as do the directory-prefix and multi-star-refusal owners. Their original
measurements, quotations, recording baselines and dated delivery snapshots
remain historical evidence. Existing journals are unchanged. GUIDE and both
tour editions no longer advertise a trial-only language; the tour's programs
and recorded results are preserved.

## What remains separate

The evaluation in journal 654 exposed a real converter compatibility problem:
two converted `mapfile` fills into the same ordinary array silently retain
the first entry. Its loop-local declaration control still works. Mutable
underscore names, reserved selector names, runtime-pattern proofs and regex
result consumption also need migration.

The new
[converter owner](../tasks/convert-glob-scratch-regex-migration.md) carries
that work after the integrated trial closes. Hand adaptations are not a
converter repair; adoption does not select blanket renaming, reset-before-fill,
private replacement or a new annotation policy. Those consequential choices
belong to undertaking that task.

Journal 655's authorized append optimization is included: cached next indices
and whole-batch reservation remove the measured repeated scan/clone cost
without weakening atomic publication. The separate regex allocation/reuse
owner remains open. This task does not resume coverage or unrelated backlog.

## Landing and evidence

Published main started at `6c7b899ff9f8e1bfe18a5746efd23d8c549003ea`.
The evaluated trial tip is `10345a0d4cdcbe477362e5a227d8db1044e7a52e`,
seventeen existing commits ahead. There is no associated PR or stack.
Ancestry, live refs, repository identity and worktree ownership were checked
before fast-forwarding local main; no historical tree was checked out.
The acceptance-record commit follows the trial tip on main, and normal push
publishes that linear history. `glob-scratch-trial` is retained at its tip;
there is no merge commit, rewrite, force push or branch deletion.

The private baseline records all tracked-file hashes, protected compiler/
runtime/lock/tour artifacts, refs and spec-history boundary 7283823.
The serialized zero-change gate
(`NEW=0 FLIPS=0 RETIRED=0 TOTAL=12573`) passes at rows 7283824-7296396:
6,398 native and 6,175 C checks, 12,560 passing plus 13 existing skips.
There are no failures, flips, new IDs, retirements, ghosts or plan mismatches;
the comparison liveness control reports 245 historical differences.
Tour regeneration passes all 70 snippets, including 15 intended failures.
All 70 source code blocks and 140 generated code/output blocks are unchanged.
GFM structure and 246 local link targets pass. Comparing all 1,558 baseline
tracked-file hashes confirms that only intended Markdown changed; all nine
protected artifacts and every existing journal remain byte-identical.
Only DD131/DD132 change decision status. Normal commit/push hooks and exact
final ref verification remain publication gates.
The acceptance delta contains documentation only; the implementation evidence
and independent before/reversion controls remain in journals 644-655.
