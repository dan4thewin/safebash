title: DD#36 — Fixed-point arithmetic via dtoi/itod
status: implemented

`dtoi(digits, string) → int` and `itod(digits, int) → string`.
Explicit scale, no silent truncation. Not a float type.
Runtime builtins alongside atoi/itoa.
