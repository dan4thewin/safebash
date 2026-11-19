title: "sash-expand panics on a value consisting of one quote"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "real sash-expand CLI rejects lone and other unterminated quotes as input errors, without a Rust panic or silently dropping the input"
  - "empty and nonempty paired quotes follow Bash contexts: single quotes are literal, double quotes permit supported expansion"
  - "focused before/after specs and the predicted ordinary gate pass; the public command manual's lone-quote warning is reconciled"

## Evidence

Found while documenting the companion tools at `c15d14b3`, 2026-11-11.
A file containing either `VALUE='` or `VALUE="` makes the release
`sash-expand` exit 101 at `sash-expand/src/main.rs:86`:

```text
byte range starts at 1 but ends at 0
```

The outer-quote test at lines 83-85 accepts one character as both the opening
and closing delimiter, then slices `[1..len-1]`. A pair requires two
characters. At this revision, other unmatched-quote input was retained as
text and both properly paired quote forms expanded variables. Those are
historical behaviors, not constraints on the rework below.

Both CLI probes used five-second timeouts. Inputs, binary identities and
exact output are in the session's `files/companion-manpages-20261111/`.
No repair or new spec is claimed by this filing.

## Delivery scope

The [Bash-compatible loader rework](sash-expand-bash-assignment-loader.md),
recorded at DG's request on 2026-11-19, replaces the outer-quote shortcut
with stock tree-sitter-bash syntax and a restricted evaluator. It supersedes
this filing's original instruction to retain expansion within single quotes
and avoid broader quote parsing. Unterminated quotes must be input errors;
properly paired single quotes must preserve their literal contents.

Keep this bug open until its actual CLI regressions are proved fixed.
The parser's recovered tree is not successful validation: both error nodes
and inserted missing tokens must be refused.

Related independent input defect:
[bug-expand-utf8-value-processing](bug-expand-utf8-value-processing.md).
