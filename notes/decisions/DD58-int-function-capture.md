title: DD#58 — Integer Function Capture Syntax
status: implemented

`-> int` functions return an integer via `return N`. Their return
value is captured in arithmetic context via s-expression call syntax:

    fn f -> int { return 42; }
    fn add -> int { return $(( $1 + $2 )); }

    int x=$((f))                      # zero-arg capture
    int y=$(( (add 3 4) + 1 ))        # call with args — sexpr
    ((x = (add x 5)))                 # reassign with expression
    if (( (add x y) > 10 )); then     # boolean context
    int z=$(( (add (mul 2 3) 4) ))    # nested calls

`$(f)` for a `-> int` function is a type error — `$(...)` captures
stdout, but `-> int` functions produce an integer return value, not
stdout output.

Int→string conversion follows DD#30: `echo "$x"` at output.

## S-expression call syntax

Inside `$(( ))` and `(( ))`, `(name args...)` where `name` is a
known `-> int` function is a function call. `(expr)` where the first
token is not a known function is arithmetic grouping (unchanged).

The compiler has the function table from the first pass, so
disambiguation is a compile-time decision.

## Name collision

Function names and variable names must not collide. Each may have
local shadows (a local can shadow an outer local or global of the
same kind), but `fn f` and `var f` coexisting is a compile error.
This eliminates ambiguity inside `(( ))` where `f` could be either.

## Interaction with existing syntax

- `int x` remains the declaration keyword (DD#16). `int` is only
  dropped as a keyword for function return types if `-> int` is
  used instead.
- `(( ))` is the arithmetic statement form for reassignment and
  boolean tests. It is NOT a declaration form — `int x` is.
- `$(( ))` is the arithmetic expression form that produces a value.
- DD#17 is narrowed: `$(func)` is capture-call for string functions
  only. `-> int` functions use `$(( ))` or `(( ))`.

## Non-void enforcement (DD#55 gates 55.13, 55.16)

`-> int` functions are non-void by definition. A bare call `f` in
statement position (outside `(( ))`, `if`, `while`, `&&`, `||`) is
a compile error [SB4016] — the return value would be silently
discarded.
