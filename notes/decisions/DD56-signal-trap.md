title: DD#56 — Signal Model: END block + signal builtin
status: implemented

## Mechanism

All signal handling is flag-based. When a signal arrives, the runtime
atomically writes the signal number to a global int. The program reads
it via `$(signum)` (keyword, like `argv`/`getenv`). Default value is 0
(no signal received).

## signal builtin

`signal` is a sash builtin. Takes a signal **number** (int) and a disposition.

| Form | Behavior |
|------|----------|
| `signal N IGN` | OS masks signal (SIG_IGN). Invisible, zero cost. |
| `signal N DFL` | Restore kernel default disposition. |
| `signal N FLAG` | Set `signum`, program resumes. Programmer polls. Zero overhead. |
| `signal N EXIT` | Flag + compiler inserts check points. At next check, jump to END. |

The compiler does not know signal names. Users may use GPP macros for
readability:

```bash
#define INT 2
#define TERM 15
signal INT EXIT
signal TERM EXIT
```

Sash ships an optional `signals.h` with standard POSIX mappings.
The converter emits numeric literals directly.

## END block

`END` is a top-level declaration (exactly one per program). It executes
immediately before process exit — from any cause (normal exit, `exit`
builtin, or signal-triggered shutdown).

```bash
END {
    case $(signum) in
        15) echo "terminated"; rm "$lockfile"; exit 143 ;;
        2)  echo "interrupted"; exit 130 ;;
        0)  cleanup; exit 0 ;;
    esac
}
```

The END block does NOT return to the program. Reaching the end of the
block, `return`, or `exit N` all terminate the process.

- `exit N` within END overrides the process exit status.
- Without explicit `exit N`, the process exits with the status that
  triggered END.
- **`$?` inside END is that triggering status**, available as the block's
  FIRST action (an abort's failing code, an `exit N`, a signal's `128+n`, or
  the last command's status on normal exit) — the same handling-position rule
  as an `if`/`while` branch. `$?` after a command in END reads that command's
  status, as usual. (Validator: the `__sash_end__` function starts its `$?`
  state machine `Available`; other functions start fresh — no leak.)

## signum keyword

`$(signum)` returns the most recent signal number as an int.

- Before any signal: 0
- After `signal SIG FLAG` fires: signal number, stays set until
  cleared with `(( signum = 0 ))`
- In END block: signal that triggered exit (or 0 for normal exit)

## Check point insertion

The compiler inserts signal flag checks only when at least one
`signal SIG EXIT` is declared. Check points:

- Before external commands
- Before function calls
- Before function returns
- At loop tops (while/for/until)

At each check point: load `signum`, if nonzero and signal is in the
exiting set, jump to END block.

FLAG traps add zero runtime overhead — no checks inserted.

## Changing signal disposition

A signal's disposition can be changed at any point:

```bash
# Critical section: ignore SIGINT
signal INT IGN

# ... work ...

# Now make it trigger shutdown
signal INT EXIT
```

Last declaration wins at that point in execution.

## Supported signals

| Signal | Number |
|--------|--------|
| HUP | 1 |
| INT | 2 |
| USR1 | 10 |
| USR2 | 12 |
| PIPE | 13 |
| TERM | 15 |

## Runtime implementation

Signal handler (Rust, async-signal-safe):
```rust
static SASH_SIGNUM: AtomicI64 = AtomicI64::new(0);

extern "C" fn sash_signal_handler(sig: c_int) {
    SASH_SIGNUM.store(sig as i64, Ordering::Relaxed);
}
```

Registration via `sigaction(2)`. Flag check in generated code:
load `SASH_SIGNUM`, compare against exiting set, branch to END.

## Interaction with DD#55

END block follows DD#55 error model. External commands inside END
get implicit exit checks. A failed command aborts END and calls
`sash_exit` directly. Use `- cmd` to suppress failures during cleanup.

## Converter mappings

| Bash | Sash-native |
|------|-------------|
| `trap "" SIG` | `signal N IGN` |
| `trap - SIG` | `signal N DFL` |
| `trap : SIG` | `signal N FLAG` |
| `trap handler SIG` | MANUAL (arbitrary handler bodies not supported) |
| `trap handler EXIT` | MANUAL: restructure as `END { ... }` |

## `trap` is not a native form (amended 2026-08-28, DG)

`trap` is **rejected** in sash source (**SB4043**), pointing at the native
`signal` builtin + `END { }`. `trap` is bash the converter rewrites (table
above) — it is not a sash spelling. Originally the compiler quietly accepted a
hand-written `trap` and transformed it to `signal`/`END` internally
(`handle_trap`); that in-compiler alias is removed (the converter, not the
compiler, owns the bash→native migration — same policy as `command`, DD#23).
The shared `signal`-lowering machinery (`TrapSignal`/`SignalFlag` SSA sigils,
the `exit_trap`/END path) is unchanged; only the `trap`-specific front-end
(`BuiltinCmd::Trap`, `handle_trap`) was deleted.
