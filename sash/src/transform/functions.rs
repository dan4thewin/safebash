//! Function-related transformations: int-func lifting, inlining, helpers.

use crate::ir::*;
use crate::ssa::*;

use super::expressions::expr_to_value;
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
    let name = state.reserve_temp_name(format!("__args{}", state.args_counter), VarType::String);
    state.args_counter += 1;
    let v = state.define(&name);
    ssa.block_mut(block).insts.push(SsaInst::Assign {
        target: v,
        value: SsaValue::IndexedArray(arg_values.to_vec()),
    });
    state.collections.insert(name.clone());
    state.current_args_name = name;
}

/// DD#100: bind the callee's collection slots `A`..`E` from the call's
/// `=name` arguments (in call order), returning the caller's bindings for
/// restore. Each passed name is resolved through the *caller's* slots first,
/// so `fn g { f =A }` forwards the collection `g` itself received rather than
/// a slot letter that means nothing in `f`.
pub(super) fn setup_callee_slots(
    names: &[String],
    state: &mut VersionState,
) -> std::collections::HashMap<String, String> {
    let bound: Vec<String> = names
        .iter()
        .map(|n| super::expressions::resolve_args_name(n, state))
        .collect();
    let saved = std::mem::take(&mut state.collection_slots);
    for (i, target) in bound.into_iter().enumerate() {
        state
            .collection_slots
            .insert(crate::ir::collection_slot_name(i), target);
    }
    saved
}

pub(super) fn restore_caller_slots(
    saved: std::collections::HashMap<String, String>,
    state: &mut VersionState,
) {
    state.collection_slots = saved;
}

/// DD#100: split a call's arguments into the scalar ones (which become the
/// callee's `args`) and the collection ones (`=name` → `A`..`E`). Part C puts
/// the scalars first, but the split is by form, not position, so a misordered
/// call still lowers coherently while the validator reports it.
pub(super) fn split_call_args(args: &[Expr]) -> (Vec<&Expr>, Vec<String>) {
    let mut scalars = Vec::new();
    let mut colls = Vec::new();
    for a in args {
        match crate::ir::collection_arg_name(a) {
            Some(n) => colls.push(n),
            None => scalars.push(a),
        }
    }
    (scalars, colls)
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
///
/// The same double-lowering hazard applies to the collection sets: an
/// autodeclared bulk-read target (`read -a`/`mapfile`/`argv`) inside a function
/// is registered in `collections`/`assoc_arrays`, which drive
/// `autodeclare_bulk_target`'s dedup. If the template's registration leaked into
/// the inlined call, the inline would skip the table-allocating `Assign` and the
/// runtime read would fill a NULL table (segfault). Snapshot those too so each
/// lowering re-allocates its own table.
///
/// A call's readers use its stdin, not an enclosing loop's lexical pipe binding.
pub(super) struct FnBodyScope {
    readonly_vars: std::collections::HashSet<String>,
    collections: std::collections::HashSet<String>,
    assoc_arrays: std::collections::HashSet<String>,
    pipe_reader: Option<(i64, bool)>,
}

pub(super) fn enter_fn_body_scope(state: &mut VersionState) -> FnBodyScope {
    FnBodyScope {
        readonly_vars: state.readonly_vars.clone(),
        collections: state.collections.clone(),
        assoc_arrays: state.assoc_arrays.clone(),
        pipe_reader: state.pipe_reader.take(),
    }
}

pub(super) fn exit_fn_body_scope(state: &mut VersionState, saved: FnBodyScope) {
    state.readonly_vars = saved.readonly_vars;
    state.collections = saved.collections;
    state.assoc_arrays = saved.assoc_arrays;
    state.pipe_reader = saved.pipe_reader;
}

/// Apply the same capture lifting the assignment and argument paths use to a
/// `[[ ]]` test's value operands. `test_to_ssa` builds an `SsaTest` from a
/// `&VersionState` alone — it has no block to emit a capture into — so the lift
/// runs here, at the one call site that does. Without it a `$(f)` operand
/// reached codegen as a bare `FuncCall`/`IntFuncCall`/`CmdSubstBody` and
/// panicked (values.rs:81/85/90) even though `v=$(f)` had always worked.
/// Returns the block to continue in: a capture may open a new one.
pub(super) fn lift_test_captures(
    test: SsaTest,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> SsaTest {
    match test {
        SsaTest::Unary { op, operand } => SsaTest::Unary {
            op,
            operand: lift_int_func_calls(operand, ssa, state, block),
        },
        SsaTest::Binary { left, op, right } => {
            let left = lift_int_func_calls(left, ssa, state, block);
            let right = match right {
                SsaTestRhs::Value(v) => {
                    SsaTestRhs::Value(lift_int_func_calls(v, ssa, state, block))
                }
                // A `=~` pattern is a comptime string, not a value.
                pattern => pattern,
            };
            SsaTest::Binary { left, op, right }
        }
        SsaTest::Not(inner) => {
            SsaTest::Not(Box::new(lift_test_captures(*inner, ssa, state, block)))
        }
        SsaTest::And(a, b) => SsaTest::And(
            Box::new(lift_test_captures(*a, ssa, state, block)),
            Box::new(lift_test_captures(*b, ssa, state, block)),
        ),
        SsaTest::Or(a, b) => SsaTest::Or(
            Box::new(lift_test_captures(*a, ssa, state, block)),
            Box::new(lift_test_captures(*b, ssa, state, block)),
        ),
    }
}

/// Materialize every nested function call / command-substitution capture in a
/// value into the current block, replacing it with a read of the temp it was
/// captured into.
///
/// **Total over `SsaValue` on purpose — do not add a wildcard.** A `FuncCall`,
/// `IntFuncCall` or `CmdSubstBody` that reaches codegen unlifted is an ICE
/// (`codegen/values.rs`, `emit_c.rs`), so every variant owning a sub-value has
/// to be walked. The `other => other` arm this replaced skipped `IndexedArray`,
/// `AssocArray`, `Subscript`, `DefaultValue` and a dozen more, so
/// `array a=( $(f) )`, `map m=([k]=$(f))`, `${arr[$(g)]}` and `${y:-$(f)}` —
/// every one a valid bash program — crashed the compiler instead of compiling.
pub(super) fn lift_int_func_calls(
    val: SsaValue,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> SsaValue {
    match val {
        // ── the captures this pass exists to materialize ──────────────
        SsaValue::IntFuncCall { name, args, colls } => {
            let lifted = lift_each(args, ssa, state, block);
            inline_int_func(&name, &lifted, &colls, ssa, state, block, false)
        }
        SsaValue::FuncCall { name, args, colls } => {
            let lifted = lift_each(args, ssa, state, block);
            lift_str_func_call(name, lifted, colls, ssa, state, block)
        }
        SsaValue::CmdSubstBody { body, .. } => {
            if let Some(value) = suppressed_capture_value(&body, state) {
                let scope = super::statements::SuppressionScope::enter(ssa, state, *block);
                let value = lift_int_func_calls(value, ssa, state, block);
                scope.finish(ssa, state);
                return value;
            }
            // A multi-statement/pipeline `$(…)` body: capture its stdout into a
            // temp (fresh name from the block count), then use the temp. The
            // capture is tainted (external output).
            let temp =
                state.reserve_temp_name(format!("__bodycap_{}", ssa.blocks.len()), VarType::String);
            *block = super::stmt_handlers::capture_body_into(&temp, &body, state, ssa, *block);
            SsaValue::Var(state.lookup(&temp))
        }

        // Source defaults are literal (SB4070); walk API values too.
        SsaValue::DefaultValue { source, default } => {
            lift_default_value(*source, *default, ssa, state, block)
        }

        // ── collections ───────────────────────────────────────────────
        SsaValue::Concat(parts) => SsaValue::Concat(lift_each(parts, ssa, state, block)),
        SsaValue::IndexedArray(parts) => {
            SsaValue::IndexedArray(lift_each(parts, ssa, state, block))
        }
        SsaValue::CmdSubst { argv, suppressed } => lift_cmdsub(argv, suppressed, ssa, state, block),
        SsaValue::AssocArray(pairs) => SsaValue::AssocArray(
            pairs
                .into_iter()
                .map(|(k, v)| (k, lift_int_func_calls(v, ssa, state, block)))
                .collect(),
        ),

        // ── single-subject wrappers ───────────────────────────────────
        SsaValue::SlurpFile(path) => SsaValue::SlurpFile(lift_boxed(*path, ssa, state, block)),
        SsaValue::CaseConvert { subject, upper } => SsaValue::CaseConvert {
            subject: lift_boxed(*subject, ssa, state, block),
            upper,
        },
        SsaValue::StrLen { subject } => SsaValue::StrLen {
            subject: lift_boxed(*subject, ssa, state, block),
        },
        SsaValue::ElideEmpty { source, taint_off } => SsaValue::ElideEmpty {
            source: lift_boxed(*source, ssa, state, block),
            taint_off,
        },
        SsaValue::Trusted { source } => SsaValue::Trusted {
            source: lift_boxed(*source, ssa, state, block),
        },
        SsaValue::Substring { source, spec } => SsaValue::Substring {
            source: lift_boxed(*source, ssa, state, block),
            spec,
        },
        SsaValue::IntCell(inner) => SsaValue::IntCell(lift_boxed(*inner, ssa, state, block)),
        SsaValue::Subscript {
            array,
            index,
            is_integer_key,
            is_integer_value,
        } => SsaValue::Subscript {
            array,
            index: lift_boxed(*index, ssa, state, block),
            is_integer_key,
            is_integer_value,
        },
        SsaValue::ArraySplat { array, disposition } => SsaValue::ArraySplat {
            array,
            disposition: disposition.map(|d| ArraySplatDisposition {
                word: lift_boxed(*d.word, ssa, state, block),
                has_word: d.has_word,
                abort: d.abort,
                taint_off: d.taint_off,
            }),
        },

        // ── multi-operand values ──────────────────────────────────────
        SsaValue::ArithBinOp { left, op, right } => {
            let left = lift_boxed(*left, ssa, state, block);
            let right = lift_boxed(*right, ssa, state, block);
            SsaValue::ArithBinOp { left, op, right }
        }
        SsaValue::Ternary {
            cond,
            true_val,
            false_val,
        } => {
            let cond = lift_boxed(*cond, ssa, state, block);
            let true_val = lift_boxed(*true_val, ssa, state, block);
            let false_val = lift_boxed(*false_val, ssa, state, block);
            SsaValue::Ternary {
                cond,
                true_val,
                false_val,
            }
        }
        SsaValue::RegexMatch { subject, pattern } => {
            let subject = lift_boxed(*subject, ssa, state, block);
            let pattern = lift_boxed(*pattern, ssa, state, block);
            SsaValue::RegexMatch { subject, pattern }
        }
        SsaValue::RegexReplace {
            subject,
            pattern,
            replacement,
            global,
        } => {
            let subject = lift_boxed(*subject, ssa, state, block);
            let pattern = lift_boxed(*pattern, ssa, state, block);
            let replacement = lift_boxed(*replacement, ssa, state, block);
            SsaValue::RegexReplace {
                subject,
                pattern,
                replacement,
                global,
            }
        }
        SsaValue::RequireValue { source, message } => {
            let source = lift_boxed(*source, ssa, state, block);
            let message = lift_boxed(*message, ssa, state, block);
            SsaValue::RequireValue { source, message }
        }

        // ── leaves: nothing below them to walk ────────────────────────
        v @ (SsaValue::Literal(_)
        | SsaValue::Var(_)
        | SsaValue::Expr(_)
        | SsaValue::ArithExpr { .. }
        | SsaValue::ParamExpansion { .. }
        | SsaValue::ArgvIndex(_)
        | SsaValue::TableValue { .. }
        | SsaValue::TableKey { .. }) => v,
    }
}

/// Preserve a sole command's value while its prefix scopes materialization,
/// including nested argument captures. Redirects stay on ordinary body capture.
pub(super) fn suppressed_capture_value(
    body: &[Statement],
    state: &VersionState,
) -> Option<SsaValue> {
    let [stmt] = body else {
        return None;
    };
    if !matches!(stmt.kind, StmtKind::Suppressed(_)) {
        return None;
    }
    let c = crate::ir::single_value_command(body)?;
    Some(expr_to_value(
        &Expr {
            kind: ExprKind::CommandSubstitution(vec![Statement::command(
                c.name.clone(),
                c.args.clone(),
                stmt.span,
            )]),
            span: stmt.span,
        },
        state,
    ))
}

fn lift_cmdsub(
    argv: Vec<SsaValue>,
    suppressed: bool,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> SsaValue {
    let argv = lift_each(argv, ssa, state, block);
    let Some(SsaValue::Literal(name)) = argv
        .first()
        .filter(|head| matches!(head, SsaValue::Literal(name) if name.starts_with('/')))
    else {
        return SsaValue::CmdSubst { argv, suppressed };
    };
    let name = name.clone();
    let temp = state.reserve_temp_name(
        format!("__extcap_{}", ssa.blocks.len()),
        crate::ir::VarType::String,
    );
    let target = state.define(&temp);
    ssa.block_mut(*block).insts.push(SsaInst::Pipeline {
        stages: vec![(name, argv.into_iter().skip(1).collect(), vec![])],
        capture: PipelineCapture::Value(target.clone()),
        checked: !suppressed,
        suppressed: vec![],
        negated: vec![],
    });
    SsaValue::Var(target)
}

fn lift_each(
    vals: Vec<SsaValue>,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> Vec<SsaValue> {
    vals.into_iter()
        .map(|v| lift_int_func_calls(v, ssa, state, block))
        .collect()
}

fn lift_boxed(
    val: SsaValue,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> Box<SsaValue> {
    Box::new(lift_int_func_calls(val, ssa, state, block))
}

/// Option A (string-func-capture-in-arg-ice): a non-int function's `$(f …)`
/// stdout capture in argument position. Reuse the assignment path's
/// capture-inline machinery (CaptureStart/End) to inline the body and capture
/// its stdout into a temp, then use the temp — so it works for runtime output
/// too, not just const-foldable stdout. DD#81: the call's args are carried on
/// `FuncCall` and threaded into the capture so the callee's `args` array is set
/// up (was dropped).
fn lift_str_func_call(
    name: String,
    args: Vec<SsaValue>,
    colls: Vec<String>,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> SsaValue {
    let temp = state.reserve_temp_name(format!("__strcap_{name}"), VarType::String);
    *block =
        super::stmt_handlers::capture_func_into(&temp, &name, &args, &colls, state, ssa, *block);
    SsaValue::Var(state.lookup(&temp))
}

/// `${x:-WORD}` / `@{a:-WORD}`: lower to the branchless `DefaultValue` select,
/// which evaluates both arms and picks one.
///
/// Evaluating an untaken arm is only sound because WORD is a **literal**
/// (SB4070, DG 2026-09-30) — it cannot read, call, fork, or advance the PRNG,
/// so "was it taken" is unobservable.
///
/// This function used to be twice this size. When WORD was a full
/// interpolating word it could hold a capture, which must not run on the
/// untaken path, so a capture-bearing default was lowered as real control flow
/// — bind the source to a temp, test it, re-bind from the lifted WORD in the
/// taken block, merge through a phi — while a captureless one kept the cheap
/// select. Two lowerings for one operator, chosen by a predicate on the
/// operand, with the seam invisible in the source. DG dissolved it by making
/// the operand literal: "let's change it to match the `:?` so
/// eager/side-effects are a non-issue."
fn lift_default_value(
    source: SsaValue,
    default: SsaValue,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> SsaValue {
    let source = lift_int_func_calls(source, ssa, state, block);
    let default = lift_int_func_calls(default, ssa, state, block);
    SsaValue::DefaultValue {
        source: Box::new(source),
        default: Box::new(default),
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
    mut block: BlockId,
) -> BlockId {
    let new_args = lift_int_func_args(new_args, ssa, state, &mut block);
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
    colls: &[String],
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
    suppressed_tail_entry: bool,
) -> SsaValue {
    let Some(body) = defined_int_function_body(name, state) else {
        return SsaValue::Expr("0".to_string());
    };

    // DD#27/§5: a call to a function already being inlined is recursion. Reject
    // A checked self-tail loop may enter one suppressed specialization. Its
    // own tail calls are back edges, so this never permits unbounded inlining.
    if !suppressed_tail_entry && state.inlining.iter().any(|n| n == name) {
        state.diagnostics.push(recursion_diagnostic(name));
        return SsaValue::Literal("0".to_string());
    }
    state.inlining.push(name.to_string());

    // DD#75 §5: positionals are backed by the `args` array (below), not scalar
    // "1".."9"/"#" vars — save/restore just `args`.
    let saved_args = save_caller_args(state);
    let saved_slots = setup_callee_slots(colls, state);
    let saved_ro = enter_fn_body_scope(state);

    // DD#75 §5: materialize the implicit `args` array (0-indexed).
    setup_callee_args(args, ssa, state, *block);

    // Save exit status
    let saved_status = state.lookup("?");

    // Inline function body
    let call_block = ssa.new_checkpoint_block(&format!("intcall_{}", name));
    ssa.block_mut(*block).terminator = Terminator::Goto(call_block);
    let restore_block = ssa.new_checkpoint_block(&format!("intret_{}", name));
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
    let temp_name = state.reserve_temp_name(format!("__intret_{}", name), VarType::Int);
    let temp_var = state.define(&temp_name);
    ssa.block_mut(restore_block).insts.push(SsaInst::Assign {
        target: temp_var.clone(),
        value: SsaValue::Var(result_ref),
    });

    restore_caller_args(saved_args, state);
    restore_caller_slots(saved_slots, state);
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

/// DD#81: record a call before its definition so that the later definition
/// reports SB5041. Whole-program return typing alone does not make it defined.
fn defined_int_function_body(name: &str, state: &VersionState) -> Option<Vec<Statement>> {
    let body = state.function_bodies.get(name).cloned();
    if body.is_none() && !state.lowering_fn_template {
        state.externally_used.borrow_mut().insert(name.to_string());
    }
    body
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
    /// Uses whole `args` or a dynamic index. Takes at least `floor` (the
    /// highest required literal index + 1, if any); counting alone does not.
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
        return (
            Arity::Variadic {
                floor: variadic_floor(&sig),
            },
            vec![],
        );
    }
    fixed_arity(&sig)
}

/// DD#84: only *required* indices set a variadic function's floor.
///
/// An index read solely through a `:-`/`:~` disposition is optional, and stays
/// optional when the function is also variadic. Chaining `optional` in here
/// made `: @args` silently promote it back to required, so DD#81's documented
/// way to accept a variable number rejected a call that the same body without
/// `: @args` accepted.
fn variadic_floor(sig: &ArgSig) -> usize {
    sig.required.iter().max().map(|m| m + 1).unwrap_or(0)
}

/// The fixed-arity half: the `[min, max]` range plus DD#81 rule (a)
/// contiguity and DD#84 mixing defects.
fn fixed_arity(sig: &ArgSig) -> (Arity, Vec<ArityDefect>) {
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
    let min = fixed_min(sig, max, &mut defects);
    (Arity::Fixed { min, max }, defects)
}

/// DD#84 — optional args are trailing: `min` is the first optional-only index,
/// and every required index must be below it. (A mixed index counts as
/// required for the range math — it errors via SB5047 regardless.)
fn fixed_min(sig: &ArgSig, max: usize, defects: &mut Vec<ArityDefect>) -> usize {
    let opt_only: std::collections::BTreeSet<usize> =
        sig.optional.difference(&sig.required).copied().collect();
    let Some(&lowest_opt) = opt_only.iter().next() else {
        return max;
    };
    if let Some(&bad) = sig.required.iter().find(|&&r| r > lowest_opt) {
        defects.push(ArityDefect::RequiredAfterOptional(bad));
    }
    lowest_opt
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
        if let Some(ref v) = a.value {
            sig_scan_expr(v, sig);
        }
    }
}

fn sig_scan_case(c: &Case, sig: &mut ArgSig) {
    sig_scan_expr(&c.value, sig);
    for item in &c.items {
        sig_scan_stmts(&item.body, sig);
    }
}

fn sig_scan_child_bodies(stmt: &Statement, sig: &mut ArgSig) {
    for body in stmt.structural_bodies() {
        sig_scan_stmts(body, sig);
    }
}

/// **Total over `StmtKind` on purpose — do not add a wildcard.**
fn sig_scan_stmt(stmt: &Statement, sig: &mut ArgSig) {
    match &stmt.kind {
        StmtKind::Command(c) => sig_scan_command(c, sig),
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
        StmtKind::Pipeline(_)
        | StmtKind::If(_)
        | StmtKind::While(_)
        | StmtKind::Redirected(_)
        | StmtKind::List(_)
        | StmtKind::Group(_)
        | StmtKind::Suppressed(_)
        | StmtKind::Negated(_)
        | StmtKind::ReturnProducer(_) => sig_scan_child_bodies(stmt, sig),
    }
}

fn sig_scan_command(c: &Command, sig: &mut ArgSig) {
    sig_scan_expr(&c.name, sig);
    // DD#81: a bare @args into an external requires at least one argument;
    // builtins and for-in handle an empty array themselves.
    if is_external_head(&c.name) && c.args.iter().any(is_bare_args_splat) {
        sig.required.insert(0);
    }
    sig_scan_exprs(&c.args, sig);
}

/// An external command head: post-DD#23 resolution it is an absolute path
/// (`/usr/bin/tool`). Builtins, intrinsics, and user functions keep bare names.
fn is_external_head(name: &Expr) -> bool {
    matches!(&name.kind, ExprKind::Literal(s) if s.starts_with('/'))
}

/// A bare `@args` splat (operator `arrvals`, name `args`) — the whole-args
/// splat, unquoted or as a single-part `"${args[@]}"`.
fn is_bare_args_splat(arg: &Expr) -> bool {
    match &arg.kind {
        ExprKind::ParameterExpansion(pe) => {
            pe.operator.as_deref() == Some("arrvals") && pe.name == "args"
        }
        ExprKind::DoubleQuoted(parts) if parts.len() == 1 => is_bare_args_splat(&parts[0]),
        _ => false,
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

/// `${args[k]:OP}` is an indexed cell; `@args` (no cell index) is whole-args
/// → variadic. `${N}` behaves like `$N` — and a positional `${N:-}`/`${N:~}`
/// reads index N-1 *optionally* (DD#84), exactly like `${args[N-1]:-}`.
/// (`${#args}`/`${#}` are retired surfaces — SB4038/SB4034.)
fn sig_scan_pexp(pe: &ParameterExpansion, sig: &mut ArgSig) {
    sig_record_pexp_subject(pe, sig);
    if let Some(op) = &pe.operand {
        sig_scan_expr(op, sig);
    }
}

/// Record the subject of a `${…}` expansion into the arg signature: an
/// `args[i]` cell, a positional `${N}`, whole-args (`${args[@]}` → variadic),
/// or an ordinary name (not an arg → ignored).
fn sig_record_pexp_subject(pe: &ParameterExpansion, sig: &mut ArgSig) {
    // DD#84: `:-` / `:~` make the read optional; any other disposition — or
    // none — is a required read.
    let optional = matches!(pe.operator.as_deref(), Some(":-") | Some(":~"));
    if pe.name == "args" {
        match &pe.index {
            Some(idx) => sig_record_index(idx, sig, optional),
            None => sig.variadic = true,
        }
    } else if let Some(n) = positional_index_1_9(&pe.name) {
        sig_record_positional(n, optional, sig);
    } else {
        sig_record_name(&pe.name, sig);
    }
}

/// A positional name `"1".."9"` → its 1-based value; any other name → `None`.
fn positional_index_1_9(name: &str) -> Option<usize> {
    name.parse::<usize>().ok().filter(|n| (1..=9).contains(n))
}

/// Record a positional `$N` (1..=9 → 0-based index N-1) as optional or required.
fn sig_record_positional(n: usize, optional: bool, sig: &mut ArgSig) {
    if optional {
        sig.optional.insert(n - 1);
    } else {
        sig.required.insert(n - 1);
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

/// `$N` / `${N}` (1..9) → 0-based index. `$@` is rejected by SB5045.
/// A bare `$N` is a *required* read.
///
/// `$#` is deliberately **not** here (DD#81 addendum 2026-09-28). It observes
/// the argument *count* without accessing an argument, so it says nothing
/// about arity — see `cmdsub_reads_whole_args`.
fn sig_record_name(name: &str, sig: &mut ArgSig) {
    if let Some(n) = positional_index_1_9(name) {
        sig_record_positional(n, false, sig);
    }
}

/// A `${args[K]}` index: a literal → a fixed index (recorded as optional or
/// required per `optional`); anything dynamic → variadic.
fn sig_record_index(idx: &Expr, sig: &mut ArgSig, optional: bool) {
    let ExprKind::Literal(s) = &idx.kind else {
        sig.variadic = true;
        return;
    };
    let n = s.parse::<usize>().unwrap_or_else(|_| {
        unreachable!("a malformed literal args index must not erase the function's arity ceiling")
    });
    if optional {
        sig.optional.insert(n);
    } else {
        sig.required.insert(n);
    }
}

/// `$(join args)` / `$(keys args)` — a whole-args *fold*: both read every
/// element, so the callee can use any number and is variadic.
///
/// `$(len args)` is deliberately **absent** (DD#81 addendum 2026-09-28, DG):
/// it reads the argument *count*, not any argument. In a real variadic
/// function it is only the loop bound — the access inside the loop
/// (`${args[$i]}`, a dynamic index) is what marks the function variadic.
/// Without such an access the count is vacuous, so it must not silently
/// erase the arity ceiling. `$#` is the same rule, in `sig_record_name`.
/// A function that genuinely wants its own count declares it with the DD#81
/// no-op: `fn n { : @args; echo $# }`.
fn cmdsub_reads_whole_args(stmts: &[Statement]) -> bool {
    stmts.iter().any(|s| match &s.kind {
        StmtKind::Command(c) => {
            matches!(&c.name.kind, ExprKind::Literal(n)
                if matches!(n.as_str(), "join" | "keys"))
                && c.args
                    .iter()
                    .any(|a| matches!(&a.kind, ExprKind::Literal(x) if x == "args"))
        }
        _ => false,
    })
}

/// Scan arithmetic positional reads and imported, disposition-bearing cells.
/// Bare `args[k]` is a string-cell type error (DD#77), not another signature.
/// Total over `ArithExpr`; do not add a wildcard.
fn sig_scan_arith(a: &ArithExpr, sig: &mut ArgSig) {
    match a {
        ArithExpr::Var(name) | ArithExpr::DollarVar(name) => sig_record_name(name, sig),
        ArithExpr::Expansion(e) => sig_scan_expr(e, sig),
        ArithExpr::Literal(_)
        | ArithExpr::Assign { .. }
        | ArithExpr::CompoundAssign { .. }
        | ArithExpr::PostIncDec { .. }
        | ArithExpr::PreIncDec { .. }
        | ArithExpr::Subscript { .. }
        | ArithExpr::SubscriptAssign { .. }
        | ArithExpr::BinOp { .. }
        | ArithExpr::UnaryOp { .. }
        | ArithExpr::Ternary { .. }
        | ArithExpr::Comma(_)
        | ArithExpr::FuncCall { .. }
        | ArithExpr::Paren(_) => {}
    }
    for child in a.children() {
        sig_scan_arith(child, sig);
    }
}

pub(super) fn collect_returns(stmts: &[Statement]) -> Vec<Option<String>> {
    let mut returns = Vec::new();
    collect_returns_inner(stmts, &mut returns);
    returns
}

/// **Total over `StmtKind` on purpose — do not add a wildcard.**
fn collect_returns_inner(stmts: &[Statement], returns: &mut Vec<Option<String>>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Command(c) => maybe_collect_return(c, returns),
            // Nested function definitions have their own returns.
            StmtKind::Function(_) => {}
            StmtKind::Assignment(_)
            | StmtKind::Declaration(_)
            | StmtKind::Pipeline(_)
            | StmtKind::If(_)
            | StmtKind::While(_)
            | StmtKind::For(_)
            | StmtKind::Case(_)
            | StmtKind::Redirected(_)
            | StmtKind::List(_)
            | StmtKind::Group(_)
            | StmtKind::Suppressed(_)
            | StmtKind::Negated(_)
            | StmtKind::ArithTest(_)
            | StmtKind::ReturnProducer(_) => collect_returns_bodies(stmt, returns),
        }
    }
}

/// The nested-body half of [`collect_returns_inner`]. `structural_bodies`, not
/// `child_bodies`: a `$( … )` body is an anonymous function body (DD#86), so a
/// `return` inside it belongs to that body, not to the enclosing function.
fn collect_returns_bodies(stmt: &Statement, returns: &mut Vec<Option<String>>) {
    for body in stmt.structural_bodies() {
        collect_returns_inner(body, returns);
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

#[cfg(test)]
mod invariant_tests {
    use super::*;
    use std::collections::HashMap;

    fn lit(s: &str) -> Expr {
        Expr::literal(s, Span::new(1, 1))
    }

    #[test]
    fn nested_function_arguments_and_returns_belong_to_the_nested_scope() {
        let span = Span::new(1, 1);
        let nested = Statement {
            kind: StmtKind::Function(Function {
                name: "inner".to_string(),
                return_type: VarType::String,
                body: vec![
                    Statement::command(
                        lit("echo"),
                        vec![Expr {
                            kind: ExprKind::SimpleExpansion("9".to_string()),
                            span,
                        }],
                        span,
                    ),
                    Statement::command(lit("return"), vec![lit("7")], span),
                ],
            }),
            span,
        };
        let body = vec![
            nested,
            Statement::command(
                lit(":"),
                vec![Expr {
                    kind: ExprKind::SimpleExpansion("1".to_string()),
                    span,
                }],
                span,
            ),
            Statement::command(lit("return"), vec![lit("0")], span),
        ];
        assert_eq!(
            analyze_arity(&body),
            (Arity::Fixed { min: 1, max: 1 }, vec![])
        );
        assert_eq!(collect_returns(&body), vec![Some("0".to_string())]);
    }

    #[test]
    fn nonliteral_command_head_is_not_a_literal_return() {
        let span = Span::new(1, 1);
        let command = Command {
            name: Expr {
                kind: ExprKind::SimpleExpansion("return_name".to_string()),
                span,
            },
            args: vec![lit("7")],
        };
        let mut returns = Vec::new();
        maybe_collect_return(&command, &mut returns);
        assert!(returns.is_empty());
    }

    #[test]
    #[should_panic(expected = "string capture requires a registered function body: missing")]
    fn unregistered_function_value_cannot_remain_unlifted() {
        let mut ssa = SsaProgram::new();
        let mut block = ssa.entry;
        let mut state = VersionState::with_types(HashMap::new());
        lift_int_func_calls(
            SsaValue::FuncCall {
                name: "missing".to_string(),
                args: vec![],
                colls: vec![],
            },
            &mut ssa,
            &mut state,
            &mut block,
        );
    }

    #[test]
    #[should_panic(expected = "a malformed literal args index must not erase")]
    fn malformed_literal_index_cannot_make_a_signature_variadic() {
        sig_record_index(&lit("not_an_index"), &mut ArgSig::default(), false);
    }
}
