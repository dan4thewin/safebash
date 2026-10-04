//! SSA taint verification pass.
//!
//! SECURITY: This is the final taint gate before codegen. It operates on
//! the SSA (after all transformations) and verifies that no tainted value
//! reaches a position where it would be executed or written unsanitized.
//!
//! This is the "airbag" — a post-transform safety check that verifies
//! the CST taint analyzer didn't miss anything. If this pass finds a
//! tainted value in a sensitive position, it's an Internal Compiler Error
//! (the CST analyzer should have caught it first).
//!
//! A reviewer should verify:
//! 1. Every SsaInst that USES a value checks taint (not just the ones that define)
//! 2. The taint propagation rules match the CST analyzer's rules
//! 3. `attest` is the ONLY way to move from Tainted → Clean

use crate::ssa::*;
/// tainted value reaching a use site, that's a bug in the CST analyzer.
///
/// The pass walks all SSA instructions and classifies each VarRef version
/// as tainted or clean based on how it was defined:
///   - getenv, argv, read, external cmdsub → tainted
///   - literal, user func, known-safe builtin → clean
///   - attest target → clean (attest source stays tainted)
///   - phi → tainted if any incoming is tainted (conservative)
///   - copy of tainted → tainted
///   - arithmetic on tainted → tainted
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Taint {
    Clean,
    Tainted,
}

/// Run the SSA taint verification pass. Returns a list of error messages
/// (each is a compiler ICE — the CST pass should have caught these).
pub fn verify_ssa_taint(program: &SsaProgram) -> Vec<String> {
    let mut taint: HashMap<String, Taint> = HashMap::new();
    let mut errors = Vec::new();

    seed_clean_specials(&mut taint);
    let user_functions = collect_user_functions(program);
    let diverging = collect_diverging_blocks(program);

    // Phase 1: classify every defined VarRef.
    for block in program.blocks.values() {
        classify_block_defs(block, &mut taint, &user_functions, &diverging);
    }

    // Phase 2: report tainted uses in output/external/index positions.
    for block in program.blocks.values() {
        check_block_uses(block, &taint, &mut errors);
    }

    errors
}

/// DD#121 R2: blocks whose state never arrives anywhere, because they exit.
///
/// The front end's `merge_paths` already drops a diverting path's out-state —
/// "A path that does not fall through (`exit` / `return` / `break` /
/// `continue`) is exempt". The SSA CFG does not model `exit` as a terminator,
/// so the block still appears as a phi predecessor and its (still-tainted)
/// version was unioned in. That made the backstop reject
/// `if attest x …; then :; else exit 1; fi` — a form DD#121's own table names
/// as legal — and **a backstop false positive is a user-facing ICE**.
///
/// Conservative in the same direction as `block_falls_through`: only an
/// `exit` as the block's final instruction counts. `return`/`break`/`continue`
/// are already structural in the SSA CFG and need nothing here.
fn collect_diverging_blocks(
    program: &SsaProgram,
) -> std::collections::HashSet<crate::ssa::BlockId> {
    program
        .blocks
        .values()
        .filter(|b| {
            matches!(
                b.insts.last(),
                Some(SsaInst::Command {
                    kind: crate::ssa::BuiltinCmd::Exit,
                    ..
                })
            )
        })
        .map(|b| b.id)
        .collect()
}

/// Seed the taint map with clean entries for positional params and the
/// surviving sugar specials.
///
/// Special variables are inherently clean — they aren't external data.
/// In sash there are no magic variables: PID and friends are intrinsics.
/// Only `?` (status) and `!` (getcpid) survive as slots, and only because
/// both are compiler-managed *state* that the intrinsic reads — `?` is
/// scoped per DD#55. `#` (argc) and `0` (argv 0) are process metadata — clean.
///
/// `$` is deliberately absent. `getpid` is stateless, so it lowers as an
/// intrinsic and no slot by that name is ever created; seeding one would be
/// seeding a variable that cannot exist.
///
/// Positional params at version 0 are clean — either program argv
/// (which the CST pass gates) or function args (clean by DD#24).
fn seed_clean_specials(taint: &mut HashMap<String, Taint>) {
    for sv in &["?", "!", "#", "0"] {
        taint.insert(format!("{}_0", sv), Taint::Clean);
    }
    for i in 1..=9 {
        taint.insert(format!("{}_0", i), Taint::Clean);
    }
    taint.insert("@_0".to_string(), Taint::Clean);
    taint.insert("*_0".to_string(), Taint::Clean);
}

fn collect_user_functions(program: &SsaProgram) -> std::collections::HashSet<String> {
    let mut user_functions = std::collections::HashSet::new();
    for block in program.blocks.values() {
        if let Some(name) = block.label.strip_prefix("fn_") {
            user_functions.insert(name.to_string());
        }
    }
    user_functions
}

/// Phase-1 per-block driver: process phi nodes, then dispatch each
/// instruction to `classify_inst_def`.
fn classify_block_defs(
    block: &BasicBlock,
    taint: &mut HashMap<String, Taint>,
    user_functions: &std::collections::HashSet<String>,
    diverging: &std::collections::HashSet<crate::ssa::BlockId>,
) {
    // Phi nodes — optimistic: default to clean, will be corrected by
    // iteration if any incoming is tainted. Since the CST pass is the
    // authority, false-clean here just means we don't catch a CST bug
    // involving loops — acceptable for a backstop.
    //
    // A source from a diverging predecessor is skipped: that path exits, so its
    // version never reaches this merge. See `collect_diverging_blocks`.
    for phi in &block.phis {
        let key = var_key(&phi.target);
        let t = phi
            .sources
            .iter()
            .filter(|(bid, _)| !diverging.contains(bid))
            .any(|(_bid, vref)| matches!(taint.get(&var_key(vref)), Some(&Taint::Tainted)));
        taint.insert(key, if t { Taint::Tainted } else { Taint::Clean });
    }
    for inst in &block.insts {
        classify_inst_def(inst, taint, user_functions);
    }
}

/// Classify the VarRef(s) defined by one instruction. Instructions that
/// define nothing (Command, Test, Pipeline, …) are no-ops.
fn classify_inst_def(
    inst: &SsaInst,
    taint: &mut HashMap<String, Taint>,
    user_functions: &std::collections::HashSet<String>,
) {
    match inst {
        SsaInst::Assign { target, value } => {
            let t = value_is_tainted(value, taint, user_functions);
            taint.insert(var_key(target), t);
        }
        SsaInst::Attest { target, .. } => {
            // Attest produces a clean version.
            taint.insert(var_key(target), Taint::Clean);
        }
        SsaInst::AttestRetag { target, .. } => {
            // DD#61: attest-all produces a clean version of the array/map.
            taint.insert(var_key(target), Taint::Clean);
        }
        SsaInst::ReadLine {
            target,
            clean_pipe,
            ..
        } => {
            taint.insert(var_key(target), read_line_taint(*clean_pipe));
        }
        SsaInst::CaptureEnd { target, tainted, .. } => {
            // A `$(user_fn)` capture is optimistically Clean (DD#24); a
            // multi-statement/pipeline `$(…)` body captures external output
            // and is Tainted. The upstream CST taint pass
            // (`cmdsub_is_taint_source`) is the authoritative gate; this keeps
            // the SSA taint view correct (defense-in-depth).
            taint.insert(var_key(target), taint_of_flag(*tainted));
        }

        SsaInst::ArgvFill { target, .. } => {
            // DD#60/#61: argv is a bulk-read source — always tainted.
            taint.insert(var_key(target), Taint::Tainted);
        }
        SsaInst::KeysFill { target, .. } => {
            // DD#75 §3 / DD#93: the keys of a map are always clean. Every
            // route to a tainted map is closed at construction, and a tainted
            // value reaches a subscript only through `attest`, which cleans
            // it — so the source map has no tainted key to hand over. This is
            // deliberately NOT the `ArgvFill` rule above: argv's bytes come
            // from outside the program, a map's keys were put there by it.
            taint.insert(var_key(target), Taint::Clean);
        }
        SsaInst::ReadArray {
            target, clean_pipe, ..
        } => {
            // Same rule as `ReadLine`: clean only from a compiler-computed pipe.
            taint.insert(var_key(target), read_line_taint(*clean_pipe));
        }
        SsaInst::SplitValue { source, target } => {
            // DD#99 `read $SRC ARR` — the array is as tainted as the scalar
            // whose bytes it splits.
            let t = value_is_tainted(source, taint, user_functions);
            taint.insert(var_key(target), t);
        }
        SsaInst::MapFromArray {
            source,
            target,
            taint_off,
            ..
        } => {
            let t = map_fill_taint(source, *taint_off, taint);
            taint.insert(var_key(target), t);
        }

        // Instructions that don't define a VarRef. Capture-form builtins are
        // governed by the CST pass; the SSA backstop does not yet re-derive
        // them (see task ssa-taint-cmdsub-operand-propagation).
        SsaInst::Command { .. }
        | SsaInst::Test(..)
        | SsaInst::SubscriptAssign { .. }
        | SsaInst::ArrayAppend { .. }
        | SsaInst::MatchInto { .. } // fills a clean array (subject is clean); CST pass governs array taint
        | SsaInst::CaptureStart { .. }
        | SsaInst::AttestAll { .. } // validates in place; AttestRetag defines the clean version
        | SsaInst::RedirectPush { .. } // moves fds around; defines no value
        | SsaInst::RedirectPop { .. }
        | SsaInst::Pipeline { .. } => {}
    }
}

/// A `read` is clean only when it draws from a clean pipe (data flowing from
/// another sash function). Reading from stdin or a file redirect is external
/// input → tainted. (sash has no here-strings; that clean-literal source is
/// gone — DD#83.)
fn read_line_taint(clean_pipe: bool) -> Taint {
    if clean_pipe {
        Taint::Clean
    } else {
        Taint::Tainted
    }
}

/// Phase-2 per-block driver: dispatch each instruction to `check_inst_uses`.
fn check_block_uses(block: &BasicBlock, taint: &HashMap<String, Taint>, errors: &mut Vec<String>) {
    for inst in &block.insts {
        check_inst_uses(inst, taint, errors);
    }
}

/// Phase 2: any instruction reading a tainted value in a position that
/// would produce output, execute an external, escape to a file descriptor,
/// or enter an index expression is an ICE.
///
/// The enumeration of read positions is **not** here: it is
/// [`SsaInst::taint_reads`], which is total over `SsaInst` with no wildcard.
/// This function only decides what to do with each one. The hand-written
/// four-arm match that used to live here ended in `_ => {}` and is how
/// `SsaInst::Test` and `SsaInst::RedirectPush` escaped the backstop for the
/// whole life of the pass.
fn check_inst_uses(inst: &SsaInst, taint: &HashMap<String, Taint>, errors: &mut Vec<String>) {
    for (read, context) in inst.taint_reads() {
        match read {
            TaintRead::Value(v) => check_value_use(v, taint, errors, &context),
            TaintRead::RedirectTarget(e) => check_redirect_target(e, taint, errors, &context),
        }
    }
}

/// A redirect target rides through SSA as an unlowered `ir::Expr` — there is
/// no `SsaValue` to check — so its variable reads are resolved by NAME.
///
/// `taint_of_name` takes the highest classified version, so this is
/// deliberately imprecise in the permissive direction: a variable attested
/// *after* the redirect reads clean here. The front-end pass is the precise
/// one and reports with a span; this is the backstop that makes the position
/// non-silent rather than a second opinion.
fn check_redirect_target(
    target: &crate::ir::Expr,
    taint: &HashMap<String, Taint>,
    errors: &mut Vec<String>,
    context: &str,
) {
    let mut names: Vec<String> = Vec::new();
    crate::ir::walk_expr_tree(target, &mut |e| collect_expr_var_name(e, &mut names));
    for name in names {
        if taint_of_name(taint, &name) == Taint::Tainted {
            errors.push(format!(
                "tainted variable ${} used as {} without attest (SSA taint backstop)",
                name, context
            ));
        }
    }
}

/// The variable this expression node names, if any. Called once per node by
/// `walk_expr_tree`, so it must not recurse itself. Names are cloned because
/// `walk_expr_tree`'s callback takes its node under an anonymous lifetime.
///
/// **Total over `ExprKind` on purpose — do not add a wildcard.**
fn collect_expr_var_name(e: &crate::ir::Expr, out: &mut Vec<String>) {
    use crate::ir::ExprKind;
    match &e.kind {
        ExprKind::SimpleExpansion(name) => out.push(name.clone()),
        ExprKind::ParameterExpansion(pe) => {
            // DD#91: `${x:!}` is a sanctioned per-use bypass. The author has
            // vouched for this read; do not second-guess it here.
            if !pe.taint_off {
                out.push(pe.name.clone());
            }
        }
        ExprKind::Subscript { name, .. } => out.push(name.clone()),
        // Composites: `walk_expr_tree` visits their children for us.
        ExprKind::DoubleQuoted(_)
        | ExprKind::Concat(_)
        | ExprKind::Array(_)
        | ExprKind::AssocArray(_)
        | ExprKind::SlurpFile(_)
        | ExprKind::Arithmetic(_)
        | ExprKind::Test(_)
        | ExprKind::CommandSubstitution(_) => {}
        // Leaves with no variable read.
        ExprKind::Literal(_) | ExprKind::SingleQuoted(_) => {}
    }
}

fn var_key(vref: &VarRef) -> String {
    format!("{}_{}", vref.name, vref.version)
}

fn is_tainted(taint: &HashMap<String, Taint>, vref: &VarRef) -> bool {
    match taint.get(&var_key(vref)) {
        Some(&Taint::Tainted) => true,
        Some(&Taint::Clean) => false,
        None => false, // DD#53: unknown vars are empty strings, not tainted
    }
}

/// Taint of an array referenced by bare name (no version). Array reads
/// resolve to the latest SSA version at runtime, so we report the taint
/// of the highest-numbered classified version of `name`. Used for
/// subscript / table-iteration reads (DD#61).
fn taint_of_name(taint: &HashMap<String, Taint>, name: &str) -> Taint {
    let prefix = format!("{}_", name);
    let mut best: Option<(u32, Taint)> = None;
    for (k, t) in taint {
        if let Some(ver) = k.strip_prefix(&prefix).and_then(|v| v.parse::<u32>().ok()) {
            if best.is_none_or(|(bv, _)| ver >= bv) {
                best = Some((ver, *t));
            }
        }
    }
    best.map(|(_, t)| t).unwrap_or(Taint::Clean)
}

fn value_is_tainted(
    value: &SsaValue,
    taint: &HashMap<String, Taint>,
    user_functions: &std::collections::HashSet<String>,
) -> Taint {
    match value {
        // Always-clean leaves: pure-computation values that the
        // lowerer guarantees do not carry external input.
        SsaValue::Literal(_)
        | SsaValue::Expr(_)
        | SsaValue::FuncCall { .. }
        | SsaValue::IntFuncCall { .. } => Taint::Clean,

        // Array reads inherit the named array's taint (DD#61). The array
        // name is unversioned in SSA; reads resolve to the latest version,
        // so we consult the latest classified version of that name.
        SsaValue::Subscript { array, .. }
        | SsaValue::TableValue { array, .. }
        | SsaValue::TableKey { array, .. } => taint_of_name(taint, array),

        // DD#91: `${x:~}` carries the source's taint; the `:!` spelling
        // (taint_off) is a sanctioned per-use bypass → always Clean.
        SsaValue::ElideEmpty { source, taint_off } => {
            elide_taint(source, *taint_off, taint, user_functions)
        }
        // DD#91: `${x:!}` — a sanctioned per-use bypass, so Clean regardless of
        // the source. The CST pass has already allowed it; this keeps the
        // backstop in agreement instead of overriding the author.
        SsaValue::Trusted { .. } => Taint::Clean,
        // DD#91: `@{a:!DISP}` (disposition.taint_off) is a sanctioned
        // per-use attest-all bypass → Clean; any other splat inherits the array's
        // taint.
        SsaValue::ArraySplat { array, disposition } => {
            splat_taint(array, disposition, taint, user_functions)
        }

        // Always-tainted: program argv is external input by construction; a
        // multi-statement/pipeline `$(…)` body captures external output; and
        // `$(<file)` is the contents of a file, which is external input in
        // exactly the same sense.
        //
        // The slurp case must be asserted here rather than inherited. It used
        // to desugar to `$(cat file)` and so was tainted only as a *side
        // effect* of being an external-command capture. Lowering it to a direct
        // read leaves that path, and note what is NOT wanted: the child value
        // is the *path*, so falling through to the any-tainted composite rule
        // below would make `$(<f.txt)` — a literal path — read CLEAN. That is a
        // hole straight through the taint model, which is why this variant is
        // listed explicitly and why the match is exhaustive.
        SsaValue::ArgvIndex(_) | SsaValue::CmdSubstBody { .. } | SsaValue::SlurpFile(_) => {
            Taint::Tainted
        }

        // Var-rooted: taint inherited from the named variable's status.
        SsaValue::Var(vref) | SsaValue::ArithExpr { var: vref, .. } => taint_of_var(taint, vref),

        // DD#60: a length/count operator measures the variable rather than
        // reading it, so it is clean even on a tainted one — the same rule
        // `check_param_expansion_use` applies at a sink, and the same verdict
        // `cmd_subst_taint` gives `len`/`strlen` spelled as a capture. Before
        // the bulk fills carried taint this arm was unreachable for a
        // collection, which is how the two sides came to disagree.
        SsaValue::ParamExpansion {
            var: vref,
            operator,
            ..
        } => param_expansion_taint(vref, operator, taint),

        // Any-tainted composites: tainted iff any owned sub-value is tainted.
        // `child_values()` yields exactly the taint-carrying operands, so the
        // array/assoc/arith/regex/case/ternary and DD#65 `:-` default all reduce
        // to the same rule (`default_value_taint` is `any_tainted([src,def])`).
        SsaValue::Concat(_)
        | SsaValue::IndexedArray(_)
        | SsaValue::AssocArray(_)
        | SsaValue::ArithBinOp { .. }
        | SsaValue::RegexReplace { .. }
        | SsaValue::RegexMatch { .. }
        | SsaValue::CaseConvert { .. }
        | SsaValue::StrLen { .. }
        | SsaValue::Ternary { .. }
        | SsaValue::DefaultValue { .. } => any_tainted(value.child_values(), taint, user_functions),

        // Wrapped-value taint: an int cell's taint is its wrapped value's
        // (DD#76), and `${…:?msg}` yields the source (the message only prints on
        // the abort path, which exits), so its taint is exactly the source's —
        // the source being a plain var (scalar) or a cell read (`${a[i]:?…}`).
        SsaValue::IntCell(inner)
        | SsaValue::RequireValue { source: inner, .. }
        | SsaValue::Substring { source: inner, .. } => {
            value_is_tainted(inner, taint, user_functions)
        }

        // Command substitution: the only variant whose taint
        // depends on the callee (builtins are clean, externals
        // tainted, user fns clean by DD#24).
        SsaValue::CmdSubst { argv, .. } => cmd_subst_taint(argv, taint, user_functions),
    }
}

/// DD#84/#91: taint of `${x:~}` / `${x:!}`. The `:!` spelling (`taint_off`) is a
/// sanctioned per-use bypass → Clean; `:~` carries the source's taint.
fn elide_taint(
    source: &SsaValue,
    taint_off: bool,
    taint: &HashMap<String, Taint>,
    user_functions: &std::collections::HashSet<String>,
) -> Taint {
    if taint_off {
        Taint::Clean
    } else {
        value_is_tainted(source, taint, user_functions)
    }
}

/// DD#99/#91: taint of `map m = @{a:DISP}`. The map is as tainted as its
/// source array (resolved by name — arrays read at their latest version),
/// unless the `!` spelling vouched for the read, which answers Clean exactly
/// as `splat_taint` does for `@a:!`.
///
/// Unlike `splat_taint` this needs no word carve-out. A splat's `:-WORD` is a
/// value the splat can yield, so `!` must not vouch for it; a map fill's
/// `:-KEY` and `:?MSG` are literals (SB4070), so neither can carry taint in
/// the first place.
fn map_fill_taint(source: &str, taint_off: bool, taint: &HashMap<String, Taint>) -> Taint {
    if taint_off {
        Taint::Clean
    } else {
        taint_of_name(taint, source)
    }
}

/// DD#61/#91: taint of an `@array` splat. `@a:!` (`disposition.taint_off`) is a
/// sanctioned per-use attest-all bypass for the ARRAY's own read; any other
/// splat inherits the array's taint.
///
/// The `:-WORD` default is a value the splat can yield, so its taint counts
/// too. `!` does not vouch for it: DD#91 scopes `!` to its own subject's read,
/// and the word is an unrelated expression. Ignoring it meant
/// `array b=( @{a:-$tainted} )` classified `b` CLEAN — measured, and masked
/// only by the front-end pass (SB5021).
///
/// A `:?` word is an abort MESSAGE, not a value. The abort path exits, so it
/// yields nothing and contributes no taint — exactly as `RequireValue`'s
/// message does not.
fn splat_taint(
    array: &str,
    disposition: &Option<crate::ssa::ArraySplatDisposition>,
    taint: &HashMap<String, Taint>,
    user_functions: &std::collections::HashSet<String>,
) -> Taint {
    let vouched = disposition.as_ref().is_some_and(|d| d.taint_off);
    if !vouched && taint_of_name(taint, array) == Taint::Tainted {
        return Taint::Tainted;
    }
    disposition
        .as_ref()
        .map_or(Taint::Clean, |d| splat_word_taint(d, taint, user_functions))
}

/// The taint a splat inherits from its own `:-WORD` default. A `:?` word is an
/// abort MESSAGE and a wordless `@{a:-}` has only a placeholder, so neither
/// yields a value and neither contributes taint.
fn splat_word_taint(
    d: &crate::ssa::ArraySplatDisposition,
    taint: &HashMap<String, Taint>,
    user_functions: &std::collections::HashSet<String>,
) -> Taint {
    if d.has_word && !d.abort {
        value_is_tainted(&d.word, taint, user_functions)
    } else {
        Taint::Clean
    }
}

/// Lift `is_tainted` (which returns bool) to the Taint enum used
/// here, with DD#53 semantics for unknown vars (treated as Clean).
fn taint_of_var(taint: &HashMap<String, Taint>, vref: &VarRef) -> Taint {
    if is_tainted(taint, vref) {
        Taint::Tainted
    } else {
        Taint::Clean
    }
}

/// `Taint::Tainted` if any of `values` is tainted, else `Taint::Clean`.
fn any_tainted<'a>(
    values: impl IntoIterator<Item = &'a SsaValue>,
    taint: &HashMap<String, Taint>,
    user_functions: &std::collections::HashSet<String>,
) -> Taint {
    if values
        .into_iter()
        .any(|v| value_is_tainted(v, taint, user_functions) == Taint::Tainted)
    {
        Taint::Tainted
    } else {
        Taint::Clean
    }
}

/// Compute the taint of a `$(...)` command substitution.
///
/// - `getenv` / `argv` / `arg` — external input, always tainted.
/// - Pure generators (`argc`, `epochseconds`, `random`, `getpid`, `getcwd`,
///   …) and `len` (cardinality only, DD#24-exempt) — always clean.
/// - **Pass-through transforms** (`echo`) — the output derives
///   from the operands, so it is tainted iff any operand is, exactly like every
///   other transform value (`Concat`, `RegexReplace`, …). (`join`/`keys` never
///   reach here: the transform lowers them to intrinsics, not a command
///   capture, so a bare `join`/`keys` argv can't appear in SSA.)
/// - User-defined functions — clean by DD#24.
/// - Everything else runs via execve/execveat — tainted.
fn cmd_subst_taint(
    argv: &[SsaValue],
    taint: &HashMap<String, Taint>,
    user_functions: &std::collections::HashSet<String>,
) -> Taint {
    // INVARIANT: the head is always a `Literal`, and argv is never empty.
    // SB5011 rejects a non-literal command name ("a command name must be a
    // literal path") in every position, and SB4045 rejects a zero-argument
    // argv; the two producers that don't go through `build_argv`
    // (`resolve_nullary_sexpr`, `lower_for_argv_slice`) hard-code a
    // `Literal` head. This returned `Taint::Tainted` until 2026-10-04 —
    // fail-closed, and the right *value* if it were ever reached, but an
    // unverified claim about a pass that is the actual defense. Probed
    // 2026-10-04: `$($c)` and `$("$c")` are both SB5011.
    let Some(SsaValue::Literal(cmd)) = argv.first() else {
        unreachable!("SB5011 guarantees a literal command head, SB4045 a non-empty argv");
    };
    let basename = cmd.rsplit('/').next().unwrap_or(cmd);
    match basename {
        "getenv" | "argv" | "arg" => Taint::Tainted,
        "len" | "strlen" | "epochseconds" | "random" | "argc" | "getpid" | "getppid"
        | "getcpid" | "getcwd" | "status" | "signum" => Taint::Clean,
        "echo" => any_tainted(&argv[1..], taint, user_functions),
        _ => {
            // Only a BARE name can be a user function: DD#23 resolution
            // rewrites an external head to an absolute path and nothing else
            // acquires a slash, so `cmd` is already the name to look up.
            //
            // Do NOT fall back to the basename. `/bin/cat` alongside a user
            // function named `cat` would match and be classified Clean — a
            // real execve laundered by a name collision, exactly contrary to
            // "everything else runs via execve/execveat" above.
            //
            // The `cmd` test is never true: a user-function capture is
            // lowered elsewhere and never reaches this arm, so the `Clean`
            // this used to return was an answer nothing could ask for. It
            // became an ICE on 2026-10-04. Measured, not assumed — the arm
            // was made to panic and eight capture spellings were run against
            // it: `$(f)`, `$(- f)`, `$(f 2>&1)` and `$(strlen "$(f)")` all
            // compile without reaching it, and `$(f x)`, `$(f | cat)`,
            // `if o=$(f)` and `for x in $(f)` are rejected upstream
            // (SB5043 / SB5020 / SB4025 / SB5009).
            if user_functions.contains(cmd.as_str()) {
                unreachable!(
                    "a user-function capture is lowered before SSA taint, not as CmdSubst"
                );
            }
            Taint::Tainted
        }
    }
}

/// Five `SsaValue` shapes are built *only* as the value of an `SsaInst::Assign`
/// — `ArithExpr`/`ArgvIndex`/`TableValue`/`TableKey` at the eight loop-lowering
/// sites in `transform/pipelines.rs`, and `AssocArray` from
/// `parse::parse_array_literal`, which only the two assignment parsers call.
/// `Assign` reports no `taint_reads` (see `SsaInst::taint_reads` in `ssa.rs`,
/// "Propagation and definition sites"), so none of them can reach this walk.
///
/// They are therefore unreachable *by construction*, not merely unexercised:
/// opening a route needs a change to the compiler's own source, so this can
/// only ever fire in the tree of whoever opened it. An untested "correct"
/// recursion arm would instead let that route open in silence — the same shape
/// as the `_ => {}` that let `SsaInst::Test` and `RedirectPush` escape this
/// pass for its whole life, and silence is the one failure mode a taint
/// reporter cannot afford.
///
/// `value_is_tainted` keeps real arms for all five: it is the *derivation*
/// half and is reached for `Assign` values. That asymmetry is the subject of
/// `notes/tasks/ssa-taint-full-backstop.md`.
const ASSIGN_ONLY_VALUE: &str = "ICE: an Assign-only SsaValue reached check_value_use — \
     a new read route was opened without extending the taint reporter";

/// Report every tainted read inside one value used at a sink.
///
/// **Total over `SsaValue` on purpose — do not add a wildcard.** The `_ => {}`
/// this replaced covered 8 of the 30 variants; the other 22 read as clean
/// whatever they held. The arms are grouped to mirror `value_is_tainted`
/// above, which has always been exhaustive — the two are the derivation and
/// the reporting half of one model and should be read side by side.
///
/// The recursion is written out rather than delegated to
/// `SsaValue::child_values()`. That getter is now total too, so the original
/// reason (it ended in a wildcard) is gone — but the arms here are not a
/// uniform "visit every child": `Trusted` and `StrLen` stop, `ElideEmpty`
/// descends only when the author has not vouched, and five shapes are an ICE.
/// Delegating would flatten those decisions into one.
fn check_value_use(
    value: &SsaValue,
    taint: &HashMap<String, Taint>,
    errors: &mut Vec<String>,
    context: &str,
) {
    match value {
        // ── Var-rooted reads ────────────────────────────────────────────
        SsaValue::Var(vref) => check_tainted_var(vref, "", context, taint, errors),
        SsaValue::ParamExpansion { var, operator, .. } => {
            check_param_expansion_use(var, operator, taint, errors, context)
        }

        // ── Unreachable by construction (see `ASSIGN_ONLY_VALUE`) ───────
        SsaValue::ArithExpr { .. }
        | SsaValue::ArgvIndex(_)
        | SsaValue::TableValue { .. }
        | SsaValue::TableKey { .. }
        | SsaValue::AssocArray(_) => unreachable!("{ASSIGN_ONLY_VALUE}"),

        // ── Collection reads, resolved by bare name (DD#61) ─────────────
        SsaValue::Subscript { array, index, .. } => {
            check_tainted_array(array, context, taint, errors);
            check_value_use(index, taint, errors, context);
        }
        SsaValue::ArraySplat { array, disposition } => {
            check_splat_use(array, disposition, taint, errors, context)
        }

        // ── DD#91 sanctioned bypasses: the author has vouched ───────────
        SsaValue::Trusted { .. } => {}
        SsaValue::ElideEmpty { source, taint_off } => {
            if !taint_off {
                check_value_use(source, taint, errors, context);
            }
        }

        // ── DD#60: length exposes no element data ───────────────────────
        // Asymmetric with `value_is_tainted`, which treats `StrLen` as
        // tainted-if-subject-is. That direction is conservative (it over-taints
        // a derived value); rejecting here would contradict DD#60 and the
        // front-end pass, and a backstop false positive is a user-facing ICE.
        SsaValue::StrLen { .. } => {}

        // ── Leaves with no read ─────────────────────────────────────────
        SsaValue::Literal(_) | SsaValue::Expr(_) => {}

        // The body's own reads are an open front-end gap
        // (`notes/tasks/bug-taint-escape-in-cmdsub-body.md`), not something
        // this value-oriented walk can see: `body` is `Vec<ir::Statement>`,
        // never lowered to SSA. Listed explicitly so it stays visible.
        SsaValue::CmdSubstBody { .. } => {}

        // ── Composites: every child is a read at the same sink ──────────
        // `SlurpFile`'s child is the PATH — `$(<$h)` reads a tainted filename,
        // which is an escape even though the slurp's own taint is asserted
        // rather than inherited (see `value_is_tainted`).
        SsaValue::SlurpFile(inner)
        | SsaValue::IntCell(inner)
        | SsaValue::CaseConvert { subject: inner, .. }
        | SsaValue::Substring { source: inner, .. } => {
            check_value_use(inner, taint, errors, context)
        }
        SsaValue::Concat(parts) | SsaValue::IndexedArray(parts) => {
            check_each(parts, taint, errors, context)
        }
        SsaValue::CmdSubst { argv: args, .. }
        | SsaValue::FuncCall { args, .. }
        | SsaValue::IntFuncCall { args, .. } => check_each(args, taint, errors, context),
        SsaValue::ArithBinOp { left, right, .. }
        | SsaValue::RegexMatch {
            subject: left,
            pattern: right,
        }
        | SsaValue::DefaultValue {
            source: left,
            default: right,
        }
        | SsaValue::RequireValue {
            source: left,
            message: right,
        } => {
            check_value_use(left, taint, errors, context);
            check_value_use(right, taint, errors, context);
        }
        SsaValue::RegexReplace {
            subject,
            pattern,
            replacement,
            ..
        } => {
            check_value_use(subject, taint, errors, context);
            check_value_use(pattern, taint, errors, context);
            check_value_use(replacement, taint, errors, context);
        }
        SsaValue::Ternary {
            cond,
            true_val,
            false_val,
        } => {
            check_value_use(cond, taint, errors, context);
            check_value_use(true_val, taint, errors, context);
            check_value_use(false_val, taint, errors, context);
        }
    }
}

/// DD#60: a length/count operator (`${#a[@]}` / `len a`, `${#s}`) exposes
/// only the element count or byte length, not any tainted element data — so
/// it is clean even on a tainted array.
///
/// Shared by both taint sides on purpose. `check_param_expansion_use` gates a
/// sink and `value_is_tainted` gates propagation; when only the former knew
/// the rule, `n=$(len a)` on a tainted array made `n` tainted while the
/// direct `echo $(len a)` passed.
fn is_measurement_op(operator: &str) -> bool {
    operator == "arrlen" || operator == "strlen"
}

/// The propagation half of the DD#60 rule above: a measurement yields a count,
/// not the measured bytes, so it is Clean regardless of the variable's status.
fn param_expansion_taint(var: &VarRef, operator: &str, taint: &HashMap<String, Taint>) -> Taint {
    if is_measurement_op(operator) {
        Taint::Clean
    } else {
        taint_of_var(taint, var)
    }
}

/// `CaptureEnd.tainted` is decided at transform time (external -> true,
/// builtin -> false); this is only the bool-to-`Taint` lift.
fn taint_of_flag(tainted: bool) -> Taint {
    if tainted {
        Taint::Tainted
    } else {
        Taint::Clean
    }
}

fn check_param_expansion_use(
    var: &VarRef,
    operator: &str,
    taint: &HashMap<String, Taint>,
    errors: &mut Vec<String>,
    context: &str,
) {
    if !is_measurement_op(operator) {
        check_tainted_var(var, " in param expansion", context, taint, errors);
    }
}

/// DD#91: `@{a:!DISP}` is a sanctioned per-use attest-all bypass, matching
/// `splat_taint`. The disposition WORD is still checked either way — `!`
/// vouches for the splat's own read, not for an unrelated variable spliced
/// into the default.
fn check_splat_use(
    array: &str,
    disposition: &Option<crate::ssa::ArraySplatDisposition>,
    taint: &HashMap<String, Taint>,
    errors: &mut Vec<String>,
    context: &str,
) {
    if !disposition.as_ref().is_some_and(|d| d.taint_off) {
        check_tainted_array(array, context, taint, errors);
    }
    // `has_word` false means `@{a:-}` / `@{a:~}` — the `word` field is a
    // placeholder, not something the program wrote. A `:?` message IS read
    // (it reaches stderr on the abort path), so `abort` is not excluded here.
    if let Some(d) = disposition.as_ref().filter(|d| d.has_word) {
        check_value_use(&d.word, taint, errors, context);
    }
}

/// Every value in one list, reported under a shared context.
fn check_each(
    values: &[SsaValue],
    taint: &HashMap<String, Taint>,
    errors: &mut Vec<String>,
    context: &str,
) {
    for v in values {
        check_value_use(v, taint, errors, context);
    }
}

/// Emit the backstop ICE if `vref` is tainted at a use site. `where_` is
/// an optional clause (e.g. " in arithmetic") describing the position.
fn check_tainted_var(
    vref: &VarRef,
    where_: &str,
    context: &str,
    taint: &HashMap<String, Taint>,
    errors: &mut Vec<String>,
) {
    if is_tainted(taint, vref) {
        errors.push(format!(
            "tainted variable {}_{} used{} as {} without attest (SSA taint backstop)",
            vref.name, vref.version, where_, context
        ));
    }
}

/// Emit the backstop ICE if array `array` is tainted at a subscript read.
fn check_tainted_array(
    array: &str,
    context: &str,
    taint: &HashMap<String, Taint>,
    errors: &mut Vec<String>,
) {
    if taint_of_name(taint, array) == Taint::Tainted {
        errors.push(format!(
            "tainted array {}[...] used as {} without attest (SSA taint backstop)",
            array, context
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Expr, ExprKind, ParameterExpansion, Span, VarType};

    fn vref(name: &str) -> VarRef {
        VarRef {
            name: name.to_string(),
            version: 1,
            var_type: VarType::String,
        }
    }

    fn tainted(name: &str) -> HashMap<String, Taint> {
        HashMap::from([(format!("{}_1", name), Taint::Tainted)])
    }

    fn e(kind: ExprKind) -> Expr {
        Expr {
            kind,
            span: Span::new(1, 1),
        }
    }

    fn names_read_by(kind: ExprKind) -> Vec<String> {
        let mut out = Vec::new();
        collect_expr_var_name(&e(kind), &mut out);
        out
    }

    fn errors_for(value: &SsaValue, taint: &HashMap<String, Taint>) -> Vec<String> {
        let mut errors = Vec::new();
        check_value_use(value, taint, &mut errors, "a sink");
        errors
    }

    fn pexp(name: &str, taint_off: bool) -> ExprKind {
        ExprKind::ParameterExpansion(ParameterExpansion {
            name: name.to_string(),
            operator: Some(":?".to_string()),
            operand: Some(Box::new(e(ExprKind::Literal("no".to_string())))),
            index: None,
            taint_off,
        })
    }

    // `collect_expr_var_name` is total over `ExprKind`, but its only caller
    // walks a REDIRECT TARGET, and `spill_scalar_redirect_targets` rewrites
    // every `${…}` and `$a[i]` target into a synthetic scalar plus a bare
    // `SimpleExpansion` before the SSA exists. Measured: one instrumented
    // compile of `/bin/echo hi > ${f:?no}` reaches the `SimpleExpansion` arm
    // and no other, and `> ${a[0]:?no}` does the same.
    //
    // Nothing is lost by the spill — the temp's own taint is derived from the
    // spilled value by `classify_inst_def`, so a tainted `${f:?no}` is still
    // rejected, via the temp's name. That makes these two arms correct and
    // unreachable from any source text, which is exactly the case a unit test
    // exists for (same shape as `every_expr_kind_renders` in
    // `transform/expressions.rs`). Pinned, not deleted: the match is total on
    // purpose, and a future target form that skips the spill must find the
    // right answer already here.
    #[test]
    fn redirect_target_forms_resolve_the_name_they_read() {
        assert_eq!(names_read_by(ExprKind::SimpleExpansion("f".into())), ["f"]);
        assert_eq!(
            names_read_by(ExprKind::Subscript {
                name: "a".into(),
                index: Box::new(e(ExprKind::Literal("0".into()))),
            }),
            ["a"]
        );
        assert_eq!(names_read_by(pexp("f", false)), ["f"]);
        // DD#91: `${f:!?no}` has vouched for this read. Naming it here would
        // make the backstop reject a program the front end deliberately allows.
        assert!(names_read_by(pexp("f", true)).is_empty());
    }

    // These five `SsaValue` shapes are built **only** as the value of an
    // `SsaInst::Assign` — `ArgvIndex` and `TableValue`/`TableKey` by the
    // for-loop binding lowerings, `AssocArray` by a map literal, and
    // `ArithExpr` by a hoisted `$(( … ))`. `SsaInst::taint_reads` returns no
    // reads for `Assign` (a value there propagates into its target, where
    // `classify_inst_def` owns it), so none of them can reach `check_value_use`
    // from source.
    //
    // Each test asserts the ICE, and the ICE *is* the claim: this shape is
    // still unreachable. The day one stops panicking, a read route was opened
    // in another file and the reporter needs an arm written deliberately, with
    // specs — which is the whole reason these are not silent recursion arms.
    #[test]
    #[should_panic(expected = "Assign-only SsaValue")]
    fn arith_expr_cannot_reach_the_value_walk() {
        errors_for(
            &SsaValue::ArithExpr {
                var: vref("v"),
                op: ArithOp::Add,
                operand: 1,
            },
            &tainted("v"),
        );
    }

    #[test]
    #[should_panic(expected = "Assign-only SsaValue")]
    fn argv_index_cannot_reach_the_value_walk() {
        errors_for(&SsaValue::ArgvIndex(vref("v")), &tainted("v"));
    }

    #[test]
    #[should_panic(expected = "Assign-only SsaValue")]
    fn table_value_cannot_reach_the_value_walk() {
        errors_for(
            &SsaValue::TableValue {
                array: "m".into(),
                index: vref("i"),
            },
            &tainted("m_1"),
        );
    }

    #[test]
    #[should_panic(expected = "Assign-only SsaValue")]
    fn table_key_cannot_reach_the_value_walk() {
        errors_for(
            &SsaValue::TableKey {
                array: "m".into(),
                index: vref("i"),
            },
            &tainted("m_1"),
        );
    }

    #[test]
    #[should_panic(expected = "Assign-only SsaValue")]
    fn assoc_array_cannot_reach_the_value_walk() {
        errors_for(
            &SsaValue::AssocArray(vec![("k".into(), SsaValue::Var(vref("v")))]),
            &tainted("v"),
        );
    }

    // `CmdSubstBody`'s reads are an open FRONT-END gap
    // (`notes/tasks/bug-taint-escape-in-cmdsub-body.md`), not something this
    // value-oriented walk can see: `body` is `Vec<ir::Statement>` and is never
    // lowered to SSA. It is also lifted out of value position by
    // `lift_int_func_calls` before codegen. Pinned so that "reports nothing"
    // stays a deliberate answer rather than an accident of the lift ordering.
    #[test]
    fn a_cmdsub_body_reports_nothing_from_the_value_walk() {
        assert!(errors_for(&SsaValue::CmdSubstBody { body: Vec::new() }, &tainted("v")).is_empty());
    }

    // Both guards in `cmd_subst_taint`'s head resolution. SB5011 rewrites or
    // rejects every non-literal head and a user-function capture is lowered
    // before SSA taint, so neither is reachable from source — both were made
    // to panic and probed in 2026-10-04 to confirm it. They answered
    // `Tainted` and `Clean` until then; per DG's ruling an ICE is a
    // regression guard, never the sole defense, so the answer became the
    // panic and the pass that enforces the invariant stayed the defense.
    #[test]
    fn cmd_subst_head_resolution_ices() {
        let taint = HashMap::new();
        let fns = std::collections::HashSet::from(["f".to_string()]);

        for (what, argv) in [
            ("no head at all", Vec::new()),
            ("a non-literal head", vec![SsaValue::Var(vref("c"))]),
            ("a user-function head", vec![SsaValue::Literal("f".into())]),
        ] {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                cmd_subst_taint(&argv, &taint, &fns)
            }));
            assert!(r.is_err(), "{what} must ICE, not return a taint");
        }

        // The live arm is unchanged: a resolved external is tainted.
        assert_eq!(
            cmd_subst_taint(&[SsaValue::Literal("/bin/cat".into())], &taint, &fns),
            Taint::Tainted
        );
    }
}
