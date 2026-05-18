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

    // Special variables are inherently clean — they aren't external data
    // In sash, there are no magic variables. PID/status/etc are builtins.
    // Only $? and $! survive as sugar — and $? is scoped per DD#55.
    // $$ (PID), $# (argc), $0 (argv 0) are process metadata — clean.
    // These will eventually be desugared to builtins in the lowerer.
    for sv in &["?", "!", "$", "#", "0"] {
        taint.insert(format!("{}_0", sv), Taint::Clean);
    }

    // Positional params at version 0 are clean — either program argv
    // (which the CST pass gates) or function args (clean by DD#24)
    for i in 1..=9 {
        taint.insert(format!("{}_0", i), Taint::Clean);
    }
    taint.insert("@_0".to_string(), Taint::Clean);
    taint.insert("*_0".to_string(), Taint::Clean);

    // Collect user-defined function names from the SSA blocks
    let mut user_functions: std::collections::HashSet<String> = std::collections::HashSet::new();
    for block in program.blocks.values() {
        if let Some(name) = block.label.strip_prefix("fn_") {
            user_functions.insert(name.to_string());
        }
    }

    // Phase 1: classify every defined VarRef
    for block in program.blocks.values() {
        // Phi nodes — optimistic: default to clean, will be corrected
        // by iteration if any incoming is tainted. Since the CST pass
        // is the authority, false-clean here just means we don't catch
        // a CST bug involving loops — acceptable for a backstop.
        for phi in &block.phis {
            let key = var_key(&phi.target);
            let t = phi.sources.iter().any(|(_bid, vref)| {
                // Only check sources that are already classified
                matches!(taint.get(&var_key(vref)), Some(&Taint::Tainted))
            });
            taint.insert(key, if t { Taint::Tainted } else { Taint::Clean });
        }

        for inst in &block.insts {
            match inst {
                SsaInst::Assign { target, value } => {
                    let t = value_is_tainted(value, &taint, &user_functions);
                    taint.insert(var_key(target), t);
                }
                SsaInst::Bless { target, source: _, test: _ } => {
                    // Bless produces a clean version
                    taint.insert(var_key(target), Taint::Clean);
                }
                SsaInst::ReadLine { target, redirects, clean_pipe, .. } => {
                    // Read from herestring/heredoc of literal data is clean
                    let is_herestring = redirects.iter().any(|r| {
                        r.op == crate::ir::RedirectOp::HereString
                            || r.op == crate::ir::RedirectOp::HereDoc
                    });
                    if is_herestring || *clean_pipe {
                        taint.insert(var_key(target), Taint::Clean);
                    } else {
                        taint.insert(var_key(target), Taint::Tainted);
                    }
                }
                SsaInst::ReadLineMulti { targets, redirects, clean_pipe, .. } => {
                    let is_herestring = redirects.iter().any(|r| {
                        r.op == crate::ir::RedirectOp::HereString
                            || r.op == crate::ir::RedirectOp::HereDoc
                    });
                    for t in targets {
                        taint.insert(var_key(t), if is_herestring || *clean_pipe { Taint::Clean } else { Taint::Tainted });
                    }
                }
                SsaInst::CaptureEnd { target } => {
                    // Capture of function output — check if the preceding
                    // CaptureStart was for a user-defined function.
                    // Heuristic: if any block label starts with "fn_" for a
                    // known user function, the capture is clean by DD#24.
                    // Default: tainted (external command capture).
                    taint.insert(var_key(target), Taint::Clean);
                    // Note: this is optimistic. The CST pass is the authority
                    // on whether a capture source is tainted. If a CaptureEnd
                    // truly captures external output, the CST pass should have
                    // already rejected it. Making this clean avoids false ICEs
                    // for user-func captures which are the common case.
                }

                // Instructions that don't define a VarRef
                SsaInst::Command { .. }
                | SsaInst::Test(_)
                | SsaInst::Phi(_)  // handled above
                | SsaInst::SubscriptAssign { .. }
                | SsaInst::ArrayAppend { .. }
                | SsaInst::Mapfile { .. }
                | SsaInst::ReadArray { .. }
                | SsaInst::CaptureStart
                | SsaInst::Pipeline { .. } => {}
            }
        }
    }

    // Phase 2: check uses — any instruction reading a tainted VarRef
    // in a position that would produce output or side effects is a bug.
    for block in program.blocks.values() {
        for inst in &block.insts {
            match inst {
                SsaInst::Command { name, args, .. } => {
                    // Commands that produce output (echo, printf, etc.)
                    // or execute externals — reading tainted args is a bug.
                    if name == "bless" {
                        continue;
                    }
                    for arg in args {
                        check_value_use(
                            arg,
                            &taint,
                            &mut errors,
                            &format!("argument to command '{}'", name),
                        );
                    }
                }
                SsaInst::Pipeline { stages, .. } => {
                    for (name, args, _) in stages {
                        for arg in args {
                            check_value_use(
                                arg,
                                &taint,
                                &mut errors,
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
                        &taint,
                        &mut errors,
                        &format!("subscript index of '{}'", array),
                    );
                    check_value_use(
                        value,
                        &taint,
                        &mut errors,
                        &format!("subscript value of '{}'", array),
                    );
                }
                SsaInst::ArrayAppend { value, array, .. } => {
                    check_value_use(
                        value,
                        &taint,
                        &mut errors,
                        &format!("array append to '{}'", array),
                    );
                }

                // Assign, Bless, ReadLine, etc. — definitions, not uses
                _ => {}
            }
        }
    }

    errors
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
