# 131 — numeric literals in arithmetic (DD#72)

DG raised the design question directly: "we have `0xff` and `0377` — why
support arbitrary bases (`16#ff`)?" Investigation turned the
`bug-arith-based-literal-hash-comment` task into a full numeric-literal
model plus the discovery of **four silent miscompiles**.

## The findings (before)

In `$(( ))`, sash accepted decimal, `0x`, C-octal (`0377`), and bash
`base#digits` — but silently mis-evaluated:
- `16#ff` → **16**, `2#1010` → **2** (the `#…` was lexed as a *comment* by
  the main scanner, truncating the literal before the arith number scanner —
  which actually *had* base# code — ever ran).
- `0o10` → **0**, `0b1010` → **0** (modern prefixes silently yielded 0).

And two more silent-error sites surfaced while implementing:
- `tokenize_arith` **silently skipped** unknown bytes (arith.rs).
- `parse_arith_str` had **no leftover-token check**, so `5 # 3` → 5
  (silent tail), and the primary parser recovered an unknown token as `0`.

## The model (DD#72, DG-decided)

Output of arithmetic is always decimal, so literal *input* syntax has no
effect on byte-identical stdout — sash is free to pick a clean set.

- **Constants** (source literals): decimal + `0x` + `0o` + `0b`. Reject
  (SB4035, never silent) leading-zero integers (`010`/`08`/`0377`), bash
  `base#`, and invalid prefix digits (`0o8`). Rationale: authored → force
  the writer to disambiguate; a ported bash `010` meaning octal-8 gets a
  compile error, not a silent value change.
- **Runtime string→int** (a variable's/data's value in arith, `sash_atoi`):
  lenient. Honors `0x`/`0o`/`0b`, otherwise decimal with **leading zeros
  ignored** (`x=010` → 10, NOT 8). Rationale: data isn't authored; plain
  decimal is robust and makes bash's `10#$var` decimal-forcing idiom
  unnecessary (sash never octal-traps `08`/`010` from `date`/`read`).

The only constant/string divergence is the leading-zero case (strict-reject
vs lenient-decimal); prefix handling is identical. DG's exact words:
"honor prefixes but continue to ignore leading zeroes without the x/o/b".

## Implementation

- `parse/arith.rs`: `scan_arith_number` rewritten — `0x`/`0o`/`0b` via a
  generic `scan_radix_literal`; leading-zero and `base#` → `ATok::BadLit`
  (new variant) → **SB4035** (`arith_bad_literal_error`), distinct from
  SB4026 (out-of-range). Removed `scan_octal_literal`/`scan_based_literal`/
  `parse_arith_base_digits`. `tokenize_arith` unknown byte → `BadLit` (hard
  error, no skip). `parse_arith_str` gained `reject_trailing_token` (leftover
  → SB4035/SB4020).
- `parse/lexer.rs`: inside `$(( ))` (`arith_depth > 0`), `#` is emitted as a
  word, not a comment — so `16#ff` reaches (and is rejected by) the arith
  parser instead of being silently truncated to `16`.
- `sash-rt/src/inner.rs`: `atoi_inner` now honors `0x`/`0o`/`0b`
  (`radix_prefix` + `parse_radix_magnitude`) and otherwise parses decimal
  (`parse_decimal_magnitude`, leading zeros ignored). Native + c share this;
  overflow still traps (DD#26).

## Verification

- `spec/arith_literals.sh` (new): 12 accept + 10 reject (SB4035) + 9 string-
  coercion. **native/c pass 31/31.** janet skips the 9 string-coercion tests
  (`in_modes native,c`) — janet doesn't coerce a string var in arith at all
  (fails even on plain `42`); filed `janet-arith-string-coercion`.
- Migrated `arithmetic.sh` 26.7 (`$(( 017 ))` → was 15; now rejects SB4035)
  + added 26.7b (`0o17` → 15). Only spec using C-octal; no other specs use
  C-octal/base#.
- Authoritative spec.db diff vs HEAD: **native 0 / c 48 / janet 71 —
  0 regressions.** sash-rt atoi tests: 31/31 pass (the 1 `overflow_tests`
  failure, `itoa_min_i64_panics`, is stale pre-existing rot on `itoa`, not
  atoi). sash clippy clean; complexity gate green.

## Next
- `janet-arith-string-coercion` (P3) — the janet half.
- Observed but out of scope: `x=16#ff` (non-arith assignment) yields empty —
  a `#`-in-word lexing quirk, separate from arith. Not filed (edge; revisit
  if it bites).
