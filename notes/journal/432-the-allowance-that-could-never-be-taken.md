# 432 — the allowance that could never be taken

DG picked the coverage queue back up, and the head of it was
`lower_value_as_str_aggregate` in `emit_c.rs`: six cold lines, all of them
ICE panics. That is a dull finding — deliberate should-never-run guards, the
kind the ranking is supposed to surface and you are supposed to wave past.

The interesting question was not "why are these cold" but "is any of them
actually *reachable*", since validator bypass is the audit's standing top
priority. Most of the arms held up. `:~` and `@a` are guarded by SB5046 in
every value position I could reach, including inside a `:-` operand — and the
splat cases turned out to be literal text that matches bash exactly, DD#90
rule 5 doing its job.

Then `str z=${y:-$(myfunc)}` panicked.

## One wildcard, fifteen missed variants

`lift_int_func_calls` walks a value and materializes every function call or
command-substitution capture into a temp, because an unlifted `FuncCall`
reaching codegen is an ICE by construction. It handled six of `SsaValue`'s
thirty-one variants — `IntFuncCall`, `FuncCall`, `CmdSubstBody`, `ArithBinOp`,
`Ternary`, `Concat` — and ended:

```rust
other => other, // no IntFuncCall inside
```

The comment is the whole bug. `other` is not "no IntFuncCall inside", it is
"fifteen more variants I did not think about", and about fifteen of them own
sub-values. So every one of these crashed the compiler:

```sh
array a=( $(f) )            # IndexedArray
map m=([k]=$(f))            # AssocArray
str z=${arr[$(f)]}          # Subscript.index
str z=${y:-$(f)}            # DefaultValue.default
a[0]=${y:-$(f)}             # ...and the subscript-assign path
```

Every one is a valid bash program. `str z=$(f)` — the same capture one level
up, not nested in anything — worked fine, which is exactly why this survived:
the control case is the one everybody writes.

This is the third time in three sessions that a non-total match over an IR
enum has been the bug. Journal 430 was the taint walker pair; journal 431 was
the `Trusted` wrap that only happened in the no-operator arm. The remedy is
the same one the codebase already applies by convention: make the match total
and say so in the doc comment, so the compiler enforces it rather than a
reviewer. `lift_int_func_calls` is now total over `SsaValue` with no wildcard
— cc 26, cog 1, comfortably inside the dispatcher exemption.

## The part that needed DG

Making the walk total is mechanical for fourteen of the fifteen. `DefaultValue`
is not, because `:-` is **lazy**: bash evaluates the operand only when the
fallback fires.

```sh
$ bash -c 'f() { echo RAN >&2; echo hi; }; y=set; z=${y:-$(f)}; echo "$z"'
set                     # no RAN
```

Recursing into the operand the same way as everywhere else hoists the capture
into the enclosing block, so `f` runs unconditionally: right value, wrong side
effects. That is the same class of divergence the `native-ternary-eager-eval`
task closed for arithmetic, and re-opening it deliberately seemed like a poor
trade. I put three options to DG — lift eagerly anyway, lower lazily with real
control flow, or reject captures in a disposition operand — and he took the
second.

## DD#78 was already there, and its premise was wrong

What I had not found when I asked — and should have, since "read
`notes/decisions/` before changing behavior that might be deliberate" is the
standing rule — was **DD#78**, accepted, titled "disposition-default discipline
(clean, side-effect-free)". It chose eager evaluation *on purpose*:

> The clean-and-non-`random` rule makes eager evaluation **safe**: a clean,
> stateless default has no observable effect regardless of which arm is chosen,
> so eager == lazy for every legal default.

That argument is sound for literals, clean reads, `$(( … ))` and the stateless
intrinsics. But the allowed list ends with **user functions**, and a user
function is not side-effect-free in any sense the compiler enforces. `f() {
echo RAN >&2; echo hi; }` is a legal DD#78 default that breaks the DD's own
premise.

The reason nobody noticed is the good part: **the compiler ICEd on exactly that
construct**, so the allowance could never be taken. DD#78 permitted user
functions in a disposition default, the eager-vs-lazy question turned entirely
on them, and no program that would have exercised it ever compiled. Grepping
the suite confirms it — the only specs putting a user-function capture in a
`:-` default are the twelve added today. A clause can sit accepted for six
weeks and be dead letter because a crash was standing in front of it.

So DG's ruling is recorded as an amendment rather than a reversal. Defaults
without a capture stay eager, which is every operand the original reasoning
actually contemplated; only a capture-bearing operand gets control flow. And
because the laziness is built in *transform* — bind the source to a temp, test
it, re-bind from the lifted operand in the taken block, merge through a phi —
neither backend grew a special case, so the cost DD#78 wanted to avoid
("keeping eager avoids per-backend branching") is not paid after all.

## Twelve specs, six neuters

`lift.1`-`lift.12` in `spec/functions.sh`, all verified against bash first.
The neuter matrix mattered again: six separate neuters, each landing a disjoint
set, together covering all twelve.

| neuter                         | lands                          |
|--------------------------------|--------------------------------|
| `IndexedArray` arm             | lift.1, lift.12                |
| `AssocArray` arm               | lift.2                         |
| `Subscript.index` arm          | lift.3                         |
| `DefaultValue` arm             | lift.4-lift.10                 |
| `handle_subscript_assign` lift | lift.8, lift.11                |
| force-eager `lift_default_value` | lift.6                       |

`lift.6` is the only witness for the laziness itself, and it has to be — it is
the one program whose wrong answer is invisible in stdout. With the operand
hoisted it still prints `[set]`; only the untaken `f`'s stderr gives it away.
A `:-` laziness spec that checks stdout alone cannot fail.

`lift.5` failing under the `DefaultValue` neuter is worth a note too: with the
operand unlifted the program does not merely compute the wrong value, it fails
to *compile*, fallback taken or not. The ICE does not care which arm runs.

Sweep predicted 6661 and returned 6661. Zero failing, zero flips.

## A second bug, filed not fixed

Chasing whether `emit_c.rs`'s "DD#77" citation on `RequireValue` was a
mis-citation — DD#77 is titled "String→int conversion discipline in
arithmetic" — turned up that it is **correct**: DD#77 §"`:?` un-prune scope"
re-added scalar `:?` everywhere after DD#65 pruned it. The stale line is
DD#65's "`:?` is rejected for SCALAR `${var:?…}` only", which predates it. No
comments needed changing, which is the outcome worth having checked for.

But the probe that settled it showed something else. DD#78 scopes itself to
every disposition default and names `${x:?MESSAGE}` explicitly as a checked
read position. It is not one:

```
str h=$(getenv HOME); str z=${y:-$h}    ->  SB5020   (correct)
str h=$(getenv HOME); str z=${y:?$h}    ->  SB5050   (dead variable)
```

SB5050 is the tell: as far as the compiler is concerned `h` is written and
never read, because the message operand is not in the value graph at all. The
message is emitted as literal source text — sash prints `$(f)`, bash prints
`hi`. Not a taint leak, since nothing is ever expanded and so nothing can
escape; an oracle divergence and an unimplemented DD#78 clause. Filed as
`bug-require-message-not-interpolated` rather than folded in here, because it
is a different bug from the one this session set out to fix.

## What I would do differently

Read the DDs for the construct *before* framing the question, not after
implementing the answer. I presented DG a three-way fork on `:-` laziness
without mentioning that an accepted DD had already chosen one of the three.
He happened to overturn it, and the overturn is well-founded — but that was
luck, not method, and it is the second half of the rule in AGENTS.md that I
skipped: checking `notes/decisions/` is not only for when you think something
might be deliberate.
