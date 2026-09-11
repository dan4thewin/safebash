# 267 — the converter learns to read, and a table that meant the other thing

*2026-09-11*

`sash-convert-read-dd99` was the last P1 outside the janet backend. DG picked
it over the three janet bugs: it is the only one that leaves a shipped tool
emitting programs that do not compile.

Two of the filing's premises turned out to be wrong, and finding that out was
most of the work.

## "The converter has no command-rewriting path"

The filing said this, and it is why the task was scoped as a real addition
rather than an edit. It is false. `let` → `(( ))` and `trap` → `signal N IGN`
have both replaced whole command nodes via `Patch::replace_node` since DD#57.
I wrote that sentence during stage 6 without checking, and it sat in the
backlog for a day making the task look bigger than it was.

Thirty seconds with the symbol table would have caught it. The lesson is not
"check before filing" so much as **a filing's claims are not evidence** — I
read my own note as if it were a finding.

## The table that meant the other thing

DD#99 has a "What is retired" table, and the filing's mapping section copied
it. It has a row:

| `read -a arr` | `read arr` (kind from the declaration) |

Copy that into the converter and bash `read -a arr` becomes sash `read arr`.
That is wrong, and wrong in the silent way:

- **bash `read -a arr`** reads **one line** and splits it on **IFS**.
- **sash `read arr`** slurps **all of stdin** and splits it on **NUL**.

Different extent, different separator, no error. The table is a **sash → sash**
rename map. Its rows describe migrating *in-tree sash code* off the old
spellings — and the tell was right there in the same DD: "`read -f:` has 4
in-tree uses and `read -a` 26." Those 26 were sash's own `read -a`, which
already split on NUL. The flag was redundant with the declaration; that is all
the row ever said.

So the bash mapping had to be derived from bash semantics instead. The general
shape of the mistake is worth naming: **a migration table and a conversion
table look identical and are not interchangeable.** One renames a spelling
within a language; the other crosses a semantic boundary.

## `readline` is `IFS= read -r`

Measuring the "obvious" conversion found a third thing. On the line
`"   spaced   "`:

```
bash  read -r ln        →  [spaced]
sash  readline ln       →  [   spaced   ]
```

Bash's `read` strips leading and trailing **IFS** whitespace. `readline` takes
the line verbatim. The exact equivalent is `IFS= read -r ln` — which is also
the spelling careful bash already uses, so the common idiom
`while IFS= read -r line` converts *exactly* and the sloppy one does not.

DG ruled: convert both, `# CHECK:` the non-`IFS=` form only. The `IFS=` prefix
is a child of the command node being replaced, so it vanishes with it — no
`IFS= readline x` is ever emitted.

## The scope ruling

I put four tiers to DG:

1. `read [-r] VAR` → `readline VAR`.
2. `read -a` / `read a b c` → a `readline` + `matchall` expansion.
3. `mapfile -t A [< f]` → `tr '\n' '\0' [< f] | read A`.
4. everything else → `# MANUAL:`.

DG took **1 + 3 + 4**. The distinction that decides it: tier 3 guesses
*nothing* — the recipe is exact for every input shape. Tier 2 has to assume
`IFS` is default whitespace, and a wrong guess there is the failure mode the
converter exists to prevent. Annotating is the honest answer.

Tier 3 had one real subtlety. `mapfile -t a < f` is a `redirected_statement`
wrapping the command, so replacing only the command yields

```sh
/usr/bin/tr "\n" "\0" | read a < f
```

where `< f` has re-bound to `read` — `tr` gets the terminal and `read` gets the
file raw, with its newlines intact. The rewrite has to replace the
*redirected_statement* and carry `< f` onto `tr`. Two redirects, or a `>`, and
it declines to a MANUAL rather than guess which one moves.

A pipeline stage needs no special handling: `cat z | mapfile -t P` replaces
just the command and the producer stays upstream, which is already correct.

## A hint that pointed at the wrong command

With MANUAL notes newly common on `read` lines, the output came out like this:

```sh
readline x
# MANUAL: multi-target `read a b c` …
read a b c
```

— except it didn't. It came out with the MANUAL *above* the ATTEST hint for
`x`, so the note read as a complaint about `readline x`.

An `# ATTEST:` hint is pushed at `line + 1`, meaning "after the line it refers
to". A `# MANUAL:` is pushed at `line`, meaning "before". For consecutive
commands those are **the same insertion point**, and `apply_patches` keeps
intra-line hints in push order — so whichever pass ran first won. `walk_node`
ran before `inject_attest_hints`, so "before" always beat "after".

Swapping the two calls fixes it; neither reads the other's output. Pre-existing,
but invisible until a construct routinely produced both kinds of hint on
adjacent lines. Spec 57.72 pins the ordering.

## The ATTEST hint had to learn the target kind

DG said keep the hint. It said "add a attest with appropriate pattern", which
is fine for a scalar and actively misleading for a collection: a converted
`mapfile` target attested with `attest names [[ … ]]` still fails SB5020, and
the hint is what sent the reader there. It now emits the matching form —
`attest NAME [[ … ]]` for a `readline` scalar, `attest x in NAME [[ … ]]` for a
`read`/`mapfile` collection.

## Two things left where they were

Converting a realistic script surfaced two failures that are not this task:

- `done < f` — a redirect on a compound command is SB4021
  (`converter-compound-postfix`).
- `${names[@]}` — the retired bracket splat (`converter-retire-bracket-splat`).
  It also draws a **wrong** MANUAL note: the expansion converter sees the `@`
  and reports it as the `${var@X}` transformation operator. Noted on that task.

Resisting both was the right call and also the uncomfortable one — a converted
`while read` loop over a file still doesn't compile end to end. What does
compile, and is pinned by spec 57.73, is the piped form: convert, fill in the
attest, compile, run.

## Result

`spec/convert.sh` 57.37 inverted from asserting the passthrough to asserting
the rewrite; 57.56–57.59 and 57.61–57.73 added. All 18 fail against the
unpatched converter. `sash-convert/tests/convert_tests.rs` +9, 27 passing.

Incidental: `convert_tests.rs` uses the same name-keyed temp-path helper that
produced yesterday's `janet_const` flake. All 27 names are currently unique, so
it is not broken — but it is the same loaded gun, one duplicate away.

## A test that had not run in two days

Verifying the sweep turned up an unrelated defect. The regression query's
"new tests" pass listed `janet:60.35` alongside the 17 convert tests — a test
I had not touched.

`spec/taint.sh` guards a block with `if in_modes native,c; then … else
for id in …; do skip_chk "$id"; done; fi`. The skip list had `60.31` and
`60.36` but not `60.35`, which was inserted into the block later. So in janet
mode that test was **neither run nor skipped** — it simply did not exist, and
`taint.sh` emitted 177 of a planned 178.

The plan mismatch could not catch it: `tally` deliberately treats
`plan != index` as an advisory NOTE, with a comment explaining that
`in_modes` guards make per-mode divergence legitimate. That reasoning is
sound, and it names `test_status` as the real source of truth — which is
exactly what the omission corrupted. `test_status` takes each test's
`MAX(rowid)` run, so an unaccounted test keeps whatever row it last wrote.
`janet:60.35` had been reporting **passing** since `9b74bde`, two days and
many commits earlier, for a test that had not executed in janet since.

That form is queryable, so I checked for others:

```sql
SELECT id, status, commit_hash, committed_at FROM test_status
WHERE committed_at < date('<timestamp of the latest run>');
```

Across 1.4M rows it returned exactly one: `janet:60.35`. One instance, not a
pattern. Adding `60.35` to the skip list brings all three modes to 178/178,
and the row now reads `skipped` — which is what it always was.

Worth keeping as a standing check. A green suite plus a stale `test_status`
row is the quietest way for a spec to stop being a spec.
