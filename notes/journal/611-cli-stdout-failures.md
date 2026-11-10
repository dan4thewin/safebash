title: Quiet broken pipes, but not successful truncation
date: 2026-11-10

## The next question

The remaining question records contain old decisions, diagnostic details
and optional extensions. The AI did not revive the map-loop proposal or
reopen the settled wait-selector and complexity-dispatcher rules.
Instead, `bug-stdout-write-failure-panics` supplied an actual queued repair
with an unresolved output contract.

The question concerns the compiler tools, not the programs they compile.
DD55's builtin-output rule and DD130's runtime failure classes do not
settle how a Rust CLI should respond when its own output consumer stops.

## Evidence

At `cfb230bc`, the index still shows `print!`/`println!` in the compiler's
usage, IR/C emission, extraction, manifest and completion-message paths.
`sash-expand` unwraps its output write. The converter instead checks
`write_all`, reports a stdout error and exits 1, including on EPIPE.

Four fresh `sash --help` invocations, with the normal release compiler and
a five-second timeout each:

| destination | result |
|---|---|
| normal stdout | exit 0, usage output, empty stderr |
| owned pipe with its read end already closed | panic, exit 101, EPIPE |
| `/dev/full` | panic, exit 101, ENOSPC |
| stdout closed in the child before exec | exit 0, empty stderr |

Closing the owned read end before invocation removes the scheduling race
in a `... | head` example. The last row establishes the existing discard
outcome; it does not identify which Rust startup/I/O mechanism supplies it.

The old filing also measured emit-C and a completion-message failure after
the binary was built. Those measurements were not rerun here. Nor were the
converter or expansion tool executed for this decision; their current
source is the evidence for their handling.

## DG's choice

The AI recommended quiet EPIPE with a failing result, reported other write
errors, and retaining deliberately closed stdout as a discard. The
alternatives were reporting EPIPE too, as conversion output currently
does, and making closed stdout an error.

DG selected:

> Quiet EPIPE with exit 1; report other write errors with exit 1;
> allow deliberately closed stdout as discard.

Exit 1 is explicit: neither silent success nor default SIGPIPE termination
is the chosen result. A host shell's `pipefail` can still reject the
pipeline. The policy is consistent across `sash`, `sash-expand` and
`sash-convert`, so the converter's EPIPE message must change too.

Closed stdout does not suppress a separate input/compiler error. Quiet
EPIPE does not erase earlier diagnostics. Successful output bytes and
completed artifacts remain; failure to deliver a completion message does
not justify deleting the binary already built. Buffered delivery and the
stdout-error reporting path must not reintroduce success or an I/O panic.

## Recording and ownership

DD133 gives this CLI-only contract its own home rather than extending
DD130's compiled-runtime policy. GUIDE and the findings pointer agree.
The existing P3 bug task retains implementation ownership and now records
the three-tool scope, fresh evidence, deterministic failure controls,
artifact preservation and independent before/reversion obligations.

No new writer abstraction, signal strategy or implementation dependency
was selected. The task stays open; no compiler/runtime/converter/expander/
spec/build source changed, and no full sweep or fresh coverage ran.

Evidence and the compiler identity are retained in the session's
`files/cli-stdout-policy-20261110.json`. No probe source or executable was
created, and all owned pipe descriptors were closed.
