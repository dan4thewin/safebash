# 501 — The arm that outlived its ruling

Continuing the cold-line queue. Two items, both small, both the same
shape as the last three sessions: a cold arm that was never a gap in
the compiler, only a gap in the specs.

## `expr_references_var` — a genuine spec gap

`validate::expr_references_var` decides whether an `attest`'s `[[ ]]`
operand actually names the variable being attested. Its only external
entry is `taint_attest` → `test_references_var`, and because
`TestExpr::Binary` passes only the LHS, the function is always called
on the left operand of a test inside an `attest`.

Two of its arms were cold: `ParameterExpansion` and `Concat`. Probing
both showed the compiler was already right —

    attest h [[ ${h:-zz} == zz ]]     # accepted, references h
    attest h [[ ${h}tail == tail ]]   # accepted, references h
    attest g [[ ${h:-qq} == qq ]]     # SB5020, doesn't reference g

— so this was category 1, a spec gap and nothing more. `atop.1`-`atop.3`
in `spec/taint.sh` (plan 334 → 337), the third a negative so the pair
cannot both pass by accident.

One harness note worth keeping: `\x27` is **not** interpreted inside a
bash `[[ ]]` pattern. Asserting on the diagnostic's apostrophe needs a
literal `'` inside double quotes.

## `join_operand_name` — the counterpart that never got the cleanup

`type_check::join_operand_name` had an arm resolving a `@a` splat to
its bare name. Probing `len @a`, `join @a ,` and `keys @m` showed all
three are SB5062 from **validate**, long before type check runs, so the
arm could never fire.

The decisive thing is that this question was already settled. Its twin,
`transform::expr_handlers::collection_operand_name`, carries DG's
ruling from 2026-10-06 verbatim: the splat arm is

> a duplicate of SB5062's own question about the same `ExprKind`
> SB5062 had already inspected, which cannot catch a drift the gate
> would not catch first … an arm handling a rejected spelling tells
> the next reader that spelling is legitimate.

That ruling was applied to one of the two functions. `join_operand_name`
is the one it was never carried to. This is the third fork found in as
many sessions — `is_capture_condition_body` against
`is_capture_condition_rhs`, `handle_break_stmt` against
`handle_continue_stmt`, and now this one. The pattern is consistent
enough to be worth stating plainly: **a fix applied to one copy of a
shared classifier does not reach the other, and nothing in the build
reports it.**

Arm removed; the doc comment now cites the ruling and also corrects its
own caller list, which claimed `len`/`strlen`/`join` when the three
callers are `check_keys_operand`, `check_join_operand` and
`check_len_operand`.

## What actually protects the deletion

Deleting an unreachable arm is only safe if the gate that shadows it is
itself pinned. It was — partly. `keys @m` had a spec (75.36). `len @a`
and `join @a ,` did not: 75.38's comment explicitly notes both were
dropped from the *control* list by DD#100 Part D, but no negative was
ever added in their place. So two thirds of the splat rule rested on an
arm in a later pass that could not run.

`75.170` and `75.171` close that, asserting SB5062 and the "not a splat"
text. Same move as `88.29`-`88.31` last session, for the same reason:
the spec is the artefact, the cold arm was only standing where one
should have been.

## Sweep

Predicted 8359 before running — 8349 + 2×3 (`atop`) + 2×2 (`types`) —
and got 4064 c + 13 skipped + 4282 native = **8359**. Zero failures,
zero flips, exactly the ten expected new ids, liveness 258. `mk test`
118 + 27 + 20.
