title: "`map m = @{a:!~}` drops the DD#91 vouch — the two taint passes disagree"
status: done
priority: P1
depends:
gates:

# `map m = @{a:!~}` drops the DD#91 vouch — the two taint passes disagree

## The program

```sh
array a
read a
map m = @{a:!~}
for k in $(keys m) { echo "k=$k" }
```

Rejected, but not by a diagnostic:

```
z.sash: tainted variable k_3 used as an argument to 'echo' without attest (SSA taint backstop)
```

No `SB` code, no span, and it names an internal SSA version (`k_3`).

## Why it is a bug

DD#91 is explicit that `:!` yields clean data:

> additionally the value is treated as **clean**, so a tainted subject is used
> without `bless` (no SB5020) and the result **does not propagate taint**
> (`array a=(${x:!} lit)` leaves `a` clean).

and that the two passes must not diverge:

> The taint bypass is applied in **both** taint passes so they agree:

They do not agree here. `validate` honours the vouch — construction is accepted,
where the guard-on spelling `map m = @{a:~}` is correctly SB5020 at the
construction line. The SSA backstop does not, so the program dies later at the
*use*, with an internal message.

`:!` works correctly everywhere else measured — `/bin/echo @{a:!~}` from a
tainted array is accepted and clean. The loss is specific to the array→map
conversion.

## Mechanism

`SsaInst::MapFromArray` has **no field for the vouch**. Its `mode` collapses the
two spellings into one value (`sash/src/ssa.rs:290-300`):

> `mode` is the odd-count disposition: 0 `:-KEY` …, 1 `:?` (abort), 2 `:~`/`:!`
> (discard).

`:~` and `:!` are both `mode: 2`, so the instruction cannot represent the
difference. The lowering drops it: `map_from_array_parts` returns
`(source, mode, key, message)` and nothing else
(`sash/src/transform/statements.rs:902`). The taint arm then propagates
unconditionally (`sash/src/ssa_taint.rs:208-213`):

```rust
SsaInst::MapFromArray { source, target, .. } => {
    let t = taint_of_name(taint, source);
    taint.insert(var_key(target), t);
}
```

Compare the two arms that get it right: `elide_taint`
(`ssa_taint.rs:455-468`, `ElideEmpty { taint_off }`) and `splat_taint`
(`ssa_taint.rs:470-492`, `disposition.taint_off`). Both consult the vouch and
return `Clean`. `MapFromArray` is the one construction path that never received
the field.

The IR carries it fine — `sash --emit-ir` shows
`:operator "arrvals_default" :taint-off true` on the declaration's value — so
this is purely a lowering/SSA gap, not a parse or validate gap.

## Fix

Add `taint_off: bool` to `SsaInst::MapFromArray`, populate it in
`map_from_array_parts`, and return `Clean` from the taint arm when set,
mirroring `splat_taint`. Keep `mode` as-is: it is the *odd-count* disposition
and `:~`/`:!` genuinely share that behaviour; the vouch is an orthogonal bit and
wants its own field rather than a third `mode` value.

Settled by DD#91, not an open design question: `map m = @{a:!~}` leaves `m`
clean, by the same sentence that makes `array a=(${x:!} lit)` leave `a` clean.

## Why it was not caught

The guard-on path is specced and the vouch path is specced, but not *through a
map*. The backstop firing looks like the system working — it is a rejection, and
a rejection of tainted data is what one expects to see. Only the message gives
it away: a genuine taint rejection carries an `SB` code and a span.

## Bearing on "are map keys always clean?"

This is the **only** known counter-example to the DD#93 claim that

> Every route to a tainted map is closed at construction … There is no program
> in which the 2-binder form clears taint.

DD#93's table enumerates three routes and predates the `:!` interaction. With
this bug fixed the claim holds again — a vouched map is *clean*, not tainted,
so it is still not a route to a tainted map. The companion P1
(`bug-bulk-fill-accepts-map-target`) is the other open hole and closes the same
way.

So: keys are always clean, but that is currently true only because the one
program that disproves it is rejected by the wrong pass with the wrong message.

## Done

`SsaInst::MapFromArray` gained a `taint_off` field, populated from
`pe.taint_off` in `map_from_array_parts` and consulted by a new `map_fill_taint`
helper, which answers `Clean` when vouched. `mode` was left alone.

The helper is not decoration: putting the branch inline tripped the complexity
gate on `classify_inst_def` (cog 2 -> 4, and that dispatcher's exemption is
`cog <= 2`). Extracting it matches what `elide_taint` and `splat_taint` already
do for the same reason, and left the dispatcher at cc 11 / cog 1 -- lower than
before the change.

Specced as `taint_switch.sh` bkstp.13-18, in the `--no-taint-cst` section so
the backstop is the only judge: two one-character positive/negative pairs (the
value route and the key route) plus the two non-`:~` dispositions, which pin
the claim that the vouch is orthogonal to `mode`.
