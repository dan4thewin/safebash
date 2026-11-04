# 565 - One checked type map, before EXIT and ERR

2026-11-04. DG asked for the next step after settling DD128's five choices.
I took the typing prerequisite first: B5's missing cleanup checks and the
coupled B1/B2/B4/B6/B7 defects. This is a bounded implementation unit, not
the EXIT/ERR implementation. END still exists, its operational-failure
treatment is unchanged, and no new handler or strict-taint rule is exposed.

## The checker now supplies lowering's types

The checker previously discarded its environment. A second walker,
`infer_types`, reconstructed a different map for SSA lowering: it knew
neither the inferred element types of collection slots nor arithmetic-attest
targets, and both walkers skipped END. Consequently, a slot-derived int
or an attested int could pass checking and then concatenate on `+=1`.

`type_check` now returns either the checked variable map or its errors.
The CLI threads that exact map into the existing checked-SSA pipeline.
The second inference walker and its helpers are deleted.

Function types come from `Program::function_defs`. Top-level and cleanup
non-function statements establish global facts before function bodies are
checked with their complete DD100 signatures. First-binding inference
converges over forward and loop-carried references without an iteration cap.
A separate `bound` set distinguishes a real earlier binding in this pass
from the previous pass's inferred seed; diagnostics are retained only from
the stable pass. Collection kind and explicit-string facts survive too.

Collection-signature inference uses the same full definition inventory and
collects calls in both roots. Main and END share global collection facts,
so a function in either root sees a collection declared in the other.
Forwarded slots and conflicting call types use the existing DD100 solver.

Every auxiliary visitor covers END using the same scoped walk. The
int-function predicate checker uses the complete shared statement walk,
including capture bodies, instead of its private structural-only recursion.
The redundant capture-operand pass is gone: it was rechecking commands that
the main body walk already checked, producing duplicate SB4030s.

## Attest targets are bindings, not a retyping escape

An arithmetic-attest target can introduce an int scalar or assign an
existing int scalar. It cannot change a string into an int, and an int
collection's element type does not make the collection a scalar target.
The latter case produced a nonsensical native length and an undeclared-name
error in generated C on the old compiler. All these invalid targets now
get SB4030 before lowering.

The redeclaration scanner reuses the existing attest-target decoder to
record the introduction. A later typed declaration gets SB5082 with the
earlier binding's line. The environment's silent keep-Int fallback is
replaced by an invariant assertion: checked bindings cannot change type.

These are repairs under DD30/DD86/DD93/DD100, not new language choices.
The DD93 text now distinguishes the earlier checker-only repair from this
complete map/binding repair and correctly describes caught arithmetic
attests as value-binding too. Existing `93.34` and `ifcv.2` continue to pin
the failed conversion's integer-zero target; no rival spec was added.
DD128's draft also clarifies that whole-collection unset is the legal
empty replacement: its failed-branch permission does not legalize scalar
unset. No taint-transfer implementation changed here.

## Evidence

`spec/types.sh` grows from 446 to 470 checks:

| Checks | Claim |
|--------|-------|
| `typeflow.1`, `.2`, `.20` | Checked slot, forwarded-map and attest int types reach lowering |
| `.3` | A capture operand produces one diagnostic, not two |
| `.4`-`.6`, `.19` | Conversion targets obey type, collection and declaration rules |
| `.7`-`.11`, `.18` | Cleanup bodies and definitions receive primary, auxiliary and predicate checks |
| `.12`-`.15` | Global typing is independent of function placement; loop-carried copies converge |
| `.16`, `.17` | Captured `if` and list predicates reject int functions |
| `.21`, `.22` | Cleanup-defined functions infer int-array and forwarded int-map slots |
| `.23` | Main and cleanup calls cannot give one function conflicting slot types |
| `.22`, `.24` | Global collection facts reach the other root in both directions |

All twenty-four fail on the saved pre-change compiler in both modes; all
446 older checks pass there. All 470 pass on the final compiler in both modes.
The initial positive run exposed one bad assertion of mine: `.11` expected
the word "operand", while the real diagnostic says "int value in a `[[ ]]`
test, which compares strings". I pinned that actual distinguishing text
and reran the final before/after files.

While updating the private-walker backlog pointer after the first gate, I
found that `infer_collection_signatures` still scanned only the main root.
The initial all-body claim was incomplete despite the green gate. The four
new signature cases fail on that initial positive too: its valid slot
copies still concatenate, and it accepts the conflicting call types.
The full inventory, both call roots and shared global metadata are now
fixed together, not left as a known gap in the new checked map.

The direct invariant test also failed before the repair specifically
because the old environment did not panic. It passes with the new guard.

Eleven independent reversions produced exactly the predicted red checks,
with identical results in native and C and no older check failures:

| Reversion | Red `typeflow` checks |
|-----------|----------------------|
| Omit the primary END walk | 7, 8, 9, 10, 11 |
| Omit END from auxiliary passes | 9, 11 |
| Collect only top-level function types | 8, 18 |
| Check bodies only once | 15 |
| Keep the prior pass's actual-binding set | 15 |
| Restore the old predicate traversal's scope | 16, 17, 18 |
| Unconditionally declare the conversion target int | 4, 6, 19 |
| Omit conversion introductions from redeclaration tracking | 5 |
| Collect only top-level definitions for collection signatures | 21, 22 |
| Collect signature calls only in main | 21, 22, 23 |
| Give each root only its own global collection facts | 22, 24 |

The first eight controls ran on the initial positive; the last three pin
the subsequent signature correction. Every stub is removed. Final source,
spec and rebuilt compiler hashes match the saved final positive
(`final.sha256`).

The initial `NEW=40 FLIPS=0 RETIRED=0 TOTAL=10892 mk -p 1 gate` passed:
10879 passing specs plus 13 existing skips, rows 6735093-6745984. After the
signature correction, the final
`NEW=8 FLIPS=0 RETIRED=0 TOTAL=10900 mk -p 1 gate` passes too:
238/27/20 unit tests; 10887 passing specs and 13 existing skips, native
5561 plus C 5339. The 241-second sweep occupies rows 6745985-6756884.
All 10852 pre-unit statuses and all 10892 first-gate statuses are unchanged.
Exactly `typeflow.1`-`.24` in each mode are the unit's additions; no missing
or unexpected IDs, flips, retirements, ghosts or plan mismatches.
Liveness remains 260.

The evidence archive is in the session files as
`port-dd128-types-probes.tar.gz`; the frozen before, initial-positive and
final compilers remain under `/tmp/safebash-dd128-types.p6zlJ9Qb`.
No coverage or runtime work was needed. W5's routing, strict adjacent taint,
ERR failure effects, actual
handler-entry values and fatal-transfer implementation remain to be done.
