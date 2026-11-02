# 558 - One collection, one access path

2026-11-02. While preparing W1b-2, DG settled strict non-aliasing and asked
that it be implemented first if the remaining port depended on it. It does:
independent slot effects cannot describe a write through A changing a read
through B when both name the same collection. The same problem exists when
a slot aliases a global named by the body.

DG chose the language contract, including discarding read-only and
taint-inert relaxations. I implemented its enforcement as a separate
prerequisite unit. This is not the slot-summary port.

## The settled rule

Every collection argument has a distinct identity. A collection received
through a slot may be accessed only through that slot or a forwarded slot,
not through its global name, by the function or anything it calls.

Reads, writes, unset, attest, metadata and identity forwarding count.
Captures, arithmetic calls, stages, background bodies and conditional
paths are included; the rule is static and does not disappear under
`--no-taint`. Unrelated globals remain available, and the caller can use
the collection after the call. Ordinary `=name` words on builtins and
external commands are still literals.

This supports one taint summary per function, without alias-pattern
specialisations or an ordered alias-parametric summary. The three claims
discussed with DG remain qualified: clearer review, true; non-aliasing by
itself prevents unsafe tainted-data use, false; simpler than the two
alias-aware summary alternatives, true. The last is not a measured size
comparison with the former caller-taint-context walker.

DD100's old alias permission is replaced in place, and DD127 records the
prerequisite and implementation boundary. There is no backlog deferral.

## Enforcement

The only Rust change is in `validate.rs`. `check_collection_args` already
runs after lexical binding and outside the optional taint check, so it is
the natural home for SB5099:

- Duplicate actual identities are rejected at the call. Diagnostics use
  numeric argument positions, so even an invalid call with more than five
  arguments cannot index beyond A-E.
- Each function gets a global-use footprint with a function/statement
  witness. The shared statement, read and binding walks collect accesses;
  user-function collection arguments and the canonical `unset` parser
  supply identity-only uses. No second expression walker or lexical-name
  resolver was added.
- The existing census supplies command and arithmetic callee edges.
  A visited graph walk closes the footprints transitively. All definitions
  of a name contribute, conservatively, without re-walking their source
  for each caller. The actual arguments are intersected with that closure.

The global-overlap diagnostic names both the call and a conflicting body
location. Forwarded slots carry restrictions outward: `inner =A` need not
know A's actual global until `outer =a` makes the overlap concrete.

`SlotInfo`, `analyze_slots`, slot usage/arity analysis, the taint driver
and the backends are unchanged. None of the reference branch's remaining
hunk-ledger rows closes in this unit. B115 stays open.

## Evidence

`collparam.sh` grows from 124 to 140 checks per mode. Existing `100.9`
keeps its source and ID but now requires SB5099. `noalias.1`-`.14` are new
rejections; `.15`/`.16` preserve unrelated-global access, mutation
write-back, subsequent caller access and literal arguments.

Against the saved 8ddfb52f compiler, exactly 15 assertions fail per mode:
`100.9` and `noalias.1`-`.14`. The native attest fixture `.6` already hit
an ICE, `no codegen slot for a_2`; the harness filtered that unlocated
message out of `$err`. Its unfiltered stderr is retained. C accepts the
fixture, and the new checker refuses it with the required located SB5099
in both modes. The other final rejection fixtures run successfully before
the change. The first draft of `.12` was invalid SB5086 (two literal
comparison operands); it was corrected to a local-variable comparison and
the before-controls rerun. That first draft is not evidence.

The complete file passes 140/140 in both modes after the change. Three
independent controls on that final implementation give exactly:

| Reversion | Red IDs, each mode |
|-----------|--------------------|
| Remove the callee edges from the footprint graph | `noalias.8`, `.9`, `.11` |
| Reject a passed collection whenever any global is used | `noalias.15` |
| Treat ordinary command `=name` words as identity uses | `noalias.16` |

All stubs are restored, and the rebuilt compiler is byte-identical to
the saved positive binary.

`NEW=32 FLIPS=0 RETIRED=0 TOTAL=10642 mk -p 1 gate` passes first time.
The sweep takes 240 seconds: rows 6648923-6659564, native 5432, C 5210,
10629 passing and 13 existing skips. Exactly 32 first-ever IDs, no flips,
retirements, ghosts or plan mismatches; the liveness control reports 260.
The gate also passes all 237 compiler, 27 converter and 20 expansion units.

Text evidence is archived as session
`files/port-w1b2-noalias-probes.tar.gz`. The pre-change compiler and active
W1b-2 probes are retained; no reversion remains in source or release output.

## What remains

W1b-2 adds slots to the taint summaries, fixes B115, enforces inherited-slot
attests at calls and applies the shared-slot firewall. When applying those
independent flows, an unused global's identity flow must not undo cleanup
through the slot bound to that global. The restriction concerns accessed
globals, not every global declared in the program.

`len A` was already accepted; it retains the direct collection's metadata
behavior. Stdout, ordinary loop edges, per-process SSA checking and END
entry remain W2-W5. No claims about those were promoted by this gate.
