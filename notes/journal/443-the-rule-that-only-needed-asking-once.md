# 443 — the rule that only needed asking once

2026-09-28

I had left DG a question with three options. He answered with a rule that
made all three moot, and then the rule — applied honestly across every place
it touches — turned over three more bugs, two of which crashed.

## The question I should not have asked

`check_callee_arity` was counting `arg_values.len()`: the number of SSA
values, over values that expand at run time. A splat is one value that
becomes 0..n tokens; a `:~` elide is one value that becomes 0 or 1. So the
arity check was wrong in both directions at once — it accepted
`f one ${x:~}` into a two-argument callee, and it rejected `f @q` on a
two-element `q` with the message "called with 1", a number true of no
execution.

The filing offered DG three ways to handle the case where a call's runtime
range *overlaps* the callee's arity but the actual run lands outside it, and
argued at some length that the obvious fail-closed move — reject dynamic
counts into fixed-arity callees — was wrong because it "kills DD#84's own
motivating case".

DG:

> an elision, given it could remove an argument, can only work for a variadic
> context and not a fixed one. similarly, a splat should only work for
> variadic given it could be empty

I went back to DD#84 to write up the conflict, and there was no conflict. The
DD says an absent optional argument is one *"known at compile time"*. A `:~`
elide is by construction not known at compile time. The case I claimed DG's
rule would kill was never in the DD; I had invented it, written it down as
the reason his answer could not work, and built three options on top of it.

The tell was available before he answered. That paragraph was asserted from
reading the DD's *intent*, not from executing anything or quoting its text —
and the very DD I was writing against has an addendum of its own recording
the same failure ("that was asserted from reading and is wrong — corrected by
execution"). The same file, three sections apart.

A second assertion in the same filing was flatly false: that bare `@a` "is
rejected earlier (SB5030/SB5032), so it does not arise". Bare `@a` is the
*common* spelling at a call site and spec sp.1 has gated it since September.
One probe would have caught it.

## What the rule cost

One spec. Out of 6865.

That number is the whole argument for the rule. I had reasoned my way to
"this would break a documented feature" about a change whose measured blast
radius was a single test — which I could have measured in ten minutes, before
writing the objection, and did measure within ten minutes of accepting it.

## The three bugs underneath

Implementing the rule meant knowing what a splat lowers to at every call
position. Four positions: plain statement, pipeline stage, capture into a
declaration, capture into an assignment or an argument.

Three of them did not lower it at all.

**The pipeline stage joined.** `f @two` gives two arguments; `f @two | cat`
gave one, with `"a b"` and `"c"` fused into `a b c` and the boundary gone.
This is `bug-splat-to-user-function-joins`, filed and fixed on 2026-09-10,
marked `status: done` — in a third position its fix never visited. That fix
had said "there are two argument lowerings" and converted both call sites it
knew about. The complete statement is not a count, it is a property: every
position that lowers a call's arguments must use `arg_to_value`. Grepping for
the *wrong* function finds the fourth site immediately. Re-running the
reproducer finds nothing, because the reproducer has no pipe in it.

Journal 441 called this pattern "the second implementation". This was the
third, of the same function, found the same way.

**`=name` reached its slot on one path in four.** `split_call_args` — the
function that binds a DD#100 collection parameter to `A`–`E` — had exactly
one caller. Everywhere else `=q` was lowered as an ordinary scalar argument:
the slot was never bound, `len A` read 0, and argc was off by one. In a
pipeline that is a silent wrong answer. In a string capture it **segfaults**.
In an `-> int` capture it is an **ICE: undefined arrlen: A**.

I nearly missed all of it. The capture paths reported SB5050 — the
dead-variable gate — on `q`, and I read that as the diagnostic. It is not; it
runs last and masks whatever is underneath, which is written down in this
repo's own memory and which I have now been caught by twice. Adding an
independent read of `q` cleared the mask and showed `len A=0` sitting there.

**Two validate scans did not descend into `$( )`.** The same audit: the
SB5050 false positive was `collect_slot_arg_reads` not looking inside a
capture, and — worse — `check_call_args_in` did not either, so DD#100's
scalars-first rule, the slot-count limit and the slot-operand rules were *not
checked at all* inside a capture. `str t=$(f =q "x")` compiled silently.

**`: @args` promoted optional arguments to required.** Found by the rule
directly. `analyze_arity`'s variadic branch computed its floor from
`required ∪ optional`; the fixed branch correctly used `required` alone. So
declaring a function variadic — DD#81's documented way to say "any number of
arguments" — made a call that had been legal illegal:

```sash
fn f {           echo "[${args[0]:-}]" };  f    # fine
fn f { : @args;  echo "[${args[0]:-}]" };  f    # SB5042
```

The marker that widens arity was narrowing it.

## What the rule is

A splat or an elide may go only where the callee takes any number, and only
when the call's *minimum* meets that callee's floor. SB5087. Everything
static keeps its existing SB5042/SB5043 message verbatim, so the change is
invisible to a program that does not use a dynamic count.

The floor half is my extension of DG's sentence — an empty splat into a
variadic callee that reads `${args[0]}` unconditionally starves it — and I
flagged it to him as applied-rather-than-asked, since it is his rule carried
to its consequence rather than a second decision. It is the one point here
still awaiting a veto.

## The half I got wrong anyway

I flagged the floor as my extension and invited a veto. DG did not veto it —
he tightened it:

> a function can have both a fixed portion and a variadic one with the fixed
> met by discrete arguments as though there were no variadic component

I had implemented the floor by summing per-argument minimums, which sounds
like the same thing and is not. `f @e x` against a callee reading
`${args[0]}` sums to `0 + 1`, reaches the floor of 1, and is accepted — and
then `${args[0]}` is `x` when `e` is empty and `e[0]` when it is not.

That is not a missing diagnostic. It is a fixed read silently taking a
different variable's value depending on an array's length, which is the same
class as every other bug in this session, sitting inside the fix for them.

The floor is about *positions*. A statically-one argument covers its own; a
variable-length one covers what its minimum guarantees and then nothing
after it is knowable, because a run-time length shifts everything behind it.
His grant survives — `@{e:-fallback}` has a minimum of one, so it covers
position 0 — and `f @{e:-fallback} y` against a floor of 2 does not, because
`y`'s position moves with `e`.

Cost against the suite: zero specs. The required arguments go before the
splat, which is how anyone would write it anyway.

Twice in one session I reached for a count where the thing being counted was
positional. The first time it was `arg_values.len()`. The second time it was
my own replacement for it.

There is a tidiness to how it lands. DD#84 listed three positions where `:~`
is legal — an external command's argv, a varargs builtin, an array literal
element — as a closed list. All three are variadic contexts. A user-function
call is the fourth, and is elidable exactly when the callee is variadic. The
closed list was never the rule; it was the rule minus the one position that
had an arity to check against.

## Ledger

6874 passing, 13 skipped, zero failing — 6887 rows, exactly the predicted
total. `mk test` clean. Specs: `optional_args.sh` 84.53–84.67 (the rule),
`collparam.sh` 100.38–100.45 (the four call paths, the segfault, the ICE, the
capture descent), `functions.sh` sp.9–sp.11 (the pipeline splat), sp.3
migrated.

Four bugs, one of them a crash and one an ICE, all reached from a question I
had answered wrongly in writing before asking it. The rule was one sentence.
