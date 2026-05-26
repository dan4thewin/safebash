title: Decide logos vs hand-roll for the fixed-token layer
status: done
priority: D1
kind: decision
source: opus-47-review
resolved: 2026-05-26
resolution: Switch to logos on a new branch. If it goes well, cement in a DD.
depends: []
gates:
  - DD or journal entry

## Resolution (DG, 2026-05-26)

> Make a new branch and switch to use logos — if it goes well, then
> cement it into a DD.

Trial first, formalize second. Tracked in
[r47-reduce-parser-complexity-hotspots](r47-reduce-parser-complexity-hotspots)
which becomes the implementation task. A new DD (DD#59 or next) lands
only after the migration is proven on the branch.

The parser-migration-postmortem (addendum #2) closes with "open
question: retrofit logos for the fixed-token layer, or accept the
current scanner." That question is still open and is gating the
top-priority refactor work in `r47-reduce-parser-complexity-hotspots`
(P1) — `next_token` is CC 78 today; ~400 lines of mechanical
longest-match dispatch is exactly what logos was built for.

## What needs to be decided

Three options:

1. **Retrofit logos** for the fixed-token layer (~400 lines), keep
   hand-written `scan_dollar`, `scan_double_quoted`, `scan_word`,
   `scan_ansi_c_quoted`, adjacency tracking (~650 lines unchanged).
   Net: −400 hand-written lines, +declarative token enum.

2. **Stay hand-written**, attack `next_token` complexity with
   table-driven dispatch or sub-extracted helpers.

3. **Defer indefinitely** — accept current state, focus on features.

## Why DG should decide

The postmortem already presents the tradeoff explicitly (correctness:
neutral; maintenance: modest improvement; legibility-as-spec for
auditors: real improvement). The choice is a project-values call, not a
technical one.

## Inputs for the decision

- Current top-10 cyclomatic complexity (run `mk complexity` — top three
  are all in `parse/`)
- `notes/parser-migration-postmortem.md` addendum #2 (~lines 393–469)
- The postmortem-addendum-#3 precedent: DG explicitly chose to take
  the larger refactor (Pratt parser) rather than ship the shortcut.
  That bias matters here too.

## Output

A new `notes/decisions/DD59-*.md` (or amend an existing DD) recording
the choice and rationale. If the choice is logos, P1 is partly
mechanical; if hand-roll, P1 is harder but no new dep.
