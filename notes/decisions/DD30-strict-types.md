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

## Addendum (2026-10-25) — `[[ ]]` does not convert an int operand

`[[ ]]` compares strings (DD#62's exact `==`, DD#63's `=~`, the unary string
tests), so a whole-int operand there is an implicit int→string conversion,
which this DD rules out. It was accepted everywhere: `[[ $(( 10 / z )) == 1 ]]`
compiled. It is now SB4030, the same code `str s=$n` gets, and the message
says how to convert: quote it (`"$n"`), or compare numbers with `(( … ))`.
Settled by this DD and DG ("I would expect an error"), as step D1 of the
batch-531 port (journal 535).

Where it applies: every operand of every test operator, unary and binary,
through `&&` and `||`; the DD#66 dispatch subject and each pattern (see the
DD#66 addendum of the same date); and `attest` predicates (DD#93), including
the Part B shorthand. `attest n -n` builds the test `[[ -n $n ]]`, so on an
int `n` the message shows the predicate written out in full,
`attest n [[ -n "$n" ]]`.

`expr_type` decides, so this agrees with the assignment checker: an int
variable or cell, `$(( … ))`, `$(f)` of an `-> int` function and `$$`
(DD#111's `$(getpid)`) are ints; `"$n"` and in-word `x$n` are strings and
stay legal. validate's codes run first, so a numeric test operator is still
SB5019 whatever its operands.

EXIT and ERR operands are covered too (B5's typing prerequisite, journal 565,
followed by DD128; `h128.typeflow.11`). The checker covers every body and
supplies lowering's actual type map. Still filed: `expr_type` types `${n:?msg}` on an int as
a string (`notes/tasks/bug-expr-type-int-disposition.md`).

`sash-convert` quotes the operands it can tell are ints (DD#57 addendum of
the same date). Specs `spec/types.sh` `30.50`-`30.65`; fifteen existing
programs were rewritten to quote or to use `(( ))` (journal 535).
