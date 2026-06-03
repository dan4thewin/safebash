# Journal 100 — scan_arith_number per-base split

## Goal

First Phase 11 target. `scan_arith_number`
(`parse/arith.rs:399`, CC 20). Recipe from the post-Phase-10 audit:
split the inline 4-form number lexer (hex / octal / arbitrary-base /
decimal) into per-form helpers.

## Pattern

The function bundled four independent scanners in one body:

| form | trigger | inner work |
| --- | --- | --- |
| hex | `0x` / `0X` prefix | scan hex digits, `from_str_radix(_, 16)` |
| octal | `0` + digit | scan `0..=7` digits, `from_str_radix(_, 8)` |
| arbitrary base | decimal digits + `#` | scan alphanumeric digits, `parse_arith_base_digits(_, base)` |
| decimal | fallthrough | parse `i64` |

Each form has its own digit-scan loop and its own `parse` call. CC
20 came from the three `if` branches multiplying with the per-form
loops and the prefix tests.

## Action

Replaced the body with a flat 4-way dispatcher and 5 helpers:

```rust
fn scan_arith_number(input: &str, bytes: &[u8], i: usize) -> (ATok, usize) {
    if is_hex_prefix(bytes, i)   { return scan_hex_literal(input, bytes, i); }
    if is_octal_prefix(bytes, i) { return scan_octal_literal(input, bytes, i); }
    let digits_end = scan_decimal_digits(bytes, i);
    if digits_end < bytes.len() && bytes[digits_end] == b'#' {
        return scan_based_literal(input, bytes, i, digits_end);
    }
    scan_decimal_literal(input, i, digits_end)
}
```

Helpers:

| helper | CC | what |
| --- | --: | --- |
| `is_hex_prefix(bytes, i)` | 1 | predicate |
| `is_octal_prefix(bytes, i)` | 1 | predicate |
| `scan_hex_literal(input, bytes, start)` | 3 | one loop + parse |
| `scan_octal_literal(input, bytes, start)` | 3 | one loop + parse |
| `scan_decimal_digits(bytes, start) → usize` | 3 | shared by decimal and based |
| `scan_based_literal(input, bytes, start, base_end)` | 5 | one loop + 3-byte predicate + parse |
| `scan_decimal_literal(input, start, end)` | 1 | just parse |

The `scan_decimal_digits` extraction is what makes the dispatcher
trivial: it does the digit-scan once, then the dispatcher decides
based on the byte after the digits whether to dispatch to the
base-N path or finish as decimal.

## Verification

```
scan_arith_number    CC 20 → 5
scan_based_literal     5  (new)
scan_hex_literal       3  (new)
scan_octal_literal     3  (new)
scan_decimal_digits    3  (new)
scan_decimal_literal   1  (new)
```

- `cargo build --release` — clean
- `cargo clippy --quiet -- -D warnings` — clean
- `cargo test --lib --quiet` — 12/12
- `bash spec/run.sh` — 291/291

`scan_arith_number` drops out of the report; no other parse/arith
symbol enters the top-15.
