title: DD#61 — Array taint construction restriction and bless-all
status: implemented

How tainted data enters arrays, and how arrays are blessed.

## Motivation

`array a=( $(getenv HOME) $(getenv PATH) ); bless a [[…]]; echo $a[1]`
compiled and ran — a taint-check escape. Two blind spots:

1. The CST/IR taint pass (`validate.rs`) did not recurse `is_taint_source`
   / `expr_uses_tainted` into `Array` / `AssocArray` / `Concat` /
   command-substitution children, so an array literal containing
   `$(getenv …)` never tainted the array. (`+=( … )` append leaked the
   same way; `a[i]=$(getenv X)` already tainted because its RHS is a bare
   cmdsub that `is_taint_source` did catch.)
2. The SSA taint backstop (`ssa_taint.rs`) `Subscript` check inspected
   only the index, never the array variable.

## Decision: restrict construction

A tainted value may **not** enter an array through program syntax:

- `array a=( $(getenv X) … )` — tainted element in a literal — rejected.
- `a+=( $(getenv X) )` — tainted element appended — rejected.
- `a[i]=$(getenv X)` — tainted value into a slot — rejected.

The **only** way an array comes to hold tainted data is a bulk external
read: `read -a`, `mapfile` / `readarray`, and the planned
`array a=$(argv)`. Element-wise taint introduction is a compile error
(SB5021) that directs the user to read/mapfile or to bless a scalar
first and place the clean value into the array.

### Why

This makes every tainted array **dynamic-arity and homogeneous** by
construction (all elements share one origin: lines of a file, fields of
a record, program args). Consequences:

- Whole-array taint (a `HashSet<String>` of variable names) becomes
  *correct*, not a compromise: per-element and whole-array taint coincide
  because all elements are uniformly tainted. DD#24's "each element
  carries its own taint" stays literally true — they all carry it
  together.
- "Validate every element with one predicate" (bless-all, below) is the
  complete and sound bless mechanism. Heterogeneous tainted arrays — the
  only case that would need per-element bless — cannot exist.
- The original bug is rejected at construction, earlier and clearer than
  any downstream bless gap.

### Escape hatch

To put external data into an array slot, bless a scalar first:

```bash
h=$(getenv HOME)
bless h [[ $h =~ ^/ ]]
array cfg=( "$h" )    # literal now holds only clean values
```

This validates per value with a per-value predicate — strictly more
precise than bless-all for heterogeneous data. Heterogeneous data should
generally be scalars, not an array.

## Decision: bless-all syntax (Go/for-style)

```bash
mapfile -t a < f
bless x in a [[ $x =~ ^[0-9]+$ ]]
```

- Fresh binder `$x` = each element; universal quantification is implied
  by bless's all-or-nothing semantics (a single tainted element is
  dangerous, so `any` is never meaningful — only `all`).
- Spelling chosen for cross-language intuition: Go `for _, x := range a`,
  Rust `a.iter().all(|x| …)`, Python `all(… for x in a)` all use an
  explicit per-element binder. `in` is already a sash keyword.
- The scalar form `bless port [[ $port =~ P ]]` is unchanged. Applying
  the scalar form to an array variable, or the bless-all form to a
  scalar, is a type error directing to the correct form.

### Maps: two binders

A map (associative array) from an external read has **both keys and
values tainted**. bless-all for a map binds both:

```bash
sash-expand cfg | mapfile -m m
bless k v in m [[ $k =~ ^[A-Z_]+$ && $v =~ ^[[:print:]]*$ ]]
```

- Two binders `k v` = key and value of each pair; both fresh, test-
  scoped, shadowing.
- Both binders must appear on the LHS of a test operator (DD#49 extended
  to every binder) — a key that escapes unvalidated would be unsound,
  so the predicate must validate both. Use a permissive pattern if a
  side is genuinely unconstrained.
- All pairs pass → whole map clean. Any pair fails → fail (statement
  form exit 1; if-form false branch, map stays tainted).

### Binder arity

- 1 binder (`bless x in a`) ⇒ `a` must be an indexed array; `$x` is each
  element value (integer keys are clean/program-generated).
- 2 binders (`bless k v in m`) ⇒ `m` must be a map; `$k`/`$v` are each
  pair's key and value.
- Arity/kind mismatch (1 binder on a map, 2 on an indexed array) is a
  type error directing to the correct form.

**Enforcement correction (2026-10-16, journal 511):** more than two binders
with a `[[ ... ]]` predicate or DD#93's unary shorthand is SB4014. The parser
previously admitted the form, which could reach the transform's malformed
attest ICE once ordinary declarations prevented SB5031 from masking it.
Specs `93.62`/`93.63`. Arithmetic attest-all remains DD#113's SB5054 in
`validate`, including an over-long lead (`93.52`); that diagnostic is unchanged.

### Semantics

- Runs the predicate once per element, binding `$x` to that element.
- **Statement form**: any element fails → "bless: validation failed" to
  stderr, exit 1. All pass → whole array becomes clean (new SSA version).
- **if-form** (DD#47): any element fails → false branch taken, array
  stays tainted.
- **Empty array** (zero elements, e.g. mapfile of an empty file):
  vacuous-pass — ∀ over ∅ is true, array becomes clean. Matches
  `all([]) == True` (Python) and `[].iter().all() == true` (Rust). There
  is no data to leak.
- **Binder scope**: `$x` is confined to the `[[ ]]` test and never leaks
  out. If `x` already names a variable in scope, the binder shadows it
  only inside the test; the outer value is untouched afterward (Go/Rust/
  Python loop-binder intuition).

### Rules carried over from scalar bless

- DD#49 anti-tautology: the binder `$x` must appear on the **LHS** of the
  test operator.
- The predicate must validate the binder `$x`, not the array name.
- Allowed test operators: the same set as scalar bless (regex `=~`,
  string `==`/`!=`/`<`/`>`, file tests `-f`/`-d`/…). No external commands,
  no arithmetic.

## Spec tests

`spec/taint.sh` section DD#61 (`60.*`). Construction-restriction tests
are compile-time (check rc/err); bless-all runtime semantics use a
tainted source readable at runtime: `mapfile -t a < file` (file read =
tainted) or a map via `mapfile -m`.

## See also

- DD#24 — Taint quarantine and bless (canonical taint/bless model)
- DD#41 — Sash-native array syntax
- DD#42 — getenv/argv as explicit-origin builtins
- DD#49 — bless anti-tautology (var on LHS)

## Addendum (2026-10-31): the first failing cell decides, in the `if` form too

"Any element fails → false branch taken" did not say whether the cells after
the failing one are still tested. The statement form never tested them, because
it aborts at the first failure. The `if` form did: it ANDed every cell's
verdict, so a later cell's predicate still ran after the answer was known. A
predicate that can abort, such as `${y:?unset}`, then aborted a program whose
`if` should have taken its false branch (batch-531 B88d).

Both forms now stop at the first counterexample. The `if` form takes its false
branch there, and no later cell's predicate runs. When every cell passes,
nothing changes: each cell is tested once, and the collection is clean in the
true branch. This is the short-circuit that DD#93's 2026-10-31 addendum gives
a predicate's own `&&` and `||`, applied across cells.

Specs taint.sh atlazy.5 (an array) and atlazy.6 (a map, in insertion order).
atlazy.7 is the all-pass control. Journal 553.
