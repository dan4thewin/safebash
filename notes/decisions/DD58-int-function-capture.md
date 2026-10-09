title: DD#58 — Integer Function Capture Syntax
status: implemented

> **Amendment — Option C (2026-08-15, commit `a8ac707`).** The original
> rule below — "`$(f)` for a `-> int` function is a type error" — was
> **superseded**. `$(...)` is now **return-type-polymorphic**: it captures
> the callee's declared result — the integer `return` value for a `-> int`
> function, or captured stdout (a string) for a string/void function. So
> `int n=$(f)`, `echo $(f)`, and `"port=$(f)"` all work. Int producers work
> in **both** the `$(f)` capture **and** the `$(( (f) … ))` sexpr, and this
> applies **uniformly** to user `-> int` functions and to int intrinsics
> (`len`, `argc`, `getpid`, `getppid`, `getcpid`, `epochseconds`, `random`,
> `signum`, `status`). The inline text below is corrected accordingly.
> Rationale: this-session decision, journal 052/145 (DG: "len shouldn't be
> different from any other integer intrinsic"). A string producer used
> inside `$(( ))` — the sexpr expects an int producer — is a type error
> (capture it first with `$()`); that is DD#30, not a `$()` restriction.

> **Amendment — the callee denotes a value, or it is rejected (2026-10-09,
> journal 482).** The amendment above names two classes, int producers and
> string producers, and the implementation followed it literally: a
> hand-written deny-list of string intrinsics, with everything else admitted
> as an int producer. There is a **third** class — a name that denotes no
> value at all (`echo`, `cd`, `read`, `break`, `argv`, …) — and nineteen of
> them were admitted and lowered as a read of an unset variable, a silent
> `0`. The rule is now stated over the whole set: **a sexpr callee must be
> an int producer; every other compiler-owned name is a type error**, with
> a distinct message for a statement (nothing to capture) and for a string
> producer (capture it first). The classification lives in
> `resolve::ArithDenotation` and is total over `BuiltinCmd`, so a new
> builtin cannot be added without one.

`-> int` functions return an integer via `return N`. Their return
value is captured in arithmetic context via s-expression call syntax:

    fn f -> int { return 42; }
    fn add -> int { return $(( $1 + $2 )); }

    int x=$((f))                      # zero-arg capture
    int y=$(( (add 3 4) + 1 ))        # call with args — sexpr
    ((x = (add x 5)))                 # reassign with expression
    if (( (add x y) > 10 )); then     # boolean context
    int z=$(( (add (mul 2 3) 4) ))    # nested calls

`$(f)` for a `-> int` function captures its **return value** — see the
Option C amendment at the top (commit `a8ac707`, supersedes the original
"type error" stance). `$(...)` is return-type-polymorphic: it yields the
callee's declared result — the integer `return` for a `-> int` function,
or captured stdout (a string) for a string/void function. The
`$(( (f) … ))` sexpr stays available for using an int producer inside an
arithmetic expression; both surfaces work for every int producer.

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
- DD#17/Option C: `$(func)` is a capture-call whose result **type is the
  callee's return type** — a string for string functions, the integer
  `return` for `-> int` functions. A `-> int` function may ALSO be used
  in `$(( ))` / `(( ))` (arithmetic composition). (DD#58 originally
  narrowed this to "string functions only"; Option C, `a8ac707`, widened
  it back — see the amendment at the top.)

## Non-void enforcement (DD#55 gates 55.13, 55.16)

`-> int` functions are non-void by definition. A bare call `f` in
statement position (outside `(( ))`, `if`, `while`, `&&`, `||`) is
a compile error [SB4016] — the return value would be silently
discarded.
