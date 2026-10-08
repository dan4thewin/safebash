---
title: DD#124 — an assignment's value must match the target's kind, and a map has no append position
status: accepted
---

Settled with DG (2026-10-08), out of a cold-line investigation: the assoc-array
branch of `handle_append_assign` had never executed in any spec run.

This is [DD#117](DD117-declaration-initializer-shape.md) one statement later.
DD#117 settled the *declaration* — `array a=x` is SB4064 — and explicitly left
append available as a statement:

> **`+=` is rejected in a declaration (SB4065).** A declaration creates a
> binding, so there is no previous value to append to. […] Append remains
> available as a *statement*, once the name is declared.

It gave that statement no shape rule, and nothing else had one either. So the
same mistake one line down was still a segfault.

## What was actually there

Sixteen broken spellings. Seven compiled to binaries that **faulted**, eight
wrote nothing and said nothing, and one ICEd the compiler:

| program | before |
|---|---|
| `array a=(x); a=y` | SIGSEGV, `len a` == 0 |
| `array a=(x); a+=y` | SIGSEGV |
| `array a=(x); a=$(echo hi)` | SIGSEGV |
| `array a=(x); a+=$(echo hi)` | SIGSEGV |
| `array a=(x); a+=$x` | SIGSEGV |
| `array a=(x); a+=([b]=2)` | SIGSEGV |
| `map m=([a]=1); m=y` | SIGSEGV |
| `array a=(x); a=([b]=2)` | silently emptied `a` |
| `map m=([a]=1); m=(y)` | silently emptied `m` |
| `map m=([a]=1); m+=([b]=2)` | **compiler ICE** (`ssa_taint.rs`) |
| `map m=([a]=1); m+=z` | compiled, wrote nothing |
| `map m=([a]=1); m+=(y)` | compiled, wrote nothing |
| `map m=([a]=1); m+=$x` | compiled, wrote nothing |
| `str s=a; s=([b]=2)` | silently emptied `s` |
| `str s=a; s+=([b]=2)` | compiled, ignored |
| `array a=([b]=2)` *(declaration)* | silently emptied `a` |

Three passes disagreed about one construct. `handle_append_assign` dispatched
on the *value's* shape and then on `state.assoc_arrays`, emitting an
`ArrayAppend` whose value was a whole `AssocArray`. `emit_array_append_c`
appends at the integer index `sash_table_len(tbl)`, which is meaningless for a
map, so that wrote nonsense. The SSA taint pass classes `AssocArray` as
assign-only, so the same shape also ICEd — correctly: it had found a genuine
unhandled read route.

The last row is DD#117's own blind spot. Its `InitShape` folded `Array` and
`AssocArray` into one `Collection`, so array↔map cross-shape matched
"collection into collection" and passed.

## The rule

**An assignment's value must have the shape of the target's kind (SB5090).**

The kinds are three, not two: `array`, `map`, scalar. A `( … )` list and a
`([key]=value)` literal are not interchangeable, and neither becomes the other.

| target | `( … )` | `([k]=v)` | scalar |
|---|---|---|---|
| `array` | ✓ | SB5090 | SB5090 |
| `map` | SB5090 | ✓ | SB5090 |
| scalar | SB5055 | SB5055 | ✓ |

A scalar target keeps SB5055, which names the declaration the author is
missing rather than the shape they wrote. Its match covered `( … )` but not
`([k]=v)`; that was the only change needed there.

**`+=` on a map is rejected (SB5091).** DG, 2026-10-08: *"reject += on maps -
favor clarity over compatibility with bash"*. bash merges, and merging is a
defensible reading, but a map has no end to append to — the append position of
a map is not a position at all. `m[key]=value` says what the author means, and
it already works. Rejecting also costs nothing we had: every spelling of
`m+=…` was broken.

A subscript assignment (`a[i]=v`) is untouched. It assigns to a *cell*, and a
scalar is exactly what belongs there; the collection's kind says nothing about
the cell's value.

## Deferrals

SB5090 defers to two better diagnostics, for the reasons DD#117 recorded:

- an **argv splat** (`array a=(x); a=$(argv 1)`) — SB5025 names the out-param
  form `argv N a`, which is the only thing that works. SB5090's advice
  ("write `a=(…)`") is actively wrong there.
- a **named splat** (`a=@{b:-}`) — SB5025 again.

In practice taint (SB5020) reaches the argv case first. The deferral is what
keeps either of them reachable at all.

## Divergence from bash, accepted

Same family as DD#117 and DD#97. bash promotes a scalar to `a[0]`, lets
`m+=([k]=v)` merge, and infers a collection from its value. sash has a declared
kind for every name and takes none of the three.

## Where it lives

One classifier, `shape_mismatch_msg`, is shared by the declaration rule
(SB4064) and the statement rule (SB5090), so `array a=x` and a later `a=x`
cannot drift apart. They are one mistake written in two places, and DD#117 only
ever reached the first. The match is total over `(TargetKind, InitShape)` and
carries the no-wildcard marker: the pair *is* the rule, so a `_` arm there is a
silent accept.

With the rule in place, `handle_append_assign` has no map case left — the only
collection append that reaches lowering is the one that works.

## Cost

Zero. The full sweep was green before and after, 0 flips. Every rejected
spelling was one no spec had ever written — which is exactly why sixteen of
them survived, and exactly what DD#117 said about its own.
