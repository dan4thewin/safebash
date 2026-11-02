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
use std::collections::{BTreeMap, HashMap};
use std::ops::RangeInclusive;

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

/// SB4044: a `const`/`readonly` variable cannot be reassigned, through any
/// channel.
///
/// Runs in the semantic batch, over the program `bindings::resolve` returns,
/// where every local already carries its scope's qualified name. One
/// program-wide set of consts is therefore exact: a function's `const x` says
/// nothing about another function's `x` or a sibling capture's, and a write
/// is caught whether it comes before or after the declaration in the text
/// (DD#86: a binding does not depend on textual order). Hence two walks — the
/// second sees every const.
///
/// `VersionState::define` asserts what this establishes; it is not a second
/// way to report it.
pub fn check_const_writes(program: &Program) -> Vec<Diagnostic> {
    let mut consts = HashSet::new();
    for block in program_blocks(program) {
        walk_stmt_tree(block, &mut |s| collect_const_names(s, &mut consts));
    }
    let mut diags = Vec::new();
    for block in program_blocks(program) {
        walk_stmt_tree(block, &mut |s| flag_const_writes(s, &consts, &mut diags));
    }
    diags
}

fn collect_const_names(stmt: &Statement, consts: &mut HashSet<String>) {
    if let StmtKind::Declaration(d) = &stmt.kind {
        if d.keyword == DeclKeyword::Readonly {
            consts.extend(d.assignments.iter().map(|a| a.name.clone()));
        }
    }
}

/// One SB4044 per const that `stmt` binds, at `stmt` — the innermost
/// statement that writes it. Sorted, so a statement that binds two reports
/// them in a fixed order.
fn flag_const_writes(stmt: &Statement, consts: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    let mut hits = std::collections::BTreeSet::new();
    stmt_bound_names(stmt, &mut |name| {
        if consts.contains(name) {
            hits.insert(name.to_string());
        }
    });
    for name in hits {
        diags.push(const_reassign_diag(stmt.span, &name));
    }
}

/// Every name `stmt` itself binds — not those of the statements nested in it,
/// which a statement walk reaches on its own.
///
/// Statement-level arithmetic (`(( x=5 ))`, `(( x++ ))`) is an `Assignment` by
/// the time it gets here, and an arithmetic *test* that writes is SB5007's or
/// SB4020's, so an `ArithTest` normally binds nothing; it still answers. A
/// declaration is the binding itself, and a second one is SB5049's
/// redeclaration. Two channels are left out because every const is a scalar
/// (the parser has no `const array`/`const map`, and SB4064 refuses a
/// collection value), and type_check rejects both on a scalar in either
/// textual order: `unset` and the collection an `attest … in a` re-tags
/// (SB4030 each).
///
/// Total over `StmtKind`; do not add a wildcard.
fn stmt_bound_names(stmt: &Statement, f: &mut impl FnMut(&str)) {
    match &stmt.kind {
        StmtKind::Assignment(a) => f(&a.name),
        StmtKind::For(fr) => f(&fr.variable),
        StmtKind::ArithTest(e) => walk_arith_var_writes(e, f),
        StmtKind::Command(c) => {
            let mut names = HashSet::new();
            command_bound_names(c, &mut names);
            names.extend(attest_retag_target(c).map(str::to_string));
            for name in &names {
                f(name);
            }
        }
        StmtKind::Declaration(_)
        | StmtKind::Pipeline(_)
        | StmtKind::If(_)
        | StmtKind::While(_)
        | StmtKind::Case(_)
        | StmtKind::Function(_)
        | StmtKind::Redirected(_)
        | StmtKind::List(_)
        | StmtKind::Group(_)
        | StmtKind::Suppressed(_)
        | StmtKind::Negated(_)
        | StmtKind::ReturnProducer(_) => {}
    }
}

/// The scalar an `attest` re-tags: `v` in `attest v [[ … ]]`, and SOURCE in
/// DD#93's `attest SOURCE (( TARGET = SOURCE ))`. Each binds a new version of
/// the name, carrying the verdict. TARGET is [`attest_convert_target`]'s.
fn attest_retag_target(c: &Command) -> Option<&str> {
    if expr_literal_str(&c.name) != "attest" {
        return None;
    }
    match classify_attest(&c.args)?.0 {
        AttestForm::Scalar { var } => Some(var),
        AttestForm::Indexed { .. } | AttestForm::Map { .. } => None,
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
/// The rule also keeps lowering sound: `lower_while` lowers a body twice (the
/// first pass is discarded) and the static `for` unrolls one, so a `const` in
/// either would be defined twice, which `VersionState::define` asserts never
/// happens.
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
/// and gets a fresh map. Captures inherit the enclosing bindings for the
/// no-shadow rule, but never export their own local introductions.
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
        StmtKind::ArithTest(e) => note_arith_bindings(e, stmt.span, seen, bound),
        _ => {}
    }
    for body in stmt.structural_bodies() {
        scan_scope_redecl(body, seen, bound, diags);
    }
    for body in stmt.cmdsub_bodies() {
        scan_scope_redecl(body, &mut seen.clone(), &mut bound.clone(), diags);
    }
}

fn note_arith_bindings(
    e: &ArithExpr,
    span: Span,
    seen: &mut HashMap<String, Span>,
    bound: &mut HashMap<String, Span>,
) {
    crate::ir::walk_arith_var_writes(e, &mut |name| note_binding(name, span, seen, bound));
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
    let Some(c) = crate::ir::single_value_command(stmts) else {
        return;
    };
    let ExprKind::Literal(name) = &c.name.kind else {
        return;
    };
    let Some(arity) = crate::ssa::value_intrinsic_arity(name) else {
        return;
    };
    if !arity.contains(&c.args.len()) {
        diags.push(cmdsub_arity_diag(name, &arity, c.args.len(), c.name.span));
    }
}

/// The SB4045 text for a value intrinsic called inside `$( )`.
///
/// Two names are both a value intrinsic and a legal statement, and the generic
/// "exact arity" sentence denies the second form exists for either:
///
/// - `$(match S P)` captures the matched text; `match S P ARR` fills `ARR`.
/// - `$(keys m)` splats the keys; `keys m k` snapshots them into `k` (DD#75).
///
/// For `match` the misdirection was costly: the repair a reader infers from
/// the generic text is to consume the status (`|| :`), which **compiles** —
/// `single_value_command` sees a list, not a command, so this check never
/// runs — and captures the empty string silently. So each gets text steering
/// to the repair that is actually right.
///
/// The rest (`sub`, `gsub`, `len`, `strlen`) are value-only — a bare statement
/// draws SB5073 — so the generic sentence is accurate for them.
fn cmdsub_arity_diag(
    name: &str,
    arity: &RangeInclusive<usize>,
    got: usize,
    span: Span,
) -> Diagnostic {
    if name == "keys" && got == 2 {
        return Diagnostic::error_span(
            span,
            "SB4045",
            "`keys` in `$( )` is the splat form and takes `m` — got 2 \
             arguments. `keys m k` is the statement form: it snapshots the \
             keys into array `k` and produces no output, so `$( )` would \
             capture the empty string. Run it as its own statement, then use \
             `k`"
            .to_string(),
        );
    }
    if name == "match" && got == 3 {
        return Diagnostic::error_span(
            span,
            "SB4045",
            "`match` in `$( )` is the capturing form and takes `S P` — got 3 \
             arguments. `match S P ARR` is the statement form: it fills `ARR` and \
             yields a status, so `$( )` would capture the empty string. Drop `ARR` \
             to capture the matched text, or use the statement form outside `$( )`"
                .to_string(),
        );
    }
    Diagnostic::error_span(
        span,
        "SB4045",
        format!(
            "`{name}` expects {} but got {got} — a value intrinsic must be \
             called with its exact arity",
            arity_phrase(arity),
        ),
    )
}

/// "1 argument" / "0 or 1 arguments" / "3 arguments" — the permitted-count
/// half of the SB4045 sentence, shared by the `$( )` and sexpr spellings so
/// one mistake cannot be described two ways.
fn arity_phrase(arity: &RangeInclusive<usize>) -> String {
    let (lo, hi) = (*arity.start(), *arity.end());
    if lo == hi {
        return format!("{lo} argument{}", if lo == 1 { "" } else { "s" });
    }
    format!("{lo} or {hi} arguments")
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
    walk_arith_funccalls(a, &mut |name, args| {
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
///
/// Arity comes from `value_intrinsic_arity`, the same table the `$( )`
/// spelling uses, so `(getpid x)` and `$(getpid x)` cannot answer differently
/// — the sexpr side used to hard-code `len`/`strlen` and let every other
/// intrinsic through with any argument count, which for a nullary one meant
/// falling into the user-int-func path and yielding a silent 0.
///
/// The *operand shape* check is narrower than the arity check on purpose. It
/// asks whether `reconstruct_intrinsic_command` can rebuild the operand as a
/// command argument, which is the question only for the intrinsics that take
/// a **name** (`len a`, `strlen $s`). `wait`'s operand is an ordinary
/// arithmetic value lowered by `arith_to_ssa`, so no shape is out of bounds.
fn arith_intrinsic_complaint(name: &str, args: &[ArithExpr]) -> Option<String> {
    let arity = crate::ssa::value_intrinsic_arity(name)?;
    if !arity.contains(&args.len()) {
        return Some(format!(
            "`{name}` expects {} but got {} — a value intrinsic must be \
             called with its exact arity",
            arity_phrase(&arity),
            args.len()
        ));
    }
    if name != "len" && name != "strlen" {
        return None;
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

/// The operand shapes the transform's `reconstruct_intrinsic_command` rebuilds
/// — asked of the transform itself, so the two cannot drift. This used to be a
/// hand-kept copy of `intrinsic_operand_kind`'s match, Paren recursion and all.
fn arith_intrinsic_operand_ok(a: &ArithExpr) -> bool {
    crate::transform::expr_handlers::intrinsic_operand_kind(a).is_some()
}

/// Does this sexpr argument rebuild to a **literal**, rather than to something
/// that reads a variable?
///
/// Only meaningful together with [`is_scalar_operand_intrinsic`]: the same
/// `ArithExpr::Var("a")` rebuilds to `ExprKind::Literal("a")` for both `len`
/// and `strlen`, but it *names a collection* to `len` and *is* the
/// one-character string `"a"` to `strlen`. The shape is shared; the discipline
/// is not.
fn sexpr_arg_is_literal_operand(a: &ArithExpr) -> bool {
    matches!(
        crate::transform::expr_handlers::intrinsic_operand_kind(a),
        Some(ExprKind::Literal(_))
    )
}

/// The arguments of an arith sexpr `(name args…)`, as reads.
///
/// DD#102 split the int intrinsics into two operand disciplines, and this is
/// one of the places the split has to be re-asked. `len` takes a collection
/// **name**, so `(len a)` reads `a`. `strlen` takes a **value**, so the bare
/// `a` in `(strlen a)` is the one-character string `"a"` and reads nothing —
/// exactly as `$(strlen a)` does, and exactly as `intrinsic_operand_kind`
/// already lowers it.
///
/// Walking every argument as a read regardless is the *pre*-DD#102 model, in
/// which every int intrinsic resolved a name. It had both failure directions:
///
/// - a false positive, `(strlen nosuch)` → SB5031, which DD#102 explicitly
///   retires — "an unknown bare word is a string literal, not a missing
///   variable, so there is nothing to report" — while the word form
///   `$(strlen nosuch)` happily answers 6;
/// - a false negative, because this sink also feeds SB5050: the bare word was
///   credited with a *phantom read*, so `str s=x; echo $(( (strlen s) ))` hid
///   a dead variable that `$(strlen s)` reports.
///
/// `is_scalar_operand_intrinsic` is the canonical list of the value-taking
/// family, shared with SB5070 — which had this very fault, keyed on the family
/// `strlen` had just left, and was repaired on 2026-09-17. Keying off that list
/// rather than naming `strlen` here is what stops a third copy from drifting.
fn check_sexpr_arg_reads(
    name: &str,
    args: &[crate::ir::ArithExpr],
    span: Span,
    known: &HashSet<String>,
    sink: &mut ReadSink,
) {
    if !is_scalar_operand_intrinsic(name) {
        check_arith_each(args, span, known, sink);
        return;
    }
    for a in args {
        if sexpr_arg_is_literal_operand(a) {
            continue;
        }
        check_arith_reads(a, span, known, sink);
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
    let Some(c) = crate::ir::single_value_command(stmts) else {
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
            check_dispatch_regex_patterns(c, diags);
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

/// True when an assignment RHS is the DD#88 capture-condition form
/// (`if x=$(cmd)` / `while x=$(cmd)`), where the command's exit status is the
/// condition and its stdout is captured into the target.
///
/// Qualifying shapes: a single external `$(cmd)`, an all-external pipeline
/// `$(a | b)`, or — the **DD#101 carve-out** — `$(getenv NAME)`, the one
/// intrinsic that can *fail*, not by malfunctioning but by looking up a name
/// the environment does not contain, so `home=$(getenv HOME) || home=/root` is
/// a real condition. The carve-out is `getenv` specifically, not intrinsics
/// generally: `$(len a)` and friends are pure functions of their arguments and
/// stay in the can't-fail set. Excluded: a concatenation, a multi-statement
/// `$(a; b)`, and any literal RHS.
///
/// Redirects are peeled and not judged here. Whether a redirect leaves the
/// capture with a writer is a property of **every** capture, not of this one
/// position, so it is asked once by [`check_capture_has_writer`].
///
/// `validate` runs BEFORE command resolution, so a bare user-function name
/// passes the external test here; the transform, which has the function table,
/// rejects the residual `$(user_fn)` capture-condition.
fn is_capture_condition_rhs(value: &Expr) -> bool {
    let ExprKind::CommandSubstitution(stmts) = &value.kind else {
        return false;
    };
    let [stmt] = stmts.as_slice() else {
        return false;
    };
    match peel_capture_wrappers(&stmt.kind).0 {
        StmtKind::Command(c) => is_external_command_head(c) || is_getenv_lookup(c),
        // `$(a | b)` — every stage must be an external command.
        StmtKind::Pipeline(p) => {
            p.commands.len() > 1 && p.commands.iter().all(stmt_is_external_command)
        }
        _ => false,
    }
}

/// Peel `Redirected` / `Suppressed` off a capture body, returning the
/// innermost kind and each `Redirected` layer's list **in application order**
/// (innermost first — an outer wrapper's redirects run after an inner one's).
///
/// The layers are returned rather than a yes/no flag for the reason recorded
/// on `transform::expr_handlers::unwrap_subst_wrappers`: a caller that peels
/// transitively but asks about redirects non-transitively disagrees with
/// itself, which is how `$(- cmd > f)` once dropped its redirect in silence.
fn peel_capture_wrappers(kind: &StmtKind) -> (&StmtKind, Vec<&Vec<crate::ir::Redirect>>) {
    let mut kind = kind;
    let mut layers = Vec::new();
    loop {
        match kind {
            StmtKind::Redirected(r) => {
                layers.push(&r.redirects);
                kind = &r.body.kind;
            }
            StmtKind::Suppressed(inner) => kind = &inner.kind,
            _ => break,
        }
    }
    layers.reverse();
    (kind, layers)
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

/// A pipeline stage (peeling `-`/`!`, and any redirect) that is a single
/// external command. The redirect peel matters: `$(a | b 2>f)` is a pipeline
/// of two external commands, and refusing to look past the stage's redirect
/// would make it disagree with `is_capture_condition_rhs`, which peels one off
/// a bare command (`peel_capture_wrappers`). Whether a stage redirect *steals
/// the capture* is a separate question, asked of the last stage only
/// (`ir::redirects_preserve_capture`).
fn stmt_is_external_command(stmt: &Statement) -> bool {
    matches!(&stmt.stage_body().kind, StmtKind::Command(c) if is_external_command_head(c))
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
    let keyword = w.keyword();
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

/// Check one for-in source word: glob metacharacters (SB5006) and a source
/// that denotes a single scalar rather than a collection (SB5009).
/// `$@` / `$*` are array splats (handled by the positional/splat passes),
/// not scalars, so they are exempt.
fn check_for_word(w: &Expr, diags: &mut Vec<Diagnostic>) {
    check_unquoted_glob(w, diags);
    if let Some(spelling) = for_source_scalar(w) {
        diags.push(Diagnostic::error_span(
            w.span,
            "SB5009",
            format!(
                "{spelling} in a for-loop is a single scalar, not a collection \
                 — word splitting is eliminated by design, so this could only \
                 ever run one iteration; iterate an array instead \
                 (`for x in @arr`), stream a command's output a line at a time \
                 with `cmd | while readline x {{ … }}`, split a value into an \
                 array with `read -a arr`, or use the value directly without a \
                 loop"
            ),
        ));
    }
}

/// Does this for-in source word denote a **single scalar value** rather than a
/// collection? Returns the spelling to name in the diagnostic.
///
/// DD#28 eliminates word splitting, so a scalar source could only ever run one
/// iteration — the loop is a wrapper around a value the author already holds.
/// **Quoting is not the discriminator.** `"$v"` is as much a scalar as `$v`,
/// yet only the bare form was rejected, so quoting a scalar *granted* the
/// iteration that quoting a collection (`"${a[@]}"`, SB5026) denies.
///
/// Literal words are not scalars in this sense: `for v in a b` is a list, and
/// the one-element `for v in a` is its legitimate degenerate case.
///
/// **Total over `ExprKind` on purpose — do not add a wildcard.** The rule this
/// replaces was keyed on `SimpleExpansion` alone; every other spelling of a
/// scalar fell through, and `$(getcwd)` fell all the way past lowering to a
/// bare `sash_exec` that aborted the compiled program at runtime.
fn for_source_scalar(w: &Expr) -> Option<String> {
    match &w.kind {
        // A word list, of any length — not a scalar.
        ExprKind::Literal(_) | ExprKind::SingleQuoted(_) => None,
        ExprKind::SimpleExpansion(v) => for_source_scalar_simple(v),
        ExprKind::DoubleQuoted(parts) => for_source_scalar_parts(parts, "a quoted concatenation"),
        ExprKind::Concat(parts) => for_source_scalar_parts(parts, "a concatenation"),
        ExprKind::ParameterExpansion(pe) => for_source_scalar_param(pe),
        // One element of an array is a scalar.
        ExprKind::Subscript { name, .. } => Some(format!("`${{{name}[…]}}`")),
        // `$(( … ))` is a number.
        ExprKind::Arithmetic(_) => Some("an arithmetic expansion".to_string()),
        ExprKind::CommandSubstitution(stmts) => for_source_scalar_cmdsubst(stmts),
        // `$(<file)` is a capture too — the whole file as one value. It used to
        // be exempted as "a stream of lines, like an external command", which
        // was the same line-stream mistake: it already ran exactly one
        // iteration binding the entire file, newlines and all.
        ExprKind::SlurpFile(_) => Some("`$(< …)`".to_string()),
        // Collection literals and a test are not scalar sources; neither is a
        // legal for-in word, and each is rejected by its own rule.
        ExprKind::Array(_) | ExprKind::AssocArray(_) | ExprKind::Test(_) => None,
    }
}

/// `$@` / `$*` are the positional splat; any other name is a scalar variable.
fn for_source_scalar_simple(var: &str) -> Option<String> {
    match var {
        "@" | "*" => None,
        _ => Some(format!("`${var}`")),
    }
}

/// A parameter expansion is a collection only when it carries a splat
/// operator — the `arrvals*` family is the `@a` splat and its dispositions.
/// Every other operator (`:-`, `:?`, `#`, `%`, …) yields one string.
fn for_source_scalar_param(pe: &ParameterExpansion) -> Option<String> {
    let splat = pe
        .operator
        .as_deref()
        .is_some_and(|op| op.starts_with("arrvals") || op.starts_with("arrkeys"));
    match (splat, pe.name.as_str()) {
        (true, _) | (_, "@") | (_, "*") => None,
        _ => Some(format!("`${{{}}}`", pe.name)),
    }
}

/// A quoted or concatenated word is one value built from parts. All-literal
/// parts are a literal word (`"@g"` is the text `@g`); anything else is a
/// scalar. A single part delegates, so `"$@"` and `"${a[@]}"` stay collections
/// — the quoted splat is SB5026's to report, not this rule's.
fn for_source_scalar_parts(parts: &[Expr], joined: &str) -> Option<String> {
    if parts
        .iter()
        .all(|p| matches!(p.kind, ExprKind::Literal(_) | ExprKind::SingleQuoted(_)))
    {
        return None;
    }
    match parts {
        [one] => for_source_scalar(one),
        _ => Some(joined.to_string()),
    }
}

/// A `$( )` for-in source is a **capture** — one scalar value, exactly as
/// `v=$( … )` binds one — so it is a scalar source unless it denotes a
/// collection. Only two intrinsics do: `$(argv …)` and `$(keys m)`.
///
/// **Deny-by-default on purpose.** This rule used to allow by default and
/// reject only [`crate::resolve::is_scalar_value_intrinsic`], so everything
/// else — every external command, builtin, pipeline, statement sequence and
/// user function — fell through the bottom and was lowered as an iterable
/// "line stream". There is no line stream in sash's value model: splitting a
/// capture on newlines is word splitting by another name, which DD#28
/// eliminates. The old lowering also miscompiled three ways (2026-10-02):
/// `$(a; b)` became the pipeline `a | b` so only `b`'s output survived,
/// a `$(a | b)` stage was silently dropped and the loop ran zero times, and
/// `$(f)` on a user function reached `sash_exec` as a bare name and aborted
/// the compiled program at runtime.
///
/// This is the same hole DD#111 found from the other side, and the reason the
/// polarity is now stated as an exception list rather than a rejection list.
fn for_source_scalar_cmdsubst(stmts: &[Statement]) -> Option<String> {
    // `$(argv …)` and `$(keys m)` are the only captures that denote a
    // collection. When one is the whole body it is a legitimate for-in source.
    // When it is one statement among several the body is incoherent — there is
    // no sane way to combine either with another command (DG 2026-10-02) — and
    // each already has a rule that says so in the author's own terms: SB5025
    // for the `argv` splat, SB5073 for `keys` as a value intrinsic, both
    // "recognised only when it is the whole substitution". Either way this rule
    // stays quiet, because its answer ("iterate an array instead") would not be
    // the problem the author has.
    if stmts.iter().any(stmt_is_collection_intrinsic) {
        return None;
    }
    let capture = || Some("a command substitution".to_string());
    let [stmt] = stmts else {
        return capture();
    };
    let inner = match &stmt.kind {
        StmtKind::Redirected(r) => &r.body.kind,
        other => other,
    };
    let StmtKind::Command(c) = inner else {
        return capture();
    };
    let ExprKind::Literal(name) = &c.name.kind else {
        return capture();
    };
    Some(format!("`$({name})`"))
}

/// A `$( … )` body statement that is a bare `argv` / `keys` command — the two
/// intrinsics whose capture denotes a collection rather than one value.
/// Peels a redirect, so `$(argv 2>/dev/null; …)` is recognised.
fn stmt_is_collection_intrinsic(stmt: &Statement) -> bool {
    stmt_command(stmt)
        .is_some_and(|c| crate::resolve::is_collection_intrinsic(expr_literal_str(&c.name)))
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
    check_static_regex_operand(name, c, diags);
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
    let kind = decl_target_kind(d);
    for a in &d.assignments {
        if let Some(diag) = decl_shape_diag(a, kind) {
            diags.push(diag);
        }
    }
}

/// What kind of thing an assignment's value denotes.
///
/// `Splat` is none of the literals: `map m=@{a:-}` is the DD#99 fill-from-array
/// form, whose own rules live in `check_map_decl_splats` and SB5025, and a
/// plain `$x` on a scalar is the ordinary case. Both must pass through here
/// untouched.
///
/// An array literal and a map literal are **separate** shapes. Folding them
/// into one `Collection` was DD#117's own blind spot: `array a=([b]=2)` and
/// `map m=(x y)` each matched "collection into collection" and passed, and the
/// first silently produced an empty array.
enum InitShape {
    ArrayLit,
    AssocLit,
    Scalar,
    Splat,
}

fn init_shape(value: &Expr) -> InitShape {
    match &value.kind {
        ExprKind::Array(_) => InitShape::ArrayLit,
        ExprKind::AssocArray(_) => InitShape::AssocLit,
        ExprKind::ParameterExpansion(_) => InitShape::Splat,
        _ => InitShape::Scalar,
    }
}

/// The kind a name holds: what its declaration introduced.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TargetKind {
    Array,
    Map,
    Scalar,
}

/// The kind a declaration introduces, read from its flags.
fn decl_target_kind(d: &Declaration) -> TargetKind {
    if d.flags.iter().any(|f| f == "-A") {
        TargetKind::Map
    } else if d.flags.iter().any(|f| f == "-a") {
        TargetKind::Array
    } else {
        TargetKind::Scalar
    }
}

/// The sentence for a value whose shape contradicts its target's kind, or
/// `None` when the two agree.
///
/// Shared by the declaration rule (SB4064) and the statement rule (SB5090), so
/// that `array a=x` and a later `a=x` cannot drift apart. They are one mistake
/// written in two places, and DD#117 only ever reached the first.
///
/// **Total over `(TargetKind, InitShape)` on purpose — do not add a wildcard.**
/// The pair is the whole rule; a `_` arm here is a silent accept.
fn shape_mismatch_msg(name: &str, kind: TargetKind, value: &Expr) -> Option<String> {
    match (kind, init_shape(value)) {
        (TargetKind::Array, InitShape::Scalar) => Some(format!(
            "`{name}` is an array, so its value must be a `( … )` literal — a \
             scalar does not become a one-element collection in sash; write \
             `{name}=({})` if that is what you meant",
            decl_value_brief(value),
        )),
        (TargetKind::Array, InitShape::AssocLit) => Some(format!(
            "`{name}` is an array, so its value must be a `( … )` list of \
             elements — `([key]=value)` pairs need a `map`"
        )),
        (TargetKind::Map, InitShape::Scalar) => Some(format!(
            "`{name}` is a map, so its value must be a `([key]=value)` literal \
             — a scalar does not become a one-element collection in sash"
        )),
        (TargetKind::Map, InitShape::ArrayLit) => Some(format!(
            "`{name}` is a map, so its value must be a `([key]=value)` literal \
             — a flat `( … )` list carries no keys"
        )),
        (TargetKind::Scalar, InitShape::ArrayLit | InitShape::AssocLit) => Some(format!(
            "`{name}` is a scalar, so its value cannot be a `( … )` collection \
             literal — declare it with `array` or `map` to hold several values"
        )),
        (TargetKind::Array, InitShape::ArrayLit)
        | (TargetKind::Map, InitShape::AssocLit)
        | (TargetKind::Scalar, InitShape::Scalar)
        | (TargetKind::Array, InitShape::Splat)
        | (TargetKind::Map, InitShape::Splat)
        | (TargetKind::Scalar, InitShape::Splat) => None,
    }
}

/// `Some(diag)` when the initializer's shape contradicts the declared kind.
fn decl_shape_diag(a: &DeclAssignment, kind: TargetKind) -> Option<Diagnostic> {
    let value = a.value.as_ref()?;
    // `array a=$(argv …)` is a scalar into a collection, but SB5025 says
    // something SB4064 cannot: use the out-param form `argv N a`. SB4064's
    // own advice — "write `a=(…)`" — is actively wrong there, because
    // `array a=($(argv 1))` is rejected in turn (SB5021/SB5025). SB4064 runs
    // in an earlier phase that exits before SB5025's, so deferring is what
    // keeps the better diagnostic reachable at all.
    if kind != TargetKind::Scalar && is_argv_splat_expr(value) {
        return None;
    }
    // `map m=(x y)` is SB4031's (DD#33), which names the `[key]=value` form the
    // author has to write. It runs in this same phase, so reporting both would
    // double up on a single mistake.
    if kind == TargetKind::Map && matches!(init_shape(value), InitShape::ArrayLit) {
        return None;
    }
    let msg = shape_mismatch_msg(&a.name, kind, value)?;
    Some(Diagnostic::error_span(value.span, "SB4064", msg))
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

/// SB5090/SB5091: an assignment's value must have the shape of the target's
/// kind, and a map has no append position.
///
/// This is DD#117's rule one statement later. DD#117 settled the *declaration*
/// (`array a=x` is SB4064) and explicitly left append "available as a
/// statement, once the name is declared" — but gave the statement no shape
/// rule at all, and nothing else had one either. The same mistake a line down
/// was still a segfault:
///
/// ```text
/// array a=(x)
/// a=y            # scalar into a collection slot → SIGSEGV, len a == 0
/// a+=$(echo hi)  # likewise
/// map m=([k]=1)
/// m=(y)          # silently emptied the map
/// m+=([j]=2)     # reached codegen as an ICE
/// ```
///
/// Fifteen spellings were wrong: seven faulted, seven wrote nothing and said
/// nothing, one ICEd. They survived because every one of them is a spelling no
/// spec had ever written — the same reason DD#117's cases survived.
///
/// A map is rejected for `+=` (SB5091) rather than given bash's merge
/// semantics. DG, 2026-10-08: favour clarity over compatibility — a map has no
/// end to append to, and `m[key]=value` says what the author means.
pub fn check_assign_value_shape(program: &Program) -> Vec<Diagnostic> {
    let mut kinds = HashMap::new();
    for block in program_blocks(program) {
        crate::type_check::collect_collection_kinds(block, &mut kinds);
    }
    let mut diags = Vec::new();
    for block in program_blocks(program) {
        crate::ir::walk_stmt_tree(block, &mut |stmt| {
            let StmtKind::Assignment(a) = &stmt.kind else {
                return;
            };
            if let Some(d) = assign_shape_diag(a, &kinds) {
                diags.push(d);
            }
        });
    }
    diags
}

/// `Some(diag)` when a statement assignment's value contradicts the target's
/// kind. `kinds` maps a declared collection name to whether it is a map.
fn assign_shape_diag(a: &Assignment, kinds: &HashMap<String, bool>) -> Option<Diagnostic> {
    // `a[i]=v` assigns to a *cell*, and a scalar is exactly what belongs
    // there. The kind of the collection says nothing about the cell's value.
    if a.index.is_some() {
        return None;
    }
    let kind = assign_target_kind(&a.name, kinds);
    if kind == TargetKind::Map && a.op == AssignOp::Append {
        return Some(map_append_diag(a));
    }
    if assign_shape_deferred(kind, &a.value) {
        return None;
    }
    let msg = shape_mismatch_msg(&a.name, kind, &a.value)?;
    Some(Diagnostic::error_span(a.value.span, "SB5090", msg))
}

/// The kind a name holds. A name absent from the program's collection
/// declarations is a scalar; SB5049 rejects a name that is both.
fn assign_target_kind(name: &str, kinds: &HashMap<String, bool>) -> TargetKind {
    match kinds.get(name) {
        Some(true) => TargetKind::Map,
        Some(false) => TargetKind::Array,
        None => TargetKind::Scalar,
    }
}

/// Shapes SB5090 leaves to a diagnostic that says more than it can.
fn assign_shape_deferred(kind: TargetKind, value: &Expr) -> bool {
    // A scalar given a collection literal is SB5055's: it names the
    // declaration the author is missing, not the shape they wrote.
    if kind == TargetKind::Scalar {
        return true;
    }
    // Deferred for the same reason SB4064 defers it: SB5025 names the
    // out-param form `argv N a`, which is the only thing that works here.
    is_argv_splat_expr(value)
}

/// SB5091: `+=` on a map. DG, 2026-10-08 — a map has no end to append to, and
/// `m[key]=value` is the spelling that already works.
fn map_append_diag(a: &Assignment) -> Diagnostic {
    Diagnostic::error_span(
        a.value.span,
        "SB5091",
        format!(
            "`{name}` is a map, which has no append position — assign to a \
             key instead: `{name}[key]=value`",
            name = a.name,
        ),
    )
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
        ExprKind::CommandSubstitution(body) => check_capture_has_writer(body, expr.span, diags),
        _ => {}
    }
}

/// SB4075 (DD#88, DG 2026-10-12): a `$( … )` capture must still have something
/// writing to it.
///
/// Inside a substitution the body starts with fd 1 attached to the capture
/// pipe. A redirect can move that pipe around, and the capture is only
/// meaningful while *some* fd still holds it:
///
/// - `$(cmd 2>/dev/null)` — fd 1 untouched, the capture reads stdout. Fine.
/// - `$(cmd 2>&1 >f)` — fd 2 takes the pipe, then fd 1 goes to the file, so
///   the capture reads *stderr*. That is the whole point of the idiom, and it
///   is the case DG named when settling the rule. Fine.
/// - `$(cmd >f)` — the pipe is abandoned, so the capture is unconditionally
///   empty. **Rejected**, as is `$(cmd >f 2>&1)` (fd 2 dups the file, not the
///   pipe) and `$(cmd &>f)`, which the parser desugars to a plain `>f`.
///
/// Scoped to the whole language rather than to the capture-condition that
/// exposed it: an abandoned capture is nonsense wherever it is written, and
/// `a=$(cmd >f)` is the same mistake as `if a=$(cmd >f)`.
///
/// Deliberately permissive about what counts as a writer — **any** top-level
/// statement that could still write qualifies, and a form whose writer is not
/// plainly visible (a compound, an assignment) counts as one. `$(a >f; b)` is
/// fine because `b` feeds the capture.
fn check_capture_has_writer(body: &[Statement], span: Span, diags: &mut Vec<Diagnostic>) {
    if body.is_empty()
        || body
            .iter()
            .any(|s| stmt_can_write_capture(&s.kind, Vec::new()))
    {
        return;
    }
    diags.push(Diagnostic::error_span(
        span,
        "SB4075",
        "this redirect sends the command's output somewhere else, leaving the `$( … )` capture \
         with nothing to read — it could only ever produce the empty string. Run the command on \
         its own if you want the redirect (`cmd >file`), or keep the capture fed by redirecting \
         the other stream into it (`$(cmd 2>&1 >file)`)"
            .to_string(),
    ));
}

/// Could this statement still write to the enclosing `$( … )`'s capture, given
/// the redirect layers `applied` already in force (outermost first)?
///
/// **Total over `StmtKind` on purpose — do not add a wildcard.** The `true`
/// arms are not a catch-all: each one is a claim that the form either writes
/// the capture directly or hides its writer somewhere this walk does not look,
/// and a new statement form has to make that claim for itself rather than
/// inheriting a silent "no".
fn stmt_can_write_capture<'a>(
    kind: &'a StmtKind,
    mut applied: Vec<&'a Vec<crate::ir::Redirect>>,
) -> bool {
    let (inner, mut layers) = peel_capture_wrappers(kind);
    applied.append(&mut layers);
    match inner {
        StmtKind::Command(_) => {
            crate::ir::redirects_preserve_capture(applied.into_iter().flatten())
        }
        // A pipeline's capture is written by its LAST stage. An earlier stage's
        // `>f` steals its own stdout from the next pipe, not from us.
        StmtKind::Pipeline(p) => match p.commands.last() {
            Some(last) => stmt_can_write_capture(&last.kind, applied),
            None => true,
        },
        // `- cmd` / `! cmd` are peeled above, so these cannot survive the peel;
        // `Redirected` likewise. Listed for totality.
        StmtKind::Suppressed(_) | StmtKind::Negated(_) | StmtKind::Redirected(_) => true,
        // Writers hidden inside a body this walk does not enter. A redirect on
        // one branch says nothing about the others, and SB4021 already rejects
        // a redirect on the compound itself — so never claim the capture is
        // abandoned on their account.
        StmtKind::If(_)
        | StmtKind::While(_)
        | StmtKind::For(_)
        | StmtKind::Case(_)
        | StmtKind::Group(_)
        | StmtKind::List(_)
        | StmtKind::ReturnProducer(_) => true,
        // No output of their own, so they neither feed nor abandon a capture.
        // `true` keeps them from being read as "the capture is dead": a body
        // of `$(x=1)` is an empty capture for reasons that are not this rule's.
        StmtKind::Assignment(_)
        | StmtKind::Declaration(_)
        | StmtKind::Function(_)
        | StmtKind::ArithTest(_) => true,
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
///
/// Every `=~` in the test is checked, not just a lone top-level one: under
/// `&&`, `||` or `!` the check used to be skipped, so `[[ $x =~ a && $x =~ ( ]]`
/// compiled and died at runtime with `regexp error`.
fn check_regex_literal(test: &TestExpr, span: Span, diags: &mut Vec<Diagnostic>) {
    match test {
        TestExpr::Binary { op, right, .. } => {
            if op == "=~" {
                check_static_regex(right, "invalid regex pattern", span, diags);
            }
        }
        TestExpr::Unary { .. } => {}
        TestExpr::Not(inner) => check_regex_literal(inner, span, diags),
        TestExpr::And(l, r) | TestExpr::Or(l, r) => {
            check_regex_literal(l, span, diags);
            check_regex_literal(r, span, diags);
        }
    }
}

/// SB5023 for the regex operand of `sub`/`gsub`/`match`/`matchall` (DD#39).
/// The runtime compiles all of them with the same engine as `=~`, so a static
/// pattern that cannot compile is a compile error here too — it used to
/// compile and die at runtime with `regexp error`. The `${x:s/…}` sugar, every
/// edit of a `,` chain, and the `x =~ s/…` statement all desugar to `sub`/
/// `gsub` and reach this through their capture bodies.
fn check_static_regex_operand(name: &str, c: &Command, diags: &mut Vec<Diagnostic>) {
    let what = match name {
        "sub" | "gsub" => "invalid regex in substitution",
        "match" | "matchall" => "invalid regex pattern",
        _ => return,
    };
    if let Some(re) = c.args.get(1) {
        check_static_regex(re, what, re.span, diags);
    }
}

/// SB5023 for a regex dispatch's patterns (`[[ $x =~ p { … } ]]`, DD#66): each
/// static pattern is compiled now, exactly as a `[[ =~ ]]` operand is.
fn check_dispatch_regex_patterns(c: &Case, diags: &mut Vec<Diagnostic>) {
    if c.op != "=~" {
        return;
    }
    for p in c.items.iter().flat_map(|it| &it.patterns) {
        check_static_regex(p, "invalid regex pattern", p.span, diags);
    }
}

/// SB5023 if `expr` is a static regex that the engine rejects. A dynamic
/// pattern is left to the runtime regcomp (DD#71).
fn check_static_regex(expr: &Expr, what: &str, span: Span, diags: &mut Vec<Diagnostic>) {
    let Some(pattern) = static_regex_pattern(expr) else {
        return;
    };
    if let Err(msg) = crate::regex_validate::validate_regex(&pattern) {
        diags.push(Diagnostic::error_span(
            span,
            "SB5023",
            format!("{what}: {msg}"),
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
///
/// DD#116: a `if classify_attest(&c.args).is_some() { return None }` guard was
/// deleted here (2026-10-03) as a *rival* — the sole caller reaches this only
/// on the `else` of its own `classify_attest`, so re-asking could only ever
/// answer `None`, and an execution probe over the whole spec suite confirmed it
/// never fired.
fn attest_arith_near_miss(c: &Command) -> Option<ArithNearMiss<'_>> {
    let last = c.args.last()?;
    if !matches!(&last.kind, ExprKind::Arithmetic(_)) {
        return None;
    }
    match &c.args[..c.args.len() - 1] {
        // `attest x (( … ))` — the form, with an RHS reading the wrong name.
        [only] => match &only.kind {
            ExprKind::Literal(s) => Some(ArithNearMiss::Scalar(s)),
            _ => None,
        },
        // DD#116: unreachable by construction. `parse_attest` raises SB4014
        // ("expected variable name after 'attest'") before an `attest (( … ))`
        // with no name can be built, so reaching here means the parser's
        // guarantee broke. The old `None` was the *silent* answer to that —
        // it waved the command on to the generic path, which is how a bare
        // `attest` came to exec in the first place.
        [] => unreachable!("SB4014 rejects an `attest` with no name before its predicate"),
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

/// DD#126: a compiler-managed value that a single statement produces as a
/// *side* output, separate from its status.
#[derive(Clone, Copy, PartialEq)]
enum Latch {
    /// The pid of a job this statement backgrounded — `$(getcpid)` / `$!`.
    ChildPid,
    /// The exit status of the child this statement reaped — `$(waitstatus)`.
    WaitStatus,
}

/// Whether a latch written by the *immediately preceding* statement is still
/// readable. Both latches are single-slot and silently overwritten, so a read
/// one statement too late is a wrong answer rather than an error — which is
/// exactly what `$!` did before this rule existed
/// (`notes/tasks/bug-bg-pid-slot-unavailable.md`).
///
/// One enum, not two, because no statement both backgrounds a job and reaps
/// one: at most one latch can be fresh.
#[derive(Clone, Copy, PartialEq)]
enum LatchState {
    Stale,
    Fresh(Latch),
}

/// What the statement about to be checked may read: the `$?` availability
/// state and the compiler-managed latch, walked together by one walker so the
/// two cannot drift (`notes/journal/456-the-copy-that-never-got-the-fix.md`).
#[derive(Clone, Copy)]
struct FlowState {
    status: StatusState,
    latch: LatchState,
}

impl FlowState {
    fn new(status: StatusState) -> Self {
        FlowState {
            status,
            latch: LatchState::Stale,
        }
    }

    /// After a catching construct (`if`/`while`/`for`/`case`, or a `&&`/`||`
    /// list): which branch ran is not known, so nothing survives.
    fn end_construct(&mut self) {
        self.status = StatusState::Undefined;
        self.latch = LatchState::Stale;
    }
}

/// Check $? usage across a statement list.
fn check_dollar_question(stmts: &[Statement], seed: StatusState, diags: &mut Vec<Diagnostic>) {
    let mut state = FlowState::new(seed);
    for stmt in stmts {
        check_dq_stmt(stmt, &mut state, diags);
    }
}

fn check_dq_stmt(stmt: &Statement, state: &mut FlowState, diags: &mut Vec<Diagnostic>) {
    match &stmt.kind {
        StmtKind::List(l) => check_dq_list(l, state, diags),
        StmtKind::Group(g) => {
            for s in &g.body {
                check_dq_stmt(s, state, diags);
            }
            // A group is braces, not a catching construct: its last statement
            // owns the latch on the way out, so do not overwrite it below.
            return;
        }
        StmtKind::Function(f) => check_dq_function(f, diags),
        StmtKind::If(i) => check_dq_if(i, state, diags),
        StmtKind::While(w) => check_dq_while(w, state, diags),
        StmtKind::For(f) => check_dq_for(f, state, diags),
        StmtKind::Case(c) => check_dq_case(c, state, diags),
        _ => check_dq_leaf(stmt, state, diags),
    }
    state.latch = latch_after(stmt);
}

/// What a statement leaves the latch in — **the single authority**, applied
/// above to every statement shape there is.
///
/// It has to be structural rather than a transition inside [`check_dq_list`],
/// because `- cmd &` is `Suppressed(List(Background, …))` — the suppression
/// wraps the whole list — and so never reaches that arm at all. Answering it
/// in two places is how the first cut rejected `- cmd & ; int p=$!`.
fn latch_after(stmt: &Statement) -> LatchState {
    if stmt_backgrounds_job(stmt) {
        LatchState::Fresh(Latch::ChildPid)
    } else if stmt_reaps_child(stmt) {
        LatchState::Fresh(Latch::WaitStatus)
    } else {
        LatchState::Stale
    }
}

/// Does this statement background a job? `cmd &` is a `ListOp::Background`
/// list; `- cmd &` wraps that list in the suppression.
fn stmt_backgrounds_job(stmt: &Statement) -> bool {
    match &stmt.kind {
        StmtKind::Suppressed(inner) => stmt_backgrounds_job(inner),
        StmtKind::Redirected(r) => stmt_backgrounds_job(&r.body),
        StmtKind::List(l) => l.op == crate::ir::ListOp::Background,
        _ => false,
    }
}

/// `a && b` / `a || b`: left runs normally, right is a branch so
/// inherited $? is available only for the first statement.
fn check_dq_list(l: &crate::ir::List, state: &mut FlowState, diags: &mut Vec<Diagnostic>) {
    check_dq_stmt(&l.left, state, diags);
    let mut branch_state = FlowState {
        status: StatusState::Available,
        latch: state.latch,
    };
    check_dq_stmt_branch(&l.right, &mut branch_state, diags);
    state.end_construct();
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
    let mut fn_state = FlowState::new(StatusState::Undefined);
    for s in &f.body {
        check_dq_stmt(s, &mut fn_state, diags);
    }
}

/// `if … then … (elif …)* (else …)? fi`: walk the condition with
/// the inherited state, then each branch body with the
/// first-statement-only rule. After the construct, state is
/// undefined regardless of which branch ran.
fn check_dq_if(i: &crate::ir::If, state: &mut FlowState, diags: &mut Vec<Diagnostic>) {
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
    state.end_construct();
}

/// `while … do … done`: condition uses inherited state, body
/// re-reads it fresh for each iteration. After the loop, state
/// is undefined.
fn check_dq_while(w: &crate::ir::While, state: &mut FlowState, diags: &mut Vec<Diagnostic>) {
    for s in &w.condition {
        check_dq_stmt(s, state, diags);
    }
    let mut bs = *state;
    for s in &w.body {
        check_dq_stmt(s, &mut bs, diags);
    }
    state.end_construct();
}

/// `for x in … do … done`: word list does not consume $? (literal/
/// expansion only). Body re-reads inherited state fresh for each
/// iteration. After the loop, state is undefined — for is a
/// catching construct (caught/uncaught model).
fn check_dq_for(f: &crate::ir::For, state: &mut FlowState, diags: &mut Vec<Diagnostic>) {
    let mut bs = *state;
    for s in &f.body {
        check_dq_stmt(s, &mut bs, diags);
    }
    state.end_construct();
}

/// `case word in pat) body ;; … esac`: scrutinee does not consume
/// $? (it's a word). Each arm body re-reads inherited state fresh.
/// After the case, state is undefined — case is a catching construct.
fn check_dq_case(c: &crate::ir::Case, state: &mut FlowState, diags: &mut Vec<Diagnostic>) {
    for item in &c.items {
        let mut bs = *state;
        for s in &item.body {
            check_dq_stmt(s, &mut bs, diags);
        }
    }
    state.end_construct();
}

/// Default arm: a leaf-ish statement (Command, Pipeline, etc.).
/// Two responsibilities: (1) if it consumes `$?`, raise SB5012
/// when the status is unavailable; (2) update state to reflect
/// what this statement does (sets, consumes, or neither).
fn check_dq_leaf(stmt: &Statement, state: &mut FlowState, diags: &mut Vec<Diagnostic>) {
    if stmt_uses_status(stmt) {
        if state.status != StatusState::Available {
            emit_status_unavailable(stmt, diags);
        }
        state.status = StatusState::Undefined;
    }
    // Reads are checked against the *incoming* latch, before this statement's
    // own transition, so `- /bin/x & ; - wait $!` reads a fresh `$!` and only
    // then latches the status it reaped.
    check_latch_reads(stmt, state.latch, diags);
    if let Some(next) = leaf_status_after(stmt) {
        state.status = next;
    }
}

/// SB5094: a latch read is legal only in the statement immediately after the
/// one that wrote it.
fn check_latch_reads(stmt: &Statement, latch: LatchState, diags: &mut Vec<Diagnostic>) {
    let mut reads = Vec::new();
    find_latch_reads(stmt, &mut reads);
    for (span, want) in reads {
        if latch != LatchState::Fresh(want) {
            diags.push(Diagnostic::error_span(
                span,
                "SB5094",
                latch_unavailable_msg(want),
            ));
        }
    }
}

/// Total over `Latch` on purpose — do not add a wildcard.
fn latch_unavailable_msg(want: Latch) -> String {
    match want {
        Latch::ChildPid => {
            "$(getcpid) is not available here — it reads the pid of a job \
             backgrounded by the immediately preceding statement (the punctuation sugar \
             $! reads the same value)"
        }
        Latch::WaitStatus => {
            "$(waitstatus) is not available here — it reads the status of \
             the child reaped by the immediately preceding statement"
        }
    }
    .to_string()
}

/// Collect the span and kind of every latch read in `stmt`. Mirrors
/// [`find_status_reads`]: both intrinsics are `$( )` heads after the DD#111
/// desugar, and both can also appear as an arithmetic sexpr.
fn find_latch_reads(stmt: &Statement, out: &mut Vec<(Span, Latch)>) {
    for (name, which) in LATCH_INTRINSICS {
        find_one_latch_read(stmt, name, which, out);
    }
}

/// The canonical spelling of each latch reader. `$!` is parser-level
/// punctuation sugar for `$(getcpid)` (DD#111), so only the long form appears
/// here — and the diagnostics name the long form for the same reason.
const LATCH_INTRINSICS: [(&str, Latch); 2] = [
    ("getcpid", Latch::ChildPid),
    ("waitstatus", Latch::WaitStatus),
];

fn find_one_latch_read(stmt: &Statement, name: &str, which: Latch, out: &mut Vec<(Span, Latch)>) {
    walk_exprs_in_stmt(stmt, &mut |expr, _| {
        if expr_calls_intrinsic(expr, name, false) {
            out.push((expr.span, which));
        }
    });
    if arith_test_names_intrinsic(stmt, name) {
        out.push((stmt.span, which));
    }
}

/// What a leaf statement leaves `$?` in, or `None` for a statement that says
/// nothing about it (so the inherited state survives).
fn leaf_status_after(stmt: &Statement) -> Option<StatusState> {
    // A redirect says where the output goes, not what becomes of the status:
    // `cmd >f` ran a command and set `$?` exactly as `cmd` did. Peel it before
    // asking anything else.
    //
    // Every other statement-shape predicate in this file already does
    // (`stmt_backgrounds_job`, `stmt_reaps_child`), and `stmt_backgrounds_job`
    // carries a doc recording the identical fault being fixed for `$!`. This
    // one was missed, so `/bin/echo hi >f` followed by `$?` drew a false
    // SB5012 claiming there was "no preceding command" — the message naming
    // the one thing that had demonstrably just happened.
    //
    // Suppression is *not* peeled, and DD#55 says so outright: "`$?` is not
    // available after `- cmd` — reading `$?` after `-` is a compile error.
    // You said you don't care; you don't get to inspect it." It names
    // `- cmd; return $?` as a compile error specifically (spec 55.15).
    //
    // DD#21 only lists `-` as one of three dispositions, from which this
    // follows but is not stated. Cite the rule that states it.
    if let StmtKind::Redirected(r) = &stmt.kind {
        return leaf_status_after(&r.body);
    }
    if is_suppressed(stmt) {
        return Some(StatusState::Undefined);
    }
    if is_command_like(stmt) || is_assignment_from_arith(stmt) || is_capture_condition_stmt(stmt) {
        return Some(StatusState::Available);
    }
    None
}

/// DD#126: does this statement reap a child, in any spelling?
///
/// `wait` is the one command whose status output is not `$?`: it returns a
/// *pid*, and the reaped child's status goes to the `$(waitstatus)` latch. So
/// `- wait` is an ordinary suppression — the *pid* is what is discarded — and
/// this predicate's one job is to say the latch is now fresh, in every
/// spelling: a capture (`int p=$(wait)`) and an arithmetic `(( wait > 0 ))`
/// are neither of them a `StmtKind::Command`.
fn stmt_reaps_child(stmt: &Statement) -> bool {
    match &stmt.kind {
        StmtKind::Suppressed(inner) => stmt_reaps_child(inner),
        StmtKind::Redirected(r) => stmt_reaps_child(&r.body),
        StmtKind::Command(c) => expr_literal_str(&c.name) == "wait",
        _ => {
            let mut found = arith_test_names_intrinsic(stmt, "wait");
            walk_own_exprs_in_stmt(stmt, &mut |e, _| found |= expr_calls_wait(e));
            found
        }
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

/// Collect the span of every status read in `stmt` — `$(status)`, `$?`, and
/// the arithmetic sexpr `(status)`.
///
/// **This is the AST-level half of SB5012 and it is load-bearing.** Until the
/// DD#111 desugar this matched `SimpleExpansion("?")`, which covered
/// `$(status)` only because the parser folded the intrinsic into that node.
/// Reversing that fold without moving this would leave the walk finding
/// nothing, and the availability state machine below would go **silently
/// vacuous** — every spec still passing, and SB5012 simply never raised again.
/// `crate::ir::intrinsic_call_name` is the recogniser; its round-trip test
/// pins it to the constructor.
///
/// **Two node types carry a status read, not one.** `walk_all_exprs` visits
/// `Expr`s, and an arithmetic `(status)` is an `ArithExpr::FuncCall` — a
/// different type, structurally invisible to an `Expr` walk however thorough.
/// `$(( $? ))` and `$(( (status) ))` were both accepted with no preceding
/// command while `"$?"` and `"$(status)"` were correctly rejected. The
/// arithmetic tree has no per-node span, so the enclosing `$(( … ))` carries
/// the diagnostic — and carries it **once**, however many reads it holds,
/// because repeating one span would print the same line twice.
fn expr_reads_status(expr: &Expr) -> bool {
    expr_calls_intrinsic(expr, "status", false)
}

/// DD#126: `$(wait)` or `$(wait $p)`, or `wait` inside `$(( … ))`.
fn expr_calls_wait(expr: &Expr) -> bool {
    expr_calls_intrinsic(expr, "wait", true)
}

/// Does `expr` name intrinsic `want` — as a `$( )` head or inside `$(( … ))`?
///
/// Two node types carry such a call, not one, which is the point of the doc
/// above: `walk_all_exprs` visits `Expr`s, and `(status)` in arithmetic is an
/// `ArithExpr::FuncCall`, a different type that an `Expr` walk cannot see
/// however thorough. One walker answers it for `status` (SB5012) and for
/// `wait` (DD#126 status availability) so the two cannot drift.
///
/// `with_args` is the one place they genuinely differ. SB5012 wants the DD#111
/// nullary shape exactly — `intrinsic_call_name`'s doc records that
/// `$(status 5)` is SB4045, a different thing from a status read — while
/// `$(wait $p)` is an ordinary `wait`.
fn expr_calls_intrinsic(expr: &Expr, want: &str, with_args: bool) -> bool {
    let head = match &expr.kind {
        ExprKind::CommandSubstitution(stmts) if with_args => {
            crate::ir::single_value_command(stmts).map(|c| expr_literal_str(&c.name))
        }
        _ => crate::ir::intrinsic_call_name(expr),
    };
    if head == Some(want) {
        return true;
    }
    let ExprKind::Arithmetic(a) = &expr.kind else {
        return false;
    };
    let mut found = false;
    walk_arith_funccalls(a, &mut |name, _| found |= name == want);
    found
}

fn find_status_reads(stmt: &Statement, spans: &mut Vec<Span>) {
    walk_exprs_in_stmt(stmt, &mut |expr, _| {
        if expr_reads_status(expr) {
            spans.push(expr.span);
        }
    });
    if arith_test_names_intrinsic(stmt, "status") {
        spans.push(stmt.span);
    }
}

/// Does the `(( … ))` *statement* form name intrinsic `want`?
///
/// **A `(( … ))` statement is structurally invisible to an `Expr` walk.** Its
/// children hang off an `ArithExpr`, and `Statement::direct_exprs` yields
/// nothing at all for `StmtKind::ArithTest` — deliberately, so the passes that
/// want `ArithExpr` nodes walk the arith tree themselves. Every check that
/// asks "does this statement name intrinsic X" therefore has to ask the arith
/// tree separately, and two of them were not:
///
/// * `if (( (status) == 0 )) { … }` read `$?` with no preceding command and
///   raised **no SB5012** — the `$(( … ))` *expression* form was caught, the
///   statement form was not;
/// * `while (( wait > 0 )) { int s=$(waitstatus) }` did not register as a
///   reap, so the drain loop DD#126 exists for could not read its own status.
///
/// The arith tree has no per-node span, so the enclosing statement carries the
/// diagnostic — once, however many reads it holds.
fn arith_test_names_intrinsic(stmt: &Statement, want: &str) -> bool {
    let StmtKind::ArithTest(a) = &stmt.kind else {
        return false;
    };
    let mut found = false;
    walk_arith_funccalls(a, &mut |name, _| found |= name == want);
    found
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
fn check_dq_stmt_branch(stmt: &Statement, state: &mut FlowState, diags: &mut Vec<Diagnostic>) {
    match &stmt.kind {
        StmtKind::Group(g) => {
            check_dq_body_branch(&g.body, *state, diags);
            state.end_construct();
        }
        _ => {
            // Single statement branch: normal check
            check_dq_stmt(stmt, state, diags);
        }
    }
}

/// Apply first-statement-only rule to a body (Vec<Statement>).
fn check_dq_body_branch(stmts: &[Statement], state: FlowState, diags: &mut Vec<Diagnostic>) {
    for (i, s) in stmts.iter().enumerate() {
        if i == 0 {
            let mut first_state = state;
            check_dq_stmt(s, &mut first_state, diags);
        } else {
            // After first statement, inherited status is consumed
            let mut inner = FlowState::new(StatusState::Undefined);
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
    "signum",
    // Filesystem
    "cd",
    "close",
    // Introspection
    "status",
    "getpid",
    "getppid",
    "getcpid",
    "waitstatus",
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
    //
    // "Never PATH-resolved" is a claim about a *different* list: `resolve`
    // gates on `is_intrinsic`, not on this one, so it holds only while every
    // name here is also an intrinsic. `kill` was the one that was not, and it
    // exec'd `/usr/bin/kill` for its whole life. `every_builtin_name_is_an_intrinsic`
    // now enforces the subset relation.
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
/// `argv` and `keys` are deliberately absent — DD#60's `argv [N] a` and
/// DD#75's `keys m k` are out-param forms, and an out-param form is a legal
/// statement. So are `match`/`matchall`, which are boolean commands with
/// out-array forms. Each of the three has its own form check instead
/// (`check_keys_form`, `argv_form_diag`, `match_arity_diag`), because for them
/// it is the *spelling* that is or is not a statement, not the name.
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
    "waitstatus",
    "getcwd",
    "epochseconds",
    "argc",
    "arg",
    "signum",
    "len",
    "strlen",
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
///
/// Every name here is scalar-valued, so the generic advice — capture it with
/// `x=$(name …)` — always names a form that works. The two **collection**
/// intrinsics are not in `VALUE_ONLY_INTRINSICS` and do not reach this
/// function; `keys` once was, and inherited a suggestion SB5025 rejects in as
/// many words. If a collection-valued name is ever added to that list, it
/// needs its own wording, not this one.
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
    if let StmtKind::Suppressed(inner) = &stmt.kind {
        check_cmd_at(inner, funcs, pos, diags);
        return;
    }
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
///
/// The same fault once had a *third* shape, and this comment named it without
/// fixing it. Six quoted constants — `""`, `"echo"`, `"/bin/echo"`, `"ec"ho`,
/// `ec"ho"`, `'ec'ho` — reached the general message below and were told the
/// identical untruth. DG, 2026-10-11: "why would `\"\"` as a command use a
/// different error from `\'\'`?" They are now decided in the parser by
/// `normalize_command_head`, which judges every quoted *constant* head.
///
/// So the general message's stated reason is true of everything that still
/// receives it: whatever reaches it is either a malformed assignment target
/// (handled above) or a head containing a genuine expansion. Pinned by
/// `spec/errors.sh` qhead.20-21, which assert both sides of that — an
/// unquoted `Concat` keeps the assignment message, and an interpolating head
/// keeps this one.
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
         can't be resolved at compile time; use the literal path, \
         single-quote it if it contains a space, or `/usr/bin/env` for a \
         runtime tool"
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
        // DD#75 §3: `keys` has two forms — the `$(keys m)` splat and the
        // `keys m k` out-param fill — so which position is legal depends on
        // the spelling, not on the name. It cannot be in
        // `VALUE_ONLY_INTRINSICS` for the same reason `argv` cannot.
        "keys" => return diags.extend(keys_form_diag(c, pos)),
        // A value-producing intrinsic (`$(getenv NAME)`, `$(status)`, …) is
        // legal only as a command-substitution head. In statement position its
        // value has nowhere to go, and it used to fall through to `sash_exec`
        // and abort at runtime on an internal assertion.
        n if VALUE_ONLY_INTRINSICS.contains(&n) => {
            diags.extend(value_intrinsic_diag(name, c, pos));
            return;
        }
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
        ExprKind::Literal(s) if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
            && s.parse::<i32>().is_ok());
    if !is_fd {
        diags.push(Diagnostic::error_span(
            arg.span,
            "SB4052",
            "`close` needs a literal file-descriptor number in 0..2147483647 (e.g. `close 3`) — \
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
    // exec replaces the process with an EXTERNAL program: an absolute path or a
    // PATH-resolvable name (not the inline `echo`/`:` shortcut — the transform
    // resolves the target to its real path).
    if tname.starts_with('/') {
        // Same standard as a command head, through the same function (SB5095).
        diags.extend(crate::resolve::command_path_diag(
            "`exec` target ",
            tname,
            target.span,
        ));
        return;
    }
    if !resolve_on_path(tname) {
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
    // No `*` arm. It used to pass any bare name containing a star, on the
    // assumption that a glob head expands at runtime. DD#39 makes the glob
    // star-only, and DG (2026-10-10) scoped it to array context: in command
    // position `*` is an ordinary filename byte, so `ech*` names a program
    // that does not exist and must be rejected rather than quietly falling
    // through to the PATH search for `echo`.
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

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::collections::HashSet;

/// Run taint analysis: track which variables hold tainted data,
/// reject any use of tainted variables outside attest expressions.
///
/// DD#127: summarize each function independently of its callers. The empty
/// entry measures its own taint; singleton global entries measure inherited
/// flows and conditional diagnostics. Calls apply these summaries in source
/// order. Only a single-use function may leave its own taint to its caller;
/// a shared function must remove it before returning (SB5067).
///
/// Collection slots still use DD#100's call-site union until their summaries
/// are integrated. Positional arguments are checked at the call (DD#46).
pub fn check_taint(program: &Program) -> Vec<Diagnostic> {
    let mut tainted: HashSet<String> = HashSet::new();
    let mut diags = Vec::new();
    let info = analyze_slots(program);
    let map_names = collect_map_names(program);
    let mut globals = HashSet::new();
    for block in program_blocks(program) {
        collect_global_names(block, &mut globals);
    }
    let census = take_census(program);
    let empty = HashMap::new();
    let scratch = BodyCx::default();
    let base = SlotCx {
        probing: false,
        info: &info,
        map_names: &map_names,
        globals: &globals,
        fns: &census.fns,
        summaries: &empty,
        fname: "",
        body: &scratch,
        log: None,
    };
    let summaries = build_summaries(program, &census, &base);
    let cx = SlotCx {
        summaries: &summaries,
        ..base
    };
    for block in program_blocks(program) {
        taint_block(block, &mut tainted, &JustTainted::new(), &cx, &mut diags);
    }
    for (f, span) in program_blocks(program)
        .into_iter()
        .flatten()
        .filter_map(as_function_def)
    {
        diags.extend(check_function_def(f, span, &tainted, &cx));
    }
    diags
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
    let snap = cx.log.map(|log| (log, log.bump(), tainted.clone()));
    taint_stmt(stmt, tainted, just, cx, diags);
    if cx.probing {
        return JustTainted::new();
    }
    let taints = stmt_taints(stmt, cx);
    if let Some((log, mark, before)) = snap {
        log_sites(log, mark, stmt, &before, tainted, &taints, cx.globals);
    }
    taints
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
    let scratch = BodyCx::default();
    let probe_cx = SlotCx {
        probing: true,
        log: None,
        body: &scratch,
        ..*cx
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
        | B::SetTestFromPipe
        | B::LetExit
        | B::StoreBgPid
        | B::TrapSignal
        | B::SignalFlag
        | B::SignalCheckpoint
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
    if let Some(index) = pe.index.as_deref() {
        dollar_at_expr(index, in_fn, diags);
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
/// `read [SRC] args`, `readline args`, `argv … args`, `keys m args`,
/// `mapfile/readarray args`, or `attest … in args`.
///
/// Every out-param form belongs here. `args` is immutable (DD#92) whatever the
/// *source* of the bytes — DD#75's `keys m args` writes keys the program put
/// there itself rather than anything external, and is rejected all the same.
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
        // DD#75: `keys m k` — the target is the second argument. One argument
        // is the `$(keys m)` splat, which writes nothing.
        "keys" => c.args.len() == 2 && c.args.last().map(expr_literal_str) == Some("args"),
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
        argv_splats += usize::from(check_for_source_word(w, diags));
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

/// Validate one for-in source word, reporting whether it is an argv splat so
/// the caller can enforce the sole-source rule (SB5027).
fn check_for_source_word(w: &Expr, diags: &mut Vec<Diagnostic>) -> bool {
    if is_array_splat_source(w) {
        return true;
    }
    if is_disposition_splat(w) {
        // DD#69 addendum: the empty-disposition is command-argument-only;
        // a for-in source is the bare `@a` (its own zero-match handling).
        diags.push(disposition_position_diag(w.span));
    } else if let Some((text, advice)) = quoted_at_splat_text(w) {
        diags.push(quoted_at_splat_diag(w.span, &text, &advice));
    } else {
        // Catches a quoted splat word (SB5026) and any nested misuse.
        check_argv_splat_expr(w, diags);
    }
    false
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
    if let Some(c) = keys_splat_command(expr) {
        return keys_splat_diag(expr.span, c);
    }
    None
}

/// The map name in `$(keys config)`. `keys` resolves its operand as a bare
/// map name (DD#75 §3), so a literal is the only spelling that can ever be
/// valid — and the only one this diagnostic may quote, because it quotes it
/// back as *advice*. Every other spelling is SB5062's to report; paraphrasing
/// one here would suggest a `for` line that is itself rejected.
///
/// Takes the `Command` rather than the `Expr` on purpose. This used to
/// re-derive it: destructure `CommandSubstitution`, unwrap `Redirected`, match
/// `StmtKind::Command` — a private fork of [`stmt_command`], whose own doc
/// says it is shared so that "a second unwrap that forgot `Redirected`" cannot
/// diverge. The fork had not forgotten `Redirected`, but it had kept
/// `stmts.first()`, the spelling [`sole_cmdsub_command`] was *fixed away from*
/// when `$(argv; /usr/bin/touch f)` turned out to compile clean and silently
/// discard the `touch`. Its caller's guard masked the difference, so the copy
/// was wrong and harmless at the same time — which is how a fork survives.
fn keys_splat_map_name(c: &Command) -> Option<&str> {
    match &c.args.first()?.kind {
        ExprKind::Literal(name) => Some(name),
        _ => None,
    }
}

/// SB5025 for a `$(keys m)` splat outside a for-in source. `None` when the
/// operand is not a bare map name — that program has a more fundamental
/// error, and SB5062 names it.
fn keys_splat_diag(span: Span, c: &Command) -> Option<Diagnostic> {
    let map = keys_splat_map_name(c)?;
    Some(Diagnostic::error_span(
        span,
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
        ExprKind::ParameterExpansion(pe) => {
            check_argv_splat_opt(pe.index.as_deref(), diags);
            check_argv_splat_opt(pe.operand.as_deref(), diags);
        }
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
         use the bare form (@arr or $(argv)), `$(join arr SEP)` for a joined \
         string, or single quotes ('@arr') if you really meant the literal text"
            .to_string(),
    )
}

/// The `@…` text of a for-in source word that is a DOUBLE-QUOTED literal
/// looking like a splat — `"@g"` or `"@{g:-x}"`.
///
/// Inside double quotes `@` is not an expansion sigil, so the parser yields a
/// plain `Literal` and no splat node ever exists (`--emit-ir` shows
/// `DoubleQuoted([Literal "@g"])`). Every other way of quoting a splat is
/// already diagnosed — `"${g[@]}"` is SB4038, `"$(keys m)"` and `"$(argv)"`
/// are SB5025/SB5026 — so this spelling was the one silent hole, and it is the
/// most natural one to write.
///
/// Purely syntactic: no symbol table is consulted, so `g` need not resolve. In
/// a for-in source that is the right call — a loop over the single constant
/// string `@g` is not something anyone writes on purpose, and it is the
/// POSITION that makes the intent obvious (DG, 2026-09-30). Elsewhere (a
/// command argument, say) `"@g"` is ordinary literal text, matches bash, and
/// stays legal.
fn quoted_at_splat_text(w: &Expr) -> Option<(String, String)> {
    let text = double_quoted_literal(w)?;
    let (name, vouched) = split_at_splat(text.strip_prefix('@')?)?;
    Some((text.to_string(), advised_splat(name, vouched)))
}

/// The text of a word that is a double-quoted string of exactly one literal
/// part, i.e. `"…"` containing no expansion at all.
fn double_quoted_literal(w: &Expr) -> Option<&str> {
    let ExprKind::DoubleQuoted(parts) = &w.kind else {
        return None;
    };
    let [part] = parts.as_slice() else {
        return None;
    };
    match &part.kind {
        ExprKind::Literal(text) => Some(text.as_str()),
        _ => None,
    }
}

/// Split the text after a leading `@` into its collection name and whether a
/// `:!` taint-off modifier follows. `None` if the name is not an identifier,
/// which keeps `"@"`, `"@{}"`, `"a@b"` and `"@g h"` out of the diagnostic.
fn split_at_splat(rest: &str) -> Option<(&str, bool)> {
    let (name, ops) = match rest.strip_prefix('{').and_then(|b| b.strip_suffix('}')) {
        Some(braced) => match braced.find(':') {
            Some(i) => (&braced[..i], &braced[i + 1..]),
            None => (braced, ""),
        },
        None => (rest, ""),
    };
    let is_name = !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_');
    is_name.then(|| (name, ops.starts_with('!')))
}

/// The splat spelling to advise, which must itself be legal in a for-in
/// source. That source rejects the empty-disposition outright (SB5034, DD#69
/// addendum), so advising `@{g:-x}` back would hand the author a form that
/// fails on the next compile. The bare `@name` is always valid; `:!` is a
/// taint MODIFIER rather than a disposition (DD#91) and is legal here, so it
/// is the one operator worth carrying over.
fn advised_splat(name: &str, vouched: bool) -> String {
    if vouched {
        format!("@{{{name}:!}}")
    } else {
        format!("@{name}")
    }
}

/// SB5026 for a double-quoted `"@a"` in a for-in source.
fn quoted_at_splat_diag(span: Span, text: &str, advice: &str) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5026",
        format!(
            "`\"{text}\"` is quoted, so it is the literal text {text} — not an \
             array splat, and this loop would run exactly once. Inside double \
             quotes `@` is not an expansion. Write `{advice}` for the splat, or \
             '{text}' if you really meant the literal"
        ),
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
///
/// **One argument.** The two-argument `keys m k` is the DD#75 out-param fill,
/// a statement; reading it as a splat here would make `for x in $(keys m k)`
/// iterate a fill that produces no value.
fn is_keys_splat_expr(expr: &Expr) -> bool {
    keys_splat_command(expr).is_some()
}

/// The `keys` command of a `$(keys m)` splat, or `None` if `expr` is not one.
///
/// The extractor is the authority and [`is_keys_splat_expr`] is defined in
/// terms of it, so the predicate and the thing it predicates cannot drift
/// apart. They did: the diagnostic re-derived the command with its own
/// traversal, which answered a question this one had already answered.
fn keys_splat_command(expr: &Expr) -> Option<&Command> {
    let ExprKind::CommandSubstitution(stmts) = &expr.kind else {
        return None;
    };
    sole_cmdsub_command(stmts, "keys").filter(|c| c.args.len() == 1)
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
/// because SB5073 catches it one pass earlier — now from `keys_form_diag`
/// rather than from `VALUE_ONLY_INTRINSICS`, which it left when DD#75's
/// `keys m k` made it a legal statement.
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
///
/// The reader is recognised beneath its `-`/`!` mark — `echo hi | - read x`
/// used to bind `x` tainted — but not beneath a redirect: a redirected
/// reader's input is not the pipe, and the parser rejects one as a stage.
fn taint_stmt_no_read_taint(
    stmt: &Statement,
    tainted: &mut HashSet<String>,
    just: &JustTainted,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    if let StmtKind::Command(c) = &stmt.stage_marks().0.kind {
        let name = expr_literal_str(&c.name);
        // read/mapfile from clean pipe source: don't taint
        if matches!(name, "read" | "readline" | "mapfile") {
            taint_cmdsub_bodies(stmt.stage_marks().0, tainted, sx, diags);
            taint_arith_calls(stmt.stage_marks().0, tainted, sx, diags);
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
    match &stmt.stage_body().kind {
        StmtKind::Command(c) => source_head_execs(expr_literal_str(&c.name)),
        _ => false,
    }
}

/// `is_external_source`'s head test: an absolute path, or a PATH-resolved
/// non-builtin.
fn source_head_execs(name: &str) -> bool {
    name.starts_with('/') || (!name.is_empty() && !BUILTINS.contains(&name))
}

fn taint_stmt(
    stmt: &Statement,
    tainted: &mut HashSet<String>,
    just: &JustTainted,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    // A capture runs before the statement that consumes its output, so its
    // body is walked against the set from before the statement's effect: in
    // `y=$(/bin/echo "$y")` the body reads the `y` held before, not the one
    // the assignment writes. A `for`'s words likewise run before its loop. A
    // declaration evaluates and binds one initializer at a time, so
    // `taint_declaration` walks each initializer's bodies itself.
    if !matches!(stmt.kind, StmtKind::Declaration(_)) {
        taint_cmdsub_bodies(stmt, tainted, sx, diags);
        taint_arith_calls(stmt, tainted, sx, diags);
    }
    match &stmt.kind {
        StmtKind::Assignment(a) => taint_assignment(a, tainted, sx, diags),
        StmtKind::Declaration(d) => taint_declaration(d, stmt.span, tainted, sx, diags),
        StmtKind::Command(c) => taint_command(c, stmt.span, tainted, just, sx, diags),
        StmtKind::Function(_) => {}
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
        StmtKind::ReturnProducer(inner) => taint_return_producer(inner, tainted, just, sx, diags),
        _ => taint_child_bodies(stmt, tainted, just, sx, diags),
    }
}

/// Taint-check the statements of every `$( … )` body `stmt` owns directly.
///
/// DD#86: a `$( … )` body is an anonymous function body, so its statements are
/// taint-checked like any others — every read in it, at any depth, by the same
/// transfer functions a top-level statement gets. This is the single place a
/// capture body is walked: the expression checks see a `$( )` only as a value
/// (a bare source used directly is SB5020) and leave its reads to this walk.
/// `taint_stmt` calls it before a statement's own effect, because the capture
/// runs first; the arms that carry a substitution — Command, Assignment, a
/// `for`'s words — are leaves or heads, so no arm could do it for them.
fn taint_cmdsub_bodies(
    stmt: &Statement,
    tainted: &mut HashSet<String>,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    taint_capture_bodies(stmt.cmdsub_bodies(), tainted, sx, diags);
}

/// A capture is single-use (DD#127) and inlined (DD#86), so its taint can
/// escape. Union is conservative for a capture in a lazy operand: it may not
/// run, so its kills cannot clear the enclosing state.
fn taint_capture_bodies(
    bodies: Vec<&[Statement]>,
    tainted: &mut HashSet<String>,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    for body in bodies {
        let frame = BodyCx::default();
        let inner = SlotCx {
            fname: "",
            body: &frame,
            ..*sx
        };
        let mut body_tainted = tainted.clone();
        taint_block(body, &mut body_tainted, &JustTainted::new(), &inner, diags);
        tainted.extend(body_tainted);
        tainted.extend(frame.returns.take());
    }
}

fn taint_child_bodies(
    stmt: &Statement,
    tainted: &mut HashSet<String>,
    just: &JustTainted,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    // `structural_bodies`, not `child_bodies`: a capture body is walked by
    // `taint_cmdsub_bodies`, before the statement that owns it. Taking it here
    // too would check every read in the body twice.
    //
    // DD#121: the branching shapes have their own arms in `taint_stmt`, so
    // everything reaching here is a transparent single-statement wrapper
    // (`Group`, `Suppressed`, `Negated`) — the wrapped
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
/// Does this block reach the statement after it? DD#121 R2: "A path that does
/// not fall through — `exit`, `return`, `break`, `continue` — is exempt."
///
/// Shared with `transform::control_flow`, which needs the same answer to decide
/// whether a diverting `else` is a predecessor of the attest merge. **Do not
/// fork it** — a private duplicate of a shared walker has already cost this
/// project a P1 (journal 456).
///
/// Looks through a trailing `{ … }`: the group is not a scope (journal 453),
/// it is only a way to put several statements where one is expected, so
/// `|| { echo bad >&2; exit 1; }` diverts exactly as `|| exit 1` does. DG
/// (2026-10-02) raised this as the two spellings not being handled
/// identically, which they now are — on the `||` right operand *and* on an
/// `else { … }` body, both of which route through here.
///
/// Still syntactic beyond that: a trailing `if` that diverts on every arm
/// reads as falling through. Widening it means deciding divergence for
/// arbitrary control flow, which is a different question.
pub(crate) fn block_falls_through(stmts: &[Statement]) -> bool {
    match stmts.last().map(|s| &s.kind) {
        Some(StmtKind::Command(c)) => !matches!(
            expr_literal_str(&c.name),
            "exit" | "return" | "break" | "continue"
        ),
        Some(StmtKind::Group(g)) => block_falls_through(&g.body),
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
///
/// A background job runs in a child: check its reads, but discard its effects.
fn taint_list(
    l: &List,
    span: Span,
    tainted: &mut HashSet<String>,
    just: &JustTainted,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    if matches!(l.op, ListOp::Background) {
        taint_discarded(&l.left, true, tainted, sx, diags);
        taint_block(
            std::slice::from_ref(l.right.as_ref()),
            tainted,
            &JustTainted::new(),
            sx,
            diags,
        );
        return;
    }
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
    // DD#121 R2: the right operand is a branch like any other, so if it diverts
    // (`exit` / `return` / `break` / `continue`) its out-state never arrives at
    // the merge — the same rule `merge_paths` applies to a diverting `else`.
    //
    // Merging it unconditionally undid the exemption R2 explicitly grants ("A
    // path that does not fall through … is exempt"). `check_list_caught_resolution`
    // honoured it, correctly staying silent, and then this line put the subject
    // back in the set, so the next read was SB5020. That rejected
    // `attest x [[ … ]] || exit 1` — the very form the SB5089 text recommends.
    if block_falls_through(std::slice::from_ref(l.right.as_ref())) {
        tainted.extend(right);
    }
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
/// and so is a bare source used as a word. The reads inside a `$(keys m)` are
/// checked where its body is walked ([`taint_cmdsub_bodies`]).
fn check_for_words(words: &[Expr], tainted: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    for w in words {
        reject_tainted_array_iter(w, tainted, diags);
        if let ExprKind::CommandSubstitution(stmts) = &w.kind {
            check_bare_cmdsub_taint(stmts, w.span, diags);
        }
    }
}

/// DD#61: `for x in @a` where `a` is a stored tainted array is a read of a
/// stored variable — rejected; the array must be attest-all'd first.
fn reject_tainted_array_iter(w: &Expr, tainted: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    let Some(arr) = splat_array_name(w) else {
        return;
    };
    if splat_vouched(w) {
        return;
    }
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

/// DD#91 (2026-09-09 addendum): does this splat carry the `!` taint-off
/// MODIFIER (`@{a:!}`)?
///
/// Deliberately separate from `splat_array_name`, which answers a purely
/// structural question ("is this word a value splat, and of what?"). Folding
/// the check in there would make a *policy* answer look structural, and the
/// addendum exists precisely because `!` was once conflated with a
/// disposition: the retired `arrvals_trust` operator meant "elide AND trust",
/// which forced the trust to inherit the elide's position restriction. `!` is
/// a modifier, so a vouched splat is legal wherever a bare one is — including
/// here, as a `for`-in source.
fn splat_vouched(w: &Expr) -> bool {
    matches!(&w.kind, ExprKind::ParameterExpansion(pe) if pe.taint_off)
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
    span: Span,
    tainted: &mut HashSet<String>,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    // Evaluated, then bound, before the next initializer — so a later
    // binding's capture reads the taint an earlier one left.
    for a in &d.assignments {
        if let Some(eq) = a.as_assignment() {
            taint_capture_bodies(eq.value.cmdsub_bodies(), tainted, sx, diags);
            taint_expr_arith_calls(&eq.value, span, tainted, sx, diags);
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
    // The RHS is read before the target is written: in `y="$y$(/bin/echo b)"`
    // the `$y` is the value `y` held before — clean here — and only the result
    // comes from a source.
    reject_scalar_rhs_read(&a.value, tainted, diags);
    let from_source = is_taint_source(&a.value);
    if from_source {
        tainted.insert(a.name.clone());
    }
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
        // Nothing to reject, for five different reasons:
        //   `Literal`/`SingleQuoted` — no expansion, so no read.
        //   `Array`/`AssocArray` — never reach here;
        //     `taint_array_literal_assignment` handles them (SB5021) and
        //     returns before this call.
        //   `CommandSubstitution` — a `$( )` in value position is the
        //     sanctioned assignment-from-source: a source taints the target
        //     (`is_taint_source`), and the reads in its body are checked where
        //     the body is walked (`taint_cmdsub_bodies`).
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
        | ExprKind::CommandSubstitution(_)
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
    // A subscript index inside the value is a read position (SB5020), which
    // `expr_uses_tainted` above does not report. A read inside a `$( )`
    // element (`a[i]=$(join b)`) is one too, checked where the body is walked.
    check_value_index_reads(e, tainted, diags);
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

/// Taint a pipeline: a `read`/`mapfile` stage is exempt from tainting its
/// targets only when EVERY stage upstream of it is compiler-computed.
///
/// This used to consult `p.commands.first()` alone, which made a leading
/// builtin launder the whole pipeline: `echo ig | /bin/cat secret | read x`
/// bound the file's contents CLEAN, and the value then flowed into argv and
/// test operands unchecked. DD#24 is operational — any output that passed
/// through `execve(2)` is tainted — so the flag must accumulate over the
/// stages the data actually traversed, not describe only the source.
/// Non-final stages run in children; their reads count, but their writes do not.
fn taint_pipeline(
    p: &crate::ir::Pipeline,
    tainted: &mut HashSet<String>,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    let mut upstream_external = false;
    for (i, s) in p.commands.iter().enumerate() {
        let read_taints = i == 0 || upstream_external;
        let in_parent = i + 1 == p.commands.len() && stage_runs_in_parent(s);
        taint_stage(s, read_taints, in_parent, tainted, sx, diags);
        upstream_external |= is_external_source(s);
    }
}

fn stage_runs_in_parent(stmt: &Statement) -> bool {
    let (body, _, negated) = stmt.stage_marks();
    (matches!(body.kind, StmtKind::While(_)) && !negated) || body.is_pipe_reader()
}

fn taint_stage(
    stmt: &Statement,
    read_taints: bool,
    in_parent: bool,
    tainted: &mut HashSet<String>,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    if in_parent {
        walk_stage(stmt, read_taints, tainted, sx, diags);
    } else {
        taint_discarded(stmt, read_taints, tainted, sx, diags);
    }
}

fn walk_stage(
    stmt: &Statement,
    read_taints: bool,
    tainted: &mut HashSet<String>,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    if read_taints {
        taint_stmt(stmt, tainted, &JustTainted::new(), sx, diags);
    } else {
        taint_stmt_no_read_taint(stmt, tainted, &JustTainted::new(), sx, diags);
    }
}

fn taint_discarded(
    stmt: &Statement,
    read_taints: bool,
    tainted: &HashSet<String>,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    let frame = BodyCx::default();
    let inner = SlotCx {
        body: &frame,
        log: None,
        ..*sx
    };
    walk_stage(stmt, read_taints, &mut tainted.clone(), &inner, diags);
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

/// Taint a dispatch's subject, its patterns and each arm body. A pattern is
/// a test operand like the subject (DD#66): `[[ $s == "$t" { … } ]]` reads
/// `t` exactly as the boolean form `[[ $s == "$t" ]]` does. A pattern's
/// capture body is walked with the statement's other captures
/// ([`taint_cmdsub_bodies`]) before dispatch, and the effects are unioned
/// (DD#127). Every operand sees every capture's possible effect, conservatively
/// over-approximating whichever pattern order runs.
fn taint_case_arms(
    c: &crate::ir::Case,
    tainted: &mut HashSet<String>,
    sx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    check_expr_taint(&c.value, tainted, diags);
    for pattern in c.items.iter().flat_map(|item| &item.patterns) {
        check_expr_taint(pattern, tainted, diags);
    }
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
    if name == "attest" {
        taint_attest(c, span, tainted, just, diags);
        return; // Don't check args of attest for taint
    }
    // The words are read before the command runs, so they are checked against
    // the set before its effect: `f "${g[0]}" =g` reads the `g` that held
    // before `f` fills it.
    check_command_reads(c, tainted, diags);
    taint_out_params(c, sx, tainted);
    taint_call(c, name, span, tainted, sx, diags);
    if matches!(name, "read" | "readline" | "mapfile" | "argv") && !is_value_split(name, c) {
        taint_read_target(c, tainted);
    }
}

/// The reads a command makes before it runs: its words, and the collections a
/// reader takes whole.
fn check_command_reads(c: &Command, tainted: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    check_expr_taint(&c.name, tainted, diags);
    for arg in &c.args {
        check_expr_taint(arg, tainted, diags);
    }
    check_collection_reader_taint(c, tainted, diags);
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
    // No early return when nothing else is tainted: the predicate may still
    // read a taint SOURCE inline (`[[ $x == $(getenv Y) ]]`), which vouches
    // for nothing.
    let mut others = tainted.clone();
    for name in attest_exempt_names(form) {
        others.remove(name);
    }
    match pred {
        AttestPredicate::Test(test) => check_test_taint(test, &others, diags),
        AttestPredicate::ArithConvert { value, .. } => {
            check_arith_convert_reads(value, span, &others, diags)
        }
    }
}

/// The reads in a DD#93 Part D conversion's value. Its bare names are the
/// form's own operands — a bare string is SB4030 anywhere else, which is why
/// [`check_arith_taint`] has no arm for one — reported at the attest; and its
/// `${…}` imports are reads like any other, each at its own span. Walking
/// only the bare names let `attest s (( n = ${t:-0} + s ))` through the front
/// end with a tainted `t`, leaving it to the unlocated SSA backstop.
fn check_arith_convert_reads(
    value: &ArithExpr,
    span: Span,
    others: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    crate::ir::walk_arith_var_reads(value, &mut |name| {
        let msg = format!("tainted variable ${name} used without attest");
        push_taint_use(others, name, span, msg, diags)
    });
    check_arith_taint(value, others, diags);
}

/// SB5098 (DD#24): an `attest` predicate tests values; it does not run code.
///
/// DD#24 confines the predicate to string comparisons, regex matches and file
/// tests, with "no external commands". The predicate is also the one place
/// the attested name may be read while still tainted, so code run there
/// would receive the unvetted value: `[[ $x == $(/bin/echo $x) ]]` hands it
/// to an external, `[[ $x == $(f $x) ]]` to a function body free to exec it.
///
/// The rule is strict (DG 2026-10-31). A `[[ ]]` predicate holds no `$( … )`
/// of any kind — not `echo`, not a value intrinsic, not `getenv`, whose value
/// is itself tainted, and not the `${…:s/…/…/}` sugar, which DD#63 makes the
/// same `$(sub …)` call — no `$(<file)`, whose contents are tainted too, and
/// no `$(( … ))`, at any depth: an index or a `${…}` operand counts. A DD#93
/// Part D conversion is arithmetic by construction, so there the rule is that
/// its value calls nothing: no sexpr call of an intrinsic or a function (a
/// `(name)` naming neither is grouping around a read), no bare int-function
/// name, and the same rule again inside each `${…}` it imports (in practice
/// only the sugar can put code there: every other `${…}` spelling that holds
/// code is rejected by its own rule).
///
/// A shape check, so it runs under `--no-taint` too. The transform lowers a
/// predicate's operands in place and has no point at which to run a body:
/// before this rule a body, a function, an external or a statement builtin
/// there reached one of the backends' "should have been lifted" ICEs, which
/// now stand behind it, and a `getenv` or `$(<file)` comparand blessed
/// whatever the attacker supplied.
pub fn check_attest_predicate_code(program: &Program) -> Vec<Diagnostic> {
    let functions: HashSet<&str> = program.function_defs().map(|f| f.name.as_str()).collect();
    let mut found = Vec::new();
    for block in program_blocks(program) {
        crate::ir::walk_stmt_tree(block, &mut |stmt| {
            predicate_code(stmt, &functions, &mut found)
        });
    }
    found
        .into_iter()
        .map(|(span, what)| {
            Diagnostic::error_span(
                span,
                "SB5098",
                format!(
                    "attest predicate must not run code, but {what} — test the attested name \
                     with comparisons, regex matches and file tests only, and bind any other \
                     operand to a variable before the statement that taints the name"
                ),
            )
        })
        .collect()
}

/// Every piece of code a decodable `attest` statement's predicate would run,
/// each with the span of the expression that runs it.
fn predicate_code(stmt: &Statement, functions: &HashSet<&str>, found: &mut Vec<(Span, String)>) {
    let StmtKind::Command(c) = &stmt.kind else {
        return;
    };
    if expr_literal_str(&c.name) != "attest" {
        return;
    }
    let (Some((_, pred)), Some(last)) = (classify_attest(&c.args), c.args.last()) else {
        return;
    };
    match pred {
        AttestPredicate::Test(_) => {
            walk_expr_tree(last, &mut |e| predicate_node(e, functions, found))
        }
        AttestPredicate::ArithConvert { value, .. } => {
            convert_code(value, last.span, functions, found)
        }
    }
}

/// One node of a `[[ ]]` predicate. `walk_expr_tree` supplies the descent,
/// so a banned node nested inside another is reported at its own span too.
///
/// **Total over `ExprKind` on purpose — do not add a wildcard.**
fn predicate_node(e: &Expr, functions: &HashSet<&str>, found: &mut Vec<(Span, String)>) {
    let what = match &e.kind {
        ExprKind::CommandSubstitution(body) => capture_in_predicate(body, functions),
        ExprKind::SlurpFile(_) => "`$(<…)` reads a file".to_string(),
        ExprKind::Arithmetic(_) => "`$(( … ))` evaluates arithmetic".to_string(),
        ExprKind::Literal(_)
        | ExprKind::SingleQuoted(_)
        | ExprKind::SimpleExpansion(_)
        | ExprKind::DoubleQuoted(_)
        | ExprKind::Concat(_)
        | ExprKind::Array(_)
        | ExprKind::AssocArray(_)
        | ExprKind::ParameterExpansion(_)
        | ExprKind::Test(_)
        | ExprKind::Subscript { .. } => return,
    };
    found.push((e.span, what));
}

/// What a `$( … )` in a predicate would run.
fn capture_in_predicate(body: &[Statement], functions: &HashSet<&str>) -> String {
    const BODY: &str = "`$( … )` runs something other than one plain command";
    let [stmt] = body else {
        return BODY.to_string();
    };
    if matches!(stmt.kind, StmtKind::Suppressed(_)) {
        return "`$(- …)` is a suppressed capture".to_string();
    }
    let StmtKind::Command(c) = &stmt.kind else {
        return BODY.to_string();
    };
    let head = expr_literal_str(&c.name);
    if functions.contains(head) {
        return format!("`$({head} …)` calls function `{head}`");
    }
    // DD#63: the `s///` sugar desugars to this very call and leaves no mark,
    // so name both spellings rather than only the one the user may not have
    // written.
    let sugar = match head {
        "sub" => " (a `${…:s/…/…/}` expansion is that call)",
        "gsub" => " (a `${…:s/…/…/g}` expansion is that call)",
        _ => "",
    };
    format!("`$({head} …)` runs `{head}`{sugar}")
}

/// The calls in a DD#93 Part D conversion's value, reported at the
/// predicate, and the code in each `${…}` it imports, at its own span. A
/// sexpr's operands are not walked: those of `(len …)` / `(strlen …)` are
/// command words, not calls, and the sexpr is reported already.
fn convert_code(
    value: &ArithExpr,
    span: Span,
    functions: &HashSet<&str>,
    found: &mut Vec<(Span, String)>,
) {
    crate::ir::walk_arith_nodes_pruned(value, &mut |node| {
        let Some((what, descend)) = arith_call(node, functions) else {
            return true;
        };
        found.push((span, what));
        descend
    });
    crate::ir::walk_arith_expansion_roots(value, &mut |root| {
        walk_expr_tree(root, &mut |e| predicate_node(e, functions, found))
    });
}

/// The call one node of a Part D value makes, if any, and whether
/// [`convert_code`]'s walk descends past it.
fn arith_call(node: &ArithExpr, functions: &HashSet<&str>) -> Option<(String, bool)> {
    let (name, descend) = match node {
        // DD#58: the parser makes every `(name)` a sexpr. One that names
        // neither an intrinsic nor a function is grouping around a read
        // (`resolve_nullary_sexpr`, `ReadSink::is_sexpr_callee`), and one with
        // arguments that names neither is SB5011's.
        ArithExpr::FuncCall { name, .. }
            if crate::resolve::is_intrinsic(name) || functions.contains(name.as_str()) =>
        {
            (name, false)
        }
        ArithExpr::Var(name) if functions.contains(name.as_str()) => (name, true),
        _ => return None,
    };
    let what = if functions.contains(name.as_str()) {
        format!("its arithmetic calls function `{name}`")
    } else {
        format!("its arithmetic calls `{name}`")
    };
    Some((what, descend))
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
        // A source in the index or default remains a source here. Like
        // `pexp_uses_tainted`, this is used by `reject_tainted_into_array`;
        // both must visit the same children without extending the trust flag.
        ExprKind::ParameterExpansion(pe) => {
            // `!` (DD#91) vouches for the read of `pe.name`'s stored value, not
            // for a *source* materialised in the index or default: in
            // `${x:!-$(getenv HOME)}` nobody asked to bypass the getenv. So
            // `taint_off` is deliberately not consulted here.
            pe.index.as_deref().is_some_and(is_taint_source)
                || pe.operand.as_deref().is_some_and(is_taint_source)
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
    // `taint_off` scopes to the stored value only, not the index or operand
    // walks: each nested expansion carries its own flag, which is what
    // makes `${x:!-${y:!}}` the spelling that vouches for both reads.
    (!pe.taint_off && tainted.contains(pe.name.as_str()))
        || pe
            .index
            .as_deref()
            .is_some_and(|i| expr_uses_tainted(i, tainted))
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
        // (SB5020), not a value flowing into a collection (SB5021), and it is
        // checked where the body is walked (`taint_cmdsub_bodies`); the caller
        // runs `check_value_index_reads` for an element's indexes. Answering
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
/// **Total over `ExprKind` on purpose — do not add a wildcard.** This match had a `_ => {}` and it
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
            check_parts_taint(parts, tainted, diags)
        }
        // DD#24: a bare scalar source ($(getenv …)/$(arg N)) used directly at
        // a read site is a use of external data without attest — rejected.
        // The reads *inside* the substitution (`$(join a ,)` where `a` is
        // tainted) are checked where its body is walked, by the same transfer
        // functions as any statement (`taint_cmdsub_bodies`).
        ExprKind::CommandSubstitution(stmts) => check_bare_cmdsub_taint(stmts, expr.span, diags),
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

/// [`check_expr_taint`] on each part of a quoted or concatenated word. Its own
/// function so that the dispatcher above has no nesting.
fn check_parts_taint(parts: &[Expr], tainted: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    for p in parts {
        check_expr_taint(p, tainted, diags);
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
    if let Some(index) = pe.index.as_deref() {
        check_expr_taint(index, tainted, diags);
    }
}

/// Check the read positions inside a value-position expression bound for a
/// collection (array element, subscript value) — the subscript indexes, where
/// even a bare source is a read (SB5020). Descends the concat/quote/array
/// structure to reach a nested index. A `$( )` value is the sanctioned
/// assignment-from-source, and the reads in its body are checked where the body
/// is walked ([`taint_cmdsub_bodies`]).
fn check_value_index_reads(expr: &Expr, tainted: &HashSet<String>, diags: &mut Vec<Diagnostic>) {
    match &expr.kind {
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) | ExprKind::Array(parts) => {
            parts_value_index_reads(parts.iter(), tainted, diags)
        }
        ExprKind::AssocArray(pairs) => {
            parts_value_index_reads(pairs.iter().map(|(_, v)| v), tainted, diags)
        }
        // A subscript *index* is a read position sitting inside a value
        // position, so it gets the read walker: `a[0]=${b[$(getenv N)]}`
        // stores `b`'s element, but it reads the env var to choose which.
        // `reject_scalar_rhs_read` makes the same call for the scalar form.
        ExprKind::Subscript { index, .. } => check_expr_taint(index, tainted, diags),
        // `${x:-…}` — the default is a value position (DD#78), the index is a
        // read position, and `!` vouches for neither of the *inner* reads.
        ExprKind::ParameterExpansion(pe) => pexp_value_index_reads(pe, tainted, diags),
        // `$(( … ))` reaches its operands through expansion leaves.
        ExprKind::Arithmetic(a) => arith_value_index_reads(a, tainted, diags),
        // Nothing to descend: a bare variable read is `expr_uses_tainted`'s
        // question, a `$( )` body is walked as statements, `$(<f)` has no
        // index, `[[ … ]]` cannot appear in a value position (SB4014 at parse),
        // and text is text. Spelled out rather than left to a wildcard — this
        // walker is read as one of the three that must jointly cover a value
        // expression, and the other two both leaked through exactly such a
        // wildcard.
        ExprKind::SimpleExpansion(_)
        | ExprKind::CommandSubstitution(_)
        | ExprKind::SlurpFile(_)
        | ExprKind::Test(_)
        | ExprKind::Literal(_)
        | ExprKind::SingleQuoted(_) => {}
    }
}

/// [`check_value_index_reads`] over each part of a word or each element or
/// value of a collection literal.
fn parts_value_index_reads<'a>(
    parts: impl Iterator<Item = &'a Expr>,
    tainted: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    for p in parts {
        check_value_index_reads(p, tainted, diags);
    }
}

/// The `${…}` arm of [`check_value_index_reads`], split out to keep that
/// dispatcher under the complexity gate — as [`pexp_uses_tainted`] is for
/// [`expr_uses_tainted`].
fn pexp_value_index_reads(
    pe: &ParameterExpansion,
    tainted: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    if let Some(idx) = pe.index.as_deref() {
        check_expr_taint(idx, tainted, diags);
    }
    if let Some(operand) = pe.operand.as_deref() {
        check_value_index_reads(operand, tainted, diags);
    }
}

/// The `$(( … ))` arm of [`check_value_index_reads`]; the read-checking
/// sibling of [`arith_uses_tainted`] and [`arith_is_taint_source`].
fn arith_value_index_reads(
    arith: &ArithExpr,
    tainted: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    crate::ir::walk_arith_expansion_roots(arith, &mut |e| {
        check_value_index_reads(e, tainted, diags);
    });
}

/// A collection reader that exposes tainted *element data* via a bare
/// argument literal: `join a` (the joined element values). Per DD#24 that is a
/// read of tainted elements — if the array is tainted (e.g. an `argv` snapshot
/// without attest-all) this is a use without attest (SB5020). The name is a
/// bare literal, so the generic `check_expr_taint` path (which only flags
/// variable-expansion reads like `@a`/`${a[i]}`) doesn't catch it. `len` is
/// exempt — it exposes only cardinality, not element data.
///
/// **`keys` is deliberately absent.** It had an arm here, and that arm was
/// wrong in every program that could reach it. DD#93 closed every route to a
/// tainted map at *construction* ("there is no program in which the 2-binder
/// form clears taint"), so no map reaching `keys` is ever tainted. What the
/// arm actually caught was a tainted **array** — where `keys` is already a
/// DD#75 §3 type error — and it reported "keys of tainted map 'a'", calling an
/// indexed array a map, preempting the correct SB4030, and advising an
/// `attest k v in a` that is itself rejected. It also masked the SB4030 for a
/// bulk fill into a map, because this pass runs before `type_check`.
fn check_collection_reader_taint(
    c: &Command,
    tainted: &HashSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    let ExprKind::Literal(cmd) = &c.name.kind else {
        return;
    };
    if cmd != "join" {
        return;
    }
    let Some(arg) = c.args.first() else {
        return;
    };
    let ExprKind::Literal(name) = &arg.kind else {
        return;
    };
    let msg = format!(
        "join of tainted collection '{name}' without attest — attest-all \
         it first: `attest … in {name} [[ … ]]`"
    );
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
/// Descends structural bodies (same scope), not functions or captures.
pub(crate) fn collect_scope_names(stmts: &[Statement], set: &mut HashSet<String>) {
    for stmt in stmts {
        collect_stmt_scope_names(stmt, set);
    }
}

/// Total over `StmtKind`; do not add a wildcard.
fn collect_stmt_scope_names(stmt: &Statement, set: &mut HashSet<String>) {
    match &stmt.kind {
        StmtKind::Assignment(a) => {
            set.insert(a.name.clone());
        }
        StmtKind::Declaration(d) => insert_decl_names(d, set),
        StmtKind::For(f) => {
            set.insert(f.variable.clone());
            collect_scope_names(&f.body, set);
        }
        StmtKind::Command(c) => command_bound_names(c, set),
        StmtKind::Function(_) => {} // separate scope — do not collect
        StmtKind::Pipeline(_)
        | StmtKind::If(_)
        | StmtKind::While(_)
        | StmtKind::Case(_)
        | StmtKind::List(_)
        | StmtKind::Group(_)
        | StmtKind::Redirected(_)
        | StmtKind::Suppressed(_)
        | StmtKind::Negated(_)
        | StmtKind::ReturnProducer(_)
        | StmtKind::ArithTest(_) => collect_child_scope_names(stmt, set),
    }
}

fn insert_decl_names(d: &Declaration, set: &mut HashSet<String>) {
    for a in &d.assignments {
        set.insert(a.name.clone());
    }
}

fn collect_child_scope_names(stmt: &Statement, set: &mut HashSet<String>) {
    for body in stmt.structural_bodies() {
        collect_scope_names(body, set);
    }
}

/// Every name a command binds: [`collect_read_targets`]' out-params and
/// DD#93 Part D's attest-convert TARGET. The one list, shared by scope
/// collection, SB5092 and SB4044.
fn command_bound_names(c: &Command, set: &mut HashSet<String>) {
    collect_read_targets(c, set);
    set.extend(attest_convert_target(c).map(str::to_string));
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
        // DD#75: `keys m k` snapshots the map's keys into the array named
        // last. One argument is the `$(keys m)` splat, which binds nothing.
        "keys" if c.args.len() == 2 => insert_last_literal_arg(&c.args, set),
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

/// A literal argument to `read`/`readline` names a variable the command
/// introduces.
///
/// There is no flag filter here: DD#99 gave the read family **no flags at
/// all** (NUL is the only separator, and there is nothing to configure). A
/// leading `-` is therefore just an unusual variable name, and `read -a arr
/// -d :` is a four-argument `read`, rejected on arity by SB5057. The filter
/// this used to carry described bash's `read`, not sash's.
fn insert_read_target_arg(arg: &Expr, set: &mut HashSet<String>) {
    if let ExprKind::Literal(s) = &arg.kind {
        set.insert(s.clone());
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
/// inside a `$( )` capture. Reuses the type checker's canonical name accessor.
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
        .map(crate::type_check::expr_to_name)
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
        // DD#75: `keys m k` likewise. One argument is the splat form.
        "keys" if c.args.len() == 2 => insert_last_literal_arg(&c.args, set),
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
    walk_arith_funccalls(a, &mut |_, args| {
        let read = args
            .iter()
            .filter_map(|arg| collection_dollar_var(arg, collections));
        diags.extend(read.map(|name| collection_scalar_read_diag(name, span)));
    });
}

/// `name` when `arg` is `$name` and `name` is a collection.
fn collection_dollar_var<'a>(arg: &'a ArithExpr, collections: &HashSet<String>) -> Option<&'a str> {
    let ArithExpr::DollarVar(name) = arg else {
        return None;
    };
    collections.contains(name).then_some(name.as_str())
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
/// - **SB5099 — no aliases.** Slots have distinct identities and do not overlap
///   globals accessed by the callee or anything it calls.
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
    let global_uses = collection_global_uses(program, &funcs);
    let mut diags = Vec::new();
    let cx = CallArgCx {
        funcs: &funcs,
        colls: &colls,
        usage: &usage,
        global_uses: &global_uses,
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
    global_uses: &'a CollectionGlobalUses,
}

#[derive(Clone)]
struct GlobalAccess {
    function: String,
    span: Span,
}

type CollectionGlobalUses = HashMap<String, BTreeMap<String, GlobalAccess>>;

/// DD#100's non-aliasing contract is static and independent of taint.
/// Resolved local names cannot collide with globals. Each function's footprint
/// includes every definition and every reachable callee, without call contexts.
fn collection_global_uses(program: &Program, funcs: &HashSet<String>) -> CollectionGlobalUses {
    let mut globals = HashSet::new();
    for block in program_blocks(program) {
        collect_global_names(block, &mut globals);
    }
    let mut direct = CollectionGlobalUses::new();
    for f in program.function_defs() {
        let uses = direct.entry(f.name.clone()).or_default();
        walk_stmt_tree(&f.body, &mut |stmt| {
            record_global_accesses(stmt, &f.name, &globals, funcs, uses);
        });
    }
    let census = take_census(program);
    direct
        .keys()
        .map(|name| {
            (
                name.clone(),
                reachable_global_accesses(name, &direct, &census.callees),
            )
        })
        .collect()
}

fn record_global_accesses(
    stmt: &Statement,
    function: &str,
    globals: &HashSet<String>,
    funcs: &HashSet<String>,
    uses: &mut BTreeMap<String, GlobalAccess>,
) {
    let mut names = HashSet::new();
    collect_reads(std::slice::from_ref(stmt), &mut names);
    stmt_bound_names(stmt, &mut |name| {
        names.insert(name.to_string());
    });
    if let StmtKind::Command(c) = &stmt.kind {
        collection_identity_uses(c, funcs, &mut names);
    }
    for name in names.intersection(globals) {
        uses.insert(
            name.clone(),
            GlobalAccess {
                function: function.to_string(),
                span: stmt.span,
            },
        );
    }
}

/// Identity forwarding and removal need not read a value, but still name the
/// collection. An external or builtin's `=name` argument remains a literal.
fn collection_identity_uses(c: &Command, funcs: &HashSet<String>, names: &mut HashSet<String>) {
    let head = expr_literal_str(&c.name);
    if funcs.contains(head) {
        names.extend(collection_args(&c.args));
    }
    if head == "unset" {
        names.extend(
            crate::transform::unset_targets(c)
                .into_iter()
                .map(|target| {
                    target
                        .split_once('[')
                        .map_or(target.as_str(), |(base, _)| base)
                        .to_string()
                }),
        );
    }
}

fn reachable_global_accesses(
    name: &str,
    direct: &CollectionGlobalUses,
    callees: &HashMap<String, BTreeSet<String>>,
) -> BTreeMap<String, GlobalAccess> {
    let mut uses = BTreeMap::new();
    let mut seen = HashSet::new();
    let mut pending = vec![name];
    while let Some(current) = pending.pop() {
        if !seen.insert(current) {
            continue;
        }
        uses.extend(direct[current].clone());
        pending.extend(
            callees
                .get(current)
                .into_iter()
                .flatten()
                .map(String::as_str),
        );
    }
    uses
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
    check_distinct_slots(fname, &passed, span, diags);
    check_slot_global_overlap(fname, &passed, span, &cx.global_uses[fname], diags);
}

fn check_distinct_slots(fname: &str, passed: &[String], span: Span, diags: &mut Vec<Diagnostic>) {
    let mut first = HashMap::new();
    for (index, name) in passed.iter().enumerate() {
        let previous = *first.entry(name).or_insert(index);
        if previous != index {
            diags.push(Diagnostic::error_span(
                span,
                "SB5099",
                format!(
                    "`{fname}` passes collection `{name}` more than once \
                     (collection arguments {} and {}); slots must have distinct \
                     identities (DD#100)",
                    previous + 1,
                    index + 1,
                ),
            ));
        }
    }
}

fn check_slot_global_overlap(
    fname: &str,
    passed: &[String],
    span: Span,
    uses: &BTreeMap<String, GlobalAccess>,
    diags: &mut Vec<Diagnostic>,
) {
    for name in passed {
        if let Some(access) = uses.get(name) {
            diags.push(Diagnostic::error_span(
                span,
                "SB5099",
                format!(
                    "`{fname}` is passed collection `{name}` but also accesses \
                     global `{name}` in `{}` at {}:{}; use only the slot or a \
                     forwarded slot throughout the call (DD#100)",
                    access.function, access.span.line, access.span.col,
                ),
            ));
        }
    }
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

/// Static call/slot facts and the current body's return and diagnostic state.
#[derive(Clone, Copy)]
struct SlotCx<'a> {
    info: &'a SlotInfo,
    map_names: &'a HashSet<String>,
    globals: &'a HashSet<String>,
    fns: &'a HashMap<String, CallSites>,
    summaries: &'a HashMap<String, FnSummary>,
    fname: &'a str,
    body: &'a BodyCx,
    log: Option<&'a SiteLog>,
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

// ── DD#127: global taint across function boundaries ──────────────

#[derive(Default)]
struct CallSites {
    defs: usize,
    calls: Vec<Span>,
    int_fn: bool,
}

impl CallSites {
    fn single_use(&self) -> bool {
        self.defs == 1 && self.calls.len() == 1
    }
}

#[derive(Default)]
struct Census {
    fns: HashMap<String, CallSites>,
    callees: HashMap<String, BTreeSet<String>>,
}

impl Census {
    fn record_call(&mut self, owner: &str, name: &str, span: Span) {
        if owner == name {
            return;
        }
        let Some(sites) = self.fns.get_mut(name) else {
            return;
        };
        sites.calls.push(span);
        if !owner.is_empty() {
            self.callees
                .entry(owner.to_string())
                .or_default()
                .insert(name.to_string());
        }
    }

    fn record_stmt(&mut self, stmt: &Statement, owner: &str) {
        if let StmtKind::Command(c) = &stmt.kind {
            self.record_call(owner, expr_literal_str(&c.name), stmt.span);
        }
        let mut names = Vec::new();
        crate::resolve::walk_arith_value_calls(
            stmt,
            &|name| is_int_fn(name, &self.fns),
            &mut |name| names.push(name.to_string()),
        );
        for name in names {
            self.record_call(owner, &name, stmt.span);
        }
    }
}

fn take_census(program: &Program) -> Census {
    let mut census = Census::default();
    for f in program.function_defs() {
        let sites = census.fns.entry(f.name.clone()).or_default();
        sites.defs += 1;
        sites.int_fn |= matches!(f.return_type, VarType::Int);
    }
    for block in program_blocks(program) {
        crate::ir::walk_scoped_stmts(block, "", &mut |stmt, owner| {
            census.record_stmt(stmt, owner);
        });
    }
    census
}

fn is_int_fn(name: &str, fns: &HashMap<String, CallSites>) -> bool {
    fns.get(name).is_some_and(|sites| sites.int_fn)
}

#[derive(Default)]
struct FnSummary {
    gens: HashSet<String>,
    flows: HashMap<String, HashSet<String>>,
    reads: HashMap<String, Vec<Diagnostic>>,
}

impl FnSummary {
    fn merge(&mut self, other: Self) {
        self.gens.extend(other.gens);
        for (name, flow) in other.flows {
            self.flows.entry(name).or_default().extend(flow);
        }
        for (name, reads) in other.reads {
            self.reads.entry(name).or_default().extend(reads);
        }
    }
}

#[derive(Default)]
struct BodyCx {
    returns: RefCell<HashSet<String>>,
    back_edges: RefCell<HashSet<String>>,
}

#[derive(Default)]
struct SiteLog {
    next: Cell<usize>,
    last: RefCell<HashMap<String, (usize, Span, String)>>,
}

impl SiteLog {
    fn bump(&self) -> usize {
        let mark = self.next.get();
        self.next.set(mark + 1);
        mark
    }

    fn record(&self, name: &str, mark: usize, span: Span, via: &str) {
        let mut sites = self.last.borrow_mut();
        if sites.get(name).is_none_or(|old| old.0 <= mark) {
            sites.insert(name.to_string(), (mark, span, via.to_string()));
        }
    }

    fn site(&self, name: &str, fallback: Span) -> (Span, String) {
        self.last
            .borrow()
            .get(name)
            .map(|(_, span, via)| (*span, via.clone()))
            .unwrap_or_else(|| (fallback, "this statement".to_string()))
    }
}

struct BodyWalk {
    exit: HashSet<String>,
    diags: Vec<Diagnostic>,
    log: SiteLog,
}

struct SummaryBuilder<'a> {
    defs: HashMap<&'a str, Vec<&'a Function>>,
    callees: &'a HashMap<String, BTreeSet<String>>,
    base: SlotCx<'a>,
    done: HashMap<String, FnSummary>,
    started: HashSet<String>,
}

impl SummaryBuilder<'_> {
    fn build(&mut self, name: &str) {
        if self.done.contains_key(name) || !self.started.insert(name.to_string()) {
            return;
        }
        let callees: Vec<_> = self
            .callees
            .get(name)
            .into_iter()
            .flatten()
            .cloned()
            .collect();
        for callee in callees {
            self.build(&callee);
        }
        let mut summary = FnSummary::default();
        let cx = SlotCx {
            summaries: &self.done,
            ..self.base
        };
        for f in &self.defs[name] {
            summary.merge(summarise_def(f, &cx));
        }
        self.done.insert(name.to_string(), summary);
    }
}

fn build_summaries(
    program: &Program,
    census: &Census,
    base: &SlotCx,
) -> HashMap<String, FnSummary> {
    let mut builder = SummaryBuilder {
        defs: HashMap::new(),
        callees: &census.callees,
        base: *base,
        done: HashMap::new(),
        started: HashSet::new(),
    };
    for f in program.function_defs() {
        builder.defs.entry(&f.name).or_default().push(f);
    }
    let names: BTreeSet<_> = builder.defs.keys().copied().collect();
    for name in names {
        builder.build(name);
    }
    builder.done
}

fn summarise_def(f: &Function, cx: &SlotCx) -> FnSummary {
    let base = walk_body(f, HashSet::new(), &JustTainted::new(), cx);
    let mut summary = FnSummary {
        gens: base.exit.intersection(cx.globals).cloned().collect(),
        ..FnSummary::default()
    };
    let names: BTreeSet<_> = cx.globals.iter().collect();
    for name in names {
        let walk = walk_body(f, HashSet::from([name.clone()]), &JustTainted::new(), cx);
        let flow = walk
            .exit
            .intersection(cx.globals)
            .filter(|out| *out == name || !summary.gens.contains(*out))
            .cloned()
            .collect();
        summary.flows.insert(name.clone(), flow);
        summary
            .reads
            .insert(name.clone(), diag_difference(walk.diags, &base.diags));
    }
    summary
}

fn walk_body(
    f: &Function,
    mut entry: HashSet<String>,
    just: &JustTainted,
    cx: &SlotCx,
) -> BodyWalk {
    loop {
        let (walk, back) = walk_body_once(f, &entry, just, cx);
        if back.is_subset(&entry) {
            return walk;
        }
        entry.extend(back);
    }
}

fn walk_body_once(
    f: &Function,
    entry: &HashSet<String>,
    just: &JustTainted,
    cx: &SlotCx,
) -> (BodyWalk, HashSet<String>) {
    let body = BodyCx::default();
    let log = SiteLog::default();
    let inner = SlotCx {
        fname: &f.name,
        body: &body,
        log: Some(&log),
        probing: false,
        ..*cx
    };
    let mut tainted = entry.clone();
    let mut diags = Vec::new();
    taint_block(&f.body, &mut tainted, just, &inner, &mut diags);
    let mut exit = body.returns.take();
    if block_falls_through(&f.body) {
        exit.extend(tainted);
    }
    (BodyWalk { exit, diags, log }, body.back_edges.take())
}

fn diag_difference(diags: Vec<Diagnostic>, baseline: &[Diagnostic]) -> Vec<Diagnostic> {
    let mut remaining: Vec<_> = baseline.iter().collect();
    diags
        .into_iter()
        .filter(|diag| match remaining.iter().position(|old| *old == diag) {
            Some(i) => {
                remaining.remove(i);
                false
            }
            None => true,
        })
        .collect()
}

fn as_function_def(stmt: &Statement) -> Option<(&Function, Span)> {
    match &stmt.kind {
        StmtKind::Function(f) => Some((f, stmt.span)),
        _ => None,
    }
}

fn check_function_def(
    f: &Function,
    span: Span,
    final_set: &HashSet<String>,
    cx: &SlotCx,
) -> Vec<Diagnostic> {
    let base = walk_body(f, HashSet::new(), &JustTainted::new(), cx);
    let mut diags = base.diags.clone();
    report_shared_globals(f, span, &base, cx, &mut diags);
    check_slot_entry(f, span, final_set, &base, cx, &mut diags);
    diags.sort_by(|a, b| (a.line, a.col, &a.message).cmp(&(b.line, b.col, &b.message)));
    diags
}

fn report_shared_globals(
    f: &Function,
    span: Span,
    walk: &BodyWalk,
    cx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(why) = shared_reason(&cx.fns[&f.name]) else {
        return;
    };
    let names: BTreeSet<_> = walk.exit.intersection(cx.globals).collect();
    for name in names {
        let (site, via) = walk.log.site(name, span);
        diags.push(global_taint_diag(site, &f.name, name, &via, &why));
    }
}

fn shared_reason(sites: &CallSites) -> Option<String> {
    if sites.single_use() {
        return None;
    }
    if sites.defs != 1 {
        return Some("it is defined more than once".to_string());
    }
    if sites.calls.is_empty() {
        return Some("it has no call site".to_string());
    }
    let calls: Vec<_> = sites
        .calls
        .iter()
        .map(|span| format!("{}:{}", span.line, span.col))
        .collect();
    Some(format!("it is called at {}", calls.join(", ")))
}

fn check_slot_entry(
    f: &Function,
    span: Span,
    final_set: &HashSet<String>,
    base: &BodyWalk,
    cx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    let slots: HashSet<_> = tainted_slots_on_entry(&f.name, cx.info, final_set)
        .into_iter()
        .collect();
    if slots.is_empty() {
        return;
    }
    let walk = walk_body(f, slots.clone(), &slots, cx);
    diags.extend(diag_difference(walk.diags, &base.diags));
    let names: BTreeSet<_> = walk
        .exit
        .intersection(cx.globals)
        .filter(|name| !base.exit.contains(*name))
        .collect();
    for name in names {
        let (site, via) = walk.log.site(name, span);
        diags.push(slot_global_taint_diag(site, &f.name, name, &via));
    }
}

fn slot_global_taint_diag(span: Span, fname: &str, target: &str, via: &str) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5067",
        format!(
            "`{fname}` leaves the global `{target}` tainted via {via}, with \
             taint a caller passes in through a collection parameter — \
             attest it here, in the statement immediately below \
             (`attest … [[ … ]]`), or attest the collection in the caller \
             before the call"
        ),
    )
}

fn log_sites(
    log: &SiteLog,
    mark: usize,
    stmt: &Statement,
    before: &HashSet<String>,
    after: &HashSet<String>,
    taints: &JustTainted,
    globals: &HashSet<String>,
) {
    let via = site_via(stmt);
    for name in after.intersection(globals) {
        if taints.contains(name) || !before.contains(name) {
            log.record(name, mark, stmt.span, &via);
        }
    }
}

fn site_via(stmt: &Statement) -> String {
    match &stmt.stage_body().kind {
        StmtKind::ReturnProducer(inner) => site_via(inner),
        StmtKind::Command(c) => match expr_literal_str(&c.name) {
            "" => "this statement".to_string(),
            name => format!("`{name}`"),
        },
        StmtKind::Assignment(_) | StmtKind::Declaration(_) => "an assignment".to_string(),
        StmtKind::Pipeline(p) => p
            .commands
            .last()
            .map(site_via)
            .unwrap_or_else(|| "this statement".to_string()),
        StmtKind::For(_) => "a `for` binder".to_string(),
        _ => "this statement".to_string(),
    }
}

fn taint_call(
    c: &Command,
    name: &str,
    span: Span,
    tainted: &mut HashSet<String>,
    cx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    if name == "return" {
        record_exit(tainted, is_self_tail_return(c, cx), cx);
    } else if cx.fns.contains_key(name) {
        apply_call(name, span, tainted, cx, diags);
    }
}

fn is_self_tail_return(c: &Command, cx: &SlotCx) -> bool {
    let [arg] = c.args.as_slice() else {
        return false;
    };
    let ExprKind::Arithmetic(arith) = &arg.kind else {
        return false;
    };
    is_int_fn(cx.fname, cx.fns) && arith_calls_self(arith, cx.fname)
}

fn arith_calls_self(mut arith: &ArithExpr, name: &str) -> bool {
    while let ArithExpr::Paren(inner) = arith {
        arith = inner;
    }
    match arith {
        ArithExpr::FuncCall { name: callee, .. } | ArithExpr::Var(callee) => callee == name,
        _ => false,
    }
}

fn record_exit(tainted: &HashSet<String>, self_tail: bool, cx: &SlotCx) {
    let target = if self_tail {
        &cx.body.back_edges
    } else {
        &cx.body.returns
    };
    target.borrow_mut().extend(tainted.iter().cloned());
}

fn taint_return_producer(
    inner: &Statement,
    tainted: &mut HashSet<String>,
    just: &JustTainted,
    cx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    taint_block(std::slice::from_ref(inner), tainted, just, cx, diags);
    record_exit(tainted, is_self_tail_producer(inner, cx.fname), cx);
}

fn is_self_tail_producer(inner: &Statement, fname: &str) -> bool {
    let StmtKind::Command(c) = &inner.kind else {
        return false;
    };
    !fname.is_empty() && expr_literal_str(&c.name) == fname && collection_args(&c.args).is_empty()
}

fn apply_call(
    name: &str,
    span: Span,
    tainted: &mut HashSet<String>,
    cx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    if name == cx.fname {
        return;
    }
    let Some(summary) = cx.summaries.get(name) else {
        return;
    };
    let arrived: BTreeSet<_> = tainted.intersection(cx.globals).cloned().collect();
    if !cx.probing {
        relay_reads(name, span, &arrived, summary, diags);
    }
    tainted.retain(|name| !cx.globals.contains(name));
    for name in &arrived {
        tainted.extend(summary.flows[name].iter().cloned());
    }
    if cx.fns[name].single_use() {
        tainted.extend(summary.gens.iter().cloned());
    }
}

fn relay_reads(
    fname: &str,
    span: Span,
    arrived: &BTreeSet<String>,
    summary: &FnSummary,
    diags: &mut Vec<Diagnostic>,
) {
    for name in arrived {
        let reads = summary.reads.get(name).into_iter().flatten();
        diags.extend(reads.map(|diag| {
            Diagnostic::error_span(
                span,
                diag.code,
                format!(
                    "`{fname}`, called here with `{name}` tainted, reports at {}:{}: {}",
                    diag.line, diag.col, diag.message
                ),
            )
        }));
    }
}

fn taint_arith_calls(
    stmt: &Statement,
    tainted: &mut HashSet<String>,
    cx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    crate::resolve::walk_arith_value_calls(stmt, &|name| is_int_fn(name, cx.fns), &mut |name| {
        taint_value_call(name, stmt.span, tainted, cx, diags)
    });
}

fn taint_expr_arith_calls(
    expr: &Expr,
    span: Span,
    tainted: &mut HashSet<String>,
    cx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    crate::resolve::walk_expr_arith_value_calls(
        expr,
        &|name| is_int_fn(name, cx.fns),
        &mut |name| taint_value_call(name, span, tainted, cx, diags),
    );
}

/// A value call may be skipped, and an earlier operand may read its entry
/// state. Without an ordered expression walk, only its taints may escape.
fn taint_value_call(
    name: &str,
    span: Span,
    tainted: &mut HashSet<String>,
    cx: &SlotCx,
    diags: &mut Vec<Diagnostic>,
) {
    let mut after = tainted.clone();
    apply_call(name, span, &mut after, cx, diags);
    tainted.extend(after);
}

/// Top-level bindings, including control flow but not function bodies.
/// Structural on purpose: a name introduced inside `$( … )` is local
/// to that body (DD#86), not a global.
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

/// The names a bulk fill (`argv g`, `read g`, `mapfile g`) writes from outside
/// the program. This is the same test as `taint_command`'s read branch, so
/// the DD#100 slot analysis and the taint walk cannot disagree.
///
/// DD#75's `keys m k` is deliberately absent. It is a fill, but not one from
/// *outside*: the keys it copies were put in the map by this program, so the
/// snapshot is clean (DD#93) and neither analysis taints it.
fn bulk_fill_targets(c: &Command) -> HashSet<String> {
    let name = expr_literal_str(&c.name);
    if !matches!(name, "read" | "readline" | "mapfile" | "argv") || is_value_split(name, c) {
        return HashSet::new();
    }
    let mut targets = HashSet::new();
    taint_read_target(c, &mut targets);
    targets
}

fn global_taint_diag(span: Span, fname: &str, target: &str, via: &str, why: &str) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5067",
        format!(
            "`{fname}` leaves the global `{target}` tainted via {via} — \
             `{fname}` is shared ({why}), and taint may not escape a shared \
             body (DD#127), so that a reader can see \
             where external data enters. Attest it here, in the statement \
             immediately below (`attest … [[ … ]]`), or fill a local, attest \
             that, and assign the clean value to `{target}`"
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
/// a name. (`expr_to_string` has since been narrowed to `literal_name_of`,
/// which ICEs rather than render — but the rule is what keeps it unreached.)
/// That silently found nothing (`len "$s"` read as **0**, `join "a"`
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

/// Which words of `c` are out-params, and whether `a[i]` is allowed among
/// them. `None` for a command that fills nothing.
///
/// `unset` takes several targets and permits the subscript form `a[i]`; the
/// fill builtins take one, and a bare name only.
fn fill_target_spec(c: &Command) -> Option<(Vec<&str>, bool)> {
    let spec = match expr_literal_str(&c.name) {
        "argv" => (argv_target_name(&c.args).into_iter().collect(), false),
        // DD#75: `keys m k` — the out-array is the second argument. One
        // argument is the splat and has no target.
        "keys" if c.args.len() == 2 => (last_arg_target(c), false),
        // DD#63: the out-array is the third argument. `match S P` is the
        // two-arg boolean form and has no target at all, and a wrong arity
        // is SB4045's to report — do not preempt it.
        "match" | "matchall" if c.args.len() == 3 => (last_arg_target(c), false),
        "unset" => (c.args.iter().map(expr_literal_str).collect(), true),
        _ => return None,
    };
    Some(spec)
}

/// The trailing word, as the sole target.
fn last_arg_target(c: &Command) -> Vec<&str> {
    c.args.last().map(expr_literal_str).into_iter().collect()
}

/// A target is a binding, so it must be a bare name; `unset` additionally
/// accepts `a[i]`.
fn is_valid_fill_target(word: &str, allow_subscript: bool) -> bool {
    is_plain_identifier(word) || (allow_subscript && is_subscripted_name(word))
}

fn flag_fill_target(c: &Command, span: Span, diags: &mut Vec<Diagnostic>) {
    let Some((targets, allow_subscript)) = fill_target_spec(c) else {
        return;
    };
    let builtin = expr_literal_str(&c.name);
    for target in targets {
        if !is_valid_fill_target(target, allow_subscript) {
            diags.push(fill_target_diag(builtin, span));
        }
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
                if name == "args" {
                    return;
                }
                match classify_read(name, span, known) {
                    ReadVerdict::Silent => {}
                    ReadVerdict::Reserved(d) => diags.push(d),
                    ReadVerdict::Undefined => diags.push(undefined_splat_diag(name, span)),
                }
            }
        }
    }

    fn record_param(&mut self, name: &str, span: Span, known: &HashSet<String>, opt_out: bool) {
        match &mut self.mode {
            SinkMode::Collect(set) => {
                set.insert(name.to_string());
            }
            SinkMode::Flag(diags) => match classify_read(name, span, known) {
                ReadVerdict::Silent => {}
                ReadVerdict::Reserved(d) => diags.push(d),
                // The `${x:-…}` opt-out applies only here, and only to a name
                // that is genuinely the author's — see `ReadVerdict::Reserved`.
                ReadVerdict::Undefined if !opt_out => diags.push(undefined_scalar_diag(name, span)),
                ReadVerdict::Undefined => {}
            },
        }
    }
}

/// What a read of an unintroduced name should produce, before the caller adds
/// its own wording. The scalar sink (`$x`) and the splat sink (`@a`) carried
/// near-identical guard lists and then both had to grow the same DD#125
/// branch; a shared classifier is what stops the third copy from drifting.
enum ReadVerdict {
    /// Not a diagnosable read: not a plain identifier, compiler-managed, or
    /// already in scope.
    Silent,
    /// DD#125: the compiler owns the name, so neither sink's advice is
    /// takeable — SB5093 answers with advice that is.
    Reserved(Diagnostic),
    /// The author's own name, genuinely unintroduced — the caller's rule.
    Undefined,
}

/// Classify one read. The reserved check sits *after* the scope guards (a
/// name the program declared is SB5092's to report, once) and *before* any
/// opt-out: `${getpid:-5}` is not an intentional default, it is a read of a
/// name that can never be bound, and it quietly produced the default.
fn classify_read(name: &str, span: Span, known: &HashSet<String>) -> ReadVerdict {
    if !is_plain_identifier(name) || handled_elsewhere(name) || known.contains(name) {
        return ReadVerdict::Silent;
    }
    match crate::resolve::reserved_read_advice(name) {
        Some(advice) => ReadVerdict::Reserved(Diagnostic::error_span(span, "SB5093", advice)),
        None => ReadVerdict::Undefined,
    }
}

/// SB5031: a scalar read of a name nothing introduces.
fn undefined_scalar_diag(name: &str, span: Span) -> Diagnostic {
    Diagnostic::error_span(
        span,
        "SB5031",
        format!(
            "'${}' is assigned nowhere (possible typo) — assign it first, \
             or write ${{{}:-DEFAULT}} for an intentional default",
            name, name
        ),
    )
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
        SubscriptAssign { .. } => unreachable!(
            "SB4020 rejects an assignment inside `$(( … ))`, and the statement \
             form `(( a[i] = 7 ))` is read by check_arith_compound — so no \
             SubscriptAssign reaches the read walker. A correct-looking \
             recursion here would be an unverified claim: nothing exercises \
             it, so if the route ever opened it would open in silence, which \
             for a read walker is the worst outcome."
        ),
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
            check_sexpr_arg_reads(name, args, span, known, sink);
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
    if let Some(index) = pe.index.as_deref() {
        check_expr_reads(index, known, sink);
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

/// DD#80: a subscript index/key must be a DIRECT, unambiguous
/// value — an integer literal, a `$var`, an expansion (`${…}`/`$(…)`/`$(( … ))`),
/// a quoted string, or a clean-identifier bareword. An implicit/ambiguous bare
/// form (brackets, operators, whitespace, or a mixed literal+expansion) is
/// rejected here (SB4041). The parser re-parses the index as a value expression,
/// so an explicit nested context is a clean expansion node and passes; an
/// ambiguous bare form falls through to a `Literal` (with stray chars) or a
/// `Concat`, which this check rejects. Type-level "string key on an int array"
/// is a separate rule, emitted by type_check as **SB4030**. (DD#80 considered a
/// distinct SB4042 for it and *deferred* — it would need a per-error code at
/// every `TypeError` site — so no SB4042 exists anywhere; this comment named it
/// as fact until 2026-10-14.)
///
/// The rule is the index's, not the position's: it is enforced on reads
/// (`${a[K]}` and its two brace-less spellings, which the lexer desugars to it)
/// by `flag_subscript_index`, and on writes (`a[K]=v`) by
/// `flag_assignment_index`. Those are different IR shapes, which is why it
/// takes two visitors to state one rule.
pub fn check_subscript_index(program: &Program) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for stmt in program
        .statements
        .iter()
        .chain(program.end_block.iter().flatten())
    {
        crate::ir::walk_all_exprs(stmt, &mut |e| flag_subscript_index(e, &mut diags));
        crate::ir::walk_stmt_tree(std::slice::from_ref(stmt), &mut |s| {
            flag_assignment_index(s, &mut diags)
        });
    }
    diags
}

/// The same rule on the WRITE side (`m[K]=v`).
///
/// An assignment's LHS index is a bare `Expr` in the `Assignment.index` struct
/// field — it is never wrapped in an `ExprKind::Subscript`, so the expression
/// walk above reaches it but cannot tell it is an index and sees only a
/// `Literal`. The rule belongs to the index, not to the spelling, so it is
/// applied here too: `m[ a b ]=9` must reject exactly as `${m[ a b ]}` does.
///
/// Before this existed the write side accepted every ambiguous form the read
/// side rejected, and `m[ $s x ]=9` silently stored the UNEXPANDED text `$s x`
/// as the key.
fn flag_assignment_index(stmt: &Statement, diags: &mut Vec<Diagnostic>) {
    let StmtKind::Assignment(a) = &stmt.kind else {
        return;
    };
    if let Some(d) = a.index.as_ref().and_then(ambiguous_index_diag) {
        diags.push(d);
    }
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

/// The SB4041 diagnostic for an ambiguous index, or `None` for an
/// unambiguous one. Explicit expansions and quoted strings are always fine; a
/// bare `Literal` is fine only when it is an integer literal or a clean
/// identifier; anything else (a `Literal` carrying `[`/operators, or a `Concat`
/// of mixed parts) is ambiguous.
///
/// Position-neutral on purpose — both the read and the write visitor call it.
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
            "ambiguous index `[{idx}]` — a subscript takes a direct key, not a computed \
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
        diags.extend(command_name_arity_diags(c, stmt.span));
    }
    for body in stmt.child_bodies() {
        check_match_arity_in(body, diags);
    }
}

/// The per-command checks this walk carries, each keyed on the command name
/// and each returning at most one diagnostic. Collected rather than pushed so
/// that adding a check does not deepen the walk above.
fn command_name_arity_diags(c: &crate::ir::Command, span: Span) -> Vec<Diagnostic> {
    let name = expr_literal_str(&c.name);
    let mut out = Vec::new();
    out.extend(match_arity_diag(name, c.args.len(), span));
    out.extend(retired_bulk_read_diag(name, span));
    out.extend(wait_arity_diag(name, c.args.len(), span));
    out
}

/// DD#126: `- wait` or `- wait PID`, and nothing else.
///
/// `check_cmdsub_arity` and `check_arith_sexpr_arity` cover the two *value*
/// spellings off `value_intrinsic_arity`; this is the statement spelling,
/// which neither sees. Without it `- wait 1 2` compiled and the surplus was
/// discarded in silence — the same shape as the `match S P g h` above.
fn wait_arity_diag(name: &str, got: usize, span: Span) -> Option<Diagnostic> {
    if name != "wait" {
        return None;
    }
    let arity = crate::ssa::value_intrinsic_arity("wait")?;
    if arity.contains(&got) {
        return None;
    }
    Some(Diagnostic::error_span(
        span,
        "SB4045",
        format!(
            "`wait` expects {} but got {got} — `wait` reaps any child, \
             `wait PID` reaps that one",
            arity_phrase(&arity)
        ),
    ))
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
// reached it, including `argv "1" a`: the index was read with `expr_to_string`,
// a *display* renderer that kept the quotes, so `"1"` never parsed as 1.
//
// (That renderer is now `literal_name_of`, which ICEs on a non-literal
// rather than rendering one; this rule is what keeps it unreached.)
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

/// The complaint for one `keys` command, or `None` if its shape is legal here.
///
/// DD#75 §3 gives `keys` two forms, and unlike `argv`'s they are told apart by
/// **arity alone** — no splat/fill discriminator is needed:
///
/// - `keys m k` (two arguments) is the out-param fill. It is a statement, and
///   legal wherever a statement is.
/// - `$(keys m)` (one argument) is the array splat. It is an *expression*, and
///   `transform` resolves it only as the whole of a `$( … )` that sources a
///   for-in loop.
///
/// The one-argument form in any other position is SB5073, with the same
/// meaning it had when `keys` was in `VALUE_ONLY_INTRINSICS`: a value form
/// where its value has nowhere to go. It left that list because the list keys
/// off the *name*, and `keys` now has a spelling that is a legal statement.
///
/// Both wordings name **both** repairs. The earlier pair named only the for-in
/// form, because at the time that was the only thing an author could do with a
/// map's keys; `keys m k` is usually what someone writing `keys m` as a
/// statement actually meant.
fn keys_form_diag(c: &Command, pos: CmdPos) -> Option<Diagnostic> {
    let operand = match c.args.first().map(expr_literal_str) {
        Some(m) if !m.is_empty() => m.to_string(),
        _ => "m".to_string(),
    };
    let complaint = match (c.args.len(), pos) {
        // The out-param fill: a statement, legal in any statement position.
        (2, _) => return None,
        (1, CmdPos::CmdsubSole) => return None,
        (1, CmdPos::Statement) => format!(
            "`keys` splats a collection — it produces a sequence of values, \
             not a statement, and a bare statement would discard them. To \
             snapshot the keys into an array, name it: `keys {operand} k`. To \
             iterate them, `for k in $(keys {operand})`"
        ),
        (1, CmdPos::CmdsubOther) => format!(
            "`keys` splats a collection — inside `$( … )` it is recognised \
             only when it is the whole substitution, not one statement among \
             several, and that substitution must be the source of a for-in \
             loop: `for k in $(keys {operand})`. It cannot be captured into a \
             scalar and used here; to get the keys as an array, use the \
             out-param form `keys {operand} k`"
        ),
        (n, _) => {
            return Some(Diagnostic::error_span(
                c.name.span,
                "SB4045",
                format!(
                    "`keys` takes `m` (splat the keys of map m, as the source \
                     of a for-in loop) or `m k` (snapshot them into array k) \
                     — got {n} argument(s)"
                ),
            ))
        }
    };
    Some(Diagnostic::error_span(c.name.span, "SB5073", complaint))
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

/// Which half of DD#125 to check. The two run in different phases, and the
/// split is not cosmetic.
///
/// `Bindings` reads a name straight off a well-formed node (an assignment
/// LHS, a declaration, a `for` variable, a `fn` name), so nothing can
/// mis-extract it — it is safe to run as the very first gate, and it has to,
/// because a naming violation is logically prior to every semantic check.
///
/// `CommandTargets` reads a name out of a *command*, through an extractor
/// that an earlier check may already have rejected the shape of: `read -a
/// arr -d :` is SB5057, and `collect_read_targets` hands back the `-d`
/// flag's argument `:`. Run early, that masks the arity error and asks the
/// author to rename a delimiter.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ReservedScope {
    Bindings,
    CommandTargets,
}

/// DD#125: a name the compiler already understands is not the user's to
/// bind. Walks every site that *introduces* a name — the same set
/// `collect_scope_names` enumerates (assignment LHS, declarations, `for`
/// variables, read/`argv`/`match`/`keys` out-params, DD#93 attest-convert
/// targets) plus `fn` names — and rejects a reserved one with SB5092.
///
/// Without this, `is_reserved_name`'s three resolution positions disagree:
/// `fn getpid -> int { return 42 }` yielded the *intrinsic* in
/// `$(( (getpid) ))` and `42` in `$(getpid)`, in one program. Reserving is
/// also what makes a bare arithmetic word unambiguous, so `$(( wait ))` can
/// name the intrinsic rather than an undeclared variable.
pub fn check_reserved_names(program: &Program, scope: ReservedScope) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for stmt in program
        .statements
        .iter()
        .chain(program.end_block.iter().flatten())
    {
        check_reserved_names_stmt(stmt, scope, &mut diags);
    }
    diags
}

fn check_reserved_names_stmt(stmt: &Statement, scope: ReservedScope, diags: &mut Vec<Diagnostic>) {
    let span = stmt.span;
    match (&stmt.kind, scope) {
        (StmtKind::Assignment(a), ReservedScope::Bindings) => {
            flag_reserved(&a.name, "variable", span, diags)
        }
        (StmtKind::Declaration(d), ReservedScope::Bindings) => flag_declared_names(d, span, diags),
        (StmtKind::For(f), ReservedScope::Bindings) => {
            flag_reserved(&f.variable, "`for` variable", span, diags)
        }
        (StmtKind::Function(f), ReservedScope::Bindings) => {
            flag_reserved(&f.name, "function", span, diags)
        }
        (StmtKind::Command(c), ReservedScope::CommandTargets) => {
            flag_command_bindings(c, span, diags)
        }
        _ => {}
    }
    for s in stmt.child_bodies().into_iter().flatten() {
        check_reserved_names_stmt(s, scope, diags);
    }
}

/// Every name a typed declaration introduces — `int a=1, b=2` binds both.
fn flag_declared_names(d: &Declaration, span: Span, diags: &mut Vec<Diagnostic>) {
    for a in &d.assignments {
        flag_reserved(&a.name, "variable", span, diags);
    }
}

/// Names a *command* introduces: `read`/`readline`/`argv`/`match`/`keys`
/// out-params, and DD#93's attest-convert target. Sorted, because two
/// bindings in one command would otherwise be diagnosed in hash order.
fn flag_command_bindings(c: &Command, span: Span, diags: &mut Vec<Diagnostic>) {
    let mut set = HashSet::new();
    command_bound_names(c, &mut set);
    let mut targets: Vec<_> = set.into_iter().collect();
    targets.sort();
    for t in targets {
        flag_reserved(&t, "variable", span, diags);
    }
}

/// Push SB5092 when `name` is the compiler's. `what` names the binding form
/// so the message says which one to rename.
fn flag_reserved(name: &str, what: &str, span: Span, diags: &mut Vec<Diagnostic>) {
    if !crate::resolve::is_reserved_name(name) {
        return;
    }
    diags.push(Diagnostic::error_span(
        span,
        "SB5092",
        format!(
            "`{name}` is a name the compiler already understands, so it cannot \
             also be a {what} (DD#125). sash resolves a bare name differently in \
             an arithmetic sexpr, a `$( )` word, and a bare arithmetic operand, \
             so a user binding that shadows one would mean two different things \
             in one program. Rename the {what}"
        ),
    ));
}

/// SB5081: `break`/`continue` outside any enclosing loop. The transform lowers
/// both through `state.loop_stack`, and an empty stack makes them a **silent
/// no-op** — the statement compiles and does nothing. bash warns; sash rejects.
/// A function body starts a fresh context: a `break` lexically inside a
/// function is not inside a loop even when the call site is. So does a `$( … )`
/// body, which is a function body (DD#86, 2026-09-29): one that inherited the
/// loop let `break` branch out of the open capture, losing every later write.
///
/// SB5097 applies the same principle to the optional loop **count**, which
/// reaches the same no-op by a second route: the transform guards on
/// `n > 0 && depth > 0`, so `continue 0` fell through and did nothing at all
/// while bash fails the builtin ("loop count out of range") and abandons the
/// loop. A count that is not a literal decimal was worse than a no-op — a
/// non-literal `continue $k` crashed the compiler in `literal_name_of`, and a
/// non-numeric `continue abc` silently became `continue 1` via `unwrap_or(1)`.
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

/// Recurse into every child body: a structural body carries the loop context
/// unchanged, and a `$( … )` body starts a fresh one, as a function body does.
fn descend_loop_control(stmt: &Statement, in_loop: bool, diags: &mut Vec<Diagnostic>) {
    for s in stmt.structural_bodies().into_iter().flatten() {
        check_loop_control_stmt(s, in_loop, diags);
    }
    for s in stmt.cmdsub_bodies().into_iter().flatten() {
        check_loop_control_stmt(s, false, diags);
    }
}

/// The SB5081 diagnostic for a `break`/`continue` with no enclosing loop, and
/// the SB5097 diagnostics for a loop count that cannot mean what it says.
fn loop_control_diag(c: &Command, span: Span, in_loop: bool) -> Option<Diagnostic> {
    let name = expr_literal_str(&c.name);
    if name != "break" && name != "continue" {
        return None;
    }
    if !in_loop {
        return Some(Diagnostic::error_span(
            span,
            "SB5081",
            format!(
                "`{name}` is not inside a loop — it would compile to a silent \
                 no-op. Remove it, or move it into a `for`/`while` body"
            ),
        ));
    }
    loop_count_diag(name, c, span)
}

/// SB5097: the optional loop count on `break`/`continue`.
///
/// The count is read once, at compile time, and selects an entry of the
/// transform's loop stack. Every spelling that cannot do that is rejected here
/// rather than silently reinterpreted:
///
/// - **more than one argument** — bash fails with "too many arguments";
/// - **a non-literal** (`continue $k`) — nothing resolves it at compile time,
///   and it reached `literal_name_of`, whose `unreachable!` says every caller
///   is a constrained name position. This one was not;
/// - **a non-decimal** (`continue abc`) — `parse::<usize>().unwrap_or(1)` made
///   it mean `continue 1`, an answer the source never asked for;
/// - **zero** — `n > 0` is false, so the statement fell through and did
///   nothing. That is SB5081's silent no-op reached by a second route.
///
/// A count *larger* than the nesting depth is **not** an error: the transform
/// clamps with `n.min(depth)` and so does bash, so `continue 9` inside one
/// loop continues that loop in both.
fn loop_count_diag(name: &str, c: &Command, span: Span) -> Option<Diagnostic> {
    if c.args.len() > 1 {
        return Some(Diagnostic::error_span(
            span,
            "SB5097",
            format!(
                "`{name}` takes at most one loop count, got {} arguments",
                c.args.len()
            ),
        ));
    }
    let arg = c.args.first()?;
    let ExprKind::Literal(text) = &arg.kind else {
        return Some(Diagnostic::error_span(
            span,
            "SB5097",
            format!(
                "`{name}` needs a literal loop count — this one is not known \
                 at compile time. Write the number, or restructure the loop"
            ),
        ));
    };
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Some(Diagnostic::error_span(
            span,
            "SB5097",
            format!("`{name}` needs a decimal loop count — `{text}` is not a number"),
        ));
    }
    if text.bytes().all(|b| b == b'0') {
        return Some(Diagnostic::error_span(
            span,
            "SB5097",
            format!(
                "`{name} {text}` would compile to a silent no-op — a loop count \
                 counts enclosing loops, so it starts at 1"
            ),
        ));
    }
    None
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

    /// `++x`. `parse::reject_prefix_incdec` raises SB4020 at both arith entry
    /// points, so no validator walker can meet one. These call the two
    /// arith walkers in this file directly with the node the parser would
    /// never hand them.
    fn prefix_incdec() -> crate::ir::ArithExpr {
        crate::ir::ArithExpr::PreIncDec {
            var: "x".into(),
            inc: true,
        }
    }

    #[test]
    #[should_panic(expected = "SB4014 rejects an `attest` with no name")]
    fn attest_arith_near_miss_rejects_a_nameless_lead() {
        // `attest (( x = 1 ))`. The parser cannot build this — it raises
        // SB4014 on the missing name — so the only way in is to hand the
        // walker the node directly.
        let c = crate::ir::Command {
            name: e(ExprKind::Literal("attest".to_string())),
            args: vec![e(ExprKind::Arithmetic(Box::new(
                crate::ir::ArithExpr::Literal(1),
            )))],
        };
        let _ = attest_arith_near_miss(&c);
    }

    #[test]
    #[should_panic(expected = "prefix ++/-- never survives the parser")]
    fn arith_has_incdec_rejects_prefix_incdec() {
        let _ = arith_has_incdec(&prefix_incdec());
    }

    #[test]
    #[should_panic(expected = "prefix ++/-- never survives the parser")]
    fn check_positionals_arith_rejects_prefix_incdec() {
        let mut diags = Vec::new();
        check_positionals_arith(&prefix_incdec(), Span::new(1, 1), &mut diags);
    }

    /// DD#116: `check_arith_reads`'s `SubscriptAssign` arm is unreachable by
    /// construction — SB4020 rejects an assignment inside `$(( … ))`, and the
    /// statement form `(( a[i] = 7 ))` is read by `check_arith_compound`. It
    /// held a correct-looking recursion, which in a *read* walker is the worst
    /// shape to leave cold: nothing exercises it, so if the route ever opened
    /// it would open in silence. That is how `SsaInst::Test` and `RedirectPush`
    /// escaped the SSA taint pass for their whole lives. The panic is the
    /// claim; a test that stops panicking is the finding.
    #[test]
    #[should_panic(expected = "SB4020 rejects an assignment")]
    fn check_arith_reads_rejects_a_subscript_assign() {
        use crate::ir::ArithExpr;
        let funcs = HashSet::new();
        let mut diags = Vec::new();
        let mut sink = ReadSink {
            mode: SinkMode::Flag(&mut diags),
            funcs: &funcs,
        };
        let a = ArithExpr::SubscriptAssign {
            name: "a".to_string(),
            index: Box::new(ArithExpr::Literal(0)),
            value: Box::new(ArithExpr::Literal(7)),
            op: None,
        };
        check_arith_reads(&a, Span::new(1, 1), &HashSet::new(), &mut sink);
    }

    /// `BUILTINS` states that a name listed in it "is never PATH-resolved".
    /// Nothing enforces that: the pass which would resolve it gates on
    /// `resolve::is_intrinsic` — `BuiltinCmd` ∪ `INTRINSICS`, a *different*
    /// list. The claim holds only while every `BUILTINS` entry is also an
    /// intrinsic, and no type says so.
    ///
    /// `kill` sat here without being either. `resolve_command` quietly
    /// rewrote the head to `/usr/bin/kill`, while `is_external_source` went
    /// on reporting it non-external for taint — the exact divergence that
    /// function's own doc comment calls a laundering hole. It was removed
    /// 2026-10-10; this is what stops the next such entry landing in silence.
    #[test]
    fn every_builtin_name_is_an_intrinsic() {
        let strays: Vec<&str> = BUILTINS
            .iter()
            .copied()
            .filter(|n| !crate::resolve::is_intrinsic(n))
            .collect();
        assert!(
            strays.is_empty(),
            "BUILTINS entries `resolve::is_intrinsic` does not own, so \
             `resolve_command` will PATH-resolve them and `is_external_source` \
             will still call them non-external: {strays:?}"
        );
    }

    /// The companion claim, from `VALUE_ONLY_INTRINSICS`' doc: "They stay in
    /// `BUILTINS` above so they are never PATH-resolved (a machine with a
    /// real `getenv` on PATH must not silently exec it)". Same shape of
    /// unenforced subset relation between two hand-maintained lists.
    #[test]
    fn value_only_intrinsics_are_all_builtins() {
        let strays: Vec<&str> = VALUE_ONLY_INTRINSICS
            .iter()
            .copied()
            .filter(|n| !BUILTINS.contains(n))
            .collect();
        assert!(
            strays.is_empty(),
            "value-only intrinsics missing from BUILTINS, so a host with a \
             binary of that name would exec it: {strays:?}"
        );
    }

    /// A map literal containing a taint source or a tainted read.
    fn assoc_with(value: ExprKind) -> Expr {
        e(ExprKind::AssocArray(vec![
            ("k".to_string(), e(ExprKind::Literal("v".to_string()))),
            ("j".to_string(), e(value)),
        ]))
    }

    fn assoc_clean() -> Expr {
        e(ExprKind::AssocArray(vec![(
            "k".to_string(),
            e(ExprKind::Literal("v".to_string())),
        )]))
    }

    /// `$(<f)` — the smallest single-node taint source.
    fn slurp() -> ExprKind {
        ExprKind::SlurpFile(Box::new(e(ExprKind::Literal("f".to_string()))))
    }

    // `is_taint_source`, `expr_uses_tainted` and `check_value_index_reads`
    // each carry an `ExprKind::AssocArray` arm, and all three are cold. A map
    // literal does not reach them: `reject_tainted_into_array` is handed each
    // element VALUE, so the literal is taken apart one level up, and twelve
    // probe programs spelling `map m=([k]=$(getenv X))` every way warmed none
    // of the three (journal 503).
    //
    // Unlike the `ExprKind::Test` arm above, these are ANSWERS rather than
    // ICEs. Nothing forbids a map literal here — it merely does not arrive —
    // and `reject_tainted_into_array` reads the first two as a pair, so an arm
    // that disagreed with its twin would be a taint hole rather than a crash.
    // Each test therefore pins the answer AND a negative, so an arm hard-wired
    // to `true` fails rather than passing half the suite.

    #[test]
    fn a_map_literal_is_a_taint_source_iff_a_value_is() {
        assert!(is_taint_source(&assoc_with(slurp())));
        assert!(!is_taint_source(&assoc_clean()));
    }

    #[test]
    fn a_map_literal_uses_taint_iff_a_value_does() {
        let tainted = HashSet::from(["t".to_string()]);
        assert!(expr_uses_tainted(
            &assoc_with(ExprKind::SimpleExpansion("t".to_string())),
            &tainted
        ));
        assert!(!expr_uses_tainted(&assoc_clean(), &tainted));
        // The two must agree about a map literal, which is the whole reason
        // both arms exist: `reject_tainted_into_array` ORs them together.
        assert!(!is_taint_source(&assoc_with(ExprKind::SimpleExpansion(
            "t".to_string()
        ))));
    }

    #[test]
    fn a_map_literal_value_is_descended_for_index_reads() {
        let tainted = HashSet::from(["t".to_string()]);
        // `([j]=${b[$t]})` — the element stores a cell of `b`, but reads
        // tainted `t` to choose which.
        let indexed = ExprKind::Subscript {
            name: "b".to_string(),
            index: Box::new(e(ExprKind::SimpleExpansion("t".to_string()))),
        };

        let mut diags = Vec::new();
        check_value_index_reads(&assoc_with(indexed), &tainted, &mut diags);
        assert_eq!(diags.len(), 1, "the element's index is a read site");
        assert_eq!(diags[0].code, "SB5020", "{diags:?}");

        let mut clean = Vec::new();
        check_value_index_reads(&assoc_clean(), &tainted, &mut clean);
        assert!(clean.is_empty(), "a literal element reads nothing");
    }

    /// `first_expansion` is the fourth walker with a cold `AssocArray` arm, for
    /// the same reason and with a fourth caller: `flag_arg_builtin` asks it
    /// about an `arg` DEFAULT, and a default is parsed as a word, so no map
    /// literal arrives. Its `Test` arm is cold for the stronger reason given on
    /// `expr_uses_tainted`'s above — `ExprKind::Test` is statement-only. Both
    /// are kept by the no-wildcard rule stated in the function's own doc, and
    /// both must answer "substituted", because the rule they serve is "can any
    /// of this change at runtime" and a silent "no" reopens it.
    #[test]
    fn first_expansion_sees_through_a_map_literal_and_a_test() {
        assert!(first_expansion(&assoc_with(slurp())).is_some());
        assert!(first_expansion(&assoc_clean()).is_none());
        assert!(first_expansion(&e(ExprKind::Test(unary("t")))).is_some());
    }
}
