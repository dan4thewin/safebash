# 396 — One of six

Journal 395 fixed the `until` miscompile that a coverage probe stumbled into.
This entry is about what the probe was originally *for*, which turned out to
be the larger of the two findings.

## The cold lines

`type_check.rs::check_condition_type` had eight cold lines — the body that
emits **SB4030**, *"int function `f` used in boolean context"*. 446 calls
reached the guard above them; none fired. The usual three outcomes apply:
spec hole, backstop, or dead. Reaching them took one program:

```sash
fn f -> int { return 7 }
if f { echo a }
```

SB4030. So: a spec hole, and a real one — there were **zero** specs for this
diagnostic.

That is where it should have ended. What made it more was reading the
function's only caller.

## One caller

```rust
fn check_if(if_stmt: &If, …) {
    // Check condition for int-function-in-boolean-context
    check_condition_type(&if_stmt.condition, env, errors);
```

One call, for the direct `if` condition. Not the `elif` clauses three lines
below it. Not `while`. Not anything else.

DD#74 does not leave the boolean contexts to inference — it lists them:

> `if f`, `elif`, `while f`, `until f`, `! f`, and `f` as an operand of
> `&&`/`||`.

Six. Checked: one. The other five accepted an int function silently, and
**silently** is the operative word. `return 7` sets status 7, and a nonzero
status is **false**, so `while f` never loops and `if ! f` takes the true
branch — in both cases the opposite of what the author's integer suggests.
An int function's return is a value; using it as a status inverts its
meaning for every value except 0 and 1.

## The advice that did not compile

A bare call of an int function gives **SB4016**:

> function 'f' returns a status — handle with if/||, discard with `-`, or
> capture with `$((f))`

Following the first suggestion lands on SB4030. That is the class DG ruled on
in `50ff49c`: if you advise, the advice must compile.

I had assumed this needed to be *type-aware* — keep `if`/`||` for plain
status functions, drop it for int ones — and DG approved that. Implementing it
showed the premise was wrong. The gate is `state.function_non_void`, and the
only thing that writes it is `analyze_function_returns`, whose first act is:

```rust
if f.return_type != crate::ir::VarType::Int {
    return;
}
```

`function_non_void` has only ever held `-> int` functions. So SB4016 is *by
construction* a diagnostic about int functions, and its advice to use `if`/`||`
was not occasionally wrong — it was wrong **every time it has ever been
printed**. The fix is unconditional, and the conditional I had written was
dead code before it ran once.

Worth noting how that was found: not by reading, but by a probe that came back
*clean* when I expected a diagnostic. `fn g { return 1 }` called bare produced
no SB4016. I had written the else-branch for that case; the case does not
exist.

## Structure

The natural instinct is to add `check_condition_type` calls at five more
sites. That is wrong for the `&&`/`||` rows, because DD#74's addendum makes
the rule position-sensitive:

- in a **condition**, the whole list is a predicate, last clause included;
- in a **bare statement**, the tail is *not* — nothing branches on it, it is
  the statement's own status. Only the left operand, and by left-associativity
  the middle clauses (`a && b && c` is `(a && b) && c`).

Two different traversals of the same syntax, chosen by context. Scattering
that across five call sites would mean five chances to pick the wrong one. It
is one pass instead, `collect_int_fn_predicates`, mirroring the traversal
`validate::mark_predicate_operand` already uses for DD#74's definite-return
rule — the same decision tree, so the two cannot drift.

It collects-then-emits. A statement inside a condition is reached twice — once
as a condition operand, once by the descent into child bodies — and *both
visits are correct*. They would simply report it twice. Deduplicating on
(span, name) is honest about that; suppressing one of the visits would not be.

## The collision

The full sweep found what the targeted probes could not. One spec failed:

```sash
fn f -> int { return 3 }
f || :            # e.b70.fndisc — asserted OK
```

`f || :` is DD#70's documented discard for an `-> int` function. It is also,
by DD#74, `f` as a `||` operand. Two DDs, one program, opposite answers — and
the spec asserting the DD#70 reading had been green the whole time.

This is the "might be deliberate" case, so it went to DG rather than being
decided at the keyboard. Two things made the question answerable rather than
merely a conflict:

1. **DD#70's principle was already partly dead.** It tells the author to
   restructure in-process logic — *"your own `-> int` function"* named
   explicitly — with *"`if` / `||` / `|| :`"*. SB4030 has rejected `if f` for
   as long as it has existed. The advice could not be followed for the case it
   named, and nobody had noticed because nobody had written the program.
2. **DD#70's own later addendum preferred the alternative.** `-` was added on
   2026-09-09 because *"`cmd || :` expresses discard as a side effect of
   short-circuit control flow; `- cmd` says it directly"*, and it legalised
   `- f` for exactly this case.

DG took the rejection. One rule — an int function's value is never a status —
and the replacement (`- f`, `$(( f ))`) already existed and was already the
spelling he preferred. Recorded as a DD#70 addendum, scoped to `-> int`
functions: `|| :` is untouched for `match` / `[[ ]]` / externals, which
produce a genuine status.

## Specs

`74.38`–`74.48` in `functions.sh` (plan 114→125), plus a rewritten
`e.b70.fndisc`. The block is not uniform:

- **74.38–74.42** — one per context: `if`, `elif`, `while`, `until`, `!`.
- **74.43** the left operand of a bare `&&`; **74.44** a *middle* clause,
  which only fails if the traversal recurses into the left operand's own
  nested list. It is the one that would catch a plausible wrong
  implementation.
- **74.45** the tail rule from the other side — `[[ 1 = 2 ]] || f` must draw
  SB4016 and **not** SB4030. Without it, "reject everything" passes.
- **74.46–74.48** the advice: SB4016 no longer names `if/||`, and the two
  forms it does name both compile and produce the right value.

Non-vacuity by execution again: with the source reverted and the specs left in
place, exactly seven fail — 74.39–74.44 and 74.46. 74.38 (already checked),
74.45, 74.47 and 74.48 pass both ways, which is what a shield does.

Sweep 6149 → **6171** (+22, eleven specs dual-mode), 0 failing, 0 flips.

## What this says about coverage as a method

Two bugs came out of eight cold lines, and neither *was* the cold lines. The
cold lines were a spec hole — the smallest of the three outcomes. What they
pointed at was a check wired to one of six call sites, and a loop in the
diagnostics that had been unfollowable since the day it was written.

The mechanism was the same both times: ask *what input reaches this line*, and
answer by running something. The first probe produced a diagnostic that named
a rule; reading the rule produced a list; checking the list against the code
produced the gap. None of that required understanding the compiler — only
refusing to stop at the first green result.
