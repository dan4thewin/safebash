# 551 - A dispatch evaluates lazily, in source order

2026-10-31. F9 (Order item 15): batch-531 B86 and B93, ported as-is from
the branch. It asks DG nothing. The plan carried one simplification
candidate (no captures in a dispatch) and one question (the capture-hoisting
order, shared with F5). Both are settled without a new decision, so they and
five smaller items are decide-and-report.

## The bug

DD66 says a dispatch's arms are tested top to bottom and the first match
wins. That held for the arms' bodies, but not for the patterns' tests.

- **The transform (B86).** `lower_case` built every pattern's test up
  front, before lowering any capture in it. A `$( )` in the subject or in a
  pattern therefore reached codegen unlifted, and both backends ICE'd. A
  function capture gave "FuncCall (f) should have been lifted during
  transform", and an external one, under `--no-taint`, gave "non-intrinsic
  command substitution must be lifted to a pipeline". Without the ICE, every
  pattern would have run, including those after the matching arm.
- **The validator (B93).** `taint_case_arms` checked the subject and each
  arm's body, but never the patterns. A tainted `"$x"` pattern was caught
  only by the SSA backstop, which reports no code and no position. An
  external `$( )` pattern was caught only by the ICE above.

## The fix (ported as-is)

- **Transform (H192-H196).**
  - `lower_case` creates `dispatch.merge`, then calls `dispatch_subject`.
  - The subject is evaluated once. When any pattern is dynamic and the
    subject is not a literal, it is copied to `__dispatch_subject_N`, a
    name reserved through `reserve_temp_name` (DD125).
  - `test_arm_patterns` tests an arm's patterns in turn. Each pattern's
    captures are lifted into the current block, and its `Test` ends that
    block.
    - A hit branches to `case.{i}`.
    - A miss branches to the arm's next pattern (`case.{i}.pat.{j}`), to the
      next arm (`case.check.{i}`), or to `case.nomatch`.
  - `lower_dispatch_arm` joins an arm's entry paths with phis (`join_paths`),
    lowers the body, and jumps to the merge.
  - A default arm is reached by a jump from the last miss. With no default,
    the no-match path sets `$?` to 0 and jumps to the merge.
  - `insert_case_phis` keeps its DISPATCH_HAS_TWO_PATHS ICE. It loses its
    default-path parameter and joins through `join_paths`.
  - The eager chain's eleven helpers are deleted: `build_case_bodies`,
    `build_check_blocks`, `wire_case_exits`, `build_dispatch_chain`,
    `emit_dispatch_arm`, `is_default_arm`, `emit_single_pattern_branch`,
    `emit_multi_pattern_chain`, `emit_pattern_link`, `wire_no_match_path` and
    `redirect_merge_false_branches`.
- **Validator (H281, F9's part).** `taint_case_arms` checks every pattern
  with `check_expr_taint`, which reports SB5020 at the pattern's position.

DD66 gained a 2026-10-31 addendum, and its Lowering section a pointer to
it.

## Measured

**Negative control.** The binary was saved from the pre-F9 tree (252f8f08)
and run in both modes.
- control_flow.sh: all of dsplazy.1-15 are red. dsplazy.1-12 hit the
  function-capture ICE, and dsplazy.13-15 the external-capture one.
- taint.sh:
  - dsptaint.1 is red: only the backstop catches it, with no code.
  - dsptaint.2 is red: it hits the external-capture ICE.
  - dsptaint.3, the attested control, passes.

**Reversion stubs, both modes.**
- (a) No subject copy. dsplazy.6/.7 are red; dispatch.sh and taint.sh stay
  green.
- (b) The pattern loop removed. dsptaint.1/.2 are red, each caught only by
  the backstop ("tainted variable … used as a test operand without attest
  (SSA taint backstop)"). control_flow.sh and dispatch.sh stay green.
- (c) Every pattern capture lifted at the dispatch's entry as well. The
  stub kept each pattern's own lift, so a reached capture ran twice.
  - Red: dsplazy.2, .3, .4 and .13.
  - In .2, .4 and .13, a capture in a pattern that is never reached ran.
    .2's stderr held one `tried` and .4's three, and .13 aborted with rc 3.
  - .3 went red only because its reached capture ran twice (`tried tried`).
    That is an artifact of the stub; .3 is a control and pins no laziness.
    These values come from the recorded xtrace (`spec/.trace`), not a
    re-run.
  - The DD66 addendum, written before the trace was read, said ".2, .3, .4
    and .13". It now says .2, .4 and .13.

The other dsplazy specs are red on the pre-F9 binary through the ICE
alone. No stub reverts the property each one pins without bringing the ICE
back.

**B93's recorded shape.** The findings entry's external capture inside a
pattern's subscript (`[[ $s == ${m[$(cat f)]:-d} {…} || {…} ]]`) has no
spec. Probed once after the change, it is SB5020 at 3:10 with bare `cat`
and with `/bin/cat`, as on the branch. The pre-F9 binary ICE'd on both.

**Trace.** Two traced probes print the same lines on the pre-F9 and new
binaries:
- one with static patterns, an alternative, a default, and a dispatch
  without one;
- one with a dynamic `"$p"` pattern.

The labels are `case.0`, `case.0.pat.1`, `case.check.1`, `case.1`,
`case.check.2` and `case.nomatch`. `dispatch.merge` is reached by a jump, so
it never prints. Both backends still treat it as the catching-construct
exit that resets `$?`. A capture pattern cannot be compared, because it
ICE'd before.

## Decide-and-report

1. **Ported as-is.** The plan's simplification candidate was "no captures
   in the dispatch subject or patterns". It would drop the subject copy and
   most of dsplazy. It is not taken, because it would change the language:
   DD66 makes a pattern an operand word, and a capture is legal in every
   other operand. The ICEs it would avoid are obvious bugs, which the 10-24
   rule fixes. F8 kept captures in `[[ && || ]]` operands for the same
   reason.
2. **Capture-hoisting order stays in the backlog** (q-capture-hoisting-order).
   Laziness across patterns is settled by DD66's top-to-bottom rule and by
   DD95's premise that an operator behaves the same inside `[[ ]]`. Within
   one pattern, its captures still run before its other reads, as in an
   argument list. The findings entry and the q-task both said that dispatch
   and `[[ && || ]]` were eager on hand-parser. F8 left the `[[ && || ]]`
   half stale; both halves are corrected now.
3. **Two specs quoted, ids kept.** dsplazy.7 (an int subject) and dsplazy.9
   (an int-function subject) quote their operand, as D1 requires (DD30;
   DD66's 2026-10-25 addendum).
4. **One unit test deleted.** `no_match_rewiring_only_changes_matching_false_edges`
   tested `redirect_merge_false_branches`, which H193 deletes. The branch
   deletes the test in H197, which the ledger filed under F1 as dropped. It
   lands here, with its subject. Units go from 238 to 237.
   `a_single_path_dispatch_is_a_loud_ice` (H196) still pins the
   DISPATCH_HAS_TWO_PATHS ICE.
5. **The taint check is described as sound and conservative.** Patterns are
   checked against the taint set from before the dispatch.
   - It is sound because SB5067 forbids a capture body from leaving a
     global newly tainted, so no pattern can read taint that the set lacks.
   - It is conservative because a capture that clears a global's taint gets
     no credit for later patterns.
6. **No W1/W2 code.** The branch's `taint_case_arms` doc names
   `taint_evaluations` (W1's walker), and its loop passes `exec_fns` (W2's
   analysis). F9 ports the loop with base's three-argument
   `check_expr_taint`, and a doc without either.

## Specs

- **control_flow.sh dsplazy.1-15** (plan 125 -> 140), after lazy.11.
  Verbatim except dsplazy.7 and dsplazy.9 (item 3).
  - Laziness:
    - .2: a pattern after the match never runs;
    - .4: the same for alternatives within an arm;
    - .13 (`--no-taint`): a failing capture that is not reached does not
      abort.
  - Controls:
    - .3: a reached pattern runs and selects its arm;
    - .14: a reached failing capture aborts with rc 3.
  - The subject:
    - .1: it is evaluated once;
    - .6/.7: a pattern capture that rewrites its variable does not move it;
    - .8: a capture in it, under each operator and inside a word;
    - .9: an int-function subject;
    - .10: a literal subject;
    - .15 (`--no-taint`): an external subject runs once, and a suppressed
      one is a value.
  - Effects: .5, a pattern capture's write is visible in its arm, in the
    default and after the dispatch.
  - Shapes: .11 has no default; .12 is in a loop and in END.
- **taint.sh dsptaint.1-3** (plan 398 -> 401), verbatim, after atlazy.7.
  - .1: a tainted `"$x"` pattern is SB5020 at 2:9.
  - .2: an external capture as a pattern is SB5020 at 2:23.
  - .3: control; the attested pattern compiles and prints `P a`.

Every id already has rows from the branch sweep, which Step 0 retired, so
none is first-ever.

## Gates

- The hook is green, and `mk test` returns 0 (units 237/27/20).
- `mk gate-static` passed before the sweep.
- `TOTAL=10476 mk gate` passed on its first run.
  - Rows 6585671-6596146: 10476 = 10463 + 13 skip (native 5349, c 5127), as
    predicted (10440 + 2 × 18).
  - No first-ever ids, flips, retirements or ghosts.
  - Liveness 260.

## Ledger

- H005/H006 (DD66) and H192-H196 (control_flow.rs) are ported.
- H193's note said "insert_case_phis removed". The branch keeps
  `insert_case_phis` and drops only its default-path parameter; the hunk
  deletes the five chain helpers. The note is corrected.
- H184 (control_flow.rs imports): F9's part is ported (`expr_is_dynamic`,
  `lift_int_func_calls`, `lift_test_captures`). The row stays pending: the
  `resolve_args_name` import goes with F10's `push_if_attest_condition`.
- H197 stays dropped; its test deletion landed here (item 4).
- H281 (validate.rs): F9's part is ported, and the row stays pending for
  W2.
- H321/H322 (control_flow.sh) are ported: F8 took lazy.1-11, and F9
  dsplazy.1-15.
- H327/H329 (taint.sh): F9's part is ported, and the rows stay pending for
  F10, W1 and W5.
- Every F9 part is ported. The four rows still pending (H184, H281, H327,
  H329) are split rows that wait only for F10, W1, W2 or W5.
- Findings: B86 and B93 are ticked.

15 of 20 Order items are done. Next is F10 (attest predicates: SB5098, B89,
B36, B88c/d, B91, B92, and the atlazy.1 carry-over), on DG's go.
