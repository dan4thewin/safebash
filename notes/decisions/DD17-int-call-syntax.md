title: DD#17 — Function Call Syntax
status: implemented

Two forms:

- `$(func args)` — capture call. Returns a value (string or int).
  Universal for any function whose result is needed.
- `func args` — bare call. Side-effect only, no capture. Used when
  the function's purpose is mutation (globals, output, map population).
