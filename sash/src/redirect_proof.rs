//! DD119: source-local, nontransitive redirect exemptions.

use crate::ir::{Expr, ExprKind, TestExpr, VarType};
use crate::ssa::{BlockId, SsaProgram, VersionState};

pub(crate) fn target_binding(expr: &Expr) -> Option<&str> {
    single_binding(expr, true).ok().flatten()
}

fn test_binding(test: &TestExpr) -> Option<&str> {
    let TestExpr::Unary { op, operand } = test else {
        return None;
    };
    if !matches!(
        op.as_str(),
        "-n" | "cpath?"
            | "p?"
            | "g?"
            | "-e"
            | "-f"
            | "-d"
            | "-r"
            | "-w"
            | "-x"
            | "-s"
            | "-L"
            | "-h"
    ) {
        return None;
    }
    single_binding(operand, false).ok().flatten()
}

/// Exactly one scalar read; only the redirect site may add nonempty literals.
fn single_binding(expr: &Expr, literals: bool) -> Result<Option<&str>, ()> {
    match &expr.kind {
        ExprKind::Literal(text) | ExprKind::SingleQuoted(text) if literals || text.is_empty() => {
            Ok(None)
        }
        ExprKind::SimpleExpansion(name) => Ok(Some(name)),
        ExprKind::ParameterExpansion(pe) if pe.operator.is_none() && pe.index.is_none() => {
            Ok(Some(&pe.name))
        }
        ExprKind::DoubleQuoted(parts) | ExprKind::Concat(parts) => joined_binding(parts, literals),
        _ => Err(()),
    }
}

fn joined_binding(parts: &[Expr], literals: bool) -> Result<Option<&str>, ()> {
    parts.iter().try_fold(None, |found, part| {
        let next = single_binding(part, literals)?;
        if found.is_some() && next.is_some() {
            Err(())
        } else {
            Ok(found.or(next))
        }
    })
}

/// Record the whole source predicate before Boolean lowering splits it.
pub(crate) fn record_test(test: &TestExpr, ssa: &mut SsaProgram) {
    if test_binding(test).is_some() {
        let origin = ssa.current_origin.expect("a source test has an origin");
        ssa.origins[origin].nonempty_test = true;
    }
}

/// Keep the source read at its evaluation point, before redirect spills erase it.
pub(crate) fn record_target(
    expr: &Expr,
    ssa: &mut SsaProgram,
    state: &VersionState,
    block: BlockId,
) -> bool {
    let Some(name) = target_binding(expr) else {
        return false;
    };
    if !ssa.source_bindings.contains(name) || state.lookup(name).var_type != VarType::String {
        return false;
    }
    let parent = ssa.enter_source(expr.span, None, false, block);
    let origin = ssa
        .current_origin
        .expect("the redirect just entered its origin");
    ssa.redirect_targets.insert(origin, name.to_string());
    ssa.leave_source(parent, block);
    true
}

pub(crate) fn missing_proof(name: &str) -> String {
    format!(
        "redirect target `${name}` may be empty; use `${{{name}:?message}}` or \
         `${{{name}:-default}}`. Without dispositions, a target must contain \
         only one immutable binding with an in-scope proof from a supported \
         nonempty unary test, plus optional literal text"
    )
}
