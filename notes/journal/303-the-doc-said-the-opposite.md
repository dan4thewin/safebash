title: 303 — the doc said the opposite, and a task I had to stop

Two tasks this stretch: one that turned into a documentation bug, and one
that turned out to be blocked on DG.

## `spec-db-clause-text` — stopped, not done

Filed 2026-09-07 with three gates, one marked "FIRST (independent of the
above)": export the clause text to a version-controlled seed, because it
lived only inside a 68 MB binary `spec.db` and was unreproducible.

That is **already done**. `spec/clauses.tsv` is tracked, 225 rows, and
`run.sh` applies `schema.sql` and imports the TSV before recording anything,
so a fresh clone rebuilds the database correctly. It happened on 2026-09-12
and the filing did not know.

What is left is the two gates marked DECIDE, and they are genuinely DG's:
whether to author clause text against today's chronological ids and re-key
later or wait for the specification document's logical scheme, and what `dd`
value the non-DD-numbered suites (`cov.*`, `vm.*`, `dp.*`, …) get. So I
re-measured, corrected the filing, and moved it to `blocked`.

The re-measurement is the part worth keeping:

| | filed 2026-09-07 | now |
|---|---:|---:|
| unbacked clause ids | 1255 | **2029** |
| tests behind them | 3703 | **4000** |
| tests *with* a backing clause | 754 | **509** |

The gap grew 62% and the backed count *fell*. The filing's own remedy was
"when a DD is touched, add its clause rows in the same commit" — and I did
not do that in any of the four commits I made before reading this task. That
is not an accusation of anyone; it is evidence that opportunistic authoring
does not happen while the keying is provisional, because nobody wants to
write 2000 rows against ids that are expected to move. Which is exactly why
it needs deciding rather than leaving `ready`.

Marking it `ready` invites the half-done authoring that widened the gap.

## `docs-tail-recursion-args-model` — the doc asserted the opposite

Two of four gates were moot. `SUBSET.md`, which gates 2 and 3 targeted, was
**removed on 2026-08-29 for having drifted from the DDs** — the same failure
this task was filed to fix, one level up. And gate 3 said the positional docs
should show `$# ≡ ${#args}`, which is the identical wrong sentence
`converter-dollar-at-migration` carried three days ago: `${#args}` is SB4038,
and the repair is not `$(len args)` either, because DD#46 keeps `$#`
sash-native.

One wrong sentence, inherited by two filings. When you correct the code, also
correct the filing — otherwise the next task to quote it repeats the work.

The real find was under gate 1. The example runs (`fact 5 1` → 120). But
writing up the *policy* around it meant reading what the docs already said:

> `GUIDE.md:459` — "Depth is static (SB5037 rejects recursion, and functions
> are inlined)."

SB5037 does not reject recursion. It rejects everything *except* a direct
self tail-call, which is compiled to a loop. `SB5037`'s own message says so —
"sash inlines functions, so only a direct self tail-call…". The diagnostic
was right; the architecture document asserted the opposite of the truth about
the mechanism it was describing, in a parenthetical, where nobody would look.

That parenthetical is the same species as journal 301's whole subject. A
message is a string that nothing compiles; a doc sentence is a string that
nothing compiles. Both rot silently, and both rot toward *over*-claiming a
restriction, because the restriction is what the author was thinking about
when they wrote it.

## What I did about it

`#### Functions, inlining, and recursion (DD#27)` in `GUIDE.md`, and a new
`.SH FUNCTIONS AND RECURSION` in `sash.1`, which had no functions section at
all. Both carry the accumulator example and the boundary table: tail
self-call → loop; non-tail self-call → SB5037; mutual recursion → SB5037.

The non-tail row earns its place. `return $(( n * (f m) ))` is what a direct
translation of the textbook factorial produces, and the reason it cannot
become a loop — the multiply happens *after* the call returns — is the whole
explanation a reader needs.

Specs `rec.1`–`rec.4` pin all of it. They are characterization tests: they
pass at HEAD, because they assert what the docs now claim rather than testing
new code. `rec.2` is the load-bearing one — 100,000 recursive calls returning
100000. No inliner survives that, so it *demonstrates* the loop compilation
instead of asserting it.

Sweep: **4508 passing, 9 skipped, 0 failing, 0 flips.**
