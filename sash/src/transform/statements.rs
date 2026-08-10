//! Statement lowering: IR Statement → SSA instructions.
//!
//! `lower_one` is the main dispatch — each StmtKind variant is handled.
//! SECURITY: This is where commands become SsaInst::Command with resolved names.

use super::control_flow::{flatten_elif, lower_case, lower_for, lower_if, lower_while};
use super::expressions::expr_to_value;
use super::functions::{collect_returns, lift_int_func_calls};
use crate::ir::*;
use crate::ssa::*;

pub(super) fn lower_stmts(
    stmts: &[Statement],
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    mut block: BlockId,
) -> BlockId {
    for stmt in stmts {
        block = lower_one(stmt, ssa, state, block);
    }
    block
}

/// Lower a single statement into SSA.
/// DD#58: Lift IntFuncCall values out of arithmetic expressions.
/// Inlines the function body, captures the return value as an int temp var.
pub(super) fn lower_one(
    stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    mut block: BlockId,
) -> BlockId {
    match &stmt.kind {
        StmtKind::Assignment(a) => {
            super::stmt_handlers::handle_assignment(a, stmt, ssa, state, block)
        }
        StmtKind::Declaration(d) => lower_declaration_stmt(d, ssa, state, block),
        StmtKind::Command(c) => super::stmt_handlers::handle_command(c, stmt, ssa, state, block),
        StmtKind::If(i) => {
            let flattened = flatten_elif(i);
            lower_if(&flattened, ssa, state, block)
        }
        StmtKind::While(w) => lower_while_stmt(w, ssa, state, block),
        StmtKind::For(f) => lower_for(f, ssa, state, block),
        StmtKind::Pipeline(p) => super::stmt_handlers::handle_pipeline(p, stmt, ssa, state, block),
        StmtKind::List(l) => lower_list_stmt(l, stmt, ssa, state, block),
        StmtKind::Function(f) => lower_function_stmt(f, stmt, ssa, state, block),
        StmtKind::Redirected(r) => lower_redirected_stmt(r, ssa, state, block),
        StmtKind::Group(g) => lower_stmts(&g.body, ssa, state, block),
        StmtKind::Case(c) => lower_case(c, ssa, state, block),
        StmtKind::Comment(_) => block,
        StmtKind::ArithTest(expr) => lower_arith_test_stmt(expr, stmt, ssa, state, &mut block),
        StmtKind::Suppressed(inner) => lower_suppressed_stmt(inner, ssa, state, block),
        StmtKind::ReturnProducer(inner) => lower_return_producer(inner, ssa, state, block),
    }
}

/// DD#74: `return <producer>` — evaluate the producer (a command, `[[ … ]]`,
/// or `(( … ))`), which sets `$?`/test_result, then return that status. The
/// producer is *consumed* by the return, so its command is unchecked (it must
/// not abort on a nonzero exit — that status is exactly the return value).
fn lower_return_producer(
    inner: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    // The producer is consumed by the return (like a condition): bump
    // in_condition so a `[[ … ]]`/`(( … ))` producer isn't flagged
    // must-consume (SB4018), and uncheck its command so it doesn't abort.
    state.in_condition += 1;
    let block = lower_one(inner, ssa, state, block);
    state.in_condition -= 1;
    super::entry::uncheck_last_condition_inst(&mut ssa.block_mut(block).insts);
    // The producer already left its status in $?/test_result; terminate the
    // function (Goto the return target when inlined, else Return).
    if let Some(target) = state.return_target {
        ssa.block_mut(block).terminator = Terminator::Goto(target);
    } else {
        ssa.block_mut(block).terminator = Terminator::Return;
    }
    ssa.new_block("after_return_producer")
}

/// `while`/`until`. For `until`, desugar to `while ! cond` by lowering as
/// `while` then patching the header block's last Test instruction with `Not`.
fn lower_while_stmt(
    w: &While,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    if !w.until {
        return lower_while(w, ssa, state, block);
    }
    let mut desugared = w.clone();
    desugared.until = false;
    let exit = lower_while(&desugared, ssa, state, block);
    // `until` = `while ! cond`: negate the condition's result in the loop
    // header, after the condition and before the loop branch. Appending the
    // NegateTestResult sigil (the same `!`-prefix primitive) works for ANY
    // condition form — [[ ]], (( )) (a LetExit), or a plain command — and
    // negates the whole condition once (the old per-Test `SsaTest::Not`
    // patch missed (( ))/command conditions and mis-negated compound ones).
    if let Terminator::Goto(header_id) = &ssa.block(block).terminator {
        let header_id = *header_id;
        ssa.block_mut(header_id).insts.push(SsaInst::sigil(
            BuiltinCmd::NegateTestResult,
            vec![],
            true,
        ));
    }
    exit
}

/// Function definition. Registers the body for inline expansion at call sites,
/// then emits the body into a dedicated SSA block sequence terminating in Return.
/// Special cases: `__sash_end__` (DD#56 exit trap), `__local__*` (DD#18 local scope),
/// `-> int` return type tracking, DD#55 non-void detection + SB4017 emission.
fn lower_function_stmt(
    f: &Function,
    stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    if f.name == "__sash_end__" {
        state.exit_trap = Some(f.body.clone());
        return block;
    }
    state.function_bodies.insert(f.name.clone(), f.body.clone());
    if f.name.starts_with("__local__") {
        state.local_fn_scope.insert(f.name.clone());
    }
    if f.return_type == crate::ir::VarType::Int {
        state.function_int_typed.insert(f.name.clone());
    }
    analyze_function_returns(f, stmt, state);
    let fn_entry = ssa.new_block(&format!("fn_{}", f.name));
    let saved_scope = state.local_fn_scope.clone();
    // The standalone function block is a template lowered without a call
    // site; its $@/$* have no caller args here (every real call is inlined
    // with the args known). Expand them to empty so this template never
    // produces a program-argv splat (DD#60). Per-call $@ is expanded with
    // the real argc in inline_function_call.
    let mut template_body = f.body.clone();
    super::cmd_handlers::expand_dollar_at_in_body(&mut template_body, 0);
    let fn_exit = lower_stmts(&template_body, ssa, state, fn_entry);
    state.local_fn_scope = saved_scope;
    ssa.block_mut(fn_exit).terminator = Terminator::Return;
    block
}

/// DD#55: for an `-> int` function, flag SB4017 when every return is a
/// literal 0 (it should be void), and record non-void status otherwise.
fn analyze_function_returns(f: &Function, stmt: &Statement, state: &mut VersionState) {
    if f.return_type != crate::ir::VarType::Int {
        return;
    }
    let returns = collect_returns(&f.body);
    if returns.is_empty() {
        return;
    }
    let all_literal_zero = returns
        .iter()
        .all(|v| matches!(v, Some(s) if s.is_empty() || s == "0"));
    if all_literal_zero {
        state.diagnostics.push(format!(
            "{}:{}: error [SB4017] function '{}' only returns 0 — omit the return (void function) or return a meaningful status",
            stmt.span.line, stmt.span.col, f.name
        ));
    }
    let has_nonzero = returns
        .iter()
        .any(|v| !matches!(v, Some(s) if s.is_empty() || s == "0"));
    if has_nonzero {
        state.function_non_void.insert(f.name.clone());
    }
}

/// Statement wrapped in a redirect. For compound bodies (while/for/if/case/group/
/// subshell/function), bracket with __redirect_push/__redirect_pop. For simple
/// commands, attach the redirects to the last emitted Command/Pipeline/Read inst.
fn lower_redirected_stmt(
    r: &Redirected,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    if let StmtKind::Function(ref f) = r.body.kind {
        return lower_redirected_function(f, r, ssa, state, block);
    }

    // Compound bodies and bulk reads (mapfile / readarray / read -a) bracket
    // with __redirect_push/pop. Bulk reads lower to Mapfile / ReadArray, which
    // carry no redirect list, so attaching to the last inst (below) would
    // silently drop a `< file` redirect (DD#61 fix — matches `read < file`).
    let needs_bracket = is_compound_stmt(&r.body.kind) || is_bulk_read_cmd(&r.body.kind);
    if needs_bracket && !r.redirects.is_empty() {
        return bracket_compound_with_redirects(r, ssa, state, block);
    }

    let result_block = lower_one(&r.body, ssa, state, block);
    attach_redirects_to_result(ssa, block, result_block, &r.redirects);
    result_block
}

/// Attach a simple command's redirects to the last emitted instruction in
/// the block(s) it produced (no-op when there are no redirects).
fn attach_redirects_to_result(
    ssa: &mut SsaProgram,
    block: BlockId,
    result_block: BlockId,
    redirects: &[Redirect],
) {
    if redirects.is_empty() {
        return;
    }
    let blocks_to_check = if result_block == block {
        vec![block]
    } else {
        vec![block, result_block]
    };
    for blk_id in blocks_to_check {
        attach_redirects_to_last_inst(ssa, blk_id, redirects);
    }
}

/// True for `mapfile` / `readarray` / `read -a` commands, whose lowered
/// SSA instructions (Mapfile / ReadArray) carry no redirect list.
fn is_bulk_read_cmd(kind: &StmtKind) -> bool {
    let StmtKind::Command(c) = kind else {
        return false;
    };
    let ExprKind::Literal(name) = &c.name.kind else {
        return false;
    };
    match name.as_str() {
        "mapfile" | "readarray" => true,
        "read" => c
            .args
            .iter()
            .any(|a| matches!(&a.kind, ExprKind::Literal(s) if s == "-a")),
        _ => false,
    }
}

fn is_compound_stmt(kind: &StmtKind) -> bool {
    matches!(
        kind,
        StmtKind::While(_)
            | StmtKind::If(_)
            | StmtKind::For(_)
            | StmtKind::Case(_)
            | StmtKind::Group(_)
            | StmtKind::Function { .. }
    )
}

/// `f() { ... } > out` — store the redirects keyed by function name so
/// they're applied at every call site (handle_function_call_or_command).
fn lower_redirected_function(
    f: &Function,
    r: &Redirected,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    if !r.redirects.is_empty() {
        state
            .function_redirects
            .insert(f.name.clone(), r.redirects.clone());
    }
    lower_one(&r.body, ssa, state, block)
}

/// `(while/if/for/case/{}/(...)) > out` — wrap the lowered body with
/// __redirect_push / __redirect_pop so the redirects apply to every
/// inst emitted between them.
fn bracket_compound_with_redirects(
    r: &Redirected,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    ssa.block_mut(block).insts.push(SsaInst::Command {
        name: "__redirect_push".to_string(),
        kind: BuiltinCmd::RedirectPush,
        args: vec![],
        redirects: r.redirects.clone(),
        checked: true,
    });
    let result_block = lower_one(&r.body, ssa, state, block);
    ssa.block_mut(result_block).insts.push(SsaInst::Command {
        name: "__redirect_pop".to_string(),
        kind: BuiltinCmd::RedirectPop,
        args: vec![],
        redirects: r.redirects.clone(),
        checked: true,
    });
    result_block
}

/// Simple command + redirects: attach the redirects directly to the last
/// emitted inst (Command/Pipeline last-stage/ReadLine/ReadLineMulti).
fn attach_redirects_to_last_inst(ssa: &mut SsaProgram, blk_id: BlockId, redirects: &[Redirect]) {
    let blk = ssa.block_mut(blk_id);
    let Some(inst) = blk.insts.last_mut() else {
        return;
    };
    match inst {
        SsaInst::Command { redirects: r, .. }
        | SsaInst::ReadLine { redirects: r, .. }
        | SsaInst::ReadLineMulti { redirects: r, .. } => {
            *r = redirects.to_vec();
        }
        SsaInst::Pipeline { stages, .. } => {
            if let Some(last) = stages.last_mut() {
                last.2 = redirects.to_vec();
            }
        }
        _ => {}
    }
}

/// `(( expr ))` as a condition: evaluate arithmetic, set exit status based on
/// result (0 = true if nonzero, 1 = false if zero).
fn lower_arith_test_stmt(
    expr: &ArithExpr,
    stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> BlockId {
    let ir_expr = crate::ir::Expr {
        kind: crate::ir::ExprKind::Arithmetic(Box::new(expr.clone())),
        span: stmt.span,
    };
    let val = expr_to_value(&ir_expr, state);
    let val = lift_int_func_calls(val, ssa, state, block);
    let target = state.define("__arith_test");
    ssa.block_mut(*block).insts.push(SsaInst::Assign {
        target: target.clone(),
        value: val,
    });
    ssa.block_mut(*block).insts.push(SsaInst::sigil(
        BuiltinCmd::LetExit,
        vec![SsaValue::Var(target)],
        true,
    ));
    *block
}

/// DD#55: `- cmd` (suppressed) — lower the inner statement, then mark the last
/// emitted Command/Pipeline instruction(s) as unchecked.
fn lower_suppressed_stmt(
    inner: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let pre_len = ssa.block(block).insts.len();
    state.suppressed += 1;
    let result_block = lower_one(inner, ssa, state, block);
    state.suppressed -= 1;
    uncheck_block_from(ssa, block, pre_len);
    if result_block != block {
        for b in (block + 1)..=result_block {
            uncheck_block_from(ssa, b, 0);
        }
    }
    result_block
}

fn uncheck_block_from(ssa: &mut SsaProgram, block_id: BlockId, from: usize) {
    let insts = &mut ssa.block_mut(block_id).insts;
    for inst in insts[from..].iter_mut() {
        match inst {
            SsaInst::Command { checked, .. } => {
                *checked = false;
            }
            SsaInst::Pipeline { checked, .. } => {
                *checked = false;
            }
            _ => {}
        }
    }
}

fn lower_declaration_stmt(
    d: &Declaration,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    mut block: BlockId,
) -> BlockId {
    let is_array = d.flags.iter().any(|f| f == "-a");
    let is_assoc = d.flags.iter().any(|f| f == "-A");
    for a in &d.assignments {
        block = lower_one_declaration(a, d, is_array, is_assoc, ssa, state, block);
    }
    block
}

fn lower_one_declaration(
    a: &Assignment,
    d: &Declaration,
    is_array: bool,
    is_assoc: bool,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    mut block: BlockId,
) -> BlockId {
    if is_bare_redeclare_noop(a, is_array, is_assoc, state) {
        return block;
    }
    let val = compute_declaration_value(a, is_array, is_assoc, ssa, state, &mut block);
    let target = state.define(&a.name);
    ssa.block_mut(block)
        .insts
        .push(SsaInst::Assign { target, value: val });
    if d.keyword == crate::ir::DeclKeyword::Readonly {
        state.readonly_vars.insert(a.name.clone());
    }
    block
}

/// `declare var` (no value, not -a/-A) on an already-defined variable
/// is a no-op — don't clobber the existing value.
fn is_bare_redeclare_noop(
    a: &Assignment,
    is_array: bool,
    is_assoc: bool,
    state: &VersionState,
) -> bool {
    let is_bare =
        matches!(&a.value.kind, ExprKind::Literal(s) if s.is_empty()) && !is_array && !is_assoc;
    is_bare && state.lookup(&a.name).version > 0
}

fn compute_declaration_value(
    a: &Assignment,
    is_array: bool,
    is_assoc: bool,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> SsaValue {
    // Empty init like `array arr` / `array arr=()` / `map m` / `map m=()`
    // — an empty table of the declared kind.
    if is_empty_collection_init(a, is_array, is_assoc) {
        return empty_collection_value(&a.name, is_assoc, state);
    }
    let val = expr_to_value(&a.value, state);
    // Option C: `$(f)` on an `-> int` function is an IntFuncCall (the return
    // value); lift_int_func_calls materializes it here, exactly like `$((f))`.
    let val = lift_int_func_calls(val, ssa, state, block);
    track_collection_kind(&a.name, &val, state);
    val
}

/// True for `array arr` / `array arr=()` / `map m` / `map m=()`.
fn is_empty_collection_init(a: &Assignment, is_array: bool, is_assoc: bool) -> bool {
    let is_empty = a.value.kind == ExprKind::Literal(String::new())
        || matches!(&a.value.kind, ExprKind::Array(e) if e.is_empty());
    is_empty && (is_array || is_assoc)
}

/// Empty table of the declared kind. (`()` parses as an empty indexed
/// Array regardless of declaration, so a `map`'s empty init is steered to
/// an associative table here.)
fn empty_collection_value(name: &str, is_assoc: bool, state: &mut VersionState) -> SsaValue {
    state.collections.insert(name.to_string());
    if is_assoc {
        state.assoc_arrays.insert(name.to_string());
        SsaValue::AssocArray(vec![])
    } else {
        SsaValue::IndexedArray(vec![])
    }
}

fn track_collection_kind(name: &str, val: &SsaValue, state: &mut VersionState) {
    if matches!(val, SsaValue::AssocArray(_)) {
        state.assoc_arrays.insert(name.to_string());
    }
    if matches!(val, SsaValue::AssocArray(_) | SsaValue::IndexedArray(_)) {
        state.collections.insert(name.to_string());
    }
}

fn lower_list_stmt(
    l: &List,
    _stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    mut block: BlockId,
) -> BlockId {
    match l.op {
        ListOp::And => {
            // left && right: run left, skip right if left failed
            state.in_condition += 1;
            block = lower_one(&l.left, ssa, state, block);
            state.in_condition -= 1;
            let right_block = ssa.new_block("and.right");
            let merge_block = ssa.new_block("and.merge");
            ssa.block_mut(block).terminator = Terminator::Branch {
                condition: block.to_string(),
                true_block: right_block,
                false_block: merge_block,
            };
            let right_end = lower_one(&l.right, ssa, state, right_block);
            ssa.block_mut(right_end).terminator = Terminator::Goto(merge_block);
            merge_block
        }
        ListOp::Or => {
            // left || right: run left, skip right if left succeeded
            state.in_condition += 1;
            block = lower_one(&l.left, ssa, state, block);
            state.in_condition -= 1;
            let right_block = ssa.new_block("or.right");
            let merge_block = ssa.new_block("or.merge");
            ssa.block_mut(block).terminator = Terminator::Branch {
                condition: block.to_string(),
                true_block: merge_block,  // success → skip right
                false_block: right_block, // failure → run right
            };
            let right_end = lower_one(&l.right, ssa, state, right_block);
            // DD#55: if the right side completes, the || list succeeded;
            // reset exit status to 0 so the program doesn't exit with
            // the left side's failure status.
            let zero_ref = state.define("?");
            ssa.block_mut(right_end).insts.push(SsaInst::Assign {
                target: zero_ref,
                value: SsaValue::Literal("0".to_string()),
            });
            ssa.block_mut(right_end).terminator = Terminator::Goto(merge_block);
            merge_block
        }
        ListOp::Background => {
            // cmd & — fork, child runs cmd, parent continues
            let child_block = ssa.new_block("bg.child");
            let parent_block = ssa.new_block("bg.parent");

            // Fork
            ssa.block_mut(block).insts.push(SsaInst::sigil(
                BuiltinCmd::SubshellStart,
                vec![],
                true,
            ));
            ssa.block_mut(block).terminator = Terminator::Branch {
                condition: "__fork_pid".to_string(),
                true_block: parent_block, // parent continues
                false_block: child_block, // child runs command
            };

            // Store fork pid in $! variable
            let bg_target = state.define("!");
            ssa.block_mut(parent_block).insts.push(SsaInst::sigil(
                BuiltinCmd::StoreBgPid,
                vec![SsaValue::Var(bg_target)],
                true,
            ));

            // Child: run the command then _exit
            let child_end = lower_one(&l.left, ssa, state, child_block);
            ssa.block_mut(child_end).terminator = Terminator::Return;

            parent_block
        }
    }
}
