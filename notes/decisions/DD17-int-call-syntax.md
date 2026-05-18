title: DD#17 — Function Call Syntax
status: implemented
superseded-by: DD#58 (for -> int functions)

Three forms:

- `$(func args)` — capture call for **string** functions only.
  Captures stdout as a string value.
- `func args` — bare call. Side-effect only, no capture. For string
  functions whose purpose is mutation (globals, output, map population).
  For `-> int` functions, bare call is a compile error [SB4016] — the
  return value would be silently discarded.
- `$((func))` / `((x = func))` — capture/use `-> int` function return
  value in arithmetic context (DD#58).

`$(func)` for a `-> int` function is a type error — `$(...)` captures
stdout, but `-> int` functions produce an integer return value, not
stdout output.
