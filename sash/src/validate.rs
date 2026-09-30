//! IR-based validation rules.
//!
//! Walks the parsed IR and rejects unsafe constructs. This is the
//! second-pass gate (after parse-time rejections in parse/).
//!
//! Two independent analyses:
//!   1. **Structural validation** — walks IR checking rule violations:
//!      SB5001 (export), SB5006 (globs), SB5007 (arith mutations),
//!      SB5009 (IFS/word-split), SB5011 (command resolution),
//!      SB5012 ($? state machine), SB5017/5018 (rejected/env vars),
//!      SB5019 (numeric test ops), SB5023 (regex validation)
//!   2. **Taint tracking** (`check_taint`) — forward dataflow:
//!      sources (getenv, argv, external commands, stdin read) →
//!      propagation → use-site rejection → attest removes taint
//!
//! SECURITY: This module + parse/ are the complete validation surface.
//! Together they guarantee: no tainted data reaches execution without
//! attest, no dynamic dispatch, no unknown commands, no rejected vars.
//!
//! Trust model:
//!   - Parser guarantees: only recognized syntax reaches IR
//!   - Validator guarantees: only safe *semantics* reach transform/codegen
//!   - If both pass with zero diagnostics, the program is in the safe subset

use crate::diagnostic::Diagnostic;
use crate::ir::*;
use std::collections::HashMap;

// ── Entry point ────────────────────────────────────────────────────

/// Run all validation rules on a parsed program.
/// Returns diagnostics for any violations found.
/// Per-block checks. Split out of `validate` for the complexity gate.
///
/// `index` is the position in `program_blocks` order: 0 is the top level,
/// 1 the DD#56 `END { }` body.
fn validate_block(
    block: &[Statement],
    index: usize,
    functions: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    for stmt in block {
        validate_stmt(stmt, diags);
    }
    check_commands(block, functions, diags); // SB5011: command resolution

    // SB5012: $? availability. DD#56 — the END body is ENTERED because of a
    // triggering status, so `$?` is available as its first action; the
    // top-level block starts with no status.
    let seed = if index == 0 {
        StatusState::Undefined
    } else {
        StatusState::Available
    };
    check_dollar_question(block, seed, diags);
    check_regex_replacements(block, diags); // SB4033: sub/gsub replacement escape
}

pub fn validate(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    // Top-level statements, plus the DD#56 `END { }` body, as a single list of
    // blocks so every check runs over each.
    let blocks: Vec<&[Statement]> = std::iter::once(program.statements.as_slice())
        .chain(program.end_block.as_deref())
        .collect();

    // Collect declared function names across all blocks first, so a check
    // in one block sees a function defined in another.
    let mut functions = HashSet::new();
    for &block in &blocks {
        collect_functions(block, &mut functions);
    }

    for (i, &block) in blocks.iter().enumerate() {
        validate_block(block, i, &functions, &mut diags);
    }
    // SB4037 (DD#74): a function used as a predicate must `return` on every path
    check_predicate_definite_return(program, &functions, &mut diags);
    // SB4044: a `const`/`readonly` variable cannot be reassigned.
    check_const_reassignment(&blocks, &mut diags);
    // SB5049 (DD#86): no shadowing — a typed declaration inside a function of a
    // top-level global name is a redeclaration error.
    check_no_shadow(program, &mut diags);
    // SB5049 (DD#86): and no second declaration of a name within one scope.
    check_redeclaration(program, &mut diags);
    // SB5071 (DD#86): a `fn` definition is legal only at the top level.
    check_fn_placement(&blocks, &mut diags);
    // SB5072 (DD#86): a `const` binding inside a loop body.
    check_const_in_loop(&blocks, &mut diags);
    // SB5070 speaks only about *collection* splats. A splat of a scalar or of
    // an unintroduced name is SB5079/SB5031's to report, and SB5070's remedy
    // (`join s SEP`) is itself rejected for those — so it must not claim
    // them. Over-approximated program-wide: sash forbids shadowing, so one
    // name has one meaning everywhere.
    let mut collections = HashSet::new();
    for scope in program_scopes(program) {
        collect_subscriptable_names(scope, &mut collections);
    }
    for &block in &blocks {
        check_block_value_rules(block, &collections, &mut diags);
    }
    diags
}

/// SB4045 (a fixed-arity value-intrinsic called via `$( )` with the wrong
/// number of arguments) and the two SB4020 rules, over one block.
fn check_block_value_rules(
    block: &[Statement],
    collections: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    for stmt in block {
        check_stmt_value_rules(stmt, collections, diags);
        // SB5070: an array splat in a scalar operand of a *statement* command
        // (`cd @a`, `if match S @a`). The `$( )` half runs inside
        // `check_stmt_value_rules`'s expression walk.
        check_stmt_splat_operand(stmt, collections, diags);
        // SB4020 (bug #3): a `(( … ))` statement / condition whose embedded
        // mutation the transform can't hoist (`++`/`--`, `a[i]`, or under
        // `&&`/`||`/`?:`).
        check_arith_test_placement(stmt, diags);
    }
}

/// SB4044: a `const`/`readonly` variable cannot be reassigned. Flat model,
/// matching the transform's `readonly_vars`: once a name is declared
/// `const`/`readonly`, a later assignment or re-declaration is rejected here
/// with a **spanned** diagnostic. Statement-level arith mutations
/// (`(( x=5 ))`, `(( x++ ))`) lower to `Assignment` nodes, so they're caught by
/// the Assignment arm too. The rarer channels (`read`/`mapfile`/`argv` into a
/// const) are caught by the spanless backstop in `VersionState::define`, so no
/// path can ever reach the old `define()` panic.
fn check_const_reassignment(blocks: &[&[Statement]], diags: &mut Vec<Diagnostic>) {
    let mut consts: std::collections::HashSet<String> = std::collections::HashSet::new();
    for &block in blocks {
        scan_const_stmts(block, &mut consts, diags);
    }
}

/// SB5071 (DD#86): a `fn` definition is legal only as a **top-level**
/// statement.
///
/// DD#86 makes scope positional and gives control flow no scope of its own, so
/// a function name is global wherever the definition is written. Anywhere but
/// the top level the definition is therefore hoisted, and its placement
/// suggests a scope — or a condition — that the language does not have:
///
/// ```text
/// if [[ 1 = 2 ]] { fn f { echo A } }
/// f                   # sash printed A; bash: `f: command not found`
/// ```
///
/// The branch never runs and `f` is still defined. The same divergence appears
/// for `false && fn f {…}` and for a `fn` used as a pipeline stage (bash runs
/// the stage in a subshell, so the definition does not survive). It was also
/// the last taint bypass: `check_taint` defers the functions it finds in the
/// *top-level statement list*, so one hidden inside control flow kept the old
/// definition-point walk and could read a global tainted later.
///
/// Nesting inside another function's body is rejected for the same reason
/// rather than a weaker one — spec 18.3 pinned that such a function is an
/// ordinary global, i.e. the placement never meant anything there either.
fn check_fn_placement(blocks: &[&[Statement]], diags: &mut Vec<Diagnostic>) {
    for &block in blocks {
        for stmt in block {
            flag_fns_below(stmt, diags);
        }
    }
}

/// Everything strictly below a top-level statement — including a top-level
/// `fn`'s own body, which is why nesting inside a function is caught too.
fn flag_fns_below(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    for body in stmt.child_bodies() {
        flag_nested_fns(body, diags);
    }
}

fn flag_nested_fns(stmts: &[Statement], diags: &mut Vec<Diagnostic>) {
    crate::ir::walk_stmt_tree(stmts, &mut |s| {
        let StmtKind::Function(f) = &s.kind else {
            return;
        };
        diags.push(Diagnostic::error_span(
            s.span,
            "SB5071",
            format!(
                "`fn {}` is not at the top level. A function name is global \
                 wherever it is written, so this definition is hoisted and its \
                 placement means nothing — but it reads as though the function \
                 were scoped or conditional. Move `fn {}` to the top level.",
                f.name, f.name
            ),
        ));
    });
}

/// SB5072 (DD#86): a `const` binding inside a loop body.
///
/// DD#86 rejected loop-variable auto-scoping, so a loop body introduces no
/// scope: the binding is one binding in the enclosing scope, re-executed. A
/// `const` re-executed is a const reassigned. Both loop kinds are covered —
/// `for` because the static form unrolls into genuinely repeated bindings,
/// `while` because the body is re-entered.
///
/// The construct was already rejected, but only by the spanless backstop in
/// `VersionState::define` (reported at `1:1`), and in `while`'s case only as a
/// side effect of `readonly_vars` leaking across the two-pass lowering. This
/// check makes the rejection deliberate and spanned.
fn check_const_in_loop(blocks: &[&[Statement]], diags: &mut Vec<Diagnostic>) {
    for &block in blocks {
        scan_for_loops(block, diags);
    }
}

fn scan_for_loops(stmts: &[Statement], diags: &mut Vec<Diagnostic>) {
    for stmt in stmts {
        if let Some(body) = loop_body(stmt) {
            flag_const_decls(body, diags);
        }
        for body in stmt.child_bodies() {
            scan_for_loops(body, diags);
        }
    }
}

/// The body of a loop statement — the region that is re-executed.
fn loop_body(stmt: &Statement) -> Option<&[Statement]> {
    match &stmt.kind {
        StmtKind::While(w) => Some(&w.body),
        StmtKind::For(f) => Some(&f.body),
        _ => None,
    }
}

fn flag_const_decls(stmts: &[Statement], diags: &mut Vec<Diagnostic>) {
    crate::ir::walk_stmt_tree(stmts, &mut |s| {
        let StmtKind::Declaration(d) = &s.kind else {
            return;
        };
        if d.keyword != DeclKeyword::Readonly {
            return;
        }
        for a in &d.assignments {
            diags.push(Diagnostic::error_span(
                s.span,
                "SB5072",
                format!(
                    "`const {}` is inside a loop body. A loop body introduces \
                     no scope (DD#86), so this is one binding re-executed on \
                     every iteration — a `const` reassigned. Declare it before \
                     the loop, or drop `const`.",
                    a.name
                ),
            ));
        }
    });
}

fn scan_const_stmts(
    stmts: &[Statement],
    consts: &mut std::collections::HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    for stmt in stmts {
        scan_const_stmt(stmt, consts, diags);
    }
}

fn scan_const_stmt(
    stmt: &Statement,
    consts: &mut std::collections::HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    match &stmt.kind {
        StmtKind::Declaration(d) => scan_const_decl(d, consts),
        StmtKind::Assignment(a) => flag_if_const(consts, &a.name, stmt.span, diags),
        _ => {}
    }
    for body in stmt.child_bodies() {
        scan_const_stmts(body, consts, diags);
    }
}

/// A `const`/`readonly` declaration records its names.
///
/// It does NOT report re-declaring an already-const name: that is a
/// *redeclaration*, which SB5049 reports with the accurate message. SB4044
/// stays for what it actually describes — a bare assignment writing a const.
/// (Both used to fire, for `const k` twice and for every typed redeclaration
/// of a const, telling the user the binding "cannot be reassigned" when they
/// had not tried to assign to it.)
fn scan_const_decl(d: &Declaration, consts: &mut std::collections::HashSet<String>) {
    if d.keyword == DeclKeyword::Readonly {
        for a in &d.assignments {
            consts.insert(a.name.clone());
        }
    }
}

fn flag_if_const(
    consts: &std::collections::HashSet<String>,
    name: &str,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    if consts.contains(name) {
        diags.push(const_reassign_diag(span, name));
    }
}

fn const_reassign_diag(span: Span, name: &str) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB4044",
        format!(
            "readonly variable `{name}` cannot be reassigned — a `const`/`readonly` \
             binding is immutable"
        ),
    )
}

/// SB5049 (DD#86, same-scope): a *typed* declaration (`str`/`int`/`array`/
/// `map`/`const`) of a name already declared in the **same** scope.
///
/// The companion rule to `check_no_shadow`, which handles the *cross*-scope
/// case. Together they say: one name, one declaration, per scope. What stays
/// legal is what lives in a genuinely different scope — two sibling functions
/// may each declare `str x`, and a `for` variable is not a `Declaration` at
/// all, so repeating `for x` is untouched.
///
/// This deliberately fires on a re-declaration that matches the original type
/// (`str x` twice), one that *changes* it (`str x` then `int x`), and one that
/// *acquires* `const` — all three were accepted before, and the latter two
/// silently changed a variable's contract.
fn check_redeclaration(program: &Program, diags: &mut Vec<Diagnostic>) {
    // The top level and the DD#56 `END { }` body are ONE scope: END's
    // declarations are globals, and `collect_top_level_decls` already pools
    // them, so `str x` in each is the same name declared twice.
    let mut seen: HashMap<String, Span> = HashMap::new();
    let mut bound: HashMap<String, Span> = HashMap::new();
    scan_scope_redecl(&program.statements, &mut seen, &mut bound, diags);
    if let Some(ref end) = program.end_block {
        scan_scope_redecl(end, &mut seen, &mut bound, diags);
    }
}

/// DD#86 (SB5082): record a name introduced WITHOUT a declaration — a bare
/// assignment, an arithmetic assignment, or a `for` header. Only the first
/// introduction counts, and a name already declared is not an introduction.
fn note_binding(
    name: &str,
    span: Span,
    seen: &HashMap<String, Span>,
    bound: &mut HashMap<String, Span>,
) {
    if seen.contains_key(name) || bound.contains_key(name) {
        return;
    }
    bound.insert(name.to_string(), span);
}

/// Walk one scope. Control-flow bodies are part of the enclosing scope (sash
/// has no block scope), so they share `seen`; a function body is a new scope
/// and gets a fresh map.
fn scan_scope_redecl(
    stmts: &[Statement],
    seen: &mut HashMap<String, Span>,
    bound: &mut HashMap<String, Span>,
    diags: &mut Vec<Diagnostic>,
) {
    for stmt in stmts {
        scan_stmt_redecl(stmt, seen, bound, diags);
    }
}

fn scan_stmt_redecl(
    stmt: &Statement,
    seen: &mut HashMap<String, Span>,
    bound: &mut HashMap<String, Span>,
    diags: &mut Vec<Diagnostic>,
) {
    // A function body is a new scope, so it starts from an empty map and its
    // names never reach the caller's.
    if let StmtKind::Function(f) = &stmt.kind {
        let mut inner = HashMap::new();
        let mut inner_bound = HashMap::new();
        scan_scope_redecl(&f.body, &mut inner, &mut inner_bound, diags);
        return;
    }
    match &stmt.kind {
        StmtKind::Declaration(d) => flag_redecl(d, stmt.span, seen, bound, diags),
        // The three ways a name is introduced without a declaration. All bind
        // like an assignment, so all fix the name's type (DD#86).
        StmtKind::Assignment(a) if a.index.is_none() => {
            note_binding(&a.name, stmt.span, seen, bound)
        }
        StmtKind::For(f) => note_binding(&f.variable, stmt.span, seen, bound),
        StmtKind::ArithTest(e) => {
            let mut names = Vec::new();
            crate::ir::walk_arith_var_writes(e, &mut |n: &str| names.push(n.to_string()));
            for n in names {
                note_binding(&n, stmt.span, seen, bound);
            }
        }
        _ => {}
    }
    for body in stmt.child_bodies() {
        scan_scope_redecl(body, seen, bound, diags);
    }
}

fn flag_redecl(
    d: &Declaration,
    span: Span,
    seen: &mut HashMap<String, Span>,
    bound: &mut HashMap<String, Span>,
    diags: &mut Vec<Diagnostic>,
) {
    for a in &d.assignments {
        if let Some(prev) = seen.get(&a.name) {
            diags.push(redecl_diag(span, &a.name, prev.line));
            continue;
        }
        if let Some(prev) = bound.get(&a.name) {
            diags.push(late_decl_diag(span, &a.name, prev.line));
            continue;
        }
        seen.insert(a.name.clone(), span);
    }
}

/// SB5082 (DD#86): a typed declaration of a name that an earlier binding
/// already introduced. Declarations are collected program-wide, so the
/// declaration used to reach BACKWARDS and retype every earlier read —
/// `x=hello; echo "$x"; int x=7` printed `0`, a value the program never wrote.
fn late_decl_diag(span: Span, name: &str, prev_line: usize) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5082",
        format!(
            "`{name}` was already introduced by an earlier binding (line \
             {prev_line}) — a variable does not change type (DD#86), and this \
             declaration would retype every read above it. Declare `{name}` \
             before its first use, or rename this one"
        ),
    )
}

fn redecl_diag(span: Span, name: &str, prev_line: usize) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5049",
        format!(
            "`{name}` is already declared in this scope (line {prev_line}) — sash has \
             one declaration per name per scope (DD#86). Assign to the existing \
             variable with a bare `{name} = …`, or rename this one"
        ),
    )
}

/// SB5049 (DD#86): no shadowing. A name declared at top level is a **global**;
/// a *typed* re-declaration (`str`/`int`/`array`/`map`/`const`) of that name
/// inside a function is a redeclaration error — there is no way to create a
/// local that shadows a global. A *bare* assignment inside a function writes
/// the global (allowed, and not flagged here).
fn check_no_shadow(program: &Program, diags: &mut Vec<Diagnostic>) {
    let mut globals: HashSet<String> = HashSet::new();
    collect_top_level_decls(&program.statements, &mut globals);
    if let Some(ref end) = program.end_block {
        collect_top_level_decls(end, &mut globals);
    }
    scan_fn_shadow(&program.statements, &globals, diags);
    if let Some(ref end) = program.end_block {
        scan_fn_shadow(end, &globals, diags);
    }
}

/// Collect the names declared/assigned at top level (the globals), NOT
/// descending into function bodies.
fn collect_top_level_decls(stmts: &[Statement], globals: &mut HashSet<String>) {
    for stmt in stmts {
        match &stmt.kind {
            // A function body is its own scope — its names are not globals.
            StmtKind::Function(_) => {}
            StmtKind::Declaration(d) => insert_decl_names(d, globals),
            StmtKind::Assignment(a) => {
                globals.insert(a.name.clone());
            }
            _ => {
                for body in stmt.child_bodies() {
                    collect_top_level_decls(body, globals);
                }
            }
        }
    }
}

/// Walk to each function body and scan it for typed declarations that shadow a
/// global.
fn scan_fn_shadow(stmts: &[Statement], globals: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Function(f) => scan_body_shadow(&f.body, globals, diags),
            _ => {
                for body in stmt.child_bodies() {
                    scan_fn_shadow(body, globals, diags);
                }
            }
        }
    }
}

/// Flag every typed declaration in a function body (recursively, including
/// nested functions and control-flow bodies) whose name is a top-level global.
fn scan_body_shadow(stmts: &[Statement], globals: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    for stmt in stmts {
        if let StmtKind::Declaration(d) = &stmt.kind {
            flag_shadow_decl(d, stmt.span, globals, diags);
        }
        for body in stmt.child_bodies() {
            scan_body_shadow(body, globals, diags);
        }
    }
}

fn flag_shadow_decl(
    d: &Declaration,
    span: Span,
    globals: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    for a in &d.assignments {
        if globals.contains(&a.name) {
            diags.push(shadow_diag(span, &a.name));
        }
    }
}

fn shadow_diag(span: Span, name: &str) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5049",
        format!(
            "cannot redeclare global `{name}` inside a function — sash has no variable \
             shadowing (DD#86). Write the global with a bare assignment `{name} = …`, or \
             rename this local so it does not collide with the top-level `{name}`"
        ),
    )
}

/// SB4045: a value-intrinsic (`len`, `sub`, `toupper`, …) invoked via `$( )`
/// with the wrong argument count. Without this, a strict-arity intrinsic
/// (`sub`, `match`, …) whose arity check fails falls through to a bare external
/// command and aborts at runtime ("bare command passed to sash_exec"); a
/// lenient one (`len`) silently ignores extra args. This makes arity a uniform
/// compile-time error.
fn check_cmdsub_arity(expr: &Expr, diags: &mut Vec<Diagnostic>) {
    let ExprKind::CommandSubstitution(stmts) = &expr.kind else {
        return;
    };
    let Some(c) = single_cmdsub_command(stmts) else {
        return;
    };
    let ExprKind::Literal(name) = &c.name.kind else {
        return;
    };
    let Some(arity) = crate::ssa::value_intrinsic_arity(name) else {
        return;
    };
    if c.args.len() != arity {
        diags.push(Diagnostic::error_span(
            c.name.span,
            "SB4045",
            format!(
                "`{name}` expects {arity} argument{} but got {} — a value intrinsic \
                 must be called with its exact arity",
                if arity == 1 { "" } else { "s" },
                c.args.len()
            ),
        ));
    }
}

/// SB4045 for the sexpr spelling: `(len a)` / `(strlen $s)` inside `$(( … ))`
/// must have the arity and the operand shape the transform can lower.
///
/// Both were silently 0. The transform rebuilds a `Command` from the sexpr
/// args and hands it to the ordinary intrinsic builder; when the rebuild
/// fails it falls back to the *user* int-function path, which finds no body
/// for a name like `strlen` and yields 0. `$(strlen a b)` has reported SB4045
/// all along — only `$(( (strlen a b) ))` was quiet, and the same mistake must
/// not answer to two different codes depending on spelling.
fn check_arith_sexpr_arity(expr: &Expr, diags: &mut Vec<Diagnostic>) {
    let ExprKind::Arithmetic(a) = &expr.kind else {
        return;
    };
    check_arith_const_faults(a, expr.span, diags);
    walk_arith_intrinsic_calls(a, &mut |name, args| {
        if let Some(complaint) = arith_intrinsic_complaint(name, args) {
            diags.push(Diagnostic::error_span(expr.span, "SB4045", complaint));
        }
    });
}

/// SB5077 / SB5084: an arithmetic fault the constant folder can prove.
///
/// DD#25 (amended): every arithmetic fault aborts. One that only the
/// *runtime* can see (`int z = 0; $(( 1 / z ))` — there is no
/// const-propagation through variables) aborts there; one the folder can
/// prove is a compile-time error instead, because the compiler already knows
/// the program cannot run. Scope is exactly what the folder proves: literals
/// and operators over literals, through parens.
///
/// Both faults are properties of **the expression as written**, so a proven
/// fault in a ternary branch that would never be taken is still an error —
/// this is not reachability analysis. That was already SB5077's behaviour
/// (`(( x ? 1 : 5 / 0 ))`); the shift check inherits it rather than choosing
/// it, which is the point of sharing one walk.
fn check_arith_const_faults(a: &ArithExpr, span: Span, diags: &mut Vec<Diagnostic>) {
    walk_arith_binops(a, &mut |op, right| {
        if let Some((code, msg)) = const_arith_fault(op, right) {
            diags.push(Diagnostic::error_span(span, code, msg));
        }
    });
}

/// The `(code, message)` for a binary operator whose right operand the folder
/// proves is a fault, or `None` when the operator cannot fault this way or the
/// operand is not provable.
///
/// A compound assignment needs no arm of its own: the parser desugars
/// `x <<= -1` into `Assign { value: BinOp { op: Shl, … } }`, so it arrives
/// here as an ordinary `BinOp`.
fn const_arith_fault(op: IrArithOp, right: &ArithExpr) -> Option<(&'static str, String)> {
    let n = const_eval_arith(right)?;
    let by_zero = |what: &str| {
        Some((
            "SB5077",
            format!("{what} by zero — this program cannot run"),
        ))
    };
    match op {
        IrArithOp::Div if n == 0 => by_zero("division"),
        IrArithOp::Mod if n == 0 => by_zero("modulo"),
        IrArithOp::Shl | IrArithOp::Shr if n < 0 => Some((
            "SB5084",
            format!("negative shift count {n} — this program cannot run"),
        )),
        _ => None,
    }
}

/// Invoke `f(op, right)` for each binary operator in an `ArithExpr` tree.
fn walk_arith_binops(a: &ArithExpr, f: &mut impl FnMut(IrArithOp, &ArithExpr)) {
    match a {
        ArithExpr::BinOp { left, op, right } => {
            f(*op, right);
            walk_arith_binops(left, f);
            walk_arith_binops(right, f);
        }
        ArithExpr::UnaryOp { operand, .. } | ArithExpr::Paren(operand) => {
            walk_arith_binops(operand, f)
        }
        ArithExpr::Assign { value, .. } | ArithExpr::CompoundAssign { value, .. } => {
            walk_arith_binops(value, f)
        }
        ArithExpr::Ternary {
            cond,
            true_val,
            false_val,
        } => {
            walk_arith_binops(cond, f);
            walk_arith_binops(true_val, f);
            walk_arith_binops(false_val, f);
        }
        ArithExpr::Comma(exprs) => {
            for e in exprs {
                walk_arith_binops(e, f);
            }
        }
        _ => {}
    }
}

/// Evaluate an `ArithExpr` to a constant, or `None` when it is not
/// compile-time provable.
///
/// The operator semantics are the transform's own `fold_arith_op`, not a
/// second copy — the two must agree about what "the folder proves" means, and
/// sharing the function is the only way to guarantee that. Only the tree walk
/// is local, and it is deliberately the weaker half: anything it declines
/// (variables, expansions, calls) simply reaches the runtime trap, which is
/// still correct, just later.
fn const_eval_arith(a: &ArithExpr) -> Option<i64> {
    use crate::transform::expr_handlers::{fold_arith_op, fold_unary_op, ir_op_to_ssa};
    match a {
        ArithExpr::Literal(n) => Some(*n),
        ArithExpr::Paren(inner) => const_eval_arith(inner),
        ArithExpr::BinOp { left, op, right } => {
            let l = const_eval_arith(left)?;
            let r = const_eval_arith(right)?;
            fold_arith_op(l, r, ir_op_to_ssa(*op))
        }
        // `-x`/`+x`/`!x`/`~x` over a constant is still a constant. Omitting
        // this arm is what let `$(( 5 / (1 + -1) ))` reach the runtime while
        // `$(( 5 / (2 - 2) ))` was SB5077. `UnaryOp` carries no side effect —
        // `x++`/`++x` are separate `PostIncDec`/`PreIncDec` variants, which
        // stay unevaluable here.
        ArithExpr::UnaryOp { op, operand } => {
            let v = const_eval_arith(operand)?;
            fold_unary_op(v, *op)
        }
        _ => None,
    }
}

/// The SB4045 complaint for one arith sexpr call, or `None` if it is fine.
fn arith_intrinsic_complaint(name: &str, args: &[ArithExpr]) -> Option<String> {
    if name != "len" && name != "strlen" {
        return None;
    }
    if args.len() != 1 {
        return Some(format!(
            "`{name}` expects 1 argument but got {} — a value intrinsic must be \
             called with its exact arity",
            args.len()
        ));
    }
    if arith_intrinsic_operand_ok(&args[0]) {
        return None;
    }
    Some(format!(
        "`{name}` operand isn't a name or a value in an arithmetic call. Use a \
         variable (`{name} $x`), a cell (`{name} ${{a[i]}}`), or capture the \
         computed operand first: `t=$(( … ))` then `({name} $t)`."
    ))
}

/// The operand shapes the transform's `reconstruct_intrinsic_command` rebuilds.
fn arith_intrinsic_operand_ok(a: &ArithExpr) -> bool {
    match a {
        ArithExpr::Var(_)
        | ArithExpr::DollarVar(_)
        | ArithExpr::Literal(_)
        | ArithExpr::Expansion(_) => true,
        ArithExpr::Paren(inner) => arith_intrinsic_operand_ok(inner),
        _ => false,
    }
}

/// Invoke `f(name, args)` for each sexpr `FuncCall` in an `ArithExpr` tree.
fn walk_arith_intrinsic_calls(a: &ArithExpr, f: &mut impl FnMut(&str, &[ArithExpr])) {
    match a {
        ArithExpr::FuncCall { name, args } => {
            f(name, args);
            for arg in args {
                walk_arith_intrinsic_calls(arg, f);
            }
        }
        ArithExpr::BinOp { left, right, .. } => {
            walk_arith_intrinsic_calls(left, f);
            walk_arith_intrinsic_calls(right, f);
        }
        ArithExpr::UnaryOp { operand, .. } => walk_arith_intrinsic_calls(operand, f),
        ArithExpr::Assign { value, .. } | ArithExpr::CompoundAssign { value, .. } => {
            walk_arith_intrinsic_calls(value, f)
        }
        ArithExpr::Ternary {
            cond,
            true_val,
            false_val,
        } => {
            walk_arith_intrinsic_calls(cond, f);
            walk_arith_intrinsic_calls(true_val, f);
            walk_arith_intrinsic_calls(false_val, f);
        }
        ArithExpr::Comma(exprs) => {
            for e in exprs {
                walk_arith_intrinsic_calls(e, f);
            }
        }
        ArithExpr::Paren(inner) => walk_arith_intrinsic_calls(inner, f),
        _ => {}
    }
}

/// SB5070: an array splat in a *scalar* operand of a value intrinsic.
///
/// DD#22/#69 give a splat one argument per element, and `sub`/`gsub`/
/// `toupper`/`tolower`/`basename`/`dirname`/`match` take scalar strings. The
/// splat was instead collapsing to a single joined argument, so
/// `$(gsub @a ":" "-")` silently behaved as `$(gsub "$(join a)" ":" "-")` —
/// arity checked out because the splat is one *syntactic* argument. DD#75
/// makes `$(join a SEP)` the explicit way to ask for a joined string, so the
/// message names it.
///
/// `len`, `join` and `keys` are excluded: they take a collection by name, so
/// `@a` there is not a silent reinterpretation (`keys @a` is already SB5025,
/// and `len @a` / `join @a` are open questions of their own — DD#75).
/// `strlen` is **not** in that group — DD#102 made it take a value like
/// `toupper` — so `strlen @a` was the one silent join left.
fn check_cmdsub_splat_operand(
    expr: &Expr,
    collections: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    let ExprKind::CommandSubstitution(stmts) = &expr.kind else {
        return;
    };
    let Some(c) = single_cmdsub_command(stmts) else {
        return;
    };
    check_command_splat_operand(c, collections, true, diags);
}

/// The operand scan shared by both positions a scalar-operand command can
/// occupy. `in_cmdsub` only picks the syntax the remedy is written in.
///
/// Keying this on the `$( )` *syntax* rather than on the command is what let
/// `if match S @a` through: `match` is legal both as a `$( )` value and as a
/// DD#70 must-consume boolean statement, and only the first was scanned. The
/// splat then reached `lower_value` as a scalar and ICEd. `cd` is
/// statement-only and so was never scanned at all.
fn check_command_splat_operand(
    c: &Command,
    collections: &HashSet<String>,
    in_cmdsub: bool,
    diags: &mut Vec<Diagnostic>,
) {
    let ExprKind::Literal(name) = &c.name.kind else {
        return;
    };
    if !is_scalar_operand_intrinsic(name) {
        return;
    }
    for arg in &c.args {
        let Some(collection) = splat_collection_name(&arg.kind) else {
            continue;
        };
        if !splat_subject_is_collection(collection, collections) {
            continue;
        }
        let remedy = splat_join_remedy(name, collection, c.args.len(), in_cmdsub);
        diags.push(Diagnostic::error_span(
            arg.span,
            "SB5070",
            format!(
                "`{name}` takes a string, but `@{collection}` is an array splat — one \
                 argument per element (DD#69), not a joined string. To operate on the \
                 joined text write `{remedy}`; to operate \
                 on each element, loop over `@{collection}`"
            ),
        ));
    }
}

/// The `join` rewrite SB5070 advises, spelled in the caller's syntax: wrapped
/// in `$( )` for the value form, bare for the statement form, and with the
/// trailing `…` only when the command actually takes further arguments (`cd`
/// does not).
fn splat_join_remedy(name: &str, collection: &str, nargs: usize, in_cmdsub: bool) -> String {
    let rest = if nargs > 1 { " …" } else { "" };
    let joined = format!("{name} \"$(join {collection} SEP)\"{rest}");
    if in_cmdsub {
        format!("$({joined})")
    } else {
        joined
    }
}

/// Every command statement in a block, recursively, for the statement half of
/// [`check_command_splat_operand`]. `child_bodies()` is total, so this reaches
/// the if/while conditions and `&&`/`||` sides where a `match` must live.
/// It does **not** descend into `$( )` bodies — those are expressions, and
/// [`check_cmdsub_splat_operand`] already scans them, so nothing is reported
/// twice.
fn check_stmt_splat_operand(
    stmt: &Statement,
    collections: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    if let StmtKind::Command(c) = &stmt.kind {
        check_command_splat_operand(c, collections, false, diags);
    }
    // `structural_bodies`: `check_cmdsub_splat_operand` handles a `$( … )`
    // body itself, and passes `in_cmdsub = true` — a scalar-operand intrinsic
    // is legal there and nowhere else, so reaching the same command through
    // the plain recursion would reject it.
    for body in stmt.structural_bodies() {
        for inner in body {
            check_stmt_splat_operand(inner, collections, diags);
        }
    }
}

/// Whether a splat subject is a collection, and so SB5070's to claim. The
/// DD#100 slots and `args` are collections with no declaration in scope, as in
/// `subscript_subject_is_scalar`.
fn splat_subject_is_collection(name: &str, collections: &HashSet<String>) -> bool {
    collections.contains(name) || is_collection_slot(name) || name == "args"
}

/// The commands whose operands are lowered as **scalar values**, so an array
/// splat in one is never right. `len` / `join` / `keys` are the
/// collection-by-name intrinsics and are deliberately absent.
///
/// This is one list for both positions on purpose. `match` is legal as a
/// `$( )` value *and* as a DD#70 boolean statement; `cd` is statement-only.
/// Keeping a second list for the statement half is exactly how the two drift.
///
/// `strlen` belongs here and was missing until 2026-09-17. SB5070 predates
/// DD#102, which moved `strlen` from the name-taking family to the
/// value-taking one — "strlen should behave like toupper" — and this
/// list, keyed on the family `strlen` had just left, was not updated with it.
/// `type_check.rs::string_operand_slots` already grouped it with `toupper`.
fn is_scalar_operand_intrinsic(name: &str) -> bool {
    matches!(
        name,
        "sub" | "gsub" | "toupper" | "tolower" | "strlen" | "basename" | "dirname" | "match" | "cd"
    )
}

/// The collection name behind any array splat — bare `@a` or one carrying a
/// disposition (`@{a:-W}`, `@{a:?W}`). Every splat operator is spelled
/// `arrvals…`. `None` when the expression is not a splat at all, so a caller
/// uses this as the test as well as the accessor.
///
/// There is deliberately **no `DoubleQuoted` case**: inside double quotes `@a`
/// and `@{a:-W}` are literal text, and the one splat spelling that survives
/// quoting is the DD#75 §4 `[@]`/`[*]` form, whose `arrvals_retired` operator
/// the **parser** rejects (SB4038, `retired_bracket_operator_diag`). A quoted
/// splat therefore never reaches `validate`.
fn splat_collection_name(kind: &ExprKind) -> Option<&str> {
    let ExprKind::ParameterExpansion(pe) = kind else {
        return None;
    };
    pe.operator
        .as_deref()
        .is_some_and(|op| op.starts_with("arrvals"))
        .then_some(pe.name.as_str())
}

/// The single command body of a `$( )` (ignoring comments), or `None` if the
/// substitution holds a pipeline / multiple statements (not a simple intrinsic
/// call).
fn single_cmdsub_command(stmts: &[Statement]) -> Option<&Command> {
    let mut cmd = None;
    for s in stmts {
        match &s.kind {
            StmtKind::Command(c) if cmd.is_none() => cmd = Some(c),
            _ => return None,
        }
    }
    cmd
}

/// SB4037 (DD#74): a function used in a boolean context must supply its
/// status via an explicit `return` on every path.
fn check_predicate_definite_return(
    program: &Program,
    functions: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    let mut predicate_uses = HashSet::new();
    collect_predicate_uses(&program.statements, functions, &mut predicate_uses);
    if let Some(ref end) = program.end_block {
        collect_predicate_uses(end, functions, &mut predicate_uses);
    }
    if predicate_uses.is_empty() {
        return;
    }
    let mut fn_defs: Vec<(&str, Span, &[Statement])> = Vec::new();
    collect_function_defs(&program.statements, &mut fn_defs);
    if let Some(ref end) = program.end_block {
        collect_function_defs(end, &mut fn_defs);
    }
    report_non_returning_predicates(&fn_defs, &predicate_uses, diags);
}

/// SB4037: emit a diagnostic for each predicate-used function whose body does
/// not `return` on every path.
fn report_non_returning_predicates(
    fn_defs: &[(&str, Span, &[Statement])],
    predicate_uses: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    for (name, span, body) in fn_defs {
        if predicate_uses.contains(*name) && !body_always_returns(body) {
            diags.push(Diagnostic::error_span(
                *span,
                "SB4037",
                format!(
                    "function `{name}` is used as a predicate but does not `return` on every \
                     path — a function in a boolean context must end each path with an explicit \
                     `return` (a command, `[[ … ]]`, `(( … ))`, or a status code)"
                ),
            ));
        }
    }
}

// ── Statement-level validation ─────────────────────────────────────

fn validate_stmt(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    validate_stmt_ctx(stmt, diags, false);
}

fn validate_stmt_ctx(stmt: &Statement, diags: &mut Vec<Diagnostic>, in_condition: bool) {
    match &stmt.kind {
        StmtKind::Command(c) => validate_command(c, stmt.span, diags),
        StmtKind::Declaration(d) => validate_declaration_stmt(d, stmt.span, in_condition, diags),
        StmtKind::Assignment(a) => validate_assignment_stmt(a, stmt.span, in_condition, diags),
        StmtKind::If(i) => validate_if(i, diags),
        StmtKind::While(w) => validate_while(w, diags),
        StmtKind::For(f) => validate_for(f, diags),
        StmtKind::Group(g) => validate_group(g, in_condition, diags),
        StmtKind::ArithTest(expr) => {
            // SB5007: check arithmetic in context
            validate_arith_context(expr, stmt.span, in_condition, diags);
        }
        StmtKind::Case(c) => {
            check_vacuous_dispatch(c, diags);
            validate_nested(stmt, in_condition, diags);
        }
        // Wrappers (List, Pipeline, Subshell, Suppressed, Redirected) and
        // reset-context constructs (Function, Case) — see `validate_nested`.
        _ => validate_nested(stmt, in_condition, diags),
    }
    // Validate the expressions this statement owns. The walk is deliberately
    // NON-recursive: this function already visits every nested statement (via
    // `validate_nested` / the `If`/`While`/`For`/`Group` arms), so a deep walk
    // here would re-validate every nested expression once per enclosing level
    // and emit each diagnostic depth+1 times (journal 308).
    walk_own_exprs_in_stmt(stmt, &mut |expr, span| validate_expr(expr, span, diags));
    // A `$( … )` body is an anonymous function body (DD#86), so every statement
    // in it gets the same validation as one written at top level. This has to
    // live here rather than in `validate_nested`, because the arms that do NOT
    // reach `validate_nested` — `Command`, `Declaration`, `Assignment`,
    // `ArithTest` — are exactly the ones that most often carry a substitution.
    // `in_condition` resets to false: a body is a function body, and the
    // condition context does not cross that boundary.
    for body in stmt.cmdsub_bodies() {
        validate_children(body, false, diags);
    }
}

/// DD#70/#71: a typed declaration is a value binding, not a boolean/exit
/// status, so it cannot serve as a condition. Mirrors `validate_assignment_stmt`.
fn validate_declaration_stmt(
    d: &Declaration,
    span: Span,
    in_condition: bool,
    diags: &mut Vec<Diagnostic>,
) {
    if in_condition {
        let is_capture = d
            .assignments
            .iter()
            .any(|a| a.value.as_ref().is_some_and(value_is_capture));
        diags.push(nonstatus_in_condition_diag(
            span,
            "a declaration",
            is_capture,
        ));
    }
    validate_declaration(d, diags);
}

/// Recurse into a wrapper statement's children with the correct condition
/// context. DD#70/#71: the left operand of `&&`/`||` and the operand of `!`
/// are boolean contexts; other wrappers forward `in_condition`; resetters
/// (Function, Case) drop it (via `forwards_condition`).
fn validate_nested(stmt: &Statement, in_condition: bool, diags: &mut Vec<Diagnostic>) {
    match &stmt.kind {
        StmtKind::List(l) => {
            let left_cond = matches!(l.op, ListOp::And | ListOp::Or) || in_condition;
            validate_stmt_ctx(&l.left, diags, left_cond);
            validate_stmt_ctx(&l.right, diags, in_condition);
        }
        StmtKind::Pipeline(p) if p.negated => validate_children(&p.commands, true, diags),
        // DD#74: `return <producer>` — validate the producer as a condition
        // (a `[[ ]]`/`(( ))`/command status producer, consumed by the return).
        StmtKind::ReturnProducer(inner) => validate_stmt_ctx(inner, diags, true),
        _ => {
            let inner = forwards_condition(stmt) && in_condition;
            // `structural_bodies`, not `child_bodies`: `validate_stmt_ctx`
            // walks the `$( … )` bodies of EVERY statement, wrapper or not.
            // Taking them here too would validate a wrapper's substitution
            // twice and print each of its diagnostics twice.
            for body in stmt.structural_bodies() {
                validate_children(body, inner, diags);
            }
        }
    }
}

fn validate_children(stmts: &[Statement], in_condition: bool, diags: &mut Vec<Diagnostic>) {
    for s in stmts {
        validate_stmt_ctx(s, diags, in_condition);
    }
}

/// True for wrapping StmtKinds whose validation context is the
/// same as the wrapper itself: Pipeline, Subshell, Suppressed,
/// Redirected, List. Group is *not* listed here because DD#31
/// requires a per-call inspection; it has its own dedicated arm.
/// Function and Case are *not* listed because their bodies must
/// be validated outside of condition context regardless of the
/// wrapper's own context.
fn forwards_condition(stmt: &Statement) -> bool {
    matches!(
        &stmt.kind,
        StmtKind::Pipeline(_)
            | StmtKind::Suppressed(_)
            | StmtKind::Redirected(_)
            | StmtKind::List(_)
    )
}

fn validate_assignment_stmt(
    a: &Assignment,
    span: Span,
    in_condition: bool,
    diags: &mut Vec<Diagnostic>,
) {
    validate_assignment(a, span, diags);
    // SB5007: arithmetic assignment in condition context.
    // The DD#31 comma pattern is handled at the Group level.
    //
    // Names the target and claims no operator: only the *decomposed* form
    // arrives here, so the mutation's spelling is already gone. (This used to
    // call `assign_brief`, which claimed `=` for `x++` and dropped the
    // subscript from `a[0] -= 2`.) The nested form reports identically, via
    // `mutation_target_in` — same message for `if (( x = 5 ))` either way.
    if in_condition && matches!(&a.value.kind, ExprKind::Arithmetic(_)) {
        diags.push(Diagnostic::error_span(
            span,
            "SB5007",
            format!(
                "mutation of {} not allowed in condition (( )) expression context — \
                 use (( )) statement instead",
                crate::ir::mutation_target_brief(&a.name, a.index.is_some())
            ),
        ));
    } else if in_condition && !is_capture_condition_rhs(&a.value) {
        // DD#70/#71: a (non-arith) assignment is a value binding, not a
        // boolean/exit-status — it cannot be a condition. EXCEPTION: a single
        // external `$(cmd)` RHS is the capture-condition form (`if x=$(cmd)`) —
        // the command's status is the condition, its stdout captured into the
        // target (handled in transform::try_capture_condition).
        diags.push(nonstatus_in_condition_diag(
            span,
            "an assignment",
            value_is_capture(&a.value),
        ));
    }
}

/// True when an assignment RHS is a single external `$(cmd)` command
/// substitution — the capture-condition form (`if x=$(cmd)` / `while x=$(cmd)`),
/// where the command's exit status is the condition and its stdout is captured
/// into the target. Post-resolution an external command's name is an absolute
/// path (contains `/`); a user function / intrinsic / builtin stays a bare
/// name, so this structurally excludes `$(user_fn)` (which is NOT a
/// capture-condition). A pipeline / multi-statement / concat RHS is also
/// excluded (a single external command only).
///
/// **DD#101 carve-out:** `$(getenv NAME)` also qualifies. It is the one
/// intrinsic that can *fail* — not by malfunctioning, but by looking up a
/// name the environment does not contain — so `home=$(getenv HOME) ||
/// home=/root` is a real condition. The carve-out is `getenv` specifically,
/// not intrinsics generally: `$(len a)` and friends are pure functions of
/// their arguments and stay in the can't-fail set.
fn is_capture_condition_rhs(value: &Expr) -> bool {
    let ExprKind::CommandSubstitution(stmts) = &value.kind else {
        return false;
    };
    let [stmt] = stmts.as_slice() else {
        return false;
    };
    let mut kind = &stmt.kind;
    loop {
        match kind {
            StmtKind::Redirected(r) => kind = &r.body.kind,
            StmtKind::Suppressed(inner) => kind = &inner.kind,
            _ => break,
        }
    }
    match kind {
        StmtKind::Command(c) => is_external_command_head(c) || is_getenv_lookup(c),
        // `$(a | b)` — every stage must be an external command.
        StmtKind::Pipeline(p) => {
            p.commands.len() > 1 && p.commands.iter().all(stmt_is_external_command)
        }
        _ => false,
    }
}

/// DD#101: `$(getenv NAME)` — the one intrinsic whose status is a real
/// condition. Arity is pinned at exactly one operand here as well as in
/// `ssa::value_intrinsic_arity`, so a malformed `$(getenv)` / `$(getenv A B)`
/// keeps its SB4045 rather than being silently admitted as a condition.
fn is_getenv_lookup(c: &crate::ir::Command) -> bool {
    matches!(&c.name.kind, ExprKind::Literal(n) if n == "getenv") && c.args.len() == 1
}

/// A command whose head is (or resolves to) an external program. `validate`
/// runs BEFORE command resolution, so the name is still bare — use the
/// resolution-time external test (excludes intrinsics + builtins) rather than a
/// post-resolution `/` check. A user function's bare name also passes here (the
/// function table isn't available in `validate`); the transform rejects the
/// `$(user_fn)` capture-condition case (DD#88).
fn is_external_command_head(c: &crate::ir::Command) -> bool {
    matches!(&c.name.kind, ExprKind::Literal(n) if crate::resolve::is_external_looking(n))
}

/// A pipeline stage (peeling `-`/`!`) that is a single external command.
fn stmt_is_external_command(stmt: &Statement) -> bool {
    let mut kind = &stmt.kind;
    while let StmtKind::Suppressed(inner) | StmtKind::Negated(inner) = kind {
        kind = &inner.kind;
    }
    matches!(kind, StmtKind::Command(c) if is_external_command_head(c))
}

/// DD#70/#71 dual (SB4025): a value binding (assignment / declaration) used
/// where a boolean or exit status is consumed (an `if`/`while`/`until`
/// condition, the left of `&&`/`||`, or the operand of `!`). When the bound
/// value is a command substitution the generic "sequence it" advice is wrong
/// (a failed `$(…)` aborts before any follow-up check), so give honest,
/// capture-specific guidance instead.
fn nonstatus_in_condition_diag(span: Span, what: &str, is_capture: bool) -> Diagnostic {
    let advice = if is_capture {
        // DD#88: a capture-condition IS supported, but only for a single
        // EXTERNAL `$(cmd)` / pipeline. This RHS captures something else — an
        // intrinsic/builtin (which can't fail) or a concatenation — so there is
        // no single external status to branch on.
        format!(
            "{what} captures a value, but a capture-condition branches on an exit status — a \
             single external command (`if x=$(cmd) {{ … }}`) or `$(getenv NAME)`, whose status \
             is the lookup result (DD#101). This RHS isn't one — it captures a sash \
             intrinsic/builtin whose result is a pure function of its arguments and so cannot \
             fail, or concatenates `$(…)` with other text. Bind it, then test: \
             `x=…; if [[ … ]]`."
        )
    } else {
        format!(
            "{what} is a value binding, not a boolean or exit status — it cannot be used as a \
             condition; sequence it instead (e.g. `x=foo; if …`)"
        )
    };
    Diagnostic::error_span(span, "SB4025", advice)
}

/// True when an assignment/declaration value is (or contains) a command
/// substitution — `out=$(cmd)`, `out="$(cmd)"`, etc.
fn value_is_capture(expr: &Expr) -> bool {
    let mut found = false;
    crate::ir::walk_expr_tree(expr, &mut |e| {
        if matches!(e.kind, ExprKind::CommandSubstitution(_)) {
            found = true;
        }
    });
    found
}

fn validate_if(i: &If, diags: &mut Vec<Diagnostic>) {
    for s in &i.condition {
        validate_stmt_ctx(s, diags, true);
    }
    for s in &i.then_body {
        validate_stmt_ctx(s, diags, false);
    }
    for elif in &i.elif_clauses {
        for s in &elif.condition {
            validate_stmt_ctx(s, diags, true);
        }
        for s in &elif.body {
            validate_stmt_ctx(s, diags, false);
        }
    }
    if let Some(ref else_body) = i.else_body {
        for s in else_body {
            validate_stmt_ctx(s, diags, false);
        }
    }
}

fn validate_while(w: &While, diags: &mut Vec<Diagnostic>) {
    check_loop_read_condition(w, diags);
    for s in &w.condition {
        validate_stmt_ctx(s, diags, true);
    }
    for s in &w.body {
        validate_stmt_ctx(s, diags, false);
    }
}

/// SB5069 (DD#99): a loop whose condition is a `read`.
///
/// `read` fills its target from the *whole* stream, so there is no second
/// read for the loop to make — `while read ARR` spins forever on an empty
/// array rather than iterating. The two-argument form (`read $s ARR`) splits
/// a value already in hand, which cannot change between iterations either.
/// `readline v` is the iteration form, and the message names it.
fn check_loop_read_condition(w: &While, diags: &mut Vec<Diagnostic>) {
    let keyword = if w.until { "until" } else { "while" };
    for stmt in &w.condition {
        let Some(span) = first_read_span(stmt) else {
            continue;
        };
        diags.push(Diagnostic::error_span(
            span,
            "SB5069",
            format!(
                "`{keyword} read …` never terminates — `read` consumes the \
                 whole stream, so the loop has no second read to make. \
                 To iterate one line at a time use `{keyword} readline v`."
            ),
        ));
    }
}

/// The span of the first `read` anywhere inside `stmt`, so the pipeline form
/// (`tr … | read ARR`) is caught as well as the bare one.
fn first_read_span(stmt: &Statement) -> Option<Span> {
    let mut found = None;
    crate::ir::walk_stmt_tree(std::slice::from_ref(stmt), &mut |s| {
        if found.is_none() && is_read_command(s) {
            found = Some(s.span);
        }
    });
    found
}

/// The DD#99 `read` builtin as a statement — the array form (`read ARR`) or
/// the split form (`read $s ARR`). Retired spellings (flags, multi-target)
/// are SB5057's business and are reported there.
fn is_read_command(stmt: &Statement) -> bool {
    let StmtKind::Command(c) = &stmt.kind else {
        return false;
    };
    matches!(&c.name.kind, ExprKind::Literal(n) if n == "read")
}

fn validate_for(f: &For, diags: &mut Vec<Diagnostic>) {
    // Check glob patterns and unquoted expansions in for-loop words
    for w in &f.words {
        check_for_word(w, diags);
    }
    for s in &f.body {
        validate_stmt_ctx(s, diags, false);
    }
}

/// Check one for-in source word: glob metacharacters (SB5006) and an
/// unquoted scalar expansion that would imply word splitting (SB5009).
/// `$@` / `$*` are array splats (handled by the positional/splat passes),
/// not word-splitting scalars, so they are exempt.
fn check_for_word(w: &Expr, diags: &mut Vec<Diagnostic>) {
    check_unquoted_glob(w, diags);
    if let ExprKind::SimpleExpansion(var) = &w.kind {
        if var != "@" && var != "*" {
            diags.push(Diagnostic::error_span(
                w.span,
                "SB5009",
                format!(
                    "`${}` in a for-loop does not word-split — word splitting \
                     is eliminated by design; iterate an array instead: \
                     `for x in @arr`",
                    var
                ),
            ));
        }
    }
}

/// DD#31: a comma expression like `(a=1, x<5)` desugars to a
/// Group whose body is a sequence of mutations followed by a
/// final ArithTest. In condition context, the mutations must
/// validate as *non*-condition (they're statements), while the
/// trailing ArithTest validates as condition. Outside that
/// shape, forward `in_condition` unchanged to every body
/// statement.
fn validate_group(g: &Group, in_condition: bool, diags: &mut Vec<Diagnostic>) {
    let is_comma_pattern = in_condition
        && g.body
            .last()
            .is_some_and(|s| matches!(s.kind, StmtKind::ArithTest(_)));
    if !is_comma_pattern {
        for s in &g.body {
            validate_stmt_ctx(s, diags, in_condition);
        }
        return;
    }
    let last = g.body.len() - 1;
    for (i, s) in g.body.iter().enumerate() {
        validate_stmt_ctx(s, diags, i == last);
    }
}

// ── SB5001/SB5002/SB4001: command-level rejections ─────────────

fn validate_command(c: &Command, span: Span, diags: &mut Vec<Diagnostic>) {
    let name = expr_literal_str(&c.name);

    if check_heredoc(name, c, span, diags) {
        return;
    }
    check_dynexec(name, span, diags);
    check_trap(name, span, diags);
    check_signal(name, c, span, diags);
    check_test_constructs(name, c, span, diags);
    check_attest_convert_names(name, c, span, diags);
    check_inline_glob(c, diags);
}

/// SB5053/SB5054 (DD#93 Part D): the shape rules for an arith attest predicate.
///
/// SB5054 — a `(( … ))` an attest carries must be `(( TARGET = EXPR ))` with
/// `EXPR` referencing the attested name. A near-miss used to fall through to
/// the generic command path and reach the value lowering as a mutation, which
/// panicked (`unreachable!` in `expr_handlers`); it is a clean error now.
///
/// SB5053 — the names must be distinct. Same-name in-place retype
/// (`attest s (( s = s ))`) is deliberately deferred: the source is a string
/// and the target an int, and collapsing them would retype a live name
/// mid-statement.
fn check_attest_convert_names(name: &str, c: &Command, span: Span, diags: &mut Vec<Diagnostic>) {
    if name != "attest" {
        return;
    }
    let Some((AttestForm::Scalar { var }, AttestPredicate::ArithConvert { target, .. })) =
        classify_attest(&c.args)
    else {
        // Carries a `(( … ))` but does not decode as the form.
        if let Some(miss) = attest_arith_near_miss(c) {
            diags.push(Diagnostic::error_span(span, "SB5054", miss.message()));
        }
        return;
    };
    if var == target {
        diags.push(Diagnostic::error_span(
            span,
            "SB5053",
            format!(
                "attest `(( {target} = {var} ))` needs distinct names — the source is \
                 a string and the target an int; use a different target name"
            ),
        ));
    }
}

/// DD#69 (globs are array sugar): an unquoted `*` glob in command-argument
/// position is a compile error. A glob must go through an array literal
/// (`array a=(*)`) and splat (`@a`) — there is no inline glob expansion.
/// Quoted `"*"` is a literal asterisk and is unaffected.
fn check_inline_glob(c: &Command, diags: &mut Vec<Diagnostic>) {
    for arg in &c.args {
        if arg_has_unquoted_star(&arg.kind) {
            diags.push(Diagnostic::error_span(
                arg.span,
                "SB5029",
                "inline `*` glob is not allowed in command arguments — use an \
                 array: `array a=(*); … @a` (or quote \"*\" for a literal \
                 asterisk)"
                    .to_string(),
            ));
        }
    }
}

/// True if the word is an unquoted literal (or concatenation with a
/// literal part) containing `*` — i.e. an inline glob attempt.
fn arg_has_unquoted_star(kind: &ExprKind) -> bool {
    match kind {
        ExprKind::Literal(s) => s.contains('*'),
        ExprKind::Concat(parts) => parts
            .iter()
            .any(|p| matches!(&p.kind, ExprKind::Literal(s) if s.contains('*'))),
        _ => false,
    }
}

fn heredoc_diag(span: Span) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5002",
        "sash has no heredocs or here-strings — pipe the data instead: \
         `printf '…' | consumer` or `echo $x | consumer`"
            .to_string(),
    )
}

/// SB5002: reject heredocs as the command name or any argument.
/// Returns true if a heredoc was pushed (caller should stop further checks).
fn check_heredoc(name: &str, c: &Command, span: Span, diags: &mut Vec<Diagnostic>) -> bool {
    if name == "__HEREDOC__" {
        diags.push(heredoc_diag(span));
        return true;
    }
    for arg in &c.args {
        if let ExprKind::Literal(s) = &arg.kind {
            if s == "__HEREDOC__" {
                diags.push(heredoc_diag(span));
                return true;
            }
        }
    }
    false
}

/// SB4001: reject dynamic-execution builtins.
fn check_dynexec(name: &str, span: Span, diags: &mut Vec<Diagnostic>) {
    if name == "eval" || name == "source" || name == "." || name == "command" {
        diags.push(Diagnostic::error_span(
            span,
            "SB4001",
            format!(
                "`{name}` is rejected — sash forbids dynamic command execution; a program's \
                 behavior is fixed at compile time and there is no runtime interpreter to run \
                 constructed code"
            ),
        ));
    } else if name == "coproc" {
        diags.push(Diagnostic::error_span(
            span,
            "SB4001",
            "sash has no `coproc` — asynchronous coprocesses are outside its model".to_string(),
        ));
    }
}

/// SB4043 (DD#56): `trap` is not part of native sash. The signal model is the
/// `signal N {IGN,DFL,FLAG,EXIT}` builtin plus the `END { }` block; the
/// converter rewrites bash `trap` to those. Reject a hand-written `trap`,
/// pointing at the native forms.
fn check_trap(name: &str, span: Span, diags: &mut Vec<Diagnostic>) {
    if name == "trap" {
        diags.push(Diagnostic::error_span(
            span,
            "SB4043",
            "`trap` is not part of sash — use the `signal` builtin \
             (`signal N IGN|DFL|FLAG|EXIT`) and the `END { }` block (DD#56); \
             the converter rewrites bash `trap` to these"
                .to_string(),
        ));
    }
}

/// The dispositions `signal` accepts (DD#56).
const SIGNAL_DISPOSITIONS: [&str; 4] = ["IGN", "DFL", "FLAG", "EXIT"];

/// A signal the kernel will actually let a program catch or ignore. 9 (KILL)
/// and 19 (STOP) are excluded by the kernel itself, so registering a
/// disposition for either is a promise the program cannot keep.
fn catchable_signum(n: i64) -> bool {
    (1..=64).contains(&n) && n != 9 && n != 19
}

/// SB4059/SB4060/SB4061/SB4062 (DD#56): the shape rules for `signal`.
///
/// Every malformed spelling used to compile clean and silently do nothing.
/// `handle_signal` gated on `args.len() >= 2`, parsed the number with
/// `unwrap_or(0)`, tested `signum > 0`, and matched the disposition with a
/// bare `_ => {}` — four silent drops in one function. `signal TERM IGN`, the
/// bash spelling and the one `check_trap`'s own message all but invites, was a
/// no-op; so were `signal 2`, `signal 2 BOGUS`, `signal 9 IGN` and
/// `signal $n EXIT`.
fn check_signal(name: &str, c: &Command, span: Span, diags: &mut Vec<Diagnostic>) {
    if name != "signal" {
        return;
    }
    if c.args.len() != 2 {
        diags.push(Diagnostic::error_span(
            span,
            "SB4059",
            format!(
                "`signal` takes exactly two arguments, a number and a \
                 disposition (`signal N IGN|DFL|FLAG|EXIT`, DD#56) — found {}",
                c.args.len()
            ),
        ));
        return;
    }
    check_signum_arg(&c.args[0], diags);
    check_disposition_arg(&c.args[1], diags);
}

/// SB4060/SB4061: the number a `signal` declaration names.
fn check_signum_arg(arg: &Expr, diags: &mut Vec<Diagnostic>) {
    if !matches!(arg.kind, ExprKind::Literal(_)) {
        diags.push(Diagnostic::error_span(
            arg.span,
            "SB4060",
            "a `signal` number must be an integer literal, not an expansion — \
             the compiler resolves the `EXIT` check-point set at compile time, \
             so it has to know the number then (DD#56)"
                .to_string(),
        ));
        return;
    }
    let text = expr_literal_str(arg);
    match text.parse::<i64>() {
        Ok(n) if catchable_signum(n) => {}
        Ok(n) => diags.push(Diagnostic::error_span(
            arg.span,
            "SB4061",
            format!(
                "signal {n} cannot be caught or ignored — `signal` accepts 1-64 \
                 apart from 9 (KILL) and 19 (STOP), which the kernel never \
                 delivers to a handler (DD#56)"
            ),
        )),
        Err(_) => diags.push(Diagnostic::error_span(
            arg.span,
            "SB4060",
            format!(
                "`{text}` is not a signal number — sash does not know signal \
                 names (DD#56); write the number, or define a GPP macro for it \
                 (`#define TERM 15`)"
            ),
        )),
    }
}

/// SB4062: the disposition a `signal` declaration names.
fn check_disposition_arg(arg: &Expr, diags: &mut Vec<Diagnostic>) {
    let text = expr_literal_str(arg);
    if SIGNAL_DISPOSITIONS.contains(&text) {
        return;
    }
    let shown = if matches!(arg.kind, ExprKind::Literal(_)) {
        format!("`{text}`")
    } else {
        "an expansion".to_string()
    };
    diags.push(Diagnostic::error_span(
        arg.span,
        "SB4062",
        format!(
            "{shown} is not a signal disposition — `signal` takes one of IGN, \
             DFL, FLAG or EXIT (DD#56)"
        ),
    ));
}

/// SB5019/SB5023: numeric test ops + literal regex validation inside `[[ ]]`.
fn check_test_constructs(name: &str, c: &Command, span: Span, diags: &mut Vec<Diagnostic>) {
    // `attest` carries its predicate as a `Test` arg exactly like `[[ ]]`, so
    // the same operator rules have to apply to both. Without `attest` here,
    // `attest x [[ -v $x ]]` (and the DD#93 Part B shorthand `attest x -v`)
    // slipped past SB5028 and reached codegen, which has no lowering for
    // -v/-o/-R and panics — a user-reachable ICE.
    if name != "[[" && name != "attest" {
        return;
    }
    for arg in &c.args {
        if let ExprKind::Test(ref test) = arg.kind {
            check_unsupported_test_ops(test, diags);
            check_vacuous_test(test, diags);
            check_regex_literal(test, span, diags);
        }
    }
}

fn validate_declaration(d: &Declaration, diags: &mut Vec<Diagnostic>) {
    // SB4064: the initializer's shape must match the declared kind.
    check_decl_value_shape(d, diags);
    // SB4031 (DD#33): a map literal must use the `[key]=value` form.
    if d.flags.iter().any(|f| f == "-A") {
        for a in &d.assignments {
            reject_flat_map_literal(a, diags);
        }
    }
    // SB4026 (Policy A): an `int`/`const` initializer that is a decimal
    // literal must fit i64. (Arith literals are range-checked in the arith
    // parser; this is the non-arith `int x=NNN` path. i64::MIN works because
    // the `-` is part of the literal here, unlike in arithmetic.)
    if matches!(d.var_type, VarType::Int) {
        check_int_literal_range(d, diags);
    }
}

/// SB4064: a declaration's initializer must have the shape of the kind being
/// declared — a collection literal for `array`/`map`, and anything but one for
/// a scalar.
///
/// Nothing checked this before 2026-09-22, in either direction, and both
/// directions were broken:
///
/// * `array a=x` (and `array a=""`, `map m=v`, `array a=$(echo x)`) stored a
///   *scalar* into a collection slot. It compiled clean and **segfaulted** on
///   the first read; `len a` returned uninitialised memory rather than
///   faulting. bash reads `declare -a a=x` as `a[0]=x`, but sash has no
///   implicit one-element promotion, so the value simply never matched the
///   slot.
/// * `str s=(x)` accepted an array literal into a scalar and silently produced
///   the empty string (bash yields `x`).
///
/// A `ParameterExpansion` is allowed on a collection because that is the
/// DD#99 splat form (`map m=@{a:-}`); its own rules are enforced by
/// `check_map_decl_splats` and SB5025.
fn check_decl_value_shape(d: &Declaration, diags: &mut Vec<Diagnostic>) {
    let is_collection = d.flags.iter().any(|f| f == "-a" || f == "-A");
    for a in &d.assignments {
        if let Some(diag) = decl_shape_diag(a, is_collection) {
            diags.push(diag);
        }
    }
}

/// What kind of thing an initializer denotes.
///
/// `Splat` is neither: `map m=@{a:-}` is the DD#99 fill-from-array form, whose
/// own rules live in `check_map_decl_splats` and SB5025, and a plain `$x` on a
/// scalar is the ordinary case. Both must pass through here untouched.
enum InitShape {
    Collection,
    Scalar,
    Splat,
}

fn init_shape(value: &Expr) -> InitShape {
    match &value.kind {
        ExprKind::Array(_) | ExprKind::AssocArray(_) => InitShape::Collection,
        ExprKind::ParameterExpansion(_) => InitShape::Splat,
        _ => InitShape::Scalar,
    }
}

/// `Some(diag)` when the initializer's shape contradicts the declared kind.
fn decl_shape_diag(a: &DeclAssignment, is_collection: bool) -> Option<Diagnostic> {
    let value = a.value.as_ref()?;
    // `array a=$(argv …)` is a scalar into a collection, but SB5025 says
    // something SB4064 cannot: use the out-param form `argv N a`. SB4064's
    // own advice — "write `a=(…)`" — is actively wrong there, because
    // `array a=($(argv 1))` is rejected in turn (SB5021/SB5025). SB4064 runs
    // in an earlier phase that exits before SB5025's, so deferring is what
    // keeps the better diagnostic reachable at all.
    if is_collection && is_argv_splat_expr(value) {
        return None;
    }
    match (is_collection, init_shape(value)) {
        (true, InitShape::Scalar) => Some(scalar_into_collection_diag(&a.name, value)),
        (false, InitShape::Collection) => Some(collection_into_scalar_diag(&a.name, value)),
        _ => None,
    }
}

fn scalar_into_collection_diag(name: &str, value: &Expr) -> Diagnostic {
    Diagnostic::error_span(
        value.span,
        "SB4064",
        format!(
            "`{name}` is declared as a collection, so its initializer must be a \
             `( … )` literal — a scalar value does not become a one-element \
             collection in sash; write `{name}=({})` if that is what you meant",
            decl_value_brief(value),
        ),
    )
}

fn collection_into_scalar_diag(name: &str, value: &Expr) -> Diagnostic {
    Diagnostic::error_span(
        value.span,
        "SB4064",
        format!(
            "`{name}` is declared as a scalar, so its initializer cannot be a \
             `( … )` collection literal — declare it with `array` or `map` to \
             hold several values"
        ),
    )
}

/// How to echo a rejected scalar initializer back in the SB4064 hint. A plain
/// literal is quoted as written; anything else is named by shape, since
/// splicing a command substitution into the hint would suggest it is valid
/// inside the parentheses when its own rules still apply.
fn decl_value_brief(value: &Expr) -> String {
    match &value.kind {
        ExprKind::Literal(s) if !s.is_empty() => s.clone(),
        _ => "…".to_string(),
    }
}

/// SB4026: reject an out-of-range decimal literal assigned to an int var.
fn check_int_literal_range(d: &Declaration, diags: &mut Vec<Diagnostic>) {
    for a in &d.assignments {
        let Some(value) = &a.value else {
            continue;
        };
        let ExprKind::Literal(s) = &value.kind else {
            continue;
        };
        if int_literal_out_of_range(s) {
            diags.push(Diagnostic::error_span(
                value.span,
                "SB4026",
                format!("integer literal out of range: {s}"),
            ));
        }
    }
}

/// True when `s` is a decimal integer literal (optional leading `-`,
/// then ASCII digits) whose magnitude does not fit i64. Non-integer
/// strings (identifiers, hex like `0xFF`, empty) are NOT flagged here.
fn int_literal_out_of_range(s: &str) -> bool {
    let body = s.strip_prefix('-').unwrap_or(s);
    !body.is_empty() && body.bytes().all(|b| b.is_ascii_digit()) && s.parse::<i64>().is_err()
}

/// SB4031 (DD#33): a flat `(a 1 b 2)` list parses as an indexed-array
/// literal, silently building an N-element array instead of a map — so a
/// `map` whose initializer is a non-empty plain `Array` is rejected. The
/// `[key]=value` form parses as `AssocArray` and an empty `()` is allowed.
fn reject_flat_map_literal(a: &DeclAssignment, diags: &mut Vec<Diagnostic>) {
    let Some(value) = &a.value else {
        return;
    };
    let ExprKind::Array(elems) = &value.kind else {
        return;
    };
    if elems.is_empty() {
        return;
    }
    diags.push(Diagnostic::error_span(
        value.span,
        "SB4031",
        format!(
            "map `{}` must use the [key]=value form \
             (e.g. `map {}=([k]=v …)`) — a flat list is not a map literal",
            a.name, a.name,
        ),
    ));
}

fn validate_assignment(a: &Assignment, span: Span, diags: &mut Vec<Diagnostic>) {
    // SB5009: IFS assignment rejected
    if a.name == "IFS" {
        diags.push(Diagnostic::error_span(
            span,
            "SB5009",
            "IFS does not exist in sash — word splitting is eliminated by design".to_string(),
        ));
    }
}

fn validate_expr(expr: &Expr, _parent_span: Span, diags: &mut Vec<Diagnostic>) {
    match &expr.kind {
        ExprKind::SimpleExpansion(var) => {
            validate_var_name(var, expr.span, diags);
        }
        // A braced parameter expansion `${name…}` references `name` just like
        // `$name` — validate the name identically (any operator/index) so a
        // braced rejected / env var (`${RANDOM}` → SB5017, `${HOME}` /
        // `${HOME:-x}` → SB5018) is not a silent bypass. `validate_var_name`
        // only flags names in the rejected/env/positional lists, so a normal
        // `${x}` / `${x:-9}` / `${a[i]:-N}` stays clean.
        ExprKind::ParameterExpansion(pe) => {
            validate_var_name(&pe.name, expr.span, diags);
        }
        // SB5006: ** globstar is always rejected (even quoted, it's never valid)
        ExprKind::Literal(s) if s.contains("**") => {
            diags.push(Diagnostic::error_span(
                expr.span,
                "SB5006",
                format!("sash has only `*` globs — `**` is omitted: {}", s),
            ));
        }
        ExprKind::Array(elems) => check_retired_regex_filter(elems, diags),
        _ => {}
    }
}

/// SB5075 (DD#108): the `arr=(GLOB =~ 're')` regex-filter shorthand is retired.
///
/// It was a list literal whose meaning depended on its own contents, which is
/// the one thing a list must never be. `(* =~ re)` filtered; `(x =~ y)` was a
/// plain three-element array; `(* =~ a b)` silently became a four-element one,
/// with no diagnostic for the difference. Quoting could not disambiguate it
/// either — `(* =~ re)`, `(* '=~' re)`, `('*' =~ re)` and `("*" =~ re)` all
/// desugared identically — so making quotes the escape would have meant
/// letting quotes change a list's *arity and meaning* rather than a value.
/// That is precisely the bash misfeature the subset exists to avoid.
///
/// The loop expresses it directly and, unlike the shorthand, names what it
/// matches, so there is no basename-versus-path ambiguity to resolve.
///
/// A **quoted** `=~` stays legal: it is an ordinary string element, and the
/// message names it as the escape.
fn check_retired_regex_filter(elems: &[Expr], diags: &mut Vec<Diagnostic>) {
    for e in elems {
        if !matches!(&e.kind, ExprKind::Literal(s) if s == "=~") {
            continue;
        }
        diags.push(Diagnostic::error_span(
            e.span,
            "SB5075",
            "the `arr=(GLOB =~ 're')` regex-filter shorthand is retired — \
             build the array with a loop, which also names what it matches: \
             `array a; for f in GLOB { if [[ $f =~ 're' ]] { a += ($f) } }`. \
             For a literal `=~` element, quote it: `arr=(x '=~' y)`."
                .to_string(),
        ));
    }
}

// ── SB5017/5018: Rejected variables ─────────────────────────────────

const REJECTED_VARS: &[(&str, &str)] = &[
    ("RANDOM", "$RANDOM does not exist in sash"),
    ("BASH_VERSION", "$BASH_VERSION does not exist in sash"),
    ("BASH_SOURCE", "$BASH_SOURCE does not exist in sash"),
    ("BASH_LINENO", "$BASH_LINENO does not exist in sash"),
    ("FUNCNAME", "$FUNCNAME does not exist in sash"),
    ("GROUPS", "$GROUPS does not exist in sash"),
    ("DIRSTACK", "$DIRSTACK does not exist in sash"),
    ("PIPESTATUS", "$PIPESTATUS does not exist in sash"),
    ("SHELLOPTS", "$SHELLOPTS does not exist in sash"),
    ("BASHOPTS", "$BASHOPTS does not exist in sash"),
    ("MACHTYPE", "$MACHTYPE does not exist in sash"),
    ("OSTYPE", "$OSTYPE does not exist in sash"),
    ("HOSTTYPE", "$HOSTTYPE does not exist in sash"),
    (
        "HOSTNAME",
        "$HOSTNAME does not exist in sash — use $(getenv HOSTNAME)",
    ),
    ("SECONDS", "$SECONDS does not exist in sash"),
    ("EPOCHSECONDS", "$EPOCHSECONDS does not exist in sash"),
    ("EPOCHREALTIME", "$EPOCHREALTIME does not exist in sash"),
    ("SRANDOM", "$SRANDOM does not exist in sash"),
    (
        "IFS",
        "IFS does not exist in sash — word splitting is eliminated by design",
    ),
];

const REJECTED_POSITIONAL: &[(&str, &str)] = &[("0", "$0 is not available in sash — use $(arg 0)")];

/// SB5018: Bare env vars require $(getenv NAME)
const ENV_VARS: &[&str] = &[
    "HOME",
    "PATH",
    "USER",
    "SHELL",
    "TERM",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "DISPLAY",
    "XDG_RUNTIME_DIR",
    "TMPDIR",
    "EDITOR",
    "VISUAL",
    "PAGER",
    "LOGNAME",
    "MAIL",
    "PWD",
    "OLDPWD",
];

fn validate_var_name(var: &str, span: Span, diags: &mut Vec<Diagnostic>) {
    // SB5017: Rejected vars
    for (name, msg) in REJECTED_VARS {
        if var == *name {
            diags.push(Diagnostic::error_span(span, "SB5017", msg.to_string()));
            return;
        }
    }
    for (name, msg) in REJECTED_POSITIONAL {
        if var == *name {
            diags.push(Diagnostic::error_span(span, "SB5017", msg.to_string()));
            return;
        }
    }
    // SB5018: Bare env vars
    for name in ENV_VARS {
        if var == *name {
            diags.push(Diagnostic::error_span(
                span,
                "SB5018",
                format!(
                    "${} is not available directly — use $(getenv {})",
                    name, name
                ),
            ));
            return;
        }
    }
    // SB5009: IFS (also in REJECTED_VARS but keeping explicit for clarity)
}

// ── Helpers ─────────────────────────────────────────────────────────

fn expr_literal_str(expr: &Expr) -> &str {
    match &expr.kind {
        ExprKind::Literal(s) => s.as_str(),
        _ => "",
    }
}

/// Check if an expression tree references a specific variable name.
fn expr_references_var(expr: &Expr, var: &str) -> bool {
    match &expr.kind {
        ExprKind::SimpleExpansion(v) => v == var,
        ExprKind::ParameterExpansion(pe) => pe.name == var,
        ExprKind::DoubleQuoted(parts) => parts.iter().any(|p| expr_references_var(p, var)),
        ExprKind::Concat(parts) => parts.iter().any(|p| expr_references_var(p, var)),
        ExprKind::Test(t) => test_references_var(t, var),
        ExprKind::Subscript { name, .. } => name == var,
        _ => false,
    }
}

fn test_references_var(test: &TestExpr, var: &str) -> bool {
    match test {
        // For binary tests, variable MUST be the LHS
        TestExpr::Binary { left, .. } => expr_references_var(left, var),
        TestExpr::Unary { operand, .. } => expr_references_var(operand, var),
        TestExpr::Not(inner) => test_references_var(inner, var),
        TestExpr::And(l, r) | TestExpr::Or(l, r) => {
            test_references_var(l, var) || test_references_var(r, var)
        }
    }
}

/// SB5023: validate a *static* regex pattern at compile time. The operand
/// word may be a `Literal`, a quoted string, or a `Concat` of static parts
/// (bare `(?=h)ello`); any interpolation makes it dynamic → validated at
/// runtime instead (DD#71).
fn check_regex_literal(test: &TestExpr, span: Span, diags: &mut Vec<Diagnostic>) {
    let TestExpr::Binary { op, right, .. } = test else {
        return;
    };
    if op != "=~" {
        return;
    }
    let Some(pattern) = static_regex_pattern(right) else {
        return;
    };
    if let Err(msg) = crate::regex_validate::validate_regex(&pattern) {
        diags.push(Diagnostic::error_span(
            span,
            "SB5023",
            format!("invalid regex pattern: {}", msg),
        ));
    }
}

/// The compile-time-constant string of a `=~` operand, or `None` if any part
/// is a runtime expansion (dynamic — not statically checkable). Quotes are
/// stripped (they are grouping, not literalisation — DD#71).
fn static_regex_pattern(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Literal(s) | ExprKind::SingleQuoted(s) => Some(s.clone()),
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) => {
            let mut out = String::new();
            for p in parts {
                out.push_str(&static_regex_pattern(p)?);
            }
            Some(out)
        }
        _ => None,
    }
}

/// A `[[ ]]` unary operator the parser accepts (is_unary_test_op) but sash does
/// not implement — codegen (tests.rs) handles only -n/-z/-i and the file
/// tests. These must be rejected here (SB5028) before codegen ICEs on
/// them. Returns the user-facing hint for the operator, or None if supported.
///   -v  DD#62: an unset/empty variable is just the empty string
///   -o  shell-option state — sash has no shell options
///   -R  nameref state — sash has no namerefs
///   -t  DG 2026-09-26: retired. It was the sole *int*-typed unary test, so it
///       sat alone against a family of string and file predicates, and it is an
///       interactivity question a compiled static binary has little business
///       asking. A `terminal?` predicate was considered and dropped as YAGNI:
///       sash arithmetic has no unary predicates at all, so it would have been
///       the first of its kind, and `[[ ]]` is already the predicate context.
fn unsupported_unary_test_hint(op: &str) -> Option<&'static str> {
    match op {
        "-v" => Some("use -n to test for a non-empty value"),
        "-o" => Some("shell-option tests are unsupported"),
        "-R" => Some("namerefs are unsupported"),
        "-t" => Some("terminal tests are unsupported"),
        _ => None,
    }
}

/// SB5019/SB5028: reject operators that are not supported in `[[ ]]`
/// (numeric comparisons → `(( ))`; `-v`/`-o`/`-R` → no analogue).
fn check_unsupported_test_ops(test: &TestExpr, diags: &mut Vec<Diagnostic>) {
    match test {
        TestExpr::Unary { op, operand } => {
            if let Some(hint) = unsupported_unary_test_hint(op) {
                diags.push(Diagnostic::error_span(
                    operand.span,
                    "SB5028",
                    format!("sash has no `[[ {} ]]` — {}", op, hint),
                ));
            }
        }
        TestExpr::Binary { left, op, right: _ } => {
            const NUMERIC_OPS: &[&str] = &["-gt", "-lt", "-ge", "-le", "-eq", "-ne"];
            if NUMERIC_OPS.contains(&op.as_str()) {
                let span = left.span;
                diags.push(Diagnostic::error_span(span, "SB5019",
                    format!("'{}' is a numeric operator — use (( )) for arithmetic comparisons, not [[ ]]", op),
                ));
            }
        }
        TestExpr::Not(inner) => check_unsupported_test_ops(inner, diags),
        TestExpr::And(l, r) | TestExpr::Or(l, r) => {
            check_unsupported_test_ops(l, diags);
            check_unsupported_test_ops(r, diags);
        }
    }
}

/// Whether a word can be empty, decided from the source text alone.
///
/// This is deliberately **syntactic**: it does not const-propagate, so
/// `str s=foo` followed by `[[ -n $s ]]` is `Unknown`. That is what makes the
/// rewrite out of a vacuous test — `[[ -n foo ]]` → `str s=foo; [[ -n $s ]]` —
/// a legal one, and it keeps the rule predictable: whether a test is vacuous
/// depends only on how the test itself is written (DD#118).
#[derive(PartialEq, Clone, Copy)]
enum Emptiness {
    Always,
    Never,
    Unknown,
}

fn word_emptiness(e: &Expr) -> Emptiness {
    match &e.kind {
        ExprKind::Literal(s) | ExprKind::SingleQuoted(s) => {
            if s.is_empty() {
                Emptiness::Always
            } else {
                Emptiness::Never
            }
        }
        // `$(( … ))` always renders at least one digit.
        ExprKind::Arithmetic(_) => Emptiness::Never,
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) => parts_emptiness(parts),
        _ => Emptiness::Unknown,
    }
}

fn parts_emptiness(parts: &[Expr]) -> Emptiness {
    if parts.iter().any(|p| word_emptiness(p) == Emptiness::Never) {
        Emptiness::Never
    } else if parts.iter().all(|p| word_emptiness(p) == Emptiness::Always) {
        Emptiness::Always
    } else {
        Emptiness::Unknown
    }
}

/// Whether a word's whole value is fixed by the source text.
fn is_static_word(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Literal(_) | ExprKind::SingleQuoted(_) => true,
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) => parts.iter().all(is_static_word),
        _ => false,
    }
}

/// `Some(answer)` when `-n`/`-z` on this operand can only ever give one answer.
fn vacuous_emptiness_answer(op: &str, operand: &Expr) -> Option<bool> {
    match (op, word_emptiness(operand)) {
        ("-n", Emptiness::Never) | ("-z", Emptiness::Always) => Some(true),
        ("-n", Emptiness::Always) | ("-z", Emptiness::Never) => Some(false),
        _ => None,
    }
}

/// SB5086 (DD#118): reject a test whose result is fixed at compile time.
///
/// Two shapes qualify. `-n`/`-z` on a word whose emptiness the source text
/// already settles (`[[ -n foo ]]`, and the glued `[[ $x=y ]]` that reads as
/// one never-empty word). And a comparison whose operands are both fixed by
/// the source text (`[[ foo == foo ]]`). File tests are excluded: their answer
/// depends on the filesystem, not the source.
fn check_vacuous_test(test: &TestExpr, diags: &mut Vec<Diagnostic>) {
    match test {
        TestExpr::Unary { op, operand } => check_vacuous_unary(op, operand, diags),
        TestExpr::Binary { left, op, right } => check_vacuous_binary(left, op, right, diags),
        TestExpr::Not(inner) => check_vacuous_test(inner, diags),
        TestExpr::And(l, r) | TestExpr::Or(l, r) => {
            check_vacuous_test(l, diags);
            check_vacuous_test(r, diags);
        }
    }
}

fn check_vacuous_unary(op: &str, operand: &Expr, diags: &mut Vec<Diagnostic>) {
    if let Some(answer) = vacuous_emptiness_answer(op, operand) {
        diags.push(Diagnostic::error_span(
            operand.span,
            "SB5086",
            format!(
                "this test is always {answer} — the source text already settles \
                 whether the word is empty; test a variable instead"
            ),
        ));
    }
}

fn check_vacuous_binary(left: &Expr, op: &str, right: &Expr, diags: &mut Vec<Diagnostic>) {
    const STATIC_OPS: &[&str] = &["==", "!=", "=~", "<", ">"];
    if STATIC_OPS.contains(&op) && is_static_word(left) && is_static_word(right) {
        diags.push(Diagnostic::error_span(
            left.span,
            "SB5086",
            format!(
                "this comparison is decided at compile time — both operands of \
                 `{op}` are fixed by the source text; compare a variable instead"
            ),
        ));
    }
}

/// SB5086 (DD#118): the DD#66 dispatch form is a test too, so the same rule
/// applies — a dispatch whose subject and every pattern are fixed by the
/// source text picks its arm at compile time.
fn check_vacuous_dispatch(c: &Case, diags: &mut Vec<Diagnostic>) {
    let patterns_static = c
        .items
        .iter()
        .all(|it| it.patterns.iter().all(is_static_word));
    if !is_static_word(&c.value) || !patterns_static {
        return;
    }
    diags.push(Diagnostic::error_span(
        c.value.span,
        "SB5086",
        "this dispatch is decided at compile time — its subject and every pattern \
         are fixed by the source text; dispatch on a variable instead"
            .to_string(),
    ));
}

// ── SB5007: arithmetic context validation ───────────────────────
//
// Rules:
//   - In condition (if/while): mutations are rejected
//   - As bare statement: comparisons are rejected (no-op)
//   - Prefix ++/-- always rejected (use postfix or explicit)
//   - DD#31: comma expressions allowed if non-final=mutation, final=comparison

/// SB5007: validate arithmetic expression in context.
fn validate_arith_context(
    expr: &ArithExpr,
    span: Span,
    in_condition: bool,
    diags: &mut Vec<Diagnostic>,
) {
    use crate::parse::classify_arith_expr;

    // No prefix `++`/`--` arm here: one cannot arrive. `reject_prefix_incdec`
    // rejects it (SB4020) in both parse entry points — `parse_arith_slot` for
    // `(( … ))` and every C-style `for` slot, `parse_arith_expansion` for
    // `$(( … ))`. Past that there is no second line of defence by design:
    // every downstream `PreIncDec` arm is an `unreachable!`
    // (`ir::PREFIX_INCDEC_INVARIANT`), because the old arrangement — riding
    // along in `PostIncDec`'s arm — meant a regression in the rejection would
    // silently lower `++x` into `x = x + 1` rather than fail. `x++` is
    // allowed and `++x` is not, so they do not share a fate.
    // Specs cov.23 / cov.163-165 / 31.6 pin the parser's rejection.
    let kind = classify_arith_expr(expr);
    check_arith_const_faults(expr, span, diags);
    if in_condition {
        validate_arith_in_condition(expr, span, diags);
    } else {
        validate_arith_as_statement(expr, kind, span, diags);
    }
}

/// In condition context (`if (( … ))`, `while (( … ))`, etc.):
/// DD#31 permits comma expressions whose non-final parts are
/// mutations and final part is a comparison. Outside that shape,
/// any mutation — top-level or embedded — is rejected (SB5007).
fn validate_arith_in_condition(expr: &ArithExpr, span: Span, diags: &mut Vec<Diagnostic>) {
    if let ArithExpr::Comma(exprs) = expr {
        validate_arith_comma_in_condition(exprs, span, diags);
        return;
    }
    // A condition is a pure test: any mutation — a top-level one (`if (( x=5 ))`,
    // `if (( x++ ))`) or one embedded under a paren / operator
    // (`if (( (a=2) ))`, `while (( (i=i+1) <= 3 ))`) — is rejected. The statement
    // form `(( … ))` is where an assignment belongs (there it is hoisted; bug #3).
    //
    // The message names the mutation's *target*, not the expression around it
    // (DG, 2026-09-23). Quoting the expression said `mutation ...` for
    // `while (( (i=i+1) <= 3 ))` — eliding the very thing being rejected — and
    // made the parenthesised and decomposed forms of one program report
    // differently. `mutation_target_in` answers "is there one" and "which" at
    // once, so the two cannot disagree.
    if let Some(target) = crate::ir::mutation_target_in(expr) {
        diags.push(Diagnostic::error_span(
            span,
            "SB5007",
            format!(
                "mutation of {} not allowed in condition (( )) expression context — \
                 use (( )) statement instead",
                target
            ),
        ));
    }
}

/// DD#31 comma expression in condition context: every non-final part must be
/// a mutation.
///
/// Only a **mutation-free** comma ever arrives here. One that holds a mutation
/// anywhere classifies as `ArithKind::Mutation` (`classify_arith_expr`) and
/// `lower_arith_command` splits it into a `Group` of statements before
/// validation runs — so DD#31's other half, "the final part must be a
/// comparison", is enforced on the decomposed form by `validate_assignment_stmt`
/// (SB5007, mutation-in-condition), identically for `if`, `while`, `until`, and
/// a C-style `for` header. A second check for it here used to be reachable only
/// from the `for` header, which was the one context that skipped the
/// decomposition; that was a bug, and closing it made this the only site that
/// still reports the *non-final* half.
///
/// It is live, and the mutation test below is a tautology given that
/// precondition: a comma list containing a mutation **anywhere** was already
/// decomposed, so everything arriving here is mutation-free and every non-final
/// part is an error. The test is kept because it states the rule DD#31 actually
/// specifies rather than the shape the current pass ordering happens to deliver.
///
/// Empty comma lists are no-ops.
fn validate_arith_comma_in_condition(exprs: &[ArithExpr], span: Span, diags: &mut Vec<Diagnostic>) {
    use crate::parse::{classify_arith_expr, ArithKind};

    let Some((_last, init)) = exprs.split_last() else {
        return;
    };
    for e in init {
        if classify_arith_expr(e) != ArithKind::Mutation {
            let desc = crate::ir::arith_brief(e);
            diags.push(Diagnostic::error_span(
                span,
                "SB5007",
                format!("non-final comma expression {} must be a mutation", desc),
            ));
        }
    }
}

/// As a bare statement (`(( … ))` standalone): a comparison is a
/// no-op (the program does nothing with the boolean); flag it (SB5007).
/// (An assignment statement `(( x = … ))` never reaches here — it routes
/// through the hoisting path / `check_arith_test_placement` SB4020.)
fn validate_arith_as_statement(
    expr: &ArithExpr,
    kind: crate::parse::ArithKind,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    use crate::parse::ArithKind;

    if kind == ArithKind::Comparison {
        let desc = crate::ir::arith_brief(expr);
        diags.push(Diagnostic::error_span(
            span,
            "SB5007",
            format!(
                "comparison {} as bare statement has no effect — \
                 use in if/while condition",
                desc
            ),
        ));
    }
}

/// DD#93 Part D: the int name an `attest SOURCE (( TARGET = SOURCE ))` binds,
/// if the command is that form. The attest introduces TARGET, so scope
/// collection must see it.
pub fn attest_convert_target(c: &Command) -> Option<&str> {
    if expr_literal_str(&c.name) != "attest" {
        return None;
    }
    match classify_attest(&c.args)? {
        (_, AttestPredicate::ArithConvert { target, .. }) => Some(target),
        _ => None,
    }
}

/// Why an `attest … (( … ))` fails to decode as the DD#93 Part D form.
enum ArithNearMiss<'a> {
    /// Scalar lead: the shape is right but the RHS never reads the attested
    /// name, so the conversion proves nothing about it.
    Scalar(&'a str),
    /// A attest-all lead (`attest e in a (( … ))`, `attest k v in m (( … ))`).
    /// DD#93 Part D is scalar-only — there is no per-element conversion form.
    AttestAll,
}

impl ArithNearMiss<'_> {
    fn message(&self) -> String {
        match self {
            Self::Scalar(attested) => format!(
                "attest's `(( … ))` must be `(( TARGET = EXPR ))` where EXPR reads \
                 ${attested} — otherwise the attest proves nothing about it"
            ),
            Self::AttestAll => "attest-all takes a `[[ … ]]` predicate — the \
                 `(( TARGET = EXPR ))` conversion form is scalar-only"
                .to_string(),
        }
    }
}

/// DD#113: an `attest … (( … ))` whose arithmetic does not decode as the DD#93
/// Part D form. Every lead arity reaches a diagnostic — a attest-all lead used
/// to fall through to the generic command path and exec a bare `attest`.
fn attest_arith_near_miss(c: &Command) -> Option<ArithNearMiss<'_>> {
    let last = c.args.last()?;
    if !matches!(&last.kind, ExprKind::Arithmetic(_)) {
        return None;
    }
    if classify_attest(&c.args).is_some() {
        return None;
    }
    match &c.args[..c.args.len() - 1] {
        // `attest x (( … ))` — the form, with an RHS reading the wrong name.
        [only] => match &only.kind {
            ExprKind::Literal(s) => Some(ArithNearMiss::Scalar(s)),
            _ => None,
        },
        // The parser always consumes a name after `attest`, so this is only
        // reachable from a hand-built IR.
        [] => None,
        // Two or more names: a attest-all carrying the scalar-only form.
        _ => Some(ArithNearMiss::AttestAll),
    }
}

/// DD#93 Part D: the `(( TARGET = SOURCE ))` an attest carries as its
/// predicate, if it has one. Rules written for `$(( … ))` **rvalues** must
/// skip it — it is a deliberate assignment whose store the transform emits,
/// not a side-effect-free value expansion.
pub fn attest_arith_predicate(stmt: &Statement) -> Option<&Expr> {
    let StmtKind::Command(c) = &stmt.kind else {
        return None;
    };
    if expr_literal_str(&c.name) != "attest" {
        return None;
    }
    // Only a predicate that actually DECODES earns the exemption. Exempting a
    // near-miss as well is what let it slip past SB4020 into the value path and
    // panic there; it is SB5054 now.
    if !matches!(
        classify_attest(&c.args),
        Some((_, AttestPredicate::ArithConvert { .. }))
    ) {
        return None;
    }
    let last = c.args.last()?;
    matches!(&last.kind, ExprKind::Arithmetic(_)).then_some(last)
}

/// Every attest arith predicate anywhere in `stmt`, including nested bodies
/// and conditions — `if attest n (( c = n ))` puts one inside an `If`, and the
/// expression walkers reach it there, so the exemption has to as well.
pub fn attest_arith_predicates(stmt: &Statement) -> Vec<*const Expr> {
    let mut out = Vec::new();
    collect_attest_arith_predicates(stmt, &mut out);
    out
}

fn collect_attest_arith_predicates(stmt: &Statement, out: &mut Vec<*const Expr>) {
    if let Some(p) = attest_arith_predicate(stmt) {
        out.push(p as *const Expr);
    }
    for body in stmt.child_bodies() {
        for s in body {
            collect_attest_arith_predicates(s, out);
        }
    }
}

/// Per-statement expression rules: `$( )` intrinsic arity (SB4045) and the
/// `$(( … ))` value-mutation rule (SB4020).
///
/// DD#93 Part D: an attest's `(( TARGET = SOURCE ))` predicate is an
/// assignment by design — the conversion IS the validation, and the transform
/// emits the store — so it is not a `$(( … ))` rvalue and SB4020 must not see
/// it. Collected recursively, since `if attest n (( c = n ))` nests one inside
/// an `If`.
fn check_stmt_value_rules(
    stmt: &Statement,
    collections: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    let attest_preds = attest_arith_predicates(stmt);
    walk_all_exprs(stmt, &mut |expr| {
        check_cmdsub_arity(expr, diags);
        check_arith_sexpr_arity(expr, diags);
        check_cmdsub_splat_operand(expr, collections, diags);
        if !attest_preds.contains(&(expr as *const Expr)) {
            check_arith_value_mutation(expr, diags);
        }
    });
}

/// SB4020 (bug #3): an arithmetic mutation inside a `$(( … ))` **value** can't
/// take effect — the value path is side-effect-free — so reject any embedded
/// assignment / `++` / `--` and direct the user to a standalone `(( … ))`
/// statement. (A `(( … ))` *statement* or condition is a `StmtKind::ArithTest`,
/// not an `ExprKind::Arithmetic`, so it is handled by
/// `check_arith_test_placement`, which lets the transform hoist the store.)
fn check_arith_value_mutation(expr: &Expr, diags: &mut Vec<Diagnostic>) {
    if let ExprKind::Arithmetic(a) = &expr.kind {
        if arith_has_mutation(a) {
            diags.push(Diagnostic::error_span(
                expr.span,
                "SB4020",
                "an assignment or `++`/`--` inside `$(( … ))` can't take effect — \
                 perform it in a separate `(( … ))` statement"
                    .to_string(),
            ));
        }
    }
}

/// SB4020 (bug #3): in a `(( … ))` statement or `if`/`while` condition, the
/// transform hoists an embedded assignment in an unconditionally-evaluated
/// position (`(( (a=2) ))`, `(( 1 + (a=2) ))`) so the store takes effect
/// (matching bash). What it can't apply in place is rejected here: an embedded
/// `++`/`--` (post/pre-value ordering), an `a[i]` mutation, or a mutation under
/// a short-circuit / ternary operator (`&&` / `||` / `?:`), whose operand may
/// go unevaluated.
fn check_arith_test_placement(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    if let StmtKind::ArithTest(a) = &stmt.kind {
        if let Some(msg) = arith_unhoistable_reason(a) {
            diags.push(Diagnostic::error_span(stmt.span, "SB4020", msg.to_string()));
        }
    }
    for body in stmt.child_bodies() {
        for s in body {
            check_arith_test_placement(s, diags);
        }
    }
}

/// The reason an embedded mutation in a `(( … ))` statement/condition can't be
/// hoisted, or `None` when every mutation is a plain / compound assignment in an
/// unconditionally-evaluated position (which the transform emits in place).
fn arith_unhoistable_reason(a: &ArithExpr) -> Option<&'static str> {
    if arith_has_incdec(a) {
        return Some(
            "an embedded `++`/`--` in `(( … ))` can't take effect — use a \
             separate `(( … ))` statement",
        );
    }
    if arith_has_subscript_assign(a) {
        return Some(
            "an embedded `a[i]` assignment in `(( … ))` can't take effect — use \
             a separate `(( … ))` statement",
        );
    }
    if arith_mutation_under_cond(a) {
        return Some(
            "an assignment under `&&` / `||` / `?:` in `(( … ))` can't take \
             effect reliably — use a separate `(( … ))` statement",
        );
    }
    None
}

fn arith_has_incdec(a: &ArithExpr) -> bool {
    if matches!(a, ArithExpr::PreIncDec { .. }) {
        unreachable!("{}", crate::ir::PREFIX_INCDEC_INVARIANT);
    }
    matches!(a, ArithExpr::PostIncDec { .. }) || a.children().into_iter().any(arith_has_incdec)
}

fn arith_has_subscript_assign(a: &ArithExpr) -> bool {
    matches!(a, ArithExpr::SubscriptAssign { .. })
        || a.children().into_iter().any(arith_has_subscript_assign)
}

/// A mutation located under a short-circuit (`&&` / `||`) or a ternary branch,
/// whose evaluation is conditional — the transform can't hoist it (hoisting
/// would run the store unconditionally). A ternary *condition* is
/// unconditional, so recurse into it for a deeper short-circuit.
fn arith_mutation_under_cond(a: &ArithExpr) -> bool {
    match a {
        ArithExpr::BinOp {
            op: IrArithOp::And | IrArithOp::Or,
            ..
        } => arith_has_mutation(a),
        ArithExpr::Ternary {
            cond,
            true_val,
            false_val,
        } => {
            arith_has_mutation(true_val)
                || arith_has_mutation(false_val)
                || arith_mutation_under_cond(cond)
        }
        _ => a.children().into_iter().any(arith_mutation_under_cond),
    }
}

/// SB5006: check for ? and [...] glob in unquoted expressions
fn check_unquoted_glob(expr: &Expr, diags: &mut Vec<Diagnostic>) {
    match &expr.kind {
        ExprKind::Literal(s) => check_glob_chars_in_literal(s, expr.span, diags),
        ExprKind::Concat(parts) => {
            for p in parts {
                if let ExprKind::Literal(s) = &p.kind {
                    check_glob_chars_in_literal(s, p.span, diags);
                }
            }
        }
        _ => {}
    }
}

/// SB5006: flag `?` / `[…]` in one literal string (sash globs are `*`-only).
fn check_glob_chars_in_literal(s: &str, span: Span, diags: &mut Vec<Diagnostic>) {
    if s.contains('?') {
        diags.push(Diagnostic::error_span(
            span,
            "SB5006",
            format!("sash has only `*` globs — `?` is omitted: {}", s),
        ));
    } else if s.contains('[') && s.contains(']') {
        diags.push(Diagnostic::error_span(
            span,
            "SB5006",
            format!("sash has only `*` globs — `[…]` is omitted: {}", s),
        ));
    }
}

/// Walk all expressions in a statement, passing each to the callback.
fn walk_exprs_in_stmt(stmt: &Statement, f: &mut impl FnMut(&Expr, Span)) {
    crate::ir::walk_all_exprs(stmt, &mut |expr| f(expr, expr.span));
}

/// Walk only the expressions this statement owns directly — see
/// [`crate::ir::walk_own_exprs`]. For passes that do their own statement
/// recursion.
fn walk_own_exprs_in_stmt(stmt: &Statement, f: &mut impl FnMut(&Expr, Span)) {
    crate::ir::walk_own_exprs(stmt, &mut |expr| f(expr, expr.span));
}

// ── SB5012: $? availability state machine ─────────────────────
//
// States: Undefined → Available (after non-suppressed command) → Undefined (consumed)
// In || branches: inherited status available only for first statement.
// SECURITY: prevents use-after-consume and uninitialized $? reads.

/// $? state machine: undefined → available (after command) → undefined (consumed)
#[derive(Clone, Copy, PartialEq)]
enum StatusState {
    Undefined,
    Available,
}

/// Check $? usage across a statement list.
fn check_dollar_question(stmts: &[Statement], seed: StatusState, diags: &mut Vec<Diagnostic>) {
    let mut state = seed;
    for stmt in stmts {
        check_dq_stmt(stmt, &mut state, diags);
    }
}

fn check_dq_stmt(stmt: &Statement, state: &mut StatusState, diags: &mut Vec<Diagnostic>) {
    match &stmt.kind {
        StmtKind::List(l) => check_dq_list(l, state, diags),
        StmtKind::Group(g) => {
            for s in &g.body {
                check_dq_stmt(s, state, diags);
            }
        }
        StmtKind::Function(f) => check_dq_function(f, diags),
        StmtKind::If(i) => check_dq_if(i, state, diags),
        StmtKind::While(w) => check_dq_while(w, state, diags),
        StmtKind::For(f) => check_dq_for(f, state, diags),
        StmtKind::Case(c) => check_dq_case(c, state, diags),
        _ => check_dq_leaf(stmt, state, diags),
    }
}

/// `a && b` / `a || b`: left runs normally, right is a branch so
/// inherited $? is available only for the first statement.
fn check_dq_list(l: &crate::ir::List, state: &mut StatusState, diags: &mut Vec<Diagnostic>) {
    check_dq_stmt(&l.left, state, diags);
    let mut branch_state = StatusState::Available;
    check_dq_stmt_branch(&l.right, &mut branch_state, diags);
    *state = StatusState::Undefined;
}

/// A function body is checked in a fresh status context — the caller's `$?`
/// state does not flow in.
///
/// The DD#56 `END { }` body is *not* a function; it is `Program::end_block`,
/// seeded with [`StatusState::Available`] by [`validate`], because it is
/// ENTERED because of a triggering status (an abort's exit code, `exit N`, a
/// signal `128+n`, or the last command's status on normal exit). That status is
/// available as the block's FIRST action, so `END { echo "exited $?" }` is valid.
fn check_dq_function(f: &crate::ir::Function, diags: &mut Vec<Diagnostic>) {
    let mut fn_state = StatusState::Undefined;
    for s in &f.body {
        check_dq_stmt(s, &mut fn_state, diags);
    }
}

/// `if … then … (elif …)* (else …)? fi`: walk the condition with
/// the inherited state, then each branch body with the
/// first-statement-only rule. After the construct, state is
/// undefined regardless of which branch ran.
fn check_dq_if(i: &crate::ir::If, state: &mut StatusState, diags: &mut Vec<Diagnostic>) {
    for s in &i.condition {
        check_dq_stmt(s, state, diags);
    }
    check_dq_body_branch(&i.then_body, *state, diags);
    for elif in &i.elif_clauses {
        let mut es = *state;
        for s in &elif.condition {
            check_dq_stmt(s, &mut es, diags);
        }
        check_dq_body_branch(&elif.body, es, diags);
    }
    if let Some(ref eb) = i.else_body {
        check_dq_body_branch(eb, *state, diags);
    }
    *state = StatusState::Undefined;
}

/// `while … do … done`: condition uses inherited state, body
/// re-reads it fresh for each iteration. After the loop, state
/// is undefined.
fn check_dq_while(w: &crate::ir::While, state: &mut StatusState, diags: &mut Vec<Diagnostic>) {
    for s in &w.condition {
        check_dq_stmt(s, state, diags);
    }
    let mut bs = *state;
    for s in &w.body {
        check_dq_stmt(s, &mut bs, diags);
    }
    *state = StatusState::Undefined;
}

/// `for x in … do … done`: word list does not consume $? (literal/
/// expansion only). Body re-reads inherited state fresh for each
/// iteration. After the loop, state is undefined — for is a
/// catching construct (caught/uncaught model).
fn check_dq_for(f: &crate::ir::For, state: &mut StatusState, diags: &mut Vec<Diagnostic>) {
    let mut bs = *state;
    for s in &f.body {
        check_dq_stmt(s, &mut bs, diags);
    }
    *state = StatusState::Undefined;
}

/// `case word in pat) body ;; … esac`: scrutinee does not consume
/// $? (it's a word). Each arm body re-reads inherited state fresh.
/// After the case, state is undefined — case is a catching construct.
fn check_dq_case(c: &crate::ir::Case, state: &mut StatusState, diags: &mut Vec<Diagnostic>) {
    for item in &c.items {
        let mut bs = *state;
        for s in &item.body {
            check_dq_stmt(s, &mut bs, diags);
        }
    }
    *state = StatusState::Undefined;
}

/// Default arm: a leaf-ish statement (Command, Pipeline, etc.).
/// Two responsibilities: (1) if it consumes `$?`, raise SB5012
/// when the status is unavailable; (2) update state to reflect
/// what this statement does (sets, consumes, or neither).
fn check_dq_leaf(stmt: &Statement, state: &mut StatusState, diags: &mut Vec<Diagnostic>) {
    if stmt_uses_status(stmt) {
        if *state != StatusState::Available {
            emit_status_unavailable(stmt, diags);
        }
        *state = StatusState::Undefined;
    }
    if is_suppressed(stmt) {
        *state = StatusState::Undefined;
    } else if is_command_like(stmt)
        || is_assignment_from_arith(stmt)
        || is_capture_condition_stmt(stmt)
    {
        *state = StatusState::Available;
    }
}

/// A capture-condition assignment (`x=$(external cmd)`) runs a command and sets
/// `$?`, so — like a plain command — it makes the status available as the first
/// expression of a handling branch (`if x=$(cmd) { … } else { echo $? }`).
fn is_capture_condition_stmt(stmt: &Statement) -> bool {
    matches!(&stmt.kind, StmtKind::Assignment(a) if is_capture_condition_rhs(&a.value))
}

/// Emit an SB5012 at every `$?` / `$(status)` in `stmt` — the status is
/// consumed with no preceding command to have set it.
fn emit_status_unavailable(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    let mut spans = Vec::new();
    find_status_reads(stmt, &mut spans);
    for span in spans {
        diags.push(Diagnostic::error_span(
            span,
            "SB5012",
            "$(status) is not available here — no preceding command \
             (the punctuation sugar $? reads the same value)"
                .to_string(),
        ));
    }
}

/// Collect the span of every status read in `stmt` — `$(status)`, and the `$?`
/// punctuation that DD#111 desugars into it at parse.
///
/// **This is the AST-level half of SB5012 and it is load-bearing.** Until the
/// DD#111 desugar this matched `SimpleExpansion("?")`, which covered
/// `$(status)` only because the parser folded the intrinsic into that node.
/// Reversing that fold without moving this would leave the walk finding
/// nothing, and the availability state machine below would go **silently
/// vacuous** — every spec still passing, and SB5012 simply never raised again.
/// `crate::ir::intrinsic_call_name` is the recogniser; its round-trip test
/// pins it to the constructor.
fn find_status_reads(stmt: &Statement, spans: &mut Vec<Span>) {
    walk_exprs_in_stmt(stmt, &mut |expr, _| {
        if crate::ir::intrinsic_call_name(expr) == Some("status") {
            spans.push(expr.span);
        }
    });
}

fn stmt_uses_status(stmt: &Statement) -> bool {
    let mut spans = Vec::new();
    find_status_reads(stmt, &mut spans);
    !spans.is_empty()
}

fn is_suppressed(stmt: &Statement) -> bool {
    matches!(&stmt.kind, StmtKind::Suppressed(_))
}

fn is_command_like(stmt: &Statement) -> bool {
    matches!(
        &stmt.kind,
        StmtKind::Command(_) | StmtKind::Pipeline(_) | StmtKind::List(_) | StmtKind::ArithTest(_)
    )
}

fn is_assignment_from_arith(stmt: &Statement) -> bool {
    if let StmtKind::Assignment(a) = &stmt.kind {
        matches!(&a.value.kind, ExprKind::Arithmetic(_))
    } else {
        false
    }
}

/// In || branches, status is available only for the first statement.
/// In handling branches (||, if/else), inherited status available for first statement only.
/// After that, the inherited status is gone.
fn check_dq_stmt_branch(stmt: &Statement, state: &mut StatusState, diags: &mut Vec<Diagnostic>) {
    match &stmt.kind {
        StmtKind::Group(g) => {
            check_dq_body_branch(&g.body, *state, diags);
            *state = StatusState::Undefined;
        }
        _ => {
            // Single statement branch: normal check
            check_dq_stmt(stmt, state, diags);
        }
    }
}

/// Apply first-statement-only rule to a body (Vec<Statement>).
fn check_dq_body_branch(stmts: &[Statement], state: StatusState, diags: &mut Vec<Diagnostic>) {
    for (i, s) in stmts.iter().enumerate() {
        if i == 0 {
            let mut first_state = state;
            check_dq_stmt(s, &mut first_state, diags);
        } else {
            // After first statement, inherited status is consumed
            let mut inner = StatusState::Undefined;
            check_dq_stmt(s, &mut inner, diags);
        }
    }
}

// ── SB5011: Command resolution ─────────────────────────────────

const BUILTINS: &[&str] = &[
    // Output
    "echo",
    "write",
    // Control flow
    ":",
    "exit",
    "return",
    "break",
    "continue",
    // Input
    "read",
    "readline",
    "mapfile",
    "readarray",
    // Taint
    "attest",
    "getenv",
    // Positional/args
    "argv",
    "argc",
    "arg",
    // Variables
    "unset",
    // Test
    "[[",
    // Process/signal
    "signal",
    "exec",
    "wait",
    "kill",
    "signum",
    // Filesystem
    "cd",
    "close",
    // Introspection
    "status",
    "getpid",
    "getppid",
    "getcpid",
    "getcwd",
    "epochseconds",
    // Regex (DD#63): match/matchall are boolean commands (sub/gsub are
    // value-only). `matchall` fills its out-array with every match.
    "match",
    "matchall",
    // Value-only intrinsics. They are listed here for exactly one reason: a
    // name in BUILTINS is never PATH-resolved. Leaving them out made the safe
    // set host-dependent — `join`, `basename` and `dirname` have coreutils
    // twins and silently exec'd them in statement position, while the rest
    // escaped only because no binary of that name happened to be installed.
    // VALUE_ONLY_INTRINSICS below then makes statement position SB5073.
    "len",
    "strlen",
    "keys",
    "join",
    "sub",
    "gsub",
    "toupper",
    "tolower",
    "basename",
    "dirname",
    "random",
];

/// Intrinsics that are **value-producing only**: they are legal as a `$( )`
/// head and nowhere else. They stay in `BUILTINS` above so they are never
/// PATH-resolved (a machine with a real `getenv` on PATH must not silently
/// exec it) and so `is_external_source` keeps classifying them as non-external
/// for taint. This list is what makes a *statement-position* use an error
/// (SB5073) instead of falling through to `sash_exec` and aborting at runtime
/// with `sash: bug: bare command passed to sash_exec`.
///
/// `argv` is deliberately absent — DD#60's `argv [N] a` out-param form is a
/// legal statement. So are `match`/`matchall`, which are boolean commands with
/// out-array forms.
///
/// The remaining value intrinsics (`len`, `keys`, `join`, `toupper`, `random`,
/// …) used to be absent from `BUILTINS` on the theory that they "already get
/// SB5011 in statement position". That was false: SB5011 only fires when no
/// PATH binary matches, so `join`, `basename` and `dirname` silently exec'd
/// their coreutils twins, and the others were safe only by accident of what
/// was installed. They are now in both lists.
const VALUE_ONLY_INTRINSICS: &[&str] = &[
    "getenv",
    "status",
    "getpid",
    "getppid",
    "getcpid",
    "getcwd",
    "epochseconds",
    "argc",
    "arg",
    "signum",
    "len",
    "strlen",
    "keys",
    "join",
    "sub",
    "gsub",
    "toupper",
    "tolower",
    "basename",
    "dirname",
    "random",
];

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

// ── DD#74: predicate functions must definite-return ────────────────
//
// A function used in a *boolean context* (an if/elif/while/until
// condition, a `! f` negation, or a `&&`/`||` operand) is a predicate:
// its truth value is its exit status, which — per DD#74 — must come
// from an explicit `return` on every path. Falling off the end (an
// implicit trailing-command status) is a compile error; the trailing
// command would abort-on-fail rather than yield a status.

/// Collect every function definition as (name, def-span, body), walking
/// nested/local functions too.
fn collect_function_defs<'a>(
    stmts: &'a [Statement],
    out: &mut Vec<(&'a str, Span, &'a [Statement])>,
) {
    for stmt in stmts {
        if let StmtKind::Function(f) = &stmt.kind {
            out.push((f.name.as_str(), stmt.span, f.body.as_slice()));
        }
        for body in stmt.child_bodies() {
            collect_function_defs(body, out);
        }
    }
}

/// Collect the names of functions used in a boolean context.
fn collect_predicate_uses(stmts: &[Statement], funcs: &HashSet<String>, out: &mut HashSet<String>) {
    for stmt in stmts {
        collect_predicate_uses_stmt(stmt, funcs, out);
    }
}

fn collect_predicate_uses_stmt(
    stmt: &Statement,
    funcs: &HashSet<String>,
    out: &mut HashSet<String>,
) {
    match &stmt.kind {
        StmtKind::If(i) => {
            mark_condition_predicate(&i.condition, funcs, out);
            for elif in &i.elif_clauses {
                mark_condition_predicate(&elif.condition, funcs, out);
            }
        }
        StmtKind::While(w) => mark_condition_predicate(&w.condition, funcs, out),
        StmtKind::List(l) if matches!(l.op, ListOp::And | ListOp::Or) => {
            // Only the LEFT operand is a predicate: its status decides whether
            // the right side runs. The final clause's status is simply the
            // statement's own status — nothing branches on it — so it is no
            // more a predicate than a bare `f` statement is (DG 2026-09-07).
            //
            // `&&`/`||` are left-associative, so in `a && b && c` the left
            // operand is the nested list `(a && b)`, and `mark_predicate_operand`
            // marks both of ITS operands — correctly, since `b`'s status decides
            // whether `c` runs. Inside an `if`/`while` condition the whole list
            // is a predicate (via `mark_condition_predicate`), last clause
            // included.
            mark_predicate_operand(&l.left, funcs, out);
        }
        StmtKind::Pipeline(p) if p.negated => {
            for c in &p.commands {
                mark_predicate_operand(c, funcs, out);
            }
        }
        _ => {}
    }
    for body in stmt.child_bodies() {
        collect_predicate_uses(body, funcs, out);
    }
}

/// The last statement of a condition supplies its truth value.
fn mark_condition_predicate(
    cond: &[Statement],
    funcs: &HashSet<String>,
    out: &mut HashSet<String>,
) {
    if let Some(last) = cond.last() {
        mark_predicate_operand(last, funcs, out);
    }
}

/// Record `stmt` as a predicate use if it is (or resolves through
/// negation / `&&` / `||` to) a bare call of a known function.
fn mark_predicate_operand(stmt: &Statement, funcs: &HashSet<String>, out: &mut HashSet<String>) {
    match &stmt.kind {
        StmtKind::Command(c) => {
            let name = expr_literal_str(&c.name);
            if !name.is_empty() && funcs.contains(name) {
                out.insert(name.to_string());
            }
        }
        StmtKind::Pipeline(p) if p.negated && p.commands.len() == 1 => {
            mark_predicate_operand(&p.commands[0], funcs, out);
        }
        StmtKind::List(l) if matches!(l.op, ListOp::And | ListOp::Or) => {
            mark_predicate_operand(&l.left, funcs, out);
            mark_predicate_operand(&l.right, funcs, out);
        }
        _ => {}
    }
}

/// DD#74 definite-return: does every path through this body end in an
/// explicit `return`?
fn body_always_returns(stmts: &[Statement]) -> bool {
    stmts.last().is_some_and(stmt_always_returns)
}

fn stmt_always_returns(stmt: &Statement) -> bool {
    match &stmt.kind {
        StmtKind::ReturnProducer(_) => true,
        StmtKind::Command(c) => expr_literal_str(&c.name) == "return",
        StmtKind::If(i) => if_always_returns(i),
        StmtKind::Group(g) => body_always_returns(&g.body),
        StmtKind::Redirected(r) => stmt_always_returns(&r.body),
        StmtKind::Case(c) => case_always_returns(c),
        _ => false,
    }
}

/// An `if` definite-returns iff it has an `else` and every branch
/// (then, each elif, else) definite-returns.
fn if_always_returns(i: &If) -> bool {
    i.else_body
        .as_ref()
        .is_some_and(|eb| body_always_returns(eb))
        && body_always_returns(&i.then_body)
        && i.elif_clauses.iter().all(|e| body_always_returns(&e.body))
}

/// A dispatch (`case`) definite-returns iff it has a default arm and
/// every arm body definite-returns.
fn case_always_returns(c: &Case) -> bool {
    c.items.iter().any(|it| it.is_default) && c.items.iter().all(|it| body_always_returns(&it.body))
}

fn check_commands(stmts: &[Statement], funcs: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    for stmt in stmts {
        check_cmd_in_stmt(stmt, funcs, diags);
    }
}

/// A `$( … )` body, checked like any other block.
///
/// DD#86's 2026-09-28 addendum makes the body an anonymous function body, so
/// every rule that governs a statement governs a statement here. It used to
/// get `check_cmdsub_heads`, which carried over the head-must-be-literal rule
/// and **nothing else**: everything keyed on *which* command it is stayed
/// behind. `$(cd /tmp /var)` and `$(unset zz)` and `$(break)` were accepted in
/// silence, `$(mapfile -m q)` slipped past its DD#99 retirement, and
/// `$(close abc)` / `$(signal INT ignore)` reached a transform that had been
/// promised a validated argument and panicked on it.
///
/// One exemption, carried by [`CmdPos`]: a value intrinsic is legal when it
/// *is* the whole substitution, because that is the shape `transform`
/// resolves into the value it produces.
fn check_cmdsub_body(stmts: &[Statement], funcs: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    let pos = if stmts.len() == 1 {
        CmdPos::CmdsubSole
    } else {
        CmdPos::CmdsubOther
    };
    for stmt in stmts {
        check_cmd_at(stmt, funcs, pos, diags);
    }
}

/// Where a command sits, for the one rule that depends on it: a value
/// intrinsic (`getenv`, `status`, `argc`, …) produces a value and is legal
/// only where something takes it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CmdPos {
    /// An ordinary statement — top level, a function body, a loop body. A
    /// value intrinsic here would discard its value.
    Statement,
    /// The sole statement of a `$( … )` body. A value intrinsic is exactly
    /// right here; this is the shape that resolves into a value.
    CmdsubSole,
    /// One statement among several in a `$( … )` body — not the shape that
    /// resolves, so a value intrinsic would survive as a bare command no
    /// backend recognises and abort at runtime on `sash_exec`'s absolute-path
    /// assertion, printing `sash: bug:` into the program's own output with a
    /// successful exit.
    CmdsubOther,
}

/// SB5073 for a value intrinsic outside the one position that takes its value.
///
/// The two wordings are not interchangeable. The statement-position sentence
/// ("only valid inside a command substitution") would be actively misleading
/// for a body statement, because that one *is* inside a command substitution.
/// Same rule, same code, different explanation.
fn value_intrinsic_diag(name: &str, c: &Command, pos: CmdPos) -> Option<Diagnostic> {
    let ellipsis = if c.args.is_empty() { "" } else { " …" };
    let complaint = match pos {
        CmdPos::CmdsubSole => return None,
        CmdPos::Statement => format!(
            "`{name}` is a value intrinsic — it produces a value and is only \
             valid inside a command substitution, e.g. `x=$({name}{ellipsis})`. \
             A bare statement would discard the value"
        ),
        CmdPos::CmdsubOther => format!(
            "`{name}` is a value intrinsic — inside `$( … )` it is recognised \
             only when it is the whole substitution, not one statement among \
             several. Capture it first (`x=$({name}{ellipsis})`) and use `$x` here"
        ),
    };
    Some(Diagnostic::error_span(c.name.span, "SB5073", complaint))
}

fn check_cmd_in_stmt(stmt: &Statement, funcs: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    check_cmd_at(stmt, funcs, CmdPos::Statement, diags);
}

/// One statement's worth of command checking: the bodies of any command
/// substitutions in its expressions, its own head, and its child bodies.
///
/// DD#23 applies to a command substitution's head too. `$c hi` in statement
/// position was rejected while `$($c hi)` was accepted, then aborted at
/// runtime in native ("bare command passed to sash_exec") and silently
/// captured the empty string in c.
///
/// `pos` is the *statement's own* position. A nested body — an `if` arm, a
/// loop body — is never itself a substitution however short it is, so it
/// passes `CmdPos::Statement` down.
fn check_cmd_at(
    stmt: &Statement,
    funcs: &HashSet<String>,
    pos: CmdPos,
    diags: &mut Vec<Diagnostic>,
) {
    for body in stmt.cmdsub_bodies() {
        check_cmdsub_body(body, funcs, diags);
    }
    let StmtKind::Command(c) = &stmt.kind else {
        for body in stmt.structural_bodies() {
            check_commands(body, funcs, diags);
        }
        return;
    };
    // DD#23: a command name must be resolvable at compile time. A variable or
    // other expansion (`$x foo`) is a runtime-chosen command — reject it here
    // rather than letting it reach `sash_exec` at runtime.
    if !matches!(&c.name.kind, ExprKind::Literal(_)) {
        diags.push(bad_command_head_diag(&c.name));
        return;
    }
    check_command_head(expr_literal_str(&c.name), c, stmt, funcs, pos, diags);
}

/// SB5011 for a command head that is not a literal. Two very different words
/// arrive here, and they want different explanations.
///
/// The arm was written for the DD#23 case — `$x foo`, a runtime-chosen command
/// — but a word that *looks* like an assignment and whose target is not a valid
/// identifier also lands here, because the `=` splits it into a `Concat` and so
/// it is not a `Literal` either. Telling the author of `a-b=1` that "a variable
/// or expansion can't be resolved at compile time" names a cause the line does
/// not have: there is no variable and no expansion in it.
fn bad_command_head_diag(name: &Expr) -> Diagnostic {
    if let Some(target) = malformed_assignment_target(&name.kind) {
        return Diagnostic::error_span(
            name.span,
            "SB5011",
            format!(
                "`{target}` is not a valid variable name, so this is neither an \
                 assignment nor a runnable command — a name is letters, digits \
                 and `_`, and cannot start with a digit"
            ),
        );
    }
    Diagnostic::error_span(
        name.span,
        "SB5011",
        "a command name must be a literal path — a variable or expansion \
         can't be resolved at compile time; use the literal path, or \
         `/usr/bin/env` for a runtime tool"
            .to_string(),
    )
}

/// The intended target of a word that looks like an assignment: literal text,
/// then an unquoted `=`. `a-b=1` parses as a command whose name is
/// `Concat([Literal("a-b"), Literal("="), Literal("1")])`, so the target is the
/// literal run before the `=`.
///
/// An expansion to the *left* of the `=` (`9x[$i]=1`) returns `None` — that word
/// really does contain an expansion, so the dynamic-head wording fits it.
fn malformed_assignment_target(kind: &ExprKind) -> Option<String> {
    let ExprKind::Concat(parts) = kind else {
        return None;
    };
    let mut target = String::new();
    for part in parts {
        match &part.kind {
            ExprKind::Literal(text) if text == "=" => {
                return (!target.is_empty()).then_some(target);
            }
            ExprKind::Literal(text) => target.push_str(text),
            _ => return None,
        }
    }
    None
}

/// Name-keyed half of `check_cmd_in_stmt`: everything that depends on *which*
/// command this is, once the head is known to be a literal.
fn check_command_head(
    name: &str,
    c: &Command,
    stmt: &Statement,
    funcs: &HashSet<String>,
    pos: CmdPos,
    diags: &mut Vec<Diagnostic>,
) {
    match name {
        // DD#14: `exec` replaces the running program (or manipulates its fds).
        // Its target — the first argument, for the replace form — must be
        // resolvable at compile time exactly like a command head; the fd-only
        // form (`exec >file`) has no target. (The `exec` head itself is a
        // BUILTIN, so it passes the resolvability check below.)
        "exec" => return check_exec_target(c, diags),
        // DD#14 fd-manipulation: `close N` closes a file descriptor. Its single
        // argument must be a literal non-negative fd number — the same
        // discipline as a `N>&M` dup target (no dynamic/variable fd).
        "close" => return check_close_arg(c, diags),
        // DD#53: `cd DIR` takes exactly one directory. The environment is a
        // separate namespace, so there is no `$HOME` to fall back on.
        "cd" => return check_cd_arg(c, diags),
        // A value-producing intrinsic (`$(getenv NAME)`, `$(status)`, …) is
        // legal only as a command-substitution head. In statement position its
        // value has nowhere to go, and it used to fall through to `sash_exec`
        // and abort at runtime on an internal assertion.
        n if VALUE_ONLY_INTRINSICS.contains(&n) => {
            diags.extend(value_intrinsic_diag(name, c, pos));
            return;
        }
        "" => return,
        _ => {}
    }
    if is_valid_command(name, funcs) {
        return;
    }
    diags.push(Diagnostic::error_span(
        stmt.span,
        "SB5011",
        format!(
            "command '{}' not found — use absolute path or define as function",
            name
        ),
    ));
}

/// DD#14: `close N` — the single argument must be a literal non-negative
/// file-descriptor number (`close 3`). A variable/expansion fd is a
/// runtime-chosen descriptor (rejected, like a dynamic exec target), and
/// zero or multiple args is a misuse. SB4052.
fn check_close_arg(c: &Command, diags: &mut Vec<Diagnostic>) {
    if c.args.len() != 1 {
        diags.push(Diagnostic::error_span(
            c.name.span,
            "SB4052",
            "`close` takes exactly one argument: a file-descriptor number (e.g. `close 3`)"
                .to_string(),
        ));
        return;
    }
    let arg = &c.args[0];
    let is_fd = matches!(&arg.kind,
        ExprKind::Literal(s) if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()));
    if !is_fd {
        diags.push(Diagnostic::error_span(
            arg.span,
            "SB4052",
            "`close` needs a literal file-descriptor number (e.g. `close 3`) — \
             not a variable or expression"
                .to_string(),
        ));
    }
}

/// DD#53: `cd DIR` — exactly one argument. Sash variables and the environment
/// are separate namespaces, so there is no `$HOME` for a bare `cd` to fall back
/// on. Both backends used to answer the misuse silently: a bare `cd` set
/// `$? = 0` without moving (bash goes to `$HOME`), and a second argument was
/// dropped without a word. The fd sibling `close` has always been strict here.
/// Unlike `close`, the argument need not be a literal — `cd $dir` is a path,
/// not a runtime-chosen descriptor. SB4066.
fn check_cd_arg(c: &Command, diags: &mut Vec<Diagnostic>) {
    if c.args.len() != 1 {
        diags.push(Diagnostic::error_span(
            c.name.span,
            "SB4066",
            "`cd` takes exactly one argument: a directory (e.g. `cd /tmp`)".to_string(),
        ));
    }
}

/// DD#14: validate the target of `exec CMD args…` (the replace form). `CMD`
/// (the first argument) must be a literal, compile-time-resolvable **external**
/// command — the same discipline as a command head (DD#23), since `exec`
/// replaces the process image with it. A variable/expansion target is a
/// runtime-chosen command (SB5011); an unresolvable one is not-found (SB5011).
/// The fd-only form (`exec > file`, no target argument) is allowed.
fn check_exec_target(c: &Command, diags: &mut Vec<Diagnostic>) {
    let Some(target) = c.args.first() else {
        return; // fd-manipulation-only form: `exec > file`
    };
    if !matches!(&target.kind, ExprKind::Literal(_)) {
        diags.push(Diagnostic::error_span(
            target.span,
            "SB5011",
            "an `exec` target must be a literal path — a variable or expansion \
             can't be resolved at compile time; use the literal path, or \
             `/usr/bin/env` for a runtime tool"
                .to_string(),
        ));
        return;
    }
    let tname = expr_literal_str(target);
    if tname.is_empty() {
        return;
    }
    // exec replaces the process with an EXTERNAL program: an absolute path or a
    // PATH-resolvable name (not the inline `echo`/`:` shortcut — the transform
    // resolves the target to its real path).
    if !tname.starts_with('/') && !resolve_on_path(tname) {
        diags.push(Diagnostic::error_span(
            target.span,
            "SB5011",
            format!(
                "`exec` target '{}' not found — use an absolute path or a \
                 command on PATH",
                tname
            ),
        ));
    }
}

/// replacement surface currently implemented (the `s///` sugar desugars
/// to it). A dynamic (interpolated) replacement is not statically
/// checkable and is skipped.
fn check_regex_replacements(stmts: &[Statement], diags: &mut Vec<Diagnostic>) {
    for stmt in stmts {
        walk_exprs_in_stmt(stmt, &mut |expr, span| {
            if let ExprKind::CommandSubstitution(inner) = &expr.kind {
                for s in inner {
                    check_replacement_in_stmt(s, span, diags);
                }
            }
        });
    }
}

fn check_replacement_in_stmt(stmt: &Statement, span: Span, diags: &mut Vec<Diagnostic>) {
    let StmtKind::Command(c) = &stmt.kind else {
        return;
    };
    let name = expr_literal_str(&c.name);
    if (name != "sub" && name != "gsub") || c.args.len() != 3 {
        return;
    }
    let Some(rep) = static_replacement_str(&c.args[2]) else {
        return; // interpolated replacement — not statically checkable
    };
    if let Some(bad) = invalid_replacement_escape(&rep) {
        diags.push(Diagnostic::error_span(
            span,
            "SB4033",
            format!(
                "invalid escape `\\{}` in a {} replacement — only \\0–\\9 (backrefs) and \\\\ (literal backslash) are allowed",
                bad, name
            ),
        ));
    }
}

/// Extract the replacement's static byte string, or `None` if any part is
/// interpolated (dynamic). `'…'`/`"…"`/concatenation of literals collapse
/// to their bytes (DD#64: backslash is verbatim in both quote styles).
fn static_replacement_str(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Literal(s) | ExprKind::SingleQuoted(s) => Some(s.clone()),
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) => {
            let mut out = String::new();
            for p in parts {
                match &p.kind {
                    ExprKind::Literal(s) | ExprKind::SingleQuoted(s) => out.push_str(s),
                    _ => return None,
                }
            }
            Some(out)
        }
        _ => None,
    }
}

/// Returns a description of the first invalid `\`-escape in a replacement,
/// or `None` if every escape is a valid `\0`–`\9` / `\\`.
fn invalid_replacement_escape(rep: &str) -> Option<String> {
    let b = rep.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' {
            match b.get(i + 1) {
                Some(&c) if c.is_ascii_digit() || c == b'\\' => i += 2,
                Some(&c) => return Some((c as char).to_string()),
                None => return Some("<end of replacement>".to_string()),
            }
        } else {
            i += 1;
        }
    }
    None
}

fn is_valid_command(name: &str, funcs: &HashSet<String>) -> bool {
    // Absolute path: always valid
    if name.starts_with('/') {
        return true;
    }
    // Relative path (`./x`, `foo/bar`): not runnable, but let `resolve` own
    // the DD#23 rejection (SB5051) so every position reports the same
    // diagnostic — don't preempt it with a generic SB5011 "not found" here.
    if name.contains('/') {
        return true;
    }
    // Builtin
    if BUILTINS.contains(&name) {
        return true;
    }
    // Declared function
    if funcs.contains(name) {
        return true;
    }
    // Glob pattern (e.g. /usr/bin/*) — not a command
    if name.contains('*') {
        return true;
    }
    // PATH-resolvable: check if command exists on system PATH
    if resolve_on_path(name) {
        return true;
    }
    false
}

fn resolve_on_path(name: &str) -> bool {
    // Don't resolve names with special characters
    if name.contains('/') || name.contains('$') {
        return false;
    }
    // Shared, sysroot-aware resolution (DD#23): honors --sysroot=<dir> and
    // --sysroot=<image.squashfs> so a cross-compile target's commands aren't
    // rejected just because they're absent from the build host's PATH.
    crate::resolve::resolve_command_path(name).is_some()
}

// ── Taint tracking (SB5020) ───────────────────────────────────────
//
// Forward dataflow analysis: track which variables hold tainted data.
// A variable is tainted if its value came from:
//   - $(getenv NAME), $(argv N)  — external input
//   - $(/absolute/path ...)      — external command output
//   - read from stdin or a file  — external input
// A variable is clean if:
//   - assigned from literals, arithmetic, clean variables
//   - read from a clean pipe (<-) — data came from another sash function
//   - function parameters ($1-$9) inside function bodies
//   - attest explicitly removes taint after validation
//
// SECURITY: This is definition-based, NOT name-based. There is no
// "clean list" of variable names. Taint flows through assignment.

use std::collections::HashSet;

/// Run taint analysis: track which variables hold tainted data,
/// reject any use of tainted variables outside attest expressions.
/// Taint runs in two phases, because a function body used to be walked at its
/// **definition** — with only the taint known at that point. A global tainted
/// further down was invisible inside the body, and an array that fell through
/// `ssa_taint`'s documented array gap leaked unattested:
///
/// ```text
/// fn user { for v in @g; do echo "$v"; done }
/// argv g
/// user            # printed unattested argv
/// ```
///
/// Phase 1 walks the whole top level, deferring function definitions; phase 2
/// then walks each body against the completed set. That is sound with no
/// fixpoint **because SB5067 forbids a function from tainting a global**: the
/// top level is the only place a global's taint can arrive, so phase 1's set
/// is final before any body is read. It also costs nothing in precision —
/// top-level flow order is preserved exactly, so `argv a` followed by an
/// attest-all still leaves `a` clean for every body.
///
/// A function defined *inside* top-level control flow keeps the old
/// definition-point walk (see `taint-fn-in-toplevel-control-flow`).
pub fn check_taint(program: &Program) -> Vec<Diagnostic> {
    let mut tainted: HashSet<String> = HashSet::new();
    let mut diags = Vec::new();
    let info = analyze_slots(program);
    let map_names = collect_map_names(program);
    let cx = SlotCx {
        probing: false,
        info: &info,
        map_names: &map_names,
    };
    for f in taint_top_level(program, &mut tainted, &cx, &mut diags) {
        taint_function_body(f, &tainted, &cx, &mut diags);
    }
    diags
}

/// Phase 1: walk the top level, deferring every function definition.
///
/// SB5067 makes the top level the only place a global's taint can arrive, so
/// the set this leaves behind is final — the deferred bodies can then be
/// checked against it in one pass, with no fixpoint.
fn taint_top_level<'a>(
    program: &'a Program,
    tainted: &mut HashSet<String>,
    cx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) -> Vec<&'a Function> {
    let mut deferred = Vec::new();
    for block in program_blocks(program) {
        taint_top_block(block, tainted, cx, diags, &mut deferred);
    }
    deferred
}

/// One top-level block. A function definition is deferred to the second pass
/// and breaks the DD#121 adjacency chain: the statement after it is adjacent
/// to a definition, which taints nothing.
fn taint_top_block<'a>(
    block: &'a [Statement],
    tainted: &mut HashSet<String>,
    cx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
    deferred: &mut Vec<&'a Function>,
) {
    let mut just = JustTainted::new();
    for stmt in block {
        match &stmt.kind {
            StmtKind::Function(f) => {
                deferred.push(f);
                just = JustTainted::new();
            }
            _ => just = taint_step(stmt, tainted, &just, cx, diags),
        }
    }
}

/// DD#121 R1: the names the *immediately preceding* sibling statement tainted.
///
/// An `attest` clears taint only for a subject in this set, which is what makes
/// the whole set grow-only: an insert and its matching removal are always one
/// statement apart in one block, so nothing observes the taint in between and
/// no removal can cross a block boundary or a branch merge.
type JustTainted = HashSet<String>;

/// One step of a DD#121 block walk: run `stmt`, then report the names it taints
/// — the `just` set the *next* statement's attest is allowed to clear.
fn taint_step(
    stmt: &Statement,
    tainted: &mut HashSet<String>,
    just: &JustTainted,
    cx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) -> JustTainted {
    taint_stmt(stmt, tainted, just, cx, diags);
    if cx.probing {
        return JustTainted::new();
    }
    stmt_taints(stmt, cx)
}

/// DD#121: the names `stmt` itself taints.
///
/// Deliberately NOT the difference the statement made to the incoming set. A
/// name that is already tainted makes no difference to it, yet a second
/// `argv a` is still the operation that taints `a` and the attest below it is
/// still adjacent. So the effect is measured against an empty set.
///
/// Every statement kind is measured, not just the leaves. R1 is a locality
/// rule, not the soundness mechanism — the per-path merge is what makes a
/// removal unobservable elsewhere — so "the statement above taints it" is the
/// whole test, whatever shape that statement has. `h=$(getenv X) || h=DEF` is
/// one statement and does taint `h`.
fn stmt_taints(stmt: &Statement, cx: &SlotCx) -> JustTainted {
    let probe_cx = SlotCx {
        info: cx.info,
        map_names: cx.map_names,
        probing: true,
    };
    let mut probe = HashSet::new();
    let mut discard = Vec::new();
    taint_stmt(
        stmt,
        &mut probe,
        &JustTainted::new(),
        &probe_cx,
        &mut discard,
    );
    probe
}

/// Walk a block of statements in order, threading DD#121's adjacency set.
///
/// `initial_just` is empty for a nested body — a body cannot attest something
/// tainted outside it — and the caller's `just` for the two positions that are
/// syntactically adjacent to the preceding statement: an `if` condition and the
/// left operand of `&&`/`||`. That is what lets `argv a` be followed by
/// `if attest v in a [[ … ]]`.
fn taint_block(
    stmts: &[Statement],
    tainted: &mut HashSet<String>,
    initial_just: &JustTainted,
    cx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    let mut just = initial_just.clone();
    for stmt in stmts {
        just = taint_step(stmt, tainted, &just, cx, diags);
    }
}

/// DD#121: a loop body runs 0..n times, so a name tainted late in the body is
/// tainted at the *top* of the next iteration. One pass would miss that, so the
/// entry state is driven to a fixpoint before the real walk.
///
/// It converges because R1 makes the set grow-only: every removal is confined
/// to an adjacent pair inside one block, so unioning a pass's output back into
/// its input is monotone. Probe diagnostics are discarded — the caller emits
/// them once, walking against the stable set.
fn loop_entry_fixpoint(bodies: &[&[Statement]], entry: &mut HashSet<String>, cx: &SlotCx) {
    // Bounded by the number of distinct names, which cannot grow during the
    // loop; the cap is a belt-and-braces guard against a future non-monotone
    // edit turning this into a hang.
    for _ in 0..MAX_TAINT_FIXPOINT_PASSES {
        let mut probe = entry.clone();
        let mut sink = Vec::new();
        for b in bodies {
            taint_block(b, &mut probe, &JustTainted::new(), cx, &mut sink);
        }
        let before = entry.len();
        entry.extend(probe);
        if entry.len() == before {
            return;
        }
    }
}

const MAX_TAINT_FIXPOINT_PASSES: usize = 64;

// ── DD#46 / DD#60: top-level positional rejection (SB5024) ────────
//
// `$@`, `$*`, `$1..$N`, `$#` are function-local — they refer to the
// nearest enclosing call's arguments, never to program argv. At top
// level they are a compile error directing to the argv/arg/argc
// builtins. Inside a function body they are clean and accepted (the
// caller's args are statically visible). `$0` is handled separately by
// REJECTED_POSITIONAL (SB5017) because it is unsupported everywhere,
// not just at top level.

/// Reject top-level positional parameters, directing to the DD#60 builtins.
/// DD#84: `${…:~}` (elide) is legal only in an *elidable position* — a direct
/// argument of a command / builtin, or a direct element of an array literal —
/// where dropping the token is well-defined. Anywhere a value is required (a
/// scalar assignment RHS, a `[[ ]]` operand, a subscript index, nested inside a
/// concatenation / double-quote) it is rejected: SB5046. `:~` never yields a
/// value, so a non-list position has no meaning (use `:-` for a default).
pub fn check_elide_positions(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for stmt in &program.statements {
        check_elide_stmt(stmt, &mut diags);
    }
    if let Some(ref end) = program.end_block {
        for stmt in end {
            check_elide_stmt(stmt, &mut diags);
        }
    }
    diags
}

fn check_elide_stmt(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    // A command's direct args are elidable; its name is not.
    if let StmtKind::Command(c) = &stmt.kind {
        check_elide_expr(&c.name, false, diags);
        let elidable = command_args_elidable(expr_literal_str(&c.name));
        for a in &c.args {
            check_elide_expr(a, elidable, diags);
        }
    } else {
        stmt.direct_exprs(&mut |e| check_elide_expr(e, false, diags));
    }
    // A bare `(( … ))` holds an `ArithExpr` directly, which `direct_exprs`
    // cannot surface (same shape as check_positionals_direct).
    if let StmtKind::ArithTest(a) = &stmt.kind {
        check_elide_arith(a, diags);
    }
    check_elide_children(stmt, diags);
}

/// DD#84: is this command's argument list an **elidable** position — one
/// where `${x:~}` may contribute zero tokens?
///
/// The DD names three elidable positions, "a position where a token is one
/// of a *sequence* and dropping it is well-defined":
///
///   1. an external / command argument list (`ls ${args[0]:~} rest`);
///   2. a `sash`-**varargs** builtin arg list (`echo ${x:~}`);
///   3. an array literal element — not a command, handled by the expression
///      walk rather than here.
///
/// So for a command the question is exactly "is this a fixed-arity builtin?",
/// and [`crate::ssa::BuiltinCmd::from_name`] is the canonical answer. A name
/// it does not recognise is an external program or a user function call —
/// both are (1).
///
/// **This is an allowlist, and it is total on purpose — do not add a
/// wildcard.** It used to read `!matches!(name, "readline" | "read")`, which
/// was never a design: those two were the exceptions that had been
/// *reported*, each appended after it panicked codegen. Eleven more were
/// still open (`cd`, `match`, `sub`, `gsub`, `toupper`, `tolower`, `strlen`,
/// `basename`, `dirname`, `return`, `exec`), and the same list had already
/// been patched twice before — once for arithmetic, once for `read`. Under a
/// denylist a newly added builtin is elidable by default and its failure mode
/// is an ICE; under this one it is non-elidable until someone decides
/// otherwise, and the arms below make that a decision the compiler will not
/// let anyone skip.
fn command_args_elidable(name: &str) -> bool {
    use crate::ssa::BuiltinCmd as B;
    match B::from_name(name) {
        // (1) An external program or a user function call: `from_name` knows
        // neither, so both arrive as `Other`.
        B::Other => true,

        // (2) The varargs builtins. `echo ${x:~}` is the DD's own worked
        // example; `write` IS `echo` minus the newline (DD#99); `exec CMD …`
        // builds an external command's argv, which is (1) wearing a builtin's
        // name; `:` discards a list it never evaluates.
        B::Echo | B::Write | B::Exec | B::Noop => true,

        // Fixed-arity builtins. Dropping an argument here does not shorten a
        // list, it SHIFTS the roles — `read ${u:~} f` becomes `read f` and
        // reads stdin instead, a different input source chosen silently. That
        // is the DD#69 hazard, so the slot is non-elidable and SB5046 says so.
        B::Cd
        | B::Read
        | B::Mapfile
        | B::Argv
        | B::Unset
        | B::Wait
        | B::Close
        | B::Signal
        | B::Exit
        | B::Return
        | B::Break
        | B::Continue => false,

        // Value-producing intrinsics: every one has a fixed arity in
        // `ssa::value_intrinsic_arity`, or (`join`, `matchall`) a fixed first
        // role followed by a collection, which is not a free-form list either.
        B::Len
        | B::Strlen
        | B::Keys
        | B::Join
        | B::Sub
        | B::Gsub
        | B::Match
        | B::MatchAll
        | B::Toupper
        | B::Tolower
        | B::Basename
        | B::Dirname => false,

        // `[[ … ]]` and its attest form. DD#84 lists a `[[ … ]]` operand
        // among the NON-elidable positions by name: an operand that vanished
        // would change which test is being run, not shorten a list.
        B::Test | B::Attest => false,

        // Compiler-internal sigils. The transform synthesises these; a user
        // cannot write one, so no `:~` can reach their arguments. `false` is
        // the answer that stays correct if that ever changes.
        B::NegateTestResult
        | B::SetTestFromStatus
        | B::LetExit
        | B::SubshellStart
        | B::StoreBgPid
        | B::TrapSignal
        | B::SignalFlag
        | B::TableDelete
        | B::TableDeleteIdx => false,
    }
}

/// Recurse into a statement's nested bodies.
fn check_elide_children(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    for body in stmt.child_bodies() {
        check_elide_stmt_seq(body, diags);
    }
}

/// Arithmetic is a value context: an imported `${…}` there can never be
/// elided, so every embedded expansion is checked non-elidable. Without this
/// the walk stopped at the `ArithExpr` boundary and `(( y = ${x:~} ))` reached
/// codegen, where lowering a scalar `ElideEmpty` panics.
fn check_elide_arith(a: &crate::ir::ArithExpr, diags: &mut Vec<Diagnostic>) {
    crate::ir::walk_arith_expansions(a, &mut |e| check_elide_expr(e, false, diags));
}

fn check_elide_expr(expr: &Expr, elidable: bool, diags: &mut Vec<Diagnostic>) {
    match &expr.kind {
        ExprKind::ParameterExpansion(pe) => check_elide_pexp(pe, expr.span, elidable, diags),
        // Array-literal elements are elidable; every other sub-expr is not.
        ExprKind::Array(elems) => check_elide_seq(elems, true, diags),
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) => {
            check_elide_seq(parts, false, diags)
        }
        ExprKind::AssocArray(pairs) => check_elide_pairs(pairs, diags),
        ExprKind::Subscript { index, .. } => check_elide_expr(index, false, diags),
        // A `[[ … ]]` test arrives as a command arg (elidable=true above), but
        // its operands are value positions — reset to non-elidable.
        ExprKind::Test(t) => check_elide_test(t, diags),
        // `$(( … ))` in value position — its operands are value positions too.
        ExprKind::Arithmetic(a) => check_elide_arith(a, diags),
        _ => {}
    }
}

fn check_elide_seq(parts: &[Expr], elidable: bool, diags: &mut Vec<Diagnostic>) {
    for p in parts {
        check_elide_expr(p, elidable, diags);
    }
}

fn check_elide_pairs(pairs: &[(String, Expr)], diags: &mut Vec<Diagnostic>) {
    for (_, v) in pairs {
        check_elide_expr(v, false, diags);
    }
}

fn check_elide_stmt_seq(stmts: &[Statement], diags: &mut Vec<Diagnostic>) {
    for s in stmts {
        check_elide_stmt(s, diags);
    }
}

/// Render the expansion the way the author wrote it.
///
/// DD#91's 2026-09-09 addendum split `!` out of the operator into its own
/// flag, so `pe.operator` alone spells `${x:~}` for someone who actually
/// wrote `${x:!~}` — quoting a different expansion back at them.
fn elide_sigil(pe: &ParameterExpansion) -> String {
    let op = pe.operator.as_deref().unwrap_or(":~");
    if pe.taint_off {
        format!(":!{}", op.trim_start_matches(':'))
    } else {
        op.to_string()
    }
}

/// What to suggest to an author whose `:~`/`:!~` landed in a value position.
///
/// The `!` has to survive the rewrite. Advising plain `${x:-DEFAULT}` for a
/// `:!~` drops the taint-off, so an author who follows it trades SB5046 for
/// SB5020 — the trust they asked for is exactly what the advice removed.
fn elide_advice(taint_off: bool, name: &str) -> String {
    if taint_off {
        return format!(
            "use `${{{name}:!-DEFAULT}}` for an empty/default value — keep the \
             `!`, or the subject's taint comes back"
        );
    }
    format!("use `${{{name}:-DEFAULT}}` for an empty/default value")
}

fn check_elide_pexp(
    pe: &ParameterExpansion,
    span: Span,
    elidable: bool,
    diags: &mut Vec<Diagnostic>,
) {
    if pe.operator.as_deref() == Some(":~") && !elidable {
        // `:!` alone never reaches here: the addendum made it a flag, so its
        // operator is `None` and it is legal in every position a bare
        // expansion is. Only the elide (`:~`, `:!~`) is position-restricted.
        let sigil = elide_sigil(pe);
        // Name the three positions DD#84 actually lists. Saying just "an
        // argument list" is the loose reading that put the allow-by-default
        // bug in `command_args_elidable` in the first place: a fixed-arity
        // builtin's argument is a command argument too, so an author whose
        // `read ${x:!~} f` was rejected was being told that the very position
        // they had used was the valid one.
        let advice = elide_advice(pe.taint_off, &pe.name);
        diags.push(Diagnostic::error_span(
            span,
            "SB5046",
            format!(
                "`${{{}{}}}` (elide) is only valid in a VARIADIC list position — \
                 the argument list of an external command, a user function, or \
                 `echo`/`write`/`exec`, or an array literal element — where \
                 dropping the token merely shortens the list. A fixed-arity \
                 builtin takes a value in each slot, not one of a list, so \
                 eliding would shift the remaining arguments into the wrong \
                 roles. Here a value is required; {}.",
                pe.name, sigil, advice
            ),
        ));
    }
    if let Some(op) = &pe.operand {
        check_elide_expr(op, false, diags);
    }
    if let Some(idx) = &pe.index {
        check_elide_expr(idx, false, diags);
    }
}

fn check_elide_test(t: &TestExpr, diags: &mut Vec<Diagnostic>) {
    match t {
        TestExpr::Unary { operand, .. } => check_elide_expr(operand, false, diags),
        TestExpr::Binary { left, right, .. } => {
            check_elide_expr(left, false, diags);
            check_elide_expr(right, false, diags);
        }
        TestExpr::Not(inner) => check_elide_test(inner, diags),
        TestExpr::And(l, r) | TestExpr::Or(l, r) => {
            check_elide_test(l, diags);
            check_elide_test(r, diags);
        }
    }
}

pub fn check_positionals(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for stmt in &program.statements {
        check_positionals_stmt(stmt, false, &mut diags);
    }
    if let Some(ref end) = program.end_block {
        for stmt in end {
            check_positionals_stmt(stmt, false, &mut diags);
        }
    }
    diags
}

fn check_positionals_stmt(stmt: &Statement, in_function: bool, diags: &mut Vec<Diagnostic>) {
    // A function body switches context: its positionals are the caller's
    // statically-known args, so they are accepted (clean).
    if let StmtKind::Function(f) = &stmt.kind {
        for s in &f.body {
            check_positionals_stmt(s, true, diags);
        }
        return;
    }
    if !in_function {
        check_positionals_direct(stmt, diags);
    }
    for body in stmt.child_bodies() {
        for s in body {
            check_positionals_stmt(s, in_function, diags);
        }
    }
}

/// Flag positionals owned directly by `stmt` (top-level context). A bare
/// `(( ))` test/condition is `StmtKind::ArithTest`, which holds an
/// `ArithExpr` directly (not wrapped in an `Expr`), so `direct_exprs`
/// cannot surface it — descend explicitly.
fn check_positionals_direct(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    stmt.direct_exprs(&mut |expr| check_positionals_expr(expr, diags));
    if let StmtKind::ArithTest(a) = &stmt.kind {
        check_positionals_arith(a, stmt.span, diags);
    }
}

/// Walk a top-level expression tree, flagging positional reads. A nested
/// command substitution runs in the same (top-level) context, so it is
/// re-entered at the statement level to keep any nested function body's
/// positionals correctly scoped.
fn check_positionals_expr(expr: &Expr, diags: &mut Vec<Diagnostic>) {
    match &expr.kind {
        ExprKind::SimpleExpansion(name) => flag_positional(name, expr.span, diags),
        ExprKind::ParameterExpansion(pe) => {
            flag_positional(&pe.name, expr.span, diags);
            flag_args_toplevel(&pe.name, expr.span, diags);
            check_positionals_opt(pe.operand.as_deref(), diags);
        }
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) | ExprKind::Array(parts) => {
            check_positionals_seq(parts, diags)
        }
        ExprKind::AssocArray(pairs) => check_positionals_seq(pairs.iter().map(|(_, v)| v), diags),
        ExprKind::Subscript { name, index } => {
            flag_args_toplevel(name, expr.span, diags);
            check_positionals_expr(index, diags);
        }
        ExprKind::Test(t) => check_positionals_test(t, diags),
        ExprKind::Arithmetic(a) => check_positionals_arith(a, expr.span, diags),
        _ => {}
    }
}

/// DD#46: positionals reached via `$#`/`$1`/`$@` inside `(( ))` / `$(( ))`
/// are still function-local. Walk the arith tree and flag positional
/// `DollarVar`s (bare `Var`/lvalue names are not positionals).
fn check_positionals_arith(a: &crate::ir::ArithExpr, span: Span, diags: &mut Vec<Diagnostic>) {
    use crate::ir::ArithExpr::*;
    match a {
        PreIncDec { .. } => unreachable!("{}", crate::ir::PREFIX_INCDEC_INVARIANT),
        DollarVar(name) => flag_positional(name, span, diags),
        // The only child that is an `Expr` rather than an `ArithExpr`, so
        // `children()` does not yield it and the descent is spelled out here.
        Expansion(inner) => check_positionals_expr(inner, diags),
        _ => {
            for child in a.children() {
                check_positionals_arith(child, span, diags);
            }
        }
    }
}

fn check_positionals_seq<'a>(
    exprs: impl IntoIterator<Item = &'a Expr>,
    diags: &mut Vec<Diagnostic>,
) {
    for e in exprs {
        check_positionals_expr(e, diags);
    }
}

fn check_positionals_opt(expr: Option<&Expr>, diags: &mut Vec<Diagnostic>) {
    if let Some(e) = expr {
        check_positionals_expr(e, diags);
    }
}

fn check_positionals_test(t: &TestExpr, diags: &mut Vec<Diagnostic>) {
    match t {
        TestExpr::Unary { operand, .. } => check_positionals_expr(operand, diags),
        TestExpr::Binary { left, right, .. } => {
            check_positionals_expr(left, diags);
            check_positionals_expr(right, diags);
        }
        TestExpr::Not(inner) => check_positionals_test(inner, diags),
        TestExpr::And(l, r) | TestExpr::Or(l, r) => {
            check_positionals_test(l, diags);
            check_positionals_test(r, diags);
        }
    }
}

/// Emit SB5024 if `name` is a function-local scalar positional form (`$1`..`$N`
/// or `$#`). `$0` is excluded — it is rejected everywhere by
/// REJECTED_POSITIONAL (SB5017). `$@`/`$*` are not handled here — they are not
/// in the language and are rejected everywhere by `check_dollar_at` (SB5045).
fn flag_positional(name: &str, span: Span, diags: &mut Vec<Diagnostic>) {
    let directive = if name == "#" {
        "$# is function-local — use $(argc) at top level \
         (C argc; subtract 1 for the user-arg count)"
            .to_string()
    } else if name != "0" && !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()) {
        format!(
            "${} is function-local — use $(arg {}) at top level",
            name, name
        )
    } else {
        return;
    };
    diags.push(Diagnostic::error_span(span, "SB5024", directive));
}

/// DD#75 §5: the implicit `args` array (and its cells/splat) is function-local,
/// exactly like `$1`/`$@`/`$#`. A top-level `@args` / `${args[i]}` / `${#args}`
/// reference is rejected — the process command line is `$(argv)`/`$(arg N)`.
fn flag_args_toplevel(name: &str, span: Span, diags: &mut Vec<Diagnostic>) {
    if name == "args" {
        diags.push(Diagnostic::error_span(
            span,
            "SB5024",
            "`args` is the function-local argument array — it does not exist at \
             top level; use $(argv) / $(arg N) / $(argc) for the process command line"
                .to_string(),
        ));
    }
}

// ── DD#75/DD#81: `$@` is dropped inside a function — use `@args` ──────────

/// Reject `$@` / `$*` (and `${@}` / `${*}`) **everywhere** (SB5045). Neither is
/// part of the language: both are `$`-sigil forms that *splat*/join, which
/// DD#75 §4 assigns solely to `@` ("`@` is the sole splat"). The suggested
/// replacement is context-sensitive — inside a function the args are `@args`
/// (splat) / `$(join args)` (joined); at top level they are `$(argv 1)` (the
/// user-arg splat) / `$(arg N)` (one). `$1`/`$#` are genuinely scalar, so they
/// stay function-local (SB5024) rather than being rejected outright.
pub fn check_dollar_at(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    // Both blocks, at `in_fn = false`: the DD#56 END body is top-level scope,
    // not a function, so `$@` there is the top-level reject and the advice
    // points at `$(argv 1)` rather than `@args`.
    for stmt in program_blocks(program).into_iter().flatten() {
        dollar_at_stmt(stmt, false, &mut diags);
    }
    diags
}

fn dollar_at_stmt(stmt: &Statement, in_fn: bool, diags: &mut Vec<Diagnostic>) {
    if let StmtKind::Function(f) = &stmt.kind {
        for s in &f.body {
            dollar_at_stmt(s, true, diags);
        }
        return;
    }
    stmt.direct_exprs(&mut |e| dollar_at_expr(e, in_fn, diags));
    if let StmtKind::ArithTest(a) = &stmt.kind {
        dollar_at_arith(a, stmt.span, in_fn, diags);
    }
    for body in stmt.child_bodies() {
        for s in body {
            dollar_at_stmt(s, in_fn, diags);
        }
    }
}

fn dollar_at_expr(expr: &Expr, in_fn: bool, diags: &mut Vec<Diagnostic>) {
    match &expr.kind {
        ExprKind::SimpleExpansion(n) if n == "@" || n == "*" => {
            dollar_at_diag(expr.span, n, in_fn, diags)
        }
        ExprKind::ParameterExpansion(pe) => dollar_at_pexp(expr.span, pe, in_fn, diags),
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) | ExprKind::Array(parts) => {
            dollar_at_exprs(parts, in_fn, diags)
        }
        ExprKind::AssocArray(pairs) => {
            for (_, v) in pairs {
                dollar_at_expr(v, in_fn, diags);
            }
        }
        ExprKind::Subscript { index, .. } => dollar_at_expr(index, in_fn, diags),
        ExprKind::Arithmetic(a) => dollar_at_arith(a, expr.span, in_fn, diags),
        ExprKind::Test(t) => dollar_at_test(t, in_fn, diags),
        _ => {}
    }
}

fn dollar_at_exprs(exprs: &[Expr], in_fn: bool, diags: &mut Vec<Diagnostic>) {
    for e in exprs {
        dollar_at_expr(e, in_fn, diags);
    }
}

fn dollar_at_pexp(span: Span, pe: &ParameterExpansion, in_fn: bool, diags: &mut Vec<Diagnostic>) {
    if pe.name == "@" || pe.name == "*" {
        dollar_at_diag(span, &pe.name, in_fn, diags);
    }
    if let Some(op) = &pe.operand {
        dollar_at_expr(op, in_fn, diags);
    }
}

/// `$@` / `$*` reached through `$(( … ))` / `(( … ))` — a bare `$@` operand is
/// an `ArithExpr::DollarVar`, so the `Expr` walk cannot surface it. Child
/// operands are gathered by `ArithExpr::children` so this stays a three-way match.
fn dollar_at_arith(a: &crate::ir::ArithExpr, span: Span, in_fn: bool, diags: &mut Vec<Diagnostic>) {
    use crate::ir::ArithExpr::*;
    match a {
        DollarVar(name) if name == "@" || name == "*" => dollar_at_diag(span, name, in_fn, diags),
        Expansion(inner) => dollar_at_expr(inner, in_fn, diags),
        _ => {
            for child in a.children() {
                dollar_at_arith(child, span, in_fn, diags);
            }
        }
    }
}

fn dollar_at_test(t: &TestExpr, in_fn: bool, diags: &mut Vec<Diagnostic>) {
    match t {
        TestExpr::Unary { operand, .. } => dollar_at_expr(operand, in_fn, diags),
        TestExpr::Binary { left, right, .. } => {
            dollar_at_expr(left, in_fn, diags);
            dollar_at_expr(right, in_fn, diags);
        }
        TestExpr::Not(inner) => dollar_at_test(inner, in_fn, diags),
        TestExpr::And(a, b) | TestExpr::Or(a, b) => {
            dollar_at_test(a, in_fn, diags);
            dollar_at_test(b, in_fn, diags);
        }
    }
}

fn dollar_at_diag(span: Span, sigil: &str, in_fn: bool, diags: &mut Vec<Diagnostic>) {
    let hint = if in_fn {
        "use `@args` (the splat), or `$(join args)` for a joined string"
    } else {
        "at top level use `$(argv 1)` (the user-arg splat), or `$(arg N)` for one"
    };
    diags.push(Diagnostic::error_span(
        span,
        "SB5045",
        format!("sash has no `${sigil}` — {hint}"),
    ));
}

//
// `$(argv)` / `$(argv N)` are array-returning splats. They are legal only
// as the sole source of a for-in header or as the right-hand side of an
// `array a=$(argv …)` snapshot. Every other appearance is SB5025. Quoting
// any array splat ("${a[@]}" / "$a[@]" / "$(argv)") is SB5026 (DD#22:
// quotes always yield a single string). A for-in header that mixes a
// tainted argv splat with any other word is SB5027.

/// Reject misuse of array splats and quoted splats.
pub fn check_argv_splat(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for stmt in &program.statements {
        check_argv_splat_stmt(stmt, &mut diags);
    }
    if let Some(ref end) = program.end_block {
        for stmt in end {
            check_argv_splat_stmt(stmt, &mut diags);
        }
    }
    diags
}

fn check_argv_splat_stmt(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    match &stmt.kind {
        StmtKind::For(f) => {
            check_for_header_splat(f, diags);
            check_argv_splat_body(&f.body, diags);
        }
        StmtKind::Declaration(d) => check_argv_splat_decl(d, diags),
        StmtKind::Assignment(a) => check_argv_splat_assignment(a, diags),
        StmtKind::Command(c) => {
            check_echo_splat(c, diags);
            check_external_splat(c, diags);
            stmt.direct_exprs(&mut |e| check_argv_splat_expr(e, diags));
        }
        // A redirect target is a single path, so it is a scalar position. This
        // was unchecked: `> @a` compiled and then misdirected — with one
        // element it silently became the target, with two it wrote nowhere.
        StmtKind::Redirected(r) => {
            check_redirect_target_splats(r, diags);
            check_argv_splat_children(stmt, diags);
        }
        _ => {
            stmt.direct_exprs(&mut |e| check_argv_splat_expr(e, diags));
            check_argv_splat_children(stmt, diags);
        }
    }
}

/// Every redirect target of a redirected statement, in scalar position.
fn check_redirect_target_splats(r: &crate::ir::Redirected, diags: &mut Vec<Diagnostic>) {
    for redir in &r.redirects {
        check_scalar_position_splat(&redir.target, diags);
    }
}

/// Recurse into a statement's nested bodies.
fn check_argv_splat_children(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    for body in stmt.child_bodies() {
        check_argv_splat_body(body, diags);
    }
}

fn check_argv_splat_body(stmts: &[Statement], diags: &mut Vec<Diagnostic>) {
    for s in stmts {
        check_argv_splat_stmt(s, diags);
    }
}

/// DD#69: a bare `@a` array splat (no empty-disposition) passed to an
/// EXTERNAL command is SB5032 — an empty array would silently run the
/// command with too few args. Require `@{a:-WORD}` / `@{a:?WORD}`. Builtins
/// and for-in are exempt (this only runs for external command statements).
/// DD#92 addendum (DG 2026-09-05): reject every write to the immutable, always-
/// clean function-argument array `args` (SB5052). Run as its own pass over the
/// program (and the END block), like the other `check_*` gates.
pub fn check_args_immutable(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for stmt in program
        .statements
        .iter()
        .chain(program.end_block.iter().flatten())
    {
        check_args_immutable_stmt(stmt, &mut diags);
    }
    diags
}

/// DD#92 addendum (DG 2026-09-05): the function-argument array `args` is
/// **immutable and always clean**. A tainted value can never be *passed* as an
/// argument (passing is a use-site read → SB5020), so the args a function
/// receives are unconditionally clean; to keep that a true invariant, `args`
/// cannot be written. Reject every write to it — an assignment (`args=…` /
/// `args[i]=…`), a bulk fill (`read -a args` / `argv … args` / `mapfile args`),
/// a declaration (`array args=…`), or a attest-all (`attest … in args`, which is
/// meaningless since `args` can't be tainted). SB5052. Use a local array.
fn check_args_immutable_stmt(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    match &stmt.kind {
        StmtKind::Assignment(a) if a.name == "args" => push_args_write(stmt.span, diags),
        StmtKind::Declaration(d) if d.assignments.iter().any(|a| a.name == "args") => {
            push_args_write(stmt.span, diags)
        }
        StmtKind::Command(c) if command_writes_args(c) => push_args_write(stmt.span, diags),
        _ => {}
    }
    for body in stmt.child_bodies() {
        for s in body {
            check_args_immutable_stmt(s, diags);
        }
    }
}

/// True if `c` is a builtin that writes the `args` array: `read -a args`,
/// `read [SRC] args`, `readline args`, `argv … args`, `mapfile/readarray
/// args`, or `attest … in args`.
fn command_writes_args(c: &Command) -> bool {
    match expr_literal_str(&c.name) {
        // DD#99: the target is the last argument for both arities
        // (`read ARRAY` and `read SRC ARRAY`), and `readline`'s only
        // one. DD#63 `match S P ARR` / `matchall S P ARR` are bulk fills
        // too, with the same trailing target.
        "read" | "readline" => c.args.last().map(expr_literal_str) == Some("args"),
        "match" | "matchall" => {
            c.args.len() == 3 && c.args.last().map(expr_literal_str) == Some("args")
        }
        "argv" => argv_target_name(&c.args) == Some("args"),
        "attest" => attest_array_target(&c.args) == Some("args"),
        _ => false,
    }
}

/// The target array of `argv [N] a` — the last positional (non-flag) arg.
fn argv_target_name(args: &[Expr]) -> Option<&str> {
    args.iter()
        .map(expr_literal_str)
        .rfind(|s| !s.starts_with('-'))
}

/// The array/map name of a `attest x in a` / `attest k v in m` (the taint-clearing
/// subject), or `None` for the scalar `attest v` form.
fn attest_array_target(args: &[Expr]) -> Option<&str> {
    match classify_attest(args)?.0 {
        AttestForm::Indexed { array, .. } => Some(array),
        AttestForm::Map { map, .. } => Some(map),
        AttestForm::Scalar { .. } => None,
    }
}

fn push_args_write(span: Span, diags: &mut Vec<Diagnostic>) {
    diags.push(Diagnostic::error_span(
        span,
        "SB5052",
        "`args` is the function's immutable argument array — it can't be written \
         (assigned, filled via read/argv/mapfile, declared, or attested). Function \
         args are always clean: a tainted value can't be passed as an argument \
         (passing is a read). Use a differently named local array."
            .to_string(),
    ))
}

fn check_external_splat(c: &Command, diags: &mut Vec<Diagnostic>) {
    // Post-resolution (DD#23) an external command's head is an absolute path;
    // builtins, user functions and intrinsics keep bare names and are exempt
    // (DD#69: the rule is external-only).
    if !expr_literal_str(&c.name).starts_with('/') {
        return;
    }
    for arg in &c.args {
        if let Some(d) = external_splat_diag(arg) {
            diags.push(d);
        }
    }
}

/// SB5032 for a bare `@a` splat into an external command, or `None` when the arg
/// is not such a splat or is the exempt `@args` (DD#92: arity-guaranteed ≥1).
fn external_splat_diag(arg: &Expr) -> Option<Diagnostic> {
    // A bare `@args` splat is allowed: it implies the function takes ≥1 argument
    // (the arity analysis records a required index 0, so a 0-arg call is SB5042
    // and the splat is provably non-empty). Only `@args` (the function args)
    // earns this; a general `@a` keeps SB5032 — DD#69 has no static-non-empty
    // exemption, and only `args` carries an arity guarantee.
    if bare_arrvals_splat_name(&arg.kind)? == "args" {
        return None;
    }
    Some(Diagnostic::error_span(
        arg.span,
        "SB5032",
        "a bare `@a` array splat into an external command is unsafe — an \
         empty array would run it with too few arguments. State the empty \
         case: `@{a:-WORD}` (empty → the single arg WORD) or `@{a:?WORD}` \
         (empty → abort with WORD)"
            .to_string(),
    ))
}

/// The array name behind a **bare** `@a` splat — operator exactly `arrvals`,
/// i.e. WITHOUT a `@{a:-…}` / `@{a:?…}` disposition — or `None`. Doubles as the
/// "is this a bare splat?" test. Two callers: SB5032 singles out `@args` (the
/// function args, which carries an arity guarantee) from a general `@a`, and
/// SB5058 requires `map t = @a` to state its odd-count disposition.
///
/// No `DoubleQuoted` case, for the reason given on [`splat_collection_name`].
fn bare_arrvals_splat_name(kind: &ExprKind) -> Option<&str> {
    let ExprKind::ParameterExpansion(pe) = kind else {
        return None;
    };
    (pe.operator.as_deref() == Some("arrvals")).then_some(pe.name.as_str())
}

/// DD#60 revision: `array a=$(argv …)` is no longer a snapshot — a command
/// substitution does not yield an array. Reject it (directing to the
/// out-param `argv [N] a`); every other assignment value (and subscript
/// index) is checked for splat misuse.
fn check_argv_splat_decl(d: &Declaration, diags: &mut Vec<Diagnostic>) {
    let is_array_decl = d.flags.iter().any(|fl| fl == "-a" || fl == "-A");
    for a in &d.assignments {
        let Some(value) = &a.value else {
            continue;
        };
        if is_array_decl && is_argv_splat_expr(value) {
            diags.push(Diagnostic::error_span(
                value.span,
                "SB5025",
                "`array a=$(argv …)` is not an array snapshot — a command \
                 substitution does not yield an array; use the out-param form \
                 `argv a`, or `argv N a` to start at argument N"
                    .to_string(),
            ));
            continue;
        }
        check_argv_splat_expr(value, diags);
    }
}

/// Validate a for-in header: reject quoted splats (SB5026), and require an
/// argv splat to be the sole source (SB5027). The argv-splat word itself
/// is an allowed source and is not flagged SB5025.
fn check_for_header_splat(f: &For, diags: &mut Vec<Diagnostic>) {
    let mut argv_splats = 0;
    for w in &f.words {
        if is_array_splat_source(w) {
            argv_splats += 1;
        } else if is_disposition_splat(w) {
            // DD#69 addendum: the empty-disposition is command-argument-only;
            // a for-in source is the bare `@a` (its own zero-match handling).
            diags.push(disposition_position_diag(w.span));
        } else {
            // Catches a quoted splat word (SB5026) and any nested misuse.
            check_argv_splat_expr(w, diags);
        }
    }
    if argv_splats > 0 && f.words.len() > 1 {
        diags.push(Diagnostic::error_span(
            f.words[0].span,
            "SB5027",
            "a for-in over $(argv) must use $(argv) as its sole source — it \
             cannot be mixed with other words or sources"
                .to_string(),
        ));
    }
}

/// DD#69 addendum: a named-array splat (`@a` / `@{a:?}`) is not a scalar —
/// it is legal only as a command argument or a for-in source. As an
/// assignment RHS or subscript index it is SB5025.
fn check_argv_splat_assignment(a: &Assignment, diags: &mut Vec<Diagnostic>) {
    if is_named_array_splat(&a.value) {
        diags.push(named_splat_scalar_diag(a.value.span));
    } else {
        check_argv_splat_expr(&a.value, diags);
    }
    match &a.index {
        Some(idx) if is_named_array_splat(idx) => diags.push(named_splat_scalar_diag(idx.span)),
        Some(idx) => check_argv_splat_expr(idx, diags),
        None => {}
    }
}

/// SB5025 for a named-array splat used where a scalar is expected (DD#69
/// addendum / DD#60 context rule extended to the native `@a` form).
fn named_splat_scalar_diag(span: Span) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5025",
        "an array splat `@a` is not a scalar — it is valid only as a command \
         argument or a for-in source; use `$(join a)` for a joined string, \
         or `${a[i]}` for a single element"
            .to_string(),
    )
}

/// A dispositioned array splat `@{a:-WORD}` / `@{a:?WORD}` / `@{a:!}` (operator
/// `arrvals_default` / `arrvals_abort`) — distinct from the bare `@a`
/// (`arrvals`), which is a valid for-in source. `@{a:!…}` carries its
/// disposition in the operator like any other, so the `!` modifier does not
/// appear here.
fn is_disposition_splat(expr: &Expr) -> bool {
    matches!(&expr.kind, ExprKind::ParameterExpansion(pe)
        if matches!(pe.operator.as_deref(),
            Some("arrvals_default") | Some("arrvals_abort")))
}

/// SB5034 for a `@{a:…}` empty-disposition used outside a command argument
/// (DD#69 addendum): for-in header, scalar, test, arith.
fn disposition_position_diag(span: Span) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5034",
        "the `@{a:-…}` / `@{a:?…}` empty-disposition is only valid on a splat \
         passed as a command argument — a for-in source uses the bare `@a` \
         (it has its own zero-match handling)"
            .to_string(),
    )
}

/// SB5026 for a quoted splat leaf — `"@a"` / `"$(argv)"`. Split out of
/// [`argv_splat_self_diag`] to keep that dispatch flat.
fn quoted_splat_self_diag(expr: &Expr) -> Option<Diagnostic> {
    let ExprKind::DoubleQuoted(parts) = &expr.kind else {
        return None;
    };
    if parts.len() == 1 && is_splat_form(&parts[0]) {
        return Some(quoted_splat_diag(expr.span));
    }
    None
}

/// SB5025 for a bare `$(argv)` splat outside a for-in source. Unlike the
/// `keys` sibling this needs no rendering: `argv` takes no operand, so the
/// fixed text is accurate for every program that reaches it.
fn argv_splat_diag(span: Span) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5025",
        "$(argv) is an array splat — it is valid only as the sole source of \
         a for-in loop; snapshot into an array with `argv a` (or `argv N a` \
         to start at argument N), or use $(arg N) for a single argument"
            .to_string(),
    )
}

/// SB5026 (quoted splat) or SB5025 (bare argv splat in a non-splat context)
/// for `expr` itself — `None` if `expr` is not a splat leaf.
fn argv_splat_self_diag(expr: &Expr) -> Option<Diagnostic> {
    if let Some(diag) = quoted_splat_self_diag(expr) {
        return Some(diag);
    }
    if is_argv_splat_expr(expr) {
        return Some(argv_splat_diag(expr.span));
    }
    if is_keys_splat_expr(expr) {
        return keys_splat_diag(expr);
    }
    None
}

/// The map name in `$(keys config)`. `keys` resolves its operand as a bare
/// map name (DD#75 §3), so a literal is the only spelling that can ever be
/// valid — and the only one this diagnostic may quote, because it quotes it
/// back as *advice*. Every other spelling is SB5062's to report; paraphrasing
/// one here would suggest a `for` line that is itself rejected.
fn keys_splat_map_name(expr: &Expr) -> Option<&str> {
    let ExprKind::CommandSubstitution(stmts) = &expr.kind else {
        return None;
    };
    let inner = match stmts.first().map(|s| &s.kind) {
        Some(StmtKind::Redirected(r)) => &r.body.kind,
        Some(other) => other,
        None => return None,
    };
    let StmtKind::Command(c) = inner else {
        return None;
    };
    match &c.args.first()?.kind {
        ExprKind::Literal(name) => Some(name),
        _ => None,
    }
}

/// SB5025 for a `$(keys m)` splat outside a for-in source. `None` when the
/// operand is not a bare map name — that program has a more fundamental
/// error, and SB5062 names it.
fn keys_splat_diag(expr: &Expr) -> Option<Diagnostic> {
    let map = keys_splat_map_name(expr)?;
    Some(Diagnostic::error_span(
        expr.span,
        "SB5025",
        format!(
            "`$(keys {map})` is an array splat — it is valid only as the sole \
             source of a for-in loop (`for k in $(keys {map})`); it cannot be \
             joined into a scalar or passed as a command argument"
        ),
    ))
}

/// Walk an expression, flagging quoted splats (SB5026) and bare argv splats
/// in a non-splat context (SB5025).
fn check_argv_splat_expr(expr: &Expr, diags: &mut Vec<Diagnostic>) {
    if let Some(diag) = argv_splat_self_diag(expr) {
        diags.push(diag);
        return;
    }
    match &expr.kind {
        ExprKind::Concat(parts) => check_concat_splat_seq(parts, diags),
        ExprKind::DoubleQuoted(parts) | ExprKind::Array(parts) => {
            check_argv_splat_seq(parts, diags)
        }
        ExprKind::AssocArray(pairs) => check_argv_splat_seq(pairs.iter().map(|(_, v)| v), diags),
        ExprKind::Subscript { index, .. } => check_argv_splat_expr(index, diags),
        ExprKind::ParameterExpansion(pe) => check_argv_splat_opt(pe.operand.as_deref(), diags),
        ExprKind::Test(t) => check_argv_splat_test(t, diags),
        _ => {}
    }
}

fn check_argv_splat_seq<'a>(
    exprs: impl IntoIterator<Item = &'a Expr>,
    diags: &mut Vec<Diagnostic>,
) {
    for e in exprs {
        check_argv_splat_expr(e, diags);
    }
}

/// Every part of a `Concat` is a scalar position: the word has other text glued
/// to the splat. `$(argv)y` and `$(keys m)y` were already SB5025 ("it cannot be
/// joined into a scalar"); the named-array family was the one splat form still
/// silent, and it did not merely mis-report — `@{a:-x}y` on `a=(p q)` emitted a
/// single `p qy` where bash emits two arguments, `p` and `qy`. DG ruled the
/// form out rather than implementing bash's attach-to-the-last-element shape.
fn check_concat_splat_seq(parts: &[Expr], diags: &mut Vec<Diagnostic>) {
    for p in parts {
        if is_named_array_splat(p) {
            diags.push(joined_splat_diag(p.span));
        } else {
            check_argv_splat_expr(p, diags);
        }
    }
}

/// The SB5083 "splat joined to adjacent text" diagnostic.
fn joined_splat_diag(span: Span) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5083",
        "an array splat cannot be joined to adjacent text — it would collapse \
         the array into a single argument; give the splat a word of its own, \
         or use `$(join a)` for a joined string"
            .to_string(),
    )
}

fn check_argv_splat_opt(expr: Option<&Expr>, diags: &mut Vec<Diagnostic>) {
    if let Some(e) = expr {
        check_argv_splat_expr(e, diags);
    }
}

/// A scalar position (a `[[ ]]` operand, a redirect target): a named-array
/// splat there is SB5025, exactly as it is on an assignment RHS. Only the
/// assignment path used to check this, so the others accepted `@a` and then
/// did the wrong thing silently.
fn check_scalar_position_splat(e: &Expr, diags: &mut Vec<Diagnostic>) {
    if is_named_array_splat(e) {
        diags.push(named_splat_scalar_diag(e.span));
    } else {
        check_argv_splat_expr(e, diags);
    }
}

fn check_argv_splat_test(t: &TestExpr, diags: &mut Vec<Diagnostic>) {
    match t {
        TestExpr::Unary { operand, .. } => check_scalar_position_splat(operand, diags),
        TestExpr::Binary { left, right, .. } => {
            check_scalar_position_splat(left, diags);
            check_scalar_position_splat(right, diags);
        }
        TestExpr::Not(inner) => check_argv_splat_test(inner, diags),
        TestExpr::And(l, r) | TestExpr::Or(l, r) => {
            check_argv_splat_test(l, diags);
            check_argv_splat_test(r, diags);
        }
    }
}

fn quoted_splat_diag(span: Span) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5026",
        "quoting an array splat is not allowed — quotes yield a single string; \
         use the bare form (@arr or $(argv))"
            .to_string(),
    )
}

/// `$(argv)` / `$(argv N)` — the array-splat command substitution.
///
/// DD#60 gives `argv` two shapes and they collide at one argument: the splat
/// `$(argv 1)` and the out-param fill `argv a`. An index is a bare
/// non-negative decimal literal (SB5085) and a fill target is a bare *name*
/// (SB5063 rejects `argv 1`), so the argument itself discriminates; two
/// arguments is always the fill. Matching on the command *name* alone read
/// `$(argv 0 b; …)` — a legal fill statement — as a misplaced splat and
/// answered SB5025, whose advice is to write `argv N a`: the form the author
/// had already written.
fn is_argv_splat_expr(expr: &Expr) -> bool {
    matches!(&expr.kind, ExprKind::CommandSubstitution(stmts)
        if sole_cmdsub_command(stmts, "argv").is_some_and(argv_command_is_splat))
}

/// `argv` / `argv N` — the splat spelling, as opposed to DD#60's out-param
/// fill `argv a` / `argv N a`. See [`is_argv_splat_expr`] for why the
/// argument is what tells them apart.
fn argv_command_is_splat(c: &Command) -> bool {
    match c.args.as_slice() {
        [] => true,
        [index] => is_bare_index_literal(index),
        _ => false,
    }
}

/// `$(keys m)` — the map-keys array-splat command substitution. Like
/// `$(argv)`, it is legal only as a for-in sole source; SB5025 elsewhere
/// (DD#75 §3 — closes the memory-unsafe silent-join / garbage bypass where
/// keys slipped past the argv-only context guard).
fn is_keys_splat_expr(expr: &Expr) -> bool {
    matches!(&expr.kind, ExprKind::CommandSubstitution(stmts)
        if sole_cmdsub_command(stmts, "keys").is_some())
}

/// An array-returning command-sub splat legal as a for-in source:
/// `$(argv …)` or `$(keys m)`.
fn is_array_splat_source(expr: &Expr) -> bool {
    is_argv_splat_expr(expr) || is_keys_splat_expr(expr)
}

/// DD#69 §3: `echo` join-and-prints its arguments, so a named-array splat
/// there would flatten the array (DD#22) and reintroduce word-splitting
/// downstream. Reject `echo @a` with SB5030, directing to the explicit
/// `echo $(join a)` — the *name*, not the splat: DD#100 removed the splat
/// spelling from `join`, and `$(join @a)` is now SB5062. This advice still
/// said `@a` after that change, so following it swapped one error for
/// another.
fn check_echo_splat(c: &Command, diags: &mut Vec<Diagnostic>) {
    // Unreachable `else`, for the same reason as `resolve::resolve_command`:
    // `check_argv_splat` runs from `run_semantic_checks`, which is reached only
    // after `run_structural_checks` has passed, and a non-`Literal` command
    // head is SB5011 there. 8125 commands, zero non-literal heads.
    let ExprKind::Literal(name) = &c.name.kind else {
        return;
    };
    if name != "echo" && name != "write" {
        return;
    }
    for arg in &c.args {
        if is_named_array_splat(arg) {
            diags.push(Diagnostic::error_span(
                arg.span,
                "SB5030",
                "`echo` joins its arguments into one output stream, so an array \
                 splat there would flatten the array. Use an explicit \
                 join: `echo $(join a)`"
                    .to_string(),
            ));
        }
    }
}

/// `@a` / `${a[@]}` / `$a[@]` — a named-array value/key splat. Unlike
/// [`is_splat_form`] this excludes `$(argv)` (already SB5025).
fn is_named_array_splat(expr: &Expr) -> bool {
    matches!(&expr.kind, ExprKind::ParameterExpansion(pe)
        if is_arrvals_splat_op(pe.operator.as_deref()))
}

/// A bare splat form: `$(argv …)`, an `arrvals` parameter expansion
/// (`@a` / `${a[@]}` / `$a[@]`), or `$(keys a)`.
fn is_splat_form(expr: &Expr) -> bool {
    is_array_splat_source(expr) || is_named_array_splat(expr)
}

/// A named-array value splat operator: `arrvals`, or any dispositioned /
/// per-element `arrvals_*` form — `@{a:?}`, `@{a:-WORD}`, `@{a^^}`, etc.
/// (the parser encodes all of these as `arrvals_<op>`).
/// Recognizing the `arrvals_*` family here is what keeps a dispositioned
/// splat from slipping past the splat guards (SB5030 / SB5026) into a
/// scalar value lowering (which ICEs).
fn is_arrvals_splat_op(op: Option<&str>) -> bool {
    matches!(op, Some("arrvals")) || matches!(op, Some(s) if s.starts_with("arrvals_"))
}

/// The command a `$( … )` body consists of, when the body is **exactly one**
/// statement invoking `name`.
///
/// One statement, not the first of several. `stmts.first()` was the test, and
/// `transform` resolves a splat substitution only when it *is* the whole body,
/// so `for x in $(argv; /usr/bin/touch f)` compiled clean, iterated the
/// program arguments, and never ran the `touch`: every statement after the
/// first was discarded with nothing reported. `keys` escaped the same defect
/// only because SB5073 catches it one pass earlier.
fn sole_cmdsub_command<'a>(stmts: &'a [Statement], name: &str) -> Option<&'a Command> {
    let [stmt] = stmts else {
        return None;
    };
    let c = stmt_command(stmt)?;
    (expr_literal_str(&c.name) == name).then_some(c)
}

/// The `Command` a statement runs, looking through a redirection wrapper.
/// Shared so that every `argv`-shape test sees the same statement — a second
/// unwrap that forgot `Redirected` would let `$(argv >f; CMD)` back past the
/// check that `$(argv; CMD)` fails.
fn stmt_command(stmt: &Statement) -> Option<&Command> {
    let inner = match &stmt.kind {
        StmtKind::Redirected(r) => &r.body.kind,
        other => other,
    };
    match inner {
        StmtKind::Command(c) => Some(c),
        _ => None,
    }
}

/// Like taint_stmt but doesn't mark read/mapfile as tainted
/// (for pipeline stages where input comes from sash code).
fn taint_stmt_no_read_taint(
    stmt: &Statement,
    tainted: &mut HashSet<String>,
    just: &JustTainted,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    if let StmtKind::Command(c) = &stmt.kind {
        let name = expr_literal_str(&c.name);
        // read/mapfile from clean pipe source: don't taint
        if matches!(name, "read" | "readline" | "mapfile") {
            check_clean_read_taint(c, tainted, diags);
            return;
        }
    }
    taint_stmt(stmt, tainted, just, sx, diags);
}

/// `read`/`mapfile` from a clean pipe source: check the name+args for taint but
/// do not taint the read targets.
fn check_clean_read_taint(c: &Command, tainted: &mut HashSet<String>, diags: &mut Vec<Diagnostic>) {
    check_expr_taint(&c.name, tainted, diags);
    for arg in &c.args {
        check_expr_taint(arg, tainted, diags);
    }
}

/// Check if a pipeline stage is an external source (execs). Shared with the
/// transform's SSA-backstop `clean_pipe` computation so the two taint layers
/// classify a stage identically — a divergence here is a laundering hole.
pub(crate) fn is_external_source(stmt: &Statement) -> bool {
    match &stmt.kind {
        StmtKind::Command(c) => {
            let name = expr_literal_str(&c.name);
            // Absolute path = external
            if name.starts_with('/') {
                return true;
            }
            // PATH-resolved non-builtin = external
            if !name.is_empty() && !BUILTINS.contains(&name) {
                return true;
            }
            false
        }
        StmtKind::Redirected(r) => is_external_source(&r.body),
        _ => false,
    }
}

fn taint_stmt(
    stmt: &Statement,
    tainted: &mut HashSet<String>,
    just: &JustTainted,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    match &stmt.kind {
        StmtKind::Assignment(a) => taint_assignment(a, tainted, sx, diags),
        StmtKind::Declaration(d) => taint_declaration(d, tainted, sx, diags),
        StmtKind::Command(c) => taint_command(c, stmt.span, tainted, just, sx, diags),
        StmtKind::Function(f) => taint_function_body(f, tainted, sx, diags),
        StmtKind::Pipeline(p) => taint_pipeline(p, tainted, sx, diags),
        StmtKind::Redirected(r) => taint_redirected(r, tainted, just, sx, diags),
        StmtKind::Case(c) => taint_case_arms(c, tainted, sx, diags),
        StmtKind::For(f) => taint_for(f, tainted, sx, diags),
        // DD#121: the three shapes with alternative paths. Each clones the set
        // per path and unions at the merge, so a removal on one path cannot be
        // seen by a sibling path or by anything after the construct.
        StmtKind::If(i) => taint_if(i, stmt.span, tainted, just, sx, diags),
        StmtKind::While(w) => taint_while(w, tainted, sx, diags),
        StmtKind::List(l) => taint_list(l, stmt.span, tainted, just, sx, diags),
        // `(( … ))` as a statement or an `if`/`while` condition. It has no
        // child bodies, so the `_` arm below walked nothing and every read
        // inside the condition went unchecked — `if (( ${x:-0} > 5 ))` reached
        // the SSA backstop (an ICE-shaped message naming `__arith_test_1`)
        // instead of a located SB5020. Same omission as the `$(( … ))` rvalue.
        StmtKind::ArithTest(a) => check_arith_taint(a, tainted, diags),
        _ => taint_child_bodies(stmt, tainted, just, sx, diags),
    }
    taint_cmdsub_bodies(stmt, tainted, sx, diags);
}

/// Taint-check the statements of every `$( … )` body hanging off `stmt`.
///
/// DD#86: a `$( … )` body is an anonymous function body, so its statements are
/// taint-checked like any others. `taint_stmt` calls this unconditionally,
/// rather than folding it into the `_` arm, because the arms that carry a
/// substitution — Command, Declaration, Assignment — are *leaves*; widening
/// `_` reaches none of them. `taint_child_bodies` stays pinned to
/// `structural_bodies()` so a wrapper statement is not visited twice.
///
/// The set is CLONED per body, matching `taint_function_body`: a name the body
/// introduces is body-local and must not leak outward. A write to a name the
/// enclosing scope already binds does persist at runtime (DD#86), and that is
/// deliberately NOT modelled here — see `notes/tasks/`.
fn taint_cmdsub_bodies(
    stmt: &Statement,
    tainted: &HashSet<String>,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    for body in stmt.cmdsub_bodies() {
        let mut body_tainted = tainted.clone();
        taint_block(body, &mut body_tainted, &JustTainted::new(), sx, diags);
    }
}

fn taint_child_bodies(
    stmt: &Statement,
    tainted: &mut HashSet<String>,
    just: &JustTainted,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    // `structural_bodies`, not `child_bodies`: `check_expr_taint`'s
    // `CommandSubstitution` arm already checks a body as a use site
    // (`check_cmdsub_use_taint`). Taking it here too would check every use in
    // the body twice.
    //
    // DD#121: the branching shapes have their own arms in `taint_stmt`, so
    // everything reaching here is a transparent single-statement wrapper
    // (`Group`, `Suppressed`, `Negated`, `ReturnProducer`) — the wrapped
    // statement sits in the caller's sequence, so the adjacency set passes
    // straight through rather than resetting.
    for body in stmt.structural_bodies() {
        taint_block(body, tainted, just, sx, diags);
    }
}

/// DD#121: `if` / `elif` / `else`, with the branch-scoped cleanliness DD#24's
/// 2026-09-01 amendment specified and the validator never implemented.
///
/// Each guarded body is walked against a *clone* taken after its own condition
/// ran, so a caught attest's removal is visible only inside the branch it
/// guards. The fall-through state carries the *union* of the pre-condition set
/// and the condition's output, which re-establishes the subject's taint on the
/// failing path — the attest did not pass, so it vouched for nothing.
/// The merge is the union of every path, which is monotone and therefore safe
/// under loops and nesting.
fn taint_if(
    i: &If,
    span: Span,
    tainted: &mut HashSet<String>,
    just: &JustTainted,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    let mut fall = tainted.clone();
    // (out-state, does this path reach the merge?)
    let mut paths: Vec<(HashSet<String>, bool)> = Vec::new();
    let mut lead = just.clone();

    let arms = std::iter::once((&i.condition, &i.then_body))
        .chain(i.elif_clauses.iter().map(|e| (&e.condition, &e.body)));
    for (cond, body) in arms {
        let mut cond_out = fall.clone();
        taint_block(cond, &mut cond_out, &lead, sx, diags);
        let mut body_out = cond_out.clone();
        // DD#88 `if x=$(cmd) { attest x … }`: the condition taints `x` and the
        // body's first statement is immediately below it.
        let body_lead = block_taints(cond, sx);
        taint_block(body, &mut body_out, &body_lead, sx, diags);
        paths.push((body_out, block_falls_through(body)));
        // The failing path keeps everything tainted before the condition plus
        // anything the condition itself tainted — a gate that did not pass
        // vouched for nothing.
        fall.extend(cond_out);
        // Only the leading condition is adjacent to the preceding statement.
        lead = JustTainted::new();
    }

    match &i.else_body {
        Some(eb) => {
            let mut else_out = fall.clone();
            taint_block(eb, &mut else_out, &JustTainted::new(), sx, diags);
            paths.push((else_out, block_falls_through(eb)));
        }
        None => paths.push((fall, true)),
    }

    if let Some(c) = condition_attest(&i.condition) {
        check_caught_resolution(
            &attest_clear_names(c),
            span,
            &paths,
            i.else_body.is_some(),
            diags,
        );
    }

    merge_paths(&paths, tainted);
}

/// DD#121: the merge of a branching construct is the union of the out-states of
/// the paths that reach it. A path that diverts (`exit` / `return` / `break` /
/// `continue`) contributes nothing, because its state never arrives.
fn merge_paths(paths: &[(HashSet<String>, bool)], tainted: &mut HashSet<String>) {
    let mut merged: Option<HashSet<String>> = None;
    for (out, _) in paths.iter().filter(|(_, f)| *f) {
        match merged {
            None => merged = Some(out.clone()),
            Some(ref mut m) => m.extend(out.iter().cloned()),
        }
    }
    if let Some(m) = merged {
        *tainted = m;
    }
}

/// DD#121 R2: does this block reach the merge?
///
/// Deliberately conservative — anything it cannot prove diverts is treated as
/// falling through, which rejects rather than admits.
fn block_falls_through(stmts: &[Statement]) -> bool {
    match stmts.last().map(|s| &s.kind) {
        Some(StmtKind::Command(c)) => !matches!(
            expr_literal_str(&c.name),
            "exit" | "return" | "break" | "continue"
        ),
        _ => true,
    }
}

/// DD#121: the caught attest at the head of an `if` condition, if the condition
/// is exactly that one command. A multi-statement condition is not a caught
/// gate — the attest in it is not the thing being branched on.
fn condition_attest(cond: &[Statement]) -> Option<&Command> {
    let [only] = cond else {
        return None;
    };
    match &only.kind {
        StmtKind::Command(c) if expr_literal_str(&c.name) == "attest" => Some(c),
        _ => None,
    }
}

/// DD#121: every name an `attest` clears, across the DD#24 scalar, DD#61
/// indexed/map and DD#93 Part D arith-convert forms. Empty for a command that
/// is not a well-formed attest.
fn attest_clear_names(c: &Command) -> Vec<&str> {
    if expr_literal_str(&c.name) != "attest" {
        return Vec::new();
    }
    let Some((form, pred)) = classify_attest(&c.args) else {
        return Vec::new();
    };
    // DD#93 Part D declassifies the source string *and* the int it binds.
    let arith_target = match (&form, &pred) {
        (AttestForm::Scalar { .. }, AttestPredicate::ArithConvert { target, .. }) => Some(*target),
        _ => None,
    };
    let mut names = match form {
        AttestForm::Scalar { var } => vec![var],
        AttestForm::Indexed { array, .. } => vec![array],
        AttestForm::Map { map, .. } => vec![map],
    };
    names.extend(arith_target);
    names
}

/// DD#121 R2: a caught attest must leave its subject clean on every path that
/// falls through to the merge.
///
/// `else :` does not qualify — it falls through and resolves nothing. The two
/// things that do: reassign the subject from something clean (`else v=0`,
/// `|| v=8080`), or leave the branch (`else exit 1`).
fn check_caught_resolution(
    names: &[&str],
    span: Span,
    paths: &[(HashSet<String>, bool)],
    has_else: bool,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(subject) = names.first() else {
        return;
    };
    if !has_else {
        diags.push(Diagnostic::error_span(
            span,
            "SB5089",
            format!(
                "caught attest of '{subject}' has no failure branch — on failure it falls \
                 through with '{subject}' still tainted. Add `else {subject}=DEFAULT`, \
                 `else exit 1`, or drop the catch and write a bare `attest` (which aborts)"
            ),
        ));
        return;
    }
    for n in names {
        if paths.iter().any(|(out, falls)| *falls && out.contains(*n)) {
            diags.push(Diagnostic::error_span(span, "SB5089", unresolved_msg(n)));
            return;
        }
    }
}

fn unresolved_msg(n: &str) -> String {
    format!(
        "caught attest of '{n}': a branch falls through to the merge with '{n}' still \
         tainted. Resolve it there (`{n}=DEFAULT`) or leave the branch \
         (`exit` / `return` / `break` / `continue`)"
    )
}

/// DD#121 R2 for `&&` / `||`. `&&` has no failure branch at all, so it can
/// never resolve the subject; `||`'s right operand is the failure branch and is
/// held to the same rule as an `else`.
fn check_list_caught_resolution(
    l: &List,
    span: Span,
    right_out: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    let StmtKind::Command(c) = &l.left.kind else {
        return;
    };
    let names = attest_clear_names(c);
    let Some(subject) = names.first() else {
        return;
    };
    match l.op {
        ListOp::And => diags.push(Diagnostic::error_span(
            span,
            "SB5089",
            format!(
                "caught attest of '{subject}': `&&` has no failure branch, so on failure it \
                 falls through with '{subject}' still tainted. Use \
                 `attest … || {subject}=DEFAULT`, an `if attest` with an `else`, or a bare \
                 `attest` followed by the command"
            ),
        )),
        ListOp::Or => {
            let falls = block_falls_through(std::slice::from_ref(l.right.as_ref()));
            if falls && names.iter().any(|n| right_out.contains(*n)) {
                diags.push(Diagnostic::error_span(
                    span,
                    "SB5089",
                    unresolved_msg(subject),
                ));
            }
        }
        ListOp::Background => {}
    }
}

/// DD#121: a `while` condition and body both run 0..n times, so the entry state
/// is driven to a fixpoint and the merge unions the pre-loop set back in (the
/// zero-trip path).
fn taint_while(w: &While, tainted: &mut HashSet<String>, sx: &SlotCx, diags: &mut Vec<Diagnostic>) {
    let mut entry = tainted.clone();
    loop_entry_fixpoint(&[&w.condition, &w.body], &mut entry, sx);
    let mut out = entry.clone();
    taint_block(&w.condition, &mut out, &JustTainted::new(), sx, diags);
    // `while readline v { attest v … }`: the condition is the statement
    // immediately above the body, so what it taints is adjacent to the body's
    // first statement.
    let lead = block_taints(&w.condition, sx);
    taint_block(&w.body, &mut out, &lead, sx, diags);
    tainted.extend(out);
}

/// The union of what every statement in a block taints — the `just` set a
/// nested body inherits from the condition that heads it.
fn block_taints(stmts: &[Statement], cx: &SlotCx) -> JustTainted {
    let mut out = JustTainted::new();
    for s in stmts {
        out.extend(stmt_taints(s, cx));
    }
    out
}

/// DD#121: `&&` / `||`. The right operand runs on only one outcome of the left,
/// so it is walked against a clone and unioned at the merge. For `||` the left
/// is a *failing* gate, so the right starts from the pre-attest state.
fn taint_list(
    l: &List,
    span: Span,
    tainted: &mut HashSet<String>,
    just: &JustTainted,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    let before = tainted.clone();
    taint_block(
        std::slice::from_ref(l.left.as_ref()),
        tainted,
        just,
        sx,
        diags,
    );
    // `&&` runs the right on success (the attest's removal stands); `||` runs
    // it on failure (the subject is still tainted).
    let mut right = if matches!(l.op, ListOp::Or) {
        let mut s = before.clone();
        s.extend(tainted.iter().cloned());
        s
    } else {
        tainted.clone()
    };
    taint_block(
        std::slice::from_ref(l.right.as_ref()),
        &mut right,
        &JustTainted::new(),
        sx,
        diags,
    );
    check_list_caught_resolution(l, span, &right, diags);
    tainted.extend(right);
}

/// DD#60/DD#61: track the taint of a for-loop variable.
///
/// - A direct tainted **source** splat (`for x in $(argv)`) binds each
///   element to the loop var *from the source* — assignment-from-source, so
///   the loop var is tainted and a per-iteration body `attest x` is valid
///   (DD#60 Q5). Seeding the taint here makes an unattested body use a clean
///   SB5020 instead of an SSA-backstop ICE.
/// - Iterating a stored tainted **array** (`for x in @a` where `a` is
///   tainted) is a read of a stored variable — DD#61 rejects it; the array
///   must be attest-all'd first. Emitted as SB5020 directing to attest-all.
/// - A clean source rebinds the loop var to clean values (it is removed
///   from the tainted set, e.g. `for x in a b c`).
fn taint_for(f: &For, tainted: &mut HashSet<String>, sx: &SlotCx, diags: &mut Vec<Diagnostic>) {
    check_for_words(&f.words, tainted, diags);
    // The binder is rebound by the loop, so it takes the in-source's taint and
    // loses whatever it carried before.
    let from_source = f.words.iter().any(is_taint_source);
    if from_source {
        tainted.insert(f.variable.clone());
    } else {
        tainted.remove(&f.variable);
    }
    // DD#121: the body runs 0..n times, so its entry state is a fixpoint and
    // the merge unions the pre-loop set back in (the zero-trip path). And
    // `for x in $(argv 1) { attest x … }`: the in-source taints the binder, so
    // the body's first statement is adjacent to it.
    let mut entry = tainted.clone();
    loop_entry_fixpoint(&[&f.body], &mut entry, sx);
    let mut out = entry.clone();
    let mut lead = JustTainted::new();
    if from_source {
        lead.insert(f.variable.clone());
    }
    taint_block(&f.body, &mut out, &lead, sx, diags);
    tainted.extend(out);
}

/// The iterated words of a `for`: a stored tainted array is a rejected read,
/// and `for k in $(keys m)` reads inside the substitution too.
fn check_for_words(words: &[Expr], tainted: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    for w in words {
        reject_tainted_array_iter(w, tainted, diags);
        if let ExprKind::CommandSubstitution(stmts) = &w.kind {
            check_cmdsub_use_taint(stmts, w.span, tainted, diags);
        }
    }
}

/// DD#61: `for x in @a` where `a` is a stored tainted array is a read of a
/// stored variable — rejected; the array must be attest-all'd first.
fn reject_tainted_array_iter(w: &Expr, tainted: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    let Some(arr) = splat_array_name(w) else {
        return;
    };
    if tainted.contains(arr) {
        diags.push(Diagnostic::error_span(
            w.span,
            "SB5020",
            format!(
                "iterating tainted array '{arr}' without attest — attest-all \
                 it first: `attest x in {arr} [[ … ]]`"
            ),
        ));
    }
}

/// The named array of a value-splat for-word (`@a`), if the word is one.
/// Key iteration is spelled `$(keys m)` (DD#75 §3) and is classified by
/// `detect_array_iter`'s command-substitution arm, not by an operator.
fn splat_array_name(w: &Expr) -> Option<&str> {
    if let ExprKind::ParameterExpansion(pe) = &w.kind {
        if pe.operator.as_deref() == Some("arrvals") {
            return Some(&pe.name);
        }
    }
    None
}

/// Apply taint rules to an assignment.
///
/// DD#61: a tainted value may not enter an array through program syntax
/// (array/map literal, `+=` append of a literal, or `a[i]=` subscript
/// assign). Such introductions are rejected (SB5021), not quarantined —
/// tainted arrays come only from bulk external reads (read -a / mapfile).
/// Scalar assignment from a tainted source follows the normal flow:
/// the scalar becomes tainted.
/// A declaration's initializers taint exactly as the equivalent plain
/// assignments do; a bare declaration binds no value and taints nothing.
fn taint_declaration(
    d: &Declaration,
    tainted: &mut HashSet<String>,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    for a in &d.assignments {
        if let Some(eq) = a.as_assignment() {
            taint_assignment(&eq, tainted, sx, diags);
        }
    }
}

fn taint_assignment(
    a: &Assignment,
    tainted: &mut HashSet<String>,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    // DD#24 lists `arr[$x]=v` as a use-site of `x`: the index is read to
    // decide which cell to write, whichever assignment form follows.
    if let Some(idx) = &a.index {
        check_expr_taint(idx, tainted, diags);
    }
    if taint_array_literal_assignment(a, tainted, sx, diags) {
        return;
    }
    // `a[i]=value` — subscript assignment into an array slot.
    if a.index.is_some() {
        reject_tainted_into_array(&a.value, &a.name, sx.is_map(&a.name), tainted, diags);
        return;
    }
    taint_scalar_assignment(a, tainted, diags);
}

/// The scalar (or string `+=`) case of an assignment.
///
/// DD#24 (journal 036 "No propagation"): there is NO taint flow inside the
/// program. Reading a stored tainted variable in the RHS is a rejected read
/// (`x=$1` is a compile error), uniform with every other read site. The
/// sanctioned way to obtain external data is assignment *from a source*
/// (`z=$(getenv X)`), which taints the target instead of being rejected.
fn taint_scalar_assignment(
    a: &Assignment,
    tainted: &mut HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    let from_source = is_taint_source(&a.value);
    if from_source {
        tainted.insert(a.name.clone());
    }
    reject_scalar_rhs_read(&a.value, tainted, diags);
    // DD#60/DD#78: an `$(arg N DEFAULT)` default (a read position) anywhere in
    // the RHS is checked for a tainted read — the surrounding source assignment
    // is allowed, so this walk finds the nested default the value walk skips.
    check_arg_default_taint(&a.value, tainted, diags);
    // DD#121: a whole-variable assignment from a clean RHS KILLS the taint —
    // the name now holds a value that came from nowhere external. This is what
    // makes DD#24's sanctioned fallback (`attest v [[ … ]] || v=8080`) leave a
    // clean, usable value on both paths; the old flat walk got the same answer
    // by accident, because `attest` removed the name unconditionally.
    //
    // `+=` appends to whatever is already there, and an indexed write replaces
    // one cell of a container. Neither kills; the latter returned above.
    if a.op == AssignOp::Set && !from_source {
        tainted.remove(&a.name);
    }
}

/// DD#24: reject every read of a stored tainted variable in a scalar assignment
/// RHS (`z=$y`, `z="$y"`, `z=${y}`, `z=${x:-$y}`) — no propagation. A *source*
/// command substitution in value position (`z=$(getenv X)`, `z="$(getenv X)b"`)
/// is the sanctioned assignment-from-source and is NOT rejected here; a source
/// or tainted read in a `:-` *default* (a read position, DD#78) is.
fn reject_scalar_rhs_read(expr: &Expr, tainted: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    match &expr.kind {
        ExprKind::SimpleExpansion(var) => push_taint_use(
            tainted,
            var,
            expr.span,
            format!(
                "tainted variable ${var} can't be copied — DD#24 has no taint \
                 propagation; `attest {var} [[ … ]]` at the source, then read it"
            ),
            diags,
        ),
        // `${y}` / `${x:-$y}` — the name read, plus (DD#78) the `:-` default as a
        // read position (rejects a tainted var or a tainted source there).
        ExprKind::ParameterExpansion(pe) => {
            check_param_expansion_taint(pe, expr.span, tainted, diags)
        }
        // DD#24: the container AND the index are both reads — `arr[$x]` is
        // listed as a use-site of `x`, position-uniformly.
        ExprKind::Subscript { name, index } => {
            push_taint_use(
                tainted,
                name,
                expr.span,
                format!(
                    "tainted array ${{{name}[…]}} can't be copied — DD#24 has no \
                     taint propagation; attest it first"
                ),
                diags,
            );
            // A read position, not a value position — so it is checked with
            // the read walker. `reject_scalar_rhs_read` is the *value* walker:
            // it treats a bare `$(getenv X)` as sanctioned assignment-from-
            // source, which is right for the RHS itself and wrong for an
            // index. That left `z=${b[$(getenv N)]}` clean while the same
            // expression as a command argument was SB5020.
            check_expr_taint(index, tainted, diags);
        }
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) => {
            for p in parts {
                reject_scalar_rhs_read(p, tainted, diags);
            }
        }
        // A command substitution in value position is assignment-from-source:
        // a bare source ($(getenv X)/$(arg N)) is the sanctioned way to obtain
        // external data (it taints the target; handled by `is_taint_source`).
        // But a *read* of already-tainted data inside it (`$(join @a)`,
        // `$(echo $t)`) is still a use-site (DD#24) — check the inner reads.
        ExprKind::CommandSubstitution(stmts) => check_cmdsub_inner_reads(stmts, tainted, diags),
        // `int n=$(( ${x:-0} ))` — an arithmetic RHS is a copy like any other.
        // Missing here, the read was not rejected and the *target* silently
        // became tainted instead, so `attest n [[ … ]]` then declassified a
        // value that was never vouched for: laundering with a receipt. The
        // arithmetic disposition converts (DD#77); only `attest s (( n = s ))`
        // converts *and* declassifies (DD#93 Part D).
        ExprKind::Arithmetic(a) => {
            crate::ir::walk_arith_expansion_roots(a, &mut |e| {
                reject_scalar_rhs_read(e, tainted, diags)
            });
        }
        // Nothing to reject, for four different reasons:
        //   `Literal`/`SingleQuoted` — no expansion, so no read.
        //   `Array`/`AssocArray` — never reach here;
        //     `taint_array_literal_assignment` handles them (SB5021) and
        //     returns before this call.
        //   `SlurpFile` — `x=$(<file)` in value position IS
        //     assignment-from-source, exactly like `$(cat file)`, so
        //     `is_taint_source` taints the target rather than rejecting it.
        //   `Test` — not a value-producing RHS form; reads inside a test are
        //     checked by `check_test_taint` where tests actually occur.
        // Grouped rather than split one-per-variant only to keep the arm count
        // under the complexity gate — the guard is the absence of a wildcard.
        ExprKind::Literal(_)
        | ExprKind::SingleQuoted(_)
        | ExprKind::Array(_)
        | ExprKind::AssocArray(_)
        | ExprKind::SlurpFile(_)
        | ExprKind::Test(_) => {}
    }
}

/// DD#61: an array/assoc-array literal RHS rejects tainted elements (external
/// data enters arrays only via read/mapfile). Returns true if `a`'s RHS was such
/// a literal (and was handled).
fn taint_array_literal_assignment(
    a: &Assignment,
    tainted: &mut HashSet<String>,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    match &a.value.kind {
        ExprKind::Array(elems) => {
            let is_map = sx.is_map(&a.name);
            reject_tainted_elements(elems.iter(), &a.name, is_map, tainted, diags);
        }
        ExprKind::AssocArray(pairs) => {
            reject_tainted_elements(pairs.iter().map(|(_k, v)| v), &a.name, true, tainted, diags);
        }
        _ => return false,
    }
    // DD#121: every element was just rejected if tainted, so a whole-container
    // literal assignment leaves nothing external behind — it kills the taint
    // exactly as a scalar assignment from a clean RHS does. `+=` appends to
    // what is already there and must not.
    if a.index.is_none() && a.op == AssignOp::Set {
        tainted.remove(&a.name);
    }
    true
}

/// Reject every tainted element of a container literal, for either shape.
fn reject_tainted_elements<'a>(
    elems: impl Iterator<Item = &'a Expr>,
    name: &str,
    is_map: bool,
    tainted: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    for e in elems {
        reject_tainted_into_array(e, name, is_map, tainted, diags);
    }
}

/// DD#78/DD#60: check the `$(arg N DEFAULT)` default (a read position) for a
/// tainted read — SB5020, like `echo $tainted`. Walks nested cmdsubs.
fn check_arg_default_taint(expr: &Expr, tainted: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    crate::ir::walk_expr_tree(expr, &mut |e| {
        if let Some(default) = arg_default(e) {
            check_expr_taint(default, tainted, diags);
        }
    });
}

/// The DEFAULT operand of a `$(arg N DEFAULT)` command substitution, if `e` is
/// one with two args.
fn arg_default(e: &Expr) -> Option<&Expr> {
    let ExprKind::CommandSubstitution(stmts) = &e.kind else {
        return None;
    };
    let Some(StmtKind::Command(c)) = stmts.first().map(|s| &s.kind) else {
        return None;
    };
    if expr_literal_str(&c.name) != "arg" {
        return None;
    }
    c.args.get(1)
}

/// DD#61: reject a tainted value being placed into a collection element.
fn reject_tainted_into_array(
    e: &Expr,
    name: &str,
    is_map: bool,
    tainted: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    if is_taint_source(e) || expr_uses_tainted(e, tainted) {
        diags.push(Diagnostic::error_span(
            e.span,
            "SB5021",
            tainted_into_collection_msg(name, is_map),
        ));
    }
    // A read of already-tainted data inside a `$( )` element (`a[i]=$(join @b)`,
    // `array a=($(join @b))`) is a use-site (SB5020) — `expr_uses_tainted`
    // above doesn't look inside command substitutions.
    check_value_cmdsub_reads(e, tainted, diags);
}

/// SB5021's advice, spelled for the kind of collection actually being filled.
///
/// DD#99 retired `mapfile`/`read -a`: an array now takes external data through
/// `read ARR`, and a map has no bulk read of its own at all — it is built from
/// an already-vouched-for array with `map m = @{ARR:DISP}` (DD#99 stage 4).
/// Naming the wrong route here sends the reader after a command that no longer
/// exists, so the message follows the declared kind.
fn tainted_into_collection_msg(name: &str, is_map: bool) -> String {
    let (kind, route) = if is_map {
        (
            "map",
            "a map is built from a vouched-for array: `read ARR`, then \
             `attest x in ARR [[ … ]]`, then `map m = @{ARR:DISP}`",
        )
    } else {
        ("array", "external data enters an array via `read ARR`")
    };
    format!(
        "tainted value cannot be placed in {kind} '{name}' — {route}; \
         or attest a scalar first and assign the clean value"
    )
}

/// Taint a function body. Positional params ($1-$9, $@) are clean within
/// a function (the caller's args are statically visible — DD#46); they are
/// never added to the tainted set, so the body simply inherits the outer
/// tainted set unchanged.
fn taint_function_body(
    f: &Function,
    tainted: &HashSet<String>,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    let mut fn_tainted = tainted.clone();
    // DD#100: a slot is the caller's collection, so it enters tainted when any
    // call site passes a tainted one. Attesting it here is the deferral Part E
    // describes — the callee, not the caller, states the gate.
    //
    // DD#121: that arrival IS the tainting operation as far as the body can
    // see, so the body's first statement is adjacent to it. Without this the
    // Part E deferral would be unwritable.
    let mut entry_just = JustTainted::new();
    for slot in tainted_slots_on_entry(&f.name, sx.info, tainted) {
        fn_tainted.insert(slot.clone());
        entry_just.insert(slot);
    }
    let inner = SlotCx {
        probing: false,
        info: sx.info,
        map_names: sx.map_names,
    };
    taint_block(&f.body, &mut fn_tainted, &entry_just, &inner, diags);
}

/// Taint a pipeline: a `read`/`mapfile` stage is exempt from tainting its
/// targets only when EVERY stage upstream of it is compiler-computed.
///
/// This used to consult `p.commands.first()` alone, which made a leading
/// builtin launder the whole pipeline: `echo ig | /bin/cat secret | read x`
/// bound the file's contents CLEAN, and the value then flowed into argv and
/// test operands unchecked. DD#24 is operational — any output that passed
/// through `execve(2)` is tainted — so the flag must accumulate over the
/// stages the data actually traversed, not describe only the source.
fn taint_pipeline(
    p: &crate::ir::Pipeline,
    tainted: &mut HashSet<String>,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    let mut upstream_external = false;
    for (i, s) in p.commands.iter().enumerate() {
        if i > 0 && !upstream_external {
            taint_stmt_no_read_taint(s, tainted, &JustTainted::new(), sx, diags);
        } else {
            taint_stmt(s, tainted, &JustTainted::new(), sx, diags);
        }
        upstream_external |= is_external_source(s);
    }
}

/// Taint a redirected statement. A redirect target is a read position, so a
/// tainted target (`echo > $t`) is checked like any other read.
fn taint_redirected(
    r: &crate::ir::Redirected,
    tainted: &mut HashSet<String>,
    just: &JustTainted,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    for rd in &r.redirects {
        check_expr_taint(&rd.target, tainted, diags);
    }
    taint_stmt(&r.body, tainted, just, sx, diags);
}

/// Taint a case statement's discriminant and each arm body.
fn taint_case_arms(
    c: &crate::ir::Case,
    tainted: &mut HashSet<String>,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    check_expr_taint(&c.value, tainted, diags);
    // DD#121: the arms are alternative paths. No arm need match, so the
    // pre-state is itself a path to the merge.
    let mut paths: Vec<(HashSet<String>, bool)> = vec![(tainted.clone(), true)];
    for item in &c.items {
        let mut out = tainted.clone();
        taint_block(&item.body, &mut out, &JustTainted::new(), sx, diags);
        paths.push((out, block_falls_through(&item.body)));
    }
    merge_paths(&paths, tainted);
}

/// Taint analysis for a command statement: read/mapfile/attest + general taint checking.
fn taint_command(
    c: &Command,
    span: Span,
    tainted: &mut HashSet<String>,
    just: &JustTainted,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    let name = expr_literal_str(&c.name);

    taint_out_params(c, sx, tainted);
    if matches!(name, "read" | "readline" | "mapfile" | "argv") && !is_value_split(name, c) {
        taint_read_target(c, tainted);
    } else if name == "attest" {
        taint_attest(c, span, tainted, just, diags);
        return; // Don't check args of attest for taint
    }

    // Check all command arguments for tainted variable use
    check_expr_taint(&c.name, tainted, diags);
    for arg in &c.args {
        check_expr_taint(arg, tainted, diags);
    }
}

/// `read VAR` / `mapfile VAR`: mark the rightmost non-flag argument
/// as the variable being assigned, so it becomes tainted.
/// DD#99: does this command split a value that is ALREADY in the program,
/// rather than reading one in from outside?
///
/// `read ARRAY` reads stdin — external, so its target is tainted by
/// definition. `read SRC ARRAY` splits `SRC`, which the program already
/// holds. The source is an ordinary read, checked like every other read by the
/// arg walk below; DD#24's "no propagation" then makes the result clean,
/// exactly as `x=$y` is clean when `y` is. Tainting the target regardless made
/// a clean split unusable and pushed the author toward a reflexive `attest` —
/// laundering by habit, which is worse than the taint it worked around.
fn is_value_split(name: &str, c: &Command) -> bool {
    name == "read" && c.args.len() == 2
}

fn taint_read_target(c: &Command, tainted: &mut HashSet<String>) {
    for arg in c.args.iter().rev() {
        let s = expr_literal_str(arg);
        if !s.starts_with('-') && !s.is_empty() {
            tainted.insert(s.to_string());
            return;
        }
    }
}

/// The names a predicate may read while still tainted: the attested
/// variable, or the attest-all binders.
fn attest_exempt_names<'a>(form: &AttestForm<'a>) -> Vec<&'a str> {
    match form {
        AttestForm::Scalar { var } => vec![var],
        AttestForm::Indexed { binder, .. } => vec![binder],
        AttestForm::Map { key, val, .. } => vec![key, val],
    }
}

/// SB5020 (DG 2026-09-07): a predicate may read the attested name(s) — they are
/// tainted by definition, that is exactly what is being vouched for — but every
/// OTHER read must be clean.
///
/// For a `[[ ]]` predicate only a boolean escapes, so an unvouched read is
/// merely influence over the gate; for the DD#93 Part D arith form the
/// predicate's *value* is bound to the clean target, so an unchecked read would
/// launder tainted data outright. Both are rejected: a declassification should
/// not turn on data the author has not vouched for.
fn check_attest_predicate_reads(
    form: &AttestForm,
    pred: &AttestPredicate,
    span: Span,
    tainted: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    let mut others = tainted.clone();
    for name in attest_exempt_names(form) {
        others.remove(name);
    }
    if others.is_empty() {
        return;
    }
    match pred {
        AttestPredicate::Test(test) => {
            walk_test_operands(test, &mut |e| check_expr_taint(e, &others, diags))
        }
        AttestPredicate::ArithConvert { value, .. } => {
            crate::ir::walk_arith_var_reads(value, &mut |name| {
                if others.contains(name) {
                    diags.push(Diagnostic::error_span(
                        span,
                        "SB5020",
                        format!("tainted variable ${name} used without attest"),
                    ));
                }
            })
        }
    }
}

/// DD#121 R1: an `attest` must immediately follow the statement that tainted
/// its subject.
///
/// Attesting a name that is already clean stays harmless — that is a no-op, not
/// a bypass. The gate fires only when the subject is tainted *and* the taint
/// came from somewhere other than the statement directly above.
fn check_adjacency(
    names: &[&str],
    span: Span,
    tainted: &HashSet<String>,
    just: &JustTainted,
    diags: &mut Vec<Diagnostic>,
) {
    for n in names {
        if tainted.contains(*n) && !just.contains(*n) {
            diags.push(Diagnostic::error_span(
                span,
                "SB5088",
                format!(
                    "attest of '{n}' is not adjacent to the operation that tainted it. \
                     Move it to immediately after the statement that taints '{n}', or read \
                     the value with ${{{n}:!}} to take it on trust"
                ),
            ));
        }
    }
}

/// `attest` taint handling for all three forms (DD#24 scalar, DD#61
/// indexed array and map). Removes taint from the attested entity if the
/// predicate validates the required name(s); otherwise emits SB5020.
///
/// - Scalar `attest v [[…]]`: predicate must reference `$v` (on the LHS).
/// - Indexed `attest x in a [[…]]`: predicate must reference the binder
///   `$x`; clears taint on `a`.
/// - Map `attest k v in m [[…]]`: predicate must reference BOTH binders
///   `$k` and `$v` (a key escaping unvalidated would be unsound); clears
///   taint on `m`.
fn taint_attest(
    c: &Command,
    span: Span,
    tainted: &mut HashSet<String>,
    just: &JustTainted,
    diags: &mut Vec<Diagnostic>,
) {
    let Some((form, pred)) = classify_attest(&c.args) else {
        return;
    };
    check_adjacency(&attest_clear_names(c), span, tainted, just, diags);
    // The attested name(s) are tainted by definition — vouching for them is the
    // point — but any OTHER tainted read in the predicate is rejected.
    check_attest_predicate_reads(&form, &pred, span, tainted, diags);
    // DD#93 Part D: the conversion IS the predicate. It validates the source by
    // construction (it can only succeed on a valid, representable i64), so
    // there is no reference check to make — and it declassifies BOTH the
    // source string and the int it binds.
    if let (AttestForm::Scalar { var }, AttestPredicate::ArithConvert { target, .. }) =
        (&form, &pred)
    {
        tainted.remove(*var);
        tainted.remove(*target);
        return;
    }
    let AttestPredicate::Test(test) = pred else {
        return;
    };
    // Each form reduces to: the binder name(s) the predicate must validate,
    // the name whose taint clears on success, and the diagnostic otherwise.
    let (binders, clear, msg): (Vec<&str>, &str, String) = match form {
        AttestForm::Scalar { var } => (
            vec![var],
            var,
            format!("attest condition must validate ${var} — condition doesn't reference the variable"),
        ),
        AttestForm::Indexed { binder, array } => (
            vec![binder],
            array,
            format!("attest-all condition must validate each element ${binder} on the left of the test"),
        ),
        AttestForm::Map { key, val, map } => (
            vec![key, val],
            map,
            format!("map attest-all must validate both the key ${key} and value ${val} on the left of a test"),
        ),
    };
    // DD#98: a `_` binder is discarded, so there is nothing to validate about
    // it — that is the whole point of writing `_` instead of a name.
    if binders
        .iter()
        .all(|b| crate::ir::is_discard_binder(b) || test_references_var(test, b))
    {
        tainted.remove(clear);
    } else {
        diags.push(Diagnostic::error_span(span, "SB5020", msg));
    }
}

/// Check if an expression is a taint source.
///
/// Recurses into string concatenation, double-quoted parts, and array /
/// map literals so an embedded `$(getenv …)` / `$(argv …)` / external
/// command substitution is detected wherever it appears (DD#61).
///
/// **Total over `ExprKind` on purpose — do not add a wildcard.** This is read
/// as a *pair* with [`expr_uses_tainted`]: `reject_tainted_into_array` asks
/// `is_taint_source(e) || expr_uses_tainted(e, tainted)`, so between them the
/// two must cover the whole tree. They did not. `expr_uses_tainted` was made
/// total after three leaks; this one kept its `_ => false`, and wherever the
/// twin descended and this did not, a *source* was invisible to both — this
/// never arrived, and the twin arrives asking a different question (its
/// `CommandSubstitution` arm deliberately answers `false`).
///
/// ```text
/// str x=""
/// array a=(q)
/// a[0]=${x:-$(</etc/passwd)}      # compiled clean — no SB5021, no backstop
/// echo "${a[0]}"                  # …and printed the file
/// ```
///
/// A tainted *variable* in that identical position (`${x:-$t}`) was SB5021
/// throughout, which is what made this a hole rather than a policy.
fn is_taint_source(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::CommandSubstitution(stmts) => cmdsub_is_taint_source(stmts),
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) | ExprKind::Array(parts) => {
            parts.iter().any(is_taint_source)
        }
        ExprKind::AssocArray(pairs) => pairs.iter().any(|(_, v)| is_taint_source(v)),
        // `$(<file)` — file contents are external input. This is the IR-level
        // twin of the `ssa_taint.rs` rule; it earns the SPANNED SB5020 at the
        // use site. Without it the value is still caught (the SSA backstop
        // fires) but only as an unspanned "SSA taint backstop" message, which
        // is a worse diagnostic for the same program.
        ExprKind::SlurpFile(_) => true,
        // DD#78: a `:-` default is part of the expansion's *value*, so a source
        // there is a source here. `!` (DD#91) vouches for the whole expansion,
        // exactly as in `pexp_uses_tainted` — these two walkers are read as a
        // pair by `reject_tainted_into_array` and must agree about what they
        // descend into.
        ExprKind::ParameterExpansion(pe) => {
            // `!` (DD#91) vouches for the read of `pe.name`'s stored value, not
            // for a *source* materialised in the default: in
            // `${x:!-$(getenv HOME)}` nobody asked to bypass the getenv. So
            // `taint_off` is deliberately not consulted here.
            pe.operand.as_deref().is_some_and(is_taint_source)
        }
        // `$(( … ))` reaches its operands through expansion leaves, like
        // [`arith_uses_tainted`]. No source can reach one today (a bare
        // `$(( $(getenv X) ))` is SB4020 at parse), but see the note above
        // about wildcards staying harmless.
        ExprKind::Arithmetic(a) => arith_is_taint_source(a),
        // A subscript's *index* is a read position, not part of the value that
        // flows in: `z=${b[$(getenv N)]}` stores `b`'s element, never the env
        // var. Answering `true` would taint `z` with data it does not receive.
        // The index read is `check_expr_taint`'s question (SB5020).
        ExprKind::Subscript { .. } => false,
        // `[[ … ]]` yields a status, never external data. Not reachable in a
        // value position today (SB4014 at parse); spelled out for totality.
        ExprKind::Test(_) => false,
        // Whether a *variable* holds tainted data is `expr_uses_tainted`'s
        // question, not this one; text is text.
        ExprKind::SimpleExpansion(_) | ExprKind::Literal(_) | ExprKind::SingleQuoted(_) => false,
    }
}

/// `$(( … ))` reaches its operands through expansion leaves; the source-side
/// twin of [`arith_uses_tainted`].
fn arith_is_taint_source(arith: &ArithExpr) -> bool {
    let mut found = false;
    crate::ir::walk_arith_expansion_roots(arith, &mut |e| {
        found = found || is_taint_source(e);
    });
    found
}

/// A command substitution is a taint source when it runs `getenv`/`argv`/
/// `arg` or an external (absolute-path) command. Unknown PATH-resolved
/// names can't be classified here (might be a user function), so default
/// clean.
/// Whether a `$(…)` body is a taint source (used for assignment/array/word
/// contexts — `is_taint_source`). Scans EVERY statement / pipeline stage, so a
/// multi-statement / pipeline body (`$(a | b)`, `$(x=1; cat f)`) is correctly
/// flagged — not just `stmts.first()`. `pub(crate)` so the transform computes a
/// matching taint flag for the captured temp (else the SSA-taint backstop ICEs).
pub(crate) fn cmdsub_is_taint_source(stmts: &[Statement]) -> bool {
    stmts.iter().any(stmt_runs_taint_source)
}

fn stmt_runs_taint_source(stmt: &Statement) -> bool {
    if let StmtKind::Command(c) = &stmt.kind {
        if command_name_is_taint_source(&c.name) {
            return true;
        }
    }
    // `structural_bodies`: the taint pass reaches a `$( … )` body through
    // `check_expr_taint`, not through the statement recursion.
    stmt.structural_bodies()
        .iter()
        .any(|body| body.iter().any(stmt_runs_taint_source))
}

fn command_name_is_taint_source(name_expr: &Expr) -> bool {
    let name = expr_literal_str(name_expr);
    // DD#23: a resolved external head is an absolute path (`/…`); intrinsics,
    // builtins and user functions keep bare names with no slash. So any name
    // containing a slash is an external exec → tainted output. `contains('/')`
    // (not `starts_with('/')`) is defense-in-depth: it flags a relative path
    // too, should one ever slip the DD#23 rejection in `resolve`.
    name == "getenv" || name == "argv" || name == "arg" || name.contains('/')
}

/// A command substitution that yields a tainted **scalar** used directly
/// at a read site (command argument, quoted interpolation) is rejected by
/// the use-site taint check — DD#24 admits only assignment-from-source or
/// a `attest` predicate. This covers `$(getenv …)`, `$(arg N)`, and
/// external `$(/abs …)`. `$(argv …)` is deliberately excluded: it is an
/// array splat governed by the splat-context check (SB5025), not a scalar.
fn cmdsub_is_scalar_taint_source(stmts: &[Statement]) -> bool {
    // A splat-spelled `argv` is an array splat (SB5025 elsewhere), not a
    // scalar source — don't double-diagnose it. `cmdsub_command_is(.., "argv")`
    // was the test and it matched the *name* on `stmts.first()`, so it also
    // swallowed the out-param fill: `$(argv 0 b; …)` was silently exempted
    // from a taint check it should get. Any other body is a scalar source iff
    // it runs an external / getenv / arg / argv anywhere (recursive scan, so a
    // multi-statement / pipeline body is covered — was `stmts.first()` only).
    if stmts.iter().any(is_argv_splat_stmt) {
        return false;
    }
    cmdsub_is_taint_source(stmts)
}

/// The `${…}` arm of [`expr_uses_tainted`], split out to keep that dispatcher
/// under the complexity gate.
fn pexp_uses_tainted(pe: &ParameterExpansion, tainted: &HashSet<String>) -> bool {
    // DD#91: the `!` modifier is a per-use taint bypass — the value is trusted
    // clean, so it does NOT propagate taint. It is a flag, not an operator, so
    // it applies to every combination (`:!`, `:!~`, `:!-W`, `:!?M`) without
    // enumerating them.
    // DD#78: a `:-` default is part of the expansion's value, so a tainted read
    // inside it taints the result too (correct propagation — without this,
    // `z=${x:-$tainted}` left `z` clean).
    //
    // `taint_off` scopes to the subject only, so it gates the name and NOT the
    // operand walk: each nested expansion carries its own flag, which is what
    // makes `${x:!-${y:!}}` the spelling that vouches for both reads.
    (!pe.taint_off && tainted.contains(pe.name.as_str()))
        || pe
            .operand
            .as_deref()
            .is_some_and(|o| expr_uses_tainted(o, tainted))
}

/// Check if an expression uses any tainted variable.
///
/// **Total over `ExprKind` on purpose — do not add a wildcard.** This is the
/// predicate behind SB5021, and it had a `_ => false` that swallowed
/// `ExprKind::Arithmetic`. The consequence was a working taint oracle:
///
/// ```text
/// str h=$(getenv SECRET)
/// array z=($(( ${h:-0} )))            # compiled clean — no SB5021, no SB5020
/// if [[ ${z[0]} == 4242 ]] { … }      # …and read the secret back out
/// ```
///
/// `z` was never entered into the tainted set, so every later check saw a
/// clean array. `echo "${z[0]}"` was caught by the SSA backstop (a bare
/// message, no code, no span); a `[[ ]]` operand is not a command argument,
/// so that route ran. A genuinely tainted array — `read arr` — is SB5020 in
/// the same position, which is what made this a hole rather than a policy.
///
/// This is the *same enum variant* journal 400 fixed in `check_expr_taint`.
/// Making one walker total does nothing for its siblings, which is the whole
/// argument for spelling every variant out here rather than trusting a
/// wildcard to stay harmless.
fn expr_uses_tainted(expr: &Expr, tainted: &HashSet<String>) -> bool {
    match &expr.kind {
        ExprKind::SimpleExpansion(var) => tainted.contains(var.as_str()),
        ExprKind::ParameterExpansion(pe) => pexp_uses_tainted(pe, tainted),
        ExprKind::Subscript { name, index } => {
            tainted.contains(name.as_str()) || expr_uses_tainted(index, tainted)
        }
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) | ExprKind::Array(parts) => {
            parts.iter().any(|p| expr_uses_tainted(p, tainted))
        }
        ExprKind::AssocArray(pairs) => pairs.iter().any(|(_, v)| expr_uses_tainted(v, tainted)),
        // A disposition *converts*; it does not declassify (DD#24 §Quarantine,
        // DD#93 Part D). `$(( ${h:-0} ))` reads `h`.
        ExprKind::Arithmetic(a) => arith_uses_tainted(a, tainted),
        // The *filename* is a read. The slurped contents are handled by
        // `is_taint_source`, which the sole caller checks first.
        ExprKind::SlurpFile(f) => expr_uses_tainted(f, tainted),
        ExprKind::Test(_) => unreachable!("{TEST_IS_STATEMENT_ONLY}"),
        // No reads by construction — text is text.
        ExprKind::Literal(_) | ExprKind::SingleQuoted(_) => false,
        // Deliberately not descended: a read inside a `$( )` is a *use site*
        // (SB5020), not a value flowing into a collection (SB5021), and the
        // caller runs `check_value_cmdsub_reads` for exactly that. Answering
        // `true` here would report the wrong diagnostic.
        ExprKind::CommandSubstitution(_) => false,
    }
}

/// `$(( … ))` reaches its reads through expansion leaves; mirrors
/// [`check_arith_taint`], which is the diagnostic-emitting twin of this.
fn arith_uses_tainted(arith: &ArithExpr, tainted: &HashSet<String>) -> bool {
    let mut found = false;
    crate::ir::walk_arith_expansion_roots(arith, &mut |e| {
        found = found || expr_uses_tainted(e, tainted);
    });
    found
}

/// `ExprKind::Test` is built only by `parse_test_command` and `parse_attest`,
/// both of which yield a *statement*. It can therefore never occupy any
/// position this predicate is asked about: a collection element
/// (`array a=( [[ -n $t ]] )` is SB4014 at parse), a disposition operand
/// (`${h:-[[ -n x ]]}` is literal text, DD#90 rule 5), a quoted/concat part,
/// a subscript index, a slurp filename, or an arithmetic expansion root.
///
/// Unreachable *by construction*, so it ICEs rather than answering silently —
/// opening a route needs a language change, which cannot happen without
/// someone editing the parser and seeing this fire in their own sweep.
/// [`check_test_taint`] is the hot, diagnostic-emitting twin, reached from
/// [`check_expr_taint`] on the `[[ … ]]` statement itself; it stays.
const TEST_IS_STATEMENT_ONLY: &str = "ICE: an ExprKind::Test reached expr_uses_tainted — \
     `[[ … ]]` became an expression without extending the SB5021 predicate";

/// Check an expression for tainted variable use, emitting diagnostics.
/// Check one expression for reads of tainted variables (DD#24).
///
/// **Total over `ExprKind` on purpose.** This match had a `_ => {}` and it
/// leaked three times: `[[ … ]]` operands, `$(<file)`, and `$(( … ))` — each
/// a silent escape rather than a compile error, and the last one a taint
/// bypass that shipped. A new variant must now be classified here explicitly,
/// even if the classification is "no reads". Do not reintroduce the wildcard.
fn check_expr_taint(expr: &Expr, tainted: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    match &expr.kind {
        ExprKind::SimpleExpansion(var) => push_taint_use(
            tainted,
            var,
            expr.span,
            format!("tainted variable ${} used without attest", var),
            diags,
        ),
        ExprKind::ParameterExpansion(pe) => {
            check_param_expansion_taint(pe, expr.span, tainted, diags)
        }
        // DD#24: `arr[$x]` is a use-site of `x` as well as of `arr`.
        ExprKind::Subscript { name, index } => {
            push_taint_use(
                tainted,
                name,
                expr.span,
                format!("tainted array ${{{}[...]}} used without attest", name),
                diags,
            );
            check_expr_taint(index, tainted, diags);
        }
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) => {
            for p in parts {
                check_expr_taint(p, tainted, diags);
            }
        }
        // DD#24: a command substitution at a read site is checked two ways:
        //   1. a bare scalar source ($(getenv …)/$(arg N)) used directly is
        //      a use of external data without attest — rejected.
        //   2. reads *inside* the substitution (e.g. `$(join @a)` where `a`
        //      is tainted) are themselves use-sites — recurse into the inner
        //      command args. Without this, `echo $(join @a)` slips through
        //      the CST pass (silent escape or SSA-backstop ICE).
        ExprKind::CommandSubstitution(stmts) => {
            check_cmdsub_use_taint(stmts, expr.span, tainted, diags)
        }
        // `[[ … ]]` test operands are read positions — a tainted variable or a
        // tainted read inside a `$( )` operand is a use without attest. These
        // were previously unchecked (the arm was missing).
        ExprKind::Test(t) => check_test_taint(t, tainted, diags),
        // `$(<file)` at a read site is a bare use of external data, exactly
        // like `$(getenv X)`. It was covered for free while the form desugared
        // to `$(cat file)` (an external capture); lowering it to a direct read
        // left that path, and the `_` fall-through below made
        // `[[ $(<f) = x ]]` silently LEGAL. Reinstated explicitly.
        ExprKind::SlurpFile(_) => diags.push(Diagnostic::error_span(
            expr.span,
            "SB5020",
            "tainted file contents `$(<…)` used without attest — assign it \
             to a variable and attest before use"
                .to_string(),
        )),
        // DD#24 §Quarantine names `${x:op}` "in any expression" as a read, and
        // `$(( … ))` is an expression. Without this arm a tainted variable was
        // laundered outright: `echo "$(( ${x:-0} ))"` compiled clean — no
        // SB5020, and no SSA backstop either, because the lone-leaf value never
        // became a tainted int. A disposition *converts*; it does not
        // declassify. DD#93 Part D is the form that does both, and it is
        // spelled `attest s (( n = s ))` precisely so the declassification
        // stays greppable. The same DD's addendum already rejects this exact
        // laundering *inside* an attest predicate ("z would be laundered into
        // clean y", SB5020) — it cannot be legal outside one.
        ExprKind::Arithmetic(a) => check_arith_taint(a, tainted, diags),
        // Nothing to check, for two different reasons. `Literal`/`SingleQuoted`
        // have no expansion at all — text is text. `Array`/`AssocArray` *are*
        // read positions, but a tainted element is rejected at the
        // *assignment* by `reject_tainted_into_array` (SB5021), which names the
        // collection and is the better diagnostic; recursing here would
        // double-report. An `${…}` subscript *index* still arrives through the
        // `Subscript` arm above. Grouped rather than split one-per-variant only
        // to keep the arm count under the complexity gate — the guard is the
        // absence of a wildcard, not the number of arms.
        ExprKind::Literal(_)
        | ExprKind::SingleQuoted(_)
        | ExprKind::Array(_)
        | ExprKind::AssocArray(_) => {}
    }
}

/// DD#24: reads inside `$(( … ))` / `(( … ))` are use-sites like any other.
///
/// A bare name in arithmetic needs no arm: a *string* one is SB4030 (DD#77
/// requires a disposition, which lands in the `Expansion` leaf this does
/// visit), and an *int* one cannot be tainted — every construction route is
/// refused first (`int n=$(getenv X)`/`$(arg N)`/`$(cmd)` are SB4030, and
/// `read` targets an array, DD#99). If a tainted int ever becomes
/// constructible, this is the function that needs the extra arm.
fn check_arith_taint(arith: &ArithExpr, tainted: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    crate::ir::walk_arith_expansion_roots(arith, &mut |e| check_expr_taint(e, tainted, diags));
}

/// Taint-check the operands of a `[[ … ]]` test — each is a read position.
///
/// Mirrors [`check_arith_taint`] exactly: the shared walker visits each
/// operand *root* and `check_expr_taint` does its own descent, so nothing is
/// reported twice. Do not reach for `ir::walk_test_exprs` here — that one
/// visits every node of every operand, which would double-report.
fn check_test_taint(t: &TestExpr, tainted: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    crate::ir::walk_test_operands(t, &mut |e| check_expr_taint(e, tainted, diags));
}

fn push_taint_use(
    tainted: &HashSet<String>,
    name: &str,
    span: Span,
    msg: String,
    diags: &mut Vec<Diagnostic>,
) {
    if tainted.contains(name) {
        diags.push(Diagnostic::error_span(span, "SB5020", msg));
    }
}

/// Taint-check a parameter expansion: the var itself, plus (DD#78) a `:-`
/// default's operand, which is a read position — a tainted read there is a use
/// without attest, like `echo $tainted`.
///
/// **No operator's operand is walked**, because none of them can be tainted:
/// every parameter-expansion operand is now a literal. The disposition
/// defaults (`:-`, `:?`) became literals on 2026-09-30 (SB4070); the pattern
/// operators always were (DD#65).
///
/// That is a property of the *parse*, not an assumption, and it is the whole
/// reason this function is short. It has been wrong once: `${var:?MESSAGE}`
/// briefly interpolated, and the missing arm turned a wrong-but-safe SB5050
/// into a live taint escape — an abort message goes to stderr, so the value
/// left the program. If any operand is ever made to interpolate again, this
/// gate must grow an arm for it **in the same commit**. See
/// `parse_disposition_literal` for why they are literal.
fn check_param_expansion_taint(
    pe: &ParameterExpansion,
    span: Span,
    tainted: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    // DD#91: the `!` modifier is a sanctioned per-use taint bypass — the
    // subject is trusted clean (a predicate-less attest), so its read is not a
    // violation. Keyed on the flag, so every combination is covered.
    if !pe.taint_off {
        push_taint_use(
            tainted,
            &pe.name,
            span,
            format!("tainted variable ${{{} ...}} used without attest", pe.name),
            diags,
        );
    }
}

fn check_cmdsub_use_taint(
    stmts: &[Statement],
    span: Span,
    tainted: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    check_bare_cmdsub_taint(stmts, span, diags);
    check_cmdsub_inner_reads(stmts, tainted, diags);
}

/// The tainted *reads* inside a command substitution's commands — collection
/// readers (`join`/`keys`) and each argument expression. This is the
/// "`_tmp=$(…)`" model: every `$( )` is checked for tainted reads the same
/// way, wherever it appears. Shared by the argument-position check (which also
/// rejects a bare scalar source via `check_bare_cmdsub_taint`) and the value
/// positions (assignment RHS, array element, redirect target), where a bare
/// source is the *sanctioned* way to obtain external data — there only the
/// inner reads are use-sites.
fn check_cmdsub_inner_reads(
    stmts: &[Statement],
    tainted: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    for s in stmts {
        if let StmtKind::Command(c) = &s.kind {
            check_collection_reader_taint(c, tainted, diags);
            for arg in &c.args {
                check_expr_taint(arg, tainted, diags);
            }
        }
    }
}

/// Check the inner reads of every command substitution within a value-position
/// expression (assignment RHS, array element, subscript value) — where a bare
/// source is sanctioned but a read of already-tainted data is still a use-site
/// (SB5020). Descends the concat/quote/array structure to reach a nested `$( )`.
fn check_value_cmdsub_reads(expr: &Expr, tainted: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    match &expr.kind {
        ExprKind::CommandSubstitution(stmts) => check_cmdsub_inner_reads(stmts, tainted, diags),
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) | ExprKind::Array(parts) => {
            for p in parts {
                check_value_cmdsub_reads(p, tainted, diags);
            }
        }
        ExprKind::AssocArray(pairs) => {
            for (_, v) in pairs {
                check_value_cmdsub_reads(v, tainted, diags);
            }
        }
        // A subscript *index* is a read position sitting inside a value
        // position, so it gets the read walker: `a[0]=${b[$(getenv N)]}`
        // stores `b`'s element, but it reads the env var to choose which.
        // `reject_scalar_rhs_read` makes the same call for the scalar form.
        ExprKind::Subscript { index, .. } => check_expr_taint(index, tainted, diags),
        // `${x:-…}` — the default is a value position (DD#78), the index is a
        // read position, and `!` vouches for neither of the *inner* reads.
        ExprKind::ParameterExpansion(pe) => pexp_value_cmdsub_reads(pe, tainted, diags),
        // `$(( … ))` reaches its operands through expansion leaves.
        ExprKind::Arithmetic(a) => arith_value_cmdsub_reads(a, tainted, diags),
        // Nothing to descend: a bare variable read is `expr_uses_tainted`'s
        // question, `$(<f)` has no inner command, `[[ … ]]` cannot appear in a
        // value position (SB4014 at parse), and text is text. Spelled out
        // rather than left to a wildcard — this walker is read as one of the
        // three that must jointly cover a value expression, and the other two
        // both leaked through exactly such a wildcard.
        ExprKind::SimpleExpansion(_)
        | ExprKind::SlurpFile(_)
        | ExprKind::Test(_)
        | ExprKind::Literal(_)
        | ExprKind::SingleQuoted(_) => {}
    }
}

/// The `${…}` arm of [`check_value_cmdsub_reads`], split out to keep that
/// dispatcher under the complexity gate — as [`pexp_uses_tainted`] is for
/// [`expr_uses_tainted`].
fn pexp_value_cmdsub_reads(
    pe: &ParameterExpansion,
    tainted: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    if let Some(idx) = pe.index.as_deref() {
        check_expr_taint(idx, tainted, diags);
    }
    if let Some(operand) = pe.operand.as_deref() {
        check_value_cmdsub_reads(operand, tainted, diags);
    }
}

/// The `$(( … ))` arm of [`check_value_cmdsub_reads`]; the read-checking
/// sibling of [`arith_uses_tainted`] and [`arith_is_taint_source`].
fn arith_value_cmdsub_reads(
    arith: &ArithExpr,
    tainted: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    crate::ir::walk_arith_expansion_roots(arith, &mut |e| {
        check_value_cmdsub_reads(e, tainted, diags);
    });
}

/// A collection reader that exposes tainted *element data* via a bare
/// argument literal: `keys m` (a map's keys) or `join a` (the joined element
/// values). Per DD#24 these are reads of tainted elements — if the collection
/// is tainted (e.g. an `argv`/`mapfile` snapshot without attest-all) this is a
/// use without attest (SB5020). The name is a bare literal, so the generic
/// `check_expr_taint` path (which only flags variable-expansion reads like
/// `@a`/`${a[i]}`) doesn't catch it. `len` is exempt — it exposes only
/// cardinality, not element data.
fn check_collection_reader_taint(
    c: &Command,
    tainted: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    let ExprKind::Literal(cmd) = &c.name.kind else {
        return;
    };
    let Some(arg) = c.args.first() else {
        return;
    };
    let ExprKind::Literal(name) = &arg.kind else {
        return;
    };
    let msg = match cmd.as_str() {
        "join" => format!(
            "join of tainted collection '{name}' without attest — attest-all \
             it first: `attest … in {name} [[ … ]]`"
        ),
        "keys" => format!(
            "keys of tainted map '{name}' without attest — attest-all \
             it first: `attest k v in {name} [[ … ]]`"
        ),
        _ => return,
    };
    push_taint_use(tainted, name, arg.span, msg, diags);
}

fn check_bare_cmdsub_taint(stmts: &[Statement], span: Span, diags: &mut Vec<Diagnostic>) {
    if cmdsub_is_scalar_taint_source(stmts) {
        diags.push(Diagnostic::error_span(
            span,
            "SB5020",
            "tainted command substitution used without attest — assign it \
             to a variable and attest before use"
                .to_string(),
        ));
    }
}

// ── SB5031: reject a scalar read of a name introduced nowhere (typo) ──
//
// DD#62 Model B (DG 2026-07-08): scalar zero values are meaningful
// (untyped → "", int → 0), so a KNOWN-but-maybe-unset read yields the zero
// value and is allowed. The only rejection is the typo: a scalar read whose
// name is introduced nowhere in scope. This is a single-pass scope-
// membership check (a "may"/union analysis), NOT definite-assignment
// dataflow — no branch intersection, no zero-trip-loop modeling.

/// Names introduced in a scope: assignment LHS, int/array/map/const
/// declaration names, `for` loop variables, and read/mapfile targets.
/// Descends control-flow bodies (same scope) but NOT nested function
/// bodies (a new scope).
fn collect_scope_names(stmts: &[Statement], set: &mut HashSet<String>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Assignment(a) => {
                set.insert(a.name.clone());
            }
            StmtKind::Declaration(d) => insert_decl_names(d, set),
            StmtKind::For(f) => {
                set.insert(f.variable.clone());
                collect_scope_names(&f.body, set);
            }
            StmtKind::Command(c) => {
                collect_read_targets(c, set);
                // DD#93 Part D: `attest SOURCE (( TARGET = SOURCE ))` introduces
                // TARGET — the attest binds it to the converted value.
                if let Some(target) = attest_convert_target(c) {
                    set.insert(target.to_string());
                }
            }
            StmtKind::Function(_) => {} // separate scope — do not collect
            _ => collect_child_scope_names(stmt, set),
        }
    }
}

fn insert_decl_names(d: &Declaration, set: &mut HashSet<String>) {
    for a in &d.assignments {
        set.insert(a.name.clone());
    }
}

fn collect_child_scope_names(stmt: &Statement, set: &mut HashSet<String>) {
    for body in stmt.child_bodies() {
        collect_scope_names(body, set);
    }
}

/// `readline x` / `read [SRC] a` introduce their last literal argument name.
/// DD#60 `argv [N] ARR` snapshots the argv (slice) into the array named by
/// its LAST argument.
fn collect_read_targets(c: &Command, set: &mut HashSet<String>) {
    let name = expr_literal_str(&c.name);
    match name {
        "read" | "readline" => insert_last_target_arg(&c.args, set),
        // DD#99: `read [SRC] ARRAY` — only the last arg is a binding.
        // DD#63: `match S P ARR` / `matchall S P ARR` fill ARR, so they
        // introduce it — the same bulk-fill precedent as argv / read. The
        // two-arg `match S P` is the boolean form and binds nothing.
        "matchall" | "match" if c.args.len() == 3 => insert_last_target_arg(&c.args, set),
        "argv" => insert_last_literal_arg(&c.args, set),
        _ => {}
    }
}

fn insert_last_target_arg(args: &[Expr], set: &mut HashSet<String>) {
    if let Some(last) = args.last() {
        insert_read_target_arg(last, set);
    }
}

fn insert_last_literal_arg(args: &[Expr], set: &mut HashSet<String>) {
    if let Some(ExprKind::Literal(s)) = args.last().map(|a| &a.kind) {
        set.insert(s.clone());
    }
}

/// A non-flag literal argument to `read`/`readline` names a variable the
/// command introduces.
fn insert_read_target_arg(arg: &Expr, set: &mut HashSet<String>) {
    if let ExprKind::Literal(s) = &arg.kind {
        if !s.starts_with('-') {
            set.insert(s.clone());
        }
    }
}

/// Names handled by other rules (their own diagnostics) — skip so the typo
/// check never double-diagnoses.
fn handled_elsewhere(name: &str) -> bool {
    ENV_VARS.contains(&name)
        || REJECTED_VARS.iter().any(|(n, _)| *n == name)
        // DD#100: `A`..`E` are bound by the caller, never assigned in scope.
        // SB5065 owns them (in-scope inside a function, rejected at top level).
        || is_collection_slot(name)
}

/// DD#62 Model B: reject a scalar read of a name introduced nowhere in
/// scope. Reads = `SimpleExpansion` and bare `ParameterExpansion` (no
/// operator — `${x:-…}` is the explicit opt-out). Subscripts are container
/// reads (out of scope); their index is still checked.
pub fn check_undeclared_reads(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let mut globals = HashSet::new();
    collect_scope_names(&program.statements, &mut globals);
    if let Some(ref end) = program.end_block {
        collect_scope_names(end, &mut globals);
    }
    // DD#58: a bare name in arithmetic may be a nullary int-function call
    // (`$((f))`), so a defined function name is a valid reference — collect
    // them so the B2 arith-read check doesn't flag them as undeclared.
    collect_functions(&program.statements, &mut globals);
    if let Some(ref end) = program.end_block {
        collect_functions(end, &mut globals);
    }
    let mut funcs = HashSet::new();
    collect_functions(&program.statements, &mut funcs);
    if let Some(ref end) = program.end_block {
        collect_functions(end, &mut funcs);
    }
    let mut sink = ReadSink {
        mode: SinkMode::Flag(&mut diags),
        funcs: &funcs,
    };
    check_reads_in_scope(&program.statements, &globals, &mut sink);
    if let Some(ref end) = program.end_block {
        check_reads_in_scope(end, &globals, &mut sink);
    }
    diags
}

/// DD#100: which collection slots a function touches. A slot is bound by the
/// caller, so `=foo` at a call site is a read of `foo` exactly when the callee
/// reads that slot, and a write exactly when it writes it. Without this the
/// dead-variable gate (SB5050) saw only the `=foo` token and reported every
/// collection argument as never-read — which made Part A's own example
/// uncompilable.
#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) struct SlotUse {
    reads: [bool; MAX_COLLECTION_SLOTS],
    writes: [bool; MAX_COLLECTION_SLOTS],
}

impl SlotUse {
    /// How many collection arguments a call must supply: one past the highest
    /// slot the function touches.
    fn arity(&self) -> usize {
        (0..MAX_COLLECTION_SLOTS)
            .rev()
            .find(|&i| self.reads[i] || self.writes[i])
            .map_or(0, |i| i + 1)
    }
}

/// `A`..`E` → 0..4.
fn slot_index(name: &str) -> Option<usize> {
    is_collection_slot(name).then(|| (name.as_bytes()[0] - b'A') as usize)
}

/// Every function body in the program, keyed by name.
fn function_bodies(program: &Program) -> Vec<(&str, &[Statement])> {
    let mut out = Vec::new();
    for block in program_blocks(program) {
        push_function_bodies(block, &mut out);
    }
    out
}

fn push_function_bodies<'a>(stmts: &'a [Statement], out: &mut Vec<(&'a str, &'a [Statement])>) {
    for stmt in stmts {
        if let StmtKind::Function(f) = &stmt.kind {
            out.push((f.name.as_str(), f.body.as_slice()));
        }
        for body in stmt.child_bodies() {
            push_function_bodies(body, out);
        }
    }
}

/// Slot usage per function, including slots forwarded through a call
/// (`fn g { f =A }` makes `g` a user of `A` iff `f` uses its first slot).
/// Recursion is rejected (SB5033), so the fixpoint converges.
pub(crate) fn slot_usage_map(program: &Program) -> HashMap<String, SlotUse> {
    let bodies = function_bodies(program);
    let mut map: HashMap<String, SlotUse> = bodies
        .iter()
        .map(|(n, b)| (n.to_string(), direct_slot_use(b)))
        .collect();
    for _ in 0..bodies.len() {
        if !slot_use_round(&bodies, &mut map) {
            break;
        }
    }
    map
}

/// One forwarding round: fold each body's forwarded slots into its own usage.
fn slot_use_round(bodies: &[(&str, &[Statement])], map: &mut HashMap<String, SlotUse>) -> bool {
    let mut changed = false;
    for (name, body) in bodies {
        let mut u = map[*name];
        propagate_slot_calls(body, map, &mut u);
        if u != map[*name] {
            map.insert(name.to_string(), u);
            changed = true;
        }
    }
    changed
}

/// The slots a body names directly, reusing the same read/write enumerators
/// the dead-variable gate uses so the two can never disagree.
fn direct_slot_use(body: &[Statement]) -> SlotUse {
    let mut reads = HashSet::new();
    collect_reads(body, &mut reads);
    let mut writes = Vec::new();
    collect_writes(body, &mut writes);
    let mut u = SlotUse::default();
    for (i, flag) in u.reads.iter_mut().enumerate() {
        *flag = reads.contains(&collection_slot_name(i));
    }
    for (i, flag) in u.writes.iter_mut().enumerate() {
        let s = collection_slot_name(i);
        *flag = writes.iter().any(|(n, _)| *n == s);
    }
    u
}

/// Fold in the slots this body forwards to other functions.
fn propagate_slot_calls(stmts: &[Statement], map: &HashMap<String, SlotUse>, u: &mut SlotUse) {
    for stmt in stmts {
        if let StmtKind::Command(c) = &stmt.kind {
            fold_forwarded_slots(c, map, u);
        }
        for body in stmt.child_bodies() {
            propagate_slot_calls(body, map, u);
        }
    }
}

fn fold_forwarded_slots(c: &Command, map: &HashMap<String, SlotUse>, u: &mut SlotUse) {
    let Some(callee) = map.get(expr_literal_str(&c.name)) else {
        return;
    };
    for (j, passed) in collection_args(&c.args).into_iter().enumerate() {
        let (Some(here), true) = (slot_index(&passed), j < MAX_COLLECTION_SLOTS) else {
            continue;
        };
        u.reads[here] |= callee.reads[j];
        u.writes[here] |= callee.writes[j];
    }
}

// ── SB5050: reject a variable assigned but never read (dead variable) ──
//
// DD#86: the dual of SB5031. Every variable must appear in both a write and a
// read. A **global** (a top-level introduction) is dead if it is read nowhere
// in the program (a global may be read from any function); a **function-local**
// is dead if it is read nowhere within its function. This is the safety net
// for a misspelled write target (a stray local stranded on the write side).
//
// A `for` loop variable and a `read`/`mapfile` target are ordinary
// writes and ARE checked (DG 2026-09-02): an unused loop variable or a
// discarded read field is dead. Reads are recognised by the SB5031 enumerator
// (`ReadSink::Collect`) plus the value-fn bare operands, so the check never
// false-positives; the escape hatch for a deliberately-unused write is to
// read it with `: $v` (an established acknowledgment, DD#81 arity).

/// SB5050 (DD#86): reject an assigned-but-never-read variable.
pub fn check_dead_variables(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let usage = slot_usage_map(program);
    check_dead_globals(program, &usage, &mut diags);
    check_dead_locals(program, &usage, &mut diags);
    diags
}

/// DD#100: `f =foo` is a read of `foo` exactly when `f` reads the slot it
/// lands in. The token itself is not a read (Part E — it passes identity, not
/// values), so without this the gate reported every collection argument as
/// never-read. Writes are deliberately NOT added: a collection must be
/// declared before it can be passed, and the declaration is already a write,
/// so an out-parameter that nobody reads still fires — which is the behaviour
/// Part E specifies.
fn collect_slot_arg_reads(
    stmts: &[Statement],
    usage: &HashMap<String, SlotUse>,
    reads: &mut HashSet<String>,
) {
    for stmt in stmts {
        if let StmtKind::Command(c) = &stmt.kind {
            push_slot_arg_reads(c, usage, reads);
        }
        // DD#100: a call inside `$( … )` is still a call, so `s=$(f =q)` is a
        // read of `q` on exactly the same terms. `child_bodies()` yields a
        // substitution body, so the ordinary recursion reaches it.
        for body in stmt.child_bodies() {
            collect_slot_arg_reads(body, usage, reads);
        }
    }
}

fn push_slot_arg_reads(c: &Command, usage: &HashMap<String, SlotUse>, reads: &mut HashSet<String>) {
    let Some(callee) = usage.get(expr_literal_str(&c.name)) else {
        return;
    };
    for (j, passed) in collection_args(&c.args).into_iter().enumerate() {
        if j < MAX_COLLECTION_SLOTS && callee.reads[j] {
            reads.insert(passed);
        }
    }
}

/// A top-level (global) write is dead iff it is read nowhere in the whole
/// program (a global may be read from any function).
fn check_dead_globals(
    program: &Program,
    usage: &HashMap<String, SlotUse>,
    diags: &mut Vec<Diagnostic>,
) {
    let mut reads = HashSet::new();
    let mut writes = Vec::new();
    for block in program_blocks(program) {
        collect_reads(block, &mut reads);
        collect_slot_arg_reads(block, usage, &mut reads);
        collect_writes(block, &mut writes);
    }
    report_dead(&writes, |n| !reads.contains(n), diags);
}

/// Each function-local write (a name that is not a top-level global) is dead
/// iff it is read nowhere within that function.
fn check_dead_locals(
    program: &Program,
    usage: &HashMap<String, SlotUse>,
    diags: &mut Vec<Diagnostic>,
) {
    let mut globals = HashSet::new();
    for block in program_blocks(program) {
        collect_scope_names(block, &mut globals);
    }
    for block in program_blocks(program) {
        check_dead_in_functions(block, &globals, usage, diags);
    }
}

/// The top-level statement list plus the `__END__` block, as separate scopes.
fn program_blocks(program: &Program) -> Vec<&[Statement]> {
    std::iter::once(program.statements.as_slice())
        .chain(program.end_block.as_deref())
        .collect()
}

/// Emit SB5050 for each write whose name `is_dead`, de-duplicating by name.
fn report_dead(
    writes: &[(String, Span)],
    is_dead: impl Fn(&str) -> bool,
    diags: &mut Vec<Diagnostic>,
) {
    let mut reported: HashSet<String> = HashSet::new();
    for (name, span) in writes {
        if exempt_from_dead_check(name) {
            continue;
        }
        if is_dead(name) && reported.insert(name.clone()) {
            diags.push(dead_var_diag(*span, name));
        }
    }
}

/// Names SB5050 must not claim are dead.
///
/// `_` (DD#98) is bound in order to be discarded — being unread is the point.
/// The reserved `args` is immutable, so ANY write to it is SB5052 whether or
/// not it is read; claiming "dead variable" first gives the author the wrong
/// reason and implies that adding a read would fix it. This pass runs before
/// the transform's, so it has to yield.
fn exempt_from_dead_check(name: &str) -> bool {
    crate::ir::is_discard_binder(name)
        || name == "args"
        // DD#100: a write through a collection slot lands on the CALLER's
        // collection, so whether it is dead is decided in the caller's scope
        // where the real name lives. Filling a slot is the out-parameter form
        // Part E exists for, and the callee never reads it back.
        || is_collection_slot(name)
}

/// Find each function body and flag its dead function-locals.
fn check_dead_in_functions(
    stmts: &[Statement],
    globals: &HashSet<String>,
    usage: &HashMap<String, SlotUse>,
    diags: &mut Vec<Diagnostic>,
) {
    for stmt in stmts {
        if let StmtKind::Function(f) = &stmt.kind {
            check_fn_dead(&f.body, globals, usage, diags);
        }
        for body in stmt.child_bodies() {
            check_dead_in_functions(body, globals, usage, diags);
        }
    }
}

/// A write in a function to a non-global name is dead iff it is read nowhere in
/// the function body.
fn check_fn_dead(
    body: &[Statement],
    globals: &HashSet<String>,
    usage: &HashMap<String, SlotUse>,
    diags: &mut Vec<Diagnostic>,
) {
    let mut fn_reads = HashSet::new();
    collect_reads(body, &mut fn_reads);
    collect_slot_arg_reads(body, usage, &mut fn_reads);
    let mut writes = Vec::new();
    collect_writes(body, &mut writes);
    report_dead(
        &writes,
        |n| !globals.contains(n) && !fn_reads.contains(n),
        diags,
    );
}

/// Collect `(write-name, span)` for each assignment / typed declaration /
/// read-target / `for` variable in `stmts`. Descends control-flow bodies (same
/// scope) but NOT nested function bodies (each its own scope).
fn collect_writes(stmts: &[Statement], out: &mut Vec<(String, Span)>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Assignment(a) => out.push((a.name.clone(), stmt.span)),
            StmtKind::Declaration(d) => push_decl_writes(d, stmt.span, out),
            StmtKind::Command(c) => push_read_target_writes(c, stmt.span, out),
            StmtKind::For(f) => {
                // DD#86 (DG 2026-09-02): a `for` variable is an ordinary
                // variable — an unused loop variable is dead.
                out.push((f.variable.clone(), stmt.span));
                collect_writes(&f.body, out);
            }
            StmtKind::Function(_) => {} // separate scope
            _ => {
                for body in stmt.child_bodies() {
                    collect_writes(body, out);
                }
            }
        }
    }
}

fn push_decl_writes(d: &Declaration, span: Span, out: &mut Vec<(String, Span)>) {
    for a in &d.assignments {
        out.push((a.name.clone(), span));
    }
}

fn push_read_target_writes(c: &Command, span: Span, out: &mut Vec<(String, Span)>) {
    let mut targets = HashSet::new();
    collect_read_targets(c, &mut targets);
    for n in targets {
        out.push((n, span));
    }
}

/// Collect every variable *read* in `stmts` into `set`, reusing the exact
/// SB5031 read-enumerator (`check_reads_in_scope` in `Collect` mode) so
/// dead-var and SB5031 can never disagree on what a read is. `known` is
/// irrelevant in collect mode (a scope-agnostic superset is what we want),
/// so an empty set is passed.
///
/// The enumerator does not recognise the bare *collection-name* operand of a
/// value builtin (`len a` / `join a` / `keys m`) — those are the asymmetric
/// case tracked in `notes/tasks/bare-collection-name-operand-asymmetry.md` —
/// so `collect_value_fn_operands` adds them.
fn collect_reads(stmts: &[Statement], set: &mut HashSet<String>) {
    let empty = HashSet::new();
    let no_funcs = HashSet::new();
    let mut sink = ReadSink {
        mode: SinkMode::Collect(set),
        funcs: &no_funcs,
    };
    check_reads_in_scope(stmts, &empty, &mut sink);
    collect_value_fn_operands(stmts, set);
}

/// A bare collection-name operand to `len`/`join`/`keys` is a read of that
/// collection (`len a` reads `a`). Record it for every command, including those
/// inside a `$( )` capture. Reuses `len_operand_name`/`join_operand_name`
/// (which also unwrap the `@a` splat form).
fn collect_value_fn_operands(stmts: &[Statement], set: &mut HashSet<String>) {
    for stmt in stmts {
        if let StmtKind::Command(c) = &stmt.kind {
            record_value_fn_operand(c, set);
        }
        walk_all_exprs(stmt, &mut |e| record_cmdsub_value_fn_operands(e, set));
    }
}

/// Record the value-fn operand of every command inside a `$( )` expression.
fn record_cmdsub_value_fn_operands(e: &Expr, set: &mut HashSet<String>) {
    let ExprKind::CommandSubstitution(inner) = &e.kind else {
        return;
    };
    for s in inner {
        if let StmtKind::Command(c) = &s.kind {
            record_value_fn_operand(c, set);
        }
    }
}

fn record_value_fn_operand(c: &Command, set: &mut HashSet<String>) {
    let name = expr_literal_str(&c.name);
    // DD#102: `strlen` is absent here because its operand is a value, not a
    // name — `strlen s` reads no variable `s`, and `strlen $s` is an ordinary
    // expansion the generic scan already records.
    if name == "len" {
        record_len_operand(c, set);
    } else if name == "join" || name == "keys" {
        record_collection_operand(c, set);
    }
}

/// `len X` reads the bare-name operand `X` (a collection or a string).
fn record_len_operand(c: &Command, set: &mut HashSet<String>) {
    if let Some(n) = len_operand_name(c) {
        set.insert(n);
    }
}

/// `join X` / `keys X` read the bare collection name `X` (or `@X` splat).
fn record_collection_operand(c: &Command, set: &mut HashSet<String>) {
    let n = c
        .args
        .first()
        .map(crate::type_check::join_operand_name)
        .filter(|s| !s.is_empty());
    if let Some(n) = n {
        set.insert(n);
    }
}

fn dead_var_diag(span: Span, name: &str) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5050",
        format!(
            "variable `{name}` is assigned but never read (dead variable) — every variable \
             must be read somewhere (DD#86). Remove the assignment, or read `{name}` \
             (a stray write is usually a misspelled target)"
        ),
    )
}

// ── SB5039: reject `len <undefined>` (DD#75 §2) ──────────────────────
//
// `len <name>` dispatches on the operand's type; a name introduced nowhere
// reads a non-existent slot and silently yields 0. Reject it as a typo. The
// valid operand names are the scope introductions (`collect_scope_names`:
// declarations incl. `array`/`map`, assignments, `for` variables, `read`/
// `mapfile` targets) plus the implicit `args` array inside a function body.
// This reuses the SB5031 scope machinery, so a loop variable (`for e in @a; do
// $(len e); done`) is correctly accepted — the gap that made a type_check-side
// reject unsafe. Only a bare-identifier operand is checked; a subscript cell
// (`len a[0]`), a `$`-expansion, or a `$(…)` operand resolve elsewhere.

/// DD#75 §2: reject `len <name>` whose name is introduced nowhere in scope.
pub fn check_len_operands(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let mut globals = HashSet::new();
    collect_scope_names(&program.statements, &mut globals);
    if let Some(ref end) = program.end_block {
        collect_scope_names(end, &mut globals);
    }
    check_len_in_scope(&program.statements, &globals, false, &mut diags);
    if let Some(ref end) = program.end_block {
        check_len_in_scope(end, &globals, false, &mut diags);
    }
    diags
}

/// `$(join …)` form: arity (SB5040) and a literal separator (SB5074).
///
/// Arity: a collection and an optional separator. `$(join)` (0 args)
/// previously reached the transform and **panicked the compiler**
/// (`ICE: join requires at least one argument`); more than 2 args silently
/// dropped the extras. Rejecting both here guarantees the transform's arity.
///
/// Separator: the SSA operand is a compile-time `String`, so a non-literal
/// separator has nothing to lower to — it was being rendered as *source text*
/// (`$(join a "$sep")` joined with the four characters `"$sep"`). Rejecting
/// it here guarantees `ir::literal_word_text` succeeds in the transform.
pub fn check_join_form(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for stmt in program
        .statements
        .iter()
        .chain(program.end_block.iter().flatten())
    {
        crate::ir::walk_all_exprs(stmt, &mut |e| {
            flag_join_arity(e, &mut diags);
            flag_join_separator(e, &mut diags);
        });
    }
    diags
}

/// SB5074: `$(join arr SEP)` where `SEP` is not a compile-time literal.
fn flag_join_separator(expr: &Expr, diags: &mut Vec<Diagnostic>) {
    let Some(c) = join_call(expr) else {
        return;
    };
    let Some(sep) = c.args.get(1) else {
        return;
    };
    if crate::ir::literal_word_text(sep).is_some() {
        return;
    }
    diags.push(Diagnostic::error_span(
        sep.span,
        "SB5074",
        "`join`'s separator is fixed at compile time, so it must be a literal \
         — an expansion cannot be used here. Write the separator directly, \
         e.g. `$(join a \",\")`"
            .to_string(),
    ));
}

/// The `join` command inside a `$( )`, if this expression is one.
fn join_call(expr: &Expr) -> Option<&Command> {
    let ExprKind::CommandSubstitution(stmts) = &expr.kind else {
        return None;
    };
    let StmtKind::Command(c) = stmts.first().map(|s| &s.kind)? else {
        return None;
    };
    (expr_literal_str(&c.name) == "join").then_some(c)
}

fn flag_join_arity(expr: &Expr, diags: &mut Vec<Diagnostic>) {
    let Some(c) = join_call(expr) else {
        return;
    };
    if c.args.is_empty() || c.args.len() > 2 {
        diags.push(Diagnostic::error_span(
            expr.span,
            "SB5040",
            format!(
                "`join` takes a collection and an optional separator \
                 (`$(join arr)` or `$(join arr SEP)`) — got {} argument(s)",
                c.args.len()
            ),
        ));
    }
}

/// How a `len` / `join` / `keys` operand is spelled. These builtins resolve
/// their operand as a variable **name**, not a value, so only a plain
/// identifier carries meaning.
enum OperandForm {
    /// A bare identifier: `a`
    Bare,
    /// A `$`-sigil expansion of an identifier: `$a`
    Dollar(String),
    /// An arrvals splat: `@a`
    Splat(String),
    /// Not a name — a quoted string, a subscript cell, a `$( )`, arithmetic, …
    Other,
}

fn classify_operand(expr: &Expr) -> OperandForm {
    match &expr.kind {
        ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrvals") => {
            OperandForm::Splat(pe.name.clone())
        }
        ExprKind::Literal(s) if is_plain_identifier(s) => OperandForm::Bare,
        ExprKind::SimpleExpansion(s) if is_plain_identifier(s) => OperandForm::Dollar(s.clone()),
        _ => OperandForm::Other,
    }
}

/// SB5064: a collection read through the scalar `$name` sigil.
///
/// `$a` for a collection `a` is an error (DG 2026-09-11). It was neither
/// rejected nor meaningful: in every value context it silently expanded to
/// **nothing** (`echo $a`, `str x=$a`, `echo $m` all produced empty), while
/// the `len` operand slot resolved the same spelling as the collection
/// *name* — `len $a` on a three-element array returned `3`. Two different
/// wrong answers for one spelling, neither of them an error.
///
/// This is the dual of `bug-scalar-subscript-segfault` (`${s[0]}` on a
/// scalar): both are the expansion form disagreeing with the variable's
/// type. Use `@a` for the values, `${a[i]}` for one element, or
/// `$(len a)` / `$(join a SEP)`.
pub fn check_collection_scalar_read(program: &Program) -> Vec<Diagnostic> {
    let mut collections = HashSet::new();
    let mut auto = HashSet::new();
    let mut scalars = HashSet::new();
    for block in program_blocks(program) {
        collect_collection_names(block, &mut collections);
        collect_auto_collection_targets(block, &mut auto);
        collect_scalar_bindings(block, &mut scalars);
    }
    // An auto-declared collection counts, unless the same name is also bound
    // as a scalar — that program is a DD#86 type change, and type-checking
    // reports it against the offending `read`/`argv` rather than against an
    // earlier read that is correct as written.
    collections.extend(auto.into_iter().filter(|n| !scalars.contains(n)));
    let mut diags = Vec::new();
    for stmt in program
        .statements
        .iter()
        .chain(program.end_block.iter().flatten())
    {
        crate::ir::walk_all_exprs(stmt, &mut |e| {
            flag_collection_scalar_read(e, &collections, &mut diags);
        });
    }
    diags
}

/// SB5068: a subscript whose subject is not a collection.
///
/// The exact dual of SB5064 (`$a` on a collection): there the sigil was scalar
/// and the variable a collection; here the subject is a scalar and the
/// expansion a container read. `${s[0]}` was neither supported nor rejected —
/// it lowered to a table read, so the generated code dereferenced a scalar's
/// `char*` as a `SashTable*`. One type mistake, seven measured outcomes: a
/// **segfault** for a string subject in each of six spellings (read, dynamic
/// index, `[[ ]]` operand, arithmetic operand, loop variable, subscript
/// assignment) and a **compiler ICE** for an `int` one.
///
/// A name introduced nowhere is left to SB5031 — an undefined variable is a
/// different mistake with its own diagnostic.
pub fn check_scalar_subscript(program: &Program) -> Vec<Diagnostic> {
    let mut subscriptable = HashSet::new();
    let mut introduced = HashSet::new();
    for block in program_scopes(program) {
        collect_subscriptable_names(block, &mut subscriptable);
        collect_scope_names(block, &mut introduced);
    }
    let mut diags = Vec::new();
    for block in program_scopes(program) {
        crate::ir::walk_stmt_tree(block, &mut |stmt| {
            flag_scalar_subscripts(stmt, &subscriptable, &introduced, &mut diags);
        });
    }
    diags
}

/// The top-level statement blocks, plus every function body — so a
/// program-wide union covers names introduced inside functions too. Sound
/// because sash forbids shadowing: one name, one meaning, program-wide.
fn program_scopes(program: &Program) -> Vec<&[Statement]> {
    let mut work: Vec<&[Statement]> = std::iter::once(program.statements.as_slice())
        .chain(program.end_block.as_deref())
        .collect();
    let mut blocks = work.clone();
    while let Some(block) = work.pop() {
        work.extend(block.iter().flat_map(|s| s.child_bodies()));
        blocks.extend(block.iter().filter_map(function_body));
    }
    blocks
}

fn function_body(stmt: &Statement) -> Option<&[Statement]> {
    match &stmt.kind {
        StmtKind::Function(f) => Some(&f.body),
        _ => None,
    }
}

/// Names a subscript may legitimately address: `-a`/`-A` declarations, plus the
/// bulk-fill targets (`read`/`mapfile`/`matchall`/`argv`), which introduce a
/// collection with no declaration at all — `argv g` then `${g[1]}` is valid.
/// `readline v` fills a *scalar* but is collected too; over-approximating here
/// costs a missed diagnostic, never a rejected working program.
fn collect_subscriptable_names(stmts: &[Statement], set: &mut HashSet<String>) {
    crate::ir::walk_stmt_tree(stmts, &mut |stmt| {
        insert_collection_decl(&stmt.kind, set);
        if let StmtKind::Command(c) = &stmt.kind {
            collect_read_targets(c, set);
        }
    });
}

fn flag_scalar_subscripts(
    stmt: &Statement,
    subscriptable: &HashSet<String>,
    introduced: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    let mut subjects = Vec::new();
    subscript_subjects(stmt, &mut subjects);
    diags.extend(
        subjects
            .iter()
            .filter(|(n, _)| subscript_subject_is_scalar(n, subscriptable, introduced))
            .map(|(n, span)| scalar_subscript_diag(n, *span)),
    );
}

/// Every subscript *subject* in a statement, with the span to report it at.
/// Three walk sites, because the faces are not all reachable from one:
/// expressions (which carry `[[ ]]` operands and quoted parts), the assignment
/// lvalue (`s[0]=x` — the subject is a statement field, present in no
/// expression), and arithmetic (`ArithExpr::Subscript`, which
/// `walk_arith_var_reads` deliberately skips).
fn subscript_subjects(stmt: &Statement, out: &mut Vec<(String, Span)>) {
    match &stmt.kind {
        StmtKind::Assignment(a) if a.index.is_some() => out.push((a.name.clone(), stmt.span)),
        StmtKind::ArithTest(arith) => push_arith_subjects(arith, stmt.span, out),
        _ => {}
    }
    stmt.direct_exprs(&mut |e| crate::ir::walk_expr_tree(e, &mut |x| push_expr_subject(x, out)));
}

fn push_expr_subject(expr: &Expr, out: &mut Vec<(String, Span)>) {
    match &expr.kind {
        ExprKind::Subscript { name, .. } => out.push((name.clone(), expr.span)),
        ExprKind::Arithmetic(a) => push_arith_subjects(a, expr.span, out),
        _ => {}
    }
}

fn push_arith_subjects(a: &crate::ir::ArithExpr, span: Span, out: &mut Vec<(String, Span)>) {
    crate::ir::walk_arith_subscripts(a, &mut |n| out.push((n.to_string(), span)));
}

/// `args` and the DD#100 slots `A`..`E` are collections with no declaration in
/// scope; an unintroduced name belongs to SB5031, not here.
fn subscript_subject_is_scalar(
    name: &str,
    subscriptable: &HashSet<String>,
    introduced: &HashSet<String>,
) -> bool {
    introduced.contains(name)
        && !subscriptable.contains(name)
        && !is_collection_slot(name)
        && name != "args"
}

fn scalar_subscript_diag(name: &str, span: Span) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5068",
        format!(
            "`{name}` is a scalar, so `{name}[…]` has no element to address — \
             declare it with `array {name}` / `map {name}` to subscript it, or \
             drop the subscript to use its value"
        ),
    )
}

/// SB5079: a splat whose subject is not a collection.
///
/// The same type mistake as SB5068, one operation over. `@name` lowers to a
/// collection traversal, so a scalar subject makes the generated code read a
/// `char*` as a `SashTable*`. Measured faces: a **segfault** for a string
/// subject in each of three spellings (`for x in @s`, `strlen @s`, and a splat
/// argument to a user function) and a **compiler ICE** for an `int` one.
///
/// Unlike the subscript case, an unintroduced name is **not** already covered.
/// SB5031 enumerates scalar *reads*, and a splat is not one, so `@nosuch`
/// reached codegen and ICE'd there too. It is rejected here under SB5031's own
/// code, matching `splat_not_subscriptable_diag`, which already reports a
/// splat misuse that way.
pub fn check_scalar_splat(program: &Program) -> Vec<Diagnostic> {
    let mut splattable = HashSet::new();
    let mut introduced = HashSet::new();
    for block in program_scopes(program) {
        collect_subscriptable_names(block, &mut splattable);
        collect_scope_names(block, &mut introduced);
    }
    let mut diags = Vec::new();
    for block in program_scopes(program) {
        crate::ir::walk_stmt_tree(block, &mut |stmt| {
            stmt.direct_exprs(&mut |e| {
                crate::ir::walk_expr_tree(e, &mut |x| {
                    flag_scalar_splat(x, &splattable, &introduced, &mut diags);
                });
            });
        });
    }
    diags
}

fn flag_scalar_splat(
    expr: &Expr,
    splattable: &HashSet<String>,
    introduced: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(name) = splat_subject(expr) else {
        return;
    };
    if subscript_subject_is_scalar(name, splattable, introduced) {
        diags.push(scalar_splat_diag(name, expr.span));
    }
}

/// The variable a `@name` splat traverses, in any of its dispositions (`@a`,
/// `@{a:-d}`, `@{a:?m}`). `@`/`*` are the positional splat — a different
/// construct, with its own rules.
fn splat_subject(expr: &Expr) -> Option<&str> {
    let ExprKind::ParameterExpansion(pe) = &expr.kind else {
        return None;
    };
    is_splat_expansion(pe).then_some(pe.name.as_str())
}

/// Is this expansion an `@name` splat (in any disposition)? `@`/`*` are the
/// positional splat — a different construct, with its own rules.
fn is_splat_expansion(pe: &ParameterExpansion) -> bool {
    pe.operator
        .as_deref()
        .is_some_and(|op| op.starts_with("arrvals"))
        && pe.name != "@"
        && pe.name != "*"
}

fn scalar_splat_diag(name: &str, span: Span) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5079",
        format!(
            "`{name}` is a scalar, so `@{name}` has nothing to splat — declare \
             it with `array {name}` / `map {name}` to splat it, or write \
             `${name}` to use its value"
        ),
    )
}

fn undefined_splat_diag(name: &str, span: Span) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5031",
        format!(
            "`@{name}` splats `{name}`, which is introduced nowhere — declare \
             it with `array {name}` / `map {name}`, or correct the spelling"
        ),
    )
}

/// Every `-a`/`-A` declaration name in the program, including inside function
/// bodies and control-flow bodies.
fn collect_collection_names(stmts: &[Statement], set: &mut HashSet<String>) {
    for stmt in stmts {
        insert_collection_decl(&stmt.kind, set);
        for body in stmt.child_bodies() {
            collect_collection_names(body, set);
        }
    }
}

/// The collection a builtin **auto-declares**, for the callers that need to
/// know a name is a collection rather than that it is merely introduced.
///
/// `type_check.rs` registers exactly these into `env.collections`
/// (`register_collection_target`) per DD#60/DD#61/DD#63, but
/// [`collect_collection_names`] walks only `-a`/`-A` declarations. Two
/// collection sets that disagreed, and SB5064 — the guard written for `$a` on
/// a collection — consulted the incomplete one. `argv x` followed by
/// `/bin/echo $x` therefore passed a `SashTable*` to `execve` as a C string,
/// which printed a live heap address (`sash_argv_guard` is handed the scalar
/// length 0, so it validated nothing).
///
/// Deliberately *narrower* than [`collect_read_targets`], which answers "what
/// names does this command introduce" and so includes `readline` — that one
/// binds a **scalar**, and listing it here would make `$x` after `readline x`
/// a spurious SB5064.
fn collect_auto_collection_targets(stmts: &[Statement], set: &mut HashSet<String>) {
    for stmt in stmts {
        insert_auto_collection_target(&stmt.kind, set);
        for body in stmt.child_bodies() {
            collect_auto_collection_targets(body, set);
        }
    }
}

fn insert_auto_collection_target(kind: &StmtKind, set: &mut HashSet<String>) {
    let StmtKind::Command(c) = kind else {
        return;
    };
    match expr_literal_str(&c.name) {
        // DD#99: `read [SRC] ARRAY` — the target is always the last arg.
        "read" => insert_last_target_arg(&c.args, set),
        // DD#63: `match S P ARR` / `matchall S P ARR` fill the trailing array.
        // Two-arg `match S P` is the boolean form and binds nothing.
        "match" | "matchall" if c.args.len() == 3 => insert_last_target_arg(&c.args, set),
        // DD#60: `argv [N] ARR` snapshots into the array named last.
        "argv" => insert_last_literal_arg(&c.args, set),
        _ => {}
    }
}

/// Names bound as **scalars** — a non-collection declaration, a bare
/// assignment, or a `for` header (DD#86: all three fix the name's type at its
/// first introduction).
///
/// Used only to keep an auto-declared collection from retroactively turning an
/// earlier, legitimate scalar read into SB5064. `x=safe; /bin/echo $x; read x`
/// is rejected either way — it rebinds `x`'s type, which DD#86 forbids — but
/// the fault is the `read`, and blaming line 2 sends the reader to a line that
/// is correct as written. Type-checking owns that message
/// (`register_collection_target`); this only declines to preempt it with a
/// worse one.
///
/// An explicit `-a`/`-A` declaration is deliberately NOT filtered this way:
/// there the keyword is authoritative, so `array a=(1); a=x; echo $a` must
/// keep reporting SB5064 as it always has.
fn collect_scalar_bindings(stmts: &[Statement], set: &mut HashSet<String>) {
    for stmt in stmts {
        insert_scalar_binding(&stmt.kind, set);
        for body in stmt.child_bodies() {
            collect_scalar_bindings(body, set);
        }
    }
}

fn insert_scalar_binding(kind: &StmtKind, set: &mut HashSet<String>) {
    match kind {
        StmtKind::Declaration(d) if !is_collection_flags(&d.flags) => {
            for a in &d.assignments {
                set.insert(a.name.clone());
            }
        }
        // `a[i]=v` writes an element, so it binds nothing.
        StmtKind::Assignment(a) if a.index.is_none() => {
            set.insert(a.name.clone());
        }
        StmtKind::For(f) => {
            set.insert(f.variable.clone());
        }
        // DD#99: `readline` binds a scalar; `read` does not.
        StmtKind::Command(c) if expr_literal_str(&c.name) == "readline" => {
            insert_last_target_arg(&c.args, set);
        }
        _ => {}
    }
}

fn insert_collection_decl(kind: &StmtKind, set: &mut HashSet<String>) {
    let StmtKind::Declaration(d) = kind else {
        return;
    };
    if !is_collection_flags(&d.flags) {
        return;
    }
    for a in &d.assignments {
        set.insert(a.name.clone());
    }
}

fn is_collection_flags(flags: &[String]) -> bool {
    flags.iter().any(|f| f == "-a" || f == "-A")
}

fn flag_collection_scalar_read(
    expr: &Expr,
    collections: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    match &expr.kind {
        ExprKind::SimpleExpansion(name) if collections.contains(name) => {
            diags.push(collection_scalar_read_diag(name, expr.span));
        }
        ExprKind::Arithmetic(a) => {
            flag_collection_arith_args(a, expr.span, collections, diags);
        }
        _ => {}
    }
}

/// SB5064 applies inside a DD#58 arith sexpr operand too: `(strlen $a)` reads
/// the collection `a` through the scalar sigil exactly as `$(strlen $a)` does.
/// `walk_all_exprs` reaches the `Arithmetic` node but not the `ArithExpr` tree
/// inside it, so the sexpr operand slot was the one place left where `$a` on a
/// collection was neither rejected nor meaningful — `(strlen $a)` silently
/// answered `0`, and `(f $a)` on a user int function ran the call with a
/// missing operand. Ordinary arithmetic already rejects a collection operand
/// (SB4030); only the call-argument slot escaped.
fn flag_collection_arith_args(
    a: &ArithExpr,
    span: Span,
    collections: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    walk_arith_funccall_args(a, &mut |arg| {
        if let ArithExpr::DollarVar(name) = arg {
            if collections.contains(name) {
                diags.push(collection_scalar_read_diag(name, span));
            }
        }
    });
}

/// Invoke `f(arg)` for every argument of every `FuncCall` in an arith tree.
/// Shape comes from `ArithExpr::children()`, so a new variant cannot silently
/// fall out of this traversal.
fn walk_arith_funccall_args(a: &ArithExpr, f: &mut impl FnMut(&ArithExpr)) {
    if let ArithExpr::FuncCall { args, .. } = a {
        for arg in args {
            f(arg);
        }
    }
    for child in a.children() {
        walk_arith_funccall_args(child, f);
    }
}

fn collection_scalar_read_diag(name: &str, span: Span) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5064",
        format!(
            "`${name}` reads the collection `{name}` with the scalar sigil, which \
             is neither the collection nor element 0 — use `@{name}` for its values, \
             `${{{name}[i]}}` for one element, or `$(len {name})` / `$(join {name} SEP)`"
        ),
    )
}

/// SB5066: the call-site rules for collection arguments (DD#100).
///
/// - **Part C — scalars first.** A scalar after a collection is rejected, so
///   source position equals `args` index and `$1` is the first word as in bash.
/// - **Part B — at most five.** A sixth collection argument has no slot.
/// - **The name must be a collection.** `=s` on a scalar, or on a name that is
///   introduced nowhere, has nothing to bind.
/// - **Enough collections for the callee.** A slot the body uses but the call
///   never fills read as an empty collection: `$(len A)` silently returned 0.
///
/// The sigil is only meaningful at a **user-function** call. `echo =a` stays a
/// plain word printing `=a` (spec `scope.sh` 86.21–86.24), and builtins take
/// bare names (Part D).
pub fn check_collection_args(program: &Program) -> Vec<Diagnostic> {
    let mut funcs = HashSet::new();
    let mut colls = HashSet::new();
    for block in program_blocks(program) {
        collect_functions(block, &mut funcs);
        collect_collection_names(block, &mut colls);
        collect_bulk_targets(block, &mut colls);
    }
    let usage = slot_usage_map(program);
    let mut diags = Vec::new();
    let cx = CallArgCx {
        funcs: &funcs,
        colls: &colls,
        usage: &usage,
    };
    for block in program_blocks(program) {
        check_call_args_in(block, &cx, &mut diags);
    }
    diags
}

struct CallArgCx<'a> {
    funcs: &'a HashSet<String>,
    colls: &'a HashSet<String>,
    usage: &'a HashMap<String, SlotUse>,
}

/// Bulk-fill targets (`read a`, `argv N a`, `mapfile a`, `matchall … a`)
/// introduce a collection without an `array` keyword, so they count as
/// declarations for "is this name a collection".
fn collect_bulk_targets(stmts: &[Statement], set: &mut HashSet<String>) {
    for stmt in stmts {
        if let StmtKind::Command(c) = &stmt.kind {
            collect_read_targets(c, set);
        }
        for body in stmt.child_bodies() {
            collect_bulk_targets(body, set);
        }
    }
}

fn check_call_args_in(stmts: &[Statement], cx: &CallArgCx, diags: &mut Vec<Diagnostic>) {
    for stmt in stmts {
        if let StmtKind::Command(c) = &stmt.kind {
            check_one_call_args(c, stmt.span, cx, diags);
        }
        // DD#100: a call inside `$( … )` is still a call — `child_bodies()`
        // yields a substitution body, so the ordinary recursion reaches it.
        for body in stmt.child_bodies() {
            check_call_args_in(body, cx, diags);
        }
    }
}

fn check_one_call_args(c: &Command, span: Span, cx: &CallArgCx, diags: &mut Vec<Diagnostic>) {
    let fname = expr_literal_str(&c.name);
    if !cx.funcs.contains(fname) {
        return; // not a user-function call — `=a` is an ordinary word
    }
    check_scalars_first(c, span, diags);
    let passed = collection_args(&c.args);
    check_slot_count(fname, passed.len(), span, diags);
    for name in &passed {
        check_slot_operand(name, span, cx, diags);
    }
    check_slot_arity(fname, passed.len(), span, cx, diags);
}

/// Part C: every collection argument comes after every scalar one.
fn check_scalars_first(c: &Command, span: Span, diags: &mut Vec<Diagnostic>) {
    let mut seen_collection = None;
    for arg in &c.args {
        match collection_arg_name(arg) {
            Some(n) => seen_collection = Some(n),
            None => {
                if let Some(first) = &seen_collection {
                    diags.push(Diagnostic::error_span(
                        span,
                        "SB5066",
                        format!(
                            "a scalar argument follows the collection argument \
                             `={first}` — list the scalars first (DD#100 Part C), so \
                             that a call's source position is its `args` index"
                        ),
                    ));
                    return;
                }
            }
        }
    }
}

fn check_slot_count(fname: &str, passed: usize, span: Span, diags: &mut Vec<Diagnostic>) {
    if passed <= MAX_COLLECTION_SLOTS {
        return;
    }
    diags.push(Diagnostic::error_span(
        span,
        "SB5066",
        format!(
            "`{fname}` is passed {passed} collections, but a function has only \
             {MAX_COLLECTION_SLOTS} slots (`A`..`E`)"
        ),
    ));
}

/// The passed name must be a collection: a declared/filled one, or a slot the
/// enclosing function itself received (`fn g { f =A }`).
fn check_slot_operand(name: &str, span: Span, cx: &CallArgCx, diags: &mut Vec<Diagnostic>) {
    if cx.colls.contains(name) || is_collection_slot(name) {
        return;
    }
    diags.push(Diagnostic::error_span(
        span,
        "SB5066",
        format!(
            "`={name}` passes a collection, but `{name}` is not one — declare it \
             `array {name}=(…)` / `map {name}=(…)`, or pass a scalar as an \
             ordinary argument"
        ),
    ));
}

/// A slot the callee uses must actually be filled — an unbound slot read as an
/// empty collection, so `$(len A)` silently returned 0.
fn check_slot_arity(
    fname: &str,
    passed: usize,
    span: Span,
    cx: &CallArgCx,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(u) = cx.usage.get(fname) else { return };
    let needed = u.arity();
    if passed >= needed {
        return;
    }
    diags.push(Diagnostic::error_span(
        span,
        "SB5066",
        format!(
            "`{fname}` uses collection slot `{}` but this call passes {passed} \
             collection{} — pass it with `=name`",
            collection_slot_name(needed - 1),
            if passed == 1 { "" } else { "s" }
        ),
    ));
}

// ── DD#100: slot taint, in both directions ────────────────────────
//
// A slot **is** the caller's collection (Part E), so taint has to cross the
// call boundary both ways:
//
//   inward   `argv a; f =a`      — reading `${A[0]}` inside `f` reads tainted
//                                  data, and must need an attest.
//   outward  `fn fill { argv A }` — `fill =a` leaves the *caller's* `a`
//                                  tainted; DG chose to allow this (a useful
//                                  out-parameter) and track it, rather than
//                                  ban it as SB5067 bans the global case.
//
// Both are resolved structurally, before the walk, so no extra pass over the
// taint state is needed:
//
//   `bindings`  slot → the caller names ever passed to it. A body read of `A`
//               is tainted if *any* bound name is. Conservative by union: a
//               function called once with a tainted array and once with a
//               clean one is checked as tainted. That matches the strict
//               slot-*kind* rule, where all call sites must agree too.
//   `taints`    the slots a body fills from outside. At a call site those
//               names become tainted, exactly as `argv a` would.
//
// Both close over forwarding (`fn g { f =A }`) with a fixpoint on the call
// graph. The sets only grow, so it terminates.
#[derive(Default)]
pub(crate) struct SlotInfo {
    bindings: HashMap<(String, usize), Vec<String>>,
    taints: HashSet<(String, usize)>,
}

/// The static facts carried through the taint walk. A forwarded slot argument
/// (`f =A` inside `g`) needs no enclosing-function context here: the binding
/// fixpoint already resolved it, and marking the slot name itself tainted is
/// what the enclosing body reads. `map_names` is separate static knowledge —
/// SB5021 needs the declared kind of a collection to name a route that exists.
struct SlotCx<'a> {
    info: &'a SlotInfo,
    map_names: &'a HashSet<String>,
    /// Set while `stmt_taints` is measuring one statement's taint effect. It
    /// suppresses the nested measurement, which would otherwise double the
    /// walk at every level of nesting.
    probing: bool,
}

impl SlotCx<'_> {
    /// Whether `name` was declared as a map (`map m` / `declare -A m`).
    fn is_map(&self, name: &str) -> bool {
        self.map_names.contains(name)
    }
}

impl SlotInfo {
    /// The caller names bound to slot `j` of `fname`.
    fn bound(&self, fname: &str, j: usize) -> &[String] {
        self.bindings
            .get(&(fname.to_string(), j))
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    fn taints_slot(&self, fname: &str, j: usize) -> bool {
        self.taints.contains(&(fname.to_string(), j))
    }
}

/// The names declared as maps anywhere in the program (DD#61 `map m`).
fn collect_map_names(program: &Program) -> HashSet<String> {
    let mut kinds = HashMap::new();
    for block in program_blocks(program) {
        crate::type_check::collect_collection_kinds(block, &mut kinds);
    }
    kinds
        .into_iter()
        .filter(|(_, is_map)| *is_map)
        .map(|(name, _)| name)
        .collect()
}

pub(crate) fn analyze_slots(program: &Program) -> SlotInfo {
    let mut info = SlotInfo::default();
    let mut calls = Vec::new();
    for block in program_blocks(program) {
        collect_slot_calls(block, &mut calls);
        collect_slot_fills(block, &mut info.taints);
    }
    while slot_fixpoint_step(&calls, &mut info) {}
    info
}

/// One fixpoint round: a forwarded argument binds through, and an out-param
/// taint propagates back through the forwarding call.
fn slot_fixpoint_step(calls: &[SlotCall], info: &mut SlotInfo) -> bool {
    let mut changed = false;
    for call in calls {
        changed |= apply_call_bindings(call, info);
        changed |= apply_call_taints(call, info);
    }
    changed
}

/// One `f =x =y` call site, tagged with the function it appears in.
struct SlotCall {
    callee: String,
    caller: String,
    args: Vec<String>,
}

fn collect_slot_calls(stmts: &[Statement], out: &mut Vec<SlotCall>) {
    crate::ir::walk_scoped_stmts(stmts, "", &mut |stmt, in_fn| {
        let StmtKind::Command(c) = &stmt.kind else {
            return;
        };
        let args = collection_args(&c.args);
        if !args.is_empty() {
            out.push(SlotCall {
                callee: expr_literal_str(&c.name).to_string(),
                caller: in_fn.to_string(),
                args,
            });
        }
    });
}

/// The slots a body fills from outside — the same bulk-fill set SB5067 uses
/// for globals, but here recorded rather than rejected.
fn collect_slot_fills(stmts: &[Statement], out: &mut HashSet<(String, usize)>) {
    crate::ir::walk_scoped_stmts(stmts, "", &mut |stmt, in_fn| {
        if let (StmtKind::Command(c), false) = (&stmt.kind, in_fn.is_empty()) {
            record_slot_fill(c, in_fn, out);
        }
    });
}

fn record_slot_fill(c: &Command, in_fn: &str, out: &mut HashSet<(String, usize)>) {
    for t in bulk_fill_targets(c) {
        if let Some(j) = slot_index(&t) {
            out.insert((in_fn.to_string(), j));
        }
    }
}

/// Bind each argument to the callee's slot. A forwarded slot (`f =A` inside
/// `g`) binds through to everything `g`'s own slot holds.
fn apply_call_bindings(call: &SlotCall, info: &mut SlotInfo) -> bool {
    let mut changed = false;
    for (j, arg) in call.args.iter().enumerate() {
        let names = binding_sources(call, arg, info);
        changed |= extend_binding(info, &call.callee, j, names);
    }
    changed
}

/// The caller-side names an argument stands for: a forwarded slot resolves to
/// whatever the forwarding function's own slot already holds.
fn binding_sources(call: &SlotCall, arg: &str, info: &SlotInfo) -> Vec<String> {
    match slot_index(arg) {
        Some(k) if !call.caller.is_empty() => info.bound(&call.caller, k).to_vec(),
        _ => vec![arg.to_string()],
    }
}

fn extend_binding(info: &mut SlotInfo, callee: &str, j: usize, names: Vec<String>) -> bool {
    let slot = info.bindings.entry((callee.to_string(), j)).or_default();
    let mut changed = false;
    for n in names {
        if !slot.contains(&n) {
            slot.push(n);
            changed = true;
        }
    }
    changed
}

/// If the callee fills slot `j` from outside, the forwarding caller's own slot
/// is filled too, so the taint reaches its caller in turn.
fn apply_call_taints(call: &SlotCall, info: &mut SlotInfo) -> bool {
    let mut changed = false;
    for (j, arg) in call.args.iter().enumerate() {
        if !info.taints_slot(&call.callee, j) || call.caller.is_empty() {
            continue;
        }
        if let Some(k) = slot_index(arg) {
            changed |= info.taints.insert((call.caller.clone(), k));
        }
    }
    changed
}

/// Inward: the slot names that are tainted on entry to `fname`.
fn tainted_slots_on_entry(fname: &str, info: &SlotInfo, tainted: &HashSet<String>) -> Vec<String> {
    let mut out = Vec::new();
    for j in 0..MAX_COLLECTION_SLOTS {
        if info.bound(fname, j).iter().any(|n| tainted.contains(n)) {
            out.push(collection_slot_name(j));
        }
    }
    out
}

/// Outward: a call to a function that fills a slot from outside taints the
/// caller's collection, exactly as a bulk fill on the name would.
fn taint_out_params(c: &Command, cx: &SlotCx, tainted: &mut HashSet<String>) {
    let callee = expr_literal_str(&c.name);
    for (j, arg) in collection_args(&c.args).iter().enumerate() {
        if cx.info.taints_slot(callee, j) {
            tainted.insert(arg.clone());
        }
    }
}

// ── SB5067: a function may not taint a global ─────────────────────
//
// The taint pass walks a function body **once**, with the tainted set known
// at that point. That was sound only because taint could not arrive later —
// and it could:
//
//     array g=()
//     fn taint_it { argv g }
//     fn user { for v in @g; do echo "$v"; done }
//     taint_it
//     user                 # printed unattested argv
//
// `g[0]=$(arg 1)` was already SB5021 ("tainted value cannot be placed in
// array 'g'"); the **bulk-fill** path simply escaped the same rule. Closing
// it makes the top level the only place a global can become tainted, which is
// what lets `check_taint` finish the whole top level before it walks any body
// — no fixpoint, and no conservative rejection of a call that ran before the
// filler.
//
// The escape hatch is the ordinary one: fill a local, attest it, assign the
// clean value out.
//
//     fn f { array t=(); argv t; attest v in t [[ … ]]; g[0]=${t[1]} }
//
// A **slot** (`A`..`E`) is exempt: it is the caller's collection, passed
// deliberately, and DD#100 lets a callee fill one as an out-parameter. That
// taint propagates back to the caller (`slot_taint_summary`), so it is
// tracked rather than banned.
pub fn check_fn_taints_global(program: &Program) -> Vec<Diagnostic> {
    let mut globals = HashSet::new();
    for block in program_blocks(program) {
        collect_global_names(block, &mut globals);
    }
    let mut diags = Vec::new();
    for block in program_blocks(program) {
        walk_fns_for_global_taint(block, &globals, &mut diags);
    }
    diags
}

/// Names declared at the top level — everything a function must not taint.
/// Descends into top-level control flow but **not** into function bodies,
/// whose declarations are local.
/// The names bound at the top level. Structural on purpose: a name first
/// introduced inside a `$( … )` body is local to that body (DD#86), so it is
/// not a global and must not join this set — otherwise an unrelated function's
/// own local of the same spelling is reported as tainting a global.
fn collect_global_names(stmts: &[Statement], out: &mut HashSet<String>) {
    crate::ir::walk_scoped_structural_stmts(stmts, "", &mut |stmt, in_fn| {
        if in_fn.is_empty() {
            record_global_name(stmt, out);
        }
    });
}

fn record_global_name(stmt: &Statement, out: &mut HashSet<String>) {
    match &stmt.kind {
        StmtKind::Declaration(d) => {
            for a in &d.assignments {
                out.insert(a.name.clone());
            }
        }
        StmtKind::Assignment(a) => {
            out.insert(a.name.clone());
        }
        StmtKind::Command(c) => collect_read_targets(c, out),
        _ => {}
    }
}

fn walk_fns_for_global_taint(
    stmts: &[Statement],
    globals: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    for stmt in stmts {
        if let StmtKind::Function(f) = &stmt.kind {
            check_body_global_taint(&f.body, &f.name, globals, diags);
        }
        for body in stmt.child_bodies() {
            walk_fns_for_global_taint(body, globals, diags);
        }
    }
}

fn check_body_global_taint(
    body: &[Statement],
    fname: &str,
    globals: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    crate::ir::walk_stmt_tree(body, &mut |stmt| {
        check_stmt_global_taint(stmt, fname, globals, diags);
    });
}

/// Each initializer of a declaration, checked as the plain assignment it is.
fn decl_global_taint(
    d: &Declaration,
    span: Span,
    fname: &str,
    globals: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    for a in &d.assignments {
        if let Some(eq) = a.as_assignment() {
            check_assign_global_taint(&eq, span, fname, globals, diags);
        }
    }
}

fn check_stmt_global_taint(
    stmt: &Statement,
    fname: &str,
    globals: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    match &stmt.kind {
        StmtKind::Command(c) => check_cmd_global_taint(c, stmt.span, fname, globals, diags),
        StmtKind::Assignment(a) => check_assign_global_taint(a, stmt.span, fname, globals, diags),
        StmtKind::Declaration(d) => {
            decl_global_taint(d, stmt.span, fname, globals, diags);
        }
        _ => {}
    }
}

/// A bulk fill (`argv g`, `read g`, `mapfile g`) draws from outside the
/// program, so its target becomes tainted.
fn check_cmd_global_taint(
    c: &Command,
    span: Span,
    fname: &str,
    globals: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    let name = expr_literal_str(&c.name);
    for t in bulk_fill_targets(c) {
        if globals.contains(&t) {
            diags.push(global_taint_diag(span, fname, &t, name));
        }
    }
}

/// The names a bulk fill (`argv g`, `read g`, `mapfile g`) writes from outside
/// the program. Empty for anything that is not a bulk fill, so SB5067 and the
/// DD#100 slot analysis can never disagree about what counts as one.
fn bulk_fill_targets(c: &Command) -> HashSet<String> {
    let name = expr_literal_str(&c.name);
    if !matches!(name, "read" | "readline" | "mapfile" | "argv") || is_value_split(name, c) {
        return HashSet::new();
    }
    let mut targets = HashSet::new();
    taint_read_target(c, &mut targets);
    targets
}

fn check_assign_global_taint(
    a: &Assignment,
    span: Span,
    fname: &str,
    globals: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    // A tainted *element* write is already SB5021; only the whole-name
    // assignment reaches here, so the two never double-report.
    if a.index.is_some() || !globals.contains(&a.name) || !is_taint_source(&a.value) {
        return;
    }
    diags.push(global_taint_diag(span, fname, &a.name, "assignment"));
}

fn global_taint_diag(span: Span, fname: &str, target: &str, via: &str) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5067",
        format!(
            "`{fname}` taints the global `{target}` via {via} — a function may \
             only taint its own locals, so that a caller can see where external \
             data enters. Fill a local, attest it, then assign the clean value \
             to `{target}`"
        ),
    )
}

// ── DD#100: collection parameters — `=name` at the call site, `A`..`E` in
// the callee ────────────────────────────────────────────────────────────────

/// SB5065: `A`..`E` are the reserved collection-parameter slots (DD#100).
///
/// They copy the `args` precedent (DD#75 §5 / DD#81): compiler-managed,
/// per-function, never declarable as a user variable. Two rules:
///
/// - **Top level has no slots.** A slot names a collection the *caller*
///   passed, so outside a function it names nothing — rejected exactly as
///   `args`/`$1` are (SB5024). Left to fall through, a top-level slot read
///   reached SB5039 ("introduced nowhere … declare it"), whose advice can't
///   be followed because declaring it is itself an error.
/// - **A slot can't be rebound.** A typed declaration (`array A=…`, `str A`)
///   or a whole-name assignment (`A=v`) would shadow the caller's collection.
///   Mutating *through* the slot — `A[0]=v`, `read A` — is the point of
///   DD#100 Part E and stays legal.
pub fn check_collection_slots(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for block in program_blocks(program) {
        crate::ir::walk_scoped_stmts(block, "", &mut |stmt, in_fn| {
            slot_rebind(stmt, &mut diags);
            if in_fn.is_empty() {
                slot_toplevel_reads(stmt, &mut diags);
            }
        });
    }
    diags
}

/// A declaration or whole-name assignment of a slot name. `A[0]=v` carries an
/// index and is a *mutation through* the slot, which Part E allows.
fn slot_rebind(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    match &stmt.kind {
        StmtKind::Assignment(a) => {
            flag_slot_rebind(a.index.is_none(), &a.name, stmt.span, diags);
        }
        // A declaration never carries an index, so every name it binds is a
        // whole-name rebind of the slot.
        StmtKind::Declaration(d) => {
            for a in &d.assignments {
                flag_slot_rebind(true, &a.name, stmt.span, diags);
            }
        }
        _ => {}
    }
}

/// Push the Part E rebind diagnostic when a whole-name write lands on a
/// collection slot. `A[0]=v` carries an index and is a mutation *through* the
/// slot, which is allowed.
fn flag_slot_rebind(whole_name: bool, name: &str, span: Span, diags: &mut Vec<Diagnostic>) {
    if whole_name && is_collection_slot(name) {
        diags.push(slot_rebind_diag(name, span));
    }
}

/// Slot reads in this statement's own expressions (not its child bodies —
/// `slot_stmt` walks those, so that a nested function flips `in_fn`).
fn slot_toplevel_reads(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    let mut visit = |e: &Expr| {
        let name = match &e.kind {
            ExprKind::SimpleExpansion(n) => n.as_str(),
            ExprKind::ParameterExpansion(pe) => pe.name.as_str(),
            ExprKind::Subscript { name, .. } => name.as_str(),
            _ => return,
        };
        if is_collection_slot(name) {
            diags.push(slot_toplevel_diag(name, e.span));
        }
    };
    stmt.direct_exprs(&mut |e| crate::ir::walk_expr_tree(e, &mut visit));
    if let StmtKind::ArithTest(arith) = &stmt.kind {
        crate::ir::walk_arith_expansions(arith, &mut visit);
    }
}

fn slot_rebind_diag(name: &str, span: Span) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5065",
        format!(
            "`{name}` is a reserved collection-parameter slot — it names the \
             collection a caller passed as `={{name}}`, so it can't be declared or \
             assigned. Write through it instead (`{name}[i]=…`, `read {name}`), or \
             use a differently named local"
        ),
    )
}

fn slot_toplevel_diag(name: &str, span: Span) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5065",
        format!(
            "`{name}` is a function-local collection-parameter slot — it does not \
             exist at top level, where there is no caller to have passed a \
             collection. Name the collection directly"
        ),
    )
}

/// SB5062: `len` / `join` / `keys` resolve their operand as a variable NAME.
/// Any other spelling was rendered by `expr_to_string` — a source-text
/// formatter, quotes and all — and the resulting text looked up as if it were
/// a name. That silently found nothing (`len "$s"` read as **0**, `join "a"`
/// as empty, and the operand was not even counted as a read) or produced a
/// bogus name that crashed codegen (`keys @m` → `ICE: undefined arrlen:
/// ${marrvals}`). Accept only what each builtin can actually resolve.
///
/// The accepted sets differ per builtin because their resolutions differ, not
/// by preference: `keys` needs the map itself, and a splat carries values,
/// which have no keys.
pub fn check_operand_spelling(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for stmt in program
        .statements
        .iter()
        .chain(program.end_block.iter().flatten())
    {
        crate::ir::walk_all_exprs(stmt, &mut |e| flag_operand_spelling(e, &mut diags));
    }
    diags
}

fn flag_operand_spelling(expr: &Expr, diags: &mut Vec<Diagnostic>) {
    let ExprKind::CommandSubstitution(stmts) = &expr.kind else {
        return;
    };
    let Some(StmtKind::Command(c)) = stmts.first().map(|s| &s.kind) else {
        return;
    };
    let builtin = expr_literal_str(&c.name);
    let Some(arg) = c.args.first() else {
        return;
    };
    let accepted = match (builtin, classify_operand(arg)) {
        ("len", OperandForm::Other) => "a variable name (`len a` or `len $a`)".to_string(),
        ("join", OperandForm::Dollar(n)) => {
            format!("a collection name (`join {n} SEP`) — a builtin takes the name")
        }
        // DD#100 Part D: a builtin's name operand takes a bare name and no
        // splat. `join @a SEP` and `join a SEP` were exact synonyms, and
        // likewise `len @a` / `len a` — the sigil asked for values that were
        // then immediately re-joined or re-counted. One spelling per question.
        ("join", OperandForm::Splat(n)) => {
            format!("a collection name (`join {n} SEP`) — a builtin takes the name, not a splat")
        }
        ("len", OperandForm::Splat(n)) => {
            format!("a collection name (`len {n}`) — a builtin takes the name, not a splat")
        }
        ("join", OperandForm::Other) => "a collection name (`join a SEP`)".to_string(),
        ("keys", OperandForm::Dollar(n) | OperandForm::Splat(n)) => {
            format!("a bare map name (`keys {n}`) — a splat carries values, which have no keys")
        }
        ("keys", OperandForm::Other) => {
            "a bare map name (`keys m`) — a splat carries values, which have no keys".to_string()
        }
        _ => return,
    };
    diags.push(Diagnostic::error_span(
        expr.span,
        "SB5062",
        format!(
            "`{builtin}` resolves its operand as a variable name, and does not \
             resolve this spelling. Use {accepted}"
        ),
    ));
}

// ── SB5063: a fill target is a binding, not a value ─────────────────
//
// `argv [N] a`, `match`/`matchall S P a` write INTO a named array, and
// `unset a` unbinds one. A quoted or
// sigilled operand in that slot is not a name, so the write landed nowhere:
// the array silently kept its previous contents, and because both are
// tainted sources, the attest obligation vanished along with the write.
// DD#99 already enforces this for `read`/`readline` (SB5057); these two
// slots were simply never covered.

/// SB5063: reject a fill target that is not a bare name.
pub fn check_fill_target(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for block in program_blocks(program) {
        check_fill_target_in(block, &mut diags);
    }
    diags
}

fn check_fill_target_in(stmts: &[Statement], diags: &mut Vec<Diagnostic>) {
    for stmt in stmts {
        check_fill_target_stmt(stmt, diags);
    }
}

/// One statement's worth of [`check_fill_target_in`].
fn check_fill_target_stmt(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    if let StmtKind::Command(c) = &stmt.kind {
        flag_fill_target(c, stmt.span, diags);
    }
    for body in stmt.structural_bodies() {
        check_fill_target_in(body, diags);
    }
    for body in stmt.cmdsub_bodies() {
        check_fill_target_body(body, diags);
    }
}

/// DD#60 gives `argv` two shapes that collide at one argument: the splat
/// EXPRESSION `$(argv N)`, whose `N` is an index, and the fill STATEMENT
/// `argv N name`, whose second word is an out-param. A body that is the splat
/// is not walked, or its index reads as a fill target and earns a spurious
/// SB5063 on the canonical `for x in $(argv 1)`.
fn check_fill_target_body(body: &[Statement], diags: &mut Vec<Diagnostic>) {
    if !cmdsub_is_argv_splat(body) {
        check_fill_target_in(body, diags);
    }
}

fn flag_fill_target(c: &Command, span: Span, diags: &mut Vec<Diagnostic>) {
    let builtin = expr_literal_str(&c.name);
    // `unset` takes several targets and permits the subscript form `a[i]`;
    // the fill builtins take one, and a bare name only.
    let (targets, allow_subscript): (Vec<&str>, bool) = match builtin {
        "argv" => (argv_target_name(&c.args).into_iter().collect(), false),
        // DD#63: the out-array is the third argument. `match S P` is the
        // two-arg boolean form and has no target at all, and a wrong arity
        // is SB4045's to report — do not preempt it.
        "match" | "matchall" if c.args.len() == 3 => (
            c.args.last().map(expr_literal_str).into_iter().collect(),
            false,
        ),
        "unset" => (c.args.iter().map(expr_literal_str).collect(), true),
        _ => return,
    };
    for target in targets {
        if is_plain_identifier(target) || (allow_subscript && is_subscripted_name(target)) {
            continue;
        }
        diags.push(fill_target_diag(builtin, span));
    }
}

/// `a[i]` — a plain identifier carrying a subscript. `unset a[i]` is legal.
fn is_subscripted_name(word: &str) -> bool {
    match word.split_once('[') {
        Some((name, rest)) => is_plain_identifier(name) && rest.ends_with(']'),
        None => false,
    }
}

fn fill_target_diag(builtin: &str, span: Span) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5063",
        format!(
            "`{builtin}`'s target must be a bare name — it is a binding, not a \
             value. A quoted or sigilled target names nothing, so the operation \
             is silently discarded"
        ),
    )
}

fn check_len_in_scope(
    stmts: &[Statement],
    known: &HashSet<String>,
    in_function: bool,
    diags: &mut Vec<Diagnostic>,
) {
    for stmt in stmts {
        if let StmtKind::Function(f) = &stmt.kind {
            // A new scope: the function's own introductions + enclosing names.
            let mut fn_known = known.clone();
            collect_scope_names(&f.body, &mut fn_known);
            check_len_in_scope(&f.body, &fn_known, true, diags);
            continue;
        }
        stmt.direct_exprs(&mut |e| {
            crate::ir::walk_expr_tree(e, &mut |sub| {
                flag_len_cmdsub(sub, known, in_function, diags);
            });
        });
        for body in stmt.child_bodies() {
            check_len_in_scope(body, known, in_function, diags);
        }
    }
}

/// If `expr` is a `$(len NAME)` command substitution, check its operand.
fn flag_len_cmdsub(
    expr: &Expr,
    known: &HashSet<String>,
    in_function: bool,
    diags: &mut Vec<Diagnostic>,
) {
    let Some((builtin, name)) = len_cmdsub_operand(expr) else {
        return;
    };
    // Both `args` (the implicit array) and `A`–`E` (caller-bound slots) exist
    // only inside a function, so neither is ever "introduced" in a body — but
    // at top level each is its own reject. `check_positionals` only sees the
    // `@args`/`${args…}` expansion forms and `slot_toplevel_reads` only sees
    // expansions, so a bare `len` operand has to be caught here.
    if name == "args" || is_collection_slot(&name) {
        flag_function_local_operand(&name, expr.span, in_function, diags);
        return;
    }
    if known.contains(&name) || handled_elsewhere(&name) {
        return;
    }
    diags.push(Diagnostic::error_span(
        expr.span,
        "SB5039",
        format!(
            "`{builtin} {name}` — `{name}` is introduced nowhere (not a variable, array, \
             map, loop variable, or `args`); this reads as 0. Declare it, or fix the typo"
        ),
    ));
}

/// The builtin name and its bare-identifier operand, for a `$(len NAME)`
/// command substitution: it resolves the operand as a name and reads 0 from a
/// slot that does not exist. DD#102 moved `strlen` to a value operand, where
/// an unknown bare word is a string literal rather than a missing variable.
fn len_cmdsub_operand(expr: &Expr) -> Option<(String, String)> {
    let ExprKind::CommandSubstitution(stmts) = &expr.kind else {
        return None;
    };
    let StmtKind::Command(c) = &stmts.first()?.kind else {
        return None;
    };
    let builtin = expr_literal_str(&c.name);
    if builtin != "len" {
        return None;
    }
    // subscript cell / expansion / cmdsub operand — resolved elsewhere
    Some((builtin.to_string(), len_operand_name(c)?))
}

/// `args` and the DD#100 slots are legitimate inside a function and a reject
/// outside one.
fn flag_function_local_operand(
    name: &str,
    span: Span,
    in_function: bool,
    diags: &mut Vec<Diagnostic>,
) {
    if in_function {
        return;
    }
    if name == "args" {
        flag_args_toplevel("args", span, diags);
    } else {
        diags.push(slot_toplevel_diag(name, span));
    }
}

/// The bare-identifier operand of `len`: a plain word (`len a`) or a splat
/// (`len @a`). `None` for a subscript cell, a `$`-expansion, a `$(…)`, or a
/// non-identifier — those are not name-membership-checkable here.
fn len_operand_name(c: &Command) -> Option<String> {
    let arg = c.args.first()?;
    let name = match &arg.kind {
        ExprKind::ParameterExpansion(pe) if pe.operator.as_deref() == Some("arrvals") => {
            pe.name.clone()
        }
        ExprKind::Literal(s) => s.clone(),
        _ => return None,
    };
    if is_plain_identifier(&name) {
        Some(name)
    } else {
        None
    }
}

/// Read-recognition sink shared by SB5031 (flag an undeclared read) and the
/// SB5050 dead-variable check (collect every read). The one traversal below
/// drives a single `record` per variable read; the sink decides what to do —
/// so the two passes can never diverge on "what counts as a read".
enum SinkMode<'a> {
    /// SB5031: flag a read whose name is not in the current scope's `known`.
    Flag(&'a mut Vec<Diagnostic>),
    /// SB5050: collect every read name (scope-agnostic superset).
    Collect(&'a mut HashSet<String>),
}

struct ReadSink<'a> {
    mode: SinkMode<'a>,
    /// Program-wide function names. `known` deliberately merges functions with
    /// variables so a nullary sexpr `(g)` is not flagged as an undeclared read —
    /// but DD#103's with-args rule has to tell them apart again, because a
    /// function is callable and a variable is not. Empty in `Collect` mode,
    /// where every branch records the name and the distinction cannot matter
    /// (the same reasoning that lets `collect_reads` pass an empty `known`).
    funcs: &'a HashSet<String>,
}

impl ReadSink<'_> {
    fn record(&mut self, name: &str, span: Span, known: &HashSet<String>) {
        self.record_param(name, span, known, false);
    }

    /// DD#103: is `name` a legal *callee* for an arith sexpr `(name …)`?
    fn is_sexpr_callee(&self, name: &str) -> bool {
        crate::resolve::is_intrinsic(name) || self.funcs.contains(name)
    }

    /// Record a name only for dead-var (Collect mode); a no-op for SB5031
    /// (Flag mode). Used where the name might be a variable but is not a read
    /// SB5031 should flag — e.g. a DD#58 arith sexpr `(name …)`, whose name may
    /// be a variable (`(x)`), a user function (`(f)`), or an intrinsic
    /// (`(random)`, `(len m)`); only the first is a variable read, and SB5031
    /// must not flag the intrinsic/function names.
    fn collect_only(&mut self, name: &str) {
        if let SinkMode::Collect(set) = &mut self.mode {
            set.insert(name.to_string());
        }
    }

    /// SB5011: an arith sexpr `(name args…)` that carries arguments is
    /// unambiguously a *call*, never `(x)` grouping — and a user function with
    /// arguments does not even parse there (SB4014), so the only legal callee is
    /// an intrinsic. An unrecognized name therefore names nothing. It used to
    /// lower to `IntFuncCall`, find no user body, and yield a silent 0.
    fn flag_unknown_sexpr_callee(&mut self, name: &str, span: Span) {
        if let SinkMode::Flag(diags) = &mut self.mode {
            diags.push(Diagnostic::error_span(
                span,
                "SB5011",
                format!(
                    "'{name}' is not a sash intrinsic — an arithmetic sexpr \
                     `({name} …)` can only call one. Capture an external command \
                     first: `x=$(/path/to/{name} …)`, then use `x` in `$(( … ))`"
                ),
            ));
        }
    }

    /// Record a variable read. `opt_out` marks a parameter expansion carrying an
    /// operator (`${x:-…}`, `@a`, `${x:s/…/…/}`): SB5031 treats it as the
    /// explicit "intentional default / transform" opt-out and does NOT flag it,
    /// but dead-var still counts it as a read of the variable.
    /// Record an `@name` splat read.
    ///
    /// DD#69: a splat's disposition covers the **empty** case, not the
    /// **undeclared** one — `@{a:-z}` still requires `a` to be a collection in
    /// scope. So unlike `${x:-…}`, a splat's operator is not SB5031's opt-out,
    /// and it reports with the splat's own wording: "write `${a:-DEFAULT}`" is
    /// not advice that makes sense for an array.
    ///
    /// This lives here, on the one scope-aware traversal, rather than in the
    /// program-wide union `check_scalar_splat` used to carry. That union was
    /// wrong in both directions: it rejected an array declared and splatted
    /// inside the same `$( )` body, and it accepted a function-local array
    /// splatted at top level, which then ran (DD#86 makes that name local).
    ///
    /// `args` is bound by the caller and introduced nowhere;
    /// `handled_elsewhere` already covers the DD#100 slots.
    fn record_splat(&mut self, name: &str, span: Span, known: &HashSet<String>) {
        match &mut self.mode {
            SinkMode::Collect(set) => {
                set.insert(name.to_string());
            }
            SinkMode::Flag(diags) => {
                if !is_plain_identifier(name)
                    || handled_elsewhere(name)
                    || name == "args"
                    || known.contains(name)
                {
                    return;
                }
                diags.push(undefined_splat_diag(name, span));
            }
        }
    }

    fn record_param(&mut self, name: &str, span: Span, known: &HashSet<String>, opt_out: bool) {
        match &mut self.mode {
            SinkMode::Collect(set) => {
                set.insert(name.to_string());
            }
            SinkMode::Flag(diags) => {
                if opt_out
                    || !is_plain_identifier(name)
                    || handled_elsewhere(name)
                    || known.contains(name)
                {
                    return;
                }
                diags.push(Diagnostic::error_span(
                    span,
                    "SB5031",
                    format!(
                        "'${}' is assigned nowhere (possible typo) — assign it first, \
                         or write ${{{}:-DEFAULT}} for an intentional default",
                        name, name
                    ),
                ));
            }
        }
    }
}

fn check_reads_in_scope(stmts: &[Statement], known: &HashSet<String>, sink: &mut ReadSink) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Function(f) => check_function_scope(f, known, sink),
            // `attest` with a decodable form is handled specially (below);
            // a non-attest / malformed one falls through to the default arm.
            StmtKind::Command(c) if try_check_attest_stmt(c, known, sink) => {}
            _ => check_stmt_exprs_and_bodies(stmt, known, sink),
        }
    }
}

/// A nested function is a new scope: its own introductions plus the
/// enclosing (global) names it can see.
fn check_function_scope(f: &Function, known: &HashSet<String>, sink: &mut ReadSink) {
    let mut fn_known = HashSet::new();
    collect_scope_names(&f.body, &mut fn_known);
    fn_known.extend(known.iter().cloned());
    // DD#75 §5: the implicit positional array `args` is in scope in every
    // function body (`$(( args[i] ))` / `${args[i]}`). Without this, the bare
    // arith cell read `$(( args[0] ))` was an SB5031 false positive
    // (arith-args-subscript-codegen).
    fn_known.insert("args".to_string());
    // DD#100: the collection-parameter slots are bound by the caller, so they
    // are in scope in every function body without being introduced in it.
    for i in 0..MAX_COLLECTION_SLOTS {
        fn_known.insert(collection_slot_name(i));
    }
    check_reads_in_scope(&f.body, &fn_known, sink);
}

/// `attest ARR in VAR [[ …$VAR… ]]` (and the map form) introduce
/// predicate-local binder variable(s): check the test with them added,
/// and don't treat the binder/array literals as reads. Returns `true`
/// when the statement was a decodable attest (and has been checked).
fn try_check_attest_stmt(c: &Command, known: &HashSet<String>, sink: &mut ReadSink) -> bool {
    if expr_literal_str(&c.name) != "attest" {
        return false;
    }
    let Some((form, pred)) = crate::ir::classify_attest(&c.args) else {
        return false;
    };
    let mut attest_known = known.clone();
    match form {
        crate::ir::AttestForm::Indexed { binder, array } => {
            attest_known.insert(binder.to_string());
            // The array being attested is read (attest iterates it) — count it for
            // dead-var; SB5031 handles an undeclared attest array elsewhere.
            sink.collect_only(array);
        }
        crate::ir::AttestForm::Map { key, val, map } => {
            attest_known.insert(key.to_string());
            attest_known.insert(val.to_string());
            sink.collect_only(map);
        }
        crate::ir::AttestForm::Scalar { .. } => {}
    }
    match pred {
        crate::ir::AttestPredicate::Test(test) => check_test_reads(test, &attest_known, sink),
        // DD#93 Part D: the conversion reads the attested source and every other
        // name its RHS mentions (`(( y = x + k ))` reads `k` too, or `k` would
        // look like a dead variable). The target is a definition, introduced by
        // the attest itself, and `check_arith_reads` only descends into an
        // `Assign`'s value, so it never sees the target.
        //
        // This must use the same collector as a plain `(( … ))` statement
        // (`check_stmt_exprs_and_bodies`). It used to hand-roll a collect-only
        // walk over `ir::walk_arith_var_reads`, which answers a *different*
        // question — DD#77 D1's "which bare int operands cross from string?" —
        // and so deliberately skips a subscript's subject name and a sexpr
        // callee name, and never checks definedness at all. Three defects fell
        // out of that one substitution: `(( c = n + v[d] ))` and
        // `(( c = n + (d) ))` reported a false SB5050 against a variable the
        // predicate plainly reads; an *undefined* name (`(( c = n + zz ))`)
        // raised nothing and silently read as 0, against DG's rule that an
        // unrecognized symbol in arithmetic is an error; and an undefined
        // *array* (`(( c = n + vv[0] ))`) reached codegen and ICEd.
        crate::ir::AttestPredicate::ArithConvert { value, .. } => {
            if let crate::ir::AttestForm::Scalar { var } = form {
                sink.collect_only(var);
            }
            check_arith_reads(value, c.name.span, &attest_known, sink);
        }
    }
    true
}

/// Default statement handling: check its own expressions, then recurse
/// into any control-flow child bodies (same scope).
fn check_stmt_exprs_and_bodies(stmt: &Statement, known: &HashSet<String>, sink: &mut ReadSink) {
    stmt.direct_exprs(&mut |e| check_expr_reads(e, known, sink));
    // A `(( … ))` statement / condition holds its reads in an `ArithExpr` tree,
    // not a direct `Expr` (`direct_exprs` returns nothing for it) — walk it so
    // a bare `(( x > 0 ))` read is seen (by both SB5031 and dead-var).
    if let StmtKind::ArithTest(a) = &stmt.kind {
        check_arith_reads(a, stmt.span, known, sink);
    }
    // `structural_bodies`, not `child_bodies`: a `$( … )` body is reached from
    // `check_expr_reads`, which opens a child scope for it (DD#86 — a name
    // first introduced inside the body is local to it). Taking it here as well
    // would check every read in the body twice, and the second pass would use
    // the *enclosing* scope, so `s=$(argv 0 b; echo @b)` reported `b` as
    // introduced nowhere.
    for body in stmt.structural_bodies() {
        check_reads_in_scope(body, known, sink);
    }
}

fn check_expr_reads(expr: &Expr, known: &HashSet<String>, sink: &mut ReadSink) {
    match &expr.kind {
        ExprKind::SimpleExpansion(name) => sink.record(name, expr.span, known),
        ExprKind::ParameterExpansion(pe) => check_param_expansion_reads(pe, expr.span, known, sink),
        ExprKind::Subscript { name, index } => {
            // The collection name is itself a read — flag an undeclared one so it
            // reports SB5031 rather than ICE-ing at codegen (`undefined array`).
            // `args` is function-local: a top-level `${args[i]}` is reported by
            // SB5024 (skip it here to avoid a double), and inside a function it
            // is already in scope.
            if name != "args" {
                sink.record(name, expr.span, known);
            }
            check_expr_reads(index, known, sink);
        }
        ExprKind::CommandSubstitution(stmts) => check_cmdsub_reads(stmts, known, sink),
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) | ExprKind::Array(parts) => {
            check_exprs(parts, known, sink)
        }
        ExprKind::AssocArray(pairs) => check_assoc_values(pairs, known, sink),
        ExprKind::Test(t) => check_test_reads(t, known, sink),
        // B2 (DD arith-subscript / SB5031-consistency): a bare name *read* in
        // arithmetic that is assigned nowhere is a typo, exactly like `$x`
        // outside arith. `(( i++ ))` / `(( x = … ))` lower to an assignment
        // (their name is collected as in-scope), so anything you assign is fine;
        // a pure read (`$(( y + 1 ))`, the index `i` in `a[i]`) is not.
        ExprKind::Arithmetic(a) => check_arith_reads(a, expr.span, known, sink),
        // `$(<$f)` — the path is a read of `f`. Without this arm the `_`
        // fall-through made it invisible, so SB5050 called `f` a dead variable
        // and SB5031 would have missed a typo'd path.
        ExprKind::SlurpFile(path) => check_expr_reads(path, known, sink),
        _ => {}
    }
}

fn check_arith_reads(
    a: &crate::ir::ArithExpr,
    span: Span,
    known: &HashSet<String>,
    sink: &mut ReadSink,
) {
    use crate::ir::ArithExpr::*;
    match a {
        Var(name) | DollarVar(name) => sink.record(name, span, known),
        // A bare int-array cell read: the array name is read, and the index is
        // a read position too.
        Subscript { name, index } => {
            sink.record(name, span, known);
            check_arith_reads(index, span, known, sink);
        }
        SubscriptAssign {
            name, index, value, ..
        } => {
            sink.record(name, span, known);
            check_arith_reads(index, span, known, sink);
            check_arith_reads(value, span, known, sink);
        }
        // DD#58 sexpr: a bare `(name …)` parses as a `FuncCall`; the name may be
        // a variable read (`$(( (x) ))`), a user function (`$((f))`), or an
        // intrinsic (`(random)`, `(len m)`).
        //
        // DG's rule: an unrecognized symbol in arithmetic context is an error
        // (an assignment target is the only exception). Every other spelling of
        // an undefined read — `$(( zz ))`, `$zz`, `${zz}`, `zz[0]`, `a[zz]` —
        // already reports SB5031; this node was the sole hole, because it
        // blanket-`collect_only`'d the name on the grounds that it *might* be an
        // intrinsic. Decide instead:
        //
        //   an intrinsic or a user function → a real callee, not a variable
        //                                read. Record for dead-var only, and
        //                                leave arity/type to SB4045 / SB4030.
        //   nullary, neither             → the transform resolves it as a
        //                                variable read (`resolve_nullary_sexpr`),
        //                                so it is one → SB5031 when undefined.
        //   with args, neither           → unambiguously a call, to something
        //                                that is not callable → SB5011.
        FuncCall { name, args } => {
            if sink.is_sexpr_callee(name) {
                sink.collect_only(name);
            } else if args.is_empty() {
                sink.record(name, span, known);
            } else {
                sink.collect_only(name);
                sink.flag_unknown_sexpr_callee(name, span);
            }
            check_arith_each(args, span, known, sink);
        }
        // `${…}` import — normal-parse rvalue, checked by the ordinary path.
        Expansion(inner) => check_expr_reads(inner, known, sink),
        _ => check_arith_compound(a, span, known, sink),
    }
}

/// The structural (non-leaf, non-name) arith arms: recurse into operands.
fn check_arith_compound(
    a: &crate::ir::ArithExpr,
    span: Span,
    known: &HashSet<String>,
    sink: &mut ReadSink,
) {
    use crate::ir::ArithExpr::*;
    match a {
        BinOp { left, right, .. } => {
            check_arith_reads(left, span, known, sink);
            check_arith_reads(right, span, known, sink);
        }
        UnaryOp { operand, .. } | Paren(operand) => check_arith_reads(operand, span, known, sink),
        Ternary {
            cond,
            true_val,
            false_val,
        } => {
            check_arith_reads(cond, span, known, sink);
            check_arith_reads(true_val, span, known, sink);
            check_arith_reads(false_val, span, known, sink);
        }
        Comma(exprs) => check_arith_each(exprs, span, known, sink),
        Assign { value, .. } | CompoundAssign { value, .. } => {
            check_arith_reads(value, span, known, sink)
        }
        // Leaf/name arms are handled by check_arith_reads; the rest are no-ops.
        _ => {}
    }
}

fn check_arith_each(
    exprs: &[crate::ir::ArithExpr],
    span: Span,
    known: &HashSet<String>,
    sink: &mut ReadSink,
) {
    for e in exprs {
        check_arith_reads(e, span, known, sink);
    }
}

/// A bare `${x}` (no operator) is a read; `${x:-…}` and friends are the
/// explicit opt-out. Any operand (default word etc.) is itself checked.
fn check_param_expansion_reads(
    pe: &ParameterExpansion,
    span: Span,
    known: &HashSet<String>,
    sink: &mut ReadSink,
) {
    // Bare `${x}` is a read SB5031 flags; `${x:-…}`/`@a`/`${x:s/…/…/}` carry an
    // operator — SB5031's opt-out, but a real read for dead-var.
    if is_splat_expansion(pe) {
        sink.record_splat(&pe.name, span, known);
    } else {
        sink.record_param(&pe.name, span, known, pe.operator.is_some());
    }
    if let Some(ref operand) = pe.operand {
        check_expr_reads(operand, known, sink);
    }
}

/// A command substitution runs in a subshell: its statements see the
/// enclosing scope plus their own (subshell-local) introductions, which
/// do NOT leak back out.
fn check_cmdsub_reads(stmts: &[Statement], known: &HashSet<String>, sink: &mut ReadSink) {
    let mut sub_known = known.clone();
    collect_scope_names(stmts, &mut sub_known);
    check_reads_in_scope(stmts, &sub_known, sink);
}

fn check_exprs(parts: &[Expr], known: &HashSet<String>, sink: &mut ReadSink) {
    for p in parts {
        check_expr_reads(p, known, sink);
    }
}

fn check_assoc_values(pairs: &[(String, Expr)], known: &HashSet<String>, sink: &mut ReadSink) {
    for (_, v) in pairs {
        check_expr_reads(v, known, sink);
    }
}

fn check_test_reads(test: &TestExpr, known: &HashSet<String>, sink: &mut ReadSink) {
    match test {
        TestExpr::Unary { operand, .. } => check_expr_reads(operand, known, sink),
        TestExpr::Binary { left, right, .. } => {
            check_expr_reads(left, known, sink);
            check_expr_reads(right, known, sink);
        }
        TestExpr::Not(inner) => check_test_reads(inner, known, sink),
        TestExpr::And(l, r) | TestExpr::Or(l, r) => {
            check_test_reads(l, known, sink);
            check_test_reads(r, known, sink);
        }
    }
}

/// DD#63: a `${var:s/…}` payload missing a mandatory `/` field delimiter is
/// parsed as an `s_malformed` sentinel (the `${…}` value path cannot itself
/// raise a parse diagnostic). Reject it here — SB4032. The `var =~ s/…`
/// mutation form raises the same code directly at parse time (its path is
/// `Result`-returning).
pub fn check_malformed_sed(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for stmt in program
        .statements
        .iter()
        .chain(program.end_block.iter().flatten())
    {
        crate::ir::walk_all_exprs(stmt, &mut |e| {
            flag_malformed_sed(e, &mut diags);
            flag_malformed_trust(e, &mut diags);
        });
    }
    diags
}

/// DD#91: `${x:!JUNK}` — `!` followed by something that is not a disposition.
/// The parser cannot raise here (the `${…}` value path is not `Result`-
/// returning), so it carries a `trust_malformed` sentinel. Rejecting matters:
/// silently degrading to a trusted bare `${x}` would grant the taint bypass
/// while dropping whatever the author actually asked for.
fn flag_malformed_trust(e: &Expr, diags: &mut Vec<Diagnostic>) {
    let ExprKind::ParameterExpansion(pe) = &e.kind else {
        return;
    };
    if pe.operator.as_deref() != Some("trust_malformed") {
        return;
    }
    diags.push(Diagnostic::error_span(
        e.span,
        "SB4051",
        format!(
            "`${{{}:!…}}` — `!` is the taint-off modifier and must be followed \
             by a disposition (`-WORD`, `?MSG`, `~`) or by nothing. `${{{}:!}}` \
             is the value, trusted.",
            pe.name, pe.name
        ),
    ));
}

fn flag_malformed_sed(e: &Expr, diags: &mut Vec<Diagnostic>) {
    let ExprKind::ParameterExpansion(pe) = &e.kind else {
        return;
    };
    if pe.operator.as_deref() == Some("s_malformed") {
        diags.push(Diagnostic::error_span(
            e.span,
            "SB4032",
            "malformed `${…:s/…}` substitution — expected s/RE/SUB/[flags] \
             with three `/` delimiters"
                .to_string(),
        ));
    }
}

/// DD#80: a braced subscript index/key `${a[K]}` must be a DIRECT, unambiguous
/// value — an integer literal, a `$var`, an expansion (`${…}`/`$(…)`/`$(( … ))`),
/// a quoted string, or a clean-identifier bareword. An implicit/ambiguous bare
/// form (brackets, operators, whitespace, or a mixed literal+expansion) is
/// rejected here (SB4041). The parser re-parses the index as a value expression,
/// so an explicit nested context is a clean expansion node and passes; an
/// ambiguous bare form falls through to a `Literal` (with stray chars) or a
/// `Concat`, which this check rejects. Type-level "string key on an int array"
/// is a separate rule (SB4042, type_check).
pub fn check_subscript_index(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for stmt in program
        .statements
        .iter()
        .chain(program.end_block.iter().flatten())
    {
        crate::ir::walk_all_exprs(stmt, &mut |e| flag_subscript_index(e, &mut diags));
    }
    diags
}

fn flag_subscript_index(e: &Expr, diags: &mut Vec<Diagnostic>) {
    if let Some(d) = splat_not_subscriptable_diag(e) {
        diags.push(d);
        return;
    }
    let index = match &e.kind {
        ExprKind::Subscript { index, .. } => Some(index.as_ref()),
        ExprKind::ParameterExpansion(pe) => pe.index.as_deref(),
        _ => None,
    };
    if let Some(d) = index.and_then(ambiguous_index_diag) {
        diags.push(d);
    }
}

/// `$@[i]` / `$*[i]` — the positional splat isn't subscriptable (`$@[0]` reached
/// codegen as a subscript on an "array" named `@` and ICE'd). Returns the SB5031
/// reject, or `None` for any other subscript name.
fn splat_not_subscriptable_diag(e: &Expr) -> Option<Diagnostic> {
    let ExprKind::Subscript { name, .. } = &e.kind else {
        return None;
    };
    if name != "@" && name != "*" {
        return None;
    }
    Some(Diagnostic::error_span(
        e.span,
        "SB5031",
        format!(
            "`${name}` (the positional splat) isn't subscriptable — use `$1`..`$N`, \
             or `${{args[i]}}` for a dynamic index inside a function"
        ),
    ))
}

/// The SB4041 diagnostic for an ambiguous braced index, or `None` for an
/// unambiguous one. Explicit expansions and quoted strings are always fine; a
/// bare `Literal` is fine only when it is an integer literal or a clean
/// identifier; anything else (a `Literal` carrying `[`/operators, or a `Concat`
/// of mixed parts) is ambiguous.
fn ambiguous_index_diag(index: &Expr) -> Option<Diagnostic> {
    match &index.kind {
        ExprKind::SimpleExpansion(_)
        | ExprKind::ParameterExpansion(_)
        | ExprKind::Subscript { .. }
        | ExprKind::CommandSubstitution(_)
        | ExprKind::Arithmetic(_)
        | ExprKind::DoubleQuoted(_)
        | ExprKind::SingleQuoted(_) => None,
        ExprKind::Literal(s) if index_is_clean_literal(s) => None,
        ExprKind::Literal(s) => Some(ambiguous_index_error(index.span, s)),
        _ => Some(ambiguous_index_error(index.span, "…")),
    }
}

/// A clean braced-index literal is an integer literal or a clean identifier.
fn index_is_clean_literal(s: &str) -> bool {
    index_is_integer(s) || index_is_identifier(s)
}

fn index_is_integer(s: &str) -> bool {
    let digits = s.strip_prefix('-').unwrap_or(s);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

fn index_is_identifier(s: &str) -> bool {
    let mut bytes = s.bytes();
    matches!(bytes.next(), Some(b) if b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn ambiguous_index_error(span: Span, idx: &str) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB4041",
        format!(
            "ambiguous braced index `[{idx}]` — `${{ }}` takes a direct key, not a computed \
             one. Quote a literal key (`[\"…\"]`), use a variable (`[$name]`), or open \
             arithmetic explicitly (`[$(( … ))]`)"
        ),
    )
}

/// Enforce `arg`'s arity (index + optional default) and the literal-operand
/// rule on its default.
///
/// This used to also host SB5035, which rejected `$(random)` in a `:-` default
/// because the default was evaluated eagerly and `random` advances the PRNG
/// whether or not the fallback is taken. Both halves of that are gone: `:-` now
/// carries a literal, so no substitution of any kind can appear in one, and the
/// ban has nothing left to reject. The general rule subsumes the special case.
pub fn check_disposition_defaults(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for stmt in program
        .statements
        .iter()
        .chain(program.end_block.iter().flatten())
    {
        crate::ir::walk_all_exprs(stmt, &mut |e| {
            flag_arg_builtin(e, &mut diags);
        });
    }
    diags
}

/// DD#60 addendum: `$(arg …)` takes an index and an optional default
/// (`$(arg N)` or `$(arg N DEFAULT)`) — reject other arities.
///
/// The default, if present, must be a **literal** — the third disposition
/// default, held to the same rule as `${x:-D}` and `${x:?MSG}` (SB4070, DG
/// 2026-09-30). Those two are literals by *parse*; this one cannot be, because
/// it is an ordinary command argument and nothing knows it belongs to `arg`
/// until validate. So the rule is enforced structurally here instead, on the
/// parsed expression.
///
/// A single-quoted default needs no special case: `$(arg 5 'costs $5')` is a
/// `SingleQuoted`, which carries no expansion and so passes. The escape comes
/// out of the existing quoting rules for free.
fn flag_arg_builtin(e: &Expr, diags: &mut Vec<Diagnostic>) {
    let ExprKind::CommandSubstitution(stmts) = &e.kind else {
        return;
    };
    let Some(StmtKind::Command(c)) = stmts.first().map(|s| &s.kind) else {
        return;
    };
    if expr_literal_str(&c.name) != "arg" {
        return;
    }
    if let Some(d) = arg_index_diag(e, c) {
        diags.push(d);
        return;
    }
    if let Some(default) = c.args.get(1) {
        if let Some(bad) = first_expansion(default) {
            diags.push(disposition_default_expansion(bad));
        }
    }
}

/// Arity / index diagnostics for `$(arg …)`: wrong argument count (SB5036) or a
/// statically-negative literal index (SB5038, DD#60). `None` when the index is
/// acceptable.
fn arg_index_diag(e: &Expr, c: &Command) -> Option<Diagnostic> {
    if c.args.is_empty() || c.args.len() > 2 {
        return Some(Diagnostic::error_span(
            e.span,
            "SB5036",
            "`arg` takes an index and an optional default — `$(arg N)` or \
             `$(arg N DEFAULT)`"
                .to_string(),
        ));
    }
    // DD#60: negative indices are rejected at compile time — no negative
    // indexing in v1. A negative *literal* index (`$(arg -1)`, `$(arg -1 D)`)
    // is a typo guard caught here; a dynamic negative index stays a runtime
    // concern (`sash_argv_get` aborts).
    if c.args.first().is_some_and(is_negative_int_literal) {
        return Some(Diagnostic::error_span(
            c.args[0].span,
            "SB5038",
            "`arg` index must not be negative — DD#60 rejects negative \
             indexing (no counting from the end in v1)"
                .to_string(),
        ));
    }
    // A literal index that is not a plain non-negative integer is a typo, and
    // a silent one: the runtime parses the index by taking leading digits, so
    // `$(arg 1x)` reads argument 1 and `$(arg abc)` reads argument 0 — the
    // program name — with a successful exit. `$(arg 5)` past the end aborts
    // rather than yielding empty (av.t6); this is the same rule for the same
    // reason. A dynamic index (`$(arg $i)`) stays a runtime concern.
    if c.args.first().is_some_and(is_nonnumeric_index_literal) {
        let s = expr_literal_str(&c.args[0]);
        return Some(Diagnostic::error_span(
            c.args[0].span,
            "SB5078",
            format!(
                "`arg` index must be a non-negative integer — `{s}` is not one, and \
             would be read as an index by taking its leading digits (`$(arg 1x)` \
             means argument 1, `$(arg abc)` means argument 0). Use a literal \
             index, or an expansion for a computed one"
            ),
        ));
    }
    None
}

/// A statically-negative integer literal (`-1`, `-42`) — the index form DD#60
/// rejects at compile time. `-` followed by ASCII digits, nothing else.
fn is_negative_int_literal(e: &Expr) -> bool {
    let s = expr_literal_str(e);
    matches!(s.strip_prefix('-'), Some(rest) if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
}

/// A literal index that is not a plain non-negative integer (`abc`, `1x`, `-`,
/// `+1`). Non-literals return `false` — `expr_literal_str` yields `""` for an
/// expansion, and a computed index is a runtime concern, not a typo.
///
/// Runs *after* [`is_negative_int_literal`], so `-1` keeps its own DD#60
/// diagnostic and only the genuinely malformed spellings land here.
fn is_nonnumeric_index_literal(e: &Expr) -> bool {
    let s = expr_literal_str(e);
    !s.is_empty() && !s.bytes().all(|b| b.is_ascii_digit())
}

/// The span of the first expansion inside `expr`, or `None` if it is all
/// literal text.
///
/// **Total over `ExprKind` on purpose — do not add a wildcard.** The whole
/// point is to answer "can any of this be substituted at runtime", so a new
/// variant must be classified deliberately. A wildcard would silently answer
/// "no" for it and reopen the hole this rule closes.
fn first_expansion(expr: &Expr) -> Option<Span> {
    match &expr.kind {
        // Inert text.
        ExprKind::Literal(_) | ExprKind::SingleQuoted(_) => None,
        // Substituted at runtime — the whole point of the rule.
        ExprKind::SimpleExpansion(_)
        | ExprKind::ParameterExpansion(_)
        | ExprKind::Subscript { .. }
        | ExprKind::CommandSubstitution(_)
        | ExprKind::SlurpFile(_)
        | ExprKind::Arithmetic(_) => Some(expr.span),
        // Containers: inert in themselves, so recurse.
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) | ExprKind::Array(parts) => {
            parts.iter().find_map(first_expansion)
        }
        ExprKind::AssocArray(pairs) => pairs.iter().find_map(|(_, v)| first_expansion(v)),
        // A test is a statement-level construct and cannot be an `arg` default;
        // treat it as substituted rather than inventing a traversal for it.
        ExprKind::Test(_) => Some(expr.span),
    }
}

fn disposition_default_expansion(span: Span) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB4070",
        "an `$(arg N DEFAULT)` default is a literal, so a `$` expansion in it \
         would be silently used as text rather than expanded — the same rule as \
         `${x:-D}` and `${x:?MSG}`. Single-quote it to mean a literal `$` \
         (`$(arg 5 'costs $5')`), or, if you want a computed value, capture the \
         argument and test it explicitly"
            .to_string(),
    )
}

// ── SB5056 (DD#98): `_` is write-only — every read of it is rejected ──
//
// This is what makes the taint exemption safe rather than a laundering hole:
// if `_` were both taint-exempt and readable, `read _ x; y=$_` would move a
// tainted value into a clean one. Nothing can observe `_`, so nothing needs to
// attest it.

/// SB5056: reject any read of the don't-care binder `_`.
pub fn check_discard_reads(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for block in program_blocks(program) {
        for stmt in block {
            check_discard_reads_stmt(stmt, &mut diags);
        }
    }
    diags
}

/// Expansion reads (`$_`, `${_}`, `${_[0]}`, `@_`) plus arithmetic reads. A
/// bare `_` in `(( … ))` is an `ArithExpr::Var`, not an expansion, so the
/// expression walkers never surface it — it reached the taint backstop instead
/// of this check's diagnostic.
fn check_discard_reads_stmt(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    crate::ir::walk_all_exprs(stmt, &mut |e| flag_discard_read(e, diags));
    check_discard_arith_stmt(stmt, diags);
}

/// Bare `_` operands in every arithmetic tree owned by `stmt` or its children.
fn check_discard_arith_stmt(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    if let StmtKind::ArithTest(a) = &stmt.kind {
        flag_discard_arith(a, stmt.span, diags);
    }
    stmt.direct_exprs(&mut |e| {
        crate::ir::walk_expr_tree(e, &mut |sub| flag_discard_arith_expr(sub, diags))
    });
    for body in stmt.child_bodies() {
        for s in body {
            check_discard_arith_stmt(s, diags);
        }
    }
}

/// A `$(( … ))` in value position.
fn flag_discard_arith_expr(expr: &Expr, diags: &mut Vec<Diagnostic>) {
    if let ExprKind::Arithmetic(a) = &expr.kind {
        flag_discard_arith(a, expr.span, diags);
    }
}

/// Each bare variable read in an arithmetic tree.
fn flag_discard_arith(a: &crate::ir::ArithExpr, span: Span, diags: &mut Vec<Diagnostic>) {
    crate::ir::walk_arith_var_reads(a, &mut |name| {
        if crate::ir::is_discard_binder(name) {
            diags.push(discard_read_diag(span, name));
        }
    });
}

/// One expression: flag it when it reads `_` in any spelling.
fn flag_discard_read(expr: &Expr, diags: &mut Vec<Diagnostic>) {
    let Some(name) = discard_read_name(expr) else {
        return;
    };
    diags.push(discard_read_diag(expr.span, name));
}

/// The SB5056 diagnostic, shared by the expansion and arithmetic paths.
fn discard_read_diag(span: Span, name: &str) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5056",
        format!(
            "`{name}` is the don't-care binder — its value cannot be read. It \
             exists to bind and discard (so it is exempt from the dead-variable \
             gate and needs no attest); if you need the value, bind it to a \
             named variable instead"
        ),
    )
}

/// The name read by `expr`, when that read is of the discard binder.
fn discard_read_name(expr: &Expr) -> Option<&str> {
    let name = match &expr.kind {
        ExprKind::SimpleExpansion(v) => v.as_str(),
        ExprKind::ParameterExpansion(pe) => pe.name.as_str(),
        ExprKind::Subscript { name, .. } => name.as_str(),
        _ => return None,
    };
    crate::ir::is_discard_binder(name).then_some(name)
}

// ── SB5057 (DD#99): `readline` takes exactly one bare target, no flags ──
//
// `readline` is the line-terminated scalar read. It is a distinct construct,
// not a flag surface: the splitting forms are `read` (NUL fields into an
// array) and `map m = @{a:DISP}`. Keeping it flagless is the point of DD#99.

/// SB5057: reject a `readline` that is not exactly `readline NAME`.
pub fn check_readline_arity(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for block in program_blocks(program) {
        check_readline_in(block, &mut diags);
    }
    diags
}

fn check_readline_in(stmts: &[Statement], diags: &mut Vec<Diagnostic>) {
    for stmt in stmts {
        check_readline_stmt(stmt, diags);
    }
}

fn check_readline_stmt(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    if let StmtKind::Command(c) = &stmt.kind {
        let n = expr_literal_str(&c.name);
        if n == "readline" || n == "read" {
            check_one_readline(n, c, stmt.span, diags);
        }
    }
    for body in stmt.child_bodies() {
        check_readline_in(body, diags);
    }
}

/// One `readline` command: exactly one argument, a bare identifier.
fn check_one_readline(name: &str, c: &Command, span: Span, diags: &mut Vec<Diagnostic>) {
    let words: Vec<String> = c
        .args
        .iter()
        .map(expr_literal_str)
        .map(str::to_string)
        .collect();
    if let Some(flag) = words.iter().find(|w| w.starts_with('-')) {
        diags.push(readline_diag(
            span,
            &format!("`{name}` takes no flags — got `{flag}`"),
        ));
        return;
    }
    // `readline NAME`; `read [SRC] ARRAY` — the two-arg form splits a value
    // (DD#99). The TARGET is a binding, so it is always the bare last word.
    let max = if name == "read" { 2 } else { 1 };
    if words.is_empty() || words.len() > max {
        diags.push(readline_diag(
            span,
            &arity_complaint(name, max, words.len()),
        ));
        return;
    }
    if let Some(complaint) = sigil_complaint(name, &words) {
        diags.push(readline_diag(span, &complaint));
    }
}

/// DD#99's sigil rule at a read call: the TARGET is a binding (bare name), and
/// a two-arg `read`'s SOURCE is a value (sigilled). `read f g` is two
/// bindings and means nothing.
fn sigil_complaint(name: &str, words: &[String]) -> Option<String> {
    if !is_plain_identifier(words.last()?) {
        return Some(format!(
            "`{name}`'s target must be a bare name — it is a binding, not a value"
        ));
    }
    if words.len() == 2 && is_plain_identifier(&words[0]) {
        return Some(format!(
            "`{name}`'s source must be a value (`${}`), not a bare name — a bare \
             name is a binding",
            words[0]
        ));
    }
    None
}

/// The arity half of the SB5057 message.
fn arity_complaint(name: &str, max: usize, got: usize) -> String {
    let shape = if max == 2 {
        "a target name, optionally preceded by a source value"
    } else {
        "exactly one target name"
    };
    format!("`{name}` takes {shape} — got {got}")
}

fn readline_diag(span: Span, what: &str) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5057",
        format!(
            "{what}. `readline NAME` reads one line into one scalar and \
             `read ARRAY` reads NUL-separated fields into an array; for a \
             map use `map m = @{{a:DISP}}`"
        ),
    )
}

// ── SB5058 (DD#99): a map built from an array must state its disposition ──
//
// `map m = @{a:DISP}` consumes the array PAIRWISE. An odd count leaves a
// trailing element with no partner, and — exactly as DD#69 requires of an
// external-command splat whose array may be empty — the splat site must say
// what happens. A silently mis-paired map is the same shape of hazard as a
// silently-empty argument list.

/// SB5058: a `map m = @a` with no disposition.
pub fn check_map_from_array(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for block in program_blocks(program) {
        check_map_from_array_in(block, &mut diags);
    }
    diags
}

fn check_map_from_array_in(stmts: &[Statement], diags: &mut Vec<Diagnostic>) {
    for stmt in stmts {
        if let StmtKind::Declaration(d) = &stmt.kind {
            check_map_decl_splats(d, stmt.span, diags);
        }
        for body in stmt.child_bodies() {
            check_map_from_array_in(body, diags);
        }
    }
}

/// Each assignment of a `map` declaration whose value is a BARE array splat.
fn check_map_decl_splats(d: &Declaration, span: Span, diags: &mut Vec<Diagnostic>) {
    if d.flags.iter().any(|f| f == "-A") {
        check_map_target_splats(d, span, diags);
    } else {
        check_array_target_splats(d, span, diags);
    }
}

/// `map m = @a` — the supported direction, but the disposition is mandatory
/// (DD#99): a bare `@a` is SB5058.
fn check_map_target_splats(d: &Declaration, span: Span, diags: &mut Vec<Diagnostic>) {
    for a in &d.assignments {
        let Some(value) = &a.value else {
            continue;
        };
        if let Some(source) = bare_arrvals_splat_name(&value.kind) {
            diags.push(map_disposition_diag(span, source, &a.name));
        }
    }
}

/// DD#99 stage 8: `array a = @m` is the reverse direction and is not a thing.
/// It was ACCEPTED and silently produced an EMPTY array — the "gives an
/// undefined answer to input it accepted" shape. A map is built FROM an
/// array; flattening one back was never specified, and guessing a pair order
/// for the author is exactly the silent mis-pairing DD#69/DD#99 make you
/// state a disposition to avoid.
fn check_array_target_splats(d: &Declaration, span: Span, diags: &mut Vec<Diagnostic>) {
    for a in &d.assignments {
        let Some(source) = a.value.as_ref().and_then(any_splat_source) else {
            continue;
        };
        diags.push(array_from_map_diag(span, source, &a.name));
    }
}

/// The subject of ANY `@name` splat (with or without a disposition).
fn any_splat_source(value: &Expr) -> Option<&str> {
    let ExprKind::ParameterExpansion(pe) = &value.kind else {
        return None;
    };
    let op = pe.operator.as_deref()?;
    if op == "arrvals" || op.starts_with("arrvals_") {
        Some(pe.name.as_str())
    } else {
        None
    }
}

fn array_from_map_diag(span: Span, source: &str, target: &str) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5061",
        format!(
            "`array {target} = @{source}` — an array is not built from a splat \
             (DD#99). A map is built FROM an array (`map m = @{{a:DISP}}`); the \
             reverse is not defined, and this silently produced an empty \
             array. Build the elements directly, or iterate the map with \
             `for k in $(keys {source})`."
        ),
    )
}

fn map_disposition_diag(span: Span, source: &str, target: &str) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5058",
        format!(
            "`map {target} = @{source}` must state what happens to an odd \
             element count — the array is consumed pairwise. Use \
             `@{{{source}:-KEY}}` (the trailing element becomes the value of \
             KEY), `@{{{source}:?MSG}}` (abort), or `@{{{source}:~}}` (discard \
             it)"
        ),
    )
}

// ── SB5059 (DD#99): `echo -n` is retired — the spelling is `write` ──
//
// `echo` terminates its output, `write` does not; neither takes a flag. This
// is DD#60/#75's "distinct constructs over flags" applied to the last flag
// `echo` had. The transform still synthesises `echo -n` when desugaring
// `write`, which is fine: this check runs on the SOURCE, before the transform.

/// SB5059: reject a source-level `echo -n`.
pub fn check_echo_no_flags(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for block in program_blocks(program) {
        check_echo_flags_in(block, &mut diags);
    }
    diags
}

fn check_echo_flags_in(stmts: &[Statement], diags: &mut Vec<Diagnostic>) {
    for stmt in stmts {
        check_echo_flags_stmt(stmt, diags);
    }
}

fn check_echo_flags_stmt(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    if let StmtKind::Command(c) = &stmt.kind {
        if expr_literal_str(&c.name) == "echo" && expr_literal_str_at(&c.args, 0) == "-n" {
            diags.push(Diagnostic::error_span(
                stmt.span,
                "SB5059",
                "`echo -n` is retired — write `write` instead. `echo` ends its \
                 output with a newline and `write` does not; neither takes a flag"
                    .to_string(),
            ));
        }
    }
    for body in stmt.child_bodies() {
        check_echo_flags_in(body, diags);
    }
}

/// The literal text of `args[i]`, or "" when absent or non-literal.
fn expr_literal_str_at(args: &[Expr], i: usize) -> &str {
    args.get(i).map(expr_literal_str).unwrap_or("")
}

/// DD#63: `match S P` (boolean / `$( )` value) or `match S P ARR` (captures),
/// and `matchall S P ARR` (every match — the out-array is mandatory, since
/// "all matches" has no single value to be).
///
/// Any other count used to be accepted in silence: the 3-arg out-array
/// lowering declined the shape, the 2-arg boolean lowering picked it up and
/// ignored the surplus, and the command reported a SUCCESS it had not
/// checked — so `if match S P g h` took the then-branch with `g` empty.
pub fn check_match_arity(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for block in program_blocks(program) {
        check_match_arity_in(block, &mut diags);
    }
    diags
}

fn check_match_arity_in(stmts: &[Statement], diags: &mut Vec<Diagnostic>) {
    for stmt in stmts {
        check_match_arity_stmt(stmt, diags);
    }
}

fn check_match_arity_stmt(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    if let StmtKind::Command(c) = &stmt.kind {
        let name = expr_literal_str(&c.name);
        if let Some(d) = match_arity_diag(name, c.args.len(), stmt.span) {
            diags.push(d);
        }
        if let Some(d) = retired_bulk_read_diag(name, stmt.span) {
            diags.push(d);
        }
    }
    for body in stmt.child_bodies() {
        check_match_arity_in(body, diags);
    }
}

/// DD#99 stage 6: `mapfile` / `readarray` are retired. Point at the
/// replacement rather than letting them fall through to SB5011 "command not
/// found", which would send the author looking for a missing binary.
fn retired_bulk_read_diag(name: &str, span: Span) -> Option<Diagnostic> {
    if !matches!(name, "mapfile" | "readarray") {
        return None;
    }
    Some(Diagnostic::error_span(
        span,
        "SB5060",
        format!(
            "`{name}` is retired (DD#99). To split a file into lines use \
             `matchall $s \"[^\\x0a]+\" ARR` on its contents, or read \
             NUL-separated fields with `tr '\\n' '\\0' < f | read ARR`; \
             for a map, pair the array with `map m = @{{ARR:DISP}}`."
        ),
    ))
}

// ── SB4045/SB5085: the `argv [N] a` out-param form (DD#60) ───────────
//
// `argv` was the one fill builtin with no shape check. Its lowering
// (`transform::stmt_handlers::handle_argv`) returned `None` for any shape it
// could not lower, and `None` there means "not a builtin" — so the statement
// became an ordinary command named `argv`, compiled clean, and died at
// runtime with `sash: bug: bare command passed to sash_exec`. Eight spellings
// reached it, including `argv "1" a`: the index is read with `expr_to_string`,
// a *display* renderer that keeps the quotes, so `"1"` never parses as 1.
//
// The target is `flag_fill_target`'s (SB5063); this covers arity and index.

/// SB4045/SB5085: every `argv` statement has a lowerable shape.
pub fn check_argv_form(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for block in program_blocks(program) {
        check_argv_form_in(block, &mut diags);
    }
    diags
}

fn check_argv_form_in(stmts: &[Statement], diags: &mut Vec<Diagnostic>) {
    for stmt in stmts {
        check_argv_form_stmt(stmt, diags);
    }
}

fn check_argv_form_stmt(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    if let StmtKind::Command(c) = &stmt.kind {
        diags.extend(argv_form_diag(c, stmt.span));
    }
    for body in stmt.structural_bodies() {
        check_argv_form_in(body, diags);
    }
    // A substitution body takes the `$(argv)` splat exemption, so it needs
    // `check_argv_form_cmdsub` rather than the plain recursion.
    for body in stmt.cmdsub_bodies() {
        check_argv_form_cmdsub(body, diags);
    }
}

/// `argv` inside a `$( … )` body.
///
/// The splat `$(argv)` / `$(argv N)` is an **expression**, and `transform`
/// resolves it only when it is the whole substitution. As the sole statement
/// it is that expression, so it is exempt from the statement shape check;
/// anywhere else the splat spelling names no array and every statement around
/// it was being dropped, so it is SB5025.
///
/// `argv` cannot simply join `VALUE_ONLY_INTRINSICS` the way `keys` did —
/// statement position is exactly where DD#60's out-param fill lives, in a
/// `$( )` body as much as anywhere else. It is the splat *spelling* that is
/// not a statement.
fn check_argv_form_cmdsub(stmts: &[Statement], diags: &mut Vec<Diagnostic>) {
    let sole = cmdsub_is_argv_splat(stmts);
    for stmt in stmts {
        check_argv_form_cmdsub_stmt(stmt, sole, diags);
    }
}

/// One statement's worth of [`check_argv_form_cmdsub`]. `sole` is a property of
/// the whole body, so it is computed once and passed down.
fn check_argv_form_cmdsub_stmt(stmt: &Statement, sole: bool, diags: &mut Vec<Diagnostic>) {
    if !is_argv_splat_stmt(stmt) {
        check_argv_form_stmt(stmt, diags);
        return;
    }
    if !sole {
        diags.push(argv_splat_not_sole_diag(stmt.span));
    }
}

/// The sole statement of this `$( … )` body is the splat spelling
/// `argv` / `argv N`, so the body is an *expression* and the body's statements
/// are not statements at all.
///
/// Shared, because two passes need the same exemption and they disagreed once
/// already: `check_fill_target_in` read the `N` of `$(argv 1)` as a fill
/// target and rejected the canonical `for x in $(argv 1)` with SB5063. DD#60
/// gives the splat and the out-param fill the same spelling at one argument —
/// `is_argv_splat_stmt` is the discriminator, and every pass that walks into a
/// body has to consult it rather than re-deciding.
fn cmdsub_is_argv_splat(stmts: &[Statement]) -> bool {
    matches!(stmts, [stmt] if is_argv_splat_stmt(stmt))
}

/// A statement that is the splat spelling `argv` / `argv N`.
fn is_argv_splat_stmt(stmt: &Statement) -> bool {
    let Some(c) = stmt_command(stmt) else {
        return false;
    };
    expr_literal_str(&c.name) == "argv" && argv_command_is_splat(c)
}

/// SB5025 for an `argv` splat that is one statement among several.
fn argv_splat_not_sole_diag(span: Span) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5025",
        "the `$(argv)` / `$(argv N)` splat is an expression — it is \
         recognised only when it is the whole command substitution, not one \
         statement among several (the others would be silently discarded). \
         `argv a` / `argv N a` fills an array and is a statement anywhere"
            .to_string(),
    )
}

/// The complaint for one `argv` command, or `None` if its shape is lowerable.
fn argv_form_diag(c: &Command, span: Span) -> Option<Diagnostic> {
    if expr_literal_str(&c.name) != "argv" {
        return None;
    }
    match c.args.len() {
        1 => None,
        2 => argv_index_diag(&c.args[0], span),
        n => Some(Diagnostic::error_span(
            span,
            "SB4045",
            format!(
                "`argv` takes `a` (every argument) or `N a` (arguments from \
                 N) — got {n} argument(s)"
            ),
        )),
    }
}

/// The index of `argv N a` must be a **bare** non-negative decimal literal
/// that fits `i64`. `ArgvFill` carries a constant start, so a quoted (`"1"`),
/// sigilled (`$n`) or bracketed (`[1]`) spelling names no index at all — the
/// same rule, and the same reason, as SB5063 for the target.
///
/// A compile-time-constant start is the **rule**, not a gap awaiting a fix
/// (DG, 2026-09-22): both alternatives the message names were measured to
/// compile and run, and between them they cover what a runtime start would
/// buy. The advice is deliberately generic — it reads correctly both for the
/// user who merely over-quoted a literal and for the user who wanted `$n`.
fn argv_index_diag(index: &Expr, span: Span) -> Option<Diagnostic> {
    if is_bare_index_literal(index) {
        return None;
    }
    Some(Diagnostic::error_span(
        span,
        "SB5085",
        "`argv N a`'s index must be a bare non-negative integer literal that \
         fits a 64-bit integer — the fill start is fixed at compile time. For \
         a start that varies at run time, fill the whole array (`argv a`) and \
         index it (`${a[$n]}`), or loop over `for x in $(argv)` and skip the \
         ones you do not want"
            .to_string(),
    ))
}

/// A bare non-negative decimal literal that fits `i64` — the only spelling of
/// an `argv` start index. Shared by [`argv_index_diag`] (which rejects every
/// other spelling of the fill start) and [`argv_command_is_splat`] (which uses
/// it to tell `$(argv 1)` from `argv a`); they must agree, or a spelling one
/// rejects becomes an array name to the other.
fn is_bare_index_literal(index: &Expr) -> bool {
    let text = expr_literal_str(index);
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) && text.parse::<i64>().is_ok()
}

fn match_arity_diag(name: &str, n: usize, span: Span) -> Option<Diagnostic> {
    let complaint = match name {
        "match" if n != 2 && n != 3 => {
            format!("`match` takes `S P` (boolean) or `S P ARR` (captures) — got {n} arguments")
        }
        "matchall" if n != 3 => format!(
            "`matchall` takes `S P ARR` — got {n} arguments. The out-array is \
             required: every match is appended, so there is no single value to \
             return"
        ),
        _ => return None,
    };
    Some(Diagnostic::error_span(span, "SB4045", complaint))
}

/// SB5080 (DD#100 Part B): the collection-slot names `A`–`E` and the
/// function-argument array `args` are **reserved**, "rejected as user
/// variables at top level and inside functions". A `for` variable introduces a
/// user variable, but it is not a *declaration*, so neither the DD#86
/// shadowing gate (which DD#86 explicitly exempts for-variables from) nor the
/// SB5052 write gate saw it. Without this, `for A in @B { … }` inside a
/// function gives one name two simultaneous meanings: `$A` reads the loop
/// variable while `len A` / `join A` / `${A[0]}` read the slot.
pub fn check_reserved_loop_var(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for stmt in program
        .statements
        .iter()
        .chain(program.end_block.iter().flatten())
    {
        check_reserved_loop_var_stmt(stmt, &mut diags);
    }
    diags
}

fn check_reserved_loop_var_stmt(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    if let StmtKind::For(f) = &stmt.kind {
        diags.extend(reserved_loop_var_diag(&f.variable, stmt.span));
    }
    for body in stmt.child_bodies() {
        for s in body {
            check_reserved_loop_var_stmt(s, diags);
        }
    }
}

/// The SB5080 diagnostic for a `for` variable that takes a reserved name, or
/// `None` when the name is the user's to use.
fn reserved_loop_var_diag(name: &str, span: Span) -> Option<Diagnostic> {
    let msg = if is_collection_slot(name) {
        format!(
            "`{name}` is a reserved collection-slot name (DD#100) and cannot be a \
             `for` variable — inside a function `${name}` would read the loop \
             variable while `len {name}` read the slot. Rename the loop variable"
        )
    } else if name == "args" {
        "`args` is the reserved function-argument array (DD#92) and cannot be a \
         `for` variable — it is immutable. Rename the loop variable"
            .to_string()
    } else {
        return None;
    };
    Some(Diagnostic::error_span(span, "SB5080", msg))
}

/// SB5081: `break`/`continue` outside any enclosing loop. The transform lowers
/// both through `state.loop_stack`, and an empty stack makes them a **silent
/// no-op** — the statement compiles and does nothing. bash warns; sash rejects.
/// A function body starts a fresh context: a `break` lexically inside a
/// function is not inside a loop even when the call site is.
pub fn check_loop_control_context(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for stmt in program
        .statements
        .iter()
        .chain(program.end_block.iter().flatten())
    {
        check_loop_control_stmt(stmt, false, &mut diags);
    }
    diags
}

fn check_loop_control_stmt(stmt: &Statement, in_loop: bool, diags: &mut Vec<Diagnostic>) {
    match &stmt.kind {
        StmtKind::Command(c) => {
            diags.extend(loop_control_diag(c, stmt.span, in_loop));
            descend_loop_control(stmt, in_loop, diags);
        }
        // A loop body is a loop context; a function body resets it.
        StmtKind::For(_) | StmtKind::While(_) => descend_loop_control(stmt, true, diags),
        StmtKind::Function(f) => {
            for s in &f.body {
                check_loop_control_stmt(s, false, diags);
            }
        }
        _ => descend_loop_control(stmt, in_loop, diags),
    }
}

/// Recurse into every child body, carrying the loop context unchanged.
fn descend_loop_control(stmt: &Statement, in_loop: bool, diags: &mut Vec<Diagnostic>) {
    for body in stmt.child_bodies() {
        for s in body {
            check_loop_control_stmt(s, in_loop, diags);
        }
    }
}

/// The SB5081 diagnostic for a `break`/`continue` with no enclosing loop.
fn loop_control_diag(c: &Command, span: Span, in_loop: bool) -> Option<Diagnostic> {
    let name = expr_literal_str(&c.name);
    if in_loop || (name != "break" && name != "continue") {
        return None;
    }
    Some(Diagnostic::error_span(
        span,
        "SB5081",
        format!(
            "`{name}` is not inside a loop — it would compile to a silent \
             no-op. Remove it, or move it into a `for`/`while` body"
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(kind: ExprKind) -> Expr {
        Expr {
            kind,
            span: Span::new(1, 1),
        }
    }

    fn unary(var: &str) -> TestExpr {
        TestExpr::Unary {
            op: "-n".to_string(),
            operand: Box::new(e(ExprKind::SimpleExpansion(var.to_string()))),
        }
    }

    // `expr_uses_tainted` must stay total over `ExprKind` — a `_ => {}` there
    // leaked three times, once as a taint bypass that shipped — so it has a
    // `Test` arm. That arm is unreachable *by construction*: `ExprKind::Test`
    // is built only by `parse_test_command` and `parse_attest`, both of which
    // yield a *statement*, while every position this predicate is asked about
    // is an operand. `array a=( [[ -n $t ]] )` is SB4014 at parse,
    // `${h:-[[ -n x ]]}` is literal text (DD#90 rule 5), and
    // `array a=( $([[ -n $t ]]) )` is a `CommandSubstitution` the walker
    // deliberately does not descend into.
    //
    // The ICE is the claim, and asserting it is what keeps the claim honest.
    // `[[ ]]` operands ARE read positions — that is `check_test_taint`'s job,
    // and it stays hot — so if `[[ ]]` ever becomes an expression, whoever
    // makes it one has to write this arm knowingly, with specs, rather than
    // inherit an answer nothing ever ran.
    #[test]
    #[should_panic(expected = "ExprKind::Test reached expr_uses_tainted")]
    fn a_test_cannot_reach_the_collection_predicate() {
        expr_uses_tainted(
            &e(ExprKind::Test(unary("t"))),
            &HashSet::from(["t".to_string()]),
        );
    }
}
