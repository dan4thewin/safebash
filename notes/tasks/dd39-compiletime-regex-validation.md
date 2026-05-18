title: Compile-time regex validation for literal patterns
status: done
depends: []
gates: [39.5, 39.6, 39.7, 39.8]
  - tests
example: check (sash/src/rules/param_transform.rs)
index_keyword: regex
  - commit

For regex patterns that are pure literals (no `$var` or `${var}`
expansions), call `regcomp9` at compile time in the sash compiler
process to validate the pattern. Reject invalid patterns with a
diagnostic at the call site.

Detection: the lowerer already distinguishes literal vs dynamic regex
patterns (see `lower.rs` ~1358, ~1815). If all parts are
`ExprKind::Literal`, the pattern is compile-time-known.

Requires linking Plan 9 regex into sash itself (it's already in
sash-rt). Factor the C source into a shared build artifact or
add a validation-only path.

Ref: DD#39, DD#25 (designed safety)
