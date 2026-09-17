---
title: "bug: splatting a non-collection (`@s`) segfaults"
status: done
priority: P1
kind: bug
depends: []
gates:
  - "`str s=hi; for x in @s { … }` is a COMPILE error naming the type mistake, not a segfault"
  - "`@nosuch` is a compile error, not a codegen ICE"
  - "array and map splats still traverse — no over-rejection"
---

## What

The same defect as `bug-scalar-subscript-segfault` (SB5068), one operator
over. `@name` lowers to a collection traversal, so a scalar subject makes the
generated code read a `char*` as a `SashTable*`.

Five faces, all compiling clean beforehand:

| program | before |
|---|---|
| `str s=hi; for x in @s { … }` | segfault |
| `str s=hi; echo $(strlen @s)` | segfault |
| `str s=hi; f @s` (user fn) | segfault |
| `int i=5; for x in @i { … }` | ICE `codegen/types.rs:29` |
| `for x in @nosuch { … }` | ICE `codegen/types.rs:190` |

`echo @s` was already masked by SB5030, but that is a *disposition* rule
(where a splat may appear), not a type rule — it says nothing about the
subject, so it covered one spelling by accident.

## Why the subscript fix did not already cover it

SB5068 could leave undefined names to SB5031 because SB5031 already fired
there. **SB5031 enumerates scalar reads, and a splat is not one** — so
`@nosuch` fell through to codegen and ICE'd. That asymmetry is the whole
reason this needed its own undefined-name arm rather than a copy of SB5068.

## Fix

`SB5079` in `validate.rs` (`check_scalar_splat`), registered in `lib.rs`
beside `check_scalar_subscript`. It reuses SB5068's machinery verbatim —
`program_scopes`, `collect_subscriptable_names`,
`subscript_subject_is_scalar` — which is the payoff for that fix having put
the scope logic in the validator rather than in `type_check.rs` (a `for`
variable is never in `TypeEnv.vars`).

Undefined splat subjects report **SB5031**, matching
`splat_not_subscriptable_diag`, which already reports a splat misuse that
way.

Specs `75.81`–`75.89` in `spec/types.sh`: five rejections and four
over-rejection guards.

## Found

2026-09-17, chasing a cold arm in `transform/expressions.rs::expr_to_string`
during the coverage push. Probing `strlen @s` to see whether a non-literal
could reach the arm segfaulted instead. The coverage question was a pretext;
the bug was the find.

## Lesson

The SB5068 postmortem said the subscript defect was "the third case of a
polymorphic operation, neither supported nor rejected". That survey listed
`len`, `join`, subscript, and **for-in** — and for-in is the splat. The
inventory named this one and it was fixed one operator at a time anyway.
When a survey enumerates siblings, check them all in the same pass.
