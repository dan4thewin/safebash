title: "An arithmetic `:-` default inside a subscript index escaped SB4030 — ICE natively, silent miscompile in C"
status: done
priority: P1
kind: bug
depends: []
gates:
  - "`$(( a[${n:-abc}] ))` is rejected by SB4030, not by a panic"
  - "both backends agree — neither compiles it"
  - "the private duplicate walker in `type_check.rs` is gone"
  - "specs pin the read, condition, and assign positions"

# The private copy of a walker that was fixed only in `ir.rs`

DD#77 D4 requires the `:-` default in arithmetic to be a DD#72 integer
literal: a non-literal would silently `atoi` to 0. `check_arith_default_literals`
enforces it, and `d4.1` has pinned the plain-operand case since the rule landed.

It did not reach a default sitting in a **subscript index**:

```sh
int n=1
int array a=(1 2 3)
y=$(( a[${n:-abc}] + 1 ))
```

- **native**: `panicked at sash/src/codegen/values.rs:1426: non-integer
  literal 'abc' reached int lowering; type_check (SB4030) should have
  rejected it` — the assertion naming the exact pass that let it through.
- **`--emit-c`**: **exit 0**, emitting `__t2 = (sash_atoi("abc", 3));`.

So the two backends disagreed about a program neither should accept, and the
C side is the worse outcome: not a crash, an answer. This is exactly the
divergence the differential oracle exists to catch, and no spec covered it.

## Cause

`type_check.rs` carried a **private duplicate** of `walk_arith_expansions`,
39 lines ending in `_ => {}`. The wildcard swallowed `ArithExpr::Subscript`
and `ArithExpr::SubscriptAssign`, so the walk never entered a subscript index
and the default inside it was never examined.

`ir.rs::walk_arith_expansion_roots` is **already total**, and its comment
names this precise hazard:

> skipping them let every expr-level check inside `a[…]` go unrun, e.g.
> `(( y = a[${x:~}] ))`

Someone fixed the shared walker for this. The copy in `type_check.rs` never
got the fix, and nothing connected the two.

## Fix

Delete the duplicate; call `crate::ir::walk_arith_expansion_roots`. The
`_roots` form is the right one: the deleted copy had root-only semantics
(`f(inner)` without descending), and `check_arith_default_literals`'s callback
does not recurse. Behaviour-preserving except for the two missing variants.

## Specs

`spec/arithmetic.sh` `d4.8`–`d4.13` (dual-mode, so +12):

- `d4.8` read position — was the ICE
- `d4.9` the same index inside `(( … ))` as a condition (`StmtKind::ArithTest`)
- `d4.10` `SubscriptAssign`
- `d4.11` a legal literal default in that position still compiles and runs
- `d4.12` one diagnostic per offending default, not one per walker
- `d4.13` the plain operand still rejects with an `int array` in scope

All three assert `$err != *panicked*`, not merely a nonzero status: a panic is
also nonzero, and asserting only the status would let the ICE back in.

## Note on reproducing

A plain `array` is a **string** array, so `(( a[i] ))` is rejected as a bare
string cell in arithmetic *before* the default is ever examined. `int array`
is required to reach the bug. The first probe attempt was confounded by this.

## The shape

This is the fifth non-total match over an IR enum to be the bug, and the
second **second-implementation** failure — a private fork of shared logic that
missed a fix the original received (the first was the argv builders). Worth a
sweep for other private duplicates of shared walkers.
