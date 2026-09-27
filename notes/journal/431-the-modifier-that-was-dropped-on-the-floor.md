# 431 — the modifier that was dropped on the floor

*2026-09-27*

Journal 430 closed a taint escape and left one question filed rather than
answered: in `${x:!-$(getenv HOME)}`, does the `!` vouch for the *source in the
default*? The fix deliberately preserved the existing behaviour so it would not
prejudge DG. He answered:

> "x=$(getenv A); y=$(getenv B); z=$x:!-$y:! ; the `!` should only apply to the
> read of the variable's stored value"

That is reading 2 of the two the task file recorded. It is also, read for all
its consequences rather than the narrow case quoted, a rule that runs in both
directions: each `!` is scoped to its own read, so a `!` in the default does not
vouch for the subject either.

## The ruling made his own example a bug report

The first thing to do with a ruling is check what currently happens. Eight
spellings, each probed so the variable under test is read *only* through the
expression being tested — a probe that reads a tainted variable in its
scaffolding reports the scaffolding's SB5020 for every row, which is how the
first differential in journal 430 came out worthless.

| expression | the rule says | actual |
|---|---|---|
| `${x:!}` | clean | clean |
| `${x:-$y}` | reject | SB5020 |
| `${x:!-$y}` | reject (`y` unvouched) | bare backstop |
| **`$x:!-$y:!`** | **clean — both vouched** | **bare backstop** |

The last row is DG's own example, and it did not compile. Not "compiled with a
worse diagnostic" — did not compile.

## Where the `!` went

`parameter_expansion_to_value` wrapped the subject in `SsaValue::Trusted` inside
the `let Some(op) = … else` arm. That is the **no-operator** path. The moment
there was an operator, control passed to `source_carrying_value`, which builds
`DefaultValue { source, default }` from `pe` — and never looks at `taint_off`.

So the `!` of every `${x:!-…}` reached neither backend nor backstop. The subject
stayed tainted, `DefaultValue` is tainted if either child is, and the SSA
backstop rejected the whole thing without a span.

The fix is one move, and the rule dictates it: wrap the subject *before* the
operator dispatch, because the subject **is** "the read of the variable's stored
value". Whatever disposition follows is a separate question.

## The mirror image, three times, in the validator

Codegen had dropped the flag; validate had over-applied it. Three sites treated
`taint_off` as covering the entire expansion:

- `is_taint_source` — `!pe.taint_off && pe.operand…`, so a `getenv` in the
  default became invisible as a source;
- `pexp_uses_tainted` — an early `return false` that skipped the operand walk
  along with the subject;
- `check_param_expansion_taint` — an early `return` that skipped the `:-`
  operand walk along with the `push_taint_use`.

All three now gate only the subject's own read. That is a single sentence of
policy — "the `!` covers `pe.name`, never the operand" — expressed in four
places, which is exactly the shape that leaked in journal 430. Noted as such in
each.

## A fourth site, which was mine

Wrapping the subject broke something that had nothing to do with taint.
`source_is_int` decides whether `${n:-7}` takes the int identity-element ternary
or the string `atoi_or` path, and it decides **structurally**:

```rust
match source {
    SsaValue::Var(_) => …,
    SsaValue::Subscript { is_integer_value, .. } => *is_integer_value,
    _ => false,
}
```

A `Trusted` wrapper falls to `_ => false`. So my own fix silently moved every
`${n:!-…}` on an int off the int path. `!` is a taint annotation, not a change
of type, so `source_is_int` now sees through the wrapper.

**A set, non-zero int cannot detect this.** `int n=42; ${n:!-7}` gives 42 down
either path — the string path atoi's "42". The probe has to be the identity
element: at `n=0` the int ternary yields the default and the string path yields
0. The first version of that spec used 42 and was vacuous; the neuter matrix is
what said so.

## Five fixes, five neuters

Each fix reverted independently, rebuilt, and run against the two affected spec
files. The first pass:

```
N1 transform wrap          lands 4 specs
N2 source_is_int           lands NOTHING
N3 is_taint_source         lands NOTHING
N4 pexp_uses_tainted       lands NOTHING
N5 check_param_expansion   lands 1 spec
```

Three of five changes were unproven, and the reasons differ. N2 was the vacuous
42 above. N3 and N4 were **masked by N5**: in scalar position
`check_param_expansion_taint` rejects first, so neutering either walker of the
`reject_tainted_into_array` pair changed no observable behaviour. They needed
collection position — `array a=( ${h:!-$p} )` — where that pair is the only
thing deciding. And N3 needed a *source* rather than a tainted variable, because
`is_taint_source(e) || expr_uses_tainted(e, …)` short-circuits: a top-level
source never reaches the second walker. That is the same short-circuit that made
`SlurpFile` look cold in journal 430.

After adding those three shapes, all five neuters land disjoint specs and the
baseline is clean.

## The spelling DG wrote still does not work, and that is his call

With all five fixed, `${x:!-${y:!}}` and `$x:!-${y:!}` compile and print the
right value. `$x:!-$y:!` — the exact spelling in the ruling — still fails, and
the taint system is no longer why.

The parser reads the inner `$y:!` inside the operand as `$y` plus the **literal
text `:!`**. That is DD#90 rule 5 ("inside double quotes the disposition must be
braced") reaching a disposition's operand, which the DD notes holds "by
construction" because the operand is parsed as an interpolating parts list.

Two consequences, and only one of them is loud. With a tainted operand you get
SB5020 blaming `y` for precisely what the author tried to vouch for. With a
clean one it is silent: `str x=""; str y=abc; z=$x:-$y:!` prints `abc:!`.

Put to DG as a genuine fork — extend brace-less into the operand, or leave rule 5
covering it. He chose to leave it:

> "A - I'd actually use attest, so avoid over-indexing on taint-bypass
> ergonomics."

Which is the right instinct about a bypass: `:!` is the audit surface DD#91
calls the adversarial review's highest-priority target, and making it more
ergonomic to reach for is not an improvement. So the brace-less-in-operand
behaviour is now **pinned deliberately** — vm.bl24 asserts the literal `abc:!`,
vm.bl25 the braced `abc`, as a pair so the two readings cannot drift apart. A
behaviour nobody has written down is indistinguishable from a bug the next time
someone finds it.

## Specs

`taint.sh` 91.24-91.33 (the DD#91 block, not the 61.x one journal 430 opened —
these are about what `!` scopes over), `var_modifiers.sh` vm.bl24-bl25.

The id-collision check earned its keep again: `vm.bl19`/`bl20` looked free from
the ids I was writing, and are not — the bl block already runs to bl23, serving
a cold-branch investigation into the operand's `$name` scanner.

Sweep: predicted 6637, got 6637. Zero failing, zero flips.
