# 378 — A storage annotation in an expression slot

Cold-line queue, next entry: `lower_disposition_as_int` in
`codegen/values.rs`. The cold region was one arm — `SsaValue::IntCell` — at
zero hits in the union measurement, and its C mirror in `emit_c.rs` was zero
too. Two backends agreeing that a case never happens is worth more than one.

## What the arm did

It unwrapped and recursed:

```rust
SsaValue::IntCell(inner) => Some(lower_value_as_int(inner, /* … */)),
```

That is the right answer. It is also exactly what made it worth removing, and
the reasoning is the same one journal 375 used on `intern_or_declare`: a
silent correct fallback papers over the drift that reaches it.

## Probing first, as always

I did not start from the call sites. I started from the question *what input
reaches this?* and ran a differential against bash across eighteen int-cell
shapes — arithmetic, cell-as-index, conditions, division, modulo, zero,
negative, `i64::MAX`, `i64::MIN`, string interpolation, `[[ = ]]`, three
spellings of `+=`, splat, join, `for … in`. Every one matched bash. Two
`SB4038` rejections were the deliberately-retired `${#…}` sugar.

So: no bug here. That is a result, not a dead end — the eighteen probes are
what let me say the following with confidence rather than hope.

## Why it is unreachable

DD#76's `IntCell` is a **storage** annotation. It is constructed at exactly
four sites, and I enumerated them mechanically rather than by reading:

- `transform/stmt_handlers.rs:981, :991` — the RHS of an int-cell subscript
  assignment, `+=` and plain.
- `transform/statements.rs:696, :702` — the elements of an `int array` /
  `int map` literal.

All four produce a value on its way *into* a table cell. Two further facts
close the set:

- Both **string**-context lowerers (`codegen::values::lower_value` at 374, 95
  hits; `emit_c::lower_value` at 1399) strip the wrapper and recurse on the
  inner value. That is the path every int cell actually takes.
- None of the four sites wraps a value that is already an `IntCell` — each
  wraps something from `expr_to_value`, which never builds one. So nesting
  cannot occur, and the stripping is therefore total.

Reaching int context with the wrapper still on would need a double-wrapped
cell. Nothing produces one.

## The distinction I owed myself

My own capture_purity note says *"never observed" is not "cannot happen"*, and
I stand by declining the ICE there. The difference is what the argument rests
on. capture_purity's unreachability depends on **pass ordering** — a property
that a future refactor changes invisibly. This one is a **closed enumeration
of construction sites**, which is mechanically re-checkable: `grep` for
`IntCell(` and count. Four. If a fifth appears, the ICE is precisely the thing
that reports it.

That is the test I want to apply going forward: not "have I seen it happen"
but "is the argument for impossibility re-verifiable by a command".

## Where the guard lives

Not in either backend. The invariant belongs to the type, so
`ssa::int_cell_in_int_context` sits next to the `IntCell` variant it describes,
and both backends call it. One doc comment, one unit test, no duplication —
and the next person to read the variant finds the rule attached to it.

This also sidesteps the practical problem that made me hesitate: a unit test
for the native arm would have to build a `FunctionBuilder` and an
`ObjectModule`. Testing the extracted helper needs neither.

## The gap the probing found

Eighteen probes, no bug — but `a[i] += n` on an int collection had **no spec
at all**. Every `ivc.*` case used plain `=`. That is a read-modify-write arm
with its own code path, and the failure mode is nasty: on single-digit values
an arithmetic add and a string concat are hard to tell apart at a glance
(`1+2` = 3, `"1"."2"` = 12 — but `5+5` = 10 either way if you squint at the
wrong case).

Five specs, `ivc.18`–`ivc.22`, pinning both halves against each other: int
array `+=`, int map `+=`, the **contrast** case where a non-int collection
concatenates, `+=` reading another cell, and a negative addend (which
subtracts, rather than concatenating `"-3"`).

The contrast case is the one that matters. A spec that only checks the int
path passes just as happily if the int path silently becomes the string path
for everything.

## Result

Sweep 5805, zero failing, zero flips. The sweep is itself the evidence: 5805
programs, including eighteen freshly-probed int-cell shapes, and the guard
never fired.
