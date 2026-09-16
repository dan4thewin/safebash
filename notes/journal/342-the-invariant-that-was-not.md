# 342 — The invariant that was not

DG overruled me on the coverage-driven purge, and the overruling was
right in a way I did not anticipate: the fix it forced exposed a real
hole within minutes of being written.

## The disagreement

I had measured the `ArithExpr::PreIncDec` purge and reported that the
filing over-counted five-fold: of ten match sites, only two were
genuinely cold, and the other eight were `| PreIncDec` alternatives
merged into arms with counts as high as 1305. My recommendation was to
take the two clean deletes and leave the rest, on the grounds that
deleting a merged alternative changes no counter and therefore buys no
coverage.

DG's answer:

> deleting a merged alternative would prevent it matching in the
> future, yes? given a disallowed usage is merged with an allowed
> usage - that's undesirable. That it cannot be measured doesn't
> prevent reasoning about it.

That is the whole argument. `x++` is allowed; `++x` is rejected. When
both ride the same arm, the arm's behaviour is correct only by
coincidence, and nothing in the code records which of the two it was
written for. The coverage number is a proxy. I had promoted the proxy
over the property it stands in for.

So instead of deleting, I split: `PreIncDec` got its own arm at nine
sites, each one an `unreachable!()` citing a new
`ir::PREFIX_INCDEC_INVARIANT`.

## What the split found on its first run

Ten prefix spellings, all of which had been giving SB4020. Eight still
did. Two now panicked:

```
/bin/echo "$(( ++x ))"
/bin/echo "${x:-$(( ++x ))}"
```

The backtrace ran `ir::arith_has_mutation` ← `check_arith_value_mutation`
← `validate`. Which means the `PreIncDec` node was reaching **validate** —
the parser had not rejected it at all.

I had written down, twice, that the two callers which bypass the
arithmetic entry points "carry their own rejection". They do not.
`emit_string_part` (`parse/expr.rs:508`) and the bare-subscript index
re-parse (`parse/arith.rs:1048`) call `parse_arith_str` raw. The SB4020
those paths produced came from `validate`, and `validate` caught the
prefix form **only because `arith_has_mutation` lumped it in with the
mutations** — the merged alternative was load-bearing.

This is exactly the hazard DG described, arriving with a user-visible
symptom I had not thought to look for:

| source | diagnostic |
|---|---|
| `int y=$(( ++x ))` | `prefix ++x is not allowed — use postfix (x++) or (( x += 1 ))` |
| `/bin/echo "$(( ++x ))"` | `an assignment or ++/-- inside $(( … )) can't take effect` |

The same construct, a vaguer message inside double quotes than outside,
for no reason a user could ever infer.

## The fix: one funnel, not two entry points

Every route into arithmetic passes through `parse_arith_str`. The
rejection had been sitting in the two *entry points* above it
(`parse_arith_expansion`, `parse_arith_slot`), which is why two callers
could go around it. Moving `reject_prefix_incdec` into `parse_arith_str`
itself makes the invariant structurally true rather than asserted in the
places someone remembered to assert it, and lets both entry-point calls
be deleted as redundant.

All ten spellings now give the specific message. `(( x++ ))` still
compiles, `$(( x++ ))` still gets the generic mutation message (correct:
in a *value* context both forms are rejected, and only the message
differed).

`cov.253-255` pin the three formerly-bypassing routes and assert the
**message**, not just the code — a slide back to the generic one has to
fail somewhere.

## The complexity gate had an opinion too

Nine new arms meant nine functions gained cc. Three tripped the gate:
`arith_has_mutation` (14/2), `check_positionals_arith` (14/3),
`lower_arith_mutation_structured` (12/6). Two of those were *already*
over the 11.98 bar and had been grandfathered; touching them un-
grandfathered them, which is the mechanism working as designed.

Worth recording that the rubric and the enforced gate disagree here.
The rubric says "high cc, low cog = wide dispatcher — accept as-is".
But the gate's dispatcher exemption needs `cog - cc <= -15`, so a
dispatcher only escapes once `cc >= cog + 15`. At cog 2 that is cc 17.
A twelve-arm walker with cog 2 sits in a dead zone: too wide for the
bar, not wide enough for the exemption. The answer is not to widen the
exemption but to stop hand-rolling the same walk.

Both walkers were re-deriving the shape of `ArithExpr` inline. Naming
the children once — `ArithExpr::children()` — collapses each of them to
a single loop over the variants they do *not* treat specially:

| function | before | after |
|---|---|---|
| `arith_has_mutation` | cc 14 / cog 2 | **cc 4 / cog 1** |
| `check_positionals_arith` | cc 14 / cog 3 | **cc 6 / cog 3** |
| `lower_arith_mutation_structured` | cc 12 / cog 6 | **cc 10 / cog 4** |

`children()` itself is cc 8. The net is a large reduction, and two
long-standing violators are now inside spec rather than waiting for
someone to touch them.

The lowerer came down a different way: four of its arms built a
byte-identical `Statement { Assignment { name, index, op: Set, value } }`
and differed only in the name, the optional index, and the RHS.
`arith_assign_stmt` takes exactly those three; `binop_on_var` and
`subscript_assign_rhs` take the two inner decisions. This is the kind
of thing `block_dupes` exists to surface.

## What I take from it

The coverage loop is a good instrument for finding unasked questions,
and a bad instrument for deciding what to delete. Journal 341 said a
cold branch is not a dead branch. This is the mirror: **a hot branch is
not a justified branch.** An arm carrying 1305 executions told me
nothing about whether the alternative I cared about had ever matched —
and in this case the honest answer was that it matched constantly, on a
path whose rejection I had mis-documented twice.

A merged alternative is an unasked question that coverage is
structurally incapable of asking. The only way to ask it is to separate
the alternatives and see what breaks. Something broke immediately.
