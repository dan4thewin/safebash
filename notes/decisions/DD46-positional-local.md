title: DD#46 — Positional parameters are function-local only
status: implemented

1. `$1` through `$N` and `$#` are function-local sugar for that function's
   `args` cells and count.
2. Program arguments use the explicit `arg`/`argv`/`argc` interface (DD60);
   function splatting and joining use `@args` and `join args`.
3. Every function call must satisfy the signature inferred from its argument
   accesses (DD81/DD84).

---

Sash-native: `$1..$N` and `$#` are only valid inside function bodies.
`$@` / `$*` are **not part of the language at all** (see the 2026-08-26
amendment). Top-level references → compile error directing to the DD#60
builtin for that form:
  - `$@` / `$*` → **rejected everywhere** (SB5045) — the user-arg splat is
    `$(argv 1)` (`$(argv)` includes the program name at index 0), one arg is
    `$(arg N)`
  - `$1`..`$N` → use `$(arg N)`   (SB5024, genuinely function-local)
  - `$#`       → use `$(argc)` (C argc — includes the program name;
    subtract 1 for the user-arg count)
  - `$0`       → use `$(arg 0)`
Bash mode: top-level positionals coerced to the builtin forms by
sash-convert.
Reading `$K` where K > call argc → compile error, not silent empty.

**Amendment (2026-08-26, DG — "Option A"):** `$@` and `$*` — including the
braced spellings `${@}`/`${*}` (and any modifier form `${@:-x}`), and inside
`$(( ))` — are **universally rejected** (SB5045 "sash has no `$@`/`$*`"), not
merely function-local. They are `$`-sigil forms that splat/join, which DD#75
§4 assigns solely to `@`. `validate::check_dollar_at` fires everywhere with a
context-sensitive hint: in a function → `@args` (splat) / `$(join args)`
(joined); at top level → `$(argv 1)` (user-arg splat) / `$(arg N)` (one).
Only `$1..$N` and `$#` remain genuinely function-local (SB5024 at top level);
they are scalar, so their `$` sigil is consistent. (This supersedes the
earlier state where top-level `$@`/`$*` was SB5024 and in-function `$*` /
braced `${@}`/`${*}` silently compiled.)

See [DD#60](DD60-argv-trio.md) for the argv/arg/argc trio.
