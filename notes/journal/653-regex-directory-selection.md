title: Regex directory selection and pattern lists
date: 2026-11-17

# Regex directory selection and pattern lists

DG's authorized DD131/DD132 trial continues after `f61aecad` on
`glob-scratch-trial`. The AI implemented `rglob` and shared pattern-list
handling. Main remains at `6c7b899f`; the package is still provisional.
Implementation of the selected package is complete, but integrated
utility/migration/cost evaluation and DG's adoption decision remain.

## One selector pipeline, two pattern languages

`rglob ARRAY DIRECTORY REGEX...` applies ordinary unanchored regex matching
to directory-entry basenames. Authors supply anchors when wanted. Matching
hidden entries are included; `.` and `..` are excluded. Regex patterns have
neither coarse glob's one-star limit nor its leading-dot prefilter.

The shared directory machinery enumerates with public rustix, creates
lexically normalized absolute paths, sorts each pattern's batch bytewise,
then concatenates batches in source order without deduplication. No matches
is success; a failed directory operation is not. Ordinary destinations retain
their previous contents on failure, while scratch remains fresh-empty.
This is neither recursive traversal nor a stable filesystem snapshot.

`GlobFill` gains an optional directory value: absent for coarse glob,
present for regex selection. Canonical SSA reads, literal interning, failure
analysis and both backends account for it. The runtime entry is
`sash_rglob(target, directory_ptr, directory_len, patterns, checked)`;
native and C independently pass the same ABI and consume its status.

The runtime owns one compiled regex per pattern. It copies counted bytes
into terminated storage, uses the actual quiet compiler and releases the
program on success or directory failure. Allocation failure remains fatal.
Negative regex execution is a fatal matching-workspace error, not no-match.
No engine replacement, multiline flag, retained handle or cache is added.

## Pattern lists are lists, not joined strings

DG settled the remaining surface during implementation:

> Allow pattern-list splats, with empty expansion succeeding without enumeration.

Both selectors now accept ordinary splats, dispositions and elision in
their pattern tails. The target stays a binding name; `rglob`'s directory
stays one non-elidable scalar. At least one source pattern operand is
required, even though its expansion can produce no patterns.

An empty expanded list succeeds without opening, enumerating or normalizing
a directory. Earlier operand effects and failures still occur. An explicitly
empty default contributes one empty regex, which matches every real entry;
it is not the same as an elided or empty list.

Lowering uses the existing argument-list path rather than scalar expression
lowering. It lifts and snapshots each operand before evaluating the next.
That includes the directory before a later pattern's index helper can change
it. A bare SSA version reference is not a physical snapshot: both backends
collapse versions of a binding into common storage. Shared retention keeps
list boundaries and trust/disposition metadata instead of reconstructing
them after scalar joining has already lost them.

The checker uses the actual splat element type. Quoting an integer splat
does not convert its elements to strings. Static pattern checking also
examines literal defaults, while a require-disposition's message remains
a message rather than a pattern.

## Proof follows actual immutable values

The existing SSA proof analysis handles whole-collection facts, known
presence and defaults. A known-empty list needs no regex proof. A possibly
empty list with a default needs validity for either possible source; a
known-nonempty list does not need proof for an unused fallback.

Byte-preserving scalar quoting retains a required present cell's identity.
Actual concatenation does not. Compiler-owned literal argument batches can
supply static facts, but source-created arrays do not gain implicit validity
merely because their initial elements or defaults were literals.

Trusted copies retain presence, not regex proof or proof-bearing aliases.
Mutable intermediate cells cannot transfer a certificate into a later
immutable copy. Checking the new immutable copy itself is legal. The existing
source-versus-synthetic binding metadata supplies this distinction; no second
immutability mechanism or taint-switch exception was introduced.

## Failure ownership is observable

Lowering publishes the private collection after a handled selector failure
too. Therefore the runtime must leave that private batch empty on error:
returning a failure status after publishing some paths is insufficient.
`Paths` frees accumulated strings on an error, and transfers ownership only
after all patterns succeed. Argument-batch metadata is consumed separately.

The resource assertions distinguish the actual mechanisms:

- `rglob.29` injects the second pattern's directory-read failure after both
  patterns have obtained nonempty directory buffers; neither batch is published.
- `.33` reaches regex compilation OOM with a 16K heap, before enumeration.
- `.34` repeats 40 injected read failures in 128K, pinning compiled-regex
  reclamation and descriptor closure rather than an earlier compilation abort.
- `.44` enumerates successfully, then exhausts matching workspace with a valid
  300-alternative regex under 2M. Even inside `if rglob`, it enters ERR,
  preserves the ordinary target and never resumes the caught/success path.

## Before controls and independent reversions

The finalized glob file has 137 assertions per mode: 79 retained, nine new
`globlist` and 49 new `rglob`. The matched frozen pre-unit compiler/runtime
fails exactly all 58 additions in each mode, with the original 79 passing.
The harness overwrites `SASH_RT_LIB`, so session-only linker wrappers select
the frozen archive; actual archive-open traces prove both native and C
linkage. Exporting a path alone would not have established the before pair.

Sixteen independent reversion campaigns were completed. Each shared
mutation ran sequentially in native and C; backend-only changes also ran
the unmodified backend as a control.

| Disabled mechanism | Failing assertions, both modes unless qualified |
|---|---|
| List-aware lowering | `globlist.1-.6`; `rglob.10,.13-.16,.24,.39,.40` |
| Immediate operand snapshots | `globlist.2,.8`; `globsel.3`; `rglob.23` |
| Mandatory consumer proof | `rglob.5-.7,.11,.12,.18,.19,.41,.43,.49` |
| Quoted required-cell presence | `rglob.23` |
| Trusted-copy proof restriction | `rglob.41` |
| Source-array literal restriction | `rglob.12,.43` |
| Mutable-cell immutability restriction | `rglob.49` |
| Known-empty list proof | `rglob.15,.39` |
| Directory's canonical SSA read | `rglob.28` |
| Actual splat element typing | `globlist.7`; `rglob.38` |
| Static default-pattern validation | `globlist.9`; `rglob.17` |
| Native regex selector call | 21 `rglob` assertions, native only |
| C regex selector call | The same 21 assertions, C only |
| Fatal negative regex execution | `rglob.44` |
| Compiled-regex release | `rglob.34` |
| Empty private batch on error | `globlist.6`; `globsel.8,.27`; `rglob.29` |

The backend set is `rglob.1-.4,.10,.13,.14,.20,.23-.25,.29-.33,.35,.36,
.40,.42,.44`. Those controls deliberately call coarse glob with its correct
three-argument ABI; they are behavioral substitutions, not malformed calls
or crash-based ABI checks.

Two initial control attempts were corrected, not counted as successes.
Reading only raw cell facts lost whole-collection proof before the intended
mutable-intermediate boundary and produced no new failures. The corrected
mutation preserved both proof sources while removing only immutability.
The empty-list prediction initially named `.13`, an attested argv list with
a fallback. The actual unproved-empty witnesses are `.15` and `.39`; the
corrected native/C campaign matches those. Both initial results are retained.
Some campaigns predicted a required subset and recorded additional failures;
the complete table is observed evidence, not a claim that every set was
predicted exactly in advance.

All mutations are restored. Recorded positive source/spec/lock hashes match;
the ordinary rebuilt compiler and runtime also match their positive hashes
exactly, including after the full gate. The focused positive files are
137/137 glob and 321/321 regex in each backend.

## Acceptance and remaining evaluation

The ordinary gate was predicted as `NEW=116 RETIRED=0 FLIPS=0 TOTAL=12565`.
It passes on the first full sweep, rows 7246113-7258677:

| Measure | Result |
|---|---:|
| Native checks | 6394 |
| C checks | 6171 |
| Passing / existing C skips / failing | 12552 / 13 / 0 |
| First-ever IDs / flips / retirements | 116 / 0 / 0 |
| Ghosts / plan mismatches | 0 / 0 |
| Historical liveness differences | 245 |
| Rust compiler / converter / expansion tests | 241 / 27 / 20 |

An independent comparison against the accepted baseline block ending at
7246112 retains all 12449 statuses unchanged and adds exactly the intended
58 IDs in each mode. The sweep took 298 seconds, with serialized builds and
no concurrent spec or coverage run. Static accounting and normal staged
formatting, Clippy, totality and touched-function complexity checks pass
without bypasses.

DD131 records DG's list ruling and selector delivery; DD39/DD132, GUIDE,
the integrated owner and status describe the delivered trial consistently.
The owner inventories these exact DD edits from `f61aecad`.
Private before, positive, reversion and gate evidence lives under session
`files/glob-trial-20261117/rglob/`.

The next work is the integrated evaluation, not another implementation or
reversion cycle: useful programs, helper/converter migration, awkward cases,
and measured repeated-use memory/time costs. Earlier units' accepted evidence
remains valid. No main adoption or fresh coverage is authorized.
