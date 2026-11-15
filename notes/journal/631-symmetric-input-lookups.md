# 631 - Symmetric input lookups

DG asked whether program-argument and environment lookups should be symmetric,
then confirmed both with and without defaults:
**"yes to all 4, update decisions, outline, and backlog"** (2026-11-13).

The accepted forms are `$(arg N)`, `$(getenv NAME)`, `$(arg N DEFAULT)` and
`$(getenv NAME DEFAULT)`. Without a default, the lookup supplies text plus
presence status, which must be explicitly handled or discarded. Missing input
supplies empty text and failure; bare `arg` no longer implicitly asserts
presence. Required arguments can say `x=$(arg N) || fail 1`.

A literal default handles absence within the lookup and returns only the
selected string. It does not expose a status for a capture-condition or an
inner `-`, and does not synthesize success. Present-but-empty always counts as
present and never selects a default. An explicitly empty default is still a
default. Malformed/negative indices remain errors, not missing input.

All four forms remain tainted, including fallback outcomes. Presence handling,
discard and defaults do not validate contents. Ordinary index/type rules,
literal-default restrictions, body/operand boundaries and legal taint resolution
remain. Program `argc`/`argv` and function-local arguments are unchanged.

The AI reconciled [DD101](../decisions/DD101-getenv-lookup-status.md) as the
shared contract, [DD60](../decisions/DD60-argv-trio.md) for program arguments,
and the dependent current-rule lists and bodies. Superseded no-default and
bare-argument abort claims are struck rather than left as competing rules.
The existing [lookup-status task](../tasks/require-getenv-status-handling.md)
now owns the full change, including form-aware result classification, migration,
and before/reversion evidence. Related capture/discard tasks agree; the original
completed getenv delivery remains completed.

The [specification outline](../../docs/SPEC-outline.md) includes the four forms
in its existing pedagogic order and list-only format, treating accepted pending
work as complete within the draft. The original `2ef86c2a` snapshot and main
remain unchanged. This review set stays uncommitted on `spec-outline`.

No compiler, runtime, converter, spec or build changes, new execution evidence,
spec sweep or coverage run. Implementation remains backlog work. The separate
discussion of `write` spacing produced no ruling and is unchanged here.
