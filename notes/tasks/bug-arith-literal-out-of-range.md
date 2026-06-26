---
title: "BUG — arithmetic integer literal > i64::MAX silently parses as 0"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "`echo $(( 9223372036854775808 ))` does NOT silently yield 0"
  - "DG decision: out-of-range literal → compile error (SB-code) vs saturate/trap"
  - "native + c + janet consistent"
---

# BUG — out-of-range integer literal silently becomes 0

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
