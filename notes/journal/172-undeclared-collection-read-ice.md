# 172 — undeclared collection read ICE → SB5031

`echo ${z[0]}` (reading an **undeclared** array/map) panicked at codegen
(`ICE: undefined array: z`, codegen/types.rs) instead of a clean compile error.
Found while implementing DD#80 (a `$b[0]` cell read on an undeclared `b` ICE'd).
Bug policy fix.

## Root cause

`validate::check_undeclared_reads` (SB5031, DD#62) deliberately checked only the
**index** of a `${a[i]}` subscript, not the collection **name** ("Subscripts are
container reads (out of scope)"). So an undeclared collection name sailed past
validation and hit the codegen slot lookup, which panics on a missing array. The
**arith** path (`check_arith_reads`) already flagged the name; only the word path
(`check_expr_reads`) omitted it — the asymmetry.

## Fix

`check_expr_reads`'s `Subscript` arm now flags an unknown collection name
(`flag_if_unknown`), mirroring the arith path, so `${z[i]}` / `$z[0]` report
SB5031. `args` is skipped in this arm: a top-level `${args[i]}` is already
reported by SB5024 (skipping avoids a double), and inside a function it is in
scope. spec/undeclared.sh ud.19-21 (×3 modes).

## Adjacent gap filed (not fixed here)

Adding `args` to the in-function scope set (to clear the arith `$(( args[0] ))`
SB5031 *false positive*) instead **exposed** a codegen gap: the arith
bare-subscript path never resolves `args` → the per-inline `__argsN` array, so it
panics `undefined array: args`. The SB5031 false positive was *protective*. Left
as-is (arith `$(( args[0] ))` stays SB5031, not an ICE) and filed
`arith-args-subscript-codegen.md`. Word `${args[i]}` is unaffected (works).

## Verification

Full `SPEC_MODES="native c janet"` sweep, diff vs HEAD (`2bac342`): 0
passing→failing, 0 gone, 9 new passing (ud.19-21 ×3); native gate 0. Spot-checked
`${z[0]}`/`${z[k]}`/`$z[0]` → SB5031; declared `${a[0]}`/`${m[k]}`, in-function
`${args[0]}`, for-loop `${arr[$i]}`, top-level `${args[0]}`→SB5024 all intact.
