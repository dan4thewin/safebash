---
title: "bare `wait` with no children: native sets `$?`=127, `--emit-c` sets 0 — a differential divergence no spec catches"
status: next
priority: P2
kind: bug
depends: []
gates:
  - "native and `--emit-c` agree on `$?` after a childless `wait`"
  - "a spec reads `$?` after `wait` in both modes"
  - "`codegen/dispatch.rs::emit_wait`'s inline `_<digits>` stripper is gone (see below)"
---

# The two `wait` lowerings disagree

Measured 2026-10-09:

```
wait
int s=$?
echo "s=$s"
```

- native → `s=127`
- `--emit-c` → `s=0`

The cause is visible in the two emitters. `codegen/dispatch.rs::emit_wait`
falls back to `iconst(I64, 0)` when no `!` slot exists, so it calls
`sash_wait(0)` → `waitpid(0, …)` → ECHILD → the runtime's `127`.
`emit_c.rs::emit_wait_c` takes the `None` branch and emits
`__exit_status = 0` with **no call at all**.

No spec catches it because no spec reads `$?` after a childless `wait`. The
differential oracle only localises a bug when something asserts the value.

## Fix it inside DD#126, not separately

DD#126 rewrites both lowerings anyway: `wait` becomes an int-returning
builtin that returns a **pid** (or `-1` on ECHILD), never aborts, and puts
the reaped child's exit status in `$?`. Bare `wait` becomes `wait(2)`
(reap any child) rather than "wait for `$!`", which deletes the `$!` lookup
from both emitters entirely.

## Carry one cleanup with it

`emit_wait`'s no-arg branch contains a hand-inlined `_<digits>` version
stripper (`vars.keys().find(…)`) — the non-determinism shape `find_array_slot`
already fixed, in a function that is about to be rewritten. It is the third
fork of that logic; the others are `codegen/redirects.rs::parse_var_base_version`
and `codegen/values.rs::strip_ssa_version`, which disagree with it on a
trailing `"x_"`. The DD#126 rewrite removes this one for free.
