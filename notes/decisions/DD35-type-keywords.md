title: DD#35 — Type keywords and declaration axes
status: implemented

Four orthogonal axes: mutability (`const`), lifetime (`local`),
type (`int`), structure (`array`, `map`). Freely stackable:
`local const int x=5`. `const` applies to scalars only.
Coercions: `declare -i` → `int`, `declare -a` → `array`, etc.
