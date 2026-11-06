title: Recovery classes, not setup position
status: complete

DG rejected setup versus execution as the useful dividing line. A failed
fork should abort like heap exhaustion, rather than allow an unpredictable
mixture of successes and failures. A file-open failure is often specific
to its destination, leaving clear ways to proceed elsewhere.

DG then endorsed two classes: failures that permit predictable, useful
continuation when caught or suppressed, and faults for which continuing
is worse than stopping. Path-specific file errors belong to the first;
fork failure belongs to the second. This rules out catching or inverting
a failed fork as well as suppressing it.

Copilot recorded the shared rule in DD130, separate from DD70's existing
operational/value-result channels, and reconciled the operative DD55,
DD70 and DD128 passages in place. GUIDE distinguishes the accepted policy
from current implementation. A recoverable attached-redirect failure is
part of its command's handling context, but no command may run with failed
setup, no output may leak to its original destination, and continuation
requires restored descriptor state. Earlier file side effects are not
transactionally undone.

The implementation backlog has three owners: the new
`runtime-failure-classification` task owns the operation/cause/state
inventory and shared implementation; the existing fork-by-form bug now
requires fatal routing everywhere; the new `recoverable-file-failures`
task owns real file-error handling and depends on classification.
The rule covers sash-owned file operations generally, not just the first
output-redirection reproducer.

The old setup question is closed as a decision, not a fix. Compound
suppression's policy prerequisite is satisfied, while the independent
direct-stderr repair still removes unnecessary descriptor operations.
The new policy does not supply an exhaustive errno table or a newly
chosen fork status. Resource errors and recovery state must be accounted
for explicitly, without relabeling missing handling support as fatal.

No-reach, function result kinds, explicit exit/fail, actual child-status
boundaries and DD128's once-only/main-process handlers remain. Partial
pipeline construction still closes and waits without killing children;
there is no new process-tree fatal-broadcast protocol.

This is documentation/backlog recording only. Compiler, runtime, spec and
build sources are unchanged; no new sweep or coverage run was made.
The native observations from journal 580 remain the evidence, and journal
577's accepted gate is unchanged. Implementation remains pending.
