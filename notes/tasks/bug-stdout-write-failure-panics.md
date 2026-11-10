title: "Handle CLI stdout failures consistently, without panics"
status: open
priority: P3
kind: bug
depends: []
gates:
  - "DD133 holds across stdout paths in sash, sash-expand and sash-convert: EPIPE is quiet exit 1; other write/flush failures are reported exit 1, never a Rust panic"
  - "deliberately inherited closed stdout remains an explicit discard, without masking other command errors"
  - "successful output bytes and completed artifacts are preserved; a failed completion message does not delete the built binary"
  - "focused CLI specs and independent reversions distinguish failure handling from successful output, including --emit-c, the completion message and both helper tools"
  - "the predicted ordinary gate passes, and the implemented handling records DD133"

## Accepted policy, 2026-11-10

DG selected:

> Quiet EPIPE with exit 1; report other write errors with exit 1;
> allow deliberately closed stdout as discard.

DD133 is the canonical contract; journal 611 records the choice. This task
retains its P3/open bug status and owns implementation, not another policy
question or a duplicate implementation task.

Apply it consistently to `sash`, `sash-expand` and `sash-convert`. Quiet
EPIPE changes the converter's existing diagnostic too. Exit 1 is deliberate,
not default SIGPIPE termination/141 and not successful truncation.
Other stdout write and buffered-flush failures report the actual cause to
stderr and exit 1. An error-reporting failure must not turn this into
success or another I/O panic. Do not silence unrelated diagnostics.

Keep deliberately inherited closed stdout as a discard, like `/dev/null`;
that does not mask an input/compiler error or excuse bad descriptors on
arbitrary files. Preserve output bytes on successful paths, and retain an
already-built artifact when its completion message fails.

This is CLI output, not the compiled runtime's I/O or suppression model.
DD55/DD130 and the separate object-file/linking repair keep their scope.
No writer ABI, buffering or signal strategy has been selected.

## Current evidence

Source inspection at `cfb230bc` still finds panicking stdout writes:

- `sash/src/lib.rs:556`: usage (`check_flags`);
- `:787`: `--emit-ir` (`maybe_emit_alt_output`);
- `:816`: `--emit-c` (`emit_c_to_stdout`);
- `:845`: `--extract` (`dump_sash_source`);
- `:1002`: `--manifest` (`emit_manifest`);
- `:1086`: the completion message (`compile_native`);
- `sash-expand/src/main.rs:101`: `writeln!(out, ...).unwrap()`.

The converter's converted-output path
(`sash-convert/src/main.rs:70-75`) checks `write_all`, reports
`sash-convert: stdout: <error>` and exits 1, including on EPIPE.
It supplies the non-panicking error-reporting precedent, but still needs
the selected quiet-EPIPE change.

Fresh `sash --help` controls at the same commit, with the normal release
compiler and a five-second timeout per invocation:

| destination | outcome |
|---|---|
| normal stdout | exit 0, usage text, no stderr |
| owned pipe whose read end was closed before invocation | Rust panic, exit 101, EPIPE |
| `/dev/full` | Rust panic, exit 101, ENOSPC |
| stdout closed in the child before exec | exit 0, no stderr |

The pipe case is deterministic and does not rely on `head` winning a race.
The closed-stdout case proves the outcome, not the precise Rust mechanism.
JSON with results, argv, HEAD and the compiler hash is in the session's
`files/cli-stdout-policy-20261110.json`. No fresh emit-C, completion-message,
converter or expansion-tool execution accompanied these controls.

## Historical filing

Found while researching B55 (`bug-native-object-temp-file`); not on the
batch-531 branch. Measured 2026-10-26 on HEAD `b62c8a7`:

- `(sleep 0.5; exec sash --emit-c m.sh) | true`: the panic
  `failed printing to stdout: Broken pipe`, rc 101.
  - Rust ignores SIGPIPE, so a write after the reader has gone fails with
    EPIPE instead of killing sash.
  - Without the delay a small program's C fits in the pipe buffer and
    the write succeeds first, so `… | head -c1` usually exits 0.
- The same with `sash m.sh -o v`: `v` is built, then rc 101.
- `--emit-c`, and the success line, to `/dev/full`: the same panic (ENOSPC),
  rc 101.
- `--emit-c >&-`: rc 0, no output and no diagnostic.

## Implementation and proof

- Inventory actual stdout writes in all three CLIs, including non-payload
  paths such as help and the completion message. Do not confuse generated
  C/runtime output with the compiler's own writes.
- Handle write and applicable buffered-flush results. A successful buffered
  enqueue or ignored destructor-flush error must not conceal failed delivery.
  Remove the I/O panic at its source, not by swallowing compiler panics.
- Exercise the CLI process itself. A generated program's output error is
  not evidence for this repair. Use an owned pipe with no readers for
  deterministic EPIPE and `/dev/full` or an independently traced fault for
  other write errors; do not rely on sleeps or a short `head` read.
- Pin exit 1 and no stdout-failure diagnostic for EPIPE, and a concise,
  cause-specific stdout diagnostic with exit 1 for other failures. Neither
  path may print a Rust panic/backtrace. Include the reporting-failure path.
- Cover emit-C and the native completion message, plus conversion and
  expansion output. Verify the built binary still exists and works after a
  failed completion message. Preserve successful bytes, explicit discard
  through closed stdout or `/dev/null`, and independent input/compile errors.
- Keep original-binary controls and independent reversions for the changed
  failure paths, including converter EPIPE silence. A test seeing exit 1
  alone cannot prove silence or correct error classification. Existing
  passing discard controls are preservations, not new fixes.

Choose fresh spec ids from history and predict the full gate after the
fixture inventory is known. Policy recording supplied neither an
implementation nor a gate count.
