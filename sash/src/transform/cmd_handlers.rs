//! Command-specific handlers extracted from handle_command.

use super::expressions::{arg_to_value, expr_to_value, literal_name_of, resolve_args_name};
use super::functions::lift_int_func_args;
use super::resolution::resolve_pipeline_command;
use crate::ir::*;
use crate::ssa::*;

/// Handle the `read` / `readline` family — emit `ReadArray`/`SplitValue`
/// (DD#99 `read`) or `ReadLine` (`readline`).
pub(super) fn handle_read(
    c: &Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    // DD#99 stage 6: `read ARRAY` / `read SRC ARRAY` IS the NUL-split array
    // read — the staging alias `read` is retired. Arity/flags are checked
    // in validate (SB5057), so by here the shape is known good.
    if literal_name_of(&c.name) == "read" {
        return lower_newread(c, ssa, state, block);
    }
    emit_readline_inst(&crate::ir::readline_target(&c.args), ssa, state, block);
    block
}

/// DD#99 `read`: one arg reads stdin, two split the first (a value) into
/// the second (a binding). Arity/flags are checked in validate (SB5057).
fn lower_newread(
    c: &Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    let (source, target) = match c.args.as_slice() {
        [only] => (None, super::expressions::fill_target_name(only, state)),
        [src, dst] => (
            Some(expr_to_value(src, state)),
            super::expressions::fill_target_name(dst, state),
        ),
        _ => unreachable!(
            "SB5057 requires one or two read operands; a malformed read must not disappear"
        ),
    };
    super::stmt_handlers::autodeclare_bulk_target(&target, ssa, state, block);
    // Define a LATER version than the autodeclare's empty-array Assign, so the
    // SSA taint backstop's max-version lookup resolves to the fill rather than
    // to the clean empty array (a naive `{name}_0` insert loses that race).
    let target = state.define(&target);
    let (fd, clean_pipe) = state.pipe_reader.unwrap_or((0, false));
    let inst = match source {
        Some(source) => SsaInst::SplitValue { source, target },
        None => SsaInst::ReadArray {
            target,
            fd,
            redirects: vec![],
            clean_pipe,
        },
    };
    ssa.block_mut(block).insts.push(inst);
    block
}

/// Emit the `ReadLine` SSA instruction for a `readline`. DD#99: exactly one
/// target, no flags, newline-terminated — SB5057 has already enforced that.
fn emit_readline_inst(name: &str, ssa: &mut SsaProgram, state: &mut VersionState, block: BlockId) {
    let target = state.define(name);
    let (fd, clean_pipe) = state.pipe_reader.unwrap_or((0, false));
    ssa.block_mut(block).insts.push(SsaInst::ReadLine {
        target,
        fd,
        redirects: vec![],
        clean_pipe,
    });
}

/// Handle `signal` command — emit signal configuration instructions.
pub(super) fn handle_signal(
    _name: &str,
    c: &Command,
    _stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    // SB4059-SB4062 guarantee the shape: exactly two literal arguments, a
    // catchable signal number and one of the four known dispositions. The
    // gates that used to stand here (`args.len() >= 2`, `unwrap_or(0)`,
    // `signum > 0`, `_ => {}`) each turned a malformed `signal` into a silent
    // no-op; the validator rejects those spellings now.
    let signum: i64 = literal_name_of(&c.args[0])
        .parse()
        .expect("SB4060/SB4061 validated the signal number");
    match literal_name_of(&c.args[1]) {
        "IGN" => {
            // SIG_IGN = 1
            ssa.block_mut(block).insts.push(SsaInst::internal(
                BuiltinCmd::TrapSignal,
                vec![
                    SsaValue::Literal(signum.to_string()),
                    SsaValue::Literal("1".to_string()),
                ],
                true,
            ));
        }
        "DFL" => {
            // SIG_DFL = 0
            ssa.block_mut(block).insts.push(SsaInst::internal(
                BuiltinCmd::TrapSignal,
                vec![
                    SsaValue::Literal(signum.to_string()),
                    SsaValue::Literal("0".to_string()),
                ],
                true,
            ));
        }
        "FLAG" => {
            ssa.block_mut(block).insts.push(SsaInst::internal(
                BuiltinCmd::SignalFlag,
                vec![
                    SsaValue::Literal(signum.to_string()),
                    SsaValue::Literal("0".to_string()),
                ],
                true,
            ));
        }
        "EXIT" => {
            // Register EXIT handler + enable checkpoints.
            ssa.block_mut(block).insts.push(SsaInst::internal(
                BuiltinCmd::SignalFlag,
                vec![
                    SsaValue::Literal(signum.to_string()),
                    SsaValue::Literal("1".to_string()),
                ],
                true,
            ));
            state.exiting_signals.insert(signum);
        }
        d => unreachable!("SB4062 validated the signal disposition: {d}"),
    }
    block
}

/// Split `unset`'s arguments into one string per target.
///
/// Flags (`-v`, `-f`, `-n`) are dropped. The parser can split a subscripted
/// target into `["m", "[b]"]`, so an argument that *starts with* `[`
/// re-attaches to the previous one. Joining every argument instead made
/// `unset a[0] a[1]` a single target named `a` with the key `0]a[1`, which
/// matches nothing — every target after the first was silently discarded.
pub(crate) fn unset_targets(c: &Command) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    for arg in c
        .args
        .iter()
        .map(literal_name_of)
        .filter(|arg| !arg.starts_with('-'))
    {
        match items.last_mut() {
            Some(prev) if arg.starts_with('[') => prev.push_str(arg),
            _ => items.push(arg.to_string()),
        }
    }
    items
}

/// `unset arr[key]` → a table delete.
///
/// A map keys its entries by the string; an indexed array by the 8 raw bytes
/// of an i64 (`lower_subscript_raw`, codegen/values.rs). Deleting with the
/// wrong encoding matches nothing, so the wrong pick here is a silent no-op.
fn unset_element(
    full: &str,
    bracket: usize,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) {
    let arr_name = resolve_args_name(&full[..bracket], state);
    let key = full[bracket + 1..full.len() - 1].to_string();
    let arr_ref = state.lookup(&arr_name);
    let cmd = if state.assoc_arrays.contains(&arr_name) {
        BuiltinCmd::TableDelete
    } else {
        BuiltinCmd::TableDeleteIdx
    };
    ssa.block_mut(block).insts.push(SsaInst::internal(
        cmd,
        vec![SsaValue::Var(arr_ref), SsaValue::Literal(key)],
        true,
    ));
}

/// `unset name` → rebind the resolved collection to a fresh empty table.
fn unset_whole(name: &str, ssa: &mut SsaProgram, state: &mut VersionState, block: BlockId) {
    let name = resolve_args_name(name, state);
    // Dead function templates retain the reserved slot name until real inlining.
    assert!(
        state.collections.contains(&name) || crate::ir::is_collection_slot(&name),
        "ICE: unset requires a collection name"
    );
    let value = SsaValue::Literal("__new_table__".to_string());
    let target = state.define(&name);
    ssa.block_mut(block)
        .insts
        .push(SsaInst::Assign { target, value });
}

/// Handle `unset` — delete an element, or clear a whole variable.
///
/// `unset -f func` is not supported; the flag is dropped and the name treated
/// as a variable.
pub(super) fn handle_unset(
    _name: &str,
    c: &Command,
    _stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    for full in unset_targets(c) {
        match full.find('[') {
            Some(bracket) => unset_element(&full, bracket, ssa, state, block),
            None => unset_whole(&full, ssa, state, block),
        }
    }
    block
}

/// Handle function call (inline expansion) or external command emit.
///
/// If the command name matches a known function, the function body is
/// inlined at the call site with positional params remapped.
/// Otherwise, emits an SsaInst::Command.
pub(super) fn handle_function_call_or_command(
    name: &str,
    c: &Command,
    _stmt: &Statement,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    if let Some(body) = state.function_bodies.get(name).cloned() {
        return inline_function_call(name, &body, c, ssa, state, block);
    }
    // DD#81: this bareword resolved to an external command (no function defined
    // so far) — record it so a later `fn NAME` is flagged as a collision. Skip
    // records made while lowering a dead `fn_` template (not real execution).
    if !state.lowering_fn_template {
        state.externally_used.borrow_mut().insert(name.to_string());
    }
    emit_external_command(name, c, ssa, state, block)
}

/// Inline a known function body into the current block.
/// Saves caller's positional params, sets up the callee's `args`,
/// lowers the body into a fresh call block, then emits a restore block.
/// Also the body of a function pipeline stage, inside its forked child.
pub(super) fn inline_function_call(
    name: &str,
    body: &[Statement],
    c: &Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    mut block: BlockId,
) -> BlockId {
    // DD#27/§5: recursion. Tail self-calls are handled before reaching here.
    if super::functions::reject_recursive_call(name, state) {
        return block;
    }
    // `f > out.txt`: the redirect parked by `lower_redirected_stmt`. Taken
    // before the body is lowered, so a nested redirected call inside the body
    // opens its own scope rather than re-opening this one.
    let scope = state.pending_redirect_scope.take();

    // Lower the call arguments ONCE in the caller's scope (before `args` is
    // redefined), so a side-effecting `$( … )` arg is not executed twice and
    // args reading the caller's positionals see the caller's values.
    // `arg_to_value`, not `expr_to_value`: a call argument is a COMMAND-ARG
    // position, so `@a` splats to one value per element. `expr_to_value` gives
    // `@a` its joined-string meaning, which silently passed a function the
    // whole array as a single argument — and made `("a b" "c")`
    // indistinguishable from `(a b c)`.
    // DD#100: a `=name` argument passes a COLLECTION by identity — it binds a
    // slot (`A`..`E`) rather than contributing a value to `args`, so split it
    // out before lowering.
    let (scalar_args, coll_args) = super::functions::split_call_args(&c.args);
    let arg_values: Vec<SsaValue> = scalar_args.iter().map(|a| arg_to_value(a, state)).collect();

    check_callee_arity(name, body, &arg_values, state);
    let arg_values = lift_int_func_args(arg_values, ssa, state, &mut block);
    state.inlining.push(name.to_string());
    let saved_args = super::functions::save_caller_args(state);
    let saved_slots = super::functions::setup_callee_slots(&coll_args, state);
    let saved_ro = super::functions::enter_fn_body_scope(state);
    super::functions::setup_callee_args(&arg_values, ssa, state, block);

    // Open the redirect scope last, so everything that only computes values
    // (the spilled targets, the argument bindings) has already run on the
    // caller's own fds. `pre_state` is the version map the skip path carries
    // into the restore block.
    let bracket = scope.map(|redirects| {
        let site = state.alloc_redirect_site();
        let fds: Vec<i64> = redirects.iter().map(|r| r.resolved_fd()).collect();
        let checked = state.in_condition == 0;
        ssa.block_mut(block).insts.push(SsaInst::RedirectPush {
            site,
            redirects,
            checked,
        });
        (site, fds)
    });
    let pre_state = state.snapshot();

    // Inline the function body
    let call_block = ssa.new_checkpoint_block(&format!("call_{}", name));
    ssa.block_mut(block).terminator = Terminator::Goto(call_block);

    let restore_block = ssa.new_checkpoint_block(&format!("ret_{}", name));
    let exit_block =
        super::functions::lower_inlined_body(name, body, call_block, restore_block, ssa, state);

    if let Some((site, fds)) = bracket {
        // Close the scope first thing in the restore block, so it also runs
        // for a `return` out of the body (which targets this block) and for
        // the skip path below.
        ssa.block_mut(restore_block)
            .insts
            .push(SsaInst::RedirectPop { site, fds });
        // DD#55: a failed `open(2)` must not run the body on the wrong fds.
        // The scope sets the test result, so branching on it here skips
        // straight to the restore — the cross-block equivalent of the
        // if/else a plain command's redirect guard emits around itself. The
        // skip edge is only *taken* in condition position (elsewhere the
        // scope has already aborted), but emitting it unconditionally keeps
        // the shape independent of the later `checked` rewrites that
        // `mark_unchecked_in_conditions` and the END-block pass apply.
        let body_state = state.snapshot();
        ssa.block_mut(block).terminator = Terminator::Branch {
            condition: "redirect".to_string(),
            true_block: call_block,
            false_block: restore_block,
        };
        super::helpers::insert_phis_two_branch(
            state,
            ssa,
            restore_block,
            exit_block,
            &body_state,
            block,
            &pre_state,
        );
    }

    super::functions::exit_fn_body_scope(state, saved_ro);
    super::functions::restore_caller_slots(saved_slots, state);
    super::functions::restore_caller_args(saved_args, state);

    state.inlining.pop();
    restore_block
}

/// DD#46/#48: emit a diagnostic if the callee reads `$K` (accounting for
/// `shift`) where K exceeds the number of arguments actually passed. The
/// positional values themselves live in the `args` array (setup_callee_args).
///
/// DD#84 (DG, 2026-09-28): a splat and an elide each contribute a number of
/// arguments that is only known at RUNTIME, so they are meaningful only where
/// the callee accepts any number. A fixed-arity callee cannot be satisfied by a
/// count that does not exist yet, and a variadic callee whose floor is above
/// the call's minimum can be starved by an empty one.
pub(super) fn check_callee_arity(
    name: &str,
    body: &[Statement],
    arg_values: &[SsaValue],
    state: &mut VersionState,
) {
    let bounds = call_arg_bounds(arg_values);
    let complaint = match super::functions::analyze_arity(body).0 {
        super::functions::Arity::Fixed { min, max } => fixed_arity_msg(name, min, max, &bounds),
        super::functions::Arity::Variadic { floor } => variadic_arity_msg(name, floor, &bounds),
    };
    if let Some(msg) = complaint {
        state.error_at_stmt(msg);
    }
}

/// What a call site can deliver, as the arity check needs to see it.
struct CallBounds {
    /// How many *leading* `args` positions are guaranteed to hold what the
    /// author wrote there. Not a count of arguments: once a variable-length
    /// argument appears, every position after the ones its minimum guarantees
    /// is decided at run time, so this stops there.
    ///
    /// The distinction is not academic. Summing per-argument minimums instead
    /// accepts `f @e x` against a callee that reads `${args[0]}`, because
    /// 0 + 1 reaches the floor — and then `${args[0]}` is `x` when `e` is
    /// empty and `e[0]` when it is not. A fixed read silently picks up a
    /// different variable's value depending on an array's length.
    covered: usize,
    /// The number of argument *values* written at the call site. Meaningful
    /// only when `dynamic` is false — a splat is one value and any number of
    /// arguments, which is the miscount this whole check exists to avoid.
    written: usize,
    /// Any argument's token count is decided at run time.
    dynamic: bool,
}

/// DD#84 (2026-09-28): a fixed-arity callee cannot take a runtime-variable
/// count at all. A static call keeps the pre-existing SB5042/SB5043 messages.
fn fixed_arity_msg(name: &str, min: usize, max: usize, b: &CallBounds) -> Option<String> {
    if b.dynamic {
        return Some(arity_dynamic_msg(name, min, max));
    }
    if b.written < min {
        return Some(arity_too_few_msg(name, min, false, b.written));
    }
    if b.written > max {
        return Some(arity_too_many_msg(name, min, max, b.written));
    }
    None
}

/// A variadic callee takes any number at or above its floor — so a dynamic
/// count is fine, provided its *minimum* still reaches that floor.
fn variadic_arity_msg(name: &str, floor: usize, b: &CallBounds) -> Option<String> {
    if b.dynamic && b.covered < floor {
        return Some(arity_floor_msg(name, floor, b.covered));
    }
    if !b.dynamic && b.written < floor {
        return Some(arity_too_few_msg(name, floor, true, b.written));
    }
    None
}

fn call_arg_bounds(arg_values: &[SsaValue]) -> CallBounds {
    let mut covered = 0usize;
    let mut dynamic = false;
    for v in arg_values {
        let (least, varies) = arg_min_tokens(v);
        covered += least;
        if varies {
            // DG (2026-09-28): a function's fixed portion is met by discrete
            // arguments "as though there were no variadic component". A
            // variable-length argument guarantees only the positions its
            // minimum covers; everything past them shifts with the array, so
            // no later position can satisfy a fixed read.
            dynamic = true;
            break;
        }
    }
    CallBounds {
        covered,
        written: arg_values.len(),
        dynamic,
    }
}

/// The least number of `args` entries one argument can deliver, and whether
/// its contribution is decided at run time.
///
/// The catch-all counts exactly one token, which is right for every value
/// shape that is not a splat or an elide. A new `SsaValue` that expands to a
/// *variable* number of tokens has to be given its own arm here, or it
/// silently counts as one and this check stops seeing it.
fn arg_min_tokens(v: &SsaValue) -> (usize, bool) {
    match v {
        // `${x:~}` — zero tokens when the source is empty, one otherwise.
        SsaValue::ElideEmpty { .. } => (0, true),
        // DD#69/#84: `@{a:-W}` substitutes one element when the array is empty
        // and `@{a:?}` aborts, so on any path that continues both deliver at
        // least one. Bare `@a` and `@{a:~}` yield zero.
        SsaValue::ArraySplat { disposition, .. } => {
            let guaranteed = disposition.as_ref().is_some_and(|d| d.has_word || d.abort);
            (usize::from(guaranteed), true)
        }
        _ => (1, false),
    }
}

fn arity_range(min: usize, max: usize) -> String {
    if min == max {
        format!("exactly {} argument{}", min, arity_plural(min))
    } else {
        format!("{} to {} arguments", min, max)
    }
}

/// SB5087: a runtime-variable argument count passed to a fixed-arity callee.
fn arity_dynamic_msg(name: &str, min: usize, max: usize) -> String {
    format!(
        "[SB5087] function '{}' takes {} — it cannot be called with a splat or a `:~` elide, whose argument count is only known at run time. Pass the arguments individually, or let '{}' accept a variable number with `: @args`",
        name,
        arity_range(min, max),
        name
    )
}

/// SB5087: a call that does not fix every position a variadic callee reads.
fn arity_floor_msg(name: &str, floor: usize, covered: usize) -> String {
    let fixed = if covered == 0 {
        "none of them are at a known position in this call".to_string()
    } else {
        format!("this call fixes only the first {}", covered)
    };
    format!(
        "[SB5087] function '{}' always reads {} argument{}, but {} — a splat or a `:~` elide has a run-time length, so nothing written after one lands at a position you can count on. Put the required arguments before it, or use `@{{a:-…}}` to guarantee an element",
        name,
        floor,
        arity_plural(floor),
        fixed
    )
}

fn arity_plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

fn arity_too_few_msg(name: &str, min: usize, variadic: bool, got: usize) -> String {
    let atleast = if variadic { "at least " } else { "" };
    format!(
        "[SB5042] function '{}' expects {}{} argument{} but was called with {}",
        name,
        atleast,
        min,
        arity_plural(min),
        got
    )
}

/// SB5043: more arguments than the callee can take. Reports the whole range,
/// not the minimum — a DD#84 callee with trailing optionals accepts a span, and
/// naming only its floor states something false. `f` reading `${args[0]}` and
/// `${args[1]:-d}` takes one argument or two; called with three it used to be
/// told "takes 1 argument", which denies that `f a b` is legal.
fn arity_too_many_msg(name: &str, min: usize, max: usize, got: usize) -> String {
    format!(
        "[SB5043] function '{}' takes {} but was called with {} — use @args to accept a variable number",
        name,
        arity_range(min, max),
        got
    )
}

fn emit_external_command(
    name: &str,
    c: &Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    mut block: BlockId,
) -> BlockId {
    let raw: Vec<SsaValue> = c.args.iter().map(|a| arg_to_value(a, state)).collect();
    // DD#99: `write` IS `echo` without the trailing newline. Both lower through
    // `lower_echo`, which reads the suppression from a leading `-n`, so the
    // distinction is carried into the SSA here. `-n` is now purely internal —
    // the source spelling is SB5059.
    let (name, raw) = rewrite_write_as_echo(name, raw);
    let name = name.as_str();
    // Option C: `$(f)` / `$((f))` on an `-> int` function is an IntFuncCall;
    // materialize it into a temp so the argv is scalar (unlifted → codegen ICE).
    let args = lift_int_func_args(raw, ssa, state, &mut block);
    // DD#14: `exec CMD …` replaces the running program — resolve the target
    // (args[0]) to its external absolute path so codegen's `sash_exec` (which
    // rejects bare names) gets a real path. Unlike a normal command head, exec
    // must not keep `echo`/`:` inline; it runs the external program. The
    // fd-only form (`exec > file`) has no args and is unaffected.
    let args = if name == "exec" {
        resolve_exec_target_arg(args, state)
    } else {
        args
    };
    ssa.block_mut(block).insts.push(SsaInst::command(
        resolve_pipeline_command(name, state),
        args,
        Vec::new(),
        true,
    ));
    block
}

/// Rewrite an `exec` target (args[0]) to its resolved external absolute path
/// and record it as an external dependency (for `--manifest`). A target that
/// already contains `/` or is not a literal (the latter is rejected at
/// validation) is left untouched.
fn resolve_exec_target_arg(mut args: Vec<SsaValue>, state: &mut VersionState) -> Vec<SsaValue> {
    let target = match args.first() {
        Some(SsaValue::Literal(s)) if !s.contains('/') => s.clone(),
        _ => return args,
    };
    if let Some(path) = crate::resolve::resolve_command_path(&target) {
        state.external_deps.borrow_mut().insert(path.clone());
        args[0] = SsaValue::Literal(path);
    } else {
        state.error_at_stmt(format!(
            "[SB5011] `exec` target '{target}' could not be resolved at compile time"
        ));
    }
    args
}

/// DD#99: `write ARGS` as the SSA-level `echo -n ARGS`. Applied wherever a
/// command's name+args are built for codegen — the statement path here and
/// the pipeline-stage builder — so no lowering path sees a bare `write`.
pub(super) fn rewrite_write_as_echo(name: &str, args: Vec<SsaValue>) -> (String, Vec<SsaValue>) {
    if name != "write" {
        return (name.to_string(), args);
    }
    let mut out = Vec::with_capacity(args.len() + 1);
    out.push(SsaValue::Literal("-n".to_string()));
    out.extend(args);
    ("echo".to_string(), out)
}

/// SB4059-SB4062 fix the `signal` shape before the transform runs: exactly
/// two literal arguments, a catchable signal number, and one of the four
/// known dispositions. The `unreachable!` on the disposition is the claim
/// that SB4062's table and this match agree; the gate it replaced turned a
/// typo'd disposition into a silent no-op. DG's rule for the coverage
/// campaign: an unreachable assertion earns a unit test proving it catches
/// what it claims.
#[cfg(test)]
mod invariant_tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    #[should_panic(expected = "unset requires a collection name")]
    fn unset_cannot_turn_a_missing_collection_into_a_scalar_assignment() {
        let mut ssa = SsaProgram::new();
        let block = ssa.entry;
        let mut state = VersionState::with_types(HashMap::new());
        unset_whole("missing", &mut ssa, &mut state, block);
    }

    fn lit(s: &str) -> Expr {
        Expr {
            kind: ExprKind::Literal(s.to_string()),
            span: Span::new(1, 1),
        }
    }

    #[test]
    #[should_panic(expected = "SB4062 validated the signal disposition")]
    fn handle_signal_rejects_an_unknown_disposition() {
        let c = Command {
            name: lit("signal"),
            args: vec![lit("15"), lit("BOGUS")],
        };
        let stmt = Statement {
            kind: StmtKind::Command(c.clone()),
            span: Span::new(1, 1),
        };
        let mut ssa = SsaProgram::new();
        let block = ssa.new_block("entry");
        ssa.entry = block;
        let mut state = VersionState::with_types(HashMap::new());
        handle_signal("signal", &c, &stmt, &mut ssa, &mut state, block);
    }

    #[test]
    #[should_panic(expected = "a malformed read must not disappear")]
    fn malformed_read_cannot_be_silently_dropped() {
        let c = Command {
            name: lit("read"),
            args: vec![],
        };
        let mut ssa = SsaProgram::new();
        let block = ssa.entry;
        let mut state = VersionState::with_types(HashMap::new());
        lower_newread(&c, &mut ssa, &mut state, block);
    }

    #[test]
    fn unresolved_exec_target_cannot_reach_codegen_as_a_bare_name() {
        let name = format!("__sash_missing_exec_{}__", std::process::id());
        assert!(crate::resolve::resolve_command_path(&name).is_none());
        let mut state = VersionState::with_types(HashMap::new());
        resolve_exec_target_arg(vec![SsaValue::Literal(name.clone())], &mut state);
        assert_eq!(state.diagnostics.len(), 1);
        assert!(state.diagnostics[0].contains("[SB5011]"));
        assert!(state.diagnostics[0].contains(&name));
        assert!(state.external_deps.borrow().is_empty());
    }

    /// The SB5037 refusal itself is pinned by `capcyc.*` and `stgcyc.1`;
    /// what no spec can see is *where* it sits. It must come before the call
    /// takes anything from its caller — the parked redirect scope, the
    /// caller's `args` (`save_caller_args` empties `current_args_name`) and
    /// the `inlining` frame — because the early return restores none of
    /// them. The function-stage route used to carry a private copy of this
    /// inliner with the refusal after the take, which owed (and had) its own
    /// unwind.
    #[test]
    fn recursion_is_refused_before_the_call_takes_any_caller_state() {
        let mut ssa = SsaProgram::new();
        let block = ssa.entry;
        let blocks_before = ssa.blocks.len();
        let mut state = VersionState::with_types(HashMap::new());
        state.function_bodies.insert("f".to_string(), vec![]);
        state.inlining.push("f".to_string());
        state.current_args_name = "args_7".to_string();
        state.pending_redirect_scope = Some(vec![]);
        let c = Command {
            name: lit("f"),
            args: vec![],
        };

        let got = inline_function_call("f", &[], &c, &mut ssa, &mut state, block);

        assert_eq!(got, block, "a refused call lowers nothing");
        assert_eq!(ssa.blocks.len(), blocks_before, "and creates no block");
        assert_eq!(state.diagnostics.len(), 1, "exactly one diagnostic");
        assert!(
            state.diagnostics[0].contains("[SB5037]")
                && state.diagnostics[0].contains("recursive call to 'f'"),
            "got: {:?}",
            state.diagnostics[0]
        );
        assert_eq!(state.current_args_name, "args_7", "the caller's args");
        assert_eq!(
            state.inlining,
            vec!["f".to_string()],
            "no frame pushed or popped"
        );
        assert!(
            state.pending_redirect_scope.is_some(),
            "the scope stays parked; lower_redirected_stmt discards it"
        );
    }
}
