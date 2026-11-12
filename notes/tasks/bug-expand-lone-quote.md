title: "sash-expand panics on a value consisting of one quote"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "real sash-expand CLI handles a lone single or double quote without a Rust panic or silently dropping the input"
  - "empty and nonempty paired quotes retain the documented expansion behavior"
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
characters. Other unmatched-quote input is currently retained as text;
do not silently introduce a broader quote grammar while repairing this
boundary. Preserve expansion within both properly paired quote forms.

Both CLI probes used five-second timeouts. Inputs, binary identities and
exact output are in the session's `files/companion-manpages-20261111/`.
No repair or new spec is claimed by this filing.

Related independent input defect:
[bug-expand-utf8-value-processing](bug-expand-utf8-value-processing.md).
