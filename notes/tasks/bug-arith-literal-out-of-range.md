---
title: "BUG — arithmetic integer literal > i64::MAX silently parses as 0"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "[DONE] `echo $(( 9223372036854775808 ))` is a compile error (SB4026), not silent 0"
  - "[DONE, DG 2026-06-27] Policy A: decimal magnitude > i64::MAX → compile error; hex/octal are 64-bit bit-patterns (u64→i64 reinterpret, 0xFFFF…F = -1); i64::MIN expressible in arith via unary-neg peephole"
  - "[DONE] non-arith `int x=NNN` out-of-range → SB4026 (validate.rs); i64::MIN still works"
  - "[DONE] native + c consistent (janet skips i64::MAX/MIN boundary values — double rounds)"
  - "[SPLIT OFF] based literals `base#digits` — separately broken (the `#` is eaten as a comment before the scanner); deferred to bug-arith-based-literal-hash-comment"
---

# RESOLVED 2026-06-27 (Policy A, DG)

Implemented in `49…` (this session). Spec section 26 in
spec/arithmetic.sh (10 cases, native+c green, janet guards the i64
boundary values). See journal 124.

- **Decimal** > i64::MAX → SB4026 (`scan_decimal_literal` returns a new
  `ATok::BadNum(raw)`; the parser rejects it with `arith_range_error`).
- **i64::MIN peephole**: `parse_unary` folds `-<BadNum(9223372036854775808)>`
  → `Literal(i64::MIN)`, so `$(( -9223372036854775808 ))` works (bash parity).
- **Hex/octal** are 64-bit bit-patterns: `u64::from_str_radix(..) as i64`
  at scan time, so `0xFFFFFFFFFFFFFFFF` == -1, `0x8000000000000000` ==
  i64::MIN; wider than 64 bits → SB4026.
- **Non-arith** `int x=NNN` / `const x=NNN`: `validate_declaration`
  rejects an out-of-range decimal literal (SB4026, `int_literal_out_of_range`).
  i64::MIN keeps working because the `-` is part of the literal there.
- **Based literals deferred** — see relationship note above.

---

# (original) BUG — out-of-range integer literal silently becomes 0


Found 2026-06-26 while extending the DD#25 overflow sweep
(`bug-runtime-arith-overflow-wraps` — Pow/Div).

## Repro

```bash
$ printf 'int x=$(( 9223372036854775808 ))\necho $x\n' | sash …   # → 0, rc 0
```

`9223372036854775808` is `i64::MAX + 1`. It silently compiles to `0` — a
silent miscompile (wrong value, no error).

## Cause

`scan_decimal_literal` (sash/src/parse/arith.rs:449):

```rust
let val: i64 = input[start..end].parse().unwrap_or(0);
```

`str::parse::<i64>()` fails for an out-of-range literal and `unwrap_or(0)`
swallows it as `0`. (Hex/octal/based literals via `scan_hex_literal` etc.
should be checked too — likely the same pattern.)

## Fix direction — DECISION for DG

What should an out-of-range integer literal do?
- **(a) compile error** (a new SB-code, e.g. "integer literal out of
  range") — most consistent with sash's reject-ambiguity stance and DD#25
  (overflow is never silent). Recommended.
- **(b) saturate / wrap** — matches bash's silent wrap, but sash
  deliberately diverges from bash on overflow (DD#25).

Either way, **never silently `0`**. The current behavior is the worst of
both (neither the literal's value nor an error). Recommend (a): reject at
compile time. Spec-first once DG picks.

## Scope

- Decimal (arith.rs:449), and audit hex/octal/based literal scanners for
  the same `unwrap_or(0)` pattern.
- Also the shell-level `int x=NNN` assignment path (non-arith) — check
  whether a too-large literal there also silently truncates.
- Sibling to `bug-runtime-arith-overflow-wraps` (runtime overflow) — this
  is the *compile-time literal* edge.
