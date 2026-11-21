//! ERR readonly imports and shared executable-SSA traversal.

use crate::diagnostic::Diagnostic;
use crate::ir::{HandlerKind, Span};
use crate::ssa::*;
use std::collections::{BTreeSet, HashSet};

pub(crate) fn check_err(program: &SsaProgram, source: &crate::ir::Program) -> Vec<Diagnostic> {
    let Some(handler) = program.handlers.get(&HandlerKind::Err) else {
        return Vec::new();
    };
    let imports = crate::validate::err_imports(source);
    let mut errors = Vec::new();
    for id in live_blocks(program, handler.block) {
        block_readonly_failures(program, program.block(id), &imports, &mut errors);
    }
    let mut seen = HashSet::new();
    errors.retain(|d| seen.insert((d.line, d.col, d.message.clone())));
    errors
}

fn block_readonly_failures(
    program: &SsaProgram,
    block: &BasicBlock,
    imports: &HashSet<String>,
    errors: &mut Vec<Diagnostic>,
) {
    let mut origin = block.origin;
    for inst in live_insts(block) {
        if let SsaInst::SourceOrigin(next, _) = inst {
            origin = *next;
            continue;
        }
        errors.extend(readonly_failure(
            program,
            origin.or(block.origin),
            inst,
            imports,
        ));
    }
}

fn readonly_failure(
    program: &SsaProgram,
    origin: Option<usize>,
    inst: &SsaInst,
    imports: &HashSet<String>,
) -> Option<Diagnostic> {
    let name = instruction_write(inst).filter(|name| imports.contains(*name))?;
    let (site, call) = origin_sites(program, origin);
    Some(Diagnostic::error_span(
        call,
        "SB5102",
        format!(
            "ERR cannot write inherited binding `{name}` at {}:{}; use ERR-only \
             working state or a fresh private local instead",
            site.line, site.col
        ),
    ))
}

pub(crate) fn origin_sites(program: &SsaProgram, origin: Option<usize>) -> (Span, Span) {
    let mut path = Vec::new();
    let mut next = origin;
    while let Some(id) = next {
        let source = program.origins[id];
        if path.last() != Some(&source.span) {
            path.push(source.span);
        }
        next = source.parent;
    }
    let site = path.first().copied().unwrap_or(Span::new(1, 1));
    let call = path.last().copied().unwrap_or(site);
    (site, call)
}

pub(crate) fn live_insts(block: &BasicBlock) -> &[SsaInst] {
    let end = block
        .insts
        .iter()
        .position(is_exit)
        .map_or(block.insts.len(), |i| i + 1);
    &block.insts[..end]
}

pub(crate) fn is_exit(inst: &SsaInst) -> bool {
    matches!(
        inst,
        SsaInst::Command {
            kind: BuiltinCmd::Exit | BuiltinCmd::Fail,
            ..
        }
    )
}

pub(crate) fn live_blocks(program: &SsaProgram, root: BlockId) -> BTreeSet<BlockId> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        if seen.insert(id) {
            pending.extend(live_successors(program.block(id)));
        }
    }
    seen
}

pub(crate) fn live_successors(block: &BasicBlock) -> Vec<BlockId> {
    if block.insts.iter().any(is_exit) {
        Vec::new()
    } else {
        block.terminator.successors()
    }
}
