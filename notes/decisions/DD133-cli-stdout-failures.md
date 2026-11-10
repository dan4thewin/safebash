title: DD#133 - Compiler-tool stdout failures
status: accepted

DG selected this policy on 2026-11-10 (journal 611):

> Quiet EPIPE with exit 1; report other write errors with exit 1;
> allow deliberately closed stdout as discard.

Implementation remains queued in `bug-stdout-write-failure-panics`.

## Scope

This is the stdout contract of **`sash`, `sash-expand` and `sash-convert`**.
It covers their actual CLI output, including usage, emitted C/IR, extracted
source, manifests, conversion/expansion output and the native compiler's
completion message. A completion message is not exempt merely because the
binary has already been built.

This is not a change to compiled programs' `echo`/`write`, pipeline results,
SIGPIPE handling, DD55 suppression or DD130's runtime failure classes.
It does not cover the separate C++ experiment or change artifact-file
writing and linking rules.

## The rule

| stdout outcome | CLI behavior |
|---|---|
| Output delivered normally | Preserve the command's ordinary result |
| Broken pipe (`EPIPE`) | Stop output, exit **1**, no diagnostic for that failure |
| Other write failure | Report a concise stdout error to stderr and exit **1** |
| Deliberately inherited closed stdout | Accept it as explicit output discard; do not fail solely because stdout is closed |

Failed buffered flushes follow the same error rule. Accepting bytes into
a buffer is not proof of successful delivery. An ordinary output error
must not become a Rust panic, be ignored, or produce a success-shaped
partial result.

**Quiet is not successful.** If a downstream reader such as `head` stops
early and a write fails with EPIPE, the tool emits no diagnostic for that
failure but still exits 1. A host shell using `pipefail` can therefore
report the pipeline as failed. Default SIGPIPE termination, commonly
exposed by shells as 141, is not the selected result.

The quiet rule suppresses only the diagnostic for EPIPE itself, not
earlier independent diagnostics or a child's stderr. For other write
errors, preserve the actual cause rather than replacing it with a generic
successful default. If stderr cannot deliver the error report, the stdout
failure still exits 1; reporting it must not introduce another I/O panic.

**Closed stdout is an explicit discard**, like directing output to
`/dev/null`. The allowance does not suppress input, conversion or
compilation errors, nor does it mean every bad-descriptor error on an
arbitrary file may be ignored.

## Completed effects

Do not roll back bytes already emitted or remove an already-built binary
because its completion message could not be delivered. A failed completion
message follows the same exit/diagnostic rule as other stdout output; the
artifact remains. Ordinary success with deliberately closed stdout also
retains the artifact.

## Implementation boundary

The existing bug task owns the three tools' output paths, buffered failure
handling and before/reversion evidence. It must preserve successful output
bytes and normal input/compiler diagnostics while removing stdout-I/O
panics. This does not authorize swallowing unrelated compiler panics.

The policy deliberately changes `sash-convert`'s current converted-output
EPIPE diagnostic to quiet exit 1; the converter is not merely an unchanged
reference implementation. Its reported handling of other write errors
remains the precedent.

No common writer ABI, buffering strategy or signal implementation is
selected here. Record the chosen policy with the handling when implemented.
