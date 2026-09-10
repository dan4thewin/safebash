title: DD#17 — Function Call Syntax
status: implemented
superseded-by: DD#58 (for -> int functions)

> **Amendment — Option C (2026-08-15, commit `a8ac707`).** The rule below —
> "`$(func)` for a `-> int` function is a type error" — was **superseded**.
> `$(...)` is **return-type-polymorphic**: it captures the callee's declared
> result — the integer `return` for a `-> int` function, or captured stdout
> (a string) for a string function. `int n=$(get_port)` / `echo $(get_port)`
> work, mirroring how `$(len a)` captures an int intrinsic. `-> int`
> functions may ALSO be used in `$(( ))` / `(( ))`. Corrected inline below.

Three forms:

- `$(func args)` — capture call. The captured value's **type is the
  callee's return type**: a string (stdout) for a string function, the
  integer `return` for a `-> int` function (Option C). 
- `func args` — bare call. Side-effect only, no capture. For string
  functions whose purpose is mutation (globals, output, map population).
  For `-> int` functions, bare call is a compile error [SB4016] — the
  return value would be silently discarded.
- `$((func))` / `((x = func))` — use a `-> int` function's return
  value in arithmetic context (DD#58). Available in addition to the
  `$(func)` capture above.

`$(func)` for a `-> int` function captures its integer return value
(Option C, `a8ac707`) — see the amendment at the top. (Originally this
was defined as a type error; that stance was superseded.)
