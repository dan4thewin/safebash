# 636 - Publishing the outline, tour, and policy work

DG asked for a series of coherent commits covering the accumulated work,
noting his edits to the tour's `.sb` files and the new `mkfile` target.
This supersedes the earlier draft-only publication boundary. It does not
authorize implementation of the recorded policies or resumption of coverage.

## Commit sets

The AI chose the grouping and preserved DG's current helper sources.

| Commit | Scope |
| --- | --- |
| `d137befe` | Indexed-update evaluation, IR schema 3, complete long `readline` records, and their specs |
| `9d4a9fc1` | Symmetric argument/environment lookup policy and implementation backlog |
| `86912c27` | DD134 element-typed loop-binder policy and backlog |
| `356c68d9` | DD135 readonly ERR inputs, NUL/path predicates, `mkdtemp`, and backlog |
| `ff090665` | Pedagogically reordered specification outline and its record |
| `b965441c` | Executable presenterm deck, configuration, and tour-polish record |
| This commit | DG's native helper versions, corrected `tour` target, and publication closeout |

Both Rust repairs land together because the pre-commit totality scan includes
the separate runtime workspace. All modified Rust sources therefore match
the first commit's staged source, rather than checking an uncommitted second
repair. The original independent before/reversion evidence remains in
[journal 633](633-indexed-updates-evaluate-once.md) and
[journal 634](634-tour-helpers-in-safebash.md).

Shared lookup/DD135 documents were staged from the verified pre-DD135
snapshot for the lookup commit, then from their final versions for DD135.
Journal 633's repair narrative accompanies the repairs; its later tour
sections accompany the deck. These are index-only slices: no working-tree
draft was rolled back or overwritten to make the commits.

## Preserved inputs and target repair

The publication baseline contained 65 inputs. Sixty-one match the completed
earlier reviews byte for byte; the four newer inputs are DG's three `.sb`
files and `mkfile`. All three helper sources retain their baseline bytes.

The four formerly untracked shell helpers were already absent at this
baseline: `docs/tour/check.sh`, `docs/tour/sash-snippet`, `docs/tour/tour`,
and `docs/tour/tour.sh`. This publication neither deleted nor restored them.
Journal 634 describes the earlier side-by-side state, not the current file
inventory. Current launch/check routes are `mk -p 1 tour` and
`target/tour/check`; the latter accepts `-v` for successful snippet output.

The new target needed two small safeguards. The compiler does not create
the parent of its output file, so the recipe now creates `target/tour`.
Every step is joined with `&&`, preventing a failed compilation from falling
through to a stale launcher. The configuration's first comment now points
to `mk tour` rather than the absent shell launcher.

## Current evidence

The exact original recipe was exercised in isolated fixtures. Without its
output directory it failed. Injecting failure at each of its three compiler
calls nevertheless ran the stale launcher and returned success. The corrected
recipe succeeds from a fresh directory and stops at each injected failure
without running later compilers or the launcher.

The actual `mk -p 1 tour` compiled all three current sources and launched a
controlled presenterm endpoint with the expected deck/configuration arguments.
The rebuilt launcher separately reached the installed presenterm's `--help`.
The real native checker reports **55 snippets, 0 mismatched**.

Four current-helper controls also pass: a directory containing spaces with
the expected `world 42` program arguments; propagation of runtime exit 7;
compiler diagnostics and exit 1 without running an existing stale snippet;
and checker exit 1 for one unexpectedly failing snippet. These are current
controls, distinct from journal 634's earlier 26-invocation measurement.
A traced run with an owned `TMPDIR` also confirms that the checker creates
and removes its temporary directory when exiting 1.

The ordinary gate passed with the prediction:

```sh
NEW=0 FLIPS=0 RETIRED=0 TOTAL=11375 timeout 900 mk -p 1 gate
```

Rows **6924876-6936250** contain 11,362 passing checks and 13 existing skips,
with native/C counts 5,799/5,576. There are no failures, flips, first-ever
IDs, retirements, ghosts, or plan mismatches. Liveness is 251. The included
unit runs pass 239 compiler, 27 converter, and 20 expansion tests.
The separate runtime formatting check also passes.

The sweep precedes these commits, so its recorded HEAD is `2ef86c2a`.
Its compiler/runtime/spec source bytes are the ones published in `d137befe`.
No new negative-control count is claimed for the unchanged Rust repairs:
their original before/reversion results remain applicable by byte identity.
All 48 pre-closeout Markdown documents render, their 128 local links resolve,
and the outline retains 21 correctly numbered topics and 417 items.

Private evidence is under session `files/publish-review-20261115/`: the
immutable baseline, prior-review reconstruction, recipe/helper controls,
gate log, and per-commit staged trees, file hashes, and hook logs.

## Boundaries

The lookup symmetry, DD134, and DD135 changes remain policy and backlog,
not shipped compiler features. The outline remains a draft that models
pending/provisional rules as complete only within that document.
The indexed-update repair does not complete general expression ordering;
the reader-capacity repair does not complete recoverable file failures.

Publication targets `spec-outline`, not `main`, through ordinary hooks and
a normal push. No amendment, history rewrite, coverage run, dependency
refresh, or deferred implementation belongs to this series. Any coverage
figure printed by the existing pre-push hook is its last-run report.
