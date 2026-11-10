title: "`$(<file)` failure policy and capture-condition decision"
status: done
priority: P3
kind: decision
depends: []
gates:
  - "DD88 records direct file-read capture-conditions under DD130, without fabricated successful emptiness"
  - "the old word-splitting question is reconciled with DD22/DD99; implementation is separately owned by recoverable-file-failures"

## Decision, 2026-11-10

DG selected **"yes to capture-condition"**. DD88 admits a direct
`$(<file)` RHS to the existing capture-condition contexts, including
branches, loops, negation and logical-list conditions. The read supplies
the operational result; assignment itself does not gain one.

A completed read succeeds even when the file is empty. A handled recoverable
error binds this attempt's bytes, possibly empty or partial, and supplies
failure. It does not retain previous target data or pretend the read
completed. Ordinary unhandled reads remain checked before assignment.
Fatal faults, taint, filename evaluation and body boundaries are unchanged.

The AI recommended this existing surface over either a new whole-file reader
command or retaining the always-checked shorthand and requiring an external
`cat` capture whenever recovery is wanted. The external route remains legal.

This record is **done as a decision**, not as an implementation.
The existing P2 `recoverable-file-failures` owns the file-read extension,
sharing `runtime-failure-classification`; there is no duplicate implementation
task. Journal 610 records the ruling and current evidence.

## What happened to the original questions

The original filing followed the 2026-09-13 first-class slurp implementation.
That implementation preserved the earlier `cat` desugar's unchecked abort.
The filing's abort-versus-Bash-empty choice predates DD130 and is no longer
the choice to make: path-specific errors are recoverable, unhandled failures
still abort, and successful emptiness must not conceal failure. DD88 now
settles the missing handling surface.

The `for w in $(<f)` IFS question is already governed by DD22 and DD99:
**no implicit word splitting**. A scalar is not silently split into words
or lines; explicit conversion supplies separators. This does not broaden
for-header legality or relax input taint. No new loop behavior was measured
or selected with this decision.

The historical change from "tainted command substitution" to "tainted
variable $w" is not a remaining language choice. Apply the ordinary located,
source-named taint diagnostics; no separate wording defect was established.

## Evidence before implementation

Fresh native CLI controls at `52b05642`, with ordinary taint checking:

| form | outcome |
|---|---|
| `if text=$(<missing) { ... }` | compile-time SB4025 at 1:4 |
| ordinary `text=$(<missing)` followed by attest/use | compiles; exits 1 with the path and ENOENT diagnostic, without continuing |
| `if text=$(/bin/cat missing) { ... } else { echo handled }` | prints `handled`, exits 0; cat's own stderr remains |

The gap is the built-in form's handling interface, not an inability to recover
from file reads anywhere in sash. Current source constructs `SlurpFile`,
while the capture-condition classifier accepts only `CommandSubstitution`;
the runtime slurp aborts on a failed open.

Sources, compile/run output, return codes and identities are in the session's
`files/slurp-handling-20261110.gYttda/`. No C behavior measurement, partial-read
fault injection, source change or spec sweep accompanied this recording.
