# 121 — P0 bug: compound-assignments other than `+=` were no-ops

Found while verifying the overflow fix (journal 120): the cargo test
`arith_cmd_compound_assign` (expects 12, gets 15) — flagged in
`rust-test-suite-rot` as "stale expectation" — turned out to be a **real,
spec-uncovered bug**.

## Symptom

`(( x OP= N ))` for OP ∈ {−, *, /, %, &, |, ^} was a **no-op** (x
unchanged); only `+=` worked. `/=`/`%=` even printed "division by 0".
Explicit `x = x - 3` worked. Pre-existing (reproduces on baseline).

```
int x=10; (( x -= 3 )); echo $x   # → 10 (want 7)
int x=12; (( x /= 2 )); echo $x   # → 12 + "division by 0" (want 6)
```

## Root cause

The janet backend made it visible:
- `(( x += 2 ))` → `(set x (+ x 2))` ✓
- `(( x -= 2 ))` → `(not= 0 (- x 0))` ✗ — a *test*, operand `2` became `0`.

Instrumenting `tokenize_arith` showed the **input string was already
mangled**: `-=` arrived as `"x - = 3"` (a space between `-` and `=`),
while `+=` arrived intact as `"x += 3"`.

Chain: the shell lexer has no arith context, so it tokenizes `-=` as two
tokens (`Minus`, `Eq`) — but `+=` is the single shell token `PlusAssign`.
`collect_arith_content`/`step_default_token` reassemble the `(( ))` body
from shell tokens, and `needs_separator_space` inserted a separator
between `-` and `=` → `"- ="`. The arith byte-lexer then read `Minus`,
then `Eq` (unknown prefix → `parse_prefix` returns `Literal(0)`), so the
expression became `x - 0` (a comparison, no write-back).

`needs_separator_space` already had this exact special-case for `<=`/`>=`
(suppress the space so the arith lexer recombines `<`/`>` + `=`); it just
didn't cover the arithmetic compound-assignment operator chars.

## Fix

Extend the `needs_separator_space` `<op>=` special-case from `< >` to
also include `- * / % & | ^` (one-liner, `parse/arith.rs`). `+=` already
fine; `<<=` is the lone remaining failure (a deeper heredoc-vs-shift
tokenization of `<<`, filed `bug-arith-left-shift-heredoc`).

Spec-first lock-in: `spec/arithmetic.sh` 31.1d–31.1i (−=, *=, /=, %=,
bitwise chain, mixed chaining). native 0-failing (527 passing);
c=58/janet=106 unchanged (zero regressions).

## Related

- `rust-test-suite-rot` listed `arith_cmd_compound_assign` as a candidate
  stale test — it was a real bug; the cargo test asserted correct
  behavior. (It uses bare `x=10` string-typed syntax, a separate rot
  issue, but the underlying arithmetic was genuinely broken.)
- Left-shift `<<`/`<<=` remains broken — `bug-arith-left-shift-heredoc`.
