//! Command-specific handlers extracted from handle_command.

use super::expressions::{arg_to_value, expr_to_string, expr_to_value};
use super::functions::lift_int_func_args;
use super::resolution::resolve_pipeline_command;
use super::statements::lower_stmts;
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
    if expr_to_string(&c.name) == "read" {
        return lower_newread(c, ssa, state, block);
    }
    emit_readline_inst(&crate::ir::readline_targets(&c.args), ssa, state, block);
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
        _ => return block,
    };
    super::stmt_handlers::autodeclare_bulk_target(&target, false, ssa, state, block);
    let inst = match source {
        Some(source) => SsaInst::SplitValue {
            source,
            array: target,
        },
        None => SsaInst::ReadArray {
            array: target,
            fd: 0,
            redirects: vec![],
        },
    };
    ssa.block_mut(block).insts.push(inst);
    block
}

/// Emit the `ReadLine` SSA instruction for a `readline`. DD#99: exactly one
/// target, no flags, newline-terminated — SB5057 has already enforced that.
fn emit_readline_inst(
    flags: &crate::ir::ReadTarget,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) {
    let [name] = flags.var_names.as_slice() else {
        return;
    };
    let target = state.define(name);
    ssa.block_mut(block).insts.push(SsaInst::ReadLine {
        target,
        fd: 0,
        redirects: vec![],
        clean_pipe: false,
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
    let signum: i64 = expr_to_string(&c.args[0])
        .parse()
        .expect("SB4060/SB4061 validated the signal number");
    match expr_to_string(&c.args[1]).as_str() {
        "IGN" => {
            // SIG_IGN = 1
            ssa.block_mut(block).insts.push(SsaInst::sigil(
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
            ssa.block_mut(block).insts.push(SsaInst::sigil(
                BuiltinCmd::TrapSignal,
                vec![
                    SsaValue::Literal(signum.to_string()),
                    SsaValue::Literal("0".to_string()),
                ],
                true,
            ));
        }
        "FLAG" => {
            ssa.block_mut(block).insts.push(SsaInst::sigil(
                BuiltinCmd::SignalFlag,
                vec![SsaValue::Literal(signum.to_string())],
                true,
            ));
        }
        "EXIT" => {
            // Register FLAG handler + mark as exiting signal
            ssa.block_mut(block).insts.push(SsaInst::sigil(
                BuiltinCmd::SignalFlag,
                vec![SsaValue::Literal(signum.to_string())],
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
fn unset_targets(c: &Command) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    for arg in c.args.iter().map(expr_to_string) {
        if arg.starts_with('-') {
            continue;
        }
        match items.last_mut() {
            Some(prev) if arg.starts_with('[') => prev.push_str(&arg),
            _ => items.push(arg),
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
    let arr_name = &full[..bracket];
    let key = full[bracket + 1..full.len() - 1].to_string();
    let arr_ref = state.lookup(arr_name);
    let cmd = if state.assoc_arrays.contains(arr_name) {
        BuiltinCmd::TableDelete
    } else {
        BuiltinCmd::TableDeleteIdx
    };
    ssa.block_mut(block).insts.push(SsaInst::sigil(
        cmd,
        vec![SsaValue::Var(arr_ref), SsaValue::Literal(key)],
        true,
    ));
}

/// `unset name` → rebind the whole variable: a collection to a fresh empty
/// table, a scalar to the empty string.
fn unset_whole(name: &str, ssa: &mut SsaProgram, state: &mut VersionState, block: BlockId) {
    let value = if state.collections.contains(name) {
        SsaValue::Literal("__new_table__".to_string())
    } else {
        SsaValue::Literal(String::new())
    };
    let target = state.define(name);
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
fn inline_function_call(
    name: &str,
    body: &[Statement],
    c: &Command,
    ssa: &mut SsaProgram,
    state: &mut VersionState,
    block: BlockId,
) -> BlockId {
    // DD#27/§5: reject a call to a function already being inlined (recursion) —
    // unbounded inlining would overflow the compiler. Tail self-calls are
    // handled before reaching here. Emit the diagnostic and skip the inline.
    if state.inlining.iter().any(|n| n == name) {
        state
            .diagnostics
            .push(super::functions::recursion_diagnostic(name));
        return block;
    }
    state.inlining.push(name.to_string());

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

    check_callee_arity(name, body, arg_values.len(), state);
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
    let call_block = ssa.new_block(&format!("call_{}", name));
    ssa.block_mut(block).terminator = Terminator::Goto(call_block);

    // Set return target so `return` jumps to restore block
    let restore_block = ssa.new_block(&format!("ret_{}", name));
    let saved_return_target = state.return_target;
    state.return_target = Some(restore_block);
    // DD#27/§5: a self tail-call (`return F …`) in the body loops to call_block.
    let args_name = state.current_args_name.clone();
    let saved_tail = state.tail_loop.take();
    state.tail_loop = Some((name.to_string(), call_block, args_name));
    let exit_block = lower_stmts(body, ssa, state, call_block);
    state.tail_loop = saved_tail;
    state.return_target = saved_return_target;

    // Normal fall-through also goes to restore block
    ssa.block_mut(exit_block).terminator = Terminator::Goto(restore_block);

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
pub(super) fn check_callee_arity(
    name: &str,
    body: &[Statement],
    call_argc: usize,
    state: &mut VersionState,
) {
    let (min, max) = match super::functions::analyze_arity(body).0 {
        super::functions::Arity::Fixed { min, max } => (min, Some(max)),
        super::functions::Arity::Variadic { floor } => (floor, None),
    };
    if call_argc < min {
        state
            .diagnostics
            .push(arity_too_few_msg(name, min, max.is_none(), call_argc));
    } else if max.is_some_and(|m| call_argc > m) {
        state
            .diagnostics
            .push(arity_too_many_msg(name, min, call_argc));
    }
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
        "1:1: error [SB5042] function '{}' expects {}{} argument{} but was called with {}",
        name,
        atleast,
        min,
        arity_plural(min),
        got
    )
}

fn arity_too_many_msg(name: &str, n: usize, got: usize) -> String {
    format!(
        "1:1: error [SB5043] function '{}' takes {} argument{} but was called with {} — use @args to accept a variable number",
        name, n, arity_plural(n), got
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
fn resolve_exec_target_arg(mut args: Vec<SsaValue>, state: &VersionState) -> Vec<SsaValue> {
    let target = match args.first() {
        Some(SsaValue::Literal(s)) if !s.contains('/') => s.clone(),
        _ => return args,
    };
    if let Some(path) = crate::resolve::resolve_command_path(&target) {
        state.external_deps.borrow_mut().insert(path.clone());
        args[0] = SsaValue::Literal(path);
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
