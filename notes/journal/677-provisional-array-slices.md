title: Provisional typed array slices
date: 2026-11-26

# Provisional typed array slices

DG directed acceptance of the complete preceding branch on main, then a
new branch implementing slice provisionally. The acceptance is already
published as `727fe371`; [journal 676](676-accept-err-attempts.md) records
that separate operation. This implementation belongs to `slice-trial`,
not main. DG retains the later adoption decision.

## The operation

[DD137](../decisions/DD137-array-slice.md) defines
`slice SOURCE START [COUNT] DEST`: a typed snapshot of present array values
in existing splat order, not a numeric-key interval. Holes are skipped and
the selected private batch has dense keys. Ordinary destinations append
after their highest live key; underscore targets construct fresh immutable
scratch. Ordinary self-source append is legal because the private batch is
complete before publication.

Bounds are nonnegative integers, evaluated once left-to-right. Omitted
COUNT takes the remainder; end/zero/empty cases produce an empty batch,
and oversized COUNT truncates without overflowing START+COUNT. Bound
effects occur before the source snapshot, but cannot change an earlier
bound's retained value. Runtime negative bounds remain terminal, including
inside ERR-owned attempts.

Slicing function `args` permits variadic calls without requiring the
positions being skipped. Other explicit required/optional reads retain
their constraints. The destination is an ordinary array: it inherits
neither the special `args` splat exemption nor any regex/nonemptiness proof.
Taint is checked at the source read, not copied or resolved by slicing.
Existing destination taint and readonly/scratch/alias rules remain.

Slice supplies no status or array-valued expression. Conditions, negation,
status returns and source discard cannot consume a result it does not have.
An ordinary stdout capture can still contain slice and collect bytes emitted
by its bound helpers; that is the capture's string, not an array result.

## Shared implementation

The AI implemented canonical source/bounds/destination roles in
`Command::slice_operands`, reused by binding, typing, arity and lowering.
Collection discovery retains concrete or forwarded slot dependencies until
the existing signature inference resolves them. Intermediate slice targets
therefore keep the source's actual string/int element type across helpers.

Lowering reuses the integer-value retention helper, then emits one typed
`SliceFill` and the existing collection publication operation. Native and C
call `sash_slice_snapshot`. It copies raw table cells, not the rendering
accessor: integer cells encode their value in `val_len` with a null
`val_ptr`. Using `sash_table_value` would silently stringify them; join-only
output would still look correct. Immutable string backing can be shared
without sharing mutable table/key state.

Fresh scratch expires before bound evaluation. Publication retains the
existing allocation-before-mutation discipline. With a 128-cell source,
the 16K-heap control prints `ready`, then `ERR:old` on slice allocation
failure; the existing destination survives. The identical program with a
64K heap completes and prints `done:129` in both modes.

## Evidence

`spec/slice.sh` has 62 cases per mode. They cover sparse ordering/dense keys,
exact binary and empty strings, raw integer arithmetic, independent copies,
self-append, bounds/effects, forwarded types, args arity, source taint and
the isolated SSA backstop, scratch lifetime, readonly/alias rules, proof
non-transfer, result use, capture output and allocation failure.

All 62 pass in native and C and fail on the frozen pre-trial compiler/runtime
pair. Actual archive-open traces verify that the before controls link the
frozen archive despite the harness's runtime override. The session-only
linker wrapper also accepts the already-rewritten archive when GCC invokes
ld; the initial wrapper failure is not a compiler finding.

Thirteen independent mutations then give exactly these red `slice.*` IDs
in **each** backend:

| Mutation | Failing IDs |
| --- | --- |
| Render integer cells before copying | 6, 7, 13, 15, 16 |
| Include deleted entries | 3 |
| Retain source keys instead of dense keys | 3, 13 |
| Omit the runtime negative-bound guard | 23, 24, 51, 60 |
| Omit scratch clearing | 51 |
| Omit earlier-bound retention | 17 |
| Replace rather than append at publication | 4, 7, 14, 56, 62 |
| Force string element types | 6, 7, 13, 14, 15, 16, 28, 29 |
| Drop intermediate forwarded-type dependencies | 13 |
| Omit slice's variadic recognition | 10, 12 |
| Omit the frontend source read | 39, 50 |
| Omit the SSA source read | 41 |
| Omit the result-use guard | 45, 46, 47, 59 |

All mutations are restored. The final compiler, runtime, 18 changed Rust
files and spec file match the frozen positive identities; both lockfiles
and protected local files also match. A final restored focused run passes
all 62 cases in each mode. The private evidence retains exact mutation
patches checked against the source hashes of all 26 control runs.

The predicted ordinary gate passes:
`NEW=124 FLIPS=0 RETIRED=0 TOTAL=13364 timeout 900 mk -p 1 gate`.
Rows 7,544,013-7,557,376 contain 13,364 distinct checks: 6,798 native and
6,566 C, with 13,351 passing and 13 existing skips. There are no failures,
flips, retirements, ghosts or plan mismatches; liveness is 246. All 13,240
accepted-main statuses are retained unchanged, with exactly 124 additions.
Rust unit totals are 240/27/20. The live touched-function complexity rubric
has no violations. No full sweep is repeated after exact restoration.

The evidence is in the session's `files/slice-trial-20261126/`: matched
before/positive binaries, final positive hashes, gate log, private per-mode
TSVs, original reversion predictions, exact patches and control results.
Standalone controls do not add spec-history rows.

## Boundary

The [existing owner](../tasks/research-array-slice.md) remains open for DG's
evaluation, not unfinished implementation. GUIDE, the specification outline
and DD75 identify the operation as provisional. No shift mutation, view/COW
mechanism, converter automation or fresh coverage measurement is included.
The branch is published through normal hooks; main and the retained
`err-attempt-trial` branch are not changed by this delivery.
