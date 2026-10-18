title: DD#30 — Strict Type Separation
status: implemented

No implicit conversion between int and string. Conversion happens
explicitly at boundaries: `$(( ))` for string→int, `"$var"` for
int→string (at output).

## Bare numeric assignment

`x=5` is a string assignment (equivalent to `x="5"`). The absence of
`int` keyword means string — there is no ambiguity in the grammar.
Use `int x=5` for integer variables.

## Refinement (2026-08-17, DG) — bare numeric literal to a known-int LHS

The "`x=5` is a string assignment" rule holds only for a **new, untyped**
variable. Once the LHS is already `int`, a bare **numeric literal** RHS is
accepted and keeps the int type — `x=5` on a known-int `x` aligns with
`int x=5`. This closes an asymmetry: declaration-init already accepted a bare
numeric literal (`int x=5`, `int array a=(1 2)`), but reassignment rejected it
(`int x=0; x=5` was SB4030). Applies to scalar reassign, `+=` (arithmetic add),
and DD#76 int-collection cells (`a[i]=99`).

Boundaries (unchanged):
- A **new** var still defaults to string — `y=5` makes `y` a string.
- Only a bare **literal** qualifies; a bare arith *expression* still needs
  `$(( ))` — `int x=0; x=5+1` stays SB4030 (`x=$(( 5+1 ))`).
- A string value to an int LHS still needs `$(( ))` (`x=$s` → SB4030).

`is_numeric_literal` (the declaration-init gate) is the shared predicate, so
reassignment accepts exactly what init accepts.

## Addendum (2026-10-18) — an unset subscript still needs a collection

`unset name[index]` deletes a collection element; brackets do not make a
scalar or an undeclared name a collection. The type checker had skipped every
subscripted operand, allowing `unset missing[0]` to compile and segfault.
The same collection-only check now covers whole and element targets,
including function locals, captures and collection slots (SB4030).

Checking and lowering share the existing logical-target normalizer, so split
bracket tokens and multiple targets cannot disagree. Both deletion forms
resolve DD#100 slots to the caller's collection; previously element deletion
looked up `A` itself, and whole deletion could silently assign an empty scalar
to it instead of clearing the caller. Missing elements of a declared
collection remain harmless no-ops.

Specs `spec/collparam.sh` `bindscope.25`-`bindscope.37`; journal 525.
