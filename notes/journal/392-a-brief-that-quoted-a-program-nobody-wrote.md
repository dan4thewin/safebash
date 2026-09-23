# 392 — A brief that quoted a program nobody wrote

The cold-line queue pointed at `ir::arith_op_str`: ten of its nineteen
operator arms had never been rendered by any test. The arms turned out to be
fine. Reading what came out of the probe found a bug in the caller.

## The probe that could not fail

First attempt reported all twelve operators compiling cleanly, which would
have meant the arms were unreachable and the whole function could shrink.

It was nonsense. `sash -c` takes the script **inline**; I handed it a
*pathname*. So every run compiled a one-word program consisting of the string
`/home/dg/sashscratch/tmp/p.sh` — which, being an absolute path to a file that
exists, resolves under DD#23 and compiles. Twelve clean builds, zero of them
of the program I wrote.

Had I believed it, I would have deleted ten live arms and called it coverage
progress. The rule that catches this costs two lines: **every probe harness
gets a case that must fail.** The rerun leads with a deliberate syntax error,
and its output is now the first thing I read.

## The ten arms are live — and the message they build is not

With the harness fixed, all ten operators reach `arith_op_str` through
`arith_brief`, which recurses through `BinOp` and quotes faithfully:

```
$ sash -c 'int x=4
if (( 1 >> x = 2 )); then echo y; fi'
error [SB5007] mutation 1 >> x = ... not allowed in condition (( )) …
```

That is the *nested* path. The path users actually take is not that one:

| source | SB5007 said | |
|---|---|---|
| `if (( x = 2 ))` | `mutation x = ...` | correct |
| `if (( x -= 2 ))` | `mutation x = ...` | wrong operator |
| `if (( x++ ))` | `mutation x = ...` | not an assignment at all |
| `if (( a[0] -= 2 ))` | `mutation a = ...` | names the whole array |

Two sites emit byte-identical SB5007 text and only one of them is honest. The
common one, `validate_assignment_stmt`, is honest for exactly the spelling
that happens to match.

## Why, and why it is not fixable by guessing

`lower_arith_mutation_structured` collapses all four mutation spellings —
`v = e`, `v op= e`, `v++`, `a[i] op= e` — into `Assignment { op: Set, value:
BinOp }`. Its doc says so outright; this is the design, not an accident. By
the time validate runs, the operator is gone. The site called `assign_brief`
regardless, so it printed the one spelling it could still name.

The tempting fix is to read it back off the decomposed form: `BinOp { left:
Var("x"), op: Sub, .. }` under a target named `x` came from `x -= 2`. It did
not, necessarily. `(( x = x - 2 ))` decomposes to exactly the same tree. Any
such reconstruction misquotes one of the two, which is the misfire class the
`[` → `$a[i]` hint is already documented as accepting.

DG's call: **name the target, claim no operator.** `mutation of x`,
`mutation of a[…]`. It satisfies the principle already written into
`arith_op_str`'s doc comment — *a brief is read as a quotation; showing a
different operator than the source is worse than showing less* — at one site,
with no provenance field added to the IR for one diagnostic's benefit.

The subscript rendering wrote itself: `a[…]` is already the house spelling for
a cell in six other diagnostics (`bare_string_cell_error`,
`scalar_subscript_diag`, `reject_scalar_rhs_read`, SB5006). My first draft
rendered a literal index as `a[0]` — both inconsistent with that convention
and dead, since the index arrives as an arithmetic expression. Deleting the
arm made the function shorter *and* more correct, which is the usual sign that
the convention was right.

## The bug was in a comment, waiting

`coverage.sh` cov.150 already said it:

> A BARE `if (( x++ ))` reports `x = ...` (the Assign arm), so these had to be
> reached through a BinOp, where the inc/dec node survives into the message.

Someone — me — observed the exact misquote, found a way around it to reach the
arm under test, wrote the workaround down, and moved on. That is the Bug
policy's named failure mode in as clean a form as it gets: *if you notice
yourself routing around a behavior, STOP.* The comment is now corrected to say
which of the two things it describes was a bug.

## Three renderers, two independent fixes, one still broken

DG asked for the sibling sites to be audited too. There are three functions
answering "quote this mutation back", all building on the same four primitives
in `ir.rs`, all dispatching separately:

| renderer | Assign | CompoundAssign | PostIncDec | BinOp | fallback |
|---|---|---|---|---|---|
| `arith_brief` | yes | yes | yes | recurses | `...` |
| `expansion_mutation_brief` | yes | yes | yes | no | `None` |
| `brief_arith` | yes | **no** | yes | no | `...` |

The middle one's doc comment records this bug being fixed *there*: "`Assign`
and `CompoundAssign` were once matched in a single pattern that printed
`x = ...` for both". `arith_brief` had its own version — fourteen operators
rendered as `?` — fixed when 31.63 pinned it. Neither fix reached
`brief_arith`, which still renders `mutation ...` for `(( x = (y -= 2) ))`.

The interesting number is not three renderers; it is **two independent
discoveries of the same defect that each fixed one copy.** A shared vocabulary
with duplicated dispatch gets fixed one site per bug report, forever.

DG: fix them now. `ir::arith_brief` is now the only renderer, total over all
seventeen `ArithExpr` variants, with the deliberate elisions listed explicitly
instead of reached through `_` — that `_` is how fourteen operators came to
print as `?`. `expansion_mutation_brief` keeps the *whether* question, which is
genuinely its own, and delegates the *how*.

Observable gain: `(( x = y -= 2 ))` said `mutation ...` and now says
`mutation y -= ...`. `Paren` is still elided by the unified renderer, so 31.106
and 31.63 are unchanged; rendering it faithfully is available and is a
diagnostic-text decision rather than part of a consolidation, so I left it.

## What consolidating found

The three renderers disagreed about one more thing, and it was not cosmetic.
Two had `PreIncDec => unreachable!(PREFIX_INCDEC_INVARIANT)`; `arith_brief`
had no arm at all and let it fall through to `...`, with a comment arguing
that was fine. `PREFIX_INCDEC_INVARIANT`'s own doc is titled *"Why every
downstream `PreIncDec` arm is an ICE"*, so the unified renderer takes the arm.

It fired immediately, on `(( y = 1 ? ++x : 0 ))`.

`reject_prefix_incdec` is the single function that makes the invariant true.
It walked the tree by hand — `BinOp`, `Assign`, `CompoundAssign`, `Comma`,
`Paren` — and ended in `_ => Ok(())`. So a prefix form inside a **ternary
branch**, a **unary operand**, or the **value of a subscript assignment** was
never looked at. Ten shapes survived the parser and hit a downstream ICE:

```
(( 1 ? ++x : 0 ))    (( y = ++x ? 1 : 0 ))   (( y = 0 ? 1 : ++x ))
(( y = -(++x) ))     (( y = !++x ))          (( y = ~++x ))
(( a[0] = ++i ))     …and the $(( )) and `if` forms of the first
```

Not silently-working code — the language's worst outcome — but a compiler
crash for a program it means to reject with a one-line message.

I verified these were pre-existing rather than introduced by my new arm, by
stashing the change, rebuilding, and reproducing all ten. Identical. Worth the
four minutes: the alternative was reporting a bug I had just written.

The subscript half is a nice tell. `(( a[++i] = 5 ))` was *already* rejected
and `(( a[0] = ++i ))` was not — because the **index** is re-parsed through
`parse_arith_str` on its own and so gets the funnel a second time, while the
**value** only ever had the partial walk. Asymmetry between two halves of one
construct is what a hand-rolled traversal produces.

The fix is the one the function directly below it already prescribes.
`reject_nested_mutation` carries this exact lesson in its doc — "a local copy
used to stop at `BinOp` and `Paren` … `(( c = n ? (d++) : 1 ))` … parsed
clean", remedied by asking the canonical whole-tree predicate — and ends
"A partial re-derivation of a shared predicate is how an ICE gets built."
`reject_prefix_incdec` was a partial re-derivation of a shared predicate, in
the same file, thirty lines above the sentence describing it, and it built an
ICE. It now delegates to `ArithExpr::children()`, which is total.

I also corrected `PREFIX_INCDEC_INVARIANT`'s doc, which flatly asserted "No
pass after the parser can receive one." **An invariant asserted in a doc
comment and enforced by a partial walk is not an invariant.** The doc now says
what makes it true and that it was false for ten shapes.

## Shields

28 specs. `31.90`-`31.94` pin the target-naming for every mutation spelling
including the cell form; `31.95`-`31.104` give each of the ten cold operators
its own spec, because one spec per arm is what proves the arm;
`31.105`-`31.106` pin the two audited siblings, one faithful and one
deliberately vague; `31.107`-`31.116` pin one `++x` shape each; `31.117` pins
the renderer the consolidation repaired.

`mk test` 51/27/20. Sweep **6107** (predicted, +2×28), 0 failing, 0 flips
against the 6051-row `58cae45` baseline. Three id checks clean; the
source-derived total agrees exactly.

## The shape of the whole thing

A cold-line query pointed at an operator table. The table was fine. Following
it out found a diagnostic that quoted a program nobody wrote, then a comment
that had recorded that bug as scenery, then three renderers that had each been
half-fixed, and finally an invariant the compiler believed and did not check.

None of that came from reading the cold line. It came from reading what the
probes printed on the way past.

---

## Addendum — DG collapses the family

I ended the above by reporting one loose end: `arith_brief` still elided
`Paren`, so `if (( y + (x *= 2) ))` read `mutation y + ...`. I framed it as a
narrow choice — render the parens or don't — and offered three options.

DG's answer went past the frame: **report the target of the mutation, e.g.
`x`; like the other error messages.**

That is the ruling he had already given for the decomposed site, applied where
it belongs: to all of it. I had treated `mutation of x` as a workaround for one
site that could not reconstruct its operator, and kept faithful quoting
everywhere else because those sites *could*. But capability was never the
question. A quotation invites the reader to compare it against their source,
and four of the five mutation sites could not produce one that matched:

| site | said | source |
|---|---|---|
| decomposed condition | `mutation of x` | anything |
| nested condition | `mutation y + ...` | `y + (x *= 2)` |
| `$(( )) `expansion | `mutation x *= ...` | `x *= 2` |
| nested in statement | `mutation ...` | `x = (y -= 2)` |
| non-final comma | `1 > x = ...` | `1 > x = 2` |

Three of those are wrong in different directions — one elides the culprit, one
quotes an operator the parser synthesised, one names nothing at all. Naming the
target makes all three unrepresentable rather than merely fixed. It also makes
two spellings of one program agree: `(( x = y -= 2 ))` and `(( x = (y -= 2) ))`
now report identically, which they never did.

The split that falls out is clean, and I had not seen it. `arith_brief`'s two
surviving callers both quote a **non**-mutation — a bare comparison, and a
comma part that was required to be a mutation and isn't. There the expression
*is* the complaint, so quoting it is the message. Mutation sites name;
non-mutation sites quote. One question, asked once, decided both.

## The fifth site, and a fifth bug

Applying it exhaustively meant touching `reject_nested_mutation`, which I had
edited an hour earlier without noticing it was a mutation reporter at all. Its
body asked `arith_has_mutation` and then rendered with `arith_brief` — a
predicate and a renderer, derived separately, which is the exact shape of every
bug in this journal. Collapsing them into `mutation_target_in` (whether and
which, one traversal; `arith_has_mutation` is now `is_some()` on it) is what
put the outer `match` under my eyes:

```rust
ArithExpr::Assign { value, .. } | ArithExpr::CompoundAssign { value, .. } => …
```

`SubscriptAssign` is not in that list. So `(( a[0] = y++ ))` fell past this
gate to `check_arith_value_mutation` — which exists for the `$(( … ))` value
path, and says so:

```
$ sash -c '(( a[0] = y++ ))'
[SB4020] an assignment or `++`/`--` inside `$(( … ))` can't take effect
```

There is no `$((` in that program. The scalar form one line away —
`(( x = y++ ))` — named `y` correctly the whole time. **Two halves of one
construct disagreeing is the tell**, and it is the *second* time in two days
the subscript half was the one missing: the prefix-`++` walk covered a
subscript assignment's index and not its value.

Verified pre-existing by rebuilding the stashed tree, as before.

This is the **third** partial re-derivation of a tree walk in `parse/arith.rs`,
and `reject_nested_mutation`'s own doc comment — thirty lines above the hole —
prescribes the remedy for the second. Writing down the lesson does not apply
it.

One nuance worth keeping: the outer `match` stays *selective*. It names the
nodes that evaluate a sub-expression as an rvalue, which is not "every node" —
descending blindly would reject `(( (a=2) ))`, a form the transform hoists and
DD#31 allows. Totality is the goal for a *renderer*; for a *gate* the goal is
that the list be complete for the property it names.

## What the ruling cost, and what it found

23 spec ids × 2 modes went red, all of them mine from the previous batch, all
pinning text that quoted an expression. Rewriting them surfaced something the
green sweep had hidden: with the condition site retargeted, `arith_brief`'s
three mutation arms dropped to **zero hits across all 6107 tests**.

That is the third time these arms have been measured, and the third time the
answer was "live, but only from one caller". The first two measurements called
them dead. This time I had the discipline to distrust a zero — a full green
sweep proves *not reached by these inputs*, not *unreachable* — and went
looking for a shape. `classify_arith_expr` recurses only into `Comma`, so a
comparison `BinOp` can still hold a mutation operand; the **non-final comma**
part reaches all three arms, for every operator. The bare-statement site
reaches only the comparison-classified ones, so `(( 1 - x = 2 ))` compiles
clean — had I rewritten the ten operator specs to use it, four would have
silently stopped testing anything.

**A shape that reaches a cold arm is not a property of the arm. It is a
property of its callers**, and it expires when a caller changes.

## Shields

11 more specs, `31.118`-`31.128`: the `--` direction at the site that still
quotes; the `SubscriptAssign` bug with its scalar control, its compound
spelling, a cell named as the culprit (`b[…]`), a regression shield that a
clean cell assignment still compiles and runs, and the borrowed message still
belonging to the construct that earned it; then the ternary, the loop-increment
idiom, and the two-paths-one-sentence case. `31.63`, `31.69`-`31.74`,
`31.95`-`31.106`, `31.117`, `cov.150`, `cov.151` and `cov.247` rewritten.

`mk test` 51/27/20. Sweep **6129** (predicted, +2×11), 0 failing, 0 flips
against the `d4d13e3` baseline. Three id checks clean; source-derived total
agrees exactly.

## The shape of the addendum

I asked a narrow question about parentheses. The answer was a rule, and the
rule had four more consequences than my question did. Applying it end-to-end
found a fifth mutation site I had edited without recognising, a fifth bug
underneath it, and a set of arms that a green sweep had quietly stopped
covering.

Asking the narrow version of a question gets the narrow version of an answer.
DG gave the general one anyway.
