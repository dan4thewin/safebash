title: DD#30 — reject numeric comparisons in [[ ]], coerce to (( ))
status: ready
depends: []
gates:
  - tests
  - commit

`[[ ]]` is string/file/boolean only. `(( ))` is arithmetic only.
No operator set overlap (journal 042, DD#30).

Currently `-eq`, `-ne`, `-lt`, `-le`, `-gt`, `-ge` are accepted in
`[[ ]]` by both the lowerer and codegen. They should be rejected in
sash-native and coerced in bash-mode.

### Sash-native
Validator rule: reject `-eq`/`-ne`/`-lt`/`-le`/`-gt`/`-ge` inside
`[[ ]]`. Suggest `(( ))` equivalent in diagnostic.

### Bash-mode coercion
`[[ $x -gt 5 ]]` → `(( x > 5 ))`
`[[ $x -eq $y ]]` → `(( x == y ))`

Applies to if/while/until conditions. The coercer rewrites before
lowering so no numeric comparison ops reach `[[ ]]` in the IR.

### Cleanup
Remove `-eq`/`-ne`/`-lt`/`-le`/`-gt`/`-ge` cases from the `[[ ]]`
path in codegen.rs (lines ~2630-2641). These should only appear in
`(( ))` contexts after this change.

Ref: DD#30, journal 042
