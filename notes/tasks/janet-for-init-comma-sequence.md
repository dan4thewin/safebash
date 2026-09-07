---
title: "janet: a comma-sequenced C-style for-init is not emitted"
status: ready
priority: P3
kind: bug
depends: []
gates:
  - "janet: `for (( i = 0, j = 10; i < 3; i++ )); do echo $i$j; done` prints 010/110/210 (matches native/c)"
---

## Found

2026-09-07, adding the DD#94 specs (journal 251). Spec `arithmetic.sh` 94.10
pins that the C-style for-header is unaffected by the new arithmetic
separators. It passes in native and c, and fails in janet:

```
/tmp/v.janet:12:25: compile error: unknown symbol i
```

Verified **pre-existing** by stashing the DD#94 source changes and rebuilding:
the clean tree produces the identical error, and the comma path was untouched
by that work (the for-parser splits the header on `;` before the arith parser
runs, and `ArithExpr::Comma` is unchanged).

A single-assignment init is fine — `for (( i = 0; i < 3; i++ ))` works — so the
gap is specifically an init slot holding a comma sequence: janet emits nothing
that binds either name, and the loop condition then references an unknown
symbol.

## Fix sketch

The janet for-loop emitter needs to lower an `ArithExpr::Comma` in the init
slot as a sequence of bindings (each `(var name value)`), rather than expecting
a single assignment. The same treatment likely applies to a comma sequence in
the step slot (`i++, j--`), which is worth checking at the same time.

## Specs

`arithmetic.sh` 94.10 is gated `native,c`.
