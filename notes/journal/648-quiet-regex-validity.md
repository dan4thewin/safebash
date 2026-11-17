title: Quiet actual-engine regex validity
date: 2026-11-17

# Quiet actual-engine regex validity

DG's integrated DD131/DD132 trial now has `r?`, following the coarse
selectors at `3ddaab55`. The AI implemented it on `glob-scratch-trial`;
main remains `6c7b899f`. Mandatory immutable-pattern proofs, caught-scratch
permissions, regex text/boolean results and `rglob` still follow.
This bounded unit does not adopt the provisional package on main.

## One engine, two reporting paths

The existing unary-test parser and both backends recognize `r?`.
Ordinary conditions, full/shorthand attests and array/map attest-all
use the same runtime predicate. An ordinary predicate does not clear
taint. A clean candidate still gets its actual validity check.

`sash_is_regex` rejects any NUL in the counted candidate, copies exactly
its supplied bytes into a terminated buffer and invokes the real engine.
Empty candidates remain valid. The new `regcompcheck9` entry suppresses
syntax diagnostics while retaining the engine's existing local jump and
failed-program cleanup. Invalid syntax returns false, including when
the candidate was a malformed literal. Literal pattern use still gets
the existing static SB5023 validation.

The quiet entry separately reports an allocation failure's requested
byte count. The vendored engine uses fallible `sash_malloc`, so a null
program alone cannot distinguish invalid syntax from OOM. Candidate-copy
and engine allocation failures both follow fatal ERR routing, including
under negation and result discard; neither becomes an invalid verdict.
Existing non-quiet engine entrypoints retain their diagnostics.

The candidate copy and a successfully compiled program are both freed.
No program handle, cache or later-consumer lifetime architecture is
selected. A short engine program still needs roughly 34 KB because its
structure reserves 128 character-class records. A 16K heap separates
engine OOM from syntax rejection; 500 valid/invalid/empty cycles fit
64K when both temporary allocations are reclaimed.

Both backend unary dispatchers retain their unknown-operator ICE.
Flattening their file-test fallback leaves them at cc10/cog1 under the
existing complexity rubric; no gate-policy exemption was added.

## Two coupled parser defects

A trailing backslash previously advanced the parser beyond its terminal
NUL. The frozen compiler accepted that literal pattern and the runtime
matched it. `nextc` now rejects the escape before reading the absent
following character.

Repetition-count accumulation previously checked DD39's limit of 32 only
after reading all digits into an `int`. `4294967297` wrapped to one and
was accepted, both as a lower and an upper bound. Each digit now checks
the limit while the preceding value is at most 32, before multiplication
can overflow. Static validation and runtime validity share this repair.
The existing counted-repeat dialect and error messages remain.

An investigated literal-brace concern was not a regression: the caller
already rejects `parsecounted` failure rather than treating malformed
brace text literally. Both frozen and repaired controls reject
`^a{1,4294967297x}$`. No extra rewrite or spec was made for that false lead.

## Correcting the evidence

The draft had four invalid ERR fixtures: `echo fatal` itself required
explicit handling in ERR and received SB5101. They now use `- echo fatal`,
so their assertions observe the intended runtime OOM rather than a
compile-time refusal.

The first length-boundary fixture used a substring, but that runtime
operation already copies into a terminated buffer. The final fixture
uses the one-byte result of `match` on `a(`. That is a genuine slice of
the original subject: the following backing byte is `(`, not NUL.
`r?` accepts the slice and rejects the whole subject.

The draft map fixture revealed an independent pre-existing parser defect:
`map m=([a]=b ["("]=good)` silently loses its second entry. The pair
recognizer misses the quoted component; when any pair is recognized,
`parse_array_literal` discards its separately collected ordinary elements.
The frozen compiler prints `1:b` for the independent size/join control.
The unchanged parser has a new P2 owner,
[bug-map-literal-drops-elements](../tasks/bug-map-literal-drops-elements.md).
The final predicate fixture uses the confirmed-present invalid key `+`;
its two-entry control rules out mistaking lost input for skipped checking.

All 158 finalized regex assertions pass in each backend. The matched
frozen compiler/runtime fail exactly the 23 new `regexvalid.1`-`.23`
assertions in each mode; all 135 retained assertions pass before.
The linker helper's argv records select this unit's frozen archive in
both modes, and an actual native archive-open trace confirms that path.
An initial private checker compared `/home/dg` with its resolved
`/mnt/dg` spelling; resolving both paths fixed the checker, not the
compiler or the frozen-link setup.

## Independent reversions

Each row was applied alone and run against the finalized regex file.
Both backends produced exactly these red assertions:

| Disabled mechanism | `regexvalid` assertions |
|---|---|
| Quiet syntax reporting | 2, 3, 5, 8, 9, 11, 12, 16 |
| Counted-NUL rejection | 4 |
| Passing the counted copy rather than original backing | 5 |
| Fatal engine-allocation distinction | 17, 18, 19 |
| Successful-program release | 16 |
| Candidate-copy release | 16 |
| Trailing-backslash rejection | 3, 21 |
| Per-digit lower-bound limit | 3, 22 |
| Per-digit upper-bound limit | 3, 23 |

Every mutation is restored. Source, compiler, runtime archive and both
local lockfiles match the accepted positive identities before and after
the ordinary gate.

## Acceptance and remaining boundary

```text
NEW=46 FLIPS=0 RETIRED=0 TOTAL=11999
rows 7123097-7135095
native 6111 + C 5888
11986 passing + 13 existing skips
```

The sweep took 281 seconds; liveness is 249. No ghosts, plan discrepancies
or failures remain. All 11953 selector-baseline statuses are unchanged.
The unit suites pass 239/27/20. The ordinary commit hooks enforce
formatting, Clippy, totality and touched-function complexity without
bypass.

Evidence lives in the session's `files/glob-trial-20261117/regex-valid/`:
frozen artifacts, final before/positive TSVs, linker/open evidence,
per-reversion xtraces, restored identities and gate log.
DD39/DD93/DD132 delivery notices, GUIDE, status and the integrated owner
now distinguish the predicate from the remaining proof/result work.
No dependency, coverage measurement or main-branch change is included.
The full trial and its useful-program/migration/memory evaluation remain
active.
