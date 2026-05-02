title: DD#31 — enforce disjoint (( )) contexts
status: ready
depends: dd31-reject-mutation-in-expr
severity: correctness

Two issues in lower_arithmetic_command:

1. **Disjointness not enforced.** `(( x > 0 ))` as bare statement
   should be a compile error (comparison in mutation context).
   `if (( x++ ))` as condition should be a compile error (mutation
   in comparison context). Currently both are silently accepted.

2. **Wrong type system.** Comparisons in `(( ))` are desugared to
   `[[ $x -gt 0 ]]` — a string test command. This violates DD#30
   (strict type separation). Integer comparisons must stay in
   arithmetic IR and lower to `icmp` in codegen. `[[ ]]` is for
   string operations.

Fix: thread context (statement vs condition) into the lowerer.
In condition context, comparisons produce integer boolean results
directly via arithmetic IR. In statement context, comparisons are
compile errors. In both contexts, the wrong kind is rejected with
an SB diagnostic code.

In bash-mode, coerce or reject (see dd31-reject-mutation-in-expr).
