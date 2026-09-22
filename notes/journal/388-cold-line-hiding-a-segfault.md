---
title: "The cold line that was hiding a segfault"
date: 2026-09-22
---

# 388 — The cold line that was hiding a segfault

Next item on the cold-line queue was `parse_one_decl_assignment`. The lcov
said lines 888-895 — the `+=` arm — had never run, while the `matches!` test
guarding them ran 55 times. A declaration list has exactly one caller, so this
looked like the same kind of small, contained question as yesterday's
`build_expr_from_parts`.

The question is always the same one: *what input reaches this?* Here it is
obvious — `str s+="a"`. So I wrote out the whole family rather than the one
case, which is the only reason this went the way it did:

```
str s+=a        ok [a]
int n+=1        ok [1]
const c+=1      ok [1]
array a+=(x)    ok []      ← Segmentation fault (core dumped)
map m+=([k]=v)  ok []      ← Segmentation fault (core dumped)
```

The compiler accepted both, emitted a binary, and the binary crashed.

## It was never about `+=`

Widening the probe one more step found the actual shape:

```
array a=x       SIGSEGV      bash: x
array a=""      SIGSEGV      bash: empty, one element
array a=$(echo x)  SIGSEGV   bash: x
map m=v         SIGSEGV      bash: accepts
```

`array a=x` is not an exotic spelling. It is a plain `=`, and it compiled
clean and segfaulted. The `+=` arm was a symptom; the defect was that
**nothing checked a declaration's initializer against the kind declared**, in
either direction. The mirror was silent rather than fatal: `str s=(x)` gave
`""` where bash gives `x`.

The worst single result was not a crash at all. `echo $(len a)` on
`array a=x` printed **3888644095** — a wild read returning uninitialised
memory. A safe-subset compiler was emitting programs that read off the end of
a table and kept going.

## The mechanism: an in-band sentinel

Bare `array a` worked. `array a=""` segfaulted. Both are an empty string, so
that difference is the whole bug:

```
array a      →  :value {:kind :literal :value ""}
array a=""   →  :value {:kind :double-quoted :parts []}
```

`Literal("")` *was* the representation of "bare declaration, no initializer",
and `is_empty_collection_init` tested for it literally. Anything else — any
scalar at all — fell past it into `expr_to_value` and was stored into a
collection slot. The sentinel is what made this invisible: the two states were
one value, so no reader could distinguish "nothing was written here" from
"a scalar was written here", and the second is not a thing a collection slot
can hold.

This is the same empty-vs-meaningful discriminator that bit in journal 385,
where `""` meant zero-init and `"1x"` meant unreadable and both became `0`.
Twice now, an in-band empty has been doing load-bearing work.

## DG's call

I took it to DG with the matrix and three options. He chose to **reject the
mismatch in both directions**, to **reject `+=` in a declaration**, and — the
part I would have been tempted to defer — to **remove the sentinel** rather
than patch around it.

The append ruling came with its reason attached: *a declaration creates, it
cannot append*. That reason is not about collections, so it applies to
`str s+="a"` and `int n+=1` too, which were working fine. Applying a stated
principle to all of its consequences rather than just the case that prompted
it is the rule in AGENTS.md, and here it was the difference between a
coherent rule and a spot-fix on the two spellings that happened to crash.

DD#117 records all of it.

## The representation

`Declaration.assignments` is `Vec<DeclAssignment>` now:

```rust
pub struct DeclAssignment {
    pub name: String,
    pub value: Option<Expr>,   // None = bare declaration
}
```

The point is not that it is tidier. Three of `Assignment`'s shapes are
meaningless in a declaration, and all three are now **unrepresentable** rather
than merely unused: no initializer is `None`, `+=` cannot be constructed, and
an index cannot either. The bug required a shape the type permitted and the
language did not.

29 sites touched `.assignments`. Most wanted the `Option` directly. Four
genuinely treat `int x=1` and `x=1` as the same thing (taint, slot rebinding),
so those go through one `as_assignment()` adapter — the equivalence stated
once, where it can be read, instead of re-derived four times where it can
drift.

## Ledger

- SB4064 (shape mismatch), SB4065 (`+=` in a declaration). DD#117.
- 20 new specs, `spec/types.sh` plan 245 → 265, `decl.1`-`decl.20`.
- Sweep **5985** (predicted), 0 failing, **0 flips** vs `9995bf6`.
- The complexity gate caught three functions at once — `check_decl_value_shape`
  9/7, `slot_rebind` 8/9, `compute_declaration_value` 7/7 against a 11.68/5.83
  bar. All three came down by extraction (max now 5/4). Extracting a helper
  remains the reliable cognitive fix.

## What to carry forward

The queue item was "a cold arm in a declaration parser". What it actually was:
memory unsafety reachable from a five-character program, sitting behind a
plain `=`.

Zero coverage did not mean the code was dead. It meant **nobody had ever
written that input** — and the reason nobody had written it is that it was
never specified, which is exactly where a compiler's undefined behaviour
lives. The cold line was not the bug. It was the only visible edge of it.

Worth repeating: the crashing spelling (`array a=x`) was never cold. Line 887
ran 55 times. If I had probed only the literal cold lines instead of the whole
family around them, I would have found `str s+="a"`, seen it work, and closed
the item.
