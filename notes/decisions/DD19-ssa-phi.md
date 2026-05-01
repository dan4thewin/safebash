title: DD#19 — Runtime Function Selection via SSA Phi
status: rejected

Conditional function definition (`if ... then f() { A } else f() { B }`)
compiled as two concrete functions with phi-based dispatch. YAGNI —
use `const int flag` + branch instead.
