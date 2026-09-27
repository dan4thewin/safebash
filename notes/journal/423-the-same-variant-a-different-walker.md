# 423 — The same variant, a different walker

The wildcard audit found what it was chartered to find. It is the fifth
escape of this shape in the taint pass, and it is the *same enum variant*
as one already fixed.

## The oracle

```sh
str h=$(getenv SECRET)
array z=($(( ${h:-0} )))
if [[ ${z[0]} == 4242 ]] { echo MATCH } else { echo NO }
```

Compiles clean. Runs.

```
SECRET=1      -> NO
SECRET=4242   -> MATCH
SECRET=9999   -> NO
```

Tainted environment content, read back out one comparison at a time, with
no diagnostic anywhere in the pipeline.

## Why it escaped

`journal 400` fixed `ExprKind::Arithmetic` falling through
`check_expr_taint`'s wildcard — `echo "$(( ${x:-0} ))"` was laundering a
tainted scalar. That fix made `check_expr_taint` total, and the charter for
this audit records it as closed.

But `check_expr_taint` does not handle collection elements. Its final arm
says so explicitly:

> `Array`/`AssocArray` *are* read positions, but a tainted element is
> rejected at the *assignment* by `reject_tainted_into_array` (SB5021),
> which names the collection and is the better diagnostic.

That is a sound design. It just means there is a **second** walker on the
path, `expr_uses_tainted`, and that one still ended:

```rust
ExprKind::AssocArray(pairs) => pairs.iter().any(|(_, v)| expr_uses_tainted(v, tainted)),
_ => false,
```

Same enum. Same variant. Different function. Making one walker total does
nothing whatsoever for its siblings — which is the entire argument for
spelling every variant out rather than trusting a wildcard to stay
harmless, and it is now recorded in the function's own doc comment.

## Why it was a bypass and not a late catch

`z` was never entered into the tainted set, so every downstream check saw a
clean array. Two different things then happened depending on where the
value went:

| use | result |
|---|---|
| `echo "${z[0]}"` | SSA taint backstop — stopped, but a bare sentence with no SB code and no span |
| `[[ ${z[0]} == … ]]` | **nothing** — compiled and ran |

The backstop checks command arguments. A `[[ ]]` operand is not one.

I checked whether that made the backstop an independent second bug. It does
not: a *genuinely* tainted array is SB5020 in the identical position —

```sh
read arr
if [[ ${arr[0]} == x ]] { … }      # SB5020
```

— so the validator covers `[[ ]]` fine. The only reason this program got
through is that `z` was never marked. One root cause, not two. Worth the
ten minutes it took to establish, because "the backstop has a hole too"
would have sent the fix to the wrong place.

## The fix

`expr_uses_tainted` is now total over `ExprKind`, with three arms that were
previously swallowed and two helpers mirroring the diagnostic-emitting
twins that already existed:

- `Arithmetic` → `arith_uses_tainted`, mirroring `check_arith_taint`. A
  disposition **converts**; it does not declassify (DD#24 §Quarantine, and
  DD#93 Part D is the form that does both, spelled so the declassification
  stays greppable).
- `SlurpFile` → the *filename* is a read. The contents are already covered,
  because `is_taint_source` answers true for the node and the sole caller
  tests that first.
- `Test` → `test_uses_tainted`, mirroring `check_test_taint`.
- `CommandSubstitution` → deliberately `false`, and now says why: a read
  inside `$( )` is a *use site* (SB5020), not a value flowing into a
  collection (SB5021), and the caller runs `check_value_cmdsub_reads` for
  exactly that. Answering `true` here would emit the wrong diagnostic.

`expr_uses_tainted` goes to cc 14 against a 3× bar of 11.52, and passes the
gate on the cog ≤ 2 dispatcher exemption (cog 2). That exemption exists for
precisely this shape: a wide flat dispatcher is not hard to read, whatever
its arm count. The helpers are cc 3 and cc 7.

## What the specs say

`atint.13`–`atint.16` in `taint.sh` (+8 dual-mode), sitting directly after
`atint.11` — journal 400's spec for the sibling bug — because that
adjacency is the finding.

`atint.16` is the one that is not about the bypass: clean arithmetic into a
collection must still compile and still compute (`array z=($(( n + 1 )))`
→ `6`). A taint arm that rejects everything passes 13–15 and is useless.

The neuter is clean in both directions: reverting the `Arithmetic` arm
fails exactly 13, 14 and 15, and leaves 16 passing.

Sweep **6540**, predicted 6540, zero failing, zero flips, eight new ids.

## Method

The audit's static enumeration *predicted this*. The task file's
expression-level cohort listed `check_value_cmdsub_reads` and four others
under "skipping `Arithmetic`", and named `SlurpFile` and `Arithmetic` as
the recurring blind spots before any of this was run. What the enumeration
could not say was which of the 31 candidate sites was live — that took the
container harness, 13 containers × 7 violating expressions, and the answer
was one square out of 91.

The other 90 came back clean, which is worth as much as the finding. The
stmt-level cohort was clean across 11 containers in the previous session.
The expression-level cohort is now clean too, with this one repaired.
