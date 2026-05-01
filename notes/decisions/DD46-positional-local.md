title: DD#46 — Positional parameters are function-local only
status: implemented

Sash-native: `$1..$N`, `$@`, `$*`, `$#` only valid inside function bodies.
Top-level references → compile error with guidance to use `argv N`.
Bash mode: top-level positionals coerced to `$(argv N)`.
Reading `$K` where K > call argc → compile error, not silent empty.
