title: DD#30 — Strict Type Separation
status: implemented

No implicit conversion between int and string. Conversion happens
explicitly at boundaries: `$(( ))` for string→int, `"$var"` for
int→string (at output).

## Bare numeric assignment

`x=5` is a string assignment (equivalent to `x="5"`). The absence of
`int` keyword means string — there is no ambiguity in the grammar.
Use `int x=5` for integer variables.
