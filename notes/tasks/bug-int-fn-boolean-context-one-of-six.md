title: "bug: int-function boolean-context check covers 1 of DD#74's 6 contexts"
status: done
priority: P2
kind: bug
completed: 2026-09-24
depends: []
gates:
  - "an `-> int` function is rejected in every boolean context DD#74 names"
  - "SB4016's advice compiles for an `-> int` function, or names a form that does"

## What

`type_check.rs::check_condition_type` (1728-1744) emits **SB4030** — *"int
function `f` used in boolean context"*. It has exactly one caller:
`check_if` at :1705, and only for the **direct `if` condition**.

DD#74 enumerates the boolean contexts verbatim:

> `if f`, `elif`, `while f`, `until f`, `! f`, and `f` as an operand of
> `&&`/`||`.

Verified by execution against the live binary — only `if f` is rejected:

| form | today |
|---|---|
| `if f` | SB4030 ✓ |
| `elif f` | accepted ✗ |
| `while f` | accepted ✗ |
| `until f` | accepted ✗ |
| `! f` | accepted ✗ |
| `f && …` | accepted ✗ |
| `f \|\| …` | accepted ✗ |

The hazard is silent, not loud. An `-> int` function's `return 3` sets status
3, which is **falsy**, so `while f` never loops and `if ! f` takes the true
branch — the opposite of what the integer suggests. This is the same hazard
SB4030 exists to catch; it simply is not checked anywhere except `if`.

DD#74's addendum matters for the `||` row: the **tail** of a bare `A || B`
statement is not a boolean context, but the **left** operand and middle
clauses are. So `f || true` (f as left operand) is in scope; the `B` in
`A || f` is not.

## The diagnostic loop

A bare call of an `-> int` function gives **SB4016**:

> function 'f' returns a status — handle with `if`/`||`, discard with `-`,
> or capture with `$((f))`

Three of those four work (`f || true`, `- f`, `int n=$((f))`). Taking the
`if` half lands on SB4030. Advice that does not compile is the class DG ruled
on in `50ff49c`.

Note the wording is generic — it is emitted for status-returning functions
generally, and is correct for those. Making it type-aware is the narrow fix;
dropping `if` from the list unconditionally would make it worse for the
common case.

## Why this is filed and not fixed

Bug policy makes fixing mandatory. The **approach** is a consequential,
hard-to-reverse fork and so is DG's:

- Extending SB4030 to five new syntactic positions **newly rejects programs
  that compile today**. Whether that is a breaking change worth taking in one
  step, or staged, is a language-surface decision.
- Whether SB4016's advice should become type-aware (naming `$((f))`/`-` and
  omitting `if`/`||` for `-> int`), or whether SB4030 should instead carry
  the redirect, is a diagnostics-design decision.

## Found by

Coverage work on `check_condition_type`'s eight cold lines (journal 395).
The same probe surfaced the `until`-inversion miscompile, fixed there.
Reaching SB4030 at all required a program nobody had written, which is why
there are **zero** specs for it — that hole should be closed by the same
change.

## Resolved 2026-09-24

DG's call: **all six contexts**, SB4016 made type-aware, specs in the same
change. Implemented as a sixth `type_check` pass (`collect_int_fn_predicates`)
rather than a hook in `check_if` — the six contexts span If/While/List/
Pipeline, and a single traversal is the only way to apply DD#74's tail rule
consistently. It collects-then-emits because a statement inside a condition is
legitimately reached twice (as a condition operand and by the descent).

SB4016's fix turned out to be **unconditional**, not type-aware: its gate is
`function_non_void`, populated only by `analyze_function_returns`, which
returns early for anything not `-> int`. Every name it has ever named was
int-typed, so `if`/`||` was wrong 100% of the time, not sometimes.

**One collision, escalated and settled.** `f || :` is DD#70's documented
discard for an `-> int` function and is, by DD#74, a `||` operand. DG took the
rejection; see the DD#70 addendum (2026-09-24). `- f` and `$(( f ))` are the
replacements, both already legal.

Specs `74.38`-`74.48` (functions.sh, plan 114->125) and a rewritten
`e.b70.fndisc`. Sweep 6149 -> 6171, 0 failing, 0 flips.
