# Rebase and function-result discard

DG asked to rebase onto the latest hand-parser and review the recent
decision about `- f`.

## Rebase

The experiment's old base was `899179d3`, with twelve commits ending at
`f8c715ab`. They rebased cleanly onto hand-parser `2fc6a22c`, ending at
`3ab0a613` before this documentation correction. All twelve patches are
identical under range-diff. The prior branch tip remains in the local
backup branch `backup/cpp-runtime-before-hand-parser-20261101`.

Checksums confirmed that every tracked `sash-cpp/` file was unchanged by
the rebase. The protected producer draft remains bit-identical and
untracked; it was not stashed, staged, or included in the replay.

## DG's ruling

[Native DD#55](../../notes/decisions/DD55-error-model.md), F6 unit B
(2026-10-30), implements DG's 2026-10-29 ruling: a `-` discards the answer
of the one command or call it prefixes and reaches nothing else. It
supersedes the 2026-10-20 complete-call suppression correction.

- `- f` discards f's returned int or status, not failures inside its body,
  argument evaluation, or nested calls.
- A failing unhandled command in f still aborts before later commands or
  the return, even when f's caller writes `- f`.
- Conditions consume a call's answer without unchecking the called body
  (DD#55's F5 addendum, 2026-10-29).
- A direct in-process `- void_f` is SB4076: nothing is returned to discard.
  `$(- int_f)` is SB4076 too, because the capture already binds the int.
  A stage or job has a separate process status.
- A status-returning function's answer must be handled or deliberately
  discarded; a bare unhandled call is SB4016.

The native question
[q-suppression-on-user-functions](../../notes/tasks/q-suppression-on-user-functions.md)
is done. Setup-failure suppressibility and residual assignment/list/compound
forms remain in their own backlog tasks; the ruling does not silently
settle those.

## Correction to the experiment

The AI's earlier explanation and explicit-context recommendation relied on
the superseded rule. In particular, the claim that `- f` must allow later
commands after an unhandled failure in f is wrong under DG's current rule.
That recommendation is withdrawn. No inherited checkedness argument or
scoped unchecked state is needed for function-result discard.

The ordinary C++ model already has the required boundary: f's checked
operations can throw, while its caller handles or discards only a value
that f actually returns. A caller must not catch those body failures merely
to ignore the returned answer. Argument evaluation also retains its own
checks. No runtime, producer, or example source changed.

The current decision, roadmap, status, contributor instructions, README,
and error-path tour now say this consistently. Earlier journals remain
historical records. The next critical-path slice is still integrated
shutdown, END, and signals; function checkedness is no longer one of its
open decisions.
