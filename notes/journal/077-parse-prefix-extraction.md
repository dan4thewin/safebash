# 077 — parse_prefix (arith.rs) extraction

## Goal

Reduce `parse/arith.rs::parse_prefix` (CC 28) to the dispatcher
floor. Per audit: 12-arm token dispatcher with duplication — `++`/`--`
arms repeat, and unary `Minus`/`Plus`/`Bang`/`Tilde` all repeat. Recipe:
extract `parse_pre_incdec(inc)` and `parse_unary(op)`.

## Approach

Three lifts:

1. `parse_pre_incdec(inc: bool)` — folds the `PlusPlus`/`MinusMinus`
   arms. Uses let-else for the "expected variable after ±±" error
   path; chooses the error wording from `inc`.
2. `parse_unary(op: IrUnaryArithOp)` — folds the four unary arms
   (`-`, `+`, `!`, `~`). Each was three lines (advance + parse_expr(25)
   + UnaryOp construction); now one call site per op.
3. `parse_lparen_form()` — lifts the longest arm (DD#58 sexpr-or-
   grouping). Tried as a function call first (`(name)` zero-arg or
   `(name args...)`); if not, restores `self.pos` and parses as
   grouping. Audit recipe didn't call this out, but lifting it
   matches the per-arm-helper shape and was needed to hit dispatcher
   floor.

## Result

| function | CC |
| --- | --: |
| `parse_prefix` | 28 → **13** |
| `parse_lparen_form` | 10 |
| `parse_pre_incdec` | 2 |
| `parse_unary` | 2 |

## Verification

- `cargo build --release` — clean
- `cargo clippy --quiet -- -D warnings` — clean
- `cargo test --lib` — 12/12
- `spec/run.sh` — 291/291

## Notes

- CC 13 is at the dispatcher floor: 11 outer match arms (3 trivial
  Num/Var/DollarVar + 2 ±± + 4 unary + LParen + `Some(_)` + None) +
  `?` operator return in PlusPlus/MinusMinus through the helper
  doesn't show; the +1 over arm count is the early-return inside
  helpers being counted elsewhere. (Calls to helpers add nothing.)
- Behaviour-preserving. The "expected variable after ++/--" error
  text remains the same byte-for-byte (chose helper wording from
  `inc` to preserve the exact original strings).
- `parse_lparen_form` reuses the original `saved_pos` rollback
  semantics for the sexpr-vs-grouping decision — no change to DD#58
  handling.
