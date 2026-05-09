title: "Session: int function capture syntax and DD#55 non-void"
date: 2025-05-09

## Context

Working on DD#55 non-void function detection (gates 55.12–55.14).
Implemented SB4016 (bare call of `-> int` function is error) and
SB4017 (return-0-only `-> int` function is error). Then hit 55.12:

    fn f -> int { return 42; }
    echo $(f)

Expected output: `42`. But this raises fundamental questions about
what `$(f)` means for a `-> int` function.

## The `$(f)` problem

In bash, `$(f)` captures stdout. A function with only `return 42`
produces no stdout — the return value goes to `$?`. So `echo $(f)`
would print an empty line in real bash.

The spec test was asserting `42`, which implies `$(f)` captures the
return value, not stdout. This contradicts DD#30 (no implicit
int→string conversion) and DD#17 (which defines `$(f)` as capture
for any function).

## Exploration: where should int captures happen?

Three options explored:

### Option A: `$(f)` captures int as string
`$(f)` becomes dual-purpose — stdout for string functions, return
value for `-> int` functions. Simple syntax but violates DD#30 and
conflates two different mechanisms (stdout pipe vs return value)
under one syntax.

### Option B: `$((f))` is the int capture
`-> int` function calls inside `$(( ))` or `(( ))`. This is already
the arithmetic domain. Composable: `$((f + g * 2))` just works.
Type boundary is explicit. `$(f)` for `-> int` becomes a type error.

### Option C: bare call + `$?`
`f; int x=$?`. Minimal syntax but `$?` is overloaded and interacts
badly with DD#55's exit check scoping.

**Chose Option B.** `$((f))` stays in the arithmetic domain, type
boundaries are explicit, composition with expressions is natural.

## The `(( ))` vs `int x` question

This opened a follow-on: if `((x = f))` assigns an int, do we need
`int x` as a declaration keyword? Explored three positions:

### Kill `(( ))`, keep `int x`
`$((expr))` already produces int values. `(( ))` as a statement is
redundant — everything it does is covered by `x=$((expr))`. Simpler
grammar. But then `-> int` function calls in non-expression context
(assignments, boolean tests) need the `$((f))` wrapper everywhere.

### Kill `int x`, keep `(( ))`
`((x=5))` as both declaration and assignment, symmetric with `y=hi`.
Type inferred from context. One fewer keyword. But: `const`, `local`,
and array declarations all compose badly with `(( ))` syntax.
`const ((x=5))`, `local ((x=5))`, no obvious array form. Readability
at scale suffers — can't grep for int declarations.

### Keep both (asymmetric)
`int x=5` for declaration, `((x=expr))` for reassignment and boolean
tests. Asymmetric with string's `y=hi`, but the asymmetry is honest:
strings are bash's default, ints are opt-in exceptions that deserve
marking. And `int` fits the existing keyword family: `local`, `const`,
`readonly`, `int`.

**What was persuasive:** The `const`/`local`/array interaction killed
the "drop `int`" option — `local int x=5` is clean, `local ((x=5))`
is awkward. The asymmetry argument almost tipped toward dropping
`int`, but the composability with other declaration keywords was
decisive.

Then considered whether `set -u` enforcement (already baked in) makes
explicit `int` unnecessary since typos are caught anyway. True, but
`int` isn't primarily about typos — it's about type visibility.
Grepping for `int ` finds all integer declarations. No equivalent
with `(( ))` implicit declaration.

## Resolved design (DD#58)

- `int x` stays as declaration keyword
- `(( ))` stays as statement form (reassignment, boolean, function call)
- `$(( ))` stays as expression form (produces int value)
- `$(f)` for `-> int` function is a type error
- `-> int` functions are non-void by definition (no return scanning needed)
- `int x=$((f))` — declare + capture
- `((x = f + 1))` — reassign with expression
- `if ((f > 0))` — boolean context

## Also this session

- Implemented SB4016/SB4017 in transform.rs via `collect_returns`
  helper, `function_non_void` set, and `in_condition` depth counter
- Fixed codegen diagnostic format (main.rs `"{}:{}"` vs `"{}: {}"`)
  so transform errors match run_sash grep filter
- Fixed spec tests 55.7/55.8 (taint is orthogonal to exit checks)
- Significant shim/tooling lessons: annotations corrupt edit tool,
  DB + ed is the correct workflow, shims are nudges toward DB usage
