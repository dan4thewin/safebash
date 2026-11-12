title: DD#117 — a declaration's initializer must match the kind declared
status: accepted

Settled with DG (2026-09-22), out of a cold-line investigation: the `+=` arm
of `parse_one_decl_assignment` had never executed in any spec run.

## What was actually there

Nothing checked that a declaration's initializer had the shape of the kind
being declared, in either direction. Both directions were broken, and one of
them was memory-unsafe.

**Scalar into a collection slot — compiled clean, segfaulted on read.**

| program | sash (before) | bash |
|---|---|---|
| `array a=x` | **SIGSEGV** | `x` |
| `array a=""` | **SIGSEGV** | empty, one element |
| `array a=$(echo x)` | **SIGSEGV** | `x` |
| `map m=v` | **SIGSEGV** | (accepts) |
| `array a+=(x)` | **SIGSEGV** | `x` |
| `array a+=x` | **SIGSEGV** | `x` |
| `map m+=([k]=v)` | **SIGSEGV** | `v` |

Worse than the fault: `echo $(len a)` on `array a=x` returned **3888644095** —
a wild read that yielded uninitialised memory instead of crashing. A compiler
whose premise is a *safe* subset was emitting programs that read off the end
of a table.

**Collection literal into a scalar slot — silent.** `str s=(x)` produced the
empty string where bash yields `x`. And `str s+=(x)` produced `(x)`, so `=`
and `+=` disagreed with each other about the same input.

## Root cause: one in-band sentinel

`parse_one_decl_assignment` decided "this is an array literal" purely on
seeing `(` after `=`, with no regard to the declared type. Downstream,
`is_empty_collection_init` tested

```rust
a.value.kind == ExprKind::Literal(String::new())
```

because a **bare** declaration (`array a`) was represented as an in-band
`Literal("")`. That is why bare `array a` worked and `array a=""` did not:
the latter parses as `DoubleQuoted([])`, semantically the same empty string,
which missed the sentinel and fell through to `expr_to_value` — storing a
*scalar* into a collection slot.

Meanwhile `check_one_decl_assignment` validated only *int collection
elements*. Nothing anywhere asked whether a collection's initializer was a
collection.

## The rule

**A declaration's initializer must have the shape of the kind declared
(SB4064).**

- `array`/`map` require a `( … )` literal. A scalar does **not** become a
  one-element collection. sash has no implicit promotion, so `array a=x` is
  rejected rather than read as `a=(x)` — it is far more often a typo than an
  intent, and DG's standing principle is that the compiler rejects anything it
  does not fully understand.
- A scalar (`str`/`int`/`const`) rejects a `( … )` literal.
- A `ParameterExpansion` initializer is neither, and passes through: that is
  the DD#99 `map m=@{a:-}` fill-from-array form and the ordinary `str s=$x`.
  Their own rules live in `check_map_decl_splats` and SB5025.

**`+=` is rejected in a declaration (SB4065).** A declaration creates a
binding, so there is no previous value to append to. This applies to every
declared kind — `str s+="a"` and `int n+=1` are rejected alongside
`array a+=(x)`, not just the collection cases that segfaulted. Append remains
available as a *statement*, once the name is declared:

```sh
str s="a"
s+="b"        # fine
```

> **2026-10-08 ([DD#124](DD124-assignment-value-shape.md)):** that statement
> had no shape rule of its own, so `array a=(x)` followed by `a=x` segfaulted
> exactly as `array a=x` had. Sixteen spellings were broken. The rule below is
> now shared with the statement site, and `InitShape`'s `Collection` has been
> split into `ArrayLit`/`AssocLit` — folding the two together let
> `array a=([b]=2)` pass *this* rule and silently empty `a`.

## Divergence from bash, accepted

bash reads `declare -a a=x` as `a[0]=x` and `declare s=(x)` as making `s` an
array. sash takes neither: it has a declared type for every name, so the first
is a mismatch and the second has no type to become. This is a deliberate
narrowing of the subset, in the same family as DD#97 (a collection is
introduced by the keyword, not inferred from its value).

## Representation: the sentinel is gone

`Declaration.assignments` is now `Vec<DeclAssignment>`, not `Vec<Assignment>`:

```rust
pub struct DeclAssignment {
    pub name: String,
    pub value: Option<Expr>,   // None = bare declaration
}
```

Three of `Assignment`'s shapes are meaningless in a declaration and are now
**unrepresentable** rather than merely unused: no initializer is `None` rather
than `Literal("")`, `+=` cannot be built at all, and an index (`array a[0]=x`)
never parsed in the first place. `DeclAssignment::as_assignment()` is the one
adapter for the passes that legitimately treat `int x=1` and `x=1` alike
(taint, slot rebinding), so that equivalence is stated once instead of being
re-derived per pass.

The `--emit-ir` seam changes with it: a declaration assignment is now
`{:name … :value <expr-or-nil>}`, with no `:op` or `:index`.

## Cost

Zero. The full sweep was green before and after, 0 flips — every rejected
spelling was one no spec had ever written, which is exactly why the segfault
survived this long.
