# 181 — DD#81 Part B: exact arity from `args` usage

Final DD#81 stage. Functions now have an inferred call signature, checked
exactly at every call site.

## The analysis (`transform/functions.rs::analyze_arity`)

Walks the body's `args` reads into an `ArgSig { indices, variadic }`:

- literal index → a **fixed** index: `$N` (→ N-1), `${args[k]}`, bare `args[k]`
  in arith.
- whole-args → **variadic**: `@args` (arrvals `ParameterExpansion`),
  `$(len|join|keys args)`, `${#args}` (arrlen), `$#`, `$@`, or a **dynamic**
  index `${args[$i]}`.

→ `Arity::Fixed(max+1)` (no whole-args) or `Arity::Variadic { floor: max+1 }`.

**Arithmetic is scanned** (`sig_scan_expr` → `ExprKind::Arithmetic`,
`sig_scan_stmt` → `StmtKind::ArithTest`, both → `sig_scan_arith`). Without this
a `$#`/`$N` inside `$(( ))` or a `(( … ))` condition is invisible and a variadic
function looks fixed — caught by av.h1/av.h3/25.8/31.24 during the sweep.

## Checking

`check_callee_arity` (rewritten) runs on all three call paths — plain
(`inline_function_call`), pipeline (`build_function_pipeline_stage`), and now
the `$( )` capture (`capture_func_into`, which carries the arg count since
50474e9):

- **Fixed(n)** → exactly n: too few **SB5042**, too many **SB5043**.
- **Variadic{floor}** → ≥ floor: too few SB5042.
- A fixed signature must read `{0..max}` contiguously — a gap is **SB5044**
  (rule (a), checked at the definition).

## The hole-rule tension + resolution (DG)

Rule (a) initially broke three existing specs: 67.2 (`echo $2` ignoring `$1`),
75.20 (`${args[0]} ${args[2]}` skipping the middle), 75.22 (`${args[10]}` — the
documented "args beyond 9 are reachable" feature). I surfaced this — rule (a)
made high-index `${args[N]}` unusable.

DG's resolution: **keep rule (a); acknowledge an unused param with a no-op**.
`fn f { : $1; echo $2; }` reads index 0 (no hole). For a genuinely variable
count or a high index without the lower ones, mark the function variadic with
`@args`: `fn f { : @args; echo ${args[10]}; }`. The three specs were updated to
the idiom (67.2/75.20 no-op; 75.22 `@args`).

## Cleanup

The old `$N`-only min-arity scanner (`collect_max_positional` +
`scan_expr_positionals_*` / `record_positional` / `scan_stmts` / `shift_amount`
/ `scan_test_positionals`, ~130 lines in helpers.rs) is now dead → removed.

## Results

specs functions.sh 81.9–81.13 (too-few, too-many, hole, variadic `@args`,
capture-path arity). Full `SPEC_MODES="native c janet"` sweep: **0 regressions**,
native gate 0, 14 new passing. DD#81 is complete (Parts A + B).
