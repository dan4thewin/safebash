# 477 — the rule that stopped at the declaration

2026-10-08. Continuing the coverage queue. The finding is not a cold line; it
is what the cold line was standing next to.

## How it started

Re-measured at `4cabee2`: 98.27% lines, 98.86% functions, 92.12% branches.
`atok_to_text` had dropped 40 → 14, so the 32 specs from the previous session
did what they were meant to. The ranking went flat — no dominant entry left.

Working down the unexamined candidates, `handle_append_assign` had four cold
lines at 1143-46. Unlike most of the queue they did not look like a totality
arm or an ICE. They looked like live code:

```rust
other => {
    if state.assoc_arrays.contains(&a.name) {
        ssa.block_mut(block).insts.push(SsaInst::ArrayAppend {
            array: a.name.clone(),
            value: other,
        });
    } else {
        append_scalar(a, other, state, ssa, block);
    }
}
```

That is the map-append path. It is cold because no spec appends to a map. So:
write the spec.

```
map m=([a]=1)
m+=([b]=2)
echo "${m[b]}"
```

The compiler panicked.

## What was there

The ICE was in `ssa_taint.rs`, and it was **right**: `ArrayAppend`'s value is a
read sink, so an `AssocArray` arriving there contradicts the `ASSIGN_ONLY_VALUE`
classification. It had found a genuine unhandled read route, which is exactly
what it was put there to do (commit `b314718`, months earlier).

Probing outward from that one spelling turned up fifteen more. Seven compiled
to binaries that **segfaulted**. Eight compiled and silently did nothing, or
silently emptied the variable. The plainest of them is not exotic at all:

```
array a=(x y)
a=z            # SIGSEGV. `len a` reports 0.
```

And:

```
array a=(x)
a=$(echo hi)   # SIGSEGV
```

Three passes disagreed about one construct. The transform dispatched on the
*value's* shape and then on `state.assoc_arrays`. `emit_array_append_c` appends
at the integer index `sash_table_len(tbl)` — meaningless for a map. The taint
pass said the shape could not arrive at all. Nobody checked whether the value
matched the variable.

Reading `a.op` across `validate.rs` found three sites, all of them taint, all
of them acting on `Set`. There was no `+=` rule. There was no assignment shape
rule of any kind.

## The part that stings

DD#117 fixed this. On 2026-09-22, also out of a cold-line investigation, with a
table listing `array a+=(x)`, `array a+=x` and `map m+=([k]=v)` as SIGSEGV.

It fixed the **declaration**. And it said so, plainly:

> Append remains available as a *statement*, once the name is declared.

Which is true, and which quietly handed the same segfault to the next line of
the program. `array a=x` was rejected; `array a=(x)` followed by `a=x` was not.
One mistake, two sites, and the fix reached one of them.

DD#117 also had a blind spot inside its own rule. `InitShape` folded `Array`
and `AssocArray` into a single `Collection`, so array↔map cross-shape read as
"collection into collection" and passed:

```
array a=([b]=2)   # a declaration, accepted, silently empty
```

So the rule did not even cover the site it was written for.

## The decision

Everything except one case was settled by DD#117's own principle — no implicit
promotion, reject what the compiler does not fully understand. Applying a
stated principle to all its consequences is not a new decision.

The exception: `map m+=([k]=v)` has a value whose shape *matches* the target's
kind, so by DD#117's rule it should work — and there was no implementation. I
put the fork to DG: implement it as a merge (bash's reading, and cheap, since
desugaring to `m[k]=v` reuses machinery that already works), or reject `+=` on
maps outright.

DG: *"reject += on maps - favor clarity over compatibility with bash"*.

That is DD#124. A map has no end to append to; the append position of a map is
not a position at all.

## The shape of the fix

One classifier, `shape_mismatch_msg`, shared by the declaration rule (SB4064)
and the new statement rule (SB5090) — because `array a=x` and a later `a=x` are
one mistake, and keeping two copies is how DD#117's blind spot would have been
reproduced in the new code. It is total over `(TargetKind, InitShape)` and
carries the no-wildcard marker: the pair *is* the rule, so a `_` arm there is a
silent accept.

`InitShape` gained the array/map distinction DD#117 lacked, which fixed the
declaration hole as a side effect — the same six-arm table answers both sites.

SB5055 kept the scalar target, since it names the declaration the author is
missing rather than the shape they wrote. Its match covered `( … )` but not
`([k]=v)`; one arm added.

And `handle_append_assign` lost its map branch entirely — the cold lines that
started this. With SB5090 and SB5091 in front of it, the only collection append
that reaches lowering is the one that works. The `ssa_taint` ICE stays: its
parameter is an `SsaValue`, so it is orthogonal to the gate by DG's rule, and
it has now proved it once.

## Gates

`mk test` 78 + 27 + 20. Sweep **7797** — predicted as 7751 + 2×23 before
running it. 0 failing, 0 flips, exactly 46 new ids, gate self-test 248.

`assign_shape_diag` came in at cc 11 / cog 7 and tripped the complexity gate
(3× mean = 11.25 / 5.31). Split into a kind lookup, a deferral predicate and a
diagnostic builder: cc 6 / cog 4. Not bypassed.

## What to take from it

DD#117's closing line was that the cost was zero, because *"every rejected
spelling was one no spec had ever written, which is exactly why the segfault
survived this long."* That is just as true of these sixteen, and it is the
second time the same sentence has been written about the same rule.

A rule that is right about a construct is not automatically applied everywhere
the construct appears. DD#117 named the statement form explicitly — in the
sentence that *granted* it — and still did not check it. The place to look for
the next one of these is wherever a DD says "X remains available as Y": that is
a scope boundary stated out loud, and nothing enforces it.
