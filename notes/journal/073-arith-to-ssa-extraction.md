# 073 — arith_to_ssa dispatcher cleanup

## Goal

Reduce `transform/expr_handlers.rs::arith_to_ssa` (CC 30) to a flat
12-arm dispatcher, extracting `try_fold_binop` and `try_fold_unop` per
the audit recommendation.

## Approach

The original had four high-content arms:

1. `BinOp`: recursive lowering, op translation, inline `if let
   (Expr, Expr)` constant-folding, fallback `ArithBinOp` construction.
2. `UnaryOp`: recursive lowering + three sub-arms (Neg, BitNot, Not)
   each duplicating the same inline `if let Expr` fold pattern with a
   different reduction.
3. `CompoundAssign`: lookup-LHS + recursive RHS + `ArithBinOp`.
4. `PreIncDec/PostIncDec`: lookup + `+1`/`-1`.

Lifted helpers:

- `try_fold_binop(l, r, op) → Option<SsaValue>` — fold both-Expr case.
- `try_fold_unop(inner, op) → Option<SsaValue>` — fold single-Expr case;
  table-driven across all four unary ops including `Pos`.
- `binop_to_ssa`, `unaryop_to_ssa` — orchestrate lower-then-fold-or-build.
- `var_to_ssa` — DD#58 int-typed function nullary call vs plain lookup.
- `incdec_to_ssa`, `compound_assign_to_ssa`, `comma_to_ssa`,
  `funccall_to_ssa` — small per-arm lifts.

The parent becomes one line per arm.

## Result

| function | CC |
| --- | --: |
| `arith_to_ssa` | 30 → **13** |
| `try_fold_unop` | 7 |
| `unaryop_to_ssa` | 6 |
| `try_fold_binop` | 3 |
| `binop_to_ssa` | 2 |
| (other lifts) | 2 each |

CC 13 is the floor for a 12-arm pure dispatcher (one CC per arm + 1).

## Verification

- `cargo build --release` — clean
- `cargo clippy --release --lib -- -D warnings` — clean
- `cargo test --lib --release` — 12/12
- `spec/run.sh` — 291/291

## Notes

- `try_fold_unop` now table-drives **all four** unary ops including
  `Pos`. The original had three inline folds (Neg, BitNot, Not) and
  no `Pos` fold — `+x` where `x` was a literal was already returned
  unmodified by the runtime-form arm (`Pos => inner`), so no
  observable change, but the lifted helper is more uniform.
- No new whitelist needed. The audit's CC-30 estimate held.
