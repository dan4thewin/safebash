title: 322 — The same hardware fault, two opposite answers

`emit_div_or_mod_with_zero_check` disposed of x86's `#DE` two different
ways, fifteen lines apart, inside one function.

The top of it tested `rhs == 0`, wrote bash's `division by 0 (error token
is "0")` to stderr, produced `0`, and let the program continue. The
bottom tested `lhs == i64::MIN && rhs == -1` — the *other* cause of the
same hardware exception — and called `sash_arith_overflow`, which prints
`<argv0>: arithmetic overflow` and exits 2. One fault, one function, two
answers, and the two disagreed about everything: the message format,
whether a value was produced, whether the program kept running.

DG found the seam by asking a question I could not answer from the code:
"I would have thought some hardware trap would prevent divide by 0 — are
we adding some explicit check?" We were. The check is why the trap never
fires, which is fine; what was not fine was what the check did next.

## What C actually does

Before proposing anything I measured gcc, because the previous behaviour
had been justified as matching bash and I did not want to replace one
borrowed answer with another.

| spelling | gcc |
|---|---|
| literal `1/0` | `-Wdiv-by-zero`, then **`ud2` as the entire body of `main`** — SIGILL, rc 132 |
| runtime `1/z` | raw `idiv`, `#DE` → SIGFPE, rc 136 |

C never wraps, never guards, and never yields a value. gcc is so certain
a literal `1/0` is a dead program that it does not bother emitting the
rest of the function.

That reframed the problem. sash was not merely diverging from C — it was
diverging *in the opposite direction from the one I assumed*. I had been
thinking of the literal case as the harmless one because the compiler can
see it. In C it is the case treated most harshly, and in sash it was the
one that had been silent until last cycle's fold fix.

## DG's call

Compile-time error for a divisor the folder can prove; abort at runtime
for one it cannot.

The part I had to correct in my own framing: those two are
**complementary, not redundant**. There is no const-propagation through
variables, so `int z = 0; $(( 1 / z ))` is not folder-provable and reaches
the runtime. The compile-time error cannot remove the runtime path, and a
proposal that assumed it could would have deleted a live one.

I also argued against DG's first instinct of letting bare SIGFPE be the
runtime answer, and DG took the argument: SIGFPE loses the `<argv0>:`
prefix, says "floating point" for an integer divide, and — the decisive
one — would push `i64::MIN / -1` back out of the message path, re-opening
the exact split this change exists to close. So the explicit check stays,
and it is now deliberate rather than incidental. That distinction is in
the code as a comment, because the next reader will otherwise see a
redundant branch in front of an instruction that already traps.

## SB5077, and where the semantics live

The compile-time half had one design constraint worth recording: the
validator must agree with the folder about what "provable" means, or the
two drift and the error starts firing on programs that would have run.

So `check_arith_div_by_zero` does not reimplement constant evaluation. It
calls the transform's own `fold_arith_op` for every operator; only the
tree walk is local, and that walk is deliberately the weaker half —
anything it declines (variables, expansions, calls) just reaches the
runtime trap, which is still correct, only later. Soundness is
structural rather than maintained by hand.

It is wired into both arithmetic entry points, the `$(( … ))` value path
and the `(( … ))` statement path. A rule answering to one spelling and
not the other would have been a new instance of the same
spelling-dependent inconsistency the whole change is about.

One judgement call is flagged for DG rather than buried: the walk
descends into ternary arms, so `$(( 1 ? 5 : 10/0 ))` is now rejected even
though that arm never evaluates (bash prints 5). This is the one place
the check is *stronger* than the folder. I kept it because sash already
rejects dead code as an error — SB5050 refuses a variable that is
assigned and never read — and a literal fault in an unreachable branch is
the same family. It is one line to revert if DG disagrees.

## Closed on the way past

The open question "could any `--emit-c` path emit a literal `/0`?" is now
closed by construction rather than by inspection: SB5077 rejects the
program before the C backend runs. Given gcc's `ud2`, that was worth more
than the audit I had been putting off.

And DD#25's "known gap" list was stale — it named `Div` (`i64::MIN / -1`)
as not-yet-trap-with-message when that had already been implemented.
Verified and struck.

## Ledger

Three tests changed meaning, five added, all in `spec/coverage.sh`
(plan 155 → 158). `cov.68/69` keep testing the runtime path but now
assert exit 2 and the new message; `cov.138/139` flip from runtime to
compile-time; `cov.157/158/159` cover parens, the statement spelling,
and the ternary arm. `cov.155/156` unchanged — non-zero divisors still
fold.

Sweep 4837 passing / 10 skipped / 0 failing, both modes. 36 lib tests
green.
