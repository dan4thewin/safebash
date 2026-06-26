title: DD#25 — Arithmetic Overflow Traps
status: implemented

Integer overflow traps at runtime (not silent wraparound). Call depth
limited to prevent stack exhaustion.

## Implementation (corrected 2026-06-26)

`+`, `-`, `*` trap on runtime overflow via cranelift `sadd_overflow` /
`ssub_overflow` / `smul_overflow` (native) and `__builtin_*_overflow` (C),
both calling `sash_arith_overflow` → `<argv0>: arithmetic overflow`,
exit 2. The trap is unconditional — a `-` (suppress) prefix does **not**
suppress it. Constant folding (`fold_arith_op`) uses checked arithmetic
and defers a compile-time-known overflow to the runtime trap rather than
wrapping. See `notes/journal/120` and `bug-runtime-arith-overflow-wraps`.

**Known gap (follow-up):** `Pow` saturates rather than traps; `Shl`
(shift ≥ 64), unary `Neg` (i64::MIN), and `Div` (i64::MIN / -1) are not
yet trap-with-message. The janet backend uses float/bignum and does not
trap (`bug-janet-int-overflow`, wontfix), so overflow specs are
`in_modes native,c`.

