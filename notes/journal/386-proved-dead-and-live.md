# 386 — Proved dead, and live

The task said the arms were dead. It did not say "cold", or "unreached by
the suite" — it said **proved**, and it showed its work: cold in union
coverage, then each arm body replaced with a `panic!`, then the full
sweep, then fourteen hand-written probes aimed at them. That is the
strongest reachability evidence this project has produced.

I nearly deleted them on that authority. DG had queued exactly that.

```
if (( 1 > x = 2 ))     → Assign arm
if (( 1 > x += 2 ))    → CompoundAssign arm
```

## The shape nobody probed

The fourteen probes are listed in the filing, which is the only reason the
gap was findable. Read together they share one property: every one puts the
mutation either at **top level** (`if (( x = 5 ))`, the loop headers) or
**inside parens** (`if (( (x += 1) ))`, `if (( y + (x *= 2) ))`).

Both route around the arms, for different reasons:

- Top level classifies as `ArithKind::Mutation`, so `lower_arith_command`
  decomposes it into statements *before* validation runs. The message the
  user sees comes from `validate_assignment_stmt`, four hundred lines
  earlier in the same file.
- Parenthesised arrives as `ArithExpr::Paren`, for which `arith_brief` has
  no arm at all, so it falls to `_` and prints `...`.

What reaches the arms is the mutation as the **unparenthesised right
operand** of a binary operator. `1 > x = 2`.

Nobody writes that. In bash `=` binds loosest and its left side must be an
lvalue, so bash refuses:

```
$ bash -c '(( 1 > x = 2 ))'
bash: ((: 1 > x = 2 : attempted assignment to non-variable
```

sash's parser groups it as `1 > (x = 2)` instead. So the shape that reaches
this code is one the reference implementation rejects as a syntax error —
which is precisely why it was never on anyone's list of things to try.

## Chasing that: is the precedence difference a bug?

It looked like one for about ten minutes. sash parses an expression bash
refuses, so somewhere there could be a program sash accepts and computes
differently.

There isn't. I probed eleven contexts — condition, bare statement, both
comma positions, all three `for`-header slots, `$(( ))`, a ternary arm, an
array subscript — and every one rejects: SB5007, SB4020, SB4041. sash
refuses everything bash refuses, by a different route and with a different
message. The divergence is real in the parser and invisible from outside.

Worth stating plainly because "the parse differs from bash" reads like a
bug and this one is not. The validator is standing where the grammar is
loose.

## The methodology finding, which is the real one

**Panic instrumentation plus a green sweep proves "not reached by these
inputs". It does not prove "unreachable".**

The sweep is 5900-odd programs, and every one of them was written by
someone writing ordinary sash. It is excellent evidence about the language
as used and says nothing whatsoever about a grammar corner no author would
enter. The filing collapsed that distinction into one word — "proved" — and
the word is what I acted on.

The honest form is the one the same filing used two paragraphs earlier:
*not reached by the suite plus these N probes*, with the probes enumerated.
That form is falsifiable by inspection, and it is what let me find the gap:
I read the fourteen, noticed they were all one shape, and went looking for
another.

I have now made this mistake's mirror image twice in three days — deleting
a `_` arm that was load-bearing (journal 384) and nearly deleting two arms
called dead. Both times the fix was to *measure differently*, not to
measure more.

## What the live arms were saying

With the arms confirmed live, their contents became a user-facing question
rather than a housekeeping one:

```
(( 1 > x *= 2 ))     →  mutation 1 > x ?=  ...
(( y + (x *= 2) ))   →  mutation y ? ...
$(( x *= 2 ))        →  mutation x = ...
```

`arith_brief` had arms for six of nineteen `IrArithOp` variants and two of
nineteen compound ones. Everything else rendered `?`. The third line is the
worst: the `$(( ))` site matched `Assign | CompoundAssign` in one pattern
and printed an `Assign`-shaped message for both, so a compound assignment
was quoted back with an operator the author never typed.

A brief is read as a **quotation**. Saying less than the source (`...`) is
honest; saying something *different* from the source is not.

DG chose totality (option A). `crate::ir::arith_op_str` is now total over
all nineteen operators, `compound_assign_brief` derives `*=` from it rather
than falling to `?=`, and the three messages read:

```
mutation 1 > x *= ...      mutation y + ...      mutation x *= ...
```

Spec **31.63 had pinned** `mutation y ? ...`, with a comment saying the pin
existed so that adding an arm would be a visible change rather than a
silent one. It worked exactly as designed: the change was visible, and the
spec is rewritten to pin the real operator.

## Four renderings of one string

`"x = ..."` was built independently at `validate.rs:2274`,
`validate.rs:1050`, `parse/arith.rs:188` and `parse/arith.rs:84`. That
duplication is why the dead-arm claim was so hard to check in the first
place: the message for `if (( x = 5 ))` is character-identical to the one
the "dead" arm would have produced, so reading the output tells you nothing
about which site produced it.

All four now call `crate::ir::assign_brief`. `emit_ir::arith_op` is total
over the same enum and is **not** a rival — it renders Janet keywords
(`:add`) for the DD#104 seam, a different output language.

## Numbers

Six specs — 31.69-31.74, reaching each arm and both `$(( ))` renderings —
plus 31.63 rewritten. Both files dual-mode, so **+12**. Predicted 5921,
swept 5921, 0 failing, **0 flips**. Units 51/27/20.

The three gates on the filing are met, but not one of them the way it was
written: the arms were not removed, the renderings were reconciled, and the
suite did move — by twelve, because the arms needed specs.
