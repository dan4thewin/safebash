# 408 — The citation that pointed at the wrong subject

DG asked one question about yesterday's coverage pass: *is the "left cold on
purpose" backstop checked by a unit test?*

The answer is no — and asking it exposed that the "on purpose" was my
invention.

## What I had claimed

Journal 407 left `check_tainted_array` (`ssa_taint.rs:545`) uncovered and
justified it:

> DD#91 says so directly: the SSA pass *"does not yet check these argument
> positions — task `ssa-taint-full-backstop` — so the flags are
> forward-compatible correctness; the CST gate enforces the bypass today."*

The quote is real. It is in DD#91, at lines 50-53, verbatim. That is what made
it convincing, and it is why re-grepping for it would only have reassured me.

It is about something else. DD#91:44-53 is a two-bullet list describing how
`:!` is applied in each taint pass; the SSA bullet is about `ElideEmpty {
taint_off }` and `ArraySplat` **dispositions** classifying Clean. The
"argument positions" it says are unchecked are the ones a *disposition* would
appear in. `check_tainted_array` is the subscript-read check — a different
mechanism, in a different function, reached by a different shape.

I had a true sentence and attached it to the wrong subject. A citation is not
validated by the quote existing; it is validated by the quote being *about*
the thing.

## What the coverage data already said

The report I was reading contained the refutation, two lines apart:

```
529|check_tainted_var   … 2317 calls
537|    errors.push(…)   →    8     ← the scalar backstop FIRES
545|check_tainted_array …  603 calls
552|    if taint_of_name(…)  →  0     ← the array backstop never fires
```

Both are backstops. Both are shadowed by the same CST gate. One of them fires
eight times anyway. "It is a backstop, so it is cold" cannot be the
explanation when its sibling is not cold.

The eight are `sw.4`/`sw.5`/`sw.8`/`sw.9` × two modes — `taint_switch.sh`,
which compiles under `--no-taint-cst` precisely so the backstop is the only
layer left. The idiom for testing this already existed. I had not looked.

## Reachable in one line

```
$ sash --no-taint-cst t.sash
array a=($(getenv HOME))
echo ${a[0]}
→ tainted array a[...] used as argument to command 'echo' without attest
```

Untested, not unreachable. Specs `sw.10` (the reject, asserting the *array*
message so it cannot pass on the scalar backstop) and `sw.11` (a clean array
subscript, which must still compile and run) now pin it. Neutering the
condition fails `sw.10` and leaves `sw.11` green.

## What is genuinely deferred — and what the probe confirmed

Probing the neighbours turned up shapes that do compile clean under
`--no-taint-cst`:

```
argv 1 a;              echo ${a[0]}   → binary, leaks
read a < /etc/hostname; echo ${a[0]}  → binary, leaks
```

This is **not** a new finding. It is the documented hole in
`notes/tasks/ssa-taint-full-backstop.md`, which DG deferred on 2026-09-12 —
*"So long as all scenarios are caught by the earlier taint pass, all
defense-in-depth should be deferred."* All of these are SB5020 in a normal
compile; I verified each one. The deferral's condition still holds.

The distinction that matters: `array a=($(getenv HOME))` lowers to an
`Assign`, whose value carries the cmdsub's taint, so `classify_inst_def`
classifies it. The bulk-read origins DD#61 names — `read`/`mapfile`/`argv`
fills — define no SSA `VarRef` and sit in the no-def group. So exactly one
tainted-array origin reaches this arm, which is why one spec covers it and the
rest stay CST-only. `sw.10`'s comment records that, so the next person to read
a cold line here does not repeat my inference.

## The answer to DG's question

No, and it could not have been. `ssa_taint.rs` has no `#[cfg(test)]` module,
and `mk cov` measures the spec suite only — so "cold" in that report always
meant "no *spec* reaches it", never "nothing reaches it". Twelve files in
`sash/src` carry unit tests; this is not one of them. Had it been unit-tested,
the line would have shown covered under `mk cov-unit` and never entered the
queue.

## The lesson

Journal 386 recorded that a `panic!` plus a green sweep proves "not reached by
these inputs", not "unreachable". This is the same error one level up: a cold
line plus a plausible citation proves nothing at all. Both times the fix was
the same — run the shape.

I supplied the phrase "left cold on purpose" in my own summary. DG did not
accept it; he asked how it was verified. That is the only reason it was caught.

## Gates

6344 specs (predicted 6340 + 2×2), zero failures, zero flips against
`96412f4`; the four new ids are the only additions. `mk test` 106 passed.
clippy and fmt clean. Non-vacuity proven by reverting the condition.

Compiler source is untouched — this commit is specs plus the correction.
