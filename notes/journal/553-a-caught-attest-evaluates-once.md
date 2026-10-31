# 553 - A caught attest evaluates its predicate once

2026-10-31. F10 unit B (Order item 16): batch-531 B88c, B88d, B91 and
B92's shared helper. DG: "go". Unit A (journal 552) added SB5098; this
unit is the transform and backend half. It needed no DG question: every
fork was either settled or decide-and-report.

## The bug

Four bugs, all in how an attest is lowered when it is caught or covers a
collection:

- **B88c.** A caught scalar attest ran its predicate twice. The caught forms
  are `if attest x [[ … ]] { … }` and `attest x [[ … ]] || cmd`
  (`try_lower_attest_list` routes the second to `lower_if_attest`). The
  condition tested the predicate, and the pass block then ran an aborting
  `SsaInst::Attest` on the same value. A file test stat'd its path twice. If
  the second answer differed, the *caught* form aborted at the statement
  gate instead of choosing a branch.
- **B88d.** The `if` form of attest-all ANDed every cell's verdict, so each
  later cell's predicate still ran after a cell failed. A predicate that can
  abort (`${y:?unset}`) then aborted a program that should have taken its
  false branch. The statement form already stopped at the first failure,
  because it aborts.
- **B91.** C's statement-form attest-all printed a value clause. On pre, C ran
  60.17's program and printed `attest: a = '123' failed the gate`. Native
  prints `attest: a failed the gate`, which is DD#93's text.
- **B92.** A caught attest-all over a DD#100 slot checked the caller's
  collection, but retagged the literal slot name `A`. Natively that ICEd
  (`no codegen slot for A_3`, types.rs:72). If the body read the slot later,
  the SSA taint backstop rejected the read in both backends, in a program the
  validator had accepted.

## The fix (ported)

I ported 17 ledger rows from the branch by hand:

- **dispatch.rs, `emit_attest_all`** (H014-H017). Both forms branch per cell
  with `brif(result, cont, fail)`. The `if` form's fail block sets
  `all_passed = 0` and jumps to `after`. The statement form's fail block calls
  `emit_attest_fail`.
- **emit_c.rs, `emit_attest_check`** (H042-H045). An `if`-form failure is
  `t_all = 0; break;`. The statement form calls
  `sash_attest_fail(name, len, (const char*)0, 0)`, so it prints no value
  clause. The names parameter is gone.
- **stmt_handlers.rs** (H225, H226). `push_attest_all` is now a check followed
  by a retag. `push_attest_all_check` resolves the slot and takes an `if_form`
  flag. `push_attest_retag` calls `resolve_args_name`, then looks up or
  defines the version, then emits `AttestRetag`. Both are `pub(super)`.
- **control_flow.rs** (H184's F10 part, H185-H188). The private
  `push_attest_retag(kind, test, …)` fork is deleted. Its scalar arm re-ran
  the aborting Attest (B88c), and its collection arm retagged the unresolved
  name (B92). Three callers now use the shared helpers: `lower_if_attest`,
  `lower_if_attest_convert`, and the collection arm of
  `push_if_attest_condition`. `AttestKind::subject()` is new.
- **ssa.rs, ssa_taint.rs** (H148, H168). Doc comments only.
- **sash-rt** needs no change. `sash_attest_fail` already omits the clause
  when the value is null.

## Measured

- **Negative control** on pre (/tmp/f10b/sash-pre, the 621cf1c7 compiler):
  - native: atlazy.5/.6, atonce.1/.2 and atslot.7/.8 are red;
  - C: atlazy.5/.6/.8/.9, atonce.1/.2 and atslot.7 are red;
  - atslot.8 is C's control, because the ICE was native only. atlazy.8/.9
    pass natively on pre, because B91 is C only.
- **Five reversion stubs**, each matching its prediction:
  1. native `jump(after)` changed to `jump(cont)`: atlazy.5/.6 red, native
     only;
  2. C's `break` removed: atlazy.5/.6 red, C only;
  3. C's value clause restored: atlazy.8/.9 red, C only;
  4. the old aborting Attest restored in the pass block: atonce.1/.2 red, in
     both modes;
  5. `resolve_args_name` dropped from `push_attest_retag`: atslot.7 red in
     both modes, and atslot.8 and collparam 100.28 red natively.
- **B92 probes** (/tmp/f10b/b92). On pre, t1 (atslot.7's program) failed the
  SSA backstop in both backends: "tainted array a[...] used as an argument to
  'echo' without attest". t2 (atslot.8's program) ICEd natively and ran in C.
  The ICE text was re-read on pre today. The s1-s6 shapes show the same two
  symptoms. Under `--no-taint`, the backstop's shapes ICE natively as well.
- **Zero divisor.** The branch's comment on the native `if`-form fail block
  said a later predicate "can abort: `${y:?}`, a zero divisor". No division
  is reachable in a predicate: `${b[1/0]}` and `${b[1/z]}` are SB4041,
  `${v:1/0}` is SB4030, and `$(( ))` has been SB5098 since unit A. The
  comment now names only `${y:?}`. I made this comment-only edit after the
  gate started. The rebuilt binary is byte-identical to the gated one.
- **C's old text.** Besides 60.17's `a = '123'`, today's spec traces hold
  `attest: a = 'bad' failed the gate` and `attest: m = 'v' failed the gate`.
  These come from C runs of atlazy.8/.9 on pre and under stub 3.

## Decide-and-report

1. **atslot.7/.8 are new specs, not the branch's atslot.1-6.** Those six
   attest slot taint that the body received. The planned T3 rule rejects
   that, so W1 rewrites them. A body that fills its own slot and attests it
   stays legal under T3. Both new specs run with taint on.
2. **One retag helper for every form.** The statement form and the Part D
   convert form use the shared retag too. Neither changes behaviour. The
   statement form already resolved its slot, and the convert form's scalar
   resolves to itself. 100.28 pins the statement path (stub 5). Under T3,
   100.28 flips to SB5088, so W1 needs another statement-form pin.
3. **The stale comment is fixed.** It sat inside a function the port touches
   (the 10-26 rule).
4. **sash-rt is unchanged.**
5. **Six ids are kept verbatim.** atlazy.5/.6/.8/.9 and atonce.1/.2 match the
   branch's text. They have rows from the branch sweep at e98fca8
   (6276924-6286458), so they are not first-ever. atslot.7/.8 are fresh.

## Specs

taint.sh plan 427 -> 435 (+8):

- **Verbatim from the branch (6):**
  - atlazy.5/.6 (B88d): an array, and a map in insertion order;
  - atlazy.8/.9 (B91): an array and a map. Each asserts the collection's
    name and no `= '` clause;
  - atonce.1/.2 (B88c): the `if` and `||` forms. strace fails the second
    `newfstatat` of the tested file, and each spec asserts one stat.
- **New (2):**
  - atslot.7 (B92): the `if` form, with a read of the slot after the `if`;
  - atslot.8 (B92): the `||` form, with no later read.
- The DD#95 block comment above atlazy.2 no longer says that atlazy.5/.6/.8/.9
  are still to land.

A block diff against `git show e98fca8:spec/taint.sh` confirms the six
verbatim ids.

## Gates

- **One attempt, `NEW=4 mk -p 1 gate`. It PASSED** (/tmp/f10b/gate.log).
  - Every static check passed. The plan sums are native 5383 + c 5161 =
    10544.
  - The sweep took 239 s, rows 6617203-6627746: 10544 = 10531 + 13 skip.
    That is 10528 + 2 × 8.
  - The first-ever ids are exactly atslot.7/.8 in both modes (4).
  - No flips, retirements or ghosts. Liveness 260.
- The hook is green, and the units are unchanged at 237/27/20.
- After the gate, the only changes are the dispatch.rs comment and the notes.

## Ledger

- 17 rows are ported: H014-H017, H042-H045, H148, H168, H184, H185-H188,
  H225 and H226. H184's F9 part landed with journal 551.
- H327 and H329 (the spec rows) stay pending, with a note on unit B's part.
  Their W1 and W5 parts remain.
- H285 is unchanged, because unit B touched nothing in validate.rs.
- F10 has no pending rows of its own. Its three split rows wait for W1, W2
  and W5.
- Findings: B88c, B88d, B91 and B92 are ticked.

F10 is done. W1 is next, on DG's go.
