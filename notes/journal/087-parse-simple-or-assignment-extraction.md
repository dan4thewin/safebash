# Journal 087 — parse_simple_or_assignment shape probes

## Goal

Reduce `parse_simple_or_assignment` (`parse/parser.rs`) from CC 17 to
floor. A-verdict from `r47-abstraction-audit.md:284`. Audit recipe:
"6-arm leading-rejected-keyword table inline. Extract
`try_reject_unsafe_keyword(scanner, span) -> Option<Diagnostic>`."

## Result

| function                     | CC |
| ---------------------------- | -: |
| `rejected_keyword_msg`       |  8 |
| `parse_simple_or_assignment` |  4 |
| `looks_like_regex_mutation`  |  3 |
| `try_rejected_keyword`       |  2 |
| `looks_like_assignment`      |  2 |

Parent 17 → 4.

## Refactor shape

```
parse_simple_or_assignment
├─ try_rejected_keyword(scanner, span) -> Option<Diagnostic>
│   └─ rejected_keyword_msg(w) -> Option<(&'static str, String)>  // SB4001/4002/5001 table
├─ looks_like_assignment(scanner) -> bool                          // word [+]= ... probe
├─ looks_like_regex_mutation(scanner) -> Option<String>            // word =~ ... probe
└─ parse_command / parse_assignment / parse_regex_mutation         // delegate
```

## Key insight

The original had three responsibilities packed in one function:

1. Reject 6 banned builtins (eval, source/., export, declare/typeset,
   readonly, select) — each arm was 6 lines of `scanner.next(); return
   Err(parse_error(span, code, msg))`. Total: 50+ lines for what is
   morally a single table lookup.
2. Look ahead to detect `name = value` / `name += value` /
   `name[idx] = value` (assignment shape).
3. Look ahead to detect `var =~ s/.../.../` (regex mutation shape).

Splitting these into three small probes makes the parent a clean
3-step gauntlet. The keyword table (`rejected_keyword_msg`) is the
single point where banned-builtin diagnostics live, and adding a new
banned word now means adding a single match arm.

## Why two `looks_like_*` helpers and not one

The two shape probes look superficially similar (peek a Word, peek_n(1)
another token), but they return different types: assignment returns a
yes/no decision (the parent will re-fetch the name from `parse_assignment`
which does the full subscript handling), while regex-mutation needs to
hand the cloned name to `parse_regex_mutation`. Distinct return types →
distinct helpers.

## scanner mut

`Scanner::peek` and `peek_n` take `&mut self` (they may need to fill
the lookahead buffer). The probes therefore take `&mut Scanner` even
though they don't consume tokens — caller is unchanged because the
parent already had `&mut Scanner`.

## Gates

- `cargo build --release` ok
- `cargo clippy -D warnings` clean
- `cargo test --lib` 12/12
- `spec/run.sh` 291/291

## Files touched

- `sash/src/parse/parser.rs` — replaced the ~90-line body with the
  4-helper tree.
- `notes/tasks/r47-abstraction-audit.md:284` — annotated.
