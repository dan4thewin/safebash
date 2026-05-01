title: DD#29 — Coerce in Lowerer, Not Separate Tool
status: implemented

One tool, one set of rules. The lowerer records coercions during CST walk.
`--emit-source` shows coerced source. `--annotate` adds `# was: ...` comments.

Coercions: `declare -i` → `int`, `declare -a` → `array`, `declare -A` → `map`,
`declare -r`/`readonly` → `const`, backtick → `$(...)`, `[ ]` → `[[ ]]`,
`${var-x}` → `${var:-x}`, `let` → `(( ))`, `typeset` → bare assignment.
