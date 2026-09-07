title: DD#74 — Predicate functions: explicit `return <producer>`, no implicit trailing status
status: accepted

Decided with DG 2026-08-09. Refines the user-defined-function / `return`
sections of **DD#55** (error model) and composes with **Option C** (a `-> int`
value function's `$(f)`/`$((f))` capture). Motivated by a confirmed bug: a
function's explicit `return N` did not drive an enclosing `if`/`while`
condition (the branch read `test_result_var`, which `return` never set), so a
predicate written with `return` was silently mis-evaluated; and by DG's wish
to make a predicate's status **explicit** rather than the accident of a bare
trailing command.

## A function used in a boolean context is a *predicate*

Boolean contexts (DD#70 dual / `reject-nonstatus-in-boolean-context`):
`if f`, `elif`, `while f`, `until f`, `! f`, and `f` as an operand of
`&&`/`||`.

**Addendum (DG, 2026-09-07): the TAIL of an `&&`/`||` *statement* is not a
boolean context.** A function is a predicate when something branches on its
status. In a bare `A || B` statement the final clause's status is simply the
statement's own status — nothing branches on it — so it is no more a predicate
than a bare `f` statement is, and requiring `return` there was wrong:

    [[ -n $x ]] || warn      # `warn` needs no `return`

Everything else is unchanged, and deliberately so:

- the **left** operand is still a predicate (its status decides whether the
  right side runs);
- so is a **middle** clause — `&&`/`||` are left-associative, so in
  `a && b && c` the left operand is the nested list `(a && b)`, and `b`'s
  status decides whether `c` runs;
- inside an `if`/`while` **condition** the whole list is a predicate, **last
  clause included** (DG: "an `if a && b && c { }` should probably still require
  `c` to be a predicate") — a condition is consumed as a boolean, so its final
  status decides the branch.

The distinction falls out of where the marking happens: a condition goes
through `mark_condition_predicate` → `mark_predicate_operand`, which recurses
into *both* operands of a list, while a bare list statement marks only its
left.

A predicate's status comes **only** from an explicit `return`. There is **no
implicit trailing status**: a bare trailing `grep` / `[[ … ]]` / `(( … ))` as
a function's last statement is an ordinary statement — subject to DD#55
(a bare fallible command **aborts** on failure) — **not** the function's
result. To yield a status, wrap it in `return`.

## `return` takes a status producer

| form | status |
|---|---|
| `return CMD [args]` | run CMD; its exit status. **The `return` is the handler — CMD never aborts** (its status is explicitly consumed), unlike a bare CMD. |
| `return [[ TEST ]]` | the test's boolean — 0 (true) / 1 (false) |
| `return (( EXPR ))` | the arith truth — 0 if `EXPR` ≠ 0, else 1 |
| `return N` | an explicit status code — **meaningful codes beyond 0/1** |
| `return $?` | the last status (handling position only — DD#55) |
| `return A \| B` | a **pipeline** producer — the pipeline's aggregate status (DD#55) |
| `return A && B`, `return A \|\| B` | an **and/or-list** producer — the list's short-circuit status |
| `return CMD > f` | a producer with a trailing **redirect** |
| `return` | status 0 |

A producer is not limited to a single command: `return` absorbs a full
**composite** — a pipeline, an `&&`/`||` list, or a trailing redirect — exactly
as a normal statement would. The `return` is still the handler for the whole
composite: every stage / operand runs under the return (no uncaught abort), and
the composite's aggregate status (DD#55 pipeline semantics) becomes the returned
status. Without this, `return A | B` parsed as `Pipeline([return A, B])` — the
`return` trapped in stage 0, with `B` running uncaught after it (SB4037 in a
predicate; a silent abort as a statement). Fixed by folding the parsed producer
through the same `maybe_wrap_redirects` a statement gets (`parse_return`, DG
2026-08-31).

`return [[ … ]]` also discharges the DD#70 must-consume obligation on `[[ … ]]`
— the `return` consumes the test — so the natural predicate spelling compiles
(a bare `f() { [[ … ]] }` is SB4018 "result unused"; `return [[ … ]]` is not).

**Arith truth (verified).** `EXPR` evaluates to an integer; `(( EXPR ))`
maps nonzero → status 0 (true), zero → status 1 (false). So
`return (( a > b ))` returns true exactly when `a > b`
(`a>b` → `1` → status 0). `(( … ))` is a **status**; `$(( … ))` is a
**value** — this disambiguation is what separates a predicate's
`return (( … ))` from an `-> int` function's `return $(( … ))`.

## `return N` drives the condition (the bug fix)

Every `return` sets **both** `$?` (exit-status) **and** `test_result_var`
(= status == 0), so `if f` / `while f` / `! f` / `&&` / `||` branch on the
returned status. A nonzero `return 2` is `$?` = 2 in the handling branch and
*false* (nonzero) for the branch — meaningful error codes propagate.

## Definite return required in a boolean context

A function used in a boolean context must be a **predicate**: **every** exit
path is an explicit `return <status>` (full definite-return analysis — an
`if` with no `else`, or a fall-through past the last `return`, is a reachable
non-returning path and is rejected). Using a **void** function (no `return`)
or a function that can fall off its end in a boolean context is a compile
error ("not a predicate — return a status"). An `-> int` **value** function
in a boolean context stays SB4030 (it is a value, not a predicate).

DD#55's "`return 0` alone is ceremony" stands: a function whose only possible
return is 0 is void/always-true, not a predicate.

## Canonical predicates

```sash
is_readable() { return [[ -r $1 ]] }
has_root()    { return grep -q root /etc/passwd }
in_range()    { return (( x >= lo && x <= hi )) }

if is_readable $f; then …
while has_root; do …; done
! in_range && abort
```

No defensive quotes — sash has no word-splitting (DD#22).

## Relationships

- **DD#55** — supersedes its "implicit trailing status makes a function a
  predicate" behavior; keeps non-void call-site handling (`if`/`||`/`-`), the
  `return 0`-ceremony rule, and bare-command abort.
- **DD#70 / reject-nonstatus-in-boolean-context** — the must-consume /
  boolean-context type discipline; `return <producer>` is the predicate
  producer, and a bare non-returning function is no longer a valid producer.
- **Option C** — a `-> int` **value** function returns a value (`return N` /
  `return $(( … ))`), captured via `$(f)`/`$((f))`. Distinct from a predicate,
  which returns a **status** via `return <producer>` and is consumed by a
  boolean context.

## Implementation (spec-first, native/c/janet)

1. `return N` sets `test_result_var` alongside `$?` (a return→test-result
   sigil, mirrored in c/janet), so `if f`/`while f`/`!`/`&&`/`||` see it.
2. `return <producer>`: parser accepts `return` + command / `[[ … ]]` /
   `(( … ))`; lower so the producer sets `$?`/`test_result_var` and `return`
   propagates it (CMD in `return` position never aborts — it is consumed).
3. Definite-return enforcement for boolean-context functions (**SB4037**);
   remove the implicit-trailing-status predicate path.

### Implemented (native/c/janet, 0 native regressions)

- Phase 1 (`return N` drives the condition) — commit `81c50dc`.
- Phase 2 (`return <producer>`) — commit `0adfc36`.
- Phase 3 (definite-return, **SB4037**) — `check_predicate_definite_return`
  in `validate.rs`. A function is a *predicate* iff it is used in a boolean
  context (an `if`/`elif`/`while`/`until` condition, a `! f` negation, or a
  `&&`/`||` operand — the last statement of the condition, plus operands,
  are the predicate positions). Every path of such a function must end in an
  explicit `return`: `stmt_always_returns` treats `return`/`return <producer>`
  as returning; an `if` returns iff it has an `else` and all branches return;
  a `[[ … ]]` dispatch returns iff it has a default (`||`) arm and all arms
  return; groups/redirections recurse. A function called **only** bare (a
  procedure) needs no `return`. Enforced in the front end, so it covers native
  and c (shared SSA); janet inherits the same front-end diagnostic.
