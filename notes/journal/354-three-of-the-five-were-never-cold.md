# 354 — Three of the five were never cold

DG, on the arms that no accepted program can reach: *"use a unit test
simulating a regression to produce an ICE."*

That is the resolution I had failed to see. I had framed it as a choice
between deleting the arms (losing a compile-time guarantee) and exempting them
(building a mechanism to excuse them). Both accept that the lines cannot be
covered. They can — just not from a *program*. The compiler is a library, and
a unit test can hand it the IR that validation makes unconstructible from
source. That is precisely the regression the guard exists to catch, so the
test is not a coverage dodge; it is the assertion the guard was written to
make.

The project already did this once. `codegen/redirects.rs` has
`unresolved_redirect_var_is_loud_ice`, whose comment says "the condition is
unreachable in real compiles — validate rejects an undeclared `$f` with SB5031
— so a unit test is the only way to exercise the guard." I read that file
this week and did not connect it.

## Applied

`transform/expressions.rs` gets three tests: one that renders all thirteen
`ExprKind` variants through `expr_to_string`, and two that call
`joined_form_to_string` / `opaque_form_to_string` with a kind outside their
domain and assert the `unreachable!` fires.

Measured, not assumed: **every line of 475–530 is now covered**, and
`opaque_form_to_string` went from a fully cold function to four hits.

`emit_c.rs` gets three more for `ice_unreachable_command`, one per arm — the
rewritten-before-codegen case (`write`), the expression-intrinsic case
(`strlen`), and the own-instruction case (`read`). Each asserts on the panic
*text*, so a regression that lands in the wrong arm fails rather than passes.

## The finding I did not expect

Of the five functions the coverage data called fully cold:

| function | actually |
|---|---|
| `transform/expressions.rs::opaque_form_to_string` | genuinely uncovered — now tested |
| `emit_c.rs::ice_unreachable_command` | genuinely uncovered — now tested |
| `codegen/entry.rs::panic_payload_message` | **already had three unit tests, all three branches** |
| `codegen/redirects.rs::redirect_target_ice` | **already covered via its caller's unit test** |
| `type_check.rs::fmt` | dead code — deleted |

Three of the five were never cold. They read that way because `mk cov` runs
`contrib/coverage.sh` *without* `--with-unit`, which is off by default with
this rationale: "the goal is [to find] the should-never-run assertions worth
finding."

That rationale is sound and it produced this exact failure. Excluding unit
tests makes every should-never-run assertion visible — including the ones
someone already went and asserted. The queue I was working down was
substantially a measurement artifact, and I spent this session's first pass
investigating functions that were already done.

## What I think the measurement should be

Not "switch the default to `--with-unit`". Both numbers are real and they
answer different questions:

- **`mk cov`** (specs only) — *what does the language surface reach?* This is
  the bug-finding view, and it is the one that produced journals 351, 352 and
  353. Its value comes precisely from ICE guards showing up in it.
- **`mk cov-unit`** — *what is covered at all?* This is the view the 100%
  target is about.

The **gap between them is exactly the set of should-never-run assertions**. Run
both and that set stops being noise in one report or invisible in the other —
it becomes a quantity you can watch. It should be small, every member should
be deliberate, and a *new* entry appearing in the gap is the interesting
event.

I have not changed any default; this is a recommendation for DG.

## One deletion

`impl Display for TypeError` in `type_check.rs` had no callers. Nothing
formats a `TypeError` with `{}` or `to_string()` — `type_check_or_exit` writes
`te.span.line`, `te.span.col` and `te.message` by hand. Rust does not warn on
an unused trait impl, which is how a whole `Display` implementation sat there
being reported as an uncovered function.

It is not an ICE guard, so DG's standing exception does not apply: purged. The
build is clean without it, which is the proof it was dead.

Units 83 → 89. Sweep unchanged at **5404, 0 failing** — as it should be, since
nothing here touches behaviour.
