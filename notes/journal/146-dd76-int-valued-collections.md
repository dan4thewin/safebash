# 146 — DD#76 int-valued collections (int array / int map), Stages A+B

DG: "fold int-valued collections into this arith-subscript effort" — a collection
whose elements are integers, the value-side mirror of DD#33's two key
representations. Recorded as **DD#76** (int-valued collections, `val_len`
double-duty). This session implemented Stages A (declaration + type) and B
(codegen store/read), native + c. Stage C (bare `a[i]` arith lvalue) and the
janet backend remain.

## Storage — `val_len` double-duty (DG chose encoding b)

`TableEntry { key_ptr, key_len, val_ptr, val_len }`. An **int cell** is

    val_ptr = null,  val_len = the i64 value

`val_len` does double duty: byte-length for a string cell, the integer value for
an int cell. Keys are hashed → an int key must be 8 bytes (DD#33); values are
never hashed → an int value rides in `val_len` with no allocation, no `atoi`.
The runtime needed **zero changes** — `sash_table_set`/`_get` already store and
return `(val_ptr, val_len)` verbatim (they copy only the *key* to the heap).
A missing key `(null, 0)` harmlessly aliases an int-0 cell `(null, 0)` — both
are zero-value (DD#53).

## Mechanism (two minimal SSA additions)

- **Write** — new `SsaValue::IntCell(Box<SsaValue>)` = "store inner (as i64) as
  `(null, i64)`". The transform wraps every element of an `int array`/`int map`
  literal (wrap_int_cells) and the RHS of an int-cell element assign
  (build_subscript_rhs). The generic table-store path (`lower_value`) then emits
  the int-cell encoding with **no** per-site plumbing — array-literal init and
  element-assign both route through it untouched.
- **Read** — `is_integer_value` on `SsaValue::Subscript` (the value-side twin of
  `is_integer_key`). String context → `itoa(val_len)`; int/arith context → the
  raw `val_len`, skipping the string round-trip and the null-`val_ptr` atoi.

Both derive from `type_map.get(name) == Some(Int)` in the transform (the int
collection's name carries the element type).

## The slot conflation (the one non-obvious fix)

Declaring the collection's *name* as `Int` (to type its elements) made codegen
allocate an **int scalar slot** for the collection variable → ICE "expected
string var, got int". A collection variable holds a **table pointer** regardless
of element type. Fixed by keying slot allocation off the *value shape*: a
variable assigned an array/map literal is a table (Str slot), even when its
elements are int. Three parallel sites: native `declare_inst_bases`, emit_c
`collect_inst_var_types`, emit_c `emit_assign`.

## Consistency fallout (documented, not a new decision)

Bare `a[1]=99` → SB4030, uniform with the scalar rule (`x=5` reassignment also
needs `$(( ))`; only *declaration* init accepts bare numerics: `int x=5`,
`int array a=(10 20 30)`). Store an int cell with `a[1]=$(( 99 ))` — no DD#30
quotes (the cell is int; a string collection still needs `a[i]="$(( … ))"`).
This falls straight out of the established rule; no `check_assignment` change.

## Parser scope

Added only the top-level **`int`+structure** stack (`int array`/`int map`);
`local int array` already stacked via parse_local's modifier loop. A broad
loop-refactor first attempt inadvertently made `const array` valid (spec 35.2
asserts it's an error) — reverted to a surgical add. Whether `const` should
stack onto a collection is a separate DD#35-orthogonality question, left to DG.

## Backends

- **native**: full (gate).
- **c**: full except `local int array` inside a function — emit_c leaves the
  function-local table var undeclared (filed c-local-int-collection-decl); ivc.8
  is native-only pending that.
- **janet**: its `is_int(name)` model conflates the collection variable with its
  element type (`scan-number` on the table) — filed janet-int-valued-collections;
  ivc runtime tests skip janet (compile-rejects ivc.9/10 run in every mode).

## Verification

spec arithmetic.sh plan 90→101 (ivc.1-11). Full 3-mode sweep, latest-run-per-
test diff vs HEAD: **0 passing→failing, 0 gone, 23 new passing**. native 0-fail.
clippy clean. Rust codegen 179/77 + e2e 89/41 == benched baseline.

Commits: DD#76 + task record; Stages A+B implementation. Stage C next.
