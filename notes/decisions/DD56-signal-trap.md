title: DD#56 — Signal and Trap Model
status: designed

## Mechanism

All signal traps are flag-setting. When a signal arrives, the runtime
atomically writes the signal number to a global int. The program reads
it via `$(signum)` (keyword, like `argv`/`getenv`). Default value is 0
(no signal received).

Two trap forms:

| Form | Behavior |
|------|----------|
| `trap : SIG` | Flag only. Sets `signum`, program resumes. Programmer polls. |
| `trap exit SIG` | Flag + compiler inserts checks. When check fires, jumps to EXIT block. |

## EXIT block

The EXIT block is the universal cleanup path. It handles normal exit
(signal 0) and all exiting traps. The programmer dispatches explicitly:

```bash
trap exit SIGTERM
trap exit SIGINT

trap EXIT {
    case $(signum) in
        15) echo "terminated"; rm "$lockfile"; exit 143 ;;
        2)  echo "interrupted"; exit 130 ;;
        0)  cleanup; exit 0 ;;
    esac
}
```

Every code path in the EXIT block must end with `exit`. The EXIT
block does not return.

## signum keyword

`$(signum)` returns the signal number as an int. Compiler-managed —
backed by a runtime global that the C signal handler writes to.

- Before any signal: 0
- After `trap : SIG` fires: signal number, stays set until
  programmer clears with `(( signum = 0 ))`
- In EXIT block: signal number that triggered exit (or 0 for
  normal exit)

## Check insertion

The compiler inserts signal flag checks only when at least one
`trap exit SIG` is declared. Check points:

- Before external commands
- Before function calls
- Before function returns
- At loop tops (while/for/until)

At each check point: load `signum`, if nonzero and signal is in the
exiting set, jump to EXIT block. If the exiting set changes during
the program (a signal goes from `:` to `exit` or vice versa), checks
adjust from that point forward.

Flag-only traps (`trap :`) add zero overhead — no checks inserted.

## Changing traps

A signal's trap can be changed at any point in the program:

```bash
# Critical section: ignore SIGINT
trap : SIGINT

# ... work ...

# Now make it fatal
trap exit SIGINT
```

Last declaration wins at that point in execution.

## Supported signals

Curated set (same as current implementation):

| Signal | Number |
|--------|--------|
| SIGHUP | 1 |
| SIGINT | 2 |
| SIGUSR1 | 10 |
| SIGUSR2 | 12 |
| SIGPIPE | 13 |
| SIGTERM | 15 |

Accept both forms: `SIGTERM` and `TERM`.

`trap '' SIG` (bash ignore) coerced to `trap : SIG`.
`trap - SIG` (bash reset) supported — removes trap, restores default.

## Runtime implementation

Signal handler (C, async-signal-safe):
```c
volatile sig_atomic_t sash_signum = 0;

void sash_signal_handler(int sig) {
    sash_signum = sig;
}
```

Registration via `sigaction(2)`. Flag check in generated code:
load `sash_signum`, compare against exiting set, branch.

## Interaction with DD#55

EXIT block follows DD#55 error model. External commands inside the
EXIT block get implicit exit checks. However, since the EXIT block
must exit, a failed command aborts the exit handler — which calls
`sash_exit` directly. The programmer can use `- cmd` to suppress
failures during cleanup.

## EXIT trap bug (current)

Current implementation inlines EXIT trap before every Return
terminator (wrong — fires on function returns) and omits it before
`exit` builtin. See bug-exit-trap-wrong-scope task.

This DD supersedes the current implementation. The EXIT block is a
single block, reached from:
- Program's final exit path (signal 0)
- `exit` builtin
- Exiting trap check points
