title: Implement symmetric arg/getenv lookups and explicit presence handling
status: open
priority: P3
kind: work
depends: []
gates:
  - "arg N and getenv NAME supply text plus presence; arg N DEFAULT and getenv NAME DEFAULT supply only the selected string, with literal-only defaults"
  - "every otherwise-legal bare lookup must have a compatible explicit status consumer or adjacent source discard; unhandled lookups receive located compile-time refusals"
  - "the bare-lookup obligation covers typed/untyped bindings, nested value positions and executable bodies, independently of attest, trusted reads and taint-disable switches"
  - "legal bare-lookup capture-conditions use actual presence: present-empty succeeds; absent fails with empty text rather than arg's former unconditional abort; evaluate the index/name and lookup once"
  - "defaults apply only on absence, including an explicitly empty default; present-empty keeps its value, and malformed/negative argument indices retain their errors"
  - "defaulted captures have no status to test, discard or grant status-read permission; explicit $(- arg N) and $(- getenv NAME) retain text while discarding only presence"
  - "outer commands, tests, captures and functions do not lend handling or discard to operand/body lookups; source discard remains distinct from internal handled lowering"
  - "a source-discarded lookup status cannot also feed a condition or logical decision; ordinary sole-intrinsic, consumer, typing, status-read and fatal-failure rules remain"
  - "all four forms stay tainted, including absent/default outcomes; adjacency, failed-attest resolution, ownership and handler analysis remain independent"
  - "affected fixtures and converter output are inventoried; focused native/C before/reversion evidence and fresh-id accounting support the ordinary predicted gate"

## Accepted contract

DG reconsidered DD101's silent-discard exception and directed:
**"require explicit handling or discard; record; update rules; add to backlog"**
(2026-11-12; journal 629).
He then accepted all four `arg`/`getenv` forms, with and without literal defaults:
**"yes to all 4, update decisions, outline, and backlog"**
(2026-11-13; journal 631). This expands the existing implementation owner;
it does not reopen the completed original `getenv` presence delivery.
[DD101](../decisions/DD101-getenv-lookup-status.md) is authoritative.

| form | result |
|---|---|
| `$(arg N)` | string plus argument-presence status |
| `$(getenv NAME)` | string plus environment-name-presence status |
| `$(arg N DEFAULT)` | present value or literal DEFAULT; value-only |
| `$(getenv NAME DEFAULT)` | present value or literal DEFAULT; value-only |

An unhandled **bare** lookup is a compile error, not an implicit runtime
assertion. Capturing, assigning, interpolating, passing or validating its
string does not consume presence. Neither `:!` nor a taint-disable switch
supplies the missing consumer, even when the input is expected to exist.

Use the existing untyped capture-condition consumers (`if`/`elif`,
`while`/`until`, outer negation and logical-list decisions), or explicitly
discard status inside `$(- arg N)` / `$(- getenv NAME)`. Existing shape
restrictions remain: a typed declaration is not a capture-condition, and a
general expression does not acquire a status merely because it contains a lookup.

Present input, including empty text, succeeds; absent input supplies empty
text and failure. For `arg`, a valid index at or beyond `argc` is absence,
replacing its former unconditional abort. Required arguments can use
`x=$(arg N) || fail 1`; do not silently restore the old assertion.

A literal DEFAULT explicitly handles absence, so that form returns only the
selected string. Present-empty never selects DEFAULT. An explicitly empty
DEFAULT still selects the value-only form. There is no spare status for a
capture-condition, inner `-` or fresh status-read permission; do not model
fallback as a synthetic successful lookup.

All four results remain tainted, including when DEFAULT is selected. Handling,
discard and defaults do not validate input or clear preexisting unresolved
taint. Acquisition-failure fallback is not DD128 failed-attest resolution.
Evaluate the index/name once and perform one lookup.

## Boundaries

Handling or discarding an enclosing command/function result does not handle a
lookup in its operands or body. Named and anonymous bodies retain their fresh
execution contexts. A later `$?` read is not retroactive handling.

The prefix belongs inside a bare lookup's capture, not on the assignment.
`x=$(- arg N)` / `x=$(- getenv NAME)` use only text; an outer `- x=...`
is not an alias. Conversely, `if x=$(- arg N)` / `if x=$(- getenv NAME)`
try to consume discarded status. Defaulted captures reject the inner prefix
because no presence result exists to discard.
Do not confuse explicit discard with compiler machinery that disables an
implicit abort because a real consumer handles the result.

Add the optional second operand to `getenv` under the same DD65 literal-only
contract as `arg`'s default, including structural rejection of expansions.
A clean variable or quoted interpolation is not a literal default; validate
the default even if the lookup could succeed. Preserve ordinary string-operand
typing for `getenv` and DD60/DD112 index validation for both `arg` forms.
Malformed/negative indices are errors, not absence, and defaults do not handle
operand errors or fatal faults. `argc`, `argv` slices and function `args` stay.

No new bare-command or pipeline-stage use of either intrinsic, multi-statement
intrinsic capture, redirect spelling, anonymous status return or
status-to-integer coercion is introduced. Use the actual legal lookup form's
result kind, not a name-wide exception that grants status to defaulted forms
or treats every `arg` as statusless. No diagnostic number or internal
representation is selected here.

## Evidence and migration

The discussions read DD42/DD60/DD65/DD101 and the original completed
`getenv-presence-handling` record; no new native/C probe was run.
The original task remains done for lookup-status delivery. Its no-default,
silent-absence and taint-as-presence claims are historical, not current gates.
The completed original arity/placement and argument-family tasks likewise
remain completed; this task owns the new contract.

Inventory source forms and the shared result-use classification before
implementing. Cover otherwise-legal direct bindings, declarations, argument/
interpolation and nested-body uses, including EXIT/ERR. Main positives use
normal taint checking; separate controls establish independence from the taint
switches. Missing-status assertions must name their distinguishing diagnostic,
not pass because a fixture also trips taint, arity or unused-variable checks.

Cover both input sources with absent, present-empty and present-nonempty
values; repeat with nonempty and empty literal defaults. Pin selection and
result use independently of taint. Preserve zero-based indexing, `argc`
including argument zero, legal computed indices and malformed/negative-index
errors. An effect witness must prove once-only index/name evaluation.

Exercise bare `arg` and `getenv` in the same legal capture-condition consumers,
including full handling-position status reads and outer negation. Defaulted
captures work in ordinary value positions but cannot supply that status.
Use before/reversion controls for selection, classification, mandatory
handling and the independent boundaries; a green sweep alone is insufficient.

Migrate affected specs by their purpose. An intentional value-only read can
discard presence; a test requiring an argument must express that requirement,
not silently change to an optional fallback. Review existing default cases
and bare-`arg` abort assertions too. Do not blanket-rewrite fixtures testing
lookup handling, or let an earlier new refusal masquerade as proof of a taint,
body-boundary or unrelated diagnostic guard. Changed contracts take fresh ids
above database history; inventory first, then predict additions and retirements.

Review converter-generated lookups, executable reference examples and active
backlog reproductions/gates too. Keep historical measurements labeled rather
than rewriting them as though they ran under the new contract.
Generated source receives no exemption from DD101; follow DD57's existing
conversion/annotation boundaries. This recording does not choose an automatic
converter rewrite or authorize implicit discard in the compiler.

## Coordination

`reject-discarded-status-consumers` owns the general consumed-after-discard
refusal; `reject-nonadjacent-suppression` owns prefix attachment.
`reject-statusless-intrinsic-suppression` must preserve the real second result
of **bare** `arg`/`getenv` while rejecting inert suppression on their
**defaulted** forms. Coordinate getter cases without assuming those whole
tasks are prerequisites.

`predicate-capture-conditions` owns the separate named-predicate extension.
`bug-sb4025-advice-misdescribes-rhs` owns broader capture-condition advice;
keep bare/default lookup reasons accurate without selecting its separate
inner-negation or quoted-RHS extensions.
`reject-anonymous-capture-status-returns` keeps lexical return ownership.
Neither the getter's status nor a named callee's return grants an anonymous
body a new result. Resolve actual shared-path prerequisites during
implementation; no blanket dependency or delivery sequence is selected.
