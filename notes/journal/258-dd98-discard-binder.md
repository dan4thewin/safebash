# 258 — `_`, and the two rules that met wrong

DG: *"how did we address discarding unwanted fields when doing read? was it
only the `: $x` mechanism? I'm thinking a 'don't care' variable like `_` might
have value. Check among pending tasks too."*

The honest answer to the first question was: **we didn't**, and the mechanism he
remembered doesn't work where it's needed most.

## What was actually there

Two rules, each defensible alone, colliding:

- **DD#86**: every variable must be read (SB5050). The safety net for a
  misspelled write target.
- **DD#24**: a `read` target is tainted; using it needs `attest`.

`: $v` is DD#86's documented escape hatch. But `: $v` is *itself a use*, so on a
standalone `read` it's a tainted read and dies in the SSA taint backstop. What
survives is:

```sh
read a b
attest a p?       # ← assert a property of the value you're discarding
attest b p?
```

That's not an idiom, it's a toll. `attest` exists to record what you checked,
and you checked nothing, because you don't care. And `read` has no discard of
its own — the *last* target silently absorbs the remainder, so you can't even
drop a trailing field by leaving it out.

Neither rule is wrong. The gap is that nothing in the language expressed *"bind
this and throw it away"*, so the two rules had no way to be jointly satisfied
except by lying with `attest`.

## Measuring before proposing

I nearly told DG that `: $v` never works on read targets. It does — after a
**pipeline** read:

```sh
read a b;                        ...; : $a   # backstop reject
/usr/bin/printf "x y\n" | read a b; ...; : $a   # compiles
```

Same taint status by SB5020, opposite answer for `: $a`. Catching that before
reporting mattered twice over: it made the report accurate, and the discrepancy
is a real bug in its own right (filed — and the standalone rejection carries no
SB code at all, coming from the backstop rather than the front-end pass).

I also had to re-run half my probes because they assigned variables without
reading them, so SB5050 fired first and reported "already rejected" for cases
that were really ICEs or successes. That's the second session running the
dead-variable gate has masked what I was measuring. **A probe in this language
must read everything it writes** — which is, amusingly, exactly the ergonomic
problem under discussion.

## The design turned on one property

DG chose `_` everywhere a binder appears. The only part I treated as *not* open
was making it **write-only**, because taint forces it:

```sh
read _ x
y=$_          # if _ were taint-exempt AND readable, this launders
```

A don't-care that is both unobservable-for-taint and readable is a validator
bypass spelled with punctuation. So "reading `_` is an error" isn't a
restriction bolted onto the feature — it's the thing that makes the taint
exemption sound. That's why it went in as a stated consequence rather than a
question: DD#24 already settles it.

Worth noting how cheap the rest was. No lexer change, no parser change, no new
sigil: `is_plain_identifier` already accepted `_`, and `read _ b _` was already
tolerated. Three checks — an exemption in `report_dead`, an exemption in the
attest-all both-binders rule, and the SB5056 reject. A feature that fits in the
grammar you already have is usually the right shape.

The attest-all case was an unplanned win. DD#61 requires a map attest to
validate **both** binders; there was no way to say "I only care about values".
Now there is: `attest _ v in m [[ -n $v ]]`.

## I tried to break it, and did

Because this is taint-adjacent, I ran laundering attempts rather than trusting
the specs I'd just written: `y=$_`, `${_:-d}`, `"p${_}q"`, a test operand, a
for-body read, a read inside the attest predicate — all correctly SB5056.

Two got through to a *different* rejection:

```sh
(( y = _ ))
y=$(( _ + 1 ))
```

Not a security hole — the taint backstop caught them — but the wrong
diagnostic: no SB code, no explanation. A bare `_` in arithmetic is an
`ArithExpr::Var`, and every expression walker in the compiler surfaces
`ArithExpr::Expansion` only. My first cut inherited the blind spot.

This is the **third** time this week a check stopped at the `ArithExpr`
boundary — the DD#65 pruned-operator reject, DD#97's `walk_arith_expansions`
subscript gap, now this. The fix each time is small; the pattern is the
finding. Arithmetic is a second expression language living inside the first,
and every check written against the outer one has to be deliberately carried
across. `walk_arith_var_reads` turned out to be the exact complement of the
expansion walker — bare operands, skipping `Expansion` — so the fix was to call
both.

## On being asked a question

DG asked how something worked and floated an idea. The useful move was not to
answer from the DDs — the DDs said `: $v` was the hatch, which was true and
insufficient — but to *run it*. Every claim in my answer came from a probe:
that the last target absorbs the remainder, that `: $a` dies on standalone
read, that `attest a p?` is the only survivor, that no `_` variable exists
anywhere in the tree, that `read _ b _` already parses.

The documentation described the design. The binaries described the language.

## Sweep

4270 passing / 39 failing / 394 skipped. 0 regressions, +45, no new failures.
