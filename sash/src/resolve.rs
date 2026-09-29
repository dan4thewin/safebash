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
    "strlen",
    "keys",
    "join",
    "epochseconds",
    "random",
    "getpid",
    "getppid",
    "getcpid",
    "getcwd",
    "signum",
];

/// True for any sash-native intrinsic — the `BuiltinCmd` set plus the
/// command-head-only names in [`INTRINSICS`]. The single source of truth for
/// "is this name the compiler's, rather than a PATH lookup or a user
/// function"; `validate` uses it to decide whether an arith sexpr `(name …)`
/// names anything at all.
pub(crate) fn is_intrinsic(name: &str) -> bool {
    BuiltinCmd::from_name(name) != BuiltinCmd::Other || INTRINSICS.contains(&name)
}

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
    // DD#23: only search absolute PATH dirs. A relative dir (`.`, `..`, an
    // empty entry = cwd) would yield a path that binds to the runtime cwd —
    // the "search ambiguity" DD#23 forbids. A command found only via such a
    // dir stays unresolved (→ SB5011).
    for dir in path_var.split(':').filter(|d| d.starts_with('/')) {
        let target_path = format!("{}/{}", dir, name);
        if resolved_path_exists(&sysroot, &target_path) {
            return Some(target_path);
        }
    }
    None
}

/// Does `target_path` name an existing file — within `--sysroot` (dir or
/// squashfs image) when set, else on the build host?
fn resolved_path_exists(sysroot: &Option<String>, target_path: &str) -> bool {
    if let Some(sr) = sysroot {
        crate::sysroot_file_exists(sr, target_path)
    } else {
        std::path::Path::new(target_path).is_file()
    }
}

/// True if `name` is a true external command head that must be resolved to
/// an absolute path (not a path already, not a builtin, not an intrinsic,
/// not a user function).
fn is_resolvable_external(name: &str, funcs: &HashSet<String>) -> bool {
    // No `name.is_empty()` arm: an empty head cannot be an `ExprKind::Literal`.
    // `''` and `""` lex as quoted forms, not literals, and the word scanner
    // never emits a zero-length unquoted word — so the arm measured 0 hits and
    // bought nothing, since an empty name would fall through to SB5011
    // "command '' not found" anyway.
    if name.contains('/') {
        return false;
    }
    if INTRINSICS.contains(&name) || funcs.contains(name) {
        return false;
    }
    matches!(BuiltinCmd::from_name(name), BuiltinCmd::Other)
}

/// A command head that is (or resolves to) an EXTERNAL program — i.e. NOT a
/// sash intrinsic and NOT a language builtin. An absolute path (`/bin/x`) or a
/// bare name that is neither an intrinsic nor a builtin qualifies. Unlike
/// [`is_resolvable_external`] this does NOT consult the function table, so a
/// user function's bare name also passes here — callers that must exclude user
/// functions (which have the function table) do so themselves. Used by the
/// capture-condition check (DD#88), which runs in `validate` BEFORE resolution
/// (names are still bare) and defers the user-function exclusion to the
/// transform.
pub(crate) fn is_external_looking(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    if name.contains('/') {
        return true;
    }
    !INTRINSICS.contains(&name) && matches!(BuiltinCmd::from_name(name), BuiltinCmd::Other)
}

fn collect_functions(stmts: &[Statement], funcs: &mut HashSet<String>) {
    for stmt in stmts {
        if let StmtKind::Function(f) = &stmt.kind {
            funcs.insert(f.name.clone());
        }
        for body in stmt.structural_bodies() {
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
    for body in stmt.structural_bodies_mut() {
        resolve_stmts(body, funcs, diags);
    }
}

fn resolve_stmt_exprs(stmt: &mut Statement, funcs: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    match &mut stmt.kind {
        StmtKind::Command(c) => resolve_command(c, funcs, diags),
        StmtKind::Assignment(a) => resolve_assignment(a, funcs, diags),
        StmtKind::Declaration(d) => resolve_declaration(d, funcs, diags),
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

/// Resolve command substitutions inside each initializer of a declaration.
fn resolve_declaration(d: &mut Declaration, funcs: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    for a in &mut d.assignments {
        if let Some(v) = &mut a.value {
            resolve_expr(v, funcs, diags);
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
    //
    // The `else` is unreachable, and measurably so: 8221 commands reached here
    // during a full sweep and not one had a non-`Literal` head. `validate`
    // rejects those with SB5011 ("a command name must be a literal path"), and
    // it runs in `run_structural_checks`, which exits before `resolve_commands`
    // is called at all. That ordering is deliberate (see the comment on the two
    // calls in `cli_main`), so this stays a plain `return` rather than an
    // assertion: the guarantee lives in a different pass, and an ICE here would
    // encode a cross-pass ordering that this file cannot enforce.
    let ExprKind::Literal(name) = &c.name.kind else {
        return;
    };
    // DD#23: an external command path must be absolute. A relative path head
    // (`./x`, `../x`, `a/b`) would bind to the runtime cwd — the "search
    // ambiguity" DD#23 forbids. Reject it. (An absolute `/x` path contains a
    // slash too but is left as-is by `is_resolvable_external` below.)
    if name.contains('/') && !name.starts_with('/') {
        diags.push(Diagnostic::error_span(
            c.name.span,
            "SB5051",
            format!(
                "relative external path '{}' — DD#23 requires an absolute path (starting with '/')",
                name
            ),
        ));
        return;
    }
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
            resolve_each(parts.iter_mut(), funcs, diags)
        }
        ExprKind::AssocArray(pairs) => resolve_each(pairs.iter_mut().map(|(_, v)| v), funcs, diags),
        ExprKind::Subscript { index, .. } | ExprKind::SlurpFile(index) => {
            resolve_expr(index, funcs, diags)
        }
        ExprKind::ParameterExpansion(pe) => {
            resolve_each(pe.operand.iter_mut().map(|b| &mut **b), funcs, diags)
        }
        ExprKind::Test(t) => resolve_test_expr(t, funcs, diags),
        ExprKind::Literal(_)
        | ExprKind::SingleQuoted(_)
        | ExprKind::SimpleExpansion(_)
        | ExprKind::Arithmetic(_) => {}
    }
}

/// Resolve every expression a form holds, whatever the shape of its
/// container — a slice, the values of a pair list, or an `Option`.
fn resolve_each<'a>(
    exprs: impl Iterator<Item = &'a mut Expr>,
    funcs: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    for e in exprs {
        resolve_expr(e, funcs, diags);
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
