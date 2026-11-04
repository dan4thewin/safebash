//! Function-related transformations: int-func lifting, inlining, helpers.

use std::collections::{HashMap, HashSet};

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
    pipe_reader: Option<i64>,
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
///
/// A `-` on the test discards the test's own status, not its operands'
/// (DD#55 2026-10-30), so they are lifted with suppression cleared:
/// `- [[ $(cmd) == x ]]` aborts when `cmd` fails.
pub(super) fn lift_test_captures(
    test: SsaTest,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> SsaTest {
    unsuppressed(state, |state| lift_test_operands(test, ssa, state, block))
}

fn lift_test_operands(
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
            SsaTest::Not(Box::new(lift_test_operands(*inner, ssa, state, block)))
        }
        SsaTest::And(a, b) => SsaTest::And(
            Box::new(lift_test_operands(*a, ssa, state, block)),
            Box::new(lift_test_operands(*b, ssa, state, block)),
        ),
        SsaTest::Or(a, b) => SsaTest::Or(
            Box::new(lift_test_operands(*a, ssa, state, block)),
            Box::new(lift_test_operands(*b, ssa, state, block)),
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
            let lifted = lift_int_func_args(args, ssa, state, block);
            inline_int_func(&name, &lifted, &colls, ssa, state, block)
        }
        SsaValue::FuncCall { name, args, colls } => {
            let lifted = lift_int_func_args(args, ssa, state, block);
            lift_str_func_call(name, lifted, colls, ssa, state, block)
        }
        SsaValue::CmdSubstBody { body, .. } => {
            if let Some(value) = suppressed_capture_value(&body, state) {
                return lift_suppressed_capture(value, ssa, state, block);
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

/// Preserve a sole command's value while its prefix scopes that command's
/// own status (see [`lift_suppressed_capture`]); nested argument captures
/// stay checked. Redirects stay on ordinary body capture.
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
    // The rebuilt capture drops the `-`, so it must not be checked again.
    Some(super::expr_handlers::capture_value(
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

/// `$(- c)` (DD#55): the `-` discards `c`'s own status and nothing `c`
/// evaluates on the way (DD#55 2026-10-30). An external capture is lifted
/// unchecked, its arguments checked. A body capture — a statement builtin,
/// or a capture whose arguments run value calls — is lowered with the
/// counter raised, which its operand lifts clear again. A value intrinsic or
/// function capture leaves the capture no status to check, so it is lifted
/// with suppression cleared.
fn lift_suppressed_capture(
    value: SsaValue,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> SsaValue {
    match value {
        SsaValue::CmdSubst { argv, .. } => lift_cmdsub(argv, true, ssa, state, block),
        body @ SsaValue::CmdSubstBody { .. } => {
            state.suppressed += 1;
            let lifted = lift_int_func_calls(body, ssa, state, block);
            state.suppressed -= 1;
            lifted
        }
        other => unsuppressed(state, |state| lift_int_func_calls(other, ssa, state, block)),
    }
}

fn lift_cmdsub(
    argv: Vec<SsaValue>,
    suppressed: bool,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> SsaValue {
    let argv = lift_int_func_args(argv, ssa, state, block);
    let Some(name) = external_capture_head(&argv).map(str::to_string) else {
        return SsaValue::CmdSubst { argv, suppressed };
    };
    let temp = state.reserve_temp_name(
        format!("__extcap_{}", ssa.blocks.len()),
        crate::ir::VarType::String,
    );
    let target = state.define(&temp);
    ssa.block_mut(*block).insts.push(SsaInst::Pipeline {
        stages: vec![(name, argv.into_iter().skip(1).collect(), vec![])],
        capture: PipelineCapture::Value(target.clone()),
        checked: !suppressed && state.suppressed == 0,
        suppressed: vec![],
        negated: vec![],
    });
    SsaValue::Var(target)
}

/// The command an external `$( … )` capture runs: its head, once DD#23 has
/// resolved it to an absolute path. Any other head is an intrinsic, which
/// codegen evaluates in place. Lifting never turns a non-literal head into a
/// literal, so this answers the same before and after the argv is lifted.
fn external_capture_head(argv: &[SsaValue]) -> Option<&str> {
    match argv.first() {
        Some(SsaValue::Literal(name)) if name.starts_with('/') => Some(name),
        _ => None,
    }
}

/// Whether [`lift_int_func_calls`] would materialize anything in `val` — a
/// function call or a capture that codegen cannot evaluate in place. A plain
/// pipeline stage evaluates its arguments where it forks, with no block of its
/// own to lift into, so a stage whose arguments need lifting runs as a forked
/// child instead (`command_pipeline_stage`).
///
/// **Total over `SsaValue` on purpose — do not add a wildcard.** It must
/// answer exactly as [`lift_int_func_calls`] acts: a capture missed here
/// reaches codegen unlifted, which is an ICE. `$(- strlen x)` answers true,
/// because lifting rewrites its body to the intrinsic; the stage then forks a
/// child it could have done without, which costs a fork and nothing more.
pub(super) fn needs_lift(val: &SsaValue) -> bool {
    match val {
        SsaValue::IntFuncCall { .. }
        | SsaValue::FuncCall { .. }
        | SsaValue::CmdSubstBody { .. } => true,
        SsaValue::CmdSubst { argv, .. } => {
            external_capture_head(argv).is_some() || argv.iter().any(needs_lift)
        }
        SsaValue::DefaultValue { .. }
        | SsaValue::Concat(_)
        | SsaValue::IndexedArray(_)
        | SsaValue::AssocArray(_)
        | SsaValue::SlurpFile(_)
        | SsaValue::CaseConvert { .. }
        | SsaValue::StrLen { .. }
        | SsaValue::ElideEmpty { .. }
        | SsaValue::Trusted { .. }
        | SsaValue::Substring { .. }
        | SsaValue::IntCell(_)
        | SsaValue::Subscript { .. }
        | SsaValue::ArraySplat { .. }
        | SsaValue::ArithBinOp { .. }
        | SsaValue::Ternary { .. }
        | SsaValue::RegexMatch { .. }
        | SsaValue::RegexReplace { .. }
        | SsaValue::RequireValue { .. }
        | SsaValue::Literal(_)
        | SsaValue::Var(_)
        | SsaValue::Expr(_)
        | SsaValue::ArithExpr { .. }
        | SsaValue::ParamExpansion { .. }
        | SsaValue::ArgvIndex(_)
        | SsaValue::TableValue { .. }
        | SsaValue::TableKey { .. } => val.child_values().into_iter().any(needs_lift),
    }
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
///
/// The operands are lifted with suppression cleared (DD#55 2026-10-30): a
/// `-` on the command discards the command's own status, so `- cmd $(g)`
/// still aborts when `g` fails.
pub(super) fn lift_int_func_args(
    args: Vec<SsaValue>,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: &mut BlockId,
) -> Vec<SsaValue> {
    unsuppressed(state, |state| lift_each(args, ssa, state, block))
}

/// Run `lower` with the `-` counter cleared, then restore it. `-` discards
/// one operation's own status (DD#55); what the operation evaluates on the
/// way is checked as it is anywhere else (DD#55 2026-10-30).
pub(super) fn unsuppressed<T>(
    state: &mut VersionState,
    lower: impl FnOnce(&mut VersionState) -> T,
) -> T {
    let saved = std::mem::take(&mut state.suppressed);
    let lowered = lower(state);
    state.suppressed = saved;
    lowered
}

/// DD#27/§5: diagnostic for a rejected recursive call. Functions are inlined
/// (no runtime call stack), so a call cycle can't be inlined — it is rejected,
/// except a direct self tail-call (`return F …`), which lowers to a loop.
pub(super) fn recursion_diagnostic(name: &str) -> String {
    format!(
        "[SB5037] recursive call to '{name}' is not supported — \
         sash inlines functions, so only a direct self tail-call (`return {name} …`) \
         is allowed (it lowers to a loop); rewrite other recursion as a loop"
    )
}

/// DD#27/§5: a call to a function already being inlined is recursion, and
/// unbounded inlining would overflow the compiler. Records SB5037 and answers
/// `true`, so the caller declines the inline. Shared by every route that
/// inlines a body — plain call, `-> int` call and `$( )` capture; the capture
/// route once had no check at all, so `$(f)` inside `f` inlined itself until
/// the compiler's own stack overflowed.
pub(super) fn reject_recursive_call(name: &str, state: &mut VersionState) -> bool {
    let recursive = state.inlining.iter().any(|n| n == name);
    if recursive {
        state.error_at_stmt(recursion_diagnostic(name));
    }
    recursive
}

/// Lower an inlined body of function `name` from `call_block`: `return` and
/// fall-through both reach `restore_block`, and a direct self tail-call
/// (`return name …`) loops back to `call_block` (DD#27/§5). Returns the body's
/// exit block. Shared by the plain call, the `-> int` call and the `$( )`
/// capture of a function, so each route gets the same body context.
///
/// The tail loop is always the callee's own ([`VersionState::enter_tail_loop`]).
///
/// So is the condition context: DD#55 recognises condition position
/// syntactically, and a call in a condition consumes the *call's* status, not
/// the statuses inside its body. Inherited, it unchecked every pipeline, `&&`
/// tail and capture-shaped assignment the body ran, so `if h` swallowed a
/// failure that `h` alone aborts on. The same holds for `-` (DD#55
/// 2026-10-30): `- h` discards only the value `h` returns, so the body runs checked.
pub(super) fn lower_inlined_body(
    name: &str,
    body: &[Statement],
    call_block: BlockId,
    restore_block: BlockId,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
) -> BlockId {
    let saved_return_target = state.return_target.replace(restore_block);
    let saved_tail = state.enter_tail_loop(name, call_block);
    let saved_condition = std::mem::take(&mut state.in_condition);
    let saved_suppressed = std::mem::take(&mut state.suppressed);
    let failed_attests = std::mem::take(&mut state.failed_attests);
    let exit_block = lower_body(body, ssa, state, call_block);
    state.failed_attests = failed_attests;
    state.suppressed = saved_suppressed;
    state.in_condition = saved_condition;
    state.tail_loop = saved_tail;
    state.return_target = saved_return_target;
    ssa.block_mut(exit_block).terminator = Terminator::Goto(restore_block);
    exit_block
}

pub(super) fn lower_body(
    body: &[Statement],
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let inherited = initialize_body_locals(body, ssa, state, block);
    let exit = lower_stmts(body, ssa, state, block);
    state.live_locals = inherited;
    exit
}

fn initialize_body_locals(
    body: &[Statement],
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> std::collections::HashSet<String> {
    let mut locals = std::collections::HashSet::new();
    crate::validate::collect_scope_names(body, &mut locals);
    locals.retain(|name| {
        crate::bindings::local_name(name)
            && !ssa.globals.contains_key(name)
            && !state.live_locals.contains(name)
    });
    let inherited = state.live_locals.clone();
    let mut locals: Vec<_> = locals.into_iter().collect();
    locals.sort();
    for name in locals {
        state.live_locals.insert(name.clone());
        let value = local_initial_value(&name, state);
        let target = state.define_initial(&name);
        ssa.block_mut(block)
            .insts
            .push(SsaInst::Assign { target, value });
    }
    inherited
}

fn local_initial_value(name: &str, state: &mut VersionState) -> SsaValue {
    match state.checked_collections.get(name) {
        Some(true) => {
            state.collections.insert(name.to_string());
            state.assoc_arrays.insert(name.to_string());
            SsaValue::AssocArray(Vec::new())
        }
        Some(false) => {
            state.collections.insert(name.to_string());
            SsaValue::IndexedArray(Vec::new())
        }
        None => match state
            .type_map
            .get(name)
            .expect("every source local has a checked type")
        {
            VarType::Int => SsaValue::Literal("0".to_string()),
            VarType::String => SsaValue::Literal(String::new()),
        },
    }
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
) -> SsaValue {
    let Some(body) = defined_int_function_body(name, state) else {
        return SsaValue::Expr("0".to_string());
    };

    // DD#27/§5: recursion is rejected; a self-tail call is a back edge.
    if reject_recursive_call(name, state) {
        return SsaValue::Literal("0".to_string());
    }
    // DD#81 Part B checks arity at *every* call site; an `-> int` call is one.
    super::cmd_handlers::check_callee_arity(name, &body, args, state);
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
    lower_inlined_body(name, &body, call_block, restore_block, ssa, state);

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
///
/// The traversal is the shared one: [`crate::ir::walk_scoped_stmts`] reaches
/// every nested body, including each `$( … )` body, and
/// [`crate::ir::walk_own_exprs`] hands over each statement's own expression
/// trees once. A private copy of that recursion ended in `_ => {}` and never
/// saw a positional read in a redirect target, a map literal, a dispatch
/// pattern, an assignment or subscript index, or `$(<$1)`, so a function
/// reading `$1` only there was reported as taking no arguments.
pub(super) fn analyze_arity(body: &[Statement]) -> (Arity, Vec<ArityDefect>) {
    let mut sig = ArgSig::default();
    crate::ir::walk_scoped_stmts(body, "", &mut |stmt, in_fn| {
        // A nested function has its own `args` scope.
        if in_fn.is_empty() {
            sig_scan_own(stmt, &mut sig);
        }
    });
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

/// One statement's own contribution to the signature: its expressions, the
/// external `@args` rule, and a `(( … ))` statement's bare positional reads.
fn sig_scan_own(stmt: &Statement, sig: &mut ArgSig) {
    crate::ir::walk_own_exprs(stmt, &mut |e| sig_record_expr(e, sig));
    // DD#81: a bare @args into an external requires at least one argument;
    // builtins and for-in handle an empty array themselves.
    if splats_args_into_external(stmt) {
        sig.required.insert(0);
    }
    // A `(( … ))` statement or condition: its `${…}` expansions came through
    // `walk_own_exprs`; its bare `$N` reads are `ArithExpr` nodes.
    if let StmtKind::ArithTest(a) = &stmt.kind {
        sig_record_arith_names(a, sig);
    }
}

/// An external command handed a bare `@args` splat (DD#81).
fn splats_args_into_external(stmt: &Statement) -> bool {
    matches!(&stmt.kind, StmtKind::Command(c)
        if is_external_head(&c.name) && c.args.iter().any(is_bare_args_splat))
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

/// Record one expression node's `args` reads. The walk hands over every node
/// of every expression tree once, so this looks at the node alone and never
/// recurses: a container's parts, a `${…}`'s index and operand, and the
/// `${…}` inside arithmetic all arrive as nodes of their own.
///
/// **Total over `ExprKind` on purpose — do not add a wildcard.** The private
/// scanner this replaced ended in `_ => {}`, and its missing arms were how a
/// function reading `$1` came to be reported as taking no arguments.
fn sig_record_expr(expr: &Expr, sig: &mut ArgSig) {
    match &expr.kind {
        ExprKind::SimpleExpansion(name) => sig_record_name(name, sig),
        ExprKind::ParameterExpansion(pe) => sig_record_pexp_subject(pe, sig),
        ExprKind::Subscript { name, index } if name == "args" => {
            sig_record_index(index, sig, false);
        }
        // The body is a statement body the walk reaches on its own; only a
        // whole-args fold is a property of the substitution itself.
        ExprKind::CommandSubstitution(stmts) if cmdsub_reads_whole_args(stmts) => {
            sig.variadic = true;
        }
        ExprKind::Arithmetic(a) => sig_record_arith_names(a, sig),
        // Another collection's cell, a substitution that does not fold the
        // whole of `args`, and the nodes with no read of their own.
        ExprKind::Subscript { .. }
        | ExprKind::CommandSubstitution(_)
        | ExprKind::Literal(_)
        | ExprKind::SingleQuoted(_)
        | ExprKind::DoubleQuoted(_)
        | ExprKind::Concat(_)
        | ExprKind::Array(_)
        | ExprKind::AssocArray(_)
        | ExprKind::Test(_)
        | ExprKind::SlurpFile(_) => {}
    }
}

/// Record the subject of a `${…}` expansion into the arg signature: an
/// `args[i]` cell, a positional `${N}`, whole-args (`${args[@]}` → variadic),
/// or an ordinary name (not an arg → ignored).
///
/// `${args[k]:OP}` is an indexed cell; `@args` (no cell index) is whole-args.
/// `${N}` behaves like `$N` — and a positional `${N:-}`/`${N:~}` reads index
/// N-1 *optionally* (DD#84), exactly like `${args[N-1]:-}`. (`${#args}`/`${#}`
/// are retired surfaces — SB4038/SB4034.)
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

/// `$(join args)` — a whole-args *fold*: it reads every element, so the callee
/// can use any number and is variadic.
///
/// `$(keys args)` is **not** here although DD#81 once listed it: `args` is an
/// indexed array and `keys` refuses every indexed array (SB4030, spec 75.100),
/// so no accepted program contains it and an answer for it would only
/// advertise a rejected spelling.
///
/// `$(len args)` is deliberately **absent** (DD#81 addendum 2026-09-28, DG):
/// it reads the argument *count*, not any argument. In a real variadic
/// function it is only the loop bound — the access inside the loop
/// (`${args[$i]}`, a dynamic index) is what marks the function variadic.
/// Without such an access the count is vacuous, so it must not silently
/// erase the arity ceiling. `$#` is the same rule, in `sig_record_name`.
/// A function that genuinely wants its own count declares it with the DD#81
/// no-op: `fn n { : @args; echo $# }`.
///
/// Suppression changes checking, not what runs (DD#55): `$(- join args)` reads
/// every argument exactly as `$(join args)` does, so the command is found
/// through the shared wrapper peel rather than a bare `StmtKind::Command`.
fn cmdsub_reads_whole_args(stmts: &[Statement]) -> bool {
    stmts.iter().filter_map(crate::ir::value_command).any(|c| {
        matches!(&c.name.kind, ExprKind::Literal(n) if n == "join")
            && c.args
                .iter()
                .any(|a| matches!(&a.kind, ExprKind::Literal(x) if x == "args"))
    })
}

/// Record an arithmetic tree's bare positional reads at every node, through
/// [`ArithExpr::children`] — which includes a sexpr call's arguments, so
/// `(strlen $1)` reads index 0. An `Expansion` holds an `Expr` the walk
/// already handed to [`sig_record_expr`], disposition-bearing cells included.
/// Bare `args[k]` is a string-cell type error (DD#77), not another signature.
/// Total over `ArithExpr`; do not add a wildcard.
fn sig_record_arith_names(a: &ArithExpr, sig: &mut ArgSig) {
    match a {
        ArithExpr::Var(name) | ArithExpr::DollarVar(name) => sig_record_name(name, sig),
        ArithExpr::Expansion(_)
        | ArithExpr::Literal(_)
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
        sig_record_arith_names(child, sig);
    }
}

/// Every way a function body hands its caller a result: the `return` commands
/// (`values`, as [`maybe_collect_return`] reads them) and the statements a
/// `return <producer>` returns the status of (`producers`, DD#74).
pub(super) struct ReturnSites<'a> {
    pub(super) values: Vec<Option<String>>,
    pub(super) producers: Vec<&'a Statement>,
}

pub(super) fn collect_return_sites(stmts: &[Statement]) -> ReturnSites<'_> {
    let mut sites = ReturnSites {
        values: Vec::new(),
        producers: Vec::new(),
    };
    collect_return_sites_inner(stmts, &mut sites);
    sites
}

/// **Total over `StmtKind` on purpose — do not add a wildcard.**
fn collect_return_sites_inner<'a>(stmts: &'a [Statement], sites: &mut ReturnSites<'a>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Command(c) => maybe_collect_return(c, &mut sites.values),
            // Nested function definitions have their own returns.
            StmtKind::Function(_) => {}
            StmtKind::ReturnProducer(inner) => {
                sites.producers.push(inner);
                collect_return_sites_bodies(stmt, sites);
            }
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
            | StmtKind::ArithTest(_) => collect_return_sites_bodies(stmt, sites),
        }
    }
}

/// The nested-body half of [`collect_return_sites_inner`]. `structural_bodies`,
/// not `child_bodies`: a `$( … )` body is an anonymous function body (DD#86),
/// so a `return` inside it belongs to that body, not to the enclosing function.
fn collect_return_sites_bodies<'a>(stmt: &'a Statement, sites: &mut ReturnSites<'a>) {
    for body in stmt.structural_bodies() {
        collect_return_sites_inner(body, sites);
    }
}

/// What a `return <producer>` hands back, for deciding whether its function
/// has a result (DD#55).
pub(super) enum ProducerStatus<'a> {
    /// `return f …` inside `f`: the loop's back edge (DD#27), so the result is
    /// whatever the function's other returns give.
    SelfTail,
    /// A call of another user function, which hands back that function's
    /// result: nothing when it is void.
    Calls(&'a str),
    /// Anything else — an external, `[[ … ]]`, `(( … ))`, a pipeline, a
    /// negation, a redirected call — yields a status the caller can see.
    Status,
}

pub(super) fn classify_producer<'a>(
    producer: &'a Statement,
    function: &str,
    is_function: impl Fn(&str) -> bool,
) -> ProducerStatus<'a> {
    let StmtKind::Command(c) = &producer.kind else {
        return ProducerStatus::Status;
    };
    let head = literal_command_name(&c.name);
    if head == function {
        ProducerStatus::SelfTail
    } else if is_function(head) {
        ProducerStatus::Calls(head)
    } else {
        ProducerStatus::Status
    }
}

impl ProducerStatus<'_> {
    /// Whether this `return <producer>` exit can only hand back 0, given which
    /// functions are void; `None` for a self tail-call, which is the loop's
    /// back edge rather than an exit (DD#27).
    pub(super) fn exit_yields_zero(&self, is_void: impl Fn(&str) -> bool) -> Option<bool> {
        match self {
            ProducerStatus::SelfTail => None,
            ProducerStatus::Calls(g) => Some(is_void(g)),
            ProducerStatus::Status => Some(false),
        }
    }
}

/// A literal `return 0`, or a bare `return` (an implicit 0).
pub(super) fn returns_zero(value: &Option<String>) -> bool {
    matches!(value.as_deref(), Some("0") | Some(""))
}

/// DD#55: what one definition's own exits decide. An `-> int` function always
/// has a result; any other has a status when some exit can hand back one — a
/// `return` with a nonzero or computed value, or a status-producing `return
/// <producer>` — and otherwise has the class of the functions its `return g`
/// exits hand the result to. `is_function` names every function the program
/// defines, so a `return g` is recorded whichever definition of `g` is in
/// force where the result is decided.
pub(super) fn exit_summary(f: &Function, is_function: impl Fn(&str) -> bool) -> ExitSummary {
    if f.return_type == VarType::Int {
        return ExitSummary::Int;
    }
    returned_callees(f, is_function).map_or(ExitSummary::Status, ExitSummary::Returns)
}

/// The functions a non-int definition's `return g` exits hand the result to,
/// or `None` when some exit already yields a status.
fn returned_callees(f: &Function, is_function: impl Fn(&str) -> bool) -> Option<Vec<String>> {
    let sites = collect_return_sites(&f.body);
    let mut callees = Vec::new();
    for producer in sites.producers {
        match classify_producer(producer, &f.name, &is_function) {
            ProducerStatus::SelfTail => {}
            ProducerStatus::Calls(g) => callees.push(g.to_string()),
            ProducerStatus::Status => return None,
        }
    }
    sites.values.iter().all(returns_zero).then_some(callees)
}

/// What a `return g` into a name with no exit summary runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Absent {
    /// An external command, so the exit yields a status: the in-force view,
    /// where `g` is not defined yet.
    Status,
    /// Some definition of a redefined `g`, so the caller's class is unknown:
    /// the static view, which has no call site to pick one.
    Unknown,
}

/// The functions a `return g` exit hands the result to, per undecided name.
type Undecided<'a> = Vec<(&'a str, &'a [String])>;

/// DD#55: each function's class from its exits. A least fixpoint over the
/// `return g` edges: a function that hands its result to a non-void one has a
/// status, and what never reaches one is void, so a cycle that reaches no
/// status stays void (SB5037 rejects the cycle anyway) and the answer does not
/// depend on the order of the definitions. A function whose class `absent`
/// leaves unknown gets none.
pub(super) fn classify(
    exits: &HashMap<String, ExitSummary>,
    absent: Absent,
) -> HashMap<String, FnClass> {
    let (mut classes, mut undecided) = seed_classes(exits);
    while let Some(i) = undecided.iter().position(|(_, callees)| {
        callees
            .iter()
            .any(|g| hands_back_result(g, &classes, exits, absent))
    }) {
        classes.insert(undecided.swap_remove(i).0.to_string(), FnClass::Status);
    }
    if absent == Absent::Unknown {
        drop_unknown(&mut undecided, exits);
    }
    let void = undecided
        .into_iter()
        .map(|(name, _)| (name.to_string(), FnClass::Void));
    classes.extend(void);
    classes
}

/// The classes the exits decide outright, and the functions whose class is
/// that of the ones they hand the result to.
fn seed_classes(exits: &HashMap<String, ExitSummary>) -> (HashMap<String, FnClass>, Undecided<'_>) {
    let mut classes = HashMap::new();
    let mut undecided = Vec::new();
    for (name, summary) in exits {
        let class = match summary {
            ExitSummary::Int => FnClass::Int,
            ExitSummary::Status => FnClass::Status,
            ExitSummary::Returns(callees) => {
                undecided.push((name.as_str(), callees.as_slice()));
                continue;
            }
        };
        classes.insert(name.clone(), class);
    }
    (classes, undecided)
}

/// Whether a `return g` exit can hand back a result: `g` is non-void, or it
/// has no exit summary and runs an external command.
fn hands_back_result(
    g: &str,
    classes: &HashMap<String, FnClass>,
    exits: &HashMap<String, ExitSummary>,
    absent: Absent,
) -> bool {
    classes.contains_key(g) || (absent == Absent::Status && !exits.contains_key(g))
}

/// [`Absent::Unknown`]: drop each function that is not yet a status and hands
/// its result to a name with no exit summary, or to a function dropped
/// already — some definitions of that name may yield a status.
fn drop_unknown(undecided: &mut Undecided<'_>, exits: &HashMap<String, ExitSummary>) {
    let mut unknown = HashSet::new();
    while let Some(i) = undecided.iter().position(|(_, callees)| {
        callees
            .iter()
            .any(|g| !exits.contains_key(g) || unknown.contains(g.as_str()))
    }) {
        unknown.insert(undecided.swap_remove(i).0);
    }
}

/// DD#55: each function's class whatever the call site, decided over every
/// definition before any body is lowered, for the `fn_NAME` templates (an
/// uncalled body is checked too). A real call runs the definition in force,
/// so only a name defined once has one class everywhere; a redefined name,
/// and every function whose class it decides, is left to the real call sites.
pub(super) fn static_classes(
    program: &Program,
    known: &HashSet<String>,
) -> HashMap<String, FnClass> {
    let mut definitions: HashMap<&str, usize> = HashMap::new();
    for f in program.function_defs() {
        *definitions.entry(f.name.as_str()).or_default() += 1;
    }
    let exits = program
        .function_defs()
        .filter(|f| definitions[f.name.as_str()] == 1)
        .map(|f| (f.name.clone(), exit_summary(f, |n| known.contains(n))))
        .collect();
    classify(&exits, Absent::Unknown)
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
        assert_eq!(
            collect_return_sites(&body).values,
            vec![Some("0".to_string())]
        );
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
