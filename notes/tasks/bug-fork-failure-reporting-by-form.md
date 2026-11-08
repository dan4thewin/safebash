---
title: "Bug: make fork failure uniformly fatal across execution forms"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "ordinary commands, jobs, pipelines and captures report failed forks through one fatal path, with one fork diagnostic and a consistent documented status in both backends"
  - "a failed fork cannot enter either condition branch, an || alternative, or later code via ! or -; no ordinary result is manufactured"
  - "DD128 dispatch remains once-only: main/EXIT attempts ERR, ERR terminates, and forked children run neither parent handler"
  - "partial construction closes pipes and waits for started stages without force-killing them; no invalid pid, empty-success capture or EOF fallback leaks out"
  - "valid current-language native/C controls prove successful forks and the actual injected failures, including negation, suppression and handler entry"
  - "pre-change/reversion proofs and the predicted ordinary gate pass; DD55 and implementation notes agree"
---

## Accepted contract (2026-11-06)

DG requires fork failure to abort like heap exhaustion: an unpredictable
mixture of progress and failure is worse than stopping. DD130 settles this
for all forms, not only the `-` spelling. No handling branch resumes the
process whose fork failed. This task remains open for implementation.

The table below is historical evidence. Its END forms and some old
function/return spellings are no longer valid source; use current-language
counterparts for new controls rather than treating a parse rejection as a
successful fork-failure test.

## Found

2026-10-27, while porting F1+F4 (journal 537). It is **pre-existing**: every
result below is the same at `8e28c61` (before F1+F4), in both backends, except
f5 and f6, which F1+F4 now rejects (SB4027, SB4019; at `8e28c61` they printed
`y` and `after`).

The fault is the first `clone` failing with EAGAIN
(`strace -e inject=clone:error=EAGAIN:when=1`):

| Id | Program | Result |
|---|---|---|
| g1 | `/bin/true` then `echo after` | rc 127, "/bin/true: error 11" then "/bin/true: command failed (exit 127)" |
| g2 | `if /bin/true { echo y } else { echo n }` | `n`, "/bin/true: error 11" |
| g2s | g2 with `echo "n $(status)"` | `n 127` |
| g3 | `- /bin/true` then `echo after` | `after`, "/bin/true: error 11" |
| g4 | `/bin/true \|\| echo alt` then `echo after` | `alt`, `after`, "/bin/true: error 11" |
| g5 | `END { echo "end $(status)" }` then `/bin/true` | `end 127`, rc 127 |
| g6 | `/bin/echo a \| /bin/cat` then `echo after` | rc 1, "fork: error 11" |
| g7 | `x=$(/bin/echo a)` | rc 1, "fork: error 11" |
| g8 | `if /bin/echo a \| /bin/cat { echo y } else { echo n }` then `echo after` | `n`, `after`, no message |
| g8s | g8 with `echo "n $(status)"` | `n 1` |
| g9 | `if x=$(/bin/echo a) { … } else { echo n }` then `echo after` | `n`, `after`, no message |
| g9s | g9 with `echo "n $(status)"` | `n 1` |
| f1 | `fn f { return ! { /bin/true } }` then `f` then `echo "rc=$?"` | `rc=0` (unfaulted: `rc=1`) |
| f2 | `if ! /bin/true { echo y } else { echo n }` | `y`, "/bin/true: error 11" |
| f3 | `if /bin/echo a \| ! /bin/grep -q b { echo y } else { echo n }` | `n`, no message |
| f4 | `fn f { return ! if /bin/true { /bin/true } }` then `f` then `echo "rc=$?"` | `rc=1`, the same as unfaulted |
| e1 | `END { /bin/echo hi &; - wait; echo after }` then `echo main` | `main`, `after`, rc 0, no message |

e1 was added 2026-10-28 (probe `bgf/endf4.sh` in `files/port-f2-probes.tar.gz`):
the same before and after F2, in both backends. At that time, outside END,
F2 made a background job report the pipeline way (below); END kept it silent.
DD128 now removes END and its blanket unchecked exception. EXIT is checked;
ERR requires explicit handling of recoverable failures. DD130 makes fork
failure a fatal fault in either handler. Row e1 is historical, not a
remaining handler exception; the general by-form repair remains open.

So there are three behaviours:
- a simple command reports `<cmd>: error 11` (naming the command that never
  ran, not the fork), always, even when the failure is caught, and leaves 127,
  the status of an exec failure;
- a pipeline or a capture reports `fork: error 11` and leaves 1, but only when
  the failure is uncaught; a caught one is silent;
- under `!`, a simple command's fork failure becomes **success** (f1, f2),
  while a pipeline stage's does not (f3). `if ! /bin/grep -q bad f { … }` runs
  its body when grep never ran. This fail-open is why the task is P2.

f4 is not a fourth behaviour: the faulted condition fails, an `if` that takes
no branch has status 0, and `!` makes that 1, which happens to equal the
unfaulted result.

Probes: `files/port-f1f4-probes.tar.gz`, directories `ff/` (the table and
`runf.sh`, the fault-injecting driver) and `ff2/` (the no-branch `if` status:
d1, d6, t1, t2).

## Cause (read, not yet confirmed by a fix)

- `sash_run` (sash-rt/src/lib.rs) handles a simple command. On a fork failure
  it prints the command's name with the errno and returns 127, which the
  caller treats as the command's own exit status, so `!` inverts it.
- `guard_pipeline_setup` (sash/src/codegen/inst_handlers.rs) and
  `emit_pipeline_setup_failure` (sash/src/emit_c.rs) handle pipelines and
  captures (DD#55 "Pipeline construction failure"). They print `fork:` and set
  status 1 on the abort path only.

F1+F4 modified none of these functions, so this goes to the backlog under DG's
rule for the port (fix a known bug in a function the port modifies; file the
others).

Related: batch-531 B20, landed with F2 on 2026-10-28. A background job whose
fork fails now reports the pipeline way: `fork: error 11`, status 1, an abort
unless caught or suppressed, and `$!` reads -1 (`bgfork.1`-`bgfork.8` in
commands.sh). The former END-specific silence (e1) is retired by DD128.
This is the implementation behavior DD130 replaces with fatal routing;
the accepted rule is no longer "abort unless caught or suppressed".

## Fix direction

Use the existing fatal-dispatch machinery rather than returning an ordinary
failed command result. Name the fork, not an exec that never happened.
Correct the runtime and both generated-code paths, including background
and capture construction. B20's reporting repair was not this fatality fix.

Keep the existing cleanup and process boundaries. A failure in a child
terminates that child without running parent handlers; actual child
completion is still observed through the established status mechanism.
This task does not add process-tree broadcasts, forced asynchronous waits
or child killing.

DD55's 2026-11-08 direct-background-exec rule removes the second launch
fork for a single external job; `background-external-direct-exec` owns
that process-model change. Coordinate the remaining spawn's fatal path
and fault controls with that task. Removing a fork is not proof that
failure of the remaining fork is handled correctly; explicit function
wrappers still exercise their own ordinary external-command forks.

DG selected fatality, not a new numeric status. Establish one documented
fork-fault status using the existing fatal conventions; if that leaves a
genuine policy choice, present it before changing the contract. The old
recommendation of 1 was not a ruling, and a numeric status must not encode
whether the fault is recoverable.

This settled fork case need not wait for classification of unrelated I/O
errors in `runtime-failure-classification`. Use successful syscall controls
and traces proving the intended failed fork; late injected clone counts
must account for restarted entries.

## Related

Q5 is resolved in `q-setup-failure-suppressibility` and DD130. Existing
legal stage/job suppression remains legal for its ordinary result; it
cannot suppress a failed fork. Result-kind restrictions, including the
rejection of integer-function pipeline stages, are unchanged.
