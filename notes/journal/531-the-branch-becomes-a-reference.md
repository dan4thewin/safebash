# 531 - The branch becomes a reference

2026-10-20 to 2026-10-24. Coverage batch 531 began from `1309385` with
46 cold production lines: 14 in `type_check.rs` and 32 in
`validate.rs`. It closed none of them. Classifying them stopped on real
bugs, the bug policy made each one a stop-everything event, and every
fix exposed more. The fixes grew into a rewrite of the taint walker,
and DG stopped it. This entry records how that happened, what DG
decided before any of it lands, and the inventory that opens the port.
Nothing in this entry changes the compiler's behaviour.

## How the batch grew

Most of the first findings were typing bugs (B1, B2, B4-B7), and B8's
fix exposed B9: every forked child ran the END block, which inside END
is a fork bomb. From there the ledger grew past a hundred findings. They
included forked children that ran parent code (B9, B11, B17), captures
that crashed the compiler or jumped out of their own body (B57-B60),
wrong positions in re-lexed text (B3, B54, B61), three sed scanners that
disagree (B62-B65, B67, B68) and several taint bypasses (B18, B25, B37).
The fixes went in as they were found, uncommitted.

A run of taint findings around function calls and END (B94-B111) then
led me to replace the taint walker with a context-sensitive one, which
walks each function body once per distinct entry state. DG:

> this run has gone on for hours and I see many files changed. why is
> this iteration so different than the prior ones?

and then *"start a new branch for this redesign. finish e7. run the spec
then stop and report."* The branch `taint-walker-redesign` holds one WIP
commit, `e98fca8`: 44 files, +5555/-2404 lines in 330 hunks, 101 new
dual-mode specs, and six existing specs rewritten or re-coded. Its sweep
came out exactly as predicted: 9535 = 9492 passing + 30 failing + 13
skips. The 30 failures are 15 specs, in two modes, that the branch left
red on purpose until DG answered B33; T1 below settles it.

The walker rewrite was a consequential architecture fork. The decision
tree sends such a fork to DG before any code is written, and I did not
send it. The bug policy made fixing those bugs mandatory; it did not
make the design of the fix mine to choose. The branch also lacked the
usual record:

- no journal;
- a commit message that omits the new rejection SB5098 and five SB4035
  re-codes;
- two citations of a "journal 531" that did not exist (the branch's
  DD#24, line 138, and `validate.rs:6450`). This entry does not hold
  what they cite either; both are rewritten when their families port
  (F10 and W1);
- spec 75.164, citing a DD#80 addendum that was never written;
- the B70 specs, attributing "the `$` starts no variable" to DD#72, which
  says no such thing, and no other DD does;
- a findings ledger that existed only outside the repository.

DG: *"This branch and rewrite is too large and lacks the usual
narrative."* He had read the specs and saw *"multiple cases that should
be simplified by design decisions"*. He asked for a series of steps that
port the branch back to `hand-parser` one family of issues at a time,
each starting with a discussion of the problem and of whether the
branch's solution stands or the design needs refining.

## Two decisions before the families

DG raised two issues before the plan was written, because both reshape
it.

**D1: an integer is not a string in `[[ ]]`.** The branch added specs
such as `[[ $(( 10 / z )) == 1 ]]`. DG: *"the $((...)) is an integer
expression and shouldn't undergo any automatic atoi within [[ ]] - I
would expect an error."* Base accepts every whole-integer operand in a
test, a dispatch or an attest predicate (B113). DD#30 already says "No
implicit conversion between int and string", and the assignment
checker already tells the user to quote. So a whole-integer operand
becomes SB4030, while `"$n"` and interpolation inside a word stay legal.
D1 is its own step. It lands before F8, F9 and F10, which hold all nine
branch programs it changes.

**T1-T3: an attest lives with its command.** DG recalled that a user
function could break the "always clean" verdict on its capture *"by
simply running a command/pipeline at the end"*, and went on:

> If a user function is used in a capture and the variable is used
> without the :! escape hatch, I think that should also be an error, as
> the attest belong immediately after, but a function without its own
> attest means the same command must have multiple attests...

Option (c) moves the attest into the function, beside the command. I
had said (c) breaks line-by-line checking. DG: *"it only means the
line-by-line needs to be within the function"*, and then *"yes to (c),
no to exceptions."* He carried the principle further: *"I very much
want taint/attest to be easy to reason about. I think that means (c)
for globals and out-params."* After the pros and cons: *"A; yes to
Q3"*, which chose the strict reading of the outward rule (A) and added
the inward one (Q3). Both are below.

One rule results: an attest lives in the body whose command produced
the taint, and no other body can attest it.

- **T1, stdout.** Unattested external output that reaches a function's
  stdout taints `$(f)`. An attest at the call site becomes a new
  rejection that names `f` and the command. The test is whether output
  reaches stdout, not the branch's broader "f runs a command", which
  introduced two regressions (B114, e5). T1 flips the 15 parked specs.
  It retires DD#24's "`$(user_func ...)` capture is clean" and the "user
  function capture" entry in AGENTS.md's list of clean sources.
- **T2, names outward.** A body may not leave its own taint on a global
  or an out-param slot: SB5067 at the source. This reverses DD#121's
  2026-09-30 "no net insert" exemption and DD#100's outward slot taint.
- **T3, names inward.** A body cannot attest taint it received (SB5088);
  it reads it with `:!`. This retires DD#100 Part E's deferral.

The consequence matters more than any one rule: taint no longer needs a
context-sensitive walker. Each body is checked once, from an empty entry
state, and summarised by three things: what it reads before writing,
what it always overwrites, and which command, if any, reaches its stdout
unattested. DD#27 (no recursion except self-tail) and SB5071 (functions
only at top level or in END) make the call graph acyclic, so the
summaries are computed bottom-up, with no fixpoint across functions.

DG asked: *"to be clear, we won't be taking the lion's share of the new
branch that is the walker redesign back?"* Measured by enclosing
function, the walker is about 1080 of the branch's 4458 added Rust
lines. Its "runs a command" analysis is about 120 more: together 27% of
the Rust, plus 11 of the 101 new specs. None of that is ported. A new
track, W1-W5, re-implements the taint checks under T1-T3. The other
3300 or so lines (the family fixes, the attest predicates, the SSA
backstop) and about 90 specs still port, each with its family. The DD
addenda for D1 and T1-T3 are written by the steps that implement them,
so no document runs ahead of the code.

## The order

DG: *"we want the least surprising ordering, which probably means
avoiding repeated work and starting with unencumbered items
dependency-wise."* A step lands only after every step whose decision
could reshape its code or its specs. A finding that shares a function
or a root cause with a family folds into that family, so the code is
written once. Walker hunks are superseded, never ported.

| # | Step | Needs |
|--:|------|-------|
| 1 | Step 0: inventory (this entry) | - |
| 2 | F13 positions in re-lexed text | Step 0 |
| 3 | F14 malformed names and arithmetic | F13 |
| 4 | F15 regex and sed operands | F13 |
| 5 | D1 | Step 0 |
| 6 | F7 condition status classifier | Step 0 |
| 7 | B55 `-o` into an unwritable directory panics | Step 0 |
| 8 | F1+F4 wrappers; operand and condition grammar | F13 |
| 9 | B37 a resolution walk that skips some captures | F1 |
| 10 | F2 forked children run parent code | F1+F4 |
| 11 | F3 functions defined in END | F2 |
| 12 | F5 capture bodies are function bodies | F1+F4 |
| 13 | F6 suppression scope | F5, F2 |
| 14 | F8 lazy `[[ && \|\| ]]` | D1, F5, F7 |
| 15 | F9 dispatch | D1, F5 |
| 16 | F10 attest predicates, SB5098 | D1, F8, F5, F3 |
| 17 | W1 per-body summaries under T2/T3 | F1+F4, F3, F5, F10, B37 |
| 18 | W2 stdout (T1), W3, W4, W5 (END) | W1 |
| 19 | Open findings the branch never fixed | their families |
| 20 | Wrap-up | all |

Every step follows one protocol. It opens with a discussion: a minimal
reproducer run on both binaries, the governing DDs, the branch's
solution and specs, and the simplifications a design decision could
make, with a recommendation. DG decides. Then come a DD addendum for
each semantic decision and the port, either applied as-is or
re-implemented on `hand-parser`. Every fix gets a spec that fails on the
base binary. The step closes with the full gates and its own journal.

## Step 0: the inventory

**Reference binaries.** `sash` and the runtime are built at both commits
in two worktrees, so every finding can be re-run on base and branch in
both backends without a rebuild. The batch's probe sources (text only)
are archived outside the repository.

**Re-baseline.** spec.db still held the branch sweep, including 202 ids
that exist only on the branch (101 specs in two modes). I first checked
that the purge cohort was exactly those 202, then let the sweep retire
them (`SPEC_PURGE_MAX=202`). The prediction was 9333 = 9320 passing + 13
skips, no first-ever ids, 202 retirements, and exactly 30 fail-to-pass
flips: the 15 parked specs, whose previous row is the branch's red one.
All of it held, in block 6286459-6295791 at `1309385`. A spec ported
verbatim keeps its branch id, whose only history is the same program in
that branch sweep. A changed or new spec takes a fresh id above the
high-water mark.

**Hunk ledger.** Each of the 330 hunks of `git diff 1309385 e98fca8` is
assigned to a family, or split by line range where one hunk serves
several (21 hunks). F2 has the most (63), then F13 (48), F5 (33), W1 and
W2 (27 each) and F10 (24). 64 hunks belong to the walker and are marked
superseded. One is proposed for dropping:
`DeclAssignment::cmdsub_bodies` in `ir.rs` has no caller even on the
branch; it was written for the walker. The ledger is committed beside
the findings index, as `notes/tasks/batch-531-hunks.tsv`, and each step
marks its rows. Wrap-up requires every row to be ported, re-implemented
or dropped by a decision.

**The order, rechecked.** Every edge was checked again against the
hunk ledger, and none changes the sequence. There is one new edge, F10 needs
F3, which the order already satisfies. B29 joins B30 in W1. B101's
transform half can land with F5's B60. F1 needs only the `stage_body`
peel from one shared hunk. F2 re-pins a unit test the branch deleted,
using base's existing recursion backstop in `inline_function_call`. The
B77-B80 collision authority does come after F3. Five more candidate
edges came from code that two families share (F13 and F14, F3 and W2,
F10 and W2, F1 and F3, B37 and F5). In each pair the step that comes
first needs nothing from the later one, so none reorders.

**Findings table.** There are 127 ids: B1-B115 (B88 is split into
B88a-B88d, and B83 has a variant, B83b), the parser findings n1-n6, and
e5 and e7. Each was re-probed on both binaries in both backends: 321
probe rows. The counts:

- 69 fixed on the branch, plus 3 that also need a question settled;
- 42 open, plus 1 that also needs a question settled;
- 5 questions;
- 2 regressions the branch introduced;
- 5 needing no work: 2 not bugs, 1 duplicate, 1 not reproduced and
  1 control.

Re-probing corrected the findings ledger and the plan in several places:

- **B76's description was inverted.** The plan said an outer capture's
  suppression leaked into a function's own checks. The probe shows the
  opposite. In `if x=$(/bin/echo $(f))` the fast path lifts the inner
  `$(f)` out of the condition capture's suppression, so a `/bin/false`
  inside `f` aborts on base. The "leaked" wording belongs to B83.
- **B29 and B30 are both W1's.** B29 is a duplicated SB5020 for a
  capture argument. B30 is `$(- join a ,)` reading a tainted `a`, which
  base catches only in the SSA backstop.
- **55.88 is B84's spec**, so it ports with F6, not with F5's
  55.76-55.87. On base its program gives the right answer by accident,
  because B83's leak, which F5 fixes, also hides B84. So 55.88 can fail
  before its fix only once F5 is in, and F6 already follows F5. B84
  itself reproduces on base in its own probe.
- **B72 is internal to the branch:** a regression of its own rule, fixed
  there.
- **B90 does not reproduce:** both taint layers reject it on both
  binaries.
- **B94 is open**, not fixed on the branch.
- **B110 is a taint bypass on both binaries.**
- **B114 and e5 are regressions the branch introduced.** Base is
  correct, and T1's narrower test keeps it so.
- **B106**, a conservative SB5088, moves to W3.

**Findings index.** `notes/tasks/batch-531-findings.md` brings the
findings ledger into the repository. It has one entry per id, grouped by
the step that lands it, with its status, its governing DD and the
branch's specs. Each step ticks its entries and names its journal.

## Next

F13 opens with its discussion. Nothing of F13 lands before DG decides.
