# 552 - An attest predicate runs no code

2026-10-31. F10 unit A (Order item 16): batch-531 B88b (the new rejection
SB5098), B89 and B36, with B32 subsumed. The discussion asked DG what a
predicate may hold. DG: "the result of $(<file) itself is tainted I would
have thought, and what use is echo in a predicate? with the possible
exception of some of the value intrinsics, I don't see how $( ) is
tenable." I recommended the strict rule, with no exceptions; DG ruled
"strict rule; file P1". The P1 is notes/tasks/bug-regex-line-anchors.md:
`^` and `$` match at line boundaries, so `^[a-z]{1,32}$` blesses
`$'abc\n;rm -rf x'`. It is filed, not fixed (the 2026-10-25 rule). Unit B
(B88c, B88d, B91 and B92's helper) follows on DG's go.

## The bug

DD24 says a predicate runs "No external commands. No arithmetic." Nothing
enforced it, and two reads escaped the predicate-reads rule.

- **B88b, code in a predicate.** The transform lowers a predicate's
  operands in place and has nowhere to run a body. A user function, an
  external, a statement builtin or a multi-statement body therefore died in
  a backend ICE, "non-intrinsic command substitution must be lifted to a
  pipeline": natively at value_handlers.rs:259, in C at emit_c.rs:3520.
  Spec 24.16, written for DD24's sentence, passed vacuously: on the pre-F10
  binary its program gives SB5020 at 2:1 and 3:6, which have nothing to do
  with the predicate, and the spec asserted only `-n $err`.
- **B89, an inline source.** `check_attest_predicate_reads` returned early
  whenever no name other than the attested one was tainted. So
  `attest x [[ $x == "$(getenv WANT)" ]]` and `[[ $x == "$(<allow)" ]]`
  compiled, and printed `[evil]` (discussion probes q1/q2). The gate
  compared the attacker's input against more of the attacker's input.
- **B36, a Part D import.** In `attest s (( n = ${t:-0} + s ))`, only the
  bare names were checked. A tainted `t` was caught only by the SSA
  backstop, which reports no source location.
- **B32.** `attest x [[ -n $x && $(ok "$x") == ok ]]` ICE'd
  (ssa.rs:2465).

## The fix (reimplemented)

The branch's SB5098 evaluated `echo` and some value intrinsics in place, so
its walk is reimplemented under the strict rule rather than ported.

- **validate.rs, the reads (B89, B36).**
  - `check_attest_predicate_reads` has no early return. A `[[ ]]`
    predicate goes through `check_test_taint`.
  - A Part D value goes through the new `check_arith_convert_reads`. Its
    bare names are the form's own operands: `walk_arith_var_reads` feeds
    `push_taint_use`, reported at the attest. Its `${…}` imports go through
    `check_arith_taint`, each at its own span.
- **validate.rs, the shape rule (B88b): `check_attest_predicate_code`.**
  - `run_semantic_checks` in lib.rs calls it beside the other shape rules,
    so it runs under `--no-taint` too.
  - It walks every block (`program_blocks`, `walk_stmt_tree`) and checks
    the attests that `classify_attest` decodes.
  - `predicate_code` and `predicate_node` report each banned node at its
    parser position, a nested one included. `capture_in_predicate` names
    what a capture runs, `convert_code` handles a Part D value, and
    `arith_call` decides whether a sexpr is a call.
  - The message is "attest predicate must not run code, but {what} — test
    the attested name with comparisons, regex matches and file tests only,
    and bind any other operand to a variable before the statement that
    taints the name". `{what}` is one of:
    - "`$(f …)` calls function `f`";
    - "`$( … )` runs something other than one plain command";
    - "`$(- …)` is a suppressed capture";
    - "`$(head …)` runs `head`", plus a suffix for the sed sugar;
    - "`$(<…)` reads a file";
    - "`$(( … ))` evaluates arithmetic";
    - in a Part D value, "its arithmetic calls function `f`" or "its
      arithmetic calls `strlen`".
- **Records.**
  - DD24 addendum: "a predicate runs no code (SB5098)". It replaces the
    branch's enforcement note, which cited a journal 531 that does not
    exist.
  - DD93 addendum: "two reads the predicate-reads rule missed" (B89, B36),
    which also settles atlazy.1.
  - DD63 addendum: the `s///` sugar in a predicate.
  - A GUIDE row for SB5098.

## Measured

**Negative control.** The binary was saved from 3b96bc40 as
/tmp/f10/sash-pre. 25 of the 27 ids are red in each mode. The two controls,
atpred.28 and .29, pass (`ok abc` rc 0; the `no` gate failure, rc 1).

**Reversion stubs, both modes.**
- (a) The capture arm removed: 24.16, atpred.1, .4-.8, .18-.20, .22, .26,
  .27, .30 and .31 are red.
- (b) The early return restored: atpred.13-15.
- (c) The `check_arith_taint` line removed: atpred.16.
- (d) `convert_code` disabled: atpred.3, .24, .25 and .31.
- (e) The SlurpFile and Arithmetic arms removed: atpred.17, .21 and .23.

**A regression the gate caught.** The first `arith_call` read a nullary
`(name)` as a call. Under DD58, a `(name)` that names neither an intrinsic
nor a function is grouping around a read. cov.277
(`if attest n (( c = n + -(d) ))`) and cov.278 (`(( c = n + (d) ))`) went
red in both modes at the first gate that ran. The guard is now
`is_intrinsic || functions.contains`. `(status)` and `(getcpid)` are
intrinsic sexprs, so they stay SB5098.

**B32.** Subsumed: SB5098 at 3:22, "`$(ok …)` calls function `ok`".
atpred.1 pins the class; B32 has no spec of its own.

**Index probes (/tmp/f10/idx).**
- `${a[(f)]}` is SB4041.
- `${#a[…]}` is SB4038.
- `${a[f]}` on a predicate's right-hand side (i2) is the existing P1
  bug-subscript-key-check-skips-positions, in a position its table did not
  list. Natively it ICEs at values.rs:1422. C reads the word as index 0, so
  the gate compares against `${a[0]}`. Both binaries agree. The P1 gains
  the position as row pa1; the fix stays with it.

**Two pre-existing message bugs, both on the pre binary.** Each is filed
P3.
- `s=$(arg 1); attest s (( n = (strlen s) ))` reports SB5054, which is
  right, and SB4020 "inside `$(( … ))`", in a program with none.
  `attest_arith_predicate` exempts only predicates that decode, so
  `check_arith_value_mutation` sees the near-miss
  (bug-sb4020-text-on-attest-near-miss).
- `s=abc; echo "${s:0:$(( 1 ))}"` gets SB4030 quoting "`$ 1 `".
  `try_substring_op` strips every paren from the operand
  (bug-substring-operand-message-strips-parens).

## Decide-and-report

1. **The sed sugar is SB5098**, and the message names both spellings. DD63
   makes `${x:s/…/…/}` the `$(sub …)` call, and the desugaring leaves no
   mark in the tree.
2. **Each banned node is reported separately**, a nested one included, at
   its parser position. Inside an index or a quoted word that is the `${`
   or the `"`; in a Part D value it is the attest.
3. **getenv and `$(<file)` get two codes**, SB5020 and SB5098. They are two
   rules: one about taint, one about shape.
4. **24.16 keeps its id and program**, and its assertion is tightened to
   SB5098 at 2:13 naming `echo` (the F14 precedent). Its comment cites
   atpred.1, .3-.8 and .13-.31.
5. **Fresh ids .17-.31.** The branch's atpred.2 and .9-.12 pinned
   evaluation in place, so none is ported. The programs of .2 and .9-.11
   are rejected at .17-.19. .12 checked that its captures decide the gate
   at run time; no capture reaches run time now, so it is dropped.
   atlazy.1 is dropped too, settled by atpred.17.
6. **`=~` stays legal.** It is a test, which DD24 names (atpred.28).
7. **The branch's `predicate_env` helper (H284) is inlined.** Its other
   use is W1's walker.
8. **The `taint_attest` split (H287/H288) is dropped.** It is
   behaviour-neutral, and the complexity gate did not demand it.
9. **The `arith_call` regression** was found by the gate and fixed (above).
10. **Three backlog tasks.** One is the P1 regex anchors task (DG's
    "file P1"). The other two are the P3 message bugs above, both
    pre-existing.

## Specs

taint.sh plan 401 -> 427 (+26):
- **24.16**: the assertion is tightened; the program is unchanged.
- **Verbatim from the branch (11)**:
  - atpred.1: a user function;
  - atpred.3: a bare int-function name in Part D;
  - atpred.4: a multi-statement body;
  - atpred.5: a suppressed capture;
  - atpred.6: a statement builtin, in END;
  - atpred.7: the rule under `--no-taint`;
  - atpred.8: a pipeline body;
  - atpred.13-15: an inline source in the statement and `if attest` forms
    (B89);
  - atpred.16: a Part D `${…}` import (B36).
  These have rows from the branch sweep at e98fca8 (6276924-6286458), so
  none is first-ever.
- **New (15)**:
  - atpred.17: `$(( ))`, even quoted, inside a function body;
  - atpred.18/.19: value intrinsics, including one that reads only the
    attested name under `--no-taint`;
  - atpred.20: the `if attest` form;
  - atpred.21: `$(<file)` under `--no-taint`;
  - atpred.22/.23: a capture and arithmetic in a map key;
  - atpred.24/.25: Part D sexpr calls of a function and an intrinsic;
  - atpred.26: the `gsub` sugar;
  - atpred.27: the attest-all form;
  - atpred.28/.29: controls (legal forms; the gate still decides);
  - atpred.30: an attest inside a capture body;
  - atpred.31: the sugar in a Part D import.

A block diff against `git show e98fca8:spec/taint.sh` confirms the 11
verbatim ids.

## Gates

- **First attempt (gate.log).** go-mk deadlocked without `-p 1` (rc 2).
  That is the runner, not a gate verdict.
- **Second attempt, `NEW=30 TOTAL=10528 mk -p 1 gate`.** It FAILED, on
  cov.277/.278 in both modes (4 failing, 4 flips): the `arith_call`
  regression. Rows 6596147-6606674.
- **Third attempt, `NEW=0 FLIPS=4 TOTAL=10528 mk -p 1 gate`.** It PASSED
  on its first run.
  - Rows 6606675-6617202: 10528 = 10515 + 13 skip (native 5375, c 5153).
    That is 10476 + 2 × 26.
  - The four flips are exactly cov.277/.278, failing to passing, against
    the second attempt's rows.
  - No first-ever ids, retirements or ghosts. Liveness 260.
  - Against the F9 block (rowid ≤ 6596146): 0 flips, and 30 first-ever ids,
    atpred.17-.31 in both modes.
- The hook is green, and the units are 237/27/20.
- After the gate, the only non-notes change is the 24.16 comment fix (no
  code).

## Ledger

- H072 (lib.rs) is ported.
- H001 (GUIDE), H004 (DD24), H007 (DD93) and H328 (24.16) are
  reimplemented.
- H284, H287 and H288 are dropped (items 7 and 8).
- H285, H327 and H329 are split rows. They stay pending, with notes for
  unit B, W1, W2 and W5.
- 16 F10 rows are pending, all unit B's: H014-H017, H042-H045, H148, H168,
  H185-H188, H225 and H226.
- Findings: B32, B36, B88b and B89 are ticked. The atlazy.1 carry-over is
  settled.

F10 unit A is done. Unit B is next, on DG's go: B88c, B88d, B91, B92's
helper, atonce.1-2, atlazy.5/.6/.8/.9 and H184.
