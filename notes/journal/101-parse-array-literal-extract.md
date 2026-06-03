# Journal 101 — parse_array_literal extraction

## Goal

Second Phase 11 target. `parse_array_literal`
(`parse/expr.rs:902`, CC 20). Recipe from the audit: extract
`try_parse_assoc_element` so the loop body distinguishes the two
assoc surface forms (`[k]=v` with `]=` inside one Word token, and
`[k] = v` with `=` as a separate token) in one place.

## Pattern

The body interleaved three concerns inside a single loop:

1. Whitespace/EOF/RParen control flow.
2. Form-1 assoc detection: `if let Tok::Word(w) = … if w.starts_with('[')
   if let Some(close) = w.find("]=")` — three layers of nesting before
   the actual key/value extraction. The inline quote-stripping (used
   only for the `=value-in-word` short-circuit path that bypasses the
   lexer) added a 6-line nested let chain.
3. Form-2 assoc detection: `if w.ends_with(']') && w.len() > 2 ` and
   `if matches!(next_tok, Tok::Eq)` — another two layers.
4. Plain element fallthrough.

CC 20 came mostly from those nested ifs around the assoc paths.

## Action

Extracted two helpers:

- `try_parse_assoc_element(scanner, span) → Result<Option<(String, Expr)>, Diagnostic>`
  — recognises either form and consumes its tokens, or returns
  `Ok(None)` so the caller can parse a plain element. Uses a let-else
  to bail when the next token isn't a `Word`, and combines the form-2
  predicate with the lookahead `matches!` so the two ifs collapse.
- `strip_matching_quotes(s) → String` — single-layer matching-quote
  strip. Used only by the form-1 in-word value path; the regular
  lexer path goes through quote handling elsewhere. Byte-indexed to
  avoid the UTF-8 boundary risk that `starts_with('"')` quietly
  carried.

`parse_array_literal` now reads:

```rust
loop {
    while matches!(scanner.peek().token, Tok::Newline) { scanner.next(); }
    if matches!(scanner.peek().token, Tok::RParen)     { scanner.next(); break; }
    if scanner.at_eof()                                { break; }

    if let Some((key, val)) = try_parse_assoc_element(scanner, span)? {
        is_assoc = true;
        assoc_pairs.push((key, val));
        continue;
    }
    elems.push(parse_expr(scanner)?);
}
```

The trailing `if is_assoc { … } else { … }` shrank to a `let kind = if …`
+ single `Ok(Expr)` construction.

## Verification

```
parse_array_literal    CC 20 → 9
try_parse_assoc_element      9  (new)
strip_matching_quotes        6  (new, byte-indexed)
```

- `cargo build --release` — clean
- `cargo clippy --quiet -- -D warnings` — clean
- `cargo test --lib --quiet` — 12/12
- `bash spec/run.sh` — 291/291

`parse_array_literal` drops out of the filtered top-15. No new entries
from `parse/expr.rs` enter the report.

## Note on the UTF-8 hardening

The previous body used `val_str.starts_with('"') && val_str.ends_with('"')`
to detect a wrapping quote pair, then sliced `[1..len-1]`. That slice
panics if either end is a multi-byte char boundary. In practice the
caller always feeds ASCII `"` / `'`, so it never tripped — but moving
the predicate into `strip_matching_quotes` made it trivial to switch
to byte-indexed comparisons that are panic-free even on bad input.
