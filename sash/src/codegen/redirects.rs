//! File descriptor redirection: save, apply, restore.
//!
//! SECURITY: Only fd manipulation via dup/dup2/close. No filename globbing.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types::I64;
use cranelift_codegen::ir::InstBuilder;
use cranelift_codegen::ir::Value;
use cranelift_frontend::{FunctionBuilder, Variable};
use cranelift_module::Module;
use cranelift_object::ObjectModule;
use std::collections::HashMap;

use super::types::{ExternFuncs, VarSlot};

#[derive(Clone)]
pub(super) struct SavedFd {
    pub(super) original_fd: i64, // the fd that was redirected (e.g. 1, 2)
    pub(super) saved_copy: cranelift_codegen::ir::Value, // dup'd copy to restore from
}

/// DD#55: a redirect `open(2)` setup-failure surfaced to the command
/// handler. `failed` is 1 when any open redirect failed (else 0);
/// `path_ptr`/`path_len` name the first open-redirect target (for the
/// `<argv0>: <path>: <reason>` diagnostic).
pub(super) struct RedirectFailure {
    pub(super) failed: Value,
    pub(super) path_ptr: Value,
    pub(super) path_len: Value,
}

/// Emit fd redirections before a command. Returns saved state for restore.
/// Backward-compatible wrapper over [`emit_redirects_guarded`] for callers
/// that don't guard against open(2) failure.
pub(super) fn emit_redirects(
    redirects: &[crate::ir::Redirect],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    literals: &HashMap<String, cranelift_module::DataId>,
    vars: &HashMap<String, VarSlot>,
    #[allow(unused_variables)] _exit_status_var: Option<Variable>,
) -> Vec<SavedFd> {
    emit_redirects_guarded(redirects, builder, module, externs, literals, vars).0
}

/// Like [`emit_redirects`], but also returns a [`RedirectFailure`] when at
/// least one redirect is a file `open(2)` (Write/Append/Read), so the
/// caller can apply the DD#55 setup-failure semantics (skip body, fail
/// status, abort governed by context). Returns `None` failure when no
/// open redirect is present (dup/herestring only).
pub(super) fn emit_redirects_guarded(
    redirects: &[crate::ir::Redirect],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    literals: &HashMap<String, cranelift_module::DataId>,
    vars: &HashMap<String, VarSlot>,
) -> (Vec<SavedFd>, Option<RedirectFailure>) {
    let mut saved = Vec::new();
    let mut acc: Option<RedirectFailure> = None;
    for redir in redirects {
        let fd = redir.fd.unwrap_or(default_fd_for_op(&redir.op)) as i64;
        let fd_val = builder.ins().iconst(I64, fd);
        saved.push(emit_save_fd(fd, fd_val, builder, module, externs));
        if let Some((failed, p_ptr, p_len)) =
            emit_one_redirect(redir, fd_val, builder, module, externs, literals, vars)
        {
            acc = Some(match acc {
                None => RedirectFailure {
                    failed,
                    path_ptr: p_ptr,
                    path_len: p_len,
                },
                Some(prev) => RedirectFailure {
                    failed: builder.ins().bor(prev.failed, failed),
                    // Keep the first failing-candidate path for the message.
                    path_ptr: prev.path_ptr,
                    path_len: prev.path_len,
                },
            });
        }
    }
    (saved, acc)
}

/// Bash default fd: stdout for write-ish ops, stdin for read-ish ops.
fn default_fd_for_op(op: &crate::ir::RedirectOp) -> u32 {
    use crate::ir::RedirectOp;
    match op {
        RedirectOp::Write | RedirectOp::Append | RedirectOp::DupOutput => 1,
        RedirectOp::Read | RedirectOp::DupInput | RedirectOp::HereString => 0,
        _ => 1,
    }
}

/// dup() the live fd so it can be restored later, and record the pairing.
fn emit_save_fd(
    fd: i64,
    fd_val: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
) -> SavedFd {
    let dup_ref = module.declare_func_in_func(externs.dup, builder.func);
    let dup_call = builder.ins().call(dup_ref, &[fd_val]);
    let saved_copy = builder.inst_results(dup_call)[0];
    SavedFd {
        original_fd: fd,
        saved_copy,
    }
}

/// Dispatch on the RedirectOp shape. `Write`/`Append`/`Read` share the
/// open-and-dup2 pattern (open_flags_for_op covers the three flag
/// values); `DupOutput` and `HereString` get their own
/// emitters. Unknown ops are no-ops (the save still happened).
/// Returns `Some((failed, path_ptr, path_len))` for a file `open(2)`
/// redirect (DD#55 setup-failure surfacing); `None` for dup/herestring/noop.
fn emit_one_redirect(
    redir: &crate::ir::Redirect,
    fd_val: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    literals: &HashMap<String, cranelift_module::DataId>,
    vars: &HashMap<String, VarSlot>,
) -> Option<(Value, Value, Value)> {
    use crate::ir::RedirectOp;
    if let Some(flags) = open_flags_for_op(&redir.op) {
        return emit_open_redirect(
            fd_val,
            &redir.target,
            flags,
            builder,
            module,
            externs,
            literals,
            vars,
        );
    }
    match redir.op {
        RedirectOp::DupOutput => {
            emit_dup_output(fd_val, &redir.target, builder, module, externs);
        }
        RedirectOp::HereString => {
            emit_herestring_redirect(
                fd_val,
                &redir.target,
                builder,
                module,
                externs,
                literals,
                vars,
            );
        }
        _ => {}
    }
    None
}

/// open(2) flags for the three open-and-dup2 redirect ops.
/// (1 = O_WRONLY|O_CREAT|O_TRUNC, 2 = O_WRONLY|O_CREAT|O_APPEND,
/// 0 = O_RDONLY — interpreted by the `redirect`/`redirect_s` runtime.)
fn open_flags_for_op(op: &crate::ir::RedirectOp) -> Option<i64> {
    use crate::ir::RedirectOp;
    match op {
        RedirectOp::Write => Some(1),
        RedirectOp::Append => Some(2),
        RedirectOp::Read => Some(0),
        _ => None,
    }
}

/// `> file` / `>> file` / `< file`. Literal path → fast `redirect`;
/// dynamic (tilde, var, concat) → `redirect_s` with computed ptr+len.
/// If a literal isn't in the literals map the redirect is silently
/// skipped (matches pre-refactor behavior) and `None` is returned.
/// Otherwise returns `(failed, path_ptr, path_len)`: `failed` is 1 when
/// the runtime `open(2)` returned < 0 (DD#55 setup-failure), else 0.
#[allow(clippy::too_many_arguments)]
fn emit_open_redirect(
    fd_val: cranelift_codegen::ir::Value,
    target: &crate::ir::Expr,
    flags: i64,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    literals: &HashMap<String, cranelift_module::DataId>,
    vars: &HashMap<String, VarSlot>,
) -> Option<(Value, Value, Value)> {
    let flags_val = builder.ins().iconst(I64, flags);
    let (path_ptr, path_len, result) = if let crate::ir::ExprKind::Literal(s) = &target.kind {
        let &data_id = literals.get(s.as_str())?;
        let path_gv = module.declare_data_in_func(data_id, builder.func);
        let path_ptr = builder.ins().global_value(I64, path_gv);
        let path_len = builder.ins().iconst(I64, s.len() as i64);
        let redirect_ref = module.declare_func_in_func(externs.redirect, builder.func);
        let call = builder
            .ins()
            .call(redirect_ref, &[fd_val, path_ptr, flags_val]);
        (path_ptr, path_len, builder.inst_results(call)[0])
    } else {
        let (ptr, len) = lower_ir_redirect_expr(target, builder, module, vars, literals, externs);
        let redirect_s_ref = module.declare_func_in_func(externs.redirect_s, builder.func);
        let call = builder
            .ins()
            .call(redirect_s_ref, &[fd_val, ptr, len, flags_val]);
        (ptr, len, builder.inst_results(call)[0])
    };
    // failed = (result < 0) ? 1 : 0  (runtime returns -1 on open failure)
    let zero = builder.ins().iconst(I64, 0);
    let lt = builder.ins().icmp(IntCC::SignedLessThan, result, zero);
    let failed = builder.ins().uextend(I64, lt);
    Some((failed, path_ptr, path_len))
}

/// `>&N` — dup2(target_fd, fd). Non-literal targets default to fd 2.
fn emit_dup_output(
    fd_val: cranelift_codegen::ir::Value,
    target: &crate::ir::Expr,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
) {
    let target_fd: i64 = match &target.kind {
        crate::ir::ExprKind::Literal(s) => s.parse().unwrap_or(2),
        _ => 2,
    };
    let target_val = builder
        .ins()
        .iconst(cranelift_codegen::ir::types::I64, target_fd);
    let dup2_ref = module.declare_func_in_func(externs.dup2, builder.func);
    builder.ins().call(dup2_ref, &[target_val, fd_val]);
}

/// `<<EOF` / `<<<"string"` — pipe body text to stdin. Literal body
/// missing from the literals map → skip (matches pre-refactor).
fn emit_herestring_redirect(
    fd_val: cranelift_codegen::ir::Value,
    body: &crate::ir::Expr,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    literals: &HashMap<String, cranelift_module::DataId>,
    vars: &HashMap<String, VarSlot>,
) {
    let (body_ptr, body_len) = if let crate::ir::ExprKind::Literal(s) = &body.kind {
        let Some(&data_id) = literals.get(s.as_str()) else {
            return;
        };
        let gv = module.declare_data_in_func(data_id, builder.func);
        let ptr = builder
            .ins()
            .global_value(cranelift_codegen::ir::types::I64, gv);
        let len = builder
            .ins()
            .iconst(cranelift_codegen::ir::types::I64, s.len() as i64);
        (ptr, len)
    } else {
        lower_ir_redirect_expr(body, builder, module, vars, literals, externs)
    };
    let herestring_ref = module.declare_func_in_func(externs.herestring_redirect, builder.func);
    builder
        .ins()
        .call(herestring_ref, &[fd_val, body_ptr, body_len]);
}

/// Restore saved fds after a command completes
pub(super) fn emit_restore(
    saved: Vec<SavedFd>,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
) {
    for s in saved.iter().rev() {
        let fd_val = builder
            .ins()
            .iconst(cranelift_codegen::ir::types::I64, s.original_fd);
        let dup2_ref = module.declare_func_in_func(externs.dup2, builder.func);
        builder.ins().call(dup2_ref, &[s.saved_copy, fd_val]);
        let close_ref = module.declare_func_in_func(externs.close, builder.func);
        builder.ins().call(close_ref, &[s.saved_copy]);
    }
}

/// Convert an IR Expr to an SsaValue for use in lower_value.
/// Used for redirect targets that contain runtime expressions (tilde expansion, etc).
pub(super) fn ir_expr_to_ssa_value(expr: &crate::ir::Expr) -> crate::ssa::SsaValue {
    use crate::ssa::{SsaValue, VarRef};
    match &expr.kind {
        crate::ir::ExprKind::Literal(s) | crate::ir::ExprKind::SingleQuoted(s) => {
            SsaValue::Literal(s.clone())
        }
        crate::ir::ExprKind::SimpleExpansion(v) => SsaValue::Var(VarRef::new(v, 0)),
        crate::ir::ExprKind::Concat(parts) | crate::ir::ExprKind::DoubleQuoted(parts) => {
            let vals: Vec<SsaValue> = parts.iter().map(ir_expr_to_ssa_value).collect();
            SsaValue::Concat(vals)
        }
        _ => SsaValue::Literal(String::new()),
    }
}

/// Scan an IR Expr (redirect target) for variable references.
pub(super) fn scan_redirect_for_vars(
    expr: &crate::ir::Expr,
    refs: &mut std::collections::HashSet<String>,
) {
    match &expr.kind {
        crate::ir::ExprKind::SimpleExpansion(v) => {
            refs.insert(v.clone());
        }
        crate::ir::ExprKind::Concat(parts) | crate::ir::ExprKind::DoubleQuoted(parts) => {
            for p in parts {
                scan_redirect_for_vars(p, refs);
            }
        }
        _ => {}
    }
}

/// Lower an IR Expr (from a redirect target) directly to Cranelift values,
/// bypassing SSA. Used for redirect targets that contain tilde-expanded paths.
pub(super) fn lower_ir_redirect_expr(
    expr: &crate::ir::Expr,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    match &expr.kind {
        crate::ir::ExprKind::Literal(s) | crate::ir::ExprKind::SingleQuoted(s) => {
            redirect_expr_literal(s, builder, module, literals)
        }
        crate::ir::ExprKind::SimpleExpansion(var) => {
            redirect_expr_simple_expansion(var, builder, vars)
        }
        crate::ir::ExprKind::Concat(parts) | crate::ir::ExprKind::DoubleQuoted(parts) => {
            redirect_expr_concat(parts, builder, module, vars, literals, externs)
        }
        _ => redirect_expr_zero(builder),
    }
}

fn redirect_expr_zero(
    builder: &mut FunctionBuilder,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    use cranelift_codegen::ir::types::I64;
    let z = builder.ins().iconst(I64, 0);
    (z, z)
}

fn redirect_expr_literal(
    s: &str,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    literals: &HashMap<String, cranelift_module::DataId>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    use cranelift_codegen::ir::types::I64;
    if let Some(&data_id) = literals.get(s) {
        let gv = module.declare_data_in_func(data_id, builder.func);
        let ptr = builder.ins().global_value(I64, gv);
        let len = builder.ins().iconst(I64, s.len() as i64);
        (ptr, len)
    } else {
        redirect_expr_zero(builder)
    }
}

fn redirect_expr_simple_expansion(
    var: &str,
    builder: &mut FunctionBuilder,
    vars: &HashMap<String, VarSlot>,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    if let Some(sv) = find_highest_version(vars, var) {
        (builder.use_var(sv.str_ptr()), builder.use_var(sv.str_len()))
    } else {
        redirect_expr_zero(builder)
    }
}

/// Find the highest-version VarSlot matching `var` as a base name.
/// Variable keys are encoded as either `name` (version 0) or `name_N`
/// (version N, where N is all-ASCII-digits).
fn find_highest_version<'a>(vars: &'a HashMap<String, VarSlot>, var: &str) -> Option<&'a VarSlot> {
    let mut best: Option<(u32, &'a VarSlot)> = None;
    for (k, sv) in vars {
        let (base, ver) = parse_var_base_version(k);
        if base == var && best.is_none_or(|(v, _)| ver > v) {
            best = Some((ver, sv));
        }
    }
    best.map(|(_, sv)| sv)
}

fn parse_var_base_version(k: &str) -> (&str, u32) {
    if let Some(idx) = k.rfind('_') {
        let suffix = &k[idx + 1..];
        if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
            return (&k[..idx], suffix.parse().unwrap_or(0));
        }
    }
    (k, 0)
}

fn redirect_expr_concat(
    parts: &[crate::ir::Expr],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    literals: &HashMap<String, cranelift_module::DataId>,
    externs: &ExternFuncs,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    use cranelift_codegen::ir::types::I64;
    assert!(!parts.is_empty());
    let (mut acc_ptr, mut acc_len) =
        lower_ir_redirect_expr(&parts[0], builder, module, vars, literals, externs);
    for part in &parts[1..] {
        let (p_ptr, p_len) = lower_ir_redirect_expr(part, builder, module, vars, literals, externs);
        let out_slot = builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
            cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
            8,
            3,
        ));
        let out_addr = builder.ins().stack_addr(I64, out_slot, 0);
        let concat_ref = module.declare_func_in_func(externs.concat, builder.func);
        let call = builder
            .ins()
            .call(concat_ref, &[acc_ptr, acc_len, p_ptr, p_len, out_addr]);
        acc_ptr = builder.inst_results(call)[0];
        acc_len = builder.ins().stack_load(I64, out_slot, 0);
    }
    (acc_ptr, acc_len)
}
