title: Give collection loop binders the element type (DD#134)
status: open
priority: P3
kind: work
depends: []
gates:
  - "for NAME in @COLL over an int array/map binds an int; (( s += v )) compiles and runs in both backends"
  - "an existing str binder over an int collection (and an int binder over a string collection) is SB4030, matching NAME=${COLL[K]}"
  - "collection slots @A..@E take the function's inferred element type, including forwarded slots"
  - "word lists, $(keys …), readers, argv and @args still bind strings"
  - "after the loop the binder retains its type; taint follows the source as before"
  - "a negative control is red on the pre-change binary, and a reversion stub turns the new specs red"
  - "docs/tour slide 'Passing collections' drops ${v:-0} for (( sum += v ))"

# Give collection loop binders the element type

[DD#134](../decisions/DD134-loop-binder-takes-element-type.md): iterating an
int collection binds integers, following ordinary assignment from a cell.

Today, `int array a=(1 2); for v in @a { (( s += v )) }` raises SB4030 ("bare
string `v` in arithmetic"), both at top level and through `@A`. A direct cell
read already types correctly, so the work is in the for-in binder's typing:
the type checker (`CheckedTypes`) and the SSA lowering of the binder. The
checker must give the binder the element type rather than inferring it again.

Existing specs that iterate int collections (`arithmetic.sh` around the
`for x in @a` null-pointer regression, `collparam.sh` 333) should still pass.
Echoing, `"$v"` and `${v:-0}` remain legal on an `int`.
