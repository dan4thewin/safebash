---
title: "Question: the forms a `-` still reaches after DD#55's no-reach rule (2026-10-30)"
status: open
priority: P3
kind: question
depends: []
gates:
  - "DG rules on R1-R4, R6 and R7 (R5 is settled by retire-suppression-on-compounds); DD#55 records each ruling"
  - "every repro below prints what the ruling says, in both backends, with specs"
---

DD#55's 2026-10-30 rule (journal 548): a `-` discards only its own
command's or call's status. It does not reach a user function's body, a
capture or value call among the command's operands, or a `(( ))` test
operand. F6 unit B implemented that for commands, calls, `$(- …)` and
arithmetic tests. The five shapes below were left as they were, because
each one is a question rather than an obvious bug (DG 2026-10-24/25; per
DG 2026-10-28, nothing here may cement the current suppression story).

Measured on the unit B binary, in both backends, with `--no-taint` so the
taint layer does not reject the probes first. Every line below behaved the
same in native and C.

## R1. An assignment's own captures stay suppressed

| repro | prints |
|---|---|
| `- x=$(/bin/false); echo "a1 [$x]"` | `a1 []` |
| `- x+=$(/bin/false)` | `a7 []` |
| `map m=(); - m[k]=$(/bin/false)` | `a8 []` |
| `map m; - m[$(/bin/false)]=v` (an index capture) | runs on, writes `m[""]` |
| `- x=$(strlen $(/bin/false)); echo "[$x]"` | `[0]` |

Compare `int n=$(- strlen $(/bin/false))` (noreach.12), which aborts: the
`-` there discards strlen's answer, not its argument capture's.

Question: is `- x=$(cmd)` a synonym of `x=$(- cmd)`? An assignment has no
status of its own except its capture's, so the synonym reading is natural.
Recommendation: yes for the right-hand capture only; an index capture and a
nested operand capture are operands, which the rule says a `-` does not
reach. Under that reading the first three rows stay as they are, and the
last two abort.

## R2. An arithmetic assignment's operand captures stay suppressed

`map m; m[k]=1; int n=0; - (( n = ${m[$(/bin/false)]:-0} )); echo "after $n"`
prints `after 0`. `hoist_arith_mutations` (transform/statements.rs) lowers
the store as a suppressed assignment, so its operand capture inherits the
`-`. The test form (noreach.14) aborts.

Recommendation: the same barrier as the test form, i.e. abort.

## R3. A `-` on a value intrinsic that has no status

`str x=$(- toupper abc); echo "[$x]"` prints `[ABC]`. The `-` is dead,
because toupper has no failure to discard. Some intrinsics do report one
(getenv, DD#101).

Question: is a `-` on a status-less intrinsic an error, as `- void_f` is
(SB4076, DD#55 2026-10-30)? Pointer: the value-intrinsic arm of
`lift_suppressed_capture` (transform/functions.rs).

## R4. A dead `-` on a void function's capture escapes SB4076

`fn f { echo tok }; - x=$(f); echo "[$x]"` prints `[tok]`, while
`x=$(- f)` is SB4076 ("function 'f' returns nothing, so `-` has nothing to
discard"). The body is checked either way: with `/bin/false` first in f
(e17), the unit B binary aborts, where the previous one printed `[tok]`.

This follows R1: if `- x=$(f)` is a synonym of `x=$(- f)`, it gets
SB4076 too.

## R5. A compound `-` still suppresses its own direct commands

| repro | prints |
|---|---|
| `- { /bin/false; echo x }; echo done` | `x`, `done` |
| `fn f { /bin/false; echo inf }; - { if (( 0 )) { f } else { /bin/false } }; echo done` | `done` |
| `fn f { /bin/false; echo x; return 1 }; - { f }; echo done` | aborts (f's body is checked) |

The third row changed with unit B: the previous binary printed `x` and
`done`. DG 2026-10-30 ruled that groups go and that a `-` never prefixes
`for` or `while`, so this shape is settled by retire-command-groups and
retire-suppression-on-compounds. It is listed here only so the rows have a
home until those land.

## R6. A statement `-` covers a whole list

`--emit-ir` shows `- a || b` as `Suppressed(List(or, a, b))`, so the `-`
covers b too: `- /bin/false || /bin/false; echo done` prints `done` (q17,
both backends). A pipeline-head `-` binds stage 1 only (`- a | b` leaves
b checked). After F6 unit C (journal 549), the list ends with b's status
either way, so the program's exit status is right, but b's failure no
longer aborts.

Question: does a statement `-` bind the whole list or only its first
operand? Under the first-operand reading, `- a || b` would be
`(- a) || b`, the same as `a || b`. That makes the `-` dead, so it would
want a diagnostic of its own.

## R7. SB4076 does not look through a list

`fn f { /bin/echo in; /bin/false }; - f` is SB4076: f is void, so the `-`
has nothing to discard. But `- /bin/false || f` compiles (r01). The `-`
is dead there either way: `||` consumes a's failure, and a void f cannot
fail. f's body is still checked, and r01 aborts at its `/bin/false` (no
reach). Which diagnostic is right depends on R6, so no spec pins this form
yet.

## B84 is fixed on every route

F6 unit C (H219, `lower_list_stmt`; journal 549) makes a suppressed `||`
list keep its right side's status, whichever route reaches it: the
statement form (R6), `- x=$(a || b)` (R1), or a group (R5). Before, all
three exited 0 on `/bin/false || /bin/false`. Specs 55.145-55.147 use only
the statement form, so that they cement none of the open questions above.

Probe sources: files/port-f6-probes.tar.gz (`f6b/r/`; R6, R7 and B84: `f6c/p/` for the q rows, `f6c/r/` for the r and s rows).
