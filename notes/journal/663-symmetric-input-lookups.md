title: Delivering symmetric input lookups
date: 2026-11-19

# Delivering symmetric input lookups

DG requested **"do the getenv/arg backlog"**. The language choices were
already settled in DD101/DD60 and journals 629/631: both bare lookups expose
text plus presence and require handling or explicit discard; both
literal-default forms expose only the selected text. DG chose that contract.
The AI implemented it, migrated its clients and completed the delivery proofs.

The baseline was `9cb782c0`, with accepted spec history ending at row 7296396.
This work does not implement the independently queued `sash-expand` loader,
general suppression cleanup, named-predicate captures or recoverable file reads.

## One lookup contract, two input sources

`$(arg N)` now behaves like `$(getenv NAME)` for presence: a supplied value,
including an empty string, succeeds; absence supplies empty text and a
nonzero result. Neither bare form may silently drop that result.
`x=$(arg 1) || fail 1` expresses required input; `x=$(- arg 1)` deliberately
uses only text. The same choices apply to environment lookups.

`$(arg N DEFAULT)` and the new `$(getenv NAME DEFAULT)` choose the literal
default only on absence. An explicitly empty default still selects the
value-only form; a present empty value is not replaced. There is no status
to test or discard, no new status-read permission, and no overwrite of an
already legal status read. All four results remain tainted.

The shared `lookup_has_status` predicate classifies the legal form by name
and operand count. Existing capture/result carriers suffice: no new SSA
variant or result-packing ABI. The transform raises located SB4018 for an
unhandled bare lookup, retaining the existing regex checks. Validation
recognizes bare lookup capture-conditions and refuses defaulted or
source-discarded ones with SB4025. SB4076 identifies inert default prefixes
and a nonadjacent prefix on a lookup-containing binding.

Source discard remains distinct from internal handled/unchecked flags.
Handling an outer call or capture cannot discharge an operand/body lookup,
and a later status read is not retroactive handling. Typed declarations,
ordinary expressions, intrinsic placement and lexical-body rules are unchanged.

Each backend now has one shared input-lookup lowering path. It evaluates
the name/index once, calls the getter once, tests pointer presence rather
than text length, and selects the value/fallback. Only a bare lookup writes
the presence status. The runtime getter returns absence for a valid high
argument index, but still aborts for a negative computed index. The former
separate argument-default getter and its declarations are removed.

## Coupled repairs

An absent getter uses NULL to communicate absence internally. Publishing
that pointer as a string was not harmless: the collection representation
interprets a NULL value pointer as an integer cell, so joining a stored
missing lookup printed `0`. Both backends now publish genuine nonnull
empty-string backing. `lookup.16/.17` pin the two sources.

The lookup validator now uses `ir::single_value_command` rather than
matching the raw first statement. That repairs B47: source suppression
can no longer hide a negative/malformed index or nonliteral default.
`lookup.43/.44/.63` pin SB5038, SB5078 and SB4070. The default assertion
also excludes SB4076, so a later refusal of the now-statusless prefix cannot
masquerade as proof that default validation ran.

Terminal analysis joins the default's NUL risk into lookup output.
It distinguishes invalid indices from absence and retains exact canonical
integer facts through scalar definitions and numeric SSA expressions.
Otherwise a safe computed high index in ERR would be falsely classified
as potentially fatal. `lookup.62` covers both a named integer and an
arithmetic-literal index without adding a general arithmetic evaluator.
The invalid-index guard stays active while those precision paths are reversed;
disabling both would hide the false rejection.

A final boundary probe found that the initial outer-prefix guard only
recognized a direct capture RHS. Quoted and concatenated default lookups
still accepted `- x=...` and ran normally. The guard now uses
`ir::walk_all_exprs`, covering those and nested expression shapes rather
than adding another private walk. `lookup.64/.65` fail on the frozen
pre-repair positive compiler and pass with the shared traversal.

## Client migration

Converter tilde, environment and positional hoists now emit explicit text
projection, such as `home=$(- getenv HOME)` and `arg1=$(- arg 1)`.
That preserves a Bash expansion's value-read intent; conversion does not
invent a required-input assertion. Existing attest scaffolding stays.
`lookup.61` converts `echo ~ "$HOME" "$1"` and compiles/runs the actual
result with normal taint checking in both modes.

Incidental text reads in 23 spec files use explicit inner discard.
Required-input assertions instead use the new presence consumer.
Eight old IDs with changed/obsolete contracts retire in each mode:
`dd101.12`, `av.t6`, `via.2`, `42.6`, `lazy.11`, `migratea.49`,
`supval.34` and `supval.35`. They are not reused.

Current DD delivery notices and active backlog guidance agree with the
implementation; historical measurements remain historical.
The lookup task closes, B47 is checked off, and the general suppression,
peel-family and SB4025 tasks record only their delivered lookup subset.
The tour explains presence separately from validation and defaults.
Its generated converter example now shows explicit discard; all 70
authored programs retain their expected outcomes. No new architecture
required a GUIDE rewrite.

## Evidence and corrections

A compiler frozen before the runtime change is not by itself a matched
baseline. Session-only linker wrappers select the frozen archive for
both native and C, and actual archive-open traces establish that pairing.
The final 65 lookup cases run exactly once per mode against that original
pair: **53 red and 12 preservation cases**. The passing before cases still
need reversal evidence where they protect the new mechanisms.

Eleven deliberate reversion campaigns were completed before the final
binding-prefix repair. Some group mechanisms with separate detectors;
they are not eleven individually isolated source edits. Failing-ID sets
were compared in full, with native and C run serially:

| Campaign | Removed or altered behavior | Failing checks |
|---|---|---|
| Required result | Bare lookup obligation | `lookup.3-12,49,55,56`, 13/mode |
| Literal default | Default expansion rejection | `lookup.22-24`, 3/mode |
| Source forms | Binding prefix, discarded consumer, default prefix | `lookup.25-28,30,31,34,35`, 8/mode |
| Result kind | Form-sensitive status and canonical extraction | `lookup.1,29,32,33,43,44`, 6/mode |
| Backend A | Native length-based absence; C default status write | Native `dd101.3,lookup.14,18,19`; C `lookup.36` |
| Backend B | Native default status write; C length-based presence/selection | Native `lookup.36`; C `dd101.3,lookup.14,18,19` |
| Empty string | Nonnull missing-string backing | `lookup.16,17`, both modes |
| Absence/wrappers | High-index absence and canonical validation | `lookup.15,17,19,20,21,36,37,39,43,44,50,62,63`, 13/mode |
| Runtime/proofs | Negative-index fatality, scalar facts, default NUL risk | `lookup.45,46,52,62`, both modes |
| Expression/converter | Numeric expression facts, default check, three hoists | `lookup.22,23,24,61,62,63`, both modes |
| Invalid index | ERR invalid-index hazard classification | `lookup.53,60`, both modes |

The source-form prediction initially omitted `.34/.35`. Their xtraces
showed that the same classifier also controls fresh status-read permission;
the revised eight-case set matched C. The original prediction is retained,
not rewritten as though it had included them.

The final quoted/concatenated pair has a separate control against the saved
pre-repair positive compiler: exactly
`lookup.64/.65` fail per mode, and all 110 capture-condition checks pass
after repair. All deliberate mutations are restored. Final source,
compiler/converter/runtime and both local lock identities are recorded
separately from the earlier positive snapshot.

The first gate stopped at stale converter unit expectations. After their
migration, the first full sweep recorded ten failures: the ERR integer-fact
precision case, two diagnostic columns shifted by explicit `- `, and the
two obsolete `supval` fixtures, all dual-mode. Those were corrected or
retired by purpose, not waived. Later focused runs used private TSV files,
leaving spec.db's failed sweep available for the final regression comparison.

## Accepted gate and remaining owners

The final command was:

```sh
timeout 1200 env NEW=6 FLIPS=6 RETIRED=4 TOTAL=12687 mk -p 1 gate
```

It passes at rows **7309082-7321768**: **12674 passing, 13 existing skips,
zero failures**, native 6455/C 6232. All six expected failure-to-pass flips
are `lookup.62`, `capwalk.2` and `atpred.30`, in both modes. The four expected
retirements are `supval.34/.35`; the six first-ever IDs are
`lookup.63-.65`. Plans, literal/variable ID collisions, both SB-census
directions and registry liveness pass; the liveness count is 246.

Against the original accepted 12573-check baseline, **all 12557 retained
statuses are unchanged**, exactly 130 new IDs are present, and exactly
16 old IDs retire. Comparing strictly earlier rowids prevents the dirty
working tree's shared commit hash from self-comparing.

The gate includes 241 compiler, 27 converter and 20 expander unit tests.
Normal fmt, Clippy with tests, totality and staged-hunk complexity checks
pass without bypass. `mk -p 1 tour-doc`
regenerates the GitHub edition, and `target/tour/check` independently reports
70 snippets with no mismatch. No dependency refresh or fresh coverage run
was performed.

Session evidence is in `files/lookup-contract-20261119/`: matched frozen
toolchains/link traces, before and reversion TSVs, restored/final positive
identities, both gate logs, exact baseline accounting and tour results.
The broad suppression, predicate/file capture, anonymous-return and
diagnostic-advice tasks remain independently open. This delivery does not
claim their general contracts are implemented.
