# Journal 096 — parse_test_primary three-shape split

## Goal

Sixth Phase 10 target. `parse_test_primary`
(`parse/parser.rs:911`, CC 25) — audit row 282 (verdict C/A).
Recipe noted "Token dispatcher with multiple binary-op arms."

## Pattern

The function did three jobs that shared no machinery:

1. Parenthesized group: consume LParen, recurse into
   `parse_test_expr`, consume RParen.
2. Leading unary: if first token is `Word("-X")` and X is a known
   unary op, consume + parse operand.
3. Operand + optional binary tail: parse an `Expr`, then dispatch
   on the next token across 5 binary-op shapes (Word-op,
   `=`/`==`, `!=`, `<`, `>`) plus an implicit-nonempty fallback.

The binary tail had 5 arms each ending with:

```rust
let right = parse_test_operand(...)?;
Ok(TestExpr::Binary { left: Box::new(left), op, right: Box::new(right) })
```

— with the `=~` arm uniquely calling `parse_regex_rhs` instead.
The fallback (and the `!` arm's else-branch) duplicated the
"implicit -n" constructor twice.

## Refactor

Five helpers + a single-line primary dispatcher:

| helper | CC | role |
| --- | --: | --- |
| `parse_test_group` | 3 | LParen path |
| `try_parse_leading_unary` | 6 | unary-op probe (returns `Option`) |
| `parse_test_binary_tail` | 9 | 6-shape binary dispatcher |
| `parse_binary_rhs` | 4 | regex-vs-operand chooser + Binary wrap |
| `implicit_nonempty` | 1 | the `TestExpr::Unary { op: "-n", … }` constructor |

The primary becomes:

```rust
fn parse_test_primary(scanner, span) -> Result<TestExpr, Diagnostic> {
    if matches!(scanner.peek().token, Tok::LParen) {
        return parse_test_group(scanner, span);
    }
    if let Some(unary) = try_parse_leading_unary(scanner, span)? {
        return Ok(unary);
    }
    let left = parse_test_operand(scanner, span)?;
    parse_test_binary_tail(left, scanner, span)
}
```

`parse_binary_rhs` collapses the 5 binary arms from ~8 lines each
to ~3, including the regex special case (`if op == "=~"`).
`implicit_nonempty` is named because the spec interprets bare
`[[ x ]]` and `[[ x ! ]]` (no `=`) as "x is nonempty" — the name
documents the rule.

## Result

| function | before | after |
| --- | --: | --: |
| `parse_test_primary` | 25 | 5 |

All new helpers under audit band. Of the 6 functions, only
`parse_test_binary_tail` (CC 9) is close to the band; it's a pure
binary-op dispatcher and won't grow.

## Insight

The audit's "C/A" verdict was honest — this was borderline. What
tipped it into A territory was the 5-arm binary tail with
near-identical bodies. Once `parse_binary_rhs` consolidates the
"consume → wrap as Binary" pattern, each arm becomes 3 lines (or 4
for the Eq/Bang arms that peek for a second token). The dispatcher
shape is now visible at a glance — previously you had to read 60
lines to learn the shape was "five token-cases + wildcard."

`implicit_nonempty` is a single-line helper that exists purely for
the documentation value. Two callers use it: the `Bang`-without-`Eq`
arm and the wildcard. Without the name, readers had to deduce why
both fall back to the same `Unary { op: "-n", … }` construction.

## Gates

- `cargo build --release` clean
- `cargo clippy -D warnings` clean
- `cargo test --lib` 12/12
- `spec/run.sh` 291/291

## Files touched

- `sash/src/parse/parser.rs` — primary rewritten; 5 helpers added.
- `notes/tasks/r47-abstraction-audit.md:282` — annotated.
