title: DD#31 — enforce disjoint (( )) contexts
status: ready
depends: dd31-reject-mutation-in-expr
severity: correctness
gates: [31.1a, 31.1b, 31.1c, 31.2, 31.3a, 31.3b, 31.3c, 31.4a, 31.4b, 31.4c, 31.7, 31.10, 31.11, 31.13, 31.14]

Issues in lower_arithmetic_command:

1. **Disjointness not enforced.** `(( x > 0 ))` as bare statement
   should be a compile error (comparison in mutation context).
   `if (( x++ ))` as condition should be a compile error (mutation
   in comparison context). Currently both are silently accepted.

2. **Wrong type system.** Comparisons in `(( ))` are desugared to
   `[[ $x -gt 0 ]]` — a string test command. This violates DD#30
   (strict type separation). Integer comparisons must stay in
   arithmetic IR and lower to `icmp` in codegen. `[[ ]]` is for
   string operations.

3. **Prefix ++/-- rejection.** `(( ++x ))` and `(( --x ))` should be
   compile errors. Only postfix forms allowed (Go rule). Currently
   lower.rs handles both prefix and postfix identically (~line 2219).

4. **Postfix ++/-- as expression rejection.** `(( x = y++ ))` should
   be a compile error. `++`/`--` are statement-only, never produce a
   value. Currently the lowerer would accept this.

5. **Comma operator desugaring.** `(( a, b ))` should desugar to
   `(( a )); (( b ))`. In condition position (`if`/`while`), non-final
   expressions must be mutations, final must be comparison. Not
   currently implemented.

6. **Bare boolean at statement level.** `(( x > 0 ))` as standalone
   statement is dead code — compile error.

Fix: thread context (statement vs condition) into the lowerer.
In condition context, comparisons produce integer boolean results
directly via arithmetic IR. In statement context, comparisons are
compile errors. In both contexts, the wrong kind is rejected with
an SB diagnostic code.

In bash-mode, coerce or reject (see dd31-reject-mutation-in-expr).
