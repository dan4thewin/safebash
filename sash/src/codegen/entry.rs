//! Code generation: SSA → Cranelift → native object.
//!
//! String representation (Decision #12):
//!   Strings are (ptr, len) where ptr points to len+1 bytes with
//!   ptr[len] == 0. Null terminator always present.
//!
//! Control flow:
//!   SSA blocks map directly to Cranelift blocks.
//!   Branch/Loop terminators become conditional branches.
//!   Test expressions ([[ ]]) compile to inline string comparisons.

use std::collections::HashMap;

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types::I64;
use cranelift_codegen::ir::{Function, InstBuilder, MemFlags, Signature, UserFuncName};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings;
use cranelift_codegen::verifier::verify_function;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{default_libcall_names, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};

use crate::ssa::*;

use super::types::InstCtx;

/// Compile an already-built SSA program to a native object file.
///
/// TRUST: this is the only path from SSA to object code, and it is
/// deliberately **not** a path from *source* to object code. It takes the
/// program and SSA the caller already parsed, validated, resolved and
/// transformed, so there is no second front-end run that could disagree with
/// the one the gates ran on.
///
/// It used to take `&str` and re-parse, re-type-check and re-transform from
/// scratch — a third full front-end pass per compile, whose result codegen
/// then used while the taint gate had run on a *different* SSA object built
/// from the same source. Nothing forced the two to agree. Its doc comment
/// claimed "the ONLY entry point from validated source to object code" while
/// accepting raw source and calling no validator; the caller enforced that,
/// and a `pub` fn asserting a property it does not check is how the old
/// fail-open test harness looked defensible (journal 348).
///
/// The `catch_unwind` wrapper turns an ICE in codegen into a clean error
/// rather than a stacktrace — codegen should never panic on validated input,
/// so this is now the only way this function can fail.
pub fn compile_to_object(
    program: &crate::ir::Program,
    ssa: &crate::ssa::SsaProgram,
    bash_source: &str,
    enable_trace: bool,
) -> Result<Vec<u8>, Vec<String>> {
    // DD#109: every native compile carries its own IR and manifest. The IR
    // emitter is exhaustive by construction, so running it on every build
    // turns "a new IR variant does not render" into an immediate failure of
    // every compile rather than a latent gap in a rarely-taken flag.
    let ir_text = crate::emit_ir::emit_ir(program);
    let manifest_text = manifest_text(ssa);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        compile_ssa(
            ssa,
            Embedded {
                source: bash_source,
                ir: &ir_text,
                manifest: &manifest_text,
            },
            enable_trace,
        )
    }));
    match result {
        Ok(obj) => Ok(obj),
        Err(payload) => Err(vec![panic_payload_message(payload)]),
    }
}

/// Decode a `catch_unwind` panic payload into a clean error message.
fn panic_payload_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else {
        "internal compiler error in codegen".to_string()
    }
}

/// The verbatim artifacts embedded as ELF note sections by a native
/// compile: the source (DD#45) plus the IR capture and the manifest
/// (DD#109). Every native compile supplies all three, including empty bodies.
pub struct Embedded<'a> {
    pub source: &'a str,
    pub ir: &'a str,
    pub manifest: &'a str,
}

/// The manifest body: one external dependency path per line. Byte-identical
/// to what `--manifest` prints, so the embedded section and the flag cannot
/// disagree.
fn manifest_text(ssa: &SsaProgram) -> String {
    let mut s = String::new();
    for dep in &ssa.external_deps {
        s.push_str(dep);
        s.push('\n');
    }
    s
}

pub fn compile_ssa(program: &SsaProgram, embedded: Embedded, trace: bool) -> Vec<u8> {
    let reachable = program.reachable();
    let shared_flags = settings::Flags::new(settings::builder());
    let isa = cranelift_native::builder()
        .expect("host ISA")
        .finish(shared_flags)
        .expect("build ISA");

    let obj_builder = ObjectBuilder::new(isa.clone(), "sash_program", default_libcall_names())
        .expect("ObjectBuilder");
    let mut module = ObjectModule::new(obj_builder);

    super::setup::define_heap_size(&mut module);
    let externs = super::setup::declare_runtime_functions(&mut module);
    let (literals, space_id, nl_id) = super::setup::intern_literals(&mut module, program, trace);
    let mut literals = literals;
    // ── Build main() ───────────────────────────────────────────
    let main_sig = Signature::new(CallConv::SystemV);
    let main_func_id = module
        .declare_function("main", Linkage::Export, &main_sig)
        .expect("declare main");

    let mut func = Function::with_name_signature(UserFuncName::user(0, 0), main_sig);

    let mut fbc = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut func, &mut fbc);

        let fs = super::setup::allocate_variables(&mut builder, program, &reachable);
        let cl_blocks = fs.cl_blocks;
        let mut vars = fs.vars;
        let test_result_var = fs.test_result_var;
        let exit_status_var = fs.exit_status_var;
        let readers = fs.readers;
        let capture_slots = fs.capture_slots;
        let redirect_slots = fs.redirect_slots;
        let saved_end_status_var = fs.saved_end_status_var;
        let end_cl_block = fs.end_cl_block;
        let end_region = program.end_region();
        let child_region = program.child_region();

        super::setup::reserve_user_fds(&program.user_fds, &mut builder, &mut module, &externs);
        super::setup::init_undeclared_redirect_vars(
            &mut builder,
            &mut module,
            program,
            &mut vars,
            &mut literals,
        );
        emit_all_blocks(EmitCtx {
            builder: &mut builder,
            module: &mut module,
            program,
            reachable: &reachable,
            cl_blocks: &cl_blocks,
            vars: &mut vars,
            literals: &literals,
            externs: &externs,
            test_result_var,
            exit_status_var,
            readers: &readers,
            capture_slots: &capture_slots,
            redirect_slots: &redirect_slots,
            saved_end_status_var,
            end_cl_block,
            end_region: &end_region,
            child_region: &child_region,
            space_id,
            nl_id,
            trace,
        });

        builder.seal_all_blocks();
        builder.finalize();
    }

    verify_function(&func, isa.as_ref()).expect("verify function");

    let mut ctx = cranelift_codegen::Context::for_function(func);
    module
        .define_function(main_func_id, &mut ctx)
        .expect("define function");

    let mut product = module.finish();
    embed_artifacts(&mut product, &embedded);
    product.emit().expect("emit object")
}

/// All the threading compile_ssa needs to hand to per-block / per-
/// terminator emit helpers. Bundled to keep the call sites readable
/// and to make the helpers' signatures bounded.
struct EmitCtx<'a, 'b> {
    builder: &'a mut FunctionBuilder<'b>,
    module: &'a mut ObjectModule,
    program: &'a SsaProgram,
    reachable: &'a std::collections::HashSet<BlockId>,
    cl_blocks: &'a HashMap<BlockId, cranelift_codegen::ir::Block>,
    vars: &'a mut HashMap<String, super::types::VarSlot>,
    literals: &'a HashMap<String, cranelift_module::DataId>,
    externs: &'a super::types::ExternFuncs,
    test_result_var: cranelift_frontend::Variable,
    exit_status_var: cranelift_frontend::Variable,
    readers: &'a std::collections::BTreeMap<u32, super::types::ReaderSlot>,
    capture_slots: &'a [(cranelift_frontend::Variable, cranelift_frontend::Variable)],
    redirect_slots: &'a [Vec<cranelift_frontend::Variable>],
    saved_end_status_var: cranelift_frontend::Variable,
    end_cl_block: Option<cranelift_codegen::ir::Block>,
    end_region: &'a std::collections::HashSet<BlockId>,
    /// Blocks that run in a forked child: an abort there exits the child
    /// directly instead of entering END (DD#56).
    child_region: &'a std::collections::HashSet<BlockId>,
    space_id: cranelift_module::DataId,
    nl_id: cranelift_module::DataId,
    trace: bool,
}

/// Caught/uncaught $? model: these labels mark the exit/merge
/// blocks of constructs that CATCH inner statuses (if, while, for,
/// case). At the start of these blocks, codegen resets
/// `exit_status_var` to 0 so caught statuses do not leak to any
/// uncaught sink (program exit). See `notes/tasks/bug-while-loop-exit-code.md`.
fn is_catching_construct_exit(label: &str) -> bool {
    matches!(
        label,
        "if.merge"
            | "if_attest.merge"
            | "while.exit"
            | "for.exit"
            | "for_cmd.exit"
            | "for_arr.exit"
            | "for_at.exit"
            | "dispatch.merge"
    )
}

fn emit_all_blocks(mut ctx: EmitCtx) {
    let block_ids: Vec<BlockId> = ctx.program.blocks.keys().copied().collect();
    for (idx, &ssa_id) in block_ids.iter().enumerate() {
        let Some(ssa_block) = block_to_emit(&ctx, ssa_id) else {
            continue;
        };
        if idx > 0 {
            ctx.builder.switch_to_block(ctx.cl_blocks[&ssa_id]);
        }
        emit_block_entry_hooks(&mut ctx, &ssa_block.label);
        let terminated = emit_block_insts(&mut ctx, ssa_block, ssa_id);
        if !terminated {
            emit_terminator(&mut ctx, ssa_block);
        }
    }
}

/// Unbound function templates have no incoming edge. A reachable one is a
/// lowering regression, not a block that can safely be left unfilled.
fn block_to_emit<'a, 'b>(ctx: &EmitCtx<'a, 'b>, ssa_id: BlockId) -> Option<&'a BasicBlock> {
    if !ctx.reachable.contains(&ssa_id) {
        return None;
    }
    let block = ctx.program.block(ssa_id);
    if block.label.starts_with("fn_") {
        panic!(
            "ICE: unbound function template {} is reachable",
            block.label
        );
    }
    Some(block)
}

/// Per-block entry hooks: actions that must run at the *start* of
/// a Cranelift block (before any SSA instruction) selected by the
/// block's SSA label.
fn emit_block_entry_hooks(ctx: &mut EmitCtx, label: &str) {
    // DD#56: save exit status on entering END block so the program
    // exits with the status that triggered END, not whatever END
    // itself did.
    if label == "__end__" {
        let es = ctx.builder.use_var(ctx.exit_status_var);
        ctx.builder.def_var(ctx.saved_end_status_var, es);
        let cleanup = ctx
            .module
            .declare_func_in_func(ctx.externs.reader_cleanup, ctx.builder.func);
        ctx.builder.ins().call(cleanup, &[]);
        // DD#120: END is the invoker's code, so it runs on the invoker's
        // fds. An abort inside a redirected command jumps straight here,
        // leaving that command's redirects applied — the restore that would
        // have undone them sits on an unreachable edge behind the abort.
        let sr = ctx
            .module
            .declare_func_in_func(ctx.externs.scope_restore, ctx.builder.func);
        ctx.builder.ins().call(sr, &[]);
    }
    // Caught/uncaught $? model: if/while/for/case CATCH the
    // statuses they contain. On exit from a catching construct,
    // the rc register is reset to 0 so the construct's internal
    // statuses cannot leak to any uncaught sink (program exit).
    // SB5012 already prevents user code from reading $? after
    // these constructs; this is the runtime counterpart.
    if is_catching_construct_exit(label) {
        let zero = ctx.builder.ins().iconst(I64, 0);
        ctx.builder.def_var(ctx.exit_status_var, zero);
    }
    // Phi nodes are no-ops in Cranelift codegen: all SSA versions
    // of the same bash variable share the same Cranelift Variable,
    // so Cranelift's SSA builder handles the merge automatically.
}

/// Emit all instructions in a block; return true if any instruction
/// terminated the block early (e.g. `exit`).
fn emit_block_insts(ctx: &mut EmitCtx, ssa_block: &BasicBlock, ssa_id: BlockId) -> bool {
    let mut block_terminated = false;
    // DD#55: the instruction in condition position has its redirect open(2)
    // failure caught by if/||/while, not aborted — distinct from `-`
    // suppression. `ssa::condition_inst_index` is shared with emit_c so the
    // two backends cannot disagree about which instruction that is.
    let cond_idx = crate::ssa::condition_inst_index(ssa_block);
    let end_cl_block = abort_end_block(ctx, ssa_id);
    for (idx, inst) in ssa_block.insts.iter().enumerate() {
        if block_terminated {
            break;
        }
        let in_condition = cond_idx == Some(idx);
        let in_end_block = ctx.end_region.contains(&ssa_id);
        block_terminated = inst.emit(
            ctx.builder,
            ctx.module,
            &mut InstCtx {
                vars: ctx.vars,
                literals: ctx.literals,
                externs: ctx.externs,
                space_id: ctx.space_id,
                nl_id: ctx.nl_id,
                test_result_var: ctx.test_result_var,
                exit_status_var: ctx.exit_status_var,
                readers: ctx.readers,
                capture_slots: ctx.capture_slots,
                redirect_slots: ctx.redirect_slots,
                end_cl_block,
                program: ctx.program,
                saved_end_status_var: ctx.saved_end_status_var,
                cl_blocks: ctx.cl_blocks,
                in_condition,
                in_end_block,
            },
        );
    }
    block_terminated
}

/// DD#56: where an abort in `ssa_id` enters END. END runs only in the main
/// process, so a forked child has no END to route through — every abort in
/// it exits the child directly.
fn abort_end_block(ctx: &EmitCtx, ssa_id: BlockId) -> Option<cranelift_codegen::ir::Block> {
    if ctx.child_region.contains(&ssa_id) {
        None
    } else {
        ctx.end_cl_block
    }
}

fn emit_terminator(ctx: &mut EmitCtx, ssa_block: &BasicBlock) {
    match &ssa_block.terminator {
        Terminator::Return => emit_return(ctx, ssa_block),
        Terminator::Goto(target) => {
            let target_cl = ctx.cl_blocks[target];
            ctx.builder.ins().jump(target_cl, &[]);
        }
        Terminator::Branch {
            true_block,
            false_block,
            ..
        } => emit_test_branch(ctx, "branch", *true_block, *false_block),
        Terminator::Loop {
            body_block,
            exit_block,
            ..
        } => emit_test_branch(ctx, "loop", *body_block, *exit_block),
        Terminator::Fork { parent, child } => emit_fork(ctx, *parent, *child),
        Terminator::ChildExit => {
            let status = ctx.builder.use_var(ctx.exit_status_var);
            emit_exit_call(ctx, status);
        }
    }
}

/// A `Branch` or `Loop` terminator: jump on the test result, tracing the
/// outcome under `--trace` as `kind`.
fn emit_test_branch(ctx: &mut EmitCtx, kind: &str, on_true: BlockId, on_false: BlockId) {
    if ctx.trace {
        emit_branch_outcome_trace(ctx, kind, on_true, on_false);
    }
    let cond = ctx.builder.use_var(ctx.test_result_var);
    let true_cl = ctx.cl_blocks[&on_true];
    let false_cl = ctx.cl_blocks[&on_false];
    ctx.builder.ins().brif(cond, true_cl, &[], false_cl, &[]);
}

/// A `Fork` terminator: the child (pid 0) continues at `child`, the parent at
/// `parent`. The parent's first instruction reads the pid: it is `$!`, or a
/// negative one is the fork failure that instruction reports.
fn emit_fork(ctx: &mut EmitCtx, parent: BlockId, child: BlockId) {
    let fork = ctx
        .module
        .declare_func_in_func(ctx.externs.fork, ctx.builder.func);
    let call = ctx.builder.ins().call(fork, &[]);
    let pid = ctx.builder.inst_results(call)[0];
    ctx.builder.def_var(ctx.test_result_var, pid);
    if ctx.trace {
        // Each process reports its own side: a nonzero pid (including a
        // failed fork's -1) is the parent's, zero the child's.
        emit_branch_outcome_trace(ctx, "branch", parent, child);
    }
    let parent_cl = ctx.cl_blocks[&parent];
    let child_cl = ctx.cl_blocks[&child];
    ctx.builder.ins().brif(pid, parent_cl, &[], child_cl, &[]);
}

fn emit_return(ctx: &mut EmitCtx, ssa_block: &BasicBlock) {
    // DD#56: if END block exists, use saved triggering status
    let status = if ctx.program.end_block.is_some() && !ssa_block.label.starts_with("fn_") {
        ctx.builder.use_var(ctx.saved_end_status_var)
    } else {
        ctx.builder.use_var(ctx.exit_status_var)
    };
    emit_exit_call(ctx, status);
}

fn emit_exit_call(ctx: &mut EmitCtx, status: cranelift_codegen::ir::Value) {
    let exit_ref = ctx
        .module
        .declare_func_in_func(ctx.externs.exit, ctx.builder.func);
    ctx.builder.ins().call(exit_ref, &[status]);
    ctx.builder
        .ins()
        .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
}

/// DD#32: emit a runtime trace message of the form
/// `+ <kind> → <target_label>\n` to stderr at branch/loop time,
/// gated on the global `sash_trace_flag`. The message picked depends
/// on which side of the branch was taken; we use a `select` between
/// the two pre-interned literal pointers to keep the emitted code
/// branchless after the trace-active check.
fn emit_branch_outcome_trace(
    ctx: &mut EmitCtx,
    kind: &str,
    true_target: BlockId,
    false_target: BlockId,
) {
    let cond_val = ctx.builder.use_var(ctx.test_result_var);
    let true_label = ctx.program.block(true_target).label.as_str();
    let false_label = ctx.program.block(false_target).label.as_str();
    let true_msg = format!("+ {} \u{2192} {}\n", kind, true_label);
    let false_msg = format!("+ {} \u{2192} {}\n", kind, false_label);
    let true_id = super::values::intern_or_declare(ctx.literals, &true_msg);
    let false_id = super::values::intern_or_declare(ctx.literals, &false_msg);

    let trace_gv = ctx
        .module
        .declare_data_in_func(ctx.externs.trace_flag, ctx.builder.func);
    let trace_base = ctx.builder.ins().global_value(I64, trace_gv);
    let trace_val = ctx.builder.ins().load(I64, MemFlags::new(), trace_base, 0);
    let zero_t = ctx.builder.ins().iconst(I64, 0);
    let is_tracing = ctx.builder.ins().icmp(IntCC::NotEqual, trace_val, zero_t);
    let trace_block = ctx.builder.create_block();
    let no_trace_block = ctx.builder.create_block();
    ctx.builder
        .ins()
        .brif(is_tracing, trace_block, &[], no_trace_block, &[]);

    ctx.builder.switch_to_block(trace_block);
    ctx.builder.seal_block(trace_block);
    let fd2 = ctx.builder.ins().iconst(I64, 2);
    let write_ref = ctx
        .module
        .declare_func_in_func(ctx.externs.write, ctx.builder.func);
    let zero2 = ctx.builder.ins().iconst(I64, 0);
    let took_true = ctx.builder.ins().icmp(IntCC::NotEqual, cond_val, zero2);
    let true_gv = ctx.module.declare_data_in_func(true_id, ctx.builder.func);
    let true_ptr = ctx.builder.ins().global_value(I64, true_gv);
    let true_len = ctx.builder.ins().iconst(I64, true_msg.len() as i64);
    let false_gv = ctx.module.declare_data_in_func(false_id, ctx.builder.func);
    let false_ptr = ctx.builder.ins().global_value(I64, false_gv);
    let false_len = ctx.builder.ins().iconst(I64, false_msg.len() as i64);
    let msg_ptr = ctx.builder.ins().select(took_true, true_ptr, false_ptr);
    let msg_len = ctx.builder.ins().select(took_true, true_len, false_len);
    ctx.builder.ins().call(write_ref, &[fd2, msg_ptr, msg_len]);
    ctx.builder.ins().jump(no_trace_block, &[]);

    ctx.builder.switch_to_block(no_trace_block);
    ctx.builder.seal_block(no_trace_block);
}

/// DD#45/DD#109: embed the source, the IR capture and the manifest as
/// `.sash_source` / `.sash_ir` / `.sash_manifest` ELF note sections
/// (retrievable via `--extract`). All three are embedded verbatim: the
/// coercion/rewrite recorder lived in the old CST→IR lowerer and was removed
/// with tree-sitter (commit 73caecf); structural bash→sash-native rewriting
/// now lives in the separate `sash-convert` crate.
///
/// Note sections survive `ld --gc-sections --strip-debug`, which is why the
/// source section has been readable since DD#45.
fn embed_artifacts(product: &mut cranelift_object::ObjectProduct, embedded: &Embedded) {
    for (name, body) in [
        (&b".sash_source"[..], embedded.source),
        (&b".sash_ir"[..], embedded.ir),
        (&b".sash_manifest"[..], embedded.manifest),
    ] {
        let section_id =
            product
                .object
                .add_section(vec![], name.to_vec(), object::SectionKind::Note);
        product
            .object
            .append_section_data(section_id, body.as_bytes(), 1);
    }
}

#[cfg(test)]
mod tests {
    use super::{compile_to_object, panic_payload_message};

    // `panic_payload_message` decodes a caught codegen `catch_unwind` payload.
    // It only runs when a codegen ICE actually fires, which every guard is
    // meant to prevent — so it's covered by unit tests, not the spec suite.
    #[test]
    fn payload_string_is_recovered() {
        let p: Box<dyn std::any::Any + Send> = Box::new(String::from("boom from panic!"));
        assert_eq!(panic_payload_message(p), "boom from panic!");
    }

    #[test]
    fn payload_static_str_is_recovered() {
        let p: Box<dyn std::any::Any + Send> = Box::new("static boom");
        assert_eq!(panic_payload_message(p), "static boom");
    }

    #[test]
    fn payload_other_type_falls_back() {
        // A non-string payload (e.g. an integer) → the generic ICE message.
        let p: Box<dyn std::any::Any + Send> = Box::new(42_i32);
        assert_eq!(
            panic_payload_message(p),
            "internal compiler error in codegen"
        );
    }

    /// The three tests above assert the *decoder*; this one asserts the
    /// *wiring*. Delete the `catch_unwind` in `compile_to_object` and all three
    /// still pass, because nothing above ever calls `compile_to_object` — the
    /// ICE would simply escape as a stacktrace, which is the behaviour the
    /// wrapper exists to prevent. That makes this an orthogonal backstop rather
    /// than a second asking of the same question.
    ///
    /// The shape: an `SsaProgram` whose `entry` names a block that does not
    /// exist. `ir_to_ssa` cannot produce one — every entry comes from
    /// `new_block`, which inserts before it returns the id — so no source
    /// program reaches this and the spec suite never can.
    #[test]
    fn codegen_ice_becomes_an_error_not_a_stacktrace() {
        let program = crate::ir::Program {
            statements: Vec::new(),
            end_block: None,
        };
        let ssa = crate::ssa::SsaProgram::new();
        assert!(
            ssa.blocks.is_empty() && ssa.entry == 0,
            "entry has no block"
        );

        // Silence the default hook: the panic is expected, and its backtrace
        // on stderr would read as a test failure.
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let result = compile_to_object(&program, &ssa, "", false);
        std::panic::set_hook(prev);

        let diags = result.expect_err("codegen must not succeed on a dangling entry");
        assert_eq!(diags.len(), 1, "one diagnostic per caught ICE: {diags:?}");
    }

    #[test]
    fn a_reachable_function_template_is_an_invariant_violation() {
        let program = crate::ir::Program {
            statements: Vec::new(),
            end_block: None,
        };
        let mut ssa = crate::ssa::SsaProgram::new();
        ssa.new_block("fn_unbound");
        let diags = compile_to_object(&program, &ssa, "", false)
            .expect_err("an unbound function template must not be emitted or silently skipped");
        assert_eq!(
            diags,
            ["ICE: unbound function template fn_unbound is reachable"]
        );
    }
}
