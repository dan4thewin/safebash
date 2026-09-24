# 395 — `until` was `while`

The coverage queue's next stop was `type_check.rs::check_condition_type`,
eight cold lines that emit SB4030 — *"int function used in boolean context"*.
Reaching them took one probe. What the probe turned up on the way is the
subject of this entry, and it is not a coverage story.

## The anomaly

Mapping which boolean contexts actually reject an `-> int` function, I wrote
the obvious pair:

```sash
fn p { return 0 }
int n=0
while p { (( n = n + 1 )); if (( n > 2 )) { break } }
```

...and the same with `until`. Both ran **zero iterations**. That cannot be
true of any correct compiler: `while p` and `until p` over the same predicate
must disagree. One of them was wrong and I did not yet know which.

A truth table, built by execution against bash rather than by reading:

| form | sash | bash |
|---|---|---|
| `while p`, `p` returns 0 | 3 | 3 ✓ |
| `while p`, `p` returns 1 | 0 | 0 ✓ |
| `until p`, `p` returns 0 | **3** | 0 ✗ |
| `until p`, `p` returns 1 | **0** | 3 ✗ |

`until` with a user-function condition was **exactly** `while`. The negation
was not weakened or mis-ordered; it had no effect at all.

The boundary mattered as much as the fault. `until (( ))`, `until [[ ]]`,
`until /bin/true` and `until /bin/false` were all correct. Only a user
function broke — which is why two existing specs, `cf.u1` and `cf.u2`, had
been passing over this for as long as it has existed. Both use `(( ))`.

## Why it survived

My first hypothesis was that the negation was being dropped for inlined
conditions. `--emit-c` disproved it: the negate is emitted in both the broken
and the working case, character for character.

```
__test_result = 1 - __test_result;
__exit_status = (__exit_status == 0) ? 1 : 0;
```

It was never missing. It was in the wrong **block**. Placed side by side, the
working `[[ ]]` case and the broken user-function case say it plainly:

```c
_blk_1: /* while.header */          _blk_3: /* while.header */
  __test_result = ((1) > 0);          __args0_ptr = ...;      /* set up args */
  __exit_status = ...;                __test_result = 1 - __test_result;
  __test_result = 1 - __test_result;  __exit_status = ...;
  if (__test_result) ... else ...;    goto _blk_8;   /* inlined `p` runs HERE */
```

On the left the condition is evaluated, negated, and branched on in one
block. On the right the header only marshals arguments; the negate fires on
the **previous iteration's** `__test_result`, and then the inlined callee
overwrites it. The negation was computed and then thrown away, every time.

The desugaring in `transform/statements.rs` appended the `NegateTestResult`
sigil to the loop *header*, and said so:

> Appending the NegateTestResult sigil ... works for ANY condition form —
> `[[ ]]`, `(( ))` (a LetExit), or a plain command

Three forms are named. All three are forms whose condition both starts and
finishes in the header block, so for all three `header` is the right answer
by coincidence rather than by construction. A user-function condition is the
case the list does not cover, and it is the case where the header ends in a
`Goto` instead of the loop branch.

`lower_while` already computes the right block and has done all along:

```rust
let cond_exit = lower_stmts(&w.condition, ssa, state, header);
...
ssa.block_mut(cond_exit).terminator = Terminator::Loop { ... };
```

The `Loop` terminator goes on `cond_exit` — the block where the condition
*finishes*. The negate belongs on the same block, immediately before it. That
is the whole fix.

## A second route to the same fault

Moving the sigil into `lower_while` meant checking who else calls it, and
`pipelines.rs::lower_pipe_while_read` does — passing the original `While`
straight through. It never applied the header patch at all, because the patch
lived in `lower_while_stmt`, a function it does not go through. So:

```sash
/bin/printf "a\nb\n" | until readline v { echo "got-$v" }
```

printed `got-a` and `got-b`. Bash's equivalent prints nothing: a successful
read makes the condition true, and `until` stops. Here the negation was
genuinely absent rather than misplaced — a different mechanism, the same
observable, and it fell out of the same fix for free. `until read` itself is
caught by SB5069, whose advice is *"use `until readline v`"* — so the one
spelling the diagnostic steers you toward was the broken one.

## What the shape of this bug argues

Both instances are the same mistake: **the patch was applied where the
condition was expected to be, not where the lowering said it was.** The
information was available locally and exact — `lower_while` returns from
`lower_stmts` holding `cond_exit` — and the desugaring reached around it to
re-derive the block by sniffing the entry block's terminator for a `Goto`.
That re-derivation is correct for simple conditions and silently wrong for
compound ones, and nothing about it announces which case you are in.

Consolidating it into `lower_while` removes the second call path as a place
the bug can hide, and the comment now records which forms *used* to work and
why, rather than asserting that all of them did.

## Specs

`cf.u4`–`cf.u7` in `control_flow.sh`. Four specs, and they are not all the
same kind of artifact:

- **cf.u4/u5** pin both rows of the truth table for a user-function
  condition. One alone would not do: the old behaviour was a *swap*, so a
  single row can be satisfied by a compiler that is wrong in the other
  direction.
- **cf.u6** is the pipeline route, which needed a different fix path even
  though it needed no different code.
- **cf.u7** is a regression shield for `/bin/true` / `/bin/false` — forms
  that were always correct and that the fix moves the sigil *away* from.
  It passes before and after, and that is its job.

Non-vacuity was established by execution, not by inspection: with the source
fix reverted and the specs left in place, `cf.u4`, `cf.u5` and `cf.u6` fail
and `cf.u7` passes. Exactly the three that should.

Sweep 6141 → **6149**, the predicted +8 (four specs, dual-mode). 0 failing,
0 flips.

## Still open

The probe that started this has not been answered. `check_condition_type`
rejects an int function in boolean context from **one** caller — the direct
`if` condition. DD#74 enumerates six: *"`if f`, `elif`, `while f`, `until f`,
`! f`, and `f` as an operand of `&&`/`||`."* Five of them accept it silently.
There is also a diagnostic loop: a bare call gives SB4016, *"handle with
`if`/`||`"*; taking the `if` half gives SB4030. That is the class DG ruled on
in `50ff49c` — if you advise, the advice must compile.

Fixing it means newly rejecting programs that compile today, in five
syntactic positions. That is a language-surface change and it is DG's call,
so it is filed rather than done.
