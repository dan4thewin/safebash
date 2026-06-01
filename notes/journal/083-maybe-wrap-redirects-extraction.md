# 083 — maybe_wrap_redirects extraction

## Goal

Reduce `parse/parser.rs::maybe_wrap_redirects` (CC 23) by applying
the audit recipe: split the 3-phase post-statement tail into
`parse_trailing_redirects`, `parse_trailing_pipeline`,
`parse_trailing_list_op`.

## Approach

Top-level becomes a straight pipeline of phase calls:

```rust
fn maybe_wrap_redirects(scanner, stmt) -> Result<Statement, Diagnostic> {
    let redirects = parse_trailing_redirects(scanner, stmt.span)?;
    let mut result = wrap_with_redirects(stmt, redirects);
    result = parse_trailing_pipeline(scanner, result)?;
    parse_trailing_list_op(scanner, result)
}
```

Phase helpers:

| helper | role |
| --- | --- |
| `parse_trailing_redirects` | Collect zero or more redirect ops (`>`, `>>`, `<`, `>&`, `<&`, `<>`, `&>`, `<<<`, and `Ndigit-prefixed` forms). Loop terminates at first non-redirect token. |
| `wrap_with_redirects` | If any collected, wrap in `Redirected`; else passthrough. |
| `parse_trailing_pipeline` | If `|` next, consume more stages; flatten right-nested pipelines so `a \| b \| c` is one 3-stage node. |
| `parse_trailing_list_op` | Dispatch on `&&`, `||`, `&`. |
| `binary_list_op` | Shared `&&`/`||` body — `parse_statement`, default to `true_cmd` if missing. |
| `background_list_op` | `&` with synthetic `true` right-hand side. |

## Result

| function | CC |
| --- | --: |
| `maybe_wrap_redirects` | 23 → **3** |
| `parse_trailing_redirects` | 8 |
| `parse_trailing_pipeline` | 8 |
| `parse_trailing_list_op` | 5 |
| `wrap_with_redirects` | 2 |
| `binary_list_op` | 2 |
| `background_list_op` | 1 |

`maybe_wrap_redirects` no longer appears in the file's CC top-10.

## Verification

- `cargo build --release` — clean
- `cargo clippy --quiet -- -D warnings` — clean
- `cargo test --lib` — 12/12
- `spec/run.sh` — 291/291

## Notes

- Behaviour-preserving. The original code maintained `result` as a
  running accumulator across three independent phases — that's the
  same as threading the value through three `Result`-returning
  functions, so the refactor is a near-mechanical lift.
- `binary_list_op` consolidates the `AndAnd`/`OrOr` arms that
  previously duplicated the parse-statement / wrap dance. The only
  difference between them was the `ListOp` variant, now passed in.
- `background_list_op` synthesises the right-hand side with
  `Statement::true_cmd` — same as the original.
- All four helpers are private to `parser.rs`; no module API change.
