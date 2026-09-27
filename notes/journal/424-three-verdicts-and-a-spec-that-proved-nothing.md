# 424 — Three verdicts, and a spec that proved nothing

The wildcard audit (`audit-validate-wildcard-escapes`) owed three verdicts
after journal 423 closed the taint oracle. All three are given here. None of
them found a second bug — but the work of writing the *specs* for one of them
nearly shipped two checks that could not fail, and catching that is the part
of this session worth recording.

## Verdict 1 — `walk_arith_expansion_roots`'s `_ => {}` was sound

This was the obvious suspect, because journal 423's bug was exactly this
shape: a wildcard in a walker on the taint path, swallowing a variant that
had an `Expr` inside it.

It is not the same. `ArithExpr` has sixteen variants; the wildcard swallowed
five — `Literal`, `Var`, `DollarVar`, `PostIncDec`, `PreIncDec` — and not one
of them carries an `Expr`. I checked that against `ArithExpr::children()`,
which is total and is therefore the ground truth for the enum rather than my
reading of it. A walker whose whole contract is "hand `f` each embedded
`Expr`" has nothing to hand over for any of the five. The wildcard was
correct.

I spelled it out anyway, and the reason is not tidiness. The cost of being
wrong here went *up* while I was working: `walk_arith_expansion_roots` had
two callers when this charter was filed, and journal 423 added a third
(`arith_uses_tainted`). **All three are taint checks.** A variant added later
with an `Expr` inside it would now silently skip taint in three places at
once, and the failure would look exactly like the oracle I had just spent a
day on. The change costs nothing — one `|`-group replaces one wildcard, cc
and cog are unchanged at 11/3 — so there was no reason to leave the trap
armed.

`PreIncDec` stays a no-op rather than an ICE. It genuinely cannot arrive:
`reject_prefix_incdec` raises SB4020 in the parser, so `++n` never reaches
any consumer. But an assertion belongs where the invariant is load-bearing,
and `children()` already carries it.

## Verdict 2 — the SB4030 shield holds, and a spec that proved nothing

The second question was whether bare `ArithExpr::Var`/`DollarVar` needed a
taint arm. The answer in the code comment was "no, a tainted int is not
constructible" — which is a claim about the *language*, resting on nothing
executable. I probed every route I could find:

| route | result |
|---|---|
| `x=$(arg 1); $(( x ))` | SB4030 |
| `x=$(arg 1); $(( $x ))` | SB4030 |
| `int n=$(arg 1)` | SB4030 |
| `int n=$(getenv …)` | SB4030 |
| `read arr; int n=${arr[0]}` | SB5020 |

The shield holds. I wrote the five up as `atint.17`–`atint.21`, ran the file,
230 passed, and was about to move on.

Then I ran the clean counterparts.

```
x=7; echo "$(( x ))"    ->  SB4030
```

The same diagnostic. `atint.17` and `atint.18` would pass with a *completely
untainted* `x`. They were asserting DD#77's disposition rule and calling it
taint — five specs in the block, two of them proving nothing about the thing
the block is named for. That is precisely the `cov.205` failure mode from
journal 422: a check whose comment describes a mechanism it does not touch.

The fix was not to delete them. Nothing else in the suite asserts the
bare-name case at all — every neighbouring `dsp.*`/`pex.*` spec exercises the
*disposition*, which is the permitted spelling, so the prohibition on the
bare one was unpinned. They are genuinely the shield's first half: no string
reaches a bare arithmetic name, tainted or not, and if DD#77 were ever
relaxed a tainted string *would* reach the variant both walkers decline to
visit. So they stay, with comments that say plainly what they are and — more
importantly — what they are not.

The other three are real discriminators, and I recorded the control for each
rather than asserting it:

```
int n=7                          ->  COMPILES     (atint.19, .20 assert SB4030)
array arr=(7); int n=${arr[0]}   ->  SB4030       (atint.21 asserts SB5020)
```

`atint.21` is the sharp one. Clean and tainted give *different codes* for the
same shape, so the assertion cannot be satisfied by the type rule alone —
break the taint check and the spec fails with SB4030 in hand.

The general lesson, and I want it written down because I have now hit it
twice in three sessions: **running the spec is not the check.** A green spec
tells you the program produced the expected output. Running the *clean
counterpart* is what tells you the spec is about the thing you named it
after. It costs one invocation.

## Verdict 3 — `StmtKind::Case` is alive, and the charter was wrong

The charter asked whether `StmtKind::Case` was dead now that `case` is
removed from the language. I filed that question. It was my error.

`StmtKind::Case` is the **DD#66 dispatch form**,
`[[ value { pat { … } } ]]`. What SB4029 rejects is `case`/`esac`, the bash
keyword, which has no parser production at all. The node name is a holdover
from before the dispatch form existed.

Three independent confirmations: the sole producer is
`parse::parser::parse_dispatch`; `stmt_is_dispatch` is literally
`matches!(kind, StmtKind::Case(_))`; and `--emit-ir` on
`[[ $x == bar { echo bar } || { echo other } ]]` emits `:kind :case` and the
program runs. The struct's own doc comment says all of this already, in the
imperative — "Do not read a surviving `case` statement into it."

So the answer was sitting in the source the whole time, written by someone
who anticipated exactly this mistake, and I filed the question anyway. Worth
noting as a reading failure rather than a code one.

## Still open

`check_value_cmdsub_reads` still has `_ => {}`, covering 5 of 13 `ExprKind`
variants — the last instance of the shape in this cohort. It is not currently
exploitable (probe table in the task file), but it is awkward against the
complexity gate: cc 7 / **cog 5**, so no dispatcher exemption, and the 3x cog
bar sits at 5.58. Making it total by brute force would breach it. That is a
change that wants shaping, not a line I should sneak in at the end of a
session about something else.

## Gate

Predicted 6550, swept 6550 (6537 passing + 13 skipped), zero failing, zero
flips against the pre-sweep baseline, exactly ten new ids (five specs x two
modes). `mk test` 107 passed / 0 failed. Clippy clean.
