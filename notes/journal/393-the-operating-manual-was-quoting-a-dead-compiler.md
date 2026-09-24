# 393 — The operating manual was quoting a dead compiler

Two sessions in a row, a number in `AGENTS.md` cost real work.

The first was mine and I caught it myself: costing the `for k v in m` task, I
wrote that `lower_for` was "cc 35" — straight out of AGENTS.md's cyclomatic
residuals table — and filed a task whose cost section was wrong by 7×. The
live index says 5. I corrected the filing and appended the finding, then
deliberately did *not* act on it, because the table's claim was not just a
number: it asserted a **grammar skew** between indexnudge and Mozilla's
`rust-code-analysis`, and you cannot find-and-replace your way out of a claim
about a mechanism. It needed a re-measure.

DG's instruction closed both halves: *"correct it; drop numbers from AGENTS.md
that are prone to go stale (number of tests, coverage %, etc.)"*.

## The re-measure

`rust-code-analysis-cli` was not installed, and `cargo install
rust-code-analysis-cli` **fails** — E0308, two different
`tree_sitter::Language` types, because the unpinned resolve drags in
tree-sitter 0.20.9 alongside the version the crate wants. `--locked` fixes it.
That is now recorded in AGENTS.md, since the file tells readers to run this
tool and the obvious invocation does not work.

Then: per-function `cyclomatic.sum` and `cognitive.sum` out of Mozilla, joined
against the indexnudge `symbols` table, across every file in `sash/src` —
**1901 functions in 45 files, 1620 agreeing exactly.**

The three functions the table named agree with Mozilla **exactly**:

| function | AGENTS.md claimed | moz | idx |
|---|---|---|---|
| `transform/statements::lower_one` | 55 (moz 57) | 17/1 | 17/1 |
| `transform/control_flow::lower_case` | 40 (moz 44) | 2/1 | 2/1 |
| `transform/control_flow::lower_for` | 35 (moz 44) | 5/4 | 5/4 |

So the table was wrong twice over. The magnitudes were stale — those functions
lost their giant `match` on CST kind when commit `1c0e64a` deleted the CST —
and the *skew* it attributed them to does not apply to them at all.

## What the residual actually is

It is not zero, but it is nothing like the "~10 slop" the table advised, and
it lives elsewhere. 281 functions diverge, and the direction is one-sided in a
way worth knowing:

- **cyclomatic: 119 low, 0 high.** Indexnudge never reports a cyclomatic score
  *above* Mozilla's, across all 1901 functions. An indexnudge cc is a floor.
- **cognitive: 158 low, 16 high.** Mostly low as well; the high tail is deeply
  nested parser code (`parse_expr` +5, `parse_body_until` +4).

Every divergence observed was within a few points. The largest cc gaps are −3
(`dispatch_statement`, `absorb_text`, `classify_operand`, `const_arith_fault`,
`check_args_immutable_stmt`) — wide dispatchers, exactly where you'd expect a
grammar version to disagree about what counts as an arm.

None of this touches the enforced gate, which compares indexnudge numbers
against indexnudge means. The residual only matters if you set an absolute
threshold from a Mozilla run.

## The general fix

The residuals table is gone. In its place: the direction, the structural
cause that remains (nested `function_item` depth), and **the command to
re-measure** — including the `--locked` requirement.

That is the pattern DG asked for across the file, so I applied it everywhere a
current-state number appeared:

- design-decision count and highest DD number → `ls notes/decisions/`
- journal file count → "one numbered file per session"
- `transform/` "(9 files)", `codegen/` "(12 files)" → dropped; the lists below
  them are the real inventory
- `clauses.tsv` "231 rows" → "curated DD clause text"
- "cannot purge all 2697 `c:` tests" → "the whole `c:` cohort"
- native-only files "191 tests" → "their combined `plan`", which is the thing
  you'd actually compute
- `runs` "past 1.4M rows" / "~95 MB" → "grows without bound" / "tens of MB"
- `sash-rt` coverage baseline percentages → "`mk rt-cov` prints the current
  baseline"
- "77 in-crate unit tests — sash (30), sash-convert (27), sash-expand (20)" →
  the crate list alone (sash was already 51)
- "4611 tests against 77 units" → "two orders of magnitude"
- cc/cog means "4.04 / 2.05 across 1735 functions, 3× bar at 12.1 / 6.2" →
  dropped; the re-measure query was already sitting right underneath
- `main.rs` "thin 4-line wrapper" → "thin wrapper"

## What stayed, and why

The distinction that did the work: **a historical number cannot go stale; a
current-state number always will.** The file is full of incident reports whose
numbers *are* the evidence — the 5585-vs-5583 ghost tests and commit `5cfeb24`,
`ran=323` against 321 greppable `chk` tokens, `[plan=217, ran=211]`, the
20 MB blob across 455 commits totalling 9.0 GB of pack, the test redux's
546 = 210 + 272 + 45 + 4, "49 of its 159 passing tests (31%) compiled programs
the real compiler rejects". Every one of those is a fact about a moment that
already happened. Deleting them would delete the reason the surrounding
instruction exists.

Two borderline cases I kept deliberately:

- **The 3× tightening evidence** (80 violate at 3×, 357 at 2×, 518 at 1.5×).
  Stale-prone in principle, but it is dated in the text, it is the rationale
  for DG's decision that the gate holds, and the paragraph already says "do
  not re-propose tightening without fresh numbers".
- **`tree-sitter-bash 0.25`.** A dependency pin, checkable against
  `sash-convert/Cargo.toml`, and verified still accurate.

## The lesson that generalises

A number in an operating document is a cache with no invalidation. The three
cyclomatic figures were correct when written; what made them harmful was not
being wrong but being *specific enough to trust* — precise numbers in a table
read as measured, and nobody re-measures a measurement. The advice built on
top ("allow ~10 slop on big-match-on-CST-kind functions") outlived not just
the numbers but the CST itself, and still sounded authoritative.

Where a number is genuinely useful, AGENTS.md should carry the **query**, not
the answer. It already did that for the cc/cog means — and then quoted the
answer in the sentence immediately above the query, which is how those went
stale too.
