title: Suppression belongs beside its command
status: complete

DG chose **"reject the - not adjacent to the command"** on 2026-11-08.
Copilot recorded the rule in DD55/DD71, reconciled DD70/DD88 and GUIDE,
and added the P3 implementation bug `reject-nonadjacent-suppression`.
This closes the policy choice, not enforcement.

The immediate example was:

```bash
- x=$(/bin/false)
x=$(- /bin/false)
```

DG chose rejection of the first spelling, not a narrowly scoped alias
for the second. An assignment binds a value and has no result of its own
to discard (DD71). The same rule covers append and cell assignments,
whether the capture is in a value or an index; declarations retain
their existing refusal. Put `-` on the intended command instead of
asking a prefix on the binding to find it.

The principle also settles the logical-list binding question. A `-`
before `a` in `- a || b` or `- a && b` belongs only to `a`; it cannot
discard `b`'s result. This matches the established first-stage binding of
a pipeline-head prefix. Result-kind checks follow the actual recipient,
so a later unprefixed void function is not itself a reason to reject the
prefix, while a directly prefixed void function cannot hide in a list.

Binding and consuming are different questions. Whether `||` may consume
the result that `- a` discards remains Q3 in
`q-suppression-and-stage-residuals`. The adjacency ruling neither makes
that spelling unconditionally legal nor defines discarded status as
success. Directly prefixed status-less intrinsics and other inert
prefixes likewise remain separate choices.

`[[ ... ]]` and `(( ... ))` are single commands, not compound bodies.
Their operand captures stay separate evaluations under the existing
no-reach rule. The historical arithmetic-write residual lowered a store
under suppression and discarded a nested capture's failure. The new
implementation task includes that barrier: source-level adjacency must
not be undone by mutation hoisting.

DD88/DD101 capture conditions retain their handling contract, including
`if ! x=$(cmd)`. They do not license an outer assignment `-`. Body
boundaries, per-stage checking, result kinds, taint and DD130's
recoverable/fatal distinction remain unchanged. The existing compound
prefix ban retains its own implementation task.

The residual question record now distinguishes settled R1/R4/R6 and
command-local R7 classification from its remaining R3 and the separately
owned consumer question. R2's operand reach is a repair under no-reach,
not another request to approve suppression inside expressions.

B84's F6 unit C repair remains a real historical fix: an unchecked
right operand's status was being reset to zero. Its old whole-list
source route is not thereby a permanent language contract. The task
requires review of 55.145-55.147, fresh ids for changed contracts and
preservation of legitimate status-retention coverage.

The old probe tables remain explicitly historical and taint-disabled;
they were not rerun or revalidated against later typing/return rules.
The backlog requires actual fixture checks, focused before/reversion
evidence and measured full-gate predictions when enforcement is built.
No compiler, runtime, spec or build code changed. No fresh behavior probe,
full sweep or coverage run was made; journal 577 remains the accepted
implementation baseline.
