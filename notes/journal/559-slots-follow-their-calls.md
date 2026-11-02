# 559 - Slots follow their calls

2026-11-02. DG said to continue after the non-aliasing prerequisite.
This is W1b-2, the slot half of the split: DD127's name summaries now cover
globals and collection parameters. DG settled the language contract and
one-summary-per-function approach; I extended the existing implementation.
The context-sensitive walker on the reference branch is not ported.

## What the old analysis got wrong

`SlotInfo` collected every collection ever passed to a slot and every
syntactic bulk fill. A function's incoming slots were checked against the
final top-level taint set, not the state at each call. Outgoing taint was
the fill set, so an in-body attest could not clear the caller's collection.
That was B115. The special body-entry adjacency set also permitted a
function to attest taint it received, contrary to DD127.

All of that machinery is removed: `SlotInfo`, `analyze_slots`, its
binding/fill fixpoints, `tainted_slots_on_entry`, `taint_out_params`,
`check_slot_entry`, and the old slot-derived global diagnostic.
`bulk_fill_targets` goes with its only remaining consumer. The independent
slot-usage analysis stays for arity and dead-variable checks; typing,
inlining and runtime collection substitution are unchanged.

## One interface, mapped at the call

The existing summary interface is extended from globals to globals plus
A-E. All five symbolic slots are present, even when unused: five additional
singleton probes per definition, rather than a second arity analysis.
This is an internal representation choice, reported to DG.

An empty-entry walk measures own generators. Singleton inputs measure
outgoing flows and conditional diagnostics. The latter are separate:
reading received taint before an eventual clean overwrite must still be
rejected even when the outgoing flow is empty.

At a call, formal slots map positionally to actual identities, including
qualified locals. `inner =B =A` maps inner A to outer B and inner B to
outer A. Unpassed slots have no mapping. Arrived taint selects conditional
diagnostics, which are relayed at the call with the actual caller name and
the original body location. Flows and, for a single-use function,
generators are mapped back to the caller.

A passed global has only its slot path. Its otherwise-unused global
identity flow is omitted: SB5099 forbids access through that global name,
and unioning the dummy identity flow back would undo cleanup through the
slot. `sflow.1` starts with an already-tainted caller array, fills and
attests A inside the function, then reads the caller array. It exercises
this distinction, not just a clean initial collection.

Shared functions may not leave their own slot taint on return. SB5067
names the collection slot, the last source and why the function is shared.
Received taint is not own taint: metadata-only calls preserve it without
raising SB5067 or making it freshly adjacent. Every body entry now has an
empty adjacency set; the old inherited-slot exception is structurally gone.

## Specs and before-controls

`collparam.sh` grows from 140 to 158 checks; `taint.sh` from 468 to 469.
`100.27` keeps its source and now requires call-site SB5020. `100.28` also
keeps its source, but deliberately changes from acceptance to call-site
SB5088. The other existing collection verdicts are preserved.

The 18 new `sflow` cases cover cleanup, forwarding and permutation,
call-site timing, read-before-kill obligations, inherited attests, shared
own sources, incoming-flow preservation, captures, a background read, the
fifth slot, local actual identities and mixed array/map slots. `atslot.9`
adds a caught own-fill attest whose bad cell takes the exit branch, with a
caller read to expose the old outgoing-taint error.

The branch's `atslot.1`-`.6` are not copied: their inherited-attest premise
is now rejected. Existing `.7`/`.8`, new `.9` and the `sflow` cases retain
their useful own-fill, forwarding and mixed-slot claims without rivals or
reused retired IDs.

The saved 685ea2f1 compiler fails exactly 17 assertions per mode: `100.27`,
`100.28`, fourteen new `sflow` cases, and `atslot.9`. The four preservation
cases `.10`, `.11`, `.16` and `.18` already pass. Final standalone positives
are 158/158 and 469/469 in both modes.

The first positive run exposed a separate SSA fault in `sflow.5`; its final
assertion is deliberately an intermediate rejection, described below.
The fifth-slot fixture was also strengthened to start E tainted, so it
pins cleanup mapping rather than merely executing a fifth argument. The
final collection file was rerun on both compilers and backends.

## Independent reversions

Each row is a separate change to the final implementation. Both backends
produce the same red set:

| Reversion | Red checks in each mode |
|-----------|-------------------------|
| Keep a passed global's dummy identity flow | `sflow.1`, `.3`, `.15` |
| Drop incoming flows | `sflow.7`, `.10`, `.11`, `.16` |
| Map every formal slot to the first actual | `sflow.1`, `.3`, `.15`, `.18` |
| Treat inherited body-entry taint as adjacent | `100.28`, `sflow.6` |
| Exclude slots from shared-own reporting | `sflow.8`, `.9` |
| Drop mapped single-use generators | `100.30`, `100.32` |
| Skip conditional diagnostics for slot inputs | `100.27`, `100.28`, `sflow.4`, `.6`, `.7`, `.12`, `.14` |
| Treat every retained name as freshly tainted after a statement | `sflow.11` |

Three initial predictions needed correction, not source changes. Dropping
all flows also clears the unrelated global `dirty` at `use =clean`, losing
the later call's obligation in `.7`. Using `first()` also binds unused
B-E, whose dummy flows undo A's cleanup in `.1`. I initially listed
`100.33` for the generator stub, but it is a clean read control, not an
adjacency rejection; it correctly stays green. The traces explain each
result, and the corrected sets match C as well.

## Boundaries, not completion claims

`sflow.5` reads a clean collection through a function, then fills it from
argv later. The validator now accepts the earlier call, but the SSA
backstop checks bare-name array reads against the highest numbered version
anywhere, including that future fill. The compiler still rejects the
program. The reference branch retains the same lookup; W4's process
partition does not supply intra-process ordering.

Under DG's port-scope rule this is a separate backlog item,
`bug-ssa-taint-array-read-uses-future-version`. The spec explicitly requires
that remaining backstop error and no SB5020. It must become an acceptance
assertion when the SSA fault is fixed; this unit does not claim that result.

The existing missing-slot arithmetic-call P1 was rechecked before relying
on the mapper's missing-argument handling. Top-level and nested sexpr calls
still reach native undefined-array ICEs on both compilers; `--emit-c`
succeeds, but the generated C was not executed. A mixed valid capture and
missing-slot arithmetic call still gets the one-signature SB4030.
The mapper does not invent implicit inherited slots to accommodate the
bug. Its task now includes the nested reproducer and recheck.

DD127's c2 was also rechecked: received global taint written into A already
gets SB5021 at the call on 685ea2f1. That belongs to W1b-1, not this unit.

Capture/value effects still union with incoming state. `sflow.13`
deliberately starts with a clean collection; it does not claim that a
capture clears an already-tainted caller collection. Stdout, ordinary loop
edges, per-process SSA checks and END entry remain W2-W5.

DD100, DD127 and GUIDE are rewritten in place, including removal of the
claim that a binding half of `analyze_slots` survives. The contributor note,
findings index and hunk ledger record the new state; split rows keep their
W2/W3/W5 remainders.

## Gate and delivery

All reversion edits are restored. The source and rebuilt compiler compare
byte-for-byte with the saved positive, both before and after the full gate.
The only Rust file changed is `validate.rs`: 110 lines added, 291 removed.
No new Rust harness, runtime change, coverage run or dependency is involved.

`NEW=38 FLIPS=0 RETIRED=0 TOTAL=10680 mk -p 1 gate` passes first time.
The sweep takes 240 seconds and records rows 6659565-6670244:
native 5451, C 5229, 10667 passing and 13 existing skips. Exactly 38
first-ever IDs; no flips, retirements, ghosts or plan mismatches. The
liveness control reports 260. All 237 compiler, 27 converter and 20
expansion units pass.

Evidence is retained in session `files/port-w1b2-slots-probes.tar.gz`.
The final standalone TSV blocks have explicit `before-final` / `after-final`
names: `SPEC_TSV` appends, so the earlier exploratory blocks are not
mistaken for the final run. W1's name-channel work is complete; W2 is next,
not started as part of this unit.
