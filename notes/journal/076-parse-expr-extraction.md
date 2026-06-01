# 076 — parse_expr (expr.rs) extraction

## Goal

Reduce `parse/expr.rs::parse_expr` (CC 32) to a leaner Tok dispatcher
by lifting the two heaviest arms. Per audit: extract the `Tok::Word`
arm (tilde / @arr) and the `Tok::Dollar` arm (`$arr[i]` / `$LINENO`).

## Approach

Four lifts:

1. `parse_word_arm(scanner, w, span) -> Expr` — owns the `Tok::Word`
   arm: tilde-bare (`~` → `$HOME`), tilde-prefix (`~/path` → concat),
   `@arr` array-values shorthand, and the bare literal fallback.
   Consumes the token.
2. `parse_dollar_arm(scanner, name, span, start, parts)` — orchestrates
   the `Tok::Dollar` arm: probes for subscript first, then handles
   `$LINENO` (DD#24 compile-time line number) or falls back to
   `SimpleExpansion`.
3. `try_subscript_arm(scanner, name, span, parts) -> bool` — DD#41
   `$arr[i]` probe. Returns true (and pushes 1–2 parts) on match.
   Cleanly factors out the nested-if-let-Some-of-find-of-Word logic.
4. `parts_to_expr(parts) -> Result<Expr, Diagnostic>` — final
   reduction: empty → SB4014, single → unwrap, multiple → `Concat`.

The dispatcher itself is now 13 token arms, each ≤ 8 lines.

## Result

| function | CC |
| --- | --: |
| `parse_expr` | 32 → **18** |
| `parse_word_arm` | 5 |
| `try_subscript_arm` | 5 |
| `parts_to_expr` | 4 |
| `parse_dollar_arm` | 3 |

## Verification

- `cargo build --release` — clean
- `cargo clippy --quiet -- -D warnings` — clean
- `cargo test --lib` — 12/12
- `spec/run.sh` — 291/291

## Notes

- CC 18 is near the dispatcher floor for this function: 12 outer
  match arms + 2 `?` operators (CommandSub, ArithExpansion) + 2 `&&`
  in the loop-entry guard + 1 loop = 17 intrinsic. The remaining +1
  comes from the nested `if`/`else if`/`else` in `parse_word_arm`'s
  caller-side... actually that's inside the helper now. The +1 is
  just leftover branchiness; further extraction (e.g., the
  Keywords-OR arm or the Eq/Dash trivial arms) would not reduce CC
  meaningfully and would obscure the dispatch shape.
- Behaviour-preserving. DD#24 (LINENO) and DD#41 (`$arr[i]`) paths
  unchanged.
- `try_subscript_arm` uses let-else for the early returns, which
  clippy prefers over nested `if let Some(close) = w.find(']') { ... }`.
