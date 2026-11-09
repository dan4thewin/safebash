title: "Exec single external background jobs directly; reject the redundant exec prefix"
status: open
priority: P3
kind: bug
depends: []
gates:
  - "A single external background job execs directly in its child in native and C; getcpid/$! identifies that command, not a wrapper"
  - "Arguments and attached redirects preserve the direct job shape and their established evaluation, failure and process boundaries"
  - "Successful exec followed by nonzero exit produces no automatic sash completion diagnostic; wait still yields the pid and waitstatus the child's outcome"
  - "Command stderr and exec-startup errno diagnostics remain; failed forks retain DD130 fatality and children never run parent handlers"
  - "An explicitly backgrounded function retains its checked body even when inlining reduces it to one external command; other job forms retain their semantics"
  - "Direct single-command exec cmd & jobs receive a located refusal advising removal of exec, including through redirects/marks and inside bodies; otherwise legal process replacement inside a function or multi-command body remains"
  - "The prefix refusal ships with the direct-exec default, not before it; otherwise legal - cmd & uses the direct child without a parent-side reporter"
  - "Focused native/C before/reversion controls and a predicted full gate pass; changed spec contracts take fresh global ids"

## Accepted rule

DG, 2026-11-08:

> let's drop the wrapper - the simplification for signalling outweighs the diagnostic

DG then chose **reject** for the redundant direct-job `exec` spelling
on 2026-11-09. DD55 is authoritative; DD70/DD126 and GUIDE agree.
Journals 594 and 595 record the two rulings. This task owns their coherent
implementation, not another process-model decision.

A job consisting of one external command directly execs it in the
background child. The implicit wrapper's second fork, wait and completion
diagnostic go. `$!` / `getcpid` identifies the command itself; the parent
can reap it normally. `wait` returns a pid, `waitstatus` carries the
outcome, and neither automatically aborts or reports a child's failure.

An explicit user function is the opt-in checked wrapper:

```sh
fn job {
    /path/to/command arg
}
job &
```

Its child still checks the external command and reports failure, and
`$!` names the function process. Preserve this distinction at the source
job boundary; do not erase it merely because the function inlines to one
external command.

Reject the redundant direct-job spelling `exec cmd &`, with or without
arguments, redirects or an additional mark. Point at `exec` and advise
`cmd &`, not foreground process replacement. The rule applies wherever
that single-command job appears, including inside a function.

Do not confuse it with an `exec` executed by a body. For example,
`fn replace { exec /path/to/command }; replace &` retains its otherwise
legal process replacement; `fn launch { exec /path/to/command & }`
contains a redundant direct job and must reject. Otherwise legal exec
in a multi-command body remains too. The pipeline-stage refusal and
fd-only rules are unchanged. No general ban on exec in forked code is
selected.

An otherwise legal `- cmd &` execs directly; it does not suppress
operand/body failures or a fatal fault. Pipelines, builtins, functions
and multi-command jobs retain their existing semantics. The pid is not
a process-group handle and does not promise control of descendants.

## Evidence and implementation boundaries

Q2 in `q-suppression-and-stage-residuals` retains the historical
native/C process, signal and status observations at `d60e74f`, with
sources and traces in `files/port-exec-probes.tar.gz`, directory `exq/`.
No new behavior measurement or impact census accompanied these rulings.
The old recommendation to keep the implicit wrapper is superseded.

Land the direct-exec default and the source-prefix refusal together.
Retiring the current direct-PID spelling while ordinary jobs still use
wrappers would temporarily remove the capability the policy preserves.

Reuse the existing direct-child exec machinery where applicable; do not
add a job table, automatic reaping/reporting or a signal-forwarding
wrapper. Preserve arguments-first redirect timing, ordinary operand
checks, lexical ownership, taint and existing body/process boundaries.
Backend-emitted exec is not source `exec` and must not broaden DD86's
capture rejection.

Distinguish a command's later nonzero exit from exec failing to start it.
The former has no sash completion report; the latter retains the child's
errno diagnostic and exit 127. The command's own stderr is unaffected.
Other pre-exec failures retain their operation-specific rules. No
parent ERR transfer, forced wait or process-tree cleanup is introduced.

Coordinate the remaining launch fork with
`bug-fork-failure-reporting-by-form`: deleting the second fork is not a
test of fatal handling at the first one. DD128's child-handler exclusion
and DD130's fatal faults remain. The wider file-failure classification
tasks retain their scope.

## Regression obligations

Prove actual process identity and signalling with a controlled live
external child, exact owned pids and bounded cleanup. Do not infer direct
exec from an exit code or from disappearance of the diagnostic alone.
For a simple job with no forking operands, a syscall trace must show one
launch fork rather than two; distinguish any restarted syscall entries.

Cover success, nonzero completion, a terminating signal, the command's
own stderr, exec-startup failure, arguments and redirects. Preserve
wait-any and wait-by-pid collection and the one-statement latch rules.
Pin the explicit function wrapper's diagnostic and distinct pid ownership,
including a one-command function and its ordinary no-reach behavior.
Keep existing pipeline and multi-command-job controls.

Prove the redundant-prefix refusal separately from process identity.
Use valid fixtures that are not already rejected by capture, stage or
other context guards; assert its distinguishing advice and location.
Cover plain, redirected and marked direct jobs and their placement inside
bodies, with taint enabled and disabled. Preserve ordinary `cmd &`,
checked function jobs and otherwise legal body-internal exec.
An earlier source refusal is not proof that the direct-exec default
actually removed a wrapper. No diagnostic code is selected by the ruling.

Review current specs rather than copying the old probe spellings or
guessing their impact. DD55/DD126 cite errors.sh 55.16/55.128 and
commands.sh 126.36; determine which assertions actually change and keep
the independent status/errno coverage. Changed contracts get fresh ids
above the database's historical high-water mark. Use the real CLI in both
backends, prove the focused changes against the before binary and
appropriate reversions, then predict the full gate from measured impact.
