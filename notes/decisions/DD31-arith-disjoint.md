title: DD#31 — Arithmetic mutations and comparisons are disjoint
status: implemented

Three arithmetic contexts, each with a single purpose:

| Context | Purpose | Accepts | Rejects |
|---|---|---|---|
| `(( ))` statement | Mutation | `=`, `+=`, `-=`, `++`, `--` | Comparisons |
| `if (( ))` condition | Comparison | `==`, `!=`, `<`, `>`, `<=`, `>=` | Mutations |
| `$(( ))` expression | Pure evaluation | Arithmetic on clean operands | Mutations |

`$(( ))` is a read context — it returns a value. Mutation inside a
read context (`$((x=10))`) is invalid sash (compile error).

**Bash-mode coercion:**
- `$((x=10))` → `((x=10))` (mutation extracted to statement form)
- `$((x=10, x<y))` → `((x=10)); $((x<y))` (compound split: mutation
  to statement, comparison stays as expression in context where it
  has meaning)

Statement `(( ))` exit status is always 0. Condition `if (( ))`
produces a boolean. `(( x > 0 ))` as bare statement → compile error.
`if (( x++ ))` → compile error.
