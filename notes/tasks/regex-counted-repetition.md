title: Regex {n,m} quantifiers — BLOCKED
status: done
depends: []
gates:
  - tests
  - commit

Plan 9 regex engine (`regcomp9`/`regexec9` in `sash-rt/csrc/`) does
not support counted repetition (`{3}`, `{2,4}`). Tests with
`[[ "abc123" =~ ^[a-z]{3}[0-9]{3}$ ]]` fail silently (no match).

Options:
1. Patch Plan 9 C source to add `{n,m}` support
2. Switch to Rust regex crate (constraint: `#![no_std]`)
3. Accept limitation and document it

Needs DG's decision.
