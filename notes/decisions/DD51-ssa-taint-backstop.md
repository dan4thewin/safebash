title: DD#51 — SSA taint verification pass
status: implemented

1. Independently verify taint after transformation and before code generation,
   using actual SSA definitions, uses and read transports.
2. An unsafe use that escaped source checking is a compiler invariant failure,
   not an alternative source-language policy.

---

`ssa_taint.rs` — second taint pass on SSA after transformation, before codegen.
Classifies each VarRef as tainted/clean by defining instruction.
If tainted VarRef reaches use site, emits ICE. This is the airbag —
~~if it fires, that's a bug in the CST pass.~~ The defect is in source
checking or lowering.
