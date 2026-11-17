//! DD131: source-owned immutable bindings, not an internal-name convention.

use crate::bindings::SourceNames;
use crate::diagnostic::Diagnostic;
use crate::ir::*;
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, Default)]
pub(crate) struct Bindings {
    pub(crate) sites: BTreeMap<String, Span>,
}

impl Bindings {
    pub(crate) fn contains(&self, name: &str) -> bool {
        self.sites.contains_key(name)
    }
}

pub(crate) fn source_name(name: &str) -> bool {
    name.starts_with('_') && name != "_"
}

pub(crate) struct Construction<'a> {
    pub(crate) name: String,
    initializer: Initializer<'a>,
}

enum Initializer<'a> {
    Value(Option<&'a Expr>),
    Words(&'a [Expr]),
    Command(&'a Command),
}

impl Construction<'_> {
    pub(crate) fn initializer(&self, span: Span) -> Option<Statement> {
        let value = match self.initializer {
            Initializer::Value(value) => value?.clone(),
            Initializer::Words(words) => Expr {
                kind: ExprKind::Array(words.to_vec()),
                span,
            },
            Initializer::Command(command) => {
                return Some(Statement {
                    kind: StmtKind::Command(command.clone()),
                    span,
                });
            }
        };
        Some(value_initializer(value, span))
    }
}

fn value_initializer(value: Expr, span: Span) -> Statement {
    Statement {
        kind: StmtKind::Assignment(Assignment {
            name: "_".to_string(),
            index: None,
            op: AssignOp::Set,
            value,
        }),
        span,
    }
}

pub(crate) fn redirect_initializer(r: &Redirected, span: Span) -> Statement {
    value_initializer(
        Expr {
            kind: ExprKind::Array(r.redirects.iter().map(|r| r.target.clone()).collect()),
            span,
        },
        span,
    )
}

pub(crate) fn reader_constructions(stmt: &Statement) -> Vec<Construction<'_>> {
    let stmt = stmt.stage_marks().0;
    if let StmtKind::While(w) = &stmt.kind {
        return w
            .condition
            .iter()
            .flat_map(|condition| constructions(condition.stage_marks().0))
            .collect();
    }
    constructions(stmt)
}

/// Source construction sites. Structural/capture recursion belongs to the
/// shared statement walker; a declaration's initializers remain interleaved.
/// Total over `StmtKind`; do not add a wildcard.
pub(crate) fn constructions(stmt: &Statement) -> Vec<Construction<'_>> {
    match &stmt.kind {
        StmtKind::Assignment(a) => {
            if a.index.is_some() || a.op != AssignOp::Set {
                return Vec::new();
            }
            vec![Construction {
                name: a.name.clone(),
                initializer: Initializer::Value(Some(&a.value)),
            }]
        }
        StmtKind::Declaration(d) => d
            .assignments
            .iter()
            .map(|a| Construction {
                name: a.name.clone(),
                initializer: Initializer::Value(a.value.as_ref()),
            })
            .collect(),
        StmtKind::For(f) => vec![Construction {
            name: f.variable.clone(),
            initializer: Initializer::Words(&f.words),
        }],
        StmtKind::Command(c) => {
            let mut names = HashSet::new();
            crate::validate::command_bound_names(c, &mut names);
            let mut names: Vec<_> = names.into_iter().collect();
            names.sort();
            names
                .into_iter()
                .map(|name| Construction {
                    name,
                    initializer: Initializer::Command(c),
                })
                .collect()
        }
        StmtKind::Pipeline(_)
        | StmtKind::If(_)
        | StmtKind::While(_)
        | StmtKind::Case(_)
        | StmtKind::Function(_)
        | StmtKind::Redirected(_)
        | StmtKind::List(_)
        | StmtKind::Group(_)
        | StmtKind::Suppressed(_)
        | StmtKind::Negated(_)
        | StmtKind::ArithTest(_)
        | StmtKind::ReturnProducer(_) => Vec::new(),
    }
}

pub(crate) fn collect(program: &Program, names: &SourceNames) -> (Bindings, Vec<Diagnostic>) {
    let mut bindings = Bindings::default();
    let mut diags = Vec::new();
    for body in program.bodies() {
        walk_stmt_tree(body, &mut |stmt| {
            record_constructions(stmt, names, &mut bindings, &mut diags);
        });
    }
    (bindings, diags)
}

fn record_constructions(
    stmt: &Statement,
    names: &SourceNames,
    bindings: &mut Bindings,
    diags: &mut Vec<Diagnostic>,
) {
    for construction in constructions(stmt)
        .into_iter()
        .filter(|construction| source_name(names.original(&construction.name)))
    {
        if let Some(first) = bindings.sites.get(&construction.name) {
            diags.push(Diagnostic::error_span(
                stmt.span,
                "SB5103",
                format!(
                    "scratch `{}` already has a construction site at {}:{}; \
                     use a different name for a new immutable result",
                    construction.name, first.line, first.col
                ),
            ));
        } else {
            bindings.sites.insert(construction.name, stmt.span);
        }
    }
}

pub(crate) fn check_writes(
    program: &crate::ssa::SsaProgram,
    bindings: &Bindings,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for diag in program
        .blocks
        .values()
        .flat_map(|block| block_write_failures(program, bindings, block))
    {
        if !diags.contains(&diag) {
            diags.push(diag);
        }
    }
    diags
}

fn block_write_failures<'a>(
    program: &'a crate::ssa::SsaProgram,
    bindings: &'a Bindings,
    block: &'a crate::ssa::BasicBlock,
) -> impl Iterator<Item = Diagnostic> + 'a {
    let mut origin = block.origin;
    block.insts.iter().filter_map(move |inst| {
        if let crate::ssa::SsaInst::SourceOrigin(next, _) = inst {
            origin = *next;
            return None;
        }
        write_failure(program, bindings, origin, inst)
    })
}

fn write_failure(
    program: &crate::ssa::SsaProgram,
    bindings: &Bindings,
    origin: Option<usize>,
    inst: &crate::ssa::SsaInst,
) -> Option<Diagnostic> {
    let name = crate::ssa::instruction_write(inst).filter(|name| bindings.contains(name))?;
    if crate::ssa::instruction_target(inst)
        .is_some_and(|target| program.private_initializations.contains(target))
        || origin
            .and_then(|id| program.scratch_constructions.get(&id))
            .is_some_and(|names| names.contains(name))
    {
        return None;
    }
    let (site, call) = crate::terminal::origin_sites(program, origin);
    Some(Diagnostic::error_span(
        call,
        "SB5103",
        format!(
            "cannot modify immutable scratch `{name}` at {}:{}; \
                 construct a new result with a different name",
            site.line, site.col
        ),
    ))
}
