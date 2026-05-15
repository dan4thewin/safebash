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
use cranelift_codegen::ir::types::{I64, I8};
use cranelift_codegen::ir::{
    instructions::BlockArg, AbiParam, Function, InstBuilder, MemFlags, Signature, StackSlotData,
    StackSlotKind, TrapCode, UserFuncName,
};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings;
use cranelift_codegen::verifier::verify_function;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{default_libcall_names, DataDescription, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};

use crate::ir::TestExpr;
use crate::ssa::*;

use super::arithmetic::emit_arith_op;
use super::commands::{lower_echo, lower_exec};
use super::dispatch::lower_inst;
use super::redirects::{
    emit_redirects, emit_restore, ir_expr_to_ssa_value, lower_ir_redirect_expr,
    scan_redirect_for_vars, SavedFd,
};
use super::tests::{emit_string_eq, lower_test_expr};
use super::types::{ExternFuncs, InstCtx, VarSlot};
use super::values::{
    emit_strlen, lower_expr_value, lower_expr_value_as_int, lower_value, lower_value_as_int,
    scan_test_for_vars, scan_value_for_vars,
};

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
    // DD#58: preprocess sexpr syntax in arithmetic
    // (arith_map may already be set by main.rs — only preprocess if not)
    let source = {
        let existing = crate::ARITH_MAP.lock().unwrap();
        if existing.is_some() {
            bash_source.to_string()
        } else {
            drop(existing);
            let (preprocessed, arith_map) = crate::preprocess_arithmetic(bash_source);
            crate::set_arith_map(arith_map);
            preprocessed
        }
    };
    let tree = crate::parse(&source);
    let program = crate::lower::lower_program(tree.root_node(), source.as_bytes());
    let type_map = crate::type_check::infer_types(&program);
    let (ssa, diags): (crate::ssa::SsaProgram, Vec<String>) =
        crate::transform::ir_to_ssa_typed(&program, type_map);

    if !diags.is_empty() {
        return Err(diags);
    }

    // Try to constant-fold elidable function calls
    let func_constants = fold_function_outputs(&ssa);

    // Catch ICE panics in codegen and convert to clean error messages
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        compile_ssa(&ssa, &func_constants, Some(bash_source), enable_trace)
    }));
    match result {
        Ok(obj) => Ok(obj),
        Err(payload) => {
            let msg = if let Some(s) = payload.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = payload.downcast_ref::<&str>() {
                s.to_string()
            } else {
                "internal compiler error in codegen".to_string()
            };
            Err(vec![msg])
        }
    }
}

/// Try to evaluate function bodies at compile time. If a function's
/// SSA blocks contain only echo with literal arguments, the output
/// is a compile-time constant string.
fn fold_function_outputs(program: &SsaProgram) -> HashMap<String, String> {
    let mut result = HashMap::new();

    for block in program.blocks.values() {
        if let Some(name) = block.label.strip_prefix("fn_") {
            if let Some(output) = try_fold_block(block, program) {
                result.insert(name.to_string(), output);
            }
        }
    }
    result
}

/// Try to evaluate a single function block as constant output.
/// Returns None if the function has non-constant output.
fn try_fold_block(block: &BasicBlock, _program: &SsaProgram) -> Option<String> {
    let mut output = String::new();

    for inst in &block.insts {
        match inst {
            SsaInst::Command { name, args, .. } if name == "echo" => {
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        output.push(' ');
                    }
                    match arg {
                        SsaValue::Literal(s) => output.push_str(s),
                        _ => return None, // non-constant argument
                    }
                }
                output.push('\n');
            }
            SsaInst::Assign { .. } => {} // assignments don't produce output
            _ => return None,            // any other instruction makes it non-constant
        }
    }

    // Bash $(cmd) strips trailing newlines
    while output.ends_with('\n') {
        output.pop();
    }

    Some(output)
}

pub fn compile_ssa(
    program: &SsaProgram,
    func_constants: &HashMap<String, String>,
    bash_source: Option<&str>,
    trace: bool,
) -> Vec<u8> {
    let reachable = program.reachable();
    let shared_flags = settings::Flags::new(settings::builder());
    let isa = cranelift_native::builder()
        .expect("host ISA")
        .finish(shared_flags)
        .expect("build ISA");

    let obj_builder = ObjectBuilder::new(isa.clone(), "sash_program", default_libcall_names())
        .expect("ObjectBuilder");
    let mut module = ObjectModule::new(obj_builder);

    let externs = super::setup::declare_runtime_functions(&mut module);
    let (literals, space_id, nl_id) =
        super::setup::intern_literals(&mut module, program, func_constants, bash_source, trace);
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
        let mut cl_blocks = fs.cl_blocks;
        let mut vars = fs.vars;
        let test_result_var = fs.test_result_var;
        let exit_status_var = fs.exit_status_var;
        let pipe_read_fd_var = fs.pipe_read_fd_var;
        let capture_saved_var = fs.capture_saved_var;
        let capture_pipe_var = fs.capture_pipe_var;
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
        // Emit code for each SSA block
        let block_ids: Vec<BlockId> = program.blocks.keys().copied().collect();
        let mut redirect_stack: Vec<Vec<SavedFd>> = Vec::new();
        for (idx, &ssa_id) in block_ids.iter().enumerate() {
            if !reachable.contains(&ssa_id) {
                continue;
            }
            let cl_block = cl_blocks[&ssa_id];

            let ssa_block = program.block(ssa_id);

            // Skip function blocks — their output was constant-folded
            if ssa_block.label.starts_with("fn_") {
                continue;
            }

            if idx > 0 {
                builder.switch_to_block(cl_block);
            }

            // DD#56: save exit status on entering END block
            if ssa_block.label == "__end__" {
                let es = builder.use_var(exit_status_var);
                builder.def_var(saved_end_status_var, es);
            }

            // Phi nodes are no-ops in Cranelift codegen: all SSA versions
            // of the same bash variable share the same Cranelift Variable,
            // so Cranelift's SSA builder handles the merge automatically.

            // Emit instructions
            let mut block_terminated = false;
            for inst in &ssa_block.insts {
                if block_terminated {
                    break;
                }
                block_terminated = lower_inst(
                    inst,
                    &mut builder,
                    &mut module,
                    &mut InstCtx {
                        vars: &mut vars,
                        literals: &literals,
                        externs: &externs,
                        space_id,
                        nl_id,
                        test_result_var,
                        exit_status_var,
                        pipe_read_fd_var,
                        capture_saved_var,
                        capture_pipe_var,
                        func_constants,
                        redirect_stack: &mut redirect_stack,
                        end_cl_block,
                        program,
                        ssa_id,
                        saved_end_status_var,
                        cl_blocks: &cl_blocks,
                        trace,
                    },
                );
            }

            // Emit terminator (unless block was already terminated by exit)
            if !block_terminated {
                match &ssa_block.terminator {
                    Terminator::Return => {
                        // DD#56: if END block exists, use saved triggering status
                        let status =
                            if program.end_block.is_some() && !ssa_block.label.starts_with("fn_") {
                                builder.use_var(saved_end_status_var)
                            } else {
                                builder.use_var(exit_status_var)
                            };
                        let exit_ref = module.declare_func_in_func(externs.exit, builder.func);
                        builder.ins().call(exit_ref, &[status]);
                        builder
                            .ins()
                            .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
                    }
                    Terminator::Goto(target) => {
                        let target_cl = cl_blocks[target];
                        builder.ins().jump(target_cl, &[]);
                    }
                    Terminator::Branch {
                        true_block,
                        false_block,
                        ..
                    } => {
                        // DD#32: trace branch outcome
                        if trace {
                            let cond_val = builder.use_var(test_result_var);
                            let true_label = program.block(*true_block).label.as_str();
                            let false_label = program.block(*false_block).label.as_str();
                            let true_msg = format!("+ branch → {}\n", true_label);
                            let false_msg = format!("+ branch → {}\n", false_label);
                            if let (Some(&true_id), Some(&false_id)) = (
                                literals.get(true_msg.as_str()),
                                literals.get(false_msg.as_str()),
                            ) {
                                let trace_gv =
                                    module.declare_data_in_func(externs.trace_flag, builder.func);
                                let trace_base = builder.ins().global_value(I64, trace_gv);
                                let trace_val =
                                    builder.ins().load(I64, MemFlags::new(), trace_base, 0);
                                let zero_t = builder.ins().iconst(I64, 0);
                                let is_tracing =
                                    builder.ins().icmp(IntCC::NotEqual, trace_val, zero_t);
                                let trace_block = builder.create_block();
                                let no_trace_block = builder.create_block();
                                builder.ins().brif(
                                    is_tracing,
                                    trace_block,
                                    &[],
                                    no_trace_block,
                                    &[],
                                );

                                builder.switch_to_block(trace_block);
                                builder.seal_block(trace_block);
                                let fd2 = builder.ins().iconst(I64, 2);
                                let write_ref =
                                    module.declare_func_in_func(externs.write, builder.func);
                                // Write the appropriate message based on condition
                                let zero2 = builder.ins().iconst(I64, 0);
                                let took_true =
                                    builder.ins().icmp(IntCC::NotEqual, cond_val, zero2);
                                let true_gv = module.declare_data_in_func(true_id, builder.func);
                                let true_ptr = builder.ins().global_value(I64, true_gv);
                                let true_len = builder.ins().iconst(I64, true_msg.len() as i64);
                                let false_gv = module.declare_data_in_func(false_id, builder.func);
                                let false_ptr = builder.ins().global_value(I64, false_gv);
                                let false_len = builder.ins().iconst(I64, false_msg.len() as i64);
                                let msg_ptr = builder.ins().select(took_true, true_ptr, false_ptr);
                                let msg_len = builder.ins().select(took_true, true_len, false_len);
                                builder.ins().call(write_ref, &[fd2, msg_ptr, msg_len]);
                                builder.ins().jump(no_trace_block, &[]);

                                builder.switch_to_block(no_trace_block);
                                builder.seal_block(no_trace_block);
                            }
                        }
                        let cond = builder.use_var(test_result_var);
                        let true_cl = cl_blocks[true_block];
                        let false_cl = cl_blocks[false_block];
                        builder.ins().brif(cond, true_cl, &[], false_cl, &[]);
                    }
                    Terminator::Loop {
                        body_block,
                        exit_block,
                        ..
                    } => {
                        if trace {
                            let cond_val = builder.use_var(test_result_var);
                            let body_label = program.block(*body_block).label.as_str();
                            let exit_label = program.block(*exit_block).label.as_str();
                            let body_msg = format!("+ loop \u{2192} {}\n", body_label);
                            let exit_msg = format!("+ loop \u{2192} {}\n", exit_label);
                            if let (Some(&body_id), Some(&exit_id)) = (
                                literals.get(body_msg.as_str()),
                                literals.get(exit_msg.as_str()),
                            ) {
                                let trace_gv =
                                    module.declare_data_in_func(externs.trace_flag, builder.func);
                                let trace_base = builder.ins().global_value(I64, trace_gv);
                                let trace_val =
                                    builder.ins().load(I64, MemFlags::new(), trace_base, 0);
                                let zero_t = builder.ins().iconst(I64, 0);
                                let is_tracing =
                                    builder.ins().icmp(IntCC::NotEqual, trace_val, zero_t);
                                let trace_block = builder.create_block();
                                let no_trace_block = builder.create_block();
                                builder.ins().brif(
                                    is_tracing,
                                    trace_block,
                                    &[],
                                    no_trace_block,
                                    &[],
                                );
                                builder.switch_to_block(trace_block);
                                builder.seal_block(trace_block);
                                let fd2 = builder.ins().iconst(I64, 2);
                                let write_ref =
                                    module.declare_func_in_func(externs.write, builder.func);
                                let zero2 = builder.ins().iconst(I64, 0);
                                let took_body =
                                    builder.ins().icmp(IntCC::NotEqual, cond_val, zero2);
                                let body_gv = module.declare_data_in_func(body_id, builder.func);
                                let body_ptr = builder.ins().global_value(I64, body_gv);
                                let body_len = builder.ins().iconst(I64, body_msg.len() as i64);
                                let exit_gv = module.declare_data_in_func(exit_id, builder.func);
                                let exit_ptr = builder.ins().global_value(I64, exit_gv);
                                let exit_len = builder.ins().iconst(I64, exit_msg.len() as i64);
                                let msg_ptr = builder.ins().select(took_body, body_ptr, exit_ptr);
                                let msg_len = builder.ins().select(took_body, body_len, exit_len);
                                builder.ins().call(write_ref, &[fd2, msg_ptr, msg_len]);
                                builder.ins().jump(no_trace_block, &[]);
                                builder.switch_to_block(no_trace_block);
                                builder.seal_block(no_trace_block);
                            }
                        }
                        let cond = builder.use_var(test_result_var);
                        let body_cl = cl_blocks[body_block];
                        let exit_cl = cl_blocks[exit_block];
                        builder.ins().brif(cond, body_cl, &[], exit_cl, &[]);
                    }
                }
            } // if !block_terminated
        }

        // Seal all blocks not yet sealed (handles complex CFGs where
        // predecessors are emitted out of order).
        builder.seal_all_blocks();
        builder.finalize();
    }

    verify_function(&func, isa.as_ref()).expect("verify function");

    let mut ctx = cranelift_codegen::Context::for_function(func);
    module
        .define_function(main_func_id, &mut ctx)
        .expect("define function");

    let mut product = module.finish();

    // Embed sash-native source in .sash_source ELF section (DD#45)
    if let Some(src) = bash_source {
        let sash_source = crate::emit_source::emit_source(src.as_bytes(), false);
        let section_id =
            product
                .object
                .add_section(vec![], b".sash_source".to_vec(), object::SectionKind::Note);
        product
            .object
            .append_section_data(section_id, sash_source.as_bytes(), 1);
    }

    product.emit().expect("emit object")
}
