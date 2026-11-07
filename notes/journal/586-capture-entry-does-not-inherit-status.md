title: Capture entry does not inherit caller status
status: complete

DG chose **"no inheritance"** on 2026-11-07. An anonymous capture body
starts without its caller's `$?`, like a named function body. Its own
handling flow can establish status under the existing rules. Copilot
recorded the contract in DD55/DD86 and reconciled DD126, GUIDE and the
linked backlog.

The example in the discussion was:

```sash
/bin/sh -c "exit 3" || echo "$(echo "$?")"
```

The inner executable body must reject the borrowed status with SB5012,
even though the capture is the first expression of the outer handling
branch. Each nested capture starts fresh in turn. Assignment, argument
and interpolation positions, redirects and outer marks do not change
that boundary.

This is status availability, not variable isolation or a default integer
zero. The caller can read its status at a legal handling position into
an ordinary binding, which the capture may then use under DD86's lexical
rules. Global writes still survive in the same process. A failure handled
inside the capture supplies its own status; the ruling does not remove
that capability.

Direct value intrinsics are not anonymous executable bodies.
`/bin/false || echo "$(status)"` remains a read of the surrounding
handling status, just as `$?` is. A getter inside an executable capture
instead reads that body's context. The same distinction preserves direct
`getcpid`/`waitstatus` expressions and their punctuation/arithmetic forms.
An AST or syntax test that treats every `$()` as a new body would reject
the canonical getters and would not implement this decision.

DD126 already requires a function body to start without its caller's
latch. The text now names anonymous bodies explicitly, consistently with
DD86. A spawn or reap inside the body can establish a fresh latch.
That is not a new job-ownership model: a same-process body may wait for
an existing child and read the newly reaped status.

The existing P2 `bug-sb5012-sb5094-nested-body-flow` owns enforcement,
including the already-filed false rejections and missed diagnostics from
checking inner reads against an enclosing statement's flow. It remains
open; only its entry-state question is closed. Structural branches and
reader loops must keep their condition-derived flow rather than becoming
fresh function contexts indiscriminately.

The historical cb13/cb16 and ev12-ev14 entry reads now require SB5012;
cl1 requires SB5094. Named-body refusals remain controls. Positive coverage
must preserve legal body-local handling, spawn/reap, direct getter reads
and explicitly saved outer values. The hoisting task and findings index
now distinguish these refusals from ordering defects. The old suggestion
of zero affected specs is not a prediction; implementation must measure
the current suite and use focused before/reversion evidence.

No compiler, runtime, spec or build code changed. No new behavior probe,
full sweep or coverage run was made; the table results remain their
original measurements and journal 577 remains the accepted implementation
baseline. Capture-entry enforcement is pending, independently of the
accepted word-order, attached-redirect and anonymous-return repairs.
