title: Snapshot key loops and runtime-indexed unset
date: 2026-11-22

# Snapshot key loops and runtime-indexed unset

DG requested both the deletion-skip repair and runtime-key unset. After
comparing mutation-aware iteration with the existing snapshot path, he
chose: "use the snapshot, update dd75". The AI implemented that choice on
`err-attempt-trial`. This does not adopt DD136 or advance main.

## The selected behavior

Direct `for k in $(keys m)` captures the map's key sequence once on entry.
Deleting the current, earlier or pending key cannot remove another key
from that sequence. Insertion, reinsertion, clearing and whole-map
replacement cannot extend or reorder it. Normal loop transfers still
work, and reentering the loop takes a fresh snapshot.

Only the keys are captured. `${m[$k]}` in the body still reads the current
map, including its ordinary empty-string or zero result for a missing cell.
Explicit `keys m out` keeps its existing ordinary accumulation and fresh
scratch-construction behavior.

`unset NAME[KEY]` keeps the collection identity static and accepts the
ordinary DD80 index expression and type. All indices are evaluated once,
left to right, and retained before this unset command applies any deletion
or whole clear. Earlier key-producing effects remain visible to later
expressions. A helper can replace the collection during evaluation;
deletion acts on the current storage afterward.

The target is storage identity, not a content read. Its index is an ordinary
checked read. Element deletion does not validate the remaining collection;
whole clearing retains DD128's existing failed-attest replacement rule.
Scratch, immutable `args`, readonly ERR imports, lexical identity, slots
and non-aliasing remain enforced. There is no collection-name indirection,
scalar unset or mutation result.

DD75 and DD80 now state these contracts in their current-rule openings
and bodies. GUIDE and the specification outline agree.

## Reusing the existing mechanisms

Mutable key loops fill a collision-safe private array through the shared
`fill_keys` path, then use ordinary array-value iteration. Immutable scratch
maps retain equivalent non-copying key traversal and its existing proofs.
Array/map-value iteration is unchanged.

The runtime already has `sash_keys_snapshot`; no runtime operation, ABI,
table layout or mutation-aware cursor was added. Its index keys are copied,
while the snapshot's values refer to the source map's stable key-byte
buffers. Deletion and clearing do not free those buffers. The runtime
comment now describes that ownership accurately rather than claiming
`sash_table_set` copies both its keys and values.

Snapshot creation allocates on every mutable-loop entry. This work does
not add snapshot reclamation or caching; DD75 and GUIDE disclose the
allocation and the immutable-source optimization.

Unset uses the existing balanced-subscript scanner, index parser and
`ExprKind::Subscript`, not another expression grammar or AST variant.
The shared fallible target decoder distinguishes collection identity
from the evaluated index. `Command::value_args` exposes that index to
ordinary read/call walks; write and alias accounting retain the collection
identity. Early analyses still tolerate malformed targets until their
canonical diagnostic.

Lowering uses the existing typed subscript operand saver to prepare all
targets, then emits existing deletion/clear operations. The old
literal-string bracket decoder is gone. Both native and C consume the same
checked lowering.

## What the first full gate found

The first predicted gate ran 12929 checks at rows 7360334-7373262,
with the expected 72 additions and two `noalias.18` retirements. It found
four failures: native/C `bkstp.16` and native/C `loopint.3`.

`bkstp.16` disables the frontend taint checker and constructs a map with
tainted keys. The old direct key access propagated that taint, but the
initial snapshot path produced the deliberately clean `KeysFill` result
without independently checking its source. The repair keeps that result
policy and adds the missing source check.

`KeysFill.source` and `TaintRead::Binding` now carry the exact reaching
`VarRef`. The SSA backstop checks it through the existing collection-use
check, rather than looking up the highest version of the name. New
`bkstp.19` copies keys before a later map attest: that later clean version
must not hide the dirty input to the earlier copy. Normal map construction
still rejects tainted values with SB5021; this is not a new taint exemption
or a policy of propagating arbitrary taint through a key snapshot.

`loopint.3` repeatedly entered a mutable-map loop under a 64K heap.
Its new snapshot allocation obscured the fixture's original claim:
loop counters must not allocate formatted strings on every iteration.
The replacement, fresh historical ID `loopint.17`, uses immutable `_m`
with the same 64K heap and 10000 iterations. It isolates that counter
claim without raising the heap limit or reducing the workload.

## Before and mechanism controls

The frozen original compiler fails exactly 33 of the 36 new subscript
cases in each backend. All 153 original cases still pass.
`unsetkey.21`, `.25` and `.30` are preservation controls, not newly repaired
behaviors. The former `noalias.18` runtime-key refusal moves to the new
acceptance case `unsetkey.27`; quoted and indirect whole-target refusals
remain.

The final taint-switch file has 79 cases. The original compiler fails only
new `bkstp.19`; the frozen initial snapshot compiler fails `.16` and `.19`;
the corrected compiler passes all 79, in both modes. The migrated
140-case control-flow file also passes on the original compiler.

Ten independent compiler reversions produced the following same failures
in native and C:

| Reverted mechanism | Failed checks |
| --- | --- |
| Snapshot loop back to live traversal | `iterdel.1`-`.6`, `unsetkey.1` |
| Apply unset effects before finishing operand evaluation | `unsetkey.4`, `.11` |
| Evaluate but do not retain earlier keys/indices | `unsetkey.3` |
| Treat the source unset target as a content read | `unsetkey.12` |
| Treat SSA deletion storage as a content read | `unsetkey.12` |
| Omit the source unset written identity | `unsetkey.22` |
| Omit immutable-args mutation checking | `unsetkey.24` |
| Omit the explicit-index grammar guard | `unsetkey.18` |
| Omit the ordinary KeysFill source check | `bkstp.16`, `.19` |
| Veto the exact-version check using highest-version name taint | `bkstp.19` |

The scratch assertion names the source diagnostic's distinguishing text.
Its earlier generic refusal was masked by the independent SSA scratch
guard. The args assertion requires two SB5052 reports, separately pinning
whole clear and runtime-index deletion.

A separate runtime allocation injection calls the real `sash_itoa` from
`sash_table_key`. Both backends compile successfully, then only
`loopint.17` fails with `sash: out of memory (requested 2 bytes)`.
This proves the migrated fixture still detects counter-format allocation.
An earlier trial that changed the counter's type failed during compilation
and is not counted as allocation evidence. The private checker initially
expected 32 bytes; the actual one-digit-plus-NUL allocation is two.

All ten reversions and the runtime injection were removed. Source,
compiler and runtime identities were checked after restoration. Runtime
logic and its archive are unchanged; only the snapshot documentation was
edited. Ordinary staged formatting, Clippy, totality and live complexity
checks passed without bypass before the corrected full gate.

## Final gate and publication

The corrected gate passes with its predictions:

```sh
timeout 1200 env NEW=4 RETIRED=2 FLIPS=2 TOTAL=12931 mk -p 1 gate
```

The 315-second sweep occupies rows 7373263-7386193: native 6580 plus
C 6351, with 12918 passing and 13 existing skips. The four first-ever
IDs are `bkstp.19` and `loopint.17` in both modes; `loopint.3` retires,
and only `bkstp.16` flips from failing to passing. No ghosts or plan
mismatches remain. Liveness reports 246 IDs different from their first
recorded status.

The same gate passes 241 compiler, 27 converter and 20 expansion tests,
152 C regex checks and five getopt checks, along with the static
accounting checks. The final source/spec/compiler/runtime manifest still
matches after the gate. No fresh coverage run was performed.

The original implementation baseline is `d3604d8f`, with spec-history
boundary 7360333. An independent comparison with its final complete
12859-row block verifies the whole-change prediction: 12855 retained
statuses unchanged, exactly 76 additions and four retirements
(`noalias.18` and `loopint.3` in each mode). No commit-hash self-comparison
is used.

Detailed before, intermediate, reversion, xtrace and identity evidence is
retained in session `files/unset-implementation-20261122/`. The original,
initially broken snapshot and corrected compiler snapshots are separate.
`final-positive.sha256` records the final runtime-comment revision as well
as the source/spec/compiler/runtime identities; older manifests are not
silently rewritten to claim those bytes.
