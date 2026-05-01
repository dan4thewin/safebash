# Experiment 017: Janet oracle

**Goal:** Bundle experiments 025–030 into one: emit Janet code from
the IR, run it as a subprocess, compare output against Bash.

**Architecture:**
```
src/
  emit_janet.rs  — IR → Janet source code emitter
  oracle.rs      — Janet/Bash subprocess runner
```

**Mapping (Bash → Janet):**
- `x=value` → `(var x "value")` / `(set x "value")`
- `readonly X=v` → `(def X "v")`
- `echo $x` → `(print x)`
- `echo "a $b"` → `(print (string "a " b))`
- `cmd arg` → `(os/execute ["cmd" "arg"] :p)`
- `cmd1 | cmd2` → `os/spawn` pipe chain with `:out :pipe`
- `if [[ cond ]]; then` → `(if cond (do ...) (do ...))`
- `for x in a b; do` → `(each x ["a" "b"] ...)`
- `func() { body }` → `(defn func [] body)` + call via `(func)`
- `[[ -f x ]]` → `(= (os/stat x :mode) :file)`
- `[[ a = b ]]` → `(= a b)`
- `[[ n -gt 0 ]]` → `(> (scan-number n) (scan-number 0))`
- `$(cmd)` → `os/spawn` with stdout capture
- `$((n+1))` → `(string (+ (scan-number n) 1))`

**Bugs found and fixed:**

1. **Double-quoted string gap detection.** `"$x $y"` — tree-sitter
   includes the inter-variable space inside the second expansion
   node's `$` token (shown as `" $"` not `"$"`). The original
   lower_string only collected named children, missing this implicit
   whitespace. Fixed by detecting leading whitespace in expansion
   tokens within strings and emitting it as a literal part.

2. **Janet variable scoping in if/else.** Janet's `var` creates a
   local binding. Variables first assigned inside an if/else branch
   aren't visible after the `if`. Fixed by pre-scanning branches for
   assigned variables and emitting `(var x nil)` before the `if`.

3. **Pipeline subprocess ordering.** Reading from the last pipe
   before upstream processes finish → `EAGAIN`. Fixed by waiting
   for upstream processes before reading the final output.

4. **Function calls vs external commands.** `greet` after
   `greet() { ... }` was emitted as `os/execute ["greet"]`.
   Fixed by tracking function definitions in EmitCtx and emitting
   Janet function calls for known functions.

**Tests:** 28 oracle tests covering:
- Janet subprocess (runs, exit code)
- Assignment (literal, reassignment, readonly → def)
- Commands (echo literal, multiple args, quoted, variable,
  interpolation, general command)
- If/else (true path, false path, string equality, branch assignment)
- Pipeline (echo|cat, echo|grep)
- For loop
- Function definition and call
- Integration (assign+echo, conditional output)
- Janet code validity (5 scripts)
- Edge cases (empty echo, multiple echo, comment)

**Conclusion:** Phase 5 (Janet oracle) complete. The full pipeline
is now: Bash source → CST → IR → Janet → execute → compare output.
The oracle confirms semantic correctness of the IR transformation
for the tested constructs.

---

