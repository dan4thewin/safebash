title: DD#30 — reject numeric comparisons in [[ ]], coerce to (( ))
status: ready
severity: correctness
gates: [30.5a, 30.5b, 30.5c, 30.6a, 30.6b]
---

`[[ ]]` is string/file/boolean only. `(( ))` is arithmetic only.
No operator set overlap (journal 042, DD#30).

Converter: best-effort `[[ $x -gt N ]]` → `(( x > N ))`.
Compiler: `-gt -lt -ge -le -eq -ne` in `[[ ]]` → compile error.
