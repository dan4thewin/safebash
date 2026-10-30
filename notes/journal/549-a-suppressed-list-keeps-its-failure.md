# 549 - a suppressed list keeps its failure

2026-10-30. F6 unit C (Order item 13): batch-531 B84. Like unit B, it asks
DG nothing. Its one decision is decide-and-report: 55.88 is not ported.
Two new questions went to the backlog (DG 2026-10-30).

## The bug

`- /bin/false || /bin/false` exited 0. The same list followed by
`echo done` printed `done`, so the right side's failure was lost twice:
once to the `-` (correctly) and once more to the program's exit status
(not correctly).

A statement-level `-` covers the whole list. `--emit-ir` shows
`- a || b` as `Suppressed(List(or, a, b))`, so b runs unchecked. After a
`||`, `reset_or_status_at_top_level` (transform/statements.rs) sets `$?`
to 0 unless the list is `caught`. That reset assumes a right side that
completed has passed its check. Under `-` it has not. `caught` read only
`state.in_condition`, so the reset ran and turned b's failure into 0.

The fix is the branch's H219: `caught` also holds under
`state.suppressed > 0`, which is unit B's depth counter. A suppressed list
consumes its status the way a condition does. `lower_short_circuit` then
neither resets `$?` nor re-checks the right operand, and the program ends
with b's status, as a lone `- /bin/false` does (rc 1 on both binaries and
backends).

## Measured

Everything below was run with `--no-taint`, on 825fea9f (before) and this
tree (after). Native and C agree on every row.

| probe | shape | before | after |
|---|---|---|---|
| r11 | `- /bin/false \|\| /bin/false` | rc 0 | rc 1 |
| r04 | `fn f { /bin/echo in; return 1 }; - /bin/false \|\| f` | `in`, rc 0 | `in`, rc 1 |
| r05 | `- /bin/false \|\| /bin/false \| /bin/cat` | rc 0 | rc 1 |
| r09 | `- /bin/false \|\| /bin/false \|\| /bin/false` | rc 0 | rc 1 |
| q02 | `- { /bin/false \|\| /bin/false }` | rc 0 | rc 1 |
| q13 | `- x=$(/bin/false \|\| /bin/false)` | rc 0 | rc 1 |
| q16 | `- { f; /bin/false \|\| /bin/false }`, f returns 0 | rc 0 | rc 1 |
| r10 | `- /bin/false \|\| /bin/true` (control) | rc 0 | rc 0 |
| r01 | `fn f { /bin/echo in; /bin/false }; - /bin/false \|\| f` | aborts in f | aborts in f |
| r07/r08 | `&&` lists | rc 1 | rc 1 |

r04's rc 1 matches a plain `- f` on the same f (s3: prints `in`, rc 1).
In r01, the abort happens inside f. The new `caught` path unchecks only
the right operand's own condition instruction (`uncheck_last_condition_inst`).
It does not reach an inlined body, so unit B's rule holds. `&&` has no
reset to skip, so r07/r08 were already right.

B84 has three routes on HEAD: the statement list, `- x=$(a || b)` (R1 in
`q-suppression-reach-residuals`), and a group (R5, slated for retirement).
H219 fixes all three, because they share `lower_list_stmt`. The specs use
only the statement route, so that they cement neither open question.

## 55.88 is not ported (decide-and-report)

55.88 is the branch's spec for B84. It reaches B84 through a function
stage of a capture condition. Its premise is that the `-` reaches into the
stage's body, and DG's Q1 ruling (2026-10-29: no reach) removed that
premise. The no-reach form of the same shape is already pinned by
noreach.6: the function stage fails, so the condition is false. The
H324/H325 ledger rows record the F6 part as dropped.

## Specs

errors.sh 55.145-55.147 (plan 721 -> 724). The ids are above the spec.db
high-water mark (55.144), and both collision checks are clean.

- 55.145: `- /bin/false || /bin/false` exits 1, silently.
- 55.146: a status function's `return 1` as the right side exits 1 after
  printing `in`.
- 55.147 (control): `- /bin/false || /bin/true` exits 0.

Negative control: on 825fea9f, 55.145 and 55.146 are red in both modes and
55.147 passes. The reversion stub for this one-line change is the same
tree, so the negative control doubles as it.

## Backlog (decide-and-report: questions, not bugs)

Two new items in `q-suppression-reach-residuals`:

- R6. A statement `-` binds the whole list (`- a || b` = `-(a || b)`),
  while a pipeline-head `-` binds stage 1 only. Should it bind the first
  operand only?
- R7. SB4076 does not look through a list. `fn f { /bin/echo in; /bin/false }`
  makes f void, and `- f` is SB4076, yet `- /bin/false || f` compiles. The
  `-` is dead there either way: `||` consumes a's failure, and a void f
  cannot fail. Which diagnostic is right depends on R6, so no spec pins
  the form yet.

## Gates

Standalone errors.sh: 724/724 in both modes.

`NEW=6 mk gate` passed on its first run, covering `mk test`, the static
checks and one full sweep.
- Rows 6554415-6564821: 10407 = 10394 + 13 skip (native 5314, c 5093),
  as predicted.
- 6 first-ever ids; no flips, retirements or ghosts.
- Liveness 260.

The first standalone errors.sh attempt ran four copies in parallel, and
rdr.23-.30 and .47-.48 failed at random. Those specs share fixed
`/tmp/sash_rdr_*` paths, so concurrent copies delete each other's files.
That is not a finding about the tree: sweeps are serial by design. Run
serially, the same binaries gave the results above.
