---
title: "Reject literal NUL in regex patterns; consider banning raw source NUL"
status: pending
priority: P2
kind: task
depends: []
gates:
  - "NUL-valued regex patterns diagnose explicitly rather than compiling a prefix"
  - "the scope of a source-wide raw-NUL ban is settled with DG"
  - "escaped NUL string values remain supported"
  - "focused real-CLI specs pin static and dynamic pattern rejection in both backends"
---

## Requested

DG requested this native backlog item during the C++ regex unit:

> add a backlog task for native to disallow a literal NUL in a pattern
> (and probably everywhere, since $'\x00' is availalbe)

Do not implement native behavior changes as part of the C++ runtime unit.

## Pattern boundary

The Plan 9 `regcomp9` interface is C-string based. The native wrappers
copy the counted pattern and append a terminator, but an interior NUL can
still make the engine compile only a prefix. Diagnose a NUL byte in an
evaluated regex pattern instead of allowing that silent interpretation.
Cover literal/static patterns and dynamic/computed patterns at their
respective validation/runtime boundaries.

A regex escape such as ordinary quoted `'\x00'` carries backslash/x/digits
to the engine; it is not the same as a literal zero byte in the pattern.
Keep those two inputs distinct.

## Broader source question

DG also proposed rejecting raw NUL bytes throughout source text. Settle
that scope before implementing a blanket source rejection.
Distinguish a raw zero byte in the source file from `$'\x00'` or `$'\0'`,
which explicitly construct NUL-bearing **string values**. The latter
remain useful for counted strings and DD#99 records; do not ban produced
NUL values everywhere.

This is separate from DD#63's deliberately retained native **subject**
boundary at the first interior NUL. Do not silently reopen or change that
behavior while implementing pattern/source admission.

## Evidence and completion

The counted native wrappers are `sash_regex_match`, `sash_regex_match_str`,
`sash_regex_match_into`, `sash_regex_matchall_into`, and `sash_regex_replace`
in `sash-rt/src/lib.rs`; inspect the current literal-validation path too.
Add focused specs for the exact diagnostic, including bytes after the NUL,
an empty prefix, and computed patterns. Keep escaped NUL string-value
controls, and use an independent expected failure rather than another
backend as the only oracle.
