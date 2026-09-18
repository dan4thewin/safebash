---
title: A function that undid its only caller
date: 2026-09-18
---

# A function that undid its only caller

The branch queue turned up two near-identical helpers, `base_var_name`
(`codegen/inst_handlers.rs`) and `parse_var_base_version`
(`codegen/redirects.rs`). Same shape, same three cold branches each:
`rfind('_')` never returns `None`, the suffix is never empty, the
suffix is always all-digits. Six cold branches between them.

The obvious hypothesis was a collision bug. A variable literally named
`x_2` at version 0 keys as `x_2_0`; a variable named `x` at version 2
keys as `x_2`. If the two could ever be confused, the fallback would be
papering over it. They cannot: `rfind` takes the *last* underscore,
which is always the one the key format appended. Three bash
differentials, all SAME. Not a bug.

## The actual finding

Every key in the `HashMap<String, VarSlot>` is built by
`VarRef::display()`:

```rust
pub fn display(&self) -> String {
    format!("{}_{}", self.name, self.version)
}
```

`version` is a `u32`, so the suffix is always non-empty and always
digits. Version 0 is spelled `name_0`, never bare `name`. I audited
every `vars.insert` site — `dispatch.rs` 466/489/491/525/533,
`setup.rs:1391` (`format!("{}_0", …)`) — and all of them are versioned.
The only other map construction is an empty one in a test.

Which makes the single call site of `base_var_name` an identity round
trip:

```rust
let base = base_var_name(&target.display());
```

Build `name_N` from a `VarRef`, then parse the `_N` back off to
recover `name` — a value the caller already had in hand. Two
allocations and a string scan to compute `target.name`. It is now:

```rust
let base = &target.name;
```

and `base_var_name` is gone. That is not a judgment call; it is
provable for all `u32` versions and all names, and the sweep agrees —
5583 tests, 0 failing, byte-identical.

Worth noting *why* the round trip was invisible: the doc comments on
**both** helpers claimed keys were "`name` (version 0) or `name_N`".
That spelling is what made the collision hypothesis plausible and what
made the fallback look necessary. Nothing in the compiler has ever
emitted a bare `name` key. Both comments were wrong; the surviving one
now records the real invariant, including why `x_2` and `x` stay
distinct, so the next reader does not re-derive it.

A stale comment does not just fail to help. It manufactures work.

## What is left

`parse_var_base_version` survives and genuinely parses — its caller
`find_highest_version` iterates map keys, where the base and version
are not separately available. Its three cold branches are a defensive
fallback on an invariant guaranteed by construction, and they are
*not* ICE guards: on an unparseable key they return a silently-wrong
answer rather than failing loudly.

There is no way to cover them without changing behaviour, so this is
DG's call. The structurally clean fix is to stop encoding a pair into
a string at all — key the map on `(String, u32)`, or give `VarSlot`
the name and version — which removes the parse instead of testing it.
That touches every `vars` site across five codegen files, so it is not
something to start unilaterally. Filed for DG with the cheaper options
alongside it.
