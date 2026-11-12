title: "janet: an auto-declared collection inside a list/`||` is scoped to that block"
status: deferred
deferred-by: DD104
priority: P2
kind: backend-parity
depends: []
gates:
  - "`argv 1 fresh || :` then `$(len fresh)` compiles and runs under --janet"
  - "same for `matchall S P fresh || :` and the other bulk-fill builtins"

## What

`emit_janet` emits a collection's `(var …)` at the point of first use rather
than hoisting it, so when the auto-declaring command sits inside a list
(`||`, `&&`) the declaration lands **inside that block** and is invisible
afterwards. Janet then rejects the later read.

```sh
argv 1 fresh || :
echo "$(len fresh)"
```
```
o.janet:13:16: compile error: unknown symbol fresh
```

The emitted preamble shows the problem — the `var` is indented, i.e. nested:

```janet
  (var fresh (array/slice (dyn :args) (min 1 (length (dyn :args)))))
...
(print (string (length fresh)))     # unknown symbol
```

## Scope

**Not specific to any one builtin.** Verified identical for `argv`, and it
applies equally to the other auto-declaring bulk fills (`read`, `newread`,
`matchall`). Native and c are unaffected — they auto-declare through the SSA
`autodeclare_bulk_target`, which has no lexical scoping problem.

## Why it is structural

There is no hoisting pass: `predeclare_one` / `ctx.declare` emit at the
current `out` position and depth, so the declaration inherits whatever block
the statement is in. Fixing it properly means collecting a program's
collection names up front and emitting the `var`s at top level — which is
essentially what `emit-janet-from-ssa` would give for free, since the SSA
already carries the auto-declaration as an explicit instruction.

## Specs

`regex.sh` 63.ma6 is gated `native,c` for this reason. Found while adding
`matchall` (DD#63), by checking whether the failure was mine — it was not.


## Deferred to post-beta (DD#104, 2026-09-12)

The in-tree janet backend is removed; `--emit-ir` replaces it with an
inert IR capture that an out-of-tree oracle can consume. This gap is
**not fixed** — it is recorded here as part of the diagnosis a restart
should read. The code it describes lives at tag `janet-babel`.
