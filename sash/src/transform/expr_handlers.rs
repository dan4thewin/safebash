//! Handlers for expression evaluation in the transform.

use super::expressions::{arg_to_value, expr_to_string, expr_to_value};
use crate::ir::*;
use crate::ssa::*;

pub(super) fn handle_command_substitution(expr: &Expr, state: &VersionState) -> SsaValue {
    let Some((stmt, name_opt, c)) = single_command_subst(expr) else {
        // Multi-statement / pipeline body (`$(x=1; echo $x)`, `$(a | b)`):
        // carry the body as a `CmdSubstBody` to be lifted into a capture temp
        // by `lift_int_func_calls` (like `IntFuncCall`). Was a silent `$(...)`
        // placeholder at runtime.
        return cmdsub_body_value(expr);
    };
    if let Some(name) = name_opt {
        if is_statement_builtin(BuiltinCmd::from_name(&name)) {
            return cmdsub_body_value(expr);
        }
        if let Some(v) = resolve_single_cmdsubst(name, c, state) {
            return v;
        }
    }
    let argv = build_argv(c, state);
    // DD#55: `$(- cmd)` suppresses the captured command's exit check (no abort
    // on failure), mirroring statement-position `- cmd`. `single_command_subst`
    // peeled the `Suppressed` wrapper to reach the command, so detect it here.
    let suppressed = subst_is_suppressed(&stmt.kind);
    SsaValue::CmdSubst { argv, suppressed }
}

/// The general `$( … )` lowering: carry the body for `lift_int_func_calls` to
/// materialize into a capture temp.
fn cmdsub_body_value(expr: &Expr) -> SsaValue {
    match &expr.kind {
        ExprKind::CommandSubstitution(stmts) => SsaValue::CmdSubstBody {
            body: stmts.to_vec(),
        },
        _ => SsaValue::Expr(expr_to_string(expr)),
    }
}

/// A `BuiltinCmd` that is a **statement**: it has no value shape, so a
/// single-command `$( … )` around it must be lowered as a *body*, not as an
/// exec argv.
///
/// `resolve_single_cmdsubst` returning `None` meant "external", and the caller
/// then built an argv whose head was a bare builtin name. Every such program
/// compiled clean and printed `sash: bug: bare command passed to sash_exec`
/// into its own output at run time, with the capture exiting 127: `$(:)`,
/// `$(write hi)`, `$(cd /tmp)`, `$(close 9)`, `$(attest x [[ … ]])`,
/// `$(argv 0 b)`, `$(wait)`, `$(exit 0)`, `$(exec …)`, `$([[ -n x ]])`,
/// `$(readline q)`, `$(matchall S P a)`, `$(signal INT ignore)`, `$(return)`.
/// The *multi*-statement path always did the right thing, so `$(:; :)` worked
/// where `$(:)` did not — and `single_command_subst` already declines the fast
/// path for a redirected body, for the same reason and with the same remedy.
///
/// DD#86's 2026-09-28 addendum settles that these are legal rather than
/// something to reject: a `$( )` body is an anonymous function body, so any
/// statement legal in a function body is legal as its sole statement.
///
/// **Total over `BuiltinCmd` on purpose — do not add a wildcard.** Which half
/// a builtin belongs to is not guessable from its name, and the cost of
/// landing a new one on the wrong side is a runtime ICE printed into the
/// program's own output.
fn is_statement_builtin(kind: BuiltinCmd) -> bool {
    match kind {
        // Value intrinsics: `resolve_single_cmdsubst` produces a value for
        // these, which is the whole reason the single-command path exists.
        // `match` is here for its two-argument value form; the DD#63
        // three-argument capture statement is SB4045 inside a `$( )` today.
        BuiltinCmd::Echo
        | BuiltinCmd::Len
        | BuiltinCmd::Strlen
        | BuiltinCmd::Join
        | BuiltinCmd::Sub
        | BuiltinCmd::Gsub
        | BuiltinCmd::Match
        | BuiltinCmd::Toupper
        | BuiltinCmd::Tolower
        | BuiltinCmd::Basename
        | BuiltinCmd::Dirname => false,
        // Not a builtin at all — an external command or a user function, which
        // is exactly what the argv path is for.
        BuiltinCmd::Other => false,
        // `keys` has no value lowering (a value-context `$(keys m)` is SB5025
        // and the for-in splat never reaches here), so it belongs on this side
        // rather than falling through to a bare external named `keys`.
        BuiltinCmd::Keys
        | BuiltinCmd::MatchAll
        | BuiltinCmd::Write
        | BuiltinCmd::Test
        | BuiltinCmd::Noop
        | BuiltinCmd::Exit
        | BuiltinCmd::Exec
        | BuiltinCmd::Cd
        | BuiltinCmd::Wait
        | BuiltinCmd::Close
        | BuiltinCmd::Attest
        | BuiltinCmd::Read
        | BuiltinCmd::Mapfile
        | BuiltinCmd::Argv
        | BuiltinCmd::Break
        | BuiltinCmd::Continue
        | BuiltinCmd::Return
        | BuiltinCmd::Signal
        | BuiltinCmd::Unset
        | BuiltinCmd::NegateTestResult
        | BuiltinCmd::SetTestFromStatus
        | BuiltinCmd::LetExit
        | BuiltinCmd::ForkBackground
        | BuiltinCmd::StoreBgPid
        | BuiltinCmd::TrapSignal
        | BuiltinCmd::SignalFlag
        | BuiltinCmd::TableDelete
        | BuiltinCmd::TableDeleteIdx => true,
    }
}

/// True when a `$(…)` single-command body carries a `- ` suppression prefix
/// (`$(- cmd)`, `$(- cmd >f)`) — peels `Redirected` wrappers to find it.
fn subst_is_suppressed(kind: &StmtKind) -> bool {
    match kind {
        StmtKind::Redirected(r) => subst_is_suppressed(&r.body.kind),
        StmtKind::Suppressed(_) => true,
        _ => false,
    }
}

/// Resolve a single-command `$(name …)`: a **defined-so-far** function → capture
/// (DD#81 execution-order gate); a native intrinsic → its value; otherwise
/// record the external use (for the SB5041 collision) and return `None` so the
/// caller emits a plain external `CmdSubst`.
fn resolve_single_cmdsubst(name: String, c: &Command, state: &VersionState) -> Option<SsaValue> {
    if state.function_bodies.contains_key(name.as_str()) {
        return Some(func_call_capture(name, c, state));
    }
    if let Some(var) = nullary_stateful_slot(&name, c) {
        return Some(SsaValue::Var(state.lookup(var)));
    }
    if let Some(v) = try_native_intrinsic(&name, c, state) {
        return Some(v);
    }
    if let Some(v) = wrong_arity_backstop(&name, c) {
        return Some(v);
    }
    if !state.lowering_fn_template {
        state.externally_used.borrow_mut().insert(name);
    }
    None
}

/// The stateful-slot intrinsics (`$(status)`, `$(getcpid)`) resolve to a
/// compiler-managed SSA var — but **only** when written with no arguments.
/// `$(status …)` with arguments is a different thing (SB4045 rejects it:
/// `status` is a value intrinsic of arity 0) and must not be mistaken for a
/// status read.
///
/// **This is what keeps a bare `$(status)` off the external-command path.**
/// `parse::expr::bareword_shortcut_expansion` used to do it, by folding the
/// intrinsic into `SimpleExpansion("?")` at parse. DD#111 reversed that
/// direction, so the job lands here — and it matters, because the quoted form
/// reaches the parser as a content *string* and never passes the unquoted
/// `try_special_word` probe. Before either mechanism existed, `p="$(status)"`
/// survived as a `Command` no backend recognises as a builtin, was dispatched
/// as an external, and the runtime's "bare command passed to sash_exec"
/// complaint was substituted into the program's output with a **0 exit**.
/// Every compile-time check passed, because `status` really is a registered
/// builtin. Spec `55.55` is the guard.
///
/// `getpid` does not need this: being stateless it keeps its own name all the
/// way to the intrinsic tables, so `try_nullary_intrinsic` claims it before
/// anything can mistake it for an external.
fn nullary_stateful_slot(name: &str, c: &Command) -> Option<&'static str> {
    if !c.args.is_empty() {
        return None;
    }
    stateful_intrinsic_slot(name)
}

/// Backstop (fork B): a fixed-arity value-intrinsic that didn't lower is a
/// malformed (wrong-arity) call. It must NOT fall through to a bare external
/// command — that reaches `sash_exec` and aborts at runtime ("bare command
/// passed to sash_exec"). Yield an empty capture instead; validate (SB4045)
/// reports the arity error under normal compilation, and this keeps the SSA
/// well-formed even when validation is bypassed.
fn wrong_arity_backstop(name: &str, c: &Command) -> Option<SsaValue> {
    let arity = crate::ssa::value_intrinsic_arity(name)?;
    if c.args.len() == arity {
        return None;
    }
    Some(SsaValue::Literal(String::new()))
}

/// `$(f …)` on a known user function. Option C: an `-> int` function
/// *returns* its value, captured as an `IntFuncCall` (identical to `$((f))`,
/// lifted into a temp downstream); any other function's result is its stdout
/// (`FuncCall`).
fn func_call_capture(name: String, c: &Command, state: &VersionState) -> SsaValue {
    // DD#100: split the `=name` collection arguments out before lowering, as
    // `inline_function_call` does — they bind slots rather than contributing to
    // `args`. They ride on the value because the capture is materialized later.
    let (scalar_args, colls) = super::functions::split_call_args(&c.args);
    let args = scalar_args.iter().map(|a| arg_to_value(a, state)).collect();
    if state.function_int_typed.contains(name.as_str()) {
        SsaValue::IntFuncCall { name, args, colls }
    } else {
        SsaValue::FuncCall { name, args, colls }
    }
}

/// Extract `$(<single-command>)` shape: returns the original outer
/// `Statement` (needed to recover any redirects), the head name if
/// it's a bare literal, and the inner `Command`. Returns `None` for
/// anything that isn't a single-command substitution after stripping
/// `Comment` filler and `Redirected` / `Suppressed` wrappers.
pub(super) fn single_command_subst(expr: &Expr) -> Option<(&Statement, Option<String>, &Command)> {
    let ExprKind::CommandSubstitution(stmts) = &expr.kind else {
        return None;
    };
    let real_stmts: Vec<&Statement> = stmts.iter().collect();
    if real_stmts.len() != 1 {
        return None;
    }
    let stmt = real_stmts[0];
    // A redirected capture (`$(cmd > f)`) declines the fast path and falls to
    // `CmdSubstBody`, where the redirect is lowered by the ordinary statement
    // machinery — the same path `$(a | b > f)` has always taken, and the only
    // one both backends implement. The fast path carried its redirects in the
    // SSA node instead, which the C backend discarded outright and native
    // dropped whenever the target was a literal path (its interner never
    // walked them), so `$(/bin/echo hi > f)` captured `hi` and left `f`
    // untouched. Costs no extra fork.
    if matches!(stmt.kind, StmtKind::Redirected(_)) {
        return None;
    }
    let inner = unwrap_subst_wrappers(&stmt.kind);
    let StmtKind::Command(c) = inner else {
        return None;
    };
    let name = match &c.name.kind {
        ExprKind::Literal(n) => Some(n.clone()),
        _ => None,
    };
    Some((stmt, name, c))
}

/// Peel any number of `Redirected` / `Suppressed` wrappers off a
/// `StmtKind`, returning a reference to the innermost kind. Used by
/// `single_command_subst` to look through `$(- foo)` etc.
fn unwrap_subst_wrappers(mut s: &StmtKind) -> &StmtKind {
    loop {
        match s {
            StmtKind::Redirected(r) => s = &r.body.kind,
            StmtKind::Suppressed(inner) => s = &inner.kind,
            other => return other,
        }
    }
}

/// Native-builtin shortcut for `$(name args…)` — the DD#41 collection
/// intrinsics, the DD#65 string sugar (delegated to
/// `try_string_intrinsic`), and echo (DD#14). Returns `None` for any other
/// builtin (or unknown name), letting the caller fall through to the regular
/// `CmdSubst` argv path.
fn try_native_intrinsic(name: &str, c: &Command, state: &VersionState) -> Option<SsaValue> {
    match BuiltinCmd::from_name(name) {
        BuiltinCmd::Len => try_len_intrinsic(c, state),
        BuiltinCmd::Strlen => try_strlen_intrinsic(c, state),
        // `keys` is not a scalar value intrinsic: a value-context `$(keys m)` is
        // rejected SB5025 in validate, and `for … in $(keys m)` uses the splat
        // path — so it falls through here (was a dead `try_keys_intrinsic`).
        BuiltinCmd::Join => Some(join_intrinsic(c, state)),
        BuiltinCmd::Echo => Some(echo_concat(c, state)),
        other => try_string_intrinsic(other, c, state),
    }
}

/// DD#65 string sugar: the intrinsics that lower to a `RegexReplace`,
/// `RegexMatch` or `CaseConvert` value rather than touching a collection.
/// Split from `try_native_intrinsic` because the two families share nothing
/// but the dispatch.
fn try_string_intrinsic(cmd: BuiltinCmd, c: &Command, state: &VersionState) -> Option<SsaValue> {
    match cmd {
        BuiltinCmd::Sub => regex_replace_intrinsic(c, state, false),
        BuiltinCmd::Gsub => regex_replace_intrinsic(c, state, true),
        BuiltinCmd::Match => regex_match_intrinsic(c, state),
        BuiltinCmd::Toupper => case_convert_intrinsic(c, state, true),
        BuiltinCmd::Tolower => case_convert_intrinsic(c, state, false),
        BuiltinCmd::Basename => path_sugar_intrinsic(c, state, ".*/"),
        BuiltinCmd::Dirname => path_sugar_intrinsic(c, state, "/[^/]*$"),
        _ => None,
    }
}

/// DD#65: `$(basename p)` / `$(dirname p)` are **sugar** for a fixed-pattern
/// `sub` — `basename` strips everything up to the last `/` (`.*/` → ``),
/// `dirname` strips the trailing `/component` (`/[^/]*$` → ``). Reuses the
/// `RegexReplace` path (no fork, no taint beyond the operand). `None` on
/// wrong arity (falls through to the generic cmdsub path).
fn path_sugar_intrinsic(c: &Command, state: &VersionState, pattern: &str) -> Option<SsaValue> {
    if c.args.len() != 1 {
        return None;
    }
    Some(SsaValue::RegexReplace {
        subject: Box::new(expr_to_value(&c.args[0], state)),
        pattern: Box::new(SsaValue::Literal(pattern.to_string())),
        replacement: Box::new(SsaValue::Literal(String::new())),
        global: false,
    })
}

/// DD#65: `$(toupper s)` / `$(tolower s)` → `SsaValue::CaseConvert`. The
/// single operand is an ordinary lowered value (quoting/interpolation are
/// normal argument evaluation). `None` (falls through to the generic cmdsub
/// path) if the arity is wrong.
fn case_convert_intrinsic(c: &Command, state: &VersionState, upper: bool) -> Option<SsaValue> {
    if c.args.len() != 1 {
        return None;
    }
    Some(SsaValue::CaseConvert {
        subject: Box::new(expr_to_value(&c.args[0], state)),
        upper,
    })
}

/// DD#63: `$(sub S P R)` / `$(gsub S P R)` → `SsaValue::RegexReplace`.
/// The three operands are ordinary lowered values, so quoting and
/// interpolation are just normal argument evaluation. Returns `None`
/// (falls through to the generic cmdsub path) if the arity is wrong.
fn regex_replace_intrinsic(c: &Command, state: &VersionState, global: bool) -> Option<SsaValue> {
    if c.args.len() != 3 {
        return None;
    }
    Some(SsaValue::RegexReplace {
        subject: Box::new(expr_to_value(&c.args[0], state)),
        pattern: Box::new(expr_to_value(&c.args[1], state)),
        replacement: Box::new(expr_to_value(&c.args[2], state)),
        global,
    })
}

/// DD#63: `$(match S P)` → `SsaValue::RegexMatch` (whole-match capture).
/// The two operands are ordinary lowered values. Returns `None` (falls
/// through to the generic cmdsub path) if the arity is wrong.
fn regex_match_intrinsic(c: &Command, state: &VersionState) -> Option<SsaValue> {
    if c.args.len() != 2 {
        return None;
    }
    Some(SsaValue::RegexMatch {
        subject: Box::new(expr_to_value(&c.args[0], state)),
        pattern: Box::new(expr_to_value(&c.args[1], state)),
    })
}

/// Build the runtime argv: head followed by every argument lowered
/// to an `SsaValue` (preserves the `expr_to_value` shape that
/// `CmdSubst` expects).
fn build_argv(c: &Command, state: &VersionState) -> Vec<SsaValue> {
    let mut argv = Vec::with_capacity(c.args.len() + 1);
    // DD#23: resolve the command head to its absolute path, like direct
    // commands and pipeline stages. Otherwise a bare external name (cat,
    // printf, …) reaches sash_exec unresolved ("bare command passed to
    // sash_exec"). echo/true/false/: stay bare — the cmdsub codegen
    // inlines them.
    let head = match &c.name.kind {
        ExprKind::Literal(name) => {
            SsaValue::Literal(super::resolution::resolve_pipeline_command(name, state))
        }
        _ => expr_to_value(&c.name, state),
    };
    argv.push(head);
    for arg in &c.args {
        argv.push(arg_to_value(arg, state));
    }
    argv
}

/// `$(strlen X)` — byte length of an ordinary value operand. Amends DD#75 §2
/// (`len` dispatched on the operand's type, so *bytes* and *elements* shared
/// one spelling) and DD#102 (the operand is a value, not a name).
///
/// The operand is lowered by `expr_to_value` exactly as `toupper`'s is, so
/// quoting and interpolation are normal argument evaluation: `strlen s` is
/// the length of the string `"s"`, `strlen $s` the length of `s`'s value, and
/// `strlen ${a[0]}` the length of that cell. `None` on wrong arity, which
/// falls through to the generic cmdsub path.
fn try_strlen_intrinsic(c: &Command, state: &VersionState) -> Option<SsaValue> {
    if c.args.len() != 1 {
        return None;
    }
    Some(SsaValue::StrLen {
        subject: Box::new(expr_to_value(&c.args[0], state)),
    })
}

/// The collection NAME a `len` / `join` operand denotes.
///
/// Validate has already fixed the operand to a bare name — every other
/// spelling is rejected, and measured: `len @a`, `join @a ,`, `len @args`,
/// `len @{a:-}`, `len "$s"` and `join $a ,` are SB5062, `len $a` is SB5064.
/// So this reads the name off `expr_to_string` and strips the sigil.
///
/// **There is deliberately no arm for the `@a` splat.** Both call sites used
/// to carry one, as an `unreachable!` citing SB5062 — a duplicate of SB5062's
/// own question about the same `ExprKind` SB5062 had already inspected, which
/// cannot catch a drift the gate would not catch first. Giving it the correct
/// answer instead (`pe.name`, since DD#100 Part D retired `@a` as an exact
/// synonym of the bare name) was no better: the arm stayed unreachable, and an
/// arm handling a rejected spelling tells the next reader that spelling is
/// legitimate (DG, 2026-10-06). The fallback would render `@a` as
/// `${aarrvals}` → `{aarrvals}`, which matches no declared collection; that is
/// unreachable too, and SB5062 is where it is said.
fn collection_operand_name(expr: &Expr) -> String {
    let s = expr_to_string(expr);
    s.strip_prefix('$').unwrap_or(&s).to_string()
}

fn try_len_intrinsic(c: &Command, state: &VersionState) -> Option<SsaValue> {
    // DD#75 §2: dispatch on the operand's TYPE, not the `$` sigil. This
    // retires the old `len a` vs `len $a` syntactic split that made
    // `len <string>` read a non-table (garbage).
    //
    // The `strlen` fallback below does NOT mean "byte length" in general — it
    // means "read the var slot's length field", and `handle_param_expansion`
    // emits exactly that. For a collection that field holds the element
    // count, which is why the branch is correct for the two collection kinds
    // that are absent from `state.collections`: DD#100 parameter slots and
    // the implicit `args`. A statement-level `len <scalar>` never arrives
    // here at all — SB4030 rejects it and points at `strlen`. What remains
    // is the DD#58 arithmetic sexpr `(len s)`, where a string operand is
    // allowed and does yield its byte length.
    let arg = c.args.first()?;
    let name = collection_operand_name(arg);
    // DD#75 §5: a user-written `args` names the implicit per-function array.
    let name = super::expressions::resolve_args_name(&name, state);
    let v = state.lookup(&name);
    let operator = if state.collections.contains(&name) {
        "arrlen"
    } else {
        "strlen"
    };
    Some(SsaValue::ParamExpansion {
        var: v,
        operator: operator.to_string(),
        operand: String::new(),
    })
}

fn join_intrinsic(c: &Command, state: &VersionState) -> SsaValue {
    // DD#75: `join arr` → space-separated; `join arr sep` → sep-separated.
    // Array first, separator second (least-surprise, collection-first —
    // consistent with `len`/`keys`).
    let (arr_expr, sep) = if c.args.len() >= 2 {
        // INVARIANT: validate::flag_join_separator (SB5074) rejects a
        // non-literal separator before transform. The predecessor of this
        // call fell back to the argument's *source text*, so
        // `$(join a "$sep")` silently joined with the four characters
        // `"$sep"` — a wrong answer, not a crash.
        let Some(sep) = crate::ir::literal_word_text(&c.args[1]) else {
            unreachable!("join separator literal guaranteed by validate::flag_join_separator");
        };
        (&c.args[0], sep)
    } else if c.args.len() == 1 {
        (&c.args[0], " ".to_string())
    } else {
        // INVARIANT: validate::check_join_arity (SB5040) rejects a 0-arg (and
        // >2-arg) `$(join …)` before transform, so this is unreachable.
        unreachable!("join arity guaranteed by validate::check_join_arity");
    };
    // The array arg is a bare NAME, or the `@a` synonym DD#100 Part D retired
    // (SB5062); `collection_operand_name` reads both.
    let arr_name = collection_operand_name(arr_expr);
    let arr_name = super::expressions::resolve_args_name(&arr_name, state);
    let v = state.lookup(&arr_name);
    SsaValue::ParamExpansion {
        var: v,
        operator: "arrjoin".to_string(),
        operand: sep,
    }
}

fn echo_concat(c: &Command, state: &VersionState) -> SsaValue {
    if c.args.is_empty() {
        return SsaValue::Literal(String::new());
    }
    let mut parts: Vec<SsaValue> = Vec::new();
    for (i, arg) in c.args.iter().enumerate() {
        if i > 0 {
            parts.push(SsaValue::Literal(" ".to_string()));
        }
        parts.push(expr_to_value(arg, state));
    }
    if parts.len() == 1 {
        parts.remove(0)
    } else {
        SsaValue::Concat(parts)
    }
}
pub(super) fn handle_structured_arithmetic(
    arith: &ArithExpr,
    expr: &Expr,
    state: &VersionState,
) -> SsaValue {
    let v = arith_to_ssa(arith, expr, state);
    // A `${…}` import is the one arith leaf whose value is a *string*
    // (`${x:-0}`/`${x:?msg}` — DD#77's conversion-with-disposition). Every
    // other position forces the conversion: an operator lowers its operands as
    // ints, and an int assignment types the destination. A lone leaf used as a
    // *value* had neither, so `echo "$(( ${x:-7} ))"` emitted the raw string —
    // `abc` where the disposition says 7, and no abort at all for `:?`, which
    // is defined to abort on a non-integer. Re-entering through the operator
    // path is what "evaluate this as arithmetic" means here; `+ 0` is the
    // identity that says it with the machinery that already exists.
    if matches!(strip_arith_parens(arith), ArithExpr::Expansion(_)) {
        return SsaValue::ArithBinOp {
            left: Box::new(v),
            op: ArithOp::Add,
            right: Box::new(SsaValue::Expr("0".to_string())),
        };
    }
    v
}

/// The arithmetic expression inside any redundant grouping — `(${x:-7})` is
/// the same lone leaf as `${x:-7}` and skipped the conversion identically.
fn strip_arith_parens(arith: &ArithExpr) -> &ArithExpr {
    match arith {
        ArithExpr::Paren(inner) => strip_arith_parens(inner),
        other => other,
    }
}

#[allow(clippy::only_used_in_recursion)]
fn arith_to_ssa(arith: &ArithExpr, expr: &Expr, state: &VersionState) -> SsaValue {
    if let Some(v) = arith_leaf_to_ssa(arith, state) {
        return v;
    }
    match arith {
        ArithExpr::BinOp { left, op, right } => binop_to_ssa(left, *op, right, expr, state),
        ArithExpr::UnaryOp { op, operand } => unaryop_to_ssa(*op, operand, expr, state),
        // A mutation (`x = …`, `x += …`, `x++`/`x--`, `a[i] = …`) never reaches
        // the value path: in a `$(( … ))` value it is rejected by validate
        // (SB4020, `check_arith_value_mutation`); in a `(( … ))` statement or
        // condition it is either a top-level statement-mutation
        // (`lower_arith_mutation_structured`) or hoisted into a preceding store
        // by `lower_arith_test_stmt`, which leaves a mutation-free residual
        // here. (Bug #3 fork A — this closed the half-wired value path that
        // used to silently drop the store.)
        ArithExpr::PreIncDec { .. }
        | ArithExpr::PostIncDec { .. }
        | ArithExpr::Assign { .. }
        | ArithExpr::CompoundAssign { .. }
        | ArithExpr::SubscriptAssign { .. } => unreachable!(
            "arith mutation in a value context is rejected by validate (SB4020) \
             or hoisted before reaching the value path"
        ),
        ArithExpr::Ternary {
            cond,
            true_val,
            false_val,
        } => SsaValue::Ternary {
            cond: Box::new(arith_to_ssa(cond, expr, state)),
            true_val: Box::new(arith_to_ssa(true_val, expr, state)),
            false_val: Box::new(arith_to_ssa(false_val, expr, state)),
        },
        ArithExpr::Comma(exprs) => comma_to_ssa(exprs, expr, state),
        ArithExpr::FuncCall { name, args } => funccall_to_ssa(name, args, expr, state),
        ArithExpr::Paren(inner) => arith_to_ssa(inner, expr, state),
        // arith-subscript model B (Stage 2): a bare int-array cell read. The
        // index is a full arith expr → lowered to an int; the read uses it as
        // the integer key (lower_subscript_raw's is_integer_key path calls
        // lower_value_as_int on the index).
        ArithExpr::Subscript { name, index } => subscript_read_to_ssa(name, index, expr, state),
        // Value leaves are handled by `arith_leaf_to_ssa` above, which returns
        // `Some` for all four of them unconditionally — so this arm is
        // unreachable by construction, not by an earlier pass, and no test
        // can call it. Naming the four rather than writing `_` is what earns
        // it: a new `ArithExpr` variant is now a compile error here instead
        // of silently landing on an ICE at runtime.
        ArithExpr::Literal(_)
        | ArithExpr::Var(_)
        | ArithExpr::DollarVar(_)
        | ArithExpr::Expansion(_) => {
            unreachable!("arith value leaf not handled by arith_leaf_to_ssa")
        }
    }
}

/// arith-subscript model B: lower a bare `a[i]` cell read to `SsaValue::
/// Subscript`. The index is the arith-int value; `is_integer_key`/`_value`
/// follow the array's declaration (a map is rejected earlier by type_check).
fn subscript_read_to_ssa(
    name: &str,
    index: &ArithExpr,
    expr: &Expr,
    state: &VersionState,
) -> SsaValue {
    // Resolve the implicit positional array `args` → the per-inline `__argsN`
    // slot, exactly as the word cell-read path does (`subscript_to_value`).
    // Without this, `$(( args[i] ))` reached codegen with the unresolved name
    // and tripped the defensive `find_array_slot` ICE
    // (arith-args-subscript-codegen); the SB5031 false-positive had masked it.
    let resolved = super::expressions::resolve_args_name(name, state);
    SsaValue::Subscript {
        index: Box::new(arith_to_ssa(index, expr, state)),
        is_integer_key: !state.assoc_arrays.contains(&resolved),
        is_integer_value: state.type_map.get(&resolved).copied() == Some(crate::ir::VarType::Int),
        array: resolved,
    }
}

/// The value-producing leaves of an `ArithExpr`: a literal, a bare/`$`-var, or a
/// `${…}` normal-expansion import (`${a[i]}`/`${m[k]}` → the cell via the shared
/// `expr_to_value`). Returns `None` for the operator/mutation/control forms,
/// which the caller's match handles.
fn arith_leaf_to_ssa(arith: &ArithExpr, state: &VersionState) -> Option<SsaValue> {
    match arith {
        ArithExpr::Literal(n) => Some(SsaValue::Expr(n.to_string())),
        // DD#75 §5: `$1`/`$#` in arithmetic mean the same `args` reads as
        // outside `$(( ))` — route positionals through the shared helper first.
        ArithExpr::Var(name) => Some(
            super::expressions::positional_to_value(name, state)
                .unwrap_or_else(|| var_to_ssa(name, state)),
        ),
        ArithExpr::DollarVar(name) => Some(
            super::expressions::positional_to_value(name, state)
                .unwrap_or_else(|| SsaValue::Var(state.lookup(name))),
        ),
        ArithExpr::Expansion(inner) => {
            Some(canonicalize_arith_default(expr_to_value(inner, state)))
        }
        _ => None,
    }
}

/// DD#72: in arith context, a `${s:-LITERAL}` default is used as an int operand.
/// The default is lowered by a decimal conversion at runtime, so rewrite a
/// non-decimal DD#72 literal default (`0xff`, `0o17`, `0b101`) to its decimal
/// form here — the type-check D4 pass accepts exactly these literals. Leaves a
/// non-literal default untouched (a plain decimal is already correct; a
/// non-literal was rejected by D4). Arith-scoped, so a *string*-context
/// `${s:-0xff}` keeps its literal "0xff" (that path never calls this).
fn canonicalize_arith_default(v: SsaValue) -> SsaValue {
    let SsaValue::DefaultValue { source, default } = v else {
        return v;
    };
    let default = match *default {
        SsaValue::Literal(ref s) => match crate::parse::parse_dd72_literal(s) {
            Some(n) => Box::new(SsaValue::Literal(n.to_string())),
            None => default,
        },
        _ => default,
    };
    SsaValue::DefaultValue { source, default }
}

/// In arithmetic context, a bare name is either a known int-typed
/// function (DD#58 nullary call) or a normal variable reference.
fn var_to_ssa(name: &str, state: &VersionState) -> SsaValue {
    if state.function_int_typed.contains(name) {
        return SsaValue::IntFuncCall {
            name: name.to_string(),
            args: vec![],
            colls: vec![],
        };
    }
    SsaValue::Var(state.lookup(name))
}

fn binop_to_ssa(
    left: &ArithExpr,
    op: IrArithOp,
    right: &ArithExpr,
    expr: &Expr,
    state: &VersionState,
) -> SsaValue {
    let l = arith_to_ssa(left, expr, state);
    let r = arith_to_ssa(right, expr, state);
    let ssa_op = ir_op_to_ssa(op);
    if let Some(folded) = try_fold_binop(&l, &r, ssa_op) {
        return folded;
    }
    SsaValue::ArithBinOp {
        left: Box::new(l),
        op: ssa_op,
        right: Box::new(r),
    }
}

/// Constant-fold `int op int` when both sides are `Expr` literals.
fn try_fold_binop(l: &SsaValue, r: &SsaValue, op: ArithOp) -> Option<SsaValue> {
    let (SsaValue::Expr(ls), SsaValue::Expr(rs)) = (l, r) else {
        return None;
    };
    let lv = ls.parse::<i64>().ok()?;
    let rv = rs.parse::<i64>().ok()?;
    Some(SsaValue::Expr(fold_arith_op(lv, rv, op)?.to_string()))
}

fn unaryop_to_ssa(
    op: IrUnaryArithOp,
    operand: &ArithExpr,
    expr: &Expr,
    state: &VersionState,
) -> SsaValue {
    let inner = arith_to_ssa(operand, expr, state);
    if let Some(folded) = try_fold_unop(&inner, op) {
        return folded;
    }
    match op {
        // `-x` → `0 - x`
        IrUnaryArithOp::Neg => SsaValue::ArithBinOp {
            left: Box::new(SsaValue::Expr("0".to_string())),
            op: ArithOp::Sub,
            right: Box::new(inner),
        },
        IrUnaryArithOp::Pos => inner,
        // `~x` → `x ^ -1`
        IrUnaryArithOp::BitNot => SsaValue::ArithBinOp {
            left: Box::new(inner),
            op: ArithOp::BitXor,
            right: Box::new(SsaValue::Expr((-1i64).to_string())),
        },
        // `!x` → `x == 0`
        IrUnaryArithOp::Not => SsaValue::ArithBinOp {
            left: Box::new(inner),
            op: ArithOp::Eq,
            right: Box::new(SsaValue::Expr("0".to_string())),
        },
    }
}

/// Constant-fold a unary arith op when its operand is an `Expr` literal.
fn try_fold_unop(inner: &SsaValue, op: IrUnaryArithOp) -> Option<SsaValue> {
    let SsaValue::Expr(s) = inner else {
        return None;
    };
    let v = s.parse::<i64>().ok()?;
    Some(SsaValue::Expr(fold_unary_op(v, op)?.to_string()))
}

/// The unary counterpart of [`fold_arith_op`]: the *semantics* of `-x`, `+x`,
/// `!x` and `~x` on a known operand, shared with `validate::const_eval_arith`
/// so the two agree about what the folder proves. They did not: validate had
/// no `UnaryOp` arm at all, so `$(( 5 / (1 + -1) ))` reached the runtime while
/// `$(( 5 / (2 - 2) ))` was SB5077 — the same fault, reported at two different
/// times, decided by whether a minus sign appeared.
///
/// `Neg` is checked for the same reason the binary ops are: negating
/// `i64::MIN` overflows, and DD#25 defers an arithmetic fault to the runtime
/// trap rather than folding to a wrapped value.
pub(crate) fn fold_unary_op(v: i64, op: IrUnaryArithOp) -> Option<i64> {
    match op {
        IrUnaryArithOp::Neg => v.checked_neg(),
        IrUnaryArithOp::Pos => Some(v),
        IrUnaryArithOp::BitNot => Some(!v),
        IrUnaryArithOp::Not => Some((v == 0) as i64),
    }
}

fn comma_to_ssa(exprs: &[ArithExpr], expr: &Expr, state: &VersionState) -> SsaValue {
    match exprs.last() {
        Some(last) => arith_to_ssa(last, expr, state),
        None => SsaValue::Expr("0".to_string()),
    }
}

fn funccall_to_ssa(name: &str, args: &[ArithExpr], expr: &Expr, state: &VersionState) -> SsaValue {
    if let Some(v) = try_arith_int_intrinsic(name, args, state) {
        return v;
    }
    if args.is_empty() {
        if let Some(v) = resolve_nullary_sexpr(name, state) {
            return v;
        }
    }
    let ssa_args: Vec<SsaValue> = args.iter().map(|a| arith_to_ssa(a, expr, state)).collect();
    // DD#100: the arith lexer owns `=`, so `$(( (f =a) ))` is SB4020 — an
    // arith-sexpr call can never carry a collection argument.
    SsaValue::IntFuncCall {
        name: name.to_string(),
        args: ssa_args,
        colls: vec![],
    }
}

/// DD#58/#75: an int-returning intrinsic with args (e.g. `len`) called as an
/// arith sexpr `(len a)` must EVALUATE the intrinsic — not flow through the
/// user-int-func path (`inline_int_func`), which finds no user body and returns
/// 0. Reconstruct a `Command` from the sexpr args and reuse the normal builder.
/// Only int-returning intrinsics are intercepted; a string intrinsic stays an
/// IntFuncCall (handled elsewhere). `None` → not this case.
fn try_arith_int_intrinsic(
    name: &str,
    args: &[ArithExpr],
    state: &VersionState,
) -> Option<SsaValue> {
    if !is_arith_int_intrinsic(name) {
        return None;
    }
    let cmd = reconstruct_intrinsic_command(name, args)?;
    try_native_intrinsic(name, &cmd, state)
}

/// The two nullary intrinsics that **read a compiler-managed SSA slot**:
/// `status` → `?` and `getcpid` → `!`. Both are *stateful* — the compiler
/// writes the slot (`?` at every statement boundary, a `return`, a `case`
/// no-match and an inlined int function; `!` when a list statement backgrounds
/// a job) and the intrinsic reads whatever is current. There is no way to
/// compute either from scratch, so the slot read **is** the lowering.
///
/// `getpid` is deliberately **not** here. It is stateless — a `getpid(2)` call
/// — so nothing ever defines a `$` slot for it to read. Mapping it to one was
/// a rename, not a lowering: it carried the intrinsic through SSA under a
/// magic-variable name that four codegen special-cases then had to intercept,
/// which is precisely the direction DD#111 forbids. It now flows as
/// `CmdSubst { argv: ["getpid"] }` like its five stateless peers
/// (`getppid`/`random`/`argc`/`signum`/`epochseconds`), and the intrinsic
/// tables lower it by name.
///
/// DD#111: variables are not magic in sash. These keyword forms are the real
/// thing and `$?`/`$!` are punctuation sugar the *parser* desugars into them,
/// so both spellings arrive here as the same node. This is the single table
/// both callers share — the word form via `resolve_single_cmdsubst` and the
/// arith sexpr (`$(( (status) ))`) via `resolve_nullary_sexpr`.
///
/// They used to reach the same slot by two unrelated routes: the sexpr through
/// this mapping, the word form through a *parser fold* that rewrote
/// `$(status)` back into `$?`. That is how `$$` came to be String-typed while
/// `$(getpid)` was Int — two spellings of one value that disagreed about its
/// type, the same fault `$!` had before `a875d58`.
pub(crate) fn stateful_intrinsic_slot(name: &str) -> Option<&'static str> {
    match name {
        "status" => Some("?"),
        "getcpid" => Some("!"),
        _ => None,
    }
}

/// Resolve a nullary sexpr atom `(name)`. Returns `None` for a known user
/// `-> int` function (the caller inlines it via `IntFuncCall`).
///
/// DD#58/Option C: a NULLARY int intrinsic — `(getpid)`, `(argc)`, … — must
/// EVALUATE (it was silently 0 via the empty-int-func path); an int producer
/// works in both `$(f)` capture and the `$(( (f) ))` sexpr, and intrinsics
/// must not differ from `len`/user int fns. `status`/`getcpid` are *stateful*
/// and reuse the `?`/`!` slots the compiler writes (native + c lower them
/// directly); every stateless one — `getpid` included — reuses the word-form
/// `CmdSubst` node and codegen computes the i64 (native direct via
/// `int_intrinsic_i64`, c via its atoi fallback).
///
/// **A stateless intrinsic missing from that list does not fail loudly.** It
/// falls through to the variable branch below and reads an unset slot: a
/// silent 0. `getcpid` was once excluded and did exactly that, invisibly,
/// because `validate` exempts every intrinsic name from the SB5031 typo check.
/// A string intrinsic here is category B (rejected in type_check); see
/// bug-arith-sexpr-intrinsics-zero.
fn resolve_nullary_sexpr(name: &str, state: &VersionState) -> Option<SsaValue> {
    if let Some(var) = stateful_intrinsic_slot(name) {
        return Some(SsaValue::Var(state.lookup(var)));
    }
    match name {
        "getpid" | "getppid" | "argc" | "random" | "signum" | "epochseconds" => {
            Some(SsaValue::CmdSubst {
                argv: vec![SsaValue::Literal(name.to_string())],
                suppressed: false,
            })
        }
        // `(x)` on a plain variable is arithmetic GROUPING, not a call: the
        // arith parser emits `FuncCall` for every `(name)` and defers the
        // decision here (DD#58). A nullary name that is neither an intrinsic
        // nor a known function is a variable reference — return its value.
        // (Was silently 0 via the empty-int-func path: `$(( (x) ))` → 0.)
        _ if !state.known_functions.contains(name) => Some(SsaValue::Var(state.lookup(name))),
        // A known user `-> int` function → None: the caller emits IntFuncCall.
        _ => None,
    }
}

/// Int-returning intrinsics that evaluate as an arith sexpr operand. `len`
/// (element/entry count) and `strlen` (byte length) are the ones in
/// `try_native_intrinsic`; the string intrinsics (join/keys/toupper/…) are
/// not int and are excluded.
pub(crate) fn is_arith_int_intrinsic(name: &str) -> bool {
    name == "len" || name == "strlen"
}

/// One arith sexpr operand as the `ExprKind` the intrinsic builder expects.
/// `None` for a shape that cannot be rebuilt — `validate::check_arith_sexpr_arity`
/// (SB4045) rejects those, so this never silently yields a wrong answer.
fn intrinsic_operand_kind(a: &ArithExpr) -> Option<ExprKind> {
    Some(match a {
        ArithExpr::Var(n) => ExprKind::Literal(n.clone()),
        ArithExpr::DollarVar(n) => ExprKind::SimpleExpansion(n.clone()),
        ArithExpr::Literal(n) => ExprKind::Literal(n.to_string()),
        // `${a[i]}` / `${m[k]}` — a normal-expansion rvalue imported into
        // arithmetic. `expr_to_value` already lowers the inner `Expr`, so
        // hand it over whole rather than rebuilding it.
        ArithExpr::Expansion(inner) => inner.kind.clone(),
        // `(strlen ($s))` — redundant grouping around an operand.
        ArithExpr::Paren(inner) => return intrinsic_operand_kind(inner),
        _ => return None,
    })
}

/// Rebuild a `Command` for an int intrinsic from its arith sexpr args, so the
/// normal intrinsic builder can consume it. Only simple name/number operands
/// are supported (`len a`, `len $a`); a complex operand returns `None` (the
/// caller falls back to the IntFuncCall path).
///
/// `Var` and `DollarVar` rebuild to *different* expressions. Both spellings
/// name the same variable to `len`, so collapsing them to one `Literal` was
/// lossless while every int intrinsic resolved a name — but DD#102 gave
/// `strlen` a value operand, where `s` is a one-character string and `$s` is
/// the variable's contents. Flattening made `$(( (strlen $s) ))` measure the
/// name.
pub(crate) fn reconstruct_intrinsic_command(name: &str, args: &[ArithExpr]) -> Option<Command> {
    let mut cmd_args = Vec::with_capacity(args.len());
    for a in args {
        cmd_args.push(Expr {
            kind: intrinsic_operand_kind(a)?,
            span: Span::new(0, 0),
        });
    }
    Some(Command {
        name: Expr {
            kind: ExprKind::Literal(name.to_string()),
            span: Span::new(0, 0),
        },
        args: cmd_args,
    })
}

/// Convert IR arithmetic op to SSA arithmetic op.
pub(crate) fn ir_op_to_ssa(op: IrArithOp) -> ArithOp {
    match op {
        IrArithOp::Add => ArithOp::Add,
        IrArithOp::Sub => ArithOp::Sub,
        IrArithOp::Mul => ArithOp::Mul,
        IrArithOp::Div => ArithOp::Div,
        IrArithOp::Mod => ArithOp::Mod,
        IrArithOp::Pow => ArithOp::Pow,
        IrArithOp::Eq => ArithOp::Eq,
        IrArithOp::Ne => ArithOp::Ne,
        IrArithOp::Lt => ArithOp::Lt,
        IrArithOp::Le => ArithOp::Le,
        IrArithOp::Gt => ArithOp::Gt,
        IrArithOp::Ge => ArithOp::Ge,
        IrArithOp::And => ArithOp::And,
        IrArithOp::Or => ArithOp::Or,
        IrArithOp::Shl => ArithOp::Shl,
        IrArithOp::Shr => ArithOp::Shr,
        IrArithOp::BitAnd => ArithOp::BitAnd,
        IrArithOp::BitOr => ArithOp::BitOr,
        IrArithOp::BitXor => ArithOp::BitXor,
    }
}

/// Constant-fold `int op int`. Returns `None` when the result is not a
/// safe compile-time constant — a +,-,* overflow, or an out-of-range
/// shift/pow — so the caller falls back to a runtime `ArithBinOp`, where
/// the DD#25 overflow trap (for +,-,*) applies. This keeps a
/// compile-time-known overflow from silently wrapping
/// (bug-runtime-arith-overflow-wraps) without introducing a new
/// compile-time error.
pub(crate) fn fold_arith_op(l: i64, r: i64, op: ArithOp) -> Option<i64> {
    match op {
        ArithOp::Add => l.checked_add(r),
        ArithOp::Sub => l.checked_sub(r),
        ArithOp::Mul => l.checked_mul(r),
        // DD#25: folding DEFERS a compile-time-known arithmetic fault to the
        // runtime rather than substituting a value. Returning Some(0) for a
        // zero divisor broke that — `$(( 1 / 0 ))` was silently 0 while
        // `int z=0; $(( 1 / z ))` reached the runtime and reported "division
        // by 0". Same fault, same value, but the literal spelling lost the
        // diagnostic. `checked_div`/`checked_rem` already answer `None` for a
        // zero divisor (and for the `i64::MIN / -1` overflow), so an explicit
        // `r == 0` arm only restated the callee's contract.
        ArithOp::Div => l.checked_div(r),
        ArithOp::Mod => l.checked_rem(r),
        ArithOp::Eq => Some((l == r) as i64),
        ArithOp::Ne => Some((l != r) as i64),
        ArithOp::Lt => Some((l < r) as i64),
        ArithOp::Le => Some((l <= r) as i64),
        ArithOp::Gt => Some((l > r) as i64),
        ArithOp::Ge => Some((l >= r) as i64),
        ArithOp::And => Some(((l != 0) && (r != 0)) as i64),
        ArithOp::Or => Some(((l != 0) || (r != 0)) as i64),
        // Unbounded non-negative count (DG 2026-09-20), matching `emit_shift`
        // in both backends. A count >= 64 shifts every bit out: `<<` is 0,
        // `>>` saturates to the sign fill (`l >> 63`). A NEGATIVE count
        // declines to fold and defers to the runtime trap, exactly as a
        // proven overflow does — the folder never produces a value the
        // running program would have refused to produce.
        ArithOp::Shl => (r >= 0).then(|| if r >= 64 { 0 } else { l << r }),
        ArithOp::Shr => (r >= 0).then(|| if r >= 64 { l >> 63 } else { l >> r }),
        ArithOp::BitAnd => Some(l & r),
        ArithOp::BitOr => Some(l | r),
        ArithOp::BitXor => Some(l ^ r),
        ArithOp::Pow => u32::try_from(r).ok().and_then(|e| l.checked_pow(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Span;

    fn lit(s: &str) -> Expr {
        Expr {
            kind: ExprKind::Literal(s.to_string()),
            span: Span::new(1, 1),
        }
    }

    fn state() -> VersionState {
        VersionState::with_types(std::collections::HashMap::new())
    }

    /// A `panic!`/`unreachable!` with a literal message carries a
    /// `&'static str` payload; one with format arguments carries a `String`.
    fn ice_text(err: &Box<dyn std::any::Any + Send>) -> String {
        if let Some(s) = err.downcast_ref::<&'static str>() {
            return (*s).to_string();
        }
        err.downcast_ref::<String>()
            .cloned()
            .expect("ICE payload is a string")
    }

    // A splat operand is SB5062 before transform (measured 2026-10-06:
    // `len @a`, `join @a ,`, `len @args`, `len @{a:-}`, `len @{a:~}` and
    // `"$(len @a)"` all raise it, at the one site that raises it). Both
    // handlers used to carry an arm for it; there is deliberately none now,
    // so there is nothing here to assert. See `collection_operand_name`.

    /// `$(join a "$sep")`. SB5074 rejects a non-literal separator; the
    /// predecessor of this guard fell back to the argument's *source text*
    /// and silently joined with the five characters `"$sep"`.
    #[test]
    fn join_non_literal_separator_is_a_loud_ice() {
        let c = Command {
            name: lit("join"),
            args: vec![
                lit("a"),
                Expr {
                    kind: ExprKind::SimpleExpansion("sep".to_string()),
                    span: Span::new(1, 1),
                },
            ],
        };
        let st = state();
        let caught =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| join_intrinsic(&c, &st)));
        let err = caught.expect_err("a non-literal separator reaching join_intrinsic must ICE");
        let msg = ice_text(&err);
        assert!(
            msg.contains("flag_join_separator"),
            "the ICE must name the validation rule that guarantees it: {msg}"
        );
    }

    /// `$(join)`. SB5040 rejects a 0-arg (and >2-arg) join.
    #[test]
    fn join_zero_arity_is_a_loud_ice() {
        let c = Command {
            name: lit("join"),
            args: vec![],
        };
        let st = state();
        let caught =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| join_intrinsic(&c, &st)));
        let err = caught.expect_err("a zero-arg join reaching join_intrinsic must ICE");
        let msg = ice_text(&err);
        assert!(
            msg.contains("check_join_arity"),
            "the ICE must name the validation rule that guarantees it: {msg}"
        );
    }

    /// Bug #3 fork A: a mutation in a *value* context is either rejected by
    /// SB4020 or hoisted into a preceding store, so the value path never
    /// meets one. The half-wired path this replaced silently dropped the
    /// store — the assignment simply did not happen.
    #[test]
    fn an_arith_mutation_cannot_reach_the_value_path() {
        let cases = [
            ArithExpr::PreIncDec {
                var: "x".to_string(),
                inc: true,
            },
            ArithExpr::PostIncDec {
                var: "x".to_string(),
                inc: true,
            },
            ArithExpr::Assign {
                var: "x".to_string(),
                value: Box::new(ArithExpr::Literal(1)),
            },
            ArithExpr::CompoundAssign {
                var: "x".to_string(),
                op: crate::ir::IrArithOp::Add,
                value: Box::new(ArithExpr::Literal(1)),
            },
            ArithExpr::SubscriptAssign {
                name: "a".to_string(),
                index: Box::new(ArithExpr::Literal(0)),
                op: None,
                value: Box::new(ArithExpr::Literal(1)),
            },
        ];
        let st = state();
        for a in &cases {
            let host = Expr {
                kind: ExprKind::Arithmetic(Box::new(a.clone())),
                span: Span::new(1, 1),
            };
            let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                arith_to_ssa(a, &host, &st)
            }));
            let err = caught.expect_err("a mutation reaching the value path must ICE");
            let msg = ice_text(&err);
            assert!(
                msg.contains("SB4020") || msg.contains("prefix ++/--"),
                "the ICE must name what guarantees it: {msg}"
            );
        }
    }
}
