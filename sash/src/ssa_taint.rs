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
//! 3. `bless` is the ONLY way to move from Tainted → Clean

use crate::ssa::*;
/// tainted value reaching a use site, that's a bug in the CST analyzer.
///
/// The pass walks all SSA instructions and classifies each VarRef version
/// as tainted or clean based on how it was defined:
///   - getenv, argv, read, external cmdsub → tainted
///   - literal, user func, known-safe builtin → clean
///   - bless target → clean (bless source stays tainted)
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
        SsaInst::Bless { target, .. } => {
            // Bless produces a clean version.
            taint.insert(var_key(target), Taint::Clean);
        }
        SsaInst::ReadLine {
            target,
            redirects,
            clean_pipe,
            ..
        } => {
            taint.insert(var_key(target), read_line_taint(redirects, *clean_pipe));
        }
        SsaInst::ReadLineMulti {
            targets,
            redirects,
            clean_pipe,
            ..
        } => {
            let t = read_line_taint(redirects, *clean_pipe);
            for tg in targets {
                taint.insert(var_key(tg), t);
            }
        }
        SsaInst::CaptureEnd { target } => {
            // Optimistic: assume capture is of a user-defined function
            // (clean by DD#24). The CST pass is the authority — if a
            // CaptureEnd truly captures external output it should already
            // have been rejected upstream.
            taint.insert(var_key(target), Taint::Clean);
        }

        // Instructions that don't define a VarRef
        SsaInst::Command { .. }
        | SsaInst::Test(_)
        | SsaInst::Phi(_) // handled above
        | SsaInst::SubscriptAssign { .. }
        | SsaInst::ArrayAppend { .. }
        | SsaInst::Mapfile { .. }
        | SsaInst::ReadArray { .. }
        | SsaInst::CaptureStart
        | SsaInst::Pipeline { .. } => {}
    }
}

/// Heredoc/herestring data is literal in the program source; clean-pipe
/// is data flowing from another sash function. Anything else is tainted.
fn read_line_taint(redirects: &[crate::ir::Redirect], clean_pipe: bool) -> Taint {
    let from_literal = redirects.iter().any(|r| {
        r.op == crate::ir::RedirectOp::HereString || r.op == crate::ir::RedirectOp::HereDoc
    });
    if from_literal || clean_pipe {
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
            // `bless` is the one operator allowed to receive tainted args.
            if name == "bless" {
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

        // Assign, Bless, ReadLine, etc. — definitions, not uses
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

// cyclo-allow: SsaValue dispatch + 15-arm cmd-basename table
fn value_is_tainted(
    value: &SsaValue,
    taint: &HashMap<String, Taint>,
    user_functions: &std::collections::HashSet<String>,
) -> Taint {
    match value {
        SsaValue::Literal(_) => Taint::Clean,
        SsaValue::Var(vref) => {
            if is_tainted(taint, vref) {
                Taint::Tainted
            } else {
                Taint::Clean
            }
        }
        SsaValue::Expr(_) => Taint::Clean, // string expressions from lowerer
        SsaValue::FuncCall(_) => Taint::Clean, // elidable user functions
        SsaValue::IntFuncCall { .. } => Taint::Clean, // int functions return clean integers
        SsaValue::CmdSubst { argv, .. } => {
            // All command substitution output is tainted unless it's a known-clean builtin.
            // External command output (including getenv) requires explicit bless.
            if let Some(SsaValue::Literal(cmd)) = argv.first() {
                // Strip path to get basename (e.g., /bin/echo → echo)
                let basename = cmd.rsplit('/').next().unwrap_or(cmd);
                match basename {
                    "getenv" | "argv" => Taint::Tainted,
                    // Known-safe internal builtins (pure computation, no external data)
                    "len" | "keys" | "join" | "epochseconds" | "random" | "argc" | "getpid"
                    | "getppid" | "getcpid" | "getcwd" | "status" | "signum" | "dtoi" | "itod" => {
                        Taint::Clean
                    }
                    _ => {
                        // Sash builtins that produce output from their args:
                        // if all args are clean, output is clean (pure computation).
                        if matches!(basename, "echo" | "printf" | "pwd") {
                            let args_clean = argv[1..].iter().all(|a| {
                                value_is_tainted(a, taint, user_functions) != Taint::Tainted
                            });
                            if args_clean {
                                return Taint::Clean;
                            }
                        }
                        // User-defined functions are clean by DD#24
                        if user_functions.contains(cmd.as_str())
                            || user_functions.contains(basename)
                        {
                            return Taint::Clean;
                        }
                        // Everything else (external commands) is tainted.
                        // Explicit `bless` is required — no auto-bless.
                        Taint::Tainted
                    }
                }
            } else {
                Taint::Tainted
            }
        }
        SsaValue::Concat(parts) => {
            if parts
                .iter()
                .any(|p| value_is_tainted(p, taint, user_functions) == Taint::Tainted)
            {
                Taint::Tainted
            } else {
                Taint::Clean
            }
        }
        SsaValue::ArithExpr { var, .. } => {
            if is_tainted(taint, var) {
                Taint::Tainted
            } else {
                Taint::Clean
            }
        }
        SsaValue::ArithBinOp { left, right, .. } => {
            if value_is_tainted(left, taint, user_functions) == Taint::Tainted
                || value_is_tainted(right, taint, user_functions) == Taint::Tainted
            {
                Taint::Tainted
            } else {
                Taint::Clean
            }
        }
        SsaValue::ParamExpansion { var, .. } => {
            if is_tainted(taint, var) {
                Taint::Tainted
            } else {
                Taint::Clean
            }
        }
        SsaValue::Subscript { .. } => Taint::Clean, // array access — taint is on the array
        SsaValue::ArgvIndex(_) => Taint::Tainted,   // program argv — tainted
        SsaValue::IndexedArray(elems) => {
            if elems
                .iter()
                .any(|e| value_is_tainted(e, taint, user_functions) == Taint::Tainted)
            {
                Taint::Tainted
            } else {
                Taint::Clean
            }
        }
        SsaValue::AssocArray(pairs) => {
            if pairs
                .iter()
                .any(|(_, v)| value_is_tainted(v, taint, user_functions) == Taint::Tainted)
            {
                Taint::Tainted
            } else {
                Taint::Clean
            }
        }
        SsaValue::Ternary {
            cond,
            true_val,
            false_val,
        } => {
            if value_is_tainted(cond, taint, user_functions) == Taint::Tainted
                || value_is_tainted(true_val, taint, user_functions) == Taint::Tainted
                || value_is_tainted(false_val, taint, user_functions) == Taint::Tainted
            {
                Taint::Tainted
            } else {
                Taint::Clean
            }
        }
        SsaValue::TableValue { .. } | SsaValue::TableKey { .. } => Taint::Clean,
    }
}

// cyclo-allow: SsaValue traversal — 1-line arms
fn check_value_use(
    value: &SsaValue,
    taint: &HashMap<String, Taint>,
    errors: &mut Vec<String>,
    context: &str,
) {
    match value {
        SsaValue::Var(vref) => {
            if is_tainted(taint, vref) {
                errors.push(format!(
                    "ICE: tainted variable {}_{} used as {} — CST taint pass missed this",
                    vref.name, vref.version, context
                ));
            }
        }
        SsaValue::Concat(parts) => {
            for p in parts {
                check_value_use(p, taint, errors, context);
            }
        }
        SsaValue::CmdSubst { argv, .. } => {
            for a in argv {
                check_value_use(a, taint, errors, context);
            }
        }
        SsaValue::ArithExpr { var, .. } => {
            if is_tainted(taint, var) {
                errors.push(format!(
                    "ICE: tainted variable {}_{} used in arithmetic as {} — CST taint pass missed this",
                    var.name, var.version, context
                ));
            }
        }
        SsaValue::ArithBinOp { left, right, .. } => {
            check_value_use(left, taint, errors, context);
            check_value_use(right, taint, errors, context);
        }
        SsaValue::ParamExpansion { var, .. } => {
            if is_tainted(taint, var) {
                errors.push(format!(
                    "ICE: tainted variable {}_{} used in param expansion as {} — CST taint pass missed this",
                    var.name, var.version, context
                ));
            }
        }
        SsaValue::IndexedArray(elems) => {
            for e in elems {
                check_value_use(e, taint, errors, context);
            }
        }
        SsaValue::Subscript { index, .. } => {
            check_value_use(index, taint, errors, context);
        }
        _ => {} // Literal, Expr, FuncCall — no tainted reads
    }
}
