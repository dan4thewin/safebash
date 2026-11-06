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

## Integer capture has one result channel (DG, 2026-11-06)

A direct capture of an integer function consumes only its returned
integer. Attached redirects do not turn it into a stdout capture or change
its result kind. This preserves Option C, including the uniform treatment
of integer functions and intrinsics; it does not restore arithmetic-only
integer calls or grant new redirect syntax to an intrinsic.

For an `-> int` function f, the required behavior is:

| Form | Meaning |
|---|---|
| `$(f)` | Capture f's returned integer |
| `$(f 2>&1)` | Capture that same integer; apply the redirect to f's I/O |
| `$(f >file)` | Capture that same integer; send f's stdout to the file |
| `$(- f)` or `$(- f 2>&1)` | SB4076: the capture already consumes the integer |

Quoting/interpolating one of these integer captures renders the integer
under DD#30; it never substitutes the function's stdout instead.
SB4075's connected-writer requirement concerns stdout captures (DD#88),
not integer-value captures: `$(f >file)` does not need a stdout writer
feeding a capture pipe.

Writing to stdout from an integer function remains legal. It is an
ordinary side effect, following the active output destination and any
attached redirects, not a second result selected by capture syntax.
No purity check or stdout prohibition is introduced.

To capture that output, use a non-integer wrapper that explicitly discards
the integer, for example `fn output_f { - f }` followed by `$(output_f)`.
The wrapper is void and its capture collects stdout. An anonymous
non-integer body follows the same body/result rules; this does not relax
the separate handling requirements of a body that returns a status.
Normal taint and failure rules still apply. The adapter discards one value
and exposes output; it is not an integer-to-string or integer-to-status cast.

**Accepted, implementation pending** in
`bug-int-capture-redirect-switches-kind` (journal 583). The old
redirect-dependent switch to stdout is not an exception to this contract.
DD#55's no-reach rule and DD130's failure classes remain: a failed setup
cannot manufacture an integer zero or turn an absent return into success.

## Integer return completeness

Under DD#55's return-kind ruling (DG, 2026-11-06), an `-> int` function
supplies an integer on every returning path. Bare return and fall-through
are not implicit integer zero. Compatible integer forwarding remains legal;
a self-tail call is a loop edge, and process termination is not a return.

Constant results, including zero, are legal. The int-only SB4017
"returns 0" check is retired, with no general constant-result check replacing
it. Implemented in journal 577 (`enforce-function-return-kinds` and
`retire-return-ceremony-check`), with native/C checks for integer zero,
missing results and compatible forwarding.

## S-expression call syntax

Inside `$(( ))` and `(( ))`, `(name args...)` where `name` is a
known `-> int` function is a function call. `(expr)` where the first
token is not a known function is arithmetic grouping (unchanged).

The compiler has the function table from the first pass, so
disambiguation is a compile-time decision.

## Name collision

Function names and variable names must not collide: `fn f` and a variable
named `f` coexisting is a compile error. This eliminates ambiguity inside
`(( ))` where `f` could be either.

DD#86 governs scope and shadowing. Function names are global and each has
one source definition, not local shadows or source-order replacements.
Duplicate rejection is accepted but remains pending in
`reject-duplicate-function-definitions` (DG, 2026-11-06; journal 582).

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
statement position is a compile error [SB4016] — the return value would
be silently discarded. Capture the value with `$(f)` or arithmetic call
syntax, or explicitly discard it with an in-process `- f`. Boolean
contexts are not value consumers: an int function used as a predicate is
SB4030 (DD#74).

## Integer values are not statuses

DG ruled on 2026-11-05 (journal 573): integer and status results are
different types. Using an integer function's answer as a status is a type
error, in the same family as mixing string and integer values, not an
unhandled-status error.

A direct `-> int` function call is therefore not a legal pipeline stage,
whether bare, negated or prefixed with `-`. The error is SB4030.
Discarding a stage's status does not repair the stage's type mismatch.
The rule holds in every otherwise valid pipeline position and surrounding
handling context.

Likewise, `return f` in a non-int function cannot turn an int f's answer
into a status; redirects or a composite producer do not provide a cast.
Int-to-int return forwarding remains valid. A wrapper must discard the
integer as a value or deliberately produce its own status, rather than
forwarding the integer as one.

Ordinary integer captures, in-process integer-value discard, explicit
numeric status construction (`return N`) and explicit predicate production
(`return (( ... ))`) retain their existing contracts. No new declaration
or conversion syntax is introduced.

Pipeline and return-producer enforcement are implemented in journal 577.
`q-int-function-pipeline-status` covers direct stages;
`enforce-function-return-kinds` covers compatible and incompatible
return-producer forwarding.
