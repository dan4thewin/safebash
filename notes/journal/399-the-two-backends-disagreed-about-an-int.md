# 399 — the two backends disagreed about an int

Next on the coverage queue: `emit_attest` in `codegen/dispatch.rs`, seven cold
lines in two clumps. Both clumps turned out to be the same thing — the
**int** paths. The `_ =>` fallback that supplies the value for the failure
message, and the `VarSlot::Int` arm that copies the attested value to the
clean SSA version.

Every `attest` spec in the suite — and there are a lot of them, 180 mentions
in `taint.sh` alone — attests a *string*.

## The shape was the hard part

The obvious probe doesn't compile:

    int n=$(arg 1)
    SB4030: cannot assign string value to int variable `n` — a string→int
    conversion goes through arithmetic and must carry a disposition

Which is right, and is exactly the sort of thing that makes a cold line look
unreachable when it isn't. A tainted int has to arrive through the
disposition:

    x=$(arg 1)
    int n=$(( ${x:-0} ))
    attest n [[ -n $n ]]
    echo "$n"

That compiles, runs, and prints the argument. Drop the `attest` and the taint
backstop rejects it, so the attest is doing real work. A working feature with
no gate at all.

Markers in both arms confirmed it rather than inferring it: the int program
fires `MARK_FALLBACK` and `MARK_INT_COPY`, the string program fires neither.

## Then the failure path disagreed with itself

Writing the failing-gate spec is where it got interesting. Native:

    $ ./prog 9
    prog: attest: n = failed the gate

and the C backend, same program, same input:

    $ ./prog 9
    prog: attest: n = '9' failed the gate

The C backend names the value. Native drops it. `emit_attest` only pulled a
value out of a `VarSlot::Str`; anything else got a null pointer and a zero
length, so the runtime printed the name alone.

That is a divergence, and the differential oracle exists precisely to turn one
into a located bug. It also isn't a close call about which side is right:

- the string path prints `x = 'bad'`,
- the C backend prints `n = '9'` for the int,
- and the comment directly above the native code says *"Name + value tell the
  user which gate failed and why."*

Three sources agreeing against one. Native was wrong.

The fix is four lines, because the machinery was already there —
`codegen/types.rs::emit_int_to_str`, the helper every other site uses to render
an i64 through `sash_itoa`, which is the same runtime call the C backend
reaches by way of `emit_var_value` → `emit_itoa_temp`:

```rust
Some(VarSlot::Int(v)) => {
    let val = builder.use_var(*v);
    super::types::emit_int_to_str(builder, module, externs, val)
}
```

The revert proof is unusually clean for a divergence: with the arm removed,
native fails exactly `atint.5` and the C run stays green — which is the
signature of a one-backend bug rather than a spec that was wrong about the
language.

## What the specs pin

`atint.1`–`atint.8` in `taint.sh`. The block opens with the no-attest form
being *rejected*, so the rest of it cannot quietly become vacuous if the taint
backstop ever stops firing. Then the passing gate, the converted value used in
arithmetic (so a constant-folded 7 can't fake it), the disposition default, the
failing gate with its value, the same gate passing, the string form for
contrast, and a clean int.

## The residual

The fix moved the int case *out* of `_ =>` into its own arm, so those two lines
are still cold — but they now mean something narrower: a source variable with
no slot at all. `VarSlot` has exactly two variants, so `_` is `None`.

I could not reach it. Attesting an undeclared name is SB5031, a condition that
doesn't mention the variable is SB5020, a collection read with a scalar sigil
is SB5064, and a tainted value entering an array is SB5021. Validation seems to
stand in front of every route.

"Seems to" is the honest verb. Checkpoint 031 is the standing reminder that a
green sweep proves *not reached by these inputs*, never *unreachable*, so this
is filed as a residual rather than deleted — two defensive lines behind four
validator rules. Five of the seven cold lines are now covered, and the two that
remain are a smaller and better-understood claim than they were this morning.

## Tally

Coverage pointed at seven cold lines. They were not themselves a bug — they
were an untested feature. But the act of writing the test for that feature is
what made the two backends answer the same question out loud, and *that* was
the bug. Which is the second time in two sessions that the cold line was the
signpost rather than the destination.

`mk test` 52/27/20. Sweep 6201 → **6217**, exactly as predicted, 0 failing,
0 flips.
