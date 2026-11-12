title: DD#19 — Runtime Function Selection via SSA Phi
status: rejected

1. Select behavior through ordinary branching; each function name identifies
   one statically defined body (DD86).

---

~~Conditional function definition (`if ... then f() { A } else f() { B }`)
compiled as two concrete functions with phi-based dispatch.~~ YAGNI —
use `const int flag` + branch instead.
