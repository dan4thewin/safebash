# 633 - Indexed updates evaluate their key once

DG's tour polish exposed an indexed-update bug. Removing unnecessary braces
from the map example was straightforward, but its proposed `++` would change
DD79: bare arithmetic cell mutation is for integer arrays, not maps. The
tour now uses the existing keyed `seen[$w]+=1` form. No map-mutation extension
was selected.

The investigation found that arithmetic `a[index] op= rhs` was expanded by
the parser to `a[index] = a[index] op rhs`, copying an effectful index.
Ordinary indexed `+=` independently lowered its read and write indices too.
Both backends therefore called a key-producing function twice. There was a
second, coupled error: an RHS could change the selected key or old cell value
before the final store used them.

DD86 already requires earlier values to survive later effects. The AI
implemented that existing rule rather than choosing a new evaluation order.

## Repair

Arithmetic cell updates retain their operator in `AssignOp::Arith`, one index
and the original RHS. Shared indexed-write lowering materializes the typed
key first; compound updates also materialize the old cell value before the
RHS runs. Plain `=` saves only the key. Integer addition and string
concatenation for ordinary `+=` use the same path.

The snapshots use `reserve_temp_name` with their actual types. Reusing an
SSA variable reference alone is insufficient: native and C storage collapse
versions of one binding, so a later write can change what that reference
reads. The lowering reuses the canonical cell builder, including collection
slot resolution and checked key/element types; arithmetic reads now share
that builder too.

The retained operation still represents an implicit target read. Scope/use
accounting and taint analysis preserve that obligation. A direct pass test
checks the latter independently: typing already rejects string-array
arithmetic, so an end-to-end refusal cannot prove this taint check.

The inert IR advances to schema 3. Arithmetic cell assignments carry their
operation rather than a synthetic RHS cell read. DD104 records the shape and
retains schema 2's separate EXIT/ERR fields. Five changed IR contracts take
fresh `ircell.1`-`.5` IDs, including their C-mode skip counterparts; scalar
mutation and C-for desugaring contracts are unchanged.

No backend or runtime change was needed. This is a bounded indexed-write
repair, not completion of the general
[expression-order work](../tasks/q-capture-hoisting-order.md).

## Evidence

`subonce.1`-`.12` cover arithmetic compounds, postfix updates, ordinary
integer/string updates, RHS changes to the key and old cell, and preserved
typing/read/taint checks. Against the frozen original compiler, `.1`-`.9`
fail in both modes and `.10`-`.12` pass; all 141 preceding subscript cases
still pass. The positive compiler passes all 153 in native and C.

The map diagnostic case counts the duplicated `is a map` message, not every
SB4030: the independent key-kind error remains. The tainted-key case pins
the index read, not the implicit old-value read.

Six independent reversions distinguish the repaired mechanisms:

| Reversion | Failure witnesses |
|---|---|
| Restore parser-created RHS cell read | `subonce.1`, `.2`, `.3`, `.6`, `.9` in both modes; `ircell.3` in native |
| Stop saving the selected key | `subonce.6`, `.7`, `.8` in both modes |
| Stop saving the old cell value | `subonce.6`, `.8` in both modes |
| Re-evaluate the ordinary `+=` read key | `subonce.4`, `.5` in both modes |
| Omit the implicit semantic read | `subonce.11` in both modes |
| Omit the implicit taint read | The direct pass test loses SB5021 |

Every predicted set matched exactly. All stubs were restored; source/spec
hashes and the release compiler matched the frozen positive before the gate.
The original compiler also fails all seven non-skipped replacement IR
assertions; the three native-only IR cases retain their C skips.

Targeted subscript, arithmetic, miscellaneous and CLI files pass in both
modes. Formatting, Clippy including tests, totality and the live
grandfather-on-modify complexity rubric pass without bypass.

The serialized gate passes with:

```text
NEW=34 RETIRED=10 FLIPS=0 TOTAL=11369 timeout 900 mk -p 1 gate
```

Its sweep is rows **6902132-6913500**: **11356 passing, 13 existing skips,
zero failing**, native 5796 and C 5573. All 11335 retained prior IDs have
unchanged results; the exact 34 additions and ten retirements match the
prediction. Liveness is 251. `mk test` passes 239 compiler, 27 converter and
20 expansion tests. The tour checker reports 49 snippets with no mismatch.

Private evidence is under the session's
`files/tour-subscript-once-20261113/`: frozen compilers, before/positive
outputs, reversion logs and TSVs, source identities, and the gate comparison.

## Boundary

The repair and tour remain uncommitted on `spec-outline`, alongside DG's
outline and policy review drafts; HEAD remains `2ef86c2a`. The gate's commit
label therefore identifies the unchanged HEAD, not a published fix.

DD134's element-typed loop binders and the symmetric lookup policy remain
queued, not implemented here. The `write` discussion remains unsettled.
Both local lockfiles and the runtime archive are unchanged. No coverage run
or unrelated implementation was started.
