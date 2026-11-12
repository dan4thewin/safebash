title: "arith `${s:-N}` disposition default: support DD#72 0x/0o/0b prefixes"
status: done
priority: P3
kind: feature
depends: []
gates:
  - "`$(( ${s:-0xff} + 0 ))` (s not an int) lowers the default as 255, not a compile error"
  - "0o/0b prefixes and the DD#72 grammar accepted as arith `:-` defaults across native/c/janet"
  - "D4's `check_arith_default_literals` widened from decimal-only to the full DD#72 literal grammar (shared parser)"
  - "no regression"

## Context

DD#77 D4 (`check_arith_default_literals` in `type_check.rs`) currently requires
the `${s:-N}` arithmetic disposition default to be a **decimal** integer literal
(optional `-`, canonical no-leading-zero). Non-decimal defaults — `0xff`,
`0o17`, `0b101`, `010`, `5+3`, interpolations — are rejected (SB4030).

The rejection exists because the default is lowered by a plain **decimal**
`sash_atoi` over the literal bytes (`values.rs` `lower_value_as_int` → the
`DefaultValue` arm passes `d_int = lower_value_as_int(default)`; the same in
`emit_c` / `emit_janet`). So `${s:-0xff}` silently lowered to **0** (atoi stops
at `x`). D4 converts that silent-0 footgun into a clear compile error — strictly
better, but it leaves an inconsistency: `0xff` is a valid DD#72 arith *literal*
(`$(( 0xff ))` → 255) yet is not accepted as a `:-` *default*.

## The gap

DD#72 established the modern literal grammar (decimal + `0x`/`0o`/`0b`, no
leading zeros / `base#`). An arith disposition default is used as an arith
operand, so for consistency it should accept the same grammar and lower it the
same way.

## Approach (sketch)

1. Extract a pure `parse_dd72_literal(&str) -> Option<i64>` from the arith lexer
   (`parse/arith.rs` `scan_number`/`scan_radix_literal` already implement the
   grammar) so both `type_check` (D4 validation) and the lowerer share one
   authority.
2. At transform time, when building `SsaValue::DefaultValue`, if the default is
   a literal-only word, parse it via `parse_dd72_literal` and emit an int-constant
   `SsaValue` — then all three backends lower it as a constant (no per-backend
   atoi change needed).
3. Widen `check_arith_default_literals` to accept anything `parse_dd72_literal`
   accepts; keep rejecting interpolations / expressions / empty.
4. Un-guard `spec/arithmetic.sh` d4.2 (currently pins `${s:-0xff}` → reject) and
   add positive cases (`0xff`→255, `0o17`→15, `0b101`→5).

## Provenance

Filed during DD#77 Stage C3 (D4). The decimal-only D4 was the scope-appropriate
fix for the ratified footgun; full DD#72-grammar default support is the
follow-on feature. Not a bug (no silent-wrong value remains) — a missing feature.
