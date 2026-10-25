---
title: "BUG: `${n:?msg}` on an int is typed as a string, unlike `$n` and `${n:-…}`"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "`${n:?msg}` on an int has n's type, as `${n:-…}` already does: d1, e4, e5, e6, d4 and d5 become SB4030, like their `$n` twins"
  - "d6 compiles and prints 6, and k4 prints `[5]`"
  - "specs pin d1 and d6 in both backends"
---

## What

`expr_type` types `${n:-…}` as n's type but `${n:?msg}` as a string. So for
an int `n`, the `:?` form slips past every check that asks whether an operand
is an int. That includes D1's rule (DD#30's addendum of 2026-10-25: a
whole-int operand of `[[ ]]`, a dispatch or an attest predicate is SB4030) and
the assignment checker. Every program starts with `int n=5`:

| Id | Program | Today | The `$n` or `${n:-0}` twin |
|---|---|---|---|
| d1 | `if [[ ${n:?bad} == 5 ]] { echo T } else { echo F }` | prints `T` | d3 `$n`, d2 `${n:-0}`: SB4030 |
| e4 | `if [[ -n ${n:?bad} ]] { echo T }` | prints `T` | SB4030 |
| e5 | `attest n [[ ${n:?bad} == 5 ]]` / `echo "$n"` | prints `5` | SB4030 |
| e6 | `[[ ${n:?bad} == 5 {echo five} \|\| {echo other} ]]` | prints `five` | SB4030 |
| d4 | `str s=${n:?bad}` / `echo "[$s]"` | prints `[5]` | d7 `str s=$n`: SB4030 |
| d5 | `s=${n:?bad}` / `s+=x` / `echo "[$s]"` | prints `[5x]` | k1 `s=$n`, k2 `s=${n:-0}`: SB4030 (`s` is inferred int) |
| d6 | `int m=${n:?bad}` / `m+=1` / `echo "$m"` | SB4030 "cannot assign string value to int variable `m`" | k3 `int m=${n:-0}`: prints `[5]` |

d6 (and k4, `int m=${n:?bad}` / `echo "[$m]"`) is a false rejection; the
other rows are false acceptances. Both backends agree. The fault is
pre-existing; D1's walker reaches these operands, and `expr_type` answers
String.

The cause is one match arm in `parameter_expansion_type`
(sash/src/type_check.rs:721), line 727:
`":-" | "-" => env.var_type(&pe.name)`. `:?` falls through to String. Both
typing walkers ask `expr_type` (133): the checker, and `infer_types` (2749)
through `infer_assignment_type` (2770). So the one arm fixes both.

The arm's `"-"` alternative is unreachable from source: `${n-0}` and
`${n?bad}` (no colon) are SB4034 (e3, e2). `${n:~}` is SB5046 (e1).

## Fix

Recommendation (made during D1, not yet ruled): add `":?"` to that arm.
`${n:?msg}` yields `$n` or aborts, so its type is n's type, as `${n:-…}`'s
already is.

## Related

- DD#30's addendum of 2026-10-25 cites this task.
- B5 in `batch-531-findings`: END is not type-checked, so an int operand
  there still compiles in every spelling.

Found during the batch-531 port (step D1, probing the operand spellings the
new rule must reach). Filed under DG's rule for the port that questions and
uncovered cases become backlog tasks.
