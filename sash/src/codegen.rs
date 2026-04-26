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

use cranelift_codegen::ir::types::{I8, I64};
use cranelift_codegen::ir::{instructions::BlockArg,
    AbiParam, Function, InstBuilder, MemFlags, Signature, StackSlotData,
    StackSlotKind, TrapCode, UserFuncName,
};
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings;
use cranelift_codegen::verifier::verify_function;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{default_libcall_names, DataDescription, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};

use crate::ir::TestExpr;
use crate::ssa::*;

#[derive(Debug, Clone, Copy)]
enum VarSlot {
    Str { ptr: Variable, len: Variable },
    Int(Variable),
}

impl VarSlot {
    #[allow(dead_code)]
    fn as_str(&self) -> (&Variable, &Variable) {
        match self {
            VarSlot::Str { ptr, len } => (ptr, len),
            VarSlot::Int(_) => panic!("ICE: expected string var, got int"),
        }
    }
    fn str_ptr(&self) -> Variable {
        match self {
            VarSlot::Str { ptr, .. } => *ptr,
            VarSlot::Int(_) => panic!("ICE: expected string var, got int"),
        }
    }
    fn str_len(&self) -> Variable {
        match self {
            VarSlot::Str { ptr: _, len } => *len,
            VarSlot::Int(_) => panic!("ICE: expected string var, got int"),
        }
    }
    fn as_int(&self) -> &Variable {
        match self {
            VarSlot::Int(v) => v,
            VarSlot::Str { .. } => panic!("ICE: expected int var, got string"),
        }
    }
    fn is_int(&self) -> bool {
        matches!(self, VarSlot::Int(_))
    }
}

#[allow(dead_code)]
struct ExternFuncs {
    write: cranelift_module::FuncId,
    exit: cranelift_module::FuncId,
    run: cranelift_module::FuncId,
    getenv: cranelift_module::FuncId,
    #[allow(dead_code)]
    capture: cranelift_module::FuncId,
    strlen: cranelift_module::FuncId,
    strip_suffix: cranelift_module::FuncId,
    strip_prefix: cranelift_module::FuncId,
    default_value: cranelift_module::FuncId,
    // Composable process primitives (Design Decision #14)
    fork: cranelift_module::FuncId,
    exec: cranelift_module::FuncId,
    wait: cranelift_module::FuncId,
    pipe: cranelift_module::FuncId,
    close: cranelift_module::FuncId,
    dup2: cranelift_module::FuncId,
    dup: cranelift_module::FuncId,
    redirect: cranelift_module::FuncId,
    redirect_s: cranelift_module::FuncId,
    read: cranelift_module::FuncId,
    alloc: cranelift_module::FuncId,
    concat: cranelift_module::FuncId,
    atoi: cranelift_module::FuncId,
    itoa: cranelift_module::FuncId,
    setenv: cranelift_module::FuncId,
    join_args: cranelift_module::FuncId,
    assoc_lookup: cranelift_module::FuncId,
    strcmp: cranelift_module::FuncId,
    regex_match: cranelift_module::FuncId,
    regex_replace: cranelift_module::FuncId,
    table_new: cranelift_module::FuncId,
    table_set: cranelift_module::FuncId,
    table_get: cranelift_module::FuncId,
    table_len: cranelift_module::FuncId,
    table_value: cranelift_module::FuncId,
    table_key: cranelift_module::FuncId,
    table_delete: cranelift_module::FuncId,
    read_line: cranelift_module::FuncId,
    read_delim: cranelift_module::FuncId,
    splice_argv: cranelift_module::FuncId,
    source_kvp: cranelift_module::FuncId,
    file_test: cranelift_module::FuncId,
    pow: cranelift_module::FuncId,
    substring: cranelift_module::FuncId,
    isatty: cranelift_module::FuncId,
    file_newer: cranelift_module::FuncId,
    file_same_dev: cranelift_module::FuncId,
    chdir: cranelift_module::FuncId,
    getcwd: cranelift_module::FuncId,
    heredoc_redirect: cranelift_module::FuncId,
    toupper_first: cranelift_module::FuncId,
    tolower_first: cranelift_module::FuncId,
    toupper_all: cranelift_module::FuncId,
    tolower_all: cranelift_module::FuncId,
    command_v: cranelift_module::FuncId,
    getpid: cranelift_module::FuncId,
    epochseconds: cranelift_module::FuncId,
    random: cranelift_module::FuncId,
    trap: cranelift_module::FuncId,
    glob: cranelift_module::FuncId,
    glob_regex: cranelift_module::FuncId,
    capture_start: cranelift_module::FuncId,
    capture_end: cranelift_module::FuncId,
    fnmatch: cranelift_module::FuncId,
    split_first_word: cranelift_module::FuncId,
    split_into_array: cranelift_module::FuncId,
    read_all: cranelift_module::FuncId,
    envp: cranelift_module::DataId,
    argc: cranelift_module::DataId,
    argv: cranelift_module::DataId,
}

pub fn compile_to_object(bash_source: &str) -> Result<Vec<u8>, Vec<String>> {
    crate::emit_source::clear_coercions();
    let tree = crate::parse(bash_source);
    let program = crate::lower::lower_program(tree.root_node(), bash_source.as_bytes());
    let type_map = crate::type_check::infer_types(&program);
    let (ssa, diags) = crate::transform::ir_to_ssa_typed(&program, type_map);

    if !diags.is_empty() {
        return Err(diags);
    }

    // Try to constant-fold elidable function calls
    let func_constants = fold_function_outputs(&ssa);
    Ok(compile_ssa(&ssa, &func_constants, Some(bash_source)))
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
                    if i > 0 { output.push(' '); }
                    match arg {
                        SsaValue::Literal(s) => output.push_str(s),
                        _ => return None, // non-constant argument
                    }
                }
                output.push('\n');
            }
            SsaInst::Assign { .. } => {} // assignments don't produce output
            _ => return None, // any other instruction makes it non-constant
        }
    }

    // Bash $(cmd) strips trailing newlines
    while output.ends_with('\n') {
        output.pop();
    }

    Some(output)
}

pub fn compile_ssa(program: &SsaProgram, func_constants: &HashMap<String, String>, bash_source: Option<&str>) -> Vec<u8> {
    let reachable = program.reachable();
    let shared_flags = settings::Flags::new(settings::builder());
    let isa = cranelift_native::builder()
        .expect("host ISA")
        .finish(shared_flags)
        .expect("build ISA");

    let obj_builder = ObjectBuilder::new(
        isa.clone(),
        "sash_program",
        default_libcall_names(),
    ).expect("ObjectBuilder");
    let mut module = ObjectModule::new(obj_builder);

    // ── Declare runtime functions ──────────────────────────────
    let mut write_sig = Signature::new(CallConv::SystemV);
    write_sig.params.push(AbiParam::new(I64));
    write_sig.params.push(AbiParam::new(I64));
    write_sig.params.push(AbiParam::new(I64));
    write_sig.returns.push(AbiParam::new(I64));
    let write_func = module.declare_function("sash_write", Linkage::Import, &write_sig)
        .expect("declare sash_write");

    let mut exit_sig = Signature::new(CallConv::SystemV);
    exit_sig.params.push(AbiParam::new(I64));
    let exit_func = module.declare_function("sash_exit", Linkage::Import, &exit_sig)
        .expect("declare sash_exit");

    let mut run_sig = Signature::new(CallConv::SystemV);
    run_sig.params.push(AbiParam::new(I64));
    run_sig.params.push(AbiParam::new(I64));
    run_sig.returns.push(AbiParam::new(I64));
    let run_func = module.declare_function("sash_run", Linkage::Import, &run_sig)
        .expect("declare sash_run");

    let envp_data = module.declare_data("sash_envp", Linkage::Import, false, false)
        .expect("declare sash_envp");
    let argc_data = module.declare_data("sash_argc", Linkage::Import, false, false)
        .expect("declare sash_argc");
    let argv_data = module.declare_data("sash_argv", Linkage::Import, false, false)
        .expect("declare sash_argv");

    // sash_getenv(name_ptr, name_len, &out_len) -> ptr
    let mut getenv_sig = Signature::new(CallConv::SystemV);
    getenv_sig.params.push(AbiParam::new(I64)); // name ptr
    getenv_sig.params.push(AbiParam::new(I64)); // name len
    getenv_sig.params.push(AbiParam::new(I64)); // &out_len
    getenv_sig.returns.push(AbiParam::new(I64)); // val ptr
    let getenv_func = module.declare_function("sash_getenv", Linkage::Import, &getenv_sig)
        .expect("declare sash_getenv");

    // sash_capture(argv, envp, &out_len) -> ptr
    let mut capture_sig = Signature::new(CallConv::SystemV);
    capture_sig.params.push(AbiParam::new(I64)); // argv
    capture_sig.params.push(AbiParam::new(I64)); // envp
    capture_sig.params.push(AbiParam::new(I64)); // &out_len
    capture_sig.returns.push(AbiParam::new(I64)); // buf ptr
    let capture_func = module.declare_function("sash_capture", Linkage::Import, &capture_sig)
        .expect("declare sash_capture");

    // strlen(ptr) -> len
    let mut strlen_sig = Signature::new(CallConv::SystemV);
    strlen_sig.params.push(AbiParam::new(I64));
    strlen_sig.returns.push(AbiParam::new(I64));
    let strlen_func = module.declare_function("strlen", Linkage::Import, &strlen_sig)
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
    let strip_suffix_func = module.declare_function("sash_strip_suffix", Linkage::Import, &strip_suffix_sig)
        .expect("declare sash_strip_suffix");

    // sash_strip_prefix — same signature, returns offset
    let strip_prefix_func = module.declare_function("sash_strip_prefix", Linkage::Import, &strip_suffix_sig)
        .expect("declare sash_strip_prefix");

    // sash_default_value(s_len) -> is_set (0 or 1)
    let mut default_value_sig = Signature::new(CallConv::SystemV);
    default_value_sig.params.push(AbiParam::new(I64)); // s_len
    default_value_sig.returns.push(AbiParam::new(I64)); // is_set
    let default_value_func = module.declare_function("sash_default_value", Linkage::Import, &default_value_sig)
        .expect("declare sash_default_value");

    // Composable process primitives (Design Decision #14)
    // sash_fork() -> pid
    let mut fork_sig = Signature::new(CallConv::SystemV);
    fork_sig.returns.push(AbiParam::new(I64));
    let fork_func = module.declare_function("sash_fork", Linkage::Import, &fork_sig)
        .expect("declare sash_fork");

    // sash_exec(argv, envp) -> ! (never returns)
    let mut exec_sig = Signature::new(CallConv::SystemV);
    exec_sig.params.push(AbiParam::new(I64)); // argv
    exec_sig.params.push(AbiParam::new(I64)); // envp
    let exec_func = module.declare_function("sash_exec", Linkage::Import, &exec_sig)
        .expect("declare sash_exec");

    // sash_wait(pid) -> exit_status
    let mut wait_sig = Signature::new(CallConv::SystemV);
    wait_sig.params.push(AbiParam::new(I64)); // pid
    wait_sig.returns.push(AbiParam::new(I64)); // status
    let wait_func = module.declare_function("sash_wait", Linkage::Import, &wait_sig)
        .expect("declare sash_wait");

    // sash_pipe(out_ptr) -> 0 ok, -1 err
    let mut pipe_sig = Signature::new(CallConv::SystemV);
    pipe_sig.params.push(AbiParam::new(I64)); // out_ptr (writes [r_fd, w_fd])
    pipe_sig.returns.push(AbiParam::new(I64));
    let pipe_func = module.declare_function("sash_pipe", Linkage::Import, &pipe_sig)
        .expect("declare sash_pipe");

    // sash_close(fd) -> 0 ok, -1 err
    let mut close_sig = Signature::new(CallConv::SystemV);
    close_sig.params.push(AbiParam::new(I64));
    close_sig.returns.push(AbiParam::new(I64));
    let close_func = module.declare_function("sash_close", Linkage::Import, &close_sig)
        .expect("declare sash_close");

    // sash_dup2(src, dst) -> 0 ok, -1 err
    let mut dup2_sig = Signature::new(CallConv::SystemV);
    dup2_sig.params.push(AbiParam::new(I64));
    dup2_sig.params.push(AbiParam::new(I64));
    dup2_sig.returns.push(AbiParam::new(I64));
    let dup2_func = module.declare_function("sash_dup2", Linkage::Import, &dup2_sig)
        .expect("declare sash_dup2");

    // sash_dup(fd) -> new_fd
    let mut dup_sig = Signature::new(CallConv::SystemV);
    dup_sig.params.push(AbiParam::new(I64));
    dup_sig.returns.push(AbiParam::new(I64));
    let dup_func = module.declare_function("sash_dup", Linkage::Import, &dup_sig)
        .expect("declare sash_dup");

    // sash_redirect(fd, path, flags) -> 0 ok, -1 err
    let mut redirect_sig = Signature::new(CallConv::SystemV);
    redirect_sig.params.push(AbiParam::new(I64)); // fd
    redirect_sig.params.push(AbiParam::new(I64)); // path (null-terminated)
    redirect_sig.params.push(AbiParam::new(I64)); // flags
    redirect_sig.returns.push(AbiParam::new(I64));
    let redirect_func = module.declare_function("sash_redirect", Linkage::Import, &redirect_sig)
        .expect("declare sash_redirect");

    // sash_redirect_s(fd, path_ptr, path_len, flags) -> 0 ok, -1 err
    let mut redirect_s_sig = Signature::new(CallConv::SystemV);
    redirect_s_sig.params.push(AbiParam::new(I64)); // fd
    redirect_s_sig.params.push(AbiParam::new(I64)); // path ptr
    redirect_s_sig.params.push(AbiParam::new(I64)); // path len
    redirect_s_sig.params.push(AbiParam::new(I64)); // flags
    redirect_s_sig.returns.push(AbiParam::new(I64));
    let redirect_s_func = module.declare_function("sash_redirect_s", Linkage::Import, &redirect_s_sig)
        .expect("declare sash_redirect_s");

    // sash_read(fd, buf, len) -> bytes read
    let mut read_sig = Signature::new(CallConv::SystemV);
    read_sig.params.push(AbiParam::new(I64)); // fd
    read_sig.params.push(AbiParam::new(I64)); // buf
    read_sig.params.push(AbiParam::new(I64)); // len
    read_sig.returns.push(AbiParam::new(I64));
    let read_func = module.declare_function("sash_read", Linkage::Import, &read_sig)
        .expect("declare sash_read");

    // sash_alloc(size) -> ptr
    let mut alloc_sig = Signature::new(CallConv::SystemV);
    alloc_sig.params.push(AbiParam::new(I64));
    alloc_sig.returns.push(AbiParam::new(I64));
    let alloc_func = module.declare_function("sash_alloc", Linkage::Import, &alloc_sig)
        .expect("declare sash_alloc");

    // sash_concat(a_ptr, a_len, b_ptr, b_len, out_len_ptr) -> ptr
    let mut concat_sig = Signature::new(CallConv::SystemV);
    concat_sig.params.push(AbiParam::new(I64)); // a_ptr
    concat_sig.params.push(AbiParam::new(I64)); // a_len
    concat_sig.params.push(AbiParam::new(I64)); // b_ptr
    concat_sig.params.push(AbiParam::new(I64)); // b_len
    concat_sig.params.push(AbiParam::new(I64)); // out_len ptr
    concat_sig.returns.push(AbiParam::new(I64)); // result ptr
    let concat_func = module.declare_function("sash_concat", Linkage::Import, &concat_sig)
        .expect("declare sash_concat");

    // sash_atoi(s_ptr, s_len) -> i64
    let mut atoi_sig = Signature::new(CallConv::SystemV);
    atoi_sig.params.push(AbiParam::new(I64)); // s_ptr
    atoi_sig.params.push(AbiParam::new(I64)); // s_len
    atoi_sig.returns.push(AbiParam::new(I64)); // value
    let atoi_func = module.declare_function("sash_atoi", Linkage::Import, &atoi_sig)
        .expect("declare sash_atoi");

    // sash_itoa(val, out_len_ptr) -> ptr
    let mut itoa_sig = Signature::new(CallConv::SystemV);
    itoa_sig.params.push(AbiParam::new(I64)); // val
    itoa_sig.params.push(AbiParam::new(I64)); // out_len ptr
    itoa_sig.returns.push(AbiParam::new(I64)); // buf ptr
    let itoa_func = module.declare_function("sash_itoa", Linkage::Import, &itoa_sig)
        .expect("declare sash_itoa");

    // sash_setenv(name_ptr, name_len, val_ptr, val_len)
    let mut setenv_sig = Signature::new(CallConv::SystemV);
    setenv_sig.params.push(AbiParam::new(I64)); // name_ptr
    setenv_sig.params.push(AbiParam::new(I64)); // name_len
    setenv_sig.params.push(AbiParam::new(I64)); // val_ptr
    setenv_sig.params.push(AbiParam::new(I64)); // val_len
    let setenv_func = module.declare_function("sash_setenv", Linkage::Import, &setenv_sig)
        .expect("declare sash_setenv");

    // sash_join_args(argv, argc, sep_ptr, sep_len, out_len_ptr) -> ptr
    let mut join_args_sig = Signature::new(CallConv::SystemV);
    join_args_sig.params.push(AbiParam::new(I64)); // argv
    join_args_sig.params.push(AbiParam::new(I64)); // argc
    join_args_sig.params.push(AbiParam::new(I64)); // sep_ptr
    join_args_sig.params.push(AbiParam::new(I64)); // sep_len
    join_args_sig.params.push(AbiParam::new(I64)); // out_len ptr
    join_args_sig.returns.push(AbiParam::new(I64)); // buf ptr
    let join_args_func = module.declare_function("sash_join_args", Linkage::Import, &join_args_sig)
        .expect("declare sash_join_args");

    // sash_assoc_lookup(table, count, key_ptr, key_len, out_len_ptr) -> val_ptr
    let mut assoc_lookup_sig = Signature::new(CallConv::SystemV);
    assoc_lookup_sig.params.push(AbiParam::new(I64)); // table ptr
    assoc_lookup_sig.params.push(AbiParam::new(I64)); // count
    assoc_lookup_sig.params.push(AbiParam::new(I64)); // key_ptr
    assoc_lookup_sig.params.push(AbiParam::new(I64)); // key_len
    assoc_lookup_sig.params.push(AbiParam::new(I64)); // out_len_ptr
    assoc_lookup_sig.returns.push(AbiParam::new(I64)); // val_ptr
    let assoc_lookup_func = module.declare_function("sash_assoc_lookup", Linkage::Import, &assoc_lookup_sig)
        .expect("declare sash_assoc_lookup");

    // sash_regex_match(str_ptr, str_len, pat_ptr, pat_len) -> i64
    let mut regex_match_sig = module.make_signature();
    regex_match_sig.params.push(AbiParam::new(I64));
    regex_match_sig.params.push(AbiParam::new(I64));
    regex_match_sig.params.push(AbiParam::new(I64));
    regex_match_sig.params.push(AbiParam::new(I64));
    regex_match_sig.returns.push(AbiParam::new(I64));
    let regex_match_func = module.declare_function("sash_regex_match", Linkage::Import, &regex_match_sig)
        .expect("declare sash_regex_match");

    // sash_fnmatch(pat_ptr, pat_len, str_ptr, str_len) -> i64
    let mut fnmatch_sig = module.make_signature();
    for _ in 0..4 { fnmatch_sig.params.push(AbiParam::new(I64)); }
    fnmatch_sig.returns.push(AbiParam::new(I64));
    let fnmatch_func = module.declare_function("sash_fnmatch", Linkage::Import, &fnmatch_sig)
        .expect("declare sash_fnmatch");

    // sash_regex_replace(str_ptr, str_len, pat_ptr, pat_len, rep_ptr, rep_len, global) -> (ptr, len)
    let mut regex_replace_sig = module.make_signature();
    for _ in 0..7 { regex_replace_sig.params.push(AbiParam::new(I64)); }
    regex_replace_sig.returns.push(AbiParam::new(I64)); // ptr
    regex_replace_sig.returns.push(AbiParam::new(I64)); // len
    let regex_replace_func = module.declare_function("sash_regex_replace", Linkage::Import, &regex_replace_sig)
        .expect("declare sash_regex_replace");

    // sash_strcmp(ptr1, len1, ptr2, len2) -> i64 (<0, 0, >0)
    let strcmp_func = module.declare_function("sash_strcmp", Linkage::Import, &regex_match_sig)
        .expect("declare sash_strcmp");

    // sash_table_new(capacity) -> *mut SashTable
    let mut table_new_sig = module.make_signature();
    table_new_sig.params.push(AbiParam::new(I64));
    table_new_sig.returns.push(AbiParam::new(I64));
    let table_new_func = module.declare_function("sash_table_new", Linkage::Import, &table_new_sig)
        .expect("declare sash_table_new");

    // sash_table_set(table, key_ptr, key_len, val_ptr, val_len)
    let mut table_set_sig = module.make_signature();
    table_set_sig.params.push(AbiParam::new(I64)); // table
    table_set_sig.params.push(AbiParam::new(I64)); // key_ptr
    table_set_sig.params.push(AbiParam::new(I64)); // key_len
    table_set_sig.params.push(AbiParam::new(I64)); // val_ptr
    table_set_sig.params.push(AbiParam::new(I64)); // val_len
    let table_set_func = module.declare_function("sash_table_set", Linkage::Import, &table_set_sig)
        .expect("declare sash_table_set");

    // sash_table_get(table, key_ptr, key_len, out_len_ptr) -> val_ptr
    let mut table_get_sig = module.make_signature();
    table_get_sig.params.push(AbiParam::new(I64)); // table
    table_get_sig.params.push(AbiParam::new(I64)); // key_ptr
    table_get_sig.params.push(AbiParam::new(I64)); // key_len
    table_get_sig.params.push(AbiParam::new(I64)); // out_len_ptr
    table_get_sig.returns.push(AbiParam::new(I64)); // val_ptr
    let table_get_func = module.declare_function("sash_table_get", Linkage::Import, &table_get_sig)
        .expect("declare sash_table_get");

    // sash_table_len(table) -> count
    let mut table_len_sig = module.make_signature();
    table_len_sig.params.push(AbiParam::new(I64));
    table_len_sig.returns.push(AbiParam::new(I64));
    let table_len_func = module.declare_function("sash_table_len", Linkage::Import, &table_len_sig)
        .expect("declare sash_table_len");

    // sash_table_value(table, index, out_len_ptr) -> val_ptr
    let mut table_value_sig = module.make_signature();
    table_value_sig.params.push(AbiParam::new(I64)); // table
    table_value_sig.params.push(AbiParam::new(I64)); // index
    table_value_sig.params.push(AbiParam::new(I64)); // out_len_ptr
    table_value_sig.returns.push(AbiParam::new(I64)); // val_ptr
    let table_value_func = module.declare_function("sash_table_value", Linkage::Import, &table_value_sig)
        .expect("declare sash_table_value");

    // sash_table_key(table, index, out_len_ptr) -> key_ptr
    let mut table_key_sig = module.make_signature();
    table_key_sig.params.push(AbiParam::new(I64)); // table
    table_key_sig.params.push(AbiParam::new(I64)); // index
    table_key_sig.params.push(AbiParam::new(I64)); // out_len_ptr
    table_key_sig.returns.push(AbiParam::new(I64)); // key_ptr
    let table_key_func = module.declare_function("sash_table_key", Linkage::Import, &table_key_sig)
        .expect("declare sash_table_key");

    // sash_table_delete(table, key_ptr, key_len) -> status
    let mut table_delete_sig = module.make_signature();
    table_delete_sig.params.push(AbiParam::new(I64)); // table
    table_delete_sig.params.push(AbiParam::new(I64)); // key_ptr
    table_delete_sig.params.push(AbiParam::new(I64)); // key_len
    table_delete_sig.returns.push(AbiParam::new(I64)); // 0=ok, 1=not found
    let table_delete_func = module.declare_function("sash_table_delete", Linkage::Import, &table_delete_sig)
        .expect("declare sash_table_delete");

    // sash_read_line(fd) -> (ptr, len)
    let mut read_line_sig = Signature::new(CallConv::SystemV);
    read_line_sig.params.push(AbiParam::new(I64)); // fd
    read_line_sig.returns.push(AbiParam::new(I64)); // ptr
    read_line_sig.returns.push(AbiParam::new(I64)); // len
    let read_line_func = module.declare_function("sash_read_line", Linkage::Import, &read_line_sig)
        .expect("declare sash_read_line");

    // sash_read_delim(fd, delim_byte) -> (ptr, len)
    let mut read_delim_sig = Signature::new(CallConv::SystemV);
    read_delim_sig.params.push(AbiParam::new(I64)); // fd
    read_delim_sig.params.push(AbiParam::new(I64)); // delim byte
    read_delim_sig.returns.push(AbiParam::new(I64)); // ptr
    read_delim_sig.returns.push(AbiParam::new(I64)); // len
    let read_delim_func = module.declare_function("sash_read_delim", Linkage::Import, &read_delim_sig)
        .expect("declare sash_read_delim");

    // sash_splice_argv(prefix, argv, argc) -> merged_argv
    let mut splice_argv_sig = module.make_signature();
    splice_argv_sig.params.push(AbiParam::new(I64)); // prefix (null-terminated array)
    splice_argv_sig.params.push(AbiParam::new(I64)); // argv
    splice_argv_sig.params.push(AbiParam::new(I64)); // argc
    splice_argv_sig.returns.push(AbiParam::new(I64)); // merged argv ptr
    let splice_argv_func = module.declare_function("sash_splice_argv", Linkage::Import, &splice_argv_sig)
        .expect("declare sash_splice_argv");

    // sash_source_kvp(path_ptr, path_len) -> i64
    let mut source_kvp_sig = module.make_signature();
    source_kvp_sig.params.push(AbiParam::new(I64));
    source_kvp_sig.params.push(AbiParam::new(I64));
    source_kvp_sig.returns.push(AbiParam::new(I64));
    let source_kvp_func = module.declare_function("sash_source_kvp", Linkage::Import, &source_kvp_sig)
        .expect("declare sash_source_kvp");

    // sash_file_test(path_ptr, path_len, op) -> i64
    let mut file_test_sig = module.make_signature();
    file_test_sig.params.push(AbiParam::new(I64)); // path_ptr
    file_test_sig.params.push(AbiParam::new(I64)); // path_len
    file_test_sig.params.push(AbiParam::new(I64)); // op
    file_test_sig.returns.push(AbiParam::new(I64)); // result
    let file_test_func = module.declare_function("sash_file_test", Linkage::Import, &file_test_sig)
        .expect("declare sash_file_test");

    // sash_pow(base, exp) -> i64
    let mut pow_sig = module.make_signature();
    pow_sig.params.push(AbiParam::new(I64));
    pow_sig.params.push(AbiParam::new(I64));
    pow_sig.returns.push(AbiParam::new(I64));
    let pow_func = module.declare_function("sash_pow", Linkage::Import, &pow_sig)
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
    let substring_func = module.declare_function("sash_substring", Linkage::Import, &substring_sig)
        .expect("declare sash_substring");

    // sash_isatty(fd) -> i64
    let mut isatty_sig = module.make_signature();
    isatty_sig.params.push(AbiParam::new(I64));
    isatty_sig.returns.push(AbiParam::new(I64));
    let isatty_func = module.declare_function("sash_isatty", Linkage::Import, &isatty_sig)
        .expect("declare sash_isatty");

    // sash_file_newer(p1_ptr, p1_len, p2_ptr, p2_len) -> i64
    let mut file_newer_sig = module.make_signature();
    file_newer_sig.params.push(AbiParam::new(I64));
    file_newer_sig.params.push(AbiParam::new(I64));
    file_newer_sig.params.push(AbiParam::new(I64));
    file_newer_sig.params.push(AbiParam::new(I64));
    file_newer_sig.returns.push(AbiParam::new(I64));
    let file_newer_func = module.declare_function("sash_file_newer", Linkage::Import, &file_newer_sig)
        .expect("declare sash_file_newer");

    // sash_file_same_dev(p1_ptr, p1_len, p2_ptr, p2_len) -> i64
    let mut file_same_dev_sig = module.make_signature();
    file_same_dev_sig.params.push(AbiParam::new(I64));
    file_same_dev_sig.params.push(AbiParam::new(I64));
    file_same_dev_sig.params.push(AbiParam::new(I64));
    file_same_dev_sig.params.push(AbiParam::new(I64));
    file_same_dev_sig.returns.push(AbiParam::new(I64));
    let file_same_dev_func = module.declare_function("sash_file_same_dev", Linkage::Import, &file_same_dev_sig)
        .expect("declare sash_file_same_dev");

    // sash_chdir(path_ptr, path_len) -> i64
    let mut chdir_sig = module.make_signature();
    chdir_sig.params.push(AbiParam::new(I64));
    chdir_sig.params.push(AbiParam::new(I64));
    chdir_sig.returns.push(AbiParam::new(I64));
    let chdir_func = module.declare_function("sash_chdir", Linkage::Import, &chdir_sig)
        .expect("declare sash_chdir");

    // sash_getcwd(out_len_ptr) -> ptr
    let mut getcwd_sig = module.make_signature();
    getcwd_sig.params.push(AbiParam::new(I64)); // out_len ptr
    getcwd_sig.returns.push(AbiParam::new(I64)); // result ptr
    let getcwd_func = module.declare_function("sash_getcwd", Linkage::Import, &getcwd_sig)
        .expect("declare sash_getcwd");

    // sash_heredoc_redirect(fd, body_ptr, body_len)
    let mut heredoc_sig = module.make_signature();
    heredoc_sig.params.push(AbiParam::new(I64));
    heredoc_sig.params.push(AbiParam::new(I64));
    heredoc_sig.params.push(AbiParam::new(I64));
    let heredoc_func = module.declare_function("sash_heredoc_redirect", Linkage::Import, &heredoc_sig)
        .expect("declare sash_heredoc_redirect");

    // Case conversion functions: (s_ptr, s_len, out_ptr) -> result_len
    let mut case_sig = module.make_signature();
    case_sig.params.push(AbiParam::new(I64));
    case_sig.params.push(AbiParam::new(I64));
    case_sig.params.push(AbiParam::new(I64));
    case_sig.returns.push(AbiParam::new(I64));
    let toupper_first_func = module.declare_function("sash_toupper_first", Linkage::Import, &case_sig).expect("declare");
    let tolower_first_func = module.declare_function("sash_tolower_first", Linkage::Import, &case_sig).expect("declare");
    let toupper_all_func = module.declare_function("sash_toupper_all", Linkage::Import, &case_sig).expect("declare");
    let tolower_all_func = module.declare_function("sash_tolower_all", Linkage::Import, &case_sig).expect("declare");

    // sash_command_v(name_ptr, name_len) -> i64 (0=found, 1=not found)
    let mut cmdv_sig = module.make_signature();
    cmdv_sig.params.push(AbiParam::new(I64));
    cmdv_sig.params.push(AbiParam::new(I64));
    cmdv_sig.returns.push(AbiParam::new(I64));
    let command_v_func = module.declare_function("sash_command_v", Linkage::Import, &cmdv_sig).expect("declare");

    // sash_getpid() -> i64
    let mut getpid_sig = module.make_signature();
    getpid_sig.returns.push(AbiParam::new(I64));
    let getpid_func = module.declare_function("sash_getpid", Linkage::Import, &getpid_sig).expect("declare");

    // sash_epochseconds() -> (ptr, len)
    let mut epoch_sig = module.make_signature();
    epoch_sig.returns.push(AbiParam::new(I64));
    epoch_sig.returns.push(AbiParam::new(I64));
    let epochseconds_func = module.declare_function("sash_epochseconds", Linkage::Import, &epoch_sig).expect("declare");

    let mut random_sig = module.make_signature();
    random_sig.returns.push(AbiParam::new(I64));
    random_sig.returns.push(AbiParam::new(I64));
    let random_func = module.declare_function("sash_random", Linkage::Import, &random_sig).expect("declare");


    // sash_glob(dir_ptr, dir_len, pat_ptr, pat_len) -> table_ptr
    let mut glob_sig = module.make_signature();
    glob_sig.params.push(AbiParam::new(I64)); // dir_ptr
    glob_sig.params.push(AbiParam::new(I64)); // dir_len
    glob_sig.params.push(AbiParam::new(I64)); // pat_ptr
    glob_sig.params.push(AbiParam::new(I64)); // pat_len
    glob_sig.returns.push(AbiParam::new(I64)); // table_ptr
    let glob_func = module.declare_function("sash_glob", Linkage::Import, &glob_sig).expect("declare");

    let glob_regex_func = module.declare_function("sash_glob_regex", Linkage::Import, &glob_sig).expect("declare");

    // sash_trap(signum, handler) -> i64
    let mut trap_sig = module.make_signature();
    trap_sig.params.push(AbiParam::new(I64));
    trap_sig.params.push(AbiParam::new(I64));
    trap_sig.returns.push(AbiParam::new(I64));
    let trap_func = module.declare_function("sash_trap", Linkage::Import, &trap_sig).expect("declare");

    // sash_capture_start() -> (saved_fd, pipe_read_fd)
    let mut cap_start_sig = module.make_signature();
    cap_start_sig.returns.push(AbiParam::new(I64));
    cap_start_sig.returns.push(AbiParam::new(I64));
    let capture_start_func = module.declare_function("sash_capture_start", Linkage::Import, &cap_start_sig).expect("declare");

    // sash_capture_end(saved_fd, pipe_read_fd) -> (ptr, len)
    let mut cap_end_sig = module.make_signature();
    cap_end_sig.params.push(AbiParam::new(I64));
    cap_end_sig.params.push(AbiParam::new(I64));
    cap_end_sig.returns.push(AbiParam::new(I64));
    cap_end_sig.returns.push(AbiParam::new(I64));
    let capture_end_func = module.declare_function("sash_capture_end", Linkage::Import, &cap_end_sig).expect("declare");

    // sash_split_first_word(ptr, len, rest_ptr_out, rest_len_out) -> (word_ptr, word_len)
    let mut split_sig = module.make_signature();
    split_sig.params.push(AbiParam::new(I64)); // ptr
    split_sig.params.push(AbiParam::new(I64)); // len
    split_sig.params.push(AbiParam::new(I64)); // rest_ptr_out
    split_sig.params.push(AbiParam::new(I64)); // rest_len_out
    split_sig.returns.push(AbiParam::new(I64)); // word_ptr
    split_sig.returns.push(AbiParam::new(I64)); // word_len
    let split_first_word_func = module.declare_function("sash_split_first_word", Linkage::Import, &split_sig).expect("declare");

    // sash_split_into_array(ptr, len, delim, table) → void
    let mut split_arr_sig = module.make_signature();
    split_arr_sig.params.push(AbiParam::new(I64)); // ptr
    split_arr_sig.params.push(AbiParam::new(I64)); // len
    split_arr_sig.params.push(AbiParam::new(I64)); // delim
    split_arr_sig.params.push(AbiParam::new(I64)); // table
    let split_into_array_func = module.declare_function("sash_split_into_array", Linkage::Import, &split_arr_sig).expect("declare");

    // sash_read_all(fd) → (ptr, len)
    let mut read_all_sig = module.make_signature();
    read_all_sig.params.push(AbiParam::new(I64));
    read_all_sig.returns.push(AbiParam::new(I64));
    read_all_sig.returns.push(AbiParam::new(I64));
    let read_all_func = module.declare_function("sash_read_all", Linkage::Import, &read_all_sig).expect("declare");

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
        setenv: setenv_func,
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
        source_kvp: source_kvp_func,
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
        command_v: command_v_func,
        getpid: getpid_func,
        epochseconds: epochseconds_func,
        random: random_func,
        trap: trap_func,
        glob: glob_func,
        glob_regex: glob_regex_func,
        capture_start: capture_start_func,
        capture_end: capture_end_func,
        split_first_word: split_first_word_func,
        split_into_array: split_into_array_func,
        read_all: read_all_func,
        splice_argv: splice_argv_func,
        envp: envp_data,
        argc: argc_data,
        argv: argv_data,
    };

    // ── Intern string literals ─────────────────────────────────
    let mut literals: HashMap<String, cranelift_module::DataId> = HashMap::new();
    let mut intern = |module: &mut ObjectModule, s: &str| -> cranelift_module::DataId {
        if let Some(&id) = literals.get(s) {
            return id;
        }
        let name = format!("__str_{}", literals.len());
        let data_id = module.declare_data(&name, Linkage::Local, false, false)
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
    fn intern_value(module: &mut ObjectModule, intern: &mut dyn FnMut(&mut ObjectModule, &str) -> cranelift_module::DataId, v: &SsaValue) {
        match v {
            SsaValue::Literal(s) | SsaValue::Expr(s) => { intern(module, s); }
            SsaValue::Concat(parts) => { for p in parts { intern_value(module, intern, p); } }
            SsaValue::ParamExpansion { operator, operand, .. } => {
                intern(module, operand);
                if operator == "glob" || operator == "glob_regex" {
                    intern(module, ".");
                }
                if operator == "regex_replace" {
                    // operand is "pattern\0replacement\0global"
                    for part in operand.split('\0') {
                        if !part.is_empty() { intern(module, part); }
                    }
                }
                if operator == "/" || operator == "//" || operator == "/#" || operator == "/%" {
                    // operand is "pattern\0replacement"
                    for part in operand.split('\0') {
                        intern(module, part);
                    }
                }
            }
            SsaValue::CmdSubst { argv, .. } => { for a in argv { intern_value(module, intern, a); } }
            SsaValue::AssocArray(pairs) => {
                for (k, v) in pairs {
                    intern(module, k);
                    intern_value(module, intern, v);
                }
            }
            SsaValue::IndexedArray(elems) => {
                for v in elems { intern_value(module, intern, v); }
            }
            SsaValue::Subscript { index, .. } => { intern_value(module, intern, index); }
            SsaValue::ArgvIndex(_) => {}
            SsaValue::Ternary { cond, true_val, false_val } => {
                intern_value(module, intern, cond);
                intern_value(module, intern, true_val);
                intern_value(module, intern, false_val);
            }
            _ => {}
        }
    }
    fn intern_ir_expr(module: &mut ObjectModule, intern: &mut dyn FnMut(&mut ObjectModule, &str) -> cranelift_module::DataId, expr: &crate::ir::Expr) {
        
        match &expr.kind {
            crate::ir::ExprKind::Literal(s) | crate::ir::ExprKind::SingleQuoted(s) => { intern(module, s); }
            crate::ir::ExprKind::Concat(parts) | crate::ir::ExprKind::DoubleQuoted(parts) => {
                for p in parts { intern_ir_expr(module, intern, p); }
            }
            _ => {}
        }
    }
    fn intern_test_expr(module: &mut ObjectModule, intern: &mut dyn FnMut(&mut ObjectModule, &str) -> cranelift_module::DataId, test: &TestExpr) {
        fn intern_expr(module: &mut ObjectModule, intern: &mut dyn FnMut(&mut ObjectModule, &str) -> cranelift_module::DataId, expr: &crate::ir::Expr) {
            
            match &expr.kind {
                crate::ir::ExprKind::Literal(s) | crate::ir::ExprKind::SingleQuoted(s) => { intern(module, s); }
                crate::ir::ExprKind::DoubleQuoted(parts) | crate::ir::ExprKind::Concat(parts) => { for p in parts { intern_expr(module, intern, p); } }
                _ => {}
            }
        }
        match test {
            TestExpr::Binary { left, op: _, right } => {
                intern_expr(module, intern, left);
                intern_expr(module, intern, right);
            }
            TestExpr::Unary { operand, .. } => { intern_expr(module, intern, operand); }
            TestExpr::Not(inner) | TestExpr::Paren(inner) => { intern_test_expr(module, intern, inner); }
            TestExpr::And(a, b) | TestExpr::Or(a, b) => {
                intern_test_expr(module, intern, a);
                intern_test_expr(module, intern, b);
            }
        }
    }
    for block in program.blocks.values() {
        for inst in &block.insts {
            match inst {
                SsaInst::Assign { value, .. } => { intern_value(&mut module, &mut intern, value); }
                SsaInst::Command { name, args, redirects, .. } => {
                    if name != "[[" {
                        intern(&mut module, name);
                    }
                    for a in args { intern_value(&mut module, &mut intern, a); }
                    for r in redirects {
                        intern_ir_expr(&mut module, &mut intern, &r.target);
                    }
                }
                SsaInst::Test(test_expr) => {
                    intern_test_expr(&mut module, &mut intern, test_expr);
                }
                SsaInst::Export { value, name, .. } => {
                    intern_value(&mut module, &mut intern, value);
                    intern(&mut module, name);
                }
                SsaInst::Pipeline { stages, .. } => {
                    for (name, args, redirects) in stages {
                        intern(&mut module, name);
                        for a in args { intern_value(&mut module, &mut intern, a); }
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

    // ── Build main() ───────────────────────────────────────────
    let main_sig = Signature::new(CallConv::SystemV);
    let main_func_id = module.declare_function("main", Linkage::Export, &main_sig)
        .expect("declare main");

    let mut func = Function::with_name_signature(
        UserFuncName::user(0, 0),
        main_sig,
    );

    let mut fbc = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut func, &mut fbc);

        // Create Cranelift blocks for each SSA block
        let mut cl_blocks: HashMap<BlockId, cranelift_codegen::ir::Block> = HashMap::new();
        for &id in program.blocks.keys() {
            if !reachable.contains(&id) { continue; }
            let cl_block = builder.create_block();
            cl_blocks.insert(id, cl_block);
        }

        let entry_cl = cl_blocks[&program.entry];
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
                        if !base_vars.contains_key(&base) {
                            let sv = VarSlot::Str { ptr: builder.declare_var(I64), len: builder.declare_var(I64) };
                            base_vars.insert(base, sv);
                        }
                    }
                }
                if let SsaInst::Mapfile { array, .. } | SsaInst::ReadArray { array, .. } = inst {
                    if !base_vars.contains_key(array) {
                        let sv = VarSlot::Str { ptr: builder.declare_var(I64), len: builder.declare_var(I64) };
                        base_vars.insert(array.clone(), sv);
                    }
                }
                if let SsaInst::Command { name, args, .. } = inst {
                    if name == "__store_bg_pid" {
                        if let Some(SsaValue::Var(vr)) = args.first() {
                            let base = vr.name.clone();
                            if !base_vars.contains_key(&base) {
                                let sv = VarSlot::Str { ptr: builder.declare_var(I64), len: builder.declare_var(I64) };
                                base_vars.insert(base, sv);
                            }
                        }
                    }
                }
                if let SsaInst::Assign { target, .. } | SsaInst::Export { target, .. } | SsaInst::ReadLine { target, .. } | SsaInst::CaptureEnd { target } = inst {
                    let base = target.name.clone();
                    if !base_vars.contains_key(&base) {
                        let sv = if target.var_type == crate::ir::VarType::Int {
                            VarSlot::Int(builder.declare_var(I64))
                        } else {
                            VarSlot::Str {
                                ptr: builder.declare_var(I64),
                                len: builder.declare_var(I64),
                            }
                        };
                        base_vars.insert(base, sv);
                    }
                }
            }
            for phi in &block.phis {
                let base = phi.target.name.clone();
                if !base_vars.contains_key(&base) {
                    let sv = if phi.target.var_type == crate::ir::VarType::Int {
                        VarSlot::Int(builder.declare_var(I64))
                    } else {
                        VarSlot::Str {
                            ptr: builder.declare_var(I64),
                            len: builder.declare_var(I64),
                        }
                    };
                    base_vars.insert(base, sv);
                }
            }
        }
        // Second pass: map every versioned name to its base variable's CL vars
        for block in program.blocks.values() {
            for inst in &block.insts {
                if let SsaInst::ReadLineMulti { targets, .. } = inst {
                    for target in targets {
                        let key = target.display();
                        if !vars.contains_key(&key) {
                            let sv = base_vars[&target.name].clone();
                            vars.insert(key, sv);
                        }
                    }
                }
                if let SsaInst::Assign { target, .. } | SsaInst::Export { target, .. } | SsaInst::ReadLine { target, .. } | SsaInst::CaptureEnd { target } = inst {
                    let key = target.display();
                    if !vars.contains_key(&key) {
                        let sv = base_vars[&target.name].clone();
                        vars.insert(key, sv);
                    }
                }
                if let SsaInst::Command { name, args, .. } = inst {
                    if name == "__store_bg_pid" {
                        if let Some(SsaValue::Var(vr)) = args.first() {
                            let key = vr.display();
                            if !vars.contains_key(&key) {
                                let sv = base_vars[&vr.name].clone();
                                vars.insert(key, sv);
                            }
                        }
                    }
                }
            }
            for phi in &block.phis {
                let key = phi.target.display();
                if !vars.contains_key(&key) {
                    let sv = base_vars[&phi.target.name].clone();
                    vars.insert(key, sv);
                }
                for (_, src) in &phi.sources {
                    let sk = src.display();
                    if !vars.contains_key(&sk) {
                        let sv = base_vars[&src.name].clone();
                        vars.insert(sk, sv);
                    }
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
            let mut positional_refs: std::collections::HashSet<String> = std::collections::HashSet::new();
            for block in program.blocks.values() {
                for inst in &block.insts {
                    match inst {
                        SsaInst::Assign { value, .. } | SsaInst::Export { value, .. } => scan_value_for_vars(value, &mut positional_refs),
                        SsaInst::Command { args, redirects, .. } => {
                            for a in args { scan_value_for_vars(a, &mut positional_refs); }
                            for r in redirects { scan_redirect_for_vars(&r.target, &mut positional_refs); }
                        }
                        SsaInst::Pipeline { stages, .. } => {
                            for (_, args, redirects) in stages {
                                for a in args { scan_value_for_vars(a, &mut positional_refs); }
                                for r in redirects { scan_redirect_for_vars(&r.target, &mut positional_refs); }
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
                if var_name == "?" { continue; } // $? handled via exit_status_var
                if var_name == "$" { continue; } // $$ handled via sash_getpid
                let key_suffix = "_0"; // version 0
                let key = format!("{}{}", var_name, key_suffix);
                if vars.contains_key(&key) { continue; }

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

                    builder.ins().brif(in_bounds, load_block, &[], empty_block, &[]);

                    builder.switch_to_block(load_block);
                    builder.seal_block(load_block);
                    let offset = builder.ins().imul_imm(idx_val, 8);
                    let arg_ptr_addr = builder.ins().iadd(argv_val, offset);
                    let arg_ptr = builder.ins().load(I64, MemFlags::trusted(), arg_ptr_addr, 0);
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
                        StackSlotKind::ExplicitSlot, 8, 3,
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
                        StackSlotKind::ExplicitSlot, 8, 3,
                    ));
                    let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                    let join_ref = module.declare_func_in_func(externs.join_args, builder.func);
                    let call = builder.ins().call(join_ref, &[argv_val, argc_val, space_ptr, space_len, out_addr]);
                    let str_ptr = builder.inst_results(call)[0];
                    let str_len = builder.ins().stack_load(I64, out_slot, 0);
                    builder.def_var(sv.str_ptr(), str_ptr);
                    builder.def_var(sv.str_len(), str_len);
                } else if var_name == "?" {
                    // $? = exit status of last command, convert to string
                    let status = builder.use_var(exit_status_var);
                    let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot, 8, 3,
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
                        StackSlotKind::ExplicitSlot, 8, 3,
                    ));
                    let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                    let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                    let call2 = builder.ins().call(itoa_ref, &[pid, out_addr]);
                    let str_ptr = builder.inst_results(call2)[0];
                    let str_len = builder.ins().stack_load(I64, out_slot, 0);
                    builder.def_var(sv.str_ptr(), str_ptr);
                    builder.def_var(sv.str_len(), str_len);
                } else {
                    // Environment variable: $HOME, $PATH, etc.
                    let name_str = intern(&mut module, var_name);
                    let name_gv = module.declare_data_in_func(name_str, builder.func);
                    let name_ptr = builder.ins().global_value(I64, name_gv);
                    let name_len = builder.ins().iconst(I64, var_name.len() as i64);

                    // Stack slot for out_len
                    let out_len_slot = builder.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3),
                    );
                    let out_len_addr = builder.ins().stack_addr(I64, out_len_slot, 0);

                    let getenv_ref = module.declare_func_in_func(externs.getenv, builder.func);
                    let call = builder.ins().call(getenv_ref, &[name_ptr, name_len, out_len_addr]);
                    let val_ptr = builder.inst_results(call)[0];
                    let val_len = builder.ins().stack_load(I64, out_len_slot, 0);
                    builder.def_var(sv.str_ptr(), val_ptr);
                    builder.def_var(sv.str_len(), val_len);
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
            if !reachable.contains(&ssa_id) { continue; }
            let cl_block = cl_blocks[&ssa_id];

            let ssa_block = program.block(ssa_id);

            // Skip function blocks — their output was constant-folded
            if ssa_block.label.starts_with("fn_") {
                continue;
            }

            if idx > 0 {
                builder.switch_to_block(cl_block);
            }

            // Phi nodes are no-ops in Cranelift codegen: all SSA versions
            // of the same bash variable share the same Cranelift Variable,
            // so Cranelift's SSA builder handles the merge automatically.

            // Emit instructions
            let mut block_terminated = false;
            for inst in &ssa_block.insts {
                if block_terminated { break; }
                block_terminated = lower_inst(inst, &mut builder, &mut module,
                           &mut vars, &literals, &externs, space_id, nl_id,
                           test_result_var, exit_status_var, pipe_read_fd_var,
                           capture_saved_var, capture_pipe_var, func_constants,
                           &mut redirect_stack);
            }

            // Emit terminator (unless block was already terminated by exit)
            if !block_terminated {
            match &ssa_block.terminator {
                Terminator::Return => {
                    let zero = builder.ins().iconst(I64, 0);
                    let exit_ref = module.declare_func_in_func(externs.exit, builder.func);
                    builder.ins().call(exit_ref, &[zero]);
                    builder.ins().trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
                }
                Terminator::Goto(target) => {
                    let target_cl = cl_blocks[target];
                    builder.ins().jump(target_cl, &[]);
                }
                Terminator::Branch { true_block, false_block, .. } => {
                    let cond = builder.use_var(test_result_var);
                    let true_cl = cl_blocks[true_block];
                    let false_cl = cl_blocks[false_block];
                    builder.ins().brif(cond, true_cl, &[], false_cl, &[]);
                }
                Terminator::Loop { body_block, exit_block, .. } => {
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
    module.define_function(main_func_id, &mut ctx).expect("define function");

    let mut product = module.finish();

    // Embed sash-native source in .sash_source ELF section (DD#45)
    if let Some(src) = bash_source {
        let sash_source = crate::emit_source::emit_source(src.as_bytes(), false);
        let section_id = product.object.add_section(
            vec![],
            b".sash_source".to_vec(),
            object::SectionKind::Note,
        );
        product.object.append_section_data(section_id, sash_source.as_bytes(), 1);
    }

    product.emit().expect("emit object")
}

/// Returns true if the block is now terminated (e.g., by exit).
fn lower_inst(
    inst: &SsaInst,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &mut HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    space_id: cranelift_module::DataId,
    nl_id: cranelift_module::DataId,
    test_result_var: Variable,
    exit_status_var: Variable,
    pipe_read_fd_var: Variable,
    capture_saved_var: Variable,
    capture_pipe_var: Variable,
    func_constants: &HashMap<String, String>,
    redirect_stack: &mut Vec<Vec<SavedFd>>,
) -> bool {
    match inst {
        SsaInst::Assign { target, value } => {
            // Special: assignment to $? updates the exit_status_var directly
            let base = {
                let d = target.display();
                if let Some(idx) = d.rfind('_') {
                    let suffix = &d[idx+1..];
                    if suffix.chars().all(|c| c.is_ascii_digit()) { d[..idx].to_string() } else { d.to_string() }
                } else { d.to_string() }
            };
            if base == "?" {
                // Parse the literal value as an integer for exit_status_var
                let int_val = match value {
                    SsaValue::Literal(s) => {
                        let n = s.parse::<i64>().unwrap_or(0);
                        builder.ins().iconst(I64, n)
                    }
                    _ => {
                        let (ptr, len) = lower_value(value, builder, module, vars, literals, func_constants, externs, Some(exit_status_var));
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
                let int_val = lower_value_as_int(value, builder, module, vars, literals, func_constants, externs);
                builder.def_var(*sv.as_int(), int_val);
            } else {
                let (ptr_val, len_val) = lower_value(value, builder, module, vars, literals, func_constants, externs, Some(exit_status_var));
                builder.def_var(sv.str_ptr(), ptr_val);
                builder.def_var(sv.str_len(), len_val);
            }
            }
        }
        SsaInst::Command { name, args, redirects, .. } if name == "echo" => {
            let restore = emit_redirects(redirects, builder, module, externs, literals, vars, Some(exit_status_var));
            lower_echo(args, builder, module, vars, literals, externs, space_id, nl_id, Some(exit_status_var));
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
        SsaInst::Command { name, redirects, .. } if name == "__redirect_push" => {
            // Compound-command redirect: save fds, apply redirects
            let saved = emit_redirects(redirects, builder, module, externs, literals, vars, Some(exit_status_var));
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
                    StackSlotKind::ExplicitSlot, 8, 3,
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
                if let (SsaValue::Literal(sig_s), SsaValue::Literal(handler_s)) = (&args[0], &args[1]) {
                    let signum = builder.ins().iconst(I64, sig_s.parse::<i64>().unwrap_or(0));
                    let handler = builder.ins().iconst(I64, handler_s.parse::<i64>().unwrap_or(0));
                    let trap_ref = module.declare_func_in_func(externs.trap, builder.func);
                    builder.ins().call(trap_ref, &[signum, handler]);
                }
            }
        }
        SsaInst::Command { name, args, .. } if name == "__table_delete" => {
            // unset map[key] → sash_table_delete(table_ptr, key_ptr, key_len)
            if args.len() >= 2 {
                let (tbl_ptr, _) = lower_value(&args[0], builder, module, vars, literals, func_constants, externs, Some(exit_status_var));
                let (key_ptr, key_len) = lower_value(&args[1], builder, module, vars, literals, func_constants, externs, Some(exit_status_var));
                let del_ref = module.declare_func_in_func(externs.table_delete, builder.func);
                builder.ins().call(del_ref, &[tbl_ptr, key_ptr, key_len]);
            }
        }
        SsaInst::Command { name, .. } if name == ":" || name == "true" || name == "false" => {
            let val = if name == "false" { 0i64 } else { 1i64 };
            let v = builder.ins().iconst(I64, val);
            builder.def_var(test_result_var, v);
            // $? = 0 for true/:, 1 for false
            let es = builder.ins().iconst(I64, if name == "false" { 1 } else { 0 });
            builder.def_var(exit_status_var, es);
        }
        SsaInst::Command { name, args, .. } if name == "exit" => {
            // exit N — get the exit code from first arg, default 0
            let code = if let Some(arg) = args.first() {
                match arg {
                    SsaValue::Literal(s) => s.parse::<i64>().unwrap_or(0),
                    _ => 0,
                }
            } else {
                0
            };
            let code_val = builder.ins().iconst(I64, code);
            let exit_ref = module.declare_func_in_func(externs.exit, builder.func);
            builder.ins().call(exit_ref, &[code_val]);
            builder.ins().trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
            return true; // block is terminated
        }
        SsaInst::Command { name, args, redirects, .. } if name == "exec" => {
            // exec with no args = standalone redirects only (fd manipulation)
            if args.is_empty() {
                if !redirects.is_empty() {
                    emit_redirects(redirects, builder, module, externs, literals, vars, Some(exit_status_var));
                }
                let zero = builder.ins().iconst(I64, 0);
                builder.def_var(exit_status_var, zero);
            } else {
            // exec cmd args... — replace process (no fork)
            // Check if any arg is $@ — if so, we need a dynamic-size argv
            let has_dollar_at = args.iter().any(|a| {
                if let SsaValue::Var(vref) = a { vref.name == "@" } else { false }
            });
            if has_dollar_at {
                // Build prefix array (args before $@) on the stack, null-terminated
                let prefix_args: Vec<_> = args.iter().filter(|a| {
                    if let SsaValue::Var(vref) = a { vref.name != "@" } else { true }
                }).collect();
                let prefix_size = ((prefix_args.len() + 1) * 8) as u32;
                let prefix_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot, prefix_size, 3,
                ));
                for (i, arg_val) in prefix_args.iter().enumerate() {
                    let (ptr, _len) = lower_value(arg_val, builder, module, vars, literals, func_constants, externs, Some(exit_status_var));
                    builder.ins().stack_store(ptr, prefix_slot, (i * 8) as i32);
                }
                let null = builder.ins().iconst(I64, 0);
                builder.ins().stack_store(null, prefix_slot, (prefix_args.len() * 8) as i32);
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
                let call = builder.ins().call(splice_ref, &[prefix_addr, argv_val, argc_val_exec]);
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
                builder.ins().trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
                return true;
            }
            // Static argv path (no $@)
            let argc = args.len();
            let argv_size = ((argc + 1) * 8) as u32;
            let argv_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot, argv_size, 3,
            ));
            for (i, arg_val) in args.iter().enumerate() {
                let (ptr, _len) = lower_value(arg_val, builder, module, vars, literals, func_constants, externs, Some(exit_status_var));
                builder.ins().stack_store(ptr, argv_slot, (i * 8) as i32);
            }
            // Null terminator
            let null = builder.ins().iconst(I64, 0);
            builder.ins().stack_store(null, argv_slot, (argc * 8) as i32);
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
            builder.ins().trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());
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
                            StackSlotKind::ExplicitSlot, 8, 3,
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
                        builder.ins().call(write_ref, &[stdout_fd, new_ptr, new_len]);
                        let nl_gv = module.declare_data_in_func(nl_id, builder.func);
                        let nl_ptr2 = builder.ins().global_value(I64, nl_gv);
                        let one = builder.ins().iconst(I64, 1);
                        builder.ins().call(write_ref, &[stdout_fd, nl_ptr2, one]);
                    }
                }
            } else if let Some(arg) = args.first() {
                let (ptr, len) = lower_value(arg, builder, module, vars, literals, func_constants, externs, Some(exit_status_var));
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
                        StackSlotKind::ExplicitSlot, 8, 3,
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
                        StackSlotKind::ExplicitSlot, 8, 3,
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
                StackSlotKind::ExplicitSlot, 8, 3,
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
                let (ptr, len) = lower_value(arg, builder, module, vars, literals, func_constants, externs, Some(exit_status_var));
                let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
                let call = builder.ins().call(atoi_ref, &[ptr, len]);
                builder.inst_results(call)[0]
            } else {
                // No arg: wait for $! — look up the "!" variable
                let key = vars.keys().find(|k| {
                    let base = if let Some(idx) = k.rfind('_') {
                        let suffix = &k[idx+1..];
                        if suffix.chars().all(|c| c.is_ascii_digit()) { &k[..idx] } else { k.as_str() }
                    } else { k.as_str() };
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
        SsaInst::Command { name, args, .. } if name == "." || name == "source" => {
            // source/. — load flat KEY=VALUE file, set env vars
            if let Some(arg) = args.first() {
                let (ptr, len) = lower_value(arg, builder, module, vars, literals, func_constants, externs, Some(exit_status_var));
                let kvp_ref = module.declare_func_in_func(externs.source_kvp, builder.func);
                builder.ins().call(kvp_ref, &[ptr, len]);
                // Re-read all env-backed variables (source may have changed them)
                let getenv_ref = module.declare_func_in_func(externs.getenv, builder.func);
                for (key, sv) in vars.iter() {
                    if sv.is_int() { continue; }
                    // Extract base name: "SSH_AUTH_SOCK_0" → "SSH_AUTH_SOCK"
                    let base = if let Some(idx) = key.rfind('_') {
                        let suffix = &key[idx+1..];
                        if suffix.chars().all(|c| c.is_ascii_digit()) {
                            &key[..idx]
                        } else {
                            key.as_str()
                        }
                    } else {
                        key.as_str()
                    };
                    // Only re-read uppercase env-style vars (not positional, not local)
                    if !base.chars().all(|c| c.is_uppercase() || c == '_') || base.len() <= 1 {
                        continue;
                    }
                    let name_data = if let Some(&id) = literals.get(base) {
                        id
                    } else { continue; };
                    let name_gv = module.declare_data_in_func(name_data, builder.func);
                    let name_ptr = builder.ins().global_value(I64, name_gv);
                    let name_len = builder.ins().iconst(I64, base.len() as i64);
                    let out_slot = builder.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3),
                    );
                    let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                    let call = builder.ins().call(getenv_ref, &[name_ptr, name_len, out_addr]);
                    let val_ptr = builder.inst_results(call)[0];
                    let val_len = builder.ins().stack_load(I64, out_slot, 0);
                    builder.def_var(sv.str_ptr(), val_ptr);
                    builder.def_var(sv.str_len(), val_len);
                }
            }
        }
        SsaInst::Command { name, args, .. } if name == "shift" => {
            // shift N: sash_argc -= N; sash_argv += N*8
            let shift_n = if let Some(arg) = args.first() {
                match arg {
                    SsaValue::Literal(s) => s.parse::<i64>().unwrap_or(1),
                    _ => 1,
                }
            } else { 1 };
            let argc_gv = module.declare_data_in_func(externs.argc, builder.func);
            let argc_addr = builder.ins().global_value(I64, argc_gv);
            let old_argc = builder.ins().load(I64, MemFlags::trusted(), argc_addr, 0);
            let n_val = builder.ins().iconst(I64, shift_n);
            let new_argc = builder.ins().isub(old_argc, n_val);
            builder.ins().store(MemFlags::trusted(), new_argc, argc_addr, 0);

            let argv_gv = module.declare_data_in_func(externs.argv, builder.func);
            let argv_addr = builder.ins().global_value(I64, argv_gv);
            let old_argv = builder.ins().load(I64, MemFlags::trusted(), argv_addr, 0);
            let offset = builder.ins().iconst(I64, shift_n * 8);
            let new_argv = builder.ins().iadd(old_argv, offset);
            builder.ins().store(MemFlags::trusted(), new_argv, argv_addr, 0);

            // Reload all positional params from updated argv/argc
            let fresh_argv = builder.ins().load(I64, MemFlags::trusted(), argv_addr, 0);
            let fresh_argc = builder.ins().load(I64, MemFlags::trusted(), argc_addr, 0);
            for (key, sv) in vars.iter() {
                // key is like "1_0", "2_0", "#_0" — extract base name
                let base = key.split('_').next().unwrap_or("");
                if let Ok(idx) = base.parse::<i64>() {
                    // Guard: only reload if idx < fresh_argc (arg still exists)
                    let idx_val = builder.ins().iconst(I64, idx);
                    let in_bounds = builder.ins().icmp(IntCC::SignedLessThan, idx_val, fresh_argc);

                    let reload_block = builder.create_block();
                    let skip_block = builder.create_block();
                    let merge_block = builder.create_block();

                    builder.ins().brif(in_bounds, reload_block, &[], skip_block, &[]);

                    builder.switch_to_block(reload_block);
                    builder.seal_block(reload_block);
                    let offset = builder.ins().imul_imm(idx_val, 8);
                    let arg_ptr_addr = builder.ins().iadd(fresh_argv, offset);
                    let arg_ptr = builder.ins().load(I64, MemFlags::trusted(), arg_ptr_addr, 0);
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
                        StackSlotKind::ExplicitSlot, 8, 3,
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
                        StackSlotKind::ExplicitSlot, 8, 3,
                    ));
                    let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                    let join_ref = module.declare_func_in_func(externs.join_args, builder.func);
                    let call = builder.ins().call(join_ref, &[fresh_argv, fresh_argc, space_ptr, space_len, out_addr]);
                    let str_ptr = builder.inst_results(call)[0];
                    let str_len = builder.ins().stack_load(I64, out_slot, 0);
                    builder.def_var(sv.str_ptr(), str_ptr);
                    builder.def_var(sv.str_len(), str_len);
                }
            }
        }
        SsaInst::Command { name, args, redirects, .. } if name == "command" => {
            let restore = emit_redirects(redirects, builder, module, externs, literals, vars, Some(exit_status_var));
            // Skip flags: -p, -v, -V, --
            let mut cmd_start = 0;
            let mut is_v = false;
            for (i, arg) in args.iter().enumerate() {
                if let SsaValue::Literal(s) = arg {
                    match s.as_str() {
                        "-v" => { is_v = true; cmd_start = i + 1; }
                        "-p" | "-V" | "--" => { cmd_start = i + 1; }
                        _ => { cmd_start = i; break; }
                    }
                } else {
                    cmd_start = i;
                    break;
                }
            }
            if is_v && cmd_start < args.len() {
                // command -v NAME — check if command exists
                let (name_ptr, name_len) = lower_value(&args[cmd_start], builder, module, vars, literals, func_constants, externs, Some(exit_status_var));
                let cmdv_ref = module.declare_func_in_func(externs.command_v, builder.func);
                let call = builder.ins().call(cmdv_ref, &[name_ptr, name_len]);
                let result = builder.inst_results(call)[0];
                builder.def_var(exit_status_var, result);
                let zero = builder.ins().iconst(I64, 0);
                let found = builder.ins().icmp(IntCC::Equal, result, zero);
                let tr = builder.ins().uextend(I64, found);
                builder.def_var(test_result_var, tr);
            } else if cmd_start < args.len() {
                if let SsaValue::Literal(real_name) = &args[cmd_start] {
                    let real_args = args[cmd_start + 1..].to_vec();
                    lower_exec(real_name, &real_args, builder, module, vars, literals, externs, test_result_var, exit_status_var);
                } else {
                    let all_args = args.clone();
                    let real_name_str = match &all_args[cmd_start] {
                        SsaValue::Literal(s) => s.clone(),
                        _ => "command".to_string(),
                    };
                    let real_args = all_args[cmd_start + 1..].to_vec();
                    lower_exec(&real_name_str, &real_args, builder, module, vars, literals, externs, test_result_var, exit_status_var);
                }
            }
            emit_restore(restore, builder, module, externs);
        }
        SsaInst::Command { name, args, redirects, .. } => {
            let restore = emit_redirects(redirects, builder, module, externs, literals, vars, Some(exit_status_var));
            lower_exec(name, args, builder, module, vars, literals, externs, test_result_var, exit_status_var);
            emit_restore(restore, builder, module, externs);
        }
        SsaInst::Phi(_) => {}
        SsaInst::SubscriptAssign { array, index, value, is_integer_key } => {
            let array_key = format!("{}_0", array);
            let sv = vars.get(&array_key).unwrap_or_else(|| {
                vars.keys().find(|k| k.starts_with(&format!("{}_", array)))
                    .and_then(|k| vars.get(k))
                    .unwrap_or_else(|| panic!("ICE: undefined array: {}", array))
            });
            let table_ptr = builder.use_var(sv.str_ptr());

            let (key_ptr, key_len) = if *is_integer_key {
                let idx_int = lower_value_as_int(index, builder, module, vars, literals, func_constants, externs);
                let idx_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot, 8, 3,
                ));
                builder.ins().stack_store(idx_int, idx_slot, 0);
                let kp = builder.ins().stack_addr(I64, idx_slot, 0);
                let kl = builder.ins().iconst(I64, 8);
                (kp, kl)
            } else {
                lower_value(index, builder, module, vars, literals, func_constants, externs, Some(exit_status_var))
            };

            let (val_ptr, val_len) = lower_value(value, builder, module, vars, literals, func_constants, externs, Some(exit_status_var));
            let set_ref = module.declare_func_in_func(externs.table_set, builder.func);
            builder.ins().call(set_ref, &[table_ptr, key_ptr, key_len, val_ptr, val_len]);
        }
        SsaInst::ArrayAppend { array, value } => {
            let array_key = format!("{}_0", array);
            let sv = vars.get(&array_key).unwrap_or_else(|| {
                vars.keys().find(|k| k.starts_with(&format!("{}_", array)))
                    .and_then(|k| vars.get(k))
                    .unwrap_or_else(|| panic!("ICE: undefined array: {}", array))
            });
            let table_ptr = builder.use_var(sv.str_ptr());

            // Get current length as the next integer key
            let len_ref = module.declare_func_in_func(externs.table_len, builder.func);
            let call = builder.ins().call(len_ref, &[table_ptr]);
            let next_idx = builder.inst_results(call)[0];

            let idx_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot, 8, 3,
            ));
            builder.ins().stack_store(next_idx, idx_slot, 0);
            let kp = builder.ins().stack_addr(I64, idx_slot, 0);
            let kl = builder.ins().iconst(I64, 8);

            let (val_ptr, val_len) = lower_value(value, builder, module, vars, literals, func_constants, externs, Some(exit_status_var));
            let set_ref = module.declare_func_in_func(externs.table_set, builder.func);
            builder.ins().call(set_ref, &[table_ptr, kp, kl, val_ptr, val_len]);
        }
        SsaInst::Mapfile { array, fd } => {
            // Read all lines from fd into an indexed array
            let array_key = format!("{}_0", array);
            let sv = vars.get(&array_key).unwrap_or_else(|| {
                vars.keys().find(|k| k.starts_with(&format!("{}_", array)))
                    .and_then(|k| vars.get(k))
                    .unwrap_or_else(|| panic!("ICE: undefined array: {}", array))
            });
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
            let got_data = builder.ins().icmp(IntCC::SignedGreaterThan, line_len, zero);
            builder.ins().brif(got_data, loop_body, &[], loop_exit, &[]);

            builder.switch_to_block(loop_body);
            builder.seal_block(loop_body);

            // Key = current table length (next integer index)
            let len_ref = module.declare_func_in_func(externs.table_len, builder.func);
            let len_call = builder.ins().call(len_ref, &[table_ptr]);
            let next_idx = builder.inst_results(len_call)[0];
            let idx_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot, 8, 3,
            ));
            builder.ins().stack_store(next_idx, idx_slot, 0);
            let kp = builder.ins().stack_addr(I64, idx_slot, 0);
            let kl = builder.ins().iconst(I64, 8);
            let set_ref = module.declare_func_in_func(externs.table_set, builder.func);
            builder.ins().call(set_ref, &[table_ptr, kp, kl, line_ptr, line_len]);
            builder.ins().jump(loop_hdr, &[]);
            builder.seal_block(loop_hdr);

            builder.switch_to_block(loop_exit);
            builder.seal_block(loop_exit);
        }
        SsaInst::ReadArray { array, fd, delim } => {
            // Slurp all input from fd, then split by delimiter into indexed array
            let array_key = format!("{}_0", array);
            let sv = vars.get(&array_key).unwrap_or_else(|| {
                vars.keys().find(|k| k.starts_with(&format!("{}_", array)))
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
            builder.ins().call(split_ref, &[data_ptr, data_len, delim_val, table_ptr]);
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
        SsaInst::Export { target, value, name } => {
            // First assign the value (same as Assign)
            let (ptr, len) = lower_value(value, builder, module, vars, literals, func_constants, externs, Some(exit_status_var));
            let sv = &vars[&target.display()];
            builder.def_var(sv.str_ptr(), ptr);
            builder.def_var(sv.str_len(), len);
            // Then call sash_setenv to push to environment
            let name_id = literals[name];
            let name_gv = module.declare_data_in_func(name_id, builder.func);
            let name_ptr = builder.ins().global_value(I64, name_gv);
            let name_len = builder.ins().iconst(I64, name.len() as i64);
            let setenv_ref = module.declare_func_in_func(externs.setenv, builder.func);
            builder.ins().call(setenv_ref, &[name_ptr, name_len, ptr, len]);
        }
        SsaInst::Test(test_expr) => {
            let result = lower_test_expr(test_expr, builder, module, vars, literals, externs, Some(exit_status_var));
            builder.def_var(test_result_var, result);
        }
        SsaInst::ReadLine { target, fd, delim, redirects } => {
            // Set up redirects (e.g. herestring <<< "text")
            let saved = emit_redirects(redirects, builder, module, externs, literals, vars, Some(exit_status_var));
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
        SsaInst::ReadLineMulti { targets, fd, delim, redirects } => {
            let saved = emit_redirects(redirects, builder, module, externs, literals, vars, Some(exit_status_var));
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
            let mut cur_ptr = line_ptr;
            let mut cur_len = line_len;

            for (i, target) in targets.iter().enumerate() {
                let sv = &vars[&target.display()];
                if i == targets.len() - 1 {
                    builder.def_var(sv.str_ptr(), cur_ptr);
                    builder.def_var(sv.str_len(), cur_len);
                } else {
                    let rest_ptr_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot, 8, 3,
                    ));
                    let rest_len_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot, 8, 3,
                    ));
                    let rest_ptr_addr = builder.ins().stack_addr(I64, rest_ptr_slot, 0);
                    let rest_len_addr = builder.ins().stack_addr(I64, rest_len_slot, 0);
                    let split_call = builder.ins().call(split_ref, &[cur_ptr, cur_len, rest_ptr_addr, rest_len_addr]);
                    let word_ptr = builder.inst_results(split_call)[0];
                    let word_len = builder.inst_results(split_call)[1];
                    builder.def_var(sv.str_ptr(), word_ptr);
                    builder.def_var(sv.str_len(), word_len);
                    cur_ptr = builder.ins().stack_load(I64, rest_ptr_slot, 0);
                    cur_len = builder.ins().stack_load(I64, rest_len_slot, 0);
                }
            }
            emit_restore(saved, builder, module, externs);
        }
        SsaInst::Pipeline { stages, capture_read_fd } => {
            for (_i, (_n, _, _r)) in stages.iter().enumerate() {
            }
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
                    StackSlotKind::ExplicitSlot, 16, 3,
                ));
                let addr = builder.ins().stack_addr(I64, slot, 0);
                builder.ins().call(pipe_ref, &[addr]);
                pipe_slots.push(slot);
            }

            // Stack slots for child pids
            let pid_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot, (n * 8) as u32, 3,
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
                builder.ins().brif(is_child, child_block, &[], parent_block, &[]);

                // Child block
                builder.switch_to_block(child_block);
                builder.seal_block(child_block);

                // Wire stdin from previous pipe
                if i > 0 {
                    let prev_read = builder.ins().stack_load(I64, pipe_slots[i-1], 0);
                    let stdin = builder.ins().iconst(I64, 0);
                    builder.ins().call(dup2_ref, &[prev_read, stdin]);
                }
                // Wire stdout to next pipe
                // Wire stdout to next pipe (or capture pipe for last stage)
                if i < n - 1 || *capture_read_fd {
                    let pipe_idx = if i < pipe_slots.len() { i } else { pipe_slots.len() - 1 };
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
                    let flags = builder.ins().iconst(I64, match redir.op {
                        crate::ir::RedirectOp::Write => 1,
                        crate::ir::RedirectOp::Append => 2,
                        _ => 0,
                    });
                    if let crate::ir::ExprKind::Literal(s) = &redir.target.kind {
                        if let Some(&data_id) = literals.get(s.as_str()) {
                            let gv = module.declare_data_in_func(data_id, builder.func);
                            let path_ptr = builder.ins().global_value(I64, gv);
                            builder.ins().call(redirect_ref, &[fd, path_ptr, flags]);
                        }
                    } else {
                        let target_ssa = ir_expr_to_ssa_value(&redir.target);
                        let (ptr, len) = lower_value(&target_ssa, builder, module, vars, literals,
                            &std::collections::HashMap::new(), externs, Some(exit_status_var));
                        let redirect_s_ref = module.declare_func_in_func(externs.redirect_s, builder.func);
                        builder.ins().call(redirect_s_ref, &[fd, ptr, len, flags]);
                    }
                }
                // Build argv and exec
                let argc = 1 + args.len();
                let argv_size = ((argc + 1) * 8) as u32;
                let argv_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot, argv_size, 3,
                ));
                let cmd_val = SsaValue::Literal(name.to_string());
                let (cmd_ptr, _) = lower_value(&cmd_val, builder, module, vars, literals,
                    &std::collections::HashMap::new(), externs, Some(exit_status_var));
                builder.ins().stack_store(cmd_ptr, argv_slot, 0);
                for (j, arg) in args.iter().enumerate() {
                    let offset = ((j + 1) * 8) as i32;
                    let (ptr, _) = lower_value(arg, builder, module, vars, literals,
                        &std::collections::HashMap::new(), externs, Some(exit_status_var));
                    builder.ins().stack_store(ptr, argv_slot, offset);
                }
                builder.ins().stack_store(null, argv_slot, (argc * 8) as i32);
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
            // Wait for all children
            for i in 0..n {
                let pid = builder.ins().stack_load(I64, pid_slot, (i * 8) as i32);
                builder.ins().call(wait_ref, &[pid]);
            }
        }
        SsaInst::Bless { source, target, test } => {
            // Runtime bless enforcement: evaluate test on source, abort if false.
            // On pass, copy source value to target (new clean SSA version).
            let result = lower_test_expr(test, builder, module, vars, literals, externs, Some(exit_status_var));
            let pass_block = builder.create_block();
            let fail_block = builder.create_block();
            let zero = builder.ins().iconst(I64, 0);
            let is_pass = builder.ins().icmp(IntCC::NotEqual, result, zero);
            builder.ins().brif(is_pass, pass_block, &[], fail_block, &[]);

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
            builder.ins().trap(cranelift_codegen::ir::TrapCode::user(1).unwrap());

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

/// Emit a byte-by-byte string comparison loop.
/// Returns an i64 value: 1 if equal, 0 if not.
fn emit_string_eq(
    builder: &mut FunctionBuilder,
    a_ptr: cranelift_codegen::ir::Value,
    a_len: cranelift_codegen::ir::Value,
    b_ptr: cranelift_codegen::ir::Value,
    b_len: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    let result_var = builder.declare_var(I64);
    let zero_init = builder.ins().iconst(I64, 0);
    builder.def_var(result_var, zero_init);

    // If lengths differ, not equal — skip to merge with result=0.
    let len_eq = builder.ins().icmp(IntCC::Equal, a_len, b_len);

    let cmp_block = builder.create_block();
    let ne_block = builder.create_block();
    let eq_block = builder.create_block();
    let merge_block = builder.create_block();

    builder.ins().brif(len_eq, cmp_block, &[], ne_block, &[]);

    // ne_block: result stays 0
    builder.switch_to_block(ne_block);
    builder.seal_block(ne_block);
    builder.ins().jump(merge_block, &[]);

    // cmp_block: byte-by-byte comparison loop
    builder.switch_to_block(cmp_block);
    builder.seal_block(cmp_block);

    let i_var = builder.declare_var(I64);
    let zero_i = builder.ins().iconst(I64, 0);
    builder.def_var(i_var, zero_i);

    let loop_hdr = builder.create_block();
    let loop_body = builder.create_block();
    let loop_ne = builder.create_block();

    builder.ins().jump(loop_hdr, &[]);

    // loop header: check i < len
    builder.switch_to_block(loop_hdr);
    let i_val = builder.use_var(i_var);
    let in_bounds = builder.ins().icmp(IntCC::SignedLessThan, i_val, a_len);
    builder.ins().brif(in_bounds, loop_body, &[], eq_block, &[]);

    // loop body: compare bytes
    builder.switch_to_block(loop_body);
    builder.seal_block(loop_body);
    let a_addr = builder.ins().iadd(a_ptr, i_val);
    let b_addr = builder.ins().iadd(b_ptr, i_val);
    let a_byte = builder.ins().load(I8, MemFlags::new(), a_addr, 0);
    let b_byte = builder.ins().load(I8, MemFlags::new(), b_addr, 0);
    let bytes_eq = builder.ins().icmp(IntCC::Equal, a_byte, b_byte);
    let i_next = builder.ins().iadd_imm(i_val, 1);
    builder.def_var(i_var, i_next);
    builder.ins().brif(bytes_eq, loop_hdr, &[], loop_ne, &[]);

    // loop_ne: bytes differed — result stays 0
    builder.switch_to_block(loop_ne);
    builder.seal_block(loop_ne);
    builder.ins().jump(merge_block, &[]);

    // eq_block: all bytes matched — set result to 1
    builder.switch_to_block(eq_block);
    let one = builder.ins().iconst(I64, 1);
    builder.def_var(result_var, one);
    builder.ins().jump(merge_block, &[]);

    // Seal blocks that have all predecessors known
    builder.seal_block(loop_hdr); // entered from cmp_block and loop_body
    builder.seal_block(eq_block); // entered from loop_hdr only

    // merge_block: read the result variable
    builder.switch_to_block(merge_block);
    builder.seal_block(merge_block);
    builder.use_var(result_var)
}

/// Lower a TestExpr to a Cranelift i64 value (1=true, 0=false).
fn lower_test_expr(
    test: &TestExpr,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
#[allow(unused_variables)]
        exit_status_var: Option<Variable>,
) -> cranelift_codegen::ir::Value {
    match test {
        TestExpr::Binary { left, op, right } => {
            match op.as_str() {
                "=" | "==" => {
                    // In [[ ]] context: quoted RHS → exact match, unquoted → glob match
                    let is_quoted = matches!(&right.kind,
                        crate::ir::ExprKind::DoubleQuoted(_) | crate::ir::ExprKind::SingleQuoted(_));
                    let (l_ptr, l_len) = lower_expr_value(left, builder, module, vars, literals, externs, exit_status_var);
                    let (r_ptr, r_len) = lower_expr_value(right, builder, module, vars, literals, externs, exit_status_var);
                    if is_quoted {
                        emit_string_eq(builder, l_ptr, l_len, r_ptr, r_len)
                    } else {
                        let fnmatch_ref = module.declare_func_in_func(externs.fnmatch, builder.func);
                        let call = builder.ins().call(fnmatch_ref, &[r_ptr, r_len, l_ptr, l_len]);
                        builder.inst_results(call)[0]
                    }
                }
                "=glob" => {
                    // Case pattern glob matching: fnmatch(pattern, string)
                    // Note: left=value, right=pattern (swap for fnmatch which takes pat first)
                    let (s_ptr, s_len) = lower_expr_value(left, builder, module, vars, literals, externs, exit_status_var);
                    let (p_ptr, p_len) = lower_expr_value(right, builder, module, vars, literals, externs, exit_status_var);
                    let fnmatch_ref = module.declare_func_in_func(externs.fnmatch, builder.func);
                    let call = builder.ins().call(fnmatch_ref, &[p_ptr, p_len, s_ptr, s_len]);
                    builder.inst_results(call)[0]
                }
                "=~" => {
                    let is_quoted = matches!(&right.kind,
                        crate::ir::ExprKind::DoubleQuoted(_) | crate::ir::ExprKind::SingleQuoted(_));
                    let (l_ptr, l_len) = lower_expr_value(left, builder, module, vars, literals, externs, exit_status_var);
                    let (r_ptr, r_len) = lower_expr_value(right, builder, module, vars, literals, externs, exit_status_var);
                    if is_quoted {
                        // Quoted regex RHS → literal string match (exact)
                        emit_string_eq(builder, l_ptr, l_len, r_ptr, r_len)
                    } else {
                        let regex_ref = module.declare_func_in_func(externs.regex_match, builder.func);
                        let call = builder.ins().call(regex_ref, &[l_ptr, l_len, r_ptr, r_len]);
                        builder.inst_results(call)[0]
                    }
                }
                "!=" => {
                    let is_quoted = matches!(&right.kind,
                        crate::ir::ExprKind::DoubleQuoted(_) | crate::ir::ExprKind::SingleQuoted(_));
                    let (l_ptr, l_len) = lower_expr_value(left, builder, module, vars, literals, externs, exit_status_var);
                    let (r_ptr, r_len) = lower_expr_value(right, builder, module, vars, literals, externs, exit_status_var);
                    if is_quoted {
                        let eq = emit_string_eq(builder, l_ptr, l_len, r_ptr, r_len);
                        let one = builder.ins().iconst(I64, 1);
                        builder.ins().isub(one, eq)
                    } else {
                        let fnmatch_ref = module.declare_func_in_func(externs.fnmatch, builder.func);
                        let call = builder.ins().call(fnmatch_ref, &[r_ptr, r_len, l_ptr, l_len]);
                        let matched = builder.inst_results(call)[0];
                        let one = builder.ins().iconst(I64, 1);
                        builder.ins().isub(one, matched)
                    }
                }
                ">" | "<" => {
                    // Lexicographic string comparison
                    let (l_ptr, l_len) = lower_expr_value(left, builder, module, vars, literals, externs, exit_status_var);
                    let (r_ptr, r_len) = lower_expr_value(right, builder, module, vars, literals, externs, exit_status_var);
                    let strcmp_ref = module.declare_func_in_func(externs.strcmp, builder.func);
                    let call = builder.ins().call(strcmp_ref, &[l_ptr, l_len, r_ptr, r_len]);
                    let cmp_result = builder.inst_results(call)[0];
                    let zero = builder.ins().iconst(I64, 0);
                    let cc = if op == ">" { IntCC::SignedGreaterThan } else { IntCC::SignedLessThan };
                    let cmp = builder.ins().icmp(cc, cmp_result, zero);
                    builder.ins().uextend(I64, cmp)
                }
                "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge" |
                ">=" | "<=" => {
                    // Numeric comparison: use int vars directly, atoi for string vars
                    let l_int = lower_expr_value_as_int(left, builder, module, vars, literals, externs);
                    let r_int = lower_expr_value_as_int(right, builder, module, vars, literals, externs);
                    let cc = match op.as_str() {
                        "-eq" | "==" => IntCC::Equal,
                        "-ne" | "!=" => IntCC::NotEqual,
                        "-lt" | "<" => IntCC::SignedLessThan,
                        "-le" | "<=" => IntCC::SignedLessThanOrEqual,
                        "-gt" | ">" => IntCC::SignedGreaterThan,
                        "-ge" | ">=" => IntCC::SignedGreaterThanOrEqual,
                        _ => unreachable!(),
                    };
                    let cmp = builder.ins().icmp(cc, l_int, r_int);
                    builder.ins().uextend(I64, cmp)
                }
                "-nt" => {
                    // file1 -nt file2: file1 is newer than file2
                    let (l_ptr, l_len) = lower_expr_value(left, builder, module, vars, literals, externs, exit_status_var);
                    let (r_ptr, r_len) = lower_expr_value(right, builder, module, vars, literals, externs, exit_status_var);
                    let fn_ref = module.declare_func_in_func(externs.file_newer, builder.func);
                    let call = builder.ins().call(fn_ref, &[l_ptr, l_len, r_ptr, r_len]);
                    builder.inst_results(call)[0]
                }
                "-ot" => {
                    // file1 -ot file2: file1 is older than file2 (reverse of -nt)
                    let (l_ptr, l_len) = lower_expr_value(left, builder, module, vars, literals, externs, exit_status_var);
                    let (r_ptr, r_len) = lower_expr_value(right, builder, module, vars, literals, externs, exit_status_var);
                    let fn_ref = module.declare_func_in_func(externs.file_newer, builder.func);
                    // Swap args: file2 -nt file1
                    let call = builder.ins().call(fn_ref, &[r_ptr, r_len, l_ptr, l_len]);
                    builder.inst_results(call)[0]
                }
                "-ef" => {
                    // file1 -ef file2: same device and inode
                    let (l_ptr, l_len) = lower_expr_value(left, builder, module, vars, literals, externs, exit_status_var);
                    let (r_ptr, r_len) = lower_expr_value(right, builder, module, vars, literals, externs, exit_status_var);
                    let fn_ref = module.declare_func_in_func(externs.file_same_dev, builder.func);
                    let call = builder.ins().call(fn_ref, &[l_ptr, l_len, r_ptr, r_len]);
                    builder.inst_results(call)[0]
                }
                other => {
                    panic!("ICE: unimplemented test operator: {}", other);
                }
            }
        }
        TestExpr::Unary { op, operand } => {
            let (ptr, len) = lower_expr_value(operand, builder, module, vars, literals, externs, exit_status_var);
            match op.as_str() {
                "-n" => {
                    // Non-empty string: len > 0
                    let zero = builder.ins().iconst(I64, 0);
                    let cmp = builder.ins().icmp(IntCC::SignedGreaterThan, len, zero);
                    builder.ins().uextend(I64, cmp)
                }
                "-z" => {
                    // Empty string: len == 0
                    let zero = builder.ins().iconst(I64, 0);
                    let cmp = builder.ins().icmp(IntCC::Equal, len, zero);
                    builder.ins().uextend(I64, cmp)
                }
                // Bare word in [[ ]] — implicit -n (non-empty test)
                "?" => {
                    let zero = builder.ins().iconst(I64, 0);
                    let cmp = builder.ins().icmp(IntCC::SignedGreaterThan, len, zero);
                    builder.ins().uextend(I64, cmp)
                }
                "-e" | "-f" | "-d" | "-r" | "-w" | "-x" | "-s" | "-L" | "-a" | "-c" | "-p" | "-k" | "-h" | "-b" | "-g" | "-u" | "-G" | "-O" | "-S" | "-N" => {
                    let op_code = match op.as_str() {
                        "-e" | "-a" => 0i64, // -a is same as -e
                        "-f" => 1,
                        "-d" => 2,
                        "-r" => 3,
                        "-w" => 4,
                        "-x" => 5,
                        "-s" => 6,
                        "-L" | "-h" => 7, // -h is same as -L
                        "-c" => 8,
                        "-p" => 9,
                        "-k" => 10,
                        "-b" => 11,
                        "-g" => 12,
                        "-u" => 13,
                        "-G" => 14,
                        "-O" => 15,
                        "-S" => 16,
                        "-N" => 17,
                        _ => unreachable!(),
                    };
                    let op_val = builder.ins().iconst(I64, op_code);
                    let ft_ref = module.declare_func_in_func(externs.file_test, builder.func);
                    let call = builder.ins().call(ft_ref, &[ptr, len, op_val]);
                    builder.inst_results(call)[0]
                }
                "-t" => {
                    // -t fd: test if fd is a terminal
                    let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
                    let call = builder.ins().call(atoi_ref, &[ptr, len]);
                    let fd_val = builder.inst_results(call)[0];
                    let isatty_ref = module.declare_func_in_func(externs.isatty, builder.func);
                    let call2 = builder.ins().call(isatty_ref, &[fd_val]);
                    builder.inst_results(call2)[0]
                }
                "-v" => {
                    // Variable is set: in sash, all vars are initialized,
                    // so -v is equivalent to -n (non-empty string).
                    let zero = builder.ins().iconst(I64, 0);
                    let cmp = builder.ins().icmp(IntCC::SignedGreaterThan, len, zero);
                    builder.ins().uextend(I64, cmp)
                }
                other => {
                    panic!("ICE: unimplemented unary test operator: {}", other);
                }
            }
        }
        TestExpr::Not(inner) => {
            let val = lower_test_expr(inner, builder, module, vars, literals, externs, exit_status_var);
            let one = builder.ins().iconst(I64, 1);
            builder.ins().isub(one, val)
        }
        TestExpr::And(a, b) => {
            let va = lower_test_expr(a, builder, module, vars, literals, externs, exit_status_var);
            let vb = lower_test_expr(b, builder, module, vars, literals, externs, exit_status_var);
            builder.ins().band(va, vb)
        }
        TestExpr::Or(a, b) => {
            let va = lower_test_expr(a, builder, module, vars, literals, externs, exit_status_var);
            let vb = lower_test_expr(b, builder, module, vars, literals, externs, exit_status_var);
            builder.ins().bor(va, vb)
        }
        TestExpr::Paren(inner) => {
            lower_test_expr(inner, builder, module, vars, literals, externs, exit_status_var)
        }
    }
}

/// Lower an IR Expr to a (ptr, len) pair for use in test expressions.
/// Uses SSA vars map to find the current version of variables.
fn lower_expr_value(
    expr: &crate::ir::Expr,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
#[allow(unused_variables)]
        exit_status_var: Option<Variable>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    
    match &expr.kind {
        crate::ir::ExprKind::Literal(s) | crate::ir::ExprKind::SingleQuoted(s) => {
            if let Some(&data_id) = literals.get(s.as_str()) {
                let gv = module.declare_data_in_func(data_id, builder.func);
                let ptr = builder.ins().global_value(I64, gv);
                let len = builder.ins().iconst(I64, s.len() as i64);
                (ptr, len)
            } else {
                let zero = builder.ins().iconst(I64, 0);
                (zero, zero)
            }
        }
        crate::ir::ExprKind::SimpleExpansion(name) => {
            // $? — exit status
            if name == "?" {
                if let Some(esv) = exit_status_var {
                    let status = builder.use_var(esv);
                    let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot, 8, 3,
                    ));
                    let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                    let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                    let call = builder.ins().call(itoa_ref, &[status, out_addr]);
                    let res_ptr = builder.inst_results(call)[0];
                    let res_len = builder.ins().stack_load(I64, out_slot, 0);
                    return (res_ptr, res_len);
                }
            }
            // $$ — process ID
            if name == "$" {
                let getpid_ref = module.declare_func_in_func(externs.getpid, builder.func);
                let call = builder.ins().call(getpid_ref, &[]);
                let pid = builder.inst_results(call)[0];
                let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot, 8, 3,
                ));
                let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                let call2 = builder.ins().call(itoa_ref, &[pid, out_addr]);
                let res_ptr = builder.inst_results(call2)[0];
                let res_len = builder.ins().stack_load(I64, out_slot, 0);
                return (res_ptr, res_len);
            }
            // Search vars map for the highest version of this variable
            // This finds phi-merged versions (x_3) as well as regular ones (x_1)
            let mut best_key = None;
            let mut best_ver: Option<u32> = None;
            for key in vars.keys() {
                if let Some(rest) = key.strip_prefix(name.as_str()) {
                    if rest.starts_with('_') {
                        if let Ok(v) = rest[1..].parse::<u32>() {
                            if best_ver.is_none() || v > best_ver.unwrap() {
                                best_ver = Some(v);
                                best_key = Some(key.clone());
                            }
                        }
                    }
                }
            }
            if let Some(key) = best_key {
                let sv = &vars[&key];
                if sv.is_int() {
                    let int_val = builder.use_var(*sv.as_int());
                    let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot, 8, 3,
                    ));
                    let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                    let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                    let call = builder.ins().call(itoa_ref, &[int_val, out_addr]);
                    let res_ptr = builder.inst_results(call)[0];
                    let res_len = builder.ins().stack_load(I64, out_slot, 0);
                    (res_ptr, res_len)
                } else {
                    let ptr = builder.use_var(sv.str_ptr());
                    let len = builder.use_var(sv.str_len());
                    (ptr, len)
                }
            } else {
                let zero = builder.ins().iconst(I64, 0);
                (zero, zero)
            }
        }
        crate::ir::ExprKind::DoubleQuoted(parts) | crate::ir::ExprKind::Concat(parts) => {
            if parts.len() == 1 {
                lower_expr_value(&parts[0], builder, module, vars, literals, externs, exit_status_var)
            } else if parts.len() >= 2 {
                // Multi-part: concat all parts using sash_concat(a_ptr, a_len, b_ptr, b_len, out_len_ptr) -> ptr
                let (mut acc_ptr, mut acc_len) = lower_expr_value(&parts[0], builder, module, vars, literals, externs, exit_status_var);
                let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot, 8, 3,
                ));
                let concat_ref = module.declare_func_in_func(externs.concat, builder.func);
                for part in &parts[1..] {
                    let (p2, l2) = lower_expr_value(part, builder, module, vars, literals, externs, exit_status_var);
                    let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                    let call = builder.ins().call(concat_ref, &[acc_ptr, acc_len, p2, l2, out_addr]);
                    acc_ptr = builder.inst_results(call)[0];
                    acc_len = builder.ins().stack_load(I64, out_slot, 0);
                }
                (acc_ptr, acc_len)
            } else {
                let zero = builder.ins().iconst(I64, 0);
                (zero, zero)
            }
        }
        crate::ir::ExprKind::ParameterExpansion(pe) => {
            if pe.operator.is_none() {
                // ${var} with no operator — same as $var (SimpleExpansion)
                let simple = crate::ir::Expr {
                    kind: crate::ir::ExprKind::SimpleExpansion(pe.name.clone()),
                    span: expr.span,
                };
                lower_expr_value(&simple, builder, module, vars, literals, externs, exit_status_var)
            } else {
                // Has operator — delegate to lower_value path via SSA
                let zero = builder.ins().iconst(I64, 0);
                (zero, zero)
            }
        }
        _ => {
            let zero = builder.ins().iconst(I64, 0);
            (zero, zero)
        }
    }
}

/// Lower an IR Expr to an i64 value for numeric contexts.
/// Int vars are loaded directly; string vars go through atoi.
fn lower_expr_value_as_int(
    expr: &crate::ir::Expr,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
) -> cranelift_codegen::ir::Value {
    
    match &expr.kind {
        crate::ir::ExprKind::Literal(s) => {
            builder.ins().iconst(I64, s.parse::<i64>().unwrap_or(0))
        }
        crate::ir::ExprKind::SimpleExpansion(name) => {
            // Find highest version of this variable
            let mut best_key = None;
            let mut best_ver: Option<u32> = None;
            for key in vars.keys() {
                if let Some(rest) = key.strip_prefix(name.as_str()) {
                    if rest.starts_with('_') {
                        if let Ok(v) = rest[1..].parse::<u32>() {
                            if best_ver.is_none() || v > best_ver.unwrap() {
                                best_ver = Some(v);
                                best_key = Some(key.clone());
                            }
                        }
                    }
                }
            }
            if let Some(key) = best_key {
                let sv = &vars[&key];
                if sv.is_int() {
                    builder.use_var(*sv.as_int())
                } else {
                    let ptr = builder.use_var(sv.str_ptr());
                    let len = builder.use_var(sv.str_len());
                    let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
                    let call = builder.ins().call(atoi_ref, &[ptr, len]);
                    builder.inst_results(call)[0]
                }
            } else {
                builder.ins().iconst(I64, 0)
            }
        }
        _ => {
            // Fallback: lower as string, atoi
            // Special case: ParameterExpansion (e.g., arrlen) — route through SSA value lowering
            if let crate::ir::ExprKind::ParameterExpansion(pe) = &expr.kind {
                let var = {
                    // Find highest version of this variable
                    let mut best_key = None;
                    let mut best_ver: Option<u32> = None;
                    for key in vars.keys() {
                        if let Some(rest) = key.strip_prefix(pe.name.as_str()) {
                            if rest.starts_with('_') {
                                if let Ok(v) = rest[1..].parse::<u32>() {
                                    if best_ver.is_none() || v > best_ver.unwrap() {
                                        best_ver = Some(v);
                                        best_key = Some(key.clone());
                                    }
                                }
                            }
                        }
                    }
                    best_key
                };
                if let Some(ref op) = pe.operator {
                    if op == "arrlen" {
                        if let Some(key) = var {
                            let sv = &vars[&key];
                            let table_ptr = builder.use_var(sv.str_ptr());
                            let len_ref = module.declare_func_in_func(externs.table_len, builder.func);
                            let call = builder.ins().call(len_ref, &[table_ptr]);
                            return builder.inst_results(call)[0];
                        }
                    }
                }
            }
            let (ptr, len) = lower_expr_value(expr, builder, module, vars, literals, externs, None);
            let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
            let call = builder.ins().call(atoi_ref, &[ptr, len]);
            builder.inst_results(call)[0]
        }
    }
}

fn lower_echo(
    args: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    space_id: cranelift_module::DataId,
    nl_id: cranelift_module::DataId,
#[allow(unused_variables)]
        exit_status_var: Option<Variable>,
) {
    let fd = builder.ins().iconst(I64, 1);
    let write_ref = module.declare_func_in_func(externs.write, builder.func);
    let empty_fc = std::collections::HashMap::new();

    // Check for -n flag (suppress trailing newline)
    let (print_args, suppress_newline) = if let Some(SsaValue::Literal(flag)) = args.first() {
        if flag == "-n" {
            (&args[1..], true)
        } else {
            (args, false)
        }
    } else {
        (args, false)
    };

    for (i, arg) in print_args.iter().enumerate() {
        if i > 0 {
            let sp_gv = module.declare_data_in_func(space_id, builder.func);
            let sp_ptr = builder.ins().global_value(I64, sp_gv);
            let sp_len = builder.ins().iconst(I64, 1);
            builder.ins().call(write_ref, &[fd, sp_ptr, sp_len]);
        }
        // For Concat values, write each part sequentially (no spaces)
        if let SsaValue::Concat(parts) = arg {
            for part in parts {
                let (ptr, len) = lower_value(part, builder, module, vars, literals, &empty_fc, externs, exit_status_var);
                builder.ins().call(write_ref, &[fd, ptr, len]);
            }
        } else {
            let (ptr, len) = lower_value(arg, builder, module, vars, literals, &empty_fc, externs, exit_status_var);
            builder.ins().call(write_ref, &[fd, ptr, len]);
        }
    }

    if !suppress_newline {
        let nl_gv = module.declare_data_in_func(nl_id, builder.func);
        let nl_ptr = builder.ins().global_value(I64, nl_gv);
        let nl_len = builder.ins().iconst(I64, 1);
        builder.ins().call(write_ref, &[fd, nl_ptr, nl_len]);
    }
}

fn lower_exec(
    name: &str,
    args: &[SsaValue],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
    test_result_var: Variable,
    exit_status_var: Variable,
) {
    let argc = 1 + args.len();
    let argv_size = ((argc + 1) * 8) as u32;
    let argv_slot = builder.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot, argv_size, 3,
    ));

    let cmd_val = SsaValue::Literal(name.to_string());
    let (cmd_ptr, _) = lower_value(&cmd_val, builder, module, vars, literals, &std::collections::HashMap::new(), externs, Some(exit_status_var));
    builder.ins().stack_store(cmd_ptr, argv_slot, 0);

    for (i, arg) in args.iter().enumerate() {
        let offset = ((i + 1) * 8) as i32;
        let (ptr, _) = lower_value(arg, builder, module, vars, literals, &std::collections::HashMap::new(), externs, Some(exit_status_var));
        builder.ins().stack_store(ptr, argv_slot, offset);
    }

    let null = builder.ins().iconst(I64, 0);
    builder.ins().stack_store(null, argv_slot, (argc * 8) as i32);

    let envp_gv = module.declare_data_in_func(externs.envp, builder.func);
    let envp_addr = builder.ins().global_value(I64, envp_gv);
    let envp = builder.ins().load(I64, MemFlags::new(), envp_addr, 0);

    let argv_addr = builder.ins().stack_addr(I64, argv_slot, 0);
    let run_ref = module.declare_func_in_func(externs.run, builder.func);
    let call = builder.ins().call(run_ref, &[argv_addr, envp]);
    let exit_status = builder.inst_results(call)[0];
    // Set test_result_var: 1 if exit status == 0 (success), 0 otherwise
    let zero = builder.ins().iconst(I64, 0);
    let ok = builder.ins().icmp(IntCC::Equal, exit_status, zero);
    let result = builder.ins().uextend(I64, ok);
    builder.def_var(test_result_var, result);
    builder.def_var(exit_status_var, exit_status);
}

fn lower_value(
    value: &SsaValue,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    func_constants: &HashMap<String, String>,
    externs: &ExternFuncs,
#[allow(unused_variables)]
        exit_status_var: Option<Variable>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    match value {
        SsaValue::Literal(s) | SsaValue::Expr(s) => {
            let data_id = literals[s];
            let gv = module.declare_data_in_func(data_id, builder.func);
            let ptr = builder.ins().global_value(I64, gv);
            let len = builder.ins().iconst(I64, s.len() as i64);
            (ptr, len)
        }
        SsaValue::Var(vref) => {
            let key = vref.display();
            // Special: $? — read exit_status_var and itoa
            let base = if let Some(idx) = key.rfind('_') {
                let suffix = &key[idx+1..];
                if suffix.chars().all(|c| c.is_ascii_digit()) { &key[..idx] } else { key.as_str() }
            } else { key.as_str() };
            if base == "?" {
                if let Some(esv) = exit_status_var {
                    let status = builder.use_var(esv);
                    let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot, 8, 3,
                    ));
                    let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                    let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                    let call = builder.ins().call(itoa_ref, &[status, out_addr]);
                    let res_ptr = builder.inst_results(call)[0];
                    let res_len = builder.ins().stack_load(I64, out_slot, 0);
                    return (res_ptr, res_len);
                }
            }
            if base == "$" {
                let getpid_ref = module.declare_func_in_func(externs.getpid, builder.func);
                let call = builder.ins().call(getpid_ref, &[]);
                let pid = builder.inst_results(call)[0];
                let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot, 8, 3,
                ));
                let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                let call2 = builder.ins().call(itoa_ref, &[pid, out_addr]);
                let res_ptr = builder.inst_results(call2)[0];
                let res_len = builder.ins().stack_load(I64, out_slot, 0);
                return (res_ptr, res_len);
            }
            let sv = match vars.get(&key) {
                Some(v) => v,
                None => {
                    // Undefined variable → empty string
                    let zero = builder.ins().iconst(I64, 0);
                    return (zero, zero);
                }
            };
            if sv.is_int() {
                // Int var used in string context: itoa
                let int_val = builder.use_var(*sv.as_int());
                let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot, 8, 3,
                ));
                let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                let call = builder.ins().call(itoa_ref, &[int_val, out_addr]);
                let res_ptr = builder.inst_results(call)[0];
                let res_len = builder.ins().stack_load(I64, out_slot, 0);
                (res_ptr, res_len)
            } else {
                let ptr = builder.use_var(sv.str_ptr());
                let len = builder.use_var(sv.str_len());
                (ptr, len)
            }
        }
        SsaValue::FuncCall(name) => {
            if let Some(constant) = func_constants.get(name.as_str()) {
                // Constant-folded: the function output is a known string
                let data_id = literals.get(constant.as_str()).unwrap_or_else(|| {
                    panic!("ICE: constant-folded string not interned: {:?}", constant);
                });
                let gv = module.declare_data_in_func(*data_id, builder.func);
                let ptr = builder.ins().global_value(I64, gv);
                let len = builder.ins().iconst(I64, constant.len() as i64);
                (ptr, len)
            } else {
                // Non-constant function — not yet supported
                panic!("ICE: non-constant elidable function call not yet supported: {}", name);
            }
        }
        SsaValue::Concat(parts) => {
            // Runtime string concatenation: fold left with sash_concat
            assert!(!parts.is_empty(), "empty Concat");
            let (mut acc_ptr, mut acc_len) = lower_value(&parts[0], builder, module, vars, literals, func_constants, externs, exit_status_var);
            for part in &parts[1..] {
                let (p_ptr, p_len) = lower_value(part, builder, module, vars, literals, func_constants, externs, exit_status_var);
                // Stack slot for out_len
                let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot, 8, 3,
                ));
                let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                let concat_ref = module.declare_func_in_func(externs.concat, builder.func);
                let call = builder.ins().call(concat_ref, &[acc_ptr, acc_len, p_ptr, p_len, out_addr]);
                acc_ptr = builder.inst_results(call)[0];
                acc_len = builder.ins().stack_load(I64, out_slot, 0);
            }
            (acc_ptr, acc_len)
        }
        SsaValue::ParamExpansion { var, operator, operand } => {
            // glob: arr=(*.sh) → sash_glob(".", pattern)
            if operator == "glob" {
                let dir_str = ".";
                let dir_id = literals.get(dir_str).unwrap_or_else(|| panic!("ICE: missing literal: {}", dir_str));
                let dir_gv = module.declare_data_in_func(*dir_id, builder.func);
                let dir_ptr = builder.ins().global_value(I64, dir_gv);
                let dir_len = builder.ins().iconst(I64, dir_str.len() as i64);
                let pat = operand.as_str();
                let pat_id = literals.get(pat).unwrap_or_else(|| panic!("ICE: missing literal: {}", pat));
                let pat_gv = module.declare_data_in_func(*pat_id, builder.func);
                let pat_ptr = builder.ins().global_value(I64, pat_gv);
                let pat_len = builder.ins().iconst(I64, pat.len() as i64);
                let glob_ref = module.declare_func_in_func(externs.glob, builder.func);
                let call = builder.ins().call(glob_ref, &[dir_ptr, dir_len, pat_ptr, pat_len]);
                let table_ptr = builder.inst_results(call)[0];
                let zero = builder.ins().iconst(I64, 0);
                return (table_ptr, zero);
            }
            if operator == "glob_regex" {
                let dir_str = ".";
                let dir_id = literals.get(dir_str).unwrap_or_else(|| panic!("ICE: missing literal: {}", dir_str));
                let dir_gv = module.declare_data_in_func(*dir_id, builder.func);
                let dir_ptr = builder.ins().global_value(I64, dir_gv);
                let dir_len = builder.ins().iconst(I64, dir_str.len() as i64);
                let pat = operand.as_str();
                let pat_id = literals.get(pat).unwrap_or_else(|| panic!("ICE: missing literal: {}", pat));
                let pat_gv = module.declare_data_in_func(*pat_id, builder.func);
                let pat_ptr = builder.ins().global_value(I64, pat_gv);
                let pat_len = builder.ins().iconst(I64, pat.len() as i64);
                let glob_regex_ref = module.declare_func_in_func(externs.glob_regex, builder.func);
                let call = builder.ins().call(glob_regex_ref, &[dir_ptr, dir_len, pat_ptr, pat_len]);
                let table_ptr = builder.inst_results(call)[0];
                let zero = builder.ins().iconst(I64, 0);
                return (table_ptr, zero);
            }

            // Get the variable's current value
            let key = var.display();
            let (s_ptr, s_len) = if let Some(sv) = vars.get(&key) {
                (builder.use_var(sv.str_ptr()), builder.use_var(sv.str_len()))
            } else if operator == ":-" || operator == "-" || operator == ":+" || operator == "+" {
                // Unset variable with default/alternate operator → treat as empty
                let null = builder.ins().iconst(I64, 0);
                let zero = builder.ins().iconst(I64, 0);
                (null, zero)
            } else {
                // Undefined variable — treat as empty string (bash behavior)
                let null = builder.ins().iconst(I64, 0);
                let zero = builder.ins().iconst(I64, 0);
                (null, zero)
            };

            // "strlen" doesn't use an operand
            if operator == "strlen" {
                let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot, 8, 3,
                ));
                let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                let call = builder.ins().call(itoa_ref, &[s_len, out_addr]);
                let result_ptr = builder.inst_results(call)[0];
                let result_len = builder.ins().stack_load(I64, out_slot, 0);
                return (result_ptr, result_len);
            }

            // ${#arr[@]} — array length
            if operator == "arrlen" {
                // s_ptr IS the table pointer (stored in str_ptr slot for arrays)
                let len_ref = module.declare_func_in_func(externs.table_len, builder.func);
                let call = builder.ins().call(len_ref, &[s_ptr]);
                let arr_len = builder.inst_results(call)[0];
                let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot, 8, 3,
                ));
                let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                let call2 = builder.ins().call(itoa_ref, &[arr_len, out_addr]);
                let result_ptr = builder.inst_results(call2)[0];
                let result_len = builder.ins().stack_load(I64, out_slot, 0);
                return (result_ptr, result_len);
            }
            if operator == "regex_replace" {
                // operand format: "pattern\0replacement\0global"
                let parts: Vec<&str> = operand.split('\0').collect();
                let pattern = parts.get(0).unwrap_or(&"");
                let replacement = parts.get(1).unwrap_or(&"");
                let global_s = parts.get(2).unwrap_or(&"0");
                let global_i: i64 = if *global_s == "1" { 1 } else { 0 };

                let pat_id = literals.get(*pattern).unwrap_or_else(|| panic!("ICE: missing literal: {}", pattern));
                let pat_gv = module.declare_data_in_func(*pat_id, builder.func);
                let pat_ptr = builder.ins().global_value(I64, pat_gv);
                let pat_len = builder.ins().iconst(I64, pattern.len() as i64);

                let rep_id = literals.get(*replacement).unwrap_or_else(|| panic!("ICE: missing literal: {}", replacement));
                let rep_gv = module.declare_data_in_func(*rep_id, builder.func);
                let rep_ptr = builder.ins().global_value(I64, rep_gv);
                let rep_len = builder.ins().iconst(I64, replacement.len() as i64);

                let global_val = builder.ins().iconst(I64, global_i);
                let replace_ref = module.declare_func_in_func(externs.regex_replace, builder.func);
                let call = builder.ins().call(replace_ref, &[s_ptr, s_len, pat_ptr, pat_len, rep_ptr, rep_len, global_val]);
                let result_ptr = builder.inst_results(call)[0];
                let result_len = builder.inst_results(call)[1];
                return (result_ptr, result_len);
            }
            if operator == "arrkeys" || operator == "arrvals" || operator == "arrjoin" {
                // Build separator-joined list of keys or values from a table.
                let len_ref = module.declare_func_in_func(externs.table_len, builder.func);
                let call = builder.ins().call(len_ref, &[s_ptr]);
                let arr_len = builder.inst_results(call)[0];

                // Buffer for concatenated output
                let buf_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot, 4096, 3,
                ));
                let buf_ptr = builder.ins().stack_addr(I64, buf_slot, 0);
                // Track current write position
                let pos_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot, 8, 3,
                ));
                let zero = builder.ins().iconst(I64, 0);
                builder.ins().stack_store(zero, pos_slot, 0);

                let item_fn = if operator == "arrkeys" {
                    module.declare_func_in_func(externs.table_key, builder.func)
                } else {
                    module.declare_func_in_func(externs.table_value, builder.func)
                };

                // Separator: arrjoin uses operand, others use space
                let sep_str = if operator == "arrjoin" { operand.as_str() } else { " " };
                let sep_len_val = sep_str.len() as i64;
                let sep_data_id = literals.get(sep_str).unwrap_or_else(|| {
                    panic!("ICE: join separator not interned: {:?}", sep_str);
                });
                let sep_gv = module.declare_data_in_func(*sep_data_id, builder.func);
                let sep_ptr = builder.ins().global_value(I64, sep_gv);
                let sep_len = builder.ins().iconst(I64, sep_len_val);

                // Loop index variable
                let idx_var = builder.declare_var(I64);
                let zero2 = builder.ins().iconst(I64, 0);
                builder.def_var(idx_var, zero2);

                let header = builder.create_block();
                let body = builder.create_block();
                let exit = builder.create_block();
                builder.ins().jump(header, &[]);

                // Header: check i < len
                builder.switch_to_block(header);
                let i = builder.use_var(idx_var);
                let done = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, i, arr_len);
                builder.ins().brif(done, exit, &[], body, &[]);

                // Body: get item, append space separator + item bytes
                builder.switch_to_block(body);
                builder.seal_block(body);

                let cur_i = builder.use_var(idx_var);

                // Insert space before all items except the first
                let need_sep = builder.ins().icmp_imm(IntCC::SignedGreaterThan, cur_i, 0);
                let sep_block = builder.create_block();
                let item_block = builder.create_block();
                builder.ins().brif(need_sep, sep_block, &[], item_block, &[]);

                builder.switch_to_block(sep_block);
                builder.seal_block(sep_block);
                let pos = builder.ins().stack_load(I64, pos_slot, 0);
                let dst = builder.ins().iadd(buf_ptr, pos);
                // Copy separator bytes
                let sep_copy_idx = builder.declare_var(I64);
                let sep_zero = builder.ins().iconst(I64, 0);
                builder.def_var(sep_copy_idx, sep_zero);
                let sep_hdr = builder.create_block();
                let sep_body = builder.create_block();
                let sep_done = builder.create_block();
                builder.ins().jump(sep_hdr, &[]);

                builder.switch_to_block(sep_hdr);
                let sj = builder.use_var(sep_copy_idx);
                let sep_end = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, sj, sep_len);
                builder.ins().brif(sep_end, sep_done, &[], sep_body, &[]);

                builder.switch_to_block(sep_body);
                builder.seal_block(sep_body);
                let sj2 = builder.use_var(sep_copy_idx);
                let src = builder.ins().iadd(sep_ptr, sj2);
                let b = builder.ins().load(cranelift_codegen::ir::types::I8, cranelift_codegen::ir::MemFlags::new(), src, 0);
                let d = builder.ins().iadd(dst, sj2);
                builder.ins().store(cranelift_codegen::ir::MemFlags::new(), b, d, 0);
                let next_sj = builder.ins().iadd_imm(sj2, 1);
                builder.def_var(sep_copy_idx, next_sj);
                builder.ins().jump(sep_hdr, &[]);
                builder.seal_block(sep_hdr);

                builder.switch_to_block(sep_done);
                builder.seal_block(sep_done);
                let pos_after_sep = builder.ins().iadd(pos, sep_len);
                builder.ins().stack_store(pos_after_sep, pos_slot, 0);
                builder.ins().jump(item_block, &[]);

                // Get item ptr+len from table
                builder.switch_to_block(item_block);
                builder.seal_block(item_block);
                let cur_i2 = builder.use_var(idx_var);
                let item_len_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot, 8, 3,
                ));
                let item_len_addr = builder.ins().stack_addr(I64, item_len_slot, 0);
                let call = builder.ins().call(item_fn, &[s_ptr, cur_i2, item_len_addr]);
                let item_ptr = builder.inst_results(call)[0];
                let item_len = builder.ins().stack_load(I64, item_len_slot, 0);

                // Byte-copy loop
                let copy_idx = builder.declare_var(I64);
                let czero = builder.ins().iconst(I64, 0);
                builder.def_var(copy_idx, czero);

                let copy_hdr = builder.create_block();
                let copy_body = builder.create_block();
                let copy_done = builder.create_block();
                builder.ins().jump(copy_hdr, &[]);

                builder.switch_to_block(copy_hdr);
                let cj = builder.use_var(copy_idx);
                let copy_end = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, cj, item_len);
                builder.ins().brif(copy_end, copy_done, &[], copy_body, &[]);

                builder.switch_to_block(copy_body);
                builder.seal_block(copy_body);
                let cj2 = builder.use_var(copy_idx);
                let cur_pos = builder.ins().stack_load(I64, pos_slot, 0);
                let src_addr = builder.ins().iadd(item_ptr, cj2);
                let byte = builder.ins().load(cranelift_codegen::ir::types::I8, cranelift_codegen::ir::MemFlags::new(), src_addr, 0);
                let dst_addr = builder.ins().iadd(buf_ptr, cur_pos);
                builder.ins().store(cranelift_codegen::ir::MemFlags::new(), byte, dst_addr, 0);
                let new_pos = builder.ins().iadd_imm(cur_pos, 1);
                builder.ins().stack_store(new_pos, pos_slot, 0);
                let next_cj = builder.ins().iadd_imm(cj2, 1);
                builder.def_var(copy_idx, next_cj);
                builder.ins().jump(copy_hdr, &[]);
                builder.seal_block(copy_hdr);

                builder.switch_to_block(copy_done);
                builder.seal_block(copy_done);

                // Increment loop index
                let cur_i3 = builder.use_var(idx_var);
                let next_i = builder.ins().iadd_imm(cur_i3, 1);
                builder.def_var(idx_var, next_i);
                builder.ins().jump(header, &[]);
                builder.seal_block(header);

                builder.switch_to_block(exit);
                builder.seal_block(exit);
                let final_len = builder.ins().stack_load(I64, pos_slot, 0);
                return (buf_ptr, final_len);
            }

            // Intern the operand pattern
            let pat_data_id = literals.get(operand.as_str()).unwrap_or_else(|| {
                panic!("ICE: param expansion operand not interned: {:?}", operand);
            });
            let pat_gv = module.declare_data_in_func(*pat_data_id, builder.func);
            let pat_ptr = builder.ins().global_value(I64, pat_gv);
            let pat_len = builder.ins().iconst(I64, operand.len() as i64);

            match operator.as_str() {
                "%%" | "%" => {
                    // strip_suffix returns new_len; ptr unchanged
                    let longest = builder.ins().iconst(I64, if operator == "%%" { 1 } else { 0 });
                    let func_ref = module.declare_func_in_func(externs.strip_suffix, builder.func);
                    let call = builder.ins().call(func_ref, &[s_ptr, s_len, pat_ptr, pat_len, longest]);
                    let new_len = builder.inst_results(call)[0];
                    (s_ptr, new_len)
                }
                "##" | "#" => {
                    // strip_prefix returns offset; ptr advances, len shrinks
                    let longest = builder.ins().iconst(I64, if operator == "##" { 1 } else { 0 });
                    let func_ref = module.declare_func_in_func(externs.strip_prefix, builder.func);
                    let call = builder.ins().call(func_ref, &[s_ptr, s_len, pat_ptr, pat_len, longest]);
                    let offset = builder.inst_results(call)[0];
                    let new_ptr = builder.ins().iadd(s_ptr, offset);
                    let new_len = builder.ins().isub(s_len, offset);
                    (new_ptr, new_len)
                }
                ":-" | "-" => {
                    // default_value(s_len) returns 0 if empty, 1 if set
                    let func_ref = module.declare_func_in_func(externs.default_value, builder.func);
                    let call = builder.ins().call(func_ref, &[s_len]);
                    let is_set = builder.inst_results(call)[0];
                    // if is_set: use (s_ptr, s_len), else use (pat_ptr, pat_len)
                    let result_ptr = builder.ins().select(is_set, s_ptr, pat_ptr);
                    let result_len = builder.ins().select(is_set, s_len, pat_len);
                    (result_ptr, result_len)
                }
                ":+" | "+" => {
                    // alternate_value: if variable is set/non-empty, use operand; else empty
                    let func_ref = module.declare_func_in_func(externs.default_value, builder.func);
                    let call = builder.ins().call(func_ref, &[s_len]);
                    let is_set = builder.inst_results(call)[0];
                    // Inverted from :- — if set, use operand; if empty, use empty
                    let null = builder.ins().iconst(I64, 0);
                    let zero = builder.ins().iconst(I64, 0);
                    let result_ptr = builder.ins().select(is_set, pat_ptr, null);
                    let result_len = builder.ins().select(is_set, pat_len, zero);
                    (result_ptr, result_len)
                }
                "substring" => {
                    // ${var:offset:length} — operand is "offset" or "offset:length"
                    let parts: Vec<&str> = operand.split(':').collect();
                    let offset_val = parts[0].parse::<i64>().unwrap_or(0);
                    let offset = builder.ins().iconst(I64, offset_val);
                    let (length, has_length) = if parts.len() > 1 {
                        let l = parts[1].parse::<i64>().unwrap_or(0);
                        (builder.ins().iconst(I64, l), builder.ins().iconst(I64, 1))
                    } else {
                        (builder.ins().iconst(I64, 0), builder.ins().iconst(I64, 0))
                    };
                    let func_ref = module.declare_func_in_func(externs.substring, builder.func);
                    let call = builder.ins().call(func_ref, &[s_ptr, s_len, offset, length, has_length]);
                    let results = builder.inst_results(call);
                    (results[0], results[1])
                }
                "^" | "^^" | "," | ",," => {
                    // Case conversion: allocate buffer, call runtime, return (ptr, len)
                    let buf_slot = builder.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot, 4096, 3,
                    ));
                    let buf_ptr = builder.ins().stack_addr(I64, buf_slot, 0);
                    let func_id = match operator.as_str() {
                        "^" => externs.toupper_first,
                        "^^" => externs.toupper_all,
                        "," => externs.tolower_first,
                        ",," => externs.tolower_all,
                        _ => unreachable!(),
                    };
                    let func_ref = module.declare_func_in_func(func_id, builder.func);
                    let call = builder.ins().call(func_ref, &[s_ptr, s_len, buf_ptr]);
                    let result_len = builder.inst_results(call)[0];
                    (buf_ptr, result_len)
                }
                "/" | "//" | "/#" | "/%" => {
                    // ${var/pat/rep} → coerce to regex_replace (DD#37)
                    // operand is "pattern\0replacement"
                    let parts: Vec<&str> = operand.splitn(2, '\0').collect();
                    let pattern = parts.get(0).unwrap_or(&"");
                    let replacement = parts.get(1).unwrap_or(&"");

                    let pat_id = literals.get(*pattern).unwrap_or_else(|| panic!("ICE: missing literal: {}", pattern));
                    let pat_gv = module.declare_data_in_func(*pat_id, builder.func);
                    let pat_ptr = builder.ins().global_value(I64, pat_gv);
                    let pat_len = builder.ins().iconst(I64, pattern.len() as i64);

                    let rep_id = literals.get(*replacement).unwrap_or_else(|| panic!("ICE: missing literal: {}", replacement));
                    let rep_gv = module.declare_data_in_func(*rep_id, builder.func);
                    let rep_ptr = builder.ins().global_value(I64, rep_gv);
                    let rep_len = builder.ins().iconst(I64, replacement.len() as i64);

                    let global_flag = if operator == "//" { 1i64 } else { 0i64 };
                    let global_val = builder.ins().iconst(I64, global_flag);

                    let func_ref = module.declare_func_in_func(externs.regex_replace, builder.func);
                    let call = builder.ins().call(func_ref, &[s_ptr, s_len, pat_ptr, pat_len, rep_ptr, rep_len, global_val]);
                    let result_ptr = builder.inst_results(call)[0];
                    let result_len = builder.inst_results(call)[1];
                    (result_ptr, result_len)
                }
                _ => {
                    panic!("ICE: unsupported parameter expansion operator: {}", operator);
                }
            }
        }
        SsaValue::ArithExpr { var, op, operand } => {
            let int_val = lower_value_as_int(&SsaValue::Var(var.clone()), builder, module, vars, literals, func_constants, externs);
            let rhs = builder.ins().iconst(I64, *operand);
            let result = emit_arith_op(*op, int_val, rhs, builder, module, externs, exit_status_var);
            // Convert back to string via itoa
            let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot, 8, 3,
            ));
            let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
            let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
            let call2 = builder.ins().call(itoa_ref, &[result, out_addr]);
            let res_ptr = builder.inst_results(call2)[0];
            let res_len = builder.ins().stack_load(I64, out_slot, 0);
            (res_ptr, res_len)
        }
        SsaValue::ArithBinOp { left, op, right } => {
            let l = lower_value_as_int(left, builder, module, vars, literals, func_constants, externs);
            let r = lower_value_as_int(right, builder, module, vars, literals, func_constants, externs);
            let result = emit_arith_op(*op, l, r, builder, module, externs, None);
            let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot, 8, 3,
            ));
            let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
            let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
            let call2 = builder.ins().call(itoa_ref, &[result, out_addr]);
            let res_ptr = builder.inst_results(call2)[0];
            let res_len = builder.ins().stack_load(I64, out_slot, 0);
            (res_ptr, res_len)
        }
        SsaValue::CmdSubst { argv: argv_values, redirects } => {
            // Builtin command substitutions — no fork/exec needed
            if let Some(SsaValue::Literal(name)) = argv_values.first() {
                match name.as_str() {
                    "epochseconds" => {
                        let func_ref = module.declare_func_in_func(externs.epochseconds, builder.func);
                        let call = builder.ins().call(func_ref, &[]);
                        let ptr = builder.inst_results(call)[0];
                        let len = builder.inst_results(call)[1];
                        return (ptr, len);
                    }
                    "random" => {
                        let func_ref = module.declare_func_in_func(externs.random, builder.func);
                        let call = builder.ins().call(func_ref, &[]);
                        let ptr = builder.inst_results(call)[0];
                        let len = builder.inst_results(call)[1];
                        return (ptr, len);
                    }
                    "getenv" if argv_values.len() >= 2 => {
                        // $(getenv NAME) — look up environment variable
                        let (name_ptr, name_len) = lower_value(
                            &argv_values[1], builder, module, vars, literals,
                            func_constants, externs, exit_status_var,
                        );
                        let out_len_slot = builder.create_sized_stack_slot(
                            StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3),
                        );
                        let out_len_addr = builder.ins().stack_addr(I64, out_len_slot, 0);
                        let getenv_ref = module.declare_func_in_func(externs.getenv, builder.func);
                        let call = builder.ins().call(getenv_ref, &[name_ptr, name_len, out_len_addr]);
                        let val_ptr = builder.inst_results(call)[0];
                        let val_len = builder.ins().stack_load(I64, out_len_slot, 0);
                        return (val_ptr, val_len);
                    }
                    "argc" => {
                        // $(argc) — return number of positional params as string
                        let argc_gv = module.declare_data_in_func(externs.argc, builder.func);
                        let argc_base = builder.ins().global_value(I64, argc_gv);
                        let argc_val = builder.ins().load(I64, MemFlags::new(), argc_base, 0);
                        // argc includes $0, so subtract 1 for $# equivalence
                        let one = builder.ins().iconst(I64, 1);
                        let count = builder.ins().isub(argc_val, one);
                        let out_slot = builder.create_sized_stack_slot(
                            StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3),
                        );
                        let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                        let itoa_ref = module.declare_func_in_func(externs.itoa, builder.func);
                        let call = builder.ins().call(itoa_ref, &[count, out_addr]);
                        let ptr = builder.inst_results(call)[0];
                        let len = builder.ins().stack_load(I64, out_slot, 0);
                        return (ptr, len);
                    }
                    "argv" if argv_values.len() >= 2 => {
                        // $(argv N) — get positional parameter by index
                        // Bounds-checked: if idx >= argc, return empty string + set $? to 1
                        let (idx_ptr, idx_len) = lower_value(
                            &argv_values[1], builder, module, vars, literals,
                            func_constants, externs, exit_status_var,
                        );
                        let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
                        let call_atoi = builder.ins().call(atoi_ref, &[idx_ptr, idx_len]);
                        let idx = builder.inst_results(call_atoi)[0];

                        // Load argc
                        let argc_gv = module.declare_data_in_func(externs.argc, builder.func);
                        let argc_base = builder.ins().global_value(I64, argc_gv);
                        let argc = builder.ins().load(I64, MemFlags::new(), argc_base, 0);

                        // Check idx < argc
                        let in_bounds = builder.ins().icmp(IntCC::SignedLessThan, idx, argc);

                        let ok_block = builder.create_block();
                        let fail_block = builder.create_block();
                        let merge_block = builder.create_block();

                        // Declare CL vars for the result
                        let res_ptr_var = builder.declare_var(I64);
                        let res_len_var = builder.declare_var(I64);
                        let res_status_var = builder.declare_var(I64);

                        builder.ins().brif(in_bounds, ok_block, &[], fail_block, &[]);

                        // OK: load argv[idx]
                        builder.switch_to_block(ok_block);
                        builder.seal_block(ok_block);
                        let argv_gv = module.declare_data_in_func(externs.argv, builder.func);
                        let argv_base = builder.ins().global_value(I64, argv_gv);
                        let argv_ptr = builder.ins().load(I64, MemFlags::new(), argv_base, 0);
                        let eight = builder.ins().iconst(I64, 8);
                        let offset = builder.ins().imul(idx, eight);
                        let elem_addr = builder.ins().iadd(argv_ptr, offset);
                        let str_ptr = builder.ins().load(I64, MemFlags::new(), elem_addr, 0);
                        let strlen_ref = module.declare_func_in_func(externs.strlen, builder.func);
                        let call_len = builder.ins().call(strlen_ref, &[str_ptr]);
                        let str_len = builder.inst_results(call_len)[0];
                        let zero_status = builder.ins().iconst(I64, 0);
                        builder.def_var(res_ptr_var, str_ptr);
                        builder.def_var(res_len_var, str_len);
                        builder.def_var(res_status_var, zero_status);
                        builder.ins().jump(merge_block, &[]);

                        // FAIL: return empty string, exit status 1
                        builder.switch_to_block(fail_block);
                        builder.seal_block(fail_block);
                        let empty_data = literals.get("").unwrap_or_else(|| {
                            panic!("ICE: empty string not interned");
                        });
                        let empty_gv = module.declare_data_in_func(*empty_data, builder.func);
                        let empty_ptr = builder.ins().global_value(I64, empty_gv);
                        let zero_len = builder.ins().iconst(I64, 0);
                        let one_status = builder.ins().iconst(I64, 1);
                        builder.def_var(res_ptr_var, empty_ptr);
                        builder.def_var(res_len_var, zero_len);
                        builder.def_var(res_status_var, one_status);
                        builder.ins().jump(merge_block, &[]);

                        // MERGE
                        builder.switch_to_block(merge_block);
                        builder.seal_block(merge_block);
                        let result_ptr = builder.use_var(res_ptr_var);
                        let result_len = builder.use_var(res_len_var);
                        let result_status = builder.use_var(res_status_var);
                        if let Some(esv) = exit_status_var {
                            builder.def_var(esv, result_status);
                        }
                        return (result_ptr, result_len);
                    }
                    _ => {}
                }
            }
            // Runtime command substitution using composable primitives:
            //   pipe → fork → (child: close read, dup2 write→stdout, close write, [redirects], exec)
            //                  (parent: close write, read loop, close read, wait)

            // 1. Build argv on stack (null-terminated pointer array)
            let argc = argv_values.len();
            let argv_size = ((argc + 1) * 8) as u32;
            let argv_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot, argv_size, 3,
            ));
            for (i, arg_val) in argv_values.iter().enumerate() {
                let (ptr, _) = lower_value(arg_val, builder, module, vars, literals, func_constants, externs, exit_status_var);
                builder.ins().stack_store(ptr, argv_slot, (i * 8) as i32);
            }
            let null = builder.ins().iconst(I64, 0);
            builder.ins().stack_store(null, argv_slot, (argc * 8) as i32);
            let argv_addr = builder.ins().stack_addr(I64, argv_slot, 0);

            // envp
            let envp_gv = module.declare_data_in_func(externs.envp, builder.func);
            let envp_addr = builder.ins().global_value(I64, envp_gv);
            let envp_val = builder.ins().load(I64, MemFlags::new(), envp_addr, 0);

            // 2. Create pipe: [read_fd, write_fd]
            let pipe_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot, 16, 3,
            ));
            let pipe_addr = builder.ins().stack_addr(I64, pipe_slot, 0);
            let pipe_ref = module.declare_func_in_func(externs.pipe, builder.func);
            builder.ins().call(pipe_ref, &[pipe_addr]);
            let read_fd = builder.ins().stack_load(I64, pipe_slot, 0);
            let write_fd = builder.ins().stack_load(I64, pipe_slot, 8);

            // 3. Fork
            let fork_ref = module.declare_func_in_func(externs.fork, builder.func);
            let fork_call = builder.ins().call(fork_ref, &[]);
            let pid = builder.inst_results(fork_call)[0];

            let zero = builder.ins().iconst(I64, 0);
            let is_child = builder.ins().icmp(IntCC::Equal, pid, zero);

            let child_block = builder.create_block();
            let parent_block = builder.create_block();

            builder.ins().brif(is_child, child_block, &[], parent_block, &[]);

            // 4. Child block: redirect stdout to pipe, exec
            builder.switch_to_block(child_block);
            builder.seal_block(child_block);

            let close_ref = module.declare_func_in_func(externs.close, builder.func);
            builder.ins().call(close_ref, &[read_fd]);

            let one = builder.ins().iconst(I64, 1); // stdout fd
            let dup2_ref = module.declare_func_in_func(externs.dup2, builder.func);
            builder.ins().call(dup2_ref, &[write_fd, one]);

            builder.ins().call(close_ref, &[write_fd]);

            // Apply redirects in child (e.g. 2>/dev/null)
            if !redirects.is_empty() {
                emit_redirects(redirects, builder, module, externs, literals, vars, exit_status_var);
            }

            let exec_ref = module.declare_func_in_func(externs.exec, builder.func);
            builder.ins().call(exec_ref, &[argv_addr, envp_val]);
            builder.ins().trap(TrapCode::user(1).unwrap()); // exec never returns

            // 5. Parent block: close write end, read loop, wait
            builder.switch_to_block(parent_block);
            builder.seal_block(parent_block);

            builder.ins().call(close_ref, &[write_fd]);

            // Allocate a 64KB capture buffer on the heap
            let buf_size = builder.ins().iconst(I64, 65536);
            let alloc_ref = module.declare_func_in_func(externs.alloc, builder.func);
            let alloc_call = builder.ins().call(alloc_ref, &[buf_size]);
            let buf_ptr = builder.inst_results(alloc_call)[0];

            // Read loop
            let total_var = builder.declare_var(I64);
            let zero_total = builder.ins().iconst(I64, 0);
            builder.def_var(total_var, zero_total);

            let read_hdr = builder.create_block();
            let read_body = builder.create_block();
            let read_done = builder.create_block();

            builder.ins().jump(read_hdr, &[]);

            // Read header: call sash_read
            builder.switch_to_block(read_hdr);
            let total_val = builder.use_var(total_var);
            let remaining = builder.ins().isub(buf_size, total_val);
            let cur_ptr = builder.ins().iadd(buf_ptr, total_val);
            let read_ref = module.declare_func_in_func(externs.read, builder.func);
            let read_call = builder.ins().call(read_ref, &[read_fd, cur_ptr, remaining]);
            let n_read = builder.inst_results(read_call)[0];

            let zero_check = builder.ins().iconst(I64, 0);
            let got_data = builder.ins().icmp(IntCC::SignedGreaterThan, n_read, zero_check);
            builder.ins().brif(got_data, read_body, &[], read_done, &[]);

            // Read body: accumulate
            builder.switch_to_block(read_body);
            builder.seal_block(read_body);
            let new_total = builder.ins().iadd(total_val, n_read);
            builder.def_var(total_var, new_total);
            builder.ins().jump(read_hdr, &[]);

            // Seal read_hdr after both predecessors (parent_block jump + read_body jump)
            builder.seal_block(read_hdr);

            // Read done: close read fd, wait, strip trailing newline
            builder.switch_to_block(read_done);
            builder.seal_block(read_done);

            builder.ins().call(close_ref, &[read_fd]);

            let wait_ref = module.declare_func_in_func(externs.wait, builder.func);
            builder.ins().call(wait_ref, &[pid]);

            // Strip trailing newline (shell convention for $(...))
            let final_total = builder.use_var(total_var);
            let one_val = builder.ins().iconst(I64, 1);
            let last_pos = builder.ins().isub(final_total, one_val);
            let last_byte_ptr = builder.ins().iadd(buf_ptr, last_pos);
            let last_byte = builder.ins().load(cranelift_codegen::ir::types::I8, MemFlags::new(), last_byte_ptr, 0);
            let last_byte_i64 = builder.ins().uextend(I64, last_byte);
            let newline = builder.ins().iconst(I64, 10);
            let is_newline = builder.ins().icmp(IntCC::Equal, last_byte_i64, newline);
            let stripped_len = builder.ins().isub(final_total, one_val);
            let result_len = builder.ins().select(is_newline, stripped_len, final_total);

            // Handle empty output (total=0): don't try to strip
            let is_empty = builder.ins().icmp(IntCC::Equal, final_total, zero_total);
            let result_len = builder.ins().select(is_empty, zero_total, result_len);

            (buf_ptr, result_len)
        }
        SsaValue::AssocArray(pairs) => {
            // Create a hash table via sash_table_new, populate with sash_table_set.
            // Store table_ptr in the variable's VarSlot (ptr slot = table, len slot = unused).
            let capacity = core::cmp::max(pairs.len() * 2, 8) as i64;
            let cap_val = builder.ins().iconst(I64, capacity);
            let new_ref = module.declare_func_in_func(externs.table_new, builder.func);
            let call = builder.ins().call(new_ref, &[cap_val]);
            let table_ptr = builder.inst_results(call)[0];

            let set_ref = module.declare_func_in_func(externs.table_set, builder.func);
            for (key, val_ssa) in pairs.iter() {
                let key_data = literals.get(key.as_str()).unwrap_or_else(|| {
                    panic!("ICE: assoc array key not interned: {}", key);
                });
                let key_gv = module.declare_data_in_func(*key_data, builder.func);
                let key_ptr = builder.ins().global_value(I64, key_gv);
                let key_len = builder.ins().iconst(I64, key.len() as i64);
                let (val_ptr, val_len) = lower_value(val_ssa, builder, module, vars, literals, func_constants, externs, exit_status_var);
                builder.ins().call(set_ref, &[table_ptr, key_ptr, key_len, val_ptr, val_len]);
            }
            let zero = builder.ins().iconst(I64, 0);
            (table_ptr, zero) // ptr = table handle, len = unused
        }
        SsaValue::Subscript { array, index, is_integer_key } => {
            // Look up key in the hash table via sash_table_get
            let array_key = format!("{}_0", array);
            let sv = vars.get(&array_key).unwrap_or_else(|| {
                vars.keys().find(|k| k.starts_with(&format!("{}_", array)))
                    .and_then(|k| vars.get(k))
                    .unwrap_or_else(|| panic!("ICE: undefined array: {}", array))
            });
            let table_ptr = builder.use_var(sv.str_ptr());

            let (key_ptr, key_len) = if *is_integer_key {
                // Integer key: convert index to i64, store on stack, use raw 8 bytes as key
                let idx_int = lower_value_as_int(index, builder, module, vars, literals, func_constants, externs);
                let idx_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot, 8, 3,
                ));
                builder.ins().stack_store(idx_int, idx_slot, 0);
                let kp = builder.ins().stack_addr(I64, idx_slot, 0);
                let kl = builder.ins().iconst(I64, 8);
                (kp, kl)
            } else {
                // String key: use string bytes directly
                lower_value(index, builder, module, vars, literals, func_constants, externs, exit_status_var)
            };

            let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot, 8, 3,
            ));
            let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
            let get_ref = module.declare_func_in_func(externs.table_get, builder.func);
            let call = builder.ins().call(get_ref, &[table_ptr, key_ptr, key_len, out_addr]);
            let val_ptr = builder.inst_results(call)[0];
            let val_len = builder.ins().stack_load(I64, out_slot, 0);
            (val_ptr, val_len)
        }
        SsaValue::ArgvIndex(idx_ref) => {
            // Load argv[atoi(idx) + 1]: dynamic positional param access
            let sv = vars.get(&idx_ref.display()).unwrap_or_else(|| panic!("ICE: undefined index var: {}", idx_ref.display()));
            let idx_ptr = builder.use_var(sv.str_ptr());
            let idx_len = builder.use_var(sv.str_len());
            let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
            let call = builder.ins().call(atoi_ref, &[idx_ptr, idx_len]);
            let idx_int = builder.inst_results(call)[0];
            // offset = (idx + 1) * 8 — positional params are 1-indexed
            let one = builder.ins().iconst(I64, 1);
            let idx_plus_1 = builder.ins().iadd(idx_int, one);
            let eight = builder.ins().iconst(I64, 8);
            let offset = builder.ins().imul(idx_plus_1, eight);
            // Load from sash_argv
            let argv_gv = module.declare_data_in_func(externs.argv, builder.func);
            let argv_ptr = builder.ins().global_value(I64, argv_gv);
            let argv_val = builder.ins().load(I64, MemFlags::trusted(), argv_ptr, 0);
            let arg_addr = builder.ins().iadd(argv_val, offset);
            let arg_ptr = builder.ins().load(I64, MemFlags::trusted(), arg_addr, 0);
            let arg_len = emit_strlen(builder, module, externs.strlen, arg_ptr);
            (arg_ptr, arg_len)
        }
        SsaValue::TableValue { array, index } => {
            // Look up the table pointer for the array
            let array_key = format!("{}_0", array);
            let sv = vars.get(&array_key).unwrap_or_else(|| {
                vars.keys().find(|k| k.starts_with(&format!("{}_", array)))
                    .and_then(|k| vars.get(k))
                    .unwrap_or_else(|| panic!("ICE: undefined array: {}", array))
            });
            let table_ptr = builder.use_var(sv.str_ptr());

            // Convert index var to i64
            let idx_sv = vars.get(&index.display()).unwrap_or_else(|| panic!("ICE: undefined index var: {}", index.display()));
            let idx_ptr = builder.use_var(idx_sv.str_ptr());
            let idx_len = builder.use_var(idx_sv.str_len());
            let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
            let call = builder.ins().call(atoi_ref, &[idx_ptr, idx_len]);
            let idx_int = builder.inst_results(call)[0];

            // sash_table_value(table, index, out_len_ptr) -> val_ptr
            let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot, 8, 3,
            ));
            let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
            let tv_ref = module.declare_func_in_func(externs.table_value, builder.func);
            let call2 = builder.ins().call(tv_ref, &[table_ptr, idx_int, out_addr]);
            let val_ptr = builder.inst_results(call2)[0];
            let val_len = builder.ins().stack_load(I64, out_slot, 0);
            (val_ptr, val_len)
        }
        SsaValue::TableKey { array, index } => {
            let array_key = format!("{}_0", array);
            let sv = vars.get(&array_key).unwrap_or_else(|| {
                vars.keys().find(|k| k.starts_with(&format!("{}_", array)))
                    .and_then(|k| vars.get(k))
                    .unwrap_or_else(|| panic!("ICE: undefined array: {}", array))
            });
            let table_ptr = builder.use_var(sv.str_ptr());

            let idx_sv = vars.get(&index.display()).unwrap_or_else(|| panic!("ICE: undefined index var: {}", index.display()));
            let idx_ptr = builder.use_var(idx_sv.str_ptr());
            let idx_len = builder.use_var(idx_sv.str_len());
            let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
            let call = builder.ins().call(atoi_ref, &[idx_ptr, idx_len]);
            let idx_int = builder.inst_results(call)[0];

            let out_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot, 8, 3,
            ));
            let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
            let tk_ref = module.declare_func_in_func(externs.table_key, builder.func);
            let call2 = builder.ins().call(tk_ref, &[table_ptr, idx_int, out_addr]);
            let key_ptr = builder.inst_results(call2)[0];
            let key_len = builder.ins().stack_load(I64, out_slot, 0);
            (key_ptr, key_len)
        }
        SsaValue::Ternary { cond, true_val, false_val } => {
            let cond_int = lower_value_as_int(cond, builder, module, vars, literals, func_constants, externs);
            let zero = builder.ins().iconst(I64, 0);
            let is_true = builder.ins().icmp(IntCC::NotEqual, cond_int, zero);
            let (t_ptr, t_len) = lower_value(true_val, builder, module, vars, literals, func_constants, externs, exit_status_var);
            let (f_ptr, f_len) = lower_value(false_val, builder, module, vars, literals, func_constants, externs, exit_status_var);
            let r_ptr = builder.ins().select(is_true, t_ptr, f_ptr);
            let r_len = builder.ins().select(is_true, t_len, f_len);
            (r_ptr, r_len)
        }
        SsaValue::IndexedArray(elems) => {
            // Create a hash table with integer keys 0, 1, 2, ...
            let capacity = core::cmp::max(elems.len() * 2, 8) as i64;
            let cap_val = builder.ins().iconst(I64, capacity);
            let new_ref = module.declare_func_in_func(externs.table_new, builder.func);
            let call = builder.ins().call(new_ref, &[cap_val]);
            let table_ptr = builder.inst_results(call)[0];

            let set_ref = module.declare_func_in_func(externs.table_set, builder.func);
            for (i, val_ssa) in elems.iter().enumerate() {
                // Key is the raw i64 bytes of the index
                let idx_slot = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot, 8, 3,
                ));
                let idx_val = builder.ins().iconst(I64, i as i64);
                builder.ins().stack_store(idx_val, idx_slot, 0);
                let key_ptr = builder.ins().stack_addr(I64, idx_slot, 0);
                let key_len = builder.ins().iconst(I64, 8);
                let (val_ptr, val_len) = lower_value(val_ssa, builder, module, vars, literals, func_constants, externs, exit_status_var);
                builder.ins().call(set_ref, &[table_ptr, key_ptr, key_len, val_ptr, val_len]);
            }
            let zero = builder.ins().iconst(I64, 0);
            (table_ptr, zero)
        }
    }
}

/// Emit Cranelift instructions for an ArithOp on two i64 values.
fn emit_arith_op(op: ArithOp, lhs: cranelift_codegen::ir::Value, rhs: cranelift_codegen::ir::Value, builder: &mut FunctionBuilder, module: &mut ObjectModule, externs: &ExternFuncs, exit_status_var: Option<Variable>) -> cranelift_codegen::ir::Value {
    use cranelift_codegen::ir::condcodes::IntCC;
    match op {
        ArithOp::Add => builder.ins().iadd(lhs, rhs),
        ArithOp::Sub => builder.ins().isub(lhs, rhs),
        ArithOp::Mul => builder.ins().imul(lhs, rhs),
        ArithOp::Div | ArithOp::Mod => {
            // Division by zero: bash prints error, sets $?=1, result is 0
            let zero = builder.ins().iconst(I64, 0);
            let is_zero = builder.ins().icmp(IntCC::Equal, rhs, zero);
            let safe_block = builder.create_block();
            let zero_block = builder.create_block();
            let merge_block = builder.create_block();
            builder.append_block_param(merge_block, I64);
            builder.ins().brif(is_zero, zero_block, &[], safe_block, &[]);

            // Zero block: write error to stderr, set $?=1, result=0
            builder.switch_to_block(zero_block);
            builder.seal_block(zero_block);
            // Write "division by 0" to stderr
            let err_msg = b"bash: division by 0 (error token is \"0\")\n";
            let err_data = module.declare_anonymous_data(true, false).unwrap();
            let mut desc = DataDescription::new();
            desc.define(err_msg.to_vec().into_boxed_slice());
            module.define_data(err_data, &desc).unwrap();
            let err_gv = module.declare_data_in_func(err_data, builder.func);
            let err_ptr = builder.ins().global_value(I64, err_gv);
            let err_len = builder.ins().iconst(I64, err_msg.len() as i64);
            let stderr_fd = builder.ins().iconst(I64, 2);
            let write_ref = module.declare_func_in_func(externs.write, builder.func);
            builder.ins().call(write_ref, &[stderr_fd, err_ptr, err_len]);
            if let Some(es_var) = exit_status_var {
                let one = builder.ins().iconst(I64, 1);
                builder.def_var(es_var, one);
            }
            builder.ins().jump(merge_block, &[BlockArg::Value(zero)]);

            // Safe block: do the division
            builder.switch_to_block(safe_block);
            builder.seal_block(safe_block);
            let result = if matches!(op, ArithOp::Div) {
                builder.ins().sdiv(lhs, rhs)
            } else {
                builder.ins().srem(lhs, rhs)
            };
            builder.ins().jump(merge_block, &[BlockArg::Value(result)]);

            builder.switch_to_block(merge_block);
            builder.seal_block(merge_block);
            builder.block_params(merge_block)[0]
        }
        ArithOp::Eq => { let c = builder.ins().icmp(IntCC::Equal, lhs, rhs); builder.ins().uextend(I64, c) }
        ArithOp::Ne => { let c = builder.ins().icmp(IntCC::NotEqual, lhs, rhs); builder.ins().uextend(I64, c) }
        ArithOp::Lt => { let c = builder.ins().icmp(IntCC::SignedLessThan, lhs, rhs); builder.ins().uextend(I64, c) }
        ArithOp::Le => { let c = builder.ins().icmp(IntCC::SignedLessThanOrEqual, lhs, rhs); builder.ins().uextend(I64, c) }
        ArithOp::Gt => { let c = builder.ins().icmp(IntCC::SignedGreaterThan, lhs, rhs); builder.ins().uextend(I64, c) }
        ArithOp::Ge => { let c = builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, lhs, rhs); builder.ins().uextend(I64, c) }
        ArithOp::And => { let l = builder.ins().icmp_imm(IntCC::NotEqual, lhs, 0); let r = builder.ins().icmp_imm(IntCC::NotEqual, rhs, 0); let b = builder.ins().band(l, r); builder.ins().uextend(I64, b) }
        ArithOp::Or => { let l = builder.ins().icmp_imm(IntCC::NotEqual, lhs, 0); let r = builder.ins().icmp_imm(IntCC::NotEqual, rhs, 0); let b = builder.ins().bor(l, r); builder.ins().uextend(I64, b) }
        ArithOp::Not => panic!("ICE: unary ! in binary arithmetic context"),
        ArithOp::Shl => builder.ins().ishl(lhs, rhs),
        ArithOp::Shr => builder.ins().sshr(lhs, rhs),
        ArithOp::BitAnd => builder.ins().band(lhs, rhs),
        ArithOp::BitOr => builder.ins().bor(lhs, rhs),
        ArithOp::BitXor => builder.ins().bxor(lhs, rhs),
        ArithOp::Pow => {
            let pow_ref = module.declare_func_in_func(externs.pow, builder.func);
            let call = builder.ins().call(pow_ref, &[lhs, rhs]);
            builder.inst_results(call)[0]
        }
    }
}

/// Lower an SsaValue to an i64. For int vars, loads directly.
/// For string values, calls atoi. For ArithExpr, computes inline.
fn lower_value_as_int(
    value: &SsaValue,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    func_constants: &HashMap<String, String>,
    externs: &ExternFuncs,
) -> cranelift_codegen::ir::Value {
    match value {
        SsaValue::Literal(s) | SsaValue::Expr(s) => {
            let n = s.parse::<i64>().unwrap_or(0);
            builder.ins().iconst(I64, n)
        }
        SsaValue::Var(vref) => {
            let key = vref.display();
            let sv = match vars.get(&key) {
                Some(v) => v,
                None => {
                    // Undefined variable → 0 in int context
                    return builder.ins().iconst(I64, 0);
                }
            };
            if sv.is_int() {
                builder.use_var(*sv.as_int())
            } else {
                // String var in int context: atoi
                let ptr = builder.use_var(sv.str_ptr());
                let len = builder.use_var(sv.str_len());
                let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
                let call = builder.ins().call(atoi_ref, &[ptr, len]);
                builder.inst_results(call)[0]
            }
        }
        SsaValue::ArithExpr { var, op, operand } => {
            let var_val = lower_value_as_int(&SsaValue::Var(var.clone()), builder, module, vars, literals, func_constants, externs);
            let rhs = builder.ins().iconst(I64, *operand);
            emit_arith_op(*op, var_val, rhs, builder, module, externs, None)
        }
        SsaValue::ArithBinOp { left, op, right } => {
            let l = lower_value_as_int(left, builder, module, vars, literals, func_constants, externs);
            let r = lower_value_as_int(right, builder, module, vars, literals, func_constants, externs);
            emit_arith_op(*op, l, r, builder, module, externs, None)
        }
        SsaValue::Ternary { cond, true_val, false_val } => {
            let cond_int = lower_value_as_int(cond, builder, module, vars, literals, func_constants, externs);
            let zero = builder.ins().iconst(I64, 0);
            let is_true = builder.ins().icmp(IntCC::NotEqual, cond_int, zero);
            let t = lower_value_as_int(true_val, builder, module, vars, literals, func_constants, externs);
            let f = lower_value_as_int(false_val, builder, module, vars, literals, func_constants, externs);
            builder.ins().select(is_true, t, f)
        }
        _ => {
            // Fallback: lower as string, atoi
            let (ptr, len) = lower_value(value, builder, module, vars, literals, func_constants, externs, None);
            let atoi_ref = module.declare_func_in_func(externs.atoi, builder.func);
            let call = builder.ins().call(atoi_ref, &[ptr, len]);
            builder.inst_results(call)[0]
        }
    }
}

/// Scan an SsaValue for variable references that might be positional or env vars
fn scan_value_for_vars(val: &SsaValue, refs: &mut std::collections::HashSet<String>) {
    match val {
        SsaValue::Var(vref) if vref.version == 0 => {
            let name = &vref.name;
            if name.parse::<u64>().is_ok() || name == "#" || name == "@" || name == "0" {
                refs.insert(name.clone());
            } else if name.chars().all(|c| c.is_ascii_uppercase() || c == '_') {
                refs.insert(name.clone());
            }
        }
        SsaValue::Concat(parts) => {
            for p in parts { scan_value_for_vars(p, refs); }
        }
        SsaValue::ParamExpansion { var, .. } if var.version == 0 => {
            let name = &var.name;
            if name.parse::<u64>().is_ok() || name == "#" || name == "@" || name == "0" {
                refs.insert(name.clone());
            } else if name.chars().all(|c| c.is_ascii_uppercase() || c == '_') {
                refs.insert(name.clone());
            }
        }
        SsaValue::CmdSubst { argv, .. } => {
            for a in argv { scan_value_for_vars(a, refs); }
        }
        SsaValue::ArithExpr { var, .. } if var.version == 0 => {
            let name = &var.name;
            if name.parse::<u64>().is_ok() || name == "#" || name == "@" || name == "0" {
                refs.insert(name.clone());
            }
        }
        SsaValue::AssocArray(pairs) => {
            for (_, v) in pairs { scan_value_for_vars(v, refs); }
        }
        SsaValue::Subscript { index, .. } => {
            scan_value_for_vars(index, refs);
        }
        SsaValue::IndexedArray(elems) => {
            for v in elems { scan_value_for_vars(v, refs); }
        }
        SsaValue::ArgvIndex(_) => {}
        SsaValue::TableValue { .. } => {}
        SsaValue::TableKey { .. } => {}
        _ => {}
    }
}

/// Emit Cranelift IR to compute strlen by calling sash-rt's strlen
fn emit_strlen(
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    strlen_func: cranelift_module::FuncId,
    ptr: cranelift_codegen::ir::Value,
) -> cranelift_codegen::ir::Value {
    let strlen_ref = module.declare_func_in_func(strlen_func, builder.func);
    let call = builder.ins().call(strlen_ref, &[ptr]);
    builder.inst_results(call)[0]
}

fn scan_test_for_vars(test: &TestExpr, refs: &mut std::collections::HashSet<String>) {
    fn scan_expr(expr: &crate::ir::Expr, refs: &mut std::collections::HashSet<String>) {
        
        match &expr.kind {
            crate::ir::ExprKind::SimpleExpansion(name) => {
                if name.parse::<u64>().is_ok() || name == "#" || name == "@" || name == "0" {
                    refs.insert(name.clone());
                } else if name.chars().all(|c| c.is_ascii_uppercase() || c == '_') {
                    refs.insert(name.clone());
                }
            }
            crate::ir::ExprKind::DoubleQuoted(parts) => { for p in parts { scan_expr(p, refs); } }
            _ => {}
        }
    }
    match test {
        TestExpr::Binary { left, right, .. } => { scan_expr(left, refs); scan_expr(right, refs); }
        TestExpr::Unary { operand, .. } => { scan_expr(operand, refs); }
        TestExpr::Not(inner) | TestExpr::Paren(inner) => { scan_test_for_vars(inner, refs); }
        TestExpr::And(a, b) | TestExpr::Or(a, b) => { scan_test_for_vars(a, refs); scan_test_for_vars(b, refs); }
    }
}

/// Saved fd state for restoring after redirections
struct SavedFd {
    original_fd: i64,              // the fd that was redirected (e.g. 1, 2)
    saved_copy: cranelift_codegen::ir::Value,  // dup'd copy to restore from
}

/// Emit fd redirections before a command. Returns saved state for restore.
fn emit_redirects(
    redirects: &[crate::ir::Redirect],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    literals: &HashMap<String, cranelift_module::DataId>,
    vars: &HashMap<String, VarSlot>,
#[allow(unused_variables)]
        _exit_status_var: Option<Variable>,
) -> Vec<SavedFd> {
    use crate::ir::RedirectOp;

    let mut saved = Vec::new();
    for redir in redirects {
        let fd = redir.fd.unwrap_or(match redir.op {
            RedirectOp::Write | RedirectOp::Append | RedirectOp::DupOutput => 1,
            RedirectOp::Read | RedirectOp::DupInput | RedirectOp::HereDoc | RedirectOp::HereString => 0,
            _ => 1,
        }) as i64;

        // Save the original fd via dup
        let fd_val = builder.ins().iconst(cranelift_codegen::ir::types::I64, fd);
        let dup_ref = module.declare_func_in_func(externs.dup, builder.func);
        let dup_call = builder.ins().call(dup_ref, &[fd_val]);
        let saved_fd = builder.inst_results(dup_call)[0];
        saved.push(SavedFd { original_fd: fd, saved_copy: saved_fd });

        match redir.op {
            RedirectOp::Write | RedirectOp::Append => {
                let flags = if matches!(redir.op, RedirectOp::Write) { 1_i64 } else { 2_i64 };
                let flags_val = builder.ins().iconst(cranelift_codegen::ir::types::I64, flags);
                // Try literal path first (null-terminated, faster)
                if let crate::ir::ExprKind::Literal(s) = &redir.target.kind {
                    if let Some(&data_id) = literals.get(s.as_str()) {
                        let path_gv = module.declare_data_in_func(data_id, builder.func);
                        let path_ptr = builder.ins().global_value(cranelift_codegen::ir::types::I64, path_gv);
                        let redirect_ref = module.declare_func_in_func(externs.redirect, builder.func);
                        builder.ins().call(redirect_ref, &[fd_val, path_ptr, flags_val]);
                    }
                } else {
                    // Dynamic path (e.g. ~/foo → $HOME/foo): emit CL directly
                    let (ptr, len) = lower_ir_redirect_expr(&redir.target, builder, module, vars, literals, externs);
                    let redirect_s_ref = module.declare_func_in_func(externs.redirect_s, builder.func);
                    builder.ins().call(redirect_s_ref, &[fd_val, ptr, len, flags_val]);
                }
            }
            RedirectOp::DupOutput => {
                // >&N — dup2(N, fd)
                let target_fd: i64 = match &redir.target.kind {
                    crate::ir::ExprKind::Literal(s) => s.parse().unwrap_or(2),
                    _ => 2,
                };
                let target_val = builder.ins().iconst(cranelift_codegen::ir::types::I64, target_fd);
                let dup2_ref = module.declare_func_in_func(externs.dup2, builder.func);
                builder.ins().call(dup2_ref, &[target_val, fd_val]);
            }
            RedirectOp::Read => {
                // < file — open for reading, dup2 to fd (default 0)
                let flags_val = builder.ins().iconst(cranelift_codegen::ir::types::I64, 0_i64); // read mode
                if let crate::ir::ExprKind::Literal(s) = &redir.target.kind {
                    if let Some(&data_id) = literals.get(s.as_str()) {
                        let path_gv = module.declare_data_in_func(data_id, builder.func);
                        let path_ptr = builder.ins().global_value(cranelift_codegen::ir::types::I64, path_gv);
                        let redirect_ref = module.declare_func_in_func(externs.redirect, builder.func);
                        builder.ins().call(redirect_ref, &[fd_val, path_ptr, flags_val]);
                    }
                } else {
                    let (ptr, len) = lower_ir_redirect_expr(&redir.target, builder, module, vars, literals, externs);
                    let redirect_s_ref = module.declare_func_in_func(externs.redirect_s, builder.func);
                    builder.ins().call(redirect_s_ref, &[fd_val, ptr, len, flags_val]);
                }
            }
            RedirectOp::HereDoc | RedirectOp::HereString => {
                // << EOF / <<< "string" — pipe body text to stdin
                let (body_ptr, body_len) = if let crate::ir::ExprKind::Literal(s) = &redir.target.kind {
                    if let Some(&data_id) = literals.get(s.as_str()) {
                        let gv = module.declare_data_in_func(data_id, builder.func);
                        let ptr = builder.ins().global_value(cranelift_codegen::ir::types::I64, gv);
                        let len = builder.ins().iconst(cranelift_codegen::ir::types::I64, s.len() as i64);
                        (ptr, len)
                    } else {
                        continue;
                    }
                } else {
                    lower_ir_redirect_expr(&redir.target, builder, module, vars, literals, externs)
                };
                let heredoc_ref = module.declare_func_in_func(externs.heredoc_redirect, builder.func);
                builder.ins().call(heredoc_ref, &[fd_val, body_ptr, body_len]);
            }
            _ => {} // TODO: other redirect types
        }
    }
    saved
}

/// Restore saved fds after a command completes
fn emit_restore(
    saved: Vec<SavedFd>,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
) {
    for s in saved.iter().rev() {
        let fd_val = builder.ins().iconst(cranelift_codegen::ir::types::I64, s.original_fd);
        let dup2_ref = module.declare_func_in_func(externs.dup2, builder.func);
        builder.ins().call(dup2_ref, &[s.saved_copy, fd_val]);
        let close_ref = module.declare_func_in_func(externs.close, builder.func);
        builder.ins().call(close_ref, &[s.saved_copy]);
    }
}

/// Convert an IR Expr to an SsaValue for use in lower_value.
/// Used for redirect targets that contain runtime expressions (tilde expansion, etc).
fn ir_expr_to_ssa_value(expr: &crate::ir::Expr) -> crate::ssa::SsaValue {
    
    use crate::ssa::{SsaValue, VarRef};
    match &expr.kind {
        crate::ir::ExprKind::Literal(s) | crate::ir::ExprKind::SingleQuoted(s) => SsaValue::Literal(s.clone()),
        crate::ir::ExprKind::SimpleExpansion(v) => SsaValue::Var(VarRef::new(v, 0)),
        crate::ir::ExprKind::Concat(parts) | crate::ir::ExprKind::DoubleQuoted(parts) => {
            let vals: Vec<SsaValue> = parts.iter().map(|p| ir_expr_to_ssa_value(p)).collect();
            SsaValue::Concat(vals)
        }
        _ => SsaValue::Literal(String::new()),
    }
}

/// Scan an IR Expr (redirect target) for variable references.
fn scan_redirect_for_vars(expr: &crate::ir::Expr, refs: &mut std::collections::HashSet<String>) {
    
    match &expr.kind {
        crate::ir::ExprKind::SimpleExpansion(v) => { refs.insert(v.clone()); }
        crate::ir::ExprKind::Concat(parts) | crate::ir::ExprKind::DoubleQuoted(parts) => {
            for p in parts { scan_redirect_for_vars(p, refs); }
        }
        _ => {}
    }
}

/// Lower an IR Expr (from a redirect target) directly to Cranelift values,
/// bypassing SSA. Used for redirect targets that contain tilde-expanded paths.
fn lower_ir_redirect_expr(
    expr: &crate::ir::Expr,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    
    use cranelift_codegen::ir::types::I64;
    match &expr.kind {
        crate::ir::ExprKind::Literal(s) | crate::ir::ExprKind::SingleQuoted(s) => {
            if let Some(&data_id) = literals.get(s.as_str()) {
                let gv = module.declare_data_in_func(data_id, builder.func);
                let ptr = builder.ins().global_value(I64, gv);
                let len = builder.ins().iconst(I64, s.len() as i64);
                (ptr, len)
            } else {
                let z = builder.ins().iconst(I64, 0);
                (z, z)
            }
        }
        crate::ir::ExprKind::SimpleExpansion(var) => {
            // Find the var in vars — match on base name, pick highest version
            let mut best: Option<(&str, u32, &VarSlot)> = None;
            for (k, sv) in vars {
                let base = if let Some(idx) = k.rfind('_') {
                    let suffix = &k[idx+1..];
                    if suffix.chars().all(|c| c.is_ascii_digit()) { &k[..idx] } else { k.as_str() }
                } else { k.as_str() };
                if base == var {
                    let ver = k.rfind('_').and_then(|i| k[i+1..].parse::<u32>().ok()).unwrap_or(0);
                    if best.is_none() || ver > best.unwrap().1 {
                        best = Some((base, ver, sv));
                    }
                }
            }
            if let Some((_, _, sv)) = best {
                return (builder.use_var(sv.str_ptr()), builder.use_var(sv.str_len()));
            }
            let z = builder.ins().iconst(I64, 0);
            (z, z)
        }
        crate::ir::ExprKind::Concat(parts) | crate::ir::ExprKind::DoubleQuoted(parts) => {
            assert!(!parts.is_empty());
            let (mut acc_ptr, mut acc_len) = lower_ir_redirect_expr(&parts[0], builder, module, vars, literals, externs);
            for part in &parts[1..] {
                let (p_ptr, p_len) = lower_ir_redirect_expr(part, builder, module, vars, literals, externs);
                let out_slot = builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
                    cranelift_codegen::ir::StackSlotKind::ExplicitSlot, 8, 3,
                ));
                let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
                let concat_ref = module.declare_func_in_func(externs.concat, builder.func);
                let call = builder.ins().call(concat_ref, &[acc_ptr, acc_len, p_ptr, p_len, out_addr]);
                acc_ptr = builder.inst_results(call)[0];
                acc_len = builder.ins().stack_load(I64, out_slot, 0);
            }
            (acc_ptr, acc_len)
        }
        _ => {
            let z = builder.ins().iconst(I64, 0);
            (z, z)
        }
    }
}
