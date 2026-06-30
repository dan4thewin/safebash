---
title: "BUG — arithmetic based literal `base#digits` drops the digits (`#` eaten as comment)"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "`echo $(( 16#ff ))` yields 255 (not 16); `echo $(( 2#1010 ))` yields 10 (not 2)"
  - "the `base#digits` scanner (scan_based_literal) is actually reached"
  - "out-of-range base (not 2..=64) and >64-bit based value are compile errors (SB4026), consistent with Policy A bit-pattern rules"
  - "native + c + janet consistent"
---

# BUG — based integer literals silently lose their digits

Found 2026-06-27 while implementing the literal-range / bit-pattern work
(`bug-arith-literal-out-of-range`, Policy A).

## Repro

```bash
$ sash -c 'echo $(( 16#ff ))'  -o b && ./b   # -> 16   (bash: 255)
$ sash -c 'echo $(( 2#1010 ))' -o b && ./b   # -> 2    (bash: 10)
```

sash returns just the **base** and drops the `#digits` payload — a silent
miscompile (wrong value, no error).

## Cause (suspected)

The `#` is being treated as a start-of-comment delimiter *before* the
arith tokenizer's `scan_based_literal` (sash/src/parse/arith.rs:435) is
reached — most likely in `collect_arith_content` (or the shell-level
lexer) stripping `#…`. So `16#ff` reaches `tokenize_arith` as just `16`.
Evidence: if `scan_based_literal` ran, `16#ff` would compute
`15*16+15 = 255`, not `16`. bash does **not** honor `#` comments inside
`$(( ))`, so within collected arith content `#` should only ever be the
base delimiter.

## Scope / fix direction

- Make arith-content collection (or the arith lexer) treat `#` as the
  base delimiter, not a comment, inside `$(( ))` / `(( ))`.
- Once the scanner is reached, fold in the Policy-A range handling that
  was deferred from `bug-arith-literal-out-of-range`:
  - `parse_arith_base_digits` currently accumulates with unchecked
    `acc * base + d` (silent wrap) — make it checked u64 → `None` on
    overflow.
  - reject base outside `2..=64` → SB4026.
  - reject a digit `>= base` (e.g. `2#9`) — bash errors "value too great
    for base" (optional, separate from overflow).
- Spec: add the based-literal cases to spec/arithmetic.sh section 26
  (the value `16#ff=255`, `2#1010=10`, plus a >64-bit-overflow and a
  bad-base compile-error case) once the `#` handling is fixed.

## Relationship

Sibling of `bug-arith-literal-out-of-range` (Policy A). Decimal/hex/octal
range handling shipped there; based literals were deferred here because
the `#`-comment bug makes the based path unreachable and thus untestable.
