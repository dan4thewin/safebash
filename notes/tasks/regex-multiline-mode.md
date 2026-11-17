title: "Decide explicit multiline regex syntax"
status: open
priority: P3
kind: decision
depends: [bug-regex-line-anchors]
gates:
  - "DG selects the opt-in spelling and scope before implementation"
  - "whole-subject anchors remain the default across the regex family"
  - "any selected mode keeps original-subject search cursors and the first-NUL boundary"
  - "newline-anchor behavior is distinguished from dot or negated-class newline matching"

## Deferred choice

DG suggested a flag selecting newline-bounded anchors, with whole-subject
anchors as the default. When authorizing the Plan 9 repair on 2026-11-16,
he selected fixing the defaults now and deferring multiline syntax.

The default repair is complete; no inline modifier, substitution flag or
builtin option was selected or added. Decide the public spelling and its
reach across tests, attests, dispatch, match forms and substitution before
implementing it. Do not silently give different regex forms different
default anchor semantics.

This is not an engine-migration, Unicode, dot-all or length-aware/NUL-matching
task. Literal validation and runtime matching must continue to agree on
the selected dialect. DD132's provisional validity-proof interface and
literal-regex caching retain their separate owners.

See [DD39](../decisions/DD39-glob-regex.md),
[DD63](../decisions/DD63-regex-quoting-interpolation.md) and
[journal 642](../journal/642-regex-subject-boundaries.md).
