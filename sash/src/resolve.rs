//! DD#23: compile-time command resolution, hoisted ahead of the front-end.
//!
//! This pass walks the IR and replaces every *external* command head with
//! its resolved absolute path BEFORE the front-end checks (validate / taint
//! / type-check) run, so the taint pass classifies on the real exec form
//! (`/usr/bin/cat`, not bare `cat`). An external that cannot be resolved is
//! a FATAL compile error (SB5011, shared with `validate::check_commands`)
//! instead of the old runtime "bare command passed to sash_exec". This pass
//! also covers command-substitution heads, which `check_commands` does not.
//!
//! Left untouched: already-absolute paths, builtins (`BuiltinCmd::from_name`
//! != `Other`), compiler intrinsics that appear as command-substitution
//! heads (`$(getenv …)`, `$(argv …)`, …), and user-function calls.
//!
//! Resolution honors `--sysroot=<dir>` and `--sysroot=<image.squashfs>` via
//! `resolve_command_path` (the squashfs case reads `unsquashfs -l`).

use crate::diagnostic::Diagnostic;
use crate::ir::*;
use crate::ssa::BuiltinCmd;
use std::collections::HashSet;

/// Names that are NOT PATH-resolved externals even though `from_name`
/// returns `Other` for them: compiler intrinsics that surface as command
/// or command-substitution heads. Kept in sync with the intrinsic set in
/// `ssa_taint::cmd_subst_taint`.
const INTRINSICS: &[&str] = &[
    "getenv",
    "argv",
    "arg",
    "argc",
    "status",
    "len",
    "keys",
    "join",
    "epochseconds",
    "random",
    "getpid",
    "getppid",
    "getcpid",
    "getcwd",
    "signum",
    "dtoi",
    "itod",
];

/// Resolve every external command head in `program` to an absolute path.
/// Returns fatal diagnostics for any command that cannot be resolved.
pub(crate) fn resolve_commands(program: &mut Program) -> Vec<Diagnostic> {
    let mut funcs = HashSet::new();
    collect_functions(&program.statements, &mut funcs);
    if let Some(end) = &program.end_block {
        collect_functions(end, &mut funcs);
    }

    let mut diags = Vec::new();
    resolve_stmts(&mut program.statements, &funcs, &mut diags);
    if let Some(end) = &mut program.end_block {
        resolve_stmts(end, &funcs, &mut diags);
    }
    diags
}

/// DD#23: search PATH (honoring `--sysroot`, including a `.squashfs` image)
/// for `name`, returning its absolute path. Shared by this front-end pass
/// and the transform's `resolve_pipeline_command`.
pub(crate) fn resolve_command_path(name: &str) -> Option<String> {
    let sysroot = crate::get_sysroot();
    let path_var = std::env::var("PATH").unwrap_or_default();
    for dir in path_var.split(':') {
        let target_path = format!("{}/{}", dir, name);
        let exists = if let Some(ref sr) = sysroot {
            crate::sysroot_file_exists(sr, &target_path)
        } else {
            std::path::Path::new(&target_path).is_file()
        };
        if exists {
            return Some(target_path);
        }
    }
    None
}

/// True if `name` is a true external command head that must be resolved to
/// an absolute path (not a path already, not a builtin, not an intrinsic,
/// not a user function).
fn is_resolvable_external(name: &str, funcs: &HashSet<String>) -> bool {
    if name.is_empty() || name.contains('/') {
        return false;
    }
    if INTRINSICS.contains(&name) || funcs.contains(name) {
        return false;
    }
    matches!(BuiltinCmd::from_name(name), BuiltinCmd::Other)
}

fn collect_functions(stmts: &[Statement], funcs: &mut HashSet<String>) {
    for stmt in stmts {
        if let StmtKind::Function(f) = &stmt.kind {
            funcs.insert(f.name.clone());
        }
        for body in stmt.child_bodies() {
            collect_functions(body, funcs);
        }
    }
}

fn resolve_stmts(stmts: &mut [Statement], funcs: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    for stmt in stmts {
        resolve_stmt(stmt, funcs, diags);
    }
}

fn resolve_stmt(stmt: &mut Statement, funcs: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    // Resolve the expressions owned directly by this statement (command
    // heads/args, assignment values, for-words, case patterns, redirect
    // targets); nested statement bodies are handled by the recursion below.
    resolve_stmt_exprs(stmt, funcs, diags);
    for body in stmt.child_bodies_mut() {
        resolve_stmts(body, funcs, diags);
    }
}

fn resolve_stmt_exprs(stmt: &mut Statement, funcs: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    match &mut stmt.kind {
        StmtKind::Command(c) => resolve_command(c, funcs, diags),
        StmtKind::Assignment(a) => resolve_assignment(a, funcs, diags),
        StmtKind::Declaration(d) => {
            for a in &mut d.assignments {
                resolve_assignment(a, funcs, diags);
            }
        }
        StmtKind::For(f) => {
            for w in &mut f.words {
                resolve_expr(w, funcs, diags);
            }
        }
        StmtKind::Case(c) => resolve_case_exprs(c, funcs, diags),
        StmtKind::Redirected(r) => {
            for redir in &mut r.redirects {
                resolve_expr(&mut redir.target, funcs, diags);
            }
        }
        _ => {}
    }
}

fn resolve_case_exprs(c: &mut Case, funcs: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    resolve_expr(&mut c.value, funcs, diags);
    for item in &mut c.items {
        for p in &mut item.patterns {
            resolve_expr(p, funcs, diags);
        }
    }
}

fn resolve_assignment(a: &mut Assignment, funcs: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    if let Some(idx) = &mut a.index {
        resolve_expr(idx, funcs, diags);
    }
    resolve_expr(&mut a.value, funcs, diags);
}

fn resolve_command(c: &mut Command, funcs: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    // Resolve command substitutions nested in the head and arguments first.
    resolve_expr(&mut c.name, funcs, diags);
    for arg in &mut c.args {
        resolve_expr(arg, funcs, diags);
    }
    // Then resolve the head itself if it is a plain external literal.
    let ExprKind::Literal(name) = &c.name.kind else {
        return;
    };
    if !is_resolvable_external(name, funcs) {
        return;
    }
    match resolve_command_path(name) {
        Some(abs) => c.name.kind = ExprKind::Literal(abs),
        None => diags.push(Diagnostic::error_span(
            c.name.span,
            "SB5011",
            format!(
                "command '{}' not found — use absolute path or define as function",
                name
            ),
        )),
    }
}

fn resolve_expr(e: &mut Expr, funcs: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    match &mut e.kind {
        ExprKind::CommandSubstitution(stmts) => resolve_stmts(stmts, funcs, diags),
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) | ExprKind::Array(parts) => {
            for p in parts {
                resolve_expr(p, funcs, diags);
            }
        }
        ExprKind::AssocArray(pairs) => {
            for (_, v) in pairs {
                resolve_expr(v, funcs, diags);
            }
        }
        ExprKind::Subscript { index, .. } => resolve_expr(index, funcs, diags),
        ExprKind::ParameterExpansion(pe) => {
            if let Some(operand) = &mut pe.operand {
                resolve_expr(operand, funcs, diags);
            }
        }
        ExprKind::Test(t) => resolve_test_expr(t, funcs, diags),
        ExprKind::Literal(_)
        | ExprKind::SingleQuoted(_)
        | ExprKind::SimpleExpansion(_)
        | ExprKind::Arithmetic(_) => {}
    }
}

fn resolve_test_expr(t: &mut TestExpr, funcs: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    match t {
        TestExpr::Unary { operand, .. } => resolve_expr(operand, funcs, diags),
        TestExpr::Binary { left, right, .. } => {
            resolve_expr(left, funcs, diags);
            resolve_expr(right, funcs, diags);
        }
        TestExpr::Not(inner) => resolve_test_expr(inner, funcs, diags),
        TestExpr::And(l, r) | TestExpr::Or(l, r) => {
            resolve_test_expr(l, funcs, diags);
            resolve_test_expr(r, funcs, diags);
        }
    }
}
