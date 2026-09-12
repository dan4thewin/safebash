---
title: "janet: positional ${N:-DEF} returns the default even when the arg is present"
status: deferred
deferred-by: DD104
priority: P2
kind: bug
depends: []
gates:
  - "janet: `fn f { echo ${1:-DEF} }; f VAL` prints VAL (not DEF)"
  - "the fix applies equally to the brace-less spelling `$1:-DEF` (same lowering)"
---

## Found

2026-09-04, adding brace-less dispositions (`$x:-foo`, journal 242). A spec
`vm.bl4` (`fn f { /bin/echo $1:-DEF }; f VAL`) failed **native passes, janet
returns DEF**. The braced `${1:-DEF}` with an arg present emits **identical
janet** and has the same bug — so this is a pre-existing janet positional-default
defect, not a brace-less-sugar issue (the sugar desugars to the braced form).

`vm.bl4` is gated `native,c` for now; native/c are correct.

## Likely area

`emit_janet.rs` positional / `${N:-…}` default lowering: the default appears to
fire whenever the parameter is a positional, rather than only when the
positional is absent/empty. Compare with the scalar `${x:-…}` path (which is
correct in janet — `vm.bl2`/`vm.bl7` pass) and the c backend (correct).

Companion to `bug-cjanet-default-on-int-var` / `janet-int-array-and-cell-default`
(the janet `:-` default family has had several holes).


## Deferred to post-beta (DD#104, 2026-09-12)

The in-tree janet backend is removed; `--emit-ir` replaces it with an
inert IR capture that an out-of-tree oracle can consume. This gap is
**not fixed** — it is recorded here as part of the diagnosis a restart
should read. The code it describes lives at tag `janet-babel`.
