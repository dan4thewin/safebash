title: "bug: `while read ARR` never terminates — reject it (DD#99)"
status: done
priority: P1
kind: bug
depends: []
gates:
  - "`while read ARR; do … done` is REJECTED with an SB code naming `while readline v`"
  - "`while readline v` keeps working (it already does)"
  - "a spec pins the rejection, and the diagnostic says what to write instead"

## Found

2026-09-08, while comparing `read`/`readline` design options.

## (a) `while read -a` is a silent no-op

```sh
array l=()
while read -a l; do
  attest e in l [[ $e =~ ^.*$ ]]
  echo "ITER"
done
echo "AFTER"
```

Input `foo\nbar\n`. sash prints only `AFTER` — the body never runs. bash prints
`ITER` twice. It compiles cleanly: no diagnostic, no SB code.

Likely cause: `is_lastpipe_read_last` explicitly excludes `-a`
(`n == "read" && !args.iter().any(|a| a == "-a")`) and `is_mapfile_last` peels
array reads off elsewhere, so the `while`-condition shape falls between the two
handlers and lowers to nothing. Either shape is fine to reject — silently
succeeding while doing nothing is not.

## (b) `read -a`'s default separator is space, not whitespace

| input | elements |
|---|---|
| `a b c` | 3 |
| `a<TAB>b` | 1 |
| `a<NL>b` | 1 |

The code and comments describe the default as a whitespace split
(`ReadLineMulti`: "split on whitespace into N vars"), and bash splits `read -a`
on IFS (space/tab/newline). sash has no IFS (DD#22), so sash is free to define
this — but it should define it *once* and say so. Today "whitespace" means
space only, and nothing pins tab/newline behavior.

## Note

Both sit inside the `read`/`mapfile` family that `switch-free-read-family`
proposes to replace. If that redesign lands, (a) and (b) may be resolved by
construction — but neither should wait on an open decision, and the redesign
needs these behaviors pinned as specs either way.

## Re-verified 2026-09-10 — the original description was wrong twice

At HEAD the spelling is `while read ARR` (DD#99 removed `-a`), and it does not
run **zero** iterations:

```
n=0
n=0
n=0
n=0            <- forever
```

It is an **infinite loop**, spinning on an empty array. Worse than filed, and
the "`read -a` splits on space only" half of the title is moot — that flag is
gone.

`while readline v` is correct and unaffected (`got a`, `got b`, `done`).

## Decided (DG): **reject** (option A)

`read ARR` consumes the whole stream by design, so looping over it is a
category error — there is no second read for the loop to make. The construct
has no meaning anyone can state in a sentence, which is the strongest argument
against keeping it in any form.

Considered and rejected: giving it a per-iteration meaning (bind the next
NUL-separated field), which puts a scalar's worth of data in an array; and
fixing EOF only, which would leave a construct that terminates but still means
nothing.

`while readline v` is the iteration form and already works. The diagnostic
must name it.

## Resolved 2026-09-11 — SB5069 (journal 265)

`validate_while` now rejects a `read` in a loop condition. The check walks each
condition statement, so it catches the pipeline form (`tr … | read ARR`) as
well as the bare one, and it fires for `until` too — the defect is looping on a
slurp, not the sense of the test. The message names `while readline v`.

Still accepted, deliberately:

- `if read ARR` — asking whether a read produced anything is meaningful; only
  iterating on it is the category error.
- `read ARR` in a loop **body** — which was separately segfaulting and is fixed
  in the same commit.

DG asked for the counter-argument before implementing. The candidate was
bash's `while read -a arr` per-line field split; it does not survive, because
`read $s ARR` splits on **NUL**, not whitespace (`read $s ARR` on `"a b c"`
yields one element). There is no per-line field split in the language for
`while read ARR` to be the spelling of, so `readline v` is the whole
replacement.

Specs `commands.sh` 14.41–14.46.
