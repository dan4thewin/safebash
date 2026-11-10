title: Better regex diagnostics without choosing the mechanism
date: 2026-11-10

## The decision

The AI proposed reporting SB5023 at each offending pattern, rather than
at a shared test, attest, word or substitution statement. DD39 explicitly
records the existing positions, so this was a quality proposal, not a
claim that the compiler violated its location contract.

DG accepted the goal while asking for alternatives:

> yes to better diagnostics, but evaluate alternate approaches like a catalog of unique regex

The comparison separated pattern identification from source navigation:
including the pattern can distinguish errors at a coarse position;
precise positions identify individual occurrences; a catalogue can group
identical patterns and their uses. None automatically recovers discarded
source provenance.

DG then directed:

> defer implementation specifics to when we undertake thw task

The accepted contract is therefore clear identification of the offending
static regex, including multi-pattern constructs. It is not a selection
of exact columns, a catalogue, grouped diagnostics or an internal
representation. Those choices belong to the implementation discussion.

## Evidence and ownership

At `7a378603`, the three static-regex checking paths share
`validate.rs::check_static_regex`. Tests pass their command span;
ordinary operands and dispatch arms pass their pattern span. Substitution
desugaring computes each edit's origin but gives the regex expression
the enclosing span. Some reconstructed arithmetic input has deliberately
coarse provenance. These source observations informed the comparison;
the F15 examples in the task were not rerun.

DD39 records the objective and labels current positions as the existing
baseline. The P3 `q-sb5023-names-the-test-not-the-pattern` changes from
question to work and stays open. Its gates no longer predetermine exact
per-pattern locations. The code, dialect and validation scope remain;
DD132's provisional quiet-false contract for invalid `r?` candidates stays.

The comparison also exposed a separate record discrepancy:
`dd39-precompile-literal-regex` was marked done in housekeeping, while the
current runtime match/substitution entrypoints call `regcomp9` on use.
The AI reopened that existing owner and qualified DD39's delivery claim
rather than assuming a reusable catalogue exists. This bookkeeping
correction selects no new cache design and creates no dependency on the
diagnostic work.

This is policy/backlog recording only. No compiler, runtime, spec or build
source changed; no new behavior probe, full sweep or coverage measurement
accompanies it. Implementation and its specifics remain deferred.
