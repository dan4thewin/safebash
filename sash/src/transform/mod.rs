//! IR → SSA transformation.
//!
//! SECURITY: This module converts validated IR into SSA form. The key
//! security property: no new operations are synthesized — every SSA
//! instruction traces back to an IR statement. Variable versioning and
//! phi nodes are mechanical (they don't add capabilities).
//!
//! A reviewer should verify:
//! 1. Function inlining preserves semantics (no capability escalation)
//! 2. Command name resolution uses compile-time PATH only (DD#23)
//! 3. Phi nodes don't merge tainted/clean values incorrectly

mod control_flow;
mod expr_handlers;
mod expressions;
mod functions;
mod helpers;
mod pipelines;
mod resolution;
mod statements;
mod stmt_handlers;

pub use self::entry::ir_to_ssa;
pub use self::entry::ir_to_ssa_typed;

mod entry;
