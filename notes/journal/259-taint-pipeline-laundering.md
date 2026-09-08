# 259 — a taint bypass, found by taking a design question seriously

DG proposed: *"split only on NUL, terminate read on newline; user puts a
preceding pipeline stage of `tr` to convert a field separator of their choosing
to NUL."*

I set out to cost the proposal empirically — does `tr` emit NUL, what does an
extra external stage do to taint — and the second question answered wrong.

## The find

```sh
/bin/cat secret.txt | read x y          # SB5020 — correctly rejected
echo ig | /bin/cat secret.txt | read x y  # CLEAN. x="SECRET" y="DATA"
```

Prefixing any pipeline with a builtin `echo` laundered it completely. The
value then flowed into an external command's argv and into `[[ ]]` test
operands with no diagnostic. `read`, `read x y`, `read -a` and `mapfile` were
all affected; only `while read` was correctly rejected.

This is the exact class the project's adversarial audit named its highest
priority: **validator bypass**.

## Both layers failed, independently

The front end, `validate.rs:3551`:

```rust
let first_is_external = p.commands.first().is_some_and(is_external_source);
```

It asks whether the pipeline's **source** is external. But taint is a property
of the data's *path*, not its origin. `echo | cat secret | read` has a clean
source and a tainted path, and the check could not see the difference.

The SSA backstop — the defence-in-depth layer that exists precisely for when
the front end misses something — was inert here. `handle_lastpipe_read` set

```rust
clean_pipe: true,
```

unconditionally, for every lastpipe read, while the field's own doc comment
read *"True when read is in a lastpipe position with clean producer stages."*
The comment described a check that was never written.

So the two failures were not independent in the way that matters. One layer
had a wrong rule; the other had a **hardcoded answer to the same question**.
Defence-in-depth that is inert is not depth, and nothing in the suite noticed,
because nothing tested the backstop in isolation for this shape.

## The fix

Front end: accumulate rather than sample.

```rust
let mut upstream_external = false;
for (i, s) in p.commands.iter().enumerate() {
    ...
    upstream_external |= is_external_source(s);
}
```

Backstop: compute `clean_pipe` from the producers instead of asserting it.

And then a third thing, which I think matters more than either: I had first
written a *second* `stage_is_external` in the transform, with slightly
different rules (it treated user functions as clean, which is arguably more
correct per DD#24 — a function stage is inlined, never exec'd). Two
classifiers, one question. That is precisely the shape that produced the bug —
a rule and a hardcoded answer that were supposed to agree and did not. I threw
mine away and made validate's `is_external_source` `pub(crate)`, so both layers
now call the *same function*. They cannot drift.

The over-conservatism this inherits (a user-function stage taints, though no
`execve` occurs) is pre-existing — verified against HEAD — and being too strict
is the safe direction. Worth revisiting as its own question, not inside a
security fix.

## What made it findable

Not a review of the taint code. A probe run for an unrelated reason, whose
result I did not expect: I inserted `tr` into a pipeline to measure the *cost*
of DG's proposal, and the read came out clean when I predicted tainted. The
useful instinct was to stop and ask why, rather than record "no taint cost —
good for the proposal" and move on. The answer would have been a nice number in
a design table and a live bypass in the compiler.

Two sessions running, the bug was found by **measuring something adjacent**
rather than by looking where bugs were expected.

## Also: my own spec was flaky

The first clean-control I wrote was `echo "a b" | echo "c d" | read x y`. It
passed in isolation and failed once in a full sweep — 1 run in 12. The middle
`echo` ignores stdin and exits, so the first `echo`'s write races against a
closed pipe; sash reports `echo: command failed (exit 128)`.

A flaky spec in a security fix is worse than no spec: it trains you to re-run
until green. I replaced it with two deterministic controls — a clean array read
(`taint.sh` tl.8) and, more importantly, `taint_switch.sh` sw.9, which runs the
laundering shape under `--no-taint-cst` so **the backstop is tested with the
front end switched off.** That is the test whose absence let `clean_pipe: true`
sit there.

(The SIGPIPE race itself is pre-existing and unrelated; noted, not chased.)

## Sweep

4302 passing / 39 failing / 401 skipped. 0 regressions, +24, no new failures.
One janet discrepancy surfaced by the new clean-control and filed separately
(`janet-read-a-element-count`): janet's `read -a` counts one element too many.

DG's design question is still open and unanswered — this was the detour, not
the answer.
