# 074 — parse_statement extraction

## Goal

Reduce `parse/parser.rs::parse_statement` (CC 30 live; 31 audit) to a
short orchestrator. Per audit: extract `reject_postfix_on_compound`.

## Approach

Three lifts:

1. `dispatch_statement(scanner, tok, span)` — the 20-arm Tok router.
   Pure dispatch, one arm per leading-token shape. Tagged
   `// cyclo-allow:` — 20 arms by construction; same shape we already
   allow elsewhere (cf. `expr_to_string`, `file_test_op_code`).
2. `parse_comment_stmt(scanner, text, span)` — the only inline arm in
   the original dispatch (advances scanner, builds Statement). Made
   symmetric to the other `parse_*` subparsers.
3. `reject_compound_postfix(stmt, scanner)` — the SB4021/22/23/24
   postfix check. Drives a small lookup table
   `compound_postfix_rejection(next_tok) -> Option<(&'static str, String)>`
   that returns the code/message pair for the offending operator.

The parent becomes 20 lines: skip terminators, dispatch, branch on
compound to either reject or wrap.

## Result

| function | CC |
| --- | --: |
| `parse_statement` | 30 → **7** |
| `dispatch_statement` | 21 (intrinsic; `cyclo-allow`'d) |
| `compound_postfix_rejection` | 5 |
| `reject_compound_postfix` | 2 |
| `parse_comment_stmt` | 1 |

## Verification

- `cargo build --release` — clean
- `cargo clippy --release --lib -- -D warnings` — clean
- `cargo test --lib --release` — 12/12
- `spec/run.sh` — 291/291

## Notes

- The `dispatch_statement` CC of 21 is one above the 20-arm count
  because the `Tok::Word(ref w) if w == "-"` arm carries a pattern
  guard. Folded into the cyclo-allow rationale.
- The original postfix block had four sequential `if matches!(...)`
  blocks each returning `Err(parse_error(...))`. The table form
  centralises that and makes adding a new SB-code arm trivial.
- No diagnostic codes/messages change. SB4021–SB4024 wording is
  preserved verbatim.
