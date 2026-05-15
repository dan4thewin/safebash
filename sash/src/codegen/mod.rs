//! Code generation: SSA → Cranelift → native object.
//!
//! SECURITY: This is the final stage. All code reaching here has been
//! validated, taint-checked, and type-checked. Codegen's obligation is
//! faithful translation — no new capabilities introduced.

mod entry;
mod types;

pub use entry::compile_ssa;
pub use entry::compile_to_object;
