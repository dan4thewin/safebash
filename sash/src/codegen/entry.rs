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

    // ── Declare runtime functions ──────────────────────────────
    let mut write_sig = Signature::new(CallConv::SystemV);
    write_sig.params.push(AbiParam::new(I64));
    write_sig.params.push(AbiParam::new(I64));
    write_sig.params.push(AbiParam::new(I64));
    write_sig.returns.push(AbiParam::new(I64));
    let write_func = module
        .declare_function("sash_write", Linkage::Import, &write_sig)
        .expect("declare sash_write");

    let mut exit_sig = Signature::new(CallConv::SystemV);
    exit_sig.params.push(AbiParam::new(I64));
    let exit_func = module
        .declare_function("sash_exit", Linkage::Import, &exit_sig)
        .expect("declare sash_exit");

    let mut run_sig = Signature::new(CallConv::SystemV);
    run_sig.params.push(AbiParam::new(I64));
    run_sig.params.push(AbiParam::new(I64));
    run_sig.returns.push(AbiParam::new(I64));
    let run_func = module
        .declare_function("sash_run", Linkage::Import, &run_sig)
        .expect("declare sash_run");

    let envp_data = module
        .declare_data("sash_envp", Linkage::Import, false, false)
        .expect("declare sash_envp");
    let argc_data = module
        .declare_data("sash_argc", Linkage::Import, false, false)
        .expect("declare sash_argc");
    let argv_data = module
        .declare_data("sash_argv", Linkage::Import, false, false)
        .expect("declare sash_argv");

    // sash_getenv(name_ptr, name_len, &out_len) -> ptr
    let mut getenv_sig = Signature::new(CallConv::SystemV);
    getenv_sig.params.push(AbiParam::new(I64)); // name ptr
    getenv_sig.params.push(AbiParam::new(I64)); // name len
    getenv_sig.params.push(AbiParam::new(I64)); // &out_len
    getenv_sig.returns.push(AbiParam::new(I64)); // val ptr
    let getenv_func = module
        .declare_function("sash_getenv", Linkage::Import, &getenv_sig)
        .expect("declare sash_getenv");

    // sash_argv_get(idx, &out_len) -> ptr  (aborts if out of range)
    let mut argv_get_sig = Signature::new(CallConv::SystemV);
    argv_get_sig.params.push(AbiParam::new(I64)); // idx (1-based)
    argv_get_sig.params.push(AbiParam::new(I64)); // &out_len
    argv_get_sig.returns.push(AbiParam::new(I64)); // str ptr
    let argv_get_func = module
        .declare_function("sash_argv_get", Linkage::Import, &argv_get_sig)
        .expect("declare sash_argv_get");

    // sash_signal_flag(signum) -> i64
    let mut signal_flag_sig = Signature::new(CallConv::SystemV);
    signal_flag_sig.params.push(AbiParam::new(I64)); // signum
    signal_flag_sig.returns.push(AbiParam::new(I64));
    let signal_flag_func = module
        .declare_function("sash_signal_flag", Linkage::Import, &signal_flag_sig)
        .expect("declare sash_signal_flag");

    // sash_dtoi(digits, ptr, len) -> i64
    let mut dtoi_sig = Signature::new(CallConv::SystemV);
    dtoi_sig.params.push(AbiParam::new(I64)); // digits
    dtoi_sig.params.push(AbiParam::new(I64)); // str ptr
    dtoi_sig.params.push(AbiParam::new(I64)); // str len
    dtoi_sig.returns.push(AbiParam::new(I64)); // int result
    let dtoi_func = module
        .declare_function("sash_dtoi", Linkage::Import, &dtoi_sig)
        .expect("declare sash_dtoi");

    // sash_itod(digits, value, &out_len) -> ptr
    let mut itod_sig = Signature::new(CallConv::SystemV);
    itod_sig.params.push(AbiParam::new(I64)); // digits
    itod_sig.params.push(AbiParam::new(I64)); // int value
    itod_sig.params.push(AbiParam::new(I64)); // &out_len
    itod_sig.returns.push(AbiParam::new(I64)); // str ptr
    let itod_func = module
        .declare_function("sash_itod", Linkage::Import, &itod_sig)
        .expect("declare sash_itod");

    // sash_signum global
    let signum_data = module
        .declare_data("sash_signum", Linkage::Import, true, false)
        .expect("declare sash_signum");

    let trace_flag_data = module
        .declare_data("sash_trace_flag", Linkage::Import, true, false)
        .expect("declare sash_trace_flag");

    // sash_capture(argv, envp, &out_len) -> ptr
    let mut capture_sig = Signature::new(CallConv::SystemV);
    capture_sig.params.push(AbiParam::new(I64)); // argv
    capture_sig.params.push(AbiParam::new(I64)); // envp
    capture_sig.params.push(AbiParam::new(I64)); // &out_len
    capture_sig.returns.push(AbiParam::new(I64)); // buf ptr
    let capture_func = module
        .declare_function("sash_capture", Linkage::Import, &capture_sig)
        .expect("declare sash_capture");

    // strlen(ptr) -> len
    let mut strlen_sig = Signature::new(CallConv::SystemV);
    strlen_sig.params.push(AbiParam::new(I64));
    strlen_sig.returns.push(AbiParam::new(I64));
    let strlen_func = module
        .declare_function("strlen", Linkage::Import, &strlen_sig)
        .expect("declare strlen");

    // sash_strip_suffix(s, s_len, pat, pat_len, longest, &out_len) -> ptr
    // sash_strip_suffix(s, s_len, pat, pat_len, longest) -> new_len
    let mut strip_suffix_sig = Signature::new(CallConv::SystemV);
    strip_suffix_sig.params.push(AbiParam::new(I64)); // s
    strip_suffix_sig.params.push(AbiParam::new(I64)); // s_len
    strip_suffix_sig.params.push(AbiParam::new(I64)); // pat
    strip_suffix_sig.params.push(AbiParam::new(I64)); // pat_len
    strip_suffix_sig.params.push(AbiParam::new(I64)); // longest
    strip_suffix_sig.returns.push(AbiParam::new(I64)); // new_len
    let strip_suffix_func = module
        .declare_function("sash_strip_suffix", Linkage::Import, &strip_suffix_sig)
        .expect("declare sash_strip_suffix");

    // sash_strip_prefix — same signature, returns offset
    let strip_prefix_func = module
        .declare_function("sash_strip_prefix", Linkage::Import, &strip_suffix_sig)
        .expect("declare sash_strip_prefix");

    // sash_default_value(s_len) -> is_set (0 or 1)
    let mut default_value_sig = Signature::new(CallConv::SystemV);
    default_value_sig.params.push(AbiParam::new(I64)); // s_len
    default_value_sig.returns.push(AbiParam::new(I64)); // is_set
    let default_value_func = module
        .declare_function("sash_default_value", Linkage::Import, &default_value_sig)
        .expect("declare sash_default_value");

    // Composable process primitives (Design Decision #14)
    // sash_fork() -> pid
    let mut fork_sig = Signature::new(CallConv::SystemV);
    fork_sig.returns.push(AbiParam::new(I64));
    let fork_func = module
        .declare_function("sash_fork", Linkage::Import, &fork_sig)
        .expect("declare sash_fork");

    // sash_exec(argv, envp) -> ! (never returns)
    let mut exec_sig = Signature::new(CallConv::SystemV);
    exec_sig.params.push(AbiParam::new(I64)); // argv
    exec_sig.params.push(AbiParam::new(I64)); // envp
    let exec_func = module
        .declare_function("sash_exec", Linkage::Import, &exec_sig)
        .expect("declare sash_exec");

    // sash_wait(pid) -> exit_status
    let mut wait_sig = Signature::new(CallConv::SystemV);
    wait_sig.params.push(AbiParam::new(I64)); // pid
    wait_sig.returns.push(AbiParam::new(I64)); // status
    let wait_func = module
        .declare_function("sash_wait", Linkage::Import, &wait_sig)
        .expect("declare sash_wait");

    // sash_pipe(out_ptr) -> 0 ok, -1 err
    let mut pipe_sig = Signature::new(CallConv::SystemV);
    pipe_sig.params.push(AbiParam::new(I64)); // out_ptr (writes [r_fd, w_fd])
    pipe_sig.returns.push(AbiParam::new(I64));
    let pipe_func = module
        .declare_function("sash_pipe", Linkage::Import, &pipe_sig)
        .expect("declare sash_pipe");

    // sash_close(fd) -> 0 ok, -1 err
    let mut close_sig = Signature::new(CallConv::SystemV);
    close_sig.params.push(AbiParam::new(I64));
    close_sig.returns.push(AbiParam::new(I64));
    let close_func = module
        .declare_function("sash_close", Linkage::Import, &close_sig)
        .expect("declare sash_close");

    // sash_dup2(src, dst) -> 0 ok, -1 err
    let mut dup2_sig = Signature::new(CallConv::SystemV);
    dup2_sig.params.push(AbiParam::new(I64));
    dup2_sig.params.push(AbiParam::new(I64));
    dup2_sig.returns.push(AbiParam::new(I64));
    let dup2_func = module
        .declare_function("sash_dup2", Linkage::Import, &dup2_sig)
        .expect("declare sash_dup2");

    // sash_dup(fd) -> new_fd
    let mut dup_sig = Signature::new(CallConv::SystemV);
    dup_sig.params.push(AbiParam::new(I64));
    dup_sig.returns.push(AbiParam::new(I64));
    let dup_func = module
        .declare_function("sash_dup", Linkage::Import, &dup_sig)
        .expect("declare sash_dup");

    // sash_redirect(fd, path, flags) -> 0 ok, -1 err
    let mut redirect_sig = Signature::new(CallConv::SystemV);
    redirect_sig.params.push(AbiParam::new(I64)); // fd
    redirect_sig.params.push(AbiParam::new(I64)); // path (null-terminated)
    redirect_sig.params.push(AbiParam::new(I64)); // flags
    redirect_sig.returns.push(AbiParam::new(I64));
    let redirect_func = module
        .declare_function("sash_redirect", Linkage::Import, &redirect_sig)
        .expect("declare sash_redirect");

    // sash_redirect_s(fd, path_ptr, path_len, flags) -> 0 ok, -1 err
    let mut redirect_s_sig = Signature::new(CallConv::SystemV);
    redirect_s_sig.params.push(AbiParam::new(I64)); // fd
    redirect_s_sig.params.push(AbiParam::new(I64)); // path ptr
    redirect_s_sig.params.push(AbiParam::new(I64)); // path len
    redirect_s_sig.params.push(AbiParam::new(I64)); // flags
    redirect_s_sig.returns.push(AbiParam::new(I64));
    let redirect_s_func = module
        .declare_function("sash_redirect_s", Linkage::Import, &redirect_s_sig)
        .expect("declare sash_redirect_s");

    // sash_read(fd, buf, len) -> bytes read
    let mut read_sig = Signature::new(CallConv::SystemV);
    read_sig.params.push(AbiParam::new(I64)); // fd
    read_sig.params.push(AbiParam::new(I64)); // buf
    read_sig.params.push(AbiParam::new(I64)); // len
    read_sig.returns.push(AbiParam::new(I64));
    let read_func = module
        .declare_function("sash_read", Linkage::Import, &read_sig)
        .expect("declare sash_read");

    // sash_alloc(size) -> ptr
    let mut alloc_sig = Signature::new(CallConv::SystemV);
    alloc_sig.params.push(AbiParam::new(I64));
    alloc_sig.returns.push(AbiParam::new(I64));
    let alloc_func = module
        .declare_function("sash_alloc", Linkage::Import, &alloc_sig)
        .expect("declare sash_alloc");

    // sash_concat(a_ptr, a_len, b_ptr, b_len, out_len_ptr) -> ptr
    let mut concat_sig = Signature::new(CallConv::SystemV);
    concat_sig.params.push(AbiParam::new(I64)); // a_ptr
    concat_sig.params.push(AbiParam::new(I64)); // a_len
    concat_sig.params.push(AbiParam::new(I64)); // b_ptr
    concat_sig.params.push(AbiParam::new(I64)); // b_len
    concat_sig.params.push(AbiParam::new(I64)); // out_len ptr
    concat_sig.returns.push(AbiParam::new(I64)); // result ptr
    let concat_func = module
        .declare_function("sash_concat", Linkage::Import, &concat_sig)
        .expect("declare sash_concat");

    // sash_atoi(s_ptr, s_len) -> i64
    let mut atoi_sig = Signature::new(CallConv::SystemV);
    atoi_sig.params.push(AbiParam::new(I64)); // s_ptr
    atoi_sig.params.push(AbiParam::new(I64)); // s_len
    atoi_sig.returns.push(AbiParam::new(I64)); // value
    let atoi_func = module
        .declare_function("sash_atoi", Linkage::Import, &atoi_sig)
        .expect("declare sash_atoi");

    // sash_itoa(val, out_len_ptr) -> ptr
    let mut itoa_sig = Signature::new(CallConv::SystemV);
    itoa_sig.params.push(AbiParam::new(I64)); // val
    itoa_sig.params.push(AbiParam::new(I64)); // out_len ptr
    itoa_sig.returns.push(AbiParam::new(I64)); // buf ptr
    let itoa_func = module
        .declare_function("sash_itoa", Linkage::Import, &itoa_sig)
        .expect("declare sash_itoa");

    // sash_join_args(argv, argc, sep_ptr, sep_len, out_len_ptr) -> ptr
    let mut join_args_sig = Signature::new(CallConv::SystemV);
    join_args_sig.params.push(AbiParam::new(I64)); // argv
    join_args_sig.params.push(AbiParam::new(I64)); // argc
    join_args_sig.params.push(AbiParam::new(I64)); // sep_ptr
    join_args_sig.params.push(AbiParam::new(I64)); // sep_len
    join_args_sig.params.push(AbiParam::new(I64)); // out_len ptr
    join_args_sig.returns.push(AbiParam::new(I64)); // buf ptr
    let join_args_func = module
        .declare_function("sash_join_args", Linkage::Import, &join_args_sig)
        .expect("declare sash_join_args");

    // sash_assoc_lookup(table, count, key_ptr, key_len, out_len_ptr) -> val_ptr
    let mut assoc_lookup_sig = Signature::new(CallConv::SystemV);
    assoc_lookup_sig.params.push(AbiParam::new(I64)); // table ptr
    assoc_lookup_sig.params.push(AbiParam::new(I64)); // count
    assoc_lookup_sig.params.push(AbiParam::new(I64)); // key_ptr
    assoc_lookup_sig.params.push(AbiParam::new(I64)); // key_len
    assoc_lookup_sig.params.push(AbiParam::new(I64)); // out_len_ptr
    assoc_lookup_sig.returns.push(AbiParam::new(I64)); // val_ptr
    let assoc_lookup_func = module
        .declare_function("sash_assoc_lookup", Linkage::Import, &assoc_lookup_sig)
        .expect("declare sash_assoc_lookup");

    // sash_regex_match(str_ptr, str_len, pat_ptr, pat_len) -> i64
    let mut regex_match_sig = module.make_signature();
    regex_match_sig.params.push(AbiParam::new(I64));
    regex_match_sig.params.push(AbiParam::new(I64));
    regex_match_sig.params.push(AbiParam::new(I64));
    regex_match_sig.params.push(AbiParam::new(I64));
    regex_match_sig.returns.push(AbiParam::new(I64));
    let regex_match_func = module
        .declare_function("sash_regex_match", Linkage::Import, &regex_match_sig)
        .expect("declare sash_regex_match");

    // sash_fnmatch(pat_ptr, pat_len, str_ptr, str_len) -> i64
    let mut fnmatch_sig = module.make_signature();
    for _ in 0..4 {
        fnmatch_sig.params.push(AbiParam::new(I64));
    }
    fnmatch_sig.returns.push(AbiParam::new(I64));
    let fnmatch_func = module
        .declare_function("sash_fnmatch", Linkage::Import, &fnmatch_sig)
        .expect("declare sash_fnmatch");

    // sash_regex_replace(str_ptr, str_len, pat_ptr, pat_len, rep_ptr, rep_len, global) -> (ptr, len)
    let mut regex_replace_sig = module.make_signature();
    for _ in 0..7 {
        regex_replace_sig.params.push(AbiParam::new(I64));
    }
    regex_replace_sig.returns.push(AbiParam::new(I64)); // ptr
    regex_replace_sig.returns.push(AbiParam::new(I64)); // len
    let regex_replace_func = module
        .declare_function("sash_regex_replace", Linkage::Import, &regex_replace_sig)
        .expect("declare sash_regex_replace");

    // sash_strcmp(ptr1, len1, ptr2, len2) -> i64 (<0, 0, >0)
    let strcmp_func = module
        .declare_function("sash_strcmp", Linkage::Import, &regex_match_sig)
        .expect("declare sash_strcmp");

    // sash_table_new(capacity) -> *mut SashTable
    let mut table_new_sig = module.make_signature();
    table_new_sig.params.push(AbiParam::new(I64));
    table_new_sig.returns.push(AbiParam::new(I64));
    let table_new_func = module
        .declare_function("sash_table_new", Linkage::Import, &table_new_sig)
        .expect("declare sash_table_new");

    // sash_table_set(table, key_ptr, key_len, val_ptr, val_len)
    let mut table_set_sig = module.make_signature();
    table_set_sig.params.push(AbiParam::new(I64)); // table
    table_set_sig.params.push(AbiParam::new(I64)); // key_ptr
    table_set_sig.params.push(AbiParam::new(I64)); // key_len
    table_set_sig.params.push(AbiParam::new(I64)); // val_ptr
    table_set_sig.params.push(AbiParam::new(I64)); // val_len
    let table_set_func = module
        .declare_function("sash_table_set", Linkage::Import, &table_set_sig)
        .expect("declare sash_table_set");

    // sash_table_get(table, key_ptr, key_len, out_len_ptr) -> val_ptr
    let mut table_get_sig = module.make_signature();
    table_get_sig.params.push(AbiParam::new(I64)); // table
    table_get_sig.params.push(AbiParam::new(I64)); // key_ptr
    table_get_sig.params.push(AbiParam::new(I64)); // key_len
    table_get_sig.params.push(AbiParam::new(I64)); // out_len_ptr
    table_get_sig.returns.push(AbiParam::new(I64)); // val_ptr
    let table_get_func = module
        .declare_function("sash_table_get", Linkage::Import, &table_get_sig)
        .expect("declare sash_table_get");

    // sash_table_len(table) -> count
    let mut table_len_sig = module.make_signature();
    table_len_sig.params.push(AbiParam::new(I64));
    table_len_sig.returns.push(AbiParam::new(I64));
    let table_len_func = module
        .declare_function("sash_table_len", Linkage::Import, &table_len_sig)
        .expect("declare sash_table_len");

    // sash_table_value(table, index, out_len_ptr) -> val_ptr
    let mut table_value_sig = module.make_signature();
    table_value_sig.params.push(AbiParam::new(I64)); // table
    table_value_sig.params.push(AbiParam::new(I64)); // index
    table_value_sig.params.push(AbiParam::new(I64)); // out_len_ptr
    table_value_sig.returns.push(AbiParam::new(I64)); // val_ptr
    let table_value_func = module
        .declare_function("sash_table_value", Linkage::Import, &table_value_sig)
        .expect("declare sash_table_value");

    // sash_table_key(table, index, out_len_ptr) -> key_ptr
    let mut table_key_sig = module.make_signature();
    table_key_sig.params.push(AbiParam::new(I64)); // table
    table_key_sig.params.push(AbiParam::new(I64)); // index
    table_key_sig.params.push(AbiParam::new(I64)); // out_len_ptr
    table_key_sig.returns.push(AbiParam::new(I64)); // key_ptr
    let table_key_func = module
        .declare_function("sash_table_key", Linkage::Import, &table_key_sig)
        .expect("declare sash_table_key");

    // sash_table_delete(table, key_ptr, key_len) -> status
    let mut table_delete_sig = module.make_signature();
    table_delete_sig.params.push(AbiParam::new(I64)); // table
    table_delete_sig.params.push(AbiParam::new(I64)); // key_ptr
    table_delete_sig.params.push(AbiParam::new(I64)); // key_len
    table_delete_sig.returns.push(AbiParam::new(I64)); // 0=ok, 1=not found
    let table_delete_func = module
        .declare_function("sash_table_delete", Linkage::Import, &table_delete_sig)
        .expect("declare sash_table_delete");

    // sash_read_line(fd) -> (ptr, len)
    let mut read_line_sig = Signature::new(CallConv::SystemV);
    read_line_sig.params.push(AbiParam::new(I64)); // fd
    read_line_sig.returns.push(AbiParam::new(I64)); // ptr
    read_line_sig.returns.push(AbiParam::new(I64)); // len
    let read_line_func = module
        .declare_function("sash_read_line", Linkage::Import, &read_line_sig)
        .expect("declare sash_read_line");

    // sash_read_delim(fd, delim_byte) -> (ptr, len)
    let mut read_delim_sig = Signature::new(CallConv::SystemV);
    read_delim_sig.params.push(AbiParam::new(I64)); // fd
    read_delim_sig.params.push(AbiParam::new(I64)); // delim byte
    read_delim_sig.returns.push(AbiParam::new(I64)); // ptr
    read_delim_sig.returns.push(AbiParam::new(I64)); // len
    let read_delim_func = module
        .declare_function("sash_read_delim", Linkage::Import, &read_delim_sig)
        .expect("declare sash_read_delim");

    // sash_splice_argv(prefix, argv, argc) -> merged_argv
    let mut splice_argv_sig = module.make_signature();
    splice_argv_sig.params.push(AbiParam::new(I64)); // prefix (null-terminated array)
    splice_argv_sig.params.push(AbiParam::new(I64)); // argv
    splice_argv_sig.params.push(AbiParam::new(I64)); // argc
    splice_argv_sig.returns.push(AbiParam::new(I64)); // merged argv ptr
    let splice_argv_func = module
        .declare_function("sash_splice_argv", Linkage::Import, &splice_argv_sig)
        .expect("declare sash_splice_argv");

    // sash_file_test(path_ptr, path_len, op) -> i64
    let mut file_test_sig = module.make_signature();
    file_test_sig.params.push(AbiParam::new(I64)); // path_ptr
    file_test_sig.params.push(AbiParam::new(I64)); // path_len
    file_test_sig.params.push(AbiParam::new(I64)); // op
    file_test_sig.returns.push(AbiParam::new(I64)); // result
    let file_test_func = module
        .declare_function("sash_file_test", Linkage::Import, &file_test_sig)
        .expect("declare sash_file_test");

    // sash_pow(base, exp) -> i64
    let mut pow_sig = module.make_signature();
    pow_sig.params.push(AbiParam::new(I64));
    pow_sig.params.push(AbiParam::new(I64));
    pow_sig.returns.push(AbiParam::new(I64));
    let pow_func = module
        .declare_function("sash_pow", Linkage::Import, &pow_sig)
        .expect("declare sash_pow");

    // sash_substring(s_ptr, s_len, offset, length, has_length) -> (ptr, len)
    let mut substring_sig = module.make_signature();
    substring_sig.params.push(AbiParam::new(I64)); // s_ptr
    substring_sig.params.push(AbiParam::new(I64)); // s_len
    substring_sig.params.push(AbiParam::new(I64)); // offset
    substring_sig.params.push(AbiParam::new(I64)); // length
    substring_sig.params.push(AbiParam::new(I64)); // has_length
    substring_sig.returns.push(AbiParam::new(I64)); // result_ptr
    substring_sig.returns.push(AbiParam::new(I64)); // result_len
    let substring_func = module
        .declare_function("sash_substring", Linkage::Import, &substring_sig)
        .expect("declare sash_substring");

    // sash_isatty(fd) -> i64
    let mut isatty_sig = module.make_signature();
    isatty_sig.params.push(AbiParam::new(I64));
    isatty_sig.returns.push(AbiParam::new(I64));
    let isatty_func = module
        .declare_function("sash_isatty", Linkage::Import, &isatty_sig)
        .expect("declare sash_isatty");

    // sash_file_newer(p1_ptr, p1_len, p2_ptr, p2_len) -> i64
    let mut file_newer_sig = module.make_signature();
    file_newer_sig.params.push(AbiParam::new(I64));
    file_newer_sig.params.push(AbiParam::new(I64));
    file_newer_sig.params.push(AbiParam::new(I64));
    file_newer_sig.params.push(AbiParam::new(I64));
    file_newer_sig.returns.push(AbiParam::new(I64));
    let file_newer_func = module
        .declare_function("sash_file_newer", Linkage::Import, &file_newer_sig)
        .expect("declare sash_file_newer");

    // sash_file_same_dev(p1_ptr, p1_len, p2_ptr, p2_len) -> i64
    let mut file_same_dev_sig = module.make_signature();
    file_same_dev_sig.params.push(AbiParam::new(I64));
    file_same_dev_sig.params.push(AbiParam::new(I64));
    file_same_dev_sig.params.push(AbiParam::new(I64));
    file_same_dev_sig.params.push(AbiParam::new(I64));
    file_same_dev_sig.returns.push(AbiParam::new(I64));
    let file_same_dev_func = module
        .declare_function("sash_file_same_dev", Linkage::Import, &file_same_dev_sig)
        .expect("declare sash_file_same_dev");

    // sash_chdir(path_ptr, path_len) -> i64
    let mut chdir_sig = module.make_signature();
    chdir_sig.params.push(AbiParam::new(I64));
    chdir_sig.params.push(AbiParam::new(I64));
    chdir_sig.returns.push(AbiParam::new(I64));
    let chdir_func = module
        .declare_function("sash_chdir", Linkage::Import, &chdir_sig)
        .expect("declare sash_chdir");

    // sash_getcwd(out_len_ptr) -> ptr
    let mut getcwd_sig = module.make_signature();
    getcwd_sig.params.push(AbiParam::new(I64)); // out_len ptr
    getcwd_sig.returns.push(AbiParam::new(I64)); // result ptr
    let getcwd_func = module
        .declare_function("sash_getcwd", Linkage::Import, &getcwd_sig)
        .expect("declare sash_getcwd");

    // sash_heredoc_redirect(fd, body_ptr, body_len)
    let mut heredoc_sig = module.make_signature();
    heredoc_sig.params.push(AbiParam::new(I64));
    heredoc_sig.params.push(AbiParam::new(I64));
    heredoc_sig.params.push(AbiParam::new(I64));
    let heredoc_func = module
        .declare_function("sash_heredoc_redirect", Linkage::Import, &heredoc_sig)
        .expect("declare sash_heredoc_redirect");

    // Case conversion functions: (s_ptr, s_len, out_ptr) -> result_len
    let mut case_sig = module.make_signature();
    case_sig.params.push(AbiParam::new(I64));
    case_sig.params.push(AbiParam::new(I64));
    case_sig.params.push(AbiParam::new(I64));
    case_sig.returns.push(AbiParam::new(I64));
    let toupper_first_func = module
        .declare_function("sash_toupper_first", Linkage::Import, &case_sig)
        .expect("declare");
    let tolower_first_func = module
        .declare_function("sash_tolower_first", Linkage::Import, &case_sig)
        .expect("declare");
    let toupper_all_func = module
        .declare_function("sash_toupper_all", Linkage::Import, &case_sig)
        .expect("declare");
    let tolower_all_func = module
        .declare_function("sash_tolower_all", Linkage::Import, &case_sig)
        .expect("declare");

    // sash_getpid() -> i64
    let mut getpid_sig = module.make_signature();
    getpid_sig.returns.push(AbiParam::new(I64));
    let getpid_func = module
        .declare_function("sash_getpid", Linkage::Import, &getpid_sig)
        .expect("declare");

    // sash_getppid() -> i64
    let getppid_func = module
        .declare_function("sash_getppid", Linkage::Import, &getpid_sig)
        .expect("declare");

    // sash_getcwd(out_len) -> ptr  (same sig as existing)
    // Already declared via pwd — reuse getcwd_func below

    // sash_epochseconds() -> (ptr, len)
    let mut epoch_sig = module.make_signature();
    epoch_sig.returns.push(AbiParam::new(I64));
    epoch_sig.returns.push(AbiParam::new(I64));
    let epochseconds_func = module
        .declare_function("sash_epochseconds", Linkage::Import, &epoch_sig)
        .expect("declare");

    // sash_glob(dir_ptr, dir_len, pat_ptr, pat_len) -> table_ptr
    let mut glob_sig = module.make_signature();
    glob_sig.params.push(AbiParam::new(I64)); // dir_ptr
    glob_sig.params.push(AbiParam::new(I64)); // dir_len
    glob_sig.params.push(AbiParam::new(I64)); // pat_ptr
    glob_sig.params.push(AbiParam::new(I64)); // pat_len
    glob_sig.returns.push(AbiParam::new(I64)); // table_ptr
    let glob_func = module
        .declare_function("sash_glob", Linkage::Import, &glob_sig)
        .expect("declare");

    let glob_regex_func = module
        .declare_function("sash_glob_regex", Linkage::Import, &glob_sig)
        .expect("declare");

    // sash_trap(signum, handler) -> i64
    let mut trap_sig = module.make_signature();
    trap_sig.params.push(AbiParam::new(I64));
    trap_sig.params.push(AbiParam::new(I64));
    trap_sig.returns.push(AbiParam::new(I64));
    let trap_func = module
        .declare_function("sash_trap", Linkage::Import, &trap_sig)
        .expect("declare");

    // sash_capture_start() -> (saved_fd, pipe_read_fd)
    let mut cap_start_sig = module.make_signature();
    cap_start_sig.returns.push(AbiParam::new(I64));
    cap_start_sig.returns.push(AbiParam::new(I64));
    let capture_start_func = module
        .declare_function("sash_capture_start", Linkage::Import, &cap_start_sig)
        .expect("declare");

    // sash_capture_end(saved_fd, pipe_read_fd) -> (ptr, len)
    let mut cap_end_sig = module.make_signature();
    cap_end_sig.params.push(AbiParam::new(I64));
    cap_end_sig.params.push(AbiParam::new(I64));
    cap_end_sig.returns.push(AbiParam::new(I64));
    cap_end_sig.returns.push(AbiParam::new(I64));
    let capture_end_func = module
        .declare_function("sash_capture_end", Linkage::Import, &cap_end_sig)
        .expect("declare");

    // sash_split_first_word(ptr, len, rest_ptr_out, rest_len_out) -> (word_ptr, word_len)
    let mut split_sig = module.make_signature();
    split_sig.params.push(AbiParam::new(I64)); // ptr
    split_sig.params.push(AbiParam::new(I64)); // len
    split_sig.params.push(AbiParam::new(I64)); // rest_ptr_out
    split_sig.params.push(AbiParam::new(I64)); // rest_len_out
    split_sig.returns.push(AbiParam::new(I64)); // word_ptr
    split_sig.returns.push(AbiParam::new(I64)); // word_len
    let split_first_word_func = module
        .declare_function("sash_split_first_word", Linkage::Import, &split_sig)
        .expect("declare");

    // sash_split_field(ptr, len, delim, rest_ptr_out, rest_len_out) -> (field_ptr, field_len)
    let mut split_field_sig = module.make_signature();
    split_field_sig.params.push(AbiParam::new(I64)); // ptr
    split_field_sig.params.push(AbiParam::new(I64)); // len
    split_field_sig.params.push(AbiParam::new(I64)); // delim byte
    split_field_sig.params.push(AbiParam::new(I64)); // rest_ptr_out
    split_field_sig.params.push(AbiParam::new(I64)); // rest_len_out
    split_field_sig.returns.push(AbiParam::new(I64)); // field_ptr
    split_field_sig.returns.push(AbiParam::new(I64)); // field_len
    let split_field_func = module
        .declare_function("sash_split_field", Linkage::Import, &split_field_sig)
        .expect("declare");

    // sash_split_into_array(ptr, len, delim, table) → void
    let mut split_arr_sig = module.make_signature();
    split_arr_sig.params.push(AbiParam::new(I64)); // ptr
    split_arr_sig.params.push(AbiParam::new(I64)); // len
    split_arr_sig.params.push(AbiParam::new(I64)); // delim
    split_arr_sig.params.push(AbiParam::new(I64)); // table
    let split_into_array_func = module
        .declare_function("sash_split_into_array", Linkage::Import, &split_arr_sig)
        .expect("declare");

    // sash_read_all(fd) → (ptr, len)
    let mut read_all_sig = module.make_signature();
    read_all_sig.params.push(AbiParam::new(I64));
    read_all_sig.returns.push(AbiParam::new(I64));
    read_all_sig.returns.push(AbiParam::new(I64));
    let read_all_func = module
        .declare_function("sash_read_all", Linkage::Import, &read_all_sig)
        .expect("declare");

    let externs = ExternFuncs {
        write: write_func,
        exit: exit_func,
        run: run_func,
        getenv: getenv_func,
        capture: capture_func,
        strlen: strlen_func,
        strip_suffix: strip_suffix_func,
        strip_prefix: strip_prefix_func,
        default_value: default_value_func,
        fork: fork_func,
        exec: exec_func,
        wait: wait_func,
        pipe: pipe_func,
        close: close_func,
        dup2: dup2_func,
        dup: dup_func,
        redirect: redirect_func,
        redirect_s: redirect_s_func,
        read: read_func,
        alloc: alloc_func,
        concat: concat_func,
        atoi: atoi_func,
        itoa: itoa_func,
        join_args: join_args_func,
        assoc_lookup: assoc_lookup_func,
        regex_match: regex_match_func,
        fnmatch: fnmatch_func,
        regex_replace: regex_replace_func,
        strcmp: strcmp_func,
        table_new: table_new_func,
        table_set: table_set_func,
        table_get: table_get_func,
        table_len: table_len_func,
        table_value: table_value_func,
        table_key: table_key_func,
        table_delete: table_delete_func,
        read_line: read_line_func,
        read_delim: read_delim_func,
        file_test: file_test_func,
        pow: pow_func,
        substring: substring_func,
        isatty: isatty_func,
        file_newer: file_newer_func,
        file_same_dev: file_same_dev_func,
        chdir: chdir_func,
        getcwd: getcwd_func,
        heredoc_redirect: heredoc_func,
        toupper_first: toupper_first_func,
        tolower_first: tolower_first_func,
        toupper_all: toupper_all_func,
        tolower_all: tolower_all_func,
        getpid: getpid_func,
        getppid: getppid_func,
        epochseconds: epochseconds_func,
        trap: trap_func,
        glob: glob_func,
        glob_regex: glob_regex_func,
        capture_start: capture_start_func,
        capture_end: capture_end_func,
        split_first_word: split_first_word_func,
        split_field: split_field_func,
        split_into_array: split_into_array_func,
        read_all: read_all_func,
        splice_argv: splice_argv_func,
        envp: envp_data,
        argc: argc_data,
        argv: argv_data,
        argv_get: argv_get_func,
        signal_flag: signal_flag_func,
        dtoi: dtoi_func,
        itod: itod_func,
        signum: signum_data,
        trace_flag: trace_flag_data,
    };

    // ── Intern string literals ─────────────────────────────────
    let mut literals: HashMap<String, cranelift_module::DataId> = HashMap::new();
    let mut intern = |module: &mut ObjectModule, s: &str| -> cranelift_module::DataId {
        if let Some(&id) = literals.get(s) {
            return id;
        }
        let name = format!("__str_{}", literals.len());
        let data_id = module
            .declare_data(&name, Linkage::Local, false, false)
            .expect("declare data");
        let mut desc = DataDescription::new();
        let mut bytes = s.as_bytes().to_vec();
        bytes.push(0);
        desc.define(bytes.into_boxed_slice());
        module.define_data(data_id, &desc).expect("define data");
        literals.insert(s.to_string(), data_id);
        data_id
    };

    let space_id = intern(&mut module, " ");
    let nl_id = intern(&mut module, "\n");
    let _empty_id = intern(&mut module, ""); // for argv bounds check

    // Pre-intern all string literals in the program
    fn intern_value(
        module: &mut ObjectModule,
        intern: &mut dyn FnMut(&mut ObjectModule, &str) -> cranelift_module::DataId,
        v: &SsaValue,
    ) {
        match v {
            SsaValue::Literal(s) | SsaValue::Expr(s) => {
                intern(module, s);
            }
            SsaValue::Concat(parts) => {
                for p in parts {
                    intern_value(module, intern, p);
                }
            }
            SsaValue::ParamExpansion {
                operator, operand, ..
            } => {
                intern(module, operand);
                if operator == "glob" || operator == "glob_regex" {
                    intern(module, ".");
                }
                if operator == "regex_replace" {
                    // operand is "pattern\0replacement\0global"
                    for part in operand.split('\0') {
                        if !part.is_empty() {
                            intern(module, part);
                        }
                    }
                }
                if operator == "/" || operator == "//" || operator == "/#" || operator == "/%" {
                    // operand is "pattern\0replacement"
                    for part in operand.split('\0') {
                        intern(module, part);
                    }
                }
                // DD#33: per-element operations on arrvals
                if operator.starts_with("arrvals_/") {
                    // operand is "regex\0replacement" (null-separated)
                    for part in operand.split('\0') {
                        intern(module, part);
                    }
                }
            }
            SsaValue::CmdSubst { argv, .. } => {
                for a in argv {
                    intern_value(module, intern, a);
                }
            }
            SsaValue::AssocArray(pairs) => {
                for (k, v) in pairs {
                    intern(module, k);
                    intern_value(module, intern, v);
                }
            }
            SsaValue::IndexedArray(elems) => {
                for v in elems {
                    intern_value(module, intern, v);
                }
            }
            SsaValue::Subscript { index, .. } => {
                intern_value(module, intern, index);
            }
            SsaValue::ArgvIndex(_) => {}
            SsaValue::Ternary {
                cond,
                true_val,
                false_val,
            } => {
                intern_value(module, intern, cond);
                intern_value(module, intern, true_val);
                intern_value(module, intern, false_val);
            }
            _ => {}
        }
    }
    fn intern_ir_expr(
        module: &mut ObjectModule,
        intern: &mut dyn FnMut(&mut ObjectModule, &str) -> cranelift_module::DataId,
        expr: &crate::ir::Expr,
    ) {
        match &expr.kind {
            crate::ir::ExprKind::Literal(s) | crate::ir::ExprKind::SingleQuoted(s) => {
                intern(module, s);
            }
            crate::ir::ExprKind::Concat(parts) | crate::ir::ExprKind::DoubleQuoted(parts) => {
                for p in parts {
                    intern_ir_expr(module, intern, p);
                }
            }
            _ => {}
        }
    }
    fn intern_test_expr(
        module: &mut ObjectModule,
        intern: &mut dyn FnMut(&mut ObjectModule, &str) -> cranelift_module::DataId,
        test: &TestExpr,
    ) {
        fn intern_expr(
            module: &mut ObjectModule,
            intern: &mut dyn FnMut(&mut ObjectModule, &str) -> cranelift_module::DataId,
            expr: &crate::ir::Expr,
        ) {
            match &expr.kind {
                crate::ir::ExprKind::Literal(s) | crate::ir::ExprKind::SingleQuoted(s) => {
                    intern(module, s);
                }
                crate::ir::ExprKind::DoubleQuoted(parts) | crate::ir::ExprKind::Concat(parts) => {
                    for p in parts {
                        intern_expr(module, intern, p);
                    }
                }
                crate::ir::ExprKind::Subscript { name, index } => {
                    intern(module, name);
                    intern_expr(module, intern, index);
                }
                _ => {}
            }
        }
        match test {
            TestExpr::Binary { left, op: _, right } => {
                intern_expr(module, intern, left);
                intern_expr(module, intern, right);
            }
            TestExpr::Unary { operand, .. } => {
                intern_expr(module, intern, operand);
            }
            TestExpr::Not(inner) | TestExpr::Paren(inner) => {
                intern_test_expr(module, intern, inner);
            }
            TestExpr::And(a, b) | TestExpr::Or(a, b) => {
                intern_test_expr(module, intern, a);
                intern_test_expr(module, intern, b);
            }
        }
    }
    for block in program.blocks.values() {
        for inst in &block.insts {
            match inst {
                SsaInst::Assign { value, .. } => {
                    intern_value(&mut module, &mut intern, value);
                }
                SsaInst::Command {
                    name,
                    args,
                    redirects,
                    ..
                } => {
                    if name != "[[" {
                        intern(&mut module, name);
                    }
                    for a in args {
                        intern_value(&mut module, &mut intern, a);
                    }
                    for r in redirects {
                        intern_ir_expr(&mut module, &mut intern, &r.target);
                    }
                }
                SsaInst::Test(test_expr) => {
                    intern_test_expr(&mut module, &mut intern, test_expr);
                }

                SsaInst::Pipeline { stages, .. } => {
                    for (name, args, redirects) in stages {
                        intern(&mut module, name);
                        for a in args {
                            intern_value(&mut module, &mut intern, a);
                        }
                        for r in redirects {
                            intern_ir_expr(&mut module, &mut intern, &r.target);
                        }
                    }
                }
                SsaInst::SubscriptAssign { index, value, .. } => {
                    intern_value(&mut module, &mut intern, index);
                    intern_value(&mut module, &mut intern, value);
                }
                SsaInst::ArrayAppend { value, .. } => {
                    intern_value(&mut module, &mut intern, value);
                }
                SsaInst::ReadLine { redirects, .. } | SsaInst::ReadLineMulti { redirects, .. } => {
                    for r in redirects {
                        intern_ir_expr(&mut module, &mut intern, &r.target);
                    }
                }
                SsaInst::Bless { test, .. } => {
                    intern_test_expr(&mut module, &mut intern, test);
                    intern(&mut module, "bless: validation failed\n");
                }
                _ => {}
            }
        }
    }

    // Intern constant-folded function outputs
    for s in func_constants.values() {
        intern(&mut module, s);
    }

    // DD#32: intern branch trace messages
    if trace {
        for block in program.blocks.values() {
            match &block.terminator {
                crate::ssa::Terminator::Branch {
                    true_block,
                    false_block,
                    ..
                } => {
                    let true_label = program.block(*true_block).label.as_str();
                    let false_label = program.block(*false_block).label.as_str();
                    intern(&mut module, &format!("+ branch \u{2192} {}\n", true_label));
                    intern(&mut module, &format!("+ branch \u{2192} {}\n", false_label));
                }
                crate::ssa::Terminator::Loop {
                    body_block,
                    exit_block,
                    ..
                } => {
                    let body_label = program.block(*body_block).label.as_str();
                    let exit_label = program.block(*exit_block).label.as_str();
                    intern(&mut module, &format!("+ loop \u{2192} {}\n", body_label));
                    intern(&mut module, &format!("+ loop \u{2192} {}\n", exit_label));
                }
                _ => {}
            }
        }
    }

    // ── Build main() ───────────────────────────────────────────
    let main_sig = Signature::new(CallConv::SystemV);
    let main_func_id = module
        .declare_function("main", Linkage::Export, &main_sig)
        .expect("declare main");

    let mut func = Function::with_name_signature(UserFuncName::user(0, 0), main_sig);

    let mut fbc = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut func, &mut fbc);

        // Create Cranelift blocks for each SSA block
        let mut cl_blocks: HashMap<BlockId, cranelift_codegen::ir::Block> = HashMap::new();
        for &id in program.blocks.keys() {
            if !reachable.contains(&id) {
                continue;
            }
            let cl_block = builder.create_block();
            cl_blocks.insert(id, cl_block);
        }

        let entry_cl = cl_blocks[&program.entry];
        // DD#56: resolve end_block CL target for abort paths
        let end_cl_block: Option<cranelift_codegen::ir::Block> =
            program.end_block.and_then(|id| cl_blocks.get(&id).copied());
        builder.append_block_params_for_function_params(entry_cl);
        builder.switch_to_block(entry_cl);

        // Declare Cranelift variables for each bash variable.
        // All SSA versions of the same bash variable (x_1, x_2, x_3) share
        // the SAME Cranelift Variable pair. Cranelift's SSA builder handles
        // the merge at join points automatically — our SSA versioning is for
        // the oracle and printer, not for Cranelift.
        let mut vars: HashMap<String, VarSlot> = HashMap::new();
        let mut base_vars: HashMap<String, VarSlot> = HashMap::new();
        // First pass: collect all base variable names
        for block in program.blocks.values() {
            for inst in &block.insts {
                if let SsaInst::ReadLineMulti { targets, .. } = inst {
                    for target in targets {
                        let base = target.name.clone();
                        base_vars.entry(base).or_insert_with(|| VarSlot::Str {
                            ptr: builder.declare_var(I64),
                            len: builder.declare_var(I64),
                        });
                    }
                }
                if let SsaInst::Mapfile { array, .. } | SsaInst::ReadArray { array, .. } = inst {
                    if !base_vars.contains_key(array) {
                        let sv = VarSlot::Str {
                            ptr: builder.declare_var(I64),
                            len: builder.declare_var(I64),
                        };
                        base_vars.insert(array.clone(), sv);
                    }
                }
                if let SsaInst::Command { name, args, .. } = inst {
                    if name == "__store_bg_pid" {
                        if let Some(SsaValue::Var(vr)) = args.first() {
                            let base = vr.name.clone();
                            base_vars.entry(base).or_insert_with(|| VarSlot::Str {
                                ptr: builder.declare_var(I64),
                                len: builder.declare_var(I64),
                            });
                        }
                    }
                }
                if let SsaInst::Assign { target, .. }
                | SsaInst::ReadLine { target, .. }
                | SsaInst::CaptureEnd { target } = inst
                {
                    let base = target.name.clone();
                    base_vars.entry(base).or_insert_with(|| {
                        if target.var_type == crate::ir::VarType::Int {
                            VarSlot::Int(builder.declare_var(I64))
                        } else {
                            VarSlot::Str {
                                ptr: builder.declare_var(I64),
                                len: builder.declare_var(I64),
                            }
                        }
                    });
                }
            }
            for phi in &block.phis {
                let base = phi.target.name.clone();
                base_vars.entry(base).or_insert_with(|| {
                    if phi.target.var_type == crate::ir::VarType::Int {
                        VarSlot::Int(builder.declare_var(I64))
                    } else {
                        VarSlot::Str {
                            ptr: builder.declare_var(I64),
                            len: builder.declare_var(I64),
                        }
                    }
                });
            }
        }
        // Second pass: map every versioned name to its base variable's CL vars
        for block in program.blocks.values() {
            for inst in &block.insts {
                if let SsaInst::ReadLineMulti { targets, .. } = inst {
                    for target in targets {
                        let key = target.display();
                        vars.entry(key).or_insert_with(|| base_vars[&target.name]);
                    }
                }
                if let SsaInst::Assign { target, .. }
                | SsaInst::ReadLine { target, .. }
                | SsaInst::CaptureEnd { target } = inst
                {
                    let key = target.display();
                    vars.entry(key).or_insert_with(|| base_vars[&target.name]);
                }
                if let SsaInst::Command { name, args, .. } = inst {
                    if name == "__store_bg_pid" {
                        if let Some(SsaValue::Var(vr)) = args.first() {
                            let key = vr.display();
                            vars.entry(key).or_insert_with(|| base_vars[&vr.name]);
                        }
                    }
                }
            }
            for phi in &block.phis {
                let key = phi.target.display();
                vars.entry(key)
                    .or_insert_with(|| base_vars[&phi.target.name]);
                for (_, src) in &phi.sources {
                    let sk = src.display();
                    vars.entry(sk).or_insert_with(|| base_vars[&src.name]);
                }
            }
        }

        // Also declare a "last_test_result" variable for [[ ]] conditions
        let test_result_var = builder.declare_var(I64);
        let zero = builder.ins().iconst(I64, 0);
        builder.def_var(test_result_var, zero);

        // $? — exit status of last command (0 = success)
        let exit_status_var = builder.declare_var(I64);
        builder.def_var(exit_status_var, zero);

        // DD#56: saved exit status for END block (preserves triggering status)
        let saved_end_status_var = builder.declare_var(I64);
        builder.def_var(saved_end_status_var, zero);

        // Pipe read fd for pipe-while-read pattern (fd from captured pipeline)
        let pipe_read_fd_var = builder.declare_var(I64);
        builder.def_var(pipe_read_fd_var, zero);

        // Capture state vars for $(func) capture
        let capture_saved_var = builder.declare_var(I64);
        builder.def_var(capture_saved_var, zero);
        let capture_pipe_var = builder.declare_var(I64);
        builder.def_var(capture_pipe_var, zero);

        // ── Initialize positional params from sash_argv ───────
        // Scan the SSA for references to positional params (1, 2, ...)
        // and special vars (#, @, 0) and pre-define them.
        {
            let argv_gv = module.declare_data_in_func(externs.argv, builder.func);
            let argv_ptr = builder.ins().global_value(I64, argv_gv);
            let argv_val = builder.ins().load(I64, MemFlags::trusted(), argv_ptr, 0);

            let argc_gv = module.declare_data_in_func(externs.argc, builder.func);
            let argc_ptr = builder.ins().global_value(I64, argc_gv);
            let argc_val = builder.ins().load(I64, MemFlags::trusted(), argc_ptr, 0);

            // Pre-scan: collect all referenced positional/special var names
            let mut positional_refs: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            for block in program.blocks.values() {
                for inst in &block.insts {
                    match inst {
                        SsaInst::Assign { value, .. } => {
                            scan_value_for_vars(value, &mut positional_refs)
                        }
                        SsaInst::Command {
                            args, redirects, ..
                        } => {
                            for a in args {
                                scan_value_for_vars(a, &mut positional_refs);
                            }
                            for r in redirects {
                                scan_redirect_for_vars(&r.target, &mut positional_refs);
                            }
                        }
                        SsaInst::Pipeline { stages, .. } => {
                            for (_, args, redirects) in stages {
                                for a in args {
                                    scan_value_for_vars(a, &mut positional_refs);
                                }
                                for r in redirects {
                                    scan_redirect_for_vars(&r.target, &mut positional_refs);
                                }
                            }
                        }
                        SsaInst::Test(test_expr) => {
                            scan_test_for_vars(test_expr, &mut positional_refs);
                        }
                        _ => {}
                    }
                }
            }

            for var_name in &positional_refs {
                if var_name == "?" {
                    continue;
                } // $? handled via exit_status_var
                if var_name == "$" {
                    continue;
                } // $$ handled via sash_getpid
                let key_suffix = "_0"; // version 0
                let key = format!("{}{}", var_name, key_suffix);
                if vars.contains_key(&key) {
                    continue;
                }

                let sv = VarSlot::Str {
                    ptr: builder.declare_var(I64),
                    len: builder.declare_var(I64),
                };

                if let Ok(idx) = var_name.parse::<i64>() {
                    // Positional: $1, $2, ... — load from argv[idx]
                    // Guard: only load if idx < argc (arg exists)
                    let idx_val = builder.ins().iconst(I64, idx);
                    let in_bounds = builder.ins().icmp(IntCC::SignedLessThan, idx_val, argc_val);

                    let load_block = builder.create_block();
                    let empty_block = builder.create_block();
                    let done_block = builder.create_block();

                    builder
                        .ins()
                        .brif(in_bounds, load_block, &[], empty_block, &[]);

                    builder.switch_to_block(load_block);
                    builder.seal_block(load_block);
                    let offset = builder.ins().imul_imm(idx_val, 8);
                    let arg_ptr_addr = builder.ins().iadd(argv_val, offset);
                    let arg_ptr = builder
                        .ins()
                        .load(I64, MemFlags::trusted(), arg_ptr_addr, 0);
                    let len = emit_strlen(&mut builder, &mut module, externs.strlen, arg_ptr);
                    builder.def_var(sv.str_ptr(), arg_ptr);
                    builder.def_var(sv.str_len(), len);
                    builder.ins().jump(done_block, &[]);

                    builder.switch_to_block(empty_block);
                    builder.seal_block(empty_block);
                    let null_ptr = builder.ins().iconst(I64, 0);
                    let zero_len = builder.ins().iconst(I64, 0);
                    builder.def_var(sv.str_ptr(), null_ptr);
                    builder.def_var(sv.str_len(), zero_len);
                    builder.ins().jump(done_block, &[]);

                    builder.switch_to_block(done_block);
                    builder.seal_block(done_block);
                } else if var_name == "#" {
                    // $# = argc - 1 (program name excluded)
                    let one = builder.ins().iconst(I64, 1);
                    let count = builder.ins().isub(argc_val, one);
                    // Convert integer to string via sash_itoa
                    let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        8,
                        3,
                    ));
                    let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                    let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                    let call = builder.ins().call(itoa_ref, &[count, out_addr]);
                    let str_ptr = builder.inst_results(call)[0];
                    let str_len = builder.ins().stack_load(I64, out_slot, 0);
                    builder.def_var(sv.str_ptr(), str_ptr);
                    builder.def_var(sv.str_len(), str_len);
                } else if var_name == "@" {
                    // $@ = join all positional params with space
                    let space_id = intern(&mut module, " ");
                    let space_gv = module.declare_data_in_func(space_id, builder.func);
                    let space_ptr = builder.ins().global_value(I64, space_gv);
                    let space_len = builder.ins().iconst(I64, 1);
                    let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        8,
                        3,
                    ));
                    let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                    let join_ref = module.declare_func_in_func(externs.join_args, builder.func);
                    let call = builder.ins().call(
                        join_ref,
                        &[argv_val, argc_val, space_ptr, space_len, out_addr],
                    );
                    let str_ptr = builder.inst_results(call)[0];
                    let str_len = builder.ins().stack_load(I64, out_slot, 0);
                    builder.def_var(sv.str_ptr(), str_ptr);
                    builder.def_var(sv.str_len(), str_len);
                } else if var_name == "?" {
                    // $? = exit status of last command, convert to string
                    let status = builder.use_var(exit_status_var);
                    let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        8,
                        3,
                    ));
                    let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                    let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                    let call = builder.ins().call(itoa_ref, &[status, out_addr]);
                    let str_ptr = builder.inst_results(call)[0];
                    let str_len = builder.ins().stack_load(I64, out_slot, 0);
                    builder.def_var(sv.str_ptr(), str_ptr);
                    builder.def_var(sv.str_len(), str_len);
                } else if var_name == "$" {
                    // $$ = PID — initialize with sash_getpid
                    let getpid_ref = module.declare_func_in_func(externs.getpid, builder.func);
                    let call = builder.ins().call(getpid_ref, &[]);
                    let pid = builder.inst_results(call)[0];
                    let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        8,
                        3,
                    ));
                    let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                    let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                    let call2 = builder.ins().call(itoa_ref, &[pid, out_addr]);
                    let str_ptr = builder.inst_results(call2)[0];
                    let str_len = builder.ins().stack_load(I64, out_slot, 0);
                    builder.def_var(sv.str_ptr(), str_ptr);
                    builder.def_var(sv.str_len(), str_len);
                } else {
                    // Undeclared variable — initialize to empty string (DD#53)
                    // Use getenv() builtin explicitly to access environment.
                    // Intern the name so source-kvp re-read can find it.
                    intern(&mut module, var_name);
                    let empty_id = intern(&mut module, "");
                    let empty_gv = module.declare_data_in_func(empty_id, builder.func);
                    let empty_ptr = builder.ins().global_value(I64, empty_gv);
                    let empty_len = builder.ins().iconst(I64, 0);
                    builder.def_var(sv.str_ptr(), empty_ptr);
                    builder.def_var(sv.str_len(), empty_len);
                }

                vars.insert(key, sv);
            }
        }

        // Seal the entry block if it has no predecessors besides itself
        // (we'll seal blocks as we go)

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

/// Returns true if the block is now terminated (e.g., by exit).
#[allow(clippy::too_many_arguments)]
fn lower_inst(
    inst: &SsaInst,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    ctx: &mut InstCtx,
) -> bool {
    let vars = &mut *ctx.vars;
    let literals = ctx.literals;
    let externs = ctx.externs;
    let space_id = ctx.space_id;
    let nl_id = ctx.nl_id;
    let test_result_var = ctx.test_result_var;
    let exit_status_var = ctx.exit_status_var;
    let pipe_read_fd_var = ctx.pipe_read_fd_var;
    let capture_saved_var = ctx.capture_saved_var;
    let capture_pipe_var = ctx.capture_pipe_var;
    let func_constants = ctx.func_constants;
    let redirect_stack = &mut *ctx.redirect_stack;
    let end_cl_block = ctx.end_cl_block;
    let program = ctx.program;
    let ssa_id = ctx.ssa_id;
    let saved_end_status_var = ctx.saved_end_status_var;
    let cl_blocks = ctx.cl_blocks;
    let _trace = ctx.trace;
    match inst {
        SsaInst::Assign { target, value } => {
            // DD#30: unset on collection → new empty table
            if let SsaValue::Literal(s) = value {
                if s == "__new_table__" {
                    let sv = vars[&target.display()];
                    let cap = builder.ins().iconst(I64, 0);
                    let new_ref = module.declare_func_in_func(externs.table_new, builder.func);
                    let call = builder.ins().call(new_ref, &[cap]);
                    let tbl_ptr = builder.inst_results(call)[0];
                    builder.def_var(sv.str_ptr(), tbl_ptr);
                    let zero_len = builder.ins().iconst(I64, 0);
                    builder.def_var(sv.str_len(), zero_len);
                    return false;
                }
            }
            // Special: assignment to $? updates the exit_status_var directly
            let base = {
                let d = target.display();
                if let Some(idx) = d.rfind('_') {
                    let suffix = &d[idx + 1..];
                    if suffix.chars().all(|c| c.is_ascii_digit()) {
                        d[..idx].to_string()
                    } else {
                        d.to_string()
                    }
                } else {
                    d.to_string()
                }
            };
            if base == "?" {
                // Parse the literal value as an integer for exit_status_var
                let int_val = match value {
                    SsaValue::Literal(s) => {
                        let n = s.parse::<i64>().unwrap_or(0);
                        builder.ins().iconst(I64, n)
                    }
                    _ => {
                        let (ptr, len) = lower_value(
                            value,
                            builder,
                            module,
                            vars,
                            literals,
                            func_constants,
                            externs,
                            Some(exit_status_var),
                        );
                        let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
                        let call = builder.ins().call(atoi_ref, &[ptr, len]);
                        builder.inst_results(call)[0]
                    }
                };
                builder.def_var(exit_status_var, int_val);
            } else {
                let sv = vars[&target.display()];
                if sv.is_int() {
                    // Int var: evaluate value as i64 and store directly
                    let int_val = lower_value_as_int(
                        value,
                        builder,
                        module,
                        vars,
                        literals,
                        func_constants,
                        externs,
                    );
                    builder.def_var(*sv.as_int(), int_val);
                } else {
                    let (ptr_val, len_val) = lower_value(
                        value,
                        builder,
                        module,
                        vars,
                        literals,
                        func_constants,
                        externs,
                        Some(exit_status_var),
                    );
                    builder.def_var(sv.str_ptr(), ptr_val);
                    builder.def_var(sv.str_len(), len_val);
                }
            }
        }
        SsaInst::Command {
            name,
            args,
            redirects,
            ..
        } if name == "echo" => {
            let restore = emit_redirects(
                redirects,
                builder,
                module,
                externs,
                literals,
                vars,
                Some(exit_status_var),
            );
            lower_echo(
                args,
                builder,
                module,
                vars,
                literals,
                externs,
                space_id,
                nl_id,
                Some(exit_status_var),
            );
            emit_restore(restore, builder, module, externs);
            // echo always succeeds — set test_result for && / || chains
            let one = builder.ins().iconst(I64, 1);
            builder.def_var(test_result_var, one);
            let zero = builder.ins().iconst(I64, 0);
            builder.def_var(exit_status_var, zero);
        }
        SsaInst::Command { name, args, .. } if name == "[[" => {
            // Legacy path — should not be reached now that transform
            // emits SsaInst::Test directly. Treat as always-true.
            let one = builder.ins().iconst(I64, 1);
            builder.def_var(test_result_var, one);
        }
        SsaInst::Command { name, .. } if name == "__negate_test_result" => {
            let cur = builder.use_var(test_result_var);
            let one = builder.ins().iconst(I64, 1);
            let flipped = builder.ins().isub(one, cur);
            builder.def_var(test_result_var, flipped);
            // Also flip exit status so $? reflects the negation
            let cur_es = builder.use_var(exit_status_var);
            let zero = builder.ins().iconst(I64, 0);
            let is_zero = builder.ins().icmp(IntCC::Equal, cur_es, zero);
            let new_es = builder.ins().select(is_zero, one, zero);
            builder.def_var(exit_status_var, new_es);
        }
        SsaInst::Command { name, args, .. } if name == "__let_exit" => {
            // let builtin: exit status 0 if value != 0, 1 if value == 0
            if let Some(SsaValue::Var(vr)) = args.first() {
                let key = format!("{}_{}", vr.name, vr.version);
                let int_val = if let Some(slot) = vars.get(&key) {
                    if slot.is_int() {
                        builder.use_var(*slot.as_int())
                    } else {
                        let ptr = builder.use_var(slot.str_ptr());
                        let len = builder.use_var(slot.str_len());
                        let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
                        let call = builder.ins().call(atoi_ref, &[ptr, len]);
                        builder.inst_results(call)[0]
                    }
                } else {
                    builder.ins().iconst(I64, 0)
                };
                let zero = builder.ins().iconst(I64, 0);
                let is_zero = builder.ins().icmp(IntCC::Equal, int_val, zero);
                let one = builder.ins().iconst(I64, 1);
                let es = builder.ins().select(is_zero, one, zero);
                builder.def_var(exit_status_var, es);
                // Also set test_result_var for branch terminators (if/while)
                let tr = builder.ins().select(is_zero, zero, one);
                builder.def_var(test_result_var, tr);
            }
        }
        SsaInst::Command { name, .. } if name == "__subshell" => {
            // Fork: pid = sash_fork(). Store pid in test_result_var
            // so the Branch terminator can dispatch on it.
            let fork_ref = module.declare_func_in_func(externs.fork, builder.func);
            let call = builder.ins().call(fork_ref, &[]);
            let pid = builder.inst_results(call)[0];
            builder.def_var(test_result_var, pid);
        }
        SsaInst::Command { name, .. } if name == "__subshell_wait" => {
            // Parent: wait for the child whose pid is in test_result_var
            let pid = builder.use_var(test_result_var);
            let wait_ref = module.declare_func_in_func(externs.wait, builder.func);
            let call = builder.ins().call(wait_ref, &[pid]);
            let status = builder.inst_results(call)[0];
            builder.def_var(exit_status_var, status);
        }
        SsaInst::Command {
            name, redirects, ..
        } if name == "__redirect_push" => {
            // Compound-command redirect: save fds, apply redirects
            let saved = emit_redirects(
                redirects,
                builder,
                module,
                externs,
                literals,
                vars,
                Some(exit_status_var),
            );
            redirect_stack.push(saved);
        }
        SsaInst::Command { name, .. } if name == "__redirect_pop" => {
            // Compound-command redirect: restore saved fds
            if let Some(saved) = redirect_stack.pop() {
                emit_restore(saved, builder, module, externs);
            }
        }
        SsaInst::Command { name, args, .. } if name == "__store_bg_pid" => {
            // Store fork pid (test_result_var) into $! variable
            if let Some(SsaValue::Var(vr)) = args.first() {
                let pid = builder.use_var(test_result_var);
                // Convert pid to string via itoa
                let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                let call = builder.ins().call(itoa_ref, &[pid, out_addr]);
                let str_ptr = builder.inst_results(call)[0];
                let str_len = builder.ins().stack_load(I64, out_slot, 0);
                let sv = &vars[&vr.display()];
                builder.def_var(sv.str_ptr(), str_ptr);
                builder.def_var(sv.str_len(), str_len);
            }
        }
        SsaInst::Command { name, args, .. } if name == "__trap_signal" => {
            // trap '' SIG or trap - SIG: call sash_trap(signum, handler)
            if args.len() >= 2 {
                if let (SsaValue::Literal(sig_s), SsaValue::Literal(handler_s)) =
                    (&args[0], &args[1])
                {
                    let signum = builder.ins().iconst(I64, sig_s.parse::<i64>().unwrap_or(0));
                    let handler = builder
                        .ins()
                        .iconst(I64, handler_s.parse::<i64>().unwrap_or(0));
                    let trap_ref = module.declare_func_in_func(externs.trap, builder.func);
                    builder.ins().call(trap_ref, &[signum, handler]);
                }
            }
        }
        SsaInst::Command { name, args, .. } if name == "__signal_flag" => {
            // signal N FLAG: call sash_signal_flag(signum)
            if let Some(SsaValue::Literal(sig_s)) = args.first() {
                let signum = builder.ins().iconst(I64, sig_s.parse::<i64>().unwrap_or(0));
                let flag_ref = module.declare_func_in_func(externs.signal_flag, builder.func);
                builder.ins().call(flag_ref, &[signum]);
            }
        }
        SsaInst::Command { name, args, .. } if name == "__table_delete" => {
            // unset map[key] → sash_table_delete(table_ptr, key_ptr, key_len)
            if args.len() >= 2 {
                let (tbl_ptr, _) = lower_value(
                    &args[0],
                    builder,
                    module,
                    vars,
                    literals,
                    func_constants,
                    externs,
                    Some(exit_status_var),
                );
                let (key_ptr, key_len) = lower_value(
                    &args[1],
                    builder,
                    module,
                    vars,
                    literals,
                    func_constants,
                    externs,
                    Some(exit_status_var),
                );
                let del_ref = module.declare_func_in_func(externs.table_delete, builder.func);
                builder.ins().call(del_ref, &[tbl_ptr, key_ptr, key_len]);
            }
        }
        SsaInst::Command { name, .. } if name == ":" || name == "true" || name == "false" => {
            let val = if name == "false" { 0i64 } else { 1i64 };
            let v = builder.ins().iconst(I64, val);
            builder.def_var(test_result_var, v);
            // $? = 0 for true/:, 1 for false
            let es = builder
                .ins()
                .iconst(I64, if name == "false" { 1 } else { 0 });
            builder.def_var(exit_status_var, es);
        }
        SsaInst::Command { name, args, .. } if name == "exit" => {
            // exit N — get the exit code from first arg, default 0
            let code = if let Some(SsaValue::Literal(s)) = args.first() {
                s.parse::<i64>().unwrap_or(0)
            } else {
                0
            };
            let code_val = builder.ins().iconst(I64, code);
            // DD#56: if END block exists and we're NOT inside it, jump to END
            if let Some(end_cl) = end_cl_block {
                if let Some(end_id) = program.end_block {
                    if ssa_id < end_id {
                        builder.def_var(exit_status_var, code_val);
                        builder.ins().jump(end_cl, &[]);
                        return true;
                    }
                }
            }
            // Direct exit (no END block, or inside END block)
            // Inside END: also update saved_end_status_var so Return uses it
            builder.def_var(saved_end_status_var, code_val);
            let exit_ref = module.declare_func_in_func(externs.exit, builder.func);
            builder.ins().call(exit_ref, &[code_val]);
            builder
                .ins()
                .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
            return true; // block is terminated
        }
        SsaInst::Command {
            name,
            args,
            redirects,
            ..
        } if name == "exec" => {
            // exec with no args = standalone redirects only (fd manipulation)
            if args.is_empty() {
                if !redirects.is_empty() {
                    emit_redirects(
                        redirects,
                        builder,
                        module,
                        externs,
                        literals,
                        vars,
                        Some(exit_status_var),
                    );
                }
                let zero = builder.ins().iconst(I64, 0);
                builder.def_var(exit_status_var, zero);
            } else {
                // exec cmd args... — replace process (no fork)
                // Check if any arg is $@ — if so, we need a dynamic-size argv
                let has_dollar_at = args.iter().any(|a| {
                    if let SsaValue::Var(vref) = a {
                        vref.name == "@"
                    } else {
                        false
                    }
                });
                if has_dollar_at {
                    // Build prefix array (args before $@) on the stack, null-terminated
                    let prefix_args: Vec<_> = args
                        .iter()
                        .filter(|a| {
                            if let SsaValue::Var(vref) = a {
                                vref.name != "@"
                            } else {
                                true
                            }
                        })
                        .collect();
                    let prefix_size = ((prefix_args.len() + 1) * 8) as u32;
                    let prefix_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        prefix_size,
                        3,
                    ));
                    for (i, arg_val) in prefix_args.iter().enumerate() {
                        let (ptr, _len) = lower_value(
                            arg_val,
                            builder,
                            module,
                            vars,
                            literals,
                            func_constants,
                            externs,
                            Some(exit_status_var),
                        );
                        builder.ins().stack_store(ptr, prefix_slot, (i * 8) as i32);
                    }
                    let null = builder.ins().iconst(I64, 0);
                    builder
                        .ins()
                        .stack_store(null, prefix_slot, (prefix_args.len() * 8) as i32);
                    let prefix_addr = builder.ins().stack_addr(I64, prefix_slot, 0);
                    // Load argv and argc
                    let argv_gv = module.declare_data_in_func(externs.argv, builder.func);
                    let argv_ptr = builder.ins().global_value(I64, argv_gv);
                    let argv_val = builder.ins().load(I64, MemFlags::new(), argv_ptr, 0);
                    let argc_gv = module.declare_data_in_func(externs.argc, builder.func);
                    let argc_ptr = builder.ins().global_value(I64, argc_gv);
                    let argc_val_exec = builder.ins().load(I64, MemFlags::new(), argc_ptr, 0);
                    // Call sash_splice_argv(prefix, argv, argc) -> merged argv
                    let splice_ref = module.declare_func_in_func(externs.splice_argv, builder.func);
                    let call = builder
                        .ins()
                        .call(splice_ref, &[prefix_addr, argv_val, argc_val_exec]);
                    let merged_argv = builder.inst_results(call)[0];
                    // Load envp
                    let envp_gv = module.declare_data_in_func(externs.envp, builder.func);
                    let envp_ptr = builder.ins().global_value(I64, envp_gv);
                    let envp_val = builder.ins().load(I64, MemFlags::new(), envp_ptr, 0);
                    // exec(merged_argv, envp)
                    let exec_ref = module.declare_func_in_func(externs.exec, builder.func);
                    builder.ins().call(exec_ref, &[merged_argv, envp_val]);
                    let code = builder.ins().iconst(I64, 127);
                    let exit_ref = module.declare_func_in_func(externs.exit, builder.func);
                    builder.ins().call(exit_ref, &[code]);
                    builder
                        .ins()
                        .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
                    return true;
                }
                // Static argv path (no $@)
                let argc = args.len();
                let argv_size = ((argc + 1) * 8) as u32;
                let argv_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    argv_size,
                    3,
                ));
                for (i, arg_val) in args.iter().enumerate() {
                    let (ptr, _len) = lower_value(
                        arg_val,
                        builder,
                        module,
                        vars,
                        literals,
                        func_constants,
                        externs,
                        Some(exit_status_var),
                    );
                    builder.ins().stack_store(ptr, argv_slot, (i * 8) as i32);
                }
                // Null terminator
                let null = builder.ins().iconst(I64, 0);
                builder
                    .ins()
                    .stack_store(null, argv_slot, (argc * 8) as i32);
                let argv_addr = builder.ins().stack_addr(I64, argv_slot, 0);
                // Load envp
                let envp_gv = module.declare_data_in_func(externs.envp, builder.func);
                let envp_ptr = builder.ins().global_value(I64, envp_gv);
                let envp_val = builder.ins().load(I64, MemFlags::new(), envp_ptr, 0);
                // exec(argv, envp) — does not return on success
                let exec_ref = module.declare_func_in_func(externs.exec, builder.func);
                builder.ins().call(exec_ref, &[argv_addr, envp_val]);
                // If exec fails, exit 127
                let code = builder.ins().iconst(I64, 127);
                let exit_ref = module.declare_func_in_func(externs.exit, builder.func);
                builder.ins().call(exit_ref, &[code]);
                builder
                    .ins()
                    .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
                return true;
            }
        }
        SsaInst::Command { name, args, .. } if name == "cd" => {
            // Determine target: explicit arg, "-" (OLDPWD), or no arg (HOME)
            let is_dash = matches!(args.first(), Some(SsaValue::Literal(s)) if s == "-");

            if is_dash {
                // cd - → cd to $OLDPWD, then print new dir
                if let Some(oldpwd_slot) = vars.get("OLDPWD_0").cloned() {
                    let old_ptr = builder.use_var(oldpwd_slot.str_ptr());
                    let old_len = builder.use_var(oldpwd_slot.str_len());
                    let chdir_ref = module.declare_func_in_func(externs.chdir, builder.func);
                    let call = builder.ins().call(chdir_ref, &[old_ptr, old_len]);
                    let result = builder.inst_results(call)[0];
                    let zero = builder.ins().iconst(I64, 0);
                    let failed = builder.ins().icmp(IntCC::SignedLessThan, result, zero);
                    let es = builder.ins().uextend(I64, failed);
                    builder.def_var(exit_status_var, es);
                    // Update OLDPWD = current PWD, then PWD = getcwd()
                    if let Some(pwd_slot) = vars.get("PWD_0").cloned() {
                        let cur_ptr = builder.use_var(pwd_slot.str_ptr());
                        let cur_len = builder.use_var(pwd_slot.str_len());
                        builder.def_var(oldpwd_slot.str_ptr(), cur_ptr);
                        builder.def_var(oldpwd_slot.str_len(), cur_len);
                        let len_slot2 = builder.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            8,
                            3,
                        ));
                        let len_addr2 = builder.ins().stack_addr(I64, len_slot2, 0);
                        let getcwd_ref2 = module.declare_func_in_func(externs.getcwd, builder.func);
                        let call2 = builder.ins().call(getcwd_ref2, &[len_addr2]);
                        let new_ptr = builder.inst_results(call2)[0];
                        let new_len = builder.ins().stack_load(I64, len_slot2, 0);
                        builder.def_var(pwd_slot.str_ptr(), new_ptr);
                        builder.def_var(pwd_slot.str_len(), new_len);
                        // Print new dir (bash cd - behavior)
                        let write_ref = module.declare_func_in_func(externs.write, builder.func);
                        let stdout_fd = builder.ins().iconst(I64, 1);
                        builder
                            .ins()
                            .call(write_ref, &[stdout_fd, new_ptr, new_len]);
                        let nl_gv = module.declare_data_in_func(nl_id, builder.func);
                        let nl_ptr2 = builder.ins().global_value(I64, nl_gv);
                        let one = builder.ins().iconst(I64, 1);
                        builder.ins().call(write_ref, &[stdout_fd, nl_ptr2, one]);
                    }
                }
            } else if let Some(arg) = args.first() {
                let (ptr, len) = lower_value(
                    arg,
                    builder,
                    module,
                    vars,
                    literals,
                    func_constants,
                    externs,
                    Some(exit_status_var),
                );
                let chdir_ref = module.declare_func_in_func(externs.chdir, builder.func);
                let call = builder.ins().call(chdir_ref, &[ptr, len]);
                let result = builder.inst_results(call)[0];
                // Set $? based on result (0=success, -1=failure → 1)
                let zero = builder.ins().iconst(I64, 0);
                let failed = builder.ins().icmp(IntCC::SignedLessThan, result, zero);
                let es = builder.ins().uextend(I64, failed);
                builder.def_var(exit_status_var, es);

                // Update $PWD and $OLDPWD after successful cd
                // Save current PWD as OLDPWD, then getcwd for new PWD
                if let Some(pwd_slot) = vars.get("PWD_0").cloned() {
                    if let Some(oldpwd_slot) = vars.get("OLDPWD_0").cloned() {
                        // OLDPWD = current PWD
                        let cur_ptr = builder.use_var(pwd_slot.str_ptr());
                        let cur_len = builder.use_var(pwd_slot.str_len());
                        builder.def_var(oldpwd_slot.str_ptr(), cur_ptr);
                        builder.def_var(oldpwd_slot.str_len(), cur_len);
                    }
                    // PWD = getcwd()
                    let len_slot2 = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        8,
                        3,
                    ));
                    let len_addr2 = builder.ins().stack_addr(I64, len_slot2, 0);
                    let getcwd_ref2 = module.declare_func_in_func(externs.getcwd, builder.func);
                    let call2 = builder.ins().call(getcwd_ref2, &[len_addr2]);
                    let new_ptr = builder.inst_results(call2)[0];
                    let new_len = builder.ins().stack_load(I64, len_slot2, 0);
                    builder.def_var(pwd_slot.str_ptr(), new_ptr);
                    builder.def_var(pwd_slot.str_len(), new_len);
                }
            } else {
                // cd with no args — cd to $HOME
                if let Some(home_slot) = vars.get("HOME_0") {
                    let ptr = builder.use_var(home_slot.str_ptr());
                    let len = builder.use_var(home_slot.str_len());
                    let chdir_ref = module.declare_func_in_func(externs.chdir, builder.func);
                    builder.ins().call(chdir_ref, &[ptr, len]);
                }
                let zero = builder.ins().iconst(I64, 0);
                builder.def_var(exit_status_var, zero);

                // Update PWD after cd to HOME
                if let Some(pwd_slot) = vars.get("PWD_0").cloned() {
                    if let Some(oldpwd_slot) = vars.get("OLDPWD_0").cloned() {
                        let cur_ptr = builder.use_var(pwd_slot.str_ptr());
                        let cur_len = builder.use_var(pwd_slot.str_len());
                        builder.def_var(oldpwd_slot.str_ptr(), cur_ptr);
                        builder.def_var(oldpwd_slot.str_len(), cur_len);
                    }
                    let len_slot2 = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        8,
                        3,
                    ));
                    let len_addr2 = builder.ins().stack_addr(I64, len_slot2, 0);
                    let getcwd_ref2 = module.declare_func_in_func(externs.getcwd, builder.func);
                    let call2 = builder.ins().call(getcwd_ref2, &[len_addr2]);
                    let new_ptr = builder.inst_results(call2)[0];
                    let new_len = builder.ins().stack_load(I64, len_slot2, 0);
                    builder.def_var(pwd_slot.str_ptr(), new_ptr);
                    builder.def_var(pwd_slot.str_len(), new_len);
                }
            }
        }
        SsaInst::Command { name, args, .. } if name == "pwd" => {
            // pwd — getcwd, write to stdout
            let len_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            let len_addr = builder.ins().stack_addr(I64, len_slot, 0);
            let getcwd_ref = module.declare_func_in_func(externs.getcwd, builder.func);
            let call = builder.ins().call(getcwd_ref, &[len_addr]);
            let cwd_ptr = builder.inst_results(call)[0];
            let cwd_len = builder.ins().stack_load(I64, len_slot, 0);
            let stdout = builder.ins().iconst(I64, 1);
            let write_ref = module.declare_func_in_func(externs.write, builder.func);
            builder.ins().call(write_ref, &[stdout, cwd_ptr, cwd_len]);
            // Write newline
            let nl_id = literals.get("\n").expect("newline literal");
            let nl_gv = module.declare_data_in_func(*nl_id, builder.func);
            let nl_ptr = builder.ins().global_value(I64, nl_gv);
            let nl_len = builder.ins().iconst(I64, 1);
            builder.ins().call(write_ref, &[stdout, nl_ptr, nl_len]);
            let zero = builder.ins().iconst(I64, 0);
            builder.def_var(exit_status_var, zero);
        }
        SsaInst::Command { name, args, .. } if name == "wait" => {
            // wait [pid] — wait for specific pid or last bg job
            let pid = if let Some(arg) = args.first() {
                let (ptr, len) = lower_value(
                    arg,
                    builder,
                    module,
                    vars,
                    literals,
                    func_constants,
                    externs,
                    Some(exit_status_var),
                );
                let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
                let call = builder.ins().call(atoi_ref, &[ptr, len]);
                builder.inst_results(call)[0]
            } else {
                // No arg: wait for $! — look up the "!" variable
                let key = vars.keys().find(|k| {
                    let base = if let Some(idx) = k.rfind('_') {
                        let suffix = &k[idx + 1..];
                        if suffix.chars().all(|c| c.is_ascii_digit()) {
                            &k[..idx]
                        } else {
                            k.as_str()
                        }
                    } else {
                        k.as_str()
                    };
                    base == "!"
                });
                if let Some(k) = key.cloned() {
                    let sv = &vars[&k];
                    let (ptr, len) = (builder.use_var(sv.str_ptr()), builder.use_var(sv.str_len()));
                    let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
                    let call = builder.ins().call(atoi_ref, &[ptr, len]);
                    builder.inst_results(call)[0]
                } else {
                    // No $! variable — wait for pid 0 (wait any child)
                    builder.ins().iconst(I64, 0)
                }
            };
            let wait_ref = module.declare_func_in_func(externs.wait, builder.func);
            let call = builder.ins().call(wait_ref, &[pid]);
            let status = builder.inst_results(call)[0];
            builder.def_var(exit_status_var, status);
        }
        SsaInst::Command { name, args, .. } if name == "shift" => {
            // shift N: sash_argc -= N; sash_argv += N*8
            let shift_n = if let Some(SsaValue::Literal(s)) = args.first() {
                s.parse::<i64>().unwrap_or(1)
            } else {
                1
            };
            let argc_gv = module.declare_data_in_func(externs.argc, builder.func);
            let argc_addr = builder.ins().global_value(I64, argc_gv);
            let old_argc = builder.ins().load(I64, MemFlags::trusted(), argc_addr, 0);
            let n_val = builder.ins().iconst(I64, shift_n);
            let new_argc = builder.ins().isub(old_argc, n_val);
            builder
                .ins()
                .store(MemFlags::trusted(), new_argc, argc_addr, 0);

            let argv_gv = module.declare_data_in_func(externs.argv, builder.func);
            let argv_addr = builder.ins().global_value(I64, argv_gv);
            let old_argv = builder.ins().load(I64, MemFlags::trusted(), argv_addr, 0);
            let offset = builder.ins().iconst(I64, shift_n * 8);
            let new_argv = builder.ins().iadd(old_argv, offset);
            builder
                .ins()
                .store(MemFlags::trusted(), new_argv, argv_addr, 0);

            // Reload all positional params from updated argv/argc
            let fresh_argv = builder.ins().load(I64, MemFlags::trusted(), argv_addr, 0);
            let fresh_argc = builder.ins().load(I64, MemFlags::trusted(), argc_addr, 0);
            for (key, sv) in vars.iter() {
                // key is like "1_0", "2_0", "#_0" — extract base name
                let base = key.split('_').next().unwrap_or("");
                if let Ok(idx) = base.parse::<i64>() {
                    // Guard: only reload if idx < fresh_argc (arg still exists)
                    let idx_val = builder.ins().iconst(I64, idx);
                    let in_bounds = builder
                        .ins()
                        .icmp(IntCC::SignedLessThan, idx_val, fresh_argc);

                    let reload_block = builder.create_block();
                    let skip_block = builder.create_block();
                    let merge_block = builder.create_block();

                    builder
                        .ins()
                        .brif(in_bounds, reload_block, &[], skip_block, &[]);

                    builder.switch_to_block(reload_block);
                    builder.seal_block(reload_block);
                    let offset = builder.ins().imul_imm(idx_val, 8);
                    let arg_ptr_addr = builder.ins().iadd(fresh_argv, offset);
                    let arg_ptr = builder
                        .ins()
                        .load(I64, MemFlags::trusted(), arg_ptr_addr, 0);
                    let len = emit_strlen(builder, module, externs.strlen, arg_ptr);
                    builder.def_var(sv.str_ptr(), arg_ptr);
                    builder.def_var(sv.str_len(), len);
                    builder.ins().jump(merge_block, &[]);

                    builder.switch_to_block(skip_block);
                    builder.seal_block(skip_block);
                    let null_ptr = builder.ins().iconst(I64, 0);
                    let zero_len = builder.ins().iconst(I64, 0);
                    builder.def_var(sv.str_ptr(), null_ptr);
                    builder.def_var(sv.str_len(), zero_len);
                    builder.ins().jump(merge_block, &[]);

                    builder.switch_to_block(merge_block);
                    builder.seal_block(merge_block);
                } else if base == "#" {
                    let one = builder.ins().iconst(I64, 1);
                    let count = builder.ins().isub(fresh_argc, one);
                    let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        8,
                        3,
                    ));
                    let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                    let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                    let call = builder.ins().call(itoa_ref, &[count, out_addr]);
                    let str_ptr = builder.inst_results(call)[0];
                    let str_len = builder.ins().stack_load(I64, out_slot, 0);
                    builder.def_var(sv.str_ptr(), str_ptr);
                    builder.def_var(sv.str_len(), str_len);
                } else if base == "@" {
                    let space_id = literals.get(" ").expect("space not interned");
                    let space_gv = module.declare_data_in_func(*space_id, builder.func);
                    let space_ptr = builder.ins().global_value(I64, space_gv);
                    let space_len = builder.ins().iconst(I64, 1);
                    let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        8,
                        3,
                    ));
                    let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                    let join_ref = module.declare_func_in_func(externs.join_args, builder.func);
                    let call = builder.ins().call(
                        join_ref,
                        &[fresh_argv, fresh_argc, space_ptr, space_len, out_addr],
                    );
                    let str_ptr = builder.inst_results(call)[0];
                    let str_len = builder.ins().stack_load(I64, out_slot, 0);
                    builder.def_var(sv.str_ptr(), str_ptr);
                    builder.def_var(sv.str_len(), str_len);
                }
            }
        }
        SsaInst::Command {
            name,
            args,
            redirects,
            ..
        } if name == "command" => {
            // DD#23: `command -v` is rejected by the validator. If we reach here,
            // it's `command [-p] <cmd>` — just run the command, bypassing functions.
            let restore = emit_redirects(
                redirects,
                builder,
                module,
                externs,
                literals,
                vars,
                Some(exit_status_var),
            );
            let mut cmd_start = 0;
            for (i, arg) in args.iter().enumerate() {
                if let SsaValue::Literal(s) = arg {
                    match s.as_str() {
                        "-p" | "--" => {
                            cmd_start = i + 1;
                        }
                        _ => {
                            cmd_start = i;
                            break;
                        }
                    }
                } else {
                    cmd_start = i;
                    break;
                }
            }
            if cmd_start < args.len() {
                if let SsaValue::Literal(real_name) = &args[cmd_start] {
                    let real_args = args[cmd_start + 1..].to_vec();
                    lower_exec(
                        real_name,
                        &real_args,
                        builder,
                        module,
                        vars,
                        literals,
                        externs,
                        test_result_var,
                        exit_status_var,
                    );
                } else {
                    let real_name_str = match &args[cmd_start] {
                        SsaValue::Literal(s) => s.clone(),
                        _ => "command".to_string(),
                    };
                    let real_args = args[cmd_start + 1..].to_vec();
                    lower_exec(
                        &real_name_str,
                        &real_args,
                        builder,
                        module,
                        vars,
                        literals,
                        externs,
                        test_result_var,
                        exit_status_var,
                    );
                }
            }
            emit_restore(restore, builder, module, externs);
        }
        SsaInst::Command {
            name,
            args,
            redirects,
            checked,
            ..
        } => {
            let restore = emit_redirects(
                redirects,
                builder,
                module,
                externs,
                literals,
                vars,
                Some(exit_status_var),
            );
            lower_exec(
                name,
                args,
                builder,
                module,
                vars,
                literals,
                externs,
                test_result_var,
                exit_status_var,
            );
            emit_restore(restore, builder, module, externs);

            // DD#55: implicit exit check — abort if nonzero
            if *checked {
                let status = builder.use_var(exit_status_var);
                let zero = builder.ins().iconst(I64, 0);
                let failed = builder.ins().icmp(IntCC::NotEqual, status, zero);

                let abort_block = builder.create_block();
                let cont_block = builder.create_block();
                builder
                    .ins()
                    .brif(failed, abort_block, &[], cont_block, &[]);

                builder.switch_to_block(abort_block);
                builder.seal_block(abort_block);
                if let Some(end_cl) = end_cl_block {
                    // DD#56: jump to END block instead of exiting directly
                    builder.def_var(exit_status_var, status);
                    builder.ins().jump(end_cl, &[]);
                } else {
                    let exit_ref = module.declare_func_in_func(externs.exit, builder.func);
                    builder.ins().call(exit_ref, &[status]);
                    builder
                        .ins()
                        .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
                }

                builder.switch_to_block(cont_block);
                builder.seal_block(cont_block);
            }

            // DD#56: signal EXIT check point after external command
            if !program.exiting_signals.is_empty() {
                if let Some(end_cl) = end_cl_block {
                    if program.end_block.is_none_or(|eb| ssa_id < eb) {
                        let signum_gv = module.declare_data_in_func(externs.signum, builder.func);
                        let signum_base = builder.ins().global_value(I64, signum_gv);
                        let signum_val = builder.ins().load(I64, MemFlags::new(), signum_base, 0);
                        let zero_sig = builder.ins().iconst(I64, 0);
                        let sig_fired = builder.ins().icmp(IntCC::NotEqual, signum_val, zero_sig);
                        let sig_end_block = builder.create_block();
                        let sig_cont_block = builder.create_block();
                        builder
                            .ins()
                            .brif(sig_fired, sig_end_block, &[], sig_cont_block, &[]);
                        builder.switch_to_block(sig_end_block);
                        builder.seal_block(sig_end_block);
                        // Set exit status to 128+signum (standard convention)
                        let c128 = builder.ins().iconst(I64, 128);
                        let exit_code = builder.ins().iadd(signum_val, c128);
                        builder.def_var(exit_status_var, exit_code);
                        builder.ins().jump(end_cl, &[]);
                        builder.switch_to_block(sig_cont_block);
                        builder.seal_block(sig_cont_block);
                    }
                }
            }
        }
        SsaInst::Phi(_) => {}
        SsaInst::SubscriptAssign {
            array,
            index,
            value,
            is_integer_key,
        } => {
            let array_key = format!("{}_0", array);
            let sv = vars.get(&array_key).unwrap_or_else(|| {
                vars.keys()
                    .find(|k| k.starts_with(&format!("{}_", array)))
                    .and_then(|k| vars.get(k))
                    .unwrap_or_else(|| panic!("ICE: undefined array: {}", array))
            });
            let table_ptr = builder.use_var(sv.str_ptr());

            let (key_ptr, key_len) = if *is_integer_key {
                let idx_int = lower_value_as_int(
                    index,
                    builder,
                    module,
                    vars,
                    literals,
                    func_constants,
                    externs,
                );
                let idx_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                builder.ins().stack_store(idx_int, idx_slot, 0);
                let kp = builder.ins().stack_addr(I64, idx_slot, 0);
                let kl = builder.ins().iconst(I64, 8);
                (kp, kl)
            } else {
                lower_value(
                    index,
                    builder,
                    module,
                    vars,
                    literals,
                    func_constants,
                    externs,
                    Some(exit_status_var),
                )
            };

            let (val_ptr, val_len) = lower_value(
                value,
                builder,
                module,
                vars,
                literals,
                func_constants,
                externs,
                Some(exit_status_var),
            );
            let set_ref = module.declare_func_in_func(externs.table_set, builder.func);
            builder
                .ins()
                .call(set_ref, &[table_ptr, key_ptr, key_len, val_ptr, val_len]);
        }
        SsaInst::ArrayAppend { array, value } => {
            let array_key = format!("{}_0", array);
            let sv = vars.get(&array_key).unwrap_or_else(|| {
                vars.keys()
                    .find(|k| k.starts_with(&format!("{}_", array)))
                    .and_then(|k| vars.get(k))
                    .unwrap_or_else(|| panic!("ICE: undefined array: {}", array))
            });
            let table_ptr = builder.use_var(sv.str_ptr());

            // Get current length as the next integer key
            let len_ref = module.declare_func_in_func(externs.table_len, builder.func);
            let call = builder.ins().call(len_ref, &[table_ptr]);
            let next_idx = builder.inst_results(call)[0];

            let idx_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            builder.ins().stack_store(next_idx, idx_slot, 0);
            let kp = builder.ins().stack_addr(I64, idx_slot, 0);
            let kl = builder.ins().iconst(I64, 8);

            let (val_ptr, val_len) = lower_value(
                value,
                builder,
                module,
                vars,
                literals,
                func_constants,
                externs,
                Some(exit_status_var),
            );
            let set_ref = module.declare_func_in_func(externs.table_set, builder.func);
            builder
                .ins()
                .call(set_ref, &[table_ptr, kp, kl, val_ptr, val_len]);
        }
        SsaInst::Mapfile {
            array,
            fd,
            map_mode,
            separator,
        } => {
            // Read all lines from fd into an array (or map in map_mode)
            let sv = vars
                .keys()
                .filter(|k| k.starts_with(&format!("{}_", array)))
                .max_by_key(|k| {
                    k.rsplit('_')
                        .next()
                        .and_then(|n| n.parse::<u32>().ok())
                        .unwrap_or(0)
                })
                .and_then(|k| vars.get(k))
                .unwrap_or_else(|| panic!("ICE: undefined array: {}", array));
            let table_ptr = builder.use_var(sv.str_ptr());

            let read_line_ref = module.declare_func_in_func(externs.read_line, builder.func);
            let fd_val = if *fd == -1 {
                builder.use_var(pipe_read_fd_var)
            } else {
                builder.ins().iconst(I64, *fd)
            };

            let loop_hdr = builder.create_block();
            let loop_body = builder.create_block();
            let loop_exit = builder.create_block();
            builder.ins().jump(loop_hdr, &[]);

            builder.switch_to_block(loop_hdr);
            let call = builder.ins().call(read_line_ref, &[fd_val]);
            let line_ptr = builder.inst_results(call)[0];
            let line_len = builder.inst_results(call)[1];
            let zero = builder.ins().iconst(I64, 0);
            let got_data = builder.ins().icmp(IntCC::NotEqual, line_ptr, zero);
            builder.ins().brif(got_data, loop_body, &[], loop_exit, &[]);

            builder.switch_to_block(loop_body);
            builder.seal_block(loop_body);

            if *map_mode {
                // Split line on separator, insert key/value into map
                let delim_val = builder.ins().iconst(I64, *separator as i64);
                let rest_ptr_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                let rest_len_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                let rest_ptr_addr = builder.ins().stack_addr(I64, rest_ptr_slot, 0);
                let rest_len_addr = builder.ins().stack_addr(I64, rest_len_slot, 0);
                let split_ref = module.declare_func_in_func(externs.split_field, builder.func);
                let split_call = builder.ins().call(
                    split_ref,
                    &[line_ptr, line_len, delim_val, rest_ptr_addr, rest_len_addr],
                );
                let key_ptr = builder.inst_results(split_call)[0];
                let key_len = builder.inst_results(split_call)[1];
                let val_ptr = builder.ins().stack_load(I64, rest_ptr_slot, 0);
                let val_len = builder.ins().stack_load(I64, rest_len_slot, 0);

                // Skip empty keys (blank lines)
                let key_empty = builder.ins().icmp(IntCC::Equal, key_len, zero);
                let skip_block = builder.create_block();
                let insert_block = builder.create_block();
                builder
                    .ins()
                    .brif(key_empty, skip_block, &[], insert_block, &[]);

                builder.switch_to_block(insert_block);
                builder.seal_block(insert_block);
                // Insert key/value into table
                let set_ref = module.declare_func_in_func(externs.table_set, builder.func);
                builder
                    .ins()
                    .call(set_ref, &[table_ptr, key_ptr, key_len, val_ptr, val_len]);
                builder.ins().jump(loop_hdr, &[]);

                builder.switch_to_block(skip_block);
                builder.seal_block(skip_block);
                builder.ins().jump(loop_hdr, &[]);
            } else {
                // Key = current table length (next integer index)
                let len_ref = module.declare_func_in_func(externs.table_len, builder.func);
                let len_call = builder.ins().call(len_ref, &[table_ptr]);
                let next_idx = builder.inst_results(len_call)[0];
                let idx_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    8,
                    3,
                ));
                builder.ins().stack_store(next_idx, idx_slot, 0);
                let kp = builder.ins().stack_addr(I64, idx_slot, 0);
                let kl = builder.ins().iconst(I64, 8);
                let set_ref = module.declare_func_in_func(externs.table_set, builder.func);
                builder
                    .ins()
                    .call(set_ref, &[table_ptr, kp, kl, line_ptr, line_len]);
                builder.ins().jump(loop_hdr, &[]);
            }
            builder.seal_block(loop_hdr);

            builder.switch_to_block(loop_exit);
            builder.seal_block(loop_exit);
        }
        SsaInst::ReadArray { array, fd, delim } => {
            // Slurp all input from fd, then split by delimiter into indexed array
            let array_key = format!("{}_0", array);
            let sv = vars.get(&array_key).unwrap_or_else(|| {
                vars.keys()
                    .find(|k| k.starts_with(&format!("{}_", array)))
                    .and_then(|k| vars.get(k))
                    .unwrap_or_else(|| panic!("ICE: undefined array: {}", array))
            });
            let table_ptr = builder.use_var(sv.str_ptr());

            let fd_val = if *fd == -1 {
                builder.use_var(pipe_read_fd_var)
            } else {
                builder.ins().iconst(I64, *fd)
            };

            // Read all data from fd
            let read_all_ref = module.declare_func_in_func(externs.read_all, builder.func);
            let call = builder.ins().call(read_all_ref, &[fd_val]);
            let data_ptr = builder.inst_results(call)[0];
            let data_len = builder.inst_results(call)[1];

            // Split by delimiter into array
            let split_ref = module.declare_func_in_func(externs.split_into_array, builder.func);
            let delim_val = builder.ins().iconst(I64, delim.unwrap_or(b' ') as i64);
            builder
                .ins()
                .call(split_ref, &[data_ptr, data_len, delim_val, table_ptr]);
        }
        SsaInst::CaptureStart => {
            // Call sash_capture_start() → (saved_fd, pipe_read_fd)
            // Store both in the capture Cranelift variables
            let cap_ref = module.declare_func_in_func(externs.capture_start, builder.func);
            let call = builder.ins().call(cap_ref, &[]);
            let saved = builder.inst_results(call)[0];
            let pipe_rd = builder.inst_results(call)[1];
            builder.def_var(capture_saved_var, saved);
            builder.def_var(capture_pipe_var, pipe_rd);
        }
        SsaInst::CaptureEnd { target } => {
            // Call sash_capture_end(saved_fd, pipe_read_fd) → (ptr, len)
            let saved = builder.use_var(capture_saved_var);
            let pipe_rd = builder.use_var(capture_pipe_var);
            let cap_ref = module.declare_func_in_func(externs.capture_end, builder.func);
            let call = builder.ins().call(cap_ref, &[saved, pipe_rd]);
            let ptr = builder.inst_results(call)[0];
            let len = builder.inst_results(call)[1];
            let sv = &vars[&target.display()];
            if sv.is_int() {
                // Int var: atoi the captured string
                let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
                let atoi_call = builder.ins().call(atoi_ref, &[ptr, len]);
                let int_val = builder.inst_results(atoi_call)[0];
                builder.def_var(*sv.as_int(), int_val);
            } else {
                builder.def_var(sv.str_ptr(), ptr);
                builder.def_var(sv.str_len(), len);
            }
        }
        SsaInst::Test(test_expr) => {
            let result = lower_test_expr(
                test_expr,
                builder,
                module,
                vars,
                literals,
                externs,
                Some(exit_status_var),
            );
            builder.def_var(test_result_var, result);
        }
        SsaInst::ReadLine {
            target,
            fd,
            delim,
            redirects,
            ..
        } => {
            // Set up redirects (e.g. herestring <<< "text")
            let saved = emit_redirects(
                redirects,
                builder,
                module,
                externs,
                literals,
                vars,
                Some(exit_status_var),
            );
            // fd=-1 is sentinel for "use captured pipe fd"
            let fd_val = if *fd == -1 {
                builder.use_var(pipe_read_fd_var)
            } else {
                builder.ins().iconst(I64, *fd)
            };
            let (ptr, len) = if let Some(d) = delim {
                let read_delim_ref = module.declare_func_in_func(externs.read_delim, builder.func);
                let delim_val = builder.ins().iconst(I64, *d as i64);
                let call = builder.ins().call(read_delim_ref, &[fd_val, delim_val]);
                (builder.inst_results(call)[0], builder.inst_results(call)[1])
            } else {
                let read_line_ref = module.declare_func_in_func(externs.read_line, builder.func);
                let call = builder.ins().call(read_line_ref, &[fd_val]);
                (builder.inst_results(call)[0], builder.inst_results(call)[1])
            };
            let sv = &vars[&target.display()];
            builder.def_var(sv.str_ptr(), ptr);
            builder.def_var(sv.str_len(), len);
            // Set test_result: 1 if we got data (len > 0), 0 if EOF
            let zero = builder.ins().iconst(I64, 0);
            let got_data = builder.ins().icmp(IntCC::SignedGreaterThan, len, zero);
            let result = builder.ins().uextend(I64, got_data);
            builder.def_var(test_result_var, result);
            // Restore redirects
            emit_restore(saved, builder, module, externs);
        }
        SsaInst::ReadLineMulti {
            targets,
            fd,
            delim,
            redirects,
            field_sep,
            ..
        } => {
            let saved = emit_redirects(
                redirects,
                builder,
                module,
                externs,
                literals,
                vars,
                Some(exit_status_var),
            );
            let fd_val = if *fd == -1 {
                builder.use_var(pipe_read_fd_var)
            } else {
                builder.ins().iconst(I64, *fd)
            };
            let (line_ptr, line_len) = if let Some(d) = delim {
                let read_delim_ref = module.declare_func_in_func(externs.read_delim, builder.func);
                let delim_val = builder.ins().iconst(I64, *d as i64);
                let call = builder.ins().call(read_delim_ref, &[fd_val, delim_val]);
                (builder.inst_results(call)[0], builder.inst_results(call)[1])
            } else {
                let read_line_ref = module.declare_func_in_func(externs.read_line, builder.func);
                let call = builder.ins().call(read_line_ref, &[fd_val]);
                (builder.inst_results(call)[0], builder.inst_results(call)[1])
            };

            let zero = builder.ins().iconst(I64, 0);
            let got_data = builder.ins().icmp(IntCC::SignedGreaterThan, line_len, zero);
            let result = builder.ins().uextend(I64, got_data);
            builder.def_var(test_result_var, result);

            let split_ref = module.declare_func_in_func(externs.split_first_word, builder.func);
            let split_field_ref = module.declare_func_in_func(externs.split_field, builder.func);
            let mut cur_ptr = line_ptr;
            let mut cur_len = line_len;

            for (i, target) in targets.iter().enumerate() {
                let sv = &vars[&target.display()];
                if i == targets.len() - 1 {
                    builder.def_var(sv.str_ptr(), cur_ptr);
                    builder.def_var(sv.str_len(), cur_len);
                } else {
                    let rest_ptr_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        8,
                        3,
                    ));
                    let rest_len_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        8,
                        3,
                    ));
                    let rest_ptr_addr = builder.ins().stack_addr(I64, rest_ptr_slot, 0);
                    let rest_len_addr = builder.ins().stack_addr(I64, rest_len_slot, 0);
                    let (word_ptr, word_len) = if let Some(sep) = field_sep {
                        let delim_val = builder.ins().iconst(I64, *sep as i64);
                        let call = builder.ins().call(
                            split_field_ref,
                            &[cur_ptr, cur_len, delim_val, rest_ptr_addr, rest_len_addr],
                        );
                        (builder.inst_results(call)[0], builder.inst_results(call)[1])
                    } else {
                        let call = builder
                            .ins()
                            .call(split_ref, &[cur_ptr, cur_len, rest_ptr_addr, rest_len_addr]);
                        (builder.inst_results(call)[0], builder.inst_results(call)[1])
                    };
                    builder.def_var(sv.str_ptr(), word_ptr);
                    builder.def_var(sv.str_len(), word_len);
                    cur_ptr = builder.ins().stack_load(I64, rest_ptr_slot, 0);
                    cur_len = builder.ins().stack_load(I64, rest_len_slot, 0);
                }
            }
            emit_restore(saved, builder, module, externs);
        }
        SsaInst::Pipeline {
            stages,
            capture_read_fd,
            checked,
        } => {
            for (_n, _, _r) in stages.iter() {}
            // N-stage pipeline: fork N children, wire pipes between them
            // For each adjacent pair, create a pipe.
            // Child i: stdin from pipe[i-1] read end, stdout to pipe[i] write end
            // Parent: close all pipe ends, wait for all children
            let n = stages.len();
            let fork_ref = module.declare_func_in_func(externs.fork, builder.func);
            let pipe_ref = module.declare_func_in_func(externs.pipe, builder.func);
            let dup2_ref = module.declare_func_in_func(externs.dup2, builder.func);
            let close_ref = module.declare_func_in_func(externs.close, builder.func);
            let exec_ref = module.declare_func_in_func(externs.exec, builder.func);
            let wait_ref = module.declare_func_in_func(externs.wait, builder.func);
            let redirect_ref = module.declare_func_in_func(externs.redirect, builder.func);
            let null = builder.ins().iconst(I64, 0);

            // Create n-1 pipes, store fd pairs on stack
            let mut pipe_slots = vec![];
            // For pipe-while-read, need one extra pipe (last stage → parent)
            let num_pipes = if *capture_read_fd { n } else { n - 1 };
            for _ in 0..num_pipes {
                let slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    16,
                    3,
                ));
                let addr = builder.ins().stack_addr(I64, slot, 0);
                builder.ins().call(pipe_ref, &[addr]);
                pipe_slots.push(slot);
            }

            // Stack slots for child pids
            let pid_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                (n * 8) as u32,
                3,
            ));

            // Fork each stage
            for (i, (name, args, redirects)) in stages.iter().enumerate() {
                let call = builder.ins().call(fork_ref, &[]);
                let pid = builder.inst_results(call)[0];
                // Store pid for later wait
                builder.ins().stack_store(pid, pid_slot, (i * 8) as i32);
                let is_child = builder.ins().icmp(IntCC::Equal, pid, null);

                let child_block = builder.create_block();
                let parent_block = builder.create_block();
                builder
                    .ins()
                    .brif(is_child, child_block, &[], parent_block, &[]);

                // Child block
                builder.switch_to_block(child_block);
                builder.seal_block(child_block);

                // Wire stdin from previous pipe
                if i > 0 {
                    let prev_read = builder.ins().stack_load(I64, pipe_slots[i - 1], 0);
                    let stdin = builder.ins().iconst(I64, 0);
                    builder.ins().call(dup2_ref, &[prev_read, stdin]);
                }
                // Wire stdout to next pipe
                // Wire stdout to next pipe (or capture pipe for last stage)
                if i < n - 1 || *capture_read_fd {
                    let pipe_idx = if i < pipe_slots.len() {
                        i
                    } else {
                        pipe_slots.len() - 1
                    };
                    let next_write = builder.ins().stack_load(I64, pipe_slots[pipe_idx], 8);
                    let stdout = builder.ins().iconst(I64, 1);
                    builder.ins().call(dup2_ref, &[next_write, stdout]);
                }
                // Close all pipe fds in child
                for slot in &pipe_slots {
                    let r = builder.ins().stack_load(I64, *slot, 0);
                    let w = builder.ins().stack_load(I64, *slot, 8);
                    builder.ins().call(close_ref, &[r]);
                    builder.ins().call(close_ref, &[w]);
                }
                // Apply redirects for this stage
                for redir in redirects {
                    let fd = builder.ins().iconst(I64, redir.fd.unwrap_or(1) as i64);
                    let flags = builder.ins().iconst(
                        I64,
                        match redir.op {
                            crate::ir::RedirectOp::Write => 1,
                            crate::ir::RedirectOp::Append => 2,
                            _ => 0,
                        },
                    );
                    if let crate::ir::ExprKind::Literal(s) = &redir.target.kind {
                        if let Some(&data_id) = literals.get(s.as_str()) {
                            let gv = module.declare_data_in_func(data_id, builder.func);
                            let path_ptr = builder.ins().global_value(I64, gv);
                            builder.ins().call(redirect_ref, &[fd, path_ptr, flags]);
                        }
                    } else {
                        let target_ssa = ir_expr_to_ssa_value(&redir.target);
                        let (ptr, len) = lower_value(
                            &target_ssa,
                            builder,
                            module,
                            vars,
                            literals,
                            &std::collections::HashMap::new(),
                            externs,
                            Some(exit_status_var),
                        );
                        let redirect_s_ref =
                            module.declare_func_in_func(externs.redirect_s, builder.func);
                        builder.ins().call(redirect_s_ref, &[fd, ptr, len, flags]);
                    }
                }
                // Run stage: builtin inline or exec external
                let is_pipeline_builtin = matches!(name.as_str(), "echo" | "true" | "false" | ":");
                if is_pipeline_builtin {
                    // Run builtin inline in the forked child, then _exit(0)
                    match name.as_str() {
                        "echo" => {
                            lower_echo(
                                args,
                                builder,
                                module,
                                vars,
                                literals,
                                externs,
                                space_id,
                                nl_id,
                                Some(exit_status_var),
                            );
                        }
                        "true" | ":" => {}
                        "false" => {}
                        _ => {}
                    }
                    let exit_code = if name == "false" {
                        builder.ins().iconst(I64, 1)
                    } else {
                        builder.ins().iconst(I64, 0)
                    };
                    let exit_ref2 = module.declare_func_in_func(externs.exit, builder.func);
                    builder.ins().call(exit_ref2, &[exit_code]);
                    builder.ins().trap(TrapCode::unwrap_user(1));
                } else if let Some(suffix) = name.strip_prefix("__compound_") {
                    // DD#14: compound command as pipeline stage
                    // Jump to the pre-lowered SSA blocks, which end with Return
                    let compound_block_id: usize = suffix.parse().unwrap();
                    let compound_cl = cl_blocks[&compound_block_id];
                    builder.ins().jump(compound_cl, &[]);
                } else {
                    // Build argv and exec
                    let argc = 1 + args.len();
                    let argv_size = ((argc + 1) * 8) as u32;
                    let argv_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        argv_size,
                        3,
                    ));
                    let cmd_val = SsaValue::Literal(name.to_string());
                    let (cmd_ptr, _) = lower_value(
                        &cmd_val,
                        builder,
                        module,
                        vars,
                        literals,
                        &std::collections::HashMap::new(),
                        externs,
                        Some(exit_status_var),
                    );
                    builder.ins().stack_store(cmd_ptr, argv_slot, 0);
                    for (j, arg) in args.iter().enumerate() {
                        let offset = ((j + 1) * 8) as i32;
                        let (ptr, _) = lower_value(
                            arg,
                            builder,
                            module,
                            vars,
                            literals,
                            &std::collections::HashMap::new(),
                            externs,
                            Some(exit_status_var),
                        );
                        builder.ins().stack_store(ptr, argv_slot, offset);
                    }
                    builder
                        .ins()
                        .stack_store(null, argv_slot, (argc * 8) as i32);
                    let envp_gv = module.declare_data_in_func(externs.envp, builder.func);
                    let envp_addr = builder.ins().global_value(I64, envp_gv);
                    let envp = builder.ins().load(I64, MemFlags::new(), envp_addr, 0);
                    let argv_addr = builder.ins().stack_addr(I64, argv_slot, 0);
                    builder.ins().call(exec_ref, &[argv_addr, envp]);
                    // exec doesn't return on success; if it does, exit 127
                    let code = builder.ins().iconst(I64, 127);
                    let exit_ref = module.declare_func_in_func(externs.exit, builder.func);
                    builder.ins().call(exit_ref, &[code]);
                    builder.ins().trap(TrapCode::unwrap_user(1));
                } // end else (non-builtin exec path)

                // Parent block
                builder.switch_to_block(parent_block);
                builder.seal_block(parent_block);
            }

            // Parent: close all pipe fds (except last read end if capturing for while-read)
            for (idx, slot) in pipe_slots.iter().enumerate() {
                let r = builder.ins().stack_load(I64, *slot, 0);
                let w = builder.ins().stack_load(I64, *slot, 8);
                let is_last_read = *capture_read_fd && idx == pipe_slots.len() - 1;
                if !is_last_read {
                    builder.ins().call(close_ref, &[r]);
                }
                builder.ins().call(close_ref, &[w]);
            }
            // If capturing pipe fd, store the last read end in pipe_read_fd_var
            if *capture_read_fd && !pipe_slots.is_empty() {
                let last_slot = pipe_slots[pipe_slots.len() - 1];
                let pipe_r = builder.ins().stack_load(I64, last_slot, 0);
                builder.def_var(pipe_read_fd_var, pipe_r);
            }
            // Wait for all children, capture last child's exit status
            // DD#55.10: pipefail — track worst (first nonzero) exit status
            let worst_var = builder.declare_var(I64);
            let init_zero = builder.ins().iconst(I64, 0);
            builder.def_var(worst_var, init_zero);

            for i in 0..n {
                let pid = builder.ins().stack_load(I64, pid_slot, (i * 8) as i32);
                let call = builder.ins().call(wait_ref, &[pid]);
                let status = builder.inst_results(call)[0];
                // Update worst: if worst==0 && status!=0, worst=status
                let cur_worst = builder.use_var(worst_var);
                let zero = builder.ins().iconst(I64, 0);
                let worst_is_zero = builder.ins().icmp(IntCC::Equal, cur_worst, zero);
                let status_nonzero = builder.ins().icmp(IntCC::NotEqual, status, zero);
                let should_update = builder.ins().band(worst_is_zero, status_nonzero);
                let new_worst = builder.ins().select(should_update, status, cur_worst);
                builder.def_var(worst_var, new_worst);
            }
            // exit_status = worst (pipefail semantics)
            let final_worst = builder.use_var(worst_var);
            builder.def_var(exit_status_var, final_worst);
            // test_result: 1 if exit==0, 0 otherwise
            let zero2 = builder.ins().iconst(I64, 0);
            let ok = builder.ins().icmp(IntCC::Equal, final_worst, zero2);
            let tr = builder.ins().uextend(I64, ok);
            builder.def_var(test_result_var, tr);

            // DD#55: implicit exit check for pipelines
            if *checked {
                let abort_block = builder.create_block();
                let cont_block = builder.create_block();
                let failed = builder.ins().icmp(IntCC::NotEqual, final_worst, zero2);
                builder
                    .ins()
                    .brif(failed, abort_block, &[], cont_block, &[]);

                builder.switch_to_block(abort_block);
                builder.seal_block(abort_block);
                if let Some(end_cl) = end_cl_block {
                    builder.def_var(exit_status_var, final_worst);
                    builder.ins().jump(end_cl, &[]);
                } else {
                    let exit_ref = module.declare_func_in_func(externs.exit, builder.func);
                    builder.ins().call(exit_ref, &[final_worst]);
                    builder
                        .ins()
                        .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
                }

                builder.switch_to_block(cont_block);
                builder.seal_block(cont_block);
            }
        }
        SsaInst::Bless {
            source,
            target,
            test,
        } => {
            // Runtime bless enforcement: evaluate test on source, abort if false.
            // On pass, copy source value to target (new clean SSA version).
            let result = lower_test_expr(
                test,
                builder,
                module,
                vars,
                literals,
                externs,
                Some(exit_status_var),
            );
            let pass_block = builder.create_block();
            let fail_block = builder.create_block();
            let zero = builder.ins().iconst(I64, 0);
            let is_pass = builder.ins().icmp(IntCC::NotEqual, result, zero);
            builder
                .ins()
                .brif(is_pass, pass_block, &[], fail_block, &[]);

            builder.switch_to_block(fail_block);
            builder.seal_block(fail_block);
            // Write "bless: validation failed\n" to stderr, then exit(1)
            let msg = "bless: validation failed\n";
            let msg_id = literals[msg];
            let msg_gv = module.declare_data_in_func(msg_id, builder.func);
            let msg_ptr = builder.ins().global_value(I64, msg_gv);
            let msg_len = builder.ins().iconst(I64, msg.len() as i64);
            let fd_2 = builder.ins().iconst(I64, 2); // stderr
            let write_ref = module.declare_func_in_func(externs.write, builder.func);
            builder.ins().call(write_ref, &[fd_2, msg_ptr, msg_len]);
            let one = builder.ins().iconst(I64, 1);
            let exit_ref = module.declare_func_in_func(externs.exit, builder.func);
            builder.ins().call(exit_ref, &[one]);
            builder
                .ins()
                .trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());

            builder.switch_to_block(pass_block);
            builder.seal_block(pass_block);

            // Copy source value to target — target is the new clean SSA version
            let src_key = format!("{}_{}", source.name, source.version);
            if let Some(slot) = vars.get(src_key.as_str()).cloned() {
                let tgt_key = format!("{}_{}", target.name, target.version);
                match slot {
                    VarSlot::Str { ptr, len } => {
                        let p = builder.use_var(ptr);
                        let l = builder.use_var(len);
                        let tp = builder.declare_var(I64);
                        let tl = builder.declare_var(I64);
                        builder.def_var(tp, p);
                        builder.def_var(tl, l);
                        vars.insert(tgt_key, VarSlot::Str { ptr: tp, len: tl });
                    }
                    VarSlot::Int(v) => {
                        let val = builder.use_var(v);
                        let tv = builder.declare_var(I64);
                        builder.def_var(tv, val);
                        vars.insert(tgt_key, VarSlot::Int(tv));
                    }
                }
            }
        }
    }
    false // block not terminated
}
