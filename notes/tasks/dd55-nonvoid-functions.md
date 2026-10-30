title: DD#55 — non-void function detection and call-site enforcement
status: done
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

## Follow-up (2026-10-28)

A bare call of a status-returning (void) function still discards its
`return` (batch-531 B15); that, and whether `- myfunc` stays legal, are open
in `q-suppression-on-user-functions` Q3 and Q2.

## Follow-up (2026-10-30)

Both are ruled (`q-suppression-on-user-functions`, "Rulings, 2026-10-30").
B15 is enforced: a bare call or a list tail of a status function is an error
unless it is caught or discarded with `-`. The original design's bare
`return` = `return 0` stands. F6 implements this. The ceremony rule for
status functions is not enforced; it moved to `q-status-function-ceremony`.
