title: DD#42 — getenv and argv as builtins
status: implemented

`$(getenv VAR)` reads an environment variable. `$(argv N)` reads
positional parameter N (1-based). `$(argc)` returns the count of
positional params (equivalent to `$#`).

These are the explicit-origin forms for external data. Environment
variables accessed via getenv are tainted (auto-blessed if in
known_externals). Positional params via argv are tainted.

All three always refer to the **program's** arguments, never a
function's. Functions receive their args via `$1..$N`.

Out-of-range `$(argv N)` aborts with a clear error message, consistent
with integer overflow behavior (DD#26). There is no silent empty return.

Codegen calls `sash_argv_get` in the runtime, which does bounds
checking and panics on violation.

## See also

- DD#24 — Taint quarantine and bless (getenv results are tainted)
- DD#26 — Integer overflow traps (same abort philosophy)
- DD#53 — Env var isolation (getenv is the only way to read env vars)
