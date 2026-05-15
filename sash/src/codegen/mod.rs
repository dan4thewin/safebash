//! Code generation: SSA → Cranelift → native object.
//!
//! SECURITY: This is the final stage. All code reaching here has been
//! validated, taint-checked, and type-checked. Codegen's obligation is
//! faithful translation — no new capabilities introduced.

mod arithmetic;
mod commands;
mod dispatch;
mod entry;
mod inst_handlers;
mod redirects;
mod setup;
mod tests;
mod types;
mod value_handlers;
mod values;

pub use entry::compile_ssa;
pub use entry::compile_to_object;
