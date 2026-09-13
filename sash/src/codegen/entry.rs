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

use super::redirects::SavedFd;
use super::types::InstCtx;

/// Compile sash source to a native object file.
///
/// TRUST: This is the ONLY entry point from validated source to object code.
/// The pipeline is: parse → validate → taint → lower → type_check → transform → codegen.
/// By the time we reach compile_ssa (called below), the source has passed all gates.
///
/// The catch_unwind wrapper ensures ICE panics in codegen produce clean errors
/// rather than stacktraces — codegen should never panic on validated input.
pub fn compile_to_object(bash_source: &str, enable_trace: bool) -> Result<Vec<u8>, Vec<String>> {
    let _trace = enable_trace;

    // New parser: parse source directly to IR
    let (program, parse_diags) = crate::parse::parse(bash_source).map_err(|diags| {
        diags
            .iter()
            .map(|d| format!("{}:{}: [{}] {}", d.line, d.col, d.code, d.message))
            .collect::<Vec<_>>()
    })?;

    if !parse_diags.is_empty() {
        let errors: Vec<String> = parse_diags
            .iter()
            .map(|d| format!("{}:{}: [{}] {}", d.line, d.col, d.code, d.message))
            .collect();
        if parse_diags
            .iter()
            .any(|d| d.severity == crate::diagnostic::Severity::Error)
        {
            return Err(errors);
        }
    }

    let type_map = crate::type_check::infer_types(&program);
    let (ssa, diags): (crate::ssa::SsaProgram, Vec<String>) =
        crate::transform::ir_to_ssa_typed(&program, type_map);

    if !diags.is_empty() {
        return Err(diags);
    }

    // Catch ICE panics in codegen and convert to clean error messages
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        compile_ssa(&ssa, Some(bash_source), enable_trace)
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

pub fn compile_ssa(program: &SsaProgram, bash_source: Option<&str>, trace: bool) -> Vec<u8> {
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
    let (literals, space_id, nl_id) =
        super::setup::intern_literals(&mut module, program, bash_source, trace);
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
        let pipe_read_fd_var = fs.pipe_read_fd_var;
        let capture_slots = fs.capture_slots;
        let saved_end_status_var = fs.saved_end_status_var;
        let end_cl_block = fs.end_cl_block;

        super::setup::init_positional_params(
            &mut builder,
            &mut module,
            program,
            &externs,
            &mut vars,
            &mut literals,
            &reachable,
            space_id,
            exit_status_var,
            test_result_var,
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
            pipe_read_fd_var,
            capture_slots: &capture_slots,
            saved_end_status_var,
            end_cl_block,
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
    embed_sash_source(&mut product, bash_source);
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
    pipe_read_fd_var: cranelift_frontend::Variable,
    capture_slots: &'a [(cranelift_frontend::Variable, cranelift_frontend::Variable)],
    saved_end_status_var: cranelift_frontend::Variable,
    end_cl_block: Option<cranelift_codegen::ir::Block>,
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
    let mut redirect_stack: Vec<Vec<SavedFd>> = Vec::new();
    for (idx, &ssa_id) in block_ids.iter().enumerate() {
        let Some(ssa_block) = block_to_emit(&ctx, ssa_id) else {
            continue;
        };
        if idx > 0 {
            ctx.builder.switch_to_block(ctx.cl_blocks[&ssa_id]);
        }
        emit_block_entry_hooks(&mut ctx, &ssa_block.label);
        let terminated = emit_block_insts(&mut ctx, ssa_block, ssa_id, &mut redirect_stack);
        if !terminated {
            emit_terminator(&mut ctx, ssa_block);
        }
    }
}

/// Return the block to emit, or None if it should be skipped
/// (unreachable, or a function block that was constant-folded).
fn block_to_emit<'a, 'b>(ctx: &EmitCtx<'a, 'b>, ssa_id: BlockId) -> Option<&'a BasicBlock> {
    if !ctx.reachable.contains(&ssa_id) {
        return None;
    }
    let block = ctx.program.block(ssa_id);
    if block.label.starts_with("fn_") {
        return None;
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
fn emit_block_insts(
    ctx: &mut EmitCtx,
    ssa_block: &BasicBlock,
    ssa_id: BlockId,
    redirect_stack: &mut Vec<Vec<SavedFd>>,
) -> bool {
    let mut block_terminated = false;
    // DD#55: the instruction in condition position has its redirect open(2)
    // failure caught by if/||/while, not aborted — distinct from `-`
    // suppression. `ssa::condition_inst_index` is shared with emit_c so the
    // two backends cannot disagree about which instruction that is.
    let cond_idx = crate::ssa::condition_inst_index(ssa_block);
    for (idx, inst) in ssa_block.insts.iter().enumerate() {
        if block_terminated {
            break;
        }
        let in_condition = cond_idx == Some(idx);
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
                pipe_read_fd_var: ctx.pipe_read_fd_var,
                capture_slots: ctx.capture_slots,
                redirect_stack,
                end_cl_block: ctx.end_cl_block,
                program: ctx.program,
                ssa_id,
                saved_end_status_var: ctx.saved_end_status_var,
                cl_blocks: ctx.cl_blocks,
                in_condition,
            },
        );
    }
    block_terminated
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
        } => {
            if ctx.trace {
                emit_branch_outcome_trace(ctx, "branch", *true_block, *false_block);
            }
            let cond = ctx.builder.use_var(ctx.test_result_var);
            let true_cl = ctx.cl_blocks[true_block];
            let false_cl = ctx.cl_blocks[false_block];
            ctx.builder.ins().brif(cond, true_cl, &[], false_cl, &[]);
        }
        Terminator::Loop {
            body_block,
            exit_block,
            ..
        } => {
            if ctx.trace {
                emit_branch_outcome_trace(ctx, "loop", *body_block, *exit_block);
            }
            let cond = ctx.builder.use_var(ctx.test_result_var);
            let body_cl = ctx.cl_blocks[body_block];
            let exit_cl = ctx.cl_blocks[exit_block];
            ctx.builder.ins().brif(cond, body_cl, &[], exit_cl, &[]);
        }
    }
}

fn emit_return(ctx: &mut EmitCtx, ssa_block: &BasicBlock) {
    // DD#56: if END block exists, use saved triggering status
    let status = if ctx.program.end_block.is_some() && !ssa_block.label.starts_with("fn_") {
        ctx.builder.use_var(ctx.saved_end_status_var)
    } else {
        ctx.builder.use_var(ctx.exit_status_var)
    };
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
    let (Some(&true_id), Some(&false_id)) = (
        ctx.literals.get(true_msg.as_str()),
        ctx.literals.get(false_msg.as_str()),
    ) else {
        return;
    };

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

/// DD#45: embed the source as a `.sash_source` ELF section (retrievable via
/// `--extract`). The source is embedded verbatim: the coercion/rewrite
/// recorder lived in the old CST→IR lowerer and was removed with tree-sitter
/// (commit 73caecf); structural bash→sash-native rewriting now lives in the
/// separate `sash-convert` crate.
fn embed_sash_source(product: &mut cranelift_object::ObjectProduct, bash_source: Option<&str>) {
    let Some(src) = bash_source else {
        return;
    };
    let section_id =
        product
            .object
            .add_section(vec![], b".sash_source".to_vec(), object::SectionKind::Note);
    product
        .object
        .append_section_data(section_id, src.as_bytes(), 1);
}

#[cfg(test)]
mod tests {
    use super::panic_payload_message;

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
}
