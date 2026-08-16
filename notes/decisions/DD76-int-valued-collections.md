title: DD#76 — Int-valued collections (val_len double-duty encoding)
status: accepted

Makes DD#35's orthogonality concrete for the *value* half of a collection: an
`int` type keyword may stack onto `array`/`map`, giving a collection whose
elements are integers (`int array a=(10 20 30)`, `int map m=([k]=5)`). This is
the value-side mirror of DD#33's two *key* representations.

## Motivation

DD#35 declares four orthogonal, stackable axes — mutability (`const`), lifetime
(`local`), type (`int`), structure (`array`/`map`). The type axis was wired for
scalars only; `int array`/`int map` errored (SB4014). DD#33 already gives keys a
dual representation (int index vs string key) in the shared `(key_ptr, key_len)`
fields. The same idea applies to values — DG, 2026-08-16: "how we accomplished
both string keys and integer indexes … presumably the same can apply to values."

## Declaration

The `int` type keyword stacks before the structure keyword (DD#35 axis order:
type then structure):

    int array a=(10 20 30)      # indexed, int-valued
    int map m=([k]=5 [j]=7)     # string keys, int values
    local const int array a=…   # all axes stack

A collection is **homogeneous**: all-int or all-string, fixed at declaration.
The default (no `int`) is string-valued, unchanged. Element initializers of an
`int` collection must be numeric (a non-numeric element is a type error, like
`int x=hello`).

## Storage — `val_len` double-duty

`TableEntry` is `(key_ptr, key_len, val_ptr, val_len)`. For an **int** cell:

    val_ptr = null,  val_len = the i64 value

`val_len` does double duty — byte-length for a string cell, the integer value
for an int cell. No heap allocation; the value lives in the length field.

**Why values differ from keys.** Keys are *hashed* (FNV-1a over the byte slice),
so an int key must be materialized as 8 bytes and passed `(&i64, 8)` (DD#33).
Values are *never hashed*, so an int value needs no bytes — it rides in
`val_len` directly. This is strictly cheaper (no stack slot, no heap copy, no
`atoi`), and it is the encoding DG recalled.

**Reads.** The collection's static type (known at compile time) selects the
interpretation — an `is_integer_value` branch mirroring `is_integer_key`:

  - arithmetic / int context → the i64 **directly** from `val_len`
    (`sash_table_get`'s `out_len`); no `atoi`, no string round-trip.
  - string context (`echo ${a[0]}`) → `itoa(val_len)`.

**Missing key / int 0 overlap is harmless.** `sash_table_get` signals a missing
key as `(null, 0)`. An int cell holding `0` is also `(null, 0)`. Both denote
**zero-value** (DD#53), so the overlap needs no disambiguation — a missing int
element and a present `0` are semantically identical.

## Runtime

No runtime change. `sash_table_set` / `sash_table_get` already carry arbitrary
`(val_ptr, val_len)`; `val_ptr = null` + `val_len = value` is a valid entry the
table stores and returns verbatim. Only codegen changes (it emits the int-cell
store and the type-directed read).

## Consequences

- `${a[i]}` / `${m[k]}` on an int collection type as **Int** — usable directly
  as `int x=${a[0]}` and in arithmetic without SB4030.
- `a[i]=$(( … ))` on an int collection stores the int with **no** DD#30 `"…"`
  quoting (the cell is int). On a string collection, the DD#30 quote is still
  required (`a[i]="$(( … ))"`) — unchanged.
- Arithmetic on an int-array cell is raw i64 end-to-end (store and read),
  making bare `a[i]` in `(( ))` (the arith-subscript lvalue, arith-subscript-
  values) a natural fit — no coercion.

## Scope note

Implemented incrementally (parser axis-stacking → type tracking → codegen
int-cell store/read → the bare `a[i]` arith lvalue). c/janet backends get the
int-cell path or a filed per-backend guard. See
notes/tasks/int-valued-collections.md.

This change adds only the **`int`+structure** stack (`int array`/`int map`) at
top level; `local int array` already stacked via parse_local. Whether **const**
should stack onto a collection (`const array` — today a compile error, spec
35.2) is a separate DD#35-orthogonality question left to DG, not decided here.
