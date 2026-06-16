title: DD#46 — Positional parameters are function-local only
status: implemented

Sash-native: `$1..$N`, `$@`, `$*`, `$#` only valid inside function bodies.
Top-level references → compile error directing to the DD#60 builtin for
that form:
  - `$@` / `$*` → use `$(argv)` (iteration) or `$(arg N)` (one arg)
  - `$1`..`$N` → use `$(arg N)`
  - `$#`       → use `$(argc)` (C argc — includes the program name;
    subtract 1 for the user-arg count)
  - `$0`       → use `$(arg 0)`
Bash mode: top-level positionals coerced to the builtin forms by
sash-convert.
Reading `$K` where K > call argc → compile error, not silent empty.

See [DD#60](DD60-argv-trio.md) for the argv/arg/argc trio.
