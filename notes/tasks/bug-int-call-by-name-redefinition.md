---
title: "bug: an int call is chosen by name, so a redefinition miscompiles it"
status: open
priority: P1
kind: bug
depends: [reject-duplicate-function-definitions]
gates:
  - "i1-i3 below reject at the duplicate source definition under DD86, in both modes"
  - "the duplicate refusal, not an unrelated result/type error, is pinned by focused specs and negative/reversion controls"
  - "valid single-definition captures and integer calls retain their results and output routing"
---

## Decision, 2026-11-06

DG rejects all duplicate function definitions, including identical and
same-kind repeats (DD86/DD55, journal 582). The new P1
`reject-duplicate-function-definitions` owns enforcement and directly
dependent bookkeeping. This bug remains open until the duplicate programs
below are rejected; the policy recording is not a fix.

The original "make the selected definition run correctly" gates are
superseded. i3 no longer depends on a diagnostic choice for a single
function name read as a variable: it has a duplicate and must reject for
that reason. The independently invalid single-definition counterpart
remains in `bug-function-name-read-as-variable`.

## Historical reproducers

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
The original proposed repair mirrored that: decide the int lowering from
the class in force (`FnClass::Int`). That is no longer the target contract;
accepted programs must have only one definition to classify.

F6 did not touch these functions, so by the 2026-10-26 rule this goes to the
backlog. `q-nonvoid-classifier-residuals` Q3 is now decided. Enforce the
unique-definition invariant before these consumers, then simplify only
the paths proved obsolete by that guard; do not add a separate duplicate
check at each use.
