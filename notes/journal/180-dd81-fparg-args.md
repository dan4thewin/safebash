# 180 — DD#81: `$(f arg)` passes args to the captured function

Third DD#81 stage — a reachable ICE. `s=$(f hello)` (or `echo $(f x)`) on a
defined function that reads its args crashed with `ICE: undefined array`.

## Root cause

Two different arg-setup models. The **plain-call** path
(`inline_function_call`) uses the DD#75 args-array model —
`save_caller_args` + `setup_callee_args` (builds the `__args` array,
sets `current_args_name`). The **capture** path (`capture_func_into`,
used by both `s=$(f)` and the `echo $(f)` lift) used the *old* positional
model — `save_positionals` + `setup_func_args` (individual `$1..$9` vars) —
**and was passed `&[]`** (the args were dropped at `func_call_capture`).

So a captured function body reading `$1` (which desugars to `${args[0]}`,
DD#75) found no `args` array → ICE.

## Fix

- `SsaValue::FuncCall(String)` → `FuncCall { name, args: Vec<SsaValue> }` —
  carries its args like `IntFuncCall`. `func_call_capture` now fills them for
  the string case too (it already did for int).
- `capture_func_into` takes `arg_values: &[SsaValue]` and uses the **same**
  args-array model as `inline_function_call` (`save_caller_args` +
  `setup_callee_args` + `restore_caller_args`, plus a small
  `restore_saved_locals`). The old `save_positionals` / `setup_func_args` /
  `restore_positionals` are now dead → deleted.
- The lift arm threads the carried args (lifting each, in case an arg is itself
  a call); the assignment path (`try_capture_inline`) converts its extracted
  Exprs to SsaValues.

## Verified

`s=$(f hello)` reads `$1` → `got:hello`; `echo $(f world)`; multi-arg
`$(greet a b)`; the `${args[i]}` form; a var arg; and the no-arg capture all
compile+run. specs functions.sh 81.7/81.8 (native,c — janet has no function
capture). Full sweep vs base `f9f16f5`: **0 regressions**, native gate 0, 4 new
passing.

## Next (DD#81)

Part B: uniform arity — the capture path now carries the arg count, so
`check_callee_arity` can run on `$( )` calls too, and the fixed/variadic
signature (SB5042/43/44) from `args` usage. Then the `bug: bare command` →
`command not found` message.
