//! Codegen setup: runtime function declarations and literal interning.
//!
//! TRUST: declare_runtime_functions() creates the complete interface between
//! compiled code and sash-rt. Every function listed here — and ONLY these
//! functions — can be called by the compiled binary.

use cranelift_codegen::ir::types::I64;
use cranelift_codegen::ir::{AbiParam, Signature};
use cranelift_codegen::isa::CallConv;
use cranelift_module::{DataDescription, Linkage, Module};
use cranelift_object::ObjectModule;
use std::collections::HashMap;

use super::types::ExternFuncs;
use crate::ssa::*;

/// Declare all runtime function signatures and return the ExternFuncs struct.
///
/// TRUST: This is the complete list of runtime capabilities available to
/// compiled binaries. A reviewer should verify each entry matches sash-rt.
pub(super) fn declare_runtime_functions(module: &mut ObjectModule) -> ExternFuncs {
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

    // sash_emsg(seg1_ptr, seg1_len, seg2_ptr, seg2_len) -> () — DD#55
    // abort diagnostic to stderr (no return value).
    let mut emsg_sig = Signature::new(CallConv::SystemV);
    for _ in 0..4 {
        emsg_sig.params.push(AbiParam::new(I64));
    }
    let emsg_func = module
        .declare_function("sash_emsg", Linkage::Import, &emsg_sig)
        .expect("declare sash_emsg");

    // sash_emsg_status(name_ptr, name_len, status) -> () — DD#55
    // status-based abort diagnostic (`command failed (exit N)`, no errno).
    let mut emsg_status_sig = Signature::new(CallConv::SystemV);
    for _ in 0..3 {
        emsg_status_sig.params.push(AbiParam::new(I64));
    }
    let emsg_status_func = module
        .declare_function("sash_emsg_status", Linkage::Import, &emsg_status_sig)
        .expect("declare sash_emsg_status");

    // sash_bless_fail(name_ptr, name_len, val_ptr, val_len) -> ! — bless
    // gate failure diagnostic (`<argv0>: bless: <name> = '<val>' failed …`).
    let mut bless_fail_sig = Signature::new(CallConv::SystemV);
    for _ in 0..4 {
        bless_fail_sig.params.push(AbiParam::new(I64));
    }
    let bless_fail_func = module
        .declare_function("sash_bless_fail", Linkage::Import, &bless_fail_sig)
        .expect("declare sash_bless_fail");

    // sash_arith_overflow() -> ! — DD#25 overflow trap (no params/returns).
    let arith_overflow_sig = Signature::new(CallConv::SystemV);
    let arith_overflow_func = module
        .declare_function("sash_arith_overflow", Linkage::Import, &arith_overflow_sig)
        .expect("declare sash_arith_overflow");

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

    // sash_argv_get_or(idx, d_ptr, d_len, &out_len) -> ptr  (DD#60 addendum:
    // returns the default (d_ptr,d_len) when idx is out of range)
    let mut argv_get_or_sig = Signature::new(CallConv::SystemV);
    argv_get_or_sig.params.push(AbiParam::new(I64)); // idx
    argv_get_or_sig.params.push(AbiParam::new(I64)); // default ptr
    argv_get_or_sig.params.push(AbiParam::new(I64)); // default len
    argv_get_or_sig.params.push(AbiParam::new(I64)); // &out_len
    argv_get_or_sig.returns.push(AbiParam::new(I64)); // str ptr
    let argv_get_or_func = module
        .declare_function("sash_argv_get_or", Linkage::Import, &argv_get_or_sig)
        .expect("declare sash_argv_get_or");

    // sash_argv_snapshot(start) -> *mut SashTable  (DD#60 array a=$(argv N))
    let mut argv_snapshot_sig = Signature::new(CallConv::SystemV);
    argv_snapshot_sig.params.push(AbiParam::new(I64)); // start
    argv_snapshot_sig.returns.push(AbiParam::new(I64)); // table ptr
    let argv_snapshot_func = module
        .declare_function("sash_argv_snapshot", Linkage::Import, &argv_snapshot_sig)
        .expect("declare sash_argv_snapshot");

    // sash_splat_disposition(tbl, word_ptr, word_len, has_word, abort) -> tbl
    // (DD#69 external `@{a:-WORD}` / `@{a:?WORD}` empty-disposition)
    let mut splat_disposition_sig = Signature::new(CallConv::SystemV);
    for _ in 0..5 {
        splat_disposition_sig.params.push(AbiParam::new(I64));
    }
    splat_disposition_sig.returns.push(AbiParam::new(I64));
    let splat_disposition_func = module
        .declare_function(
            "sash_splat_disposition",
            Linkage::Import,
            &splat_disposition_sig,
        )
        .expect("declare sash_splat_disposition");

    // sash_require_value(s_len, msg_ptr, msg_len) — DD#77 `${var:?MESSAGE}`
    // abort-if-unset/empty (no return; caller keeps the value).
    let mut require_value_sig = Signature::new(CallConv::SystemV);
    require_value_sig.params.push(AbiParam::new(I64)); // s_len
    require_value_sig.params.push(AbiParam::new(I64)); // msg_ptr
    require_value_sig.params.push(AbiParam::new(I64)); // msg_len
    let require_value_func = module
        .declare_function("sash_require_value", Linkage::Import, &require_value_sig)
        .expect("declare sash_require_value");

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

    // sash_is_int(s_ptr, s_len) -> i64 (1 = strict integer, 0 = not) — DD#77
    let mut is_int_sig = Signature::new(CallConv::SystemV);
    is_int_sig.params.push(AbiParam::new(I64)); // s_ptr
    is_int_sig.params.push(AbiParam::new(I64)); // s_len
    is_int_sig.returns.push(AbiParam::new(I64)); // 0/1
    let is_int_func = module
        .declare_function("sash_is_int", Linkage::Import, &is_int_sig)
        .expect("declare sash_is_int");

    // sash_atoi_or(s_ptr, s_len, default) -> i64 — DD#77 arith `${s:-N}`
    let mut atoi_or_sig = Signature::new(CallConv::SystemV);
    atoi_or_sig.params.push(AbiParam::new(I64)); // s_ptr
    atoi_or_sig.params.push(AbiParam::new(I64)); // s_len
    atoi_or_sig.params.push(AbiParam::new(I64)); // default
    atoi_or_sig.returns.push(AbiParam::new(I64));
    let atoi_or_func = module
        .declare_function("sash_atoi_or", Linkage::Import, &atoi_or_sig)
        .expect("declare sash_atoi_or");

    // sash_atoi_or_abort(s_ptr, s_len, msg_ptr, msg_len) -> i64 — DD#77 `${s:?M}`
    let mut atoi_or_abort_sig = Signature::new(CallConv::SystemV);
    atoi_or_abort_sig.params.push(AbiParam::new(I64)); // s_ptr
    atoi_or_abort_sig.params.push(AbiParam::new(I64)); // s_len
    atoi_or_abort_sig.params.push(AbiParam::new(I64)); // msg_ptr
    atoi_or_abort_sig.params.push(AbiParam::new(I64)); // msg_len
    atoi_or_abort_sig.returns.push(AbiParam::new(I64));
    let atoi_or_abort_func = module
        .declare_function("sash_atoi_or_abort", Linkage::Import, &atoi_or_abort_sig)
        .expect("declare sash_atoi_or_abort");

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

    // sash_regex_match_str(str_ptr, str_len, pat_ptr, pat_len) -> (ptr, len)
    // DD#63: whole-match capture for `$(match S P)`.
    let mut regex_match_str_sig = module.make_signature();
    for _ in 0..4 {
        regex_match_str_sig.params.push(AbiParam::new(I64));
    }
    regex_match_str_sig.returns.push(AbiParam::new(I64)); // ptr
    regex_match_str_sig.returns.push(AbiParam::new(I64)); // len
    let regex_match_str_func = module
        .declare_function(
            "sash_regex_match_str",
            Linkage::Import,
            &regex_match_str_sig,
        )
        .expect("declare sash_regex_match_str");

    // sash_regex_match_into(str_ptr, str_len, pat_ptr, pat_len, table) -> i64
    // DD#63: out-array capture form `match S P ARR`.
    let mut regex_match_into_sig = module.make_signature();
    for _ in 0..5 {
        regex_match_into_sig.params.push(AbiParam::new(I64));
    }
    regex_match_into_sig.returns.push(AbiParam::new(I64));
    let regex_match_into_func = module
        .declare_function(
            "sash_regex_match_into",
            Linkage::Import,
            &regex_match_into_sig,
        )
        .expect("declare sash_regex_match_into");

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

    // Case conversion functions: (s_ptr, s_len, out_ptr) -> result_len
    let mut case_sig = module.make_signature();
    case_sig.params.push(AbiParam::new(I64));
    case_sig.params.push(AbiParam::new(I64));
    case_sig.params.push(AbiParam::new(I64));
    case_sig.returns.push(AbiParam::new(I64));
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

    // sash_random() -> i64  (same nullary → i64 signature)
    let random_func = module
        .declare_function("sash_random", Linkage::Import, &getpid_sig)
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

    // sash_split_first_word_cooked — same sig as split_first_word (DD#86 readc).
    let split_first_word_cooked_func = module
        .declare_function("sash_split_first_word_cooked", Linkage::Import, &split_sig)
        .expect("declare");

    // sash_deescape(ptr, len) -> (out_ptr, out_len)  (DD#86 readc)
    let mut deescape_sig = module.make_signature();
    deescape_sig.params.push(AbiParam::new(I64)); // ptr
    deescape_sig.params.push(AbiParam::new(I64)); // len
    deescape_sig.returns.push(AbiParam::new(I64)); // out_ptr
    deescape_sig.returns.push(AbiParam::new(I64)); // out_len
    let deescape_func = module
        .declare_function("sash_deescape", Linkage::Import, &deescape_sig)
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

    ExternFuncs {
        write: write_func,
        exit: exit_func,
        emsg: emsg_func,
        emsg_status: emsg_status_func,
        bless_fail: bless_fail_func,
        arith_overflow: arith_overflow_func,
        run: run_func,
        getenv: getenv_func,
        capture: capture_func,
        strlen: strlen_func,
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
        is_int: is_int_func,
        atoi_or: atoi_or_func,
        atoi_or_abort: atoi_or_abort_func,
        itoa: itoa_func,
        join_args: join_args_func,
        assoc_lookup: assoc_lookup_func,
        regex_match: regex_match_func,
        regex_replace: regex_replace_func,
        regex_match_str: regex_match_str_func,
        regex_match_into: regex_match_into_func,
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
        toupper_all: toupper_all_func,
        tolower_all: tolower_all_func,
        getpid: getpid_func,
        getppid: getppid_func,
        random: random_func,
        epochseconds: epochseconds_func,
        trap: trap_func,
        glob: glob_func,
        glob_regex: glob_regex_func,
        capture_start: capture_start_func,
        capture_end: capture_end_func,
        split_first_word: split_first_word_func,
        split_first_word_cooked: split_first_word_cooked_func,
        deescape: deescape_func,
        split_field: split_field_func,
        split_into_array: split_into_array_func,
        read_all: read_all_func,
        splice_argv: splice_argv_func,
        envp: envp_data,
        argc: argc_data,
        argv: argv_data,
        argv_get: argv_get_func,
        argv_get_or: argv_get_or_func,
        argv_snapshot: argv_snapshot_func,
        splat_disposition: splat_disposition_func,
        require_value: require_value_func,
        signal_flag: signal_flag_func,
        dtoi: dtoi_func,
        itod: itod_func,
        signum: signum_data,
        trace_flag: trace_flag_data,
    }
}

/// Intern all string literals used by the SSA program into the object module.
/// Returns a map from string content to DataId.
pub(super) fn intern_literals(
    module: &mut ObjectModule,
    program: &SsaProgram,
    _bash_source: Option<&str>,
    trace: bool,
) -> (
    HashMap<String, cranelift_module::DataId>,
    cranelift_module::DataId,
    cranelift_module::DataId,
) {
    let mut interner = Interner::new(module);

    let space_id = interner.intern(" ");
    let nl_id = interner.intern("\n");
    let _empty_id = interner.intern(""); // for argv bounds check
                                         // DD#55 abort diagnostics: pre-intern the static message segments so
                                         // the (immutable) literals map can resolve them at instruction-emit
                                         // time (cd / echo write-error contexts; the path segment is dynamic).
    let _ = interner.intern("cd");
    let _ = interner.intern("echo: write error");

    for block in program.blocks.values() {
        for inst in &block.insts {
            interner.intern_inst(inst);
        }
    }

    if trace {
        for block in program.blocks.values() {
            interner.intern_trace_terminator(block, program);
        }
    }

    (interner.literals, space_id, nl_id)
}

/// Owns the literal map and a mutable borrow of the object module while a
/// single SSA program is scanned for string literals to intern. Splitting
/// `intern_literals` by SSA-value / IR-expr / test-expr / instruction shape
/// keeps each method small enough to read on one screen.
struct Interner<'a> {
    module: &'a mut ObjectModule,
    literals: HashMap<String, cranelift_module::DataId>,
}

impl<'a> Interner<'a> {
    fn new(module: &'a mut ObjectModule) -> Self {
        Self {
            module,
            literals: HashMap::new(),
        }
    }

    fn intern(&mut self, s: &str) -> cranelift_module::DataId {
        if let Some(&id) = self.literals.get(s) {
            return id;
        }
        let name = format!("__str_{}", self.literals.len());
        let data_id = self
            .module
            .declare_data(&name, Linkage::Local, false, false)
            .expect("declare data");
        let mut desc = DataDescription::new();
        let mut bytes = s.as_bytes().to_vec();
        bytes.push(0);
        desc.define(bytes.into_boxed_slice());
        self.module
            .define_data(data_id, &desc)
            .expect("define data");
        self.literals.insert(s.to_string(), data_id);
        data_id
    }

    fn intern_value(&mut self, v: &SsaValue) {
        // Leaf interning: literal strings, the param-expansion operand, and
        // associative-array keys are interned here; every nested SsaValue
        // is recursed into uniformly via `child_values`.
        match v {
            SsaValue::Literal(s) | SsaValue::Expr(s) => {
                self.intern(s);
            }
            SsaValue::ParamExpansion {
                operator, operand, ..
            } => self.intern_param_expansion(operator, operand),
            SsaValue::AssocArray(pairs) => {
                for (k, _) in pairs {
                    self.intern(k);
                }
            }
            // DD#69: the `@{a:…}` splat disposition's WORD is nested outside
            // `child_values` (kept lean) — intern it here.
            SsaValue::ArraySplat {
                disposition: Some(d),
                ..
            } => self.intern_value(&d.word),
            _ => {}
        }
        for child in v.child_values() {
            self.intern_value(child);
        }
    }

    fn intern_param_expansion(&mut self, operator: &str, operand: &str) {
        self.intern(operand);
        if operator == "glob" || operator == "glob_regex" {
            self.intern(".");
        }
        // DD#33: per-element operations on arrvals
        if operator.starts_with("arrvals_/") {
            // operand is "regex\0replacement" (null-separated)
            for part in operand.split('\0') {
                self.intern(part);
            }
        }
    }

    fn intern_ir_expr(&mut self, expr: &crate::ir::Expr) {
        match &expr.kind {
            crate::ir::ExprKind::Literal(s) | crate::ir::ExprKind::SingleQuoted(s) => {
                self.intern(s);
            }
            crate::ir::ExprKind::Concat(parts) | crate::ir::ExprKind::DoubleQuoted(parts) => {
                for p in parts {
                    self.intern_ir_expr(p);
                }
            }
            _ => {}
        }
    }

    fn intern_test_expr(&mut self, test: &SsaTest) {
        match test {
            SsaTest::Binary { left, right, .. } => {
                self.intern_value(left);
                match right {
                    SsaTestRhs::Value(v) => self.intern_value(v),
                    SsaTestRhs::Pattern(p) => {
                        self.intern(p);
                    }
                }
            }
            SsaTest::Unary { operand, .. } => {
                self.intern_value(operand);
            }
            SsaTest::Not(inner) => {
                self.intern_test_expr(inner);
            }
            SsaTest::And(a, b) | SsaTest::Or(a, b) => {
                self.intern_test_expr(a);
                self.intern_test_expr(b);
            }
        }
    }

    fn intern_inst(&mut self, inst: &SsaInst) {
        match inst {
            SsaInst::Assign { value, .. } => self.intern_value(value),
            SsaInst::Command {
                name,
                args,
                redirects,
                ..
            } => self.intern_command_parts(name, args, redirects, true),
            SsaInst::Test(test_expr) => self.intern_test_expr(test_expr),
            SsaInst::Pipeline { stages, .. } => {
                for (name, args, redirects) in stages {
                    self.intern_command_parts(name, args, redirects, false);
                }
            }
            SsaInst::SubscriptAssign { index, value, .. } => {
                self.intern_value(index);
                self.intern_value(value);
            }
            SsaInst::ArrayAppend { value, .. } => self.intern_value(value),
            SsaInst::MatchInto {
                subject, pattern, ..
            } => {
                self.intern_value(subject);
                self.intern_value(pattern);
            }
            SsaInst::ReadLine { redirects, .. } | SsaInst::ReadLineMulti { redirects, .. } => {
                self.intern_redirect_targets(redirects);
            }
            SsaInst::Bless { test, source, .. } => {
                self.intern_test_expr(test);
                self.intern(&source.name);
            }
            SsaInst::BlessAll { test, source, .. } => {
                self.intern_test_expr(test);
                self.intern(&source.name);
            }
            _ => {}
        }
    }

    /// Intern the name + each arg + each redirect target for a Command
    /// or one stage of a Pipeline. Skip the bare-`[[` command name (it's
    /// a sigil, not an executable path needing string interning).
    fn intern_command_parts(
        &mut self,
        name: &str,
        args: &[SsaValue],
        redirects: &[crate::ir::Redirect],
        skip_dbl_bracket: bool,
    ) {
        if !(skip_dbl_bracket && name == "[[") {
            self.intern(name);
        }
        for a in args {
            self.intern_value(a);
        }
        self.intern_redirect_targets(redirects);
    }

    fn intern_redirect_targets(&mut self, redirects: &[crate::ir::Redirect]) {
        for r in redirects {
            self.intern_ir_expr(&r.target);
        }
    }

    /// DD#32: intern branch trace messages for the block's terminator.
    fn intern_trace_terminator(&mut self, block: &BasicBlock, program: &SsaProgram) {
        match &block.terminator {
            crate::ssa::Terminator::Branch {
                true_block,
                false_block,
                ..
            } => {
                let true_label = program.block(*true_block).label.as_str();
                let false_label = program.block(*false_block).label.as_str();
                self.intern(&format!("+ branch \u{2192} {}\n", true_label));
                self.intern(&format!("+ branch \u{2192} {}\n", false_label));
            }
            crate::ssa::Terminator::Loop {
                body_block,
                exit_block,
                ..
            } => {
                let body_label = program.block(*body_block).label.as_str();
                let exit_label = program.block(*exit_block).label.as_str();
                self.intern(&format!("+ loop \u{2192} {}\n", body_label));
                self.intern(&format!("+ loop \u{2192} {}\n", exit_label));
            }
            _ => {}
        }
    }
}

/// Create Cranelift blocks and allocate variables for the SSA program.
/// Returns the per-function compilation state.
pub(super) fn allocate_variables(
    builder: &mut cranelift_frontend::FunctionBuilder,
    program: &SsaProgram,
    reachable: &std::collections::HashSet<usize>,
) -> super::types::FuncState {
    use super::types::FuncState;

    let cl_blocks = build_cl_blocks(builder, program, reachable);
    let entry_cl = cl_blocks[&program.entry];
    // DD#56: resolve end_block CL target for abort paths
    let end_cl_block: Option<cranelift_codegen::ir::Block> =
        program.end_block.and_then(|id| cl_blocks.get(&id).copied());
    builder.append_block_params_for_function_params(entry_cl);
    builder.switch_to_block(entry_cl);

    // All SSA versions of the same bash variable (x_1, x_2, x_3) share the
    // SAME Cranelift Variable pair. Cranelift's SSA builder handles the
    // merge at join points automatically — our SSA versioning is for the
    // oracle and printer, not for Cranelift.
    let base_vars = collect_base_vars(builder, program);
    let vars = map_versioned_names(program, &base_vars);
    let ambient = declare_ambient_vars(builder);

    FuncState {
        cl_blocks,
        vars,
        test_result_var: ambient.test_result_var,
        exit_status_var: ambient.exit_status_var,
        pipe_read_fd_var: ambient.pipe_read_fd_var,
        capture_saved_var: ambient.capture_saved_var,
        capture_pipe_var: ambient.capture_pipe_var,
        saved_end_status_var: ambient.saved_end_status_var,
        end_cl_block,
    }
}

fn build_cl_blocks(
    builder: &mut cranelift_frontend::FunctionBuilder,
    program: &SsaProgram,
    reachable: &std::collections::HashSet<usize>,
) -> HashMap<BlockId, cranelift_codegen::ir::Block> {
    let mut cl_blocks = HashMap::new();
    for &id in program.blocks.keys() {
        if !reachable.contains(&id) {
            continue;
        }
        cl_blocks.insert(id, builder.create_block());
    }
    cl_blocks
}

fn make_str_slot(builder: &mut cranelift_frontend::FunctionBuilder) -> super::types::VarSlot {
    super::types::VarSlot::Str {
        ptr: builder.declare_var(I64),
        len: builder.declare_var(I64),
    }
}

fn slot_for_var_type(
    builder: &mut cranelift_frontend::FunctionBuilder,
    var_type: &crate::ir::VarType,
) -> super::types::VarSlot {
    if *var_type == crate::ir::VarType::Int {
        super::types::VarSlot::Int(builder.declare_var(I64))
    } else {
        make_str_slot(builder)
    }
}

fn collect_base_vars(
    builder: &mut cranelift_frontend::FunctionBuilder,
    program: &SsaProgram,
) -> HashMap<String, super::types::VarSlot> {
    let mut base_vars = HashMap::new();
    for block in program.blocks.values() {
        for inst in &block.insts {
            declare_inst_bases(builder, inst, &mut base_vars);
        }
        for phi in &block.phis {
            base_vars
                .entry(phi.target.name.clone())
                .or_insert_with(|| slot_for_var_type(builder, &phi.target.var_type));
        }
    }
    base_vars
}

fn declare_inst_bases(
    builder: &mut cranelift_frontend::FunctionBuilder,
    inst: &SsaInst,
    base_vars: &mut HashMap<String, super::types::VarSlot>,
) {
    match inst {
        SsaInst::ReadLineMulti { targets, .. } => {
            for target in targets {
                base_vars
                    .entry(target.name.clone())
                    .or_insert_with(|| make_str_slot(builder));
            }
        }
        SsaInst::Mapfile { array, .. }
        | SsaInst::ReadArray { array, .. }
        | SsaInst::MatchInto { array, .. } => {
            base_vars
                .entry(array.clone())
                .or_insert_with(|| make_str_slot(builder));
        }
        SsaInst::Command { name, args, .. } if name == "__store_bg_pid" => {
            declare_bg_pid_base(builder, args, base_vars);
        }
        SsaInst::Assign { target, value } => {
            base_vars
                .entry(target.name.clone())
                .or_insert_with(|| assign_target_slot(builder, target, value));
        }
        SsaInst::ReadLine { target, .. } | SsaInst::CaptureEnd { target, .. } => {
            base_vars
                .entry(target.name.clone())
                .or_insert_with(|| slot_for_var_type(builder, &target.var_type));
        }
        _ => {}
    }
}

/// The slot for an `Assign` target: a collection-valued assignment holds a
/// table pointer (a Str slot) even when its elements are int (DD#76); otherwise
/// the slot follows the target's declared type.
fn assign_target_slot(
    builder: &mut cranelift_frontend::FunctionBuilder,
    target: &crate::ssa::VarRef,
    value: &SsaValue,
) -> super::types::VarSlot {
    if matches!(value, SsaValue::IndexedArray(_) | SsaValue::AssocArray(_)) {
        make_str_slot(builder)
    } else {
        slot_for_var_type(builder, &target.var_type)
    }
}

/// `__store_bg_pid <var>` — declare the pid target as a Str slot.
fn declare_bg_pid_base(
    builder: &mut cranelift_frontend::FunctionBuilder,
    args: &[SsaValue],
    base_vars: &mut HashMap<String, super::types::VarSlot>,
) {
    if let Some(SsaValue::Var(vr)) = args.first() {
        base_vars
            .entry(vr.name.clone())
            .or_insert_with(|| make_str_slot(builder));
    }
}

fn map_versioned_names(
    program: &SsaProgram,
    base_vars: &HashMap<String, super::types::VarSlot>,
) -> HashMap<String, super::types::VarSlot> {
    let mut vars = HashMap::new();
    for block in program.blocks.values() {
        for inst in &block.insts {
            map_inst_versioned(inst, base_vars, &mut vars);
        }
        for phi in &block.phis {
            vars.entry(phi.target.display())
                .or_insert_with(|| base_vars[&phi.target.name]);
            for (_, src) in &phi.sources {
                vars.entry(src.display())
                    .or_insert_with(|| base_vars[&src.name]);
            }
        }
    }
    vars
}

fn map_inst_versioned(
    inst: &SsaInst,
    base_vars: &HashMap<String, super::types::VarSlot>,
    vars: &mut HashMap<String, super::types::VarSlot>,
) {
    match inst {
        SsaInst::ReadLineMulti { targets, .. } => {
            for target in targets {
                vars.entry(target.display())
                    .or_insert_with(|| base_vars[&target.name]);
            }
        }
        SsaInst::Assign { target, .. }
        | SsaInst::ReadLine { target, .. }
        | SsaInst::CaptureEnd { target, .. } => {
            vars.entry(target.display())
                .or_insert_with(|| base_vars[&target.name]);
        }
        SsaInst::Command { name, args, .. } if name == "__store_bg_pid" => {
            if let Some(SsaValue::Var(vr)) = args.first() {
                vars.entry(vr.display())
                    .or_insert_with(|| base_vars[&vr.name]);
            }
        }
        _ => {}
    }
}

struct AmbientVars {
    test_result_var: cranelift_frontend::Variable,
    exit_status_var: cranelift_frontend::Variable,
    saved_end_status_var: cranelift_frontend::Variable,
    pipe_read_fd_var: cranelift_frontend::Variable,
    capture_saved_var: cranelift_frontend::Variable,
    capture_pipe_var: cranelift_frontend::Variable,
}

fn declare_ambient_vars(builder: &mut cranelift_frontend::FunctionBuilder) -> AmbientVars {
    use cranelift_codegen::ir::InstBuilder;

    let zero = builder.ins().iconst(I64, 0);
    let decl = |v: &mut cranelift_frontend::FunctionBuilder| {
        let var = v.declare_var(I64);
        v.def_var(var, zero);
        var
    };
    AmbientVars {
        // [[ ]] last test result
        test_result_var: decl(builder),
        // $? — exit status of last command (0 = success)
        exit_status_var: decl(builder),
        // DD#56: saved exit status for END block (preserves triggering status)
        saved_end_status_var: decl(builder),
        // Pipe read fd for pipe-while-read pattern (fd from captured pipeline)
        pipe_read_fd_var: decl(builder),
        // Capture state vars for $(func) capture
        capture_saved_var: decl(builder),
        capture_pipe_var: decl(builder),
    }
}

/// Initialize positional parameters ($1-$9, $@, $#, $0, $$, $?, $!) from argv.
/// Scans the SSA to determine which positionals are referenced.
#[allow(clippy::too_many_arguments)]
pub(super) fn init_positional_params(
    builder: &mut cranelift_frontend::FunctionBuilder,
    module: &mut ObjectModule,
    program: &SsaProgram,
    _externs: &ExternFuncs,
    vars: &mut std::collections::HashMap<String, super::types::VarSlot>,
    literals: &mut std::collections::HashMap<String, cranelift_module::DataId>,
    _reachable: &std::collections::HashSet<usize>,
    _space_id: cranelift_module::DataId,
    _exit_status_var: cranelift_frontend::Variable,
    _test_result_var: cranelift_frontend::Variable,
) {
    let positional_refs = collect_positional_refs(program);

    for var_name in &positional_refs {
        // $? and $$ are served by other mechanisms (exit_status_var
        // and sash_getpid via codegen of $? and $$ usages) — no
        // VarSlot needed.
        if var_name == "?" || var_name == "$" {
            continue;
        }
        let key = format!("{}_0", var_name); // version 0
        if vars.contains_key(&key) {
            continue;
        }

        let sv = init_one_positional(var_name, builder, module, literals);
        vars.insert(key, sv);
    }
}

/// Pre-scan: walk every SsaInst in every block and collect names
/// that are positional ($1, $2, …) or special ($#, $@, etc.).
fn collect_positional_refs(program: &SsaProgram) -> std::collections::HashSet<String> {
    let mut refs: std::collections::HashSet<String> = std::collections::HashSet::new();
    for block in program.blocks.values() {
        for inst in &block.insts {
            scan_inst_for_vars(inst, &mut refs);
        }
    }
    refs
}

/// Walk a single SsaInst, accumulating every positional / special
/// var name it references into `refs`.
fn scan_inst_for_vars(inst: &SsaInst, refs: &mut std::collections::HashSet<String>) {
    use super::values::{scan_test_for_vars, scan_value_for_vars};

    match inst {
        SsaInst::Assign { value, .. } => scan_value_for_vars(value, refs),
        SsaInst::Command {
            args, redirects, ..
        } => scan_command_args(args, redirects, refs),
        SsaInst::Pipeline { stages, .. } => {
            for (_, args, redirects) in stages {
                scan_command_args(args, redirects, refs);
            }
        }
        SsaInst::Test(test_expr) => scan_test_for_vars(test_expr, refs),
        SsaInst::MatchInto {
            subject, pattern, ..
        } => {
            scan_value_for_vars(subject, refs);
            scan_value_for_vars(pattern, refs);
        }
        _ => {}
    }
}

/// Walk a command's args + redirects, accumulating var refs.
/// Shared between Command and each Pipeline stage.
fn scan_command_args(
    args: &[SsaValue],
    redirects: &[crate::ir::Redirect],
    refs: &mut std::collections::HashSet<String>,
) {
    use super::redirects::scan_redirect_for_vars;
    use super::values::scan_value_for_vars;

    for a in args {
        scan_value_for_vars(a, refs);
    }
    for r in redirects {
        scan_redirect_for_vars(&r.target, refs);
    }
}

/// Allocate a VarSlot for a referenced positional/special var and give it the
/// DD#53 empty-string default. The numeric (`$1`), `$#`, and `$@` init
/// strategies were retired: top-level positionals are validation-rejected
/// (SB5024/SB5045) and function positionals resolve through the `args` array
/// (DD#60), so those forms never reach here — only special names that fall to
/// the empty default do.
fn init_one_positional(
    var_name: &str,
    builder: &mut cranelift_frontend::FunctionBuilder,
    module: &mut ObjectModule,
    literals: &mut std::collections::HashMap<String, cranelift_module::DataId>,
) -> super::types::VarSlot {
    use super::types::VarSlot;
    use cranelift_codegen::ir::types::I64;

    let sv = VarSlot::Str {
        ptr: builder.declare_var(I64),
        len: builder.declare_var(I64),
    };
    init_empty_var(builder, module, literals, &sv, var_name);
    sv
}

/// DD#53: any undeclared variable referenced as `$NAME` is
/// initialized to the empty string. We also intern the name so a
/// later source-kvp re-read can find it.
fn init_empty_var(
    builder: &mut cranelift_frontend::FunctionBuilder,
    module: &mut ObjectModule,
    literals: &mut std::collections::HashMap<String, cranelift_module::DataId>,
    sv: &super::types::VarSlot,
    var_name: &str,
) {
    use cranelift_codegen::ir::types::I64;
    use cranelift_codegen::ir::InstBuilder;

    intern_string(module, literals, var_name);
    let empty_id = intern_string(module, literals, "");
    let empty_gv = module.declare_data_in_func(empty_id, builder.func);
    let empty_ptr = builder.ins().global_value(I64, empty_gv);
    let empty_len = builder.ins().iconst(I64, 0);
    builder.def_var(sv.str_ptr(), empty_ptr);
    builder.def_var(sv.str_len(), empty_len);
}

/// Intern a string literal into the object module.
pub(super) fn intern_string(
    module: &mut ObjectModule,
    literals: &mut std::collections::HashMap<String, cranelift_module::DataId>,
    s: &str,
) -> cranelift_module::DataId {
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
}
