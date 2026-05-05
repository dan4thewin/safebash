title: DD#55 — non-void function detection and call-site enforcement
status: ready
depends: []
severity: correctness
gates: [55.12, 55.13, 55.14, 55.15]
  - tests
  - commit

DD#55 requires:
- Any function with a `return` statement is non-void
- `return 0` alone (only possible return is zero) → compile error
- Bare call of non-void function → compile error
- Non-void must be handled: `if func`, `func || handle`, `- func`

### Detection

Walk function IR bodies. Collect all `return` statements. If any
exist, function is non-void. If ALL return statements are `return 0`
(or `return` with no argument, which defaults to 0), that's a
compile error — the return is ceremony.

### Call-site enforcement

For every bare function call (not in `if`/`while` condition, not
on LHS of `||`/`&&`, not prefixed with `-`), check whether the
function is non-void. If so, compile error with diagnostic:
"function 'X' returns a status — handle with if/||/- or capture".

### Where to implement

Could be a validation pass after lowering (needs function table),
or integrated into transform where function calls are resolved.

### int-typed functions

Bare call of int-typed function → compile error (discards return
value). Must capture: `x=$(func)` or handle: `if func`.
