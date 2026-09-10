# 140 — DD#74: predicate functions, explicit `return`

DG established DD#74: a function used in a *boolean context* is a **predicate**,
and its truth value comes only from an explicit `return`. No implicit
trailing-command status (that reintroduced `set -e` surprises — the trailing
command would abort-on-fail rather than yield a status). Shipped in three
phases, native/c/janet, 0 native regressions throughout.

## Phase 1 — `return N` drives the condition (`62fa5c1`)

The bug: `f() { return 0 }; if f; then …` branched on `test_result_var`
(set by commands / `[[ ]]`), but `return N` only set `$?`, so `if f` never
saw a status. Fix: `handle_exit_status_assign` (the `$?`-assign path a
`return` funnels through) now also syncs `test_result_var`; c mirrors it on
`__test_result`, janet on `__last_exit`. Also filled c's missing `!` sigil
and fixed janet's broken `if <userfunc>`.

## Phase 2 — `return <producer>` (`679262a`)

`return` now takes a status *producer*: a command, `[[ … ]]`, or `(( … ))`
(as well as `N` / `$?` / bare). New `StmtKind::ReturnProducer`; `parse_return`
disambiguates producer vs. value form; `lower_return_producer` lowers the
inner in `in_condition` (so `[[ ]]`/`(( ))` aren't flagged must-consume) and
uncheck the command so a `return grep …` is consumed, never aborts.

## Phase 3 — definite return in a boolean context (SB4037)

`check_predicate_definite_return` (validate.rs). A function is a predicate iff
it is *used* in a boolean context — the last statement of an `if`/`elif`/
`while`/`until` condition, a `! f` negation, or a `&&`/`||` operand where the
name resolves to a known function. Every path of a predicate must end in an
explicit `return`:

- `stmt_always_returns`: `return` / `return <producer>` → returns; an `if`
  returns iff it has an `else` and then/all-elif/else all return; a `[[ … ]]`
  dispatch returns iff it has a default (`||`) arm and every arm returns;
  groups/redirections recurse; everything else (bare command, loop, …) → not
  a return.
- A function called **only** bare (a procedure — `greet() { echo hi }; greet`)
  needs no `return`; only a boolean-context use requires definite-return.

Front-end check, so native + c (shared SSA) and janet all inherit the same
`SB4037` diagnostic. specs `spec/functions.sh` 74.13–74.18 cover: bare-trailing
predicate rejected, `return 0` accepted, `if`-without-`else` fall-through
rejected, `if`/`else` both-return accepted, bare void call still fine, `! f`
also requires a predicate. 18 new passing (×3 modes), 0 regressions.

## Complexity gate

Adding the check touched `validate()`, un-grandfathering it (it sat at
cog=8 > 3×mean). Rather than bypass, refactored `validate()` to iterate a
`blocks` list (statements + the always-`None` `__END__` block) uniformly —
behavior-identical, cog 8→4. Split `collect_predicate_uses` into a loop +
per-statement helper and extracted `if_always_returns` / `case_always_returns`
from `stmt_always_returns` so every new function clears the gate.
