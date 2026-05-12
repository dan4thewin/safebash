title: Type-check arithmetic operands
status: done
depends: []
gates: [30.1, 30.2, 30.3, 30.4]
  - tests
  - commit

The type checker does not verify that variables used in `(( ))` and
`$(( ))` are `int`-typed. A string variable like `x="5"` can be used
in arithmetic without error today.

Add a check: all variable operands inside arithmetic contexts must have
`VarType::Int`. String variables produce a type error with a hint to
use `int` declaration or explicit `$(( ))` conversion.

This closes the gap in DD#30 strict type separation — without this
check, the boundary between string and int is not enforced at
arithmetic use sites.

Ref: DD#30, type_check.rs
