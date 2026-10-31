# 555 - A capture body is walked once, before its statement

2026-10-31. W1 unit a (Order item 17): batch-531 B29, B30 and the local
part of H282. DG: "proceed with the port work". DD127 (journal 554) set
the model W1 implements. W1 is split into three units (decide-and-report):

- W1a, this one, fixes the per-statement transfer functions;
- W1b adds function summaries (DD127's name channels, B115, 100.27/100.28);
- W1c applies the single-use rule and its spec rewrites.

This unit needed no DG question.

## The bugs

- **B29. A capture body was walked twice.**
  - `taint_stmt` walked every capture body in full (`taint_cmdsub_bodies`).
    It did so AFTER the statement's own arm, so the body saw the
    statement's own effect.
  - Shallow read checks also re-read the body wherever a capture can sit:
    - `check_cmdsub_use_taint` at arguments, operands and subscripts
      (`check_expr_taint`'s capture arm), and at `for` words;
    - `check_cmdsub_inner_reads`, through `reject_scalar_rhs_read`, for a
      right side;
    - `check_value_cmdsub_reads` at array and map elements;
    - `check_arg_default_taint` for an `arg` default, on top of the
      right-side check.
  - So a tainted read in a body was reported twice at the same position,
    and three times in an `arg` default.
  - Both walks ran after the assignment, so a body that read the name its
    own statement assigns saw that assignment's taint. That gave two false
    positives:
    - `y=hi; y=$(/bin/echo "$y")` was SB5020 at 2:15, twice;
    - `y=hi; y="$y$(/bin/echo b)"` was "tainted variable $y can't be
      copied — DD#24 has no taint propagation" at 2:3.
- **B30. A suppressed reader in a capture had no located report.**
  - `check_collection_reader_taint` had one caller, the shallow right-side
    check. That check matched only a body's top-level command, so it
    skipped `$(- join a ,)`.
  - The full walk checked only a command's words.
  - Only the SSA backstop caught it: "tainted variable y_1 used as an
    argument to 'echo' without attest (SSA taint backstop)". That message
    has no code and no position.
- **H282's local part.** The branch filed this under B29.
  - `taint_command` checked a call's words after `taint_out_params`, which
    taints the caller's collections that the callee fills from a source.
  - In `fn fill { argv A; echo "$1" }; array g=(x); fill "${g[0]}" =g`, the
    argument was therefore read after `g` was tainted: a false SB5020 at
    3:6.

## The fix (reimplemented)

The branch fixed all three inside its new walker (`taint_evaluations`). That
walker is not ported: W1's premise keeps base's per-statement transfer
functions. So I reimplemented the fixes on those functions, and reused the
branch's shapes where they fit.

- **One walk, before the statement.**
  - `taint_stmt` walks a statement's capture bodies before its own arm. The
    trailing walk after the match is gone.
  - Each body is still walked on a clone of the state, and
    `report_exit_tainted_globals` still runs for it
    (`taint_capture_bodies`).
  - `Statement::cmdsub_bodies` covers only a statement's own words. So a
    `while` condition or a loop body is still walked once per iteration of
    the loop's fixpoint, as its own statements.
- **A declaration interleaves.**
  - A Declaration is left out of that up-front walk.
  - `taint_declaration` walks each initializer's bodies just before it
    binds that initializer, using the new `Expr::cmdsub_bodies` in ir.rs.
  - So in `str a=$(getenv X) b=$(/bin/echo "$a")`, `b`'s body sees `a`
    tainted.
- **A clean-pipe reader walks its own bodies.** `taint_stmt_no_read_taint`
  does not go through `taint_stmt`. For a read, readline or mapfile under
  stage marks, it now walks the bodies before `check_clean_read_taint`.
- **The shallow checks are deleted:** `check_cmdsub_use_taint`,
  `check_cmdsub_inner_reads`, `check_arg_default_taint` and `arg_default`.
  - Where a capture sits as a value, only its own bare use is checked
    (`check_bare_cmdsub_taint`, in `check_expr_taint`'s capture arm and at
    `for` words).
  - `reject_scalar_rhs_read` runs before the insert, and its capture arm
    is a no-op.
  - `check_value_cmdsub_reads` is renamed `check_value_index_reads`, after
    what it still checks: an element's index reads. Its helpers become
    `pexp_value_index_reads` and `arith_value_index_reads`.
  - A new `parts_value_index_reads` holds the loop over a word's parts or a
    collection literal's elements, which is the branch's H304 shape. The
    rename touches the function, so the
    complexity gate applies to it, and at HEAD it was already cc 10, cog 5
    (rust-code-analysis), over the cognitive bar of 4.4.
    `check_value_index_reads` is now cc 8, cog 1.
- **A command reads before it writes.** `taint_command` now runs in this
  order:
  1. the attest early return;
  2. `check_command_reads`, which checks the name, the arguments and
     `check_collection_reader_taint`;
  3. `taint_out_params`;
  4. the read target.

  The walk descends through `-` (`taint_child_bodies`) to a suppressed
  command, so B30's `$(- join a ,)` gets a located SB5020.
- **`check_parts_taint` is extracted** from `check_expr_taint`, which is the
  branch's H297 shape.
  - At HEAD, `check_expr_taint` was already above the complexity gate: cc
    11 and cog 3, against the gate's bars (3× the codebase means) of 10.3
    and 4.4.
  - This unit touches it, so it had to come down. It is now cc 10, cog 1.
- **The cov.93 comment** in coverage.sh named `check_arg_default_taint`. It
  now names `taint_cmdsub_bodies`.

## Measured

Probes ran on pre (/tmp/w1/sash-pre, 20debc87's compiler) and on the
gated tree. Their sources are in /tmp/w1/tb, tb2 and lp, archived with the
results in the session's files/port-w1a-probes.tar.gz.

| Probe | Program | Pre | Now |
|---|---|---|---|
| t2 | `str x=$(getenv HOME); str y=$(/bin/echo "$x"); echo "$y"` | 2:19 twice | 2:19 once |
| d1 | `str a=$(getenv X) b=$(/bin/echo "$a")` | 1:33 twice | once |
| a1 | `argv 1 a; t=$(getenv HOME); x=$(arg 1 $t)` | 3:11 three times | once |
| m1 | a capture in a map element | 2:24 twice | once |
| p1 | `${b[$(/bin/echo "$t")]}` | 3:19 twice | once |
| o1 | a capture in a `[[ ]]` operand | the inner read twice | 2:20 once |
| dp1 | a capture in a dispatch | the inner read twice | 2:22 once |
| c9 | `echo "$(/bin/echo "$t")"` | 2:19 twice | 2:19 once |
| y1 | `y=hi; y=$(/bin/echo "$y")` | false SB5020 2:15, twice | accepted |
| y2 | `y=hi; y="$y$(/bin/echo b)"` | false "can't be copied" 2:3 | accepted |
| h1 | `fill "${g[0]}" =g` (above) | false SB5020 3:6 | accepted, prints `x` |
| h2 | h1, then `echo "${g[0]}"` | 3:6 and 4:6 | 4:6 only |

Notes on the table:

- A capture of an external command is itself a tainted value, so its bare
  use is a separate SB5020 in both versions. That covers c9's 2:6, p1's
  3:3, o1's 2:7 and dp1's 2:9.
- m1 also gets SB5021 at 2:12 (a tainted value in a map). a1 also gets
  SB4070 at 3:11 (no `$` in a default).

Other measurements:

- **B30 probes j2, j3, jb and j8.** Each is `array a=(); argv 1 a; int
  n=$(len a)` followed by one placement of `$(- join a ,)`:
  - a declaration (j2), an `echo` argument (j3), an array element (jb) and
    a `/bin/echo` argument (j8);
  - on pre, each reached only the backstop;
  - now each is "SB5020 join of tainted collection 'a' without attest", at
    4:16, 4:19, 4:20 and 4:26.
- **Loops (lp/w1, w2).** A `while` condition's capture, and a capture in a
  loop body, both read a name the body taints later.
  - Both still see that taint on the second iteration.
  - Pre reported each read twice; now each is reported once.
- **Unchanged.** These give the same diagnostics as pre:
  - the readers r1 and r2 (a capture in a clean-pipe reader's word, always
    SB5057), whose diagnostics now come in a different order;
  - k1-k3, an unsuppressed `join` of a tainted `a` in a capture: in a
    declaration, in an `echo` argument, and after `len` in a two-statement
    body;
  - s1, a bare `join` statement;
  - f1, a capture other than `$(keys …)` as a `for` word;
  - l1, `int n=$(len a)`;
  - g1 and g2 (SB5067).
- **The `for`-word site cannot be pinned.** SB5009 rejects every capture in
  a `for` word except `$(keys m)`:
  - fw1, `$(keys a)` on an array, is SB4030;
  - a map can never hold taint (SB5021), so `$(keys m)` is always clean
    (fw2).

  fw2 also tried a capture in a redirect target. That is SB4049, so no
  accepted program has a capture there either.
- **Negative control.** capwalk.1-11 are red on pre in both modes, each for
  its predicted reason:

  | Spec | Pre symptom |
  |---|---|
  | .1, .2 | a double report |
  | .3 | the false 2:15 |
  | .4 | "can't be copied" |
  | .5-.7 | backstop only |
  | .8 | the false 3:6 |
  | .9-.11 | duplicates |

  - The first draft of .5-.7 was red for the wrong reason: a false SB5050,
    "a assigned but never read". That is B27; see Decide-and-report 5.
  - Adding `int n=$(len a)` as a real reader fixed it.
- **Reversion stubs (native),** each as predicted:

  | Stub | Change | Red |
  |---|---|---|
  | (a) | the right-side check after the insert | capwalk.4 |
  | (b) | the body walk after the match | capwalk.3 |
  | (c) | the Declaration exclusion and the per-initializer walk removed | capwalk.2 |
  | (d) | `check_command_reads` after `taint_out_params` | capwalk.8 |
  | (e) | `check_collection_reader_taint` removed | capwalk.5-7, and 60.33, 60.37-60.43, 60.48, 60.50, 60.52 |

- **Stub (f) was mis-aimed, then redone per site.**
  - My first (f) restored the shallow walk at one of its five sites, and
    nothing went red.
  - I redid it as one stub per site:

    | Stub | Site | Red |
    |---|---|---|
    | f1 | the `check_expr_taint` arm | capwalk.9 |
    | f2 | the right-side arm | capwalk.1, .2, .11 |
    | f3 | the index-reads arm | capwalk.10 |
    | f4 | the `arg` default | capwalk.11 |

  - cov.93 runs .11's program but asserts only a bare SB5020, so it
    survives f4.
- **Refactor.** A 29-probe differential before and after the
  `check_parts_taint` extraction differs on none.

## Decide-and-report

1. **W1 is three units.** The local fixes come first, so the summaries in
   W1b start from transfer functions that report each read once and in the
   right state.
2. **Diagnostics come body first.** A capture's inner reads are now
   reported before the statement's own diagnostics, such as the capture's
   bare use. That order is the order of evaluation. No spec depended on the
   old order.
3. **The clean-pipe reader path walks its own bodies** before its reader
   check. Its capture is always SB5057, so only the order shows.
4. **The attest early return moved first** in `taint_command`, as on the
   branch. The move changes nothing:
   - HEAD already returned before the argument check for an attest, so its
     operands were never checked there; `taint_attest` checks them;
   - the one step that ran ahead of HEAD's return was `taint_out_params`.
     It taints the collections that a called user function fills from a
     source, and no function can be named `attest` (`fn attest` is SB4014,
     "expected function name").
5. **B27 stays in the backlog** (bug-peel-family-classifiers). capwalk.5-7
   add `int n=$(len a)` so that `a` has a reader the classifier sees.
6. **The `for`-word site has no spec,** because no accepted program
   reaches a tainted capture there (see Measured).
7. **B30's findings text is corrected in place.** It said "W1's
   capture-site summary fixes it". The fix is the one body walk plus
   `check_command_reads`; no summary is involved.
8. **Reimplemented, not ported.** The branch's fixes live in its walker.
   - Its H282 note also shares `check_command_reads` with in-place value
     commands. That is not needed here: validation walks every capture
     body as statements, whatever path the transform later takes.

## Specs

taint.sh plan 435 -> 446 (+11). capwalk.1-11 are fresh ids; spec.db has no
`capwalk` rows before this sweep.

| Spec | Pins |
|---|---|
| capwalk.1 | B29: one report for a tainted read in a body; the use of `y` at 3:6 is separate |
| capwalk.2 | a declaration: `b`'s body sees `a`, once |
| capwalk.3 | y1 with an attest: accepted, prints `[hi]` |
| capwalk.4 | y2 with an attest: accepted, prints `[hib]` |
| capwalk.5 | B30 in a declaration |
| capwalk.6 | B30 in an `echo` argument |
| capwalk.7 | B30 in an array element |
| capwalk.8 | H282: h1 prints `x` |
| capwalk.9 | one report in an argument's capture |
| capwalk.10 | one report in a map element's capture |
| capwalk.11 | one report in an `arg` default |

## Gates

- **One attempt, `NEW=22 mk -p 1 gate`. It PASSED** (/tmp/w1/gate.log).
  - Every static check passed. The plan sums are native 5394 + c 5172 =
    10566.
  - The sweep took 239 s, rows 6627747-6638312: 10566 = 10553 + 13 skip.
    That is 10544 + 2 × 11.
  - The first-ever ids are exactly capwalk.1-11 in both modes (22).
  - No flips, retirements or ghosts. Liveness 260.
- The hook is green with no bypass.
- The units are 237/27/20. One unit test is renamed, to
  `a_map_literal_value_is_descended_for_index_reads`.

## Ledger

- **Reimplemented:** H275, H278, H294, H297, H303, H304 and H316
  (superseded before). H314, a comment rename, is ported as it stands.
  - Their `exec_fns` parts are the branch's "runs a command" analysis
    (B114), which is not ported. W2 builds DD127's stdout analysis instead.
- **Still superseded, with a W1a note:** H261, H263, H264 and H270. W1a
  did their capture-body part. Their value calls, `walk_body`,
  `report_leaks` and `flow.introduce` spans remain for W1b and W1c, and
  their END checkpoint for W5.
- **Still pending:** H272 and H282, each with a W1a note.
  - What remains of them: `flow.introduce` (both), and `record_call` and
    `bulk_fill_targets` (H282). Those are for W1b.
  - Also remaining: H282's break/continue/return deposits and
    `jump_levels`, which are W3's (B95).
- **Findings:** B29 and B30 are ticked.

W1b is next.
