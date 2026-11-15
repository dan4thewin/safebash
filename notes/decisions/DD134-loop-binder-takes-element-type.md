title: DD#134 — A collection loop binder takes the element type
status: accepted

1. `for NAME in @COLL` binds each element as if by `NAME=${COLL[K]}`:
   iterating an `int array`/`int map` binds integers; a string collection binds strings.
2. Ordinary binding rules apply: a new binder takes the element type; an
   existing binder of the other type is a type error (SB4030).
3. Collection slots use the function's single inferred slot signature (DD#100).
4. Word lists, `$(keys …)`, readers, `argv` and `@args` still bind strings.

---

DG, 2026-11-13, on the tour's `total` example: **"yes to loop var takes
type"**.

## Motivation

DD#76 fixes an int collection's element type, and DD#123 makes a cell behave
like a variable. A direct read already supplies an integer:
`$(( a[0] + 1 ))` and `v=${a[0]}` (which makes `v` an `int`) both work.
Iterating the same collection did not: the binder was always a string, so
every integer loop owed a disposition it had no reason to need:

    int array prices=(3 4 5)
    for v in @prices { (( sum += ${v:-0} )) }   # SB4030 without :-0

Under this rule the loop is `(( sum += v ))`.

## The rule

The binder is an assignment from the current cell, so it follows the same
type rules as `NAME=${COLL[K]}`. The binder is not a special case.

- **Fresh binder.** It is introduced with the element type and keeps that type
  after the loop (loop bindings survive; DD#86).
- **Existing binder of the other type.** It is SB4030, the same as
  `str s; s=${a[0]}` today. There is no conversion spelling in a loop header;
  quote the value inside the body or use a differently named binder.
- **Inside a function.** `@A`…`@E` have one inferred element type per function
  (DD#100), so the binder's type is fixed for the whole body.
- **Unchanged.** String collections, word lists, `$(keys …)` (keys are
  strings), reader loops, `argv` and `@args` bind strings. Named-collection
  iteration is already a sole bare splat, so a header cannot mix element
  types. Taint propagation from the source is unchanged.

## Consequences

String-only uses of an integer binder become type errors, as for any `int`.
For example, `${v:0:1}` and the string intrinsics are rejected (DD#105), and
assigning it to a `str` is rejected. Write `"$v"` to convert. Rendering
`$v` in output is unaffected. `${v:-0}` remains legal on an `int` (DD#123),
so existing arithmetic loops keep compiling.

Implementation: [loop-binder-element-type](../tasks/loop-binder-element-type.md).
