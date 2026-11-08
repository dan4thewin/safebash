---
title: "Suppression residuals: adjacency settled; status-less intrinsic question remains"
status: open
priority: P3
kind: question
depends: []
gates:
  - "DG rules on R3's directly prefixed status-less intrinsics; DD55 records the choice"
  - "R3 enforcement has an explicit owner after the ruling; settled repairs remain open in reject-nonadjacent-suppression and retire-suppression-on-compounds"
  - "Consuming an already discarded status is Q3 in q-suppression-and-stage-residuals, not an unresolved adjacency question"
---

## Decision update (2026-11-08)

DG chose **"reject the - not adjacent to the command"**. DD55/DD71 reject
`- x=$(cmd)` rather than retaining it as an alias of `x=$(- cmd)`.
The rule covers append and cell assignments too. A prefix before `a` in
`- a || b` or `- a && b` belongs only to `a`, not the list or `b`.
This settles R1, R4 and R6. R7's result-kind check likewise belongs to the
actually prefixed command, not an unprefixed list tail.

R2's operand reach violates the existing no-reach rule: the `(( ... ))`
command is not permission to discard a nested capture's failure. The
new P3 `reject-nonadjacent-suppression` owns these repairs. No compiler
change is claimed here; journal 588 records the ruling. R5 remains with
the existing compound-retirement tasks. R3 and the separate consumer
question Q3 remain open.

## Historical evidence

DD#55's 2026-10-30 rule (journal 548): a `-` discards only its own
command's or call's status. It does not reach a user function's body, a
capture or value call among the command's operands, or a `(( ))` test
operand. F6 unit B implemented that for commands, calls, `$(- …)` and
arithmetic tests. F6 left the shapes below unchanged while their policy
was being discussed. They are not exceptions to the accepted rule above.

The R1-R5 measurements used the unit B binary, in both backends, with
`--no-taint` so taint did not reject the probes first. R6/R7 and B84 also
record unit C evidence. These are historical results, not fresh execution
or a claim that every fixture remains otherwise legal after later
return-kind and capture rulings. Revalidate fixtures before implementation.

## R1. Assignment prefixes: reject

| repro | prints |
|---|---|
| `- x=$(/bin/false); echo "a1 [$x]"` | `a1 []` |
| `- x+=$(/bin/false)` | `a7 []` |
| `map m=(); - m[k]=$(/bin/false)` | `a8 []` |
| `map m; - m[$(/bin/false)]=v` (an index capture) | runs on, writes `m[""]` |
| `- x=$(strlen $(/bin/false)); echo "[$x]"` | `[0]` |

The historical noreach.12 control, `int n=$(- strlen $(/bin/false))`,
aborted at its nested capture. It illustrated the operand boundary, not a
decision on R3's directly prefixed intrinsics.

**Ruled:** all five assignment-prefix rows must reject at compile time.
Do not silently rewrite the first three to an inner `-` or merely arrange
for the last two to abort. Suppress the intended command explicitly inside
its capture; the surrounding binding has no result of its own to discard.

## R2. An arithmetic assignment's operand captures stay suppressed

`map m; m[k]=1; int n=0; - (( n = ${m[$(/bin/false)]:-0} )); echo "after $n"`
prints `after 0`. `hoist_arith_mutations` (transform/statements.rs) lowers
the store as a suppressed assignment, so its operand capture inherits the
`-`. The test form (noreach.14) aborts.

**Required by no-reach:** its operand capture must remain checked, as in
the test form. `(( ... ))` is a single command, unlike R1's binding
prefix; adjacency does not settle whether a directly attached prefix is
otherwise inert. The implementation task must not let mutation hoisting
turn the prefix into permission to discard an operand's failure.

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

**Ruled:** reject the outer assignment prefix under R1. Do not retain it
as an alias with a special void-function check. The inner `x=$(- f)`
still has its existing SB4076 refusal.

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

## R6. List binding: only the adjacent command

`--emit-ir` shows `- a || b` as `Suppressed(List(or, a, b))`, so the `-`
covers b too: `- /bin/false || /bin/false; echo done` prints `done` (q17,
both backends). A pipeline-head `-` binds stage 1 only (`- a | b` leaves
b checked). After F6 unit C (journal 549), the list ends with b's status
either way, so the program's exit status is right, but b's failure no
longer aborts.

**Ruled:** the prefix binds only the adjacent command, as a pipeline-head
prefix already does. It cannot make a later operand unchecked. Whether
`(- a) || b` is legal when `||` consumes the discarded status remains Q3
in `q-suppression-and-stage-residuals`; this ruling does not select a
consumer policy or make a discarded status mean success.

## R7. Result-kind checks belong to the prefixed command

`fn f { /bin/echo in; /bin/false }; - f` is SB4076: f is void, so the `-`
has nothing to discard. But `- /bin/false || f` compiles (r01). The `-`
was historically inert there: `||` consumed the left failure, and a void
f has no returned status to discard. f's body was still checked, and r01
aborted at its `/bin/false` (no reach).

R6 now settles the recipient: `- /bin/false || f` does not prefix `f`,
so its void kind is not a reason for SB4076 there. Conversely, in
`- f || other`, the prefix really does target the void function and must
not evade its existing refusal. Q3 still owns the validity and diagnostic
policy for consuming a discarded result.

## B84 is fixed on every route

F6 unit C (H219, `lower_list_stmt`; journal 549) made a suppressed `||`
list keep its right side's status, whichever route reaches it: the
statement form (R6), `- x=$(a || b)` (R1), or a group (R5). Before, all
three exited 0 on `/bin/false || /bin/false`. Specs 55.145-55.147 use the
statement form. They pin the historical status-retention repair, not a
permanent whole-list suppression contract. Review them during enforcement,
use fresh ids for changed contracts and retain status-retention coverage
where the source form remains legal.

Probe sources: files/port-f6-probes.tar.gz (`f6b/r/`; R6, R7 and B84: `f6c/p/` for the q rows, `f6c/r/` for the r and s rows).
