//! Function-related transformations: int-func lifting, inlining, helpers.

use crate::ir::*;
use crate::ssa::*;

use super::statements::lower_stmts;

/// DD#75 §5: bind the callee's implicit `args` array (0-indexed) from the
/// call's argument values, and mark `args` a collection for the inlined scope
/// so subscript/splat/length ops use the indexed-array path. `$1..$9`/`$@`/`$#`
/// desugar to reads of this array (see `simple_expansion_to_value`).
/// DD#75 §5: materialize the callee's implicit `args` array (0-indexed,
/// read-only) under a FRESH unique name, so nested/recursive calls never
/// collide — no save/restore of the array itself is needed. Records the name
/// as the current positional scope; `$1..$9`/`$@`/`$#` (and a user-written
/// `args`) resolve against it (see `positional_to_value`/`resolve_args_name`).
pub(super) fn setup_callee_args(
    arg_values: &[SsaValue],
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) {
    let name = format!("__args{}", state.args_counter);
    state.args_counter += 1;
    let v = state.define(&name);
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target: v,
        value: SsaValue::IndexedArray(arg_values.to_vec()),
    });
    state.collections.insert(name.clone());
    state.current_args_name = name;
}

/// Save the *name* of the current positional scope (a cheap string — the array
/// itself is uniquely named, so nothing in SSA needs saving/restoring).
pub(super) fn save_caller_args(state: &mut VersionState) -> String {
    std::mem::take(&mut state.current_args_name)
}

pub(super) fn restore_caller_args(saved: String, state: &mut VersionState) {
    state.current_args_name = saved;
}

/// DD#86: a `const`/`readonly` declaration inside a function body is a
/// *function-local* — its readonly-ness must not leak into the caller scope.
/// A function body is lowered more than once (a throwaway template at the
/// definition + once per inlined call), so snapshot `readonly_vars` before
/// lowering the body and restore it after. A top-level const remains in the
/// restored set, so global-const enforcement is preserved; a fn-local const
/// is dropped, so re-lowering the same declaration doesn't trip the readonly
/// backstop.
pub(super) fn enter_fn_body_scope(state: &VersionState) -> std::collections::HashSet<String> {
    state.readonly_vars.clone()
}

pub(super) fn exit_fn_body_scope(
    state: &mut VersionState,
    saved: std::collections::HashSet<String>,
) {
    state.readonly_vars = saved;
}

pub(super) fn lift_int_func_calls(
    val: SsaValue,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> SsaValue {
    match val {
        SsaValue::IntFuncCall { name, args } => {
            // Inline the function and capture return value
            let lifted_args: Vec<SsaValue> = args
                .into_iter()
                .map(|a| lift_int_func_calls(a, ssa, state, block))
                .collect();
            inline_int_func(&name, &lifted_args, ssa, state, block)
        }
        SsaValue::FuncCall { name, args } => {
            // Option A (string-func-capture-in-arg-ice): a non-int function's
            // `$(f …)` stdout capture in argument position. Reuse the assignment
            // path's capture-inline machinery (CaptureStart/End) to inline the
            // body and capture its stdout into a temp, then use the temp — so it
            // works for runtime output too, not just const-foldable stdout.
            // DD#81: the call's args are carried on FuncCall and threaded into
            // the capture so the callee's `args` array is set up (was dropped).
            let lifted_args: Vec<SsaValue> = args
                .into_iter()
                .map(|a| lift_int_func_calls(a, ssa, state, block))
                .collect();
            let temp = format!("__strcap_{name}");
            match super::stmt_handlers::capture_func_into(
                &temp,
                &name,
                &lifted_args,
                state,
                ssa,
                *block,
            ) {
                Some(new_block) => {
                    *block = new_block;
                    SsaValue::Var(state.lookup(&temp))
                }
                None => SsaValue::FuncCall {
                    name,
                    args: lifted_args,
                },
            }
        }
        SsaValue::CmdSubstBody { body, .. } => {
            // A multi-statement/pipeline `$(…)` body: capture its stdout into a
            // temp (fresh name from the block count), then use the temp. The
            // capture is tainted (external output).
            let temp = format!("__bodycap_{}", ssa.blocks.len());
            *block = super::stmt_handlers::capture_body_into(&temp, &body, state, ssa, *block);
            SsaValue::Var(state.lookup(&temp))
        }
        SsaValue::ArithBinOp { left, op, right } => {
            let l = lift_int_func_calls(*left, ssa, state, block);
            let r = lift_int_func_calls(*right, ssa, state, block);
            SsaValue::ArithBinOp {
                left: Box::new(l),
                op,
                right: Box::new(r),
            }
        }
        SsaValue::Ternary {
            cond,
            true_val,
            false_val,
        } => {
            let c = lift_int_func_calls(*cond, ssa, state, block);
            let t = lift_int_func_calls(*true_val, ssa, state, block);
            let f = lift_int_func_calls(*false_val, ssa, state, block);
            SsaValue::Ternary {
                cond: Box::new(c),
                true_val: Box::new(t),
                false_val: Box::new(f),
            }
        }
        SsaValue::Concat(parts) => {
            // A double-quoted string with an embedded `$(f)` / `$((f))` lowers
            // to a Concat; lift each part so a nested IntFuncCall materializes.
            let lifted = parts
                .into_iter()
                .map(|p| lift_int_func_calls(p, ssa, state, block))
                .collect();
            SsaValue::Concat(lifted)
        }
        other => other, // no IntFuncCall inside
    }
}

/// Option C (DD#55): materialize any `IntFuncCall` (produced by `$(f)` /
/// `$((f))` on an `-> int` function) in a command's argument list into a temp
/// variable via [`lift_int_func_calls`]. An `IntFuncCall` that reaches codegen
/// unlifted is an ICE, so every arg-building site that can carry a `$(f)`
/// capture routes its argv through this.
pub(super) fn lift_int_func_args(
    args: Vec<SsaValue>,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> Vec<SsaValue> {
    args.into_iter()
        .map(|v| lift_int_func_calls(v, ssa, state, block))
        .collect()
}

/// Inline a -> int function call, returning an SsaValue::Var pointing
/// to the temp variable holding the return value.
/// DD#27/§5: diagnostic for a rejected recursive call. Functions are inlined
/// (no runtime call stack), so a call cycle can't be inlined — it is rejected,
/// except a direct self tail-call (`return F …`), which lowers to a loop.
pub(super) fn recursion_diagnostic(name: &str) -> String {
    format!(
        "1:1: error [SB5037] recursive call to '{name}' is not supported — \
         sash inlines functions, so only a direct self tail-call (`return {name} …`) \
         is allowed (it lowers to a loop); rewrite other recursion as a loop"
    )
}

/// DD#27/§5: emit a self tail-call as a loop back-edge. Rebinds the current
/// function's `args` array to `new_args` (evaluated in the caller-of-the-tail
/// scope, i.e. reading the current iteration's values) and branches to the
/// loop header. Returns a fresh (dead) block for any trailing lowering. Shared
/// by the two `return`-lowering sites (int `$(( (F …) ))` and producer
/// `return F …`).
pub(super) fn emit_tail_call(
    header: usize,
    args_name: &str,
    new_args: Vec<SsaValue>,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let av = state.define(args_name);
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target: av,
        value: SsaValue::IndexedArray(new_args),
    });
    ssa.block_mut(block).terminator = Terminator::Goto(header);
    ssa.new_block("after_tail_call")
}

/// Inline a -> int function call, returning an SsaValue::Var pointing
/// to the temp variable holding the return value.
pub(super) fn inline_int_func(
    name: &str,
    args: &[SsaValue],
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> SsaValue {
    let body = match state.function_bodies.get(name).cloned() {
        Some(b) => b,
        None => return SsaValue::Expr("0".to_string()),
    };

    // DD#27/§5: a call to a function already being inlined is recursion. Reject
    // (no crash from unbounded inlining) — tail self-calls are handled earlier.
    if state.inlining.iter().any(|n| n == name) {
        state.diagnostics.push(recursion_diagnostic(name));
        return SsaValue::Literal("0".to_string());
    }
    state.inlining.push(name.to_string());

    // DD#75 §5: positionals are backed by the `args` array (below), not scalar
    // "1".."9"/"#" vars — save/restore just `args`.
    let saved_args = save_caller_args(state);
    let saved_ro = enter_fn_body_scope(state);

    // DD#75 §5: materialize the implicit `args` array (0-indexed).
    setup_callee_args(args, ssa, state, *block);

    // Save exit status
    let saved_status = state.lookup("?");

    // Inline function body
    let call_block = ssa.new_block(&format!("intcall_{}", name));
    ssa.block_mut(*block).terminator = Terminator::Goto(call_block);
    let restore_block = ssa.new_block(&format!("intret_{}", name));
    let saved_return_target = state.return_target;
    state.return_target = Some(restore_block);
    // DD#27/§5: a self tail-call in the body loops back to call_block.
    let args_name = state.current_args_name.clone();
    let saved_tail = state.tail_loop.take();
    state.tail_loop = Some((name.to_string(), call_block, args_name));
    let exit_block = lower_stmts(&body, ssa, state, call_block);
    state.tail_loop = saved_tail;
    state.return_target = saved_return_target;
    ssa.block_mut(exit_block).terminator = Terminator::Goto(restore_block);

    // The return value is in "?" (set by `return N`)
    let result_ref = state.lookup("?");
    let temp_name = format!("__intret_{}", name);
    let temp_var = state.define(&temp_name);
    ssa.block_mut(restore_block).insts.push(SsaInst::Assign {
        target: temp_var.clone(),
        value: SsaValue::Var(result_ref),
    });

    restore_caller_args(saved_args, state);
    exit_fn_body_scope(state, saved_ro);

    // Restore exit status
    let status_restore = state.define("?");
    ssa.block_mut(restore_block).insts.push(SsaInst::Assign {
        target: status_restore,
        value: SsaValue::Var(saved_status),
    });

    *block = restore_block;
    state.inlining.pop();
    SsaValue::Var(temp_var)
}

// ── DD#81 Part B: function-call arity from `args` usage ──────────────────

/// A function's inferred call signature (DD#81 Part B).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Arity {
    /// Reads only literal indices (`$N`, `${args[k]}`) — no whole-args access.
    /// The call must pass `min..=max` args. `min < max` when trailing args are
    /// optional (DD#84: read only via `:-`/`:~`); `min == max` when every read
    /// index is required.
    Fixed { min: usize, max: usize },
    /// Uses `@args` / `$(len args)` / `${#args}` / `$#` / `$@` / a dynamic
    /// index. Takes at least `floor` (the highest literal index + 1, if any).
    Variadic { floor: usize },
}

/// A structural defect in a fixed-arity `args` signature (DD#81 rule (a) +
/// DD#84), emitted at the function definition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum ArityDefect {
    /// SB5044 — index `i` in `0..max` is never read (a hole).
    Hole(usize),
    /// SB5047 — index `i` is read both bare (required) and via a disposition
    /// (`:-`/`:~`, optional).
    Mixed(usize),
    /// SB5048 — a required index `i` follows an optional one (DD#84: optional
    /// args must be trailing).
    RequiredAfterOptional(usize),
}

#[derive(Default)]
struct ArgSig {
    /// 0-based `args` indices read *bare* (`$N`, `${args[k]}`, `${args[k]:?}`)
    /// — required.
    required: std::collections::BTreeSet<usize>,
    /// 0-based indices read only via a `:-`/`:~` disposition (DD#84) —
    /// candidate optional.
    optional: std::collections::BTreeSet<usize>,
    /// Whole-args access → variadic.
    variadic: bool,
}

/// Infer a function's arity from how its body reads `args`, plus any structural
/// defects (DD#81 rule (a) contiguity + DD#84 mixing / optional-trailing).
pub(super) fn analyze_arity(body: &[Statement]) -> (Arity, Vec<ArityDefect>) {
    let mut sig = ArgSig::default();
    for s in body {
        sig_scan_stmt(s, &mut sig);
    }
    if sig.variadic {
        let floor = sig
            .required
            .iter()
            .chain(sig.optional.iter())
            .max()
            .map(|m| m + 1)
            .unwrap_or(0);
        return (Arity::Variadic { floor }, vec![]);
    }
    let mut defects = Vec::new();
    // SB5047 — an index read both bare and via a disposition.
    for &i in sig.required.intersection(&sig.optional) {
        defects.push(ArityDefect::Mixed(i));
    }
    let all: std::collections::BTreeSet<usize> =
        sig.required.union(&sig.optional).copied().collect();
    let max = all.iter().max().map(|m| m + 1).unwrap_or(0);
    // SB5044 — contiguity: every index in `0..max` must be read.
    if let Some(hole) = (0..max).find(|i| !all.contains(i)) {
        defects.push(ArityDefect::Hole(hole));
    }
    // DD#84 — optional args are trailing: `min` is the first optional-only
    // index; every required index must be below it. (A mixed index counts as
    // required for the range math — it errors via SB5047 regardless.)
    let opt_only: std::collections::BTreeSet<usize> =
        sig.optional.difference(&sig.required).copied().collect();
    let min = match opt_only.iter().next() {
        Some(&lowest_opt) => {
            if let Some(&bad) = sig.required.iter().find(|&&r| r > lowest_opt) {
                defects.push(ArityDefect::RequiredAfterOptional(bad));
            }
            lowest_opt
        }
        None => max,
    };
    (Arity::Fixed { min, max }, defects)
}

fn sig_scan_stmts(stmts: &[Statement], sig: &mut ArgSig) {
    for s in stmts {
        sig_scan_stmt(s, sig);
    }
}

fn sig_scan_exprs(exprs: &[Expr], sig: &mut ArgSig) {
    for e in exprs {
        sig_scan_expr(e, sig);
    }
}

fn sig_scan_decl(d: &Declaration, sig: &mut ArgSig) {
    for a in &d.assignments {
        sig_scan_expr(&a.value, sig);
    }
}

fn sig_scan_case(c: &Case, sig: &mut ArgSig) {
    sig_scan_expr(&c.value, sig);
    for item in &c.items {
        sig_scan_stmts(&item.body, sig);
    }
}

fn sig_scan_child_bodies(stmt: &Statement, sig: &mut ArgSig) {
    for body in stmt.child_bodies() {
        sig_scan_stmts(body, sig);
    }
}

fn sig_scan_stmt(stmt: &Statement, sig: &mut ArgSig) {
    match &stmt.kind {
        StmtKind::Command(c) => {
            sig_scan_expr(&c.name, sig);
            sig_scan_exprs(&c.args, sig);
        }
        StmtKind::Assignment(a) => sig_scan_expr(&a.value, sig),
        StmtKind::Declaration(d) => sig_scan_decl(d, sig),
        StmtKind::For(f) => {
            sig_scan_exprs(&f.words, sig);
            sig_scan_stmts(&f.body, sig);
        }
        StmtKind::Case(c) => sig_scan_case(c, sig),
        // A nested function has its own `args` scope — don't descend.
        StmtKind::Function(_) => {}
        // `(( … ))` as a statement / if/while condition carries an ArithExpr,
        // not child bodies — scan it directly (else `$#`/`$N` inside a
        // condition are missed and a variadic function looks fixed).
        StmtKind::ArithTest(a) => sig_scan_arith(a, sig),
        _ => sig_scan_child_bodies(stmt, sig),
    }
}

fn sig_scan_expr(expr: &Expr, sig: &mut ArgSig) {
    match &expr.kind {
        ExprKind::SimpleExpansion(name) => sig_record_name(name, sig),
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) | ExprKind::Array(parts) => {
            sig_scan_exprs(parts, sig)
        }
        ExprKind::ParameterExpansion(pe) => sig_scan_pexp(pe, sig),
        ExprKind::Subscript { name, index } => {
            if name == "args" {
                sig_record_index(index, sig, false);
            }
            sig_scan_expr(index, sig);
        }
        ExprKind::CommandSubstitution(stmts) => {
            if cmdsub_reads_whole_args(stmts) {
                sig.variadic = true;
            }
            sig_scan_stmts(stmts, sig);
        }
        ExprKind::Test(t) => sig_scan_test(t, sig),
        ExprKind::Arithmetic(a) => sig_scan_arith(a, sig),
        _ => {}
    }
}

/// `${args[k]:OP}` is an indexed cell; `@args` / `${#args}` (no cell index) is
/// whole-args → variadic. `${N}`/`${#}` behave like `$N`/`$#`.
fn sig_scan_pexp(pe: &ParameterExpansion, sig: &mut ArgSig) {
    if pe.name == "args" {
        match &pe.index {
            // DD#84: `${args[i]:-}` / `${args[i]:~}` reads index i *optionally*;
            // any other disposition (`:?`, substring, …) is a required read.
            Some(idx) => {
                let optional = matches!(pe.operator.as_deref(), Some(":-") | Some(":~"));
                sig_record_index(idx, sig, optional);
            }
            None => sig.variadic = true,
        }
    } else {
        sig_record_name(&pe.name, sig);
    }
    if let Some(op) = &pe.operand {
        sig_scan_expr(op, sig);
    }
}

fn sig_scan_test(test: &TestExpr, sig: &mut ArgSig) {
    match test {
        TestExpr::Unary { operand, .. } => sig_scan_expr(operand, sig),
        TestExpr::Binary { left, right, .. } => {
            sig_scan_expr(left, sig);
            sig_scan_expr(right, sig);
        }
        TestExpr::Not(inner) => sig_scan_test(inner, sig),
        TestExpr::And(a, b) | TestExpr::Or(a, b) => {
            sig_scan_test(a, sig);
            sig_scan_test(b, sig);
        }
    }
}

/// `$N` / `${N}` (1..9) → 0-based index; `$#`/`$@` → whole-args (variadic).
/// A bare `$N` is a *required* read.
fn sig_record_name(name: &str, sig: &mut ArgSig) {
    if let Ok(n) = name.parse::<usize>() {
        if (1..=9).contains(&n) {
            sig.required.insert(n - 1);
        }
    } else if name == "#" || name == "@" {
        sig.variadic = true;
    }
}

/// A `${args[K]}` index: a literal → a fixed index (recorded as optional or
/// required per `optional`); anything dynamic → variadic.
fn sig_record_index(idx: &Expr, sig: &mut ArgSig, optional: bool) {
    match &idx.kind {
        ExprKind::Literal(s) => match s.parse::<usize>() {
            Ok(n) => {
                if optional {
                    sig.optional.insert(n);
                } else {
                    sig.required.insert(n);
                }
            }
            Err(_) => sig.variadic = true,
        },
        _ => sig.variadic = true,
    }
}

/// `$(len args)` / `$(join args)` / `$(keys args)` — a whole-args count/fold.
fn cmdsub_reads_whole_args(stmts: &[Statement]) -> bool {
    stmts.iter().any(|s| match &s.kind {
        StmtKind::Command(c) => {
            matches!(&c.name.kind, ExprKind::Literal(n)
                if matches!(n.as_str(), "len" | "join" | "keys"))
                && c.args
                    .iter()
                    .any(|a| matches!(&a.kind, ExprKind::Literal(x) if x == "args"))
        }
        _ => false,
    })
}

/// Scan an arithmetic expression for `args` reads: `$N`/`$#` (`DollarVar`),
/// bare `args[k]` (`Subscript`), and imported `${args[k]}` (`Expansion`).
fn sig_scan_arith(a: &ArithExpr, sig: &mut ArgSig) {
    match a {
        ArithExpr::Var(name) | ArithExpr::DollarVar(name) => sig_record_name(name, sig),
        ArithExpr::Expansion(e) => sig_scan_expr(e, sig),
        ArithExpr::Subscript { name, index } | ArithExpr::SubscriptAssign { name, index, .. } => {
            if name == "args" {
                sig_record_arith_index(index, sig);
            }
        }
        _ => {}
    }
    for child in arith_children(a) {
        sig_scan_arith(child, sig);
    }
}

/// The sub-expressions of an arith node, for the recursive `args` scan.
fn arith_children(a: &ArithExpr) -> Vec<&ArithExpr> {
    match a {
        ArithExpr::BinOp { left, right, .. } => vec![left, right],
        ArithExpr::UnaryOp { operand, .. } | ArithExpr::Paren(operand) => vec![operand],
        ArithExpr::Assign { value, .. } | ArithExpr::CompoundAssign { value, .. } => vec![value],
        ArithExpr::Ternary {
            cond,
            true_val,
            false_val,
        } => vec![cond, true_val, false_val],
        ArithExpr::Comma(parts) | ArithExpr::FuncCall { args: parts, .. } => parts.iter().collect(),
        ArithExpr::Subscript { index, .. } => vec![index],
        ArithExpr::SubscriptAssign { index, value, .. } => vec![index, value],
        _ => vec![],
    }
}

/// A bare `args[k]` arith index: an int literal → fixed *required* index; else
/// variadic. (Arithmetic reads are always bare — no disposition in `$(( ))`.)
fn sig_record_arith_index(idx: &ArithExpr, sig: &mut ArgSig) {
    match idx {
        ArithExpr::Literal(n) if *n >= 0 => {
            sig.required.insert(*n as usize);
        }
        _ => sig.variadic = true,
    }
}
pub(super) fn collect_returns(stmts: &[Statement]) -> Vec<Option<String>> {
    let mut returns = Vec::new();
    collect_returns_inner(stmts, &mut returns);
    returns
}

fn collect_returns_inner(stmts: &[Statement], returns: &mut Vec<Option<String>>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Command(c) => maybe_collect_return(c, returns),
            // Nested function definitions have their own returns.
            StmtKind::Function(_) => {}
            // Default: descend into every nested statement body via
            // child_bodies(). Covers If (incl. elif_clauses), While,
            // For, Case, Group, Pipeline, List, Redirected, Suppressed.
            // Leaf StmtKinds (Assignment, Declaration, Comment,
            // ArithTest) return empty bodies.
            _ => {
                for body in stmt.child_bodies() {
                    collect_returns_inner(body, returns);
                }
            }
        }
    }
}

/// If `c` is a `return` command, push its target into `returns`:
/// `None` = computed/non-trivial value, `Some(s)` = literal value
/// (empty string means bare `return` → implicit 0).
fn maybe_collect_return(c: &Command, returns: &mut Vec<Option<String>>) {
    if literal_command_name(&c.name) != "return" {
        return;
    }
    let val = if c.args.is_empty() {
        Some(String::new())
    } else if let Some(ExprKind::Literal(s)) = c.args.first().map(|a| &a.kind) {
        Some(s.clone())
    } else {
        None
    };
    returns.push(val);
}

fn literal_command_name(name: &Expr) -> &str {
    match &name.kind {
        ExprKind::Literal(s) => s.as_str(),
        _ => "",
    }
}
