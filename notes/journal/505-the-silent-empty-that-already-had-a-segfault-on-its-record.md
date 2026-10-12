# 505 — The silent empty that already had a segfault on its record

2026-10-12. DG said yes to both `unreachable!()`s. Implementing them turned up
a third thing neither of us was looking at.

## What was asked

Two native-backend sites answered "this variable has no codegen slot" with a
silent empty string:

- `load_var_or_empty` (`codegen/value_handlers.rs`), reached from every
  `${var:op:operand}` parameter expansion;
- `emit_attest`'s `_` arm (`codegen/dispatch.rs`), supplying the value printed
  in `attest: <name> = '<val>' failed the gate`.

Both cold. I had flagged them as a behaviour question rather than a coverage
item, because making a fallback into an ICE changes what the compiler does.

## The justification was stale, and the evidence was already in the tree

`load_var_or_empty`'s doc argued from *bash*:

> Bash treats undefined vars as empty strings in parameter expansion, so the
> `:- / -` / `:+ / +` defaulting operators do not need a special path here.

sash is not bash here. SB5031 rejects a scalar read of a name introduced
nowhere in scope, so by the time codegen runs, "undefined variable" is not a
state the program can be in. A missing slot means something else entirely.

What it means is written down two files away, in `setup.rs::map_inst_versioned`:

> Every version must ALIAS the one base slot: an unmapped version makes
> `load_var_or_empty` miss, hand back a null pointer, and **segfault in
> `sash_table_len`**.

So the fallback had already been a segfault once. The comment warning about it
sat in the function that *populates* the map, while the function that *reads*
the map explained its null away as bash semantics. Neither comment knew about
the other.

## Verifying the invariant before asserting it

An `unreachable!()` is a claim, so it needs the enumeration, not the
assumption. Ten `SsaInst` variants carry a `target: VarRef`:

```
Assign  ReadLine  MapFromArray  SplitValue  ReadArray
ArgvFill  KeysFill  CaptureEnd  Attest  AttestRetag
```

`map_inst_versioned` names the first eight. The other two are inserted during
emission by `emit_attest` and `emit_attest_retag` themselves. Ten accounted
for, so the claim holds — and the ICE text names `map_inst_versioned`, which
is the function that has to grow a case when an eleventh arrives.

That enumeration is the whole value of the exercise. `map_inst_versioned` ends
in `_ => {}`, so nothing in the type system would have told us.

## emit_attest asked the same question twice

The attest path looked up `{source.name}_{source.version}` **twice** — once in
the fail block for the diagnostic, once in the pass block for the retagged
copy — and handled a miss differently each time (zeros in the first, a silently
skipped copy in the second, which is what made line 601 cold too). Two lookups
of one key that must not be able to disagree. Now one lookup, one ICE, and the
`match` over `VarSlot` is total at both uses.

## The thing nobody was looking at

`emit_c.rs::load_var_or_empty_c` claimed in its doc to mirror the native
function. It does not. It has **three** tiers where native has one:

1. exact `(name, version)`;
2. else the **highest version** of `name`;
3. else `("\"\"", "0")`.

And unlike the native miss, **these are live.** From `coverage.db` over the
full sweep:

| tier | hits |
|---|---|
| exact version | 489 / 509 |
| exact **missed** | **20** |
| highest-version fallback taken | **8** |
| empty-string sentinel | **12** |

Native: 232 calls, **0** misses.

So on the same corpus, the C backend fails to find the version the IR names 20
times, and in 8 of those emits the value of a *different* SSA version. The
sweep is green in both modes, so this is not a known divergence — but
`--emit-c` exists to *catch* substitutions like that, not to contain them.

I did not touch it. The call counts are not comparable as they stand
(`load_var_or_empty_c` has a second caller, `lower_value`'s `Var` arm, which
the native function does not), and deleting a tier 20 sites depend on would
turn the sweep red with no information about why. Filed as
`notes/tasks/emit-c-var-slot-fallback-asymmetry.md`, P2, with the measured
numbers and the first step: attribute each miss to a caller.

Its doc comment now records the asymmetry instead of claiming a mirror.

## Scaffolding, finally

Pinning an ICE in codegen meant solving the problem that had kept every
emitter untested: a `FunctionBuilder` borrows a `Function` and a
`FunctionBuilderContext` that must both outlive it, so a helper cannot return
one. It can lend one to a closure. `codegen/test_support.rs` is eleven lines
of `with_builder(|builder| …)` and it unblocks the whole directory.

`load_var`'s ICE is pinned with it.

## DG: "does the backstop pass the rival test?"

His own rule, turned on the thing I had just landed: an unreachable line earns
its place only if it catches a regression **orthogonally** — "if the backstop
is essentially an identical test to the original then why is it more
trustworthy". Three artefacts, and they do not all survive it.

**The `unreachable!()` passes.** The rival is the spec sweep, and the two
detect on different axes. The sweep is behavioural and only sees shapes a spec
covers; its version of this fault is a segfault in the user's compiled binary,
which journal 449 shows took a session to diagnose because it presented as
memory-layout nondeterminism. The ICE is structural, fires at the point of the
fault for *any* program, and names the function to fix.

**The negative unit test passes, but not for the reason I first wrote.** I had
claimed it asserts the invariant. It does not — if an eleventh defining
instruction arrives unmapped, the test still passes and the ICE fires in the
opener's sweep. What it catches is the **reversion**: someone restoring the
tolerant fallback. That is not hypothetical; it is what the function did for
its entire life, on a doc comment arguing from bash. A silent empty is silent,
so the sweep stays green and only this test goes red. The data is orthogonal
too — an empty map is a shape none of the 8367 programs produces.

Proved it can fail rather than asserting it: restoring the `(null, 0)` arm
turns the test red, and nothing else in either gate moves.

**The positive companion fails, and I removed it.** I had added
`a_mapped_version_loads_its_slot` as a control — a `#[should_panic(expected =
…)]` passes vacuously if the function panics on everything. But the sweep
already runs the mapped path **232 times** and would fail on the first
compile. That is precisely "re-asking the gate's own question on the same
data". The sweep *is* the control. `mk test` 125 → 124.

The counter-argument I considered and rejected: a unit test should be
self-contained, not lean on a different gate for interpretability. But the
harness is self-checking anyway — `with_builder` failing internally produces a
panic whose message does not contain `setup::map_inst_versioned`, so the test
goes red rather than passing vacuously.

`emit_attest`'s ICE is **not** pinned, and the rival test is why. The *line*
earns its place — it guards a second data path, since an attest source can be
populated by a previous attest's own `vars.insert` rather than by
`map_inst_versioned`, so `load_var`'s ICE cannot catch a fault there. But a
unit *test* of it would assert the same sentence against the same named
function as the one above, which is a restatement, not a backstop. Left cold
deliberately. (Reaching it also needs the heavier harness — a full `InstCtx`,
an `ObjectModule` with the runtime declared, and the source name pre-interned.)

## Gates

Sweep **8367** — 4068 c + 13 skipped + 4286 native, predicted before running.
Zero flips, zero new ids, liveness 258 (the comparison can report a non-zero).
`mk test` 123 → **124** (125 before the rival test removed one).

The sweep is the real result: neither ICE fires on any of 8367 programs. The
invariant held before it was asserted; now it says so.
