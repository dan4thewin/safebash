# 122 — DD#25 overflow sweep: Pow + Div (continuation)

Continues the DD#25 overflow work (journal 120) — the remaining
overflow-capable ops from `bug-runtime-arith-overflow-wraps`.

## Probed each op (DD#25 = "overflow traps at runtime")

| op | before | after |
|----|--------|-------|
| Pow (`2 ** 70`) | saturated to i64::MAX, **rc 0** (silent) | traps `<argv0>: arithmetic overflow`, rc 2 |
| Div (`MIN / -1`) | **SIGFPE core dump**, rc 136 | clean trap, rc 2 |
| Neg (`-MIN`) | already traps (`-x` → `0 - x` → `ssub_overflow`, journal 120) | unchanged ✓ |
| Mod (`MIN % -1`) | `0` (correct — `srem` doesn't overflow) | unchanged ✓ |
| Shl (`<<`) | blocked — `<<` mis-lexed as heredoc | deferred (`bug-arith-left-shift-heredoc`) |

So only **Pow** (silent saturate) and **Div** (ungraceful SIGFPE) needed
fixing; Neg/Mod were already correct.

## Fixes (spec-first; native + C, janet excluded — bignum)

- **`sash_pow`** (sash-rt): `None => sash_arith_overflow()` instead of
  `return i64::MAX`. The const-fold path (`l.checked_pow`) already returns
  `None` → runtime `sash_pow`, which now traps.
- **Native Div** (`emit_div_or_mod_with_zero_check`): after the divisor==0
  check, guard `lhs == i64::MIN && rhs == -1` → `sash_arith_overflow`
  before the `sdiv` (x86 `idiv` would `#DE`). `srem` (Mod) is left alone —
  cranelift lowers `srem(MIN, -1)` safely to 0.
- **C Div/Mod** (`emit_div_or_mod_c`): C lowers *both* `/` and `%` to
  `idiv`, so both `#DE` on `MIN/-1`. Added an `else if (l == INT64_MIN &&
  r == -1)` arm: Div → `sash_arith_overflow()`, Mod → `0` (matching
  native). Used the literal `(-9223372036854775807L - 1)` (no
  `<stdint.h>`; `i64` is `long`).

## Spec

`spec/arithmetic.sh` 25.9 (Pow traps), 25.10 (Div MIN/-1 clean trap —
asserts rc==2, not SIGFPE 136), 25.11 (Neg of MIN traps — regression),
25.12 (Mod MIN/-1 == 0 — regression). native **0 failing** (531 passing);
c=58/janet=106 unchanged (zero regressions).

## Build gotcha (re-confirmed)

`sash-rt` is **not** in the cargo workspace — `cargo build --release`
does NOT rebuild it. The Pow fix needed `cd sash-rt && cargo build
--release` before it took effect (the first test still saturated against
the stale `libsash_rt.a`).

## Bonus bug filed

`$(( 9223372036854775808 ))` (i64::MAX+1) silently parses to `0`
(`scan_decimal_literal` does `.parse().unwrap_or(0)`). Filed
`bug-arith-literal-out-of-range` (P2) — fix direction (compile-error vs
saturate) is a small DG decision.

## Remaining

Shl overflow (blocked on `bug-arith-left-shift-heredoc`); const-fold
trap-vs-compile-error (DG sub-decision). With those, DD#25 is fully
closed.
