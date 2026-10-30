---
title: "bug: an int call is chosen by name, so a redefinition miscompiles it"
status: open
priority: P1
kind: bug
depends: []
gates:
  - "i1 below prints `x=[one]`, as it does without the later `-> int` definition (today: `one` on the real stdout, then `x=[0]`)"
  - "i2's second capture gives `x=[one]` (today: `one` on the real stdout, then `x=[0]`)"
  - "i3 behaves as it does without the later definition (today it runs the void f: `one` on stdout, then `n=0`). Without it, `$(( f ))` reads an unassigned variable and prints only `n=0`, which is itself `bug-function-name-read-as-variable`; i3 follows that task's ruling"
  - "the same in both backends, pinned by specs in both modes"
---

A silent miscompile in a program the compiler accepts. Found 2026-10-30 in
F6 unit A (journal 547). The binary before F6 gives the same output, so it is
pre-existing.

```sh
# i1
fn f { echo one }
x=$(f)
echo "x=[$x]"
fn f -> int { return 3 }
echo after

# i2
fn f -> int { return 3 }
x=$(f)
echo "x=[$x]"       # x=[3], correct
fn f { echo one }
x=$(f)
echo "x=[$x]"

# i3
fn f { echo one }
int n=$(( f ))
echo "n=$n"
fn f -> int { return 3 }
```

## Cause

`register_functions` (`transform/entry.rs`) adds the name of every `-> int`
definition to `function_int_typed`, whichever definition a call will run.
Every site that asks whether a call is an int call asks by that name:
- `func_call_capture` and `var_to_ssa` (`expr_handlers.rs`) choose
  `IntFuncCall` over `FuncCall`. That is the miscompile in i1-i3.
- `capture_args_run_value_calls` (`expr_handlers.rs`) and the two callers of
  `capture_body_is_pure`, `capture_func_into` and `capture_body_into`
  (`stmt_handlers.rs`), use the same table. They classify a capture's path
  by a definition the call may not run.

The body they inline, however, is the definition in force
(`function_bodies`, filled by `lower_function_stmt`). A call of a void
definition therefore lowers as an int call: the body's output goes to the
real stdout and the "int" is 0.

F6 unit A fixed the same shape for DD#55's class check. A call now checks
the class of the definition in force (`VersionState::call_class`), and only
the `fn_NAME` templates use a table, which holds only names defined once.
The likely fix mirrors that: decide the int lowering from the class in force
(`FnClass::Int`).

F6 did not touch these functions, so by the 2026-10-26 rule this goes to the
backlog. A DG ruling in `q-nonvoid-classifier-residuals` Q3 that
rejects a redefinition which changes a function's class would also make
these programs illegal. The remaining sites would then become an ICE guard.
