---
title: DD#97 — a collection is introduced by the `array`/`map` keyword
status: accepted
---

Settled with DG (2026-09-08), from his bug report: *"when I do `f=(*)` I see
`$(len f)` return 0 despite the array having elements."*

## The two defects

The report was one symptom of two independent defects that met at the same
place — the transform's `state.collections` registry, which decides whether a
name's length is `arrlen` (element count) or `strlen` (byte length of the
slot). An unregistered collection gets `strlen` on a table, which reads **0**.

**1. A glob array was never registered, even when declared.**
`track_collection_kind` inferred collection-ness from the *lowered value*,
matching only `IndexedArray`/`AssocArray`. A glob literal lowers to
`ParamExpansion { operator: "glob" }`, so it fell through:

```sh
array f=(*.txt)
$(join f)   # → /work/a.txt /work/b.txt /work/c.txt   (three elements)
$(len f)    # → 0                                      ✗
```

`join` and `len` disagreed about the same variable. Fixed by registering from
the **declaration keyword** instead of the value shape — the keyword is what
the user wrote, and it cannot be defeated by however the value happens to lower.

**2. A bare `f=(…)` registered nothing at all** — only the declaration path
ever called `track_collection_kind`. It half-worked, which is worse than not
working:

| | `array f=(a b c)` | `f=(a b c)` |
|---|---|---|
| `${f[1]}`, `for x in @f` | ✓ | ✓ |
| `$(len f)` | 3 | **0** |
| `$(join f)` | `a b c` | **SB4030** |
| `f+=(d)` then `len` | 4 | **0** |
| `unset f` | ✓ | **SB4030** |
| `f+=(d)` undeclared | — | **codegen ICE** |

## The rule

**A collection is introduced by the `array`/`map` keyword.** Giving a
collection value to a name that is not already a collection is **SB5055**:

```sh
f=(a b c)      # SB5055 — declare it `array f=(a b c)`
f=(*)          # SB5055 — a glob is an array literal (DD#69)
f=()           # SB5055
f+=(d)         # SB5055 (was an "ICE: undefined array" panic)
x=hi; x+=(a)   # SB5055
```

*Re-assigning* an already-declared collection stays legal — the rule is about
**introducing** one without the keyword:

```sh
array f=(a b c)
f=(x y)        # ok — len 2
f+=(d)         # ok — len 4
```

## Why reject rather than infer

DG chose rejection over "make bare `f=(…)` work." Both would have removed the
0, but:

- **DD#41 already made `array arr=(…)` the declaration form.** Bare `f=(…)`
  was never specified — it was an accident of the assignment path, not a
  designed alternative.
- **DD#75's principle**: the named/keyword form is canonical. Two spellings for
  one concept is exactly the ambiguity that DD pivoted away from.
- `f=(a b c)` is visually a scalar assignment that silently produces a table.
  Requiring the keyword makes the kind of a name legible at its introduction —
  *unambiguous, self-consistent, transparent*.
- Migration cost was zero: no spec and no contrib script used the bare form.

The counter-argument — scalars need no keyword, so requiring one for
collections is asymmetric — is answered by what the keyword buys: a scalar's
kind is not in question, a collection's is, and the `(…)` alone was too quiet
a signal to carry it.

## Consequences

- **An ICE is gone.** `f+=(d)` on an undeclared name panicked in codegen
  (`types.rs:190`, "ICE: undefined array"). It is now SB5055. This is the same
  shape as ud.19 (`${z[0]}` on an undeclared array), on the write side.
- `len`/`join`/`unset` can no longer disagree about whether a name is a
  collection, because there is exactly one way to introduce one.

## Not in scope

- The bulk-read auto-declares (`argv a`, `read -a a`, `mapfile a`) still
  introduce a collection without an `array` keyword. They name the target in a
  command that is unambiguously a collection fill, and both the transform
  (`autodeclare_bulk_target`) and the type checker (`register_bulk_read_target`)
  already register them. Unchanged.
- The janet backend renders array append as string concatenation
  (`(set f (string f @["d"]))`). Independent of this DD — janet emits from the
  AST, not the SSA — and filed as `janet-array-append-string-concat`.

## Specs

`spec/undeclared.sh` ud.22–ud.29 (the reject, its message, the former ICE, and
the two "declared re-assignment stays legal" controls) and `spec/glob.sh`
g.7–g.9 (`len` on a glob array; `len` and `join` agreeing; a zero-match glob
being a real empty array).
