---
title: "janet: coerce string variables to int in arithmetic ($((x+0)) where x is a string)"
status: done
priority: P3
kind: bug
depends: []
gates:
  - "janet: `x=42; echo $((x + 0))` -> 42 (string var coerced to int in arith)"
  - "janet honors DD#72 string coercion: 0x/0o/0b prefixes + decimal with leading zeros ignored (x=010 -> 10)"
  - "then spec/arith_literals.sh nl.s1-s10 can drop their `in_modes native,c` guard"
---

# janet doesn't coerce a string variable to int in arithmetic

Surfaced 2026-07-05 (DD#72 numeric literals). In native/c, a variable's
string value used in arithmetic is coerced via `sash_atoi` (DD#72: honors
`0x`/`0o`/`0b`, else decimal with leading zeros ignored). **janet doesn't
coerce at all** — it keeps the var as a string and the arithmetic op fails:

```
$ sash --janet -c 'x=0xff; echo $((x + 0))' ...
error: could not find method :+ for "0xff"
```

This fails even for a plain decimal string (`x=42; $((x+0))`), so it's a
general janet arith-coercion gap, not specific to DD#72's prefix rules.

Per the janet-is-an-IR-illustration-not-an-oracle convention (native/c are
the numeric oracles), `spec/arith_literals.sh` guards the string-coercion
tests `nl.s1`-`nl.s10` to `in_modes native,c` (skip in janet).

## Fix

In `emit_janet`, when lowering an arithmetic operand that is a variable (or
any string value), wrap it in a scan-to-int coercion (janet `scan-number`
or an explicit helper) that mirrors DD#72: honor `0x`/`0o`/`0b`, otherwise
parse decimal digits ignoring leading zeros. Then drop the `in_modes`
guard on `nl.s1`-`nl.s10`.

## Relationship
- DD#72 numeric literals (native/c done). This is the janet half of the
  string-coercion story.
- Consistent with other janet numeric divergences (e.g. bug-janet-int-overflow,
  wontfix; janet uses idiomatic numbers).

## 58.2/58.3 dependency (2026-07-14)

DD#58 int-func-in-arith with args (`fn add -> int { return $(($1+$2)); }`,
`$(( (add 3 4) ))`) fails in janet only because `(add 3 4)` passes numeric
args while the body does `(scan-number (get _args 0))` — scan-number errors
on a number. Fixing the string/number coercion here (a `number?`-guarded
coerce, or passing string args) unblocks janet spec 58.2 and 58.3.

## RESOLVED (2026-07-14, commit 28db76d)

Added a `_num` preamble helper coercing arith operands to int at each use
site (bare `x`, `$x`, `$1`, compound-assign target read, inc/dec). DD#72
0x/0o/0b honored via janet radix `8r`/`2r` (scan-number handles 0x + leading
zeros but not 0o/0b). Fixes janet 58.2/58.3, cf.f5, cf.f8, 16.2; un-guarded
arith_literals nl.s1-s10 to all modes. c/native unchanged; 0 regressions.
