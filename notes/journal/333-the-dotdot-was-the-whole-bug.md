# 333 — `..` was the whole bug

Bug #13, found in the same probe session as #12, and the most serious of the
three this segment.

I was collecting behaviour for the `validate.rs` cold branches around
subscripts and tried a tainted map key:

```sash
map m=([k]=v)
k=$(printf k)
/bin/echo "${m[$k]}"
```

It was rejected — but by the SSA taint backstop, with

```
c.sh: tainted variable k_1 used as argument to command '/bin/echo' without attest (SSA taint backstop)
```

No SB code, no line:col, an SSA-internal name. Every other taint rejection in
the compiler is a clean `SB5020` at a source location. `validate` is the
authoritative gate and the backstop is defense-in-depth
(`notes/tasks/ssa-taint-full-backstop.md` says so); anything reaching the
backstop is a hole in the gate.

So I built the position matrix. Six positions, tainted index:

| position | result |
|---|---|
| external command arg | SSA backstop |
| builtin `echo` arg | SSA backstop |
| `printf` arg | SSA backstop |
| `a[$i]=v` write index | SSA backstop |
| `s="${a[$i]}"` | **accepted** |
| `[[ -n ${a[$i]} ]]` | **accepted** |

The last two compiled and ran. A tainted index selecting a cell, the result
treated as clean, no diagnostic anywhere. That is a laundering bypass, not a
message-quality problem.

## The code

```rust
ExprKind::Subscript { name, .. } => push_taint_use(tainted, name, …)
```

Three walkers, all identical, all discarding the index. `..` was the whole
bug.

DD#24 could not be clearer. Its use-site list contains `arr[$x]=v` — "index
expression" — and the 2026-08-26 amendment says the list is
**position-uniform**, naming the assignment RHS and the `[[ … ]]` operand as
positions that had already been found missing once.

So this is the sixth instance of the pattern, and the first where the
authoritative text is not a code comment but a decision document. The DD
described a class. The code handled the instances someone had tried.

## Why it survived

The sweep after the fix was **5071 — identical to before**. Not one existing
spec used a tainted subscript index. The entire surface was uncovered, so
nothing could disagree with the code.

That is worth sitting with. This is the exact failure mode the coverage push
is for, and coverage *did* find it — but not through the number. The cold
branches I was mining (`splat_collection_name`, `ambiguous_index_diag`) are
not the ones that were wrong. What found the bug was being forced to enumerate
the shapes that reach a cold branch, and trying one nobody had tried.

The number is a to-do list, not a detector.

## A second bug, exposed by the first

With the fix in, the new diagnostic pointed at line 1 of a 3-line program.

`parse_subscript_index` re-parses the captured index text with a **fresh
`Scanner`**. `Span` is `{line, col}`, not a byte offset, so that scanner
numbers from the start of the index *substring*. Every index expression the
compiler has ever built carried a 1:1-relative span — invisible until a
diagnostic finally reported on one.

The index expression is now re-anchored to the enclosing `${name[…]}` span,
which is where the read is. Exprs nested deeper inside an index still carry
relative spans; that needs either a deep re-span or a `Scanner` that accepts a
base position, and it is filed rather than guessed at
(`notes/tasks/subscript-index-nested-span.md`). Spec 60.68 pins the line
number so this cannot silently come back.

## What I did not decide

Whether a tainted index makes the retrieved value tainted. It does not arise —
DD#24 has no propagation, and the read of the index is rejected before
anything can be asked about the result. Worth stating because it looked like a
fork for a minute, and it is not one.

## Numbers

- Sweep **5095** passing / 10 skipped / 0 failing. The fix itself moved
  nothing (5071 → 5071); the 12 new specs are all new surface.
- Units 83 green.
