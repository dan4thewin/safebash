//! File descriptor redirection: save, apply, restore.
//!
//! SECURITY: Only fd manipulation via dup/dup2/close. No filename globbing.

use cranelift_codegen::ir::InstBuilder;
use cranelift_frontend::{FunctionBuilder, Variable};
use cranelift_module::Module;
use cranelift_object::ObjectModule;
use std::collections::HashMap;

use super::types::{ExternFuncs, VarSlot};

pub(super) struct SavedFd {
    pub(super) original_fd: i64, // the fd that was redirected (e.g. 1, 2)
    pub(super) saved_copy: cranelift_codegen::ir::Value, // dup'd copy to restore from
}

/// Emit fd redirections before a command. Returns saved state for restore.
pub(super) fn emit_redirects(
    redirects: &[crate::ir::Redirect],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    literals: &HashMap<String, cranelift_module::DataId>,
    vars: &HashMap<String, VarSlot>,
    #[allow(unused_variables)] _exit_status_var: Option<Variable>,
) -> Vec<SavedFd> {
    let mut saved = Vec::new();
    for redir in redirects {
        let fd = redir.fd.unwrap_or(default_fd_for_op(&redir.op)) as i64;
        let fd_val = builder.ins().iconst(cranelift_codegen::ir::types::I64, fd);
        saved.push(emit_save_fd(fd, fd_val, builder, module, externs));
        emit_one_redirect(redir, fd_val, builder, module, externs, literals, vars);
    }
    saved
}

/// Bash default fd: stdout for write-ish ops, stdin for read-ish ops.
fn default_fd_for_op(op: &crate::ir::RedirectOp) -> u32 {
    use crate::ir::RedirectOp;
    match op {
        RedirectOp::Write | RedirectOp::Append | RedirectOp::DupOutput => 1,
        RedirectOp::Read | RedirectOp::DupInput | RedirectOp::HereDoc | RedirectOp::HereString => 0,
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
/// values); `DupOutput` and `HereDoc`/`HereString` get their own
/// emitters. Unknown ops are no-ops (the save still happened).
fn emit_one_redirect(
    redir: &crate::ir::Redirect,
    fd_val: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    literals: &HashMap<String, cranelift_module::DataId>,
    vars: &HashMap<String, VarSlot>,
) {
    use crate::ir::RedirectOp;
    if let Some(flags) = open_flags_for_op(&redir.op) {
        emit_open_redirect(
            fd_val,
            &redir.target,
            flags,
            builder,
            module,
            externs,
            literals,
            vars,
        );
        return;
    }
    match redir.op {
        RedirectOp::DupOutput => {
            emit_dup_output(fd_val, &redir.target, builder, module, externs);
        }
        RedirectOp::HereDoc | RedirectOp::HereString => {
            emit_heredoc_redirect(
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
/// skipped (matches pre-refactor behavior).
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
) {
    let flags_val = builder
        .ins()
        .iconst(cranelift_codegen::ir::types::I64, flags);
    if let crate::ir::ExprKind::Literal(s) = &target.kind {
        if let Some(&data_id) = literals.get(s.as_str()) {
            let path_gv = module.declare_data_in_func(data_id, builder.func);
            let path_ptr = builder
                .ins()
                .global_value(cranelift_codegen::ir::types::I64, path_gv);
            let redirect_ref = module.declare_func_in_func(externs.redirect, builder.func);
            builder
                .ins()
                .call(redirect_ref, &[fd_val, path_ptr, flags_val]);
        }
    } else {
        let (ptr, len) = lower_ir_redirect_expr(target, builder, module, vars, literals, externs);
        let redirect_s_ref = module.declare_func_in_func(externs.redirect_s, builder.func);
        builder
            .ins()
            .call(redirect_s_ref, &[fd_val, ptr, len, flags_val]);
    }
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
fn emit_heredoc_redirect(
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
    let heredoc_ref = module.declare_func_in_func(externs.heredoc_redirect, builder.func);
    builder
        .ins()
        .call(heredoc_ref, &[fd_val, body_ptr, body_len]);
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
                    let suffix = &k[idx + 1..];
                    if suffix.chars().all(|c| c.is_ascii_digit()) {
                        &k[..idx]
                    } else {
                        k.as_str()
                    }
                } else {
                    k.as_str()
                };
                if base == var {
                    let ver = k
                        .rfind('_')
                        .and_then(|i| k[i + 1..].parse::<u32>().ok())
                        .unwrap_or(0);
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
            let (mut acc_ptr, mut acc_len) =
                lower_ir_redirect_expr(&parts[0], builder, module, vars, literals, externs);
            for part in &parts[1..] {
                let (p_ptr, p_len) =
                    lower_ir_redirect_expr(part, builder, module, vars, literals, externs);
                let out_slot =
                    builder.create_sized_stack_slot(cranelift_codegen::ir::StackSlotData::new(
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
        _ => {
            let z = builder.ins().iconst(I64, 0);
            (z, z)
        }
    }
}
