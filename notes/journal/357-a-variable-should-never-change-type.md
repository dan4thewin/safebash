# 357 — A variable should never change type

Two turns of DG correcting me produced a principle, and the principle produced
two bugs that had been sitting in the open.

## How it started: being caught asserting

Journal 356 shipped two ICE guards with a rationale I had never executed — I
wrote that the fallback they shadow "renders the operand as the literal text
`@a`". DG asked what that fallback actually was. It renders `${aarrvals}`,
strips to `{aarrvals}`, misses the collection set, and emits `strlen` of an
undefined var: a silent **0** where an element count belongs. Same conclusion,
invented mechanism.

Then the harder one. I had cited DD#86 — *"a `for` variable is not a
declaration and is untouched"* — to **reverse an instruction from DG**. He
said he never sanctioned it. `git log -L` agrees: the sentence entered in
`06912b0`, an implementation commit, whose message gives the reasoning outright
— *"A for variable is its own StmtKind, never a Declaration."* An observation
about the AST, written into a model section, promoted to a decision by being
sat next to real ones, and three days later used to override its own author's
director.

The postmortem collects six cases of narrowing DG's options. This was worse:
overriding him on authority the AI had manufactured. Step 1 of the decision
tree — "settled by a source of truth" — silently assumes the source of truth
is *someone else's*. Checking provenance is part of citing it.

## The principle

DG's own model: the loop variable **survives the loop**, so `for` is an
implicit declaration, not a non-declared special var. That is measurable, and
it measures true — `for i in 1 2 3 { … }; echo "after=$i"` prints `after=3` in
sash and bash alike.

Then, asked how far the rule should reach:

> *"I don't think a variable should ever change type."*

and, closing the gap that would otherwise make it painful:

> *"the following should also create an int by assignment: `((x=7))`"*

That second sentence is what makes the first one cheap. I had assumed
assignment-instantiation always yields a string — which is true of `x=7`, and
is why `$(( x + 1 ))` on it is SB4030 — and concluded that fixing types at
first introduction would leave no way to make an int without a declaration.
There already was one. `(( x = 7 ))` produces a genuine int, and even rejects
a later `x=hello` correctly. The arithmetic path was the model of the
behaviour the rest of the language was missing.

## Two bugs

**The `for` binding was never type-checked.** `check_for` registered the loop
variable and checked the header words for bad subscripts, then descended into
the body. It never checked the binding itself:

```sh
int x=5
x=hello          # SB4030
for x in a b { } # accepted — x silently became 0, twice
```

`const` (SB4044) and collections (SB5064) already gated the `for` binding
correctly. `int` was the only hole, and the fix is four lines that call the
same type comparison assignment uses.

**A declaration reached backwards in time.** Declarations are collected
program-wide, so one appearing *after* a use retyped that use:

```sh
x=hello
echo "$x"     # 0
int x=7
echo "$x"     # 7
```

`x=hello; echo "$x"` printing `0` is wrong under any type discipline. It hid
because the obvious test case, `x=5`, prints `5 7` and looks right — `5`
survives the round trip through int. Only a non-numeric value exposes it.
`(( x = 7 )); echo "$x"; str x="a"` is the same bug mirrored, printing `7 0`.

That is **SB5082**: a typed declaration of a name an earlier binding already
introduced. It needed a small walker for arithmetic write targets
(`walk_arith_var_writes`, the companion to the existing reads walker) so that
`(( x = 7 ))` counts as an introduction alongside `x=7` and `for x in …`.

## The overreach, and the spec that caught it

Implementing the binding rule, I also declared *undeclared* loop variables
`String`, reasoning that a word list binds words. The sweep came back with one
failure: `cf.f5`.

```sh
int sum=0; for x in 10 20 30; do (( sum += x )); done
```

A counting loop whose variable is a word-list binding. I read that as my
overreach — a typing decision DG had not asked for that broke working code —
reverted it, and wrote a spec pinning the permissive behaviour.

DG, on seeing it:

> *"the for x in 10 20 30 should probably be for (( i=10; i<40; i+=10 ))"*

Which is the principle applied where I had stopped applying it. A word list
binds words; that loop wants numbers; sash has a header that binds numbers.
The pattern wasn't working code the rule broke — it was code relying on the
loop variable having **no type at all**, which is the very gap being closed.

I had done the thing AGENTS.md names explicitly: *"Apply a stated principle to
all its consequences; do not narrow or reinterpret it into a smaller decision
so you can defer the rest."* Faced with a failing test, I narrowed the
principle to fit the test rather than asking whether the test encoded the gap.

The measurement made the call cheap, and I should have taken it before
reverting rather than after: **two** sites in sash use arithmetic on a
word-list loop variable — `cf.f5`, and the spec I had just written to protect
it. The other eighteen hits are in `suites/`: bash's own test corpus, oils,
and shellcheck's Haskell source. Reference material, not sash code, and not a
constraint.

Both exits already existed and both were already green:

```sh
for x in 10 20 30 { (( sum += ${x:-0} )) }   # the disposition SB4030 names
for (( i = 10; i < 40; i += 10 )) { … }      # the form that means a count
```

Nothing is lost. `cf.f5` became the second, and `86.56`–`86.58` now pin the
rule together with both ways through it.

## A gate that improved the diagnostic

The complexity gate rejected the first commit, naming two functions: the new
`check_for_binding_type` (cog 6, against a 5.96 bar) and `check_stmt` (cog 6),
which I had touched for one reason only — to thread `stmt.span` through to the
new check.

The second was the useful signal. The span was needed to point the diagnostic
somewhere, and reaching into the dispatcher for it pulled a grandfathered
function into the gate's scope. But a `for` header has several words and only
one of them is wrong; the statement span was the wrong answer anyway. Taking
the span from the offending `Expr` instead dropped `check_stmt` out of the
diff entirely and made the error point where the mistake is:

```
q.sh:2:14: error [SB4030] cannot bind string value to int loop variable `x`
```

Column 14 of `for x in 1 2 bad 4` is `bad`. Splitting the search into
`first_binding_mismatch` took the new function to cc 2 / cog 1.

## Result

Sweep **5476 → 5480, 0 failing** (20 specs `86.39`–`86.58`, dual-mode;
predicted the total three times and hit it three times, including the run that
surfaced `cf.f5`). Units **91 → 92**.

The reversal is worth keeping in view. I spent a commit defending a rule DG
never made, then, when he made the real one, spent a sweep defending a spec
against it. Both times the instinct was to protect what was already there.
Both times the thing already there was the bug.
