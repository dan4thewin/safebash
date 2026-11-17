title: Coarse directory selectors and ordered glob lists
date: 2026-11-17

# Coarse directory selectors and ordered glob lists

DG's integrated DD131/DD132 trial now has explicit `glob` and bare-word
list sugar, following atomic accumulation at `3a15ddce`. The AI implemented
the selected contracts on `glob-scratch-trial`. Main remains `6c7b899f`;
this is not adoption of the provisional package. `rglob`, DD132 proof and
result handling, and useful-program/converter/memory evaluation still follow.

## One bounded pattern language

`glob ARRAY PATTERN...` accepts ordinary string operands. The compiler and
runtime share `sash-rt/src/glob_pattern.rs`: split at the last slash, reject
NUL, reject stars in directory components, and allow at most one star in
the filename component. Known violations get SB5006; computed violations
fail the operation. Zero-star exact names are legal. `?` and bracket bytes
in explicit patterns are literal, not additional wildcard operators.

The new runtime directory module uses public rustix `open` and `RawDir`.
Each pattern enumerates one directory, selects names, constructs normalized
absolute paths and sorts that batch bytewise. Pattern batches concatenate
in source order, preserving duplicates. A leading dot is required to select
hidden names; neither exact nor wildcard patterns select `.` or `..`.
Lexical normalization remains distinct from symlink canonicalization.

Path storage and normalization are dynamically sized with checked lengths.
The 300-component cwd case fixes the former component-stack limit. The
4,121-byte result case is a preservation control: the frozen implementation
already returned that complete pathname, so it is not evidence of a newly
fixed 4K truncation.

Successful enumeration, including no matches, returns zero. Failed open,
read or required getcwd retains its actual cause and returns failure when
handled or discarded. Checked failure reports and aborts after releasing
the private paths, argument table and directory descriptor. These operations
have no partially published destination to recover; that is the local
classification, not a blanket ruling about every errno in every API.
Heap exhaustion and index overflow remain fatal.

The runtime builds every pattern's paths before filling its private target.
The existing `CollectionPublish` then appends to an ordinary destination or
installs a completed scratch batch. A failed scratch attempt remains the
new empty instance, including when a pattern operand fails before the
directory call. Ordinary targets retain their contents. No consumed input
or earlier operand effect is rolled back.

## Source provenance and evaluation order

Only a bare literal list word containing `*` activates sugar. Quoted stars,
including decoded ANSI-C strings, and computed strings remain data.
Mixed words containing an unquoted source star reject with SB5029 and
explicit-`glob` guidance. ANSI-C tokens now retain quoted AST provenance;
the compiler no longer guesses glob intent from decoded string contents.

Array construction, replacement, append and glob-containing `for` lists
share the ordered list lowering. It supports multiple glob expansions,
ordinary elements and splats without a whole-list sort or deduplication.
An append publishes the complete list, not each element as it is evaluated.

Explicit pattern operands are retained before later operand effects.
Mixed lists retain earlier scalar and splat values before a later capture.
Within a word, `lift_concat` materializes each nonliteral component before
later lifted work, including integer-to-string conversion. For example,
`"$work$(change)"` retains the old work value; `"$(change)$work"` reads the
new one. Reserved synthetic identities prevent these snapshots from
clobbering source globals or nested calls.

This fixes the shared within-word path and the selector/list prerequisites,
not the complete general argument-list/status-order task. That owner stays
open, as do the separate redirect-timing and output-publication tasks.

`GlobFill` carries the operational result through transparent collection
publication in both backends. Shared binding, signature, immutability,
taint and terminal analyses recognize its target and operand roles.
Patterns are reads, not name operands; successful output is clean and
NUL-free but cannot erase an ordinary destination's earlier taint or bytes.
Existing outputless-filler redirect, pipeline and background restrictions
remain. The runtime glob ABI changes from four pattern/path arguments to
three arguments: private target, owned pattern table and checking flag.

## Allocation regression found by the full gate

All 78 initial glob assertions passed, but the first full sweep exposed an
introduced failure in the existing `fillacc.24` resource control. The 96K
fixture could no longer construct its 400 initial cells: it failed at 83,
and ERR printed `::83` instead of `ready` followed by `old::400`.
The 5120-byte failure was during setup, not the intended later batch growth.

Tracing all private-publication producers showed two ownership costs:
literal append allocated an empty private table and immediately replaced it
with another, and append publication did not consume the completed private
batch's keys, metadata or header. The new `globsugar.17` independently
demonstrates the retention: 500 empty appends under 16K fail requesting
64 bytes in both unrepaired backends.

The repair constructs a literal batch directly and consumes private batches.
`TableBatch` shares the directory argument-table destructor rather than
duplicating it. It frees owned numeric keys, index/entry storage and the
heap header, not borrowed string or integer value backing. Empty batches
are consumed too. Publication callers were enumerated; none passes a
source/global header or the metadata-only destination clone.

That first repair restored the 400 cells, but `fillacc.24` still failed:
the request was 20480 bytes, the initial destination clone, rather than
40960 bytes at later entry growth. The fixture and assertion were retained.
No allocator-layout measurement establishes fragmentation as the cause.

A one-cell batch now uses `sash_table_set` directly. Its allocations and
owned-key copy precede publishing that new cell; metadata growth preserves
all existing logical entries. Thus a single insertion needs no full clone
for logical atomicity. Multi-cell batches retain clone-then-publish.
The corrected fixture again reaches the intended 40960-byte later failure
and exposes only the original 400 entries to ERR, in both modes.

This is not general table or string reclamation. Value backing is retained,
and some snapshot producers still allocate an initial table before replacing
it. Internal list snapshots and repeated successful selections have further
costs. Those remain explicit subjects of the integrated evaluation.

## Before controls and independent reversions

The final 79-case glob file contains 17 retained cases and 62 additions.
All pass in native and C. Against the matched frozen pre-selector compiler
and runtime, the first 78 have the same 58 failures per mode: every
`globsel.1`-`.45` and thirteen sugar assertions. `globsugar.2`, `.5` and
`.11` preserve behavior, as do all 17 retained cases. The new `.17` is
proved separately against the frozen allocation-regressing selector pair.

The old compiler's four-argument glob ABI must never be linked with the
new runtime. The session-only argv-vector linker shim substitutes the
matched archive; actual archive-open traces establish the pairing. Setting
`SASH_RT_LIB` alone is insufficient because the harness resets it.

Eleven original selector campaigns were restored:

| Disabled mechanism | Actual red assertions, both modes unless noted |
|---|---|
| Per-pattern ascending sort | `globsel.1`, `.41`; `globsugar.1`, `.6`, `.15` |
| All-pattern publication isolation | `globsel.8`, `.27` |
| Failure status | `globsel.6`, `.8`, `.10`, `.27`-`.29` |
| Directory descriptor cleanup | `globsel.27`, `.30` |
| Within-word variable snapshots | `globsel.31`; `globsugar.16` |
| Mixed-list variable/splat snapshots | `globsugar.8` |
| Completed explicit-pattern snapshots | `globsel.3` |
| Static pattern bounds | `globsel.24`-`.26` |
| Quoted provenance and quote-aware star classification | `globsel.38`; `globsugar.1`, `.3`, `.5` |
| Native checked-call flag | `globsel.11`; `globsugar.13`, `.14`; native only |
| C checked-call flag | The same three; C only |

The quote campaign contains two mutations with separate witnesses.
The repeated suppressed-read case `.30` witnesses fd cleanup, not status
checking. Fault controls inspect actual `open`, `getdents64` and `getcwd`
events; the read failure follows a successful entry buffer.

Three additional independent allocation controls match in both modes:
disabling batch reclamation or restoring the redundant initial table makes
only `fillacc.24` and `globsugar.17` fail in their respective files.
Removing the one-cell path makes only `fillacc.24` fail.
The static-bound campaign was also repeated for the migrated capture-body
check below; only `csc.41` fails in `errors.sh`.

All mutations are restored. Final source, compiler, runtime and lockfile
identities match the repaired positive before and after the full gate.
The earlier `positive.sha256` deliberately remains the allocation-regressing
snapshot; `reclaimed-positive.sha256` identifies the accepted repair.

## Contract migrations and acceptance

Three old assertions move to fresh IDs:

| Retired | Replacement | Contract |
|---|---|---|
| `cov.19` | `cov.375` | Ordinary quoted `**` is data |
| `csc.33` | `csc.41` | An explicit invalid glob inside a capture still receives SB5006 |
| `errnul.38` | `errnul.39` | An earlier word component retains its safe bytes before a later mutation |

`errnul.40` adds the opposite-order control: a component read after the
mutation still receives the ERR NUL-guard refusal. The frozen original
compiler fails the new quote and earlier-read assertions; the capture-bound
and reverse-order assertions pass before. The independent reversion, not
before failure, establishes the capture-bound assertion's sensitivity.

The first failed gate recorded rows 7099195-7111143, 11949 checks:
122 first-ever mode IDs, eight failures and eight pass-to-fail flips.
Six failed mode IDs were the three intentional migrations above; the other
two were the real allocation regression. None was waived.

The repaired ordinary gate is:

```text
NEW=10 RETIRED=6 FLIPS=2 TOTAL=11953
rows 7111144-7123096
native 6088 + C 5865
11940 passing + 13 existing skips
```

The sweep took 279 seconds; liveness is 249. No failures, ghosts or plan
mismatches remain. The two expected flips repair `fillacc.24`; the six
other failed IDs retire. Against the accepted accumulation baseline,
all 11821 retained statuses are unchanged, with 132 added and six retired
mode IDs. Unit suites pass 239/27/20, and normal formatting, Clippy,
totality and touched-function complexity checks pass without bypass.

Evidence is in the session's `files/glob-trial-20261117/selectors/`:
both frozen pairs, actual-link controls, complete xtraces, before/positive
TSVs, restored reversion campaigns, identities and both gate logs.
No dependency, DD or coverage-policy change is included. The directory
prefix and multi-star owners record trial delivery but remain open until
the adoption boundary is resolved. The integrated task remains active.
