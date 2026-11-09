title: Const freezes a value, not validation
date: 2026-11-09

## The next policy question

After next-line clauses, the AI brought forward
`q-attest-retag-of-const`. A const initialized from external input
cannot currently be attested: the compiler classifies the retag as a
reassignment and reports SB4044.

```sash
const home=$(getenv HOME)
attest home -n
echo "$home"
```

The AI recommended separating immutability from validation. The
alternatives were retaining the refusal, requiring a validated mutable
temporary before creating the const unless the author used `:!`, or
banning tainted const initializers outright.

DG chose:

> yes, separate immutability from validation

## One principle across the forms

A const freezes the value, not what has been validated about it. An
otherwise legal taint-source initializer remains allowed and does not
become implicitly clean. An eligible attest validates the same bytes.
An internal SSA version carrying the verdict is not a language-level
assignment.

The allowance covers full predicates, unary shorthands, standalone and
legal caught forms. It also covers a const as an arithmetic conversion's
source: its bytes and type do not change; a distinct target receives
the integer. The principle is not limited to the shorthand example.

Real writes remain forbidden. A conversion target is an actual write,
and a failed attest does not grant permission to replace a const with
a default. Same-name conversion, collection consts, redeclaration and
const-in-loop restrictions remain. So do adjacency, per-binding locality,
shared/inherited-taint ownership, every-continuing-path resolution,
explicit `:!` reads and the EXIT/ERR and signal contracts.

This introduces neither a new declaration form nor a temporary mutable
phase.

## What the current implementation does

Source inspection at `59489a40`, not a fresh runtime probe, confirms
the two relevant boundaries:

- `check_const_writes` in `sash/src/validate.rs:134-144` uses
  `stmt_bound_names`, whose command arm adds `attest_retag_target`
  (183-222) to the real command-bound names.
- `push_attest_retag` in
  `sash/src/transform/stmt_handlers.rs:510-522` uses `state.define` for
  the validation version. `AttestRetag` aliases the source value.

Removing only the validator refusal would reach the lowering invariant.
The implementation must distinguish validation versions while retaining
the real-write guard, across standalone and caught paths and conversion
sources. No lowering architecture or new diagnostic was selected here.

The old task said no DD ruled on the rejection. That was inaccurate by
the time of this discussion: DD86's F13 addendum explicitly recorded the
retained behavior. F13 had preserved it under the port's no-digression
rule. Today's choice supersedes that part; it does not retroactively
make the port a different decision. The historical c13 probe was not rerun.

## Recording and queued enforcement

DD86 carries the accepted rule and its old real-write paragraph is
reconciled in place. DD24/DD35/DD93/DD128 and GUIDE agree. The existing
P3 task retains its slug and becomes implementation work; the findings
index and status distinguish the settled policy from pending enforcement.

The task requires real CLI acceptance and failure-path evidence in both
backends, preservation of real-write and taint restrictions, and
before/reversion controls. Changed spec contracts need fresh ids;
their counts have not been guessed in this documentation-only step.

No compiler, runtime, converter, spec or build code changed. No new
behavior probe, sweep or coverage run was made. Journal 577 remains
the accepted implementation baseline.
