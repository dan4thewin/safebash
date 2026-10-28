---
title: "Bug: a fork failure is reported three different ways by form, and `!` turns it into success"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "every form in the table reports a fork failure the same way: one message naming the fork, one status, in both backends"
  - "`!` does not invert a fork failure: f1 and f2 take the failure path (f2 prints `n`), as f3 already does"
  - "the status a fork failure leaves is the one DG rules on below, and DD#55 records it"
  - "specs pin g1, g2s, g6, g8s, f1 and f2 under an injected fork failure, in both backends"
---

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
the same before and after F2, in both backends. Outside END, F2 makes a
background job report the pipeline way (below); inside END it is still silent.

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
commands.sh). Inside END it is still silent (e1). DD#55's addendum of
2026-10-26 says "Fork failure is meant to abort as written".

## Fix direction

One fork-failure path for every form: print `fork: <errno>`, leave one
status, and follow DD#55's caught/uncaught handling. The failure belongs to
the statement, not to the command that never ran, so `!` does not invert it.
B20 landed first and took the pipeline behaviour, so a background job is now
in the second group, and this fix covers it with the rest (e1 included).

The question for DG is the status. **Recommended: 1**, as pipelines and
captures leave today. 127 is what DD#55 gives an exec failure in the child
("command failed (exit 127)"), and a command that was never forked did not
fail to exec.

## Related (2026-10-28)

Whether `-` may discard a fork failure, and on which forms (`- f &`,
`- f |` for a user function), is open in `q-suppression-on-user-functions`
Q5 (setup failures), with Q2 (may `-` prefix a user-function call at all).
