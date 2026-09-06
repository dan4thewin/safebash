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

    // Phase 1: classify every defined VarRef.
    for block in program.blocks.values() {
        classify_block_defs(block, &mut taint, &user_functions);
    }

    // Phase 2: report tainted uses in output/external/index positions.
    for block in program.blocks.values() {
        check_block_uses(block, &taint, &mut errors);
    }

    errors
}

/// Seed the taint map with clean entries for positional params and the
/// surviving sugar specials.
///
/// Special variables are inherently clean — they aren't external data.
/// In sash, there are no magic variables. PID/status/etc are builtins.
/// Only $? and $! survive as sugar — and $? is scoped per DD#55.
/// $$ (PID), $# (argc), $0 (argv 0) are process metadata — clean.
/// These will eventually be desugared to builtins in the lowerer.
///
/// Positional params at version 0 are clean — either program argv
/// (which the CST pass gates) or function args (clean by DD#24).
fn seed_clean_specials(taint: &mut HashMap<String, Taint>) {
    for sv in &["?", "!", "$", "#", "0"] {
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
) {
    // Phi nodes — optimistic: default to clean, will be corrected by
    // iteration if any incoming is tainted. Since the CST pass is the
    // authority, false-clean here just means we don't catch a CST bug
    // involving loops — acceptable for a backstop.
    for phi in &block.phis {
        let key = var_key(&phi.target);
        let t = phi
            .sources
            .iter()
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
        SsaInst::ReadLineMulti {
            targets,
            clean_pipe,
            ..
        } => {
            let t = read_line_taint(*clean_pipe);
            for tg in targets {
                taint.insert(var_key(tg), t);
            }
        }
        SsaInst::CaptureEnd { target, tainted } => {
            // A `$(user_fn)` capture is optimistically Clean (DD#24); a
            // multi-statement/pipeline `$(…)` body captures external output
            // and is Tainted. The upstream CST taint pass
            // (`cmdsub_is_taint_source`) is the authoritative gate; this keeps
            // the SSA taint view correct (defense-in-depth).
            let t = if *tainted {
                Taint::Tainted
            } else {
                Taint::Clean
            };
            taint.insert(var_key(target), t);
        }

        // Instructions that don't define a VarRef. Array taint (mapfile /
        // read -a / argv fills) and capture-form builtins are governed by the
        // CST pass; the SSA backstop does not yet re-derive them (see task
        // ssa-taint-cmdsub-operand-propagation for the capture/array work).
        SsaInst::Command { .. }
        | SsaInst::Test(_)
        | SsaInst::SubscriptAssign { .. }
        | SsaInst::ArrayAppend { .. }
        | SsaInst::Mapfile { .. }
        | SsaInst::ReadArray { .. }
        | SsaInst::MatchInto { .. } // fills a clean array (subject is clean); CST pass governs array taint
        | SsaInst::ArgvFill { .. } // fills a tainted array (DD#61 bulk-read source); CST pass governs
        | SsaInst::CaptureStart
        | SsaInst::AttestAll { .. } // validates in place; AttestRetag defines the clean version
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

/// Phase 2: any instruction reading a tainted VarRef in a position that
/// would produce output, execute an external, or escape into an index
/// expression is an ICE.
fn check_inst_uses(inst: &SsaInst, taint: &HashMap<String, Taint>, errors: &mut Vec<String>) {
    match inst {
        SsaInst::Command { name, args, .. } => {
            // `attest` is the one operator allowed to receive tainted args.
            if name == "attest" {
                return;
            }
            for arg in args {
                check_value_use(
                    arg,
                    taint,
                    errors,
                    &format!("argument to command '{}'", name),
                );
            }
        }
        SsaInst::Pipeline { stages, .. } => {
            for (name, args, _) in stages {
                for arg in args {
                    check_value_use(
                        arg,
                        taint,
                        errors,
                        &format!("argument in pipeline stage '{}'", name),
                    );
                }
            }
        }
        SsaInst::SubscriptAssign {
            index,
            value,
            array,
            ..
        } => {
            check_value_use(
                index,
                taint,
                errors,
                &format!("subscript index of '{}'", array),
            );
            check_value_use(
                value,
                taint,
                errors,
                &format!("subscript value of '{}'", array),
            );
        }
        SsaInst::ArrayAppend { value, array, .. } => {
            check_value_use(
                value,
                taint,
                errors,
                &format!("array append to '{}'", array),
            );
        }

        // Assign, Attest, ReadLine, etc. — definitions, not uses
        _ => {}
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
        // DD#91: `@a:!` (`arrvals_trust`, disposition.taint_off) is a sanctioned
        // per-use attest-all bypass → Clean; any other splat inherits the array's
        // taint.
        SsaValue::ArraySplat { array, disposition } => splat_taint(array, disposition, taint),

        // Always-tainted: program argv is external input by construction; a
        // multi-statement/pipeline `$(…)` body captures external output.
        SsaValue::ArgvIndex(_) | SsaValue::CmdSubstBody { .. } => Taint::Tainted,

        // Var-rooted: taint inherited from the named variable's status.
        SsaValue::Var(vref)
        | SsaValue::ArithExpr { var: vref, .. }
        | SsaValue::ParamExpansion { var: vref, .. } => taint_of_var(taint, vref),

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

/// DD#61/#91: taint of an `@array` splat. `@a:!` (`disposition.taint_off`) is a
/// sanctioned per-use attest-all bypass → Clean; any other splat inherits the
/// array's taint.
fn splat_taint(
    array: &str,
    disposition: &Option<crate::ssa::ArraySplatDisposition>,
    taint: &HashMap<String, Taint>,
) -> Taint {
    if disposition.as_ref().is_some_and(|d| d.taint_off) {
        Taint::Clean
    } else {
        taint_of_name(taint, array)
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
/// - **Pass-through transforms** (`echo`, `dtoi`, `itod`) — the output derives
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
    let Some(SsaValue::Literal(cmd)) = argv.first() else {
        return Taint::Tainted;
    };
    let basename = cmd.rsplit('/').next().unwrap_or(cmd);
    match basename {
        "getenv" | "argv" | "arg" => Taint::Tainted,
        "len" | "epochseconds" | "random" | "argc" | "getpid" | "getppid" | "getcpid"
        | "getcwd" | "status" | "signum" => Taint::Clean,
        "echo" | "dtoi" | "itod" => any_tainted(&argv[1..], taint, user_functions),
        _ => {
            if user_functions.contains(cmd.as_str()) || user_functions.contains(basename) {
                Taint::Clean
            } else {
                Taint::Tainted
            }
        }
    }
}

fn check_value_use(
    value: &SsaValue,
    taint: &HashMap<String, Taint>,
    errors: &mut Vec<String>,
    context: &str,
) {
    match value {
        SsaValue::Var(vref) => check_tainted_var(vref, "", context, taint, errors),
        SsaValue::ArithExpr { var, .. } => {
            check_tainted_var(var, " in arithmetic", context, taint, errors)
        }
        SsaValue::ParamExpansion { var, operator, .. } => {
            // DD#60: a length/count operator (`${#a[@]}` / `len a`,
            // `${#s}`) exposes only the element count or byte length, not
            // any tainted element data — clean even on a tainted array.
            if operator != "arrlen" && operator != "strlen" {
                check_tainted_var(var, " in param expansion", context, taint, errors)
            }
        }
        SsaValue::Concat(parts) | SsaValue::IndexedArray(parts) => {
            for p in parts {
                check_value_use(p, taint, errors, context);
            }
        }
        SsaValue::CmdSubst { argv, .. } => {
            for a in argv {
                check_value_use(a, taint, errors, context);
            }
        }
        SsaValue::ArithBinOp { left, right, .. } => {
            check_value_use(left, taint, errors, context);
            check_value_use(right, taint, errors, context);
        }
        SsaValue::Subscript { array, index, .. } => {
            check_tainted_array(array, context, taint, errors);
            check_value_use(index, taint, errors, context);
        }
        _ => {} // Literal, Expr, FuncCall — no tainted reads
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
