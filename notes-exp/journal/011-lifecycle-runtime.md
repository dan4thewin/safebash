# Lifecycle runtime and explicit END

DG directed the next slice under [DD#5](../decisions/DD05-planned-end-explicit-handling.md):
END admits no source function definitions, and every fallible END
operation is explicitly caught or suppressed. Nothing outside END changes.
The AI implemented that prototype contract, retaining DD#55's local
function-result discard and DD#56's cooperative signal model.

## Implementation

`sash-cpp/src/lifecycle.cpp` supplies the shared POSIX program boundary.
The original two-argument `run` delegates to the same driver with an empty
END; a three-argument form borrows body/END callbacks. A typed
`ExitRequest` stays distinct from command verdicts and function returns.
`Shutdown` records completion, explicit exit, known body failure, or
signal shutdown, preserving the incoming status and signal identity.

Successful body unwinding restores capture and closes pipeline endpoints
before waiting for children; only then does the owner enter END.
END runs at most once and preserves the triggering status unless it
explicitly exits with another. The driver catches only typed exit requests
around END, not operation failures. Invalid unhandled C++ END exceptions
escape to the host caller. Source rejection remains future front-end
work; no temporary parser or validator was added.

The internal END flag only prevents checkpoint re-entry. It does not
disable command checks or alter outside function bodies. A fixture calls
an outside int function from END, discards its answer, and explicitly
catches the unchanged body failure. Another explicitly handles an
allocation failure within END.

Signal handlers perform only lock-free integer stores. Most-recent
`signum` and pending EXIT are separate, including FLAG after EXIT,
rearming a historical FLAG, and clearing `signum` without clearing EXIT.
Ignore/default/FLAG/EXIT registration uses host POSIX signal APIs; snapshots
are reserved before changing the kernel disposition. Failed installation
does not leave a bogus restoration entry. Scoped handoffs restore incoming
masks and dispositions, even when END escapes.

Checkpoint placement is a reversible prototype implementation choice:
visible calls at command/function/loop boundaries, with entry/completion
checks in `run`, rather than hidden checks inside existing execution,
reader, or capture APIs. The eventual emitter owns generated placement.
Blocking reads/waits still retry EINTR. A signal records a request rather
than killing children or throwing in its handler; owned work can delay the
next checkpoint. No runtime timeout or force-kill was added.

Signal setup failures within END can be explicitly caught as `SetupError`.
Broken restoration or private lifecycle-state handoff retains the
nonallocating fatal cleanup boundary, reusing the existing POSIX reporter.
External pre-exec children still allocate/unwind nothing; the driver
checks owning PID before END, and typed exit in a forked child uses
`_exit`. This is not implementation of future forked-C++ reader disposal.

## Sixth source-shaped pair

`lifecycle_report.cpp` and `.sash` capture a real function's two-worker
collection, discard another function's returned int, and explicitly exit
into END. END catches one failed command, discards another, and traverses
saved state. Source functions are outside END. The C++ handler explicitly
handles setup/allocation/capacity failures too, choosing exit 2.

Both programs match the independent eight-line golden. The native
counterpart uses spellings accepted under current policy and explicit
command handling compatible with DG's planned END contract. It does not
claim the native compiler now enforces that contract.

## Evidence and controls

Predicted and observed: 35 fixture groups, six C++ goldens, six native
goldens, and the unchanged 169-case Linux command/capture catalog.

- GCC and Clang pass `fault-test error-demo`; GCC also passes `fmt`.
- Supported system-GCC/linker ASan passes the same 35 groups and six goldens.
- On `rime`, in the existing isolated `/tmp/sash-cpp-macos.bOMAPJ`,
  real Apple Clang/libc++ and Apple ASan both pass 35 groups and six goldens.
  Only scoped `sash-cpp` files were transferred, never the producer draft.
- DG's supplied real Linux compiler/archive matches all six sash reports.
- A retained Linux trace shows actual SIGUSR1 delivery interrupting
  foreground `wait4` and the handshake-backed pipeline `read`. They
  complete/retry or unwind before ordinary-context END. The pipeline
  handshake keeps the producer active until the consumer acknowledges;
  no sleep is used.
- Final review found and repaired fixture-only acknowledgement ownership:
  its writer must be close-on-exec and close on consumer unwind before the
  runtime waits. An explicit pre-acknowledgement consumer failure now
  delivers EOF to the helper, preserves the consumer error, and reaches
  END with the producer already reaped, rather than depending on timeout.
  The finite helper also polls for closed output, so partial pipeline
  setup releases it even when no consumer takes the acknowledgement writer;
  a failed second exec pins that case.
  Apple ASan exposed that a zero-event poll silently misses closed-pipe
  events on Darwin. Isolated open/closed-pipe controls confirm that read
  interest reports closure and blocks while intact; writable interest
  would busy-wait. The helper now requests read interest. An additional
  ready/read/close fixture keeps the acknowledgement writer open and
  deterministically closes output only after the producer is waiting;
  restoring the old helper reaches the test deadline on the actual Mac,
  while the repaired helper completes with the exact diagnostic and no
  remaining child.
- Six Linux probes inject body/END signal installation, END mask setup,
  fatal disposition restoration, and entry/restoration handoff failures
  with allocation denied. Trace assertions confirm the intended
  syscall; mask failure reaches END's explicit catch before any sigaction.
  All six selectors reject an uninjected run.
- Combined handoff EIO and allocation denial exposed another coupled bug:
  an allocating `SetupError` was constructed before the fatal reporter,
  so both private entry and restoration aborted with status 134 rather
  than reporting status 2. The mask guard now retains a nonallocating
  error code: private handoffs report directly, while signal registration
  still raises a catchable setup exception. Before/after traces pin both
  sites; END runs before the restoration failure, not the entry failure.
- Three isolated implementation mutations fail for the intended reasons:
  reading most-recent FLAG as pending EXIT; automatically handling END's
  command error; and removing the pre-END owner check. Restored-source
  lifecycle controls pass, and checksums prove the worktree runtime and
  protected producer draft were never mutated.

Logs, traces, and isolated mutation sources remain in the session artifacts.
Temporary mutation objects/executables are removed after their checks.
Existing Linux UBSan/Clang-sanitizer host limitations are not claimed fixed.
The repeatable Darwin fault harness remains follow-up tooling.

The broader runtime roadmap stays open for value/argument operations,
general redirects, public wait, background/forked-C++ forms, and their
inherited-reader disposal. No producer, native language implementation,
IR reader, or emitter changed in this slice.
