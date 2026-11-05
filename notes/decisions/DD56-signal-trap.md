title: DD#56 — Signal model and orderly EXIT
status: implemented

## Mechanism

Signal handlers only record delivery in atomics. `$(signum)` reads the
most recently delivered FLAG or EXIT signal, initially zero. A separate
pending-EXIT atomic records requests for orderly shutdown. Reading the
most-recent signal alone cannot distinguish the two dispositions.

DD128 defines the generated EXIT and ERR handlers. This DD defines signal
registration and checkpoints. `END` is removed, not an alias for either
handler.

## signal builtin

| Form | Behavior |
|------|----------|
| `signal N IGN` | Install SIG_IGN; no signal flag or checkpoint overhead. |
| `signal N DFL` | Restore the kernel default disposition. |
| `signal N FLAG` | Record `signum`; resume without requesting shutdown. |
| `signal N EXIT` | Record `signum` and a pending EXIT request; dispatch at a checkpoint. |

The number is a literal. The compiler does not interpret signal names.
Users may use GPP macros or the optional `signals.h`:

```sash
#define INT 2
#define TERM 15
signal INT EXIT
signal TERM EXIT
```

The converter emits numeric literals. The accepted range is 1–64 except
9 (KILL) and 19 (STOP), which cannot be caught or ignored. The table of
common numbers below is illustrative, not a whitelist.

| Code | Rejects |
|------|---------|
| **SB4059** | Any arity other than exactly two arguments. |
| **SB4060** | A nonliteral signal number, including names and expansions. |
| **SB4061** | A number outside 1–64, or 9 / 19. |
| **SB4062** | A disposition other than `IGN`, `DFL`, `FLAG`, `EXIT`. |

These rules apply uniformly to all four dispositions. Before journal 351
the lowering silently discarded malformed registrations. Runtime rejection
of a validated registration is an invariant failure: report the kernel
refusal and terminate rather than pretend the requested disposition exists.
This is not an ordinary operational failure or an additional DD128 fatal
exception.

## EXIT and ERR

Each optional handler is a top-level declaration, at most once per kind.
SB4054 rejects a nested declaration and the removed END spelling; SB4055
rejects a duplicate. Both bodies share the program's logical globals.

**SB5071 permits function definitions only directly in the program's main
statement list.** Neither handler is a second definition root. Helpers
defined at program top level may be called from either, subject to that
handler's ordinary checks.

```sash
signal 15 EXIT
EXIT {
    int code=$?
    echo "orderly shutdown: $code"
}
ERR {
    int code=$?
    - echo "failure: $code"
}
```

Selection is by cause:

- Fallthrough, explicit zero exit and signal checkpoints select EXIT.
- Uncaught failures, explicit nonzero exits and first fatal faults select ERR.
- An absent handler does not substitute the other one.

A checkpoint supplies `128 + signal` to EXIT; that nonzero status does not
select ERR. Explicit exits use the full i64 before process-boundary
truncation (DD89). The incoming status is available as the handler's first
`$?` use. Subsequent `$?` uses follow DD55's normal handling-position rule.
Handler completion preserves its entry status; an explicit `exit N`
overrides it.

EXIT is checked code. Its failure can enter ERR. ERR must explicitly handle
its own operational failures; neither handler is blanket-unchecked.
The only progression is main → EXIT → termination, main → ERR → termination,
or main → EXIT → ERR → termination. Neither resumes interrupted execution.
An exit inside ERR terminates, including `exit 0`; it never enters EXIT.

### Main process only

Forked stages and background jobs run neither handler. Their own failures
retain diagnostics and statuses; the parent may handle that process status.
A job forked from a handler does not re-enter it. SSA uses
`Terminator::ChildExit`, and the runtime marks forked children independently
of generated code's region metadata.

Historically, END ran in every child and could recursively fork itself
(B9); reader teardown also reached the wrong process (B17). Journal 539
closed those faults. DD128 preserves the ownership rule, with migrated
`h128.56.89`, `.91`–`.93` retaining the process/status assertions.

The former F3 acceptance of definitions inside END was accidental, not a
design exception. DG restored SB5071 in journal 566. Exception-only
56.94–56.100 retired; `h128.fnroot.1` and `handler128.4` now pin placement
in EXIT and ERR. Whole-program executable-body and call traversals remain.

## Checkpoints and adjacent resolution

Only programs declaring an EXIT disposition receive checkpoints:

- Before and after external commands and pipelines, and before replacing exec.
- At function entry/return and loop boundaries, including captured and
  integer calls.
- After parent-reader completion and at semantic acquisition/resolution
  boundaries required by DD128.

Checkpoints read the pending-EXIT atomic. Disposition is recorded **at
delivery**: a FLAG delivery cannot become an exit merely because EXIT is
registered later. Conversely, changing a disposition does not erase a
request already received.

A legal adjacent acquisition/attest pair is protected from signal
**dispatch**, not receipt. Check before acquisition; retain pending signals
while its required resolution is incomplete; resume ordinary dispatch
afterward. The protected region includes a single-use producer's return,
reader conditions and direct replacement in a failed-attest branch.
Explicit exits and ordinary or fatal failures still transfer immediately.
No implicit checkpoint is invented before an explicit failure transfer.
A false reader path that skips attestation retains its actual taint state.

The source adjacency analysis supplies pairs to the shared SSA/CFG planner.
That planner installs explicit checkpoints after lowering's discovery passes,
and marks the directly emitted native/C polls with the same deferral state.
Source diagnostics and the independent SSA backstop use actual signal-entry
states. This is not a signal mask or a bounded-latency guarantee.

Signals during either handler may update the atomics but never dispatch
another handler. FLAG-only programs receive no polling instructions.

## Changing disposition

Last registration wins at that point in execution:

```sash
signal 2 IGN
# critical section
signal 2 EXIT
```

| Signal | Number |
|--------|--------|
| HUP | 1 |
| INT | 2 |
| USR1 | 10 |
| USR2 | 12 |
| PIPE | 13 |
| TERM | 15 |

Runtime registration uses `sash_signal_flag` / `sash_trap`, the kernel
sigaction ABI and its syscall restorer. Both generated backends call the
same atomic-reading checkpoint routine.

## Converter and trap

| Bash | Sash-native |
|------|-------------|
| `trap "" SIG` | `signal N IGN` |
| `trap - SIG` | `signal N DFL` |
| `trap : SIG` | `signal N FLAG` |
| `trap handler SIG` | Manual restructuring; arbitrary asynchronous bodies are unsupported. |
| `trap handler EXIT` | Manually choose checked EXIT or explicitly handled ERR. |

Native `trap` is SB4043, with signal/EXIT guidance. The converter owns
bash-to-native migration; the compiler does not retain a trap alias.
An old unchecked END body requires a deliberate migration, not token
replacement. DD128 and journal 569 record that migration.
