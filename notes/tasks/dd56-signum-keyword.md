title: DD#56 — implement signum keyword
status: done
depends: []
severity: feature
gates:
  - 56.3
  - tests
  - commit

Add `signum` as a compiler-managed keyword (same category as
`argv`, `getenv`, `RANDOM`, `EPOCHSECONDS`).

### Runtime

Add `volatile sig_atomic_t sash_signum = 0;` global to sash-rt.
Export as `sash_signum` data symbol.

### Codegen

Declare `sash_signum` as imported data. `$(signum)` reads load
the global and itoa to string (same pattern as `$?` → exit_status_var,
but from a global rather than a local).

For `(( signum = 0 ))` (reset): store 0 to the global.

### Transform

Intercept `signum` in `expr_to_value` — similar to EPOCHSECONDS/RANDOM
but simpler: just load the global, no runtime function call needed.

### Lowerer

Recognize `signum` as a keyword. Reject `local signum`, `const signum`,
etc. — it's compiler-managed.
