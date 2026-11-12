title: Require explicit handling or discard of getenv lookup status
status: open
priority: P3
kind: work
depends: []
gates:
  - "every otherwise-legal getenv lookup must have a compatible explicit status consumer or adjacent source discard; an unhandled lookup receives a located compile-time refusal"
  - "the obligation covers typed/untyped bindings, nested value positions and executable bodies, independently of attest, trusted reads and taint-disable switches"
  - "legal capture-conditions consume actual presence status; present-empty succeeds and absent fails with empty text, without evaluating the lookup twice"
  - "explicit $(- getenv NAME) retains the string and its taint while discarding only lookup status; pure statusless intrinsic restrictions remain"
  - "outer commands, tests, captures and functions do not lend handling or discard to operand/body lookups; source discard remains distinct from internal handled lowering"
  - "a source-discarded lookup status cannot also feed a condition or logical decision; existing grammar, arity, status-read and fatal-failure rules remain"
  - "taint, adjacency, failed-attest resolution, ownership and handler analysis remain independent; acquisition-failure fallback does not declassify a binding"
  - "affected fixtures and converter output are inventoried; focused native/C before/reversion evidence and fresh-id accounting support the ordinary predicted gate"

## Accepted contract

DG reconsidered DD101's silent-discard exception and directed:
**"require explicit handling or discard; record; update rules; add to backlog"**
(2026-11-12; journal 629).
[DD101](../decisions/DD101-getenv-lookup-status.md) is authoritative.

An unhandled `getenv` lookup is a **compile error**, not an implicit runtime
assertion and not a successful value-only use. Capturing, assigning,
interpolating, passing or validating its string does not consume lookup status.
Neither `:!` nor a taint-disable switch supplies the missing status consumer.
The rule does not depend on whether a name is expected to exist.

Use the existing untyped capture-condition consumers (`if`/`elif`,
`while`/`until`, outer negation and logical-list decisions), or explicitly
discard status inside `$(- getenv NAME)`. Existing shape restrictions remain:
a typed declaration is not a capture-condition, and a general expression does
not acquire a status merely because it contains a lookup.

The value/status contract is unchanged: a present name, even with empty text,
succeeds; an absent name fails and supplies empty text. Both are tainted.
Explicit discard keeps the value and its taint. Status handling does not
validate contents or turn an acquisition-failure fallback into failed-attest
resolution. Evaluate each lookup once.

## Boundaries

Handling or discarding an enclosing command/function result does not handle a
lookup in its operands or body. Named and anonymous bodies retain their fresh
execution contexts. A later `$?` read is not retroactive handling.

The prefix belongs on the lookup, not the assignment. `x=$(- getenv NAME)`
is value-only use; `- x=$(getenv NAME)` is not an alias. Conversely,
`if x=$(- getenv NAME)` tries to consume the status source `-` discarded.
Do not confuse explicit discard with compiler machinery that disables an
implicit abort because a real consumer handles the result.

No new bare `getenv` command, multi-statement intrinsic capture, redirect
spelling, anonymous status return, result kind or status-to-integer coercion
is introduced. Keep one-name arity, actual lookup presence rather than a
nonempty-text test, existing status-read permission and fatal-fault behavior.
No diagnostic number or internal representation is selected here.

## Evidence and migration

The policy discussion read DD101 and the original completed
`getenv-presence-handling` record; it did not run a new native/C probe.
The original task remains done for lookup-status delivery. Its silent-absence
permission and taint-as-presence rationale are historical, not acceptance gates.

Inventory source forms and the shared result-use classification before
implementing. Cover otherwise-legal direct bindings, declarations, argument/
interpolation and nested-body uses, including EXIT/ERR. Main positives use
normal taint checking; separate controls establish independence from the taint
switches. Missing-status assertions must name their distinguishing diagnostic,
not pass because a fixture also trips taint, arity or unused-variable checks.

Preserve handled absence, present-empty success, ordinary nonempty success,
explicit value-only discard and the existing handling-position status reads.
Use before/reversion controls for the new obligation and its boundaries; a
green sweep alone is insufficient.

Migrate affected specs by their purpose. An intentional value-only environment
read can use explicit discard, but do not blanket-rewrite fixtures that test
lookup handling or let an earlier new refusal masquerade as proof of a taint,
body-boundary or other unrelated guard. Changed contracts take fresh ids above
database history; inventory first, then predict additions and retirements.

Review converter-generated lookups and executable reference examples too.
Generated source receives no exemption from DD101; follow DD57's existing
conversion/annotation boundaries. This recording does not choose an automatic
converter rewrite or authorize implicit discard in the compiler.

## Coordination

`reject-discarded-status-consumers` owns the general consumed-after-discard
refusal; `reject-nonadjacent-suppression` owns prefix attachment.
`reject-statusless-intrinsic-suppression` must retain getenv's real second
result. Coordinate their getter cases without assuming those whole tasks
are prerequisites.

`predicate-capture-conditions` owns the separate named-predicate extension.
`reject-anonymous-capture-status-returns` keeps lexical return ownership.
Neither the getter's status nor a named callee's return grants an anonymous
body a new result. Resolve actual shared-path prerequisites during
implementation; no blanket dependency or delivery sequence is selected.
