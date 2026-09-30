---
title: "Taint bypass: inward slot taint consults the FINAL top-level set, so a later attest cleans an earlier read"
status: done
priority: P1
kind: bug
depends: []
gates: []
---

# A `attest` written *after* a call retroactively cleans the read inside it

Found 2026-09-30, while explaining to DG how the DD#100 slot machinery handles
the add/remove problem that a taint fixpoint cannot. It does — in the
**outward** direction. The **inward** direction has the bug.

## Reproducer

```sash
fn each { for v in @A; do /bin/echo "$v"; done; }
argv a
each =a
attest v in a [[ -n $v ]]
```

Compiles. Run as `./n SECRET1 SECRET2` it prints `./n`, `SECRET1`, `SECRET2` —
**unattested `argv` reaching `/bin/echo`**, which is the exact class SB5020
exists to prevent.

The identical read written at the top level is correctly rejected:

```sash
argv a
for v in @a; do /bin/echo "$v"; done
attest v in a [[ -n $v ]]
```

→ `SB5020 iterating tainted array 'a' without attest`.

So routing the read through a slot loses the diagnostic. The attest is *after*
the read in both programs.

## Narrowed

| variant | result |
|---|---|
| no attest anywhere | SB5020 — correct |
| attest on a *different* array | SB5020 — correct |
| attest in an unreachable branch | SB4067 — correct (attest is statement-shaped) |
| **attest on the right array, after the call** | **compiles — BUG** |

So it is specifically the presence of a correctly-named attest *anywhere later
in the top level* that suppresses it.

## Mechanism

`check_taint` is two-phase: phase 1 walks the top level flow-sensitively
(statement order matters — that is why the top-level form above is caught),
then phase 2 walks each function body against the **completed** set.

The two slot directions are implemented asymmetrically:

- **Outward is correct.** `taint_out_params` (`validate.rs:7521`) is called
  *during* the flow-sensitive walk. At the call site `fill =a`, if `fill`
  fills slot `j` from outside, `a` is inserted into `tainted` **at that point
  in the sequence**, exactly as if `argv a` had been written there. So a later
  attest legitimately cleans it, and no attest is correctly rejected — both
  measured.

- **Inward is wrong.** `tainted_slots_on_entry` (`validate.rs:7509`) is called
  from `taint_function_body`, which receives the **final** top-level set:

  ```rust
  for f in taint_top_level(program, &mut tainted, &cx, &mut diags) {
      taint_function_body(f, &tainted, &cx, &mut diags);   // final set
  }
  ```

  By then the trailing `attest v in a` has already removed `a`, so
  `tainted_slots_on_entry` reports slot `A` clean and the body is walked as if
  it had been handed a clean collection.

This is the flow-insensitivity of phase 2, but it is not the *documented*
imprecision. DD#100's "inward taint is a union over call sites" describes
being conservative — checking a function as tainted if **any** call site passes
tainted. This is the opposite failure: it is *permissive*, treating the slot as
clean because of a statement that runs strictly later.

## Proposed fix — mirror the outward direction

Capture the inward fact **at the call site, during phase 1**, instead of
reconstructing it from the final set in phase 2. When the flow-sensitive walk
reaches `each =a` and `a` is tainted *at that point*, record `(each, j)` as
entered-tainted. Phase 2 then seeds `fn_tainted` from that record rather than
from `tainted_slots_on_entry`.

That restores the union-over-call-sites semantics DD#100 actually specifies
(union of the point-in-time facts), makes the two directions symmetric, and
keeps the no-fixpoint architecture — the record is grow-only and built during
the same single pass.

Note `analyze_slots`'s own fixpoint is **not** implicated: it computes a purely
structural summary (which `(function, slot)` pairs are bulk-filled from
outside) with no attest in it at all. The bug is entirely in how that summary
is joined to the flow-sensitive set.

## Why it went unseen

`spec/collparam.sh` covers inward taint (`100.38`-`100.45` and neighbours) but
every case either attests *before* the call or not at all. No spec places a
correctly-named attest after the call, which is the only shape that triggers it.

## Found by

Answering DG's question "concretely, how does the slot approach address this?"
— measuring the two directions separately is what exposed the asymmetry.

---

## Resolved 2026-09-30 by DD#121

Fixed, by dissolution rather than by patching `tainted_slots_on_entry`.

DD#121 R1 requires an `attest` to be the immediately following statement of the
one that taints its subject. The reproducer's trailing `attest v in a` is three
statements below `argv a`, so it is now **SB5088** and the program never
compiles — the question of which program point the slot analysis reads the set
at no longer has an answer that matters, because the set no longer shrinks
anywhere a caller could observe.

Verified: the reproducer is rejected with SB5088. DD#100 Part E (the attest
deferred *into* the callee) still works — the slot's arrival at the function
body counts as the tainting operation for R1, so the body's first statement is
adjacent to it. Pinned by `spec/collparam.sh` `100.28`.
