---
title: "spec.db clause text: 1255 clause ids have no requirement text"
status: blocked
priority: P3
kind: effort
depends: []
blocked-on: "DG — two DECIDE gates below (id keying, and the `dd` value for named suites)"
gates:
  - "The consumer is the eventual specification document (`specification-document`, DG 2026-09-07) — so the text is wanted, but its KEYING is provisional"
  - "DECIDE: author against today's chronological ids and re-key later, or wait for the document's logical id scheme"
  - "DONE (2026-09-12): the seed exists — `spec/clauses.tsv`, tracked, 225 rows, applied idempotently by `run.sh` alongside `spec/schema.sql`"
  - "DECIDE: what `dd` value the named suites (`cov.*`, `vm.*`, `dp.*`, …) get — they are not DD-numbered but most map to a governing DD"
  - "Each slice: clause rows added, `SELECT COUNT(*) FROM tests WHERE NOT EXISTS (…clause…)` drops, `PRAGMA foreign_key_check` shrinks"
---

# Clause text for the 1255 unbacked clause ids

Surfaced 2026-09-07 while repairing spec.db (journal/status same date). Fixing
the `clause_id` derivation relinked **231 clauses / 754 tests**; the rest have a
`clause_id` with no row behind it.

## Scope

| bucket | clause ids | tests |
|---|---:|---:|
| DD-numbered (`14.7`, `60.3`, `94.1`, …) | 571 | 1731 |
| named suites (`cov.*`, `vm.*`, `dp.*`, `cf.*`, `av.*`, `gs.*`, …) | 684 | 1972 |
| **total** | **1255** | **3703** |

Two things this makes clear:

- It is **not** simply "DD#59 onward is missing". 21 of the 38 *seeded* DDs are
  only **partially** covered — the table was seeded once and the specs kept
  growing (e.g. DD#14 has clauses, but 22 more `14.x` ids do not).
- The named suites are the larger half. The biggest are `cov.*` (92 ids),
  `vm.*` (67), `dp.*` (44), `cf.*` (44), `av.*` (44), `gs.*` (35), `nl.*` (31).

Entirely unseeded DD families: 0, 26, 27, 60, 62, 64, 67, 74, 75, 77, 80, 81,
84, 86, 88, 89, 91, 93, 94.

## The consumer (was gate 1, now answered)

**Nothing reads `clauses` today** — no script, no report, no `mk` target; the
only consumers of spec.db use `test_status`. That made "is this worth writing?"
the first question.

DG answered it (2026-09-07): *"there will ultimately be a specification
document, presumably with a logical ordering and id scheme, built from the DD
and spec work."* The clause text is an **input** to that document — see
`specification-document`. Its most valuable direction is the inverse query,
"which clauses have no test?", i.e. untested requirements.

**What that changes here:** the text is wanted, but the **keying is
provisional**. Clause ids today mirror spec test ids, which are DD-numbered by
*chronology*, not logic; the document will have its own scheme, so expect a
re-keying step. Practical consequence — author text freely, but do not build
tooling that assumes `clause_id == test id` permanently, and keep the text in a
form that survives re-keying (which is exactly what the seed file below gives).

## Fragility to fix regardless (gate 2)

The 231 existing clause texts live **only inside `spec.db`**, a 68 MB binary
blob. There is no seed file and no insert script anywhere in the repo — they
were added ad hoc and are unreproducible. Any rebuild-from-scratch loses them.

Cheap and worth doing on its own merits:

```sh
sqlite3 spec.db ".mode insert clauses" "SELECT * FROM clauses ORDER BY id;" \
  > spec/clauses.sql
```

then have `run.sh` (or a `mk` target) apply it idempotently with
`INSERT OR IGNORE`. That makes the text diffable, reviewable and restorable,
and gives new clauses an obvious home.

## The `dd` column for named suites (gate 3)

`clauses.dd` is `INTEGER NOT NULL`, but `cov.*`/`vm.*`/`dp.*` are not
DD-numbered. Most do map to a governing DD (`dp.*` → the dispatch DDs #66/#71,
`gs.*` → DD#69 globs-as-arrays, `vm.*` → the var-modifier DDs, `cf.*` →
control flow). Options: map each family to its DD, or use a sentinel (0) for
"suite, not a DD clause". Wants a decision before bulk authoring.

## Suggested slicing

Opportunistic rather than a campaign: when a DD is touched, add its clause rows
in the same commit (DD#93 and DD#94 would have been ~47 ids between them). The
in-repo seed file from gate 2 is what makes that a one-line addition per spec.

## Related

`specification-document` — the eventual consumer, and the source of the final
id scheme.

The residual is *not* an integrity fault — FKs are unenforced
(`PRAGMA foreign_keys = 0`), so an unbacked `clause_id` is inert. The
alternative to authoring text is making `clause_id` nullable and NULLing the
unbacked ones for a clean `foreign_key_check`; that discards the "which clause
*would* apply" hint, so it is only worth it if gate 1 comes back won't-do.


---

## Re-measured 2026-09-14

**Gate 2 is done and the filing did not know it.** `spec/clauses.tsv` is
tracked (225 rows — the original 231 less DD#15's six orphans, dropped in
`219a8bc`), and `spec/run.sh:20-23` applies `schema.sql` then imports the TSV
before recording anything, so a fresh clone with no `spec.db` rebuilds it
correctly. The export recipe is in `AGENTS.md`. Nothing to do here.

**The gap has grown 62% since filing.**

| | filed 2026-09-07 | now |
|---|---:|---:|
| unbacked clause ids | 1255 | **2029** |
| tests behind them | 3703 | **4000** |
| tests with a backing clause | 754 | **509** |

Split today: 957 DD-numbered ids (1839 tests), 1072 named-suite ids (2161).

Backed *fell* because DD#15's orphans were dropped while the suite grew from
~3100 to 4509 tests. The filing's own suggestion — "when a DD is touched, add
its clause rows in the same commit" — has not been happening. I did not do it
either across the four commits before this one, which is the honest evidence
that opportunistic authoring does not work without the keying question
settled: there is no point writing rows against ids that are expected to be
re-keyed.

## Blocked on DG — two questions, both consequential

Both were already marked DECIDE in the original gates and neither has been
answered. Stating them plainly so they can be answered quickly:

**1. Key clause text against today's chronological ids, or wait?**
Clause ids currently mirror spec test ids, which are DD-numbered by the order
things were built, not by logic. DG has said (2026-09-07) the specification
document will have its own logical scheme. So either:
- **(a) author now against today's ids** and accept a re-keying pass later.
  2029 ids is a large body of text to move, but the TSV makes it a mechanical
  transform and the text itself survives.
- **(b) wait for the document's scheme.** Nothing is lost meanwhile except
  time — the `clauses` table has no consumer today.

(a) only pays if the re-key is genuinely mechanical, which depends on whether
the document's ids are a *renaming* of these or a *regrouping* of them. That
is DG's call and not knowable from here.

**2. What `dd` value do named suites get?**
`clauses.dd` is `INTEGER NOT NULL`, and `cov.*` / `vm.*` / `dp.*` / `cf.*` /
`av.*` / `gs.*` / `nl.*` are not DD-numbered. Most map to a governing DD
(`gs.*` → DD#69, `dp.*` → DD#66/#71, `vm.*` → the var-modifier DDs, `cf.*` →
control flow). Either map each family, or use a sentinel `0` for "suite, not a
DD clause". This one is cheap to reverse and could be decided either way, but
it should be decided *before* bulk authoring, not during.

Until (1) is answered, this task should stay blocked rather than ready —
listing it as ready invites exactly the half-done authoring that widened the
gap.
