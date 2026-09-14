//! Statement lowering: IR Statement → SSA instructions.
//!
//! `lower_one` is the main dispatch — each StmtKind variant is handled.
//! SECURITY: This is where commands become SsaInst::Command with resolved names.

use super::control_flow::{flatten_elif, lower_case, lower_for, lower_if, lower_while};
use super::expressions::{expr_to_string, expr_to_value};
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
        StmtKind::ArithTest(expr) => lower_arith_test_stmt(expr, stmt, ssa, state, &mut block),
        StmtKind::Suppressed(inner) => lower_suppressed_stmt(inner, ssa, state, block),
        StmtKind::Negated(_) => unreachable!(
            "a per-segment `Negated` is peeled by \
             stmt_handlers::push_pipeline_stage; parse only builds it for a \
             non-leading pipeline stage (parser.rs `append_pipeline_stage`), \
             so it never reaches statement dispatch"
        ),
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
    // DD#27/§5: `return F …` where F is the enclosing function is a self
    // tail-call — lower to a loop back-edge instead of inlining F again.
    if let Some(after) = try_producer_tail_call(inner, ssa, state, block) {
        return after;
    }
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

/// DD#27/§5: if `inner` is a self tail-call to the enclosing function
/// (`return F …`), emit the loop back-edge and return the trailing block.
/// `None` when it is not a tail self-call.
fn try_producer_tail_call(
    inner: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> Option<BlockId> {
    let StmtKind::Command(c) = &inner.kind else {
        return None;
    };
    let (tf, header, aname) = state.tail_loop.clone()?;
    if expr_to_string(&c.name) != tf {
        return None;
    }
    let new_args: Vec<SsaValue> = c.args.iter().map(|a| expr_to_value(a, state)).collect();
    Some(super::functions::emit_tail_call(
        header, &aname, new_args, ssa, state, block,
    ))
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
/// Special cases: `-> int` return type tracking, DD#55 non-void detection +
/// SB4017 emission. (The DD#56 END body is `Program::end_block`, seeded into
/// `state.exit_trap` by `ir_to_ssa` — it is not a function and never reaches
/// here.)
fn lower_function_stmt(
    f: &Function,
    stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    // DD#81 Part A: one name, one meaning. If this name was already used as an
    // external command earlier in execution order, defining it as a function
    // now is a command/function shadowing collision.
    if state.externally_used.borrow().contains(&f.name) {
        state.diagnostics.push(format!(
            "{}:{}: error [SB5041] '{}' is used as a command before this definition — a name is either a command or a function, not both (rename, or invoke the external via an absolute path)",
            stmt.span.line, stmt.span.col, f.name
        ));
    }
    state.function_bodies.insert(f.name.clone(), f.body.clone());
    // DD#81 rule (a) contiguity + DD#84 mixing / optional-trailing: emit each
    // structural defect of the fixed-arity `args` signature at the definition.
    for defect in super::functions::analyze_arity(&f.body).1 {
        let (line, col, name) = (stmt.span.line, stmt.span.col, &f.name);
        let msg = match defect {
            super::functions::ArityDefect::Hole(i) => format!(
                "{line}:{col}: error [SB5044] function '{name}' reads args non-contiguously — argument {i} is unused; read args in order from index 0, or use @args for a variable number"
            ),
            super::functions::ArityDefect::Mixed(i) => format!(
                "{line}:{col}: error [SB5047] function '{name}' reads argument {i} both as required (${{args[{i}]}}) and optional (${{args[{i}]:-}}/`:~`) — a positional is one or the other; drop the bare read or give every read a disposition"
            ),
            super::functions::ArityDefect::RequiredAfterOptional(i) => format!(
                "{line}:{col}: error [SB5048] function '{name}' has a required argument {i} after an optional one — optional args (`:-`/`:~`) must be trailing; reorder so required args come first"
            ),
        };
        state.diagnostics.push(msg);
    }
    if f.return_type == crate::ir::VarType::Int {
        state.function_int_typed.insert(f.name.clone());
    }
    analyze_function_returns(f, stmt, state);
    let fn_entry = ssa.new_block(&format!("fn_{}", f.name));
    // The standalone function block is a template lowered without a call
    // site; $@/$* are rejected at validation (DD#81) and never reach here.
    // Per-call args are bound when inline_function_call lowers each call.
    let template_body = f.body.clone();
    let saved_template_flag = state.lowering_fn_template;
    let saved_ro = super::functions::enter_fn_body_scope(state);
    state.lowering_fn_template = true;
    let fn_exit = lower_stmts(&template_body, ssa, state, fn_entry);
    state.lowering_fn_template = saved_template_flag;
    super::functions::exit_fn_body_scope(state, saved_ro);
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
    // A compound body brackets its redirects with __redirect_push/pop.
    // DD#99 `read` lowers to `ReadArray`, which carries its own redirect list
    // (and therefore gets the DD#55 open(2) guard), so it is attached to the
    // last inst below rather than bracketed.
    // (A redirect on a function definition / declaration / assignment is
    // rejected at parse time — SB4046 — so those never reach here.)
    let needs_bracket = is_compound_stmt(&r.body.kind);
    let redirects = spill_scalar_redirect_targets(&r.redirects, ssa, state, block);
    if needs_bracket && !redirects.is_empty() {
        return bracket_compound_with_redirects(r, &redirects, ssa, state, block);
    }

    let result_block = lower_one(&r.body, ssa, state, block);
    attach_redirects_to_result(ssa, block, result_block, &redirects);
    result_block
}

/// A redirect target that is a scalar *read* — `${x:-f}` (ParameterExpansion)
/// or `$a[i]` (Subscript), possibly nested inside a concatenation / double
/// quote — can't be lowered by the ad-hoc redirect-target codegen, which only
/// knows literals, `$x`, and concatenations of those (the rest silently
/// redirected to an *empty* filename). Spill it: bind the fully-lowered value
/// (via `expr_to_value`, so it inherits the same default / subscript / string
/// semantics as any other value position) to a synthetic scalar, then rewrite
/// the target to read that scalar (`$__redir_target_N`), which codegen already
/// handles. Command-substitution / arithmetic targets never reach here — they
/// are rejected at parse (SB4049).
fn target_needs_spill(kind: &ExprKind) -> bool {
    match kind {
        ExprKind::ParameterExpansion(_) | ExprKind::Subscript { .. } => true,
        ExprKind::Concat(parts) | ExprKind::DoubleQuoted(parts) => {
            parts.iter().any(|p| target_needs_spill(&p.kind))
        }
        _ => false,
    }
}

pub(super) fn spill_scalar_redirect_targets(
    redirects: &[Redirect],
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> Vec<Redirect> {
    let mut out = Vec::with_capacity(redirects.len());
    for redir in redirects {
        if target_needs_spill(&redir.target.kind) {
            let val = expr_to_value(&redir.target, state);
            let name = format!("__redir_target_{}_{}", ssa.blocks.len(), out.len());
            let target = state.define(&name);
            ssa.block_mut(block)
                .insts
                .push(SsaInst::Assign { target, value: val });
            out.push(Redirect {
                fd: redir.fd,
                op: redir.op,
                target: Expr {
                    kind: ExprKind::SimpleExpansion(name),
                    span: Span::default(),
                },
            });
        } else {
            out.push(redir.clone());
        }
    }
    out
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

/// `(while/if/for/case/{}/(...)) > out` — wrap the lowered body with
/// __redirect_push / __redirect_pop so the redirects apply to every
/// inst emitted between them.
fn bracket_compound_with_redirects(
    r: &Redirected,
    redirects: &[Redirect],
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    ssa.block_mut(block).insts.push(SsaInst::Command {
        name: "__redirect_push".to_string(),
        kind: BuiltinCmd::RedirectPush,
        args: vec![],
        redirects: redirects.to_vec(),
        checked: true,
    });
    let result_block = lower_one(&r.body, ssa, state, block);
    ssa.block_mut(result_block).insts.push(SsaInst::Command {
        name: "__redirect_pop".to_string(),
        kind: BuiltinCmd::RedirectPop,
        args: vec![],
        redirects: redirects.to_vec(),
        checked: true,
    });
    result_block
}

/// Simple command + redirects: attach the redirects directly to the last
/// emitted inst (Command / Pipeline last-stage / ReadLine / ReadArray).
fn attach_redirects_to_last_inst(ssa: &mut SsaProgram, blk_id: BlockId, redirects: &[Redirect]) {
    let blk = ssa.block_mut(blk_id);
    let Some(inst) = blk.insts.last_mut() else {
        return;
    };
    match inst {
        SsaInst::Command { redirects: r, .. }
        | SsaInst::ReadLine { redirects: r, .. }
        | SsaInst::ReadArray { redirects: r, .. } => {
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
    // Bug #3: a `(( … ))` statement / `if`/`while` condition whose value embeds
    // an assignment (`(( (a=2) ))`, `(( 1 + (a=2) ))`) must apply the store —
    // bash evaluates the assignment for its side effect and uses the assigned
    // value. Hoist the embedded stores here (where we hold `&mut` state/block),
    // leaving a mutation-free residual to evaluate. The common mutation-free
    // condition skips this entirely.
    let hoisted;
    let expr: &ArithExpr = if arith_has_mutation(expr) {
        hoisted = hoist_arith_mutations(expr, stmt.span, ssa, state, block);
        &hoisted
    } else {
        expr
    };
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

/// Bug #3: apply the stores for assignments embedded in a `(( … ))` statement or
/// condition, then return a mutation-free residual in which each assignment is a
/// read of its now-stored target. Each store is emitted as an `Assignment`
/// lowered through the standard path (`lower_one`), so taint / type / SSA
/// bookkeeping match a top-level `(( a=2 ))`. `validate` (SB4020) has already
/// rejected the forms this can't apply in place — an embedded `++`/`--`, an
/// `a[i]` mutation, or a mutation under `&&`/`||`/`?:` — so those arms are
/// defensive; the recursion only ever emits stores from unconditionally-
/// evaluated positions.
fn hoist_arith_mutations(
    a: &ArithExpr,
    span: Span,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> ArithExpr {
    if let Some(residual) = hoist_mutation_node(a, span, ssa, state, block) {
        return residual;
    }
    match a {
        ArithExpr::BinOp { left, op, right } => ArithExpr::BinOp {
            left: Box::new(hoist_arith_mutations(left, span, ssa, state, block)),
            op: *op,
            right: Box::new(hoist_arith_mutations(right, span, ssa, state, block)),
        },
        ArithExpr::UnaryOp { op, operand } => ArithExpr::UnaryOp {
            op: *op,
            operand: Box::new(hoist_arith_mutations(operand, span, ssa, state, block)),
        },
        ArithExpr::Ternary {
            cond,
            true_val,
            false_val,
        } => ArithExpr::Ternary {
            cond: Box::new(hoist_arith_mutations(cond, span, ssa, state, block)),
            true_val: Box::new(hoist_arith_mutations(true_val, span, ssa, state, block)),
            false_val: Box::new(hoist_arith_mutations(false_val, span, ssa, state, block)),
        },
        ArithExpr::Paren(inner) => ArithExpr::Paren(Box::new(hoist_arith_mutations(
            inner, span, ssa, state, block,
        ))),
        ArithExpr::Comma(es) => ArithExpr::Comma(
            es.iter()
                .map(|e| hoist_arith_mutations(e, span, ssa, state, block))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Emit the store for a single arith mutation node and return its residual read
/// (`Var` / `Subscript`); return `None` when `a` is not a mutation, so
/// `hoist_arith_mutations` recurses structurally instead. `validate` (SB4020)
/// has already rejected the mutation forms that can't be applied in place (an
/// embedded `++`/`--`, an `a[i]` mutation, or a mutation under `&&`/`||`/`?:`),
/// so those arms here are defensive — reached only from unconditionally-
/// evaluated positions.
fn hoist_mutation_node(
    a: &ArithExpr,
    span: Span,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> Option<ArithExpr> {
    match a {
        ArithExpr::Assign { var, value } => {
            let rhs = hoist_arith_mutations(value, span, ssa, state, block);
            emit_arith_store(var.clone(), None, rhs, span, ssa, state, block);
            Some(ArithExpr::Var(var.clone()))
        }
        ArithExpr::CompoundAssign { var, op, value } => {
            let rhs = hoist_arith_mutations(value, span, ssa, state, block);
            let full = ArithExpr::BinOp {
                left: Box::new(ArithExpr::Var(var.clone())),
                op: *op,
                right: Box::new(rhs),
            };
            emit_arith_store(var.clone(), None, full, span, ssa, state, block);
            Some(ArithExpr::Var(var.clone()))
        }
        ArithExpr::PostIncDec { var, inc } | ArithExpr::PreIncDec { var, inc } => {
            let op = if *inc { IrArithOp::Add } else { IrArithOp::Sub };
            let full = ArithExpr::BinOp {
                left: Box::new(ArithExpr::Var(var.clone())),
                op,
                right: Box::new(ArithExpr::Literal(1)),
            };
            emit_arith_store(var.clone(), None, full, span, ssa, state, block);
            Some(ArithExpr::Var(var.clone()))
        }
        ArithExpr::SubscriptAssign {
            name,
            index,
            op,
            value,
        } => {
            let rhs0 = hoist_arith_mutations(value, span, ssa, state, block);
            let rhs = match op {
                None => rhs0,
                Some(o) => ArithExpr::BinOp {
                    left: Box::new(ArithExpr::Subscript {
                        name: name.clone(),
                        index: index.clone(),
                    }),
                    op: *o,
                    right: Box::new(rhs0),
                },
            };
            let idx = crate::ir::Expr {
                kind: crate::ir::ExprKind::Arithmetic(index.clone()),
                span,
            };
            emit_arith_store(name.clone(), Some(idx), rhs, span, ssa, state, block);
            Some(ArithExpr::Subscript {
                name: name.clone(),
                index: index.clone(),
            })
        }
        _ => None,
    }
}

/// Emit `name[index] = value` (index `None` = scalar) as an `Assignment`
/// statement lowered through the standard path, advancing `block`.
fn emit_arith_store(
    name: String,
    index: Option<crate::ir::Expr>,
    value: ArithExpr,
    span: Span,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) {
    let stmt = Statement {
        kind: StmtKind::Assignment(Assignment {
            name,
            index,
            op: AssignOp::Set,
            value: crate::ir::Expr {
                kind: crate::ir::ExprKind::Arithmetic(Box::new(value)),
                span,
            },
        }),
        span,
    };
    *block = lower_one(&stmt, ssa, state, *block);
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
    // DD#99: `map m = @{a:DISP}` fills a map from an array, pairwise.
    if is_assoc && lower_map_from_array(a, ssa, state, block) {
        return block;
    }
    let val = compute_declaration_value(a, is_array, is_assoc, ssa, state, &mut block);
    let val = shape_declared_value(val, d, is_array || is_assoc);
    let target = state.define(&a.name);
    ssa.block_mut(block)
        .insts
        .push(SsaInst::Assign { target, value: val });
    note_readonly(a, d, state);
    block
}

/// DD#76: an `int array`/`int map` stores each element as an int cell.
fn shape_declared_value(val: SsaValue, d: &Declaration, is_collection: bool) -> SsaValue {
    if d.var_type == crate::ir::VarType::Int && is_collection {
        wrap_int_cells(val)
    } else {
        val
    }
}

fn note_readonly(a: &Assignment, d: &Declaration, state: &mut VersionState) {
    if d.keyword == crate::ir::DeclKeyword::Readonly {
        state.readonly_vars.insert(a.name.clone());
    }
}

fn compute_declaration_value(
    a: &Assignment,
    is_array: bool,
    is_assoc: bool,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> SsaValue {
    // DD#41: the `array`/`map` KEYWORD is what makes a name a collection, so
    // register it from the declaration rather than inferring from the lowered
    // value. A glob literal (`array a=(*)`) lowers to a `glob` ParamExpansion,
    // not an IndexedArray, so shape-inference alone missed it — `join` saw
    // every element while `len` read the table as a string and answered 0.
    if is_array || is_assoc {
        state.collections.insert(a.name.clone());
        if is_assoc {
            state.assoc_arrays.insert(a.name.clone());
        }
    }
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

/// DD#76: wrap each element of an int-valued collection literal in an `IntCell`
/// so the generic table-store path emits the `(null, i64)` int-cell encoding.
fn wrap_int_cells(val: SsaValue) -> SsaValue {
    match val {
        SsaValue::IndexedArray(elems) => SsaValue::IndexedArray(
            elems
                .into_iter()
                .map(|e| SsaValue::IntCell(Box::new(e)))
                .collect(),
        ),
        SsaValue::AssocArray(pairs) => SsaValue::AssocArray(
            pairs
                .into_iter()
                .map(|(k, v)| (k, SsaValue::IntCell(Box::new(v))))
                .collect(),
        ),
        other => other,
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

/// A `attest VAR [[ … ]]` command as an `&&`/`||` operand — detected so
/// `lower_list_stmt` can route it through the if-attest catch machinery.
fn is_attest_left(stmt: &Statement) -> bool {
    matches!(&stmt.kind, StmtKind::Command(c)
        if expr_to_string(&c.name) == "attest" && c.args.len() >= 2)
}

/// DD#24/#47: a `attest` as the left operand of `&&`/`||` is a *caught* gate, not
/// a bare abort — its pass/fail must feed the short-circuit exactly like `if
/// attest`. Rewrite to the if-attest form so it reuses `lower_if_attest` (branch on
/// the test, retag clean on pass):
///   `attest V [[t]] || X`  →  `if attest V [[t]] { } else { X }`  (X on fail)
///   `attest V [[t]] && X`  →  `if attest V [[t]] { X }`           (X on pass)
/// Without this the attest lowers as the abort form and the `||` never catches it
/// — it aborts on failure, and on *success* the branch reads a stale status and
/// wrongly runs the right side (`attest … || v=dflt` gave the default even for
/// valid input). The fallback is trusted, not re-validated (DG 2026-09-01).
/// Returns `None` (fall through to the normal list lowering) unless the left is
/// a attest and the op is `&&`/`||`.
fn try_lower_attest_list(
    l: &List,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> Option<BlockId> {
    if !matches!(l.op, ListOp::And | ListOp::Or) || !is_attest_left(&l.left) {
        return None;
    }
    let cond = vec![l.left.as_ref().clone()];
    let right = vec![l.right.as_ref().clone()];
    let synth = if matches!(l.op, ListOp::Or) {
        If {
            condition: cond,
            then_body: vec![],
            elif_clauses: vec![],
            else_body: Some(right),
        }
    } else {
        If {
            condition: cond,
            then_body: right,
            elif_clauses: vec![],
            else_body: None,
        }
    };
    Some(lower_if(&synth, ssa, state, block))
}

/// DD#55: at top level, `a || b` is a guard — if the right side completes, the
/// list "succeeded", so reset `$?` to 0 so the program doesn't exit with the
/// left side's failure status. In a *caught* position (a condition, or the left
/// of an enclosing `&&`/`||`) the aggregate's real status is the condition
/// value — resetting it would make `if false || false` wrongly take the
/// then-branch, so keep the right side's true status. No-op when caught.
fn reset_or_status_at_top_level(
    caught: bool,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) {
    if caught {
        return;
    }
    let zero_ref = state.define("?");
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target: zero_ref,
        value: SsaValue::Literal("0".to_string()),
    });
}

fn lower_list_stmt(
    l: &List,
    _stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    mut block: BlockId,
) -> BlockId {
    if let Some(done) = try_lower_attest_list(l, ssa, state, block) {
        return done;
    }
    // DD#55: in a *caught* position — an `if`/`while` condition, or the left
    // operand of an enclosing `&&`/`||` (both bump `in_condition`) — the whole
    // aggregate's status is consumed, so the RIGHT operand's failure is caught,
    // not aborted. The LEFT operand is always a guard: its block ends in the
    // short-circuit `Branch`, so `mark_unchecked_in_conditions` already unchecks
    // it. At top level the rightmost operand's status is discarded → it stays
    // checked and aborts (bare-command-uncaught).
    let caught = state.in_condition > 0;
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
            if caught {
                super::entry::uncheck_last_condition_inst(&mut ssa.block_mut(right_end).insts);
            }
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
            if caught {
                super::entry::uncheck_last_condition_inst(&mut ssa.block_mut(right_end).insts);
            }
            reset_or_status_at_top_level(caught, ssa, state, right_end);
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

/// Emit the empty map plus its pairwise fill. `false` when the declaration's
/// value is not an array splat, so ordinary map literals fall through.
fn lower_map_from_array(
    a: &Assignment,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> bool {
    let Some(inst) = map_from_array_inst(a, state) else {
        return false;
    };
    state.collections.insert(a.name.clone());
    state.assoc_arrays.insert(a.name.clone());
    let target = state.define(&a.name);
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target,
        value: SsaValue::AssocArray(vec![]),
    });
    ssa.block_mut(block).insts.push(inst);
    true
}

/// DD#99: `map m = @{a:DISP}` — the source array splat and its odd-count
/// disposition. `None` when the declaration's value is not an array splat, so
/// ordinary map literals fall through. A BARE `@a` is rejected in validate,
/// not here (the diagnostic belongs with the other splat-position rules).
fn map_from_array_inst(a: &Assignment, state: &mut VersionState) -> Option<SsaInst> {
    let ExprKind::ParameterExpansion(pe) = &a.value.kind else {
        return None;
    };
    let (mode, message) = match pe.operator.as_deref()? {
        // `:~` is a wordless `arrvals_default`; `:-WORD` carries the key.
        "arrvals_default" if pe.operand.is_none() => (2u8, String::new()),
        "arrvals_default" => (0u8, String::new()),
        "arrvals_abort" => (
            1u8,
            pe.operand
                .as_deref()
                .map(expr_to_string)
                .unwrap_or_default(),
        ),
        _ => return None,
    };
    // For `:?` the abort MESSAGE rides in the key slot (the runtime prints it);
    // for `:-` it is the supplied KEY. Neither is used by `:~`/`:!~`.
    let key = match pe.operand.as_deref() {
        Some(w) if mode == 0 || mode == 1 => expr_to_value(w, state),
        _ => SsaValue::Literal(if mode == 1 {
            "map from array: odd element count".to_string()
        } else {
            String::new()
        }),
    };
    Some(SsaInst::MapFromArray {
        source: pe.name.clone(),
        target: a.name.clone(),
        mode,
        key,
        message,
    })
}
