title: DD#31 — enforce disjoint (( )) contexts
status: ready
depends: dd31-reject-mutation-in-expr
severity: correctness

lower_arithmetic_command handles both statement and condition forms
but does not enforce DD#31's disjointness:

- `(( x > 0 ))` as bare statement → should be compile error
  (currently silently desugars to [[ test ]])
- `if (( x++ ))` as condition → should be compile error
  (currently silently emits assignment)

Need to thread context (statement vs condition) into the lowerer
and emit diagnostics when the wrong kind appears. Assign SB codes.

In bash-mode, coerce:
- `(( x > 0 ))` as statement → `if (( x > 0 )); then :; fi` or
  reject (this is likely dead code in bash too)
- `if (( x++ ))` as condition → `((x++)); if (( x != 0 ))` 
  (extract mutation, test result — but this changes semantics;
  may be better to just reject)
