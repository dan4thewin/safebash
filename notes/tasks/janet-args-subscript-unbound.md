---
status: ready
priority: P2
---

# janet: `${args[N]}` inside a function emits an unbound symbol

Found 2026-09-12 while writing the SB5068 controls (`spec/types.sh` 75.54).
Pre-existing — confirmed by stashing the SB5068 change and re-running at
`fe71aa1`, so it is unrelated to that work.

    f() { echo "${args[0]}"; }
    f one

The janet backend emits

    (defn f []
      (print (get args 0)))
    (f "one")

`f` declares no parameters and `args` is bound nowhere, so janet rejects the
program at compile time:

    compile error: unknown symbol args

## Why the preamble doesn't cover it

`emit_janet` has `needs_args_preamble` / `_stmt_uses_args`, which emit

    (var _args (array/slice (dyn :args) 1))

for the *script's* positional parameters (`$#`, `$@`, `$*`, `$N`). That is the
top-level surface. The DD#? `args` collection inside a **function** is a
different binding — the callee's own argument vector — and nothing maps it.
The name reaches the output verbatim.

## Scope

The native and c backends are correct; this is janet-only. The same family as
`janet-collection-parameters` (the DD#100 slots `A`..`E` are likewise emitted
unresolved), and the two should probably be fixed together: both are "a
collection the callee receives rather than declares".

## Gate

`f() { echo "${args[0]}"; }` + `f one` prints `one` under `--janet`, and
`spec/types.sh` 75.54 runs unguarded in all three modes.
