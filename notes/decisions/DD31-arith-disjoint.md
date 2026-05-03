title: DD#31 — Arithmetic mutations and comparisons are disjoint
status: implemented

Three arithmetic contexts, each with a single purpose:

| Context | Purpose | Accepts | Rejects |
|---|---|---|---|
| `(( ))` statement | Mutation | `=`, `+=`, `-=`, `x++`, `x--` | Comparisons, prefix `++x`/`--x` |
| `if (( ))` condition | Comparison | `==`, `!=`, `<`, `>`, `<=`, `>=` | Mutations |
| `$(( ))` expression | Pure evaluation | Arithmetic on clean operands | Mutations |

`$(( ))` is a read context — it returns a value. Mutation inside a
read context (`$((x=10))`) is invalid sash (compile error).

## Increment/decrement (Go rule)

Only postfix `x++` and `x--`. No prefix forms. Statement-only —
never produces a value. These are syntactic sugar for `x = x + 1`
and `x = x - 1`.

Valid:
- `(( count++ ))` — standalone mutation statement
- `(( x++, x > 0 ))` — non-final position in comma expression

Invalid:
- `(( x = y++ ))` — `++` inside expression (not a statement)
- `(( ++x ))` — prefix form rejected
- `if (( x++ ))` — mutation in condition position

## Comma operator

The comma in `(( ))` is syntactic sugar that splits one `(( ))` into
multiple statements. Each non-final expression must be a mutation.
In condition position (`if`/`while`), the final expression must be a
comparison:

```bash
# desugars to: (( count = count - 1 )); if (( count > 0 ))
while (( count = count - 1, count > 0 )); do
```

Equivalent to `while` with multiple statements before `do`, using
the final as the condition.

## Bash-mode coercion

- `$((x=10))` → `((x=10))` (mutation extracted to statement form)
- `$((x=10, x<y))` → `((x=10)); $((x<y))` (compound split: mutation
  to statement, comparison stays as expression in context where it
  has meaning)

Statement `(( ))` exit status is always 0. Condition `if (( ))`
produces a boolean. `(( x > 0 ))` as bare statement → compile error.
`if (( x++ ))` → compile error.
