title: DD#42 — getenv and argv as builtins
status: implemented

`$(getenv VAR)` reads an environment variable. `$(argv N)` reads
positional parameter N. `$(argc)` returns the count of positional
params (equivalent to `$#`).

These are the explicit-origin forms for external data. Environment
variables accessed via getenv are tainted (auto-blessed if in
known_externals). Positional params via argv are tainted.

Codegen fully supports all three forms. Return status handles
missing values (out-of-range argv returns empty string, sets `$?` to 1).
