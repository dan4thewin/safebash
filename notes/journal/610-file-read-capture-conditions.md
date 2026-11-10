title: Give the existing file read a capture-condition
date: 2026-11-10

## A pertinent question

DG challenged the AI's choice of the optional two-binder map loop as the
next question: was the existing approach flawed or insufficient? No such
case had been established. Keys plus indexed lookup already supplies the
operation. The proposal remained parked; its open decision label was not
by itself a reason to reopen it.

For the next pertinent question, the AI followed the already-accepted
file-failure model. DD130 classifies ordinary path-specific errors as
recoverable, but the existing first-class `$(<file)` has no handling form.
`recoverable-file-failures` explicitly requires a usable interface where
the current API lacks one.

The old `dollar-lt-file-policy` filing asked whether to retain abort or
imitate Bash's empty-value behavior. DD130 has superseded that framing:
unchecked failure aborts, and failure must not look like successful
emptiness. The remaining choice was its handling surface.

## Evidence and choice

At `52b05642`, three native CLI controls with normal taint checking gave:

| form | result |
|---|---|
| direct file-read capture-condition | SB4025 at 1:4 |
| ordinary missing-file slurp, followed by attest/use | compilation succeeds; execution exits 1 with the path/ENOENT diagnostic |
| external `cat` capture-condition for the same missing path | failure branch prints `handled`, exit 0; cat's stderr remains |

Thus the language already has a recovery route through an external
command. The gap belongs to the built-in file-read form, not to all file
handling. Source inspection agrees: the parser constructs `SlurpFile`,
the capture-condition classifier accepts `CommandSubstitution`, and the
runtime slurp aborts on a failed open.

The AI recommended admitting the direct read to existing capture-conditions.
Alternatives were deliberately retaining the always-checked shorthand and
using `cat` for recovery, or adding a whole-file reader command with a
target and status. DG chose:

> yes to capture-condition

## The accepted contract

`if text=$(<file) { ... }` consumes the read's operational status and binds
its text. The existing loop, negation and logical-list consumers follow
the same rule. A completed read succeeds even when empty; content validation
remains separate.

A handled recoverable failure binds this attempt's bytes, possibly empty
or partial, rather than older target data. The proposal explicitly included
that failure-data rule. This is not a successful empty fallback or an atomic
array fill. A scratch target, when that separate trial exists, receives its
new instance and cannot recover the preceding instance or its regex proof.

An ordinary unhandled slurp remains checked before assignment. Fatal
faults remain fatal; the handler does not reach into filename evaluation,
operand captures or called bodies. Taint, attest locality, scope, result
kinds and byte/newline behavior stay. Assignment gains no result of its
own, and there is no new suppression spelling or reader command.

## Ownership and boundary

DD88 states the extension, with pointers in DD55, DD130 and GUIDE.
The existing P2 `recoverable-file-failures` owns implementation and its
native/C, ERR and independent failure/publication proofs. The earlier
policy filing closes as a decision rather than becoming a duplicate owner.
Its IFS question was already answered by DD22/DD99; its historical wording
change was not a remaining language decision. Related task pointers now agree.

Sources, outputs, return codes and compiler/runtime identities are retained
in the session's `files/slurp-handling-20261110.gYttda/`; the two generated
executables were removed. The controls did not measure C or mid-read failure.
No compiler/runtime/converter/spec/build source changed, no full sweep or
fresh coverage measurement ran, and implementation remains queued.
