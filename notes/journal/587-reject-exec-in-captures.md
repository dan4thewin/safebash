title: Reject process-replacing exec inside captures
status: complete

DG chose **"reject exec within capture"** on 2026-11-08. Copilot recorded
the rule in DD86, reconciled DD55/DD58 and GUIDE, and converted the existing
`q-exec-in-capture` from an open question to an open P3 implementation
bug. The slug and priority are unchanged. This journal closes the policy
recording, not compiler enforcement.

The discussed operation is the source command `exec PROGRAM ...` inside
a stdout-capture body, directly or through called functions. The rule
includes anonymous bodies and captured named functions; a helper's return
kind does not hide an `exec` within the capture. Conditional paths,
wrappers, nested captures and forked child bodies do not provide
exceptions. This is a structural rejection independent of taint.

The motivating example replaced the process that would otherwise finish
the capture. Its output went into a capture nobody completed, and the
assignment and later statements never ran. That is the filing's historical
native/C evidence, not a new execution made for this recording.

DG asked in the preceding discussion how an ordinary external capture,
`x=$(/bin/echo hi)`, works. The current source was traced: it lowers to a
one-stage pipe-backed pipeline, prepares arguments in the parent and forks
one child. The child connects stdout to the pipe and execs `/bin/echo`.
The parent drains the pipe before waiting, strips trailing newlines,
checks the child's status and assigns the result. The command fork is not
a subshell surrounding the capture body.

That ordinary external-command path remains legal. The new rejection is
of the explicit source-level replacement operation, not the runtime's
`execve` syscall. The fd-only form, such as `exec >file`, also remains legal
under its existing descriptor rules.

The alternatives were to keep process replacement in the body or to fork
the body to accommodate it. DG chose rejection. No automatic subshell,
global-state isolation or output-channel change is introduced. This is
not a blanket ban on non-returning paths; DD128's `exit` and `fail` retain
their contracts.

DD58 still distinguishes direct integer-value calls from stdout-capture
bodies. `$(f)` and arithmetic call syntax remain uniform for an integer
function, including through redirects. An integer helper called inside a
stdout-capture body is subject to that body's rejection rule. The checker
must use this semantic distinction rather than banning `$()` punctuation
or relying on the separately queued redirect-dependent kind bug.

The backlog now requires direct and transitive refusal coverage,
taint-independent enforcement, located diagnostics and preservation of
ordinary external captures, fd-only redirection, global effects and
`exec` outside captures. cap.35/cap.36 currently pin the old acceptance
and transport behavior; their changed contracts need fresh spec ids.
The task requires focused before/reversion evidence and measured full-gate
predictions, not a claim that the ruling affects only those two specs.
B34 remains unchecked until enforcement lands.

No compiler, runtime, spec or build code changed. No new behavior probe,
full sweep or coverage run was made. Journal 577 remains the accepted
implementation baseline; word ordering, attached-redirect timing,
anonymous status returns and fresh status entry remain separate queued
repairs.
