# 430 — the twin that was never made total

*2026-09-27*

Second item off the coverage queue: `test_uses_tainted`, 7 cold lines in
`validate.rs` — and unusually, *every* line cold. A whole function nobody
calls. The queue entry was a coverage question. It turned into a taint escape.

## What the cold lines meant

`test_uses_tainted` is reached from one place: `expr_uses_tainted`'s
`ExprKind::Test` arm, which is cold too. Two neighbouring arms are also cold —
`AssocArray` and `SlurpFile`.

`expr_uses_tainted` is one half of SB5021's test. `reject_tainted_into_array`
asks:

```rust
if is_taint_source(e) || expr_uses_tainted(e, tainted) {
```

Two walkers, read as a pair. Between them they have to cover the whole tree.

The probes said:

- `Test` — unreachable. `[[ … ]]` in an array element, a map value, or an
  `a[i]=` RHS is SB4014 at parse. It never becomes a value.
- `AssocArray` — the caller destructures a map literal itself and passes each
  value, so this arm never sees one.
- `SlurpFile` — reachable, and the way it is reachable is the finding.

`array a=( $(<f) )` does *not* reach it: `is_taint_source` answers true first
and short-circuits the `||`. To reach it you have to hide the slurp somewhere
`is_taint_source` does not look. There is such a place:

```sh
array a=( ${x:-$(</etc/passwd)} )
```

`is_taint_source` has no `ParameterExpansion` arm. It ends in `_ => false`.

## The escape

So the file contents are invisible to both walkers. `is_taint_source` never
arrives. `expr_uses_tainted` arrives — and answers the wrong question, because
it is asking "does this read a tainted *variable*", and its
`CommandSubstitution` arm deliberately answers `false`.

```sh
str x=""
array a=(q)
a[0]=${x:-$(</tmp/secret)}
echo "${a[0]}"
```

Compiles clean. Prints the file. No SB5021, no SB5020, and not even the SSA
backstop.

An array *literal* did at least trip the backstop, but only because `echo` is
a command argument. Journal 400's note in this very file records that a
`[[ ]]` operand is not one — so:

```sh
array a=( ${x:-$(</tmp/secret)} )
if [[ ${a[0]} == 4242 ]] { echo "leaked" }
```

compiles silently and compares the file contents. Same for `$(getenv …)` and
for an external capture. A tainted **variable** in the identical position —
`${x:-$t}` — was SB5021 the whole time. That asymmetry is what makes this a
hole and not a policy.

## The shape of it

`expr_uses_tainted` carries a long comment explaining that it is **total over
`ExprKind` on purpose** and that a wildcard leaked three times. It was made
total in journal 400.

Its twin was not. The lesson was recorded on one of the two functions that
have to be read together, and the other one kept the `_ => false` the comment
is about.

A third walker in the same family, `check_value_cmdsub_reads`, had the same
`_ => {}`.

That is really the finding. Not "`is_taint_source` was missing an arm" — that
is the symptom. The invariant is *between* the walkers, no single function
owns it, and so a fix applied to one of them looked complete.

## The second hole

Building the differential that proved the first one — the same RHS expression
assigned to a scalar, to `a[0]`, and to an array literal — turned up another
row:

```sh
array b=(one two)
str z=${b[$(getenv HOME)]}     # clean
echo "${b[$(getenv HOME)]}"    # SB5020
```

Same expression, same read, two answers. `reject_scalar_rhs_read`'s subscript
arm carries the comment *"DD#24: the container AND the index are both reads"*
and then checks the index with the **value** walker, which treats a bare
source as sanctioned assignment-from-source. Right for the RHS. Wrong for an
index. The code contradicted its own comment.

The first draft of that differential was worthless, incidentally: the
scaffolding did `echo "$z$t"` to keep `t` alive against SB5050, and `$t` is
tainted, so every row reported SB5020 for the scaffolding. Nine rows of
identical, meaningless agreement. Per-row preludes fixed it.

## The fix

Three arms, one principle: a walker in this family is total, and the reason
goes in the doc comment rather than in a journal nobody re-reads.

- `is_taint_source` descends into a `:-` default (DD#78 — the default is part
  of the value) and into arithmetic, and now spells out every variant.
- `reject_scalar_rhs_read` checks a subscript index with the read walker.
- `check_value_cmdsub_reads` does the same for a collection element, and is
  total.

`Subscript` stays `false` in `is_taint_source` on purpose: `z=${b[$(getenv
N)]}` stores `b`'s element, never the env var, so answering true would taint
`z` with data it does not receive. The index is a read, and reads belong to
`check_expr_taint`.

`Test` stays a correct recursive answer rather than becoming an ICE, which is
the opposite of what I did to a dead arm yesterday (journal 429). The
difference is what guarantees the deadness. There, SB5046 rejected the shape a
pass earlier, so reaching the arm meant a validate rule had stopped applying —
a compiler bug. Here nothing guarantees it; `Test` is merely a shape the
parser does not currently produce in a value position. An ICE would convert a
future legitimate program into a crash, and this whole bug is a demonstration
that arms in these walkers become reachable.

## `!`

One thing I did not decide. `${x:!-$(getenv HOME)}` — does DD#91's bypass
vouch for a *fresh source* materialised in the default, or only for the read
of `x`?

Today it is rejected, but only by the SSA backstop: a bare unspanned message,
because the IR layer honours `taint_off` and the SSA layer does not know about
`!`. The two layers disagree and the user gets the worse diagnostic.

Mirroring `pexp_uses_tainted`'s `taint_off` check was the choice that leaves
that behaviour exactly as it was, deliberately, so the fix does not prejudge
the answer. Filed as `notes/tasks/dd91-bang-vouches-for-source-in-default.md`.

## Gate

Twelve specs, `61.1`-`61.12` (the `61.*` block was entirely free, and DD#61 is
the governing DD). Three are controls: a clean `:-` default must still work,
`${t:!}` must still be permitted, and a clean index must still resolve — so
`61.1` pins "a source may not enter" rather than "`:-` is banned here".

DD#78 already had specs for a source in a `:-` default. They assert the
**command-argument** read position, `echo ${x:-$(getenv HOME)}`. The rule was
specced at one site and absent at another, which is the same
one-of-a-pair-only pattern as the walkers.

Three disjoint neuters, each landing exactly its own specs: revert the pexp
descent → 7 failures (`61.1`-`61.7`); revert the scalar index → `61.10`;
revert the collection index → `61.11`. The three controls pass under all of
them.

Predicted 6613, swept 6613, zero failing, zero flips, 24 new ids.

`check_value_cmdsub_reads` came out at cc 13 / cog 9 and would have tripped
the gate, so the `${…}` and `$(( … ))` arms were split into helpers — the same
move, for the same stated reason, as `pexp_uses_tainted`. Now 10 / 5 against
11.52 / 5.58.

## The queue item

`test_uses_tainted`'s 7 lines are still cold, and should stay cold. That is
the answer: `ExprKind::Test` cannot appear in a value position. The arm earns
its place by being correct if that ever changes, which is the whole argument
the file already makes — the one its twin was not held to.
