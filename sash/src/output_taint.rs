//! DD#127: kernel reads introduce taint; compiler-owned copies do not.

use crate::capture_purity::capture_body_is_pure;
use crate::ir::*;
use crate::ssa::BuiltinCmd;
use std::collections::{BTreeSet, HashMap, HashSet};

/// Output paths not yet owned by a shared function, and the innermost shared
/// owners of the other paths. This never changes the buffer/read classifier.
#[derive(Clone, Default)]
pub(crate) struct OutputPaths {
    direct: bool,
    pub owners: BTreeSet<String>,
    pub stdin: bool,
}

impl OutputPaths {
    pub fn extend(&mut self, other: Self) {
        self.direct |= other.direct;
        self.owners.extend(other.owners);
        self.stdin |= other.stdin;
    }

    pub fn owned_by(&mut self, name: &str) {
        if self.direct {
            self.owners.insert(name.to_string());
            self.direct = false;
        }
    }
}

#[derive(Default)]
pub(crate) struct OutputTaint {
    bodies: HashMap<String, Vec<Statement>>,
    value_fns: HashSet<String>,
    int_returns: HashSet<String>,
}

impl OutputTaint {
    pub fn new(program: &Program) -> Self {
        let mut output = Self::default();
        let mut string_returns = HashSet::new();
        for f in program.function_defs() {
            output
                .bodies
                .entry(f.name.clone())
                .or_default()
                .extend(f.body.iter().cloned());
            if f.return_type == VarType::Int {
                output.value_fns.insert(f.name.clone());
            } else {
                string_returns.insert(f.name.clone());
            }
        }
        // All definitions contribute, as they do to the independent name
        // summaries. One integer definition cannot hide another's stdout.
        output.int_returns = output
            .value_fns
            .difference(&string_returns)
            .cloned()
            .collect();
        output
    }

    pub fn capture(&self, body: &[Statement]) -> bool {
        single_value_command(body)
            .and_then(|c| self.value_source(c))
            .unwrap_or_else(|| !capture_body_is_pure(body, &self.bodies, &self.value_fns))
    }

    pub fn paths(
        &self,
        body: &[Statement],
        summary: &impl Fn(&str) -> Option<OutputPaths>,
    ) -> OutputPaths {
        let mut paths = OutputPaths::default();
        for stmt in body {
            paths.extend(self.stmt_paths(stmt, summary));
        }
        paths
    }

    fn stmt_paths(
        &self,
        stmt: &Statement,
        summary: &impl Fn(&str) -> Option<OutputPaths>,
    ) -> OutputPaths {
        if matches!(stmt.kind, StmtKind::Function(_)) {
            return OutputPaths::default();
        }
        let mut paths = self.shape_paths(stmt, summary);
        crate::resolve::walk_value_calls(
            stmt,
            &|name| self.value_fns.contains(name),
            &mut |name| {
                if let Some(output) = summary(name) {
                    paths.extend(output);
                }
            },
        );
        paths
    }

    fn shape_paths(
        &self,
        stmt: &Statement,
        summary: &impl Fn(&str) -> Option<OutputPaths>,
    ) -> OutputPaths {
        match &stmt.kind {
            StmtKind::Command(c) => command_paths(c, summary),
            StmtKind::Pipeline(p) => self.pipeline_paths(p, summary),
            StmtKind::Redirected(r) => {
                let mut paths = self.stmt_paths(&r.body, summary);
                paths.direct = true;
                paths.stdin &= !redirect_reads_new_input(r);
                paths
            }
            _ => self.child_paths(stmt, summary),
        }
    }

    fn child_paths(
        &self,
        stmt: &Statement,
        summary: &impl Fn(&str) -> Option<OutputPaths>,
    ) -> OutputPaths {
        let mut paths = OutputPaths::default();
        for body in stmt.structural_bodies() {
            paths.extend(self.paths(body, summary));
        }
        paths
    }

    fn pipeline_paths(
        &self,
        p: &Pipeline,
        summary: &impl Fn(&str) -> Option<OutputPaths>,
    ) -> OutputPaths {
        let mut paths = OutputPaths {
            direct: true,
            ..OutputPaths::default()
        };
        for (index, stmt) in p.commands.iter().enumerate() {
            let mut stage = self.stmt_paths(stmt, summary);
            if index != 0 && !stage_runs_in_parent(stmt) {
                stage.stdin = false;
            }
            paths.extend(stage);
        }
        paths
    }

    fn value_source(&self, c: &Command) -> Option<bool> {
        let name = match &c.name.kind {
            ExprKind::Literal(name) => name.as_str(),
            _ => return Some(true),
        };
        if matches!(name, "getenv" | "arg" | "argv") {
            return Some(true);
        }
        // Value calls return a value, not their stdout. Any input they use
        // is checked separately; stdout effects belong to the outer capture.
        crate::resolve::evaluates_args_in_place(name, &|n| self.int_returns.contains(n))
            .then_some(false)
    }
}

fn command_paths(c: &Command, summary: &impl Fn(&str) -> Option<OutputPaths>) -> OutputPaths {
    let ExprKind::Literal(name) = &c.name.kind else {
        return OutputPaths {
            direct: true,
            ..OutputPaths::default()
        };
    };
    summary(name).unwrap_or_else(|| OutputPaths {
        direct: name.contains('/')
            || matches!(
                BuiltinCmd::from_name(name),
                BuiltinCmd::Echo | BuiltinCmd::Write | BuiltinCmd::Exec
            ),
        stdin: name.contains('/') || BuiltinCmd::from_name(name) == BuiltinCmd::Exec,
        ..OutputPaths::default()
    })
}

pub(crate) fn stage_runs_in_parent(stmt: &Statement) -> bool {
    let (body, _, negated) = stmt.stage_marks();
    (matches!(body.kind, StmtKind::While(_)) && !negated) || body.is_pipe_reader()
}

/// An explicit input open is a new read source, still tainted. Dups retain
/// inherited-input obligations; this is not a descriptor-isolation proof.
pub(crate) fn redirect_reads_new_input(r: &Redirected) -> bool {
    r.redirects
        .iter()
        .rev()
        .find(|redirect| redirect.resolved_fd() == 0)
        .is_some_and(|redirect| redirect.op.open_flags().is_some())
}
