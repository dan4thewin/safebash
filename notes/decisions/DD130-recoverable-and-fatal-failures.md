title: DD#130 - Recoverable failures and fatal faults
status: accepted

Decided by DG on 2026-11-06 (journal 581). DG distinguishes failures where
catching or suppressing permits predictable, useful continuation from those
where "continuing is a worse choice than aborting". Path-specific file
errors belong to the first class. DG separately specified that failure
to fork must abort like heap exhaustion.

This is the accepted contract. Implementation is pending in the tasks
below; the current compiler still has the setup-position inconsistencies
recorded in `q-setup-failure-suppressibility`.

## The two failure classes

| Class | Meaning | Handling |
|---|---|---|
| Recoverable failure | The operation failed, but execution remains in a known state with meaningful ways to proceed | The ordinary implicit check aborts if unchecked; a catch or explicit suppression may continue |
| Fatal fault | Continuing is a worse choice than stopping | Abort the executing process; `if`, `||`, `!` and `-` cannot turn the fault into a resumable result |

Classification follows the failure and the resulting execution state,
not whether it occurred during setup or command execution. A single
operation can encounter both classes. Being catchable today does not
prove that a failure is recoverable; lacking a handling form today does
not make an ordinary failure fatal.

This is distinct from DD70's operational-result versus must-consume-value
channels. It changes neither function return kinds nor the rule that an
integer result is not a status. Failure class is not encoded by choosing
a special numeric exit status.

## Settled cases and classification obligations

- **Fork failure is fatal**, whether launching an ordinary command, a job,
  a pipeline stage or a capture. A condition must not enter either branch,
  a `!` must not turn it into success, and a `-` must not continue after
  the failed fork.
- **Runtime heap exhaustion and DD25 arithmetic faults remain fatal.**
  Their existing DD128 cleanup protocol is unchanged.
- **Path-specific file errors are recoverable.** A missing directory,
  denied access or a read-only destination does not by itself make future
  execution unreliable. This principle applies to sash-owned file
  operations, not only the first output-redirection example.
- **Not every failed `open` is a path error.** Resource errors such as
  `EMFILE`, `ENFILE` or `ENOMEM`, pipe creation failures, and descriptor
  save/restore failures need classification by their cause and the state
  left behind. The implementation task must establish that mapping,
  applying this rule and the settled cases; no blanket "all setup is
  fatal" or "all open errors are suppressible" shortcut.
- **Continuation requires a known descriptor state.** An open failure
  contained by successful restoration is different from an unrecovered
  restoration failure that leaves output destinations unreliable.

The classification inventory must retain the operation and error cause;
one errno does not necessarily mean the same thing at every syscall.
Genuinely underdetermined policy cases go to DG before implementation,
without reopening the two-class rule or its settled cases.

## Recoverable file failures and suppression

A command's attached redirect is part of that operation's failure surface.
Where the handling form is otherwise legal, its recoverable open failure
can be caught or suppressed, just like its recoverable execution failure:

```sh
- echo diagnostic >optional.log
```

If opening this path fails recoverably, do not run `echo`, do not write to
the original stdout, restore scoped descriptor state, and continue.
Without handling or suppression, the ordinary implicit check still aborts.
A fatal fault encountered by the same spelling still aborts.

This replaces DD70's rule that `-` could discard the command's verdict but
never its attached open failure. It does not extend suppression into a
called function's body, an argument capture or a value call. Nor does an
attached redirect change a function's void/status/integer kind.

Source-prefix eligibility remains DD55's separate rule. In particular,
its 2026-11-08 rejection of `-` on `return`, `exit`, `fail`, `break` and
`continue` also covers attached redirects. A recoverable open failure
does not legalize that prefix or become fatal merely because it lacks
that spelling of a handler.

Redirects remain ordered, stopping at the first failure. Later redirects
and the command body do not run. Earlier file creation, truncation or
writes are not rolled back; recoverability is not transactional execution.
DD86's 2026-11-07 arguments-first ruling places argument evaluation before
this redirect phase, including in stages. Its completed effects are not
undone by a later redirect failure either.
Do not invent a successful empty value to conceal a failed file operation.

**Directory selectors follow this rule too** (DG, 2026-11-09;
DD39/DD69/DD73/DD131). A directory successfully enumerated with no
matches produces zero elements; a failed directory open/read or required
`getcwd` is an operation failure. Preserve the cause, rather than treating
an errno as empty success or a read error as EOF. DD131 provisionally supplies explicit
`glob`/`rglob` commands with operational statuses, including success on
zero matches. On operation error an ordinary existing target is unchanged;
an immutable scratch construction leaves a fresh empty instance, not
its previous batch. These modes are a branch experiment, not shipped behavior.

DD132 adds required validation before runtime regex use: `r?` returns
quiet false for an invalid candidate, while allocation/resource faults
remain fatal. Regex consumers' logical no-match results are distinct
from their I/O or redirect failures. Neither failed validation nor
`rglob`'s zero matches may disguise an actual operation failure.

Coordinate the trial with `recoverable-file-failures` and the classification
inventory. It neither grants a status to an assignment/loop nor changes
fatal-fault handling. An atomic collection update is not rollback of
already consumed input or other completed effects. The implementation must
account for allocation failures and the actual destination visible to ERR,
not assume staging some entries alone proves the publication guarantee.

## Cleanup and process boundaries

DD128 still governs termination: a first fatal fault in main or EXIT
attempts ERR once; a fatal fault in ERR terminates without re-entry.
Dispatch allocates nothing and does not resume the failed operation.
Forked children run neither parent handler. This rule does not introduce
process-tree killing, implicit waits for background jobs, or a new
cross-process fatal-notification protocol.

Failure of a fork in the parent is not a child's returned status: no child
was created. A parent observing an actual child's completion retains the
existing child-status rules. Arbitrary external exit codes are not proof
of a particular internal failure class.

Partial pipeline construction still closes its pipes and waits for
already-started stages, without forced termination or a kill timeout.
Stopping further work does not undo work that already completed.

## Implementation backlog

- `runtime-failure-classification`: establish and implement the shared
  operation/cause/state classification and align runtime, both backends
  and ERR effect analysis.
- `bug-fork-failure-reporting-by-form`: use the fatal path uniformly for
  every failed fork, with consistent diagnostics and no resumed branch.
- `recoverable-file-failures`: expose and enforce recoverable file-error
  handling, including attached redirects; depends on the classification
  work so fatal causes are not accidentally discarded.

`q-setup-failure-suppressibility` is resolved as a question, not as an
implementation. `retire-suppression-on-compounds` no longer awaits a
blanket setup-suppression ruling.
