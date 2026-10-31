# 550 - `&&` and `||` inside `[[ ]]` short-circuit

2026-10-31. F8 (Order item 14): batch-531 B87 and B88a, ported as-is from
the branch. It asks DG nothing. The one question the plan carried, the
`--trace` block labels, is settled by DD32, so it and four smaller items
are decide-and-report.

## The bug

DD95 gives `[[ ]]` bash's `&&` and `||`: the right operand is evaluated
only when the left one has not decided the result. Two layers ignored that.

- **The transform (B87).** `handle_test_command` lowered the whole `[[ ]]`
  as one `Test` instruction and lifted every operand capture ahead of it.
  In `[[ $x == a || $(f) == b ]]`, `f` ran, and wrote its globals, even
  when `$x == a` had already decided the test.
- **Both backends (B87, B88a).** The native emitter combined a `Test`'s
  And/Or operands with `band`/`bor`, and the C emitter with `&`/`|`. Every
  operand's value was therefore computed, including one that aborts: a
  right-hand `${y:?unset}`, a zero divisor, or `$(arg 5)` past the
  argument count. An `attest` predicate keeps its `[[ ]]` as one
  instruction, so it stays eager even once the transform is fixed. That
  half is B88a.

## The fix (ported as-is)

- **Transform (H223/H224).** `handle_test_command` returns
  `lower_test_tree`. A test without a junction stays a single `Test`. A
  junction becomes a branch:
  - the left operand's `Test` ends its block;
  - `test.and.right` / `test.or.right` lowers the right operand;
  - `test.and.merge` / `test.or.merge` joins the two paths with
    `insert_phis_two_branch`;
  - a negated junction gets a `NegateTestResult` after its merge.

  Each operand's captures are lifted into that operand's own block.
- **Native (H033/H034).** `lower_test_junction` in `codegen/tests.rs`
  evaluates the right operand in its own block. That block is reached only
  when the left operand has not decided.
- **C (H054).** `lower_test_junction` in `emit_c.rs` assigns the junction's
  result through a guarded `i64` temporary, so the right operand's
  statements run only when needed.

DD95 and DD93 each gained a 2026-10-31 addendum: the first for the
`[[ ]]` statement and condition forms, the second for attest predicates.

## Measured

**Negative control.** The binary was saved from the pre-F8 tree
(af2f1b02).
- Red in both modes: lazy.1/.2/.3/.4/.6/.7/.11 and atlazy.2/.3/.4/.7.
- Red: tr.5 (trace.sh is native-only).
- Pass: the controls lazy.5/.8/.9/.10.

**Reversion stubs, one per layer.**
- (a) Transform: `handle_test_command` back to a single `Test`. Red:
  lazy.4 and lazy.6 in both modes, plus tr.5. Only the capture specs go
  red: the backends' junctions keep `${y:?}`, the divisor and `$(arg 5)`
  lazy inside a single `Test`. taint.sh stays green.
- (b) Native: the And/Or arms back to `band`/`bor`. Red: atlazy.2/.3/.4/.7,
  native only. control_flow.sh, trace.sh and C mode stay green, because
  the transform split keeps every plain test in the specs lazy. Only
  attest predicates reach this junction.
- (c) C: the arms back to `&`/`|`. Red: atlazy.2/.3/.4/.7, C only.

lazy.1/.2/.3/.7/.11 go red only when both layers are gone, as on the
pre-F8 binary; either layer alone keeps them lazy. Each layer is still
pinned by specs of its own: lazy.4/.6 and tr.5 the transform, and the
atlazy ids each backend.

## Decide-and-report

1. **Trace labels.** `--trace` now prints `+ branch →` lines for
   `test.or.right`, `test.or.merge`, `test.and.right` and `test.and.merge`.
   DD32 says every branch traces, so these lines are new output, not a
   regression. tr.5 pins them, and DD32's spec range now reads
   `tr.1`-`tr.5`.
2. **Two specs quoted, ids kept.** The branch's lazy.2 had a bare
   `$(( 10 / z ))` operand, and lazy.9 bare `$i`/`$j` counters. After D1, a
   whole-int operand is SB4030 (DD30). Quoting keeps each spec's purpose: a
   zero divisor that must not be evaluated, and a counter compared as text.
   The ids stay, as in F14, F1+F4 and D1.
3. **atlazy.1 is deferred to F10.** Its operand `$(( 1 / z ))` is SB4030
   after D1. Quoting it would pin `$(( ))` inside an attest predicate,
   which is one of F10's open questions. atlazy.5/.6 (B88d) and atlazy.8/.9
   (B91) belong to F10 anyway. taint.sh's block header names all five, and
   findings §16 carries atlazy.1.
4. **Findings correction.** B88a's specs are atlazy.1-4 and .7, not
   "1-7"; .5/.6 are B88d's.
5. **The `lower_short_circuit` sibling.** The plan listed "lower_short_circuit
   phis" as an F8 sibling. The source is a 2026-10-21 ledger note: the
   function "needs phis", unverified. F8 does not touch that function
   (`transform/statements.rs`), so under the 10-26 rule it is not F8's to
   fix.

   One probe on the F8 tree found no symptom:
   - `x=1; /bin/false && x=2; echo "$x"` prints 1 in both backends;
   - so do its `||` twin and an int twin;
   - a tainted `x` stays SB5020 after either list form.

   `--no-taint` turns off both taint layers, so the SSA backstop cannot be
   observed on its own. With no repro, no backlog task is filed, and the
   note stays unverified.

## Specs

- **control_flow.sh lazy.1-11** (plan 114 -> 125). Verbatim from the
  branch except lazy.2 and lazy.9 (item 2).
  - Negatives: `${y:?}` in lazy.1/.3/.7, a zero divisor in lazy.2, a
    capture that writes a global in lazy.4, a negated junction in lazy.6,
    and `$(arg 5)` in lazy.11.
  - Controls:
    - lazy.5: an undecided left operand runs the capture, and its write
      survives the merge;
    - lazy.8: `$?` and nesting;
    - lazy.9: loop conditions;
    - lazy.10: a predicate function and a captured condition.
- **taint.sh atlazy.2/.3/.4/.7** (plan 394 -> 398), verbatim.
- **trace.sh tr.5** (plan 4 -> 5), new (item 1).

The lazy and atlazy ids already have rows from the branch sweep, and
Step 0 retired them, so they are not first-ever. tr.5 is the only new id.

## Gates

- The hook is green, and `mk test` returns 0 (units 238/27/20).
- `mk gate-static` passed before the sweep.
- `NEW=1 mk gate` passed on its first run.
  - Rows 6575231-6585670: 10440 = 10427 + 13 skip (native 5331, c 5109), as
    predicted (10409 + 22 + 8 + 1).
  - One first-ever id (tr.5); no flips, retirements or ghosts.
  - Liveness 260.

## Ledger

- H033/H034/H054/H223/H224 are ported.
- H321/H322 (control_flow.sh) and H329 (taint.sh) also carry F9's dsplazy
  and F10's atpred/atlazy specs, plus W1's atslot. Their F8 parts are
  ported; the rows stay pending for the rest.
- F8 has no pending rows.
- Findings: B87 and B88a are ticked.

14 of 20 Order items are done. Next is F9 (dispatch, B86/B93), on DG's go.
