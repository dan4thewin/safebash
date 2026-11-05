//! Statement lowering: IR Statement → SSA instructions.
//!
//! `lower_one` is the main dispatch — each StmtKind variant is handled.
//! SECURITY: This is where commands become SsaInst::Command with resolved names.

use super::control_flow::{flatten_elif, lower_case, lower_for, lower_if, lower_while};
use super::expressions::{arg_to_value, expr_to_value, literal_name_of};
use super::functions::{
    classify, classify_producer, collect_return_sites, exit_summary, lift_int_func_calls,
    returns_zero, Absent,
};
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

/// Lower a single statement into SSA. The statement's span locates any error
/// the transform raises while lowering it (`VersionState::stmt_span`); the
/// enclosing statement's is restored afterwards, because a nested body — an
/// inlined function, a `$( )` — is lowered from inside this call.
pub(super) fn lower_one(
    stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let enclosing = std::mem::replace(&mut state.stmt_span, stmt.span);
    let origin = ssa.enter_source(
        stmt.span,
        source_attest_span(stmt),
        matches!(stmt.kind, StmtKind::For(_)),
        block,
    );
    let block = lower_stmt_kind(stmt, ssa, state, block);
    ssa.leave_source(origin, block);
    state.flush_expr_diagnostics();
    state.stmt_span = enclosing;
    block
}

fn source_attest_span(stmt: &Statement) -> Option<Span> {
    let candidate = match &stmt.kind {
        StmtKind::If(i) if i.condition.len() == 1 => &i.condition[0],
        StmtKind::List(l) => l.left.as_ref(),
        _ => stmt,
    };
    attest_command(candidate).map(|_| candidate.span)
}

fn lower_stmt_kind(
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
    let tail = state.tail_loop.as_ref()?;
    if literal_name_of(&c.name) != tail.func {
        return None;
    }
    let (func, header, aname) = (tail.func.clone(), tail.header, tail.args_name.clone());
    let (scalar_args, coll_args) = super::functions::split_call_args(&c.args);
    if !coll_args.is_empty() {
        return None;
    }
    // The tail call stands in for `lower_one(inner)`, so it is lowered at the
    // call's own span, as the inlined call it replaces would be.
    let enclosing = std::mem::replace(&mut state.stmt_span, inner.span);
    let new_args: Vec<SsaValue> = scalar_args.iter().map(|a| arg_to_value(a, state)).collect();
    // DD#81 Part B checks arity at every call site, and a self tail-call is
    // one. It used to be checked only because the `fn_` template inlined it
    // as an ordinary call; the template now loops, so the check is made here.
    let body = state
        .function_bodies
        .get(&func)
        .cloned()
        .expect("a tail loop is entered only for a registered function body");
    super::cmd_handlers::check_callee_arity(&func, &body, &new_args, state);
    let after = super::functions::emit_tail_call(header, &aname, new_args, ssa, state, block);
    state.stmt_span = enclosing;
    Some(after)
}

/// `while`/`until`. Both forms lower through `lower_while`, which applies the
/// `until` negation itself — it is the only place that knows which block the
/// condition actually finishes in (see the comment there).
fn lower_while_stmt(
    w: &While,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    lower_while(w, ssa, state, block)
}

/// Function definition. Registers the body for inline expansion at call sites,
/// then emits the body into a dedicated SSA block sequence terminating in Return.
/// Special case: DD#55's SB4017. The name and its `-> int` return type were
/// decided up front by `register_functions`. A call runs the definition in
/// force, so the classes checked by subsequent calls (DD#55) are recomputed
/// as this definition comes into force.
/// Terminal handlers have separate roots and never reach this definition arm.
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
    let exits = exit_summary(f, |n| state.known_functions.contains(n));
    state.exits_in_force.insert(f.name.clone(), exits);
    state.class_in_force = classify(&state.exits_in_force, Absent::Status);
    report_arity_defects(f, stmt, state);
    analyze_function_returns(f, stmt, state);
    let fn_entry = ssa.new_block(&format!("fn_{}", f.name));
    let fn_exit = lower_fn_template(&f.name, &f.body, ssa, state, fn_entry);
    ssa.block_mut(fn_exit).terminator = Terminator::Return;
    block
}

/// The standalone `fn_NAME` block: a template lowered at the definition, with
/// no call site and no incoming edge (codegen ICEs if one is reachable). It is
/// lowered exactly as a call with no arguments is — `inlining`, its own `args`
/// and its own tail loop — so a self tail-call in it loops instead of inlining
/// the body a second time, which re-bound the body's `const`s and raised a
/// false SB4044 at the definition of every such function. $@/$* are rejected
/// at validation (DD#81) and never reach here.
///
/// `args_counter` is restored, so templates leave the numbering of the real
/// calls' `args` arrays as it was; `reserve_temp_name` keeps the names
/// distinct. Current versions are restored too: defining a function does not
/// execute its writes. The header must not be labelled `fn_…`: ssa_taint reads
/// those labels as function names.
fn lower_fn_template(
    name: &str,
    body: &[Statement],
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    fn_entry: BlockId,
) -> BlockId {
    use super::functions as fns;
    let saved_versions = state.snapshot();
    let saved_flag = std::mem::replace(&mut state.lowering_fn_template, true);
    state.inlining.push(name.to_string());
    let saved_args = fns::save_caller_args(state);
    let saved_slots = fns::setup_callee_slots(&[], state);
    let saved_ro = fns::enter_fn_body_scope(state);
    let saved_counter = state.args_counter;
    fns::setup_callee_args(&[], ssa, state, fn_entry);
    let header = ssa.new_checkpoint_block(&format!("call_{}", name));
    ssa.block_mut(fn_entry).terminator = Terminator::Goto(header);
    let saved_tail = state.enter_tail_loop(name, header);
    let fn_exit = fns::lower_body(body, ssa, state, header);
    state.tail_loop = saved_tail;
    state.args_counter = saved_counter;
    fns::exit_fn_body_scope(state, saved_ro);
    fns::restore_caller_slots(saved_slots, state);
    fns::restore_caller_args(saved_args, state);
    state.inlining.pop();
    state.lowering_fn_template = saved_flag;
    state.restore(&saved_versions);
    fn_exit
}

/// DD#81 rule (a) contiguity + DD#84 mixing / optional-trailing: emit each
/// structural defect of the fixed-arity `args` signature at the definition.
fn report_arity_defects(f: &Function, stmt: &Statement, state: &mut VersionState) {
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
}

/// DD#55: an `-> int` function whose every exit yields 0 should be void
/// (SB4017). A `return g` exit yields what `g` does: a self tail-call is a
/// loop, not an exit, a void function yields 0, and any other command can
/// yield a non-zero status. Checked once per definition, so `g` counts as
/// void only when it is whatever the call site — a redefined `g` may not be.
fn analyze_function_returns(f: &Function, stmt: &Statement, state: &mut VersionState) {
    if f.return_type != crate::ir::VarType::Int {
        return;
    }
    let sites = collect_return_sites(&f.body);
    let producer_exits = sites.producers.iter().filter_map(|p| {
        classify_producer(p, &f.name, |n| state.known_functions.contains(n))
            .exit_yields_zero(|g| state.class_static.get(g) == Some(&FnClass::Void))
    });
    // One entry per exit: whether it can only hand back 0.
    let yields_zero: Vec<bool> = sites
        .values
        .iter()
        .map(returns_zero)
        .chain(producer_exits)
        .collect();
    if yields_zero.is_empty() || yields_zero.contains(&false) {
        return;
    }
    state.diagnostics.push(format!(
        "{}:{}: error [SB4017] function '{}' only returns 0 — omit the return (void function) or return a meaningful status",
        stmt.span.line, stmt.span.col, f.name
    ));
}

/// Statement wrapped in a redirect.
fn lower_redirected_stmt(
    r: &Redirected,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    // Most bodies that reach here attach their redirects to the single inst
    // they lower to. A compound body cannot reach here at all — the parser
    // gives SB4021 for while/if/for/case/`{ }`, and SB4046 for a function
    // definition, declaration, assignment, or a test that produces no output.
    // DD#99 `read` lowers to `ReadArray`, which carries its own redirect list
    // and therefore gets the DD#55 open(2) guard.
    //
    // A call to a user function is the exception: SB4021 tells the user to
    // "wrap the compound in a function" precisely so it can be redirected, so
    // `f > out.txt` is the sanctioned spelling of a compound redirect. Its
    // body is inlined across several blocks and has no single inst to carry
    // the list, so it gets a real scope — parked here and opened by
    // `inline_function_call`, which holds those blocks. Without it the
    // redirect attached to nothing and was silently dropped.
    let redirects = spill_scalar_redirect_targets(&r.redirects, ssa, state, block);
    let bracketed = body_calls_user_function(&r.body, state);
    if bracketed {
        state.pending_redirect_scope = Some(redirects.clone());
    }

    let pipe_reader = state.pipe_reader.take();
    let result_block = lower_one(&r.body, ssa, state, block);
    state.pipe_reader = pipe_reader;
    // A rejected recursive call leaves its scope parked and emits no command.
    // Discard it; only a non-function command needs per-instruction redirects.
    state.pending_redirect_scope = None;
    if !bracketed {
        attach_redirects_to_last_inst(ssa, result_block, &redirects);
    }
    result_block
}

/// Does this redirected body resolve to a call of a user function — the one
/// shape whose lowering spans blocks and so needs a redirect *scope*?
///
/// Deliberately narrow: a bare `Command` whose name is a plain word already
/// registered as a function. A builtin, an external, or anything wearing a
/// sigil keeps the per-inst path. `function_bodies` is the same table
/// `handle_function_call_or_command` consults, so the two agree by
/// construction.
fn body_calls_user_function(body: &Statement, state: &VersionState) -> bool {
    let StmtKind::Command(c) = &body.kind else {
        return false;
    };
    state
        .function_bodies
        .contains_key(crate::transform::expressions::literal_name_of(&c.name))
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
///
/// `$$` and `$!` need the same treatment for the same reason: they are served
/// by intrinsic codegen (`sash_getpid` / the last-child pid), not by a
/// `VarSlot`, so the slot-based redirect-target codegen cannot see them. Left
/// alone, `> $$` raised the unresolved-target ICE and `> $!` took the silent
/// empty-filename path this function exists to close. Spilling them makes
/// `> $$` mean exactly what `str s=$$; > $s` already meant.
fn is_slotless_special(v: &str) -> bool {
    v == "$" || v == "!"
}

fn target_needs_spill(kind: &ExprKind) -> bool {
    match kind {
        ExprKind::ParameterExpansion(_) | ExprKind::Subscript { .. } => true,
        ExprKind::SimpleExpansion(v) => is_slotless_special(v),
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
            let name = state.reserve_temp_name(
                format!("__redir_target_{}_{}", ssa.blocks.len(), out.len()),
                VarType::String,
            );
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

/// Simple command + redirects: attach the redirects directly to the last
/// emitted inst (Command / Pipeline last-stage / ReadLine / ReadArray).
/// The caller passes the final command block, not its argument-setup block;
/// rejected function calls do not reach this attachment path.
///
/// **Total over `SsaInst` on purpose — do not add a wildcard.** The arms that
/// ICE below used to be a single `_ => {}`, and it was silently discarding
/// redirects: `return 0 >f`, `unset m[k] >f` and `if match … >f` all compiled
/// clean and never opened the file. So did the no-instruction case above,
/// which used to be a bare `return`.
///
/// Nothing can legally arrive here in those shapes any more. SB4046 is now a
/// closed allow-list applied in `parse_command`, so a redirect only survives
/// parsing on `echo`, `write`, `exec`, `:`, `read`/`readline`, an external
/// command or a user function — which is exactly the set that lowers to the
/// four handled arms — or on a pipeline. A panic from here is therefore a
/// report that the parse gate and this lowering have drifted apart, and the
/// repair is to reconcile them, not to restore a wildcard that would resume
/// dropping output on the floor.
///
/// The allow-list is [`crate::resolve::head_may_take_redirect`], **not**
/// `BuiltinCmd::may_take_redirect`, which this doc and both panics below used
/// to name. That method maps all twelve `resolve::INTRINSICS` to
/// `BuiltinCmd::Other` and so answers "allow" for every one of them; it is the
/// enum-total list, not the authority for a *name*.
///
/// The `SsaInst::Pipeline` arm is unreachable by construction and is kept only
/// for totality. `a | b > f` parses as `Pipeline[Command, Redirected(Command)]`
/// — the redirect binds to the final *stage*, which is bash's semantics — so
/// it arrives via `stage_redirect_flags`, never here. Reaching this arm needs
/// a `Redirected(Pipeline)`, and every spelling that would wrap a pipeline as
/// a unit is rejected at parse: `{ a | b } > f` is SB4021, a backgrounded one
/// SB4022. Measured 2026-10-11 by replacing the arm with a panic and running
/// the suite, which stayed green; spec `rdr.47`-`rdr.49` cover the real path.
fn attach_redirects_to_last_inst(ssa: &mut SsaProgram, blk_id: BlockId, redirects: &[Redirect]) {
    let blk = ssa.block_mut(blk_id);
    let Some(inst) = blk
        .insts
        .iter_mut()
        .rfind(|inst| !matches!(inst, SsaInst::SourceOrigin(..)))
    else {
        panic!(
            "ICE: a statement carrying {} redirect(s) lowered to no instruction at all — \
             SB4046 (resolve::head_may_take_redirect) should have rejected it at parse",
            redirects.len()
        );
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
        SsaInst::Assign { .. }
        | SsaInst::Test(..)
        | SsaInst::SubscriptAssign { .. }
        | SsaInst::ArrayAppend { .. }
        | SsaInst::MapFromArray { .. }
        | SsaInst::SplitValue { .. }
        | SsaInst::MatchInto { .. }
        | SsaInst::ArgvFill { .. }
        | SsaInst::KeysFill { .. }
        | SsaInst::CaptureStart { .. }
        | SsaInst::CaptureEnd { .. }
        | SsaInst::RedirectPush { .. }
        | SsaInst::RedirectPop { .. }
        | SsaInst::SourceOrigin(..)
        | SsaInst::PipelineFinish { .. }
        | SsaInst::Attest { .. }
        | SsaInst::AttestAll { .. }
        | SsaInst::AttestRetag { .. } => {
            panic!(
                "ICE: {} redirect(s) reached an instruction that cannot carry them — \
                 SB4046 (resolve::head_may_take_redirect) should have rejected it at parse: {inst:?}",
                redirects.len()
            );
        }
    }
}

/// `(( expr ))` as a condition: evaluate arithmetic, set exit status based on
/// result (0 = true if nonzero, 1 = false if zero).
/// Feed the integer directly to `LetExit`; an untyped temporary would force an
/// int -> string -> int roundtrip in both backends.
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
    // DD#55 2026-10-30: `- (( … ))` discards the test's status, not its operands'.
    let val =
        super::functions::unsuppressed(state, |state| lift_int_func_calls(val, ssa, state, block));
    ssa.block_mut(*block).insts.push(SsaInst::internal(
        BuiltinCmd::LetExit,
        vec![val],
        state.suppressed == 0,
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
        ArithExpr::PreIncDec { .. } => unreachable!("{}", crate::ir::PREFIX_INCDEC_INVARIANT),
        ArithExpr::PostIncDec { .. } | ArithExpr::SubscriptAssign { .. } => {
            unreachable!("{}", crate::ir::ARITH_TEST_HOISTABLE_INVARIANT)
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

/// DD#55: lower the suppressed operation with its own status discarded.
/// Every check reads `state.suppressed` where it is constructed; operand
/// lifts and inlined function bodies clear it, so what the operation
/// evaluates on the way stays checked (DD#55 2026-10-30).
fn lower_suppressed_stmt(
    inner: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    check_void_suppression(inner, state);
    state.suppressed += 1;
    let result_block = lower_one(inner, ssa, state, block);
    state.suppressed -= 1;
    result_block
}

/// DD#55 (2026-10-29): on a user function, `-` discards the value it returns,
/// so a void function leaves it nothing to discard (SB4076). The class is
/// that of the definition the call runs. A redirect does not change what the
/// call returns. A stage or a job is a process with an exit status of its
/// own, and never reaches here.
fn check_void_suppression(inner: &Statement, state: &mut VersionState) {
    let call = match &inner.kind {
        StmtKind::Redirected(r) => &r.body,
        _ => inner,
    };
    let StmtKind::Command(c) = &call.kind else {
        return;
    };
    let ExprKind::Literal(name) = &c.name.kind else {
        return;
    };
    if state.call_class(name) == Some(FnClass::Void) {
        state.diagnostics.push(format!(
            "{}:{}: error [SB4076] function '{}' returns nothing, so `-` has nothing \
             to discard — call it without `-`",
            state.stmt_span.line, state.stmt_span.col, name
        ));
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
    a: &DeclAssignment,
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
    let target = state.define_replacement(&a.name);
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

fn note_readonly(a: &DeclAssignment, d: &Declaration, state: &mut VersionState) {
    if d.keyword == crate::ir::DeclKeyword::Readonly {
        state.readonly_vars.insert(a.name.clone());
    }
}

fn compute_declaration_value(
    a: &DeclAssignment,
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
    register_collection_name(&a.name, is_array, is_assoc, state);
    // No initializer at all (`array arr`, `int x`). Previously this arrived
    // as an in-band `Literal("")`, indistinguishable from the explicit
    // `array a=""` that stored a SCALAR into a collection slot and segfaulted.
    let Some(value) = &a.value else {
        return uninitialized_value(&a.name, is_array, is_assoc, state);
    };
    // `array arr=()` / `map m=()` — an empty table of the declared kind.
    if is_empty_collection_init(value, is_array, is_assoc) {
        return empty_collection_value(&a.name, is_assoc, state);
    }
    let val = expr_to_value(value, state);
    // Option C: `$(f)` on an `-> int` function is an IntFuncCall (the return
    // value); lift_int_func_calls materializes it here, exactly like `$((f))`.
    let val = lift_int_func_calls(val, ssa, state, block);
    track_collection_kind(&a.name, &val, state);
    val
}

/// DD#41: the `array`/`map` KEYWORD is what makes a name a collection, so
/// register it from the declaration rather than inferring from the lowered
/// value.
fn register_collection_name(name: &str, is_array: bool, is_assoc: bool, state: &mut VersionState) {
    if is_array || is_assoc {
        state.collections.insert(name.to_string());
    }
    if is_assoc {
        state.assoc_arrays.insert(name.to_string());
    }
}

/// The value a declaration with no initializer binds: an empty table of the
/// declared kind, or the empty string for a scalar.
fn uninitialized_value(
    name: &str,
    is_array: bool,
    is_assoc: bool,
    state: &mut VersionState,
) -> SsaValue {
    if is_array || is_assoc {
        return empty_collection_value(name, is_assoc, state);
    }
    expr_to_value(&empty_literal(), state)
}

/// The empty-string initializer a bare scalar declaration (`str s`, `int n`)
/// lowers to. `int` turns it into a zero via `int_literal_or_zero_init`.
fn empty_literal() -> Expr {
    Expr {
        kind: ExprKind::Literal(String::new()),
        span: Span { line: 0, col: 0 },
    }
}

/// True for an explicit empty collection literal: `array arr=()` / `map m=()`.
fn is_empty_collection_init(value: &Expr, is_array: bool, is_assoc: bool) -> bool {
    matches!(&value.kind, ExprKind::Array(e) if e.is_empty()) && (is_array || is_assoc)
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
        _ => unreachable!("an int collection initializer must lower to a table literal"),
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

/// DD#24's `attest VAR [[ … ]]` command, if this statement is one.
///
/// Two callers ask the same question of different shapes — `is_attest_left`
/// wants a yes/no for an `&&`/`||` operand, `lower_if` wants the `Command`
/// back out of a one-statement condition — so the predicate lives here once.
/// A second spelling of it is exactly how a fix lands on one copy and not
/// the other.
pub(super) fn attest_command(stmt: &Statement) -> Option<&Command> {
    let StmtKind::Command(c) = &stmt.kind else {
        return None;
    };
    (literal_name_of(&c.name) == "attest" && c.args.len() >= 2).then_some(c)
}

/// A `attest VAR [[ … ]]` command as an `&&`/`||` operand — detected so
/// `lower_list_stmt` can route it through the if-attest catch machinery.
fn is_attest_left(stmt: &Statement) -> bool {
    attest_command(stmt).is_some()
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
    block: BlockId,
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
    //
    // Suppression consumes the status too: under `-` the right operand runs
    // unchecked, so its completing says nothing about success, and `||`'s
    // top-level reset would turn a failed right side into status 0
    // (`- /bin/false || /bin/false` exited 0, B84).
    let caught = state.in_condition > 0 || state.suppressed > 0;
    match l.op {
        ListOp::And | ListOp::Or => lower_short_circuit(l, ssa, state, block, caught),
        ListOp::Background => lower_background(l, ssa, state, block),
    }
}

/// `left && right` / `left || right`. One implementation: the two differ only
/// in which way the guard branches and in `||`'s top-level status reset, so
/// keeping them apart meant two copies of the block wiring.
fn lower_short_circuit(
    l: &List,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    mut block: BlockId,
    caught: bool,
) -> BlockId {
    let is_and = matches!(l.op, ListOp::And);
    let tag = if is_and { "and" } else { "or" };

    state.in_condition += 1;
    block = lower_one(&l.left, ssa, state, block);
    state.in_condition -= 1;

    let right_block = ssa.new_block(&format!("{tag}.right"));
    let merge_block = ssa.new_block(&format!("{tag}.merge"));
    // `&&` runs the right operand when the left SUCCEEDED; `||` when it failed.
    let (true_block, false_block) = match is_and {
        true => (right_block, merge_block),
        false => (merge_block, right_block),
    };
    ssa.block_mut(block).terminator = Terminator::Branch {
        condition: block.to_string(),
        true_block,
        false_block,
    };

    let right_end = lower_one(&l.right, ssa, state, right_block);
    if caught {
        super::entry::uncheck_last_condition_inst(&mut ssa.block_mut(right_end).insts);
    }
    if !is_and {
        reset_or_status_at_top_level(caught, ssa, state, right_end);
    }
    ssa.block_mut(right_end).terminator = Terminator::Goto(merge_block);
    merge_block
}

/// `cmd &` — fork; the child runs the command and exits with its status
/// (never through a handler — DD#128), the parent continues with the pid in `$!`.
/// The child is lowered in a forked context, so none of its versions,
/// reader bindings or jump targets reach the parent's code.
fn lower_background(
    l: &List,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let child_block = ssa.new_block("bg.child");
    let parent_block = ssa.new_block("bg.parent");
    ssa.block_mut(block).terminator = Terminator::Fork {
        parent: parent_block,
        child: child_block,
    };

    let parent = state.enter_forked_child();
    let child_end = lower_one(&l.left, ssa, state, child_block);
    ssa.block_mut(child_end).terminator = Terminator::ChildExit;
    state.exit_forked_child(parent);

    // `$!` is defined after the child, so the child reads the previous job's
    // pid. The parent block is still empty: the fork's pid is the first thing
    // it reads.
    let bg_target = state.define("!");
    ssa.block_mut(parent_block).insts.push(SsaInst::internal(
        BuiltinCmd::StoreBgPid,
        vec![SsaValue::Var(bg_target)],
        state.suppressed == 0,
    ));
    parent_block
}

/// Emit the empty map plus its pairwise fill. `false` when the declaration's
/// value is not an array splat, so ordinary map literals fall through.
fn lower_map_from_array(
    a: &DeclAssignment,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> bool {
    let Some((source, mode, key, message, taint_off)) = map_from_array_parts(a, state) else {
        return false;
    };
    let source = state.lookup(&source);
    state.collections.insert(a.name.clone());
    state.assoc_arrays.insert(a.name.clone());
    let target = state.define_replacement(&a.name);
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target,
        value: SsaValue::AssocArray(vec![]),
    });
    // The fill is a separate definition from the empty-map initialization.
    let target = state.define_replacement(&a.name);
    ssa.block_mut(block).insts.push(SsaInst::MapFromArray {
        source,
        target,
        mode,
        key,
        message,
        taint_off,
    });
    true
}

/// DD#99: `map m = @{a:DISP}` — the source array splat and its odd-count
/// disposition. `None` when the declaration's value is not an array splat, so
/// ordinary map literals fall through. A BARE `@a` is rejected in validate,
/// not here (the diagnostic belongs with the other splat-position rules).
/// DD#99 fill mode for `map m=@{a:DISP}`: `:-` carries a key (0), `:?` an
/// abort message (1), `:~` is wordless (2). Any other operator is not a fill.
fn arrvals_fill_mode(pe: &ParameterExpansion) -> Option<(u8, String)> {
    match pe.operator.as_deref()? {
        "arrvals_default" if pe.operand.is_none() => Some((2u8, String::new())),
        "arrvals_default" => Some((0u8, String::new())),
        "arrvals_abort" => Some((
            1u8,
            pe.operand
                .as_deref()
                .map(|e| literal_name_of(e).to_string())
                .unwrap_or_default(),
        )),
        _ => None,
    }
}

fn map_from_array_parts(
    a: &DeclAssignment,
    state: &mut VersionState,
) -> Option<(String, u8, SsaValue, String, bool)> {
    let ExprKind::ParameterExpansion(pe) = &a.value.as_ref()?.kind else {
        return None;
    };
    let (mode, message) = arrvals_fill_mode(pe)?;
    // For `:?` the abort MESSAGE rides in the key slot (the runtime prints it);
    // for `:-` it is the supplied KEY. Neither is used by `:~`/`:!~`.
    let key = match pe.operand.as_deref() {
        Some(w) if mode == 0 || mode == 1 => expr_to_value(w, state),
        _ => SsaValue::Literal(String::new()),
    };
    Some((pe.name.clone(), mode, key, message, pe.taint_off))
}

/// The should-never-run assertions in this file guard shapes an earlier pass
/// makes unreachable: the parser only builds `StmtKind::Negated` for a
/// non-leading pipeline stage, `reject_prefix_incdec` raises SB4020 on every
/// `PreIncDec`, and `check_arith_test_placement` (SB4020) rejects `++`/`--`
/// and `a[i]` mutations in a `(( … ))` statement or condition before the
/// transform runs. DG's rule for the coverage campaign: an unreachable
/// assertion earns a unit test proving it catches what it claims. A test
/// that stops panicking means the earlier pass has a hole.
#[cfg(test)]
mod invariant_tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn redirect_scope_classification_declines_non_commands() {
        let (_, mut state, _) = fixtures();
        state.function_bodies.insert("f".into(), vec![]);
        let command =
            Statement::command(Expr::literal("f", Span::default()), vec![], Span::default());
        let wrapped = stmt(StmtKind::Negated(Box::new(command)));
        assert!(!body_calls_user_function(&wrapped, &state));
    }

    #[test]
    #[should_panic(expected = "an int collection initializer must lower to a table literal")]
    fn a_scalar_cannot_silently_initialize_an_int_table() {
        wrap_int_cells(SsaValue::Literal("not a table".into()));
    }

    #[test]
    fn a_non_fill_parameter_operator_is_not_a_map_fill() {
        let ExprKind::ParameterExpansion(pe) =
            Expr::param("a", Some("arrjoin"), None, Span::default()).kind
        else {
            unreachable!()
        };
        assert_eq!(arrvals_fill_mode(&pe), None);
    }

    fn fixtures() -> (SsaProgram, VersionState, BlockId) {
        let mut ssa = SsaProgram::new();
        let block = ssa.new_block("entry");
        ssa.entry = block;
        (ssa, VersionState::with_types(HashMap::new()), block)
    }

    fn stmt(kind: StmtKind) -> Statement {
        Statement {
            kind,
            span: Span::new(1, 1),
        }
    }

    #[test]
    #[should_panic(expected = "never reaches statement dispatch")]
    fn lower_one_rejects_a_per_segment_negation() {
        // `! cmd` standing alone is `Pipeline { negated: true, .. }`;
        // `StmtKind::Negated` is only ever a non-leading pipeline *stage*,
        // and `push_pipeline_stage` peels it there.
        let inner = stmt(StmtKind::Command(Command {
            name: Expr {
                kind: ExprKind::Literal("true".to_string()),
                span: Span::new(1, 1),
            },
            args: vec![],
        }));
        let (mut ssa, mut state, block) = fixtures();
        let negated = stmt(StmtKind::Negated(Box::new(inner)));
        lower_one(&negated, &mut ssa, &mut state, block);
    }

    #[test]
    #[should_panic(expected = "prefix ++/-- never survives the parser")]
    fn hoist_mutation_node_rejects_prefix_incdec() {
        let (mut ssa, mut state, mut block) = fixtures();
        let a = ArithExpr::PreIncDec {
            var: "x".to_string(),
            inc: true,
        };
        hoist_mutation_node(&a, Span::new(1, 1), &mut ssa, &mut state, &mut block);
    }

    #[test]
    fn hoist_mutation_node_rejects_the_non_hoistable_mutations() {
        let cases = [
            ArithExpr::PostIncDec {
                var: "x".to_string(),
                inc: true,
            },
            ArithExpr::SubscriptAssign {
                name: "a".to_string(),
                index: Box::new(ArithExpr::Literal(0)),
                op: None,
                value: Box::new(ArithExpr::Literal(1)),
            },
        ];
        for a in cases {
            let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let (mut ssa, mut state, mut block) = fixtures();
                hoist_mutation_node(&a, Span::new(1, 1), &mut ssa, &mut state, &mut block)
            }));
            assert!(caught.is_err(), "SB4020 rejects {a:?} before the transform");
        }
    }
}

/// The three cold sites in [`attach_redirects_to_last_inst`]. Two are ICEs
/// and one is a live totality arm, and all three are unreachable from any
/// *program* — so under DG's coverage rule they are the unit-test half of
/// the campaign rather than exempt. Each ICE test asserts the message names
/// the rule that guarantees it; a test that stops panicking is the finding.
#[cfg(test)]
mod redirect_attach_tests {
    use super::*;
    use crate::ir::{Redirect, RedirectOp};

    fn one_redirect() -> Vec<Redirect> {
        vec![Redirect {
            fd: None,
            op: RedirectOp::Write,
            target: Expr {
                kind: ExprKind::Literal("/tmp/x".into()),
                span: Span::new(1, 1),
            },
        }]
    }

    fn stage(name: &str) -> (String, Vec<SsaValue>, Vec<Redirect>) {
        (name.to_string(), vec![], vec![])
    }

    /// Per-stage redirect counts, or an empty vec if the instruction is not
    /// a `Pipeline` -- which fails the caller's `assert_eq!` with the counts
    /// it expected, rather than needing an unreachable panic arm of its own.
    fn stage_redirect_counts(ssa: &mut SsaProgram, blk: BlockId) -> Vec<usize> {
        match &ssa.block_mut(blk).insts[0] {
            SsaInst::Pipeline { stages, .. } => stages.iter().map(|s| s.2.len()).collect(),
            _ => vec![],
        }
    }

    fn catch(f: impl FnOnce()) -> String {
        let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
            .expect_err("this shape must ICE");
        // Both ICEs here panic with format arguments, so the payload is
        // always a `String` -- never the `&'static str` a literal would give.
        err.downcast_ref::<String>()
            .cloned()
            .expect("ICE payload is a formatted String")
    }

    /// A redirect-carrying statement that lowered to nothing at all. SB4046
    /// rejects the heads that lower to no instruction, so the block is never
    /// empty here.
    #[test]
    fn redirects_on_an_empty_block_are_a_loud_ice() {
        let msg = catch(|| {
            let mut ssa = SsaProgram::new();
            let blk = ssa.new_block("entry");
            attach_redirects_to_last_inst(&mut ssa, blk, &one_redirect());
        });
        assert!(
            msg.contains("SB4046") && msg.contains("head_may_take_redirect"),
            "the ICE must name the rule that guarantees it, got: {msg}"
        );
    }

    /// An instruction that cannot carry redirects at all — SB4046 again.
    /// `Assign` stands for the eighteen arms that share this panic.
    #[test]
    fn redirects_on_a_non_redirectable_inst_are_a_loud_ice() {
        let msg = catch(|| {
            let mut ssa = SsaProgram::new();
            let blk = ssa.new_block("entry");
            ssa.block_mut(blk).insts.push(SsaInst::Assign {
                target: crate::ssa::VarRef::new("x", 0),
                value: SsaValue::Literal("1".into()),
            });
            attach_redirects_to_last_inst(&mut ssa, blk, &one_redirect());
        });
        assert!(
            msg.contains("SB4046") && msg.contains("head_may_take_redirect"),
            "the ICE must name the rule that guarantees it, got: {msg}"
        );
    }

    /// The `Pipeline` arm. Not an ICE — it does real work, and no program
    /// reaches it because the parser binds `a | b > f` to the final *stage*
    /// (`Pipeline[Command, Redirected(Command)]`) and rejects every spelling
    /// that would wrap a pipeline as a unit: `{ a | b } > f` is SB4021, a
    /// backgrounded one SB4022.
    ///
    /// It is kept rather than made an ICE because it is the correct answer
    /// if that binding ever changes. The test pins *which* stage receives
    /// the redirects, which is the only thing the arm decides and the one
    /// thing a reader could plausibly get backwards.
    #[test]
    fn a_pipeline_passes_redirects_to_its_final_stage_only() {
        let mut ssa = SsaProgram::new();
        let blk = ssa.new_block("entry");
        ssa.block_mut(blk).insts.push(SsaInst::Pipeline {
            stages: vec![stage("/bin/echo"), stage("/bin/cat")],
            capture: PipelineCapture::None,
            checked: true,
            suppressed: vec![],
            negated: vec![],
        });
        attach_redirects_to_last_inst(&mut ssa, blk, &one_redirect());

        assert_eq!(
            stage_redirect_counts(&mut ssa, blk),
            vec![0, 1],
            "a whole-pipeline redirect lands on the final stage and no other"
        );
    }

    /// The `if let`'s implicit else: a `Pipeline` with no stages at all.
    /// `push_pipeline_stage` always pushes at least one, so this is a second
    /// layer of unreachability inside an already-unreachable arm.
    ///
    /// The arm drops the redirects silently rather than panicking. That is
    /// the right call precisely *because* this is already dead code -- an
    /// ICE here would claim something about pipeline construction that this
    /// function cannot see. The test pins the silence so it stays a choice
    /// rather than an oversight.
    #[test]
    fn an_empty_pipeline_swallows_redirects_without_panicking() {
        let mut ssa = SsaProgram::new();
        let blk = ssa.new_block("entry");
        ssa.block_mut(blk).insts.push(SsaInst::Pipeline {
            stages: vec![],
            capture: PipelineCapture::None,
            checked: true,
            suppressed: vec![],
            negated: vec![],
        });
        attach_redirects_to_last_inst(&mut ssa, blk, &one_redirect());
        assert!(
            matches!(
                &ssa.block_mut(blk).insts[0],
                SsaInst::Pipeline { stages, .. } if stages.is_empty()
            ),
            "the instruction must be left as an empty Pipeline"
        );
    }
}
