# 538 - The walk resolution kept to itself

2026-10-27. B37 is Order item 9 of the batch-531 port. DG asked for a brief
status of the port and then said "go for B37". The branch's fix is ported
as-is. I added a unit test and a table row of my own, and a P1 task for a
neighbouring panic that the probes turned up.

## The bug

DD#23's resolution pass rewrites a bare external head (`cat`) to the
absolute path it will exec (`/usr/bin/cat`). The taint pass depends on that
rewrite: `validate::command_name_is_taint_source` calls a capture tainted
when its head is `getenv`, `argv` or `arg`, or contains a `/`. After
resolution only intrinsics, builtins and user functions keep a bare name, so
a bare head reads as clean. A command that resolution never reaches keeps
its bare head, and the checkers read it as clean.

`resolve.rs` reached `$( … )` bodies through an expression walk of its own
(`resolve_expr`, `resolve_each`, `resolve_test_expr`). The checkers use
`Statement::cmdsub_bodies` in `ir.rs`, which reaches a body in three places
the private walk does not:

- a parameter expansion's index, `${m[$(cat f)]:-d}`;
- the leaves of `$(( … ))`;
- a `(( … ))` statement or condition.

The program still ran the real `cat`: the transform resolves heads again as
it lowers (`transform/resolution.rs`). Only the AST checkers saw the bare
name; the SSA backstop, which checks the lowered program, caught 7 of the 16
shapes the specs below pin, without a position.

On base `1309385`, with `map m=([k]=v)` and a file holding `k`:

```
b=${m[$(cat b37.key)]:-dflt}
echo "$b"
```

compiled and printed `v`, a value selected by an unattested file read.

This is the shape of journals 441 and 456, and of
`notes/tasks/sweep-private-duplicates-of-shared-walkers.md`: two walks over
one tree, and only one of them reaching every body. Here the private walk is
the older one. `resolve_test_expr` came with DD#23's hoisting of resolution
(d4fdc63, June), and `Statement::cmdsub_bodies` with 93d4ac1 in September.
So nothing was forked: the shared walk is the one the checkers trust, and
`resolve.rs` never moved onto it. I added it to that task as a fourth
instance.

## The fix

As the branch wrote it (ledger rows H059, H061, H062, H067 and H071 in
`ir.rs`; H139, H142 and H143 in `resolve.rs`; the B37 half of H066):

- `ir.rs` gains `Statement::cmdsub_bodies_mut`, the mutable twin of
  `cmdsub_bodies`. It visits the same nodes in the same order, through
  `own_expr_roots_mut` (total over `StmtKind`), `push_cmdsub_bodies_mut`
  (total over `ExprKind`), `walk_arith_expansion_roots_mut` and
  `walk_test_operands_mut`. A callback that kept a mutable borrow of a
  node's body would leave the walk unable to descend into that node, so the
  twin is the recursion itself rather than a callback on a
  `walk_expr_tree_mut`. `cmdsub_bodies` now goes through a named
  `push_cmdsub_body`. The branch named it to share it with
  `DeclAssignment::cmdsub_bodies` (H069, dropped below); I kept the name so
  that the twin's doc can say which test it mirrors.
- `resolve_stmt` resolves every body from `cmdsub_bodies_mut`, then the
  command's own head, then the structural bodies. The private walk is
  deleted (`resolve.rs` +16/−105).
- `ir::cmdsub_twin_tests` pins the twin against the canonical walk: a table
  of statements, each parsed and walked both ways, comparing the bodies
  found by address and in order; one test for a parameter expansion's
  operand; and one that I added for the `SubscriptAssign` arm (below). The
  table is the branch's 19 rows plus one I added,
  `(( w[${v[$(a)]:-0}] = ${w[$(b)]:-0} ))`, whose two bodies put an
  arithmetic assignment's index and value in the comparison.

## What changes

- **Sixteen shapes are now rejected with a position.** On base, 9 of the 16
  capres negatives compiled and ran silently, and 7 were caught only by the
  SSA backstop, which names a synthetic variable (`__extcap_1_1`) and gives
  no position. All 16 now draw a positioned SB5020, or SB5021 in an array
  or map literal.
- **SB5095 is reported value-first.** When an indexed assignment's value
  and index both name a missing absolute program, the old walk resolved the
  index first (`resolve_assignment`); the shared walk visits the value
  first, as SB5011's check already did on every tree (probes o1/o2). The
  DD23 addendum records this.
- **Three ICEs become SB5020 with taint on.** p11 (a capture in a redirect
  target's subscript), w01 (in a dispatch subject's subscript) and w07 (in a
  pipeline stage argument's subscript) panicked on base in both backends
  (native `value_handlers.rs:259`, C `emit_c.rs:3430`). The taint pass read
  the bare head as clean, so compilation went on to a lowering that never
  lifts a capture in that position. Now SB5020 stops each one first, as on
  the branch. Under `--no-taint` all three still panic at the same two
  lines; those panics are not B37's, and are filed or owned below.
- **x02 and x04 change code, SB4030 to SB5020.** Each carries a string
  default inside `$(( … ))` (`(strlen ${m[$(cat f)]:-dflt})`), which DD#77
  D4 rejects (SB4030; c6 shows the same without a capture, on both trees).
  With the head now resolved, the taint check fires first. Both are
  rejections, so I wrote no spec for which of two faults is reported.

## Specs and proof

`spec/taint.sh` capres.1-20 (plan 374 to 394; 40 first-ever ids). Every
negative asserts `rc != 0`, empty output, and the full located diagnostic.
The keys are `map` keys: an indexed-array index may not be a capture
(DD#80, SB4030), so an indexed-array spec would pin SB4030 once
bug-subscript-key-check-skips-positions is fixed, not the taint fix.

| Spec | Where the capture sits | Base |
|---|---|---|
| .1 / .2 | an assignment / a declaration value | silent |
| .3 | a command argument | backstop |
| .4 | an assignment target's index | backstop |
| .5 | a `[[ ]]` operand | backstop |
| .6 / .7 | an array / a map literal's element (SB5021) | silent |
| .8 | a `$(( ))` operand | silent |
| .9 / .10 | an index inside `$(( ))`, nested once or twice | backstop |
| .11 | a `(( ))` condition | backstop |
| .12 | a `(( ))` statement | silent |
| .13 | a `(( ))` assignment target's index | backstop |
| .14 / .15 / .16 | in a function, a capture body, END | silent |
| .17-.20 | positives: a builtin, a function in `$(( ))`, a `(( ))` statement, `--no-taint` | pass |

The positives pass on base too; they pin that the fix leaves a builtin and a
function call alone, and that the resolved head runs.

**Negative control.** The block run against the base binary fails .1-.16
and passes .17-.20, in both modes. Compiling and running each negative alone
with the base binary gives the split in the table: the 9 silent shapes print
the selected value (or arithmetic on it), and the 7 backstop shapes fail
with no position, naming the synthetic `__extcap_1_1` (.11's message names
the internal `__let_exit` as the command it reaches).

**Reversion stubs.** Each stub deletes one arm of the twin (or reverts
`resolve.rs`), and I predicted the specs it would turn red before running
it:

| Stub | Removes | Predicted | Turned red | Unit test that fails |
|---|---|---|---|---|
| S1 | a parameter expansion's index | .1-.16 | .1-.16 | both mirrors |
| S2 | the `$(( ))` arm | .8-.10 | .8-.10, .12, .13 | table |
| S3 | the `(( ))` statement roots | .11-.13 | .11 | table |
| S4 | `resolve.rs` reverted to base | all 16 | all 16 | not run (the twin is intact) |
| S5 | `$(( ))` subscript indices | .9, .13 | .9 | table |
| S6 | `[[ ]]` operands | .5 | .5 | table |
| S7 | an assignment target's index | .4 | .4, .13 | table |

Every stub fails in both backends. The four mispredictions share one cause:
`parse/arith.rs`'s `arith_assign_stmt` and `lower_arith_mutation_structured`
lower a `(( x = … ))` or `(( w[i] = … ))` statement to an `Assignment` whose
value and index are `ExprKind::Arithmetic`, not to an `ArithTest`. So .12's
value and .13's index go through the `$(( ))` arm (S2); .13's index hangs
off the assignment's index root (S7), not an arithmetic subscript (S5); and
the `(( ))` roots (S3) see only .11's condition.

**The `SubscriptAssign` arm.** The same lowering means no program reaches
`walk_arith_expansion_roots_mut`'s `SubscriptAssign` arm. A
`(( w[1] = 5 ))` statement becomes an `Assignment`. Every other spelling is
rejected before resolution runs (`lib.rs:484`): as a condition, SB5007
(`if (( w[1] = 5 ))`); inside `$(( ))`, SB4020 from validate
(`$(( (w[1] = 5) + 1 ))`), which runs at `lib.rs:479`; and nested in a
statement `(( ))`, SB4020 from validate (`(( (w[1] = 5) ))`) or from the
parser (`(( x = (w[1] = 5) ))`, `parse/arith.rs:249`). Bindings ICEs on one
(`bindings.rs:281`). The arm stays because it mirrors the canonical walk's
arm and the walk is total. The new unit test
`cmdsub_bodies_mut_mirrors_a_subscript_assignment` builds the node
directly, and three stubs (the twin's value only, the twin's index only, the
canonical walk's value only) each fail exactly that test.

**Restore.** After the stubs, `ir.rs` and `resolve.rs` match the saved fix
by checksum, and the unit and spec runs pass.

## Decide-and-report

DG may veto any of these.

- **H069 dropped.** `DeclAssignment::cmdsub_bodies` has no caller on the
  branch: every `.cmdsub_bodies()` call there has a `Statement` receiver,
  and being `pub` it drew no dead-code warning. Its doc says it is for a
  pass that walks capture bodies in per-binding evaluation order, which is
  the branch walker's, and track W does not port that walker.
- **One correction to the branch's parameter-operand test doc** (H071). It
  said every operator but `:-`/`:?` is retired (SB4034), but the substring
  form is live: `${x:1:2}` prints `bc` in both backends. My doc says no
  parse puts a `$( … )` in an operand, because every operand the parser
  builds is literal text: `:-` and `:?` take a literal (a `$` there is
  SB4070, as `${x:-$y}` shows), substring offsets and a retired operator's
  text (`${x#a}`, `${x^^}`: SB4034) are kept verbatim, and `:s/…/`
  desugars to a nested `$(sub …)` in place of the expansion.
- **The F5 half of H066 is not ported.** `walk_arith_nodes` and
  `walk_arith_funccalls` belong to B73 and land with F5. The ledger row
  stays `pending` with the B37 half noted.
- **SB5095 value-first**, as above, recorded in the DD23 addendum.
- **Seven specs pin shapes base already rejected.** The plan named i2 and
  i9, which base printed. Of the 16 negatives, 9 were silent on base and 7
  were caught by the backstop. I kept the 7: the fix changes what they
  report (a positioned SB5020 instead of an unpositioned message about
  `__extcap_1_1`), and they are the only specs that stubs S3, S5, S6 and S7
  turn red (.11, .9, .5, .4 and .13).
- **Probes not made into specs.**
  - p11 goes to the P1 task below.
  - w01, a capture in a dispatch subject's subscript, belongs to F9. Under
    `--no-taint` it compiles on the branch and still panics here.
  - w02, a capture in a dispatch pattern's subscript, is B93 (F9). It still
    panics here with taint on; the branch says SB5020 at 3:10.
  - d1 and d2, a function capture as a dispatch subject or pattern, panic
    at `ssa.rs:2217` here (2030 on base); the branch prints `a` (F9, B86).
  - w07 is B81's stage argument and still panics under `--no-taint`. B81
    needs no subscript: under `--no-taint`, e2
    (`echo $(/bin/cat f) | /bin/cat`) panics in both backends on every
    tree, the branch included. With taint on, e1 (`echo $(g) | /bin/cat`,
    a function capture) panics at `ssa.rs` on every tree (2217 here). The
    findings index has the B81 re-probes.
  - p16 and p17 (`$(( ))` as a command argument, a `while (( ))`
    condition) reach the same twin arms as .8 and .11, and are SB5020 now,
    at 2:6 and 3:7. Base's backstop caught both, so neither adds a silent
    shape.
  - Absolute-head probes (`/bin/cat` for `cat`) were tainted on base
    already and pin nothing.
  - x02 and x04 are covered under What changes.

## Backlog

**notes/tasks/bug-redirect-target-index-capture-ice.md (P1).** A capture
inside a redirect target's subscript, `echo hi >${m[$(f)]:-d}`, panics the
compiler in both backends: `redirect_target_is_scalar` does not look at the
index, so SB4049 lets it through, and the target spill never lifts the
capture. It is pre-existing: base and the branch panic too.
The compiler's own SB4030 advice for `>$m[…]` (use `${m[…]:?message}`)
leads straight into it. The task offers (a) SB4049 for any `$( … )` in a
target, recommended per DG's 2026-08-30 ruling and DD#119, with arithmetic
subscripts staying legal; or (b) lifting the capture before the spill. Its
SB4049 wording is a question either way.

The findings index records the re-probes for B81, B86 and B93: unchanged
by B37 except that their taint-on ICEs (w01, w07) are now SB5020.

## Gates

- `mk test`: 237/27/20. The three new unit tests are in
  `ir::cmdsub_twin_tests`: the branch's table and parameter-operand tests,
  and my `SubscriptAssign` test.
- Sweep, rows 6421079-6431127: 10049 = 10036 passing + 13 skips (native
  5134; c 4902 + 13 skips), up 40 from 10009. Exactly the 40 predicted
  first-ever ids (capres.1-20 in both modes), no flips, no retirements; the
  can-fail query reports 260.
- The SB census is empty both ways; both collision checks are empty, and
  spec.db held no capres history; `plan` matches `chk` but for
  `coverage.sh`'s documented +2; the `plan` sums, native 5134 + c 4915,
  equal the summary's 10049.
- The pre-commit hook passes with no bypass. The means are cc 3.47 and
  cognitive 1.49, so the gate's 3× bar is 10.4 and 4.5:

| Function | cc | cog |
|---|---|---|
| `own_expr_roots_mut` | 13 | 1 |
| `push_cmdsub_bodies_mut` | 12 | 1 |
| `walk_arith_expansion_roots_mut` | 11 | 1 |
| `walk_test_operands_mut` | 5 | 1 |
| `resolve_stmt` | 4 | 3 |
| `cmdsub_bodies_mut`, `push_cmdsub_body`, `push_each_cmdsub_body_mut` | 2 | 1 |

  The three above cc 10.4 are flat dispatchers (cognitive ≤ 2), which the
  gate accepts as-is.

## Next

F2 (Order item 10), on DG's go. Still DG's: the F1+F4 rulings listed in
status.md, and the P1 task's choice between (a) and (b).
