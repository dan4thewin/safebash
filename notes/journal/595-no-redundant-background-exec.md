title: No redundant background exec
date: 2026-11-09

## DG's ruling

Journal 594 recorded direct exec for a single-external-command background
job, without inferring an additional syntax ban. The next question was
whether to retain the now-redundant `exec cmd &` spelling.

The AI recommended rejecting it and advising `cmd &`, matching the
existing reason for rejecting `exec` as a pipeline stage. Keeping an
explicit but redundant spelling was the alternative. The question
expressly preserved ordinary exec and meaningful replacement inside
otherwise legal function or multi-command bodies.

DG answered:

> reject

## The boundary

A direct single-command `exec cmd &` job must reject. Arguments,
attached redirects and an additional mark do not justify the prefix.
The diagnostic points at `exec` and says to remove it, retaining `&`.
Advice to remove `&` would change the job into foreground replacement.

This applies wherever that direct job appears. It is not a blanket
ban on exec in a background child:

```sh
fn replace { exec /path/to/command }
replace &                              # otherwise legal replacement

fn launch { exec /path/to/command & }   # redundant direct job: reject
```

An explicitly backgrounded function with an ordinary external command
still provides the checked wrapper chosen in journal 594. Otherwise
legal exec inside a multi-command body, foreground exec, the fd-only
rules and the pipeline-stage refusal retain their existing contracts.
No result kind, failure class, status latch or handler rule changes.

## Recording and queued implementation

DD55 and GUIDE now replace the temporary permission in place. Q2 and
the findings index agree. The existing P3
`background-external-direct-exec` owns the refusal as well as removal
of the wrapper; no separate overlapping implementation task is added.

Both changes must ship together. Retiring the current direct-PID
spelling while ordinary jobs still have wrappers would remove the
capability DG chose. Refusal coverage must distinguish the new guard
from older capture/stage refusals, and rejection alone does not prove
the ordinary job's process shape changed.

This is policy/backlog recording only: no compiler, runtime, spec or build
code changed, and no new behavior probe, sweep or coverage run was made.
Historical Q2 probes remain historical; implementation must measure
current fixture impact and allocate fresh ids for changed contracts.
Journal 577 remains the accepted implementation baseline.
