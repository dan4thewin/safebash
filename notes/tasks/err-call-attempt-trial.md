title: Implement and evaluate ERR-owned call attempts
status: done
priority: P2
kind: work
depends: []
gates:
  - Native and C implement the complete DD136 contract
  - Focused before and independent reversion controls
  - Predicted ordinary spec gate and normal commit hooks
  - Authored tour, generated tour and specification outline agree
  - DG evaluates and accepts the complete branch before landing on main

# ERR-owned call attempt trial

DG accepted the complete implemented branch on 2026-11-26 and directed its
landing on main ([journal 676](../journal/676-accept-err-attempts.md)).
The implementation, verification and evaluation are complete.

DG originally requested the provisional branch and implementation on 2026-11-21.
[DD136](../decisions/DD136-err-call-attempts.md) is the contract.
Branch: `err-attempt-trial`, based on main `02217248`.
That branch is retained at its evaluated tip. Its former adoption boundary
is satisfied; it is not a restriction on the accepted main implementation.

## Scope

- Replace colon's operand-discard role with the existing `_` binder.
  Preserve bare/redirect-only noop. Reject misspelled, mistyped and
  misplaced attempt calls rather than falling back to ignored words.
- Implement resultless colon attempts on void functions and caught
  status-function attempts at ERR-owned call sites.
- Abandon the complete helper chain, including call preparation, while
  preserving ordinary nested result handling and source `-`.
- Apply DG's selected narrow recovery boundary. Keep unconditional guards,
  fatal faults, explicit exit and failed restoration terminal.
- Restore attempt-owned scopes and readers; preserve enclosing resources,
  completed writes, child ownership and ERR's original entry status.
- Include abort prefixes in source and SSA taint reasoning. Preserve
  readonly imports and existing taint/attest/forced-read contracts.
- Align dependent converter output, live examples and regression fixtures.
- Update the authored tour, regenerate its GitHub edition, and align the
  specification outline and architecture guide.

## Companion-rule inventory

The pre-trial contract is retained in Git at `02217248`. Reconcile current
openings and affected bodies in place, not through contradictory addenda:

- DD55/DD70: ordinary result handling versus ERR attempt recovery,
  diagnostics, suppression and noop.
- DD58/DD74/DD88: distinct return kinds, caught calls, forwarding and
  otherwise-legal capture consumers.
- DD69/DD81/DD84/DD92/DD98: operand disposal, arity, splat dispositions
  and write-only `_`.
- DD86/DD127: body boundaries, completed effects and taint summaries.
- DD104: schema 4 retains the command's explicit best-effort marker in
  both emitted and embedded IR; unsupported consumers must refuse it.
- DD128: terminal routing, SB5101 replacement and abort-prefix states.
- DD130/DD135: narrow contextual recovery versus failure classification;
  readonly imports remain while mandatory ERR NUL-freedom proofs retire.
- GUIDE, SPEC-outline and both tour editions.

The broader runtime/file-failure, converter glob/scratch/regex, capture
consumer and suppression owners are not automatically complete. Record
only the subsets actually delivered here. Fresh coverage measurement and
unrelated backlog implementation are outside this request.

## Evidence

Freeze matched compiler/runtime/helper artifacts and both local lockfiles
before implementation. Native and C before-controls must demonstrably link
the frozen runtime rather than the changing working archive.

Use globally fresh spec IDs for new or changed contracts, including the
retirement of SB5101/colon-discard expectations. Predict additions,
retirements, flips and totals before the serialized ordinary gate.

Focused controls must distinguish:

- Full-chain abandonment from a fabricated nearest-helper return.
- Normal status return from abort, including `fail 0` and negation.
- Local handling and ordinary `-` from an ERR-owned attempt.
- An eligible operational failure from each terminal failure class.
- Restored nested scopes/readers from damaged enclosing ERR resources.
- The actual aborted prefix from a helper's normal-completion taint summary.
- Both backend mechanisms, not only their common lowering.

Run the actual tour checker and verify all generated examples. Restore all
reversions, verify final source/artifact identities, update the journal and
current status, then commit and push through normal hooks.

## Implementation and focused evidence, 2026-11-21

The trial is implemented in both backends. Compiler-visible exceptional
edges carry the abandoned prefix into source/SSA taint and regex analysis;
a runtime cleanup ledger restores only attempt-owned resources. Independent
write-only `_` identities replace operand disposal through colon. SB5108
rejects invalid attempt sites; IR schema 4 preserves the explicit marker.
SB5101 and its private mandatory NUL prover are removed, while readonly ERR
imports, runtime guards and checked terminal failures remain.

The accepted named-predicate capture extension is delivered alongside the
trial, without extending ordinary handling into a body or operand. DD121's
existing condition-to-both-branches adjacency also now reaches `else`.
Freshness filtering prevents an abandoned prefix borrowing a later source.
Fork failure remains fatal after closing construction-owned pipes and
reaping already-started stages; it cannot become an attempted false return.

The final `capture_cond` and `err_attempt` files pass 368 native/C checks.
After the pipeline fixture migrations, matched original compiler/runtime
controls cover all 315 new mode IDs: 269 fail, 44 pass and two skip.
The latter are preservation or independently qualified controls, not
46 failed-before fixes. Twenty of 21 independent
mutation campaigns produce red assertions; the ineffective SSA timing
mutation and corrected predictions remain recorded rather than counted
as proof. All mutations are restored to the verified positive identities.

The authored tour and regenerated GitHub edition contain 73 checked
programs. Three new examples demonstrate checked disposal, full-chain void
abandonment, and partial predicate output with abnormal `fail 0`.
The specification outline and all sixteen companion DDs above agree.

[Journal 664](../journal/664-err-owned-call-attempts.md) carries the detailed
evidence and gate record. The first full gate found seven missed fixture
migrations per mode: five recoverable-fork expectations and two colon
operands. Their fresh-ID replacements preserve cleanup, interrupted waits,
discard effects and stage diagnostics; both 179-case pipeline files pass.
The nested injection now proves the outer producer was reaped before the
inner forks, including a before/after natural-restart control.

The corrected ordinary gate passes with NEW=14, RETIRED=14, FLIPS=0 and
TOTAL=12853, at rowids 7334622-7347474: 12840 passing and 13 existing skips,
with no failures or ghosts. The implementation and proof package is
delivered on `err-attempt-trial`. DG's subsequent acceptance is recorded
above and in journal 676; this owner is now complete. The broader
file-handling, suppression, converter and coverage work remains separately
owned.
