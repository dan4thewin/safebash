title: "sash-expand corrupts UTF-8 literals and panics while stripping UTF-8 values"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "real sash-expand CLI preserves UTF-8 literal and substituted values without byte-to-character corruption"
  - "all four prefix/suffix stripping operations handle UTF-8 values without slicing inside a character"
  - "focused specs fail on the original binary and pass after the repair; retained ASCII behavior is pinned separately from intentional quoting/output migrations"
  - "the predicted ordinary gate passes and the public command manual's UTF-8 bug warning is reconciled"

## Evidence

Found while documenting the companion tools at `c15d14b3`, 2026-11-11.
The actual release CLI, with five-second per-invocation timeouts:

| input | environment | result |
|---|---|---|
| `LABEL=café` | none required | exit 0, `LABEL=cafÃ©` |
| `NAME=${SASH_DOC_UTF8%z}` | `SASH_DOC_UTF8=é` | exit 101, panic: byte index 1 is not a character boundary |

The second pattern does not match; it should not need to alter the value.
Ordinary ASCII assignments and references still work. These are input
processing defects, not an accepted ASCII-only restriction.

At this revision, `sash-expand/src/main.rs:160,163` appends individual
UTF-8 bytes with `as char`. The four stripping helpers at lines 293-327
slice strings at every byte index. Fix both literal/escaped text copying
and valid slicing boundaries; a literal-only repair leaves inherited or
substituted Unicode values able to panic.

The exact inputs, binary identities and output are retained in the session's
`files/companion-manpages-20261111/initial-controls.json` and sibling fixtures.
No repair or new spec is claimed by this filing. End-to-end coverage belongs
in the existing `spec/` harness, driving the real helper command.

## Boundaries

The [Bash-compatible loader rework](sash-expand-bash-assignment-loader.md),
recorded at DG's request on 2026-11-19, owns the broader quote-aware parser
and evaluator change. This defect belongs in that work and stays open until
both literal copying and all four stripping operations are proved fixed.
The original narrow-repair instruction to retain the old quoting and
nonrecursive operand behavior is superseded by that scope.

Preserve raw key/value output and the no-execution boundary; distinguish
retained ASCII behavior from the rework's intentional grammar/output changes.
If fixing the byte-oriented pattern matcher requires selecting what `?`
means for Unicode, state it against the expansion contract rather than
inventing it as a slicing side effect.

The lone-quote crash retains its own input-boundary evidence in
[bug-expand-lone-quote](bug-expand-lone-quote.md).
Stdout failure handling remains with
[bug-stdout-write-failure-panics](bug-stdout-write-failure-panics.md).
