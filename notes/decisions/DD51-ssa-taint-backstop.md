title: DD#51 — SSA taint verification pass
status: implemented

`ssa_taint.rs` — second taint pass on SSA after transformation, before codegen.
Classifies each VarRef as tainted/clean by defining instruction.
If tainted VarRef reaches use site, emits ICE. This is the airbag —
if it fires, that's a bug in the CST pass.
