//! File descriptor redirection: save, apply, restore.
//!
//! SECURITY: Only fd manipulation via dup/dup2/close. No filename globbing.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types::I64;
use cranelift_codegen::ir::InstBuilder;
use cranelift_codegen::ir::Value;
use cranelift_codegen::ir::{StackSlotData, StackSlotKind};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::Module;
use cranelift_object::ObjectModule;
use std::collections::HashMap;

use super::types::{emit_int_to_str, ExternFuncs, VarSlot};
use super::values::lower_literal_value;

#[derive(Clone)]
pub(super) struct SavedFd {
    pub(super) original_fd: i64, // the fd that was redirected (e.g. 1, 2)
    pub(super) saved_copy: cranelift_codegen::ir::Value, // dup'd copy to restore from
}

/// DD#55: a redirect setup-failure surfaced to the command
/// handler. `failed` is 1 when a redirect failed (else 0);
/// `path_ptr`/`path_len` name the **first redirect that actually failed**,
/// which is the path DD#70's `<argv0>: <path>: <reason>` contract wants.
#[derive(Clone, Copy)]
pub(super) struct RedirectFailure {
    pub(super) failed: Value,
    pub(super) path_ptr: Value,
    pub(super) path_len: Value,
}

/// Apply redirections and return a [`RedirectFailure`] when at
/// least one redirect is present, so the
/// caller can apply the DD#55 setup-failure semantics (skip body, fail
/// status, abort governed by context). Returns `None` failure when no
/// redirect is present.
pub(super) fn emit_redirects_guarded(
    redirects: &[crate::ir::Redirect],
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    literals: &HashMap<String, cranelift_module::DataId>,
    vars: &HashMap<String, VarSlot>,
) -> (Vec<SavedFd>, Option<RedirectFailure>) {
    if redirects.is_empty() {
        return (Vec::new(), None);
    }
    let mut saved = Vec::new();
    let mut acc: Option<RedirectFailure> = None;
    let fds = crate::ir::redirect_referenced_fds(redirects);
    let size = u32::try_from(fds.len() * 8).expect("redirect descriptor table fits a stack slot");
    let slot =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, size, 3));
    for (i, fd) in fds.iter().enumerate() {
        let value = builder.ins().iconst(I64, *fd);
        let offset = i32::try_from(i * 8).expect("redirect descriptor offset fits a stack slot");
        builder.ins().stack_store(value, slot, offset);
    }
    let protected = builder.ins().stack_addr(I64, slot, 0);
    let count = builder.ins().iconst(I64, fds.len() as i64);
    for redir in redirects {
        let fd = redir.fd.unwrap_or(redir.op.default_fd()) as i64;
        let fd_val = builder.ins().iconst(I64, fd);
        saved.push(emit_save_fd(
            fd, fd_val, protected, count, builder, module, externs,
        ));
        // Bash performs redirects left to right and STOPS at the first failed
        // open(2). Once one has failed, a later redirect must not run: an
        // eager `open` creates or truncates a file bash never touches, so
        // `cmd > /bad > keep.txt` used to clobber `keep.txt`. Guarding with
        // real control flow (the `native-ternary-eager-eval` precedent) also
        // makes the reported path right *by construction* — only the first
        // failure survives, so there is no "which path?" left to get wrong.
        acc = Some(emit_next_redirect(
            acc, redir, fd_val, builder, module, externs, literals, vars,
        ));
    }
    (saved, acc)
}

/// Emit the next redirect in the list, guarded by any failure before it.
///
/// With no prior failure this is a plain `emit_one_redirect`; with one, the
/// redirect is wrapped in the skip/do branch so it never runs. Returns the
/// failure that survives. Every redirect, including a dup, can fail.
#[allow(clippy::too_many_arguments)]
fn emit_next_redirect(
    prev: Option<RedirectFailure>,
    redir: &crate::ir::Redirect,
    fd_val: Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    literals: &HashMap<String, cranelift_module::DataId>,
    vars: &HashMap<String, VarSlot>,
) -> RedirectFailure {
    if let Some(prev) = prev {
        return emit_guarded_redirect(
            prev, redir, fd_val, builder, module, externs, literals, vars,
        );
    }
    let (failed, path_ptr, path_len) =
        emit_one_redirect(redir, fd_val, builder, module, externs, literals, vars);
    RedirectFailure {
        failed,
        path_ptr,
        path_len,
    }
}

/// Emit `redir` only when no earlier open has failed.
///
/// `brif` on `prev.failed` into a do-block (which performs the redirect) or a
/// skip-block (which performs nothing), merging the outcome through a
/// three-wide block-param phi. Taking the skip path carries `prev`'s triple
/// through unchanged, so the surviving `path_ptr`/`path_len` always name the
/// **first** redirect that failed.
///
#[allow(clippy::too_many_arguments)]
fn emit_guarded_redirect(
    prev: RedirectFailure,
    redir: &crate::ir::Redirect,
    fd_val: Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    literals: &HashMap<String, cranelift_module::DataId>,
    vars: &HashMap<String, VarSlot>,
) -> RedirectFailure {
    use cranelift_codegen::ir::BlockArg;

    let do_blk = builder.create_block();
    let skip_blk = builder.create_block();
    let merge = builder.create_block();
    let m_failed = builder.append_block_param(merge, I64);
    let m_ptr = builder.append_block_param(merge, I64);
    let m_len = builder.append_block_param(merge, I64);

    builder.ins().brif(prev.failed, skip_blk, &[], do_blk, &[]);

    builder.switch_to_block(do_blk);
    builder.seal_block(do_blk);
    let (failed, path_ptr, path_len) =
        emit_one_redirect(redir, fd_val, builder, module, externs, literals, vars);
    builder.ins().jump(
        merge,
        &[
            BlockArg::Value(failed),
            BlockArg::Value(path_ptr),
            BlockArg::Value(path_len),
        ],
    );

    builder.switch_to_block(skip_blk);
    builder.seal_block(skip_blk);
    builder.ins().jump(
        merge,
        &[
            BlockArg::Value(prev.failed),
            BlockArg::Value(prev.path_ptr),
            BlockArg::Value(prev.path_len),
        ],
    );

    builder.switch_to_block(merge);
    builder.seal_block(merge);
    RedirectFailure {
        failed: m_failed,
        path_ptr: m_ptr,
        path_len: m_len,
    }
}

/// dup() the live fd so it can be restored later, and record the pairing.
#[allow(clippy::too_many_arguments)]
fn emit_save_fd(
    fd: i64,
    fd_val: cranelift_codegen::ir::Value,
    protected: Value,
    count: Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
) -> SavedFd {
    let dup_ref = module.declare_func_in_func(externs.save_fd, builder.func);
    let dup_call = builder.ins().call(dup_ref, &[fd_val, protected, count]);
    let saved_copy = builder.inst_results(dup_call)[0];
    SavedFd {
        original_fd: fd,
        saved_copy,
    }
}

/// Both file opens and descriptor duplication can fail during setup.
fn emit_one_redirect(
    redir: &crate::ir::Redirect,
    fd_val: cranelift_codegen::ir::Value,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
    literals: &HashMap<String, cranelift_module::DataId>,
    vars: &HashMap<String, VarSlot>,
) -> (Value, Value, Value) {
    if let Some(flags) = redir.op.open_flags() {
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
    // in which fd they default to.
    let result = emit_dup_fd(fd_val, &redir.target, builder, module, externs);
    let (ptr, len) =
        lower_ir_redirect_expr(&redir.target, builder, module, vars, literals, externs);
    let failed = builder.ins().icmp_imm(IntCC::SignedLessThan, result, 0);
    let failed = builder.ins().uextend(I64, failed);
    (failed, ptr, len)
}

/// `> file` / `>> file` / `< file`. Literal path → fast `redirect`;
/// dynamic (tilde, var, concat) → `redirect_s` with computed ptr+len.
/// Returns `(failed, path_ptr, path_len)`: `failed` is 1 when the runtime
/// `open(2)` returned < 0 (DD#55 setup-failure), else 0.
///
/// A literal target absent from the literals map used to be **silently
/// skipped** here — the redirect simply did not happen and the program ran on
/// with its output somewhere else, which turned a missed invariant into wrong
/// output rather than a crash. It is what made the dropped-capture-redirect
/// bug silent. Interning every redirect target is a compiler invariant, so a
/// miss is an ICE, exactly as it already is for a non-literal target
/// (`redirect_target_ice`).
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
) -> (Value, Value, Value) {
    let flags_val = builder.ins().iconst(I64, flags);
    let (path_ptr, path_len, result) = if let crate::ir::ExprKind::Literal(s) = &target.kind {
        let (path_ptr, path_len) = lower_literal_value(s, builder, module, literals);
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
    (failed, path_ptr, path_len)
}

/// `>&N` / `<&N` — malformed operands must not silently become fd 2.
fn emit_dup_fd(
    fd_val: cranelift_codegen::ir::Value,
    target: &crate::ir::Expr,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
) -> Value {
    let target_fd = crate::ir::validated_fd_number(target);
    let target_val = builder
        .ins()
        .iconst(cranelift_codegen::ir::types::I64, target_fd);
    let dup2_ref = module.declare_func_in_func(externs.dup2, builder.func);
    let call = builder.ins().call(dup2_ref, &[target_val, fd_val]);
    builder.inst_results(call)[0]
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
        let restore_ref = module.declare_func_in_func(externs.restore_fd, builder.func);
        builder.ins().call(restore_ref, &[s.saved_copy, fd_val]);
    }
}

pub(super) fn emit_discard_saved(
    saved: Vec<SavedFd>,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    externs: &ExternFuncs,
) {
    let close_ref = module.declare_func_in_func(externs.close, builder.func);
    for s in saved {
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
            lower_literal_value(s, builder, module, literals)
        }
        crate::ir::ExprKind::SimpleExpansion(var) => {
            redirect_expr_simple_expansion(var, builder, module, vars, externs)
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
/// transform, and an undeclared `$f` is rejected by validate (SB5031) —
/// so arriving here is a compiler-invariant
/// violation. Raise a loud ICE (caught by `catch_unwind` in
/// `compile_to_object`, surfaced as a clean error) instead of the old silent
/// `(0, 0)`, which redirected to an EMPTY filename
/// (bug-redirect-nonsimple-target-empty class).
fn redirect_target_ice(reason: &str) -> ! {
    panic!("ICE: unresolved redirect target — {reason}");
}

fn redirect_expr_simple_expansion(
    var: &str,
    builder: &mut FunctionBuilder,
    module: &mut ObjectModule,
    vars: &HashMap<String, VarSlot>,
    externs: &ExternFuncs,
) -> (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value) {
    match redirect_slot_or_ice(var, vars) {
        VarSlot::Str { .. } => {
            let sv = redirect_slot_or_ice(var, vars);
            (sv.str_ptr().load(builder), sv.str_len().load(builder))
        }
        // An int variable in a target is a path *component*, not a number:
        // `> "$p".log`. The braced spelling `${p}` is spilled to a scalar temp
        // by the transform and so has always worked; the brace-less one arrives
        // here raw and used to panic on `str_ptr`. Render it, matching both the
        // braced form and `echo $p`. DD#119 rejects the *unquoted* int target
        // upstream, so what reaches here is the converted form.
        VarSlot::Int(v) => {
            let n = v.load(builder);
            emit_int_to_str(builder, module, externs, n)
        }
    }
}

/// The slot behind a redirect-target variable. Absent is a loud ICE, not a
/// silent `(0, 0)` empty target (bug-redirect-nonsimple-target-empty class).
/// Split out of `redirect_expr_simple_expansion` so the guard stays unit
/// testable without standing up an `ObjectModule`.
fn redirect_slot_or_ice<'a>(var: &str, vars: &'a HashMap<String, VarSlot>) -> &'a VarSlot {
    match find_highest_version(vars, var) {
        Some(sv) => sv,
        None => redirect_target_ice(&format!("variable ${var} has no slot")),
    }
}

/// Find the highest-version VarSlot matching `var` as a base name.
/// Every key is `name_N`: they are built by `VarRef::display()`, which is
/// `format!("{}_{}", name, version)` over a `u32`, so version 0 is spelled
/// `name_0` and never bare `name`. (Both this and its former twin in
/// `inst_handlers.rs` documented a bare-`name` spelling that nothing emits.)
/// A base name may itself end in `_N` — `rfind` takes the last `_`, which is
/// always the one `display` appended, so `x_2` at v0 keys as `x_2_0` and
/// stays distinct from `x` at v2.
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

pub(crate) fn parse_var_base_version(k: &str) -> (&str, u32) {
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

    // `redirect_slot_or_ice` on an absent variable takes the
    // find_highest_version → None branch, which is a loud ICE (not a silent
    // empty target). The condition is unreachable in real compiles — validate
    // rejects an undeclared `$f` with SB5031 — so a unit test is the only way to
    // exercise the guard. Confirms it panics rather than returning (0, 0).
    #[test]
    fn unresolved_redirect_var_is_loud_ice() {
        let vars: HashMap<String, VarSlot> = HashMap::new();
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            redirect_slot_or_ice("nonexistent_var", &vars)
        }));
        assert!(
            caught.is_err(),
            "an unresolved redirect-target variable must raise a loud ICE"
        );
    }
}

/// [`parse_var_base_version`]'s fallback path — the `(k, 0)` return taken
/// when a key carries no `_<digits>` suffix. Cold because every key in the
/// slot map is built with one, so it is reachable only by direct call.
#[cfg(test)]
mod parse_var_base_version_tests {
    use super::parse_var_base_version;

    #[test]
    fn a_versioned_key_splits_into_base_and_version() {
        assert_eq!(parse_var_base_version("count_7"), ("count", 7));
        // Documented at the site: a base ending in `_N` stays distinct —
        // `x_2` at v0 keys as `x_2_0`, not as `x` at v2.
        assert_eq!(parse_var_base_version("x_2_0"), ("x_2", 0));
    }

    #[test]
    fn a_key_with_no_version_suffix_reads_as_version_zero() {
        assert_eq!(parse_var_base_version("count"), ("count", 0));
        assert_eq!(parse_var_base_version("my_var"), ("my_var", 0));
        // The `is_empty` guard: `"".chars().all(..)` is vacuously true, so
        // without it this would strip to `("x", 0)`. See the companion test
        // in `codegen::values`, where the guard is absent.
        assert_eq!(parse_var_base_version("x_"), ("x_", 0));
    }

    /// `find_highest_version` compares versions numerically, so a suffix too
    /// large for `u32` must not silently read as a *high* version. It reads
    /// as 0 — the lowest — which loses to any real version rather than
    /// beating all of them.
    #[test]
    fn an_unparseable_numeric_suffix_falls_back_to_zero() {
        assert_eq!(parse_var_base_version("x_99999999999999999999"), ("x", 0));
    }
}
