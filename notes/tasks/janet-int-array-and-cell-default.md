title: "janet: int arrays miscompile (scan-number on the array literal); int-cell `:-` too"
status: deferred
priority: P3
kind: bug
depends: []
gates:
  - "`int array a=(4 5 6); echo ${a[1]}` prints 5 in janet (native+c already do)"
  - "`int array a=(1 2 3); echo ${a[0]:-0}` prints 1 in janet (cov.85, currently native+c only)"

## Symptom

Janet miscompiles a declared **int array**: the declaration path wraps the whole
array literal in `scan-number`:

```
$ sash --janet <<<'int array a=(4 5 6); echo ${a[1]}'
(var a (scan-number @["4" "5" "6"]))   # scan-number on an ARRAY → nil
(print (get a 1))                      # → nil
```

`scan-number` expects a string, gets a vector, returns nil, so `a` is nil and
every read yields nil. Native + c are correct.

Consequence: an int-array **cell** `:-` default (`${a[i]:-N}`) also fails in
janet, even though native + c handle it (the transform's identity-element
ternary — see `bug-cjanet-default-on-int-var`, resolved). cov.85 is therefore
guarded `in_modes native,c`; un-guard it once this is fixed.

## Cause (likely)

`emit_janet.rs` `emit_decl_value`: the `d.var_type == VarType::Int` arm calls
`emit_expr_as_int(&a.value, …)`, which for an **array literal** value emits a
single `(scan-number …)` around the whole `@[…]`. It should map `scan-number`
over each element (or store the elements already numeric): e.g.
`(map scan-number @["4" "5" "6"])` → `@[4 5 6]`, and likewise coerce per-cell on
indexed assignment.

Also then the `emit_pe_default` index branch needs an int-cell arm (mirror the
scalar `ctx.is_int` fix): for an int cell, out-of-range/absent → `0` (bash
unset-int) and compare `(= cell 0)` rather than `(= (length cell) 0)`.

## Found

2026-08-29, finishing `bug-cjanet-default-on-int-var` (int `:-` identity-element
default). The scalar int `:-` fix landed in all three backends; the int-cell
case exposed this broader pre-existing janet int-array gap.

## Deferred to post-beta (DD#104, 2026-09-30)

The in-tree janet backend is removed; `--emit-ir` replaces it with an
inert IR capture that an out-of-tree oracle can consume. This gap is
**not fixed** -- it is recorded here as part of the diagnosis a restart
should read. The code it describes lives at tag `janet-babel`.

This file was still marked live (`open`/`pending`/`active`) long after the
backend it targets was deleted; the other 28 janet tasks were deferred at the
time and these four were missed. Deferred 2026-09-30 for consistency -- no
assessment of the underlying gap has changed.
