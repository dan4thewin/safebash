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
//!      propagation → use-site rejection → bless removes taint
//!
//! SECURITY: This module + parse/ are the complete validation surface.
//! Together they guarantee: no tainted data reaches execution without
//! bless, no dynamic dispatch, no unknown commands, no rejected vars.
//!
//! Trust model:
//!   - Parser guarantees: only recognized syntax reaches IR
//!   - Validator guarantees: only safe *semantics* reach transform/codegen
//!   - If both pass with zero diagnostics, the program is in the safe subset

use crate::diagnostic::Diagnostic;
use crate::ir::*;

// ── Entry point ────────────────────────────────────────────────────

/// Run all validation rules on a parsed program.
/// Returns diagnostics for any violations found.
pub fn validate(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    // Top-level statements, plus any `__END__` block, as a single list of
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

    for &block in &blocks {
        for stmt in block {
            validate_stmt(stmt, &mut diags);
        }
        check_commands(block, &functions, &mut diags); // SB5011: command resolution
        check_dollar_question(block, &mut diags); // SB5012: $? availability
        check_regex_replacements(block, &mut diags); // SB4033: sub/gsub replacement escape
    }
    // SB4037 (DD#74): a function used as a predicate must `return` on every path
    check_predicate_definite_return(program, &functions, &mut diags);
    // SB4044: a `const`/`readonly` variable cannot be reassigned.
    check_const_reassignment(&blocks, &mut diags);
    // SB5049 (DD#86): no shadowing — a typed declaration inside a function of a
    // top-level global name is a redeclaration error.
    check_no_shadow(program, &mut diags);
    // SB4045: a fixed-arity value-intrinsic (`sub`, `len`, …) called via `$( )`
    // with the wrong number of arguments.
    for &block in &blocks {
        for stmt in block {
            walk_all_exprs(stmt, &mut |expr| {
                check_cmdsub_arity(expr, &mut diags);
                // SB4020 (bug #3): an assignment / `++` / `--` inside a
                // `$(( … ))` value can't take effect (the value path has no
                // side effects) — direct to a standalone `(( … ))` statement.
                check_arith_value_mutation(expr, &mut diags);
            });
            // SB4020 (bug #3): a `(( … ))` statement / condition whose embedded
            // mutation the transform can't hoist (`++`/`--`, `a[i]`, or under
            // `&&`/`||`/`?:`).
            check_arith_test_placement(stmt, &mut diags);
        }
    }
    diags
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
        StmtKind::Declaration(d) => scan_const_decl(d, stmt.span, consts, diags),
        StmtKind::Assignment(a) => flag_if_const(consts, &a.name, stmt.span, diags),
        _ => {}
    }
    for body in stmt.child_bodies() {
        scan_const_stmts(body, consts, diags);
    }
}

/// A declaration reassigns a const if it re-declares/re-inits an already-const
/// name; a `const`/`readonly` declaration records its names.
fn scan_const_decl(
    d: &Declaration,
    span: Span,
    consts: &mut std::collections::HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    for a in &d.assignments {
        flag_if_const(consts, &a.name, span, diags);
    }
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
        StmtKind::Declaration(d) => {
            // DD#70/#71: a typed declaration is a value binding, not a
            // boolean/exit-status — it cannot be a condition.
            if in_condition {
                let is_capture = d.assignments.iter().any(|a| value_is_capture(&a.value));
                diags.push(nonstatus_in_condition_diag(
                    stmt.span,
                    "a declaration",
                    is_capture,
                ));
            }
            validate_declaration(d, stmt.span, diags)
        }
        StmtKind::Assignment(a) => validate_assignment_stmt(a, stmt.span, in_condition, diags),
        StmtKind::If(i) => validate_if(i, diags),
        StmtKind::While(w) => validate_while(w, diags),
        StmtKind::For(f) => validate_for(f, diags),
        StmtKind::Group(g) => validate_group(g, in_condition, diags),
        StmtKind::ArithTest(expr) => {
            // SB5007: check arithmetic in context
            validate_arith_context(expr, stmt.span, in_condition, diags);
        }
        // Wrappers (List, Pipeline, Subshell, Suppressed, Redirected) and
        // reset-context constructs (Function, Case) — see `validate_nested`.
        _ => validate_nested(stmt, in_condition, diags),
    }
    // Validate all expressions in the statement
    walk_exprs_in_stmt(stmt, &mut |expr, span| validate_expr(expr, span, diags));
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
            for body in stmt.child_bodies() {
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
    if in_condition && matches!(&a.value.kind, ExprKind::Arithmetic(_)) {
        diags.push(Diagnostic::error_span(
            span,
            "SB5007",
            format!(
                "mutation {} = ... not allowed in condition (( )) expression context — \
                 use (( )) statement instead",
                a.name
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
/// name, so this structurally excludes `$(user_fn)` / `$(getenv X)` (which are
/// NOT capture-conditions). A pipeline / multi-statement / concat RHS is also
/// excluded (a single external command only).
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
        StmtKind::Command(c) => is_external_command_head(c),
        // `$(a | b)` — every stage must be an external command.
        StmtKind::Pipeline(p) => {
            p.commands.len() > 1 && p.commands.iter().all(stmt_is_external_command)
        }
        _ => false,
    }
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
        format!(
            "{what} binds the captured output, not the command's exit status — and sequencing \
             won't help (a failed `$(…)` aborts before any check). To branch on the command, run \
             it in the condition without capturing: `if ! cmd {{ … }}`. Capturing output *and* \
             branching on that command's status isn't currently supported."
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
    for s in &w.condition {
        validate_stmt_ctx(s, diags, true);
    }
    for s in &w.body {
        validate_stmt_ctx(s, diags, false);
    }
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
    check_test_constructs(name, c, span, diags);
    check_inline_glob(c, diags);
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
                "`{}` is rejected — sash forbids dynamic command execution",
                name
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

/// SB5019/SB5023: numeric test ops + literal regex validation inside `[[ ]]`.
fn check_test_constructs(name: &str, c: &Command, span: Span, diags: &mut Vec<Diagnostic>) {
    if name != "[[" {
        return;
    }
    for arg in &c.args {
        if let ExprKind::Test(ref test) = arg.kind {
            check_unsupported_test_ops(test, diags);
            check_regex_literal(test, span, diags);
        }
    }
}

fn validate_declaration(d: &Declaration, span: Span, diags: &mut Vec<Diagnostic>) {
    // SB5001: export/declare -x
    if matches!(d.keyword, DeclKeyword::Export) {
        diags.push(Diagnostic::error_span(
            span,
            "SB5001",
            "sash has no `export` — use `/usr/bin/env NAME=value …` to set a \
             variable for a command"
                .to_string(),
        ));
    }
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

/// SB4026: reject an out-of-range decimal literal assigned to an int var.
fn check_int_literal_range(d: &Declaration, diags: &mut Vec<Diagnostic>) {
    for a in &d.assignments {
        let ExprKind::Literal(s) = &a.value.kind else {
            continue;
        };
        if int_literal_out_of_range(s) {
            diags.push(Diagnostic::error_span(
                a.value.span,
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
fn reject_flat_map_literal(a: &Assignment, diags: &mut Vec<Diagnostic>) {
    let ExprKind::Array(elems) = &a.value.kind else {
        return;
    };
    if elems.is_empty() {
        return;
    }
    diags.push(Diagnostic::error_span(
        a.value.span,
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
        ExprKind::Literal(s) => {
            // SB5006: ** globstar is always rejected (even quoted, it's never valid)
            if s.contains("**") {
                diags.push(Diagnostic::error_span(
                    expr.span,
                    "SB5006",
                    format!("sash has only `*` globs — `**` is omitted: {}", s),
                ));
            }
        }
        _ => {}
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
/// not implement — codegen (tests.rs) handles only -n/-z/-i/-t and the file
/// tests. These three must be rejected here (SB5028) before codegen ICEs on
/// them. Returns the user-facing hint for the operator, or None if supported.
///   -v  DD#62: an unset/empty variable is just the empty string
///   -o  shell-option state — sash has no shell options
///   -R  nameref state — sash has no namerefs
fn unsupported_unary_test_hint(op: &str) -> Option<&'static str> {
    match op {
        "-v" => Some("use -n to test for a non-empty value"),
        "-o" => Some("shell-option tests are unsupported"),
        "-R" => Some("namerefs are unsupported"),
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

    // Prefix ++/-- always rejected
    if let ArithExpr::PreIncDec { var, .. } = expr {
        diags.push(Diagnostic::error_span(
            span,
            "SB5007",
            format!(
                "prefix ++{} is not allowed — use postfix ({}++) or (( {} += 1 )) instead",
                var, var, var
            ),
        ));
        return;
    }

    let kind = classify_arith_expr(expr);
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
    if arith_has_mutation(expr) {
        let desc = arith_brief(expr);
        diags.push(Diagnostic::error_span(
            span,
            "SB5007",
            format!(
                "mutation {} not allowed in condition (( )) expression context — \
                 use (( )) statement instead",
                desc
            ),
        ));
    }
}

/// DD#31 comma expression in condition context: every non-final
/// part must be a mutation; the final part must be a comparison
/// (not a mutation). Empty comma lists are no-ops.
fn validate_arith_comma_in_condition(exprs: &[ArithExpr], span: Span, diags: &mut Vec<Diagnostic>) {
    use crate::parse::{classify_arith_expr, ArithKind};

    let Some((last, init)) = exprs.split_last() else {
        return;
    };
    for e in init {
        if classify_arith_expr(e) != ArithKind::Mutation {
            let desc = arith_brief(e);
            diags.push(Diagnostic::error_span(
                span,
                "SB5007",
                format!("non-final comma expression {} must be a mutation", desc),
            ));
        }
    }
    if classify_arith_expr(last) == ArithKind::Mutation {
        let desc = arith_brief(last);
        diags.push(Diagnostic::error_span(
            span,
            "SB5007",
            format!(
                "final comma expression {} must be a comparison in condition context",
                desc
            ),
        ));
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
        let desc = arith_brief(expr);
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

/// The directly-nested arith sub-expressions of `a`, for generic recursion.
fn arith_children(a: &ArithExpr) -> Vec<&ArithExpr> {
    match a {
        ArithExpr::BinOp { left, right, .. } => vec![left, right],
        ArithExpr::UnaryOp { operand, .. } => vec![operand],
        ArithExpr::Paren(inner) => vec![inner],
        ArithExpr::Comma(es) => es.iter().collect(),
        ArithExpr::Ternary {
            cond,
            true_val,
            false_val,
        } => vec![cond, true_val, false_val],
        ArithExpr::Assign { value, .. } | ArithExpr::CompoundAssign { value, .. } => vec![value],
        ArithExpr::SubscriptAssign { index, value, .. } => vec![index, value],
        ArithExpr::FuncCall { args, .. } => args.iter().collect(),
        ArithExpr::Subscript { index, .. } => vec![index],
        _ => vec![],
    }
}

fn arith_has_incdec(a: &ArithExpr) -> bool {
    matches!(
        a,
        ArithExpr::PostIncDec { .. } | ArithExpr::PreIncDec { .. }
    ) || arith_children(a).into_iter().any(arith_has_incdec)
}

fn arith_has_subscript_assign(a: &ArithExpr) -> bool {
    matches!(a, ArithExpr::SubscriptAssign { .. })
        || arith_children(a)
            .into_iter()
            .any(arith_has_subscript_assign)
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
        _ => arith_children(a).into_iter().any(arith_mutation_under_cond),
    }
}

fn arith_brief(expr: &ArithExpr) -> String {
    match expr {
        ArithExpr::Assign { var, .. } => format!("{} = ...", var),
        ArithExpr::CompoundAssign { var, op, .. } => {
            let op_s = match op {
                IrArithOp::Add => "+=",
                IrArithOp::Sub => "-=",
                _ => "?=",
            };
            format!("{} {}", var, op_s)
        }
        ArithExpr::PostIncDec { var, inc } => {
            format!("{}{}", var, if *inc { "++" } else { "--" })
        }
        ArithExpr::PreIncDec { var, inc } => {
            format!("{}{}", if *inc { "++" } else { "--" }, var)
        }
        ArithExpr::BinOp { left, op, right } => {
            format!(
                "{} {} {}",
                arith_brief(left),
                match op {
                    IrArithOp::Gt => ">",
                    IrArithOp::Lt => "<",
                    IrArithOp::Eq => "==",
                    IrArithOp::Ne => "!=",
                    IrArithOp::Ge => ">=",
                    IrArithOp::Le => "<=",
                    _ => "?",
                },
                arith_brief(right)
            )
        }
        ArithExpr::Var(name) => name.clone(),
        ArithExpr::Literal(n) => n.to_string(),
        _ => "...".to_string(),
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
fn check_dollar_question(stmts: &[Statement], diags: &mut Vec<Diagnostic>) {
    let mut state = StatusState::Undefined;
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

/// A function body is checked in a fresh `Undefined` status
/// context (the caller's $? state does not flow in).
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
    find_dollar_question(stmt, &mut spans);
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

fn find_dollar_question(stmt: &Statement, spans: &mut Vec<Span>) {
    walk_exprs_in_stmt(stmt, &mut |expr, _| {
        if let ExprKind::SimpleExpansion(var) = &expr.kind {
            if var == "?" {
                spans.push(expr.span);
            }
        }
    });
}

fn stmt_uses_status(stmt: &Statement) -> bool {
    let mut found = false;
    walk_exprs_in_stmt(stmt, &mut |expr, _| {
        if let ExprKind::SimpleExpansion(var) = &expr.kind {
            if var == "?" {
                found = true;
            }
        }
    });
    found
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
    // Control flow
    ":",
    "exit",
    "return",
    "break",
    "continue",
    // Input
    "read",
    "mapfile",
    // Taint
    "bless",
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
    // Introspection
    "status",
    "getpid",
    "getppid",
    "getcpid",
    "getcwd",
    "epochseconds",
    // Conversion
    "dtoi",
    "itod",
    // Regex (DD#63): match is a boolean command (sub/gsub are value-only)
    "match",
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
            mark_predicate_operand(&l.left, funcs, out);
            mark_predicate_operand(&l.right, funcs, out);
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

fn check_cmd_in_stmt(stmt: &Statement, funcs: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    let StmtKind::Command(c) = &stmt.kind else {
        for body in stmt.child_bodies() {
            check_commands(body, funcs, diags);
        }
        return;
    };
    // DD#23: a command name must be resolvable at compile time. A variable or
    // other expansion (`$x foo`) is a runtime-chosen command — reject it here
    // rather than letting it reach `sash_exec` at runtime.
    if !matches!(&c.name.kind, ExprKind::Literal(_)) {
        diags.push(Diagnostic::error_span(
            c.name.span,
            "SB5011",
            "a command name must be a literal path — a variable or expansion \
             can't be resolved at compile time; use the literal path, or \
             `/usr/bin/env` for a runtime tool"
                .to_string(),
        ));
        return;
    }
    let name = expr_literal_str(&c.name);
    // DD#14: `exec` replaces the running program (or manipulates its fds). Its
    // target — the first argument, for the replace form — must be resolvable at
    // compile time exactly like a command head; the fd-only form (`exec >file`)
    // has no target. Validate the target here (the `exec` head itself is a
    // BUILTIN, so it passes the resolvability check below).
    if name == "exec" {
        check_exec_target(c, diags);
        return;
    }
    if name.is_empty() || is_valid_command(name, funcs) {
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
//   - bless explicitly removes taint after validation
//
// SECURITY: This is definition-based, NOT name-based. There is no
// "clean list" of variable names. Taint flows through assignment.

use std::collections::HashSet;

/// Run taint analysis: track which variables hold tainted data,
/// reject any use of tainted variables outside bless expressions.
pub fn check_taint(program: &Program) -> Vec<Diagnostic> {
    let mut tainted: HashSet<String> = HashSet::new();
    let mut diags = Vec::new();
    for stmt in &program.statements {
        taint_stmt(stmt, &mut tainted, &mut diags);
    }
    if let Some(ref end) = program.end_block {
        for stmt in end {
            taint_stmt(stmt, &mut tainted, &mut diags);
        }
    }
    diags
}

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
        for a in &c.args {
            check_elide_expr(a, true, diags);
        }
    } else {
        stmt.direct_exprs(&mut |e| check_elide_expr(e, false, diags));
    }
    for body in stmt.child_bodies() {
        for s in body {
            check_elide_stmt(s, diags);
        }
    }
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
        ExprKind::CommandSubstitution(stmts) => check_elide_stmt_seq(stmts, diags),
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

fn check_elide_pexp(
    pe: &ParameterExpansion,
    span: Span,
    elidable: bool,
    diags: &mut Vec<Diagnostic>,
) {
    if pe.operator.as_deref() == Some(":~") && !elidable {
        diags.push(Diagnostic::error_span(
            span,
            "SB5046",
            format!(
                "`${{{}:~}}` (elide) is only valid in a list position — a command \
                 argument or an array literal element — where the token can \
                 disappear. Here a value is required; use `${{{}:-DEFAULT}}` for \
                 an empty/default value.",
                pe.name, pe.name
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
        ExprKind::CommandSubstitution(stmts) => check_positionals_stmt_seq(stmts, diags),
        _ => {}
    }
}

/// DD#46: positionals reached via `$#`/`$1`/`$@` inside `(( ))` / `$(( ))`
/// are still function-local. Walk the arith tree and flag positional
/// `DollarVar`s (bare `Var`/lvalue names are not positionals).
fn check_positionals_arith(a: &crate::ir::ArithExpr, span: Span, diags: &mut Vec<Diagnostic>) {
    use crate::ir::ArithExpr::*;
    match a {
        DollarVar(name) => flag_positional(name, span, diags),
        Expansion(inner) => check_positionals_expr(inner, diags),
        BinOp { left, right, .. } => {
            check_positionals_arith(left, span, diags);
            check_positionals_arith(right, span, diags);
        }
        UnaryOp { operand, .. } => check_positionals_arith(operand, span, diags),
        Assign { value, .. } | CompoundAssign { value, .. } => {
            check_positionals_arith(value, span, diags)
        }
        Ternary {
            cond,
            true_val,
            false_val,
        } => {
            check_positionals_arith(cond, span, diags);
            check_positionals_arith(true_val, span, diags);
            check_positionals_arith(false_val, span, diags);
        }
        Comma(exprs) | FuncCall { args: exprs, .. } => {
            for e in exprs {
                check_positionals_arith(e, span, diags);
            }
        }
        Paren(inner) => check_positionals_arith(inner, span, diags),
        Subscript { index, .. } => check_positionals_arith(index, span, diags),
        SubscriptAssign { index, value, .. } => {
            check_positionals_arith(index, span, diags);
            check_positionals_arith(value, span, diags);
        }
        Literal(_) | Var(_) | PostIncDec { .. } | PreIncDec { .. } => {}
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

/// A command substitution at top level runs in top-level context; recurse
/// at the statement level so nested function bodies are correctly scoped.
fn check_positionals_stmt_seq(stmts: &[Statement], diags: &mut Vec<Diagnostic>) {
    for s in stmts {
        check_positionals_stmt(s, false, diags);
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
    for stmt in &program.statements {
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
        // A `$( … )` runs in the enclosing arg scope — inherit `in_fn`.
        ExprKind::CommandSubstitution(stmts) => {
            for s in stmts {
                dollar_at_stmt(s, in_fn, diags);
            }
        }
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
/// operands are gathered by `arith_operands` so this stays a three-way match.
fn dollar_at_arith(a: &crate::ir::ArithExpr, span: Span, in_fn: bool, diags: &mut Vec<Diagnostic>) {
    use crate::ir::ArithExpr::*;
    match a {
        DollarVar(name) if name == "@" || name == "*" => dollar_at_diag(span, name, in_fn, diags),
        Expansion(inner) => dollar_at_expr(inner, in_fn, diags),
        _ => {
            for child in arith_operands(a) {
                dollar_at_arith(child, span, in_fn, diags);
            }
        }
    }
}

/// The `ArithExpr` sub-operands of `a` (an `Expansion`'s inner `Expr` is
/// handled by the caller). A flat dispatcher — mirrors
/// `transform::functions::arith_children`.
fn arith_operands(a: &crate::ir::ArithExpr) -> Vec<&crate::ir::ArithExpr> {
    use crate::ir::ArithExpr::*;
    match a {
        BinOp { left, right, .. } => vec![left, right],
        UnaryOp { operand, .. } | Paren(operand) => vec![operand],
        Assign { value, .. } | CompoundAssign { value, .. } => vec![value],
        Ternary {
            cond,
            true_val,
            false_val,
        } => vec![cond, true_val, false_val],
        Comma(parts) | FuncCall { args: parts, .. } => parts.iter().collect(),
        Subscript { index, .. } => vec![index],
        SubscriptAssign { index, value, .. } => vec![index, value],
        _ => vec![],
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
        _ => {
            stmt.direct_exprs(&mut |e| check_argv_splat_expr(e, diags));
            for body in stmt.child_bodies() {
                check_argv_splat_body(body, diags);
            }
        }
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
fn check_external_splat(c: &Command, diags: &mut Vec<Diagnostic>) {
    // Post-resolution (DD#23) an external command's head is an absolute path;
    // builtins, user functions and intrinsics keep bare names and are exempt
    // (DD#69: the rule is external-only).
    if !expr_literal_str(&c.name).starts_with('/') {
        return;
    }
    for arg in &c.args {
        if is_bare_arrvals_splat(&arg.kind) {
            diags.push(Diagnostic::error_span(
                arg.span,
                "SB5032",
                "a bare `@a` array splat into an external command is unsafe — an \
                 empty array would run it with too few arguments. State the empty \
                 case: `@{a:-WORD}` (empty → the single arg WORD) or `@{a:?WORD}` \
                 (empty → abort with WORD)"
                    .to_string(),
            ));
        }
    }
}

/// A bare `@a` / single-part `"${a[@]}"` splat (operator `arrvals`), i.e.
/// WITHOUT a `@{a:-…}` / `@{a:?…}` disposition.
fn is_bare_arrvals_splat(kind: &ExprKind) -> bool {
    match kind {
        ExprKind::ParameterExpansion(pe) => pe.operator.as_deref() == Some("arrvals"),
        ExprKind::DoubleQuoted(parts) if parts.len() == 1 => is_bare_arrvals_splat(&parts[0].kind),
        _ => false,
    }
}

/// DD#60 revision: `array a=$(argv …)` is no longer a snapshot — a command
/// substitution does not yield an array. Reject it (directing to the
/// out-param `argv [N] a`); every other assignment value (and subscript
/// index) is checked for splat misuse.
fn check_argv_splat_decl(d: &Declaration, diags: &mut Vec<Diagnostic>) {
    let is_array_decl = d.flags.iter().any(|fl| fl == "-a" || fl == "-A");
    for a in &d.assignments {
        if is_array_decl && is_argv_splat_expr(&a.value) {
            diags.push(Diagnostic::error_span(
                a.value.span,
                "SB5025",
                "`array a=$(argv …)` is not an array snapshot — a command \
                 substitution does not yield an array; use the out-param form \
                 `argv [N] a`"
                    .to_string(),
            ));
            continue;
        }
        check_argv_splat_expr(&a.value, diags);
        check_argv_splat_opt(a.index.as_ref(), diags);
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
         argument or a for-in source; use `$(join @a)` for a joined string, \
         or `${a[i]}` for a single element"
            .to_string(),
    )
}

/// A dispositioned array splat `@{a:-WORD}` / `@{a:?WORD}` (operator
/// `arrvals_default` / `arrvals_abort`) — distinct from the bare `@a`
/// (`arrvals`), which is a valid for-in source.
fn is_disposition_splat(expr: &Expr) -> bool {
    matches!(&expr.kind, ExprKind::ParameterExpansion(pe)
        if matches!(pe.operator.as_deref(), Some("arrvals_default") | Some("arrvals_abort")))
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

/// SB5026 (quoted splat) or SB5025 (bare argv splat in a non-splat context)
/// for `expr` itself — `None` if `expr` is not a splat leaf.
fn argv_splat_self_diag(expr: &Expr) -> Option<Diagnostic> {
    if let ExprKind::DoubleQuoted(parts) = &expr.kind {
        if parts.len() == 1 && is_splat_form(&parts[0]) {
            return Some(quoted_splat_diag(expr.span));
        }
    }
    if is_argv_splat_expr(expr) {
        return Some(Diagnostic::error_span(
            expr.span,
            "SB5025",
            "$(argv) is an array splat — it is valid only as the sole source of \
             a for-in loop; snapshot into an array with `argv [N] a`, or use \
             $(arg N) for a single argument"
                .to_string(),
        ));
    }
    if is_keys_splat_expr(expr) {
        return Some(Diagnostic::error_span(
            expr.span,
            "SB5025",
            "`$(keys m)` is an array splat — it is valid only as the sole source \
             of a for-in loop (`for k in $(keys m)`); it cannot be joined into a \
             scalar or passed as a command argument"
                .to_string(),
        ));
    }
    None
}

/// Walk an expression, flagging quoted splats (SB5026) and bare argv splats
/// in a non-splat context (SB5025).
fn check_argv_splat_expr(expr: &Expr, diags: &mut Vec<Diagnostic>) {
    if let Some(diag) = argv_splat_self_diag(expr) {
        diags.push(diag);
        return;
    }
    match &expr.kind {
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) | ExprKind::Array(parts) => {
            check_argv_splat_seq(parts, diags)
        }
        ExprKind::AssocArray(pairs) => check_argv_splat_seq(pairs.iter().map(|(_, v)| v), diags),
        ExprKind::Subscript { index, .. } => check_argv_splat_expr(index, diags),
        ExprKind::ParameterExpansion(pe) => check_argv_splat_opt(pe.operand.as_deref(), diags),
        ExprKind::Test(t) => check_argv_splat_test(t, diags),
        ExprKind::CommandSubstitution(stmts) => check_argv_splat_stmt_seq(stmts, diags),
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

fn check_argv_splat_opt(expr: Option<&Expr>, diags: &mut Vec<Diagnostic>) {
    if let Some(e) = expr {
        check_argv_splat_expr(e, diags);
    }
}

fn check_argv_splat_stmt_seq(stmts: &[Statement], diags: &mut Vec<Diagnostic>) {
    for s in stmts {
        check_argv_splat_stmt(s, diags);
    }
}

fn check_argv_splat_test(t: &TestExpr, diags: &mut Vec<Diagnostic>) {
    match t {
        TestExpr::Unary { operand, .. } => check_argv_splat_expr(operand, diags),
        TestExpr::Binary { left, right, .. } => {
            check_argv_splat_expr(left, diags);
            check_argv_splat_expr(right, diags);
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
fn is_argv_splat_expr(expr: &Expr) -> bool {
    matches!(&expr.kind, ExprKind::CommandSubstitution(stmts) if cmdsub_command_is(stmts, "argv"))
}

/// `$(keys m)` — the map-keys array-splat command substitution. Like
/// `$(argv)`, it is legal only as a for-in sole source; SB5025 elsewhere
/// (DD#75 §3 — closes the memory-unsafe silent-join / garbage bypass where
/// keys slipped past the argv-only context guard).
fn is_keys_splat_expr(expr: &Expr) -> bool {
    matches!(&expr.kind, ExprKind::CommandSubstitution(stmts) if cmdsub_command_is(stmts, "keys"))
}

/// An array-returning command-sub splat legal as a for-in source:
/// `$(argv …)` or `$(keys m)`.
fn is_array_splat_source(expr: &Expr) -> bool {
    is_argv_splat_expr(expr) || is_keys_splat_expr(expr)
}

/// DD#69 §3: `echo` join-and-prints its arguments, so a named-array splat
/// there would flatten the array (DD#22) and reintroduce word-splitting
/// downstream. Reject `echo @a` with SB5030, directing to the explicit
/// `echo $(join @a)`. External commands keep the structured argv splat.
fn check_echo_splat(c: &Command, diags: &mut Vec<Diagnostic>) {
    let ExprKind::Literal(name) = &c.name.kind else {
        return;
    };
    if name != "echo" {
        return;
    }
    for arg in &c.args {
        if is_named_array_splat(arg) {
            diags.push(Diagnostic::error_span(
                arg.span,
                "SB5030",
                "`echo` joins its arguments into one output stream, so an array \
                 splat there would flatten the array. Use an explicit \
                 join: `echo $(join @a)`"
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

/// A bare splat form: `$(argv …)`, an `arrvals`/`arrkeys` parameter
/// expansion (`@a` / `${a[@]}` / `$a[@]`), or `$(keys a)`.
fn is_splat_form(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::CommandSubstitution(stmts) => {
            cmdsub_command_is(stmts, "argv") || cmdsub_command_is(stmts, "keys")
        }
        ExprKind::ParameterExpansion(pe) => is_arrvals_splat_op(pe.operator.as_deref()),
        _ => false,
    }
}

/// A named-array value/key splat operator: `arrvals` / `arrkeys`, or any
/// dispositioned / per-element `arrvals_*` form — `@{a:?}`, `@{a:-WORD}`,
/// `@{a^^}`, etc. (the parser encodes all of these as `arrvals_<op>`).
/// Recognizing the `arrvals_*` family here is what keeps a dispositioned
/// splat from slipping past the splat guards (SB5030 / SB5026) into a
/// scalar value lowering (which ICEs).
fn is_arrvals_splat_op(op: Option<&str>) -> bool {
    matches!(op, Some("arrvals") | Some("arrkeys"))
        || matches!(op, Some(s) if s.starts_with("arrvals_"))
}

/// True if the single-command substitution invokes `name`.
fn cmdsub_command_is(stmts: &[Statement], name: &str) -> bool {
    let inner = match stmts.first().map(|s| &s.kind) {
        Some(StmtKind::Redirected(r)) => &r.body.kind,
        Some(other) => other,
        None => return false,
    };
    matches!(inner, StmtKind::Command(c) if expr_literal_str(&c.name) == name)
}

/// Like taint_stmt but doesn't mark read/mapfile as tainted
/// (for pipeline stages where input comes from sash code).
fn taint_stmt_no_read_taint(
    stmt: &Statement,
    tainted: &mut HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    if let StmtKind::Command(c) = &stmt.kind {
        let name = expr_literal_str(&c.name);
        // read/mapfile from clean pipe source: don't taint
        if name == "read" || name == "mapfile" {
            check_clean_read_taint(c, tainted, diags);
            return;
        }
    }
    taint_stmt(stmt, tainted, diags);
}

/// `read`/`mapfile` from a clean pipe source: check the name+args for taint but
/// do not taint the read targets.
fn check_clean_read_taint(c: &Command, tainted: &mut HashSet<String>, diags: &mut Vec<Diagnostic>) {
    check_expr_taint(&c.name, tainted, diags);
    for arg in &c.args {
        check_expr_taint(arg, tainted, diags);
    }
}

/// Check if a pipeline stage is an external source (execs).
fn is_external_source(stmt: &Statement) -> bool {
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

fn taint_stmt(stmt: &Statement, tainted: &mut HashSet<String>, diags: &mut Vec<Diagnostic>) {
    match &stmt.kind {
        StmtKind::Assignment(a) => taint_assignment(a, tainted, diags),
        StmtKind::Declaration(d) => {
            for a in &d.assignments {
                taint_assignment(a, tainted, diags);
            }
        }
        StmtKind::Command(c) => taint_command(c, stmt.span, tainted, diags),
        StmtKind::Function(f) => taint_function_body(&f.body, tainted, diags),
        StmtKind::Pipeline(p) => taint_pipeline(p, tainted, diags),
        StmtKind::Redirected(r) => taint_redirected(r, tainted, diags),
        StmtKind::Case(c) => taint_case_arms(c, tainted, diags),
        StmtKind::For(f) => taint_for(f, tainted, diags),
        _ => {
            for body in stmt.child_bodies() {
                for s in body {
                    taint_stmt(s, tainted, diags);
                }
            }
        }
    }
}

/// DD#60/DD#61: track the taint of a for-loop variable.
///
/// - A direct tainted **source** splat (`for x in $(argv)`) binds each
///   element to the loop var *from the source* — assignment-from-source, so
///   the loop var is tainted and a per-iteration body `bless x` is valid
///   (DD#60 Q5). Seeding the taint here makes an unblessed body use a clean
///   SB5020 instead of an SSA-backstop ICE.
/// - Iterating a stored tainted **array** (`for x in @a` where `a` is
///   tainted) is a read of a stored variable — DD#61 rejects it; the array
///   must be bless-all'd first. Emitted as SB5020 directing to bless-all.
/// - A clean source rebinds the loop var to clean values (it is removed
///   from the tainted set, e.g. `for x in a b c`).
fn taint_for(f: &For, tainted: &mut HashSet<String>, diags: &mut Vec<Diagnostic>) {
    for w in &f.words {
        reject_tainted_array_iter(w, tainted, diags);
        // `for k in $(keys m)` etc.: a tainted read *inside* the iterated
        // command substitution is a use site (e.g. keys of an unblessed map).
        if let ExprKind::CommandSubstitution(stmts) = &w.kind {
            check_cmdsub_use_taint(stmts, w.span, tainted, diags);
        }
    }
    if f.words.iter().any(is_taint_source) {
        tainted.insert(f.variable.clone());
    } else {
        tainted.remove(&f.variable);
    }
    for s in &f.body {
        taint_stmt(s, tainted, diags);
    }
}

/// DD#61: `for x in @a` where `a` is a stored tainted array is a read of a
/// stored variable — rejected; the array must be bless-all'd first.
fn reject_tainted_array_iter(w: &Expr, tainted: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    let Some(arr) = splat_array_name(w) else {
        return;
    };
    if tainted.contains(arr) {
        diags.push(Diagnostic::error_span(
            w.span,
            "SB5020",
            format!(
                "iterating tainted array '{arr}' without bless — bless-all \
                 it first: `bless x in {arr} [[ … ]]`"
            ),
        ));
    }
}

/// The named array of a value/key splat for-word (`@a` / `${a[@]}` /
/// `${!a[@]}`), if the word is one.
fn splat_array_name(w: &Expr) -> Option<&str> {
    if let ExprKind::ParameterExpansion(pe) = &w.kind {
        if matches!(pe.operator.as_deref(), Some("arrvals") | Some("arrkeys")) {
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
fn taint_assignment(a: &Assignment, tainted: &mut HashSet<String>, diags: &mut Vec<Diagnostic>) {
    if taint_array_literal_assignment(a, tainted, diags) {
        return;
    }
    // `a[i]=value` — subscript assignment into an array slot.
    if a.index.is_some() {
        reject_tainted_into_array(&a.value, &a.name, tainted, diags);
        return;
    }
    // Scalar (or string `+=`) assignment.
    // DD#24 (journal 036 "No propagation"): there is NO taint flow inside the
    // program. Reading a stored tainted variable in the RHS is a rejected read
    // (`x=$1` is a compile error), uniform with every other read site. The
    // sanctioned way to obtain external data is assignment *from a source*
    // (`z=$(getenv X)`), which taints the target instead of being rejected.
    if is_taint_source(&a.value) {
        tainted.insert(a.name.clone());
    }
    reject_scalar_rhs_read(&a.value, tainted, diags);
    // DD#60/DD#78: an `$(arg N DEFAULT)` default (a read position) anywhere in
    // the RHS is checked for a tainted read — the surrounding source assignment
    // is allowed, so this walk finds the nested default the value walk skips.
    check_arg_default_taint(&a.value, tainted, diags);
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
                 propagation; `bless {var} [[ … ]]` at the source, then read it"
            ),
            diags,
        ),
        // `${y}` / `${x:-$y}` — the name read, plus (DD#78) the `:-` default as a
        // read position (rejects a tainted var or a tainted source there).
        ExprKind::ParameterExpansion(pe) => {
            check_param_expansion_taint(pe, expr.span, tainted, diags)
        }
        ExprKind::Subscript { name, .. } => push_taint_use(
            tainted,
            name,
            expr.span,
            format!(
                "tainted array ${{{name}[…]}} can't be copied — DD#24 has no \
                 taint propagation; bless it first"
            ),
            diags,
        ),
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
        _ => {}
    }
}

/// DD#61: an array/assoc-array literal RHS rejects tainted elements (external
/// data enters arrays only via read/mapfile). Returns true if `a`'s RHS was such
/// a literal (and was handled).
fn taint_array_literal_assignment(
    a: &Assignment,
    tainted: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    match &a.value.kind {
        ExprKind::Array(elems) => {
            for e in elems {
                reject_tainted_into_array(e, &a.name, tainted, diags);
            }
            true
        }
        ExprKind::AssocArray(pairs) => {
            for (_k, v) in pairs {
                reject_tainted_into_array(v, &a.name, tainted, diags);
            }
            true
        }
        _ => false,
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

/// DD#61: reject a tainted value being placed into an array element.
fn reject_tainted_into_array(
    e: &Expr,
    name: &str,
    tainted: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    if is_taint_source(e) || expr_uses_tainted(e, tainted) {
        diags.push(Diagnostic::error_span(
            e.span,
            "SB5021",
            format!(
                "tainted value cannot be placed in array '{}' — external data enters \
                 arrays via read/mapfile; or bless a scalar first and assign the clean value",
                name
            ),
        ));
    }
    // A read of already-tainted data inside a `$( )` element (`a[i]=$(join @b)`,
    // `array a=($(join @b))`) is a use-site (SB5020) — `expr_uses_tainted`
    // above doesn't look inside command substitutions.
    check_value_cmdsub_reads(e, tainted, diags);
}

/// Taint a function body. Positional params ($1-$9, $@) are clean within
/// a function (the caller's args are statically visible — DD#46); they are
/// never added to the tainted set, so the body simply inherits the outer
/// tainted set unchanged.
fn taint_function_body(body: &[Statement], tainted: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    let mut fn_tainted = tainted.clone();
    for s in body {
        taint_stmt(s, &mut fn_tainted, diags);
    }
}

/// Taint a pipeline: if first stage is external, subsequent reads are tainted.
fn taint_pipeline(
    p: &crate::ir::Pipeline,
    tainted: &mut HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    let first_is_external = p.commands.first().is_some_and(is_external_source);
    for (i, s) in p.commands.iter().enumerate() {
        if i > 0 && !first_is_external {
            taint_stmt_no_read_taint(s, tainted, diags);
        } else {
            taint_stmt(s, tainted, diags);
        }
    }
}

/// Taint a redirected statement. A redirect target is a read position, so a
/// tainted target (`echo > $t`) is checked like any other read.
fn taint_redirected(
    r: &crate::ir::Redirected,
    tainted: &mut HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    for rd in &r.redirects {
        check_expr_taint(&rd.target, tainted, diags);
    }
    taint_stmt(&r.body, tainted, diags);
}

/// Taint a case statement's discriminant and each arm body.
fn taint_case_arms(
    c: &crate::ir::Case,
    tainted: &mut HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    check_expr_taint(&c.value, tainted, diags);
    for item in &c.items {
        for s in &item.body {
            taint_stmt(s, tainted, diags);
        }
    }
}

/// Taint analysis for a command statement: read/mapfile/bless + general taint checking.
fn taint_command(
    c: &Command,
    span: Span,
    tainted: &mut HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    let name = expr_literal_str(&c.name);

    if name == "read" || name == "mapfile" || name == "argv" {
        taint_read_target(c, tainted);
    } else if name == "bless" {
        taint_bless(c, span, tainted, diags);
        return; // Don't check args of bless for taint
    }

    // Check all command arguments for tainted variable use
    check_expr_taint(&c.name, tainted, diags);
    for arg in &c.args {
        check_expr_taint(arg, tainted, diags);
    }
}

/// `read VAR` / `mapfile VAR`: mark the rightmost non-flag argument
/// as the variable being assigned, so it becomes tainted.
fn taint_read_target(c: &Command, tainted: &mut HashSet<String>) {
    for arg in c.args.iter().rev() {
        let s = expr_literal_str(arg);
        if !s.starts_with('-') && !s.is_empty() {
            tainted.insert(s.to_string());
            return;
        }
    }
}

/// `bless` taint handling for all three forms (DD#24 scalar, DD#61
/// indexed array and map). Removes taint from the blessed entity if the
/// predicate validates the required name(s); otherwise emits SB5020.
///
/// - Scalar `bless v [[…]]`: predicate must reference `$v` (on the LHS).
/// - Indexed `bless x in a [[…]]`: predicate must reference the binder
///   `$x`; clears taint on `a`.
/// - Map `bless k v in m [[…]]`: predicate must reference BOTH binders
///   `$k` and `$v` (a key escaping unvalidated would be unsound); clears
///   taint on `m`.
fn taint_bless(
    c: &Command,
    span: Span,
    tainted: &mut HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    let Some((form, test)) = classify_bless(&c.args) else {
        return;
    };
    // Each form reduces to: the binder name(s) the predicate must validate,
    // the name whose taint clears on success, and the diagnostic otherwise.
    let (binders, clear, msg): (Vec<&str>, &str, String) = match form {
        BlessForm::Scalar { var } => (
            vec![var],
            var,
            format!("bless condition must validate ${var} — condition doesn't reference the variable"),
        ),
        BlessForm::Indexed { binder, array } => (
            vec![binder],
            array,
            format!("bless-all condition must validate each element ${binder} on the left of the test"),
        ),
        BlessForm::Map { key, val, map } => (
            vec![key, val],
            map,
            format!("map bless-all must validate both the key ${key} and value ${val} on the left of a test"),
        ),
    };
    if binders.iter().all(|b| test_references_var(test, b)) {
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
fn is_taint_source(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::CommandSubstitution(stmts) => cmdsub_is_taint_source(stmts),
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) | ExprKind::Array(parts) => {
            parts.iter().any(is_taint_source)
        }
        ExprKind::AssocArray(pairs) => pairs.iter().any(|(_, v)| is_taint_source(v)),
        _ => false,
    }
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
    stmt.child_bodies()
        .iter()
        .any(|body| body.iter().any(stmt_runs_taint_source))
}

fn command_name_is_taint_source(name_expr: &Expr) -> bool {
    let name = expr_literal_str(name_expr);
    name == "getenv" || name == "argv" || name == "arg" || name.starts_with('/')
}

/// A command substitution that yields a tainted **scalar** used directly
/// at a read site (command argument, quoted interpolation) is rejected by
/// the use-site taint check — DD#24 admits only assignment-from-source or
/// a `bless` predicate. This covers `$(getenv …)`, `$(arg N)`, and
/// external `$(/abs …)`. `$(argv …)` is deliberately excluded: it is an
/// array splat governed by the splat-context check (SB5025), not a scalar.
fn cmdsub_is_scalar_taint_source(stmts: &[Statement]) -> bool {
    // A lone `$(argv …)` is an array splat (SB5025 elsewhere), not a scalar
    // source — don't double-diagnose it. Any other body is a scalar source iff
    // it runs an external / getenv / arg / argv anywhere (recursive scan, so a
    // multi-statement / pipeline body is covered — was `stmts.first()` only).
    if cmdsub_command_is(stmts, "argv") {
        return false;
    }
    cmdsub_is_taint_source(stmts)
}

/// Check if an expression uses any tainted variable.
fn expr_uses_tainted(expr: &Expr, tainted: &HashSet<String>) -> bool {
    match &expr.kind {
        ExprKind::SimpleExpansion(var) => tainted.contains(var.as_str()),
        ExprKind::ParameterExpansion(pe) => {
            // DD#78: a `:-` default is part of the expansion's value, so a
            // tainted read inside it taints the result too (correct
            // propagation — without this, `z=${x:-$tainted}` left `z` clean).
            tainted.contains(pe.name.as_str())
                || pe
                    .operand
                    .as_deref()
                    .is_some_and(|o| expr_uses_tainted(o, tainted))
        }
        ExprKind::Subscript { name, .. } => tainted.contains(name.as_str()),
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) | ExprKind::Array(parts) => {
            parts.iter().any(|p| expr_uses_tainted(p, tainted))
        }
        ExprKind::AssocArray(pairs) => pairs.iter().any(|(_, v)| expr_uses_tainted(v, tainted)),
        _ => false,
    }
}

/// Check an expression for tainted variable use, emitting diagnostics.
fn check_expr_taint(expr: &Expr, tainted: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    match &expr.kind {
        ExprKind::SimpleExpansion(var) => push_taint_use(
            tainted,
            var,
            expr.span,
            format!("tainted variable ${} used without bless", var),
            diags,
        ),
        ExprKind::ParameterExpansion(pe) => {
            check_param_expansion_taint(pe, expr.span, tainted, diags)
        }
        ExprKind::Subscript { name, .. } => push_taint_use(
            tainted,
            name,
            expr.span,
            format!("tainted array ${{{}[...]}} used without bless", name),
            diags,
        ),
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) => {
            for p in parts {
                check_expr_taint(p, tainted, diags);
            }
        }
        // DD#24: a command substitution at a read site is checked two ways:
        //   1. a bare scalar source ($(getenv …)/$(arg N)) used directly is
        //      a use of external data without bless — rejected.
        //   2. reads *inside* the substitution (e.g. `$(join @a)` where `a`
        //      is tainted) are themselves use-sites — recurse into the inner
        //      command args. Without this, `echo $(join @a)` slips through
        //      the CST pass (silent escape or SSA-backstop ICE).
        ExprKind::CommandSubstitution(stmts) => {
            check_cmdsub_use_taint(stmts, expr.span, tainted, diags)
        }
        // `[[ … ]]` test operands are read positions — a tainted variable or a
        // tainted read inside a `$( )` operand is a use without bless. These
        // were previously unchecked (the arm was missing).
        ExprKind::Test(t) => check_test_taint(t, tainted, diags),
        _ => {}
    }
}

/// Taint-check the operands of a `[[ … ]]` test — each is a read position.
fn check_test_taint(t: &TestExpr, tainted: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    match t {
        TestExpr::Unary { operand, .. } => check_expr_taint(operand, tainted, diags),
        TestExpr::Binary { left, right, .. } => {
            check_expr_taint(left, tainted, diags);
            check_expr_taint(right, tainted, diags);
        }
        TestExpr::Not(inner) => check_test_taint(inner, tainted, diags),
        TestExpr::And(a, b) | TestExpr::Or(a, b) => {
            check_test_taint(a, tainted, diags);
            check_test_taint(b, tainted, diags);
        }
    }
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
/// without bless, like `echo $tainted`. (`:?` and the pattern operators carry a
/// *literal* operand, DD#65, which is clean by construction.)
fn check_param_expansion_taint(
    pe: &ParameterExpansion,
    span: Span,
    tainted: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    push_taint_use(
        tainted,
        &pe.name,
        span,
        format!("tainted variable ${{{} ...}} used without bless", pe.name),
        diags,
    );
    if pe.operator.as_deref() == Some(":-") {
        if let Some(operand) = &pe.operand {
            check_expr_taint(operand, tainted, diags);
        }
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
        _ => {}
    }
}

/// A collection reader that exposes tainted *element data* via a bare
/// argument literal: `keys m` (a map's keys) or `join a` (the joined element
/// values). Per DD#24 these are reads of tainted elements — if the collection
/// is tainted (e.g. an `argv`/`mapfile` snapshot without bless-all) this is a
/// use without bless (SB5020). The name is a bare literal, so the generic
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
            "join of tainted collection '{name}' without bless — bless-all \
             it first: `bless … in {name} [[ … ]]`"
        ),
        "keys" => format!(
            "keys of tainted map '{name}' without bless — bless-all \
             it first: `bless k v in {name} [[ … ]]`"
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
            "tainted command substitution used without bless — assign it \
             to a variable and bless before use"
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
            StmtKind::Command(c) => collect_read_targets(c, set),
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

/// `read x`, `read -a a`, `mapfile -t a`, `readarray a` introduce their
/// non-flag literal argument names. DD#60 `argv [N] ARR` snapshots the argv
/// (slice) into the array named by its LAST argument.
fn collect_read_targets(c: &Command, set: &mut HashSet<String>) {
    let name = expr_literal_str(&c.name);
    match name {
        "read" => collect_read_flag_targets(&c.args, set),
        "mapfile" | "readarray" => {
            for arg in &c.args {
                insert_read_target_arg(arg, set);
            }
        }
        "argv" => {
            if let Some(ExprKind::Literal(s)) = c.args.last().map(|a| &a.kind) {
                set.insert(s.clone());
            }
        }
        _ => {}
    }
}

/// `read x`/`read -a ARR`: the shared classifier's scalar var names plus the
/// `-a` array-target name. The classifier skips `-d`/`-f` VALUES (a delim like
/// `:` is not a target).
fn collect_read_flag_targets(args: &[Expr], set: &mut HashSet<String>) {
    let f = crate::ir::classify_read_flags(args);
    set.extend(f.var_names);
    if let Some(a) = f.array_name {
        set.insert(a);
    }
}

/// A non-flag literal argument to `read`/`mapfile`/`readarray` names a
/// variable the command introduces.
fn insert_read_target_arg(arg: &Expr, set: &mut HashSet<String>) {
    if let ExprKind::Literal(s) = &arg.kind {
        if !s.starts_with('-') {
            set.insert(s.clone());
        }
    }
}

/// A plausible user identifier — excludes punctuation specials ($?, $$,
/// $!, $#, $@, $*, $-) and positionals ($1…) by construction.
fn is_plain_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Names handled by other rules (their own diagnostics) — skip so the typo
/// check never double-diagnoses.
fn handled_elsewhere(name: &str) -> bool {
    ENV_VARS.contains(&name) || REJECTED_VARS.iter().any(|(n, _)| *n == name)
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
    let mut sink = ReadSink::Flag(&mut diags);
    check_reads_in_scope(&program.statements, &globals, &mut sink);
    if let Some(ref end) = program.end_block {
        check_reads_in_scope(end, &globals, &mut sink);
    }
    diags
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
    check_dead_globals(program, &mut diags);
    check_dead_locals(program, &mut diags);
    diags
}

/// A top-level (global) write is dead iff it is read nowhere in the whole
/// program (a global may be read from any function).
fn check_dead_globals(program: &Program, diags: &mut Vec<Diagnostic>) {
    let mut reads = HashSet::new();
    let mut writes = Vec::new();
    for block in program_blocks(program) {
        collect_reads(block, &mut reads);
        collect_writes(block, &mut writes);
    }
    report_dead(&writes, |n| !reads.contains(n), diags);
}

/// Each function-local write (a name that is not a top-level global) is dead
/// iff it is read nowhere within that function.
fn check_dead_locals(program: &Program, diags: &mut Vec<Diagnostic>) {
    let mut globals = HashSet::new();
    for block in program_blocks(program) {
        collect_scope_names(block, &mut globals);
    }
    for block in program_blocks(program) {
        check_dead_in_functions(block, &globals, diags);
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
        if is_dead(name) && reported.insert(name.clone()) {
            diags.push(dead_var_diag(*span, name));
        }
    }
}

/// Find each function body and flag its dead function-locals.
fn check_dead_in_functions(
    stmts: &[Statement],
    globals: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    for stmt in stmts {
        if let StmtKind::Function(f) = &stmt.kind {
            check_fn_dead(&f.body, globals, diags);
        }
        for body in stmt.child_bodies() {
            check_dead_in_functions(body, globals, diags);
        }
    }
}

/// A write in a function to a non-global name is dead iff it is read nowhere in
/// the function body.
fn check_fn_dead(body: &[Statement], globals: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    let mut fn_reads = HashSet::new();
    collect_reads(body, &mut fn_reads);
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
    let mut sink = ReadSink::Collect(set);
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

/// `$(join …)` arity: a collection and an optional separator. `$(join)` (0 args)
/// previously reached the transform and **panicked the compiler**
/// (`ICE: join requires at least one argument`); more than 2 args silently
/// dropped the extras. Reject both here (SB5040) so the transform's arity is
/// guaranteed.
pub fn check_join_arity(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for stmt in program
        .statements
        .iter()
        .chain(program.end_block.iter().flatten())
    {
        crate::ir::walk_all_exprs(stmt, &mut |e| flag_join_arity(e, &mut diags));
    }
    diags
}

fn flag_join_arity(expr: &Expr, diags: &mut Vec<Diagnostic>) {
    let ExprKind::CommandSubstitution(stmts) = &expr.kind else {
        return;
    };
    let Some(StmtKind::Command(c)) = stmts.first().map(|s| &s.kind) else {
        return;
    };
    if expr_literal_str(&c.name) != "join" {
        return;
    }
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
    let ExprKind::CommandSubstitution(stmts) = &expr.kind else {
        return;
    };
    let Some(StmtKind::Command(c)) = stmts.first().map(|s| &s.kind) else {
        return;
    };
    if expr_literal_str(&c.name) != "len" {
        return;
    }
    let Some(name) = len_operand_name(c) else {
        return; // subscript cell / expansion / cmdsub operand — resolved elsewhere
    };
    // `args` is the implicit array: valid inside a function; a top-level
    // `$(len args)` is the function-local reject (SB5024) — check_positionals
    // only sees the `@args`/`${args…}` expansion forms, not this bare operand.
    if name == "args" {
        if !in_function {
            flag_args_toplevel("args", expr.span, diags);
        }
        return;
    }
    if known.contains(&name) || handled_elsewhere(&name) {
        return;
    }
    diags.push(Diagnostic::error_span(
        expr.span,
        "SB5039",
        format!(
            "`len {name}` — `{name}` is introduced nowhere (not a variable, array, \
             map, loop variable, or `args`); this reads as 0. Declare it, or fix the typo"
        ),
    ));
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
enum ReadSink<'a> {
    /// SB5031: flag a read whose name is not in the current scope's `known`.
    Flag(&'a mut Vec<Diagnostic>),
    /// SB5050: collect every read name (scope-agnostic superset).
    Collect(&'a mut HashSet<String>),
}

impl ReadSink<'_> {
    fn record(&mut self, name: &str, span: Span, known: &HashSet<String>) {
        self.record_param(name, span, known, false);
    }

    /// Record a name only for dead-var (Collect mode); a no-op for SB5031
    /// (Flag mode). Used where the name might be a variable but is not a read
    /// SB5031 should flag — e.g. a DD#58 arith sexpr `(name …)`, whose name may
    /// be a variable (`(x)`), a user function (`(f)`), or an intrinsic
    /// (`(random)`, `(len m)`); only the first is a variable read, and SB5031
    /// must not flag the intrinsic/function names.
    fn collect_only(&mut self, name: &str) {
        if let ReadSink::Collect(set) = self {
            set.insert(name.to_string());
        }
    }

    /// Record a variable read. `opt_out` marks a parameter expansion carrying an
    /// operator (`${x:-…}`, `@a`, `${x:s/…/…/}`): SB5031 treats it as the
    /// explicit "intentional default / transform" opt-out and does NOT flag it,
    /// but dead-var still counts it as a read of the variable.
    fn record_param(&mut self, name: &str, span: Span, known: &HashSet<String>, opt_out: bool) {
        match self {
            ReadSink::Collect(set) => {
                set.insert(name.to_string());
            }
            ReadSink::Flag(diags) => {
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
            // `bless` with a decodable form is handled specially (below);
            // a non-bless / malformed one falls through to the default arm.
            StmtKind::Command(c) if try_check_bless_stmt(c, known, sink) => {}
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
    check_reads_in_scope(&f.body, &fn_known, sink);
}

/// `bless ARR in VAR [[ …$VAR… ]]` (and the map form) introduce
/// predicate-local binder variable(s): check the test with them added,
/// and don't treat the binder/array literals as reads. Returns `true`
/// when the statement was a decodable bless (and has been checked).
fn try_check_bless_stmt(c: &Command, known: &HashSet<String>, sink: &mut ReadSink) -> bool {
    if expr_literal_str(&c.name) != "bless" {
        return false;
    }
    let Some((form, test)) = crate::ir::classify_bless(&c.args) else {
        return false;
    };
    let mut bless_known = known.clone();
    match form {
        crate::ir::BlessForm::Indexed { binder, array } => {
            bless_known.insert(binder.to_string());
            // The array being blessed is read (bless iterates it) — count it for
            // dead-var; SB5031 handles an undeclared bless array elsewhere.
            sink.collect_only(array);
        }
        crate::ir::BlessForm::Map { key, val, map } => {
            bless_known.insert(key.to_string());
            bless_known.insert(val.to_string());
            sink.collect_only(map);
        }
        crate::ir::BlessForm::Scalar { .. } => {}
    }
    check_test_reads(test, &bless_known, sink);
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
    for body in stmt.child_bodies() {
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
        // intrinsic (`(random)`, `(len m)`). Only the first is a variable read;
        // collect it for dead-var but never flag it for SB5031 (the intrinsic /
        // function names are not undeclared reads).
        FuncCall { name, args } => {
            sink.collect_only(name);
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
    sink.record_param(&pe.name, span, known, pe.operator.is_some());
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
        crate::ir::walk_all_exprs(stmt, &mut |e| flag_malformed_sed(e, &mut diags));
    }
    diags
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

/// DD#78: a disposition default is evaluated **eagerly** (`emit_default_value`
/// selects between both arms), so it must be side-effect-free. `$(random)` is
/// *clean* (a compiler intrinsic) but **stateful** — it advances the PRNG
/// whether or not the fallback is taken. Reject it in a `:-` default or an
/// `$(arg N DEFAULT)` default (SB5035). Also enforce `arg`'s arity here (index +
/// optional default). Tainted / forking defaults are caught as read-position
/// uses by `check_taint`; `random` is the only clean-but-stateful intrinsic, so
/// it needs this dedicated structural check.
pub fn check_disposition_defaults(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for stmt in program
        .statements
        .iter()
        .chain(program.end_block.iter().flatten())
    {
        crate::ir::walk_all_exprs(stmt, &mut |e| {
            flag_random_default(e, &mut diags);
            flag_arg_builtin(e, &mut diags);
        });
    }
    diags
}

fn flag_random_default(e: &Expr, diags: &mut Vec<Diagnostic>) {
    let ExprKind::ParameterExpansion(pe) = &e.kind else {
        return;
    };
    if pe.operator.as_deref() != Some(":-") {
        return;
    }
    let Some(operand) = &pe.operand else {
        return;
    };
    if expr_invokes_random(operand) {
        diags.push(random_default_diag(e.span));
    }
}

/// DD#60 addendum: `$(arg …)` takes an index and an optional default
/// (`$(arg N)` or `$(arg N DEFAULT)`) — reject other arities. The default, if
/// present, is DD#78-constrained: reject a `$(random)` default (side effect
/// under eager eval; a tainted default is caught by `check_taint`).
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
        if expr_invokes_random(default) {
            diags.push(random_default_diag(default.span));
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
    None
}

/// A statically-negative integer literal (`-1`, `-42`) — the index form DD#60
/// rejects at compile time. `-` followed by ASCII digits, nothing else.
fn is_negative_int_literal(e: &Expr) -> bool {
    let s = expr_literal_str(e);
    matches!(s.strip_prefix('-'), Some(rest) if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()))
}

fn random_default_diag(span: Span) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5035",
        "a disposition default must be side-effect-free — `$(random)` advances \
         the PRNG and the default is evaluated eagerly; capture it into a \
         variable first and use the clean value"
            .to_string(),
    )
}

/// Does `expr` contain a `$(random)` command substitution?
fn expr_invokes_random(expr: &Expr) -> bool {
    let mut found = false;
    crate::ir::walk_expr_tree(expr, &mut |e| {
        if cmdsub_head_is(e, "random") {
            found = true;
        }
    });
    found
}

/// True if `e` is a `$(NAME …)` command substitution whose head literal is `name`.
fn cmdsub_head_is(e: &Expr, name: &str) -> bool {
    let ExprKind::CommandSubstitution(stmts) = &e.kind else {
        return false;
    };
    let Some(StmtKind::Command(c)) = stmts.first().map(|s| &s.kind) else {
        return false;
    };
    expr_literal_str(&c.name) == name
}
