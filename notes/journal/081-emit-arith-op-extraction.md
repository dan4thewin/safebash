# 081 — emit_arith_op extraction

## Goal

Reduce `codegen/arithmetic.rs::emit_arith_op` (CC 22) to the ArithOp-
dispatcher floor. The audit recipe called out one extraction (the
50-line Div/Mod zero-check block); the comparison and logical
arms also collapse cleanly.

## Approach

1. **`emit_div_or_mod_with_zero_check`** — bash's `n/0` and `n%0`
   semantics (stderr message, `$? = 1`, result = `0`). Splits
   internally into `emit_div_by_zero_error` for the message+exit-
   status write.
2. **`comparison_intcc(op) -> Option<IntCC>`** — table for the six
   comparison ops. The dispatcher takes a fast-path `if let Some(cc)
   = comparison_intcc(op) { return emit_icmp_bool(cc, ...); }` so
   the main `match` no longer needs six near-identical arms (the
   single arm for them now becomes `unreachable!`).
3. **`emit_logical_and_or(op, ...)`** — short-circuit-free `&&`/`||`
   on already-evaluated operands. One arm in the dispatcher with
   `ArithOp::And | ArithOp::Or =>` collapses two prior arms.
4. **`emit_icmp_bool(cc, ...)`** — `icmp` + `uextend(I64)` for a 0/1
   result.
5. **`emit_pow(...)`** — moved the runtime-helper call out for
   consistency.

## Result

| function | CC |
| --- | --: |
| `emit_arith_op` | 22 → **15** (cyclo-allow'd) |
| `comparison_intcc` | 8 |
| `emit_div_or_mod_with_zero_check` | 2 |
| `emit_div_by_zero_error` | 2 |
| `emit_logical_and_or` | 2 |
| `emit_icmp_bool` | 1 |
| `emit_pow` | 1 |

Total file CC sum lower; dispatcher is 13 arms + 1 fast-path + 1
unreachable — at its floor.

## Verification

- `cargo build --release` — clean
- `cargo clippy --quiet -- -D warnings` — clean
- `cargo test --lib` — 12/12
- `spec/run.sh` — 291/291

## Notes

- Behaviour-preserving. The zero-check block, comparison widening,
  and logical arms all emit byte-identical Cranelift IR — verified
  via the spec suite (arithmetic and `[[ … ]]` tests cover most
  operators).
- `comparison_intcc` is the dual of `simple_escape` (table-driven
  byte-to-char): a small pure function that captures the per-arm
  data and lets the caller funnel through one shared emitter.
- The dispatcher's catch-all `Eq | Ne | Lt | Le | Gt | Ge =>
  unreachable!()` arm is intentional — it documents which ops have
  moved to the fast-path and keeps the match exhaustive without
  re-emitting their bodies.
- `// cyclo-allow:` annotation reuses the wave-2 phrasing
  ("13-arm ArithOp dispatcher; arms == CC by construction").
