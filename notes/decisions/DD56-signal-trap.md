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

### Shape enforcement (added 2026-09-17, journal 351)

None of the above was enforced until journal 351 — `signal` had no entry in
`validate.rs` or `type_check.rs` at all, and `handle_signal` carried four
silent drops: it gated on `args.len() >= 2`, parsed the number with
`unwrap_or(0)`, tested `signum > 0`, and matched the disposition with a bare
`_ => {}`. Anything outside the table compiled clean and did nothing.
`signal TERM IGN` — the bash spelling, and the one **SB4043**'s own message
all but invites — was a no-op, as were `signal 2`, `signal 2 BOGUS`,
`signal 2 IGN extra` and `signal $n EXIT`.

Four rules now hold the shape:

| Code | Rejects |
|------|---------|
| **SB4059** | any arity but exactly two arguments |
| **SB4060** | a number that is not an integer literal (a name, or an expansion) |
| **SB4061** | a number outside 1-64, or 9 (KILL) / 19 (STOP) |
| **SB4062** | a disposition outside `IGN`/`DFL`/`FLAG`/`EXIT` |

Two of those thresholds are decisions rather than deductions (DG, 2026-09-17):

- **The "Supported signals" table above is illustrative, not a whitelist.**
  The accepted range is 1-64 less the two the kernel will not let a process
  catch or ignore. `signal 9 IGN` was previously emitted as a syscall the
  compiler already knew must fail.
- **A non-literal number is rejected for all four dispositions**, not just
  `EXIT`. `EXIT` has no choice — `exiting_signals` is a compile-time set, so
  the check-point insertion cannot see a runtime number — and allowing
  expansions for the other three would split the surface for no gain.

The runtime half matches: `rt_sigaction` returning `-1` used to be discarded
by the caller, leaving a program running without a disposition it believed it
had. `sash_trap` and `sash_signal_flag` now abort on it. After the four rules
above that failure should be unreachable, which is exactly why it is worth
reporting rather than swallowing.

## END block

`END` is a top-level declaration (at most one per program). It executes
immediately before process exit — from any cause (normal exit, `exit`
builtin, or signal-triggered shutdown).

Both halves of that sentence are enforced as of journal 309 — neither was
before. `END { }` parses into `ir::Program::end_block`; **SB4054** rejects one
anywhere but the top level, and **SB4055** rejects a second (previously the
second silently won and the first never ran). The body shares the program's
**global variable scope**, but is not another program-level statement list:
SB5071 rejects function definitions inside it. Helpers must be defined at
program top level and may be called from END. Before journal 309, the block
was lowered as a function literally named `__sash_end__`, which left
`:end-block nil` in the DD#104 capture, leaked that name into user diagnostics,
and let a user function of the same name compile and then die at runtime.

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
  status, as usual. (The validator seeds the cleanup body's `$?` state
  machine as `Available`; ordinary functions start fresh — no leak.)

### END runs only in the main process (added 2026-10-28, batch-531 F2)

"Immediately before process exit" means the **main** process. A forked
child (a background job such as `cmd &` or `f &`, or a forked pipeline stage
such as a function stage) leaves with its own status and never runs END. A
job forked inside END does not re-enter END. END runs once, in the process
that owns the program's exit status.

Before this, the transform turned every non-function return into a jump to
END, a background child's last block and a function stage's return block
included. So every forked child ran END (B9). When END itself forked a job,
the job re-entered END, recursively: a fork bomb. A pipe reader's finish on
that path also panicked in a background child (B17). Now the SSA ends a
child with `Terminator::ChildExit`, which exits with the child's `$?`. Both
backends exclude a child's blocks (the child region) from END's abort
routing. A checked failure inside a child still aborts the child with its
diagnostic and status. The parent sees that status as the job's (`wait`,
`waitstatus`) or as the stage's, and a final stage's failure names the
function (56.92), not an internal label.

Specs 56.89 and 56.91-56.93 (signals.sh); findings B9, B10 and B17 in
`notes/tasks/batch-531-findings.md`. Open, and backlogged as row e1 of
`notes/tasks/bug-fork-failure-reporting-by-form.md`: a background job whose
fork fails inside END is silent (no diagnostic, END continues), while a
command that fails to start there is reported and END continues.

### Function definitions belong only at program top level

DD86's SB5071 permits `fn` only as a direct element of the program's
top-level statement list. A cleanup body is not an exception. Sharing
global variables does not grant another place to define global functions.
DG confirmed this rule on 2026-11-04 and directed that DD128's EXIT and ERR
apply it uniformly.

The former F3 section here incorrectly described END's acceptance as
intentional. The validator had exempted the direct statements in both
bodies, and F3 completed downstream support for that accidental exception.
Journal 566 restores the stated rule: `fnroot.1` pins SB5071 in END;
56.94-56.100, whose sole purpose was that exception, are retired.

`Program::function_defs` remains the shared inventory, now of program-level
definitions only. Handler calls, global bindings and executable statements
still need their complete traversals. The B5 typing gap is closed
(journal 565); `typeflow.25`-`.28`, `.23` and `.24` retain cleanup call and
signature checks with helpers defined at program top level.

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

At each check point: load the pending-EXIT signal, and if nonzero enter END
with status `128+signal` (or exit directly if there is no END).

FLAG traps add zero runtime overhead — no checks inserted.

### Checkpoint implementation corrected (2026-10-17, journal 518)

The check reads a separate pending-EXIT atomic, not `signum` against a
compile-time set. FLAG updates only `signum`; EXIT updates both. This records
the disposition **at delivery**: a FLAG signal cannot trigger shutdown merely
because the program also declares EXIT, and a historical FLAG does not become
an exit when that signal is later armed EXIT.

Both backends check before and after external commands and pipelines, before
a replacing `exec`, and at the loop/function boundaries above. This includes
captured functions, integer functions and forked function stages. A pending
EXIT sets status `128+signal` and enters END, or exits directly when there is
no END. Checks inside the canonical END region are skipped: numeric block
ordering is not a reliable test of region membership.

Loop lowering discards its first-pass discovery instructions. The transform
therefore records checkpoint block IDs, then installs `SignalCheckpoint`
instructions after all lowering and END construction. FLAG-only programs
receive none.

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

Two async-signal-safe Rust handlers store into `AtomicI64` globals
`sash_signum` and `sash_exit_signum` as described above.
`sash_signal_flag(signum, exiting)` selects FLAG (`0`) or EXIT (`1`) and
delegates to the shared `sash_trap` sigaction registration and naked syscall
restorer. Native code uses atomic loads; emitted C uses volatile loads.

## Interaction with DD#55

Cleanup commands run unchecked: a failed cleanup step must not skip the
rest of END or re-enter it. This is DG's established "diagnose, restore,
keep cleaning up" policy (journal 367; specs 56.80–56.87), not the earlier
abort-on-cleanup-failure description. Redirect setup failure diagnoses,
restores its scope, and continues; ordinary command failure also continues.
External value captures follow the same policy through their pipeline
instructions. Explicit `exit N` still ends cleanup and overrides the status.

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
