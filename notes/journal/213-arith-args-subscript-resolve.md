## 213 — `$(( args[i] ))` in a function: resolve args in arith (unmask the ICE)

**On `hand-parser`.** DG: "fix now" (the latent native bug I'd flagged when
answering "were there more bugs"). This is `arith-args-subscript-codegen`.

### What it was (and why "masked by SB5031", not dead code)

`$(( args[0] ))` inside a function was rejected at validate by SB5031
(undeclared-read) — a **false positive** (the implicit positional array `args`
IS in scope in a function, DD#75 §5; the word form `${args[0]}` worked). That
rejection sat in front of a real codegen gap: the arith-subscript lowering
(`subscript_read_to_ssa`) never called `resolve_args_name`, so `args` reached
codegen unresolved and would trip the **defensive** `find_array_slot` ICE
(codegen/types.rs, `panic!("ICE: undefined array: args")`). The ICE is a
correct safety guard (bucket D — fires on "an array reached codegen
unresolved"), not removable dead code; SB5031 just hid the path.

### Fix (two coupled edits)

- **transform/expr_handlers.rs** `subscript_read_to_ssa`: resolve `args` →
  `__argsN` via `resolve_args_name`, and use the resolved name for the
  `assoc_arrays`/`type_map` lookups too — a verbatim mirror of the word path
  `subscript_to_value`.
- **validate.rs** `check_function_scope`: `fn_known.insert("args")` (DD#75 §5),
  removing the SB5031 false positive. Function-scoped, so top-level `args` stays
  rejected.

### Verification

`f(){ echo $(( args[0]+1 )); }; f 5` → 6; `$(( args[0]+args[1] ))`; f 3 4 → 7
(native+c). Unmasking correctly routes arith args through the **pre-existing**
positional rules (they were fully hidden before): non-contiguous `args[1]` →
SB5044, over-arity → SB5043 — identical to `${args[i]}` (verified parity). Scope
guard intact: top-level `$(( args[0] ))` → SB5031; typo `$(( notavar[0] ))` →
SB5031. janet's arith VALUE path still can't read an embedded subscript
(pre-existing `janet-arith-subscript-read`), so positives are native,c like the
sibling `bsx.*`. specs arithmetic.sh bsx.8/9 (positive) + bsx.10 (top-level
reject). Full 3-mode sweep vs `499859c`: **0 regressions**; +3 native/c, +1
janet. fmt + clippy clean.

Native gate now has no known latent bugs — clear to resume the never-run grind.
