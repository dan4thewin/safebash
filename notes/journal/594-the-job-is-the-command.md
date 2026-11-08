title: The job is the command
date: 2026-11-08

## DG's ruling

The next question was Q2's split between `cmd &` and `exec cmd &`.
The recorded implementation gives the first a sash wrapper that forks
the command, waits, checks its status and prints a failure diagnostic.
The second makes the background child become the command. `$!` therefore
names different processes, with different consequences when signalled.

The AI initially recommended retaining the split. DG asked what the
wrapper's failure report actually did: it is a stderr diagnostic, not
failure propagation to the main process. The parent can reap the outcome
with or without the wrapper. With a wrapper, it reaps the wrapper's
forwarded status; without it, the command's own. Killing the wrapper is
the important exception: that outcome can leave the command still running.

DG then identified an explicit replacement:

> if we don't have an implicit wrapper, I could get the same result by writing a user function and backgrounding that, yes?

Yes. The function's child retains its ordinary command checks and failure
report. The wrapper is available when written, rather than imposed on
every external background job.

DG chose:

> let's drop the wrapper - the simplification for signalling outweighs the diagnostic

## Accepted contract

A single-external-command background job execs directly in its child.
`getcpid` / `$!` names that command. Its later nonzero exit produces no
automatic sash completion diagnostic, but the parent can still reap it:
`wait` yields the pid and `waitstatus` the outcome. Neither automatic
parent failure propagation nor parent-side reporting replaces the wrapper.

The command's own stderr and exec-startup errno diagnostic remain.
Fork failure remains fatal under DD130; children run neither parent
handler. Waiting remains optional, jobs may outlive the parent, and a
pid is not a process-group or descendant-tree handle.

A backgrounded function remains a checked body, even if it inlines to
one external command. Builtins, pipelines and multi-command jobs retain
their existing semantics. The AI reported that it would not infer the
old proposal's additional ban on redundant `exec cmd &`; the existing
spelling remains legal. An otherwise legal `- cmd &` uses the direct
child too. No additional syntax retirement was selected.

## Recording and queued implementation

DD55's background section and older active addendum now agree in place.
DD70 distinguishes completion reports from exec-startup errors; DD126
records direct PID ownership without changing its latch contract.
GUIDE, Q2, the findings index and fork-failure owner point to the new P3
`background-external-direct-exec` task. Q2's process-model choice is
settled; implementation remains open.

The old native/C probes at `d60e74f` remain explicitly historical.
Implementation must measure current fixture impact, prove actual process
identity and signalling, and preserve explicit wrappers and status
collection. No diagnostic disappearance alone proves the process changed.

This is policy/backlog recording only: no compiler, runtime, spec or build
code changed, and no new behavior probe, sweep or coverage run was made.
Journal 577 remains the accepted implementation baseline.
