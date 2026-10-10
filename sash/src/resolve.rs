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

/// What a name the compiler owns denotes in **arithmetic** — as a DD#58
/// sexpr callee `(name …)` or a bare word in `$(( … ))`.
///
/// Arithmetic is the one context where a compiler name is not a command but
/// an *operand*, so every such name has to answer for itself here. Nothing
/// made them: the arith type rule carried a hand-written deny-list of string
/// intrinsics and treated every other compiler name as a valid int producer.
/// Nineteen names were therefore accepted and lowered as reads of an unset
/// variable — `$(( (echo) ))`, `(cd)`, `(read)`, `(break)` and the rest were
/// each a silent `0`. Five of the nineteen (`write`, `readline`, `mapfile`,
/// `attest`, `matchall`) were `BuiltinCmd` variants added *after* the
/// deny-list was written, which is the failure a deny-list guarantees.
///
/// This table is keyed on the `BuiltinCmd` enum instead, so a new builtin
/// does not compile until someone says what it denotes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArithDenotation {
    /// Yields an i64 — legal as an arithmetic operand.
    Int,
    /// Yields a string — it belongs in a `$( )` capture, not in `$(( ))`.
    Str,
    /// Denotes no value at all: a statement.
    Statement,
}

/// Names that are NOT PATH-resolved externals even though `from_name`
/// returns `Other` for them: compiler intrinsics that surface as command
/// or command-substitution heads, each with what it denotes in arithmetic.
/// Kept in sync with the intrinsic set in `ssa_taint::cmd_subst_taint`.
///
/// `argv`, `len`, `strlen`, `keys` and `join` were listed here too, in
/// contradiction of the sentence above — `from_name` recognizes all five.
/// The duplication was harmless while every reader asked only "is this name
/// the compiler's", because both spellings answer yes; it became a hazard
/// the moment a name had to carry a *denotation*, which two tables can
/// disagree about. All five are classified once, on the `BuiltinCmd` side.
const INTRINSICS: &[(&str, ArithDenotation)] = &[
    ("getenv", ArithDenotation::Str),
    ("arg", ArithDenotation::Str),
    ("argc", ArithDenotation::Int),
    ("status", ArithDenotation::Int),
    ("epochseconds", ArithDenotation::Int),
    ("random", ArithDenotation::Int),
    ("getpid", ArithDenotation::Int),
    ("getppid", ArithDenotation::Int),
    ("getcpid", ArithDenotation::Int),
    ("waitstatus", ArithDenotation::Int),
    ("getcwd", ArithDenotation::Str),
    ("signum", ArithDenotation::Int),
];

/// What each [`BuiltinCmd`] denotes in arithmetic.
///
/// **Total over `BuiltinCmd` on purpose — do not add a wildcard.** The
/// classification a new builtin needs is exactly the one the old deny-list
/// let it skip.
///
/// `Other` is a real answer, not a hole: it means "not the compiler's name",
/// which is how a plain variable (`$(( (x) ))` is grouping, not a call) and a
/// user function reach their own resolution.
fn builtin_denotation(k: BuiltinCmd) -> Option<ArithDenotation> {
    use ArithDenotation::{Int, Statement, Str};
    use BuiltinCmd as B;
    Some(match k {
        B::Len | B::Strlen => Int,
        // DD#126: `wait` yields a *pid* — the reaped child, or -1 for "no
        // children". It is the one int-valued name here that also has an
        // effect, which is why it stays out of `VALUE_ONLY_INTRINSICS`:
        // `- wait` is a legal statement, where `- len s` is not.
        B::Wait => Int,
        B::Keys
        | B::Join
        | B::Sub
        | B::Gsub
        | B::Match
        | B::Toupper
        | B::Tolower
        | B::Basename
        | B::Dirname => Str,
        B::Echo
        | B::Write
        | B::Test
        | B::Noop
        | B::Exit
        | B::Exec
        | B::Cd
        | B::Close
        | B::Attest
        | B::Read
        | B::Mapfile
        | B::Argv
        | B::Break
        | B::Continue
        | B::Return
        | B::Signal
        | B::Unset
        | B::MatchAll => Statement,
        // The compiler-internal opcodes. `from_name` cannot produce one (it
        // classifies only names a user can write), so this arm is a totality
        // arm — it names its variants rather than using `_` so that a new
        // opcode is a compile error here. `Statement` is nonetheless the
        // right answer if the route ever opened: an opcode is not a value.
        B::NegateTestResult
        | B::SetTestFromStatus
        | B::LetExit
        | B::ForkBackground
        | B::StoreBgPid
        | B::TrapSignal
        | B::SignalFlag
        | B::TableDelete
        | B::TableDeleteIdx => Statement,
        B::Other => return None,
    })
}

/// What `name` denotes in arithmetic, or `None` when the compiler does not
/// own the name — a user function or a plain variable, which the caller
/// resolves from its own tables.
pub(crate) fn arith_denotation(name: &str) -> Option<ArithDenotation> {
    builtin_denotation(BuiltinCmd::from_name(name))
        .or_else(|| INTRINSICS.iter().find(|(n, _)| *n == name).map(|(_, d)| *d))
}

/// True when `name` is a compiler intrinsic whose value is an i64. The one
/// predicate behind both "is `$(name)` int-typed" (`type_check`) and "may
/// `(name …)` appear in `$(( ))`" — they are the same question, and asking
/// it twice from two lists is what let the second answer rot.
pub(crate) fn is_int_valued_intrinsic(name: &str) -> bool {
    arith_denotation(name) == Some(ArithDenotation::Int)
}

/// True for any sash-native intrinsic — the `BuiltinCmd` set plus the
/// command-head-only names in [`INTRINSICS`]. The single source of truth for
/// "is this name the compiler's, rather than a PATH lookup or a user
/// function"; `validate` uses it to decide whether an arith sexpr `(name …)`
/// names anything at all.
///
/// Deliberately *not* the predicate for "may this appear in arithmetic" —
/// that is [`arith_denotation`]. Owning a name and denoting a value are
/// different questions about it.
pub(crate) fn is_intrinsic(name: &str) -> bool {
    arith_denotation(name).is_some()
}

/// Words the parser gives a meaning of their own. Most are already
/// unusable as a variable name because the lexer turns them into a
/// non-`Word` token, so `int if=5` fails to parse — but that is an
/// accident of tokenization, not a rule, and it does not cover the
/// *contextual* keywords: `in` and `do` lex as `Tok::Word` everywhere
/// except their one position, so `int in=5` and `int do=5` compiled.
/// Kept in sync with `parse::lexer::classify_word_token`.
const KEYWORDS: &[&str] = &[
    "if", "then", "else", "elif", "fi", "while", "until", "do", "done", "for", "in", "case",
    "esac", "fn", "function", "str", "int", "const", "array", "map", "attest", "signal", "END",
    "return", "break", "continue",
];

/// DD#125: is `name` the compiler's, rather than the user's to bind?
///
/// A name the compiler already understands may not be introduced as a
/// variable, collection, loop variable, read target, or function. The rule
/// exists because sash resolves a bare name differently in three positions
/// — an arith sexpr, a `$( )` word, and a bare arithmetic operand — and
/// without reservation the same name in one program resolved to two
/// different things: `fn getpid -> int { return 42 }` gave the intrinsic's
/// pid in `$(( (getpid) ))` and `42` in `$(getpid)`.
///
/// Reserving makes a bare word in arithmetic unambiguous, which is what
/// lets `$(( wait ))` / `$(( getpid ))` name the intrinsic (DD#125).
pub(crate) fn is_reserved_name(name: &str) -> bool {
    KEYWORDS.contains(&name) || is_intrinsic(name)
}

/// DD#125: why a *read* of a reserved name names nothing the author could
/// have bound, and what to write instead. `None` when the name is theirs.
///
/// SB5031's advice — "assign it first, or write `${x:-DEFAULT}`" — is advice
/// SB5092 forbids the author from taking. `echo $getpid` sent them to
/// `int getpid=…`, which is rejected *because* the compiler owns the name.
/// A rule satisfiable only by breaking another rule is a dead end, so the
/// read side has to answer for itself. The reservation created this the
/// moment it landed: before DD#125, `int getpid=5` was legal and SB5031's
/// advice worked.
pub(crate) fn reserved_read_advice(name: &str) -> Option<String> {
    match arith_denotation(name) {
        Some(ArithDenotation::Int) => Some(format!(
            "`{name}` is a sash intrinsic, not a variable — write `$({name})`, \
             or a bare `{name}` inside `$(( … ))`"
        )),
        Some(ArithDenotation::Str) => Some(format!(
            "`{name}` is a sash intrinsic, not a variable — capture it with \
             `$({name} …)`"
        )),
        Some(ArithDenotation::Statement) => Some(format!(
            "`{name}` is a sash command, not a variable — it denotes no value"
        )),
        None if KEYWORDS.contains(&name) => {
            Some(format!("`{name}` is a sash keyword, not a variable"))
        }
        None => None,
    }
}

/// True for the two intrinsics whose `$( )` denotes a **collection**: `argv`
/// yields the positional list, `keys` a map's keys. Every other intrinsic —
/// and every real command — yields a single value.
///
/// Stated in the positive because that is the exception set. The predicate
/// used to be spelled the other way round (`is_scalar_value_intrinsic`), which
/// made the `$( )` for-in rule allow-by-default: a name that was not a scalar
/// intrinsic fell through as a collection, so every external command, builtin,
/// pipeline and user function was treated as an iterable "line stream". There
/// is no such thing in sash's value model — DD#28 eliminates word splitting,
/// so a capture is one value however many lines it holds.
pub(crate) fn is_collection_intrinsic(name: &str) -> bool {
    name == "argv" || name == "keys"
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
/// for `name`, returning its absolute path. A candidate that is present but
/// not executable does not win the search — the shell's own rule, and the one
/// that keeps resolution from baking in a path that can only fail at `exec`.
/// Shared by this front-end pass and the transform's
/// `resolve_pipeline_command`.
pub(crate) fn resolve_command_path(name: &str) -> Option<String> {
    let sysroot = crate::get_sysroot();
    let path_var = std::env::var("PATH").unwrap_or_default();
    // DD#23: only search absolute PATH dirs. A relative dir (`.`, `..`, an
    // empty entry = cwd) would yield a path that binds to the runtime cwd —
    // the "search ambiguity" DD#23 forbids. A command found only via such a
    // dir stays unresolved (→ SB5011).
    for dir in path_var.split(':').filter(|d| d.starts_with('/')) {
        let target_path = format!("{}/{}", dir, name);
        if resolved_path_executable(&sysroot, &target_path) {
            return Some(target_path);
        }
    }
    None
}

/// Does the resolution root hold a program at `target_path`? A present but
/// unrunnable entry — a directory, a device node, a mode-0444 file — answers
/// no, so a PATH search keeps looking rather than resolving to something that
/// can only fail at `exec` time.
fn resolved_path_executable(sysroot: &Option<String>, target_path: &str) -> bool {
    crate::root_path_facts(sysroot, target_path).is_ok_and(|f| f.is_executable())
}

/// Why the resolution root cannot run `path`, phrased as a diagnostic clause,
/// or `None` if it can. An absolute head skips the PATH search but is held to
/// the same standard: DD#23 resolves command paths at compile time, and the
/// compiler already knows whether the program is there and runnable.
///
/// **Total over `FileKind` and `PathMiss` on purpose — do not add a
/// wildcard.** Each variant is a different answer to "why can this not run",
/// and the whole value of SB5095 over a runtime 127 is that it names which.
pub(crate) fn command_path_fault(path: &str) -> Option<String> {
    let sysroot = crate::get_sysroot();
    let facts = match crate::root_path_facts(&sysroot, path) {
        Ok(f) => f,
        Err(crate::PathMiss::Absent) => return Some("no such file".to_string()),
        Err(crate::PathMiss::LinkLoop) => {
            return Some("its symlink chain does not terminate".to_string())
        }
    };
    match facts.kind {
        crate::FileKind::Directory => return Some("it is a directory".to_string()),
        crate::FileKind::Other => return Some("it is not a regular file".to_string()),
        crate::FileKind::Regular => {}
    }
    if facts.mode & 0o111 == 0 {
        return Some(format!(
            "mode {:04o} grants no execute permission",
            facts.mode
        ));
    }
    None
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
    !is_intrinsic(name) && !funcs.contains(name)
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
    !is_intrinsic(name)
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
    // A head that already carries a path is not a search, so it never reaches
    // `resolve_command_path`; DD#23 judges it on its own terms.
    if name.contains('/') {
        check_path_head(name, c.name.span, diags);
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

/// DD#23 on a command head that spells out a path.
///
/// A **relative** one (`./x`, `../x`, `a/b`) binds to the runtime cwd — the
/// "search ambiguity" DD#23 forbids — and is SB5051.
///
/// An **absolute** one skips the PATH search, which is how it became the one
/// spelling of "this program is not here" that nobody checked: a bare name is
/// SB5011 and a relative one SB5051, both at compile time, while the path the
/// author was most explicit about compiled clean and failed at `exec`. SB5095
/// closes that, and names which fault it found.
fn check_path_head(name: &str, span: Span, diags: &mut Vec<Diagnostic>) {
    if !name.starts_with('/') {
        diags.push(Diagnostic::error_span(
            span,
            "SB5051",
            format!(
                "relative external path '{}' — DD#23 requires an absolute path (starting with '/')",
                name
            ),
        ));
        return;
    }
    diags.extend(command_path_diag("", name, span));
}

/// The SB5095 diagnostic for `path`, or `None` if the resolution root can run
/// it. The single authority for the rule, shared by the command-head pass here
/// and `validate`'s `exec`-target check: two positions, one standard, and no
/// second implementation to drift from this one.
///
/// `what` prefixes the message to name the position. A command head needs no
/// prefix; an `exec` target says so, because a failed `exec` has already torn
/// down the process image that would have reported it.
pub(crate) fn command_path_diag(what: &str, path: &str, span: Span) -> Option<Diagnostic> {
    let why = command_path_fault(path)?;
    Some(Diagnostic::error_span(
        span,
        "SB5095",
        format!("{}'{}' is not an executable program — {}", what, path, why),
    ))
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
