# 629 - Require explicit handling or discard of getenv status

DG questioned DD101's permission to ignore lookup absence after accepting
named predicate capture-conditions. The policy discussion read DD101 and
the completed original lookup-status task, rather than inferring a reason
from current compiler acceptance.

The recorded reasons were convenience, preserving existing plain captures,
and avoiding a second mandatory gate beside taint validation. The last
argument conflated two facts: acceptable contents do not establish presence.
A valid predicate may admit empty text while the application still requires
the name to exist. An absent lookup and a present empty value cannot be
distinguished by inspecting that text.

The old claim that `||` rules out abort was overbroad too. It rules out
unconditional abort before handling, not the ordinary distinction between
a handled failure and an unchecked one. Neither observation forces a
particular new policy; DG selected it:

> require explicit handling or discard; record; update rules; add to backlog

## The accepted rule

Every `getenv` lookup must have a legal explicit status consumer or source
discard. An unhandled lookup is a compile error, not an implicit runtime
assertion. Capturing or validating its string alone does not consume status.
`x=$(- getenv NAME)` retains the tainted value while explicitly disregarding
presence. Existing untyped capture-conditions consume the actual lookup
result: present-empty succeeds, absent fails with empty text.

Status handling and taint remain independent. Attest, `:!` and taint-disable
switches do not supply a status consumer; handling or discard does not
declassify either outcome. A fallback on acquisition failure is still not
DD128 failed-attest resolution.

The obligation follows the lookup in every otherwise-legal value/body
position. An outer handler does not cover operand or body lookups, and a
later status read is not retroactive handling. No new intrinsic statement
form, anonymous return, redirect syntax or result kind is introduced.
Discarded lookup status cannot also drive a condition. Existing arity,
status-read, fatal-fault and handler boundaries remain.

## Record and reconcile

The AI updated DD101's opening and current contract, struck the former
silent-discard permission and rationale, and corrected the `||` argument.
DD42/DD53, DD55/DD70, DD88 and DD111 now agree. Current taint/adjacency/
handler examples in DD24/DD53/DD121/DD128 explicitly discard presence so
they still illustrate their own independent obligations.

The new P3 work item
[`require-getenv-status-handling`](../tasks/require-getenv-status-handling.md)
owns enforcement and affected-fixture accounting. The original
`getenv-presence-handling` task remains done for its delivered feature;
its old acceptance claims are labeled historical. Predicate-capture,
anonymous-return and discard-related owners preserve the revised boundary.
No whole-task dependency or implementation order is imposed.

Implementation must inspect shared classification and every source placement,
including generated converter output. Missing-status checks must not be
masked by co-fired taint/arity/unused diagnostics. Migrations must preserve
what existing specs prove, with fresh ids for changed contracts and actual
before/reversion evidence. No counts, diagnostic code, internal representation
or converter migration strategy are selected by this recording.

Only decision/task/journal/status Markdown changes. No compiler, runtime,
spec, build rule or shipped architecture changes; no new behavior probe,
spec sweep or coverage measurement is claimed. Enforcement remains queued,
and the generated status block remains the previous actual gate snapshot.
