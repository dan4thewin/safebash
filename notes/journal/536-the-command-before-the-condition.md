# 536 - The command before the condition

2026-10-26. F7 is the fourth family ported: B85, the condition status
classifier. DG said "go for F7", approving the recommendation: port the
branch's four hunks as-is, add a spec for `if a; [[ … ]]`, write DD#55 and
DD#70 notes, and file the questions as backlog.

## The bug

A condition catches the status of its operand and nothing else (DD#55).
Three scans decided which instruction that was, using two lists:

- The transform's `uncheck_last_condition_inst` clears the operand's abort.
  At `fec83f7` it walked the block backwards, skipped the `!` sigil, stopped
  at a `RedirectPush`, and unchecked the first `Command`, `Pipeline` or
  `PipelineFinish` it met. Everything else fell to `_ => {}`: `Test`,
  `ReadLine`, `ReadArray`, `MatchInto`, `AttestAll` and `Assign`.
- Both backends use `ssa::condition_inst_index` to decide whose redirect
  open(2) failure is caught (DD#70). It took the last `Command` (but the
  sigil), `Pipeline`, `PipelineFinish`, `ReadLine`, `ReadArray` or
  `RedirectPush`, and passed over `Test`, `MatchInto` and `AttestAll`.

Both lists ended in a pass-over default, so a `[[ … ]]` in condition
position was skipped and the scan settled on whatever came before it. Spec
55.89 is the plain case:

```
x=a
/bin/sh -c "exit 3"
if [[ $x == a ]] { echo yes }
echo done
```

At `fec83f7` this prints `yes` and `done` and exits 0, in both backends:
`/bin/sh` was unchecked as though it were the condition. A `readline`, a
`read`, a `match`, an `attest … in`, the dispatch form and `return [[ … ]]`
were skipped the same way, by one list or by both. The lists also disagreed
with each other: a read was the operand for the backends but not for the
transform.

## The fix

The branch made one definition and pointed all three scans at it.
`ssa::condition_operand_index(insts)` is the index of the last instruction
that sets the status a following condition reads. Its classifier,
`sets_condition_status`, is total over `SsaInst`, with no wildcard:

- a `Command` (but the sigil), `Pipeline`, `PipelineFinish`, `Test`,
  `ReadLine`, `ReadArray`, `MatchInto`, `AttestAll` and `RedirectPush` set
  the status;
- an `Assign` sets it only when its target is `?` (DD#74: `return N` is
  `Assign(?, N)`);
- the other eleven variants do not.

`condition_inst_index` returns that index for a block that ends in a
`Branch` or a `Loop`, and both backends call it as before (native
`codegen/entry.rs:319`, C `emit_c.rs:759`). The transform now asks the same
function and clears `checked` only if the instruction it names is a command
or a pipeline. A `[[ … ]]`, a read, a `match`, an `attest … in` and `$?`
never abort, so there is nothing to clear, and the command before them
keeps its abort.

Ported as-is: H150 and H151 in `ssa.rs`, H200 and H201 in
`transform/entry.rs`, the four hunks the ledger gives F7. There is no new
diagnostic and no change of surface.

## What changes

Three things, each previously silent:

- A command, or a redirect failure, **before** a condition whose operand is
  a `[[ … ]]`, a read, a `match`, an `attest … in`, the dispatch form or
  `return [[ … ]]` now aborts, as it would before any other statement. That
  includes a capture assignment before the condition (55.99), and the
  non-final member of `if a; [[ … ]]`, which DD#55 already treats as
  `a; if [[ … ]]` (55.104).
- A failed capture **inside** a `[[ … ]]` operand aborts: the capture is an
  argument of the test, not the condition. An external capture reaches a
  test operand only under `--no-taint`, where `while [[ $(cmd) … ]]` used to
  loop on `cmd`'s failure (55.91). A function capture there already aborted
  at `fec83f7` (probes u1–u3), so outside `--no-taint` only the first change
  is visible.
- `cmd > /bad; if [[ … ]]` no longer "catches" `cmd`'s failed open (55.103).

DD#55 has an addendum dated 2026-10-26 that states the rule and these
changes. DD#70's implementation note said the "in condition" signal was the
last `Command`/`Pipeline` of a `Branch`-terminated block; it now names
`condition_inst_index`.

Before any edit, a trial build (HEAD plus the four hunks) was compared with
the branch binary on 24 edge probes, in both backends, and agreed on every
one. Against HEAD, three change, all of them B85: x21 (`if a; [[ … ]]`, now
55.104), and a failed capture in the operand of `[[ … ]] &&` (x33) and of
`! [[ … ]]` (x34), both under `--no-taint`. The redirect guard gives the
same verdicts before and after on a redirected call to a bad path, as a
statement and as an `if` condition, in both backends.

## Specs and proof

`errors.sh` `plan` goes from 491 to 509: 18 specs, each in both modes.

- 55.89–55.103 are verbatim from the branch and keep their ids. Their only
  earlier rows are the same programs at the branch sweep, so they are not
  first-ever; they passed there and pass now.
- 55.104 is new: `if /bin/sh -c "exit 3"; [[ $x == a ]]`.
- 55.105 (a pipeline as the condition) and 55.106 (a redirected function
  call) are new too. They aborted before the fix, so they are not
  regressions. They pin two arms of the classifier that no other spec
  reaches: without them, removing either arm turned nothing red.

The new ids start above the 55.x high-water mark in spec.db, which was 103
(the branch's last).

**Negative control.** The `fec83f7` binary fails exactly 55.89–55.104, 16
specs, in both modes, and passes the other 491; the F7 binary passes all
507. That was before 55.105 and 55.106 existed. Those two pass on
`fec83f7`, so their control is a stub: removing the `Pipeline` arm fails
only 55.105, and removing the `RedirectPush` arm fails only 55.106, in both
modes. On those builds the two programs print `yes done` and `done` instead
of aborting.

**Reversion stubs**, one per arm of `sets_condition_status`. Each stub
makes its arm answer `false` (the sigil's stub makes it answer `true`),
followed by a full sweep. Each red spec was red in both modes:

| Stub | Red specs |
|---|---|
| `Test` | 55.89–55.94, 55.99–55.104 |
| `ReadLine` | 55.95; 55.68 |
| `ReadArray` | 55.98; 55.63, 55.64, 55.71 |
| `MatchInto` | 55.96 |
| `AttestAll` | 55.97 |
| `Command` | 52 ids, among them 0.5, 55.17c and the `bc.`, `bng.` and `ex.` families |
| the sigil | 55.69–55.71, bng.1, bng.2, cf.u7, div.32 |
| `Pipeline` | none; 55.105 once written |
| `RedirectPush` | none; 55.106 once written |
| `Assign(?)` | none; the unit test below |
| `PipelineFinish` | none; unreachable, see below |
| no stub | none |

The stub sweeps ran at 9603 checks, before 55.105 and 55.106 existed. The
`ReadLine` and `ReadArray` stubs also turn older redirect specs red, because
the backends' redirect guard reads the same classifier.

**`Assign(?)`** sets the status, but no program can tell that arm from its
opposite. Each producer of `Assign(?, …)` (`return N`, an int call's
restore, the caught `||` reset, a `case` with no match) either ends its
block in a `Goto` or `Return`, which no condition scans, or opens a fresh
block in which nothing before it sets the status. A unit test pins it
instead: `an_assignment_to_status_is_the_condition_operand` asserts that
`[cmd, Assign(?)]` selects the assignment and `[cmd, Assign(n)]` the
command.

**`PipelineFinish`** is counted but cannot reach the scan. Its only
producer, reader cleanup, runs before the scan and never leaves a finish in
a block that ends in a `Branch` or `Loop`: on such an edge it adds a
bridging `Goto` block (`install_edge`, `reader_cleanup.rs` 67–88). Whether
the arm should become an ICE or keep its answer is DG's question, filed
below.

**The signal checkpoint** is a `Command`, so the classifier counts it, as
both old lists did. That is harmless and unchanged: lowering ignores its
`checked`, `may_take_redirect` excludes it, and it writes the status only on
its exit path.

## Gates

- `mk test`: 226/27/20. The new unit test is the one added.
- Full sweep at `fec83f7`, with F7 uncommitted:
  - 9607 = 9594 passing + 13 skips (native 4913, c 4694), which is D1's
    9571 plus 36, as predicted;
  - rowids 6371668–6381274, one row per id;
  - exactly the six predicted first-ever ids (55.104–55.106 in both
    modes), no flips and no retirements;
  - the can-fail query reports 260.
- The SB census is empty in both directions; `plan` matches `chk` (but for
  `coverage.sh`'s known +2); both collision checks are empty; and the `plan`
  sums, native 4913 + c 4694 = 9607, equal the summary.
- The pre-commit hook passes with no bypass. Every touched function is under
  the complexity gate's threshold, three times the means (cc 3.47,
  cognitive 1.52):

| Function | cc | cog |
|---|---|---|
| `condition_inst_index` | 2 | 1 |
| `condition_operand_index` | 1 | 0 |
| `sets_condition_status` | 5 | 1 |
| `mark_unchecked_in_conditions` | 3 | 3 |
| `uncheck_last_condition_inst` | 2 | 1 |

## Backlog

Under DG's rule, three tasks:

- `bug-match-out-array-capture-operand-ice` (P1). A capture as the subject
  or pattern of the out-array `match S P ARR` or `matchall` panics the
  compiler in both backends (native exits 1, C 101). It is pre-existing: the
  same at `fec83f7`, with F7, and on the branch. It turned up while probing
  `match` in condition position. `try_handle_match_into` lowers its operands
  without lifting captures; `bug-fn-capture-in-test-operand-ice` fixed the
  same shape for `[[ ]]` only.
- `q-convert-capture-in-condition-operand` (P3). Bash's `set -e` does not
  apply inside a condition, so `if [[ -n $(grep …) ]]` tolerates a failing
  `grep`. sash-convert copies the test verbatim, and since F7 the failure
  aborts. Only `--no-taint` sees it, because SB5020 already stops the
  capture in the default mode. The options are a `# CHECK:` hint
  (recommended), rewriting to `$(- …)`, or leaving it.
- `q-condition-classifier-pipelinefinish` (P4). The unreachable arm above
  could become an `unreachable!()` that names reader cleanup's bridging, or
  keep an answer by `preserve_status`.

## Corrected along the way

- The recommendation predicted `plan` 507 and two first-ever ids. Adding
  55.105 and 55.106, once the stubs showed those arms unpinned, made it 509
  and six.
- It named two backlog tasks; the `PipelineFinish` question makes three.
  That one is P4 rather than the P3 I planned, following
  `q-attest-shorthand-int-operand`.
- My first DD#55 text said that reader cleanup "bridges every `Branch` and
  `Loop` edge". It bridges only the edges that carry finishes; the addendum
  now says that it never leaves a finish in such a block.
- Two bullets in the findings index, on the match ICE's scope and the
  converter question's options, said more than their task files; they now
  match.
- H324 and H325, the `errors.sh` rows that the ledger splits among F5, F6
  and F7, stay `pending`, with a note that F7's part is ported. The ledger
  has no partial status. Decide-and-report.

## Next

B55, when DG says go: `-o` into an unwritable directory panics at
`compile_native`'s `.expect("write .o")`, rc 101. The rustix P1
(`bug-fresh-clone-rustix-runtime-private`) still awaits DG's choice of
remedy.
