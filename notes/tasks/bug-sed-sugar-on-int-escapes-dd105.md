---
title: "BUG: `${n:s/…/…/}` on an `int` compiles, because the desugar writes the `\"$n\"` conversion the author did not (DD#105)"
status: open
priority: P3
kind: bug
depends: []
gates:
  - "`${n:s/…}` on an int-typed scalar or cell is SB4030, as `$(sub $n …)` and `n =~ s/…/` already are"
  - "`str s=\"$n\"` then `${s:s/…}` stays legal"
  - "every row listed as already correct keeps its verdict"
---

## What

DD#105 rejects a string intrinsic applied to an int-typed operand. It keys on
the operand's type, not its spelling, and it allows `"$n"` as the explicit
conversion, because "a double-quoted word is a string". The written-out
intrinsic follows the rule, and the statement form is rejected too, by the
assignment rule. The expansion sugar compiles.

Results were measured after F15, and are the same in both backends, on the
base commit 1309385 and on the batch-531 branch.

| Id | Program | Today | Expected |
|---|---|---|---|
| ai3 | `int y=123` / `echo ${y:s/1/9/}` | prints `923` | SB4030 |
| ai8 | `int y=123` / `echo ${y:s/1/9/g}` | prints `923` | SB4030 |
| ai5 | `int y=123` / `str s=${y:s/1/9/}` / `echo $s` | prints `923` | SB4030 |
| ai10 | `int array a=(123)` / `echo ${a[0]:s/1/9/}` | prints `923` | SB4030 |
| ai2 | `int y=123` / `int n=$(( ${y:s/1/9/} + 1 ))` / `echo $n` | prints `924` | SB4030; see also `bug-arith-operator-expansion-unconverted` |

### Already correct

| Id | Program | Today |
|---|---|---|
| ai6 | `int y=123` / `y =~ s/1/9/` / `echo $y` | SB4030 at 2:1, the assignment rule: "cannot assign string value to int variable `y`" |
| ai7 | `int y=123` / `echo $(sub $y 1 9)` | SB4030 at 2:8, DD#105's message, reported twice (B2 in `batch-531-findings`) |
| ai4 | `int y=123` / `echo ${y:0:1}` | SB4030 at 2:6, substring of an int variable |
| ai11 | `int array a=(123)` / `echo ${a[0]:0:1}` | the same |
| ai12 | `int y=123` / `str s="$y"` / `echo ${s:s/1/9/}` | prints `923`, through the explicit conversion |

## Cause

The parser desugars `${y:s/1/9/}` to `$(sub "$y" "1" "9")`: `--emit-ir`
shows the subject as a `:double-quoted` word. `try_sed_op` builds a scalar's
subject with `quoted_var_expr`, and `subscript_operator_expr` builds a
cell's with `quoted_cell_expr`. Both wrap the read in a double-quoted word.

`check_string_intrinsic_operands` treats a double-quoted operand as String by
construction. That is exactly the escape DD#105 keeps for `"$n"`. So the
desugar writes the explicit conversion on the author's behalf, and the
checker cannot tell the two apart.

## Fix

Two approaches would work:

- Build the subject so that the checker still sees its type. One option is
  an unquoted expansion, if that is equivalent as a `sub` argument; another
  is a quoted word marked as synthetic. Either keeps one check site, in
  `check_string_intrinsic_operands`.
- Check the subject's type before the desugar.

Found during the batch-531 port (step F15), while probing the sed-extent
family. Filed under DG's rule for the port that questions and uncovered cases
become backlog tasks.
