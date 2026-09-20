# 374 — the splat that collapsed into one argument

*2026-09-19*

Two bugs, both found by probing the `@{…}` cold lines from the coverage queue
rather than by anything either bug was itself responsible for.

## DG's ruling on unreachable backstops

Asked what to do about provably-unreachable defensive code, since it is the
structural obstacle to the 100% target. DG:

> 100% would be the union of reachability from spec tests and unit tests.
> Unit tests should be able to bypass other layers. Any code that should be
> unreachable should halt with an ICE if it ever ran, no?

That settles it, and the codebase already had the pattern — `panic!("ICE: …")`
plus a `*_is_loud_ice` unit test driving the helper directly through
`catch_unwind` (`joined_form_to_string`, `opaque_form_to_string`,
`redirect_reject_noun_is_loud_ice`). Two consequences worth recording:

- **My cold-function list was measured wrong.** `mk cov` is spec-only by
  design, so `joined_form_to_string` and `opaque_form_to_string` appeared cold
  while already having ICE tests. Under DG's definition they were covered all
  along. The union needs `--branch --with-unit`, which no `mk` target ran.
- **One entry on my own list was not unreachable.** I reported `expr.rs:230`
  (`find(':')` in `parse_at_brace_disposition`) as dead. `@{a}` reaches it and
  raises SB5033 — it was a plain spec gap, now `gs.38`. Reporting a line as
  unreachable without a probe is the same error as reading a call site instead
  of running it.

## The bug: a splat glued to adjacent text collapsed the array

Probing `@{…}` malformed forms turned up one that did not fail at all:

```
array a=(p q); /usr/bin/printf "[%s]\n" @{a:-x}y
  sash:  [p qy]        ← ONE argument
  bash:  [p]  [qy]     ← two, suffix on the last element
```

All three dispositions did it (`:-`, `:?`, `:~`), and an adjacent *expansion*
did too (`@{a:-x}$s` → `[p qS]`). The scalar analogue `${s:-x}y` was correct
in both, so the defect was specific to the array splat.

**It was the third instance of one shape.** `validate.rs` already had

```rust
/// A scalar position (a `[[ ]]` operand, a redirect target): a named-array
/// splat there is SB5025 … Only the assignment path used to check this, so
/// the others accepted `@a` and then did the wrong thing silently.
```

— the same bug, found twice before and fixed for assignments, then for test
operands and redirect targets. `Concat` parts were the position nobody came
back for. `$(argv)y` and `$(keys m)y` were already rejected, with wording that
states the rule exactly: *"it cannot be joined into a scalar or passed as a
command argument."* Only the `arrvals` family was silent.

DG chose rejection over bash parity. The reasoning that makes it easy: sash
already diverges deliberately on splat safety (bare `@a` into an external
command is SB5032), and byte-identical output is a promise about programs sash
**accepts** — so rejecting restores the invariant immediately, while
silently joining breaks it. No spec depended on the form. New code SB5083.

## The bug behind the bug: a hardcoded span

Chasing `@{a:-x}}` (a trailing brace) produced a diagnostic pointing at
**line 1, column 1** — the `array` declaration, nothing to do with it. It
reproduced for every stray `}`, on any line:

```rust
0 => Err(parse_error(
    Span { line: 1, col: 1 },   // <- every "expected expression", ever
    "SB4014",
    "expected expression".to_string(),
)),
```

`parts_to_expr` had no span to give, so someone gave it a constant. Every
SB4014 from that path has pointed at the first byte of the file for as long as
the function has existed. Threading the word's own span through fixes all of
them at once.

Someone had already been bitten: `spec/scope.sh:291` asserts
`$err != *:1:1:*` on an unrelated code — a spec written to defend against a
symptom whose cause was two functions away.

## Complexity

The span fix un-grandfathered `parse_word_parts` (12/6 against an 11.69/5.85
bar — already over before I touched it). Extracted the match into
`absorb_one_part` and inverted the loop guard: `if !parts.is_empty() &&
!adjacent { break }` is exactly `while parts.is_empty() || adjacent`, which
says the intent out loud. Now 5/4 and 10/3.

## Gates

`cargo fmt`; units green; sweep **5783, 0 failing, 0 flips vs `4a93503`**
(5765 + 9×2, predicted exactly). Nine specs: `gs.40`–`gs.45` in
`glob_splat.sh`, `dspan.1`–`dspan.3` in `errors.sh`.

## Ten bugs

Neither of these was on the coverage queue. The queue supplied the *question*
— what input reaches `parse_at_brace_disposition`'s error closures? — and the
inputs written to answer it were malformed `@{…}` words, which is precisely
the input class nobody had tried. The cold line stayed cold; two bugs next to
it did not.
