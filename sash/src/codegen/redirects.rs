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
/// open redirect is present (dup only).
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
pub(super) fn default_fd_for_op(op: &crate::ir::RedirectOp) -> u32 {
    use crate::ir::RedirectOp;
    match op {
        RedirectOp::Write | RedirectOp::Append | RedirectOp::DupOutput => 1,
        // `<>` is a `<` form: bash defaults it to stdin, not stdout.
        RedirectOp::Read | RedirectOp::DupInput | RedirectOp::ReadWrite => 0,
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
/// values); `DupOutput` gets its own emitter. Unknown ops are no-ops
/// (the save still happened). Returns `Some((failed, path_ptr, path_len))`
/// for a file `open(2)` redirect (DD#55 setup-failure surfacing); `None`
/// for dup/noop.
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
    // `>&N` and `<&N` are the same operation — dup2(N, fd). They differ only
    // in which fd they default to, which `default_fd_for_op` already handles.
    if matches!(redir.op, RedirectOp::DupOutput | RedirectOp::DupInput) {
        emit_dup_fd(fd_val, &redir.target, builder, module, externs);
    }
    None
}

/// open(2) flags for the four open-and-dup2 redirect ops.
/// (1 = O_WRONLY|O_CREAT|O_TRUNC, 2 = O_WRONLY|O_CREAT|O_APPEND,
/// 0 = O_RDONLY, 3 = O_RDWR|O_CREAT — interpreted by the
/// `redirect`/`redirect_s` runtime.)
fn open_flags_for_op(op: &crate::ir::RedirectOp) -> Option<i64> {
    use crate::ir::RedirectOp;
    match op {
        RedirectOp::Write => Some(1),
        RedirectOp::Append => Some(2),
        RedirectOp::Read => Some(0),
        RedirectOp::ReadWrite => Some(3),
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

/// `>&N` / `<&N` — dup2(target_fd, fd). Non-literal targets default to fd 2
/// (cov.86-88 reject a non-numeric fd-dup target, so that arm is defensive).
fn emit_dup_fd(
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
        other => redirect_target_ice(&format!("unexpected non-scalar target kind {other:?}")),
    }
}

/// A redirect target that couldn't be resolved to a `(ptr, len)`. Every path
/// that reaches here is guarded upstream — a non-scalar target is rejected at
/// parse (SB4049), a `${x:-f}` / `$a[i]` target is spilled to a scalar in the
/// transform, an undeclared `$f` is rejected by validate (SB5031), and a
/// literal target is always interned — so arriving here is a compiler-invariant
/// violation. Raise a loud ICE (caught by `catch_unwind` in
/// `compile_to_object`, surfaced as a clean error) instead of the old silent
/// `(0, 0)`, which redirected to an EMPTY filename
/// (bug-redirect-nonsimple-target-empty class).
fn redirect_target_ice(reason: &str) -> ! {
    panic!("ICE: unresolved redirect target — {reason}");
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
        redirect_target_ice(&format!("literal {s:?} not interned in the literals map"))
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
        redirect_target_ice(&format!("variable ${var} has no slot"))
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

#[cfg(test)]
mod tests {
    use super::*;
    use cranelift_codegen::ir::Function;
    use cranelift_frontend::FunctionBuilderContext;

    // `redirect_expr_simple_expansion` on an absent variable takes the
    // find_highest_version → None branch, which is a loud ICE (not a silent
    // empty target). The condition is unreachable in real compiles — validate
    // rejects an undeclared `$f` with SB5031 — so a unit test is the only way to
    // exercise the guard. Confirms it panics rather than returning (0, 0).
    #[test]
    fn unresolved_redirect_var_is_loud_ice() {
        let mut fbctx = FunctionBuilderContext::new();
        let mut func = Function::new();
        let mut builder = FunctionBuilder::new(&mut func, &mut fbctx);
        let entry = builder.create_block();
        builder.switch_to_block(entry);
        let vars: HashMap<String, VarSlot> = HashMap::new();
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            redirect_expr_simple_expansion("nonexistent_var", &mut builder, &vars)
        }));
        assert!(
            caught.is_err(),
            "an unresolved redirect-target variable must raise a loud ICE"
        );
    }
}
