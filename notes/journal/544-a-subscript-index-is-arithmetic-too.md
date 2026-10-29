# 544 - A subscript index is arithmetic too

2026-10-29. F5 unit C of the batch-531 port (Order item 12; journal 542
has the four-unit split): B73.

## The bug

type_check walked a sexpr's calls with its own `walk_arith_funccalls`. It
spelled out its recursion and ended in `_ => {}`, and that wildcard
swallowed `ArithExpr::Subscript` and `ArithExpr::SubscriptAssign`. A
subscript index is arithmetic, so a call written there escaped every rule
the walker feeds:
- DD58's string-callee rule;
- the `len`/`strlen` operand rules (DD75, and DD30's "no implicit
  conversion");
- DD100's one signature per function.

Each program below compiled before this unit. Both backends agreed unless
noted:

| Probe | Form | Printed | Now (SB4030) |
|---|---|---|---|
| p1 | `fn s { echo abc }` … `$(( a[(s)] ))` | `abc`, then `1` | "`s` can't be called in an arithmetic sexpr" 3:6 |
| p4 | `$(( a[(toupper x)] ))` | `1` | "`toupper` can't be called …" 2:6 |
| v6 | `if (( a[(toupper x)] > 0 )) { echo y }` | `y` | the same, 2:4 |
| p2 | `str q=abc` … `$(( a[(len q)] ))` | `0` | "`len` needs an array or map — `q` is a scalar" 3:6 |
| v5 | `int n=5` … `$(( a[(strlen $n)] ))` | `2` | "`strlen` is a string operation but `n` is an int variable" 3:6 |
| p3 | slot function `f`, called as `$(f =x)` and as `a[(f)]` | native ICE `undefined arrlen: A`; C `0 2` | "function `f` is called with both 1 and 0 collection arguments" 5:1 |

The same calls outside a subscript (the `t` twins, e.g. `$(( (s) ))`) were
already rejected with the same texts. Now both positions give the same
answer.

validate had two more private copies: `walk_arith_intrinsic_calls`, which
feeds SB5012 and the sexpr arity check, and `walk_arith_funccall_args`, which
feeds `flag_collection_arith_args`. Both already descended through the total
`ArithExpr::children`. validate's copy had once had the same wildcard;
`bug-sb5012-blind-to-arith-status-reads` repaired it in place, and the repair
never reached type_check's copy. Deleting the two changes no behaviour.

## The port

As-is, from the branch's hunks (decide-and-report):
- ir.rs adds two functions:
  - `walk_arith_nodes` visits every node, outermost first, and descends
    through `ArithExpr::children`;
  - `walk_arith_funccalls` hands each `FuncCall` to its callback through
    `walk_arith_nodes`.
- type_check.rs: the private walker is deleted. `check_arith_sexpr` and
  `walk_stmt_arith_calls` now resolve to ir's walker through
  `use crate::ir::*`.
- validate.rs: `walk_arith_intrinsic_calls` and `walk_arith_funccall_args`
  are deleted.
  - Their callers use the shared walker: `check_arith_sexpr_arity`,
    `expr_calls_intrinsic`, `arith_test_names_intrinsic` and
    `flag_collection_arith_args`.
  - `flag_collection_arith_args` splits out `collection_dollar_var`, the
    branch's complexity-gate refactor.
- `ArithExpr::children`'s doc names `walk_arith_nodes` as one of the two
  callers that rely on its totality.

`children` panics on `PreIncDec`, which the old type_check walker never
did. No program reaches it: the parser rejects `++x` with SB4020
(`reject_prefix_incdec`) before any checker runs.

No DD addendum. DD58, DD30, DD75 and DD100 are unchanged; one position
that escaped them is closed.

## Probes

Probes are in /tmp/port/f5/b73. Each ran in both backends against the
current tree, with the pre-F5 binary (2320434) as the baseline.
- p1-p4, v5 and v6 changed as in the table.
- These are unchanged:
  - p5, `(( a[(f)] = 1 ))`, and v3, `(( a[0] = (toupper x) ))`, were
    already SB4030 by another route;
  - v1 and v2, an assignment inside `$(( ))`, stay SB4020;
  - v7, `$m` on a map, stays SB5064;
  - the `t` twins are untouched.

Every SubscriptAssign spelling was rejected before this unit (p5, v3, v1,
v2), so no spec pins that variant. Each Subscript row is a spec.

## Specs

arithmetic.sh sx.36-sx.40 (plan 479 → 484):

| Spec | Probe |
|---|---|
| sx.36 | p1 |
| sx.37 | p2 |
| sx.38 | p3 |
| sx.39 | v5 |
| sx.40 | v6 |

Each asserts SB4030 together with its distinguishing text, because SB4030
is raised at many sites ("A diagnostic CODE is not a site"). p4 is left
out: sx.36 already pins a string callee under a `$(( ))` subscript, and
sx.40 an intrinsic one. as.4's comment named the deleted
`walk_arith_intrinsic_calls`; it now names `ir::walk_arith_funccalls`.

Negative control on the pre-F5 binary, both modes: 479 ok, and red exactly
sx.36-sx.40.

Reversion stub (a) makes `walk_arith_nodes` return at a `Subscript` or
`SubscriptAssign` without descending. Red in both modes: sx.36-sx.40 and
as.4. as.4 is SB5012's subscript guard, so the stub also shows that
validate now depends on the shared walker.

## The gate

`NEW=10 mk gate` passed (rc 0):
- static checks clean;
- sweep rows 6492383-6502667: 10285 = native 5253 + c 5032, which is
  10272 pass + 13 skip;
- first-ever ids: exactly sx.36-sx.40 in both modes;
- 0 flips, 0 retired, 0 ghosts;
- liveness 260.

## Ledger

Rows ported:
- H066's F5 half (B37's half was ported in journal 538, so the whole row
  is now ported);
- H070: the `children` doc;
- H244: type_check's walker;
- H247, H248, H253, H254: validate's callers and its deleted copy;
- H306: `flag_collection_arith_args` and `collection_dollar_var`.

Finding B73 is ticked.

## Backlog

`bug-sexpr-call-slot-arity-unchecked` (P1). A slot function called only
from a sexpr is never checked against its slot. With
`fn f -> int { int n=$(len A); return $n }`:
- `echo $(( (f) ))` is a native ICE (`undefined arrlen: A`), and C prints
  garbage (v4t);
- `$(( a[(f)] ))` gives the same ICE, and C prints `0` (v4).

The command and capture forms give SB5066. validate's `check_slot_arity`
visits only `StmtKind::Command` calls. The bug reproduces on base, on the
branch and on this tree. That function is not one this unit touches, so
the 2026-10-26 rule sends it to the backlog. A sexpr cannot pass `=x`
(`$(( (f =x) ))` is SB4020, v8), so the fix's advice must say to capture
first.

`sweep-private-duplicates-of-shared-walkers` gains a section for B53
(journal 543) and B73. The done task `bug-sb5012-blind-to-arith-status-reads`
gets a one-line pointer to the walker that replaced the one it fixed.

## Narrative

This unit has the same shape as B53 (journal 543) and B37 (journal 538). A
private copy of a walk drifted from the shared one, and the fix deletes the
copy. This time the drift came from a fix: SB5012's repair reached
validate's copy, and type_check's twin kept the wildcard. Three walkers
are now one, and its descent is `children`, which the totality check
guards.
