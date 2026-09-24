title: 400 — a disposition converts; it does not declassify

DG read the spec block I shipped in 399 and asked one question:

> this doesn't seem right - you read tainted x to assign n and then attest n?

He was right, and the shape was not merely odd. It was a taint-quarantine
bypass, and my specs had just pinned it as correct behaviour.

## What the specs were resting on

Journal 399's `atint` block needed a tainted int. The shape I found by
experiment was

```sh
x=$(arg 1)
int n=$(( ${x:-0} ))
attest n [[ -n $n ]]
```

and I convinced myself it was idiomatic — parse, then validate the parsed
value. I checked that dropping the `attest` made the program fail, concluded
the attest was load-bearing, and stopped. What I never asked was why the read
of `x` on line 2 was allowed at all.

DD#24 §Quarantine answers it in a sentence:

> Every operation other than direct assignment-from-source counts as a read
> and is rejected:
> - `$x`, `${x}`, `${x:op}`, `${x[i]}` **in any expression**

`${x:-0}` is `${x:op}`, and `$(( … ))` is an expression. The program should
never have compiled. The validator says so itself elsewhere, in the message
for the string case:

```
tainted variable $x can't be copied — DD#24 has no taint propagation;
`attest x [[ … ]]` at the source, then read it
```

`y=$x` gets that. `int n=$(( ${x:-0} ))` got nothing.

## Two bugs

**Bug A — a validating conversion that did not validate.** Measured against
the release binary, with a *clean* variable, so taint is not involved at all:

| program | printed | should print |
|---|---|---|
| `x=abc; echo "$(( ${x:-7} ))"` | `abc` | `7` |
| `x=abc; echo "$(( (${x:-7}) ))"` | `abc` | `7` |
| `x=abc; echo "$(( ${x:?no} ))"` | `abc` | abort |
| `x=abc; echo "$(( ${x:-7} + 0 ))"` | `7` | `7` |
| `x=abc; int n=$(( ${x:-7} ))` | `7` | `7` |

`arith_leaf_to_ssa` hands an `ArithExpr::Expansion` back as the string it is.
Every other position forces the conversion afterwards — an operator lowers its
operands as ints, an int assignment types the destination. A lone leaf in
*value* position had neither, so the string went to the sink verbatim. The
`:?` row is the sharpest: the one disposition whose entire job is to fail on a
non-integer, silently didn't.

**Bug B — the taint bypass.** `validate::check_expr_taint` had no
`ExprKind::Arithmetic` arm; reads inside `$(( … ))` fell through `_ => {}`.
Combined with Bug A:

```sh
x=$(arg 1); echo "$(( ${x:-0} ))"     # compiled clean, printed argv verbatim
```

No SB5020, no SSA backstop, no diagnostic of any kind, on both backends. Not
even coerced to an integer — Bug A meant arbitrary bytes reached `echo`.

## The same omission, four times, in one pass

The fall-through is not a one-off. `check_expr_taint`'s own comments already
record two previous instances:

- `[[ … ]]` operands — *"These were previously unchecked (the arm was
  missing)"*
- `$(<file)` — *"the `_` fall-through below made `[[ $(<f) = x ]]` silently
  LEGAL"*

This session added two more: `ExprKind::Arithmetic` in `check_expr_taint`,
and `StmtKind::ArithTest` in `taint_stmt`, where `_ => taint_child_bodies`
walked child bodies an arith test does not have — so `if (( ${x:-0} > 5 ))`
never checked its condition.

Then a third site, found only because the first fix did not close the hole:
`reject_scalar_rhs_read` had the same `_ => {}`, so an *assignment's*
arithmetic RHS was still not a rejected copy. The target silently became
tainted instead, and `attest n [[ … ]]` then declassified a value nothing had
vouched for. Laundering with a receipt. That one mattered most — it is the
literal shape of the 399 spec.

DG asked for a structural guard and it is in: `check_expr_taint` and
`reject_scalar_rhs_read` are now **total over `ExprKind`**, with every
no-op arm carrying the reason it is a no-op. A new variant is a compile
error, not the fifth silent escape.

## What settled the design question

I asked DG whether the disposition should *declassify* — it is a checked
parse, and a checked parse is what a declassifier is. He replied:

> what did dd93 decide related to this?

DD#93 settles it twice, and I should have read it before asking:

- Its unifying constraint is *"the one DG defended when he **rejected** 'make
  every arithmetic result clean'"*. My option was that proposal in narrower
  clothing.
- Part D exists **because** the disposition does not declassify. It completes
  DD#77's split in three verbs: *"`i?` validates-and-branches; the
  disposition **converts** inline; and now `attest … (( n = s ))`
  validates-converts-**declassifies** in one accountable step."*

And the addendum had already ruled on this exact laundering *inside* an attest
predicate — `attest x (( y = 0*x + z ))` is SB5020, *"z would be laundered
into clean y"*. If it is rejected inside the accountable form it cannot be
legal outside one.

No new DD. The DDs were right; the implementation had a hole.

## A consequence worth recording

After the fix, **no int variable can be tainted**. Every route is refused
first: `int n=$(getenv X)`/`$(arg N)`/`$(cmd)` are SB4030 (string→int needs a
disposition), the disposition route is now SB5020, and `read` targets an array
(DD#99). So `attest n [[ … ]]` on an int is **validation-only** — a runtime
gate, exactly like DD#93's map 2-binder form and for the same stated reason:
attest is the only assert-with-diagnostic in the language.

Which makes journal 399's fix the whole remaining point of the int form. The
message it corrected — `attest: n = '9' failed the gate` instead of a bare
`attest: n` — is now the only thing that form produces.

`check_arith_taint` carries this reasoning as a comment, naming itself as the
function that needs an extra arm if a tainted int ever becomes constructible.
A green sweep proves "not reached by these inputs", never "unreachable".

## Method note

Bug A was found by probing *behaviour*, not by reading code. The cold line in
`emit_attest` (journal 399) was a signpost to an untested feature; writing the
test for that feature forced a shape nobody had run; DG reading the shape
found the bypass. Three sessions running, the cold line has been the signpost
rather than the destination.

The part I got wrong is worth naming plainly. I verified the attest was
load-bearing and treated that as verifying the *program* — but "the compiler
rejects this without the attest" only says something is being caught, not that
what is being caught is what should be caught. The backstop firing where
SB5020 should have fired was visible in that first probe and I read past it.
An ICE-shaped message with no source location, where a located diagnostic
exists for the equivalent string case, was the tell.

## Blast radius

Fourteen `coverage.sh` specs used the laundering shape as a convenient way to
obtain a tainted int for an `attest n (( … ))` predicate — their subjects
(SB4020 walk widening, attest-predicate read collection) have nothing to do
with taint. Six failed outright; the preamble is now the sanctioned
`str s=$(arg 1); attest s i?; int n=$(( ${s:-0} ))`, with a comment saying the
attest is load-bearing so it is not stripped later to shorten the line.

## Gates

Revert proof, both fixes reverted together: predicted 9 native failures and
got exactly 9 — `adisp.1/2/4/5` and `atint.1/2/9/10/11`. `adisp.3` (`x=5`
passes through as the same digits either way), `adisp.6` (has an operator, so
it always converted) and `atint.12` (a subscript index, which the existing
`Subscript` arm already reached) pass on the reverted build by construction;
they are guards, not regression proofs, and are kept as such.

Sweep 6217 → **6237**, exactly as predicted (+4 taint, +6 arith, both
dual-mode). 0 failing, 0 flips vs `625843d`. `mk test` 52/27/20.
