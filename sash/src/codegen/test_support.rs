//! Shared `#[cfg(test)]` scaffolding for the Cranelift emitters.
//!
//! Almost every function in `codegen` takes a `&mut FunctionBuilder`, which
//! borrows both a `Function` and a `FunctionBuilderContext` that must outlive
//! it. That self-reference is why no emitter had a direct-call test for so
//! long: a helper cannot *return* a builder. It can lend one to a closure.
//!
//! Keep this module free of anything an individual test needs to vary — a
//! harness that bakes in a particular `vars` map or a particular `ExternFuncs`
//! stops being scaffolding and becomes a second compiler.

use cranelift_codegen::ir::{Function, Signature, UserFuncName};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

/// Run `f` with a builder positioned in a sealed entry block, so instructions
/// it emits have somewhere to land.
///
/// The function is never compiled or verified — these tests are about what an
/// emitter *decides*, not about the machine code it produces. Anything that
/// needs a real `ObjectModule` (interned literals, extern calls) wants its own
/// helper rather than an option on this one.
pub(super) fn with_builder<R>(f: impl FnOnce(&mut FunctionBuilder) -> R) -> R {
    let mut func =
        Function::with_name_signature(UserFuncName::user(0, 0), Signature::new(CallConv::SystemV));
    let mut fbc = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut func, &mut fbc);
    let entry = builder.create_block();
    builder.switch_to_block(entry);
    builder.seal_block(entry);
    f(&mut builder)
}
