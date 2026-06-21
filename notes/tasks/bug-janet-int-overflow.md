---
title: "BUG — janet backend: integer arithmetic does not trap i64 overflow (DD#25)"
status: ready
priority: P2
kind: bug
depends: []
gates:
  - "janet: `int x=9223372036854775807; (( x += 1 ))` exits non-zero (overflow aborts) like native/c"
  - "drop the in_modes native,c guard on spec errors.sh 25.1 and arithmetic.sh 25.2/25.3"
---

# BUG — janet `(( ))` integer overflow does not abort

## Symptom (janet backend only)

```
int x=9223372036854775807
(( x += 1 ))
echo $x
```

native/c: the program aborts with a non-zero exit (DD#25 — i64 overflow
is a runtime trap). janet: prints `9.22337203685478e+18` and exits 0.

## Cause

The janet backend lowers `int` arithmetic onto janet's native number
tower (IEEE double / bignum), which has no fixed-width i64 overflow.
`(( x += 1 ))` emits `(set x (+ x 1))`, and janet silently promotes past
`i64::MAX` instead of trapping. There is no equivalent of the native
checked-arith / `LetExit`-with-overflow path.

## Discovery

Found 2026-06-21 while fixing `bug-dollar-hash-in-arith`. The janet
arith-value emission had a pre-existing double-paren bug
(`(string ((+ ...)))` → janet evaluated `(+ ...)` then tried to *call*
the result as a function, e.g. `5 called with 0 arguments`). That crash
made the overflow specs 25.1–25.3 *false-pass* in janet: the program
aborted (non-zero exit) for an unrelated reason, satisfying
`[[ $rc != 0 ]]`.

Fixing the double-paren (correct: `(string (+ ...))`) removed the
spurious crash and exposed that janet never implemented overflow
trapping. The three overflow specs are now guarded `in_modes native,c`
with `skip_chk` for janet (errors.sh 25.1, arithmetic.sh 25.2/25.3).

## Fix direction (for DG review)

Implementing i64 overflow semantics in janet needs explicit checks. One
option: wrap each `int`-typed arith op in a checked form that compares
against `int/min`/`int/max` and aborts (janet has 64-bit `int/s64` via
the `int` module — using `(int/s64 ...)` operands would trap or wrap in
a defined way). This is a real feature, not a one-liner; surface to DG
before implementing.
