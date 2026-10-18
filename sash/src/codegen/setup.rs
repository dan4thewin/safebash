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

/// Append `params` I64 arguments and `returns` I64 results to `sig`. Every
/// sash-rt ABI boundary is i64-only, so a runtime import signature is just a
/// count of I64 slots — this keeps `declare_runtime_functions` free of
/// repetitive push loops (and its cognitive complexity down).
fn push_i64s(sig: &mut Signature, params: usize, returns: usize) {
    for _ in 0..params {
        sig.params.push(AbiParam::new(I64));
    }
    for _ in 0..returns {
        sig.returns.push(AbiParam::new(I64));
    }
}

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
    let mut reader_start_sig = Signature::new(CallConv::SystemV);
    push_i64s(&mut reader_start_sig, 4, 0);
    let reader_start = module
        .declare_function("sash_reader_start", Linkage::Import, &reader_start_sig)
        .expect("declare reader start");
    let mut reader_finish_sig = Signature::new(CallConv::SystemV);
    push_i64s(&mut reader_finish_sig, 1, 0);
    let reader_finish = module
        .declare_function("sash_reader_finish", Linkage::Import, &reader_finish_sig)
        .expect("declare reader finish");
    let mut reader_cleanup_sig = Signature::new(CallConv::SystemV);
    push_i64s(&mut reader_cleanup_sig, 0, 0);
    let reader_cleanup = module
        .declare_function("sash_reader_cleanup", Linkage::Import, &reader_cleanup_sig)
        .expect("declare reader cleanup");

    // sash_emsg(seg1_ptr, seg1_len, seg2_ptr, seg2_len) -> () — DD#55
    // abort diagnostic to stderr (no return value).
    let mut emsg_sig = Signature::new(CallConv::SystemV);
    push_i64s(&mut emsg_sig, 4, 0);
    let emsg_func = module
        .declare_function("sash_emsg", Linkage::Import, &emsg_sig)
        .expect("declare sash_emsg");

    // sash_emsg_status(name_ptr, name_len, status) -> () — DD#55
    // status-based abort diagnostic (`command failed (exit N)`, no errno).
    let mut emsg_status_sig = Signature::new(CallConv::SystemV);
    push_i64s(&mut emsg_status_sig, 3, 0);
    let emsg_status_func = module
        .declare_function("sash_emsg_status", Linkage::Import, &emsg_status_sig)
        .expect("declare sash_emsg_status");

    // sash_scope_restore() -> () — DD#120. Put the invoker's 0/1/2 back on
    // entry to END, which an abort inside a redirected command reaches
    // without ever running that command's restore.
    let mut scope_restore_sig = Signature::new(CallConv::SystemV);
    push_i64s(&mut scope_restore_sig, 0, 0);
    let scope_restore_func = module
        .declare_function("sash_scope_restore", Linkage::Import, &scope_restore_sig)
        .expect("declare sash_scope_restore");

    // sash_scope_bind() -> () — DD#120. `exec 2>…` rebinds the INVOKER's own
    // stderr, so the diagnostic channel must follow it; `cmd 2>…` must not.
    let mut scope_bind_sig = Signature::new(CallConv::SystemV);
    push_i64s(&mut scope_bind_sig, 0, 0);
    let scope_bind_func = module
        .declare_function("sash_scope_bind", Linkage::Import, &scope_bind_sig)
        .expect("declare sash_scope_bind");

    let mut reserve_fds_sig = Signature::new(CallConv::SystemV);
    push_i64s(&mut reserve_fds_sig, 2, 0);
    let reserve_fds_func = module
        .declare_function("sash_reserve_fds", Linkage::Import, &reserve_fds_sig)
        .expect("declare sash_reserve_fds");

    // sash_attest_fail(name_ptr, name_len, val_ptr, val_len) -> ! — attest
    // gate failure diagnostic (`<argv0>: attest: <name> = '<val>' failed …`).
    let mut attest_fail_sig = Signature::new(CallConv::SystemV);
    push_i64s(&mut attest_fail_sig, 4, 0);
    let attest_fail_func = module
        .declare_function("sash_attest_fail", Linkage::Import, &attest_fail_sig)
        .expect("declare sash_attest_fail");

    // sash_arith_overflow() -> ! — DD#25 overflow trap (no params/returns).
    let arith_overflow_sig = Signature::new(CallConv::SystemV);
    let arith_overflow_func = module
        .declare_function("sash_arith_overflow", Linkage::Import, &arith_overflow_sig)
        .expect("declare sash_arith_overflow");

    // sash_arith_divzero() -> ! — DD#25 (amended) zero-divisor trap. Same
    // shape as the overflow trap: no params, no returns, aborts with exit 2.
    let arith_divzero_sig = Signature::new(CallConv::SystemV);
    let arith_divzero_func = module
        .declare_function("sash_arith_divzero", Linkage::Import, &arith_divzero_sig)
        .expect("declare sash_arith_divzero");

    // sash_arith_negshift() -> ! — DD#25 negative-shift-count trap.
    let arith_negshift_sig = Signature::new(CallConv::SystemV);
    let arith_negshift_func = module
        .declare_function("sash_arith_negshift", Linkage::Import, &arith_negshift_sig)
        .expect("declare sash_arith_negshift");

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

    // sash_keys_snapshot(map) -> *mut SashTable  (DD#75 `keys m k`)
    let mut keys_snapshot_sig = Signature::new(CallConv::SystemV);
    keys_snapshot_sig.params.push(AbiParam::new(I64)); // source map
    keys_snapshot_sig.returns.push(AbiParam::new(I64)); // table ptr
    let keys_snapshot_func = module
        .declare_function("sash_keys_snapshot", Linkage::Import, &keys_snapshot_sig)
        .expect("declare sash_keys_snapshot");

    // sash_splat_disposition(tbl, word_ptr, word_len, has_word, abort) -> tbl
    // (DD#69 external `@{a:-WORD}` / `@{a:?WORD}` empty-disposition)
    let mut splat_disposition_sig = Signature::new(CallConv::SystemV);
    push_i64s(&mut splat_disposition_sig, 5, 1);
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

    // sash_signal_flag(signum, exiting) -> i64
    let mut signal_flag_sig = Signature::new(CallConv::SystemV);
    signal_flag_sig.params.push(AbiParam::new(I64)); // signum
    signal_flag_sig.params.push(AbiParam::new(I64)); // exiting
    signal_flag_sig.returns.push(AbiParam::new(I64));
    let signal_flag_func = module
        .declare_function("sash_signal_flag", Linkage::Import, &signal_flag_sig)
        .expect("declare sash_signal_flag");

    // sash_signum global
    let signum_data = module
        .declare_data("sash_signum", Linkage::Import, true, false)
        .expect("declare sash_signum");
    let exit_signum_data = module
        .declare_data("sash_exit_signum", Linkage::Import, true, false)
        .expect("declare sash_exit_signum");

    let trace_flag_data = module
        .declare_data("sash_trace_flag", Linkage::Import, true, false)
        .expect("declare sash_trace_flag");

    // sash_capture_drain(fd) -> (ptr, len)
    let mut capture_drain_sig = Signature::new(CallConv::SystemV);
    capture_drain_sig.params.push(AbiParam::new(I64)); // fd
    capture_drain_sig.returns.push(AbiParam::new(I64)); // buf ptr
    capture_drain_sig.returns.push(AbiParam::new(I64)); // len
    let capture_drain_func = module
        .declare_function("sash_capture_drain", Linkage::Import, &capture_drain_sig)
        .expect("declare sash_capture_drain");

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

    // DD#126: sash_wait_any(pid) -> reaped pid, or -1. The reaped child's
    // status goes into the runtime's own latch, which `sash_wait_status`
    // reads — not into `$?`, and not into anything either backend maintains.
    let mut wait_any_sig = Signature::new(CallConv::SystemV);
    wait_any_sig.params.push(AbiParam::new(I64)); // pid (<= 0 means any)
    wait_any_sig.returns.push(AbiParam::new(I64)); // reaped pid or -1
    let wait_any_func = module
        .declare_function("sash_wait_any", Linkage::Import, &wait_any_sig)
        .expect("declare sash_wait_any");

    // DD#126: sash_wait_status() -> the last reap's status, or -1.
    let mut wait_status_sig = Signature::new(CallConv::SystemV);
    wait_status_sig.returns.push(AbiParam::new(I64));
    let wait_status_func = module
        .declare_function("sash_wait_status", Linkage::Import, &wait_status_sig)
        .expect("declare sash_wait_status");

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

    // Private redirect backups: close-on-exec, excluding the referenced fd set.
    let mut dup_sig = Signature::new(CallConv::SystemV);
    dup_sig.params.push(AbiParam::new(I64));
    dup_sig.params.push(AbiParam::new(I64));
    dup_sig.params.push(AbiParam::new(I64));
    dup_sig.returns.push(AbiParam::new(I64));
    let save_fd_func = module
        .declare_function("sash_save_fd", Linkage::Import, &dup_sig)
        .expect("declare sash_save_fd");
    let mut restore_sig = Signature::new(CallConv::SystemV);
    restore_sig.params.push(AbiParam::new(I64));
    restore_sig.params.push(AbiParam::new(I64));
    let restore_fd_func = module
        .declare_function("sash_restore_fd", Linkage::Import, &restore_sig)
        .expect("declare sash_restore_fd");

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

    // DD#93 Part C content-shape tests. Same (s_ptr, s_len) -> 0/1 shape as
    // sash_is_int; adding a further class is one declaration here plus one
    // dispatch arm.
    // sash_is_print(s_ptr, s_len) -> i64 — `p?`, ASCII printable 0x20–0x7E
    let mut is_print_sig = Signature::new(CallConv::SystemV);
    is_print_sig.params.push(AbiParam::new(I64)); // s_ptr
    is_print_sig.params.push(AbiParam::new(I64)); // s_len
    is_print_sig.returns.push(AbiParam::new(I64)); // 0/1
    let is_print_func = module
        .declare_function("sash_is_print", Linkage::Import, &is_print_sig)
        .expect("declare sash_is_print");

    // sash_is_graph(s_ptr, s_len) -> i64 — `g?`, ASCII graph 0x21–0x7E
    let mut is_graph_sig = Signature::new(CallConv::SystemV);
    is_graph_sig.params.push(AbiParam::new(I64)); // s_ptr
    is_graph_sig.params.push(AbiParam::new(I64)); // s_len
    is_graph_sig.returns.push(AbiParam::new(I64)); // 0/1
    let is_graph_func = module
        .declare_function("sash_is_graph", Linkage::Import, &is_graph_sig)
        .expect("declare sash_is_graph");

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
    push_i64s(&mut regex_match_str_sig, 4, 2); // returns (ptr, len)
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
    push_i64s(&mut regex_match_into_sig, 5, 1);
    let regex_match_into_func = module
        .declare_function(
            "sash_regex_match_into",
            Linkage::Import,
            &regex_match_into_sig,
        )
        .expect("declare sash_regex_match_into");

    // sash_regex_matchall_into(str_ptr, str_len, pat_ptr, pat_len, table) -> i64
    // DD#63: `matchall S P ARR` — every match (captures, or the whole match
    // when the pattern has no groups). Same signature as the singular form.
    let mut regex_matchall_into_sig = module.make_signature();
    push_i64s(&mut regex_matchall_into_sig, 5, 1);
    let regex_matchall_into_func = module
        .declare_function(
            "sash_regex_matchall_into",
            Linkage::Import,
            &regex_matchall_into_sig,
        )
        .expect("declare sash_regex_matchall_into");

    // sash_argv_guard(ptr, len) — DD#99 stage 7: abort rather than let an
    // interior NUL truncate the argument at execve.
    let mut argv_guard_sig = module.make_signature();
    push_i64s(&mut argv_guard_sig, 2, 0);
    let argv_guard_func = module
        .declare_function("sash_argv_guard", Linkage::Import, &argv_guard_sig)
        .expect("declare sash_argv_guard");

    // sash_regex_replace(str_ptr, str_len, pat_ptr, pat_len, rep_ptr, rep_len, global) -> (ptr, len)
    let mut regex_replace_sig = module.make_signature();
    push_i64s(&mut regex_replace_sig, 7, 2); // returns (ptr, len)
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

    let mut table_join_sig = module.make_signature();
    table_join_sig.params.extend([AbiParam::new(I64); 4]);
    table_join_sig.returns.push(AbiParam::new(I64));
    let table_join_func = module
        .declare_function("sash_table_join", Linkage::Import, &table_join_sig)
        .expect("declare sash_table_join");

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

    // sash_slurp_file(path_ptr, path_len, out_len_ptr) -> ptr
    let mut slurp_sig = module.make_signature();
    slurp_sig.params.push(AbiParam::new(I64)); // path ptr
    slurp_sig.params.push(AbiParam::new(I64)); // path len
    slurp_sig.params.push(AbiParam::new(I64)); // out_len ptr
    slurp_sig.returns.push(AbiParam::new(I64)); // result ptr
    let slurp_file_func = module
        .declare_function("sash_slurp_file", Linkage::Import, &slurp_sig)
        .expect("declare sash_slurp_file");

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

    // sash_epochseconds() -> i64  (same nullary → i64 signature)
    let epochseconds_func = module
        .declare_function("sash_epochseconds", Linkage::Import, &getpid_sig)
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

    // sash_capture_start_buf() -> void  (pure site: no fd, no syscall)
    let cap_start_buf_sig = module.make_signature();
    let capture_start_buf_func = module
        .declare_function(
            "sash_capture_start_buf",
            Linkage::Import,
            &cap_start_buf_sig,
        )
        .expect("declare");

    // sash_capture_end_buf() -> (ptr, len)
    let mut cap_end_buf_sig = module.make_signature();
    cap_end_buf_sig.returns.push(AbiParam::new(I64));
    cap_end_buf_sig.returns.push(AbiParam::new(I64));
    let capture_end_buf_func = module
        .declare_function("sash_capture_end_buf", Linkage::Import, &cap_end_buf_sig)
        .expect("declare");

    // sash_split_into_array(ptr, len, delim, terminator, table) → void
    let mut split_arr_sig = module.make_signature();
    split_arr_sig.params.push(AbiParam::new(I64)); // ptr
    split_arr_sig.params.push(AbiParam::new(I64)); // len
    split_arr_sig.params.push(AbiParam::new(I64)); // delim
    split_arr_sig.params.push(AbiParam::new(I64)); // terminator (drop trailing empty)
    split_arr_sig.params.push(AbiParam::new(I64)); // table
                                                   // DD#99: sash_map_from_array(src_tbl, dst_tbl, mode, key_ptr, key_len) → i64
    let mut map_from_sig = module.make_signature();
    map_from_sig.params.push(AbiParam::new(I64)); // src table
    map_from_sig.params.push(AbiParam::new(I64)); // dst table
    map_from_sig.params.push(AbiParam::new(I64)); // mode
    map_from_sig.params.push(AbiParam::new(I64)); // key ptr
    map_from_sig.params.push(AbiParam::new(I64)); // key len
    map_from_sig.returns.push(AbiParam::new(I64)); // 1 = odd under `:?`
    let map_from_array_func = module
        .declare_function("sash_map_from_array", Linkage::Import, &map_from_sig)
        .expect("declare sash_map_from_array");

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
        reader_start,
        reader_finish,
        reader_cleanup,
        emsg: emsg_func,
        emsg_status: emsg_status_func,
        scope_bind: scope_bind_func,
        reserve_fds: reserve_fds_func,
        scope_restore: scope_restore_func,
        attest_fail: attest_fail_func,
        arith_overflow: arith_overflow_func,
        arith_divzero: arith_divzero_func,
        arith_negshift: arith_negshift_func,
        run: run_func,
        getenv: getenv_func,
        capture_drain: capture_drain_func,
        strlen: strlen_func,
        default_value: default_value_func,
        fork: fork_func,
        exec: exec_func,
        wait: wait_func,
        wait_any: wait_any_func,
        wait_status: wait_status_func,
        pipe: pipe_func,
        close: close_func,
        dup2: dup2_func,
        save_fd: save_fd_func,
        restore_fd: restore_fd_func,
        redirect: redirect_func,
        redirect_s: redirect_s_func,
        read: read_func,
        alloc: alloc_func,
        concat: concat_func,
        atoi: atoi_func,
        is_int: is_int_func,
        is_print: is_print_func,
        is_graph: is_graph_func,
        atoi_or: atoi_or_func,
        atoi_or_abort: atoi_or_abort_func,
        itoa: itoa_func,
        join_args: join_args_func,
        assoc_lookup: assoc_lookup_func,
        regex_match: regex_match_func,
        regex_replace: regex_replace_func,
        regex_match_str: regex_match_str_func,
        regex_match_into: regex_match_into_func,
        regex_matchall_into: regex_matchall_into_func,
        argv_guard: argv_guard_func,
        strcmp: strcmp_func,
        table_new: table_new_func,
        table_set: table_set_func,
        table_get: table_get_func,
        table_len: table_len_func,
        table_value: table_value_func,
        table_join: table_join_func,
        table_key: table_key_func,
        table_delete: table_delete_func,
        read_line: read_line_func,
        file_test: file_test_func,
        pow: pow_func,
        substring: substring_func,
        file_newer: file_newer_func,
        file_same_dev: file_same_dev_func,
        chdir: chdir_func,
        getcwd: getcwd_func,
        slurp_file: slurp_file_func,
        toupper_all: toupper_all_func,
        tolower_all: tolower_all_func,
        getpid: getpid_func,
        getppid: getppid_func,
        random: random_func,
        epochseconds: epochseconds_func,
        trap: trap_func,
        glob: glob_func,
        capture_start: capture_start_func,
        capture_end: capture_end_func,
        capture_start_buf: capture_start_buf_func,
        capture_end_buf: capture_end_buf_func,
        split_into_array: split_into_array_func,
        map_from_array: map_from_array_func,
        read_all: read_all_func,
        splice_argv: splice_argv_func,
        envp: envp_data,
        argc: argc_data,
        argv: argv_data,
        argv_get: argv_get_func,
        argv_get_or: argv_get_or_func,
        argv_snapshot: argv_snapshot_func,
        keys_snapshot: keys_snapshot_func,
        splat_disposition: splat_disposition_func,
        require_value: require_value_func,
        signal_flag: signal_flag_func,
        signum: signum_data,
        exit_signum: exit_signum_data,
        trace_flag: trace_flag_data,
    }
}

/// Intern all string literals used by the SSA program into the object module.
/// Returns a map from string content to DataId.
pub(super) fn intern_literals(
    module: &mut ObjectModule,
    program: &SsaProgram,
    trace: bool,
) -> (
    HashMap<String, cranelift_module::DataId>,
    cranelift_module::DataId,
    cranelift_module::DataId,
) {
    let mut interner = Interner::new(module, &program.source_names);

    let space_id = interner.intern(" ");
    let nl_id = interner.intern("\n");
    let _empty_id = interner.intern(""); // for argv bounds check
                                         // DD#55 abort diagnostics: pre-intern the static message segments so
                                         // the (immutable) literals map can resolve them at instruction-emit
                                         // time (cd / echo write-error contexts; the path segment is dynamic).
    let _ = interner.intern("cd");
    let _ = interner.intern("echo: write error");
    let _ = interner.intern("pipe");
    let _ = interner.intern("fork");
    let _ = interner.intern("dup2");

    for inst in program.blocks.values().flat_map(|block| &block.insts) {
        interner.intern_inst(inst);
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
    source_names: &'a crate::bindings::SourceNames,
}

impl<'a> Interner<'a> {
    fn new(module: &'a mut ObjectModule, source_names: &'a crate::bindings::SourceNames) -> Self {
        Self {
            module,
            literals: HashMap::new(),
            source_names,
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
        // associative-array keys are the string-keyed halves that
        // `child_values` deliberately does not yield; every nested SsaValue
        // (the splat disposition word included) is recursed into uniformly
        // via `child_values`.
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
            _ => {}
        }
        for child in v.child_values() {
            self.intern_value(child);
        }
    }

    fn intern_param_expansion(&mut self, operator: &str, operand: &str) {
        self.intern(operand);
        if operator == "glob" {
            self.intern(".");
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

    fn intern_pipeline(
        &mut self,
        stages: &[(String, Vec<SsaValue>, Vec<crate::ir::Redirect>)],
        capture: &PipelineCapture,
    ) {
        if capture.target().is_some() {
            self.intern("");
        }
        for (name, args, redirects) in stages {
            self.intern_command_parts(name, args, redirects, false);
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
            SsaInst::Test(test_expr, _) => self.intern_test_expr(test_expr),
            SsaInst::Pipeline {
                stages, capture, ..
            } => self.intern_pipeline(stages, capture),
            SsaInst::SubscriptAssign { index, value, .. } => {
                self.intern_value(index);
                self.intern_value(value);
            }
            // One interned value each. DD#99: `MapFromArray`'s is the `:-KEY`
            // key (or the `:?MSG` message), `SplitValue`'s is the split source.
            SsaInst::ArrayAppend { value: v, .. }
            | SsaInst::MapFromArray { key: v, .. }
            | SsaInst::SplitValue { source: v, .. } => self.intern_value(v),
            other => self.intern_inst_rest(other),
        }
    }

    /// The tail of `intern_inst` — the match/read/attest instructions. Split
    /// out only to keep each dispatcher under the complexity gate.
    fn intern_inst_rest(&mut self, inst: &SsaInst) {
        match inst {
            SsaInst::MatchInto {
                subject, pattern, ..
            } => {
                self.intern_value(subject);
                self.intern_value(pattern);
            }
            SsaInst::ReadLine { redirects, .. }
            | SsaInst::ReadArray { redirects, .. }
            | SsaInst::RedirectPush { redirects, .. } => {
                self.intern_redirect_targets(redirects);
            }
            SsaInst::Attest { test, source, .. } | SsaInst::AttestAll { test, source, .. } => {
                self.intern_test_expr(test);
                let name = self.source_names.original(&source.name).to_string();
                self.intern(&name);
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
        readers: declare_reader_slots(builder, program),
        capture_slots: declare_capture_slots(builder, program),
        redirect_slots: declare_redirect_slots(builder, program),
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

/// Bulk fills reuse the collection's initializing Assign; only defining
/// instructions allocate base slots.
fn declare_inst_bases(
    builder: &mut cranelift_frontend::FunctionBuilder,
    inst: &SsaInst,
    base_vars: &mut HashMap<String, super::types::VarSlot>,
) {
    match inst {
        SsaInst::Command { name, args, .. } if name == "__store_bg_pid" => {
            declare_bg_pid_base(builder, args, base_vars);
        }
        SsaInst::Assign { target, value } => {
            base_vars
                .entry(target.name.clone())
                .or_insert_with(|| assign_target_slot(builder, target, value));
        }
        SsaInst::ReadLine { target, .. }
        | SsaInst::CaptureEnd { target, .. }
        | SsaInst::Pipeline {
            capture: PipelineCapture::Value(target),
            ..
        } => {
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
        SsaInst::Assign { target, .. }
        | SsaInst::ReadLine { target, .. }
        | SsaInst::CaptureEnd { target, .. }
        | SsaInst::Pipeline {
            capture: PipelineCapture::Value(target),
            ..
        }
        // The bulk fills define a real SSA version so the taint backstop can
        // see them (they fill a collection in place). Every version must ALIAS
        // the one base slot: an unmapped version makes `load_var` miss, which
        // used to hand back a null pointer and segfault in `sash_table_len`
        // and is now a compiler ICE naming this function.
        | SsaInst::ArgvFill { target, .. }
        | SsaInst::KeysFill { target, .. }
        | SsaInst::ReadArray { target, .. }
        | SsaInst::SplitValue { target, .. }
        | SsaInst::MapFromArray { target, .. } => {
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
    }
}

fn declare_reader_slots(
    builder: &mut cranelift_frontend::FunctionBuilder,
    program: &SsaProgram,
) -> std::collections::BTreeMap<u32, super::types::ReaderSlot> {
    use cranelift_codegen::ir::{StackSlotData, StackSlotKind};
    program
        .reader_pipelines()
        .into_iter()
        .map(|(site, pipeline)| {
            let scope = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                32,
                3,
            ));
            let pids = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                (pipeline.stages.len() * 8) as u32,
                3,
            ));
            (
                site,
                super::types::ReaderSlot {
                    scope,
                    pids,
                    pipeline,
                },
            )
        })
        .collect()
}

/// Declare one `(saved_stdout, capture_handle)` variable pair per capture site.
/// Sites are numbered by the transform, so the count is just the high-water
/// mark; nesting depth is static because sash inlines functions and rejects
/// recursion (SB5037), which is why per-site slots replace a runtime stack.
fn declare_capture_slots(
    builder: &mut cranelift_frontend::FunctionBuilder,
    program: &SsaProgram,
) -> Vec<(cranelift_frontend::Variable, cranelift_frontend::Variable)> {
    use cranelift_codegen::ir::InstBuilder;

    let n = program.capture_site_count();
    let zero = builder.ins().iconst(I64, 0);
    (0..n)
        .map(|_| {
            let saved = builder.declare_var(I64);
            let handle = builder.declare_var(I64);
            builder.def_var(saved, zero);
            builder.def_var(handle, zero);
            (saved, handle)
        })
        .collect()
}

/// One saved-fd slot per redirect in each `RedirectPush` scope. Widths come
/// from the program (a scope saves one fd per redirect it applies), and the
/// count is the high-water site number — scopes nest, so per-site slots
/// replace a runtime stack for exactly the reason captures do.
fn declare_redirect_slots(
    builder: &mut cranelift_frontend::FunctionBuilder,
    program: &SsaProgram,
) -> Vec<Vec<cranelift_frontend::Variable>> {
    use cranelift_codegen::ir::InstBuilder;

    let zero = builder.ins().iconst(I64, 0);
    program
        .redirect_site_widths()
        .into_iter()
        .map(|width| {
            (0..width)
                .map(|_| {
                    let v = builder.declare_var(I64);
                    builder.def_var(v, zero);
                    v
                })
                .collect()
        })
        .collect()
}

/// DD#53: give every *undeclared* variable used as a redirect target the
/// empty-string default, so `/bin/echo hi > $f` opens "" rather than reading
/// a slot nothing ever defined.
///
/// Redirect targets are the only names that need this. The pre-scan used to
/// walk assignment values, test expressions, `MatchInto` subjects and command
/// args too, looking for positional (`$1`) and special (`$#`, `$@`, `$0`)
/// names — but every one of those forms is rejected long before codegen
/// (SB5024 for a top-level `$1`/`$#`, SB5045 for `$@`, SB5017 for `$0`), and
/// in-function positionals resolve through the `args` array (DD#60). That
/// filter ran 89 times per sweep and inserted exactly nothing, so it and the
/// four scanners feeding it are gone.
pub(super) fn init_undeclared_redirect_vars(
    builder: &mut cranelift_frontend::FunctionBuilder,
    module: &mut ObjectModule,
    program: &SsaProgram,
    vars: &mut std::collections::HashMap<String, super::types::VarSlot>,
    literals: &mut std::collections::HashMap<String, cranelift_module::DataId>,
) {
    for var_name in &collect_redirect_target_vars(program) {
        // `_0` is this pass's private namespace, so no collected name can
        // already be present: SSA versions start at 1 (`VersionState::define`
        // increments before use) and the ref set is a `HashSet`. Everything
        // else that spells `{name}_0` only ever reads the convention.
        let sv = init_one_redirect_var(var_name, builder, module, literals);
        vars.insert(format!("{}_0", var_name), sv);
    }
}

/// Pre-scan: every name used as a redirect target, in a plain command or in
/// any stage of a pipeline.
fn collect_redirect_target_vars(program: &SsaProgram) -> std::collections::HashSet<String> {
    let mut refs: std::collections::HashSet<String> = std::collections::HashSet::new();
    let insts = program.blocks.values().flat_map(|b| b.insts.iter());
    for r in insts.flat_map(inst_redirects) {
        super::redirects::scan_redirect_for_vars(&r.target, &mut refs);
    }
    refs
}

/// The redirects an instruction carries: a plain command's own, or one list
/// per pipeline stage. Naming them rather than looping inline keeps the walk
/// above a single flat loop — per-arm `for`s are the nesting the cognitive
/// metric charges for, and they say nothing about the shape.
fn inst_redirects(inst: &SsaInst) -> Vec<&crate::ir::Redirect> {
    match inst {
        SsaInst::Command { redirects, .. } => redirects.iter().collect(),
        SsaInst::RedirectPush { redirects, .. } => redirects.iter().collect(),
        SsaInst::Pipeline { stages, .. } => stages.iter().flat_map(|(_, _, r)| r.iter()).collect(),
        _ => Vec::new(),
    }
}

/// Allocate a VarSlot for a redirect-target name and give it the DD#53
/// empty-string default.
fn init_one_redirect_var(
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

/// Define the strong `sash_heap_size` that displaces the runtime's weak
/// 256 MB default at link time (`--heap-size`). Emitted only when the flag
/// was given, so an ordinary build carries no such symbol and keeps the
/// default. Little-endian: both supported targets (x86_64, aarch64) are.
pub(crate) fn define_heap_size(module: &mut ObjectModule) {
    let Some(bytes) = crate::get_heap_size() else {
        return;
    };
    let data_id = module
        .declare_data("sash_heap_size", Linkage::Export, false, false)
        .expect("declare sash_heap_size");
    let mut desc = DataDescription::new();
    desc.define((bytes as u64).to_le_bytes().to_vec().into_boxed_slice());
    module
        .define_data(data_id, &desc)
        .expect("define sash_heap_size");
}

pub(super) fn reserve_user_fds(
    fds: &[i64],
    builder: &mut cranelift_frontend::FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
) {
    use cranelift_codegen::ir::InstBuilder;

    if fds.is_empty() {
        return;
    }
    let data = module
        .declare_data("__sash_user_fds", Linkage::Local, false, false)
        .expect("declare source descriptor set");
    let mut desc = DataDescription::new();
    let bytes = fds
        .iter()
        .flat_map(|fd| fd.to_le_bytes())
        .collect::<Vec<_>>();
    desc.define(bytes.into_boxed_slice());
    desc.set_align(8);
    module
        .define_data(data, &desc)
        .expect("define source descriptor set");
    let global = module.declare_data_in_func(data, builder.func);
    let ptr = builder.ins().global_value(I64, global);
    let len = builder.ins().iconst(I64, fds.len() as i64);
    let reserve = module.declare_func_in_func(externs.reserve_fds, builder.func);
    builder.ins().call(reserve, &[ptr, len]);
}
