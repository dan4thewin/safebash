---
title: "janet: join intrinsic gaps — bare-name join a, and separator join @a SEP"
status: ready
priority: P3
kind: bug
depends: []
effort: effort-janet-backend-parity
gates:
  - "janet `$(join a)` (bare-name array) space-joins, instead of exec'ing /usr/bin/join"
  - "janet `$(join @a SEP)` / `$(join a SEP)` joins with SEP, instead of erroring (`bad slot, expected string, got array`)"
  - "un-guard spec/types.sh 33.19–33.21 to include janet once both work"
---

# janet join intrinsic gaps

Surfaced 2026-08-11 while implementing DD#75 join (`join arr SEP`, array-first).
janet's join only handles the `@a` **space**-join (`$(join @a)` → `(join a " ")`,
works). Two gaps:

- **bare-name `join a`** → janet emits an external-command exec (`join a`),
  running `/usr/bin/join` instead of the intrinsic. native/c resolve bare `a`
  to the array; janet's emit doesn't.
- **separator `join @a SEP`** → janet emits a call that passes the array to a
  janet builtin expecting a string → `error: bad slot #1, expected string, got
  <array>`. The separator isn't wired through janet's join emission.

Guarded `spec/types.sh` 33.19–33.21 to `native,c` meanwhile. The arg-order and
the native non-array reject (SB4030) are backend-independent and already land.

## Addendum (2026-08-14): keys intrinsic also missing in janet

Same class: janet execs `$(keys m)` as an external command (`("keys" "m"):
No such file or directory`) instead of emitting the map-keys intrinsic.
Pre-existing (cf. `janet:33.10`). Surfaced again by DD#75 §3 spec 75.7
(guarded to native,c). Gate: janet `for k in $(keys m)` iterates the map's
string keys. Fold into the janet collection-intrinsics work.

## Addendum (2026-08-14): `len` in arith-operand position missing in janet

`emit_janet` lowers a `len`/`arrlen`/`strlen` arith operand to `(len …)`, but
janet's builtin is `length` — so `$(( (len a) + 1 ))` and `$(( ${#a} + 1 ))`
emit an undefined `len` symbol (janet compile error). native/c fine. Guarded
`spec/arithmetic.sh` 58.10–58.16 to native,c. Gate: janet evaluates the len
intrinsic (array/map count, string byte length) as an arith operand — emit
`(length …)` or a table-len helper. Same collection-intrinsic class as join/keys.
