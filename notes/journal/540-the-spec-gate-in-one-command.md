# 540 - The spec gate in one command

2026-10-29. On 2026-10-28 DG asked whether all of the gates are automated.
They were not:
- Only the pre-commit hook blocks: fmt, clippy --tests, totality and the
  complexity gate.
- `mk test` and `spec/run.sh` are one command each, but run.sh exits 0
  whatever fails (the per-file `|| true` at run.sh:44).
- Every check that AGENTS.md's Spec tests section asks for after a sweep was
  a query typed by hand:
  - the predicted total and first-ever count;
  - flips against each id's previous row, and the liveness query that
    proves the flip query can fail at all;
  - plan= against each file's chk count;
  - both id-collision checks, and the high-water check for retired ids;
  - the SB census in both directions.

Coverage sessions 520 to 530 each wrote their own spec-gate SQL file, eleven
in all, and the port's families ran the same queries by hand again. A gate
typed by hand can be skipped or typed wrong. AGENTS.md already records two
hand comparisons that went silently wrong: one compared a sweep with itself,
and the other compared new specs with the history of retired specs that had
the same ids. DG then asked for it: "build mk gate".

## What `mk gate` runs

`mk gate` depends on `mk test` (build, unit tests, the status block), then
runs `contrib/gate.sh`, which has three phases:

1. **Static checks.** If any fails, the gate stops before sweeping.
   - duplicate chk ids;
   - variable-id collisions;
   - plan= against each file's chk count;
   - the SB census in both directions.
2. **One full sweep** of spec/run.sh in both modes. The rowids it adds to
   `runs` form a block, and the next phase reads only that block.
3. **spec.db checks** against the block:
   - rows, distinct ids and each mode's ids equal the plan sums;
   - nothing is failing (skips are reported, not gated);
   - no file printed a `[plan=` note;
   - flips equal FLIPS, first-ever ids equal NEW, retired ids equal RETIRED;
   - no registry row went unrun (a ghost);
   - liveness: some ids differ from their first-ever status.

Every check prints `PASS` or `FAIL` with its count, and a failure lists up to
40 of the offending items. The script exits 0 on a pass, 1 when a check
fails and 2 on a usage error. `mk gate-static` (`--static`) runs only
phase 1, in under a second. `--recheck` re-runs phase 3 against the last
gate sweep, so a corrected prediction costs no second sweep.

## Choices (decide-and-report)

- **Predictions come from the environment:** NEW, FLIPS, RETIRED, TOTAL. mk
  passes the environment through, so `NEW=12 mk gate` works. Each defaults to
  0, and TOTAL to the plan sums. A forgotten prediction therefore fails the
  gate instead of passing it.
- **The purge cap is the RETIRED prediction.** run.sh deletes registry rows
  for ids the sweep no longer produces, up to SPEC_PURGE_MAX. Under the gate,
  a retirement nobody predicted deletes nothing. It then shows up twice: as a
  retired-count mismatch and as a ghost.
- **The previous row is the latest row before the block,** never one inside
  it. This is AGENTS.md's row-order comparison, so a dirty tree that shares a
  commit hash with the last sweep cannot make the gate compare a sweep with
  itself.
- **First-ever means no row before the block.** A new spec that reuses a
  retired id has an old row, so the first-ever count falls short of NEW. That
  replaces the manual high-water query.
- **plan vs chk counts each `chk $var` token once per distinct `_id=` value
  in its file.** With that, coverage.sh's variable-id loop agrees with its
  plan (372), so the gate needs no exemption for it. The hand query in
  AGENTS.md still reports coverage.sh as its one legitimate mismatch.
- **spec/run.sh still exits 0.** contrib/coverage.sh runs it under `set -eu`
  (coverage.sh:34 and :119) and must reach its export even when a spec is
  red, so the gate is the command that fails. My offer on 2026-10-28 said
  run.sh would exit nonzero on a failing spec; this is the change from it.
- **Not automated:**
  - the negative control (each new spec red on the pre-change binary);
  - running reversion stubs;
  - `mk cov-stale`, which matters only for coverage ranking;
  - fmt, clippy, totality and complexity, which the pre-commit hook already
    blocks on.

  The first two are notes/tasks/gate-negative-control.md. A spec file finds
  its binaries and runtime relative to its own directory (common.sh:19,
  :25), so a negative control needs the current spec files copied into a
  built tree of the base revision. That is more script than a 30-minute unit
  should carry.

## Proving it can fail

Before trusting a PASS, each static check was broken on purpose. Each edit
was reverted with `git checkout spec/`.

| Edit | Result |
|---|---|
| `chk 39.56` renamed to `chk 39.55` in regex.sh | duplicate ids: FAIL |
| regex.sh `plan=` raised by one | `regex.sh plan=115 chk=114`: FAIL |
| a literal `chk cov.306` appended to coverage.sh | variable-id collision and plan: FAIL |
| `SB9999` asserted in a spec | "asserted but raised nowhere": FAIL |
| every spec mention of SB5095 replaced (cli.sh, common.sh, errors.sh) | "raised but asserted by no spec": FAIL |

The census control was vacuous the first time. I removed SB4002 from a spec
and nothing changed, because SB4002 is retired: it is raised nowhere, and
its only mention in a spec file is a comment (coverage.sh:45), which the
census already ignores. SB5095 is live, and replacing it made the check fire.

`NEW=x mk gate-static` stops with the usage error and mk returns 1, so mk
passes the predictions through.

## Results

The first full `mk gate` passed exactly as predicted. No code had changed
since F2's last sweep, so NEW, FLIPS and RETIRED were all 0.
- The sweep took 231 s: rowids 6451475-6461661.
- 10187 rows and 10187 distinct ids (native 5204, c 4983).
- 0 failing, 13 skipped, no `[plan=` note.
- 0 flips, 0 first-ever, 0 retired, 0 ghosts.
- Liveness 260, the can-fail figure every recent sweep has recorded.

Then the spec.db checks were proved to fail. `--recheck` made this cheap,
because no further sweep was needed.
- `NEW=1` failed on first-ever (0, expected 1). `FLIPS=1 RETIRED=1` failed
  on both. Each exited 1.
- Pointing the saved state at older blocks replays the gate over sweeps
  whose figures were recorded by hand:
  - F2 commit B's block (6441288-6451474) passes whole with `NEW=27`,
    matching journal 539's table. With no prediction it fails and lists the
    27 ids.
  - Step 0's block (6286459-6295791) gives 9333 rows (native 4769, c 4564),
    30 flips and 0 first-ever, which are the three predictions journal 531
    recorded. Its retired and ghost checks fail, as they should: today's
    registry has 854 ids that sweep never ran.
- A failing gate fails mk: `NEW=x mk gate` stops at the usage error and mk
  returns 1.

One change came out of the run. `mk test` regenerates status.md's mk-status
block before the sweep, so after a gate the block always trailed by one
sweep. A pass now regenerates it again, from the gate's own sweep. The two
commands are chained with `&&`, because this mk does not stop at a failing
recipe line (journal 518). The `NEW=x mk gate` control above shows the chain
holds: its log ends at the usage error. Without the `&&`, the regeneration
would have run, and its exit status of 0 would have become the recipe's.

## Using it

Predict, then run, e.g. `NEW=12 mk gate` for six new dual-mode specs. A
FAIL is a finding before it is a typo: read the listed ids first, then
correct the prediction and `--recheck`, or fix the tree and sweep again.
AGENTS.md's Spec tests section now opens with the gate, and Completion
checklist step 2 is `NEW=<n> mk gate`.
