# 336 — two bugs in `unset`, and the spec that could not see them

Coverage mining walked into `type_check.rs::check_unset_args` and, probing its
cold branches, turned up a live bug. Then a second one next to it.

## 1. `unset a[0]` on an indexed array was a silent no-op

```
array a=(x y z)     map m=([k]=v [j]=w)
unset a[0]          unset m[k]
→ len=3, 0=x        → len=1, k=      ← the map form worked
```

The map form working is what makes this a bug rather than a restriction:
`type_check.rs`'s own DD#30 comment says *"Subscript forms like `unset name[i]`
are allowed"*, and the operation was accepted, validated, and compiled. It just
did nothing.

`--emit-c` named the cause in two adjacent lines:

```c
sash_table_set(__t0_tbl, (const char*)&__t0_k, 8, "x", 1);  /* write: 8-byte i64 key */
sash_table_delete((SashTable*)a_ptr, "0", 1);               /* unset: ASCII "0" */
```

An indexed array keys its entries by the **8 raw bytes of an i64**; a map keys
by the string. `handle_unset` emitted `SsaValue::Literal(key)` unconditionally,
so for an array the delete hashed a key that cannot exist, found nothing, and
returned 1 — which nobody checks. Maps were fine because for them the string
*is* the key.

Every other subscript site already gets this right. `SsaValue::Subscript` and
`SsaInst::SubscriptAssign` both carry an `is_integer_key` flag, set with the
same one-liner in two places:

```rust
let is_int_key = !state.assoc_arrays.contains(&name);
```

`unset` was the one subscript path that bypassed the machinery. The fix adds
`BuiltinCmd::TableDeleteIdx` so the instruction carries the encoding the way
the rest of the IR does, and both backends branch on it exactly as
`emit_subscript_value` / `lower_subscript_raw` already do.

No new semantics were invented. Sparse arrays already existed — `a[5]=q` on a
three-element array gives `len=4` with a hole at 3 — so "the length drops and
the slot reads empty" follows from the model already in place. No DD needed.

## 2. `unset a[0] a[1]` discarded every target after the first

Probing the fix turned up its neighbour. The multi-target path did this:

```rust
let joined = var_args.join("");
if joined.contains('[') { items.push(joined); }   // "a[0]a[1]" as ONE target
```

The join exists for a real reason — the parser can split a target into
`["m", "[b]"]` — but joining *all* args flattens distinct targets together.
`unset a[0] a[1]` became one target named `a` with the key `0]a[1`:

| before | result |
|---|---|
| `unset a[0] a[1]` on 3 elements | `n=2` — one delete, not two |
| `unset m[a] m[b]` on 3 keys | `n=3` — **nothing** deleted |
| `unset a[0] m[k]` | `na=2 nm=2` — the map untouched |

Now the re-attachment is conditional: an arg that *starts with* `[` joins the
previous item, anything else starts a new one. `unset a m` (plain multi-target)
always worked and still does.

## The spec that could not see it

`types.sh` has had this since the beginning:

```sh
# 30.41 unset arr[N] removes element
run_sash 'array a=(one two three); unset a[1]; echo ${a[0]} ${a[2]}'
[[ $rc = 0 && $out = "one three" ]]; chk 30.41
```

It asserts indices **0 and 2** — the two elements `unset a[1]` does not touch.
It passes identically whether the delete works or does nothing at all. A spec
named for the behaviour, sitting in the right file, testifying to nothing.

This is the same shape as the green-sweep lesson from 334: coverage tells you a
line *ran*, and a spec title tells you what someone *meant* to test. Neither is
evidence about the value. The replacement asserts the length and the emptied
slot, which are the only things that actually change.

There is a matching precedent two files over. `subscript.sh` 80.27 exists
because `unset m[K]` was once a silent no-op **in the C backend** — the map
half of this bug, found and fixed earlier. The array half survived because the
native backend was never wrong in a way 30.41 could detect.

## Specs

Seven added, all failing before the fix and passing after:

- `types.sh` **30.46–30.49** — the length drops and the slot empties; index 0
  is not special; deleting every element gives `len=0`; a deleted index is
  writable again (so the length counts live entries, not a high-water mark).
- `subscript.sh` **80.28–80.30** — multi-target for an array, for a map, and
  mixed kinds in one statement.

Sweep **5153 passing, 10 skipped, 0 failing** — 5139 + 7 × 2 modes, predicted
exactly. Units 83 green. Branch coverage measured before this batch: **88.66%,
422 cold** (was 431).

## Noted, not changed

`SB5063` already rejects a *sigilled* subscript in `unset` with the message
*"A quoted or sigilled target names nothing, so the operation is silently
discarded"*. The validator was guarding against exactly this failure mode for
one spelling while the literal spelling did it anyway. The key in `unset` is
therefore always a literal, which is what bounds the fix — no variable-index
plumbing is needed.
