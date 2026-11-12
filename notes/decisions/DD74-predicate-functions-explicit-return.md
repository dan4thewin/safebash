title: "DD#74 — Predicate functions: explicit `return <producer>`, no implicit trailing status"
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
status. In a bare `A || B` statement nothing branches on the final
clause's result: it is an ordinary action, not a predicate, and may
supply no result. Requiring a `return` there was wrong. A non-void
operation still has its ordinary DD55 handling/discard obligation:

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

**Discarded statuses (DG, 2026-11-08; DD55).** These same predicate
contexts reject a command whose result the source explicitly discards
with `-`. A middle clause is consumed by the next connector; a final
clause is consumed when the whole list is a condition. Thus
`a || - b` remains legal as a statement, but `if a || - b { ... }`
rejects. Per-stage discard remains legal when a pipeline's aggregate
still has an unsuppressed stage. Enforcement is pending in
`reject-discarded-status-consumers`.

**Resultless control flow (DG, 2026-11-09; DD68).** The same consumer
boundary applies to `if`, `while`, `until` and `for`: they are
statements, not status producers. A left/middle clause or a consumed
list tail cannot be such a statement; an otherwise legal unconsumed
final action can. A condition inside a compound has a result, but
the compound itself does not inherit it. This includes the DD70
conditional-block shorthands. Enforcement is pending in
`q-compound-as-condition`.

A predicate's status comes **only** from an explicit `return`. There is **no
implicit trailing status**: a bare trailing `grep` / `[[ … ]]` / `(( … ))` as
a function's last statement is an ordinary statement — subject to DD#55
(a bare fallible command **aborts** on failure) — **not** the function's
result. To yield a status, wrap it in `return`.

## `return` takes a status producer

An int-valued function is not a status producer. In a non-int function,
`return int_fn` is a type mismatch (SB4030), not an implicit conversion;
neither a redirect nor suppression changes that. DG's 2026-11-05 ruling
in DD#58 settles this boundary; journal 577 implements it in
`enforce-function-return-kinds`. Explicit numeric status construction and
the status-producing tests below are unchanged.

`if` and loops are not return producers either (DD68). Reject
`return if ...`, `return ! if ...` and a returned logical list whose
consumed tail is a resultless compound or conditional-block shorthand.
Execute the control flow as statements and return an explicit result.
An explicit `return` inside an `if` or loop still belongs to the
enclosing function; compatible forwarding of a function call remains
unchanged. This does not add a compound-statement forwarding syntax.

In an anonymous capture body, the status-producing return forms below are
rejected (DG, 2026-11-06; DD#55/DD#86). Bare return and compatible void
forwarding remain legal. A named status function retains these forms,
including when called and explicitly handled/discarded inside a capture.
Enforcement is pending in `reject-anonymous-capture-status-returns`.

The `return` statement itself cannot carry a source `-` (DG,
2026-11-08; DD55). Reject `- return N`, `- return CMD` and the bare or
forwarding variants, including through redirects. Remove the prefix;
ordinary producer handling and the outgoing result are unchanged.
This is distinct from legal discard at a non-void function's call site.
Enforcement is pending in `reject-control-transfer-suppression`.

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
| `return` | no result; valid in a void function, not a status return |

Numeric return operands follow DD89's explicit-conversion rule
(DG, 2026-11-06): for a string `s`, write `return $(( ${s:-7} ))` or
`return $(( ${s:?bad} ))`, not a bare string-valued disposition. An
already-integer disposition retains its type. Removal of the compiler's
old return-only exception is pending in `bug-return-status-string-operand`;
this does not change producer forwarding or the function's result kind.

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

Because `return` is a status consumer, it cannot forward a status already
discarded by an explicit source `-` (DD55, DG 2026-11-08). This includes
`return A || - B` and all-suppressed pipeline producers. Partial pipeline
producers retain their usual aggregate. A body may still execute `- cmd`
and then `return 0` or another fresh status; constant results remain
legal. Ordinary producer handling is not source-level discard merely
because lowering disables its implicit abort check.

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

Every status-producing `return` sets **both** `$?` (exit-status) **and**
`test_result_var` (= status == 0), so `if f` / `while f` / `! f` / `&&` / `||` branch on the
returned status. A nonzero `return 2` is `$?` = 2 in the handling branch and
*false* (nonzero) for the branch — meaningful error codes propagate.

## Return completeness follows the function's kind

DG's 2026-11-06 ruling in DD#55 requires a status on every returning path of
a status function, whether or not a caller uses it as a predicate. Bare
return supplies no result, and fall-through does not manufacture status
zero. The same completeness requirement applies to integer results.
Paths that terminate the process and self-tail back edges are not missing
returns. Compatible forwarding preserves the callee's kind, including
through redirects.

A function used as a predicate must supply a status, not a void or integer
result. Using a void function is still an error ("not a predicate — return
a status"); an `-> int` value function in that context stays SB4030.

An unannotated function returning only `return 0` is a status function and
a valid predicate, not void. Constant status results of any value are legal;
their callers still have the ordinary non-void handling obligation.
There is no ceremony or constant-result check.

DD70's 2026-11-08 statement-effect check does not change that definition
rule. It can reject a known effect-free call whose result is ignored,
including an otherwise valid `- f`. A consumed result or observable
invocation effect is different; `return [[ ... ]]` consumes its result
and is not an unused-test statement. The new check remains pending in
`reject-pointless-statements`.

The kind/completeness refinement and SB4017 removal are implemented in
journal 577 (`enforce-function-return-kinds` and
`retire-return-ceremony-check`). Shared body lowering enforces the contract
independently of predicate use, including anonymous capture bodies.

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
  distinction between void and status results, and bare-command abort.
- **DD#70 / reject-nonstatus-in-boolean-context** — the must-consume /
  boolean-context type discipline; `return <producer>` is the predicate
  producer, and a bare non-returning function is no longer a valid producer.
- **Option C** — a `-> int` **value** function returns a value (`return N` /
  `return $(( … ))`), captured via `$(f)`/`$((f))`. Distinct from a predicate,
  which returns a **status** via `return <producer>` and is consumed by a
  boolean context.

## Original implementation (spec-first, native/c/janet)

1. `return N` sets `test_result_var` alongside `$?` (a return→test-result
   sigil, mirrored in c/janet), so `if f`/`while f`/`!`/`&&`/`||` see it.
2. `return <producer>`: parser accepts `return` + command / `[[ … ]]` /
   `(( … ))`; lower so the producer sets `$?`/`test_result_var` and `return`
   propagates it (CMD in `return` position never aborts — it is consumed).
3. Definite-return enforcement for boolean-context functions (**SB4037**);
   remove the implicit-trailing-status predicate path.

### Implemented (native/c/janet, 0 native regressions)

This is the historical implementation record. Its return scan counted a
bare return as status zero; the accepted 2026-11-06 refinement above has
separate implementation tasks.

- Phase 1 (`return N` drives the condition) — commit `62fa5c1`.
- Phase 2 (`return <producer>`) — commit `679262a`.
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

## Addendum (2026-10-27): which word is `N`

The table's `return N` meant digits only. Any other word headed a producer and
was looked up as a command, so `return -1` and `return +1` were SB5011
"command '-1' not found", while `exit -1` (DD#89) and `return $(( -1 ))`
returned -1. Worse, a number past i64 was taken as a value: native int
lowering ICE'd on it and C aborted at run time with "arithmetic overflow".

`N` is now exactly the word an int sink takes as a literal, a decimal that is
optionally signed and fits in i64. `parse_return` and the int-sink check in
`type_check` share one predicate, `ir::is_int_sink_literal`, so the two cannot
disagree. `return -1` returns -1 and `return +1` returns 1. Every other word
heads a producer, as `return abc` does (74.54): `return 9223372036854775808`
and `return -x` are SB5011 at compile time.

This was a bug, fixed because `parse_return` is a function the F1+F4 port
modifies (DG 2026-10-26). A string operand (`return $s`) is still not an int
sink. That bug is `bug-return-status-string-operand`, which also carries the
question for DG.

Specs: functions.sh 74.67-74.69. Journal 537.
