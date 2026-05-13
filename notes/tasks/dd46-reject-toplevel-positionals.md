title: DD#46 — reject top-level positional params in sash-native
status: done
depends: []
gates: [46.1, 46.2, 46.4]
  - tests
example: lower_value (sash/src/transform.rs)
index_keyword: positional
  - commit

`$1`–`$N`, `$@`, `$*` at top level should be a compile error in
sash-native mode with guidance to use `$(argv N)`. Currently they
pass validation and are only caught incidentally by taint (SB4020).

Requires tracking function depth in the validator or taint pass.
Top-level (depth 0) positional references → dedicated diagnostic
(not SB4020 taint, a new rule like SB40xx).

Inside function bodies (depth > 0) positionals are valid and map
to function arguments.

Ref: DD#46
