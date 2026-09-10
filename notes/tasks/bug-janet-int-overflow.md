---
title: "BUG — janet backend: integer arithmetic does not trap i64 overflow (DD#25)"
status: done
priority: P2
kind: bug
depends: []
gates:
  - "RESOLVED by design (2026-06-21): janet uses idiomatic numbers; native/c are the arithmetic reference. Overflow specs 25.1-25.3 stay in_modes native,c."
---

# BUG — janet `(( ))` integer overflow does not abort

## Resolution (DG, 2026-06-21): wontfix — janet uses idiomatic numbers

**Relax janet's arithmetic fidelity.** The janet backend exists to
**illuminate the IR** with readable, idiomatic janet — not to be a
byte-faithful execution oracle. Making it i64-faithful would mean adopting
`int/s64` everywhere plus an explicit overflow-check-and-abort around every
`int` op; that buries the IR-illustrating output in noise and defeats
janet's purpose. So **janet uses its idiomatic number tower** (doubles),
accepting two divergences from native/c at the i64 edges:

- **No overflow trap** (DD#25): `int/s64` *wraps* (`MAX+1` → `MIN`) and
  plain janet numbers are doubles — neither aborts. native/c abort.
- **Precision past 2^53**: e.g. `int x=9007199254740993` prints
  `9.00719925474099e+15` in janet vs the exact value in native/c.

native and c remain the reference oracles for arithmetic. The overflow
specs (errors.sh 25.1, arithmetic.sh 25.2/25.3) stay `in_modes native,c`
(already guarded). No code change; this documents the accepted limitation.

## Original symptom (for reference)

```
int x=9223372036854775807
(( x += 1 ))
echo $x
```

native/c abort (non-zero exit, DD#25 trap); janet prints a wrapped/float
result and exits 0. By design per the resolution above.

## Discovery

Found 2026-06-21 fixing `bug-dollar-hash-in-arith` (the janet double-paren
arith crash had been *false-passing* the overflow specs). Resolved as
by-design in the 2026-06-21 decision review.

## Closed 2026-09-10 (backlog prune)

Closed 2026-09-10 (backlog prune) — `wontfix` normalised to `done`. janet-only
in any case.
