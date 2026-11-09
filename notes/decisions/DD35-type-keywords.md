title: DD#35 — Type keywords and declaration axes
status: implemented

Three orthogonal axes: mutability (`const`), type (`int`, `str`),
structure (`array`, `map`). Freely stackable: `const int x=5`.
`const` applies to scalars only.
Coercions: `declare -i` → `int`, `declare -a` → `array`, etc.

Immutability constrains value writes, not validation (DD#86, DG
2026-11-09). A const may hold tainted input and be attested without
changing its value; it is not implicitly clean. Const-attest enforcement
is pending in `q-attest-retag-of-const`; the declaration axes are unchanged.

**Amended (DD#86, 2026-09-02):** the *lifetime* axis (`local`) is
retired. `local` is removed from the language — scope is structural
(a name first assigned in a function is auto-local; a top-level
declaration is a global), so lifetime is positional, not a keyword.
`str` was added to the type axis (a string declared with meaningful-
zero `""`, parallel to `int`).
