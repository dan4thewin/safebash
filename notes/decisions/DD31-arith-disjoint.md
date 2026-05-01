title: DD#31 — Arithmetic mutations and comparisons are disjoint
status: implemented

Statement `(( ))` — only mutations: `=`, `+=`, `-=`, `++`, `--`. Exit status always 0.
Condition `if (( ))` — only comparisons: `==`, `!=`, `<`, `>`. Produces boolean.
`(( x > 0 ))` as bare statement → compile error. `if (( x++ ))` → compile error.
`$((x=10))` does NOT update `x` — this is intentional, not a bug.
